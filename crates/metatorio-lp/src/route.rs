//! Structural route proposals for a cold restricted master.
//!
//! These columns are only search hints. Unlike relevance reduction, this is
//! deliberately not an equivalent projection and proves no LP status.
use crate::{Model, SolveControl};
use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap, VecDeque};

/// Search width, independent of material names, saved solutions and model size.
#[derive(Clone, Copy, Debug)]
pub struct RouteOptions {
    /// Maximum distinct signed row patterns retained for each requested row.
    pub directions_per_row: usize,
    /// Maximum coefficient/cost variants retained within a signed row pattern.
    pub variants_per_direction: usize,
    /// Weight of log relative acquisition cost versus source distance.
    pub cost_weight: f64,
}

impl Default for RouteOptions {
    fn default() -> Self {
        Self {
            directions_per_row: 2,
            variants_per_direction: 1,
            cost_weight: 0.25,
        }
    }
}

#[derive(Clone, Copy)]
struct Entry {
    row: usize,
    coefficient: f64,
}

/// Flat CSC avoids an allocation (and rational arithmetic) per original column.
struct Routes {
    starts: Vec<usize>,
    entries: Vec<Entry>,
    scales: Vec<f64>,
    flips: Vec<bool>,
    signatures: Vec<u64>,
    missing: Vec<usize>,
    input_cost: Vec<f64>,
    max_depth: Vec<usize>,
    second_depth: Vec<usize>,
    max_count: Vec<usize>,
    depth: Vec<usize>,
    price: Vec<f64>,
}

#[derive(Clone, Copy)]
struct Candidate {
    column: usize,
    signature: u64,
    missing: usize,
    score: f64,
}

impl Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.missing
            .cmp(&other.missing)
            .then_with(|| self.score.total_cmp(&other.score))
            .then_with(|| self.column.cmp(&other.column))
    }
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for Candidate {}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        Candidate::cmp(self, other)
    }
}

struct Family {
    best: Candidate,
    // Keep the representative outside the heap: singleton families require no
    // heap allocation, even when a wide scan encounters millions of patterns.
    variants: BinaryHeap<Candidate>,
}

impl Family {
    fn insert(&mut self, mut candidate: Candidate, limit: usize) {
        if candidate.cmp(&self.best).is_lt() {
            std::mem::swap(&mut candidate, &mut self.best);
        }
        if self.variants.len() < limit - 1 {
            self.variants.push(candidate);
        } else if self
            .variants
            .peek()
            .is_some_and(|worst| candidate.cmp(worst).is_lt())
        {
            *self.variants.peek_mut().unwrap() = candidate;
        }
    }
}

fn bounded(value: f64) -> f64 {
    value.min(f64::MAX / 16.0)
}

impl Routes {
    fn new(model: &Model, control: &SolveControl) -> Result<Self, String> {
        let n = model.columns.len();
        let m = model.rows.len();
        let mut starts = vec![0usize; n + 1];
        let mut scales = vec![1.0f64; m];
        let mut flips = vec![false; m];
        for (i, row) in model.rows.iter().enumerate() {
            control.check()?;
            // Orient a positive equality demand as a production demand too.
            flips[i] = row.equality && row.rhs > 0.0;
            let scale = row.terms.iter().map(|(_, a)| a.abs()).fold(0.0, f64::max);
            if scale > 0.0 {
                scales[i] = scale;
            }
            for &(j, a) in &row.terms {
                if a != 0.0 {
                    starts[j + 1] += 1;
                }
            }
        }
        for j in 0..n {
            starts[j + 1] += starts[j];
        }
        let mut cursor = starts[..n].to_vec();
        let mut entries = vec![
            Entry {
                row: 0,
                coefficient: 0.0
            };
            starts[n]
        ];
        let mut missing = vec![0; n];
        let mut signatures = vec![0xcbf29ce484222325u64; n];
        for (i, row) in model.rows.iter().enumerate() {
            control.check()?;
            for &(j, original) in &row.terms {
                if original == 0.0 {
                    continue;
                }
                let positive = (original > 0.0) != flips[i];
                // No small coefficient is dropped from structural incidence.
                // Clamping here affects ranking only, never the original LP.
                let magnitude = (original.abs() / scales[i]).max(f64::MIN_POSITIVE);
                entries[cursor[j]] = Entry {
                    row: i,
                    coefficient: if positive { magnitude } else { -magnitude },
                };
                cursor[j] += 1;
                missing[j] += usize::from(positive);
                // Entries arrive in row order; ignore coefficient magnitudes
                // so thousands of module variants cannot crowd out routes.
                signatures[j] ^= (i as u64).wrapping_mul(2) + u64::from(positive);
                signatures[j] = signatures[j].wrapping_mul(0x100000001b3);
            }
        }
        let mut routes = Self {
            starts,
            entries,
            scales,
            flips,
            signatures,
            missing,
            input_cost: model.columns.iter().map(|c| c.cost).collect(),
            max_depth: vec![0; n],
            second_depth: vec![0; n],
            max_count: vec![0; n],
            depth: vec![usize::MAX; m],
            price: vec![0.0; m],
        };
        routes.acquire(model, control)?;
        Ok(routes)
    }

    fn activate(&mut self, j: usize, queue: &mut BinaryHeap<Reverse<(usize, usize)>>) {
        let depth = self.max_depth[j].saturating_add(1);
        for entry in &self.entries[self.starts[j]..self.starts[j + 1]] {
            if entry.coefficient >= 0.0 {
                continue;
            }
            let price = bounded(self.input_cost[j] / -entry.coefficient);
            if depth < self.depth[entry.row] {
                self.depth[entry.row] = depth;
                self.price[entry.row] = price;
                queue.push(Reverse((depth, entry.row)));
            } else if depth == self.depth[entry.row] {
                self.price[entry.row] = self.price[entry.row].min(price);
            }
        }
    }

    /// A column becomes source reachable once every positive input is reached.
    /// Layered hyperedge traversal is linear in incidence, plus a row heap.
    /// Unreached cycles remain available to candidate selection below.
    fn acquire(&mut self, model: &Model, control: &SolveControl) -> Result<(), String> {
        let mut queue = BinaryHeap::new();
        for j in 0..model.columns.len() {
            if j % 4096 == 0 {
                control.check()?;
            }
            if self.missing[j] == 0 {
                self.activate(j, &mut queue);
            }
        }
        while let Some(Reverse((depth, i))) = queue.pop() {
            control.check()?;
            if depth != self.depth[i] {
                continue;
            }
            for &(j, a) in &model.rows[i].terms {
                if a == 0.0 || (a > 0.0) == self.flips[i] {
                    continue;
                }
                let magnitude = (a.abs() / self.scales[i]).max(f64::MIN_POSITIVE);
                self.missing[j] -= 1;
                self.input_cost[j] = bounded(self.input_cost[j] + magnitude * self.price[i]);
                if depth > self.max_depth[j] {
                    self.second_depth[j] = self.max_depth[j];
                    self.max_depth[j] = depth;
                    self.max_count[j] = 1;
                } else if depth == self.max_depth[j] {
                    self.max_count[j] += 1;
                } else {
                    self.second_depth[j] = self.second_depth[j].max(depth);
                }
                if self.missing[j] == 0 {
                    self.activate(j, &mut queue);
                }
            }
        }
        Ok(())
    }

    fn candidate(&self, i: usize, j: usize, a: f64, options: RouteOptions) -> Candidate {
        let magnitude = (a.abs() / self.scales[i]).max(f64::MIN_POSITIVE);
        let positive = (a > 0.0) != self.flips[i];
        let mut missing = self.missing[j];
        let mut cost = self.input_cost[j];
        let mut depth = self.max_depth[j];
        if positive {
            // A requested equality disposal consumes the requested material;
            // do not count that material as an additional acquisition input.
            if self.depth[i] == usize::MAX {
                missing = missing.saturating_sub(1);
            } else {
                cost = (cost - bounded(magnitude * self.price[i])).max(0.0);
                if self.depth[i] == depth && self.max_count[j] == 1 {
                    depth = self.second_depth[j];
                }
            }
        }
        let unit_cost = bounded(cost / magnitude);
        let reference = if self.price[i] > 0.0 {
            self.price[i]
        } else {
            // Zero-cost reachable rows should still prefer zero-cost routes.
            f64::MIN_POSITIVE
        };
        // Bound only the ranking benefit, never an LP coefficient or price.
        // Signed log rewards cheaper routes as well as penalizing expensive
        // ones; a finite floor preserves distance tie-breaking at zero cost.
        let log_limit = -f64::EPSILON.ln();
        let log_relative = if unit_cost == 0.0 {
            if self.price[i] > 0.0 { -log_limit } else { 0.0 }
        } else {
            // Difference of logarithms also handles subnormal quality yields
            // without an overflowing cost/reference division.
            (unit_cost.ln() - reference.ln()).clamp(-log_limit, log_limit)
        };
        Candidate {
            column: j,
            signature: self.signatures[j],
            missing,
            score: depth.saturating_add(1) as f64 + options.cost_weight * log_relative,
        }
    }

    fn select_row(
        &self,
        model: &Model,
        request: (usize, bool),
        options: RouteOptions,
        marks: &mut [usize],
        stamp: usize,
        control: &SolveControl,
    ) -> Result<Vec<Candidate>, String> {
        let (i, positive) = request;
        let wide = options.directions_per_row > 8 || options.variants_per_direction > 8;
        let mut small: Vec<Vec<Candidate>> = Vec::new();
        let mut families: HashMap<u64, Family> = HashMap::new();
        for (k, &(j, a)) in model.rows[i].terms.iter().enumerate() {
            if k % 4096 == 0 {
                control.check()?;
            }
            if a == 0.0 || (a > 0.0) != positive || marks[j] == stamp {
                continue;
            }
            // A duplicate column occurrence is only a scoring hint. Use its
            // first coefficient, without changing/aggregating original terms.
            marks[j] = stamp;
            let candidate = self.candidate(i, j, a, options);
            if wide {
                match families.entry(candidate.signature) {
                    std::collections::hash_map::Entry::Occupied(mut entry) => {
                        entry
                            .get_mut()
                            .insert(candidate, options.variants_per_direction);
                    }
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        entry.insert(Family {
                            best: candidate,
                            variants: BinaryHeap::new(),
                        });
                    }
                }
            } else if let Some(group) = small
                .iter_mut()
                .find(|g| g[0].signature == candidate.signature)
            {
                if candidate.cmp(&group[0]).is_lt() {
                    group[0] = candidate;
                    small.sort_unstable_by(|a, b| a[0].cmp(&b[0]));
                }
            } else if small.len() < options.directions_per_row
                || candidate.cmp(&small.last().unwrap()[0]).is_lt()
            {
                small.push(vec![candidate]);
                small.sort_unstable_by(|a, b| a[0].cmp(&b[0]));
                small.truncate(options.directions_per_row);
            }
        }
        if wide {
            let mut groups = Vec::with_capacity(families.len());
            for (k, family) in families.into_values().enumerate() {
                if k % 4096 == 0 {
                    control.check()?;
                }
                groups.push(family);
            }
            control.check()?;
            if groups.len() > options.directions_per_row {
                groups
                    .select_nth_unstable_by(options.directions_per_row, |a, b| a.best.cmp(&b.best));
                groups.truncate(options.directions_per_row);
            }
            control.check()?;
            let mut selected = Vec::new();
            for group in groups {
                if selected.len() % 4096 == 0 {
                    control.check()?;
                }
                selected.push(group.best);
                for candidate in group.variants {
                    if selected.len() % 4096 == 0 {
                        control.check()?;
                    }
                    selected.push(candidate);
                }
            }
            return Ok(selected);
        }
        if options.variants_per_direction > 1 {
            // Representatives are now final. A second bounded scan retains
            // earlier variants even if their family was evicted and reentered
            // while selecting representatives. Default width (2,1) is one pass.
            for (k, &(j, a)) in model.rows[i].terms.iter().enumerate() {
                if k % 4096 == 0 {
                    control.check()?;
                }
                if a == 0.0 || (a > 0.0) != positive || marks[j] == stamp + 1 {
                    continue;
                }
                marks[j] = stamp + 1;
                let signature = self.signatures[j];
                if let Some(group) = small.iter_mut().find(|g| g[0].signature == signature) {
                    if group.iter().any(|old| old.column == j) {
                        continue;
                    }
                    group.push(self.candidate(i, j, a, options));
                    group.sort_unstable_by(Candidate::cmp);
                    group.truncate(options.variants_per_direction);
                }
            }
        }
        control.check()?;
        Ok(small.into_iter().flatten().collect())
    }
}

impl Model {
    /// Propose a small demand-closed cold start. This may be infeasible and is
    /// never an original-model feasibility or optimality certificate.
    pub fn route_columns_with_control(&self, control: &SolveControl) -> Result<Vec<usize>, String> {
        self.route_columns_with_options_and_control(RouteOptions::default(), control)
    }

    pub fn route_columns_with_options_and_control(
        &self,
        options: RouteOptions,
        control: &SolveControl,
    ) -> Result<Vec<usize>, String> {
        let requests: Vec<_> = self
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
        self.route_columns_for_rows_with_control(&requests, options, control)
    }

    /// Propose routes for specific deficits. Each pair is an ORIGINAL row
    /// index and the requested ORIGINAL coefficient sign (true = positive).
    /// Callers can union this result with existing columns and increase widths
    /// after a restricted-master deficit; all returned indices are original.
    pub fn route_columns_for_rows_with_control(
        &self,
        requests: &[(usize, bool)],
        options: RouteOptions,
        control: &SolveControl,
    ) -> Result<Vec<usize>, String> {
        control.check()?;
        self.check()?;
        if options.directions_per_row == 0
            || options.variants_per_direction == 0
            || !options.cost_weight.is_finite()
            || options.cost_weight < 0.0
        {
            return Err(
                "route search requires positive widths and a finite nonnegative cost weight".into(),
            );
        }
        if self
            .columns
            .iter()
            .any(|c| c.lower != 0.0 || c.upper != f64::INFINITY || c.cost < 0.0)
        {
            return Err("route search requires nonnegative unbounded columns and costs".into());
        }
        if requests.iter().any(|(i, _)| *i >= self.rows.len()) {
            return Err("route request row index out of range".into());
        }
        if options.directions_per_row >= self.columns.len()
            && options.variants_per_direction >= self.columns.len()
        {
            let mut all = Vec::with_capacity(self.columns.len());
            for j in 0..self.columns.len() {
                if j % 4096 == 0 {
                    control.check()?;
                }
                all.push(j);
            }
            control.check()?;
            return Ok(all);
        }
        let routes = Routes::new(self, control)?;
        let mut needed = vec![[false; 2]; self.rows.len()];
        let mut queue = VecDeque::new();
        let request = |i: usize,
                       positive: bool,
                       needed: &mut Vec<[bool; 2]>,
                       queue: &mut VecDeque<(usize, bool)>| {
            if !needed[i][positive as usize] {
                needed[i][positive as usize] = true;
                queue.push_back((i, positive));
            }
        };
        for &(i, positive) in requests {
            request(i, positive, &mut needed, &mut queue);
        }
        let mut keep = vec![false; self.columns.len()];
        let mut marks = vec![0; self.columns.len()];
        let mut stamp = 0usize;
        while let Some((i, positive)) = queue.pop_front() {
            control.check()?;
            stamp += 2;
            for candidate in
                routes.select_row(self, (i, positive), options, &mut marks, stamp, control)?
            {
                let j = candidate.column;
                if keep[j] {
                    continue;
                }
                keep[j] = true;
                for entry in &routes.entries[routes.starts[j]..routes.starts[j + 1]] {
                    let row = entry.row;
                    let canonical_positive = entry.coefficient > 0.0;
                    if canonical_positive || (self.rows[row].equality && self.rows[row].rhs == 0.0)
                    {
                        // Positive input needs production; an equality output
                        // in a homogeneous balance also needs disposal. A
                        // nonzero equality target can absorb its output, so
                        // defer any excess disposal to an actual LP deficit.
                        // Cycles terminate via the requested-sign set.
                        let original_positive = (!canonical_positive) != routes.flips[row];
                        request(row, original_positive, &mut needed, &mut queue);
                    }
                }
            }
        }
        control.check()?;
        Ok(keep
            .into_iter()
            .enumerate()
            .filter_map(|(j, yes)| yes.then_some(j))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Column, Row};

    fn model(costs: &[f64], rows: &[(bool, f64, &[(usize, f64)])]) -> Model {
        Model {
            columns: costs
                .iter()
                .map(|&cost| Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost,
                })
                .collect(),
            rows: rows
                .iter()
                .map(|&(equality, rhs, terms)| Row {
                    equality,
                    rhs,
                    terms: terms.to_vec(),
                })
                .collect(),
            offset: 0.0,
        }
    }

    fn narrow() -> RouteOptions {
        RouteOptions {
            directions_per_row: 1,
            ..RouteOptions::default()
        }
    }

    #[test]
    fn source_near_route_does_not_expand_a_long_alternative() {
        let m = model(
            &[1.0, 0.01, 0.01, 0.01],
            &[
                (false, -1.0, &[(0, -1.0), (1, -1.0)]),
                (false, 0.0, &[(1, 1.0), (2, -1.0)]),
                (false, 0.0, &[(2, 1.0), (3, -1.0)]),
            ],
        );
        assert_eq!(
            m.route_columns_with_options_and_control(narrow(), &SolveControl::default())
                .unwrap(),
            vec![0]
        );
    }

    #[test]
    fn free_supply_with_large_units_does_not_make_a_near_route_distant() {
        let mut m = model(
            &[1.0, 0.0, 0.01, 0.01, 0.01],
            &[
                (false, -1.0, &[(0, -1.0), (2, -1.0)]),
                (false, 0.0, &[(0, 1e200), (1, -1e100)]),
                (false, 0.0, &[(2, 1.0), (3, -1.0)]),
                (false, 0.0, &[(3, 1.0), (4, -1.0)]),
            ],
        );
        for _ in 0..2 {
            assert_eq!(
                m.route_columns_with_options_and_control(narrow(), &SolveControl::default())
                    .unwrap(),
                vec![0, 1]
            );
            for (_, a) in &mut m.rows[1].terms {
                *a *= 1e-200;
            }
        }
    }

    #[test]
    fn signed_patterns_preserve_diversity_instead_of_module_variants() {
        let m = model(
            &[1.0, 2.0, 3.0, 1.0, 1.0],
            &[
                (false, -1.0, &[(0, -1.0), (1, -2.0), (2, -1.0)]),
                (false, 0.0, &[(0, 1.0), (1, 2.0), (3, -1.0)]),
                (false, 0.0, &[(2, 1.0), (4, -1.0)]),
            ],
        );
        assert_eq!(
            m.route_columns_with_control(&SolveControl::default())
                .unwrap(),
            vec![0, 2, 3, 4]
        );
        let wide = RouteOptions {
            variants_per_direction: 2,
            ..RouteOptions::default()
        };
        assert_eq!(
            m.route_columns_with_options_and_control(wide, &SolveControl::default())
                .unwrap(),
            vec![0, 1, 2, 3, 4]
        );
    }

    #[test]
    fn no_source_productive_cycle_is_proposed_without_a_history_seed() {
        let m = model(
            &[1.0, 1.0],
            &[
                (false, -1.0, &[(0, -2.0), (1, 1.0)]),
                (false, 0.0, &[(0, 1.0), (1, -1.0)]),
            ],
        );
        assert_eq!(
            m.route_columns_with_control(&SolveControl::default())
                .unwrap(),
            vec![0, 1]
        );
    }

    #[test]
    fn positive_equality_and_strict_coproduct_require_opposite_directions() {
        let m = model(
            &[1.0, 1.0, 0.0],
            &[
                (true, 1.0, &[(0, 1.0)]),
                (true, 0.0, &[(0, -1.0), (1, 1.0)]),
                (false, 0.0, &[(0, 1.0), (2, -1.0)]),
            ],
        );
        assert_eq!(
            m.route_columns_with_control(&SolveControl::default())
                .unwrap(),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn equality_demand_does_not_preemptively_expand_its_recycling_chain() {
        let m = model(
            &[1.0, 0.0, 0.0, 0.0],
            &[
                (true, 1.0, &[(0, 1.0), (1, -1.0)]),
                (true, 0.0, &[(1, -1.0), (2, 1.0)]),
                (true, 0.0, &[(2, -1.0), (3, 1.0)]),
            ],
        );
        assert_eq!(
            m.route_columns_with_control(&SolveControl::default())
                .unwrap(),
            vec![0]
        );
        // If a restricted solve really requires target disposal, it can still
        // explicitly request that direction and obtain the whole chain.
        assert_eq!(
            m.route_columns_for_rows_with_control(
                &[(0, false)],
                RouteOptions::default(),
                &SolveControl::default(),
            )
            .unwrap(),
            vec![0, 1, 2, 3]
        );
    }

    #[test]
    fn tiny_quality_yield_is_a_real_route_with_normal_inputs() {
        let m = model(
            &[1.0, 0.0],
            &[
                (false, -1.0, &[(0, -1e-300)]),
                (false, 0.0, &[(0, 1.0), (1, -1.0)]),
            ],
        );
        assert_eq!(
            m.route_columns_with_control(&SolveControl::default())
                .unwrap(),
            vec![0, 1]
        );
        assert_eq!(m.rows[0].terms[0].1, -1e-300);
    }

    #[test]
    fn row_units_and_column_units_do_not_change_proposed_structure() {
        let mut m = model(
            &[1.0, 4.0, 1.0],
            &[
                (false, -1.0, &[(0, -1.0), (1, -1.0)]),
                (false, 0.0, &[(0, 2.0), (1, 1.0), (2, -1.0)]),
            ],
        );
        let expected = m
            .route_columns_with_options_and_control(narrow(), &SolveControl::default())
            .unwrap();
        for row in &mut m.rows {
            for (j, a) in &mut row.terms {
                if *j == 0 {
                    *a *= 1e-90;
                }
                *a *= 1e50;
            }
            row.rhs *= 1e50;
        }
        m.columns[0].cost *= 1e-90;
        assert_eq!(
            m.route_columns_with_options_and_control(narrow(), &SolveControl::default())
                .unwrap(),
            expected
        );
    }

    #[test]
    fn explicit_deficits_can_request_disconnected_routes() {
        let m = model(
            &[1.0, 1.0],
            &[(false, -1.0, &[(0, -1.0)]), (false, 0.0, &[(1, -1.0)])],
        );
        assert_eq!(
            m.route_columns_for_rows_with_control(
                &[(1, false)],
                narrow(),
                &SolveControl::default()
            )
            .unwrap(),
            vec![1]
        );
        assert!(
            m.route_columns_for_rows_with_control(
                &[(2, false)],
                narrow(),
                &SolveControl::default()
            )
            .is_err()
        );
    }

    #[test]
    fn cancellation_returns_no_partial_route_certificate() {
        let m = model(&[1.0], &[(false, -1.0, &[(0, -1.0)])]);
        let control = SolveControl::with_deadline(std::time::Instant::now());
        assert!(
            m.route_columns_with_control(&control)
                .unwrap_err()
                .contains("cancelled")
        );
    }

    #[test]
    fn wide_heap_and_small_scan_select_the_same_families_and_variants() {
        // Family A is evicted by B/C before its last, cheap variant arrives.
        // Earlier variants of A must survive once A reenters the final set.
        let m = model(
            &[10.0, 9.0, 0.1, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0],
            &[
                (
                    false,
                    -1.0,
                    &[
                        (0, -1.0),
                        (3, -1.0),
                        (6, -1.0),
                        (1, -1.0),
                        (7, -1.0),
                        (4, -1.0),
                        (2, -1.0),
                        (5, -1.0),
                        (8, -1.0),
                    ],
                ),
                (false, 0.0, &[(0, -1.0), (1, -1.0), (2, -1.0)]),
                (false, 0.0, &[(3, -1.0), (4, -1.0), (5, -1.0)]),
                (false, 0.0, &[(6, -1.0), (7, -1.0), (8, -1.0)]),
            ],
        );
        for variants in [3, 9] {
            let options = RouteOptions {
                directions_per_row: 2,
                variants_per_direction: variants,
                ..RouteOptions::default()
            };
            assert_eq!(
                m.route_columns_with_options_and_control(options, &SolveControl::default())
                    .unwrap(),
                vec![0, 1, 2, 3, 4, 5]
            );
        }
    }

    #[test]
    fn duplicate_terms_use_the_first_candidate_coefficient_in_both_paths() {
        let m = model(
            &[10.0, 1.0, 2.0],
            &[(false, -1.0, &[(0, -1.0), (1, -1.0), (2, -1.0), (0, -10.0)])],
        );
        for variants in [1, 9] {
            let options = RouteOptions {
                directions_per_row: 1,
                variants_per_direction: variants,
                ..RouteOptions::default()
            };
            let selected = m
                .route_columns_with_options_and_control(options, &SolveControl::default())
                .unwrap();
            assert!(selected.contains(&1));
            assert!(!selected.contains(&0));
        }
        assert_eq!(m.rows[0].terms.len(), 4);
    }

    #[test]
    fn saturated_width_returns_every_original_column_including_disconnected_ones() {
        let m = model(&[1.0; 4], &[(false, -1.0, &[(0, -1.0)])]);
        let options = RouteOptions {
            directions_per_row: 4,
            variants_per_direction: 4,
            ..RouteOptions::default()
        };
        assert_eq!(
            m.route_columns_with_options_and_control(options, &SolveControl::default())
                .unwrap(),
            vec![0, 1, 2, 3]
        );
    }

    #[test]
    fn wide_scan_checks_cancellation_before_collecting_families() {
        let m = model(&[1.0; 16], &[(false, -1.0, &[(0, -1.0), (1, -1.0)])]);
        let routes = Routes::new(&m, &SolveControl::default()).unwrap();
        let options = RouteOptions {
            directions_per_row: 9,
            ..RouteOptions::default()
        };
        let mut marks = vec![0; m.columns.len()];
        let control = SolveControl::with_deadline(std::time::Instant::now());
        assert!(
            routes
                .select_row(&m, (0, false), options, &mut marks, 2, &control)
                .is_err()
        );
    }
}
