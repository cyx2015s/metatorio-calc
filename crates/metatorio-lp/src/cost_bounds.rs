//! Exact domains for solutions no more expensive than a verified incumbent.
//! These are search restrictions, never original-model infeasibility proofs.
use crate::{
    ExactSolution, FloatCandidate, FloatOptions, Model, Rational, Row, SolveControl, rational,
};
use num_traits::{Signed, ToPrimitive, Zero};

#[derive(Debug)]
pub struct CostBounds {
    /// Objective without the constant offset.
    pub budget: Rational,
    pub lower: Vec<Rational>,
    /// None denotes an upper bound not established (not a large sentinel).
    pub upper: Vec<Option<Rational>>,
}

impl Model {
    /// Bound x_j by (budget - sum_{k != j} c_k lower_k) / c_j, then
    /// propagate exact row intervals. Zero costs are allowed and never divided by.
    pub fn cost_bounds_with_control(
        &self,
        incumbent: &[Rational],
        control: &SolveControl,
    ) -> Result<CostBounds, String> {
        control.check()?;
        self.check()?;
        if self
            .columns
            .iter()
            .any(|c| c.lower != 0.0 || c.upper != f64::INFINITY || c.cost < 0.0)
        {
            return Err(
                "cost bounds require nonnegative costs and nonnegative unbounded columns".into(),
            );
        }
        let witness = ExactSolution {
            values: incumbent.to_vec(),
            row_duals: vec![Rational::zero(); self.rows.len()],
            basis_dimension: 0,
        };
        if !self.certify(&witness)?.primal_feasible {
            return Err("cost bounds require an exactly feasible original-model incumbent".into());
        }
        let costs: Vec<_> = self
            .columns
            .iter()
            .map(|c| rational(c.cost))
            .collect::<Result<_, _>>()?;
        let budget = costs
            .iter()
            .zip(incumbent)
            .map(|(c, x)| c * x)
            .sum::<Rational>();
        let mut bounds = CostBounds {
            budget,
            lower: vec![Rational::zero(); self.columns.len()],
            upper: vec![None; self.columns.len()],
        };
        // A bounded number of passes limits rational bit growth on cyclic models.
        for _ in 0..8 {
            control.check()?;
            let mut changed = false;
            let used = costs
                .iter()
                .zip(&bounds.lower)
                .map(|(c, x)| c * x)
                .sum::<Rational>();
            let remaining = &bounds.budget - used;
            if remaining.is_negative() {
                return Err("inconsistent incumbent cost domains".into());
            }
            for (j, c) in costs.iter().enumerate() {
                if !c.is_zero() {
                    let upper = &bounds.lower[j] + &remaining / c;
                    changed |= tighten_upper(&mut bounds.upper[j], upper);
                }
            }
            for row in &self.rows {
                control.check()?;
                // Aggregate duplicate terms before interval reasoning.
                let mut terms = std::collections::BTreeMap::<usize, Rational>::new();
                for (j, a) in &row.terms {
                    *terms.entry(*j).or_default() += rational(*a)?;
                }
                let terms: Vec<_> = terms.into_iter().filter(|(_, a)| !a.is_zero()).collect();
                for sign in if row.equality {
                    &[1.0, -1.0][..]
                } else {
                    &[1.0][..]
                } {
                    let sign = rational(*sign)?;
                    let rhs = rational(row.rhs)? * &sign;
                    let signed: Vec<_> = terms.iter().map(|(j, a)| (*j, a * &sign)).collect();
                    let mut minimum = Rational::zero();
                    let mut unbounded = 0;
                    let contributions: Vec<_> = signed
                        .iter()
                        .map(|(j, a)| {
                            let value = if a.is_positive() {
                                Some(a * &bounds.lower[*j])
                            } else {
                                bounds.upper[*j].as_ref().map(|x| a * x)
                            };
                            if let Some(x) = &value {
                                minimum += x;
                            } else {
                                unbounded += 1;
                            }
                            value
                        })
                        .collect();
                    for ((j, a), contribution) in signed.iter().zip(contributions) {
                        let other = match contribution {
                            Some(value) if unbounded == 0 => &minimum - value,
                            None if unbounded == 1 => minimum.clone(),
                            _ => continue,
                        };
                        let limit = (&rhs - other) / a;
                        if a.is_positive() {
                            changed |= tighten_upper(&mut bounds.upper[*j], limit);
                        } else if limit > bounds.lower[*j] {
                            bounds.lower[*j] = limit;
                            changed = true;
                        }
                    }
                }
            }
            for (j, x) in incumbent.iter().enumerate() {
                if bounds.lower[j] > *x || bounds.upper[j].as_ref().is_some_and(|u| x > u) {
                    return Err("cost propagation excluded its verified incumbent".into());
                }
            }
            if !changed {
                break;
            }
        }
        Ok(bounds)
    }

    /// Add at most 64 valid incumbent-budget upper rows to a floating search.
    /// Keep a hint only if all extra slacks remain basic, so deleting them gives
    /// a square basis of the ORIGINAL model. Every caller must certify it there.
    pub fn cost_bounded_float_candidate_with_control(
        &self,
        incumbent: &[Rational],
        control: &SolveControl,
    ) -> Result<Option<FloatCandidate>, String> {
        let bounds = self.cost_bounds_with_control(incumbent, control)?;
        let mut selected: Vec<_> = bounds
            .upper
            .iter()
            .enumerate()
            .filter_map(|(j, u)| u.as_ref().map(|u| (j, u)))
            .map(|(j, u)| Ok((j, u, rational(self.columns[j].cost)? * u)))
            .collect::<Result<_, String>>()?;
        // Prefer bounds tightened by material balance beyond the plain U/c cap.
        selected.sort_by(|(j, _, score), (k, _, other)| score.cmp(other).then(j.cmp(k)));
        let mut search = self.clone();
        // The floating basis no longer allocates a dense row-square inverse.
        // Bound the hint batch, not the total model row count.
        for (j, upper, _) in selected.into_iter().take(64) {
            let Some(mut value) = upper.to_f64().filter(|v| v.is_finite()) else {
                continue;
            };
            if rational(value)? < *upper {
                value = value.next_up();
            }
            if value.is_finite() {
                search.rows.push(Row {
                    equality: false,
                    rhs: value,
                    terms: vec![(j, 1.0)],
                });
            }
        }
        if search.rows.len() == self.rows.len() {
            return Ok(None);
        }
        let mut hint = search.float_candidate_with_options(
            FloatOptions {
                ratio_tolerance: 1e-12,
                primal_tolerance: 1e-12,
            },
            10000,
            |_| !control.is_cancelled(),
        )?;
        if hint.basis.rows[self.rows.len()..]
            .iter()
            .any(|s| *s != crate::BasisStatus::Basic)
        {
            return Ok(None);
        }
        hint.basis.rows.truncate(self.rows.len());
        Ok(Some(hint))
    }
}
fn tighten_upper(bound: &mut Option<Rational>, value: Rational) -> bool {
    if bound.as_ref().is_none_or(|old| value < *old) {
        *bound = Some(value);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Column;
    fn model(costs: &[f64], rows: Vec<Row>) -> Model {
        Model {
            columns: costs
                .iter()
                .map(|c| Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost: *c,
                })
                .collect(),
            rows,
            offset: 17.0,
        }
    }
    fn r(x: f64) -> Rational {
        rational(x).unwrap()
    }
    #[test]
    fn budget_excludes_offset_and_zero_cost_is_not_fixed() {
        let m = model(&[2.0, 0.0], vec![]);
        let b = m
            .cost_bounds_with_control(&[r(3.0), r(9.0)], &SolveControl::default())
            .unwrap();
        assert_eq!(b.budget, r(6.0));
        assert_eq!(b.upper, vec![Some(r(3.0)), None]);
    }
    #[test]
    fn bounded_hint_is_not_disabled_by_the_old_dense_row_limit() {
        let mut rows = vec![Row {
            equality: true,
            rhs: 1.0,
            terms: vec![(0, 1.0)],
        }];
        rows.extend((0..4096).map(|_| Row {
            equality: false,
            rhs: 2.0,
            terms: vec![(0, 1.0)],
        }));
        let m = model(&[1.0], rows);
        let hint = m
            .cost_bounded_float_candidate_with_control(&[r(1.0)], &SolveControl::default())
            .unwrap()
            .expect("a valid upper-bound hint must still be attempted");
        assert_eq!(hint.basis.rows.len(), m.rows.len());
        let solution = m.reconstruct(&hint.basis).unwrap();
        assert!(m.certify(&solution).unwrap().optimal);
    }
    #[test]
    fn lower_bounds_spend_budget_and_equality_bounds_free_supplier() {
        let m = model(
            &[2.0, 3.0, 0.0],
            vec![
                Row {
                    equality: false,
                    rhs: -2.0,
                    terms: vec![(0, -1.0)],
                },
                Row {
                    equality: true,
                    rhs: 0.0,
                    terms: vec![(1, 1.0), (2, -1.0)],
                },
            ],
        );
        let b = m
            .cost_bounds_with_control(&[r(2.0), r(2.0), r(2.0)], &SolveControl::default())
            .unwrap();
        assert_eq!(b.lower[0], r(2.0));
        assert_eq!(b.upper[1], Some(r(2.0)));
        assert_eq!(b.upper[2], Some(r(2.0)));
    }
    #[test]
    fn duplicate_cancellation_does_not_invent_a_bound() {
        let m = model(
            &[0.0],
            vec![Row {
                equality: true,
                rhs: 0.0,
                terms: vec![(0, 1.0), (0, -1.0)],
            }],
        );
        assert_eq!(
            m.cost_bounds_with_control(&[r(10.0)], &SolveControl::default())
                .unwrap()
                .upper,
            vec![None]
        );
    }
    #[test]
    fn tiny_positive_cost_keeps_exact_finite_bound() {
        let m = model(&[1e-30], vec![]);
        let b = m
            .cost_bounds_with_control(&[r(1e20)], &SolveControl::default())
            .unwrap();
        assert_eq!(b.upper[0], Some(r(1e20)));
    }
    #[test]
    fn infeasible_incumbent_is_not_a_budget() {
        let m = model(
            &[1.0],
            vec![Row {
                equality: false,
                rhs: -1.0,
                terms: vec![(0, -1.0)],
            }],
        );
        assert!(
            m.cost_bounds_with_control(&[r(0.0)], &SolveControl::default())
                .is_err()
        );
    }
    #[test]
    fn bounded_hint_improves_incumbent_and_is_certified_on_original_rows() {
        let m = model(
            &[1.0, 10.0],
            vec![Row {
                equality: false,
                rhs: -1.0,
                terms: vec![(0, -1.0), (1, -1.0)],
            }],
        );
        let control = SolveControl::default();
        let hint = m
            .cost_bounded_float_candidate_with_control(&[r(0.0), r(2.0)], &control)
            .unwrap()
            .unwrap();
        assert_eq!(hint.basis.rows.len(), m.rows.len());
        let solved = m
            .certify_candidate_with_control(&hint, 100, &Rational::zero(), |_, _| true, &control)
            .unwrap();
        assert!(solved.accepted && solved.certificate.optimal);
        assert_eq!(solved.solution.values, vec![r(1.0), r(0.0)]);
    }
    #[test]
    fn domains_preserve_all_enumerated_nonworse_feasible_points() {
        let m = model(
            &[1.0, 2.0, 0.0],
            vec![
                Row {
                    equality: false,
                    rhs: -2.0,
                    terms: vec![(0, -1.0), (1, -2.0)],
                },
                Row {
                    equality: true,
                    rhs: 0.0,
                    terms: vec![(1, 1.0), (2, -1.0)],
                },
                Row {
                    equality: false,
                    rhs: 3.0,
                    terms: vec![(0, 1.0), (1, -1.0)],
                },
            ],
        );
        let bounds = m
            .cost_bounds_with_control(&[r(2.0), r(2.0), r(2.0)], &SolveControl::default())
            .unwrap();
        let mut checked = 0;
        for a in 0..=12 {
            for b in 0..=12 {
                let values = vec![r(a as f64 / 2.0), r(b as f64 / 2.0), r(b as f64 / 2.0)];
                let witness = ExactSolution {
                    values: values.clone(),
                    row_duals: vec![Rational::zero(); 3],
                    basis_dimension: 0,
                };
                if m.certify(&witness).unwrap().primal_feasible
                    && &values[0] + r(2.0) * &values[1] <= bounds.budget
                {
                    checked += 1;
                    for (j, x) in values.iter().enumerate() {
                        assert!(x >= &bounds.lower[j]);
                        assert!(bounds.upper[j].as_ref().is_none_or(|upper| x <= upper));
                    }
                }
            }
        }
        assert!(checked > 20);
    }
    #[test]
    fn deadline_is_respected_before_propagation() {
        let m = model(&[1.0], vec![]);
        let control = SolveControl::with_deadline(std::time::Instant::now());
        assert!(m.cost_bounds_with_control(&[r(1.0)], &control).is_err());
    }
}
