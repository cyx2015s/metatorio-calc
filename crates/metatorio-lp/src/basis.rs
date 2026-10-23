//! Sparse exact basis factorization with full-basis eta updates and compressed compatibility.
use super::{Basis, BasisStatus, ExactSolution, Model, Rational, SolveControl, rational};
use num_traits::Zero;
use std::collections::BTreeMap;

pub(super) struct System {
    pub columns: Vec<usize>,
    pub rows: Vec<usize>,
    factor: super::sparse_exact_factor::SparseExactFactor,
    ids: Vec<usize>,
    positions: Vec<Option<usize>>,
    etas: Vec<Eta>,
    factor_weight: u64,
    eta_weight: u64,
}

struct Eta {
    position: usize,
    pivot: Rational,
    off_diagonal: Vec<(usize, Rational)>,
}
pub(super) struct Factored {
    pub columns: Vec<usize>,
    pub rows: Vec<usize>,
    pub factor: super::sparse_exact_factor::SparseExactFactor,
}
impl Factored {
    #[cfg(test)]
    pub fn direction(&self, model: &Model, entering: usize) -> Result<Vec<Rational>, String> {
        self.direction_with_control(model, entering, &SolveControl::default())
    }
    pub fn direction_with_control(
        &self,
        model: &Model,
        entering: usize,
        control: &SolveControl,
    ) -> Result<Vec<Rational>, String> {
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
        self.factor.solve_with_control(&rhs, control)
    }
    #[cfg(test)]
    pub fn functional(&self, model: &Model, variable: usize) -> Result<Vec<Rational>, String> {
        self.functional_with_control(model, variable, &SolveControl::default())
    }
    pub fn functional_with_control(
        &self,
        model: &Model,
        variable: usize,
        control: &SolveControl,
    ) -> Result<Vec<Rational>, String> {
        let mut rhs = vec![Rational::zero(); self.columns.len()];
        let mut result = vec![Rational::zero(); model.rows.len()];
        if variable < model.columns.len() {
            let p = self
                .columns
                .iter()
                .position(|i| *i == variable)
                .ok_or("functional requires a basic column")?;
            rhs[p] = rational(1.0)?;
        } else {
            let row = variable - model.columns.len();
            if row >= model.rows.len() || self.rows.contains(&row) {
                return Err("functional requires a basic row slack".into());
            }
            let mut positions = vec![None; model.columns.len()];
            for (j, column) in self.columns.iter().enumerate() {
                positions[*column] = Some(j);
            }
            for (column, a) in &model.rows[row].terms {
                if let Some(j) = positions[*column] {
                    rhs[j] -= rational(*a)?;
                }
            }
            result[row] = rational(1.0)?;
        }
        for (row, value) in self
            .rows
            .iter()
            .zip(self.factor.solve_transpose_with_control(&rhs, control)?)
        {
            result[*row] = value;
        }
        Ok(result)
    }
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

impl System {
    #[cfg(test)]
    pub fn new(model: &Model, basis: &Basis) -> Result<Self, String> {
        Self::new_with_control(model, basis, &SolveControl::default())
    }
    pub fn new_with_control(
        model: &Model,
        basis: &Basis,
        control: &SolveControl,
    ) -> Result<Self, String> {
        Self::validate(model, basis, control)?;
        if basis.columns.len() != model.columns.len() || basis.rows.len() != model.rows.len() {
            return Err("basis dimensions differ from model".into());
        }
        let (columns, rows) = indices(basis);
        if columns.len() != rows.len() {
            return Err("compressed basis is not square".into());
        }
        let ids: Vec<_> = columns
            .iter()
            .copied()
            .chain(basis.rows.iter().enumerate().filter_map(|(row, status)| {
                (*status == BasisStatus::Basic).then_some(model.columns.len() + row)
            }))
            .collect();
        let positions = Self::positions(model, &ids);
        let factor = super::sparse_exact_factor::SparseExactFactor::new(
            Self::full_matrix(model, &positions, control)?,
            control,
        )?;
        let factor_weight = factor.arithmetic_weight(control)?;
        control.check()?;
        Ok(Self {
            columns,
            rows,
            factor,
            ids,
            positions,
            etas: Vec::new(),
            factor_weight,
            eta_weight: 0,
        })
    }
    fn validate(model: &Model, basis: &Basis, control: &SolveControl) -> Result<(), String> {
        control.check()?;
        if basis.columns.len() != model.columns.len() || basis.rows.len() != model.rows.len() {
            return Err("basis dimensions differ from model".into());
        }
        for (column, status) in model.columns.iter().zip(&basis.columns) {
            control.check()?;
            let value = match status {
                BasisStatus::Basic => continue,
                BasisStatus::Lower => rational(column.lower)?,
                BasisStatus::Upper => rational(column.upper)?,
                BasisStatus::Zero if column.lower <= 0.0 && column.upper >= 0.0 => Rational::zero(),
                BasisStatus::Zero => return Err("nonbasic zero outside column bounds".into()),
            };
            if !value.is_zero() {
                return Err("cached basis requires zero nonbasic column values".into());
            }
        }
        for (row, status) in model.rows.iter().zip(&basis.rows) {
            control.check()?;
            if *status == BasisStatus::Basic {
                continue;
            }
            if !row.equality
                && *status != BasisStatus::Upper
                && !(*status == BasisStatus::Zero && row.rhs == 0.0)
            {
                return Err("inequality basis row must be at its finite upper bound".into());
            }
            if row.equality && *status == BasisStatus::Zero && row.rhs != 0.0 {
                return Err("nonzero equality row cannot have zero activity".into());
            }
            rational(row.rhs)?;
        }
        Ok(())
    }
    fn positions(model: &Model, ids: &[usize]) -> Vec<Option<usize>> {
        let mut positions = vec![None; model.columns.len() + model.rows.len()];
        for (slot, &id) in ids.iter().enumerate() {
            positions[id] = Some(slot);
        }
        positions
    }
    fn full_matrix(
        model: &Model,
        positions: &[Option<usize>],
        control: &SolveControl,
    ) -> Result<Vec<BTreeMap<usize, Rational>>, String> {
        let mut matrix = Vec::with_capacity(model.rows.len());
        for (row, source) in model.rows.iter().enumerate() {
            control.check()?;
            let mut coefficients = BTreeMap::new();
            for &(column, a) in &source.terms {
                control.check()?;
                if let Some(slot) = *positions.get(column).ok_or("invalid model column index")? {
                    *coefficients.entry(slot).or_insert_with(Rational::zero) += rational(a)?;
                }
            }
            if let Some(slot) = positions[model.columns.len() + row] {
                coefficients.insert(slot, rational(1.0)?);
            }
            matrix.push(coefficients);
        }
        Ok(matrix)
    }
    fn column(
        model: &Model,
        variable: usize,
        control: &SolveControl,
    ) -> Result<Vec<Rational>, String> {
        if variable >= model.columns.len() + model.rows.len() {
            return Err("invalid entering variable".into());
        }
        model
            .rows
            .iter()
            .enumerate()
            .map(|(row, source)| {
                control.check()?;
                if variable >= model.columns.len() {
                    return rational(if variable == model.columns.len() + row {
                        1.0
                    } else {
                        0.0
                    });
                }
                let mut value = Rational::zero();
                for (term, &(column, a)) in source.terms.iter().enumerate() {
                    if term % 1024 == 0 {
                        control.check()?;
                    }
                    if column == variable {
                        value += rational(a)?;
                    }
                }
                Ok(value)
            })
            .collect()
    }
    fn solve(&self, rhs: &[Rational], control: &SolveControl) -> Result<Vec<Rational>, String> {
        let mut result = self.factor.solve_with_control(rhs, control)?;
        for eta in &self.etas {
            control.check()?;
            if result[eta.position].is_zero() {
                continue;
            }
            let value = &result[eta.position] / &eta.pivot;
            for (row, coefficient) in &eta.off_diagonal {
                control.check()?;
                result[*row] -= coefficient * &value;
            }
            result[eta.position] = value;
        }
        Ok(result)
    }
    fn solve_transpose(
        &self,
        rhs: &[Rational],
        control: &SolveControl,
    ) -> Result<Vec<Rational>, String> {
        let mut result = rhs.to_vec();
        for eta in self.etas.iter().rev() {
            control.check()?;
            let mut value = result[eta.position].clone();
            for (row, coefficient) in &eta.off_diagonal {
                control.check()?;
                if !result[*row].is_zero() {
                    value -= coefficient * &result[*row];
                }
            }
            result[eta.position] = if value.is_zero() {
                value
            } else {
                value / &eta.pivot
            };
        }
        self.factor.solve_transpose_with_control(&result, control)
    }
    #[cfg(test)]
    pub fn reconstruct(&self, model: &Model) -> Result<ExactSolution, String> {
        self.reconstruct_with_control(model, &SolveControl::default())
    }
    pub fn reconstruct_with_control(
        &self,
        model: &Model,
        control: &SolveControl,
    ) -> Result<ExactSolution, String> {
        control.check()?;
        let rhs = model
            .rows
            .iter()
            .map(|row| rational(row.rhs))
            .collect::<Result<Vec<_>, _>>()?;
        let basic = self.solve(&rhs, control)?;
        let costs = self
            .ids
            .iter()
            .map(|id| {
                if *id < model.columns.len() {
                    rational(model.columns[*id].cost)
                } else {
                    Ok(Rational::zero())
                }
            })
            .collect::<Result<Vec<_>, String>>()?;
        let row_duals = self.solve_transpose(&costs, control)?;
        let mut values = vec![Rational::zero(); model.columns.len()];
        for &column in &self.columns {
            values[column] = basic[self.positions[column].unwrap()].clone();
        }
        Ok(ExactSolution {
            values,
            row_duals,
            basis_dimension: self.columns.len(),
        })
    }
    #[cfg(test)]
    pub fn direction(&self, model: &Model, entering: usize) -> Result<Vec<Rational>, String> {
        self.direction_with_control(model, entering, &SolveControl::default())
    }
    pub fn direction_with_control(
        &self,
        model: &Model,
        entering: usize,
        control: &SolveControl,
    ) -> Result<Vec<Rational>, String> {
        let rhs = Self::column(model, entering, control)?;
        let solution = self.solve(&rhs, control)?;
        Ok(self
            .columns
            .iter()
            .map(|column| -solution[self.positions[*column].unwrap()].clone())
            .collect())
    }
    #[cfg(test)]
    pub fn functional(
        model: &Model,
        basis: &Basis,
        variable: usize,
    ) -> Result<Vec<Rational>, String> {
        Self::functional_with_control(model, basis, variable, &SolveControl::default())
    }
    #[cfg(test)]
    pub fn functional_with_control(
        model: &Model,
        basis: &Basis,
        variable: usize,
        control: &SolveControl,
    ) -> Result<Vec<Rational>, String> {
        Self::new_with_control(model, basis, control)?
            .cached_functional_with_control(model, variable, control)
    }
    #[cfg(test)]
    pub fn cached_functional(
        &self,
        model: &Model,
        variable: usize,
    ) -> Result<Vec<Rational>, String> {
        self.cached_functional_with_control(model, variable, &SolveControl::default())
    }
    pub fn cached_functional_with_control(
        &self,
        model: &Model,
        variable: usize,
        control: &SolveControl,
    ) -> Result<Vec<Rational>, String> {
        control.check()?;
        if self.positions.len() != model.columns.len() + model.rows.len() {
            return Err("basis dimensions differ from model".into());
        }
        let slot = self
            .positions
            .get(variable)
            .copied()
            .flatten()
            .ok_or("functional requires a basic variable")?;
        let mut rhs = vec![Rational::zero(); self.ids.len()];
        rhs[slot] = rational(1.0)?;
        self.solve_transpose(&rhs, control)
    }
    #[cfg(test)]
    pub fn update(
        &mut self,
        model: &Model,
        basis: &Basis,
        entering: usize,
        leaving: usize,
    ) -> Result<(), String> {
        self.update_with_control(model, basis, entering, leaving, &SolveControl::default())
    }
    pub fn update_with_control(
        &mut self,
        model: &Model,
        basis: &Basis,
        entering: usize,
        leaving: usize,
        control: &SolveControl,
    ) -> Result<(), String> {
        Self::validate(model, basis, control)?;
        let count = model.columns.len() + model.rows.len();
        if basis.columns.len() != model.columns.len()
            || basis.rows.len() != model.rows.len()
            || self.positions.len() != count
            || self.ids.len() != model.rows.len()
        {
            return Err("basis dimensions differ from model".into());
        }
        if entering >= count || leaving >= count || entering == leaving {
            return Err("invalid basis exchange indices".into());
        }
        let position = self.positions[leaving].ok_or("invalid leaving variable status")?;
        if self.positions[entering].is_some() {
            return Err("invalid entering variable status".into());
        }
        let (columns, rows) = indices(basis);
        if columns.len() != rows.len() {
            return Err("compressed basis is not square".into());
        }
        for (variable, status) in basis.columns.iter().chain(&basis.rows).enumerate() {
            control.check()?;
            let expected =
                variable == entering || (variable != leaving && self.positions[variable].is_some());
            if (*status == BasisStatus::Basic) != expected {
                return Err("basis exchange shape mismatch".into());
            }
        }
        let transformed = self.solve(&Self::column(model, entering, control)?, control)?;
        let pivot = transformed[position].clone();
        if pivot.is_zero() {
            return Err("exactly singular basis exchange".into());
        }
        let eta = Eta {
            position,
            pivot,
            off_diagonal: transformed
                .into_iter()
                .enumerate()
                .filter(|(row, value)| *row != position && !value.is_zero())
                .collect(),
        };
        let mut ids = self.ids.clone();
        ids[position] = entering;
        let positions = Self::positions(model, &ids);
        let eta_weight = std::iter::once(&eta.pivot)
            .chain(eta.off_diagonal.iter().map(|(_, value)| value))
            .fold(self.eta_weight, |weight, value| {
                weight.saturating_add(super::sparse_exact_factor::rational_work(value))
            });
        // Large rational coefficients can make a short update chain more
        // expensive than the factors. Rebuild early relative to factor cost;
        // neither this policy nor the 64-update cadence rejects any model.
        let factor = if self.etas.len() + 1 >= 64 || eta_weight > (self.factor_weight / 2).max(64) {
            let factor = super::sparse_exact_factor::SparseExactFactor::new(
                Self::full_matrix(model, &positions, control)?,
                control,
            )?;
            let weight = factor.arithmetic_weight(control)?;
            Some((factor, weight))
        } else {
            None
        };
        control.check()?;
        self.columns = columns;
        self.rows = rows;
        self.ids = ids;
        self.positions = positions;
        if let Some((factor, weight)) = factor {
            self.factor = factor;
            self.factor_weight = weight;
            self.eta_weight = 0;
            self.etas.clear();
        } else {
            self.etas.push(eta);
            self.eta_weight = eta_weight;
        }
        Ok(())
    }
    #[cfg(test)]
    pub fn uncached_direction(
        model: &Model,
        basis: &Basis,
        entering: usize,
    ) -> Result<(Vec<usize>, Vec<Rational>), String> {
        let (columns, rows) = indices(basis);
        let rhs = rows
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
        let values = product(&invert(matrix(model, &columns, &rows)?)?, &rhs);
        Ok((columns, values))
    }
}
#[cfg(test)]
fn dense_functional(
    model: &Model,
    basis: &Basis,
    variable: usize,
) -> Result<Vec<Rational>, String> {
    let (columns, rows) = indices(basis);
    let inverse = invert(matrix(model, &columns, &rows)?)?;
    let mut result = vec![Rational::zero(); model.rows.len()];
    if variable < model.columns.len() {
        let p = columns
            .iter()
            .position(|i| *i == variable)
            .ok_or("nonbasic column")?;
        for (j, row) in rows.iter().enumerate() {
            result[*row] = inverse[p][j].clone();
        }
    } else {
        let row = variable - model.columns.len();
        result[row] = rational(1.0)?;
        for (j, active) in rows.iter().enumerate() {
            for (column, coefficient) in &model.rows[row].terms {
                if let Some(p) = columns.iter().position(|i| i == column) {
                    result[*active] -= rational(*coefficient)? * &inverse[p][j];
                }
            }
        }
    }
    Ok(result)
}
#[cfg(test)]
fn dense_reconstruct(model: &Model, basis: &Basis) -> Result<ExactSolution, String> {
    let (columns, rows) = indices(basis);
    let inverse = invert(matrix(model, &columns, &rows)?)?;
    let rhs = rows
        .iter()
        .map(|i| rational(model.rows[*i].rhs))
        .collect::<Result<Vec<_>, _>>()?;
    let basic = product(&inverse, &rhs);
    let costs = columns
        .iter()
        .map(|i| rational(model.columns[*i].cost))
        .collect::<Result<Vec<_>, _>>()?;
    let mut values = vec![Rational::zero(); model.columns.len()];
    let mut row_duals = vec![Rational::zero(); model.rows.len()];
    for (i, value) in columns.iter().zip(basic) {
        values[*i] = value;
    }
    for (j, row) in rows.iter().enumerate() {
        row_duals[*row] = inverse.iter().zip(&costs).map(|(r, c)| &r[j] * c).sum();
    }
    Ok(ExactSolution {
        values,
        row_duals,
        basis_dimension: columns.len(),
    })
}

#[cfg(test)]
fn matrix(model: &Model, columns: &[usize], rows: &[usize]) -> Result<Vec<Vec<Rational>>, String> {
    if columns.len() != rows.len() {
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
#[cfg(test)]
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
#[cfg(test)]
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

#[cfg(test)]
mod tests {
    use super::super::{Column, Row};
    use super::*;
    #[test]
    fn large_rational_updates_rebuild_early_without_rejecting_the_basis() {
        let large = 2.0f64.powi(500);
        let model = Model {
            columns: vec![Column {
                lower: 0.0,
                upper: f64::INFINITY,
                cost: 1.0,
            }],
            rows: vec![
                Row {
                    equality: false,
                    rhs: 1.0,
                    terms: vec![(0, 1.0)],
                },
                Row {
                    equality: false,
                    rhs: large,
                    terms: vec![(0, large)],
                },
            ],
            offset: 0.0,
        };
        let mut basis = Basis {
            columns: vec![BasisStatus::Lower],
            rows: vec![BasisStatus::Basic; 2],
        };
        let mut system = System::new(&model, &basis).unwrap();
        basis.columns[0] = BasisStatus::Basic;
        basis.rows[0] = BasisStatus::Upper;
        system.update(&model, &basis, 0, 1).unwrap();
        assert!(
            system.etas.is_empty(),
            "expensive arithmetic triggers maintenance, not a model limit"
        );
        let expected = dense_reconstruct(&model, &basis).unwrap();
        let actual = system.reconstruct(&model).unwrap();
        assert_eq!(actual.values, expected.values);
        assert_eq!(actual.row_duals, expected.row_duals);
    }
    #[test]
    fn invalid_nonbasic_statuses_are_rejected_and_update_is_atomic() {
        let mut model = Model {
            columns: vec![Column {
                lower: 0.0,
                upper: f64::INFINITY,
                cost: 1.0,
            }],
            rows: vec![Row {
                equality: false,
                rhs: 2.0,
                terms: vec![(0, 1.0)],
            }],
            offset: 0.0,
        };
        let slack_basis = Basis {
            columns: vec![BasisStatus::Lower],
            rows: vec![BasisStatus::Basic],
        };
        let mut invalid = slack_basis.clone();
        invalid.columns[0] = BasisStatus::Upper;
        assert!(System::new(&model, &invalid).is_err());
        model.columns[0].upper = 4.0;
        assert!(System::new(&model, &invalid).is_err());
        invalid.columns[0] = BasisStatus::Zero;
        model.columns[0].lower = 1.0;
        assert!(System::new(&model, &invalid).is_err());
        invalid.columns[0] = BasisStatus::Lower;
        assert!(System::new(&model, &invalid).is_err());
        model.columns[0].lower = 0.0;
        let basic = Basis {
            columns: vec![BasisStatus::Basic],
            rows: vec![BasisStatus::Upper],
        };
        for status in [BasisStatus::Lower, BasisStatus::Zero] {
            let bad_row = Basis {
                columns: basic.columns.clone(),
                rows: vec![status],
            };
            assert!(System::new(&model, &bad_row).is_err());
        }
        model.rows[0].equality = true;
        let bad_equality = Basis {
            columns: basic.columns.clone(),
            rows: vec![BasisStatus::Zero],
        };
        assert!(System::new(&model, &bad_equality).is_err());
        model.rows[0].equality = false;
        let mut system = System::new(&model, &basic).unwrap();
        let before = system.reconstruct(&model).unwrap();
        let bad_update = Basis {
            columns: vec![BasisStatus::Upper],
            rows: vec![BasisStatus::Basic],
        };
        assert!(system.update(&model, &bad_update, 1, 0).is_err());
        assert_eq!(system.ids, vec![0]);
        assert!(system.etas.is_empty());
        let after = system.reconstruct(&model).unwrap();
        assert_eq!(before.values, after.values);
        assert_eq!(before.row_duals, after.row_duals);
        system.update(&model, &slack_basis, 1, 0).unwrap();
    }
    #[test]
    fn eta_updates_cross_refactor_period_and_reject_extra_exchange_atomically() {
        let model = Model {
            columns: vec![Column {
                lower: 0.0,
                upper: f64::INFINITY,
                cost: 3.0,
            }],
            rows: vec![
                Row {
                    equality: false,
                    rhs: 4.0,
                    terms: vec![(0, 2.0)],
                },
                Row {
                    equality: false,
                    rhs: 7.0,
                    terms: vec![(0, 5.0)],
                },
            ],
            offset: 0.0,
        };
        // The basic slack has a nonempty structural row, so transpose solves
        // must include its coupling to the structural basic variable.
        let mut basis = Basis {
            columns: vec![BasisStatus::Basic],
            rows: vec![BasisStatus::Upper, BasisStatus::Basic],
        };
        let mut system = System::new(&model, &basis).unwrap();
        let mut refactors = 0;
        for iteration in 0..140 {
            let (entering, leaving) = if iteration % 2 == 0 { (1, 0) } else { (0, 1) };
            basis.columns[0] = if entering == 0 {
                BasisStatus::Basic
            } else {
                BasisStatus::Lower
            };
            basis.rows[0] = if entering == 1 {
                BasisStatus::Basic
            } else {
                BasisStatus::Upper
            };
            system.update(&model, &basis, entering, leaving).unwrap();
            assert!(system.etas.len() < 64);
            if system.etas.is_empty() {
                refactors += 1;
            }
            let expected = dense_reconstruct(&model, &basis).unwrap();
            let actual = system.reconstruct(&model).unwrap();
            assert_eq!(actual.values, expected.values);
            assert_eq!(actual.row_duals, expected.row_duals);
            for (variable, status) in basis.columns.iter().chain(&basis.rows).enumerate() {
                if *status == BasisStatus::Basic {
                    assert_eq!(
                        system.cached_functional(&model, variable).unwrap(),
                        dense_functional(&model, &basis, variable).unwrap()
                    );
                } else {
                    assert_eq!(
                        system.direction(&model, variable).unwrap(),
                        System::uncached_direction(&model, &basis, variable)
                            .unwrap()
                            .1
                    );
                }
            }
        }
        assert!(refactors >= 2);
        let previous = system.reconstruct(&model).unwrap();
        let previous_ids = system.ids.clone();
        let previous_etas = system.etas.len();
        // Square, but exchanges the two slacks instead of the requested column.
        let wrong = Basis {
            columns: vec![BasisStatus::Basic],
            rows: vec![BasisStatus::Basic, BasisStatus::Upper],
        };
        assert!(system.update(&model, &wrong, 1, 0).is_err());
        assert_eq!(system.ids, previous_ids);
        assert_eq!(system.etas.len(), previous_etas);
        let preserved = system.reconstruct(&model).unwrap();
        assert_eq!(preserved.values, previous.values);
        assert_eq!(preserved.row_duals, previous.row_duals);
    }
    #[test]
    fn cancelled_update_preserves_factor_and_indices() {
        let model = Model {
            columns: [1.0, 2.0]
                .into_iter()
                .map(|cost| Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost,
                })
                .collect(),
            rows: vec![
                Row {
                    equality: false,
                    rhs: 3.0,
                    terms: vec![(0, 2.0), (1, 1.0)],
                },
                Row {
                    equality: false,
                    rhs: 7.0,
                    terms: vec![(0, 1.0), (1, 3.0)],
                },
            ],
            offset: 0.0,
        };
        let basis = Basis {
            columns: vec![BasisStatus::Basic, BasisStatus::Lower],
            rows: vec![BasisStatus::Upper, BasisStatus::Basic],
        };
        let next_basis = Basis {
            columns: vec![BasisStatus::Basic, BasisStatus::Basic],
            rows: vec![BasisStatus::Upper, BasisStatus::Upper],
        };
        let mut system = System::new(&model, &basis).unwrap();
        let previous = dense_reconstruct(&model, &basis).unwrap();
        let control = SolveControl::with_deadline(std::time::Instant::now());
        assert!(
            system
                .update_with_control(&model, &next_basis, 1, 3, &control)
                .unwrap_err()
                .contains("cancelled")
        );
        assert_eq!(system.columns, vec![0]);
        assert_eq!(system.rows, vec![0]);
        let preserved = system.reconstruct(&model).unwrap();
        assert_eq!(preserved.values, previous.values);
        assert_eq!(preserved.row_duals, previous.row_duals);
        assert_eq!(
            system.direction(&model, 1).unwrap(),
            System::uncached_direction(&model, &basis, 1).unwrap().1
        );
        system.update(&model, &next_basis, 1, 3).unwrap();
        let resumed = system.reconstruct(&model).unwrap();
        let expected = dense_reconstruct(&model, &next_basis).unwrap();
        assert_eq!(resumed.values, expected.values);
        assert_eq!(resumed.row_duals, expected.row_duals);
    }
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
            let independent = dense_reconstruct(&model, &basis).unwrap();
            let (_, factored) = model.reconstruct_factored(&basis).unwrap();
            assert_eq!(updated.values, independent.values);
            assert_eq!(updated.row_duals, independent.row_duals);
            for (variable, status) in basis.columns.iter().chain(&basis.rows).enumerate() {
                if *status == BasisStatus::Basic {
                    assert_eq!(
                        System::functional(&model, &basis, variable).unwrap(),
                        dense_functional(&model, &basis, variable).unwrap()
                    );
                    assert_eq!(
                        system.cached_functional(&model, variable).unwrap(),
                        dense_functional(&model, &basis, variable).unwrap()
                    );
                    assert_eq!(
                        factored.functional(&model, variable).unwrap(),
                        dense_functional(&model, &basis, variable).unwrap()
                    );
                } else {
                    let (columns, direction) =
                        System::uncached_direction(&model, &basis, variable).unwrap();
                    assert_eq!(system.direction(&model, variable).unwrap(), direction);
                    assert_eq!(factored.columns, columns);
                    assert_eq!(factored.direction(&model, variable).unwrap(), direction);
                }
            }
        }
    }
}
