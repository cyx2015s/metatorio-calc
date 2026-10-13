//! Sparse zero propagation with an independently checked original-model proof.
use crate::{InfeasibilityCertificate, Model, Rational, SolveControl, rational};
use num_traits::{Signed, Zero};
use std::collections::{HashSet, VecDeque};

impl Model {
    /// Nonnegative variables and homogeneous one-sided rows can force columns
    /// to zero. A contradicted demand is returned only with a full Farkas proof.
    /// None means unknown, not feasible. No dense basis or floating tolerance.
    pub fn prove_zero_propagation_infeasible(
        &self,
        control: &SolveControl,
    ) -> Result<Option<InfeasibilityCertificate>, String> {
        control.check()?;
        self.check()?;
        if self
            .columns
            .iter()
            .any(|c| c.lower != 0.0 || c.upper != f64::INFINITY)
        {
            return Ok(None);
        }
        let mut incidence = vec![Vec::new(); self.columns.len()];
        let mut counts = vec![(0usize, 0usize); self.rows.len()];
        let mut usable = vec![true; self.rows.len()];
        for (i, row) in self.rows.iter().enumerate() {
            control.check()?;
            // Duplicate terms may cancel. Skip such rows rather than reason
            // from their unaggregated signs; the final verifier still sees all.
            let mut seen = HashSet::new();
            if row.terms.iter().any(|(j, _)| !seen.insert(*j)) {
                usable[i] = false;
                continue;
            }
            for &(j, a) in &row.terms {
                if a != 0.0 {
                    incidence[j].push((i, a > 0.0));
                    if a > 0.0 {
                        counts[i].0 += 1;
                    } else {
                        counts[i].1 += 1;
                    }
                }
            }
        }
        let mut forcing = vec![None; self.columns.len()];
        let mut order = Vec::new();
        let mut queue: VecDeque<_> = (0..self.rows.len()).collect();
        while let Some(i) = queue.pop_front() {
            control.check()?;
            if !usable[i] {
                continue;
            }
            let row = &self.rows[i];
            let (positive, negative) = counts[i];
            let orientation = if negative == 0 {
                Some(-1.0)
            } else if row.equality && positive == 0 {
                Some(1.0)
            } else {
                None
            };
            let Some(sign) = orientation else {
                continue;
            };
            // Empty equality rows must also detect a positive demand.
            let sign = if row.equality && positive == 0 && negative == 0 && row.rhs > 0.0 {
                1.0
            } else {
                sign
            };
            if sign * row.rhs > 0.0 {
                return self.zero_propagation_witness(i, sign, &forcing, &order, control);
            }
            if row.rhs != 0.0 {
                continue;
            }
            for &(j, a) in &row.terms {
                if a == 0.0 || forcing[j].is_some() {
                    continue;
                }
                forcing[j] = Some((i, sign));
                order.push(j);
                for &(affected, is_positive) in &incidence[j] {
                    if is_positive {
                        counts[affected].0 -= 1;
                    } else {
                        counts[affected].1 -= 1;
                    }
                    queue.push_back(affected);
                }
            }
        }
        Ok(None)
    }

    fn zero_propagation_witness(
        &self,
        row: usize,
        sign: f64,
        forcing: &[Option<(usize, f64)>],
        order: &[usize],
        control: &SolveControl,
    ) -> Result<Option<InfeasibilityCertificate>, String> {
        let mut duals = vec![Rational::zero(); self.rows.len()];
        let mut coefficients = vec![Rational::zero(); self.columns.len()];
        let add = |i: usize,
                   weight: Rational,
                   duals: &mut [Rational],
                   coefficients: &mut [Rational]|
         -> Result<(), String> {
            duals[i] += &weight;
            for &(j, a) in &self.rows[i].terms {
                coefficients[j] += rational(a)? * &weight;
            }
            Ok(())
        };
        add(row, rational(sign)?, &mut duals, &mut coefficients)?;
        // Each forcing row only depends positively on earlier zero columns.
        // Eliminate those dependencies in reverse discovery order, exactly.
        for &j in order.iter().rev() {
            control.check()?;
            if !coefficients[j].is_positive() {
                continue;
            }
            let (i, sign) = forcing[j].ok_or("missing zero propagation reason")?;
            let a = self.rows[i].terms.iter().find(|(k, _)| *k == j).unwrap().1;
            let weight = -coefficients[j].clone() / rational(a)?;
            debug_assert!(
                (weight.is_positive() && sign > 0.0) || (weight.is_negative() && sign < 0.0)
            );
            add(i, weight, &mut duals, &mut coefficients)?;
        }
        control.check()?;
        self.verify_infeasibility(&duals)?
            .map(Some)
            .ok_or_else(|| "zero propagation proof failed original-model verification".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Column, Row};
    fn model(rows: Vec<Row>) -> Model {
        Model {
            columns: vec![
                Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost: 1.0
                };
                3
            ],
            rows,
            offset: 0.0,
        }
    }
    fn row(equality: bool, rhs: f64, terms: &[(usize, f64)]) -> Row {
        Row {
            equality,
            rhs,
            terms: terms.to_vec(),
        }
    }
    #[test]
    fn chained_missing_supply_has_exact_original_model_proof() {
        let m = model(vec![
            row(false, 0.0, &[(0, 2.0)]),
            row(true, 0.0, &[(0, -3.0), (1, 0.5)]),
            row(false, -1.0, &[(1, -1.0), (2, 1.0)]),
        ]);
        let proof = m
            .prove_zero_propagation_infeasible(&SolveControl::default())
            .unwrap()
            .unwrap();
        let mut duals = vec![Rational::zero(); m.rows.len()];
        for (i, v) in proof.row_duals {
            duals[i] = v.parse().unwrap();
        }
        assert!(m.verify_infeasibility(&duals).unwrap().is_some());
    }
    #[test]
    fn supply_cycle_and_external_supply_are_not_declared_infeasible() {
        for rows in [
            vec![
                row(true, 0.0, &[(0, 1.0), (1, -1.0)]),
                row(false, -1.0, &[(1, -1.0)]),
            ],
            vec![
                row(true, 0.0, &[(0, -1.0), (1, 1.0)]),
                row(false, -1.0, &[(1, -1.0)]),
            ],
        ] {
            assert!(
                model(rows)
                    .prove_zero_propagation_infeasible(&SolveControl::default())
                    .unwrap()
                    .is_none()
            );
        }
    }
    #[test]
    fn positive_equality_demand_negative_forcing_and_duplicates() {
        let m = model(vec![
            row(true, 0.0, &[(0, -2.0)]),
            row(true, 1.0, &[(0, 1.0)]),
        ]);
        assert!(
            m.prove_zero_propagation_infeasible(&SolveControl::default())
                .unwrap()
                .is_some()
        );
        let m = model(vec![
            row(false, 0.0, &[(0, 1.0), (0, -1.0)]),
            row(true, 1.0, &[(0, 1.0)]),
        ]);
        assert!(
            m.prove_zero_propagation_infeasible(&SolveControl::default())
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn dense_limit_is_irrelevant_and_cancellation_is_not_infeasibility() {
        let mut rows = vec![row(false, 0.0, &[]); 4100];
        rows.push(row(true, 1.0, &[]));
        let m = model(rows);
        assert!(
            m.prove_zero_propagation_infeasible(&SolveControl::default())
                .unwrap()
                .is_some()
        );
        assert!(
            m.prove_zero_propagation_infeasible(&SolveControl::with_deadline(
                std::time::Instant::now()
            ))
            .unwrap_err()
            .contains("cancelled")
        );
    }
}
