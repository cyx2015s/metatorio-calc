//! Exact correction of an untrusted candidate basis, preserving full-model scope.
use crate::{
    Basis, BasisStatus, Certificate, DualPivotRule, ExactSolution, InfeasibilityCertificate, Model,
    PivotProgress, Rational, SolveControl, rational,
};
use num_traits::{Signed, ToPrimitive};
#[derive(Debug)]
pub struct Refinement {
    pub basis: Basis,
    pub solution: ExactSolution,
    pub certificate: Certificate,
    pub repair_pivots: usize,
    pub optimization_pivots: usize,
    pub stop_reason: Option<String>,
    pub infeasibility: Option<InfeasibilityCertificate>,
    pub unbounded_ray: Option<Vec<Rational>>,
}
impl Model {
    // Raise only offending nonbasic costs, preserving useful prices and the
    // basis duals. This is an auxiliary objective, never an optimality proof
    // for the original model. Fall back to zero costs when it cannot be made
    // dual feasible with finite binary coefficients.
    fn feasibility_objective(&self, basis: &Basis, solution: &ExactSolution) -> Self {
        let mut auxiliary = self.clone();
        let shifted = (|| -> Result<(), String> {
            let prices = self.reduced_costs(solution);
            for (i, price) in prices.numerators.iter().enumerate() {
                if !price.is_negative() {
                    continue;
                }
                if basis.columns[i] != BasisStatus::Lower
                    || self.columns[i].lower != 0.0
                    || self.columns[i].upper != f64::INFINITY
                {
                    return Err("unsupported cost shift".into());
                }
                let target = rational(self.columns[i].cost)?
                    - Rational::new(price.clone(), prices.denominator.clone());
                let mut cost = target.to_f64().ok_or("unrepresentable shifted cost")?;
                if rational(cost)? < target {
                    cost = cost.next_up();
                }
                rational(cost)?;
                auxiliary.columns[i].cost = cost;
            }
            if !auxiliary.certify(solution)?.dual_feasible {
                return Err("cost shifts do not correct row dual signs".into());
            }
            Ok(())
        })();
        if shifted.is_err() {
            for column in &mut auxiliary.columns {
                column.cost = 0.0;
            }
        }
        auxiliary.offset = 0.0;
        auxiliary
    }

    /// Correct a candidate on the complete original model. If both feasibility
    /// conditions fail, auxiliary costs provide a dual-feasible repair phase;
    /// the original objective is restored before any optimality claim.
    /// Observer phase=true means feasibility repair, false means optimization.
    pub fn refine_basis_with_observer(
        &self,
        basis: Basis,
        max_pivots: usize,
        observer: impl FnMut(bool, PivotProgress) -> bool,
    ) -> Result<Refinement, String> {
        self.refine_basis_with_rule(basis, DualPivotRule::FirstViolation, max_pivots, observer)
    }
    /// Select the exact repair strategy; final certification always uses self.
    pub fn refine_basis_with_rule(
        &self,
        basis: Basis,
        rule: DualPivotRule,
        max_pivots: usize,
        observer: impl FnMut(bool, PivotProgress) -> bool,
    ) -> Result<Refinement, String> {
        self.refine_basis_with_rule_control(
            basis,
            rule,
            max_pivots,
            observer,
            &SolveControl::default(),
        )
    }
    /// Unlike a pivot observer stop, cooperative interruption can return Err
    /// before a new exact solution is available. It never proves infeasibility.
    pub fn refine_basis_with_control(
        &self,
        basis: Basis,
        max_pivots: usize,
        observer: impl FnMut(bool, PivotProgress) -> bool,
        control: &SolveControl,
    ) -> Result<Refinement, String> {
        self.refine_basis_with_rule_control(
            basis,
            DualPivotRule::FirstViolation,
            max_pivots,
            observer,
            control,
        )
    }
    fn refine_basis_with_rule_control(
        &self,
        mut basis: Basis,
        rule: DualPivotRule,
        max_pivots: usize,
        mut observer: impl FnMut(bool, PivotProgress) -> bool,
        control: &SolveControl,
    ) -> Result<Refinement, String> {
        let mut solution = self.reconstruct_with_control(&basis, control)?;
        let mut certificate = self.certify(&solution)?;
        let mut repair_pivots = 0;
        if !certificate.primal_feasible {
            let preserve_objective = certificate.dual_feasible;
            let repaired = if preserve_objective {
                self.repair_dual_feasible_basis_with_control(
                    basis,
                    rule,
                    max_pivots,
                    |p| observer(true, p),
                    control,
                )?
            } else {
                let feasibility = self.feasibility_objective(&basis, &solution);
                feasibility.repair_dual_feasible_basis_with_control(
                    basis,
                    rule,
                    max_pivots,
                    |p| observer(true, p),
                    control,
                )?
            };
            repair_pivots = repaired.pivots;
            basis = repaired.basis;
            solution = if preserve_objective {
                repaired.solution
            } else {
                self.reconstruct_with_control(&basis, control)?
            };
            certificate = self.certify(&solution)?;
            if !certificate.primal_feasible {
                return Ok(Refinement {
                    basis,
                    solution,
                    certificate,
                    repair_pivots,
                    optimization_pivots: 0,
                    stop_reason: repaired.stop_reason,
                    infeasibility: repaired.infeasibility,
                    unbounded_ray: None,
                });
            }
        }
        if certificate.optimal {
            return Ok(Refinement {
                basis,
                solution,
                certificate,
                repair_pivots,
                optimization_pivots: 0,
                stop_reason: None,
                infeasibility: None,
                unbounded_ray: None,
            });
        }
        let optimized = self.optimize_without_inverse_with_control(
            basis,
            max_pivots.saturating_sub(repair_pivots),
            |p| {
                observer(
                    false,
                    PivotProgress {
                        pivots: p.pivots + repair_pivots,
                        ..p
                    },
                )
            },
            control,
        )?;
        let certificate = self.certify(&optimized.solution)?;
        Ok(Refinement {
            basis: optimized.basis,
            solution: optimized.solution,
            certificate,
            repair_pivots,
            optimization_pivots: optimized.pivots,
            stop_reason: optimized.stop_reason,
            infeasibility: None,
            unbounded_ray: optimized.unbounded_ray,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BasisStatus, Column, Row};
    #[test]
    fn auxiliary_cost_rounds_up_and_keeps_original_basis_prices() {
        let model = Model {
            columns: [1.0, 0.0]
                .into_iter()
                .map(|cost| Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost,
                })
                .collect(),
            rows: vec![Row {
                equality: true,
                rhs: -1.0,
                terms: vec![(0, 7.0), (1, 1.0)],
            }],
            offset: 9.0,
        };
        let basis = Basis {
            columns: vec![BasisStatus::Basic, BasisStatus::Lower],
            rows: vec![BasisStatus::Upper],
        };
        let solution = model.reconstruct(&basis).unwrap();
        assert!(!model.certify(&solution).unwrap().dual_feasible);
        let auxiliary = model.feasibility_objective(&basis, &solution);
        assert_eq!(auxiliary.columns[0].cost, 1.0);
        assert!(rational(auxiliary.columns[1].cost).unwrap() >= Rational::new(1.into(), 7.into()));
        assert_eq!(
            auxiliary.reconstruct(&basis).unwrap().row_duals,
            solution.row_duals
        );
        assert!(auxiliary.certify(&solution).unwrap().dual_feasible);
        assert_eq!(model.columns[1].cost, 0.0);
        assert_eq!(model.offset, 9.0);
    }
    #[test]
    fn refine_restores_original_costs_after_doubly_infeasible_start() {
        let model = Model {
            columns: [1.0, 0.0, 2.0]
                .into_iter()
                .map(|cost| Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost,
                })
                .collect(),
            rows: vec![Row {
                equality: true,
                rhs: -1.0,
                terms: vec![(0, 1.0), (1, 1.0), (2, -1.0)],
            }],
            offset: 7.0,
        };
        let basis = Basis {
            columns: vec![BasisStatus::Basic, BasisStatus::Lower, BasisStatus::Lower],
            rows: vec![BasisStatus::Upper],
        };
        let result = model
            .refine_basis_with_observer(basis.clone(), 20, |_, _| true)
            .unwrap();
        assert!(result.certificate.optimal);
        assert_eq!(result.certificate.objective, "9");
        assert!(result.repair_pivots > 0);
        let mut inequality = model.clone();
        inequality.rows[0].equality = false;
        let hint = inequality.reconstruct(&basis).unwrap();
        assert!(hint.row_duals[0].is_positive());
        let auxiliary = inequality.feasibility_objective(&basis, &hint);
        assert!(auxiliary.columns.iter().all(|c| c.cost == 0.0));
        let result = inequality
            .refine_basis_with_observer(basis, 20, |_, _| true)
            .unwrap();
        assert!(result.certificate.optimal);
        assert_eq!(result.certificate.objective, "9");
    }
    #[test]
    fn refine_optimizes_a_feasible_hint_and_preserves_unbounded_proof() {
        let model = Model {
            columns: [2.0, 1.0]
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
        let basis = Basis {
            columns: vec![BasisStatus::Basic, BasisStatus::Lower],
            rows: vec![BasisStatus::Upper],
        };
        let result = model
            .refine_basis_with_observer(basis, 20, |_, _| true)
            .unwrap();
        assert!(result.certificate.optimal);
        assert_eq!(result.optimization_pivots, 1);
        let model = Model {
            columns: vec![Column {
                lower: 0.0,
                upper: f64::INFINITY,
                cost: -1.0,
            }],
            rows: vec![],
            offset: 0.0,
        };
        let basis = Basis {
            columns: vec![BasisStatus::Lower],
            rows: vec![],
        };
        let result = model
            .refine_basis_with_observer(basis, 20, |_, _| true)
            .unwrap();
        assert!(result.unbounded_ray.is_some());
        assert!(!result.certificate.optimal);
    }
}
