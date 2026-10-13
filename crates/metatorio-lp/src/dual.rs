//! Exact dual-feasible basis repair for nonnegative planning models.
use super::{
    Basis, BasisStatus, BigInt, ExactSolution, InfeasibilityCertificate, Model, PivotProgress,
    Rational, basis, rational,
};
use num_integer::Integer;
use num_traits::{Signed, Zero};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DualPivotRule {
    FirstViolation,
    LargestViolation,
    FirstViolationLargePivot,
    SteepestEdge,
}

#[derive(Debug)]
pub struct DualRepair {
    pub solution: ExactSolution,
    pub basis: Basis,
    pub pivots: usize,
    pub stop_reason: Option<String>,
    pub infeasibility: Option<InfeasibilityCertificate>,
}
impl Model {
    /// Start without a candidate basis or artificial variables. Nonnegative
    /// costs make the zero structural solution dual feasible; fixed equality
    /// slacks can initially violate either side of zero.
    pub fn solve_dual_with_observer(
        &self,
        max_pivots: usize,
        observer: impl FnMut(PivotProgress) -> bool,
    ) -> Result<DualRepair, String> {
        self.solve_dual_with_rule(DualPivotRule::FirstViolation, max_pivots, observer)
    }
    pub fn solve_dual_with_rule(
        &self,
        rule: DualPivotRule,
        max_pivots: usize,
        observer: impl FnMut(PivotProgress) -> bool,
    ) -> Result<DualRepair, String> {
        if self.columns.iter().any(|c| c.cost < 0.0) {
            return Err("dual cold start requires nonnegative objective costs".into());
        }
        self.repair_dual_inner(
            Basis {
                columns: vec![BasisStatus::Lower; self.columns.len()],
                rows: vec![BasisStatus::Basic; self.rows.len()],
            },
            max_pivots,
            observer,
            true,
            rule,
        )
    }
    pub fn repair_dual_feasible_basis(
        &self,
        basis: Basis,
        max_pivots: usize,
    ) -> Result<DualRepair, String> {
        self.repair_dual_feasible_basis_with_observer(basis, max_pivots, |_| true)
    }
    /// Preserve dual feasibility while restoring primal feasibility. The input
    /// basis must be exactly dual feasible; a rejected hint is not infeasibility.
    /// Failed ratio tests require an independently verified original Farkas ray.
    pub fn repair_dual_feasible_basis_with_observer(
        &self,
        basis: Basis,
        max_pivots: usize,
        observer: impl FnMut(PivotProgress) -> bool,
    ) -> Result<DualRepair, String> {
        self.repair_dual_inner(
            basis,
            max_pivots,
            observer,
            false,
            DualPivotRule::FirstViolation,
        )
    }
    fn repair_dual_inner(
        &self,
        mut basis: Basis,
        max_pivots: usize,
        mut observer: impl FnMut(PivotProgress) -> bool,
        cache: bool,
        rule: DualPivotRule,
    ) -> Result<DualRepair, String> {
        self.check()?;
        if self
            .columns
            .iter()
            .any(|c| c.lower != 0.0 || c.upper != f64::INFINITY)
        {
            return Err("dual repair requires nonnegative columns without upper bounds".into());
        }

        // Cold start has an empty inverse, so caching avoids repeated solves
        // without paying the expensive dense initialization of a warm basis.
        let mut system = if cache {
            Some(basis::System::new(self, &basis)?)
        } else {
            None
        };
        let mut seen = HashSet::new();
        for iteration in 0..=max_pivots {
            let solution = match &system {
                Some(system) => system.reconstruct(self)?,
                None => self.reconstruct(&basis)?,
            };
            let certificate = self.certify(&solution)?;
            if !certificate.dual_feasible {
                return Err(
                    "basis is not exactly dual feasible; use Phase I or primal optimization".into(),
                );
            }
            if certificate.optimal {
                return Ok(DualRepair {
                    solution,
                    basis,
                    pivots: iteration,
                    stop_reason: None,
                    infeasibility: None,
                });
            }
            let repeated = !seen.insert(
                basis
                    .columns
                    .iter()
                    .chain(&basis.rows)
                    .map(|s| *s as u8)
                    .collect::<Vec<_>>(),
            );
            let proceed = observer(PivotProgress {
                pivots: iteration,
                basis_dimension: solution.basis_dimension,
            });
            if repeated || !proceed || iteration == max_pivots {
                let reason = if repeated {
                    "dual repair repeated a basis"
                } else if !proceed {
                    "dual repair observer requested stop"
                } else {
                    "dual repair pivot limit reached"
                };
                return Ok(DualRepair {
                    solution,
                    basis,
                    pivots: iteration,
                    stop_reason: Some(reason.into()),
                    infeasibility: None,
                });
            }
            // Smallest indexed violating basic variable; fixed equality slacks
            // may violate either side of their zero bound.
            let mut leaving = solution
                .values
                .iter()
                .enumerate()
                .find(|(_, x)| x.is_negative())
                .map(|(i, _)| (i, true));
            if leaving.is_none() {
                for (i, row) in self.rows.iter().enumerate() {
                    if basis.rows[i] != BasisStatus::Basic {
                        continue;
                    }
                    let mut slack = rational(row.rhs)?;
                    for (column, a) in &row.terms {
                        if !solution.values[*column].is_zero() {
                            slack -= rational(*a)? * &solution.values[*column];
                        }
                    }
                    if slack.is_negative() || (row.equality && !slack.is_zero()) {
                        leaving = Some((self.columns.len() + i, slack.is_negative()));
                        break;
                    }
                }
            }
            let (leaving, increase) = if matches!(
                rule,
                DualPivotRule::LargestViolation | DualPivotRule::SteepestEdge
            ) {
                self.priced_dual_violation(&basis, &solution, rule, system.as_ref())?
            } else {
                leaving.ok_or("failed primal check has no violating basic variable")?
            };
            let mut ray = match &system {
                Some(system) => system.cached_functional(self, leaving)?,
                None => basis::System::functional(self, &basis, leaving)?,
            };
            if increase {
                for y in &mut ray {
                    *y = -y.clone();
                }
            }
            let ray_solution = ExactSolution {
                values: vec![Rational::zero(); self.columns.len()],
                row_duals: ray,
                basis_dimension: 0,
            };
            let directions = self.pricing(&ray_solution, true);
            let reduced = self.reduced_costs(&solution);
            // All reduced costs share one positive denominator; all directional
            // coefficients share another. Compare ratios by integer products.
            let mut entering: Option<(usize, BigInt, BigInt)> = None;
            let mut consider = |index: usize, cost: BigInt, positive_direction: BigInt| {
                let better = entering.as_ref().is_none_or(|(old, c, d)| {
                    (&cost * d)
                        .cmp(&(c * &positive_direction))
                        .then_with(|| {
                            if rule == DualPivotRule::FirstViolationLargePivot {
                                d.cmp(&positive_direction)
                            } else {
                                std::cmp::Ordering::Equal
                            }
                        })
                        .then_with(|| index.cmp(old))
                        .is_lt()
                });
                if better {
                    entering = Some((index, cost, positive_direction));
                }
            };
            for (i, d) in directions.numerators.iter().enumerate() {
                if basis.columns[i] != BasisStatus::Basic && d.is_negative() {
                    consider(i, reduced.numerators[i].clone(), -d);
                }
            }
            for (i, row) in self.rows.iter().enumerate() {
                if row.equality || basis.rows[i] == BasisStatus::Basic {
                    continue;
                }
                let direction_dual = &ray_solution.row_duals[i];
                if direction_dual.is_positive() {
                    let y = &solution.row_duals[i];
                    let cost = -y.numer() * (&reduced.denominator / y.denom());
                    let direction =
                        direction_dual.numer() * (&directions.denominator / direction_dual.denom());
                    consider(self.columns.len() + i, cost, direction);
                }
            }
            let Some((entering, _, _)) = entering else {
                let proof = self
                    .verify_infeasibility(&ray_solution.row_duals)?
                    .ok_or("dual ratio failure has no original Farkas certificate")?;
                return Ok(DualRepair {
                    solution,
                    basis,
                    pivots: iteration,
                    stop_reason: None,
                    infeasibility: Some(proof),
                });
            };
            if solution.basis_dimension >= super::MAX_BASIS_DIMENSION
                && entering < self.columns.len()
                && leaving >= self.columns.len()
            {
                return Ok(DualRepair {
                    solution,
                    basis,
                    pivots: iteration,
                    infeasibility: None,
                    stop_reason: Some("exact basis dimension limit reached".into()),
                });
            }
            if leaving < self.columns.len() {
                basis.columns[leaving] = BasisStatus::Lower;
            } else {
                basis.rows[leaving - self.columns.len()] = BasisStatus::Upper;
            }
            if entering < self.columns.len() {
                basis.columns[entering] = BasisStatus::Basic;
            } else {
                basis.rows[entering - self.columns.len()] = BasisStatus::Basic;
            }
            if let Some(system) = &mut system {
                system.update(self, &basis, entering, leaving)?;
            }
        }
        unreachable!()
    }
    fn priced_dual_violation(
        &self,
        basis: &Basis,
        solution: &ExactSolution,
        rule: DualPivotRule,
        system: Option<&basis::System>,
    ) -> Result<(usize, bool), String> {
        let mut best: Option<(usize, bool, Rational)> = None;
        let mut consider = |index, increase, magnitude: Rational| -> Result<(), String> {
            let magnitude = if rule == DualPivotRule::SteepestEdge {
                let ray = match system {
                    Some(system) => system.cached_functional(self, index)?,
                    None => basis::System::functional(self, basis, index)?,
                };
                edge_merit(&magnitude, &ray)?
            } else {
                magnitude
            };
            if best.as_ref().is_none_or(|(_, _, old)| magnitude > *old) {
                best = Some((index, increase, magnitude));
            }
            Ok(())
        };
        for (i, value) in solution.values.iter().enumerate() {
            if value.is_negative() {
                consider(i, true, -value)?;
            }
        }
        for (i, row) in self.rows.iter().enumerate() {
            if basis.rows[i] != BasisStatus::Basic {
                continue;
            }
            let mut slack = rational(row.rhs)?;
            for (column, a) in &row.terms {
                if !solution.values[*column].is_zero() {
                    slack -= rational(*a)? * &solution.values[*column];
                }
            }
            if slack.is_negative() || (row.equality && !slack.is_zero()) {
                consider(self.columns.len() + i, slack.is_negative(), slack.abs())?;
            }
        }
        best.map(|(i, increase, _)| (i, increase))
            .ok_or("no violating basic variable".into())
    }
}

// Squared violation / squared norm of the full inverse-basis row. Accumulate
// the norm with shared-denominator integers, avoiding a GCD per squared term.
fn edge_merit(violation: &Rational, row: &[Rational]) -> Result<Rational, String> {
    let denominator = row.iter().fold(BigInt::from(1), |d, x| d.lcm(x.denom()));
    let norm: BigInt = row
        .iter()
        .filter(|x| !x.is_zero())
        .map(|x| {
            let n = x.numer() * (&denominator / x.denom());
            &n * &n
        })
        .sum();
    if norm.is_zero() {
        return Err("zero inverse-basis row norm".into());
    }
    let n = violation.numer() * denominator;
    Ok(Rational::new(
        &n * &n,
        violation.denom() * violation.denom() * norm,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Column, Row};
    #[test]
    fn dimension_limits_preserve_primal_and_dual_candidates() {
        let n = crate::MAX_BASIS_DIMENSION + 1;
        let mut model = Model {
            columns: (0..n).map(|_| column(1.0)).collect(),
            rows: (0..n)
                .map(|i| Row {
                    equality: true,
                    rhs: 1.0,
                    terms: vec![(i, 1.0)],
                })
                .collect(),
            offset: 0.0,
        };
        let mut basis = Basis {
            columns: vec![BasisStatus::Basic; n],
            rows: vec![BasisStatus::Upper; n],
        };
        basis.columns[n - 1] = BasisStatus::Lower;
        basis.rows[n - 1] = BasisStatus::Basic;
        let result = model.repair_dual_feasible_basis(basis.clone(), 10).unwrap();
        assert_eq!(result.pivots, 0);
        assert!(result.stop_reason.unwrap().contains("dimension"));
        assert_eq!(result.basis.columns, basis.columns);
        assert_eq!(result.basis.rows, basis.rows);
        assert!(model.certify(&result.solution).unwrap().dual_feasible);
        assert!(!model.certify(&result.solution).unwrap().primal_feasible);
        for c in &mut model.columns {
            c.cost = 0.0;
        }
        model.columns[n - 1].cost = -1.0;
        for r in &mut model.rows {
            r.equality = false;
        }
        let result = model.optimize_from_basis(basis.clone(), 10).unwrap();
        assert_eq!(result.pivots, 0);
        assert!(result.stop_reason.unwrap().contains("dimension"));
        assert_eq!(result.basis.columns, basis.columns);
        assert!(model.certify(&result.solution).unwrap().primal_feasible);
        assert!(!model.certify(&result.solution).unwrap().optimal);
    }
    #[test]
    fn integer_edge_merit_matches_direct_rational_norm() {
        let row = vec![
            rational(0.0).unwrap(),
            Rational::new(2.into(), 3.into()),
            Rational::new((-5).into(), 7.into()),
        ];
        let violation = Rational::new(11.into(), 13.into());
        let norm: Rational = row.iter().map(|x| x * x).sum();
        assert_eq!(
            edge_merit(&violation, &row).unwrap(),
            &violation * &violation / norm
        );
        assert!(edge_merit(&violation, &[]).is_err());
    }
    #[test]
    fn dual_cold_start_solves_a_production_chain_without_artificials() {
        let model = Model {
            columns: vec![column(3.0), column(1.0), column(0.0)],
            rows: vec![
                Row {
                    equality: true,
                    rhs: 1e-15,
                    terms: vec![(0, 1.0), (2, 1.0)],
                },
                Row {
                    equality: false,
                    rhs: 0.0,
                    terms: vec![(2, 2.0), (1, -1.0)],
                },
            ],
            offset: 0.0,
        };
        let result = model.solve_dual_with_observer(20, |_| true).unwrap();
        assert!(model.certify(&result.solution).unwrap().optimal);
        let largest = model
            .solve_dual_with_rule(DualPivotRule::LargestViolation, 20, |_| true)
            .unwrap();
        assert!(model.certify(&largest.solution).unwrap().optimal);
        assert_eq!(
            model.certify(&result.solution).unwrap().objective,
            model.certify(&largest.solution).unwrap().objective
        );
        let stable = model
            .solve_dual_with_rule(DualPivotRule::FirstViolationLargePivot, 20, |_| true)
            .unwrap();
        assert!(model.certify(&stable.solution).unwrap().optimal);
        let edge = model
            .solve_dual_with_rule(DualPivotRule::SteepestEdge, 20, |_| true)
            .unwrap();
        assert!(model.certify(&edge.solution).unwrap().optimal);
        assert!(result.solution.values[0].is_zero());
        assert_eq!(result.solution.values[2], rational(1e-15).unwrap());
        let mut infeasible = model.clone();
        infeasible.rows.push(Row {
            equality: false,
            rhs: -1.0,
            terms: vec![],
        });
        assert!(
            infeasible
                .solve_dual_with_observer(20, |_| true)
                .unwrap()
                .infeasibility
                .is_some()
        );
        let mut negative_cost = model;
        negative_cost.columns[0].cost = -1.0;
        assert!(
            negative_cost
                .solve_dual_with_observer(20, |_| true)
                .is_err()
        );
    }
    fn column(cost: f64) -> Column {
        Column {
            lower: 0.0,
            upper: f64::INFINITY,
            cost,
        }
    }
    #[test]
    fn repair_negative_basic_column_and_negative_slack() {
        let model = Model {
            columns: vec![column(0.0), column(1.0)],
            rows: vec![Row {
                equality: true,
                rhs: -1.0,
                terms: vec![(0, 1.0), (1, -1.0)],
            }],
            offset: 0.0,
        };
        let result = model
            .repair_dual_feasible_basis(
                Basis {
                    columns: vec![BasisStatus::Basic, BasisStatus::Lower],
                    rows: vec![BasisStatus::Upper],
                },
                10,
            )
            .unwrap();
        assert_eq!(result.pivots, 1);
        assert!(model.certify(&result.solution).unwrap().optimal);
        assert_eq!(result.solution.values[1].to_string(), "1");
        let model = Model {
            columns: vec![column(1.0)],
            rows: vec![Row {
                equality: false,
                rhs: -1.0,
                terms: vec![(0, -1.0)],
            }],
            offset: 0.0,
        };
        let result = model
            .repair_dual_feasible_basis(
                Basis {
                    columns: vec![BasisStatus::Lower],
                    rows: vec![BasisStatus::Basic],
                },
                10,
            )
            .unwrap();
        assert!(model.certify(&result.solution).unwrap().optimal);
    }
    #[test]
    fn fixed_equality_slack_repaired_from_both_sides() {
        for sign in [-1.0, 1.0] {
            let model = Model {
                columns: vec![column(1.0)],
                rows: vec![Row {
                    equality: true,
                    rhs: sign,
                    terms: vec![(0, sign)],
                }],
                offset: 0.0,
            };
            let result = model
                .repair_dual_feasible_basis(
                    Basis {
                        columns: vec![BasisStatus::Lower],
                        rows: vec![BasisStatus::Basic],
                    },
                    10,
                )
                .unwrap();
            assert!(model.certify(&result.solution).unwrap().optimal);
            assert_eq!(result.solution.values[0].to_string(), "1");
        }
    }
    #[test]
    fn ratio_failure_proves_infeasibility_and_limits_do_not() {
        let model = Model {
            columns: vec![column(1.0)],
            rows: vec![Row {
                equality: false,
                rhs: -1.0,
                terms: vec![(0, 1.0)],
            }],
            offset: 0.0,
        };
        let basis = Basis {
            columns: vec![BasisStatus::Lower],
            rows: vec![BasisStatus::Basic],
        };
        let stopped = model.repair_dual_feasible_basis(basis.clone(), 0).unwrap();
        assert!(stopped.stop_reason.is_some());
        assert!(stopped.infeasibility.is_none());
        let result = model.repair_dual_feasible_basis(basis, 10).unwrap();
        assert!(result.infeasibility.is_some());
        let ray: Vec<Rational> = result
            .infeasibility
            .unwrap()
            .row_duals
            .iter()
            .map(|(_, s)| s.parse().unwrap())
            .collect();
        assert!(model.verify_infeasibility(&ray).unwrap().is_some());
    }
    #[test]
    fn ratio_test_chooses_the_best_scaled_column_even_for_tiny_deficits() {
        let model = Model {
            columns: vec![column(0.0), column(3.0), column(4.0)],
            rows: vec![Row {
                equality: true,
                rhs: -1e-30,
                terms: vec![(0, 1.0), (1, -1.0), (2, -2.0)],
            }],
            offset: 0.0,
        };
        let result = model
            .repair_dual_feasible_basis(
                Basis {
                    columns: vec![BasisStatus::Basic, BasisStatus::Lower, BasisStatus::Lower],
                    rows: vec![BasisStatus::Upper],
                },
                10,
            )
            .unwrap();
        assert!(model.certify(&result.solution).unwrap().optimal);
        assert!(result.solution.values[1].is_zero());
        assert_eq!(
            result.solution.values[2],
            rational(1e-30).unwrap() / rational(2.0).unwrap()
        );
    }
    #[test]
    fn slack_entering_can_shrink_or_replace_the_active_basis() {
        let model = Model {
            columns: vec![column(1.0)],
            rows: vec![Row {
                equality: false,
                rhs: 1.0,
                terms: vec![(0, -1.0)],
            }],
            offset: 0.0,
        };
        let result = model
            .repair_dual_feasible_basis(
                Basis {
                    columns: vec![BasisStatus::Basic],
                    rows: vec![BasisStatus::Upper],
                },
                10,
            )
            .unwrap();
        assert_eq!(result.solution.basis_dimension, 0);
        assert!(model.certify(&result.solution).unwrap().optimal);
        let model = Model {
            columns: vec![column(-1.0)],
            rows: vec![
                Row {
                    equality: false,
                    rhs: 1.0,
                    terms: vec![(0, 1.0)],
                },
                Row {
                    equality: false,
                    rhs: 0.0,
                    terms: vec![(0, 1.0)],
                },
            ],
            offset: 0.0,
        };
        let result = model
            .repair_dual_feasible_basis(
                Basis {
                    columns: vec![BasisStatus::Basic],
                    rows: vec![BasisStatus::Upper, BasisStatus::Basic],
                },
                10,
            )
            .unwrap();
        assert_eq!(
            result.basis.rows,
            vec![BasisStatus::Basic, BasisStatus::Upper]
        );
        assert!(model.certify(&result.solution).unwrap().optimal);
    }
}
