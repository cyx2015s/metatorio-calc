//! Restricted-master search, with every terminal certificate lifted to self.
use crate::{
    Basis, BasisStatus, CertifiedResult, ExactSolution, FloatOptions, Model, Rational, Row,
    SolveControl,
};
use num_traits::{Signed, Zero};
use std::time::Instant;

impl Model {
    /// Generate columns from exact missing-supplier rays or dual prices. A
    /// restricted infeasibility is never returned as an original-model proof.
    pub fn solve_column_generation_with_control(
        &self,
        seed: Option<&[f64]>,
        budget: &Rational,
        control: &SolveControl,
    ) -> Result<CertifiedResult, String> {
        control.check()?;
        self.check()?;
        if budget.is_negative()
            || self
                .columns
                .iter()
                .any(|c| c.lower != 0.0 || c.upper != f64::INFINITY || c.cost < 0.0)
        {
            return Err("column generation requires a nonnegative gap budget and unbounded nonnegative columns/costs".into());
        }
        if seed.is_some_and(|s| s.len() != self.columns.len()) {
            return Err("column generation seed dimension mismatch".into());
        }
        let mut included = vec![false; self.columns.len()];
        if let Some(seed) = seed {
            for (j, v) in seed.iter().enumerate() {
                included[j] = v.is_finite() && *v != 0.0;
            }
        }
        let mut warm: Option<Basis> = None;
        let mut exact_pivots = 0;
        let mut float_pivots = 0;
        let mut feasibility_rule = crate::DualPivotRule::FirstViolationLargePivot;
        for round in 0..512 {
            control.check()?;
            let selected: Vec<_> = included
                .iter()
                .enumerate()
                .filter_map(|(j, yes)| yes.then_some(j))
                .collect();
            if selected.len() > 4096 {
                return Err("column generation working column limit exceeded".into());
            }
            let mut positions = vec![None; self.columns.len()];
            for (j, &old) in selected.iter().enumerate() {
                positions[old] = Some(j);
            }
            // A dropped row is homogeneous and has no participating column.
            // Its value is exactly zero after lifting the primal vector.
            let rows: Vec<_> = self
                .rows
                .iter()
                .enumerate()
                .filter_map(|(i, r)| {
                    (r.rhs != 0.0 || r.terms.iter().any(|(j, a)| included[*j] && *a != 0.0))
                        .then_some(i)
                })
                .collect();
            let working = Model {
                columns: selected.iter().map(|j| self.columns[*j].clone()).collect(),
                rows: rows
                    .iter()
                    .map(|i| Row {
                        equality: self.rows[*i].equality,
                        rhs: self.rows[*i].rhs,
                        terms: self.rows[*i]
                            .terms
                            .iter()
                            .filter_map(|(j, a)| positions[*j].map(|p| (p, *a)))
                            .collect(),
                    })
                    .collect(),
                offset: self.offset,
            };
            let began = Instant::now();
            let (solution, basis, proof) = if let Some(previous) = &warm {
                let basis = Basis {
                    columns: selected.iter().map(|j| previous.columns[*j]).collect(),
                    rows: rows.iter().map(|i| previous.rows[*i]).collect(),
                };
                let initial = working.reconstruct_with_control(&basis, control)?;
                if !working.certify(&initial)?.primal_feasible {
                    // Keep the same zero objective throughout interrupted
                    // Phase I. Re-shifting costs on every resumed segment can
                    // redirect repair before a feasible basis is reached.
                    let mut feasibility = working.clone();
                    for column in &mut feasibility.columns {
                        column.cost = 0.0;
                    }
                    feasibility.offset = 0.0;
                    let repaired = feasibility.repair_dual_feasible_basis_with_control(
                        basis,
                        feasibility_rule,
                        1000,
                        |_| began.elapsed().as_secs_f64() < 20.0,
                        control,
                    )?;
                    if repaired.stop_reason.as_deref() == Some("dual repair repeated a basis") {
                        if feasibility_rule == crate::DualPivotRule::FirstViolation {
                            return Err("column feasibility repair repeated a basis under both pivot strategies; no terminal status established".into());
                        }
                        feasibility_rule = crate::DualPivotRule::FirstViolation;
                        eprintln!(
                            "[LP-EXACT-COLUMNS] repeated feasibility basis; switching to indexed pivots"
                        );
                    }
                    exact_pivots += repaired.pivots;
                    let solution = working.reconstruct_with_control(&repaired.basis, control)?;
                    // Phase II starts on the next segment with the original
                    // objective; neither a Phase-I optimum nor its prices is
                    // used as an original-cost optimality certificate.
                    (solution, repaired.basis, repaired.infeasibility)
                } else {
                    let repaired = working.refine_basis_with_control(
                        basis,
                        1000,
                        |_, _| began.elapsed().as_secs_f64() < 20.0,
                        control,
                    )?;
                    exact_pivots += repaired.repair_pivots + repaired.optimization_pivots;
                    (repaired.solution, repaired.basis, repaired.infeasibility)
                }
            } else {
                let proposal = working.float_candidate_with_options(
                    FloatOptions {
                        ratio_tolerance: 1e-12,
                        primal_tolerance: 1e-12,
                    },
                    20000,
                    |_| !control.is_cancelled() && began.elapsed().as_secs_f64() < 10.0,
                );
                let refined = match proposal {
                    Ok(hint) => {
                        float_pivots += hint.pivots;
                        working.certify_candidate_with_control(
                            &hint,
                            1000,
                            budget,
                            |_, _| began.elapsed().as_secs_f64() < 20.0,
                            control,
                        )
                    }
                    Err(error) => Err(error),
                };
                match refined {
                    Ok(result) => {
                        exact_pivots += result.exact_pivots;
                        float_pivots += result.support_search_pivots;
                        (result.solution, result.primal_basis, result.infeasibility)
                    }
                    Err(_) if !control.is_cancelled() => {
                        let began = Instant::now();
                        let cold = working.solve_dual_with_control(
                            crate::DualPivotRule::FirstViolationLargePivot,
                            1000,
                            |_| began.elapsed().as_secs_f64() < 20.0,
                            control,
                        )?;
                        exact_pivots += cold.pivots;
                        (cold.solution, cold.basis, cold.infeasibility)
                    }
                    Err(error) => return Err(error),
                }
            };
            let mut full_basis = Basis {
                columns: vec![BasisStatus::Lower; self.columns.len()],
                rows: vec![BasisStatus::Basic; self.rows.len()],
            };
            for (j, &old) in selected.iter().enumerate() {
                full_basis.columns[old] = basis.columns[j];
            }
            for (i, &old) in rows.iter().enumerate() {
                full_basis.rows[old] = basis.rows[i];
            }
            warm = Some(full_basis.clone());
            let mut values = vec![Rational::zero(); self.columns.len()];
            for (j, &old) in selected.iter().enumerate() {
                values[old] = solution.values[j].clone();
            }
            let mut duals = vec![Rational::zero(); self.rows.len()];
            for (i, &old) in rows.iter().enumerate() {
                duals[old] = solution.row_duals[i].clone();
            }
            let lifted = ExactSolution {
                values,
                row_duals: duals,
                basis_dimension: solution.basis_dimension,
            };
            let missing = if let Some(proof) = proof {
                let mut ray = vec![Rational::zero(); self.rows.len()];
                for (i, value) in proof.row_duals {
                    ray[rows[i]] = value
                        .parse()
                        .map_err(|_| "invalid restricted Farkas multiplier")?;
                }
                if let Some(proof) = self.verify_infeasibility(&ray)? {
                    let solution = ExactSolution {
                        values: vec![Rational::zero(); self.columns.len()],
                        row_duals: ray,
                        basis_dimension: 0,
                    };
                    return Ok(CertifiedResult {
                        certificate: self.certify(&solution)?,
                        solution,
                        primal_basis: full_basis,
                        relative_gap: None,
                        accepted: false,
                        exact_pivots,
                        support_search_pivots: float_pivots,
                        support_columns: selected.len(),
                        stop_reason: None,
                        infeasibility: Some(proof),
                    });
                }
                self.column_generation_prices(&ray, true, &included)?
            } else {
                let working_certificate = working.certify(&solution)?;
                if !working_certificate.primal_feasible
                    || !working_certificate.dual_feasible
                    || !working_certificate
                        .relative_gap()?
                        .as_ref()
                        .is_some_and(|g| g <= budget)
                {
                    eprintln!(
                        "[LP-EXACT-COLUMNS] round={} columns={} rows={} resuming incomplete basis: pivots={} basis={} bad_bounds={} bad_rows={} bad_dual={}",
                        round + 1,
                        selected.len(),
                        rows.len(),
                        exact_pivots,
                        solution.basis_dimension,
                        working_certificate.violated_bounds,
                        working_certificate.violated_rows,
                        working_certificate.violated_dual_signs
                    );
                    continue;
                }
                let (adjusted, _) = self.scale_to_dual_feasible(&lifted)?;
                let certificate = self.certify(&adjusted)?;
                if !certificate.primal_feasible {
                    return Err("column generation full primal lift failed".into());
                }
                let gap = certificate.relative_gap()?;
                if gap.as_ref().is_some_and(|g| g <= budget) {
                    return Ok(CertifiedResult {
                        solution: adjusted,
                        primal_basis: full_basis,
                        certificate,
                        relative_gap: gap,
                        accepted: true,
                        exact_pivots,
                        support_search_pivots: float_pivots,
                        support_columns: selected.len(),
                        stop_reason: None,
                        infeasibility: None,
                    });
                }
                self.column_generation_prices(&lifted.row_duals, false, &included)?
            };
            if missing.is_empty() {
                return Err("column generation has no certified terminal outcome or improving missing column".into());
            }
            for &j in &missing {
                included[j] = true;
            }
            eprintln!(
                "[LP-EXACT-COLUMNS] round={} columns={} rows={} added={}",
                round + 1,
                selected.len(),
                rows.len(),
                missing.len()
            );
        }
        Err("column generation round limit reached; no terminal status established".into())
    }

    fn column_generation_prices(
        &self,
        duals: &[Rational],
        zero_cost: bool,
        included: &[bool],
    ) -> Result<Vec<usize>, String> {
        let priced = ExactSolution {
            values: vec![],
            row_duals: duals.to_vec(),
            basis_dimension: 0,
        };
        let prices = self.pricing(&priced, zero_cost);
        let mut missing: Vec<_> = prices
            .numerators
            .iter()
            .enumerate()
            .filter(|(j, p)| !included[*j] && p.is_negative())
            .collect();
        // All prices share one positive denominator; integer comparisons retain
        // even tiny improving columns. There is no numerical pricing cutoff.
        missing.sort_unstable_by(|a, b| a.1.cmp(b.1).then_with(|| a.0.cmp(&b.0)));
        Ok(missing.into_iter().take(64).map(|(j, _)| j).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Column, rational};
    fn columns(costs: &[f64]) -> Vec<Column> {
        costs
            .iter()
            .map(|cost| Column {
                lower: 0.0,
                upper: f64::INFINITY,
                cost: *cost,
            })
            .collect()
    }
    #[test]
    fn missing_supplier_ray_adds_original_columns_and_keeps_many_empty_rows() {
        let mut rows = vec![
            Row {
                equality: true,
                rhs: 1.0,
                terms: vec![(0, 1.0)],
            },
            Row {
                equality: false,
                rhs: 0.0,
                terms: vec![(0, 1.0), (1, -1.0)],
            },
        ];
        rows.extend((0..4200).map(|_| Row {
            equality: true,
            rhs: 0.0,
            terms: vec![],
        }));
        let model = Model {
            columns: columns(&[1.0, 2.0]),
            rows,
            offset: 0.0,
        };
        let result = model
            .solve_column_generation_with_control(
                Some(&[1.0, 0.0]),
                &Rational::zero(),
                &SolveControl::default(),
            )
            .unwrap();
        assert!(result.accepted && result.certificate.optimal);
        assert_eq!(result.solution.values, vec![rational(1.0).unwrap(); 2]);
        assert_eq!(result.certificate.objective, "3");
        assert_eq!(result.solution.row_duals.len(), 4202);
    }
    #[test]
    fn omitted_cheaper_column_is_added_before_accepting_a_restricted_optimum() {
        let model = Model {
            columns: columns(&[2.0, 1.0]),
            rows: vec![Row {
                equality: true,
                rhs: 1.0,
                terms: vec![(0, 1.0), (1, 1.0)],
            }],
            offset: 5.0,
        };
        let result = model
            .solve_column_generation_with_control(
                Some(&[1.0, 0.0]),
                &Rational::zero(),
                &SolveControl::default(),
            )
            .unwrap();
        assert!(result.accepted && result.certificate.optimal);
        assert_eq!(
            result.solution.values,
            vec![Rational::zero(), rational(1.0).unwrap()]
        );
        assert_eq!(result.certificate.objective, "6");
    }
    #[test]
    fn no_seed_can_prove_a_full_lossy_cycle_without_a_restricted_false_positive() {
        let model = Model {
            columns: columns(&[1.0, 1.0]),
            rows: vec![
                Row {
                    equality: true,
                    rhs: 1.0,
                    terms: vec![(0, 1.0)],
                },
                Row {
                    equality: false,
                    rhs: 0.0,
                    terms: vec![(0, 1.0), (1, -1.0)],
                },
                Row {
                    equality: false,
                    rhs: 0.0,
                    terms: vec![(1, 2.0), (0, -1.0)],
                },
            ],
            offset: 0.0,
        };
        let result = model
            .solve_column_generation_with_control(None, &Rational::zero(), &SolveControl::default())
            .unwrap();
        assert!(!result.accepted);
        assert!(result.infeasibility.is_some());
        assert!(
            model
                .verify_infeasibility(&result.solution.row_duals)
                .unwrap()
                .is_some()
        );
    }
    #[test]
    fn cancelled_column_generation_does_not_establish_infeasibility() {
        let model = Model {
            columns: columns(&[1.0]),
            rows: vec![Row {
                equality: true,
                rhs: 1.0,
                terms: vec![(0, 1.0)],
            }],
            offset: 0.0,
        };
        let control = SolveControl::with_deadline(Instant::now());
        let result = model.solve_column_generation_with_control(None, &Rational::zero(), &control);
        assert!(matches!(result,Err(ref error) if error.contains("cancelled")));
    }
}
