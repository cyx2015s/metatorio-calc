//! Floating-point basis search. Every outcome is only an unverified hint.
//! Positive diagonal scaling preserves basis identities, not exact arithmetic.
use crate::sparse_float_basis::SparseFloatBasis;
use crate::{Basis, BasisStatus, Model};
use rayon::prelude::*;
use std::sync::OnceLock;
use std::time::Instant;

// Optional diagnostics only: no extra model scans are performed per pivot.
struct FloatProfile {
    started: Option<Instant>,
    pivots: usize,
    zero_steps: usize,
    stop: String,
}
impl FloatProfile {
    fn new() -> Self {
        Self {
            started: std::env::var_os("METATORIO_LP_FLOAT_PROFILE").map(|_| Instant::now()),
            pivots: 0,
            zero_steps: 0,
            stop: "returned error before candidate".into(),
        }
    }
    fn timer(&self) -> Option<Instant> {
        self.started.map(|_| Instant::now())
    }
    fn phase(&self, phase: &str, timer: Option<Instant>) {
        if let Some(timer) = timer {
            eprintln!(
                "[LP-FLOAT-PROFILE] phase={phase} pivots={} ms={:.3}",
                self.pivots,
                timer.elapsed().as_secs_f64() * 1000.0
            );
        }
    }
}
impl Drop for FloatProfile {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            eprintln!(
                "[LP-FLOAT-PROFILE] phase=stop pivots={} zero_steps={} ms={:.3} reason={}",
                self.pivots,
                self.zero_steps,
                started.elapsed().as_secs_f64() * 1000.0,
                self.stop
            );
        }
    }
}

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

/// Search heuristics only; original-model certification is unchanged.
#[derive(Debug, Clone, Copy)]
pub struct FloatOptions {
    pub ratio_tolerance: f64,
    pub primal_tolerance: f64,
}
impl Default for FloatOptions {
    fn default() -> Self {
        Self {
            ratio_tolerance: 1e-9,
            primal_tolerance: 1e-9,
        }
    }
}

fn power_scale(maximum: f64) -> f64 {
    if maximum == 0.0 {
        1.0
    } else {
        2.0f64.powi((-maximum.log2().floor() as i32).clamp(-500, 500))
    }
}

// Partition by disjoint output columns, retaining the original row/term order.
// Moving the row terms here avoids retaining another complete matrix copy.
fn partition_pricing_rows(
    rows: Vec<Vec<(usize, f64)>>,
    width: usize,
) -> Vec<Vec<Vec<(usize, f64)>>> {
    let m = rows.len();
    let mut blocks = vec![vec![Vec::new(); m]; 4];
    for (i, row) in rows.into_iter().enumerate() {
        for (j, a) in row {
            blocks[j / width][i].push((j % width, a));
        }
    }
    blocks
}

fn blocked_float_pricing(
    costs: &[f64],
    blocks: &[Vec<Vec<(usize, f64)>>],
    width: usize,
    duals: &[f64],
    functional: &[f64],
    orientation: f64,
    prices: &mut [f64],
    scales: &mut [f64],
    directions: &mut [f64],
    pool: &rayon::ThreadPool,
) {
    pool.install(|| {
        prices
            .par_chunks_mut(width)
            .zip(scales.par_chunks_mut(width))
            .zip(directions.par_chunks_mut(width))
            .enumerate()
            .for_each(|(block, ((prices, scales), directions))| {
                let offset = block * width;
                prices.copy_from_slice(&costs[offset..offset + prices.len()]);
                for (scale, cost) in scales.iter_mut().zip(&costs[offset..]) {
                    *scale = 1.0 + cost.abs();
                }
                directions.fill(0.0);
                for (i, row) in blocks[block].iter().enumerate() {
                    let y = duals[i];
                    let w = orientation * functional[i];
                    if y == 0.0 && w == 0.0 {
                        continue;
                    }
                    for &(j, a) in row {
                        prices[j] -= y * a;
                        scales[j] += (y * a).abs();
                        directions[j] += w * a;
                    }
                }
            });
    });
}

fn float_pricing_pool(columns: usize) -> Option<&'static rayon::ThreadPool> {
    if columns < 100_000 {
        return None;
    }
    static POOL: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(std::thread::available_parallelism().map_or(1, |n| n.get().min(4)))
            .build()
            .ok()
    })
    .as_ref()
}

impl Model {
    /// Search without exactly forced-zero columns, then restore original basis
    /// identities. The reduced search does not certify the original duals.
    pub fn float_candidate_after_zero_propagation_with_control(
        &self,
        options: FloatOptions,
        max_pivots: usize,
        mut observer: impl FnMut(FloatProgress) -> bool,
        control: &crate::SolveControl,
    ) -> Result<FloatCandidate, String> {
        let zero = self.zero_forced_columns_with_control(control)?;
        if !zero.iter().any(|v| *v) {
            return self.float_candidate_with_options(options, max_pivots, |p| {
                !control.is_cancelled() && observer(p)
            });
        }
        let selected: Vec<_> = (0..self.columns.len()).filter(|j| !zero[*j]).collect();
        let mut positions = vec![None; self.columns.len()];
        for (j, old) in selected.iter().enumerate() {
            positions[*old] = Some(j);
        }
        let rows: Vec<_> = self
            .rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| {
                (r.rhs != 0.0 || r.terms.iter().any(|(j, a)| !zero[*j] && *a != 0.0)).then_some(i)
            })
            .collect();
        let reduced = Model {
            columns: selected.iter().map(|j| self.columns[*j].clone()).collect(),
            rows: rows
                .iter()
                .map(|i| crate::Row {
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
        eprintln!(
            "[LP-FLOAT-ZERO-PRESOLVE] columns={} -> {} rows={} -> {}",
            self.columns.len(),
            selected.len(),
            self.rows.len(),
            rows.len()
        );
        let hint = reduced.float_candidate_with_options(options, max_pivots, |p| {
            !control.is_cancelled() && observer(p)
        })?;
        let mut basis = Basis {
            columns: vec![BasisStatus::Lower; self.columns.len()],
            rows: vec![BasisStatus::Basic; self.rows.len()],
        };
        let mut values = vec![0.0; self.columns.len()];
        for (j, old) in selected.iter().enumerate() {
            basis.columns[*old] = hint.basis.columns[j];
            values[*old] = hint.values[j];
        }
        for (i, old) in rows.iter().enumerate() {
            basis.rows[*old] = hint.basis.rows[i];
        }
        Ok(FloatCandidate {
            basis,
            values,
            ..hint
        })
    }

    /// Propose a basis with a native-free, scaled floating dual simplex.
    /// No stop reason constitutes a feasibility, optimality or Farkas proof.
    pub fn float_candidate_with_observer(
        &self,
        max_pivots: usize,
        observer: impl FnMut(FloatProgress) -> bool,
    ) -> Result<FloatCandidate, String> {
        self.float_candidate_with_options(FloatOptions::default(), max_pivots, observer)
    }
    pub fn float_candidate_with_options(
        &self,
        options: FloatOptions,
        max_pivots: usize,
        observer: impl FnMut(FloatProgress) -> bool,
    ) -> Result<FloatCandidate, String> {
        self.float_candidate_from_basis(None, options, max_pivots, observer)
    }

    /// Continue floating search from a supplied basis. This remains only a
    /// hint: the caller must certify every outcome against the original model.
    pub fn float_candidate_with_basis(
        &self,
        basis: &Basis,
        options: FloatOptions,
        max_pivots: usize,
        observer: impl FnMut(FloatProgress) -> bool,
    ) -> Result<FloatCandidate, String> {
        self.float_candidate_from_basis(Some(basis), options, max_pivots, observer)
    }

    fn float_candidate_from_basis(
        &self,
        basis: Option<&Basis>,
        options: FloatOptions,
        max_pivots: usize,
        mut observer: impl FnMut(FloatProgress) -> bool,
    ) -> Result<FloatCandidate, String> {
        let mut profile = FloatProfile::new();
        let build_timer = profile.timer();
        if [options.ratio_tolerance, options.primal_tolerance]
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.0)
        {
            return Err("floating search tolerances must be finite and positive".into());
        }
        self.check()?;
        let n = self.columns.len();
        let m = self.rows.len();
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
        // Normalize in both directions. Large demands otherwise amplify
        // primal-update roundoff and may drive a numerically plausible but
        // exactly singular sequence of degenerate pivots. Power-of-two scaling
        // also keeps homogeneous binary-scaled demands on the same search path.
        let rhs_scale = power_scale(rhs_max);
        let mut basic_values: Vec<_> = rhs.iter().map(|x| x * rhs_scale).collect();
        if costs.iter().chain(&basic_values).any(|x| !x.is_finite())
            || rows.iter().flatten().any(|(_, a)| !a.is_finite())
        {
            return Err("floating scaling overflow".into());
        }
        let mut ids: Vec<_> = if let Some(basis) = basis {
            if basis.columns.len() != n
                || basis.rows.len() != m
                || basis
                    .columns
                    .iter()
                    .any(|s| !matches!(s, BasisStatus::Basic | BasisStatus::Lower))
                || basis
                    .rows
                    .iter()
                    .any(|s| !matches!(s, BasisStatus::Basic | BasisStatus::Upper))
            {
                return Err("unsupported floating warm basis".into());
            }
            let ids: Vec<_> = basis
                .columns
                .iter()
                .enumerate()
                .filter_map(|(j, s)| (*s == BasisStatus::Basic).then_some(j))
                .chain(
                    basis
                        .rows
                        .iter()
                        .enumerate()
                        .filter_map(|(i, s)| (*s == BasisStatus::Basic).then_some(n + i)),
                )
                .collect();
            if ids.len() != m {
                return Err("nonsquare floating warm basis".into());
            }
            ids
        } else {
            (n..n + m).collect()
        };
        let mut basic = vec![false; n + m];
        for &id in &ids {
            basic[id] = true;
        }
        let mut structural_columns = ids.iter().filter(|id| **id < n).count();
        profile.phase("column-build-and-scaling", build_timer);
        let factor_timer = profile.timer();
        let mut factor = SparseFloatBasis::factor_with_observer(m, n, &ids, &columns, || {
            observer(FloatProgress {
                pivots: 0,
                structural_columns,
            })
        })?;
        profile.phase("initial-factor", factor_timer);
        let mut duals = vec![0.0; m];
        let mut prices = costs.clone();
        let mut price_scales = costs.clone();
        let pricing_pool = float_pricing_pool(n);
        let pricing_width = n.div_ceil(4).max(1);
        let (rows, pricing_blocks) = if pricing_pool.is_some() {
            (
                Vec::new(),
                Some(partition_pricing_rows(rows, pricing_width)),
            )
        } else {
            (rows, None)
        };
        let mut dual_scales = vec![0.0; m];
        let mut directions = vec![0.0; n];
        let mut pivots = 0;
        let mut diagnostic = None;
        let mut last_refactor = None;
        let mut needs_recompute = true;
        let mut last_profile_pivot = None;
        let stop_reason = loop {
            if pivots > 0 && pivots % 128 == 0 && last_refactor != Some(pivots) {
                let factor_timer = profile.timer();
                match SparseFloatBasis::factor_with_observer(m, n, &ids, &columns, || observer(FloatProgress { pivots, structural_columns })) {
                    Ok(rebuilt) => factor = rebuilt,
                    Err(error) => { diagnostic = Some(error); break "floating basis refactorization failed"; },
                }
                profile.phase("periodic-refactor", factor_timer);
                last_refactor = Some(pivots);
                needs_recompute = true;
            }
            // Reconstruct the primal and dual vectors periodically, so their
            // incremental updates do not silently accumulate unchecked drift.
            if needs_recompute || pivots % 32 == 0 {
                needs_recompute = false;
                let basic_timer = profile.timer();
                let scaled_rhs: Vec<_> = rhs.iter().map(|a| a * rhs_scale).collect();
                let basic_costs: Vec<_> = ids.iter().map(|&id| if id < n { costs[id] } else { 0.0 }).collect();
                basic_values = factor.solve(&scaled_rhs)?;
                profile.phase("recompute-basic", basic_timer);
                let dual_timer = profile.timer();
                duals = factor.solve_transpose(&basic_costs)?;
                profile.phase("recompute-dual", dual_timer);
                let pricing_timer = profile.timer();
                let mut pricing_solves = 0usize;
                // Preserve the inverse-based pricing scale without storing
                // the inverse. Absolute triangular bounds can be much wider
                // after cancellation, admitting poor pivots in the ratio test.
                // Only nonbasic inequality slacks use this scale; basic slacks
                // have zero basic cost and therefore zero scale here.
                dual_scales.fill(0.0);
                for i in 0..m {
                    if basic[n + i] || self.rows[i].equality { continue; }
                    if i % 32 == 0 && !observer(FloatProgress { pivots, structural_columns }) {
                        return Err("floating observer requested stop during pricing scale reconstruction".into());
                    }
                    let mut unit = vec![0.0; m];
                    unit[i] = 1.0;
                    let inverse_column = factor.solve(&unit)?;
                    pricing_solves += 1;
                    dual_scales[i] = inverse_column.iter().zip(&basic_costs)
                        .map(|(a, c)| (a * c).abs()).sum();
                }
                if let Some(timer) = pricing_timer {
                    eprintln!("[LP-FLOAT-PROFILE] phase=recompute-pricing-scale pivots={pivots} solves={pricing_solves} ms={:.3}", timer.elapsed().as_secs_f64() * 1000.0);
                }
            }
            if basic_values.iter().chain(&duals).chain(&dual_scales).any(|x| !x.is_finite()) {
                break "nonfinite floating iterate";
            }
            let mut leaving = None;
            let mut violation = options.primal_tolerance;
            let mut worst_basic_violation = 0.0f64;
            for i in 0..m {
                let fixed = ids[i] >= n && self.rows[ids[i] - n].equality;
                let amount = if fixed {
                    basic_values[i].abs()
                } else {
                    (-basic_values[i]).max(0.0)
                };
                if profile.started.is_some() {
                    worst_basic_violation = worst_basic_violation.max(amount);
                }
                if amount > violation {
                    violation = amount;
                    leaving = Some(i);
                }
            }
            if profile.started.is_some() && pivots % 100 == 0 && last_profile_pivot != Some(pivots) {
                last_profile_pivot = Some(pivots);
                let (mut positive, mut negative) = (0usize, 0usize);
                for (id, value) in ids.iter().zip(&basic_values) {
                    if *id < n {
                        positive += usize::from(*value > 0.0);
                        negative += usize::from(*value < 0.0);
                    }
                }
                eprintln!("[LP-FLOAT-PROFILE] phase=progress pivots={pivots} structural={structural_columns} support={} positive={positive} negative={negative} worst_basic_violation={worst_basic_violation:e} zero_steps={}", positive + negative, profile.zero_steps);
            }
            let Some(leaving) = leaving else {
                break "floating primal tolerances met; exact verification required";
            };
            if pivots == max_pivots {
                break "floating pivot limit reached";
            }
            if !observer(FloatProgress {
                pivots,
                structural_columns,
            }) {
                break "floating observer requested stop";
            }
            let orientation = if basic_values[leaving] < 0.0 {
                1.0
            } else {
                -1.0
            };
            let mut unit = vec![0.0; m];
            unit[leaving] = 1.0;
            let functional = factor.solve_transpose(&unit)?;
            if let Some(pool) = pricing_pool {
                blocked_float_pricing(&costs, pricing_blocks.as_ref().unwrap(), pricing_width, &duals, &functional, orientation,
                    &mut prices, &mut price_scales, &mut directions, pool);
            } else {
                prices.copy_from_slice(&costs);
                for (scale, cost) in price_scales.iter_mut().zip(&costs) {
                    *scale = 1.0 + cost.abs();
                }
                directions.fill(0.0);
                for i in 0..m {
                    let y = duals[i];
                    let w = orientation * functional[i];
                    if y == 0.0 && w == 0.0 { continue; }
                    for (j, a) in &rows[i] {
                        prices[*j] -= y * a;
                        price_scales[*j] += (y * a).abs();
                        directions[*j] += w * a;
                    }
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
                relaxed_step=relaxed_step.min((price.max(0.0)+options.ratio_tolerance*scale)/(-direction));
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
                let factor_timer = profile.timer();
                if last_refactor != Some(pivots)
                    && let Ok(rebuilt) = SparseFloatBasis::factor_with_observer(m, n, &ids, &columns, || observer(FloatProgress { pivots, structural_columns })) {
                        factor = rebuilt;
                        last_refactor = Some(pivots);
                        needs_recompute = true;
                        profile.phase("drift-refactor", factor_timer);
                        continue;
                }
                if last_refactor != Some(pivots) {
                    profile.phase("failed-drift-refactor", factor_timer);
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
            let mut entering_column = vec![0.0; m];
            if entering < n {
                for &(r, a) in &columns[entering] { entering_column[r] += a; }
            } else {
                entering_column[entering - n] = 1.0;
            }
            let direction = factor.solve(&entering_column)?;
            let pivot = direction[leaving];
            if !pivot.is_finite() || pivot.abs() < 1e-12 {
                break "unstable floating pivot";
            }
            let step = basic_values[leaving] / pivot;
            if profile.started.is_some() && step == 0.0 {
                profile.zero_steps += 1;
            }
            let entering_price = if entering < n {
                prices[entering]
            } else {
                -duals[entering - n]
            };
            for j in 0..m {
                duals[j] += entering_price / pivot * functional[j];
                dual_scales[j] += (entering_price / pivot * functional[j]).abs();
            }
            for i in 0..m {
                if i == leaving || direction[i] == 0.0 {
                    continue;
                }
                basic_values[i] -= direction[i] * step;
            }
            factor.update(leaving, &direction)?;
            basic_values[leaving] = step;
            basic[ids[leaving]] = false;
            basic[entering] = true;
            structural_columns += usize::from(entering < n);
            structural_columns -= usize::from(ids[leaving] < n);
            ids[leaving] = entering;
            pivots += 1;
            profile.pivots = pivots;
        }
        .to_string();
        profile.stop.clone_from(&stop_reason);
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
    fn parallel_block_scatter_pricing_matches_sequential_scatter_exactly() {
        let rows = [
            vec![(0, 1.0), (1, 1e-24), (2, 0.25)],
            vec![(0, -1.0), (1, 2.0), (1, -2.0)],
            vec![(2, 4.0), (3, 1.0)],
            vec![(0, -0.0), (3, 1e14)],
        ];
        let costs: [f64; 4] = [0.0, 2.0, 0.125, 0.0];
        let width = costs.len().div_ceil(4);
        let blocks = partition_pricing_rows(rows.to_vec(), width);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap();
        for duals in [[1.0, 1.0, -0.5, 0.0], [0.0, -0.0, 0.0, 0.0]] {
            for functional in [[1.0, 1.0, 0.0, -0.0], [0.0, 0.0, 2.0, 1e-14]] {
                for orientation in [1.0, -1.0] {
                    let mut expected_prices = costs.to_vec();
                    let mut expected_scales: Vec<_> = costs.iter().map(|c| 1.0 + c.abs()).collect();
                    let mut expected_directions = vec![0.0; costs.len()];
                    for (i, row) in rows.iter().enumerate() {
                        let y = duals[i];
                        let w = orientation * functional[i];
                        if y == 0.0 && w == 0.0 {
                            continue;
                        }
                        for &(j, a) in row {
                            expected_prices[j] -= y * a;
                            expected_scales[j] += (y * a).abs();
                            expected_directions[j] += w * a;
                        }
                    }
                    let (mut prices, mut scales, mut directions) =
                        (vec![0.0; 4], vec![0.0; 4], vec![0.0; 4]);
                    blocked_float_pricing(
                        &costs,
                        &blocks,
                        width,
                        &duals,
                        &functional,
                        orientation,
                        &mut prices,
                        &mut scales,
                        &mut directions,
                        &pool,
                    );
                    for (actual, expected) in [
                        (&prices, &expected_prices),
                        (&scales, &expected_scales),
                        (&directions, &expected_directions),
                    ] {
                        assert_eq!(
                            actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                            expected.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn warm_basis_repairs_an_added_supplier_constraint() {
        let mut model = Model {
            columns: [0.0, 1.0]
                .into_iter()
                .map(|cost| Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost,
                })
                .collect(),
            rows: vec![Row {
                equality: false,
                rhs: -1.0,
                terms: vec![(0, -1.0)],
            }],
            offset: 0.0,
        };
        let previous = model.float_candidate_with_observer(20, |_| true).unwrap();
        assert!(
            model
                .certify(&model.reconstruct(&previous.basis).unwrap())
                .unwrap()
                .optimal
        );
        model.rows.push(Row {
            equality: false,
            rhs: 0.0,
            terms: vec![(0, 1.0), (1, -1.0)],
        });
        let mut basis = previous.basis;
        basis.rows.push(BasisStatus::Basic);
        let hint = model
            .float_candidate_with_basis(&basis, FloatOptions::default(), 20, |_| true)
            .unwrap();
        let solution = model.reconstruct(&hint.basis).unwrap();
        assert!(model.certify(&solution).unwrap().optimal);
        assert_eq!(solution.values, vec![crate::rational(1.0).unwrap(); 2]);
        let cold = model.float_candidate_with_observer(20, |_| true).unwrap();
        assert!(hint.pivots < cold.pivots);
        let stopped = model
            .float_candidate_with_basis(&basis, FloatOptions::default(), 20, |_| false)
            .unwrap_err();
        assert!(stopped.contains("observer requested stop"));
        basis.rows.clear();
        assert!(
            model
                .float_candidate_with_basis(&basis, FloatOptions::default(), 20, |_| true)
                .is_err()
        );
    }

    #[test]
    fn invalid_search_tolerances_are_rejected() {
        let model = Model {
            columns: vec![],
            rows: vec![],
            offset: 0.0,
        };
        for value in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            for options in [
                FloatOptions {
                    ratio_tolerance: value,
                    ..FloatOptions::default()
                },
                FloatOptions {
                    primal_tolerance: value,
                    ..FloatOptions::default()
                },
            ] {
                assert!(
                    model
                        .float_candidate_with_options(options, 10, |_| true)
                        .is_err()
                );
            }
        }
    }
    #[test]
    fn rhs_scaling_preserves_the_search_for_tiny_and_large_demands() {
        let mut previous: Option<FloatCandidate> = None;
        for exponent in [-120, 0, 120] {
            let demand = 2.0f64.powi(exponent);
            let model = Model {
                columns: [5.0, 2.0, 0.0]
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
                        rhs: demand,
                        terms: vec![(0, 1.0), (2, 1.0)],
                    },
                    Row {
                        equality: false,
                        rhs: 0.0,
                        terms: vec![(1, -1.0), (2, 2.0)],
                    },
                ],
                offset: 0.0,
            };
            let candidate = model.float_candidate_with_observer(20, |_| true).unwrap();
            let solution = model.reconstruct(&candidate.basis).unwrap();
            assert!(model.certify(&solution).unwrap().optimal);
            assert_eq!(solution.values[2], crate::rational(demand).unwrap());
            if let Some(previous) = previous {
                assert_eq!(previous.basis.columns, candidate.basis.columns);
                assert_eq!(previous.basis.rows, candidate.basis.rows);
                assert_eq!(previous.pivots, candidate.pivots);
            }
            previous = Some(candidate);
        }
    }
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
            for tolerance in [1e-12, 1e-14] {
                let proposal = model
                    .float_candidate_with_options(
                        FloatOptions {
                            ratio_tolerance: tolerance,
                            primal_tolerance: tolerance,
                        },
                        200,
                        |_| true,
                    )
                    .unwrap();
                let refined = model
                    .refine_basis_with_observer(proposal.basis, 200, |_, _| true)
                    .unwrap();
                assert!(refined.certificate.optimal, "strict search case {case}");
                assert_eq!(refined.certificate.objective, certificate.objective);
            }
            assert_eq!(
                refined.certificate.objective, certificate.objective,
                "case {case}"
            );
            let mut scaled = model.clone();
            for (r, row) in scaled.rows.iter_mut().enumerate() {
                let scale = 2.0f64.powi(((case * 11 + r * 17) % 81) as i32 - 40);
                row.rhs *= scale;
                for (_, a) in &mut row.terms {
                    *a *= scale;
                }
            }
            let proposal = scaled.float_candidate_with_observer(200, |_| true).unwrap();
            let refined = scaled
                .refine_basis_with_observer(proposal.basis, 200, |_, _| true)
                .unwrap();
            assert!(refined.certificate.optimal, "scaled case {case}");
            assert_eq!(
                refined.certificate.objective, certificate.objective,
                "scaled case {case}"
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
    #[test]
    fn zero_reduced_hint_restores_full_identities_and_requires_full_dual_repair() {
        let model = Model {
            columns: vec![
                Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost: 0.0,
                },
                Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost: 2.0,
                },
            ],
            rows: vec![
                Row {
                    equality: true,
                    rhs: 0.0,
                    terms: vec![(0, 1e-30)],
                },
                Row {
                    equality: true,
                    rhs: 1.0,
                    terms: vec![(0, 1.0), (1, 1.0)],
                },
            ],
            offset: 0.0,
        };
        let control = crate::SolveControl::default();
        assert_eq!(
            model.zero_forced_columns_with_control(&control).unwrap(),
            vec![true, false]
        );
        let hint = model
            .float_candidate_after_zero_propagation_with_control(
                FloatOptions::default(),
                100,
                |_| true,
                &control,
            )
            .unwrap();
        assert_eq!(hint.values, vec![0.0, 1.0]);
        assert_eq!(hint.basis.rows.len(), model.rows.len());
        assert_eq!(hint.basis.columns.len(), model.columns.len());
        let raw = model
            .certify(&model.reconstruct(&hint.basis).unwrap())
            .unwrap();
        assert!(raw.primal_feasible);
        assert!(
            !raw.dual_feasible,
            "reduced dual must not be accepted as a full proof"
        );
        let full = model
            .certify_candidate_with_control(
                &hint,
                100,
                &crate::rational(0.0).unwrap(),
                |_, _| true,
                &control,
            )
            .unwrap();
        assert!(full.accepted && full.certificate.optimal);
        assert_eq!(full.certificate.objective, "2");
    }
}
