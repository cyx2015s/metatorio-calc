//! Sparse exact Gaussian LU. Row and column permutations are represented by
//! pivot identities; no dense matrix or dense triangular factor is allocated.
use crate::{Rational, SolveControl};
use num_traits::Zero;
use std::collections::{BTreeMap, BTreeSet};

struct Step {
    row: usize,
    column: usize,
    pivot: Rational,
    upper: BTreeMap<usize, Rational>,
    lower: Vec<(usize, Rational)>,
}

pub(crate) struct SparseExactFactor {
    steps: Vec<Step>,
}

/// Relative rational-arithmetic cost: growing numerators/denominators make
/// normalization superlinear. Saturation affects maintenance timing only.
pub(crate) fn rational_work(value: &Rational) -> u64 {
    let bits = value.numer().bits().saturating_add(value.denom().bits());
    bits.saturating_mul(bits)
}

impl SparseExactFactor {
    /// A relative maintenance cost estimate, never a model acceptance limit.
    pub(crate) fn arithmetic_weight(&self, control: &SolveControl) -> Result<u64, String> {
        let mut weight = 0u64;
        for step in &self.steps {
            for value in std::iter::once(&step.pivot)
                .chain(step.upper.values())
                .chain(step.lower.iter().map(|(_, value)| value))
            {
                control.check()?;
                weight = weight.saturating_add(rational_work(value));
            }
        }
        Ok(weight)
    }
    pub(crate) fn new(
        mut rows: Vec<BTreeMap<usize, Rational>>,
        control: &SolveControl,
    ) -> Result<Self, String> {
        control.check()?;
        let n = rows.len();
        let mut columns = vec![BTreeSet::new(); n];
        for (i, row) in rows.iter_mut().enumerate() {
            control.check()?;
            row.retain(|_, value| !value.is_zero());
            for &j in row.keys() {
                if j >= n {
                    return Err("exact sparse factor column dimension invalid".into());
                }
                columns[j].insert(i);
            }
        }
        // Keep degrees current for active columns only. Extracting the minimum
        // costs O(log n), including for large diagonal or chain bases.
        let mut degrees: BTreeSet<_> = columns
            .iter()
            .enumerate()
            .map(|(j, incidence)| (incidence.len(), j))
            .collect();
        let mut steps = Vec::with_capacity(n);
        for _ in 0..n {
            control.check()?;
            let (_, column) = degrees.pop_first().unwrap();
            let row = columns[column]
                .iter()
                .copied()
                .min_by_key(|&i| (rows[i].len(), i))
                .ok_or_else(|| "exactly singular basis".to_string())?;
            let mut upper = std::mem::take(&mut rows[row]);
            for &j in upper.keys() {
                if j != column {
                    degrees.remove(&(columns[j].len(), j));
                }
                columns[j].remove(&row);
                if j != column {
                    degrees.insert((columns[j].len(), j));
                }
            }
            let pivot = upper.remove(&column).unwrap();
            let affected = std::mem::take(&mut columns[column]);
            let mut lower = Vec::with_capacity(affected.len());
            for i in affected {
                control.check()?;
                let factor = rows[i].remove(&column).unwrap() / &pivot;
                for (&j, coefficient) in &upper {
                    control.check()?;
                    let old = rows[i].remove(&j);
                    let value = old.as_ref().cloned().unwrap_or_else(Rational::zero)
                        - &factor * coefficient;
                    if value.is_zero() {
                        if old.is_some() {
                            degrees.remove(&(columns[j].len(), j));
                            columns[j].remove(&i);
                            degrees.insert((columns[j].len(), j));
                        }
                    } else {
                        if old.is_none() {
                            degrees.remove(&(columns[j].len(), j));
                            columns[j].insert(i);
                            degrees.insert((columns[j].len(), j));
                        }
                        rows[i].insert(j, value);
                    }
                }
                lower.push((i, factor));
            }
            steps.push(Step {
                row,
                column,
                pivot,
                upper,
                lower,
            });
        }
        Ok(Self { steps })
    }

    pub(crate) fn solve_with_control(
        &self,
        rhs: &[Rational],
        control: &SolveControl,
    ) -> Result<Vec<Rational>, String> {
        control.check()?;
        if rhs.len() != self.steps.len() {
            return Err("exact sparse factor RHS dimension invalid".into());
        }
        let mut values = rhs.to_vec();
        for step in &self.steps {
            control.check()?;
            let pivot_rhs = values[step.row].clone();
            if pivot_rhs.is_zero() {
                continue;
            }
            for (row, factor) in &step.lower {
                control.check()?;
                values[*row] -= factor * &pivot_rhs;
            }
        }
        let mut result = vec![Rational::zero(); rhs.len()];
        for step in self.steps.iter().rev() {
            control.check()?;
            let mut value = values[step.row].clone();
            for (column, coefficient) in &step.upper {
                control.check()?;
                if !result[*column].is_zero() {
                    value -= coefficient * &result[*column];
                }
            }
            if !value.is_zero() {
                result[step.column] = value / &step.pivot;
            }
        }
        Ok(result)
    }

    pub(crate) fn solve_transpose_with_control(
        &self,
        rhs: &[Rational],
        control: &SolveControl,
    ) -> Result<Vec<Rational>, String> {
        control.check()?;
        if rhs.len() != self.steps.len() {
            return Err("exact sparse transpose RHS dimension invalid".into());
        }
        let mut residual = rhs.to_vec();
        let mut result = vec![Rational::zero(); rhs.len()];
        // U^T z = Q^T b, then apply E^T in reverse elimination order.
        for step in &self.steps {
            control.check()?;
            if residual[step.column].is_zero() {
                continue;
            }
            let value = &residual[step.column] / &step.pivot;
            for (column, coefficient) in &step.upper {
                control.check()?;
                residual[*column] -= coefficient * &value;
            }
            result[step.row] = value;
        }
        for step in self.steps.iter().rev() {
            control.check()?;
            for (row, factor) in &step.lower {
                control.check()?;
                if !result[*row].is_zero() {
                    let contribution = factor * &result[*row];
                    result[step.row] -= contribution;
                }
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn q(value: i64) -> Rational {
        Rational::from_integer(value.into())
    }
    fn sparse(matrix: &[Vec<Rational>]) -> Vec<BTreeMap<usize, Rational>> {
        matrix
            .iter()
            .map(|row| {
                row.iter()
                    .cloned()
                    .enumerate()
                    .filter(|(_, value)| !value.is_zero())
                    .collect()
            })
            .collect()
    }
    #[test]
    fn direct_and_transpose_match_independent_dense_elimination() {
        let control = SolveControl::default();
        // Fractional, nonsymmetric systems and independent row/column shuffles.
        for seed in 0..12usize {
            let n = 9;
            let mut a = vec![vec![Rational::zero(); n]; n];
            for i in 0..n {
                a[i][i] = q(30);
                for j in 0..n {
                    if (i * 11 + j * 7 + seed) % 4 == 0 {
                        a[i][j] += Rational::new(((i + j + seed) as i64 % 7 - 3).into(), 3.into());
                    }
                }
            }
            a.rotate_left(seed % n);
            for row in &mut a {
                row.rotate_right((seed * 2 + 1) % n);
            }
            let rhs: Vec<_> = (0..n).map(|i| q(i as i64 - 4)).collect();
            let factor = SparseExactFactor::new(sparse(&a), &control).unwrap();
            assert_eq!(
                factor.solve_with_control(&rhs, &control).unwrap(),
                crate::solve(a.clone(), rhs.clone()).unwrap()
            );
            let transpose = (0..n)
                .map(|i| (0..n).map(|j| a[j][i].clone()).collect())
                .collect();
            assert_eq!(
                factor.solve_transpose_with_control(&rhs, &control).unwrap(),
                crate::solve(transpose, rhs).unwrap()
            );
        }
    }
    #[test]
    fn singular_and_cancelled_errors_are_distinct() {
        let control = SolveControl::default();
        let a = vec![vec![q(1), q(1)], vec![q(1), q(2)]];
        assert!(
            SparseExactFactor::new(sparse(&[vec![q(0)]]), &control)
                .err()
                .unwrap()
                .contains("singular")
        );
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let cancelled = control.clone().with_cancellation(flag);
        assert!(
            SparseExactFactor::new(sparse(&a), &cancelled)
                .err()
                .unwrap()
                .contains("cancelled")
        );
        let factor = SparseExactFactor::new(sparse(&a), &control).unwrap();
        assert!(
            factor
                .solve_with_control(&[q(1), q(2)], &cancelled)
                .unwrap_err()
                .contains("cancelled")
        );
        assert!(
            factor
                .solve_transpose_with_control(&[q(1), q(2)], &cancelled)
                .unwrap_err()
                .contains("cancelled")
        );
    }
    #[test]
    fn sparse_chain_8192_uses_linear_factor_storage() {
        let n = 8192;
        let rows = (0..n)
            .map(|i| {
                let mut row = BTreeMap::from([(i, q(2))]);
                if i + 1 < n {
                    row.insert(i + 1, q(-1));
                }
                row
            })
            .collect();
        let control = SolveControl::default();
        let factor = SparseExactFactor::new(rows, &control).unwrap();
        let stored: usize = factor
            .steps
            .iter()
            .map(|step| 1 + step.upper.len() + step.lower.len())
            .sum();
        assert!(stored <= 2 * n);
        let rhs: Vec<_> = (0..n).map(|i| q(if i + 1 == n { 2 } else { 1 })).collect();
        assert_eq!(
            factor.solve_with_control(&rhs, &control).unwrap(),
            vec![q(1); n]
        );
        let rhs: Vec<_> = (0..n).map(|i| q(if i == 0 { 2 } else { 1 })).collect();
        assert_eq!(
            factor.solve_transpose_with_control(&rhs, &control).unwrap(),
            vec![q(1); n]
        );
    }
}
