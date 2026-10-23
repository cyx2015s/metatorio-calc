//! Restricted-master search, with every terminal certificate lifted to self.
use crate::{
    Basis, BasisStatus, CertifiedResult, ExactSolution, FloatOptions, Model, Rational, SolveControl,
};
use num_traits::{Signed, Zero};
use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};

/// Families only influence admission order. Even a hash collision must never
/// remove a column from exact pricing or from the eventual admission fallback.
struct ColumnSelectionPolicy {
    families: Vec<u64>,
    admitted_families: HashSet<u64>,
    batch_limit: usize,
    last_limit: usize,
    last_unique: usize,
}

impl ColumnSelectionPolicy {
    fn new(model: &Model, control: &SolveControl) -> Result<Self, String> {
        control.check()?;
        let mut families = vec![0xcbf29ce484222325u64; model.columns.len()];
        let mut signs = vec![0u8; model.columns.len()];
        let mut touched = Vec::new();
        for (i, row) in model.rows.iter().enumerate() {
            control.check()?;
            for (p, &(j, coefficient)) in row.terms.iter().enumerate() {
                if p % 4096 == 0 {
                    control.check()?;
                }
                if coefficient == 0.0 {
                    continue;
                }
                if signs[j] == 0 {
                    touched.push(j);
                }
                signs[j] |= if coefficient.is_sign_negative() { 1 } else { 2 };
            }
            for (p, j) in touched.drain(..).enumerate() {
                if p % 4096 == 0 {
                    control.check()?;
                }
                // Row order is fixed; coefficient magnitudes, term ordering,
                // duplicate terms, labels and initial supports do not enter.
                families[j] = families[j]
                    .wrapping_mul(0x100000001b3)
                    .wrapping_add(i as u64)
                    .wrapping_mul(0x100000001b3)
                    .wrapping_add(u64::from(signs[j]));
                signs[j] = 0;
            }
        }
        Ok(Self {
            families,
            admitted_families: HashSet::new(),
            batch_limit: 32.min(model.columns.len()),
            last_limit: 0,
            last_unique: 0,
        })
    }

    fn expand(&mut self) {
        self.batch_limit = self.batch_limit.saturating_mul(2).min(self.families.len());
    }

    fn record_batch(&mut self, selected: &[usize]) {
        let mut unique = HashSet::new();
        let mut new_families = 0;
        for &j in selected {
            let family = self.families[j];
            unique.insert(family);
            if self.admitted_families.insert(family) {
                new_families += 1;
            }
        }
        self.last_limit = self.batch_limit;
        self.last_unique = unique.len();
        // Repeated module variants or directions across rounds warrant a
        // wider next batch. Expansion is monotone and has no fixed cap below
        // the original column count; all exact-negative columns remain eligible.
        if new_families <= selected.len() / 2 {
            self.expand();
        }
    }
}

fn truncate_prices(candidates: &mut Vec<(usize, &crate::BigInt)>, limit: usize) {
    let compare = |a: &(usize, &crate::BigInt), b: &(usize, &crate::BigInt)| {
        a.1.cmp(b.1).then_with(|| a.0.cmp(&b.0))
    };
    if candidates.len() > limit {
        candidates.select_nth_unstable_by(limit, compare);
        candidates.truncate(limit);
    }
    candidates.sort_unstable_by(compare);
}

impl Model {
    /// Generate columns from exact missing-supplier rays or dual prices. A
    /// restricted infeasibility is never returned as an original-model proof.
    pub fn solve_column_generation_with_control(
        &self,
        seed: Option<&[f64]>,
        budget: &Rational,
        control: &SolveControl,
    ) -> Result<CertifiedResult, String> {
        self.solve_column_generation_impl(seed, None, budget, control)
    }

    /// Continue from an exactly verified original-model primal-feasible basis.
    /// All basic structural columns, including zero-valued ones, enter the
    /// initial restricted master. Global exact pricing still certifies the
    /// original model; a feasible basis is not an optimality certificate.
    pub fn solve_column_generation_from_basis_with_control(
        &self,
        basis: &Basis,
        budget: &Rational,
        control: &SolveControl,
    ) -> Result<CertifiedResult, String> {
        self.solve_column_generation_impl(None, Some(basis), budget, control)
    }

    fn solve_column_generation_impl(
        &self,
        seed: Option<&[f64]>,
        initial_basis: Option<&Basis>,
        budget: &Rational,
        control: &SolveControl,
    ) -> Result<CertifiedResult, String> {
        let search_began = Instant::now();
        let profile = std::env::var_os("METATORIO_LP_COLUMN_PROFILE").is_some();
        let mut first_feasible_reported = false;
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
        if let Some(basis) = initial_basis {
            if basis.columns.len() != self.columns.len() || basis.rows.len() != self.rows.len() {
                return Err("column generation initial basis dimension mismatch".into());
            }
            if basis
                .columns
                .iter()
                .any(|s| !matches!(s, BasisStatus::Basic | BasisStatus::Lower))
                || basis
                    .rows
                    .iter()
                    .any(|s| !matches!(s, BasisStatus::Basic | BasisStatus::Upper))
            {
                return Err("column generation initial basis has unsupported statuses".into());
            }
            let basic_count = basis
                .columns
                .iter()
                .chain(&basis.rows)
                .filter(|s| **s == BasisStatus::Basic)
                .count();
            if basic_count != self.rows.len() {
                return Err("column generation initial basis is not square".into());
            }
            let initial = self.reconstruct_with_control(basis, control)?;
            control.check()?;
            let certificate = self.certify(&initial)?;
            control.check()?;
            if !certificate.primal_feasible {
                return Err(
                    "column generation initial basis is not original-model primal feasible".into(),
                );
            }
        }
        let mut index = None;
        let mut included = vec![false; self.columns.len()];
        if seed.is_none()
            && initial_basis.is_none()
            && !std::env::var("METATORIO_LP_ROUTE").is_ok_and(|value| value == "0")
        {
            match self.route_columns_with_control(control) {
                Ok(route) => {
                    for j in route {
                        included[j] = true;
                    }
                }
                Err(error) => {
                    control.check()?;
                    eprintln!("[LP-EXACT-COLUMNS] route hint unavailable: {error}; starting empty");
                }
            }
        }
        if let Some(seed) = seed {
            for (j, v) in seed.iter().enumerate() {
                included[j] = v.is_finite() && *v != 0.0;
            }
        }
        if let Some(basis) = initial_basis {
            for (j, status) in basis.columns.iter().enumerate() {
                included[j] = *status == BasisStatus::Basic;
            }
        }
        let mut warm: Option<Basis> = initial_basis.cloned();
        let mut exact_pivots = 0;
        let mut float_pivots = 0;
        let mut feasibility_rule = crate::DualPivotRule::FirstViolationLargePivot;
        let legacy_selection =
            std::env::var("METATORIO_LP_COLUMN_SELECTION").is_ok_and(|value| value == "legacy");
        let mut selection_policy = if legacy_selection {
            None
        } else {
            Some(ColumnSelectionPolicy::new(self, control)?)
        };
        // A round may add columns or resume an interrupted Phase I basis.
        // Neither a fixed round count nor the number of nonbasic columns is
        // an algorithmic dimension limit. The caller's control bounds search;
        // individual factorization paths retain their own resource guards.
        for round in 0usize.. {
            control.check()?;
            let selected: Vec<_> = included
                .iter()
                .enumerate()
                .filter_map(|(j, yes)| yes.then_some(j))
                .collect();
            let (working, rows) = if !selected.is_empty() && selected.len() < self.columns.len() / 4
            {
                if index.is_none() {
                    index = Some(crate::restricted::RestrictedIndex::new(self, control)?);
                }
                index.as_ref().unwrap().build(&selected, control)?
            } else {
                crate::restricted::RestrictedIndex::by_rows(self, &selected, control)?
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
                let missing = if let Some(policy) = &mut selection_policy {
                    self.column_generation_diverse_prices(&ray, true, &included, policy, control)?
                } else {
                    self.column_generation_prices(&ray, true, &included)?
                };
                if std::env::var("METATORIO_LP_ROUTE").is_ok_and(|value| value == "0") {
                    missing
                } else {
                    self.column_generation_close_farkas_routes(missing, &included, control)?
                }
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
                    if let Some(policy) = &mut selection_policy {
                        policy.expand();
                    }
                    continue;
                }
                let (adjusted, _, certificate) = self.scale_to_dual_feasible_certified(&lifted)?;
                if !certificate.primal_feasible {
                    return Err("column generation full primal lift failed".into());
                }
                if profile && !first_feasible_reported {
                    eprintln!(
                        "[LP-EXACT-COLUMNS-PROFILE] first_feasible_s={:.6} round={} support={} cost={}",
                        search_began.elapsed().as_secs_f64(),
                        round + 1,
                        selected.len(),
                        certificate.objective
                    );
                    first_feasible_reported = true;
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
                if let Some(policy) = &mut selection_policy {
                    self.column_generation_diverse_prices(
                        &lifted.row_duals,
                        false,
                        &included,
                        policy,
                        control,
                    )?
                } else {
                    self.column_generation_prices(&lifted.row_duals, false, &included)?
                }
            };
            if missing.is_empty() {
                return Err("column generation has no certified terminal outcome or improving missing column".into());
            }
            for &j in &missing {
                included[j] = true;
            }
            if profile {
                if let Some(policy) = &selection_policy {
                    eprintln!(
                        "[LP-EXACT-COLUMNS-PROFILE] round={} selected={} unique_families={} batch_limit={} next_batch_limit={}",
                        round + 1,
                        missing.len(),
                        policy.last_unique,
                        policy.last_limit,
                        policy.batch_limit
                    );
                } else {
                    eprintln!(
                        "[LP-EXACT-COLUMNS-PROFILE] round={} selected={} selection=legacy",
                        round + 1,
                        missing.len()
                    );
                }
            }
            eprintln!(
                "[LP-EXACT-COLUMNS] round={} columns={} rows={} added={}",
                round + 1,
                selected.len(),
                rows.len(),
                missing.len()
            );
        }
        unreachable!()
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
        let compare = |a: &(usize, &crate::BigInt), b: &(usize, &crate::BigInt)| {
            a.1.cmp(b.1).then_with(|| a.0.cmp(&b.0))
        };
        if missing.len() > 64 {
            missing.select_nth_unstable_by(64, compare);
            missing.truncate(64);
        }
        missing.sort_unstable_by(compare);
        Ok(missing.into_iter().take(64).map(|(j, _)| j).collect())
    }

    /// Complete only Farkas repair proposals. Phase-II pricing stays small;
    /// neither route ranking nor its failure can discard an exact-negative
    /// column selected to escape a restricted infeasibility.
    fn column_generation_close_farkas_routes(
        &self,
        missing: Vec<usize>,
        included: &[bool],
        control: &SolveControl,
    ) -> Result<Vec<usize>, String> {
        control.check()?;
        let mut chosen = vec![false; self.columns.len()];
        let mut expanded = Vec::with_capacity(missing.len());
        for j in missing {
            if !included[j] && !chosen[j] {
                chosen[j] = true;
                expanded.push(j);
            }
        }
        if expanded.is_empty() {
            return Ok(expanded);
        }
        let mut requests = Vec::new();
        for (i, row) in self.rows.iter().enumerate() {
            control.check()?;
            let flipped = row.equality && row.rhs > 0.0;
            let mut requested = [false; 2];
            for (p, &(j, coefficient)) in row.terms.iter().enumerate() {
                if p % 4096 == 0 {
                    control.check()?;
                }
                if !chosen[j] || coefficient == 0.0 {
                    continue;
                }
                let canonical_positive = (coefficient > 0.0) != flipped;
                if canonical_positive || (row.equality && row.rhs == 0.0) {
                    // Opposite ORIGINAL sign provides a consumed input or
                    // disposes of a homogeneous equality byproduct. Preserve
                    // even the smallest nonzero structural coefficients.
                    requested[usize::from(coefficient < 0.0)] = true;
                }
            }
            for (positive, yes) in requested.into_iter().enumerate() {
                if yes {
                    requests.push((i, positive != 0));
                }
            }
        }
        if requests.is_empty() {
            return Ok(expanded);
        }
        match self.route_columns_for_rows_with_control(
            &requests,
            crate::RouteOptions::default(),
            control,
        ) {
            Ok(route) => {
                for (p, j) in route.into_iter().enumerate() {
                    if p % 4096 == 0 {
                        control.check()?;
                    }
                    if !included[j] && !chosen[j] {
                        chosen[j] = true;
                        expanded.push(j);
                    }
                }
            }
            Err(error) => {
                control.check()?;
                eprintln!(
                    "[LP-EXACT-COLUMNS] Farkas route closure unavailable: {error}; keeping exact-priced columns"
                );
            }
        }
        Ok(expanded)
    }

    fn column_generation_diverse_prices(
        &self,
        duals: &[Rational],
        zero_cost: bool,
        included: &[bool],
        policy: &mut ColumnSelectionPolicy,
        control: &SolveControl,
    ) -> Result<Vec<usize>, String> {
        control.check()?;
        let prices = self.pricing(
            &ExactSolution {
                values: vec![],
                row_duals: duals.to_vec(),
                basis_dimension: 0,
            },
            zero_cost,
        );
        control.check()?;
        let mut missing = Vec::new();
        let mut best_by_family: HashMap<u64, (usize, &crate::BigInt)> = HashMap::new();
        for (j, price) in prices.numerators.iter().enumerate() {
            if j % 4096 == 0 {
                control.check()?;
            }
            // Keep exact sign eligibility, including arbitrarily tiny prices.
            if included[j] || !price.is_negative() {
                continue;
            }
            missing.push((j, price));
            let best = best_by_family
                .entry(policy.families[j])
                .or_insert((j, price));
            if price < best.1 || (price == best.1 && j < best.0) {
                *best = (j, price);
            }
        }
        let limit = policy.batch_limit.min(missing.len());
        let mut representatives: Vec<_> = best_by_family.into_values().collect();
        truncate_prices(&mut representatives, limit);
        control.check()?;
        let mut selected: Vec<_> = representatives.into_iter().map(|(j, _)| j).collect();
        if selected.len() < limit {
            let representatives: HashSet<_> = selected.iter().copied().collect();
            missing.retain(|(j, _)| !representatives.contains(j));
            truncate_prices(&mut missing, limit - selected.len());
            selected.extend(missing.into_iter().map(|(j, _)| j));
        }
        control.check()?;
        // No exclusion persists across rounds. Each nonempty exact-negative
        // set admits at least one new column, even if every family hash agrees.
        policy.record_batch(&selected);
        Ok(selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Column, Row, rational};
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
    fn farkas_route_closure_supplies_all_inputs_and_disposes_equality_byproducts() {
        // Missing column 0 needs A and a tiny amount of B, produces the
        // demanded output and a waste. B itself needs C from column 3.
        let model = Model {
            columns: columns(&[1.0, 1.0, 1.0, 1.0, 1.0]),
            rows: vec![
                Row {
                    equality: false,
                    rhs: -1.0,
                    terms: vec![(0, -1.0)],
                },
                Row {
                    equality: true,
                    rhs: 0.0,
                    terms: vec![(0, 1.0), (1, -1.0)],
                },
                Row {
                    equality: true,
                    rhs: 0.0,
                    terms: vec![(0, 1e-200), (2, -1.0)],
                },
                Row {
                    equality: true,
                    rhs: 0.0,
                    terms: vec![(2, 1.0), (3, -1.0)],
                },
                Row {
                    equality: true,
                    rhs: 0.0,
                    terms: vec![(0, -1.0), (4, 1.0)],
                },
            ],
            offset: 0.0,
        };
        let included = [false, true, false, false, false];
        let expanded = model
            .column_generation_close_farkas_routes(
                vec![0, 0, 1],
                &included,
                &SolveControl::default(),
            )
            .unwrap();
        assert_eq!(expanded, vec![0, 2, 3, 4]);
        assert!(expanded.iter().all(|j| !included[*j]));
        assert_eq!(
            expanded.iter().copied().collect::<HashSet<_>>().len(),
            expanded.len()
        );
    }
    #[test]
    fn diverse_pricing_keeps_other_signed_patterns_ahead_of_module_variants() {
        let model = Model {
            columns: columns(&vec![0.0; 102]),
            rows: vec![
                Row {
                    equality: true,
                    rhs: 0.0,
                    terms: (0..100)
                        .map(|j| (j, 10.0 + j as f64 * 1e-6))
                        .chain([(100, -1.0)])
                        .collect(),
                },
                Row {
                    equality: true,
                    rhs: 0.0,
                    terms: vec![(100, 2.0), (101, 1.0)],
                },
            ],
            offset: 0.0,
        };
        let control = SolveControl::default();
        let mut policy = ColumnSelectionPolicy::new(&model, &control).unwrap();
        policy.batch_limit = 3;
        let selected = model
            .column_generation_diverse_prices(
                &[rational(1.0).unwrap(), rational(1.0).unwrap()],
                true,
                &[false; 102],
                &mut policy,
                &control,
            )
            .unwrap();
        assert_eq!(selected, vec![99, 100, 101]);
        assert_eq!(policy.last_unique, 3);
        assert_eq!(policy.batch_limit, 3);
    }

    #[test]
    fn diversity_admission_eventually_covers_all_improving_columns_even_with_hash_collisions() {
        let n = 257;
        let model = Model {
            columns: columns(&vec![0.0; n]),
            rows: vec![Row {
                equality: true,
                rhs: 0.0,
                terms: (0..n).map(|j| (j, (j + 1) as f64)).collect(),
            }],
            offset: 0.0,
        };
        let control = SolveControl::default();
        let mut policy = ColumnSelectionPolicy::new(&model, &control).unwrap();
        // Deliberate collision: a family signature is never an equivalence
        // proof and must never become a persistent exclusion.
        policy.families.fill(0);
        let mut included = vec![false; n];
        let mut batches = Vec::new();
        loop {
            let selected = model
                .column_generation_diverse_prices(
                    &[rational(1.0).unwrap()],
                    true,
                    &included,
                    &mut policy,
                    &control,
                )
                .unwrap();
            if selected.is_empty() {
                break;
            }
            batches.push(selected.len());
            for j in selected {
                assert!(!included[j]);
                included[j] = true;
            }
            assert!(batches.len() <= n);
        }
        assert!(included.iter().all(|yes| *yes));
        assert_eq!(batches, vec![32, 64, 128, 33]);
    }

    #[test]
    fn diversity_families_survive_column_units_and_preserve_tiny_exact_negatives() {
        let make = |scale: f64| Model {
            columns: columns(&[0.0, 0.0, 0.0, 0.0]),
            rows: vec![
                Row {
                    equality: true,
                    rhs: 0.0,
                    terms: vec![(0, 4.0 * scale), (1, 3.0), (2, -1e-200)],
                },
                Row {
                    equality: true,
                    rhs: 0.0,
                    terms: vec![(2, 2e-200), (3, 1e-250)],
                },
            ],
            offset: 0.0,
        };
        let control = SolveControl::default();
        let mut signatures = None;
        for scale in [1e-100, 1.0, 1e100] {
            let model = make(scale);
            let mut policy = ColumnSelectionPolicy::new(&model, &control).unwrap();
            if let Some(expected) = &signatures {
                assert_eq!(&policy.families, expected);
            } else {
                signatures = Some(policy.families.clone());
            }
            policy.batch_limit = 3;
            let selected = model
                .column_generation_diverse_prices(
                    &[rational(1.0).unwrap(), rational(1.0).unwrap()],
                    true,
                    &[false; 4],
                    &mut policy,
                    &control,
                )
                .unwrap();
            assert_eq!(selected.len(), 3);
            assert!(selected.contains(&2));
            assert!(selected.contains(&3));
            assert_eq!(policy.last_unique, 3);
        }
    }

    #[test]
    fn incomplete_and_repeated_directions_expand_batches_deterministically() {
        let model = Model {
            columns: columns(&vec![0.0; 200]),
            rows: vec![],
            offset: 0.0,
        };
        let mut policy = ColumnSelectionPolicy::new(&model, &SolveControl::default()).unwrap();
        assert_eq!(policy.batch_limit, 32);
        policy.expand();
        assert_eq!(policy.batch_limit, 64);
        policy.record_batch(&[0, 1, 2, 3]);
        assert_eq!(policy.last_limit, 64);
        assert_eq!(policy.last_unique, 1);
        assert_eq!(policy.batch_limit, 128);
        policy.record_batch(&[4]);
        assert_eq!(policy.batch_limit, 200);
    }

    #[test]
    fn diversity_cache_honors_cancellation() {
        use std::sync::{Arc, atomic::AtomicBool};
        let control = SolveControl::default().with_cancellation(Arc::new(AtomicBool::new(true)));
        let model = Model {
            columns: columns(&[1.0]),
            rows: vec![Row {
                equality: true,
                rhs: 1.0,
                terms: vec![(0, 1.0)],
            }],
            offset: 0.0,
        };
        assert!(ColumnSelectionPolicy::new(&model, &control).is_err());
    }

    #[test]
    fn diverse_pricing_finds_original_optimum_through_a_material_cycle() {
        let model = Model {
            columns: columns(&[4.0, 1.0, 0.25, 10.0]),
            rows: vec![
                Row {
                    equality: false,
                    rhs: 0.0,
                    terms: vec![(0, -1.0), (1, 1.0), (2, -1.0)],
                },
                Row {
                    equality: false,
                    rhs: -1.0,
                    terms: vec![(1, -2.0), (2, 1.0), (3, -1.0)],
                },
            ],
            offset: 0.0,
        };
        let result = model
            .solve_column_generation_with_control(
                Some(&[0.0, 0.0, 0.0, 1.0]),
                &Rational::zero(),
                &SolveControl::default(),
            )
            .unwrap();
        assert!(result.accepted && result.certificate.optimal);
        assert_eq!(result.certificate.objective, "5/4");
        assert!(model.certify(&result.solution).unwrap().optimal);
    }
    #[test]
    fn feasible_nonoptimal_original_basis_expands_to_full_optimum() {
        let model = Model {
            columns: columns(&[4.0, 1.0]),
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
        let initial = model.certify(&model.reconstruct(&basis).unwrap()).unwrap();
        assert!(initial.primal_feasible && !initial.optimal);
        let result = model
            .solve_column_generation_from_basis_with_control(
                &basis,
                &Rational::zero(),
                &SolveControl::default(),
            )
            .unwrap();
        assert!(result.accepted && result.certificate.optimal);
        assert_eq!(result.certificate.objective, "1");
        assert_eq!(result.support_search_pivots, 0);
        assert_eq!(result.support_columns, 2);
        assert!(model.certify(&result.solution).unwrap().optimal);
    }
    #[test]
    fn original_basis_must_be_supported_square_and_exactly_primal_feasible() {
        let model = Model {
            columns: columns(&[1.0]),
            rows: vec![Row {
                equality: false,
                rhs: -1.0,
                terms: vec![(0, -1.0)],
            }],
            offset: 0.0,
        };
        let infeasible = Basis {
            columns: vec![BasisStatus::Lower],
            rows: vec![BasisStatus::Basic],
        };
        let error = model
            .solve_column_generation_from_basis_with_control(
                &infeasible,
                &Rational::zero(),
                &SolveControl::default(),
            )
            .err()
            .expect("infeasible basis must be rejected");
        assert!(error.contains("not original-model primal feasible"));
        for invalid in [
            Basis {
                columns: vec![],
                rows: vec![BasisStatus::Basic],
            },
            Basis {
                columns: vec![BasisStatus::Zero],
                rows: vec![BasisStatus::Basic],
            },
            Basis {
                columns: vec![BasisStatus::Basic],
                rows: vec![BasisStatus::Basic],
            },
        ] {
            assert!(
                model
                    .solve_column_generation_from_basis_with_control(
                        &invalid,
                        &Rational::zero(),
                        &SolveControl::default()
                    )
                    .is_err()
            );
        }
        let seeded = model
            .solve_column_generation_with_control(
                Some(&[1.0]),
                &Rational::zero(),
                &SolveControl::default(),
            )
            .unwrap();
        assert!(seeded.accepted && seeded.certificate.optimal);
    }
    #[test]
    fn sparse_master_and_dense_fallback_find_the_full_certified_optimum() {
        let n = 128;
        let model = Model {
            columns: columns(&(0..n).map(|j| (j / 8 + 1) as f64).collect::<Vec<_>>()),
            rows: (0..8)
                .map(|i| Row {
                    equality: true,
                    rhs: 1.0,
                    // Preserve duplicate terms rather than aggregate with floats.
                    terms: (i..n)
                        .step_by(8)
                        .flat_map(|j| [(j, 0.5), (j, 0.5)])
                        .collect(),
                })
                .collect(),
            offset: 0.0,
        };
        let seed: Vec<_> = (0..n).map(|j| if j >= 120 { 1.0 } else { 0.0 }).collect();
        let result = model
            .solve_column_generation_with_control(
                Some(&seed),
                &Rational::zero(),
                &SolveControl::with_deadline(Instant::now() + std::time::Duration::from_secs(10)),
            )
            .unwrap();
        assert!(result.accepted && result.certificate.optimal);
        assert_eq!(result.certificate.objective, "8");
        assert!(result.support_columns >= n / 4);
        for (j, value) in result.solution.values.iter().enumerate() {
            assert_eq!(*value, rational(if j < 8 { 1.0 } else { 0.0 }).unwrap());
        }
        assert!(model.certify(&result.solution).unwrap().optimal);
    }
    #[test]
    #[ignore = "small release microbenchmark; run explicitly, not a performance assertion"]
    fn small_exact_pricing_benchmark() {
        use std::{hint::black_box, time::Instant};
        let n = 16_384;
        let model = Model {
            columns: columns(&vec![1.0; n]),
            rows: vec![Row {
                equality: true,
                rhs: 1.0,
                terms: (0..n)
                    .map(|j| (j, 2.0 + ((j * 7919) % n) as f64 / 16384.0))
                    .collect(),
            }],
            offset: 0.0,
        };
        let duals = vec![rational(1.0).unwrap()];
        let included = vec![false; n];
        let mut partial = Vec::new();
        let mut full = Vec::new();
        for run in 0..7 {
            for method in [run % 2, 1 - run % 2] {
                let began = Instant::now();
                for _ in 0..5 {
                    let result = if method == 0 {
                        model
                            .column_generation_prices(&duals, false, &included)
                            .unwrap()
                    } else {
                        let prices = model.pricing(
                            &ExactSolution {
                                values: vec![],
                                row_duals: duals.clone(),
                                basis_dimension: 0,
                            },
                            false,
                        );
                        let mut missing: Vec<_> = prices
                            .numerators
                            .iter()
                            .enumerate()
                            .filter(|(j, p)| !included[*j] && p.is_negative())
                            .collect();
                        missing.sort_unstable_by(|a, b| a.1.cmp(b.1).then_with(|| a.0.cmp(&b.0)));
                        missing.into_iter().take(64).map(|(j, _)| j).collect()
                    };
                    assert_eq!(result.len(), 64);
                    black_box(result);
                }
                if method == 0 {
                    partial.push(began.elapsed());
                } else {
                    full.push(began.elapsed());
                }
            }
        }
        partial.sort();
        full.sort();
        eprintln!(
            "[EXACT-PRICE-BENCH] n=16384 nnz=16384 iterations=5 partial_median_us={} full_sort_median_us={}",
            partial[3].as_micros(),
            full[3].as_micros()
        );
    }
    #[test]
    fn partial_pricing_selection_matches_full_exact_order_with_ties_and_tiny_prices() {
        let n = 300;
        let model = Model {
            columns: columns(&vec![1.0; n]),
            rows: vec![Row {
                equality: true,
                rhs: 1.0,
                terms: (0..n)
                    .flat_map(|j| [(j, 1.0), (j, ((j * 73) % 19) as f64 * 1e-30)])
                    .collect(),
            }],
            offset: 0.0,
        };
        let included: Vec<_> = (0..n).map(|j| j % 7 == 0).collect();
        for zero_cost in [false, true] {
            let duals = vec![rational(1.0).unwrap()];
            let prices = model.pricing(
                &ExactSolution {
                    values: vec![],
                    row_duals: duals.clone(),
                    basis_dimension: 0,
                },
                zero_cost,
            );
            let mut expected: Vec<_> = (0..n)
                .filter(|j| !included[*j] && prices.numerators[*j].is_negative())
                .collect();
            expected.sort_unstable_by(|a, b| {
                prices.numerators[*a]
                    .cmp(&prices.numerators[*b])
                    .then_with(|| a.cmp(b))
            });
            expected.truncate(64);
            assert_eq!(
                model
                    .column_generation_prices(&duals, zero_cost, &included)
                    .unwrap(),
                expected
            );
        }
    }
    #[test]
    fn active_family_expansion_misses_an_optimum_until_global_pricing() {
        // Abstract production families A and B, with pure and mixed variants.
        // Positive costs and nonnegative variables alone do not make the
        // family selected by the coarse model safe to keep exclusively.
        let production_model = |outputs: &[f64]| Model {
            columns: columns(&vec![1.0; outputs.len()]),
            rows: vec![Row {
                equality: true,
                rhs: 1.0,
                terms: outputs.iter().copied().enumerate().collect(),
            }],
            offset: 0.0,
        };
        for (outputs, expected) in [
            (vec![2.0, 1.0], "1/2"),           // A pure, B pure: choose A.
            (vec![2.0, 3.0, 1.0], "1/3"),      // Expand A only: still choose A.
            (vec![2.0, 3.0, 1.0, 4.0], "1/4"), // B mixed wins globally.
        ] {
            let model = production_model(&outputs);
            let result = model.solve_dual_with_observer(100, |_| true).unwrap();
            let certificate = model.certify(&result.solution).unwrap();
            assert!(certificate.optimal);
            assert_eq!(certificate.objective, expected);
        }

        let full = production_model(&[2.0, 3.0, 1.0, 4.0]);
        let result = full
            .solve_column_generation_with_control(
                Some(&[1.0, 1.0, 1.0, 0.0]),
                &Rational::zero(),
                &SolveControl::default(),
            )
            .unwrap();
        assert!(result.accepted && result.certificate.optimal);
        assert_eq!(result.certificate.objective, "1/4");
        assert_eq!(result.solution.values[3].to_string(), "1/4");
        assert!(full.certify(&result.solution).unwrap().optimal);
    }
    #[test]
    fn infeasible_pure_variants_do_not_prove_mixed_variants_infeasible() {
        // Net production U=(1,-2), V=(-2,1), mixed=(1,1).
        // Each resource must have net production >= 1. U and V cannot
        // supply both resources, whereas one mixed column supplies both.
        let full = Model {
            columns: columns(&[1.0, 1.0, 1.0]),
            rows: vec![
                Row {
                    equality: false,
                    rhs: -1.0,
                    terms: vec![(0, -1.0), (1, 2.0), (2, -1.0)],
                },
                Row {
                    equality: false,
                    rhs: -1.0,
                    terms: vec![(0, 2.0), (1, -1.0), (2, -1.0)],
                },
            ],
            offset: 0.0,
        };
        let pure = Model {
            columns: full.columns[..2].to_vec(),
            rows: full
                .rows
                .iter()
                .map(|row| Row {
                    terms: row.terms.iter().copied().filter(|(j, _)| *j < 2).collect(),
                    ..row.clone()
                })
                .collect(),
            offset: 0.0,
        };
        let coarse = pure.solve_dual_with_observer(100, |_| true).unwrap();
        assert!(coarse.infeasibility.is_some());
        // DualRepair.solution holds the current basis solution, not the ray.
        // Adding both inequalities supplies an explicit Farkas multiplier.
        let ray = vec![rational(-1.0).unwrap(); 2];
        assert!(pure.verify_infeasibility(&ray).unwrap().is_some());
        assert!(full.verify_infeasibility(&ray).unwrap().is_none());

        let result = full
            .solve_column_generation_with_control(
                Some(&[1.0, 1.0, 0.0]),
                &Rational::zero(),
                &SolveControl::default(),
            )
            .unwrap();
        assert!(result.accepted && result.certificate.optimal);
        assert!(result.infeasibility.is_none());
        assert_eq!(result.certificate.objective, "1");
        assert_eq!(result.solution.values[2], rational(1.0).unwrap());
        assert!(full.certify(&result.solution).unwrap().optimal);
    }
    #[test]
    fn more_than_4096_selected_columns_can_have_a_one_dimensional_basis() {
        let count = 4097;
        let model = Model {
            columns: columns(&(1..=count).map(|v| v as f64).collect::<Vec<_>>()),
            rows: vec![Row {
                equality: true,
                rhs: 1.0,
                terms: (0..count).map(|j| (j, 1.0)).collect(),
            }],
            offset: 0.0,
        };
        let result = model
            .solve_column_generation_with_control(
                Some(&vec![1.0; count]),
                &Rational::zero(),
                &SolveControl::with_deadline(Instant::now() + std::time::Duration::from_secs(10)),
            )
            .unwrap();
        assert!(result.accepted && result.certificate.optimal);
        assert_eq!(result.support_columns, count);
        assert_eq!(result.solution.values[0], rational(1.0).unwrap());
        assert!(result.solution.values[1..].iter().all(Zero::is_zero));
        assert_eq!(result.certificate.objective, "1");
        assert!(model.certify(&result.solution).unwrap().optimal);

        let cancelled = model.solve_column_generation_with_control(
            Some(&vec![1.0; count]),
            &Rational::zero(),
            &SolveControl::with_deadline(Instant::now()),
        );
        assert!(matches!(cancelled, Err(error) if error.contains("cancelled")));
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
        // A seed selects columns only: neither a negative value nor a missing
        // supplier is a feasible primal point. All must lead to the same full
        // original-model optimum, without trusting the proposed magnitudes.
        for seed in [[1.0, 0.0], [-1e-20, 0.0], [1e-30, 0.0]] {
            let result = model
                .solve_column_generation_with_control(
                    Some(&seed),
                    &Rational::zero(),
                    &SolveControl::default(),
                )
                .unwrap();
            assert!(result.accepted && result.certificate.optimal);
            assert_eq!(result.solution.values, vec![rational(1.0).unwrap(); 2]);
            assert_eq!(result.certificate.objective, "3");
            assert_eq!(result.solution.row_duals.len(), 4202);
        }
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
