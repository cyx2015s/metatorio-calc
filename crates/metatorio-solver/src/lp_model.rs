//! Backend-independent continuous LP expressions used by production planning.
use crate::concept::AIndexMap;
use std::ops::{Add, AddAssign, Mul, Sub, SubAssign};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Variable(pub(crate) usize);

#[derive(Debug, Clone)]
pub struct VariableDefinition {
    lower: f64,
    upper: f64,
    name: String,
}
pub fn variable() -> VariableDefinition {
    VariableDefinition {
        lower: f64::NEG_INFINITY,
        upper: f64::INFINITY,
        name: String::new(),
    }
}
impl VariableDefinition {
    pub fn min(mut self, value: impl Into<f64>) -> Self {
        self.lower = value.into();
        self
    }
    pub fn max(mut self, value: impl Into<f64>) -> Self {
        self.upper = value.into();
        self
    }
    pub fn name(mut self, value: impl Into<String>) -> Self {
        self.name = value.into();
        self
    }
    pub fn get_min(&self) -> f64 {
        self.lower
    }
    pub fn get_max(&self) -> f64 {
        self.upper
    }
    pub fn get_name(&self) -> &str {
        &self.name
    }
    pub fn is_integer(&self) -> bool {
        false
    }
}

#[derive(Debug, Default)]
pub struct ProblemVariables {
    definitions: Vec<VariableDefinition>,
}
impl ProblemVariables {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add(&mut self, definition: VariableDefinition) -> Variable {
        let variable = Variable(self.definitions.len());
        self.definitions.push(definition);
        variable
    }
    pub fn iter_variables_with_def(&self) -> impl Iterator<Item = (Variable, &VariableDefinition)> {
        self.definitions
            .iter()
            .enumerate()
            .map(|(i, d)| (Variable(i), d))
    }
}

#[derive(Debug, Clone, Default)]
pub struct Expression {
    terms: AIndexMap<Variable, f64>,
    constant: f64,
}
impl Expression {
    pub fn constant(&self) -> f64 {
        self.constant
    }
    pub fn linear_coefficients(&self) -> impl Iterator<Item = (Variable, f64)> + '_ {
        self.terms.iter().map(|(v, a)| (*v, *a))
    }
    pub fn eq(self, rhs: f64) -> Constraint {
        Constraint {
            expression: self - rhs,
            equality: true,
        }
    }
    pub fn leq(self, rhs: f64) -> Constraint {
        Constraint {
            expression: self - rhs,
            equality: false,
        }
    }
    pub fn geq(self, rhs: f64) -> Constraint {
        (-1.0 * self).leq(-rhs)
    }
}
impl From<f64> for Expression {
    fn from(constant: f64) -> Self {
        Self {
            constant,
            ..Self::default()
        }
    }
}
impl From<Variable> for Expression {
    fn from(variable: Variable) -> Self {
        1.0 * variable
    }
}
pub trait IntoAffineExpression {
    fn into_expression(self) -> Expression;
}
impl IntoAffineExpression for Variable {
    fn into_expression(self) -> Expression {
        self.into()
    }
}
impl IntoAffineExpression for &Variable {
    fn into_expression(self) -> Expression {
        (*self).into()
    }
}
impl<T: Into<Expression>> AddAssign<T> for Expression {
    fn add_assign(&mut self, rhs: T) {
        let rhs = rhs.into();
        self.constant += rhs.constant;
        for (v, a) in rhs.terms {
            *self.terms.entry(v).or_insert(0.0) += a;
        }
    }
}
impl<T: Into<Expression>> SubAssign<T> for Expression {
    fn sub_assign(&mut self, rhs: T) {
        *self += -1.0 * rhs.into();
    }
}
impl<T: Into<Expression>> Add<T> for Expression {
    type Output = Expression;
    fn add(mut self, rhs: T) -> Self::Output {
        self += rhs;
        self
    }
}
impl<T: Into<Expression>> Sub<T> for Expression {
    type Output = Expression;
    fn sub(mut self, rhs: T) -> Self::Output {
        self -= rhs;
        self
    }
}
impl Add<Variable> for Variable {
    type Output = Expression;
    fn add(self, rhs: Variable) -> Expression {
        Expression::from(self) + rhs
    }
}
impl Sub<Variable> for Variable {
    type Output = Expression;
    fn sub(self, rhs: Variable) -> Expression {
        Expression::from(self) - rhs
    }
}
impl Mul<Variable> for f64 {
    type Output = Expression;
    fn mul(self, variable: Variable) -> Expression {
        Expression {
            terms: [(variable, self)].into_iter().collect(),
            constant: 0.0,
        }
    }
}
impl Mul<Expression> for f64 {
    type Output = Expression;
    fn mul(self, mut expression: Expression) -> Expression {
        for a in expression.terms.values_mut() {
            *a *= self;
        }
        expression.constant *= self;
        expression
    }
}

#[derive(Debug, Clone)]
pub struct Constraint {
    expression: Expression,
    equality: bool,
}
impl Constraint {
    pub fn is_equality(&self) -> bool {
        self.equality
    }
    pub fn expression(&self) -> &Expression {
        &self.expression
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionError {
    Unbounded,
    Infeasible,
    /// The selected search backend cannot handle this model representation.
    /// This is neither an infeasibility proof nor a retryable numerical failure.
    UnsupportedModel(String),
    Other(&'static str),
    Str(String),
}
impl std::fmt::Display for ResolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unbounded => f.write_str("unbounded LP"),
            Self::Infeasible => f.write_str("infeasible LP"),
            Self::UnsupportedModel(s) => write!(f, "unsupported LP model: {s}"),
            Self::Other(s) => f.write_str(s),
            Self::Str(s) => f.write_str(s),
        }
    }
}
impl std::error::Error for ResolutionError {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn balances_combine_repeated_variables_without_discarding_tiny_flows() {
        let mut vars = ProblemVariables::new();
        let x = vars.add(variable().min(0));
        let y = vars.add(variable().min(0));
        let mut balance = 3.0 * x;
        balance += 1e-30 * y;
        balance -= 2.0 * x;
        let constraint = balance.geq(2.0);
        assert!(!constraint.is_equality());
        assert_eq!(constraint.expression().constant(), 2.0);
        let terms: Vec<_> = constraint.expression().linear_coefficients().collect();
        assert_eq!(terms, vec![(x, -1.0), (y, -1e-30)]);
    }
}
