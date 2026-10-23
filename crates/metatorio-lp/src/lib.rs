//! Pure Rust exact reconstruction and verification for continuous binary-f64 LPs.
//! Every input coefficient is interpreted as its exact binary rational value.
//! A basis is a hint, not a proof; verification always checks the original LP.
use num_bigint::BigInt;
use num_integer::Integer;
pub use num_rational::BigRational as Rational;
use num_traits::{Signed, ToPrimitive, Zero};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
mod basis;
mod certified;
mod columns;
mod control;
mod cost_bounds;
mod dual;
// Retained as independent dense references for exact-factor regression tests.
#[cfg(test)]
mod factor;
#[cfg(test)]
mod factor_blocks;
mod sparse_exact_factor;
pub use control::SolveControl;
pub use cost_bounds::CostBounds;
mod float;
mod presolve;
mod relative;
mod relevance;
mod restricted;
mod sparse_float_basis;
pub use relative::{RelativeCandidate, RelativeVerification};
pub use relevance::RelevanceReduction;
mod refine;
pub use certified::CertifiedResult;
pub use dual::{DualPivotRule, DualRepair};
pub use float::{FloatCandidate, FloatOptions, FloatProgress};
pub use refine::Refinement;

#[derive(Clone, Debug)]
pub struct Column {
    pub lower: f64,
    pub upper: f64,
    pub cost: f64,
}
#[derive(Clone, Debug)]
pub struct Row {
    pub equality: bool,
    pub rhs: f64,
    pub terms: Vec<(usize, f64)>,
}
#[derive(Clone, Debug)]
pub struct Model {
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
    pub offset: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BasisStatus {
    Lower,
    Basic,
    Upper,
    Zero,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Basis {
    pub columns: Vec<BasisStatus>,
    pub rows: Vec<BasisStatus>,
}
#[derive(Clone, Debug)]
pub struct ExactSolution {
    pub values: Vec<Rational>,
    pub row_duals: Vec<Rational>,
    pub basis_dimension: usize,
}
#[derive(Debug)]
pub struct Optimization {
    pub solution: ExactSolution,
    pub basis: Basis,
    pub pivots: usize,
    pub unbounded_ray: Option<Vec<Rational>>,
    pub bland_fallback: bool,
    pub stop_reason: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct PivotProgress {
    pub pivots: usize,
    pub basis_dimension: usize,
}
#[derive(Debug, Serialize)]
pub struct InfeasibilityCertificate {
    pub margin: String,
    pub row_duals: Vec<(usize, String)>,
}
#[derive(Debug)]
pub struct TwoPhaseResult {
    pub phase1: Optimization,
    pub phase2: Option<Optimization>,
    pub infeasibility: Option<InfeasibilityCertificate>,
}
#[derive(Debug, Serialize)]
pub struct Certificate {
    pub primal_feasible: bool,
    pub dual_feasible: bool,
    pub optimal: bool,
    pub violated_rows: usize,
    pub violated_bounds: usize,
    pub violated_dual_signs: usize,
    pub objective: String,
    pub dual_objective: Option<String>,
    pub objective_gap: Option<String>,
    pub first_violated_rows: Vec<(usize, String)>,
    pub first_improving_columns: Vec<(usize, String)>,
}

fn rational(value: f64) -> Result<Rational, String> {
    Rational::from_float(value).ok_or_else(|| "non-finite rational input".into())
}

struct Pricing {
    numerators: Vec<BigInt>,
    denominator: BigInt,
}
fn denominator_power(value: f64) -> usize {
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 2047) as i32;
    let mantissa = (bits & ((1u64 << 52) - 1)) | if exponent == 0 { 0 } else { 1u64 << 52 };
    if mantissa == 0 {
        return 0;
    }
    let exponent = if exponent == 0 {
        -1074
    } else {
        exponent - 1075
    };
    (-(exponent + mantissa.trailing_zeros() as i32)).max(0) as usize
}
fn binary_integer(value: f64, power: usize) -> BigInt {
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 2047) as i32;
    let mantissa = (bits & ((1u64 << 52) - 1)) | if exponent == 0 { 0 } else { 1u64 << 52 };
    if mantissa == 0 {
        return BigInt::zero();
    }
    let trailing = mantissa.trailing_zeros();
    let exponent = if exponent == 0 {
        -1074
    } else {
        exponent - 1075
    };
    let shift = exponent + trailing as i32 + power as i32;
    assert!(
        shift >= 0,
        "binary common denominator must cover every input"
    );
    let integer = BigInt::from(mantissa >> trailing) << shift as usize;
    if bits >> 63 == 1 { -integer } else { integer }
}

/// Independent dense Gaussian elimination used only as a small-test oracle.
/// Zero pivots are rejected exactly, without a numerical tolerance.
#[cfg(test)]
fn solve(mut matrix: Vec<Vec<Rational>>, mut rhs: Vec<Rational>) -> Result<Vec<Rational>, String> {
    let n = rhs.len();
    if matrix.len() != n || matrix.iter().any(|row| row.len() != n) {
        return Err("dense test oracle requires a square matrix".into());
    }
    for k in 0..n {
        let pivot = (k..n)
            .filter(|i| !matrix[*i][k].is_zero())
            .min_by_key(|i| matrix[*i][k..].iter().filter(|a| !a.is_zero()).count())
            .ok_or("exactly singular basis")?;
        matrix.swap(k, pivot);
        rhs.swap(k, pivot);
        for i in k + 1..n {
            if matrix[i][k].is_zero() {
                continue;
            }
            let factor = &matrix[i][k] / &matrix[k][k];
            matrix[i][k] = Rational::zero();
            for j in k + 1..n {
                if matrix[k][j].is_zero() {
                    continue;
                }
                let product = &factor * &matrix[k][j];
                matrix[i][j] -= product;
            }
            if !rhs[k].is_zero() {
                let product = &factor * &rhs[k];
                rhs[i] -= product;
            }
        }
    }
    let mut result = vec![Rational::zero(); n];
    for i in (0..n).rev() {
        let mut value = rhs[i].clone();
        for (j, x) in result.iter().enumerate().skip(i + 1) {
            if !matrix[i][j].is_zero() && !x.is_zero() {
                value -= &matrix[i][j] * x;
            }
        }
        result[i] = value / &matrix[i][i];
    }
    Ok(result)
}

impl Model {
    fn reduced_costs(&self, solution: &ExactSolution) -> Pricing {
        self.pricing(solution, false)
    }
    fn pricing(&self, solution: &ExactSolution, zero_cost: bool) -> Pricing {
        // All binary inputs have power-of-two denominators. Share one exact
        // denominator across prices, avoiding a costly GCD for every term.
        let power = self
            .columns
            .iter()
            .map(|c| denominator_power(c.cost))
            .chain(
                self.rows
                    .iter()
                    .zip(&solution.row_duals)
                    .filter(|(_, y)| !y.is_zero())
                    .flat_map(|(r, _)| r.terms.iter().map(|(_, a)| denominator_power(*a))),
            )
            .max()
            .unwrap_or(0);
        let mut denominator = BigInt::from(1);
        for y in &solution.row_duals {
            if !y.is_zero() {
                denominator = denominator.lcm(y.denom());
            }
        }
        let mut reduced: Vec<_> = self
            .columns
            .iter()
            .map(|c| {
                if zero_cost {
                    BigInt::zero()
                } else {
                    binary_integer(c.cost, power) * &denominator
                }
            })
            .collect();
        for (row, y) in self.rows.iter().zip(&solution.row_duals) {
            if y.is_zero() {
                continue;
            }
            let integer_y = y.numer() * (&denominator / y.denom());
            for (i, a) in &row.terms {
                reduced[*i] -= binary_integer(*a, power) * &integer_y;
            }
        }
        Pricing {
            numerators: reduced,
            denominator: denominator << power,
        }
    }

    /// An exact Farkas alternative for x>=0, Ax<=b and equality rows:
    /// y<=0 on inequalities, A^T y<=0, and b^T y>0 contradict feasibility.
    /// Original objective coefficients do not participate in this proof.
    pub fn verify_infeasibility(
        &self,
        row_duals: &[Rational],
    ) -> Result<Option<InfeasibilityCertificate>, String> {
        self.check()?;
        if row_duals.len() != self.rows.len()
            || self
                .columns
                .iter()
                .any(|c| c.lower != 0.0 || c.upper != f64::INFINITY)
        {
            return Err(
                "Farkas verifier requires matching rows and nonnegative unbounded-above columns"
                    .into(),
            );
        }
        let dual = ExactSolution {
            values: vec![Rational::zero(); self.columns.len()],
            row_duals: row_duals.to_vec(),
            basis_dimension: 0,
        };
        let prices = self.pricing(&dual, true);
        if prices.numerators.iter().any(|z| z.is_negative()) {
            return Ok(None);
        }
        let mut margin = Rational::zero();
        for (row, y) in self.rows.iter().zip(row_duals) {
            if !row.equality && y.is_positive() {
                return Ok(None);
            }
            margin += rational(row.rhs)? * y;
        }
        if !margin.is_positive() {
            return Ok(None);
        }
        Ok(Some(InfeasibilityCertificate {
            margin: margin.to_string(),
            row_duals: row_duals
                .iter()
                .enumerate()
                .filter(|(_, y)| !y.is_zero())
                .map(|(i, y)| (i, y.to_string()))
                .collect(),
        }))
    }

    pub fn solve(&self, max_pivots: usize) -> Result<TwoPhaseResult, String> {
        self.solve_with_observer(max_pivots, |_, _| true)
    }
    /// Exact two-phase simplex from zero, with no floating solver or basis hint.
    /// The observer receives true during Phase I; limits leave status undetermined.
    pub fn solve_with_observer(
        &self,
        max_pivots: usize,
        mut observer: impl FnMut(bool, PivotProgress) -> bool,
    ) -> Result<TwoPhaseResult, String> {
        self.check()?;
        if self
            .columns
            .iter()
            .any(|c| c.lower != 0.0 || c.upper != f64::INFINITY)
        {
            return Err(
                "two-phase prototype requires nonnegative columns without upper bounds".into(),
            );
        }
        let count = self.columns.len();
        let mut auxiliary = self.clone();
        auxiliary.offset = 0.0;
        for column in &mut auxiliary.columns {
            column.cost = 0.0;
        }
        let mut basis = Basis {
            columns: vec![BasisStatus::Lower; count],
            rows: vec![BasisStatus::Basic; self.rows.len()],
        };
        let mut artificial = Vec::new();
        for (i, row) in auxiliary.rows.iter_mut().enumerate() {
            if (row.equality && row.rhs != 0.0) || row.rhs < 0.0 {
                let column = auxiliary.columns.len();
                auxiliary.columns.push(Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost: 1.0,
                });
                row.terms
                    .push((column, if row.rhs < 0.0 { -1.0 } else { 1.0 }));
                basis.columns.push(BasisStatus::Basic);
                basis.rows[i] = BasisStatus::Upper;
                artificial.push((column, i));
            }
        }
        let phase1 = auxiliary
            .optimize_from_basis_with_observer(basis, max_pivots, |p| observer(true, p))?;
        if phase1.stop_reason.is_some() {
            return Ok(TwoPhaseResult {
                phase1,
                phase2: None,
                infeasibility: None,
            });
        }
        if phase1.unbounded_ray.is_some() {
            return Err("nonnegative artificial objective cannot be unbounded below".into());
        }
        let objective: Rational = artificial
            .iter()
            .map(|(i, _)| phase1.solution.values[*i].clone())
            .sum();
        if objective.is_positive() {
            let proof = self
                .verify_infeasibility(&phase1.solution.row_duals)?
                .ok_or("auxiliary optimum has no valid original Farkas certificate")?;
            return Ok(TwoPhaseResult {
                phase1,
                phase2: None,
                infeasibility: Some(proof),
            });
        }
        if !objective.is_zero() {
            return Err("invalid negative artificial objective".into());
        }
        let mut basis = phase1.basis.clone();
        // Each artificial column has exactly one signed unit entry. If basic,
        // deleting it and its corresponding active row leaves a nonsingular
        // cofactor. Promote that row slack (fixed at zero for equality rows).
        for (column, row) in artificial {
            if basis.columns[column] == BasisStatus::Basic {
                if basis.rows[row] == BasisStatus::Basic {
                    return Err("basic artificial column has no active original row".into());
                }
                basis.rows[row] = BasisStatus::Basic;
            }
        }
        basis.columns.truncate(count);
        let remaining = max_pivots.saturating_sub(phase1.pivots);
        let phase2 =
            self.optimize_from_basis_with_observer(basis, remaining, |p| observer(false, p))?;
        Ok(TwoPhaseResult {
            phase1,
            phase2: Some(phase2),
            infeasibility: None,
        })
    }

    /// Exact primal simplex for the application's nonnegative, unbounded-above
    /// continuous columns. Requires a feasible basis hint; phase I comes later.
    /// Dantzig pricing starts fast; a repeated basis activates Bland pricing.
    pub fn optimize_from_basis(
        &self,
        basis: Basis,
        max_pivots: usize,
    ) -> Result<Optimization, String> {
        self.optimize_from_basis_with_observer(basis, max_pivots, |_| true)
    }
    /// Returning false preserves the current feasible basis with a nonterminal
    /// stop reason. A limit never proves infeasibility or discards the candidate.
    pub fn optimize_from_basis_with_observer(
        &self,
        basis: Basis,
        max_pivots: usize,
        observer: impl FnMut(PivotProgress) -> bool,
    ) -> Result<Optimization, String> {
        self.optimize_inner(basis, max_pivots, observer, true, &SolveControl::default())
    }
    /// Compatibility entry point using the same sparse incremental basis as
    /// optimize_from_basis; neither entry point constructs an exact inverse.
    pub fn optimize_without_inverse_with_observer(
        &self,
        basis: Basis,
        max_pivots: usize,
        observer: impl FnMut(PivotProgress) -> bool,
    ) -> Result<Optimization, String> {
        self.optimize_inner(basis, max_pivots, observer, true, &SolveControl::default())
    }
    pub fn optimize_without_inverse_with_control(
        &self,
        basis: Basis,
        max_pivots: usize,
        observer: impl FnMut(PivotProgress) -> bool,
        control: &SolveControl,
    ) -> Result<Optimization, String> {
        self.optimize_inner(basis, max_pivots, observer, true, control)
    }
    fn optimize_inner(
        &self,
        mut basis: Basis,
        max_pivots: usize,
        mut observer: impl FnMut(PivotProgress) -> bool,
        cache: bool,
        control: &SolveControl,
    ) -> Result<Optimization, String> {
        control.check()?;
        self.check()?;
        if self
            .columns
            .iter()
            .any(|c| c.lower != 0.0 || c.upper != f64::INFINITY)
        {
            return Err(
                "exact simplex prototype requires nonnegative columns without upper bounds".into(),
            );
        }
        let mut system = if cache && self.prefers_incremental_basis(&basis) {
            Some(basis::System::new_with_control(self, &basis, control)?)
        } else {
            None
        };
        let mut seen = HashSet::new();
        let mut bland = false;
        for iteration in 0..=max_pivots {
            control.check()?;
            if !bland
                && !seen.insert(
                    basis
                        .columns
                        .iter()
                        .chain(&basis.rows)
                        .map(|s| *s as u8)
                        .collect::<Vec<_>>(),
                )
            {
                bland = true;
                seen.clear();
            }
            let (solution, factored) = match &system {
                Some(system) => (system.reconstruct_with_control(self, control)?, None),
                None => {
                    let (solution, factor) =
                        self.reconstruct_factored_with_control(&basis, control)?;
                    (solution, Some(factor))
                }
            };
            let certificate = self.certify(&solution)?;
            if !certificate.primal_feasible {
                return Err(
                    "basis is not exactly primal feasible; phase I or dual repair required".into(),
                );
            }
            if !observer(PivotProgress {
                pivots: iteration,
                basis_dimension: solution.basis_dimension,
            }) {
                return Ok(Optimization {
                    solution,
                    basis,
                    pivots: iteration,
                    unbounded_ray: None,
                    bland_fallback: bland,
                    stop_reason: Some("observer requested a nonterminal stop".into()),
                });
            }
            let reduced = self.reduced_costs(&solution);
            let mut choices: Vec<_> = reduced
                .numerators
                .iter()
                .enumerate()
                .filter(|(i, z)| basis.columns[*i] != BasisStatus::Basic && z.is_negative())
                .map(|(i, z)| (i, z.clone()))
                .collect();
            for (i, (row, y)) in self.rows.iter().zip(&solution.row_duals).enumerate() {
                if !row.equality && basis.rows[i] != BasisStatus::Basic && y.is_positive() {
                    choices.push((
                        self.columns.len() + i,
                        -y.numer() * (&reduced.denominator / y.denom()),
                    ));
                }
            }
            let entering = if bland {
                choices.iter().min_by_key(|(i, _)| *i)
            } else {
                choices
                    .iter()
                    .min_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
            }
            .map(|(i, _)| *i);
            let Some(entering) = entering else {
                if !certificate.optimal {
                    return Err("no entering variable but exact optimality check failed".into());
                }
                return Ok(Optimization {
                    solution,
                    basis,
                    pivots: iteration,
                    unbounded_ray: None,
                    bland_fallback: bland,
                    stop_reason: None,
                });
            };
            if iteration == max_pivots {
                return Ok(Optimization {
                    solution,
                    basis,
                    pivots: iteration,
                    unbounded_ray: None,
                    bland_fallback: bland,
                    stop_reason: Some("exact simplex pivot limit reached".into()),
                });
            }
            let (columns, basic_direction) = match &system {
                Some(system) => (
                    system.columns.clone(),
                    system.direction_with_control(self, entering, control)?,
                ),
                None => {
                    let factored = factored.as_ref().ok_or("missing exact factorization")?;
                    (
                        factored.columns.clone(),
                        factored.direction_with_control(self, entering, control)?,
                    )
                }
            };
            let mut direction = vec![Rational::zero(); self.columns.len()];
            for (i, d) in columns.iter().zip(&basic_direction) {
                direction[*i] = d.clone();
            }
            if entering < self.columns.len() {
                direction[entering] = rational(1.0)?;
            }
            let mut leaving: Option<(Rational, usize)> = None;
            let mut consider = |ratio: Rational, index: usize| {
                if leaving
                    .as_ref()
                    .is_none_or(|best| (&ratio, index) < (&best.0, best.1))
                {
                    leaving = Some((ratio, index));
                }
            };
            for (i, d) in columns.iter().zip(&basic_direction) {
                if d.is_negative() {
                    consider(&solution.values[*i] / -d, *i);
                }
            }
            for (i, row) in self.rows.iter().enumerate() {
                if basis.rows[i] != BasisStatus::Basic {
                    continue;
                }
                let mut activity = Rational::zero();
                let mut change = Rational::zero();
                for (column, a) in &row.terms {
                    if !solution.values[*column].is_zero() {
                        activity += rational(*a)? * &solution.values[*column];
                    }
                    if !direction[*column].is_zero() {
                        change -= rational(*a)? * &direction[*column];
                    }
                }
                if row.equality && !change.is_zero() {
                    consider(Rational::zero(), self.columns.len() + i);
                } else if change.is_negative() {
                    consider(
                        (rational(row.rhs)? - activity) / -change,
                        self.columns.len() + i,
                    );
                }
            }
            let Some((_, leaving)) = leaving else {
                // Verify the improving recession direction independently before
                // returning unbounded, including equalities and nonnegativity.
                if direction.iter().any(|d| d.is_negative()) {
                    return Err("invalid nonnegative recession direction".into());
                }
                let mut cost = Rational::zero();
                for (c, d) in self.columns.iter().zip(&direction) {
                    if !d.is_zero() {
                        cost += rational(c.cost)? * d;
                    }
                }
                for row in &self.rows {
                    let mut change = Rational::zero();
                    for (i, a) in &row.terms {
                        if !direction[*i].is_zero() {
                            change += rational(*a)? * &direction[*i];
                        }
                    }
                    if (row.equality && !change.is_zero())
                        || (!row.equality && change.is_positive())
                    {
                        return Err("invalid original-row recession direction".into());
                    }
                }
                if !cost.is_negative() {
                    return Err("recession direction does not improve objective".into());
                }
                return Ok(Optimization {
                    solution,
                    basis,
                    pivots: iteration,
                    unbounded_ray: Some(direction),
                    bland_fallback: bland,
                    stop_reason: None,
                });
            };
            if leaving < self.columns.len() {
                basis.columns[leaving] = BasisStatus::Lower;
            } else {
                basis.rows[leaving - self.columns.len()] = BasisStatus::Upper;
            }
            if entering < self.columns.len() {
                basis.columns[entering] = BasisStatus::Basic;
            } else {
                basis.rows[entering - self.columns.len()] = BasisStatus::Basic;
            }
            if let Some(system) = &mut system {
                system.update_with_control(self, &basis, entering, leaving, control)?;
            } else if cache && self.prefers_incremental_basis(&basis) {
                system = Some(basis::System::new_with_control(self, &basis, control)?);
            }
        }
        unreachable!()
    }
    // A slack-heavy full basis has linear overhead in every constraint, while
    // the compressed structural factor can be much smaller. This chooses the
    // representation only; it never rejects a basis or limits its dimension.
    fn prefers_incremental_basis(&self, basis: &Basis) -> bool {
        basis
            .columns
            .iter()
            .filter(|status| **status == BasisStatus::Basic)
            .count()
            >= self.rows.len().div_ceil(2)
    }
    pub fn check(&self) -> Result<(), String> {
        if !self.offset.is_finite() {
            return Err("non-finite objective offset".into());
        }
        for c in &self.columns {
            if c.lower.is_nan()
                || c.upper.is_nan()
                || c.lower > c.upper
                || c.lower == f64::INFINITY
                || c.upper == f64::NEG_INFINITY
                || !c.cost.is_finite()
            {
                return Err("invalid continuous column".into());
            }
        }
        for row in &self.rows {
            if !row.rhs.is_finite()
                || row
                    .terms
                    .iter()
                    .any(|(i, a)| *i >= self.columns.len() || !a.is_finite())
            {
                return Err("invalid original row".into());
            }
        }
        Ok(())
    }

    /// Eliminate basic row slacks and solve the remaining structural basis.
    /// Reconstruct nonbasic columns at their actual bounds, not rounded values.
    pub fn reconstruct(&self, basis: &Basis) -> Result<ExactSolution, String> {
        self.reconstruct_factored(basis)
            .map(|(solution, _)| solution)
    }
    fn reconstruct_factored(
        &self,
        basis: &Basis,
    ) -> Result<(ExactSolution, basis::Factored), String> {
        self.reconstruct_factored_with_control(basis, &SolveControl::default())
    }
    /// Interrupted reconstruction returns an error, never a partial solution.
    pub fn reconstruct_with_control(
        &self,
        basis: &Basis,
        control: &SolveControl,
    ) -> Result<ExactSolution, String> {
        self.reconstruct_factored_with_control(basis, control)
            .map(|(solution, _)| solution)
    }
    fn reconstruct_factored_with_control(
        &self,
        basis: &Basis,
        control: &SolveControl,
    ) -> Result<(ExactSolution, basis::Factored), String> {
        control.check()?;
        self.check()?;
        if basis.columns.len() != self.columns.len() || basis.rows.len() != self.rows.len() {
            return Err("basis dimensions differ from model".into());
        }
        let columns: Vec<_> = basis
            .columns
            .iter()
            .enumerate()
            .filter_map(|(i, s)| (*s == BasisStatus::Basic).then_some(i))
            .collect();
        let rows: Vec<_> = basis
            .rows
            .iter()
            .enumerate()
            .filter_map(|(i, s)| (*s != BasisStatus::Basic).then_some(i))
            .collect();
        if columns.len() != rows.len() {
            return Err("structural basis is not square".into());
        }
        let mut positions = vec![None; self.columns.len()];
        for (i, column) in columns.iter().enumerate() {
            positions[*column] = Some(i);
        }
        let mut values = vec![Rational::zero(); self.columns.len()];
        for (i, (column, status)) in self.columns.iter().zip(&basis.columns).enumerate() {
            if i % 1024 == 0 {
                control.check()?;
            }
            values[i] = match status {
                BasisStatus::Basic => Rational::zero(),
                BasisStatus::Lower => rational(column.lower)?,
                BasisStatus::Upper => rational(column.upper)?,
                BasisStatus::Zero if column.lower <= 0.0 && column.upper >= 0.0 => Rational::zero(),
                BasisStatus::Zero => return Err("nonbasic zero outside column bounds".into()),
            };
        }
        let mut matrix = vec![std::collections::BTreeMap::new(); rows.len()];
        let mut rhs = Vec::with_capacity(rows.len());
        for (i, original) in rows.iter().enumerate() {
            control.check()?;
            let row = &self.rows[*original];
            if !row.equality
                && basis.rows[*original] != BasisStatus::Upper
                && !(basis.rows[*original] == BasisStatus::Zero && row.rhs == 0.0)
            {
                return Err("inequality basis row must be at its finite upper bound".into());
            }
            if row.equality && basis.rows[*original] == BasisStatus::Zero && row.rhs != 0.0 {
                return Err("nonzero equality row cannot have zero activity".into());
            }
            let mut value = rational(row.rhs)?;
            for (term, (column, coefficient)) in row.terms.iter().enumerate() {
                if term % 1024 == 0 {
                    control.check()?;
                }
                if let Some(j) = positions[*column] {
                    let value = matrix[i].entry(j).or_insert_with(Rational::zero);
                    *value += rational(*coefficient)?;
                    if value.is_zero() {
                        matrix[i].remove(&j);
                    }
                } else if !values[*column].is_zero() {
                    value -= rational(*coefficient)? * &values[*column];
                }
            }
            rhs.push(value);
        }
        let factor = sparse_exact_factor::SparseExactFactor::new(matrix, control)?;
        let basic = factor.solve_with_control(&rhs, control)?;
        for (column, value) in columns.iter().zip(basic) {
            values[*column] = value;
        }
        let costs = columns
            .iter()
            .map(|i| rational(self.columns[*i].cost))
            .collect::<Result<Vec<_>, _>>()?;
        // The primal solve already checked nonsingularity. A zero objective
        // has exactly zero duals and needs no second factorization.
        let dual = if costs.iter().all(|c| c.is_zero()) {
            vec![Rational::zero(); columns.len()]
        } else {
            factor.solve_transpose_with_control(&costs, control)?
        };
        let mut row_duals = vec![Rational::zero(); self.rows.len()];
        for (row, value) in rows.iter().zip(dual) {
            row_duals[*row] = value;
        }
        let solution = ExactSolution {
            values,
            row_duals,
            basis_dimension: columns.len(),
        };
        Ok((
            solution,
            basis::Factored {
                columns,
                rows,
                factor,
            },
        ))
    }

    /// For nonnegative costs and nonnegative unbounded-above variables, shrink
    /// a dual witness toward zero to obtain an exact lower bound. This keeps
    /// the original costs unchanged. It does not prove primal feasibility or
    /// optimality; callers must check the returned solution's certificate/gap.
    pub fn scale_to_dual_feasible(
        &self,
        solution: &ExactSolution,
    ) -> Result<(ExactSolution, Rational), String> {
        self.scale_to_dual_feasible_certified(solution)
            .map(|(adjusted, scale, _)| (adjusted, scale))
    }

    /// Reuse the full certificate already required to validate dual scaling.
    /// This is the certificate of the returned adjusted solution, not the hint.
    pub(crate) fn scale_to_dual_feasible_certified(
        &self,
        solution: &ExactSolution,
    ) -> Result<(ExactSolution, Rational, Certificate), String> {
        self.check()?;
        if solution.values.len() != self.columns.len()
            || solution.row_duals.len() != self.rows.len()
            || self
                .columns
                .iter()
                .any(|c| c.lower != 0.0 || c.upper != f64::INFINITY || c.cost < 0.0)
        {
            return Err(
                "dual scaling requires matching dimensions and nonnegative columns/costs".into(),
            );
        }
        let mut scale = rational(1.0)?;
        if self
            .rows
            .iter()
            .zip(&solution.row_duals)
            .any(|(r, y)| !r.equality && y.is_positive())
        {
            scale = Rational::zero();
        } else {
            let reduced = self.reduced_costs(solution);
            for (column, price) in self.columns.iter().zip(reduced.numerators) {
                if price.is_negative() {
                    let cost = rational(column.cost)?;
                    let rc = Rational::new(price, reduced.denominator.clone());
                    scale = scale.min(&cost / (&cost - rc));
                }
            }
        }
        let adjusted = ExactSolution {
            values: solution.values.clone(),
            row_duals: solution.row_duals.iter().map(|y| y * &scale).collect(),
            basis_dimension: solution.basis_dimension,
        };
        let certificate = self.certify(&adjusted)?;
        if !certificate.dual_feasible {
            return Err("scaled dual failed independent verification".into());
        }
        Ok((adjusted, scale, certificate))
    }

    /// Exact primal feasibility and weak-duality bound against every original
    /// row and column, including columns omitted from the candidate support.
    pub fn certify(&self, solution: &ExactSolution) -> Result<Certificate, String> {
        self.check()?;
        if solution.values.len() != self.columns.len()
            || solution.row_duals.len() != self.rows.len()
        {
            return Err("certificate dimensions differ from original model".into());
        }
        let mut result = Certificate {
            primal_feasible: true,
            dual_feasible: true,
            optimal: false,
            violated_rows: 0,
            violated_bounds: 0,
            violated_dual_signs: 0,
            objective: String::new(),
            dual_objective: None,
            objective_gap: None,
            first_violated_rows: Vec::new(),
            first_improving_columns: Vec::new(),
        };
        let reduced = self.reduced_costs(solution);
        let mut dual_objective = rational(self.offset)?;
        for (i, (row, y)) in self.rows.iter().zip(&solution.row_duals).enumerate() {
            let mut activity = Rational::zero();
            for (column, a) in &row.terms {
                let x = &solution.values[*column];
                if !x.is_zero() {
                    activity += rational(*a)? * x;
                }
            }
            let rhs = rational(row.rhs)?;
            let violation = if row.equality {
                (&activity - &rhs).abs()
            } else {
                (&activity - &rhs).max(Rational::zero())
            };
            if !violation.is_zero() {
                result.violated_rows += 1;
                if result.first_violated_rows.len() < 12 {
                    result.first_violated_rows.push((i, violation.to_string()));
                }
            }
            if !row.equality && y.is_positive() {
                result.violated_dual_signs += 1;
            }
            dual_objective += rhs * y;
        }
        let mut objective = rational(self.offset)?;
        for (i, ((column, x), z)) in self
            .columns
            .iter()
            .zip(&solution.values)
            .zip(&reduced.numerators)
            .enumerate()
        {
            if (column.lower.is_finite() && x < &rational(column.lower)?)
                || (column.upper.is_finite() && x > &rational(column.upper)?)
            {
                result.violated_bounds += 1;
            }
            if column.cost != 0.0 && !x.is_zero() {
                objective += rational(column.cost)? * x;
            }
            if z.is_zero() {
                continue;
            }
            let bound = if z.is_positive() {
                column.lower
            } else {
                column.upper
            };
            if bound.is_finite() {
                if bound != 0.0 {
                    dual_objective +=
                        rational(bound)? * Rational::new(z.clone(), reduced.denominator.clone());
                }
            } else {
                result.violated_dual_signs += 1;
                if result.first_improving_columns.len() < 12 {
                    result.first_improving_columns.push((
                        i,
                        Rational::new(z.clone(), reduced.denominator.clone()).to_string(),
                    ));
                }
            }
        }
        result.primal_feasible = result.violated_rows == 0 && result.violated_bounds == 0;
        result.dual_feasible = result.violated_dual_signs == 0;
        result.optimal =
            result.primal_feasible && result.dual_feasible && objective == dual_objective;
        result.objective = objective.to_string();
        if result.dual_feasible {
            result.objective_gap = Some((&objective - &dual_objective).to_string());
            result.dual_objective = Some(dual_objective.to_string());
        }
        Ok(result)
    }
}
impl ExactSolution {
    pub fn approximate(&self) -> Result<Vec<f64>, String> {
        self.values
            .iter()
            .map(|x| {
                x.to_f64()
                    .filter(|x| x.is_finite())
                    .ok_or_else(|| "exact solution overflows f64 output".into())
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn exact_dual_scaling_provides_a_bound_without_changing_costs() {
        use super::*;
        let mut model = Model {
            columns: [8.0, 4.0]
                .into_iter()
                .map(|cost| Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost,
                })
                .collect(),
            rows: vec![Row {
                equality: true,
                rhs: 1.0,
                terms: vec![(0, 1.0), (1, 0.5)],
            }],
            offset: 9.0,
        };
        let solution = ExactSolution {
            values: vec![rational(1.0).unwrap(), Rational::zero()],
            row_duals: vec![rational(8.5).unwrap()],
            basis_dimension: 1,
        };
        assert!(!model.certify(&solution).unwrap().dual_feasible);
        let (adjusted, scale) = model.scale_to_dual_feasible(&solution).unwrap();
        assert_eq!(scale, Rational::new(16.into(), 17.into()));
        let certificate = model.certify(&adjusted).unwrap();
        assert!(certificate.optimal);
        assert_eq!(certificate.objective, "17");
        assert_eq!(model.columns[1].cost, 4.0);
        model.columns[1].cost = 0.0;
        let (adjusted, scale) = model.scale_to_dual_feasible(&solution).unwrap();
        assert!(scale.is_zero());
        let certificate = model.certify(&adjusted).unwrap();
        assert!(certificate.primal_feasible && certificate.dual_feasible);
        assert!(!certificate.optimal);
        assert_eq!(certificate.objective_gap.as_deref(), Some("8"));
        model.rows[0].equality = false;
        model.columns[1].cost = 4.0;
        let (_, scale) = model.scale_to_dual_feasible(&solution).unwrap();
        assert!(scale.is_zero());
        model.columns[1].cost = -1.0;
        assert!(model.scale_to_dual_feasible(&solution).is_err());
    }
    use super::*;
    fn column(cost: f64) -> Column {
        Column {
            lower: 0.0,
            upper: f64::INFINITY,
            cost,
        }
    }
    #[test]
    fn exact_basis_and_missing_global_column() {
        let restricted = Model {
            columns: vec![column(2.0)],
            rows: vec![Row {
                equality: true,
                rhs: 1.0,
                terms: vec![(0, 3.0)],
            }],
            offset: 0.0,
        };
        let solution = restricted
            .reconstruct(&Basis {
                columns: vec![BasisStatus::Basic],
                rows: vec![BasisStatus::Upper],
            })
            .unwrap();
        assert_eq!(solution.values[0].to_string(), "1/3");
        assert!(restricted.certify(&solution).unwrap().optimal);
        let full = Model {
            columns: vec![column(2.0), column(1.0)],
            rows: vec![Row {
                equality: true,
                rhs: 1.0,
                terms: vec![(0, 3.0), (1, 3.0)],
            }],
            offset: 0.0,
        };
        let extended = ExactSolution {
            values: vec![solution.values[0].clone(), Rational::zero()],
            ..solution
        };
        let certificate = full.certify(&extended).unwrap();
        assert!(certificate.primal_feasible);
        assert!(!certificate.dual_feasible);
        assert_eq!(certificate.first_improving_columns[0].0, 1);
    }
    #[test]
    fn exact_check_catches_sub_tolerance_deficit_and_negative_bound() {
        let model = Model {
            columns: vec![column(0.0)],
            rows: vec![Row {
                equality: false,
                rhs: 0.0,
                terms: vec![(0, 1.0)],
            }],
            offset: 0.0,
        };
        let mut solution = ExactSolution {
            values: vec![rational(1e-30).unwrap()],
            row_duals: vec![Rational::zero()],
            basis_dimension: 0,
        };
        assert_eq!(model.certify(&solution).unwrap().violated_rows, 1);
        solution.values[0] = rational(-1e-30).unwrap();
        assert_eq!(model.certify(&solution).unwrap().violated_bounds, 1);
    }
    #[test]
    fn finite_upper_bound_basic_slack_and_singular_hint() {
        let model = Model {
            columns: vec![Column {
                upper: 2.0,
                ..column(-1.0)
            }],
            rows: vec![Row {
                equality: false,
                rhs: 3.0,
                terms: vec![(0, 1.0)],
            }],
            offset: 5.0,
        };
        let solution = model
            .reconstruct(&Basis {
                columns: vec![BasisStatus::Upper],
                rows: vec![BasisStatus::Basic],
            })
            .unwrap();
        let certificate = model.certify(&solution).unwrap();
        assert!(certificate.optimal);
        assert_eq!(certificate.objective, "3");
        let mut singular = model.clone();
        singular.rows[0].terms.clear();
        assert!(
            singular
                .reconstruct(&Basis {
                    columns: vec![BasisStatus::Basic],
                    rows: vec![BasisStatus::Upper]
                })
                .unwrap_err()
                .contains("singular")
        );
    }

    #[test]
    fn rust_simplex_improves_wrong_support_and_certifies_full_model() {
        let model = Model {
            columns: vec![column(2.0), column(1.0)],
            rows: vec![Row {
                equality: true,
                rhs: 1.0,
                terms: vec![(0, 3.0), (1, 3.0)],
            }],
            offset: 0.0,
        };
        let result = model
            .optimize_from_basis(
                Basis {
                    columns: vec![BasisStatus::Basic, BasisStatus::Lower],
                    rows: vec![BasisStatus::Upper],
                },
                10,
            )
            .unwrap();
        assert_eq!(result.pivots, 1);
        assert_eq!(result.solution.values[1].to_string(), "1/3");
        assert!(model.certify(&result.solution).unwrap().optimal);
    }
    #[test]
    fn bland_rule_escapes_classic_degenerate_cycle() {
        let model = Model {
            columns: vec![column(-10.0), column(57.0), column(9.0), column(24.0)],
            rows: vec![
                Row {
                    equality: false,
                    rhs: 0.0,
                    terms: vec![(0, 0.5), (1, -5.5), (2, -2.5), (3, 9.0)],
                },
                Row {
                    equality: false,
                    rhs: 0.0,
                    terms: vec![(0, 0.5), (1, -1.5), (2, -0.5), (3, 1.0)],
                },
                Row {
                    equality: false,
                    rhs: 1.0,
                    terms: vec![(0, 1.0)],
                },
            ],
            offset: 0.0,
        };
        let result = model
            .optimize_from_basis(
                Basis {
                    columns: vec![BasisStatus::Lower; 4],
                    rows: vec![BasisStatus::Basic; 3],
                },
                30,
            )
            .unwrap();
        let certificate = model.certify(&result.solution).unwrap();
        assert!(certificate.optimal);
        assert_eq!(certificate.objective, "-1");
        assert!(result.pivots > 3);
        assert!(result.bland_fallback);
    }
    #[test]
    fn unbounded_has_verified_ray_and_bad_hint_is_not_infeasibility() {
        let model = Model {
            columns: vec![column(-1.0)],
            rows: vec![],
            offset: 0.0,
        };
        let result = model
            .optimize_from_basis(
                Basis {
                    columns: vec![BasisStatus::Lower],
                    rows: vec![],
                },
                10,
            )
            .unwrap();
        assert_eq!(result.unbounded_ray.unwrap()[0].to_string(), "1");
        let impossible_hint = Model {
            rows: vec![Row {
                equality: false,
                rhs: -1.0,
                terms: vec![(0, 1.0)],
            }],
            ..model
        };
        assert!(
            impossible_hint
                .optimize_from_basis(
                    Basis {
                        columns: vec![BasisStatus::Lower],
                        rows: vec![BasisStatus::Basic]
                    },
                    10
                )
                .unwrap_err()
                .contains("phase I")
        );
    }

    #[test]
    fn integer_pricing_matches_rational_arithmetic_at_binary_extremes() {
        let inputs = [
            f64::from_bits(1),
            f64::MIN_POSITIVE,
            -f64::MAX,
            1e-200,
            -1e200,
            -0.0,
            0.1,
        ];
        let y = rational(1.0).unwrap() / rational(3.0).unwrap();
        for a in inputs {
            let model = Model {
                columns: vec![column(0.1)],
                rows: vec![Row {
                    equality: true,
                    rhs: 0.0,
                    terms: vec![(0, a)],
                }],
                offset: 0.0,
            };
            let solution = ExactSolution {
                values: vec![Rational::zero()],
                row_duals: vec![y.clone()],
                basis_dimension: 0,
            };
            let priced = model.reduced_costs(&solution);
            assert_eq!(
                Rational::new(priced.numerators[0].clone(), priced.denominator),
                rational(0.1).unwrap() - rational(a).unwrap() * &y
            );
        }
    }
    #[test]
    fn nonterminal_limit_preserves_a_resumable_feasible_basis() {
        let model = Model {
            columns: vec![column(2.0), column(1.0)],
            rows: vec![Row {
                equality: true,
                rhs: 1.0,
                terms: vec![(0, 1.0), (1, 1.0)],
            }],
            offset: 0.0,
        };
        let stopped = model
            .optimize_from_basis(
                Basis {
                    columns: vec![BasisStatus::Basic, BasisStatus::Lower],
                    rows: vec![BasisStatus::Upper],
                },
                0,
            )
            .unwrap();
        assert!(stopped.stop_reason.is_some());
        let certificate = model.certify(&stopped.solution).unwrap();
        assert!(certificate.primal_feasible);
        assert!(!certificate.optimal);
        let resumed = model.optimize_from_basis(stopped.basis, 10).unwrap();
        assert!(resumed.stop_reason.is_none());
        assert!(model.certify(&resumed.solution).unwrap().optimal);
    }
    #[test]
    fn phase_one_starts_from_zero_and_removes_redundant_artificials() {
        let model = Model {
            columns: vec![column(1.0)],
            rows: vec![
                Row {
                    equality: true,
                    rhs: 1.0,
                    terms: vec![(0, 1.0)],
                },
                Row {
                    equality: true,
                    rhs: 2.0,
                    terms: vec![(0, 2.0)],
                },
                Row {
                    equality: true,
                    rhs: 0.0,
                    terms: vec![],
                },
            ],
            offset: 0.0,
        };
        let result = model.solve(30).unwrap();
        assert!(result.infeasibility.is_none());
        let optimum = result.phase2.unwrap();
        assert_eq!(optimum.basis.columns.len(), 1);
        assert!(model.certify(&optimum.solution).unwrap().optimal);
        assert_eq!(optimum.solution.values[0].to_string(), "1");
    }
    #[test]
    fn phase_one_proves_inconsistent_equalities_and_impossible_empty_row() {
        let model = Model {
            columns: vec![column(100.0)],
            rows: vec![
                Row {
                    equality: true,
                    rhs: 1.0,
                    terms: vec![(0, 1.0)],
                },
                Row {
                    equality: true,
                    rhs: 2.0,
                    terms: vec![(0, 1.0)],
                },
            ],
            offset: 7.0,
        };
        let result = model.solve(30).unwrap();
        assert!(result.phase2.is_none());
        assert!(result.infeasibility.is_some());
        assert!(
            model
                .verify_infeasibility(&result.phase1.solution.row_duals)
                .unwrap()
                .is_some()
        );
        let feasible = Model {
            rows: vec![model.rows[0].clone(), model.rows[0].clone()],
            ..model.clone()
        };
        assert!(
            feasible
                .verify_infeasibility(&result.phase1.solution.row_duals)
                .unwrap()
                .is_none()
        );
        let empty = Model {
            columns: vec![],
            rows: vec![Row {
                equality: false,
                rhs: -1.0,
                terms: vec![],
            }],
            offset: 0.0,
        };
        assert!(empty.solve(10).unwrap().infeasibility.is_some());
    }
    #[test]
    fn phase_one_handles_negative_rhs_and_preserves_nonterminal_limits() {
        let model = Model {
            columns: vec![column(2.0)],
            rows: vec![Row {
                equality: false,
                rhs: -1.0,
                terms: vec![(0, -1.0)],
            }],
            offset: 0.0,
        };
        let stopped = model.solve(0).unwrap();
        assert!(stopped.phase1.stop_reason.is_some());
        assert!(stopped.phase2.is_none());
        assert!(stopped.infeasibility.is_none());
        let result = model.solve(30).unwrap();
        assert!(
            model
                .certify(&result.phase2.unwrap().solution)
                .unwrap()
                .optimal
        );
        let unbounded = Model {
            columns: vec![column(-1.0)],
            rows: vec![],
            offset: 0.0,
        };
        assert!(
            unbounded
                .solve(10)
                .unwrap()
                .phase2
                .unwrap()
                .unbounded_ray
                .is_some()
        );
    }
}
