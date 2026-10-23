//! Adaptive structural cold starts. Only original-model checks can accept them.
use crate::{
    Basis, BasisStatus, Certificate, ExactSolution, FloatCandidate, FloatOptions, FloatProgress,
    Model, RouteOptions, SolveControl, restricted::RestrictedIndex,
};
use num_traits::{Signed, Zero};
use std::time::{Duration, Instant};

pub struct RouteCandidate {
    pub candidate: FloatCandidate,
    pub solution: ExactSolution,
    pub certificate: Certificate,
    pub selected_columns: Vec<usize>,
    pub rounds: usize,
}

impl Model {
    /// Find an original-model primal-feasible starting basis without a saved
    /// solution. Failure or cancellation establishes no terminal LP status.
    pub fn float_candidate_after_route_with_control(
        &self,
        options: FloatOptions,
        max_pivots: usize,
        mut observer: impl FnMut(FloatProgress) -> bool,
        control: &SolveControl,
    ) -> Result<RouteCandidate, String> {
        control.check()?;
        self.check()?;
        let began = Instant::now();
        let index = RestrictedIndex::new(self, control)?;
        let mut included = vec![false; self.columns.len()];
        let mut requests: Vec<_> = self
            .rows
            .iter()
            .enumerate()
            .filter_map(|(i, row)| {
                if row.rhs < 0.0 {
                    Some((i, false))
                } else if row.equality && row.rhs > 0.0 {
                    Some((i, true))
                } else {
                    None
                }
            })
            .collect();
        let mut width = RouteOptions::default().directions_per_row;
        let mut pivots = 0usize;
        for round in 1usize.. {
            control.check()?;
            let previous_requests = requests.len();
            let route_options = RouteOptions {
                directions_per_row: width,
                variants_per_direction: if width >= self.columns.len() {
                    self.columns.len().max(1)
                } else {
                    width.div_ceil(2)
                },
                ..RouteOptions::default()
            };
            for j in self.route_columns_for_rows_with_control(&requests, route_options, control)? {
                included[j] = true;
            }
            let selected: Vec<_> = included
                .iter()
                .enumerate()
                .filter_map(|(j, yes)| yes.then_some(j))
                .collect();
            eprintln!(
                "[LP-ROUTE] round={round} columns={} width={width} requested_rows={} seconds={:.3}",
                selected.len(),
                requests.len(),
                began.elapsed().as_secs_f64()
            );
            let (working, rows) = index.build(&selected, control)?;
            let child = control.limited_to(Instant::now() + Duration::from_secs(2));
            let mut stopped = false;
            let mut observed_pivots = 0;
            let result = working.float_candidate_after_relevance_with_control(
                options,
                max_pivots.saturating_sub(pivots),
                |p| {
                    observed_pivots = observed_pivots.max(p.pivots);
                    if child.is_cancelled() {
                        return false;
                    }
                    let keep_going = observer(FloatProgress {
                        pivots: pivots + p.pivots,
                        structural_columns: p.structural_columns,
                    });
                    stopped |= !keep_going;
                    keep_going
                },
                &child,
            );
            if stopped {
                return Err(
                    "structural route observer requested stop; no terminal status established"
                        .into(),
                );
            }
            let reported_pivots = result.as_ref().map_or(0, |hint| hint.pivots);
            pivots += observed_pivots.max(reported_pivots);
            if let Err(error) = &result {
                eprintln!("[LP-ROUTE] round={round} floating_hint_error={error}");
            }
            if let Ok(mut hint) = result {
                // A floating stop may hide an exact equality residual. Repair
                // the small master at zero objective before lifting, so even
                // sub-tolerance disposal flows can enter its basis.
                let repair_control = control.limited_to(Instant::now() + Duration::from_secs(3));
                if let Ok(initial) = working.reconstruct_with_control(&hint.basis, &repair_control)
                {
                    if !working.certify(&initial)?.primal_feasible {
                        let mut feasibility = working.clone();
                        for column in &mut feasibility.columns {
                            column.cost = 0.0;
                        }
                        feasibility.offset = 0.0;
                        let mut repair_observed = 0;
                        let repaired = feasibility.repair_dual_feasible_basis_with_control(
                            hint.basis.clone(),
                            crate::DualPivotRule::FirstViolationLargePivot,
                            max_pivots.saturating_sub(pivots),
                            |p| {
                                repair_observed = repair_observed.max(p.pivots);
                                let keep_going = observer(FloatProgress {
                                    pivots: pivots + p.pivots,
                                    structural_columns: p.basis_dimension,
                                });
                                stopped |= !keep_going;
                                keep_going
                            },
                            &repair_control,
                        );
                        pivots += repair_observed.max(repaired.as_ref().map_or(0, |r| r.pivots));
                        if stopped {
                            return Err("structural route observer requested stop; no terminal status established".into());
                        }
                        if let Ok(repaired) = repaired {
                            hint.basis = repaired.basis;
                            hint.values = repaired.solution.approximate()?;
                        }
                    }
                }
                let mut basis = Basis {
                    columns: vec![BasisStatus::Lower; self.columns.len()],
                    rows: vec![BasisStatus::Basic; self.rows.len()],
                };
                let mut values = vec![0.0; self.columns.len()];
                for (new, &old) in selected.iter().enumerate() {
                    basis.columns[old] = hint.basis.columns[new];
                    values[old] = hint.values[new];
                }
                for (new, &old) in rows.iter().enumerate() {
                    basis.rows[old] = hint.basis.rows[new];
                }
                let mut candidate = FloatCandidate {
                    basis,
                    values,
                    pivots,
                    ..hint
                };
                let exact_control = control.limited_to(Instant::now() + Duration::from_secs(3));
                if let Ok(solution) =
                    self.reconstruct_with_control(&candidate.basis, &exact_control)
                {
                    control.check()?;
                    let certificate = self.certify(&solution)?;
                    control.check()?;
                    if certificate.primal_feasible {
                        candidate.values = solution.approximate()?;
                        eprintln!(
                            "[LP-ROUTE] first_feasible_s={:.6} rounds={round} columns={} pivots={pivots}",
                            began.elapsed().as_secs_f64(),
                            selected.len()
                        );
                        return Ok(RouteCandidate {
                            candidate,
                            solution,
                            certificate,
                            selected_columns: selected,
                            rounds: round,
                        });
                    }
                    // Exact, clamped activity exposes missing directions even
                    // when the residual is far below floating tolerances.
                    // This point is solely a search request, never a witness.
                    for (i, row) in self.rows.iter().enumerate() {
                        control.check()?;
                        let mut activity = crate::Rational::zero();
                        for &(j, a) in &row.terms {
                            if a != 0.0 && solution.values[j].is_positive() {
                                activity += crate::rational(a)? * &solution.values[j];
                            }
                        }
                        let rhs = crate::rational(row.rhs)?;
                        if activity > rhs {
                            requests.push((i, false));
                        } else if row.equality && activity < rhs {
                            requests.push((i, true));
                        }
                    }
                }
                // Clamping is only for deficit ranking. It is never used as a
                // feasible point or a certificate, and leaves the LP unchanged.
                for (i, row) in self.rows.iter().enumerate() {
                    control.check()?;
                    let mut lhs = 0.0;
                    let mut magnitude = row.rhs.abs();
                    for &(j, a) in &row.terms {
                        let term = a * candidate.values[j].max(0.0);
                        lhs += term;
                        magnitude += term.abs();
                    }
                    let delta = lhs - row.rhs;
                    if delta > 1e-10 * magnitude.max(f64::MIN_POSITIVE) {
                        requests.push((i, false));
                    } else if row.equality && -delta > 1e-10 * magnitude.max(f64::MIN_POSITIVE) {
                        requests.push((i, true));
                    }
                }
                requests.sort_unstable();
                requests.dedup();
            }
            if requests.len() == previous_requests {
                // A singular/unfinished floating hint may have no usable
                // residual. A small exact Phase I can still expose missing
                // directions. Its restricted proof is never a full LP status.
                let mut feasibility = working.clone();
                for column in &mut feasibility.columns {
                    column.cost = 0.0;
                }
                feasibility.offset = 0.0;
                let proof_control = control.limited_to(Instant::now() + Duration::from_secs(2));
                let mut proof_observed = 0;
                let repaired = feasibility.solve_dual_with_control(
                    crate::DualPivotRule::FirstViolationLargePivot,
                    max_pivots.saturating_sub(pivots),
                    |p| {
                        proof_observed = proof_observed.max(p.pivots);
                        let keep_going = observer(FloatProgress {
                            pivots: pivots + p.pivots,
                            structural_columns: p.basis_dimension,
                        });
                        stopped |= !keep_going;
                        keep_going
                    },
                    &proof_control,
                );
                pivots += proof_observed.max(repaired.as_ref().map_or(0, |r| r.pivots));
                if stopped {
                    return Err(
                        "structural route observer requested stop; no terminal status established"
                            .into(),
                    );
                }
                if let Ok(repaired) = repaired {
                    if let Some(proof) = repaired.infeasibility {
                        for (i, value) in proof.row_duals {
                            let value: crate::Rational = value
                                .parse()
                                .map_err(|_| "invalid restricted route multiplier")?;
                            if !value.is_zero() {
                                requests.push((rows[i], value.is_positive()));
                            }
                        }
                    } else if feasibility.certify(&repaired.solution)?.primal_feasible {
                        let mut basis = Basis {
                            columns: vec![BasisStatus::Lower; self.columns.len()],
                            rows: vec![BasisStatus::Basic; self.rows.len()],
                        };
                        for (new, &old) in selected.iter().enumerate() {
                            basis.columns[old] = repaired.basis.columns[new];
                        }
                        for (new, &old) in rows.iter().enumerate() {
                            basis.rows[old] = repaired.basis.rows[new];
                        }
                        if let Ok(solution) = self.reconstruct_with_control(&basis, control) {
                            let certificate = self.certify(&solution)?;
                            control.check()?;
                            if certificate.primal_feasible {
                                let candidate = FloatCandidate {
                                    basis,
                                    values: solution.approximate()?,
                                    pivots,
                                    stop_reason:
                                        "exact route primal feasible; full optimization required"
                                            .into(),
                                    diagnostic: None,
                                };
                                eprintln!(
                                    "[LP-ROUTE] first_feasible_s={:.6} rounds={round} columns={} pivots={pivots} exact_phase_one=true",
                                    began.elapsed().as_secs_f64(),
                                    selected.len()
                                );
                                return Ok(RouteCandidate {
                                    candidate,
                                    solution,
                                    certificate,
                                    selected_columns: selected,
                                    rounds: round,
                                });
                            }
                        }
                    }
                }
                requests.sort_unstable();
                requests.dedup();
            }
            control.check()?;
            if pivots >= max_pivots
                || (width >= self.columns.len().max(1) && requests.len() == previous_requests)
            {
                return Err(
                    "structural route search incomplete; no terminal status established".into(),
                );
            }
            width = width.saturating_mul(2).min(self.columns.len().max(1));
        }
        unreachable!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Column, Row};

    fn quality_model() -> Model {
        Model {
            columns: [0.25, 0.5, 0.5]
                .into_iter()
                .map(|cost| Column {
                    cost,
                    lower: 0.0,
                    upper: f64::INFINITY,
                })
                .collect(),
            rows: vec![
                Row {
                    equality: false,
                    rhs: 0.0,
                    terms: vec![(0, -1.0), (2, 1.0)],
                },
                Row {
                    equality: false,
                    rhs: 0.0,
                    terms: vec![(1, -1.0), (2, 1.0)],
                },
                Row {
                    equality: true,
                    rhs: 1.0,
                    terms: vec![(2, 0.125)],
                },
            ],
            offset: 0.0,
        }
    }

    #[test]
    fn cold_quality_route_checks_original_primal_and_cost() {
        let model = quality_model();
        let route = model
            .float_candidate_after_route_with_control(
                FloatOptions::default(),
                100,
                |_| true,
                &SolveControl::default(),
            )
            .unwrap();
        assert!(route.certificate.primal_feasible);
        assert_eq!(route.certificate.objective, "10");
        assert!(
            model
                .certify(&model.reconstruct(&route.candidate.basis).unwrap())
                .unwrap()
                .optimal
        );
    }

    #[test]
    fn observer_stop_and_cancellation_do_not_establish_infeasibility() {
        let model = quality_model();
        let stopped = model
            .float_candidate_after_route_with_control(
                FloatOptions::default(),
                100,
                |_| false,
                &SolveControl::default(),
            )
            .err()
            .unwrap();
        assert!(stopped.contains("observer requested stop"));
        let cancelled = model
            .float_candidate_after_route_with_control(
                FloatOptions::default(),
                100,
                |_| true,
                &SolveControl::with_deadline(Instant::now()),
            )
            .err()
            .unwrap();
        assert!(cancelled.contains("cancelled"));
    }

    #[test]
    fn exact_tiny_equality_deficit_adds_disposal_despite_float_tolerance() {
        let model = Model {
            // Unrelated columns prevent immediate all-column saturation, so
            // the tiny missing direction is discovered through feedback.
            columns: [1.0; 8]
                .into_iter()
                .map(|cost| Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost,
                })
                .collect(),
            rows: vec![
                Row {
                    equality: true,
                    rhs: 1.0,
                    terms: vec![(0, 1.0 + 2.0f64.powi(-46)), (1, -1.0)],
                },
                Row {
                    equality: true,
                    rhs: 1.0,
                    terms: vec![(0, 1.0)],
                },
            ],
            offset: 0.0,
        };
        let route = model
            .float_candidate_after_route_with_control(
                FloatOptions::default(),
                100,
                |_| true,
                &SolveControl::default(),
            )
            .unwrap();
        assert!(route.certificate.primal_feasible);
        assert!(route.rounds > 1);
        assert!(route.solution.values[1].is_positive());
    }

    #[test]
    fn saturated_width_keeps_a_required_expensive_same_pattern_variant() {
        let model = Model {
            columns: [0.1, 0.1, 0.1, 1.0]
                .into_iter()
                .map(|cost| Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost,
                })
                .collect(),
            rows: vec![
                Row {
                    equality: true,
                    rhs: 1.0,
                    terms: vec![(0, 1.0), (1, 1.0), (2, 1.0), (3, 1.0)],
                },
                Row {
                    equality: true,
                    rhs: 1.0,
                    terms: vec![(0, 2.0), (1, 3.0), (2, 4.0), (3, 0.5)],
                },
            ],
            offset: 0.0,
        };
        let route = model
            .float_candidate_after_route_with_control(
                FloatOptions::default(),
                200,
                |_| true,
                &SolveControl::default(),
            )
            .unwrap();
        assert!(route.certificate.primal_feasible);
        assert!(route.selected_columns.contains(&3));
        assert!(route.solution.values[3].is_positive());
    }

    #[test]
    fn exact_cold_basis_survives_floating_cost_scaling_overflow() {
        let model = Model {
            columns: vec![Column {
                lower: 0.0,
                upper: f64::INFINITY,
                cost: 1e200,
            }],
            rows: vec![Row {
                equality: true,
                rhs: 1.0,
                terms: vec![(0, 1e-300)],
            }],
            offset: 0.0,
        };
        assert!(
            model
                .float_candidate_with_options(FloatOptions::default(), 20, |_| true)
                .is_err()
        );
        let route = model
            .float_candidate_after_route_with_control(
                FloatOptions::default(),
                20,
                |_| true,
                &SolveControl::default(),
            )
            .unwrap();
        assert!(route.certificate.primal_feasible);
        assert!(
            model
                .certify(&model.reconstruct(&route.candidate.basis).unwrap())
                .unwrap()
                .optimal
        );
    }
}
