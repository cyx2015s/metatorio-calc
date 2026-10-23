//! Sign-closed relevance reduction for nonnegative-cost production LPs.
//! Rows use A x <= b (or equality). This is not ordinary backwards reachability:
//! selected inputs require every possible supplier, and equalities also require
//! all disposal directions. Cycles are retained without requiring a start seed.
use crate::{
    Basis, BasisStatus, ExactSolution, FloatCandidate, FloatOptions, FloatProgress, Model,
    Rational, Row, SolveControl, rational,
};
use num_traits::{Signed, Zero};
use std::collections::{BTreeMap, HashSet, VecDeque};

pub struct RelevanceReduction {
    pub model: Model,
    pub columns: Vec<usize>,
    pub rows: Vec<usize>,
    /// Original zero-cost columns with no consumption or equality effects.
    pub free_suppliers: Vec<usize>,
    original_columns: usize,
    original_rows: usize,
}

impl Model {
    /// Projecting a full feasible solution onto selected columns, and increasing
    /// retained free suppliers if needed, stays feasible and cannot raise cost.
    /// Conversely every reduced feasible solution lifts
    /// with omitted columns zero. Omitted rows have b>=0 and only nonpositive
    /// selected coefficients (or are homogeneous equalities with zero activity).
    /// A kept inequality omits only nonnegative coefficients unless a retained
    /// free supplier forces its dual to zero. Zero-extended reduced duals are
    /// therefore feasible for every original column as well.
    pub fn relevance_reduction_with_control(
        &self,
        control: &SolveControl,
    ) -> Result<RelevanceReduction, String> {
        control.check()?;
        self.check()?;
        if self
            .columns
            .iter()
            .any(|c| c.lower != 0.0 || c.upper != f64::INFINITY || c.cost < 0.0)
        {
            return Err(
                "relevance reduction requires nonnegative unbounded columns and nonnegative costs"
                    .into(),
            );
        }
        let mut signs = Vec::with_capacity(self.rows.len());
        let mut incidence = vec![Vec::new(); self.columns.len()];
        for (i, row) in self.rows.iter().enumerate() {
            control.check()?;
            let mut seen = HashSet::with_capacity(row.terms.len());
            let duplicate = row.terms.iter().any(|(j, _)| !seen.insert(*j));
            let row_signs: Vec<_> = if duplicate {
                // No floating aggregation: cancellation can hide a 1e-30 term.
                let mut sums = BTreeMap::<usize, Rational>::new();
                for (j, a) in &row.terms {
                    *sums.entry(*j).or_default() += rational(*a)?;
                }
                sums.into_iter()
                    .filter(|(_, a)| !a.is_zero())
                    .map(|(j, a)| (j, a.is_positive()))
                    .collect()
            } else {
                row.terms
                    .iter()
                    .filter(|(_, a)| *a != 0.0)
                    .map(|(j, a)| (*j, *a > 0.0))
                    .collect()
            };
            for &(j, positive) in &row_signs {
                incidence[j].push((i, positive));
            }
            signs.push(row_signs);
        }
        let mut needed = vec![[false; 2]; self.rows.len()];
        let free_suppliers: Vec<_> = incidence
            .iter()
            .enumerate()
            .filter(|(j, edges)| {
                self.columns[*j].cost == 0.0
                    && !edges.is_empty()
                    && edges
                        .iter()
                        .all(|(i, positive)| !positive && !self.rows[*i].equality)
            })
            .map(|(j, _)| j)
            .collect();
        let mut free_for_row = vec![None; self.rows.len()];
        for &j in &free_suppliers {
            for &(i, _) in &incidence[j] {
                free_for_row[i].get_or_insert(j);
            }
        }
        let mut keep = vec![false; self.columns.len()];
        let mut queue = VecDeque::new();
        // Work items are (row index, requested coefficient sign).
        let request = |i: usize,
                       positive: bool,
                       needed: &mut Vec<[bool; 2]>,
                       queue: &mut VecDeque<(usize, bool)>| {
            if !needed[i][positive as usize] {
                needed[i][positive as usize] = true;
                queue.push_back((i, positive));
            }
        };
        for (i, row) in self.rows.iter().enumerate() {
            if row.equality && row.rhs != 0.0 {
                request(i, false, &mut needed, &mut queue);
                request(i, true, &mut needed, &mut queue);
            } else if row.rhs < 0.0 {
                request(i, false, &mut needed, &mut queue);
            }
        }
        while let Some((i, positive)) = queue.pop_front() {
            control.check()?;
            for &(j, sign) in &signs[i] {
                // An unlimited zero-cost pure supplier can replace all other
                // providers without consuming another material or making waste.
                if !positive && free_for_row[i].is_some_and(|free| free != j) {
                    continue;
                }
                if sign != positive || keep[j] {
                    continue;
                }
                keep[j] = true;
                for &(affected, coefficient_positive) in &incidence[j] {
                    if self.rows[affected].equality {
                        request(affected, false, &mut needed, &mut queue);
                        request(affected, true, &mut needed, &mut queue);
                    } else if coefficient_positive {
                        request(affected, false, &mut needed, &mut queue);
                    }
                }
            }
        }
        let columns: Vec<_> = (0..keep.len()).filter(|j| keep[*j]).collect();
        let rows: Vec<_> = (0..self.rows.len())
            .filter(|i| needed[*i][0] || needed[*i][1])
            .collect();
        let mut positions = vec![None; keep.len()];
        for (j, old) in columns.iter().enumerate() {
            positions[*old] = Some(j);
        }
        let model = Model {
            columns: columns.iter().map(|j| self.columns[*j].clone()).collect(),
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
        Ok(RelevanceReduction {
            model,
            columns,
            rows,
            free_suppliers: free_suppliers.into_iter().filter(|j| keep[*j]).collect(),
            original_columns: self.columns.len(),
            original_rows: self.rows.len(),
        })
    }

    /// Search only the sign-closed relevant subsystem, then restore original
    /// identities. This hint is still subject to complete original certification.
    pub fn float_candidate_after_relevance_with_control(
        &self,
        options: FloatOptions,
        max_pivots: usize,
        mut observer: impl FnMut(FloatProgress) -> bool,
        control: &SolveControl,
    ) -> Result<FloatCandidate, String> {
        let reduced = self.relevance_reduction_with_control(control)?;
        eprintln!(
            "[LP-RELEVANCE] columns={} -> {} rows={} -> {}",
            self.columns.len(),
            reduced.columns.len(),
            self.rows.len(),
            reduced.rows.len()
        );
        let projected = reduced.model.free_supply_projection_with_control(control)?;
        eprintln!(
            "[LP-FREE-SUPPLY] columns={} -> {} rows={} -> {}",
            reduced.model.columns.len(),
            projected.model.columns.len(),
            reduced.model.rows.len(),
            projected.model.rows.len()
        );
        let hint = projected
            .model
            .float_candidate_with_options(options, max_pivots, |p| {
                !control.is_cancelled() && observer(p)
            })?;
        reduced.lift_candidate(projected.lift_candidate(hint)?)
    }
}

impl RelevanceReduction {
    pub fn lift_solution(&self, solution: &ExactSolution) -> Result<ExactSolution, String> {
        if solution.values.len() != self.columns.len()
            || solution.row_duals.len() != self.rows.len()
        {
            return Err("relevance solution dimension mismatch".into());
        }
        let mut values = vec![Rational::zero(); self.original_columns];
        let mut row_duals = vec![Rational::zero(); self.original_rows];
        for (j, old) in self.columns.iter().enumerate() {
            values[*old] = solution.values[j].clone();
        }
        for (i, old) in self.rows.iter().enumerate() {
            row_duals[*old] = solution.row_duals[i].clone();
        }
        Ok(ExactSolution {
            values,
            row_duals,
            basis_dimension: solution.basis_dimension,
        })
    }
    pub fn lift_candidate(&self, hint: FloatCandidate) -> Result<FloatCandidate, String> {
        if hint.basis.columns.len() != self.columns.len()
            || hint.values.len() != self.columns.len()
            || hint.basis.rows.len() != self.rows.len()
        {
            return Err("relevance hint dimension mismatch".into());
        }
        let mut basis = Basis {
            columns: vec![BasisStatus::Lower; self.original_columns],
            rows: vec![BasisStatus::Basic; self.original_rows],
        };
        let mut values = vec![0.0; self.original_columns];
        for (j, old) in self.columns.iter().enumerate() {
            basis.columns[*old] = hint.basis.columns[j];
            values[*old] = hint.values[j];
        }
        for (i, old) in self.rows.iter().enumerate() {
            basis.rows[*old] = hint.basis.rows[i];
        }
        Ok(FloatCandidate {
            basis,
            values,
            ..hint
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Column;
    fn row(equality: bool, rhs: f64, terms: &[(usize, f64)]) -> Row {
        Row {
            equality,
            rhs,
            terms: terms.to_vec(),
        }
    }
    fn model(costs: &[f64], rows: Vec<Row>) -> Model {
        Model {
            columns: costs
                .iter()
                .map(|cost| Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost: *cost,
                })
                .collect(),
            rows,
            offset: 0.0,
        }
    }
    fn reduce(m: &Model) -> RelevanceReduction {
        m.relevance_reduction_with_control(&SolveControl::default())
            .unwrap()
    }
    fn assert_same_optimum(m: &Model) {
        let reduced = reduce(m);
        let full = m.solve(1000).unwrap();
        let small = reduced.model.solve(1000).unwrap();
        if full.infeasibility.is_some() {
            let proof = small
                .infeasibility
                .expect("reduction must preserve infeasibility");
            let mut y = vec![Rational::zero(); m.rows.len()];
            for (i, value) in proof.row_duals {
                y[reduced.rows[i]] = value.parse().unwrap();
            }
            assert!(m.verify_infeasibility(&y).unwrap().is_some());
        } else {
            let full_certificate = m.certify(&full.phase2.unwrap().solution).unwrap();
            let lifted = reduced
                .lift_solution(&small.phase2.unwrap().solution)
                .unwrap();
            let certificate = m.certify(&lifted).unwrap();
            assert!(full_certificate.optimal && certificate.optimal);
            assert_eq!(full_certificate.objective, certificate.objective);
        }
    }
    #[test]
    fn disconnected_homogeneous_cycle_is_safely_zeroed() {
        let m = model(
            &[1.0, 0.0, 3.0],
            vec![
                row(false, -1.0, &[(0, -1.0)]),
                row(true, 0.0, &[(1, 1.0), (2, -1.0)]),
            ],
        );
        let reduced = reduce(&m);
        assert_eq!(reduced.columns, vec![0]);
        assert_eq!(reduced.rows, vec![0]);
        assert_same_optimum(&m);
    }
    #[test]
    fn free_unlimited_energy_removes_paid_generation_and_its_fuel_chain() {
        let m = model(
            &[1.0, 0.0, 2.0, 3.0],
            vec![
                row(false, -1.0, &[(0, -1.0)]),
                row(false, 0.0, &[(0, 1.0), (1, -1.0), (2, -1.0)]),
                row(false, 0.0, &[(2, 1.0), (3, -1.0)]),
                row(false, 0.0, &[(1, -2.0)]),
            ],
        );
        let reduced = reduce(&m);
        assert_eq!(reduced.columns, vec![0, 1]);
        assert_eq!(reduced.free_suppliers, vec![1]);
        assert_same_optimum(&m);
    }
    #[test]
    fn tiny_consumption_or_equality_disqualifies_a_free_supplier() {
        let mut m = model(
            &[1.0, 0.0, 2.0, 3.0],
            vec![
                row(false, -1.0, &[(0, -1.0)]),
                row(false, 0.0, &[(0, 1.0), (1, -1.0), (2, -1.0)]),
                row(false, 0.0, &[(1, 1e-30), (2, 1.0), (3, -1.0)]),
            ],
        );
        assert_eq!(reduce(&m).columns, vec![0, 1, 2, 3]);
        assert!(reduce(&m).free_suppliers.is_empty());
        m.rows[2].terms.remove(0);
        m.rows[1].equality = true;
        assert!(reduce(&m).free_suppliers.is_empty());
        assert_same_optimum(&m);
    }
    #[test]
    fn finite_supply_capacity_and_tiny_positive_cost_are_not_free() {
        let mut m = model(
            &[1.0, 0.0, 2.0],
            vec![
                row(false, -3.0, &[(0, -1.0)]),
                row(false, 0.0, &[(0, 1.0), (1, -1.0), (2, -1.0)]),
                row(false, 2.0, &[(1, 1.0)]),
            ],
        );
        assert!(reduce(&m).free_suppliers.is_empty());
        assert_eq!(reduce(&m).columns, vec![0, 1, 2]);
        assert_same_optimum(&m);
        m.rows.pop();
        m.columns[1].cost = 1e-30;
        assert!(reduce(&m).free_suppliers.is_empty());
        assert_eq!(reduce(&m).columns, vec![0, 1, 2]);
        assert_same_optimum(&m);
    }
    #[test]
    fn surplus_does_not_require_disposal_but_strict_output_does() {
        let mut m = model(
            &[1.0; 3],
            vec![
                row(false, -1.0, &[(0, -1.0)]),
                row(false, 0.0, &[(0, -2.0), (1, 1.0)]),
                row(false, 0.0, &[(1, 1.0), (2, -1.0)]),
            ],
        );
        assert_eq!(reduce(&m).columns, vec![0]);
        assert_same_optimum(&m);
        m.rows[1].equality = true;
        assert_eq!(reduce(&m).columns, vec![0, 1, 2]);
        assert_same_optimum(&m);
    }
    #[test]
    fn productive_supply_cycle_is_retained_without_external_seed() {
        let m = model(
            &[1.0; 4],
            vec![
                row(false, -1.0, &[(0, -1.0)]),
                row(false, 0.0, &[(0, 1.0), (1, -2.0), (2, 1.0)]),
                row(false, 0.0, &[(1, 1.0), (2, -1.0)]),
                row(false, 0.0, &[(3, -1.0)]),
            ],
        );
        assert_eq!(reduce(&m).columns, vec![0, 1, 2]);
        assert_same_optimum(&m);
    }
    #[test]
    fn upper_limits_and_positive_rhs_equalities_are_not_dropped() {
        let m = model(
            &[1.0; 2],
            vec![
                row(false, -3.0, &[(0, -1.0)]),
                row(false, 2.0, &[(0, 1.0)]),
                row(true, 1.0, &[(1, 1.0)]),
            ],
        );
        assert_eq!(reduce(&m).rows, vec![0, 1, 2]);
        assert_same_optimum(&m);
    }
    #[test]
    fn duplicate_cancellation_cannot_hide_tiny_required_supplier() {
        let m = model(
            &[1.0; 2],
            vec![
                row(false, -1.0, &[(0, -1.0)]),
                row(false, 0.0, &[(0, 1e20), (0, 1e-30), (0, -1e20), (1, -1.0)]),
            ],
        );
        assert_eq!(reduce(&m).columns, vec![0, 1]);
    }
    #[test]
    fn empty_contradiction_and_empty_relevant_model_are_preserved() {
        let m = model(&[1.0], vec![row(false, -1.0, &[])]);
        assert_same_optimum(&m);
        let m = model(&[0.0], vec![row(false, 1.0, &[(0, -1.0)])]);
        assert!(reduce(&m).columns.is_empty());
        assert_same_optimum(&m);
    }
    #[test]
    fn lifted_floating_basis_is_an_original_basis() {
        let m = model(
            &[1.0; 3],
            vec![
                row(false, -1.0, &[(0, -1.0)]),
                row(false, 0.0, &[(0, -1.0), (1, 1.0)]),
                row(true, 0.0, &[(2, 1.0)]),
            ],
        );
        let hint = m
            .float_candidate_after_relevance_with_control(
                FloatOptions::default(),
                100,
                |_| true,
                &SolveControl::default(),
            )
            .unwrap();
        assert_eq!(hint.basis.rows.len(), m.rows.len());
        assert_eq!(hint.basis.columns.len(), m.columns.len());
        assert!(
            m.certify(&m.reconstruct(&hint.basis).unwrap())
                .unwrap()
                .optimal
        );
    }
    #[test]
    fn generated_feasible_models_keep_exact_objective_and_full_dual_validity() {
        let mut state = 17u64;
        let mut next = || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            (state >> 33) as usize
        };
        for _ in 0..40 {
            let witness = [1.0, 2.0, 0.0];
            let mut rows = vec![row(false, -1.0, &[(0, -1.0)])];
            for i in 0..4 {
                let terms: Vec<_> = (0..3).map(|j| (j, (next() % 5) as f64 - 2.0)).collect();
                let activity: f64 = terms.iter().map(|(j, a)| a * witness[*j]).sum();
                let equality = i % 2 == 0;
                rows.push(row(
                    equality,
                    activity + if equality { 0.0 } else { (next() % 3) as f64 },
                    &terms,
                ));
            }
            rows.push(row(true, 0.0, &[(3, 1.0), (4, -1.0)]));
            let m = model(&[1.0, 2.0, 0.0, 0.0, 3.0], rows);
            assert!(reduce(&m).columns.iter().all(|j| *j < 3));
            assert_same_optimum(&m);
        }
    }
}
