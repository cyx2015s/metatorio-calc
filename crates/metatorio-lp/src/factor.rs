//! Exact sparsity-ordered LU shared by primal and transpose solves.
use crate::{MAX_RECONSTRUCTION_DIMENSION, Rational, SolveControl};
use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::Zero;

/// Bareiss elimination avoids rational normalization inside the cubic loop.
/// The recorded row transformations also solve the transpose, so pricing and
/// primal reconstruction use exactly the same integer factorization.
struct FractionFree {
    upper: Vec<Vec<BigInt>>,
    scales: Vec<BigInt>,
    columns: Vec<usize>,
    steps: Vec<IntegerStep>,
}
struct IntegerStep {
    swap: usize,
    pivot: BigInt,
    previous: BigInt,
    factors: Vec<BigInt>,
}
impl FractionFree {
    fn new(matrix: Vec<Vec<Rational>>, control: &SolveControl) -> Result<Self, String> {
        let n = matrix.len();
        let mut scales = Vec::with_capacity(n);
        let mut upper = Vec::with_capacity(n);
        for row in matrix {
            control.check()?;
            let denominator = row.iter().fold(BigInt::from(1), |d, a| d.lcm(a.denom()));
            upper.push(
                row.iter()
                    .map(|a| a.numer() * (&denominator / a.denom()))
                    .collect::<Vec<_>>(),
            );
            scales.push(denominator);
        }
        let mut columns: Vec<_> = (0..n).collect();
        let mut previous = BigInt::from(1);
        let mut steps = Vec::with_capacity(n);
        for k in 0..n {
            control.check()?;
            let column = (k..n)
                .min_by_key(|j| upper[k..].iter().filter(|r| !r[*j].is_zero()).count())
                .unwrap();
            for row in &mut upper {
                row.swap(k, column);
            }
            columns.swap(k, column);
            let row = (k..n)
                .filter(|i| !upper[*i][k].is_zero())
                .min_by_key(|i| upper[*i][k..].iter().filter(|a| !a.is_zero()).count())
                .ok_or("exactly singular basis")?;
            upper.swap(k, row);
            let pivot = upper[k][k].clone();
            let factors = (k + 1..n).map(|i| upper[i][k].clone()).collect::<Vec<_>>();
            for i in k + 1..n {
                control.check()?;
                for j in k + 1..n {
                    if upper[i][j].is_zero()
                        && (factors[i - k - 1].is_zero() || upper[k][j].is_zero())
                    {
                        continue;
                    }
                    let numerator = &pivot * &upper[i][j] - &factors[i - k - 1] * &upper[k][j];
                    let (value, remainder) = numerator.div_rem(&previous);
                    if !remainder.is_zero() {
                        return Err("nonexact fraction-free factor division".into());
                    }
                    upper[i][j] = value;
                }
                upper[i][k] = BigInt::zero();
            }
            steps.push(IntegerStep {
                swap: row,
                pivot: pivot.clone(),
                previous,
                factors,
            });
            previous = pivot;
        }
        Ok(Self {
            upper,
            scales,
            columns,
            steps,
        })
    }
    fn solve(&self, rhs: &[Rational], control: &SolveControl) -> Result<Vec<Rational>, String> {
        let n = self.upper.len();
        if rhs.len() != n {
            return Err("exact factor RHS dimension invalid".into());
        }
        let denominator = rhs.iter().fold(BigInt::from(1), |d, a| d.lcm(a.denom()));
        let mut values = rhs
            .iter()
            .zip(&self.scales)
            .map(|(a, s)| a.numer() * (&denominator / a.denom()) * s)
            .collect::<Vec<_>>();
        for (k, step) in self.steps.iter().enumerate() {
            control.check()?;
            values.swap(k, step.swap);
            for i in k + 1..n {
                let numerator = &step.pivot * &values[i] - &step.factors[i - k - 1] * &values[k];
                let (value, remainder) = numerator.div_rem(&step.previous);
                if !remainder.is_zero() {
                    return Err("nonexact fraction-free RHS division".into());
                }
                values[i] = value;
            }
        }
        let mut x = vec![Rational::zero(); n];
        for i in (0..n).rev() {
            control.check()?;
            let mut value = Rational::new(values[i].clone(), denominator.clone());
            for (j, entry) in self.upper[i].iter().enumerate().skip(i + 1) {
                if !entry.is_zero() && !x[j].is_zero() {
                    value -= Rational::from_integer(entry.clone()) * &x[j];
                }
            }
            x[i] = value / Rational::from_integer(self.upper[i][i].clone());
        }
        let mut result = vec![Rational::zero(); n];
        for (&j, value) in self.columns.iter().zip(x) {
            result[j] = value;
        }
        Ok(result)
    }
    fn transpose(&self, rhs: &[Rational], control: &SolveControl) -> Result<Vec<Rational>, String> {
        let n = self.upper.len();
        if rhs.len() != n {
            return Err("exact transpose RHS dimension invalid".into());
        }
        let mut x = vec![Rational::zero(); n];
        for i in 0..n {
            control.check()?;
            let mut value = rhs[self.columns[i]].clone();
            for (j, previous) in x.iter().enumerate().take(i) {
                if !self.upper[j][i].is_zero() && !previous.is_zero() {
                    value -= Rational::from_integer(self.upper[j][i].clone()) * previous;
                }
            }
            x[i] = value / Rational::from_integer(self.upper[i][i].clone());
        }
        // U = E D A Q: A^T y = b uses U^T z = Q^T b, y = D E^T z.
        for (k, step) in self.steps.iter().enumerate().rev() {
            control.check()?;
            for i in k + 1..n {
                let contribution =
                    Rational::new(step.factors[i - k - 1].clone(), step.previous.clone()) * &x[i];
                x[k] -= contribution;
                x[i] *= Rational::new(step.pivot.clone(), step.previous.clone());
            }
            x.swap(k, step.swap);
        }
        for (value, scale) in x.iter_mut().zip(&self.scales) {
            *value *= Rational::from_integer(scale.clone());
        }
        Ok(x)
    }
}

pub(crate) struct Factorization {
    blocks: Option<crate::factor_blocks::BlockFactorization>,
    integer: Option<FractionFree>,
    lu: Vec<Vec<Rational>>,
    permutation: Vec<usize>,
    column_permutation: Vec<usize>,
}

impl Factorization {
    #[cfg(test)]
    pub fn new(matrix: Vec<Vec<Rational>>) -> Result<Self, String> {
        Self::new_with_control(matrix, &SolveControl::default())
    }
    pub fn new_with_control(
        mut matrix: Vec<Vec<Rational>>,
        control: &SolveControl,
    ) -> Result<Self, String> {
        control.check()?;
        let n = matrix.len();
        if n > MAX_RECONSTRUCTION_DIMENSION || matrix.iter().any(|r| r.len() != n) {
            return Err("exact factorization dimension invalid".into());
        }
        if n >= 32 {
            if let Some(blocks) =
                crate::factor_blocks::BlockFactorization::try_new(&matrix, control)?
            {
                return Ok(Self {
                    blocks: Some(blocks),
                    integer: None,
                    lu: vec![],
                    permutation: vec![],
                    column_permutation: vec![],
                });
            }
            return Ok(Self {
                blocks: None,
                integer: Some(FractionFree::new(matrix, control)?),
                lu: vec![],
                permutation: vec![],
                column_permutation: vec![],
            });
        }
        let mut permutation: Vec<_> = (0..n).collect();
        let mut column_permutation = permutation.clone();
        let mut counts: Vec<_> = (0..n)
            .map(|j| matrix.iter().filter(|r| !r[j].is_zero()).count())
            .collect();
        for k in 0..n {
            control.check()?;
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
                control.check()?;
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
            blocks: None,
            integer: None,
            lu: matrix,
            permutation,
            column_permutation,
        })
    }

    #[cfg(test)]
    pub fn solve(&self, rhs: &[Rational]) -> Result<Vec<Rational>, String> {
        self.solve_with_control(rhs, &SolveControl::default())
    }
    pub fn solve_with_control(
        &self,
        rhs: &[Rational],
        control: &SolveControl,
    ) -> Result<Vec<Rational>, String> {
        control.check()?;
        if let Some(blocks) = &self.blocks {
            return blocks.solve(rhs, control);
        }
        if let Some(integer) = &self.integer {
            return integer.solve(rhs, control);
        }
        let n = self.lu.len();
        if rhs.len() != n {
            return Err("exact factor RHS dimension invalid".into());
        }
        let mut x: Vec<_> = self.permutation.iter().map(|i| rhs[*i].clone()).collect();
        // P A Q = L U; solve the triangular systems, then undo Q.
        for i in 0..n {
            control.check()?;
            for j in 0..i {
                if !self.lu[i][j].is_zero() && !x[j].is_zero() {
                    let product = &self.lu[i][j] * &x[j];
                    x[i] -= product;
                }
            }
        }
        for i in (0..n).rev() {
            control.check()?;
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

    #[cfg(test)]
    pub fn solve_transpose(&self, rhs: &[Rational]) -> Result<Vec<Rational>, String> {
        self.solve_transpose_with_control(rhs, &SolveControl::default())
    }
    pub fn solve_transpose_with_control(
        &self,
        rhs: &[Rational],
        control: &SolveControl,
    ) -> Result<Vec<Rational>, String> {
        control.check()?;
        if let Some(blocks) = &self.blocks {
            return blocks.transpose(rhs, control);
        }
        if let Some(integer) = &self.integer {
            return integer.transpose(rhs, control);
        }
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
            control.check()?;
            for j in 0..i {
                if !self.lu[j][i].is_zero() && !x[j].is_zero() {
                    let product = &self.lu[j][i] * &x[j];
                    x[i] -= product;
                }
            }
            x[i] /= &self.lu[i][i];
        }
        for i in (0..n).rev() {
            control.check()?;
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
    fn large_fraction_free_dispatch_preserves_scaled_and_permuted_solutions() {
        let n = 40;
        let expected: Vec<_> = (0..n)
            .map(|i| Rational::new(((i % 5) as i32 - 2).into(), 3.into()))
            .collect();
        let matrix: Vec<Vec<_>> = (0..n)
            .map(|i| {
                (0..n)
                    .map(|j| {
                        let row = (i * 7) % n;
                        let column = (j * 3) % n;
                        let coefficient = if row == column {
                            rational(3.0).unwrap()
                        } else if column == (row + 1) % n {
                            Rational::new(1.into(), 7.into())
                        } else if column == (row + 2) % n {
                            Rational::new((-1).into(), 13.into())
                        } else {
                            Rational::zero()
                        };
                        coefficient * rational(2.0f64.powi((i as i32 % 9) - 4)).unwrap()
                    })
                    .collect()
            })
            .collect();
        let rhs = matrix
            .iter()
            .map(|row| row.iter().zip(&expected).map(|(a, x)| a * x).sum())
            .collect::<Vec<_>>();
        let factor = Factorization::new(matrix.clone()).unwrap();
        assert!(factor.integer.is_some());
        assert_eq!(factor.solve(&rhs).unwrap(), expected);
        let transpose_rhs = (0..n)
            .map(|j| {
                matrix
                    .iter()
                    .zip(&expected)
                    .map(|(row, x)| &row[j] * x)
                    .sum()
            })
            .collect::<Vec<_>>();
        assert_eq!(factor.solve_transpose(&transpose_rhs).unwrap(), expected);
        let mut singular = matrix;
        singular[0].fill(Rational::zero());
        assert!(Factorization::new(singular).is_err());
    }

    #[test]
    fn fraction_free_directions_match_independent_rational_elimination() {
        let mut state = 41u64;
        for n in 0..9 {
            for _ in 0..12 {
                let mut next = || {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                    Rational::new(
                        (((state >> 32) % 9) as i32 - 4).into(),
                        ((state % 7) + 1).into(),
                    )
                };
                let matrix: Vec<Vec<_>> =
                    (0..n).map(|_| (0..n).map(|_| next()).collect()).collect();
                let rhs: Vec<_> = (0..n).map(|_| next()).collect();
                let reference = solve(matrix.clone(), rhs.clone());
                let factor = FractionFree::new(matrix.clone(), &SolveControl::default());
                assert_eq!(factor.is_ok(), reference.is_ok());
                if let Ok(factor) = factor {
                    assert_eq!(
                        factor.solve(&rhs, &SolveControl::default()).unwrap(),
                        reference.unwrap()
                    );
                    let transpose = (0..n)
                        .map(|j| matrix.iter().map(|r| r[j].clone()).collect())
                        .collect();
                    assert_eq!(
                        factor.transpose(&rhs, &SolveControl::default()).unwrap(),
                        solve(transpose, rhs.clone()).unwrap()
                    );
                    let expired = SolveControl::with_deadline(std::time::Instant::now());
                    if n > 0 {
                        assert!(
                            factor
                                .solve(&rhs, &expired)
                                .unwrap_err()
                                .contains("cancelled")
                        );
                        assert!(
                            factor
                                .transpose(&rhs, &expired)
                                .unwrap_err()
                                .contains("cancelled")
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn deadline_interrupts_dense_elimination_and_both_solve_directions() {
        let n = 88;
        let matrix = (0..n)
            .map(|i| {
                (0..n)
                    .map(|j| Rational::new(1.into(), (i + j + 1).into()))
                    .collect()
            })
            .collect();
        let control = SolveControl::with_deadline(
            std::time::Instant::now() + std::time::Duration::from_millis(1),
        );
        let result = Factorization::new_with_control(matrix, &control);
        assert!(matches!(result, Err(message) if message.contains("cancelled")));
        let matrix = vec![
            vec![rational(2.0).unwrap(), rational(1.0).unwrap()],
            vec![rational(1.0).unwrap(), rational(3.0).unwrap()],
        ];
        let factor = Factorization::new(matrix).unwrap();
        let rhs = vec![rational(1.0).unwrap(); 2];
        let expired = SolveControl::with_deadline(std::time::Instant::now());
        assert!(
            factor
                .solve_with_control(&rhs, &expired)
                .unwrap_err()
                .contains("cancelled")
        );
        assert!(
            factor
                .solve_transpose_with_control(&rhs, &expired)
                .unwrap_err()
                .contains("cancelled")
        );
        assert_eq!(
            factor.solve(&rhs).unwrap(),
            factor.solve_transpose(&rhs).unwrap()
        );
    }
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
