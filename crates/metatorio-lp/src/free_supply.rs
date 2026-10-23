//! Projection of isolated unlimited free supplies, for floating hints only.
use crate::{Basis, BasisStatus, FloatCandidate, Model, Row, SolveControl};
use std::collections::HashSet;

struct Supply {
    column: usize,
    row: usize,
    coefficient: f64,
    constraint: Row,
}

pub(crate) struct FreeSupplyProjection {
    pub model: Model,
    columns: Vec<usize>,
    rows: Vec<usize>,
    supplies: Vec<Supply>,
    original_columns: usize,
    original_rows: usize,
}

impl Model {
    /// Remove a zero-cost nonnegative unlimited supplier with exactly one
    /// nonredundant inequality. Associated automatically satisfied rows are
    /// removed too, so the lifted basis is block triangular. Other suppliers
    /// of the same row remain ordinary columns. No coefficients are rounded.
    pub(crate) fn free_supply_projection_with_control(
        &self,
        control: &SolveControl,
    ) -> Result<FreeSupplyProjection, String> {
        control.check()?;
        self.check()?;
        let nonnegative = self.columns.iter().all(|c| c.lower >= 0.0);
        let mut incidence = vec![Vec::new(); self.columns.len()];
        let mut automatic = vec![false; self.rows.len()];
        let mut duplicate = vec![false; self.rows.len()];
        for (i, row) in self.rows.iter().enumerate() {
            control.check()?;
            let mut seen = HashSet::with_capacity(row.terms.len());
            duplicate[i] = row.terms.iter().any(|(j, _)| !seen.insert(*j));
            automatic[i] = nonnegative
                && !duplicate[i]
                && !row.equality
                && row.rhs >= 0.0
                && row.terms.iter().all(|(_, a)| *a <= 0.0);
            for &(j, a) in &row.terms {
                if a != 0.0 {
                    incidence[j].push((i, a));
                }
            }
        }
        let mut remove_columns = vec![false; self.columns.len()];
        let mut remove_rows = vec![false; self.rows.len()];
        let mut claimed = vec![false; self.rows.len()];
        let mut supplies = Vec::new();
        for (j, edges) in incidence.iter().enumerate() {
            control.check()?;
            let column = &self.columns[j];
            if !nonnegative
                || column.cost != 0.0
                || column.lower != 0.0
                || column.upper != f64::INFINITY
            {
                continue;
            }
            let mut required = None;
            let mut eligible = true;
            for &(i, a) in edges {
                if duplicate[i] || self.rows[i].equality || a >= 0.0 {
                    eligible = false;
                    break;
                }
                if !automatic[i] && required.replace((i, a)).is_some() {
                    eligible = false;
                    break;
                }
            }
            let Some((row, coefficient)) = required else {
                continue;
            };
            if !eligible || claimed[row] {
                continue;
            }
            claimed[row] = true;
            remove_columns[j] = true;
            remove_rows[row] = true;
            for &(i, _) in edges {
                if automatic[i] {
                    remove_rows[i] = true;
                }
            }
            supplies.push(Supply {
                column: j,
                row,
                coefficient,
                constraint: self.rows[row].clone(),
            });
        }
        let columns: Vec<_> = (0..self.columns.len())
            .filter(|j| !remove_columns[*j])
            .collect();
        let rows: Vec<_> = (0..self.rows.len()).filter(|i| !remove_rows[*i]).collect();
        let mut positions = vec![None; self.columns.len()];
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
        Ok(FreeSupplyProjection {
            model,
            columns,
            rows,
            supplies,
            original_columns: self.columns.len(),
            original_rows: self.rows.len(),
        })
    }
}

impl FreeSupplyProjection {
    pub fn lift_candidate(&self, hint: FloatCandidate) -> Result<FloatCandidate, String> {
        if hint.values.len() != self.columns.len()
            || hint.basis.columns.len() != self.columns.len()
            || hint.basis.rows.len() != self.rows.len()
        {
            return Err("free supply hint dimension mismatch".into());
        }
        let mut values = vec![0.0; self.original_columns];
        let mut basis = Basis {
            columns: vec![BasisStatus::Lower; self.original_columns],
            rows: vec![BasisStatus::Basic; self.original_rows],
        };
        for (j, old) in self.columns.iter().enumerate() {
            values[*old] = hint.values[j];
            basis.columns[*old] = hint.basis.columns[j];
        }
        for (i, old) in self.rows.iter().enumerate() {
            basis.rows[*old] = hint.basis.rows[i];
        }
        for supply in &self.supplies {
            let mut lhs = 0.0;
            for &(j, a) in &supply.constraint.terms {
                if j != supply.column {
                    lhs += a * values[j];
                }
            }
            let required = (lhs - supply.constraint.rhs) / -supply.coefficient;
            if !required.is_finite() {
                return Err("nonfinite lifted free supply".into());
            }
            if required > 0.0 {
                values[supply.column] = required;
                basis.columns[supply.column] = BasisStatus::Basic;
                basis.rows[supply.row] = BasisStatus::Upper;
            }
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
    use crate::FloatOptions;
    fn row(rhs: f64, terms: &[(usize, f64)]) -> Row {
        Row {
            equality: false,
            rhs,
            terms: terms.to_vec(),
        }
    }
    fn fixture() -> Model {
        Model {
            columns: [2.0, 0.0, 0.0]
                .into_iter()
                .map(|cost| crate::Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost,
                })
                .collect(),
            rows: vec![
                row(-3.0, &[(0, -1.0)]),
                row(0.0, &[(0, 7.0), (1, -2.0), (2, -1.0)]),
                row(0.0, &[(1, -1.0)]),
            ],
            offset: 4.0,
        }
    }
    #[test]
    fn lifted_basis_reconstructs_and_certifies_original_cost_with_multiple_sources() {
        let m = fixture();
        let p = m
            .free_supply_projection_with_control(&SolveControl::default())
            .unwrap();
        assert_eq!(p.columns, vec![0, 2]);
        assert_eq!(p.rows, vec![0]);
        let hint = p
            .model
            .float_candidate_with_options(FloatOptions::default(), 100, |_| true)
            .unwrap();
        let lifted = p.lift_candidate(hint).unwrap();
        assert_eq!(lifted.values[1], 10.5);
        let exact = m.reconstruct(&lifted.basis).unwrap();
        let cert = m.certify(&exact).unwrap();
        assert!(cert.optimal && cert.primal_feasible && cert.dual_feasible);
        let original = m.solve(100).unwrap().phase2.unwrap();
        assert_eq!(
            cert.objective,
            m.certify(&original.solution).unwrap().objective
        );
    }
    #[test]
    fn unsafe_suppliers_are_retained() {
        for variant in 0..7 {
            let mut m = fixture();
            // Disable the second otherwise valid supplier.
            m.columns[2].cost = 1.0;
            match variant {
                0 => m.columns[1].cost = 1.0,
                1 => m.columns[1].upper = 10.0,
                2 => m.rows[1].equality = true,
                3 => m.rows.push(row(0.0, &[(1, 1.0)])),
                4 => m.rows.push(row(-1.0, &[(1, -1.0)])),
                5 => m.rows[1].terms.push((1, -1e-30)),
                _ => m.columns[1].lower = 1.0,
            }
            let p = m
                .free_supply_projection_with_control(&SolveControl::default())
                .unwrap();
            assert_eq!(p.columns.len(), m.columns.len(), "variant {variant}");
        }
    }
    #[test]
    fn tiny_unrelated_coefficients_are_retained() {
        let mut m = fixture();
        m.rows.push(row(1.0, &[(0, 1e-30)]));
        let p = m
            .free_supply_projection_with_control(&SolveControl::default())
            .unwrap();
        assert_eq!(p.model.rows[1].terms, vec![(0, 1e-30)]);
    }

    #[test]
    fn negative_unfinished_hint_is_not_an_original_feasibility_certificate() {
        let m = fixture();
        let p = m
            .free_supply_projection_with_control(&SolveControl::default())
            .unwrap();
        // Stop before any pivot: the all-slack basis still violates demand.
        let mut hint = p
            .model
            .float_candidate_with_options(FloatOptions::default(), 0, |_| true)
            .unwrap();
        assert_eq!(hint.pivots, 0);
        hint.values[0] = -3.0;
        let lifted = p.lift_candidate(hint).unwrap();
        assert_eq!(lifted.values[0], -3.0);
        // Supply lifting is only a hint operation. Reconstruct and certify the
        // original basis rather than trusting these unfinished float values.
        let exact = m.reconstruct(&lifted.basis).unwrap();
        let certificate = m.certify(&exact).unwrap();
        assert!(!certificate.primal_feasible);
        assert!(!certificate.optimal);
    }

    #[test]
    fn separate_sources_with_a_shared_automatic_row_lift_a_triangular_basis() {
        let mut m = fixture();
        m.rows[1].terms.pop();
        m.rows[2].terms.push((2, -1.0));
        m.rows.push(row(0.0, &[(0, 5.0), (2, -3.0)]));
        let p = m
            .free_supply_projection_with_control(&SolveControl::default())
            .unwrap();
        assert_eq!(p.supplies.len(), 2);
        assert_eq!(p.columns, vec![0]);
        assert_eq!(p.rows, vec![0]);
        let hint = p
            .model
            .float_candidate_with_options(FloatOptions::default(), 100, |_| true)
            .unwrap();
        let lifted = p.lift_candidate(hint).unwrap();
        assert_eq!(lifted.values, vec![3.0, 10.5, 5.0]);
        assert_eq!(lifted.basis.columns[1], BasisStatus::Basic);
        assert_eq!(lifted.basis.columns[2], BasisStatus::Basic);
        assert_eq!(lifted.basis.rows[1], BasisStatus::Upper);
        assert_eq!(lifted.basis.rows[2], BasisStatus::Basic);
        assert_eq!(lifted.basis.rows[3], BasisStatus::Upper);
        let exact = m.reconstruct(&lifted.basis).unwrap();
        let certificate = m.certify(&exact).unwrap();
        assert!(certificate.primal_feasible && certificate.dual_feasible);
        assert!(certificate.optimal);
    }

    #[test]
    fn separate_sources_and_unused_supply_lift_to_exact_original_basis() {
        let mut m = fixture();
        m.rows[1].terms.pop();
        // The second free source supplies another row; its demand is already
        // met by the paid production column, so its original slack stays basic.
        m.rows.push(row(0.0, &[(0, -1.0), (2, -1.0)]));
        // Give this row a positive demand coefficient on an unused column,
        // making it nonautomatic without changing this optimum.
        m.columns.push(crate::Column {
            lower: 0.0,
            upper: f64::INFINITY,
            cost: 1.0,
        });
        m.rows.last_mut().unwrap().terms.push((3, 1.0));
        let p = m
            .free_supply_projection_with_control(&SolveControl::default())
            .unwrap();
        assert_eq!(p.supplies.len(), 2);
        let hint = p
            .model
            .float_candidate_with_options(FloatOptions::default(), 100, |_| true)
            .unwrap();
        let lifted = p.lift_candidate(hint).unwrap();
        assert_eq!(lifted.basis.columns[2], BasisStatus::Lower);
        assert_eq!(lifted.basis.rows[3], BasisStatus::Basic);
        assert!(
            m.certify(&m.reconstruct(&lifted.basis).unwrap())
                .unwrap()
                .optimal
        );
    }
}
