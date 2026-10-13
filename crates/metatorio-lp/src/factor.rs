//! Exact sparsity-ordered LU shared by primal and transpose solves.
use crate::{MAX_RECONSTRUCTION_DIMENSION, Rational};
use num_traits::Zero;

pub(crate) struct Factorization {
    lu: Vec<Vec<Rational>>,
    permutation: Vec<usize>,
    column_permutation: Vec<usize>,
}

impl Factorization {
    pub fn new(mut matrix: Vec<Vec<Rational>>) -> Result<Self, String> {
        let n = matrix.len();
        if n > MAX_RECONSTRUCTION_DIMENSION || matrix.iter().any(|r| r.len() != n) {
            return Err("exact factorization dimension invalid".into());
        }
        let mut permutation: Vec<_> = (0..n).collect();
        let mut column_permutation = permutation.clone();
        let mut counts: Vec<_> = (0..n)
            .map(|j| matrix.iter().filter(|r| !r[j].is_zero()).count())
            .collect();
        for k in 0..n {
            // Eliminate sparse columns first, especially singleton chains.
            // Counts cover only the active trailing submatrix, updated exactly
            // for cancellation and fill. No small coefficient is dropped.
            let column = (k..n).min_by_key(|j| counts[*j]).unwrap();
            if counts[column] == 0 {
                return Err("exactly singular basis".into());
            }
            for row in &mut matrix {
                row.swap(k, column);
            }
            counts.swap(k, column);
            column_permutation.swap(k, column);
            let p = (k..n)
                .filter(|i| !matrix[*i][k].is_zero())
                .min_by_key(|i| matrix[*i][k..].iter().filter(|a| !a.is_zero()).count())
                .ok_or("exactly singular basis")?;
            // Earlier lower-triangular entries must follow every row exchange.
            matrix.swap(k, p);
            permutation.swap(k, p);
            let nonzeros: Vec<_> = (k + 1..n).filter(|j| !matrix[k][*j].is_zero()).collect();
            for j in &nonzeros {
                counts[*j] -= 1;
            }
            for i in k + 1..n {
                if matrix[i][k].is_zero() {
                    continue;
                }
                let multiplier = &matrix[i][k] / &matrix[k][k];
                for j in &nonzeros {
                    let was_zero = matrix[i][*j].is_zero();
                    let product = &multiplier * &matrix[k][*j];
                    matrix[i][*j] -= product;
                    let is_zero = matrix[i][*j].is_zero();
                    if was_zero && !is_zero {
                        counts[*j] += 1;
                    }
                    if !was_zero && is_zero {
                        counts[*j] -= 1;
                    }
                }
                matrix[i][k] = multiplier;
            }
        }
        Ok(Self {
            lu: matrix,
            permutation,
            column_permutation,
        })
    }

    pub fn solve(&self, rhs: &[Rational]) -> Result<Vec<Rational>, String> {
        let n = self.lu.len();
        if rhs.len() != n {
            return Err("exact factor RHS dimension invalid".into());
        }
        let mut x: Vec<_> = self.permutation.iter().map(|i| rhs[*i].clone()).collect();
        // P A Q = L U; solve the triangular systems, then undo Q.
        for i in 0..n {
            for j in 0..i {
                if !self.lu[i][j].is_zero() && !x[j].is_zero() {
                    let product = &self.lu[i][j] * &x[j];
                    x[i] -= product;
                }
            }
        }
        for i in (0..n).rev() {
            for j in i + 1..n {
                if !self.lu[i][j].is_zero() && !x[j].is_zero() {
                    let product = &self.lu[i][j] * &x[j];
                    x[i] -= product;
                }
            }
            x[i] /= &self.lu[i][i];
        }
        let mut result = vec![Rational::zero(); n];
        for (p, value) in self.column_permutation.iter().zip(x) {
            result[*p] = value;
        }
        Ok(result)
    }

    pub fn solve_transpose(&self, rhs: &[Rational]) -> Result<Vec<Rational>, String> {
        let n = self.lu.len();
        if rhs.len() != n {
            return Err("exact transpose RHS dimension invalid".into());
        }
        let mut x: Vec<_> = self
            .column_permutation
            .iter()
            .map(|i| rhs[*i].clone())
            .collect();
        // Q^T A^T P^T = U^T L^T; solve, then undo P.
        for i in 0..n {
            for j in 0..i {
                if !self.lu[j][i].is_zero() && !x[j].is_zero() {
                    let product = &self.lu[j][i] * &x[j];
                    x[i] -= product;
                }
            }
            x[i] /= &self.lu[i][i];
        }
        for i in (0..n).rev() {
            for j in i + 1..n {
                if !self.lu[j][i].is_zero() && !x[j].is_zero() {
                    let product = &self.lu[j][i] * &x[j];
                    x[i] -= product;
                }
            }
        }
        let mut result = vec![Rational::zero(); n];
        for (p, value) in self.permutation.iter().zip(x) {
            result[*p] = value;
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{rational, solve};
    #[test]
    fn both_solve_directions_match_independent_elimination_with_row_swaps() {
        let mut state = 11u64;
        for n in 0..9 {
            for _ in 0..12 {
                let mut next = || {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                    rational(((state >> 32) % 7) as f64 - 3.0).unwrap()
                };
                let matrix: Vec<Vec<_>> =
                    (0..n).map(|_| (0..n).map(|_| next()).collect()).collect();
                let rhs: Vec<_> = (0..n).map(|_| next()).collect();
                let reference = solve(matrix.clone(), rhs.clone());
                let factor = Factorization::new(matrix.clone());
                assert_eq!(factor.is_ok(), reference.is_ok());
                if let Ok(factor) = factor {
                    assert_eq!(factor.solve(&rhs).unwrap(), reference.unwrap());
                    let transpose = (0..n)
                        .map(|j| matrix.iter().map(|r| r[j].clone()).collect())
                        .collect();
                    assert_eq!(
                        factor.solve_transpose(&rhs).unwrap(),
                        solve(transpose, rhs).unwrap()
                    );
                }
            }
        }
    }
}
