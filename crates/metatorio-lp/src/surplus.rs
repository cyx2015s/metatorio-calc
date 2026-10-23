//! A small initial admission hint using the current primal's unused resources.
//! Floating scores only rank columns; no point or LP status is accepted here.
use crate::{Basis, BasisStatus, ExactSolution, Model, SolveControl, restricted::RestrictedIndex};
use num_traits::ToPrimitive;

impl Model {
    pub(crate) fn initial_surplus_columns(
        &self,
        solution: &ExactSolution,
        basis: &Basis,
        index: &RestrictedIndex<'_>,
        control: &SolveControl,
    ) -> Result<Vec<usize>, String> {
        control.check()?;
        let mut demands = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.rhs != 0.0);
        let Some((target, demand)) = demands.next() else {
            return Ok(Vec::new());
        };
        if demands.next().is_some() {
            return Ok(Vec::new());
        }
        let rhs = demand.rhs;
        let mut slack = vec![0.0; self.rows.len()];
        let mut variable_cost = 0.0;
        for (j, status) in basis.columns.iter().enumerate() {
            if j % 4096 == 0 {
                control.check()?;
            }
            if *status != BasisStatus::Basic {
                continue;
            }
            let Some(value) = solution.values[j].to_f64().filter(|v| v.is_finite()) else {
                return Ok(Vec::new());
            };
            variable_cost += value * self.columns[j].cost;
            for (row, a) in index.column_entries(j) {
                slack[row] -= a * value;
            }
        }
        if !variable_cost.is_finite() || slack.iter().any(|v| !v.is_finite()) {
            return Ok(Vec::new());
        }
        for value in &mut slack {
            *value = value.max(0.0);
        }
        // A pure zero-cost supplier can increase without violating any of its
        // inequalities. Redundant -x <= 0 rows must not hide free services.
        // Services remain ordinary columns in the exact LP.
        let mut services = vec![None; self.rows.len()];
        for (j, column) in self.columns.iter().enumerate() {
            if j % 4096 == 0 {
                control.check()?;
            }
            if column.cost != 0.0 {
                continue;
            }
            let mut supplies = false;
            let eligible = index.column_entries(j).all(|(row, a)| {
                if a == 0.0 {
                    return true;
                }
                supplies = true;
                row != target && !self.rows[row].equality && a < 0.0
            });
            if eligible && supplies {
                for (row, a) in index.column_entries(j) {
                    if a < 0.0 {
                        services[row].get_or_insert(j);
                    }
                }
            }
        }
        let mut bounds = vec![f64::NAN; self.columns.len()];
        let mut best_score = variable_cost;
        let mut best = Vec::new();
        for (ordinal, &(craft, target_a)) in demand.terms.iter().enumerate() {
            if ordinal % 128 == 0 {
                control.check()?;
            }
            let gain_c = target_a / rhs;
            if gain_c <= 0.0 || !gain_c.is_finite() {
                continue;
            }
            let inputs: Vec<_> = index
                .column_entries(craft)
                .filter(|(row, a)| *row != target && *a > 0.0 && services[*row].is_none())
                .collect();
            let direct = inputs
                .iter()
                .map(|(row, a)| slack[*row] / a)
                .fold(f64::INFINITY, f64::min);
            let direct = if direct.is_infinite() { 1.0 } else { direct };
            if direct > 0.0 {
                let score =
                    (variable_cost + self.columns[craft].cost * direct) / (1.0 + gain_c * direct);
                if score.is_finite() && score < best_score {
                    best_score = score;
                    best = vec![craft];
                }
            }
            let Some(&(anchor, _)) = inputs.iter().min_by(|a, b| {
                (slack[a.0] / a.1)
                    .total_cmp(&(slack[b.0] / b.1))
                    .then_with(|| self.rows[a.0].terms.len().cmp(&self.rows[b.0].terms.len()))
            }) else {
                continue;
            };
            for (ordinal, &(supplier, a)) in self.rows[anchor].terms.iter().enumerate() {
                if ordinal % 128 == 0 {
                    control.check()?;
                }
                if a >= 0.0 || supplier == craft {
                    continue;
                }
                let extent = &mut bounds[supplier];
                if extent.is_nan() {
                    *extent = index
                        .column_entries(supplier)
                        .filter(|(row, a)| *row != target && *a > 0.0 && services[*row].is_none())
                        .map(|(row, a)| slack[row] / a)
                        .fold(f64::INFINITY, f64::min);
                }
                let t = *extent;
                // Unbounded/free suppliers are handled as services above. A
                // finite existing surplus anchors this two-column experiment.
                if t <= 0.0 || !t.is_finite() {
                    continue;
                }
                let mut u = f64::INFINITY;
                for &(row, consumed) in &inputs {
                    let supplied: f64 = index
                        .column_entries(supplier)
                        .filter(|(i, _)| *i == row)
                        .map(|(_, a)| a)
                        .sum();
                    u = u.min((slack[row] - supplied * t).max(0.0) / consumed);
                }
                if u <= 0.0 || !u.is_finite() {
                    continue;
                }
                let gain_r: f64 = index
                    .column_entries(supplier)
                    .filter(|(i, _)| *i == target)
                    .map(|(_, a)| a / rhs)
                    .sum();
                let scale = 1.0 + gain_c * u + gain_r * t;
                if scale <= 0.0 || !scale.is_finite() {
                    continue;
                }
                let score = (variable_cost
                    + self.columns[supplier].cost * t
                    + self.columns[craft].cost * u)
                    / scale;
                if score.is_finite() && score < best_score {
                    best_score = score;
                    best = vec![supplier, craft];
                }
            }
        }
        let mut admissions = Vec::new();
        for &j in &best {
            if basis.columns[j] != BasisStatus::Basic && !admissions.contains(&j) {
                admissions.push(j);
            }
            for (row, a) in index.column_entries(j) {
                if a > 0.0
                    && let Some(service) = services[row]
                    && basis.columns[service] != BasisStatus::Basic
                    && !admissions.contains(&service)
                {
                    admissions.push(service);
                }
            }
        }
        Ok(admissions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Column, Row};

    fn pair_fixture() -> (Model, Basis) {
        let model = Model {
            columns: [10.0, 1.0, 1.0, 0.0]
                .map(|cost| Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost,
                })
                .to_vec(),
            rows: vec![
                Row {
                    equality: true,
                    rhs: 1.0,
                    terms: vec![(0, 1.0), (1, 1.0)],
                },
                Row {
                    equality: false,
                    rhs: 0.0,
                    terms: vec![(0, -1.0), (2, 1.0)],
                },
                Row {
                    equality: false,
                    rhs: 0.0,
                    terms: vec![(1, 1.0), (2, -1.0)],
                },
                Row {
                    equality: false,
                    rhs: 0.0,
                    terms: vec![(1, 1.0), (2, 1.0), (3, -1.0)],
                },
            ],
            offset: 0.0,
        };
        let basis = Basis {
            columns: vec![
                BasisStatus::Basic,
                BasisStatus::Lower,
                BasisStatus::Lower,
                BasisStatus::Lower,
            ],
            rows: vec![
                BasisStatus::Upper,
                BasisStatus::Basic,
                BasisStatus::Basic,
                BasisStatus::Basic,
            ],
        };
        (model, basis)
    }

    #[test]
    fn initial_surplus_pair_prioritizes_positive_price_recycler_and_free_service() {
        let (model, basis) = pair_fixture();
        let control = SolveControl::default();
        let solution = model.reconstruct(&basis).unwrap();
        let prices = model.reduced_costs(&solution);
        assert!(prices.numerators[2] > 0.into());
        let index = RestrictedIndex::new(&model, &control).unwrap();
        let columns = model
            .initial_surplus_columns(&solution, &basis, &index, &control)
            .unwrap();
        assert_eq!(columns, vec![2, 3, 1]);
        let optimized = model
            .optimize_anytime_from_basis_with_control(&basis, 3, &control)
            .unwrap();
        assert_eq!(optimized.certificate.objective, "6");
        assert!(optimized.certificate.primal_feasible && optimized.certificate.optimal);
        assert!(optimized.improving_pivots > 0);
        assert!(model.certify(&optimized.solution).unwrap().primal_feasible);
    }

    #[test]
    fn redundant_nonnegativity_row_does_not_hide_a_free_service() {
        let (mut model, mut basis) = pair_fixture();
        model.rows.push(Row {
            equality: false,
            rhs: 0.0,
            terms: vec![(3, -1.0)],
        });
        basis.rows.push(BasisStatus::Basic);
        let solution = model.reconstruct(&basis).unwrap();
        let control = SolveControl::default();
        let index = RestrictedIndex::new(&model, &control).unwrap();
        assert_eq!(
            model
                .initial_surplus_columns(&solution, &basis, &index, &control)
                .unwrap(),
            vec![2, 3, 1]
        );
        let result = model
            .optimize_anytime_from_basis_with_control(&basis, 3, &control)
            .unwrap();
        assert!(result.certificate.optimal);
        assert_eq!(result.certificate.objective, "6");
    }

    #[test]
    fn direct_surplus_use_does_not_require_a_pair() {
        let (mut model, basis) = pair_fixture();
        // Craft uses B directly; recycle cannot improve the target route.
        model.rows[1].terms.push((1, 1.0));
        model.rows[2].terms = vec![(2, -1.0)];
        let solution = model.reconstruct(&basis).unwrap();
        let control = SolveControl::default();
        let index = RestrictedIndex::new(&model, &control).unwrap();
        assert_eq!(
            model
                .initial_surplus_columns(&solution, &basis, &index, &control)
                .unwrap(),
            vec![1, 3]
        );
        let optimized = model
            .optimize_anytime_from_basis_with_control(&basis, 3, &control)
            .unwrap();
        assert_eq!(optimized.certificate.objective, "11/2");
        assert!(optimized.certificate.optimal);
    }

    #[test]
    fn nonhomogeneous_side_constraint_skips_surplus_scaling_hint() {
        let (mut model, basis) = pair_fixture();
        model.rows[1].rhs = 1.0;
        let solution = model.reconstruct(&basis).unwrap();
        let control = SolveControl::default();
        let index = RestrictedIndex::new(&model, &control).unwrap();
        assert!(
            model
                .initial_surplus_columns(&solution, &basis, &index, &control)
                .unwrap()
                .is_empty()
        );
    }
}
