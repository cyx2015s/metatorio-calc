//! Complete-model feasibility and exact objective-gap certification.
use crate::{
    Basis, BasisStatus, Certificate, ExactSolution, FloatCandidate, InfeasibilityCertificate,
    Model, Rational, Row, rational,
};
use num_traits::{Signed, ToPrimitive, Zero};

pub struct CertifiedResult {
    pub solution: ExactSolution,
    /// Reconstructs the primal values; the certified dual may be independent.
    pub primal_basis: Basis,
    pub certificate: Certificate,
    pub relative_gap: Option<Rational>,
    pub accepted: bool,
    pub exact_pivots: usize,
    pub support_search_pivots: usize,
    pub support_columns: usize,
    pub stop_reason: Option<String>,
    /// Verified for the complete original model, never only a restriction.
    pub infeasibility: Option<InfeasibilityCertificate>,
}
impl Certificate {
    /// Exact normalization: gap / max(1, |primal objective|).
    pub fn relative_gap(&self) -> Result<Option<Rational>, String> {
        if !self.primal_feasible || !self.dual_feasible {
            return Ok(None);
        }
        let objective: Rational = self
            .objective
            .parse()
            .map_err(|_| "invalid exact objective")?;
        let gap: Rational = self
            .objective_gap
            .as_ref()
            .ok_or("missing exact gap")?
            .parse()
            .map_err(|_| "invalid exact gap")?;
        if gap.is_negative() {
            return Err("negative primal-dual gap".into());
        }
        Ok(Some(gap / objective.abs().max(rational(1.0)?)))
    }
    /// Round upward for reporting. Acceptance always uses the rational value.
    pub fn relative_gap_upper_bound(&self) -> Result<Option<f64>, String> {
        let Some(gap) = self.relative_gap()? else {
            return Ok(None);
        };
        let mut value = gap.to_f64().ok_or("gap cannot be represented")?;
        if !value.is_finite() {
            return Err("gap exceeds finite reporting range".into());
        }
        if rational(value)? < gap {
            value = value.next_up();
            if !value.is_finite() {
                return Err("gap has no finite upper reporting bound".into());
            }
        }
        Ok(Some(value))
    }
}

impl Model {
    /// Refine a Rust floating hint, accepting only full-model exact feasibility
    /// and an exact comparison against the caller's objective-gap budget.
    /// Requires nonnegative costs and nonnegative variables without upper bounds.
    /// Observer phases distinguish support work from full-model work. Floating
    /// tolerances and support selection never relax an original constraint.
    pub fn certify_candidate_with_observer(
        &self,
        candidate: &FloatCandidate,
        max_pivots: usize,
        gap_budget: &Rational,
        observer: impl FnMut(&str, usize) -> bool,
    ) -> Result<CertifiedResult, String> {
        self.certify_candidate_with_control(
            candidate,
            max_pivots,
            gap_budget,
            observer,
            &crate::SolveControl::default(),
        )
    }
    /// Cooperative interruption is separate from progress stops and may return
    /// Err during a reconstruction, without creating a partial certificate.
    pub fn certify_candidate_with_control(
        &self,
        candidate: &FloatCandidate,
        max_pivots: usize,
        gap_budget: &Rational,
        mut observer: impl FnMut(&str, usize) -> bool,
        control: &crate::SolveControl,
    ) -> Result<CertifiedResult, String> {
        control.check()?;
        if gap_budget.is_negative() {
            return Err("negative objective-gap budget".into());
        }
        if !observer("reconstruct", 0) {
            return Err("certified solve cancelled before reconstruction".into());
        }
        // Degenerate floating bases may retain hundreds of zero-flow columns.
        // Rebase a much smaller support before factoring that large exact block.
        // A restricted failure proves nothing here; all acceptance and further
        // refinement still use self, with every original column restored.
        let basics = candidate
            .basis
            .columns
            .iter()
            .filter(|s| **s == BasisStatus::Basic)
            .count();
        // Block triangular reconstruction can make a large degenerate basis
        // cheap. Check its full witness before replacing it with a support
        // hint, which may lose useful original-model dual prices.
        if basics >= 32 {
            if let Ok(original) = self.reconstruct_with_control(&candidate.basis, control) {
                let (adjusted, scale) = self.scale_to_dual_feasible(&original)?;
                let result = self.certified_result(
                    adjusted,
                    candidate.basis.clone(),
                    gap_budget,
                    0,
                    self.columns.len(),
                )?;
                if result.accepted {
                    return Ok(result);
                }
                // A one-sided feasible original basis already has a direct
                // exact repair path. Support rebasing can discard its useful
                // dual witness and turn a tiny bound repair into cold search.
                if result.certificate.primal_feasible || scale == rational(1.0)? {
                    if !observer("verify", 0) || !observer("full-refine", 0) {
                        return Ok(CertifiedResult {
                            stop_reason: Some(
                                "certified observer requested stop before full refinement".into(),
                            ),
                            ..result
                        });
                    }
                    let mut primal_basis = candidate.basis.clone();
                    let mut feasibility_pivots = 0;
                    if !result.certificate.primal_feasible
                        && scale == rational(1.0)?
                        && !gap_budget.is_zero()
                    {
                        // Original duals already provide a rigorous lower
                        // bound. Repair only primal feasibility with stable
                        // zero-cost pivots, then check the ORIGINAL gap before
                        // paying for exact original-cost optimization.
                        let mut feasibility = self.clone();
                        for column in &mut feasibility.columns {
                            column.cost = 0.0;
                        }
                        feasibility.offset = 0.0;
                        let repaired = feasibility.repair_dual_feasible_basis_with_control(
                            primal_basis,
                            crate::DualPivotRule::FirstViolationLargePivot,
                            max_pivots,
                            |p| observer("full-refine", p.pivots),
                            control,
                        )?;
                        feasibility_pivots = repaired.pivots;
                        primal_basis = repaired.basis;
                        let mut combined = self.certified_result(
                            ExactSolution {
                                row_duals: result.solution.row_duals.clone(),
                                ..repaired.solution
                            },
                            primal_basis.clone(),
                            gap_budget,
                            feasibility_pivots,
                            self.columns.len(),
                        )?;
                        combined.stop_reason = repaired.stop_reason;
                        // Costs were the only auxiliary change; Farkas
                        // multipliers still cover every original constraint.
                        combined.infeasibility = repaired.infeasibility;
                        if combined.accepted
                            || combined.infeasibility.is_some()
                            || !combined.certificate.primal_feasible
                        {
                            return Ok(combined);
                        }
                    }
                    let refined = self.refine_basis_with_control(
                        primal_basis,
                        max_pivots.saturating_sub(feasibility_pivots),
                        |_, p| observer("full-refine", feasibility_pivots + p.pivots),
                        control,
                    )?;
                    let pivots =
                        feasibility_pivots + refined.repair_pivots + refined.optimization_pivots;
                    let mut full = self.certified_result(
                        refined.solution.clone(),
                        refined.basis.clone(),
                        gap_budget,
                        pivots,
                        self.columns.len(),
                    )?;
                    let combined = self.certified_result(
                        ExactSolution {
                            row_duals: result.solution.row_duals,
                            ..refined.solution
                        },
                        refined.basis,
                        gap_budget,
                        pivots,
                        self.columns.len(),
                    )?;
                    if combined
                        .relative_gap
                        .as_ref()
                        .is_some_and(|g| full.relative_gap.as_ref().is_none_or(|old| g < old))
                    {
                        full = combined;
                    }
                    full.stop_reason = refined.stop_reason;
                    full.infeasibility = refined.infeasibility;
                    return Ok(full);
                }
            }
            control.check()?;
        }
        let support = candidate.values.iter().filter(|v| **v != 0.0).count();
        let compacted = if basics >= 32
            && support.saturating_mul(2) < basics
            && observer("compact-search", 0)
        {
            // This only proposes a basis, so a small deficit must not pull an
            // entire high-degree supplier row into the support. Missing columns
            // are restored before reconstruction/refinement against self.
            let (restricted, selected) = self.candidate_support(&candidate.values, false)?;
            if selected.len() < basics {
                match restricted.float_candidate_with_options(
                    crate::FloatOptions {
                        ratio_tolerance: 1e-12,
                        primal_tolerance: 1e-12,
                    },
                    20000,
                    |p| !control.is_cancelled() && observer("compact-search", p.pivots),
                ) {
                    Ok(hint) => {
                        let mut basis = Basis {
                            columns: vec![BasisStatus::Lower; self.columns.len()],
                            rows: hint.basis.rows,
                        };
                        let mut values = vec![0.0; self.columns.len()];
                        for (new, &old) in selected.iter().enumerate() {
                            basis.columns[old] = hint.basis.columns[new];
                            values[old] = hint.values[new];
                        }
                        Some(FloatCandidate {
                            basis,
                            values,
                            pivots: hint.pivots,
                            stop_reason: hint.stop_reason,
                            diagnostic: hint.diagnostic,
                        })
                    }
                    Err(_) => None,
                }
            } else {
                None
            }
        } else {
            None
        };
        let original_candidate = candidate;
        let mut candidate = compacted.as_ref().unwrap_or(original_candidate);
        let mut compacted_used = compacted.is_some();
        control.check()?;
        let initial = match self.reconstruct_with_control(&candidate.basis, control) {
            Ok(solution) => solution,
            Err(_) if compacted.is_some() && !control.is_cancelled() => {
                candidate = original_candidate;
                compacted_used = false;
                self.reconstruct_with_control(&candidate.basis, control)?
            }
            Err(error) => return Err(error),
        };
        let (adjusted, _) = self.scale_to_dual_feasible(&initial)?;
        let compact_pivots = compacted.as_ref().map_or(0, |hint| hint.pivots);
        let mut initial_result = self.certified_result(
            adjusted.clone(),
            candidate.basis.clone(),
            gap_budget,
            0,
            self.columns.len(),
        )?;
        initial_result.support_search_pivots = compact_pivots;
        control.check()?;
        if initial_result.accepted {
            return Ok(initial_result);
        }
        if !observer("verify", 0) {
            return Ok(CertifiedResult {
                stop_reason: Some("certified observer requested stop".into()),
                ..initial_result
            });
        }
        let mut primal_basis = candidate.basis.clone();
        let mut exact_pivots = 0;
        let mut support_columns = self.columns.len();
        let mut support_search_pivots = compact_pivots;
        if !initial_result.certificate.primal_feasible && !compacted_used {
            if !observer("support-search", 0) {
                return Ok(CertifiedResult {
                    stop_reason: Some(
                        "certified observer requested stop before support search".into(),
                    ),
                    ..initial_result
                });
            }
            let (restricted, selected) = self.candidate_support(&candidate.values, true)?;
            support_columns = selected.len();
            let proposal = restricted.float_candidate_with_observer(20000, |p| {
                !control.is_cancelled() && observer("support-search", p.pivots)
            })?;
            support_search_pivots += proposal.pivots;
            let repaired = restricted.refine_basis_with_control(
                proposal.basis,
                max_pivots,
                |_, p| observer("support-refine", p.pivots),
                control,
            )?;
            exact_pivots += repaired.repair_pivots + repaired.optimization_pivots;
            if repaired.certificate.primal_feasible {
                let mut values = vec![Rational::zero(); self.columns.len()];
                primal_basis = Basis {
                    columns: vec![BasisStatus::Lower; self.columns.len()],
                    rows: repaired.basis.rows,
                };
                for (new, old) in selected.iter().enumerate() {
                    values[*old] = repaired.solution.values[new].clone();
                    primal_basis.columns[*old] = repaired.basis.columns[new];
                }
                let combined = ExactSolution {
                    values,
                    row_duals: adjusted.row_duals.clone(),
                    basis_dimension: repaired.solution.basis_dimension,
                };
                let mut result = self.certified_result(
                    combined,
                    primal_basis.clone(),
                    gap_budget,
                    exact_pivots,
                    support_columns,
                )?;
                result.support_search_pivots = support_search_pivots;
                if result.accepted {
                    return Ok(result);
                }
                if !observer("verify", exact_pivots) {
                    return Ok(CertifiedResult {
                        stop_reason: Some("certified observer requested stop".into()),
                        ..result
                    });
                }
            }
            // Failure of a restricted model is not an original-model proof.
            // The remaining full-model refinement retains all original columns.
        }
        if !observer("full-refine", exact_pivots) {
            return Ok(CertifiedResult {
                exact_pivots,
                support_search_pivots,
                support_columns,
                stop_reason: Some(
                    "certified observer requested stop before full refinement".into(),
                ),
                ..initial_result
            });
        }
        let refined = self.refine_basis_with_control(
            primal_basis,
            max_pivots.saturating_sub(exact_pivots),
            |_, p| observer("full-refine", exact_pivots + p.pivots),
            control,
        )?;
        exact_pivots += refined.repair_pivots + refined.optimization_pivots;
        let mut result = self.certified_result(
            refined.solution.clone(),
            refined.basis.clone(),
            gap_budget,
            exact_pivots,
            support_columns,
        )?;
        if !result.accepted {
            let combined = ExactSolution {
                row_duals: adjusted.row_duals,
                ..refined.solution
            };
            let other = self.certified_result(
                combined,
                refined.basis,
                gap_budget,
                exact_pivots,
                support_columns,
            )?;
            if other
                .relative_gap
                .as_ref()
                .is_some_and(|g| result.relative_gap.as_ref().is_none_or(|old| g < old))
            {
                result = other;
            }
        }
        result.stop_reason = if result.accepted || refined.infeasibility.is_some() {
            None
        } else {
            refined
                .stop_reason
                .or(Some("objective-gap budget not established".into()))
        };
        result.support_search_pivots = support_search_pivots;
        result.infeasibility = refined.infeasibility;
        Ok(result)
    }
    fn certified_result(
        &self,
        solution: ExactSolution,
        primal_basis: Basis,
        budget: &Rational,
        exact_pivots: usize,
        support_columns: usize,
    ) -> Result<CertifiedResult, String> {
        let certificate = self.certify(&solution)?;
        let relative_gap = certificate.relative_gap()?;
        let accepted = relative_gap.as_ref().is_some_and(|gap| gap <= budget);
        Ok(CertifiedResult {
            solution,
            primal_basis,
            certificate,
            relative_gap,
            accepted,
            exact_pivots,
            support_search_pivots: 0,
            support_columns,
            stop_reason: None,
            infeasibility: None,
        })
    }
    fn candidate_support(
        &self,
        values: &[f64],
        include_deficits: bool,
    ) -> Result<(Self, Vec<usize>), String> {
        if values.len() != self.columns.len() || values.iter().any(|x| !x.is_finite()) {
            return Err("support requires a finite full vector".into());
        }
        // Compact rebasing is only a hint. Exclude negative/noise-size flows
        // from that search, then restore EVERY original column before exact
        // reconstruction and repair. This cutoff is not a feasibility tolerance
        // and may omit a genuinely necessary tiny supplier.
        let mut keep: Vec<_> = values
            .iter()
            .map(|x| {
                if include_deficits {
                    *x != 0.0
                } else {
                    *x > 1e-9
                }
            })
            .collect();
        for row in self.rows.iter().filter(|_| include_deficits) {
            let mut sum = 0.0f64;
            let mut correction = 0.0;
            let mut magnitude = 0.0;
            for (j, a) in &row.terms {
                let product = a * values[*j];
                correction += a.mul_add(values[*j], -product);
                let next = sum + product;
                correction += if sum.abs() >= product.abs() {
                    (sum - next) + product
                } else {
                    (product - next) + sum
                };
                sum = next;
                magnitude += product.abs();
            }
            let residual = if row.equality {
                (sum + correction - row.rhs).abs()
            } else {
                (sum + correction - row.rhs).max(0.0)
            };
            if !residual.is_finite() || residual > 1e-6 * magnitude.max(row.rhs.abs()) + 1e-8 {
                for (j, _) in &row.terms {
                    keep[*j] = true;
                }
            }
        }
        let selected: Vec<_> = keep
            .iter()
            .enumerate()
            .filter_map(|(i, k)| k.then_some(i))
            .collect();
        let mut remap = vec![None; keep.len()];
        for (new, old) in selected.iter().enumerate() {
            remap[*old] = Some(new);
        }
        Ok((
            Self {
                columns: selected.iter().map(|j| self.columns[*j].clone()).collect(),
                rows: self
                    .rows
                    .iter()
                    .map(|r| Row {
                        equality: r.equality,
                        rhs: r.rhs,
                        terms: r
                            .terms
                            .iter()
                            .filter_map(|(j, a)| remap[*j].map(|k| (k, *a)))
                            .collect(),
                    })
                    .collect(),
                offset: self.offset,
            },
            selected,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Column;
    #[test]
    fn degenerate_hint_restores_omitted_cheaper_and_tiny_supplier_columns() {
        let mut model = Model {
            columns: vec![
                Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost: 3.0
                };
                34
            ],
            rows: vec![],
            offset: 0.0,
        };
        model.columns[0].cost = 2.0;
        model.columns[1].cost = 1.0;
        model.rows.push(Row {
            equality: true,
            rhs: 1.0,
            terms: vec![(0, 1.0), (1, 1.0)],
        });
        for j in 2..34 {
            model.rows.push(Row {
                equality: true,
                rhs: 0.0,
                terms: vec![(j, 1.0)],
            });
        }
        let mut basis = Basis {
            columns: vec![BasisStatus::Basic; 34],
            rows: vec![BasisStatus::Upper; 33],
        };
        basis.columns[1] = BasisStatus::Lower;
        let mut values = vec![0.0; 34];
        values[0] = 1.0;
        let hint = FloatCandidate {
            basis,
            values,
            pivots: 0,
            stop_reason: "test hint".into(),
            diagnostic: None,
        };
        let stopped = model
            .certify_candidate_with_observer(
                &hint,
                100,
                &Rational::new(1.into(), 1_000_000_000_000u64.into()),
                |phase, _| phase != "full-refine",
            )
            .unwrap();
        assert!(!stopped.accepted);
        assert!(stopped.certificate.primal_feasible && stopped.certificate.dual_feasible);
        assert_eq!(stopped.certificate.objective, "2");
        let result = model
            .certify_candidate_with_observer(
                &hint,
                100,
                &Rational::new(1.into(), 1_000_000_000_000u64.into()),
                |_, _| true,
            )
            .unwrap();
        assert!(result.accepted && result.certificate.optimal);
        assert!(result.solution.values[0].is_zero());
        assert_eq!(result.solution.values[1], rational(1.0).unwrap());
        assert_eq!(result.certificate.objective, "1");

        // The small support lacks a required supplier and is infeasible. Its
        // stopped floating hint must be repaired on the full original model.
        model.columns[0].cost = 1.0;
        model.rows[0].terms = vec![(0, 1.0)];
        model.rows.insert(
            1,
            Row {
                equality: false,
                rhs: 0.0,
                terms: vec![(0, 1.0), (1, -1.0)],
            },
        );
        let mut missing_supplier = hint;
        missing_supplier.basis.rows.insert(1, BasisStatus::Basic);
        let stopped = model
            .certify_candidate_with_observer(
                &missing_supplier,
                100,
                &Rational::zero(),
                |phase, _| phase != "full-refine",
            )
            .unwrap();
        assert!(!stopped.accepted && stopped.infeasibility.is_none());
        let repaired = model
            .certify_candidate_with_observer(&missing_supplier, 100, &Rational::zero(), |_, _| true)
            .unwrap();
        assert!(repaired.accepted && repaired.certificate.optimal);
        assert_eq!(repaired.certificate.objective, "2");
        assert_eq!(repaired.solution.values[0], rational(1.0).unwrap());
        assert_eq!(repaired.solution.values[1], rational(1.0).unwrap());

        // A positive flow below the rebasing cutoff can be indispensable.
        // Omitting it must cause full repair, never a false infeasibility or
        // acceptance with a relaxed original constraint.
        model.rows[1].terms[1].1 = -1e15;
        missing_supplier.values[1] = 1e-15;
        let stopped = model
            .certify_candidate_with_observer(
                &missing_supplier,
                100,
                &Rational::zero(),
                |phase, _| phase != "full-refine",
            )
            .unwrap();
        assert!(!stopped.accepted && stopped.infeasibility.is_none());
        let repaired = model
            .certify_candidate_with_observer(&missing_supplier, 100, &Rational::zero(), |_, _| true)
            .unwrap();
        assert!(repaired.accepted && repaired.certificate.optimal);
        assert_eq!(
            repaired.solution.values[1],
            rational(1.0).unwrap() / rational(1e15).unwrap()
        );
    }
    #[test]
    fn auxiliary_primal_optimum_must_pass_original_gap_or_continue_original_optimization() {
        let mut model = Model {
            columns: (0..35)
                .map(|_| Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost: 3.0,
                })
                .collect(),
            rows: vec![Row {
                equality: true,
                rhs: -1.0,
                terms: vec![(0, 1.0), (1, -1.0), (2, -1000.0)],
            }],
            offset: 0.0,
        };
        model.columns[0].cost = 0.0;
        model.columns[1].cost = 1.0;
        model.columns[2].cost = 10000.0;
        for j in 3..35 {
            model.rows.push(Row {
                equality: true,
                rhs: 0.0,
                terms: vec![(j, 1.0)],
            });
        }
        let mut basis = Basis {
            columns: vec![BasisStatus::Basic; 35],
            rows: vec![BasisStatus::Upper; 33],
        };
        basis.columns[1] = BasisStatus::Lower;
        basis.columns[2] = BasisStatus::Lower;
        let mut values = vec![0.0; 35];
        values[0] = -1.0;
        let hint = FloatCandidate {
            basis,
            values,
            pivots: 0,
            stop_reason: "test hint".into(),
            diagnostic: None,
        };
        let budget = Rational::new(1.into(), 1_000_000_000_000u64.into());
        let stopped = model
            .certify_candidate_with_observer(&hint, 1, &budget, |_, _| true)
            .unwrap();
        assert!(stopped.certificate.primal_feasible && stopped.certificate.dual_feasible);
        assert_eq!(stopped.certificate.objective, "10");
        assert!(!stopped.accepted && stopped.infeasibility.is_none());
        let result = model
            .certify_candidate_with_observer(&hint, 10, &budget, |_, _| true)
            .unwrap();
        assert!(result.accepted && result.certificate.optimal);
        assert_eq!(result.certificate.objective, "1");
        assert_eq!(result.solution.values[1], rational(1.0).unwrap());
    }
    #[test]
    fn full_infeasibility_certificate_survives_the_combined_entry_point() {
        let model = Model {
            columns: vec![],
            rows: vec![Row {
                equality: true,
                rhs: 1.0,
                terms: vec![],
            }],
            offset: 0.0,
        };
        let candidate = model.float_candidate_with_observer(20, |_| true).unwrap();
        let result = model
            .certify_candidate_with_observer(&candidate, 20, &Rational::zero(), |_, _| true)
            .unwrap();
        assert!(!result.accepted && !result.certificate.primal_feasible);
        let proof = result
            .infeasibility
            .expect("missing original-model Farkas proof");
        let mut dual = vec![Rational::zero(); model.rows.len()];
        for (i, y) in proof.row_duals {
            dual[i] = y.parse().unwrap();
        }
        assert!(model.verify_infeasibility(&dual).unwrap().is_some());
        assert!(result.stop_reason.is_none());
    }
    #[test]
    fn rational_budget_boundary_and_display_rounding_are_separate() {
        let model = Model {
            columns: [1.0, 0.125]
                .into_iter()
                .map(|cost| Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost,
                })
                .collect(),
            rows: vec![Row {
                equality: true,
                rhs: 1.0,
                terms: vec![(0, 7.0), (1, 1.0)],
            }],
            offset: 0.0,
        };
        let candidate = FloatCandidate {
            basis: Basis {
                columns: vec![BasisStatus::Basic, BasisStatus::Lower],
                rows: vec![BasisStatus::Upper],
            },
            values: vec![1.0 / 7.0, 0.0],
            pivots: 0,
            stop_reason: "test hint".into(),
            diagnostic: None,
        };
        let gap = Rational::new(1.into(), 56.into());
        let result = model
            .certify_candidate_with_observer(&candidate, 0, &gap, |_, _| true)
            .unwrap();
        assert!(result.accepted && !result.certificate.optimal);
        assert_eq!(result.relative_gap.as_ref(), Some(&gap));
        let reported = result
            .certificate
            .relative_gap_upper_bound()
            .unwrap()
            .unwrap();
        assert!(rational(reported).unwrap() >= gap);
        assert!(rational(reported.next_down()).unwrap() < gap);
        let tighter = &gap - Rational::new(1.into(), crate::BigInt::from(1) << 200usize);
        let stopped = model
            .certify_candidate_with_observer(&candidate, 20, &tighter, |phase, _| {
                phase != "full-refine"
            })
            .unwrap();
        assert!(!stopped.accepted && stopped.exact_pivots == 0);
        assert!(stopped.certificate.primal_feasible && stopped.certificate.dual_feasible);
        assert_eq!(stopped.relative_gap.as_ref(), Some(&gap));
        let result = model
            .certify_candidate_with_observer(&candidate, 0, &tighter, |_, _| true)
            .unwrap();
        assert!(!result.accepted);
        assert!(result.certificate.primal_feasible && result.certificate.dual_feasible);
    }
    #[test]
    fn gap_budget_is_exact_and_does_not_claim_zero_gap() {
        let model = Model {
            columns: [1.0, 1.0 - 2.0f64.powi(-40)]
                .into_iter()
                .map(|cost| Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost,
                })
                .collect(),
            rows: vec![Row {
                equality: true,
                rhs: 1.0,
                terms: vec![(0, 1.0), (1, 1.0)],
            }],
            offset: 0.0,
        };
        let candidate = FloatCandidate {
            basis: Basis {
                columns: vec![BasisStatus::Basic, BasisStatus::Lower],
                rows: vec![BasisStatus::Upper],
            },
            values: vec![1.0, 0.0],
            pivots: 0,
            stop_reason: "test hint".into(),
            diagnostic: None,
        };
        let budget = Rational::new(1.into(), 1_000_000_000_000u64.into());
        let result = model
            .certify_candidate_with_observer(&candidate, 20, &budget, |_, _| true)
            .unwrap();
        assert!(
            result.accepted
                && result.certificate.primal_feasible
                && result.certificate.dual_feasible
        );
        assert!(!result.certificate.optimal);
        assert_eq!(result.exact_pivots, 0);
        let gap = result.relative_gap.unwrap();
        assert!(gap > Rational::zero() && gap <= budget);
        assert!(
            rational(
                result
                    .certificate
                    .relative_gap_upper_bound()
                    .unwrap()
                    .unwrap()
            )
            .unwrap()
                >= gap
        );
        let result = model
            .certify_candidate_with_observer(&candidate, 20, &Rational::zero(), |_, _| true)
            .unwrap();
        assert!(result.accepted && result.certificate.optimal);
        assert_eq!(result.exact_pivots, 1);
    }
    #[test]
    fn infeasible_restriction_is_not_a_full_model_infeasibility_claim() {
        let model = Model {
            columns: [0.0, 1.0]
                .into_iter()
                .map(|cost| Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost,
                })
                .collect(),
            rows: vec![Row {
                equality: true,
                rhs: 1e-15,
                terms: vec![(0, -1.0), (1, 1.0)],
            }],
            offset: 0.0,
        };
        let candidate = FloatCandidate {
            basis: Basis {
                columns: vec![BasisStatus::Basic, BasisStatus::Lower],
                rows: vec![BasisStatus::Upper],
            },
            values: vec![0.0, 0.0],
            pivots: 0,
            stop_reason: "misleading hint".into(),
            diagnostic: None,
        };
        let mut full = false;
        let stopped = model
            .certify_candidate_with_observer(&candidate, 20, &Rational::zero(), |phase, _| {
                phase != "support-search"
            })
            .unwrap();
        assert!(!stopped.accepted && !stopped.certificate.primal_feasible);
        assert_eq!(stopped.support_search_pivots, 0);
        assert_eq!(stopped.exact_pivots, 0);
        assert!(stopped.infeasibility.is_none());
        assert_eq!(
            stopped.stop_reason.as_deref(),
            Some("certified observer requested stop before support search")
        );
        let result = model
            .certify_candidate_with_observer(&candidate, 20, &Rational::zero(), |phase, _| {
                full |= phase == "full-refine";
                true
            })
            .unwrap();
        assert!(full && result.accepted && result.certificate.optimal);
        assert_eq!(result.support_columns, 0);
        assert_eq!(result.solution.values[1], rational(1e-15).unwrap());
        assert!(
            model
                .certify_candidate_with_observer(&candidate, 20, &Rational::zero(), |_, _| false)
                .is_err()
        );
    }
}
