//! Exact compressed basis inverse, including row-slack exchanges.
use super::{Basis, BasisStatus, ExactSolution, Model, Rational, rational};
use num_traits::Zero;

pub(super) struct System {
    pub columns: Vec<usize>,
    pub rows: Vec<usize>,
    inverse: Vec<Vec<Rational>>,
}

fn indices(basis: &Basis) -> (Vec<usize>, Vec<usize>) {
    (
        basis
            .columns
            .iter()
            .enumerate()
            .filter_map(|(i, s)| (*s == BasisStatus::Basic).then_some(i))
            .collect(),
        basis
            .rows
            .iter()
            .enumerate()
            .filter_map(|(i, s)| (*s != BasisStatus::Basic).then_some(i))
            .collect(),
    )
}
fn matrix(model: &Model, columns: &[usize], rows: &[usize]) -> Result<Vec<Vec<Rational>>, String> {
    if columns.len() != rows.len() || columns.len() > super::MAX_BASIS_DIMENSION {
        return Err("compressed basis dimension exceeds limit or is not square".into());
    }
    let mut positions = vec![None; model.columns.len()];
    for (i, column) in columns.iter().enumerate() {
        positions[*column] = Some(i);
    }
    let mut result = vec![vec![Rational::zero(); columns.len()]; rows.len()];
    for (i, row) in rows.iter().enumerate() {
        for (column, a) in &model.rows[*row].terms {
            if let Some(j) = positions[*column] {
                result[i][j] += rational(*a)?;
            }
        }
    }
    Ok(result)
}
fn invert(mut matrix: Vec<Vec<Rational>>) -> Result<Vec<Vec<Rational>>, String> {
    let n = matrix.len();
    let mut inverse = vec![vec![Rational::zero(); n]; n];
    for (i, row) in inverse.iter_mut().enumerate() {
        row[i] = rational(1.0)?;
    }
    for k in 0..n {
        let p = (k..n)
            .find(|i| !matrix[*i][k].is_zero())
            .ok_or("exactly singular cached basis")?;
        matrix.swap(k, p);
        inverse.swap(k, p);
        let pivot = matrix[k][k].clone();
        for j in 0..n {
            matrix[k][j] /= &pivot;
            inverse[k][j] /= &pivot;
        }
        for i in 0..n {
            if i == k || matrix[i][k].is_zero() {
                continue;
            }
            let factor = matrix[i][k].clone();
            for j in 0..n {
                let a = &factor * &matrix[k][j];
                matrix[i][j] -= a;
                let a = &factor * &inverse[k][j];
                inverse[i][j] -= a;
            }
        }
    }
    Ok(inverse)
}
fn product(matrix: &[Vec<Rational>], rhs: &[Rational]) -> Vec<Rational> {
    matrix
        .iter()
        .map(|row| {
            row.iter()
                .zip(rhs)
                .filter(|(a, b)| !a.is_zero() && !b.is_zero())
                .map(|(a, b)| a * b)
                .sum()
        })
        .collect()
}
impl System {
    pub fn new(model: &Model, basis: &Basis) -> Result<Self, String> {
        if basis.columns.len() != model.columns.len() || basis.rows.len() != model.rows.len() {
            return Err("basis dimensions differ from model".into());
        }
        let (columns, rows) = indices(basis);
        let inverse = invert(matrix(model, &columns, &rows)?)?;
        Ok(Self {
            columns,
            rows,
            inverse,
        })
    }
    pub fn reconstruct(&self, model: &Model) -> Result<ExactSolution, String> {
        // Used only by the nonnegative-unbounded-above simplex: all nonbasic
        // structural variables are exactly zero, including deleted artificials.
        let rhs = self
            .rows
            .iter()
            .map(|i| rational(model.rows[*i].rhs))
            .collect::<Result<Vec<_>, _>>()?;
        let basic = product(&self.inverse, &rhs);
        let costs = self
            .columns
            .iter()
            .map(|i| rational(model.columns[*i].cost))
            .collect::<Result<Vec<_>, _>>()?;
        let mut values = vec![Rational::zero(); model.columns.len()];
        let mut row_duals = vec![Rational::zero(); model.rows.len()];
        for (i, x) in self.columns.iter().zip(basic) {
            values[*i] = x;
        }
        for (j, row) in self.rows.iter().enumerate() {
            row_duals[*row] = self
                .inverse
                .iter()
                .zip(&costs)
                .filter(|(r, c)| !r[j].is_zero() && !c.is_zero())
                .map(|(r, c)| &r[j] * c)
                .sum();
        }
        Ok(ExactSolution {
            values,
            row_duals,
            basis_dimension: self.columns.len(),
        })
    }
    pub fn direction(&self, model: &Model, entering: usize) -> Result<Vec<Rational>, String> {
        let rhs = self
            .rows
            .iter()
            .map(|row| {
                let mut value = Rational::zero();
                if entering == model.columns.len() + row {
                    value -= rational(1.0)?;
                }
                for (i, a) in &model.rows[*row].terms {
                    if *i == entering {
                        value -= rational(*a)?;
                    }
                }
                Ok(value)
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(product(&self.inverse, &rhs))
    }
    /// Row of the full basis inverse for a structural variable or basic slack.
    /// Multiplying this functional by b gives that variable's current value.
    pub fn functional(
        model: &Model,
        basis: &Basis,
        variable: usize,
    ) -> Result<Vec<Rational>, String> {
        let (columns, rows) = indices(basis);
        let mut rhs = vec![Rational::zero(); columns.len()];
        let mut result = vec![Rational::zero(); model.rows.len()];
        if variable < model.columns.len() {
            let p = columns
                .iter()
                .position(|i| *i == variable)
                .ok_or("functional requires a basic column")?;
            rhs[p] = rational(1.0)?;
        } else {
            let row = variable - model.columns.len();
            if row >= model.rows.len() || rows.contains(&row) {
                return Err("functional requires a basic row slack".into());
            }
            let mut positions = vec![None; model.columns.len()];
            for (j, column) in columns.iter().enumerate() {
                positions[*column] = Some(j);
            }
            for (column, a) in &model.rows[row].terms {
                if let Some(j) = positions[*column] {
                    rhs[j] -= rational(*a)?;
                }
            }
            result[row] = rational(1.0)?;
        }
        let matrix = matrix(model, &columns, &rows)?;
        let transpose = (0..columns.len())
            .map(|j| matrix.iter().map(|r| r[j].clone()).collect())
            .collect();
        for (row, value) in rows.iter().zip(super::solve(transpose, rhs)?) {
            result[*row] = value;
        }
        Ok(result)
    }
    pub fn cached_functional(
        &self,
        model: &Model,
        variable: usize,
    ) -> Result<Vec<Rational>, String> {
        let mut result = vec![Rational::zero(); model.rows.len()];
        if variable < model.columns.len() {
            let p = self
                .columns
                .iter()
                .position(|i| *i == variable)
                .ok_or("functional requires a basic column")?;
            for (j, row) in self.rows.iter().enumerate() {
                result[*row] = self.inverse[p][j].clone();
            }
        } else {
            let row = variable - model.columns.len();
            if row >= model.rows.len() || self.rows.contains(&row) {
                return Err("functional requires a basic row slack".into());
            }
            let mut positions = vec![None; model.columns.len()];
            for (j, column) in self.columns.iter().enumerate() {
                positions[*column] = Some(j);
            }
            let mut coefficients = vec![Rational::zero(); self.columns.len()];
            for (column, a) in &model.rows[row].terms {
                if let Some(j) = positions[*column] {
                    coefficients[j] += rational(*a)?;
                }
            }
            result[row] = rational(1.0)?;
            for (j, active) in self.rows.iter().enumerate() {
                result[*active] = -self
                    .inverse
                    .iter()
                    .zip(&coefficients)
                    .filter(|(r, a)| !r[j].is_zero() && !a.is_zero())
                    .map(|(r, a)| &r[j] * a)
                    .sum::<Rational>();
            }
        }
        Ok(result)
    }
    pub fn update(
        &mut self,
        model: &Model,
        basis: &Basis,
        entering: usize,
        leaving: usize,
    ) -> Result<(), String> {
        let n = self.columns.len();
        let mut columns = self.columns.clone();
        let mut rows = self.rows.clone();
        let structural_enter = entering < model.columns.len();
        let structural_leave = leaving < model.columns.len();
        let inverse = match (structural_enter, structural_leave) {
            (true, true) => {
                let p = columns
                    .iter()
                    .position(|i| *i == leaving)
                    .ok_or("leaving column not basic")?;
                columns[p] = entering;
                let a = matrix(model, &columns, &rows)?;
                let column: Vec<_> = a.iter().map(|r| r[p].clone()).collect();
                let u = product(&self.inverse, &column);
                if u[p].is_zero() {
                    return Err("zero exact column update pivot".into());
                }
                let mut result = self.inverse.clone();
                for value in &mut result[p] {
                    *value /= &u[p];
                }
                for i in 0..n {
                    if i != p && !u[i].is_zero() {
                        for j in 0..n {
                            let change = &u[i] * &result[p][j];
                            result[i][j] -= change;
                        }
                    }
                }
                result
            }
            (false, false) => {
                let q = rows
                    .iter()
                    .position(|i| *i == entering - model.columns.len())
                    .ok_or("entering slack not nonbasic")?;
                rows[q] = leaving - model.columns.len();
                let a = matrix(model, &columns, &rows)?;
                let w: Vec<Rational> = (0..n)
                    .map(|j| a[q].iter().zip(&self.inverse).map(|(v, r)| v * &r[j]).sum())
                    .collect();
                if w[q].is_zero() {
                    return Err("zero exact row update pivot".into());
                }
                let mut result = self.inverse.clone();
                for i in 0..n {
                    for j in 0..n {
                        let delta = if j == q {
                            &w[j] - rational(1.0)?
                        } else {
                            w[j].clone()
                        };
                        result[i][j] -= &self.inverse[i][q] * delta / &w[q];
                    }
                }
                result
            }
            (true, false) => {
                columns.push(entering);
                rows.push(leaving - model.columns.len());
                let a = matrix(model, &columns, &rows)?;
                let u: Vec<_> = a.iter().take(n).map(|r| r[n].clone()).collect();
                let pu = product(&self.inverse, &u);
                let vp: Vec<Rational> = (0..n)
                    .map(|j| {
                        a[n].iter()
                            .take(n)
                            .zip(&self.inverse)
                            .map(|(v, r)| v * &r[j])
                            .sum()
                    })
                    .collect();
                let schur = &a[n][n]
                    - a[n]
                        .iter()
                        .take(n)
                        .zip(&pu)
                        .map(|(v, p)| v * p)
                        .sum::<Rational>();
                if schur.is_zero() {
                    return Err("zero exact bordered update pivot".into());
                }
                let mut result = vec![vec![Rational::zero(); n + 1]; n + 1];
                for i in 0..n {
                    for j in 0..n {
                        result[i][j] = &self.inverse[i][j] + &pu[i] * &vp[j] / &schur;
                    }
                    result[i][n] = -&pu[i] / &schur;
                    result[n][i] = -&vp[i] / &schur;
                }
                result[n][n] = rational(1.0)? / schur;
                result
            }
            (false, true) => {
                let p = columns
                    .iter()
                    .position(|i| *i == leaving)
                    .ok_or("leaving column not basic")?;
                let q = rows
                    .iter()
                    .position(|i| *i == entering - model.columns.len())
                    .ok_or("entering slack not nonbasic")?;
                let pivot = &self.inverse[p][q];
                if pivot.is_zero() {
                    return Err("zero exact deletion pivot".into());
                }
                let result = (0..n)
                    .filter(|i| *i != p)
                    .map(|i| {
                        (0..n)
                            .filter(|j| *j != q)
                            .map(|j| {
                                &self.inverse[i][j]
                                    - &self.inverse[i][q] * &self.inverse[p][j] / pivot
                            })
                            .collect()
                    })
                    .collect();
                columns.remove(p);
                rows.remove(q);
                result
            }
        };
        let (new_columns, new_rows) = indices(basis);
        // Basis statuses use canonical index order; permute the updated inverse
        // without arithmetic so checkpoint reconstruction is deterministic.
        let cp = new_columns
            .iter()
            .map(|i| {
                columns
                    .iter()
                    .position(|j| i == j)
                    .ok_or("column update shape mismatch")
            })
            .collect::<Result<Vec<_>, _>>()?;
        let rp = new_rows
            .iter()
            .map(|i| {
                rows.iter()
                    .position(|j| i == j)
                    .ok_or("row update shape mismatch")
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.inverse = cp
            .iter()
            .map(|i| rp.iter().map(|j| inverse[*i][*j].clone()).collect())
            .collect();
        self.columns = new_columns;
        self.rows = new_rows;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Column, Row};
    use super::*;
    #[test]
    fn all_exchange_shapes_match_independent_full_reconstruction() {
        let model = Model {
            columns: (1..=3)
                .map(|cost| Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost: cost as f64,
                })
                .collect(),
            rows: vec![
                Row {
                    equality: false,
                    rhs: 2.0,
                    terms: vec![(0, 2.0), (1, 3.0), (2, 5.0)],
                },
                Row {
                    equality: false,
                    rhs: 7.0,
                    terms: vec![(0, 7.0), (1, 11.0), (2, 13.0)],
                },
                Row {
                    equality: false,
                    rhs: 17.0,
                    terms: vec![(0, 17.0), (1, 19.0), (2, 23.0)],
                },
            ],
            offset: 0.0,
        };
        let mut basis = Basis {
            columns: vec![BasisStatus::Basic, BasisStatus::Lower, BasisStatus::Lower],
            rows: vec![BasisStatus::Upper, BasisStatus::Basic, BasisStatus::Basic],
        };
        let mut system = System::new(&model, &basis).unwrap();
        // column replacement, border, row replacement, deletion to 1, to 0.
        for (entering, leaving) in [(1, 0), (2, 4), (3, 5), (4, 1), (5, 2)] {
            if entering < 3 {
                basis.columns[entering] = BasisStatus::Basic;
            } else {
                basis.rows[entering - 3] = BasisStatus::Basic;
            }
            if leaving < 3 {
                basis.columns[leaving] = BasisStatus::Lower;
            } else {
                basis.rows[leaving - 3] = BasisStatus::Upper;
            }
            system.update(&model, &basis, entering, leaving).unwrap();
            let updated = system.reconstruct(&model).unwrap();
            let independent = model.reconstruct(&basis).unwrap();
            assert_eq!(updated.values, independent.values);
            assert_eq!(updated.row_duals, independent.row_duals);
            for (variable, status) in basis.columns.iter().chain(&basis.rows).enumerate() {
                if *status == BasisStatus::Basic {
                    assert_eq!(
                        system.cached_functional(&model, variable).unwrap(),
                        System::functional(&model, &basis, variable).unwrap()
                    );
                }
            }
        }
    }
}
