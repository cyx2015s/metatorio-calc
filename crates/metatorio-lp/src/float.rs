//! Floating-point basis search. Every outcome is only an unverified hint.
//! Positive diagonal scaling preserves basis identities, not exact arithmetic.
use crate::{Basis, BasisStatus, Model};

#[derive(Debug)]
pub struct FloatCandidate {
    pub basis: Basis,
    pub values: Vec<f64>,
    pub pivots: usize,
    pub stop_reason: String,
    pub diagnostic: Option<String>,
}
#[derive(Debug, Clone, Copy)]
pub struct FloatProgress {
    pub pivots: usize,
    pub structural_columns: usize,
}

fn power_scale(maximum: f64) -> f64 {
    if maximum == 0.0 {
        1.0
    } else {
        2.0f64.powi((-maximum.log2().floor() as i32).clamp(-500, 500))
    }
}

// Rebuild through the structural block; basic row slacks eliminate all other
// rows. The resulting full inverse uses the current (noncanonical) basis order.
fn refactor(
    m: usize,
    n: usize,
    ids: &[usize],
    columns: &[Vec<(usize, f64)>],
) -> Result<Vec<f64>, String> {
    let structural: Vec<_> = ids
        .iter()
        .enumerate()
        .filter(|(_, id)| **id < n)
        .map(|(p, id)| (p, *id))
        .collect();
    let mut slack_position = vec![None; m];
    for (p, id) in ids.iter().enumerate() {
        if *id >= n {
            slack_position[*id - n] = Some(p);
        }
    }
    let active: Vec<_> = (0..m).filter(|r| slack_position[*r].is_none()).collect();
    let k = structural.len();
    if active.len() != k {
        return Err("nonsquare floating structural block".into());
    }
    let mut active_position = vec![None; m];
    for (i, r) in active.iter().enumerate() {
        active_position[*r] = Some(i);
    }
    let width = 2 * k;
    let mut work = vec![0.0; k * width];
    let mut row_terms = vec![Vec::new(); m];
    for (j, (_, id)) in structural.iter().enumerate() {
        for (r, a) in &columns[*id] {
            row_terms[*r].push((j, *a));
            if let Some(i) = active_position[*r] {
                work[i * width + j] += a;
            }
        }
    }
    for i in 0..k {
        work[i * width + k + i] = 1.0;
    }
    for p in 0..k {
        let q = (p..k)
            .max_by(|i, j| {
                work[i * width + p]
                    .abs()
                    .total_cmp(&work[j * width + p].abs())
            })
            .unwrap();
        if work[q * width + p].abs() < 1e-14 {
            return Err("unstable floating refactor pivot".into());
        }
        for j in 0..width {
            work.swap(p * width + j, q * width + j);
        }
        let pivot = work[p * width + p];
        for j in p + 1..width {
            work[p * width + j] /= pivot;
        }
        work[p * width + p] = 1.0;
        for i in 0..k {
            if i == p {
                continue;
            }
            let factor = work[i * width + p];
            if factor == 0.0 {
                continue;
            }
            for j in p + 1..width {
                work[i * width + j] -= factor * work[p * width + j];
            }
            work[i * width + p] = 0.0;
        }
    }
    let mut inverse = vec![0.0; m * m];
    for (i, (position, _)) in structural.iter().enumerate() {
        for (j, r) in active.iter().enumerate() {
            inverse[position * m + r] = work[i * width + k + j];
        }
    }
    for r in 0..m {
        if let Some(position) = slack_position[r] {
            inverse[position * m + r] = 1.0;
            for (j, active_row) in active.iter().enumerate() {
                inverse[position * m + active_row] = -row_terms[r]
                    .iter()
                    .map(|(i, a)| a * work[i * width + k + j])
                    .sum::<f64>();
            }
        }
    }
    if inverse.iter().any(|x| !x.is_finite()) {
        return Err("nonfinite floating refactor".into());
    }
    Ok(inverse)
}

impl Model {
    /// Propose a basis with a native-free, scaled floating dual simplex.
    /// No stop reason constitutes a feasibility, optimality or Farkas proof.
    pub fn float_candidate_with_observer(
        &self,
        max_pivots: usize,
        mut observer: impl FnMut(FloatProgress) -> bool,
    ) -> Result<FloatCandidate, String> {
        self.check()?;
        let n = self.columns.len();
        let m = self.rows.len();
        if m > 4096 {
            return Err("floating candidate dense row limit exceeded".into());
        }
        if self
            .columns
            .iter()
            .any(|c| c.lower != 0.0 || c.upper != f64::INFINITY || c.cost < 0.0)
        {
            return Err(
                "floating candidate requires nonnegative columns and costs, without upper bounds"
                    .into(),
            );
        }
        let row_scale: Vec<_> = self
            .rows
            .iter()
            .map(|r| power_scale(r.terms.iter().fold(0.0f64, |v, (_, a)| v.max(a.abs()))))
            .collect();
        let mut column_max = vec![0.0f64; n];
        for (i, row) in self.rows.iter().enumerate() {
            for (j, a) in &row.terms {
                column_max[*j] = column_max[*j].max((a * row_scale[i]).abs());
            }
        }
        let column_scale: Vec<_> = column_max.iter().map(|v| power_scale(*v)).collect();
        let rows: Vec<Vec<(usize, f64)>> = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| {
                r.terms
                    .iter()
                    .map(|(j, a)| (*j, a * row_scale[i] * column_scale[*j]))
                    .collect()
            })
            .collect();
        let mut columns = vec![Vec::new(); n];
        for (i, row) in rows.iter().enumerate() {
            for (j, a) in row {
                columns[*j].push((i, *a));
            }
        }
        let costs: Vec<_> = self
            .columns
            .iter()
            .zip(&column_scale)
            .map(|(c, s)| c.cost * s)
            .collect();
        let rhs: Vec<_> = self
            .rows
            .iter()
            .zip(&row_scale)
            .map(|(r, s)| r.rhs * s)
            .collect();
        let rhs_max = rhs.iter().fold(0.0f64, |a, b| a.max(b.abs()));
        let rhs_scale = if rhs_max > 0.0 {
            (1.0 / rhs_max).clamp(1.0, 1e20)
        } else {
            1.0
        };
        let mut basic_values: Vec<_> = rhs.iter().map(|x| x * rhs_scale).collect();
        if costs.iter().chain(&basic_values).any(|x| !x.is_finite())
            || rows.iter().flatten().any(|(_, a)| !a.is_finite())
        {
            return Err("floating scaling overflow".into());
        }
        let mut ids: Vec<_> = (n..n + m).collect();
        let mut basic = vec![false; n + m];
        basic[n..].fill(true);
        let mut inverse = vec![0.0; m * m];
        for i in 0..m {
            inverse[i * m + i] = 1.0;
        }
        let mut duals = vec![0.0; m];
        let mut prices = costs.clone();
        let mut price_scales = costs.clone();
        let mut dual_scales = vec![0.0; m];
        let mut directions = vec![0.0; n];
        let mut pivots = 0;
        let mut diagnostic = None;
        let mut last_refactor = None;
        let mut needs_recompute = true;
        let stop_reason = loop {
            if pivots > 0 && pivots % 128 == 0 && last_refactor != Some(pivots) {
                match refactor(m, n, &ids, &columns) {
                    Ok(rebuilt) => inverse = rebuilt,
                    Err(_) => break "floating basis refactorization failed",
                }
                last_refactor = Some(pivots);
                needs_recompute = true;
            }
            // Reconstruct the primal and dual vectors periodically, so their
            // incremental updates do not silently accumulate unchecked drift.
            if needs_recompute || pivots % 32 == 0 {
                needs_recompute = false;
                duals.fill(0.0);
                dual_scales.fill(0.0);
                for i in 0..m {
                    basic_values[i] = inverse[i * m..(i + 1) * m]
                        .iter()
                        .zip(&rhs)
                        .map(|(a, b)| a * b * rhs_scale)
                        .sum();
                    let cost = if ids[i] < n { costs[ids[i]] } else { 0.0 };
                    if cost != 0.0 {
                        for j in 0..m {
                            duals[j] += cost * inverse[i * m + j];
                            dual_scales[j] += (cost * inverse[i * m + j]).abs();
                        }
                    }
                }
            }
            if basic_values.iter().chain(&duals).any(|x| !x.is_finite()) {
                break "nonfinite floating iterate";
            }
            let mut leaving = None;
            let mut violation = 1e-9;
            for i in 0..m {
                let fixed = ids[i] >= n && self.rows[ids[i] - n].equality;
                let amount = if fixed {
                    basic_values[i].abs()
                } else {
                    (-basic_values[i]).max(0.0)
                };
                if amount > violation {
                    violation = amount;
                    leaving = Some(i);
                }
            }
            let Some(leaving) = leaving else {
                break "floating primal tolerances met; exact verification required";
            };
            if pivots == max_pivots {
                break "floating pivot limit reached";
            }
            if !observer(FloatProgress {
                pivots,
                structural_columns: ids.iter().filter(|i| **i < n).count(),
            }) {
                break "floating observer requested stop";
            }
            prices.copy_from_slice(&costs);
            for (scale, cost) in price_scales.iter_mut().zip(&costs) {
                *scale = 1.0 + cost.abs();
            }
            directions.fill(0.0);
            let orientation = if basic_values[leaving] < 0.0 {
                1.0
            } else {
                -1.0
            };
            let functional = &inverse[leaving * m..(leaving + 1) * m];
            for i in 0..m {
                let y = duals[i];
                let w = orientation * functional[i];
                if y == 0.0 && w == 0.0 {
                    continue;
                }
                for (j, a) in &rows[i] {
                    prices[*j] -= y * a;
                    price_scales[*j] += (y * a).abs();
                    directions[*j] += w * a;
                }
            }
            let mut eligible = Vec::new();
            let mut relaxed_step = f64::INFINITY;
            let mut dual_drift = false;
            let mut worst_drift=0.0;
            let mut consider = |id, price: f64, direction: f64, scale: f64| {
                if !price.is_finite() || !direction.is_finite() {
                    dual_drift = true;
                    return;
                }
                if price < -1e-8 * scale {
                    dual_drift = true;
                    if -price/scale>worst_drift {
                        worst_drift=-price/scale;
                        diagnostic=Some(format!("variable={id} reduced_cost={price:e} scale={scale:e} direction={direction:e}"));
                    }
                }
                if direction >= -1e-12 {
                    return;
                }
                let ratio = price.max(0.0) / (-direction);
                relaxed_step=relaxed_step.min((price.max(0.0)+1e-9*scale)/(-direction));
                eligible.push((id,ratio,-direction));
            };
            for j in 0..n {
                if !basic[j] {
                    consider(j, prices[j], directions[j], price_scales[j]);
                }
            }
            for i in 0..m {
                if !basic[n + i] && !self.rows[i].equality {
                    consider(
                        n + i,
                        -duals[i],
                        orientation * functional[i],
                        1.0 + dual_scales[i],
                    );
                }
            }
            if dual_drift {
                if last_refactor != Some(pivots)
                    && let Ok(rebuilt) = refactor(m, n, &ids, &columns) {
                        inverse = rebuilt;
                        last_refactor = Some(pivots);
                        needs_recompute = true;
                        continue;
                }
                break "floating dual drift; exact verification required";
            }
            // Two-pass ratio test: within a bounded pricing-error budget,
            // choose the largest pivot rather than a nearly singular exact tie.
            let entering=eligible.into_iter().filter(|(_,r,_)| *r<=relaxed_step)
                .max_by(|a,b| a.2.total_cmp(&b.2).then_with(|| b.1.total_cmp(&a.1)).then_with(|| b.0.cmp(&a.0)));
            let Some((entering, _, _)) = entering else {
                break "no stable floating entering column; not an infeasibility proof";
            };
            let mut direction = vec![0.0; m];
            if entering < n {
                for i in 0..m {
                    direction[i] = columns[entering]
                        .iter()
                        .map(|(r, a)| inverse[i * m + *r] * a)
                        .sum();
                }
            } else {
                for i in 0..m {
                    direction[i] = inverse[i * m + entering - n];
                }
            }
            let pivot = direction[leaving];
            if !pivot.is_finite() || pivot.abs() < 1e-12 {
                break "unstable floating pivot";
            }
            let step = basic_values[leaving] / pivot;
            let entering_price = if entering < n {
                prices[entering]
            } else {
                -duals[entering - n]
            };
            for j in 0..m {
                duals[j] += entering_price / pivot * inverse[leaving * m + j];
                dual_scales[j] += (entering_price / pivot * inverse[leaving * m + j]).abs();
                inverse[leaving * m + j] /= pivot;
            }
            for i in 0..m {
                if i == leaving || direction[i] == 0.0 {
                    continue;
                }
                basic_values[i] -= direction[i] * step;
                for j in 0..m {
                    inverse[i * m + j] -= direction[i] * inverse[leaving * m + j];
                }
            }
            basic_values[leaving] = step;
            basic[ids[leaving]] = false;
            basic[entering] = true;
            ids[leaving] = entering;
            pivots += 1;
        }
        .to_string();
        let mut values = vec![0.0; n];
        for (id, value) in ids.iter().zip(&basic_values) {
            if *id < n {
                values[*id] = value * column_scale[*id] / rhs_scale;
            }
        }
        Ok(FloatCandidate {
            basis: Basis {
                columns: basic[..n]
                    .iter()
                    .map(|b| {
                        if *b {
                            BasisStatus::Basic
                        } else {
                            BasisStatus::Lower
                        }
                    })
                    .collect(),
                rows: basic[n..]
                    .iter()
                    .map(|b| {
                        if *b {
                            BasisStatus::Basic
                        } else {
                            BasisStatus::Upper
                        }
                    })
                    .collect(),
            },
            values,
            pivots,
            stop_reason,
            diagnostic,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Column, Row};
    #[test]
    fn generated_feasible_models_match_independent_exact_cold_start() {
        let mut state = 17u64;
        let mut next = || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((state >> 32) % 9) as i32 - 4
        };
        for case in 0..64 {
            let columns = (0..7)
                .map(|_| Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost: f64::from(next().abs() + 1),
                })
                .collect();
            let witness: Vec<_> = (0..7).map(|_| f64::from(next().abs())).collect();
            let rows = (0..3)
                .map(|r| {
                    let terms: Vec<_> = (0..7)
                        .map(|j| {
                            let a = if j < 3 {
                                f64::from(j == r)
                            } else {
                                f64::from(next())
                            };
                            (j, a)
                        })
                        .filter(|(_, a)| *a != 0.0)
                        .collect();
                    Row {
                        equality: (case + r) % 3 != 0,
                        rhs: terms.iter().map(|(j, a)| a * witness[*j]).sum(),
                        terms,
                    }
                })
                .collect();
            let model = Model {
                columns,
                rows,
                offset: 3.0,
            };
            let proposal = model.float_candidate_with_observer(200, |_| true).unwrap();
            let refined = model
                .refine_basis_with_observer(proposal.basis, 200, |_, _| true)
                .unwrap();
            assert!(refined.certificate.optimal, "generated case {case}");
            let exact = model.solve_dual_with_observer(200, |_| true).unwrap();
            let certificate = model.certify(&exact.solution).unwrap();
            assert!(certificate.optimal, "cold case {case}");
            assert_eq!(
                refined.certificate.objective, certificate.objective,
                "case {case}"
            );
        }
    }
    #[test]
    fn candidate_reconstruction_supports_more_rows_than_the_inverse_update_limit() {
        let n = 512;
        let model = Model {
            columns: vec![
                Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost: 1.0
                };
                n
            ],
            rows: (0..n)
                .map(|i| Row {
                    equality: true,
                    rhs: 1.0,
                    terms: vec![(i, 1.0)],
                })
                .collect(),
            offset: 0.0,
        };
        let basis = Basis {
            columns: vec![BasisStatus::Basic; n],
            rows: vec![BasisStatus::Upper; n],
        };
        let solution = model.reconstruct(&basis).unwrap();
        assert!(model.certify(&solution).unwrap().optimal);
    }
    #[test]
    fn structural_refactor_rebuilds_the_full_mixed_basis_inverse() {
        let columns = vec![
            vec![(0, 2.0), (1, 3.0), (2, 5.0), (3, 7.0)],
            vec![(0, 3.0), (1, 4.0), (2, 7.0), (3, 11.0)],
        ];
        let ids = vec![0, 3, 1, 5];
        let inverse = refactor(4, 2, &ids, &columns).unwrap();
        for r in 0..4 {
            for c in 0..4 {
                let product: f64 = ids
                    .iter()
                    .enumerate()
                    .map(|(j, id)| {
                        let a = if *id < 2 {
                            columns[*id]
                                .iter()
                                .find(|(i, _)| *i == r)
                                .map_or(0.0, |(_, a)| *a)
                        } else if *id - 2 == r {
                            1.0
                        } else {
                            0.0
                        };
                        a * inverse[j * 4 + c]
                    })
                    .sum();
                assert!((product - if r == c { 1.0 } else { 0.0 }).abs() < 1e-12);
            }
        }
        assert!(refactor(2, 2, &[0, 1], &[vec![(0, 1.0)], vec![(0, 2.0)]]).is_err());
    }
    #[test]
    fn scaled_chain_proposes_an_exactly_optimal_basis() {
        let model = Model {
            columns: vec![
                Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost: 1.0
                };
                2
            ],
            rows: vec![
                Row {
                    equality: true,
                    rhs: 1e-15,
                    terms: vec![(0, 1.0)],
                },
                Row {
                    equality: true,
                    rhs: 0.0,
                    terms: vec![(0, 1e15), (1, -1.0)],
                },
            ],
            offset: 0.0,
        };
        let result = model.float_candidate_with_observer(100, |_| true).unwrap();
        assert!(
            model
                .certify(&model.reconstruct(&result.basis).unwrap())
                .unwrap()
                .optimal
        );
    }
    #[test]
    fn floating_failure_is_not_an_infeasibility_claim() {
        let model = Model {
            columns: vec![],
            rows: vec![Row {
                equality: true,
                rhs: 1.0,
                terms: vec![],
            }],
            offset: 0.0,
        };
        let result = model.float_candidate_with_observer(100, |_| true).unwrap();
        assert_eq!(result.pivots, 0);
        assert!(
            !model
                .certify(&model.reconstruct(&result.basis).unwrap())
                .unwrap()
                .primal_feasible
        );
        assert!(result.stop_reason.contains("not an infeasibility proof"));
    }
}
