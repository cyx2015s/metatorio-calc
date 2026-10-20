//! Relative feasibility with exact arithmetic and an exact original dual witness.
//! No absolute floor: an entirely missing microscopic supply has error one.
use crate::{
    Basis, BasisStatus, Certificate, ExactSolution, FloatCandidate, FloatOptions, Model, Rational,
    SolveControl, rational,
};
use num_traits::{Signed, ToPrimitive, Zero};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub struct RelativeVerification {
    pub accepted: bool,
    pub maximum_relative_balance_error: String,
    /// Absolute primal-cost / dual-bound difference. For an inexactly feasible
    /// point this is a stopping criterion, not an original-LP upper bound.
    pub relative_duality_gap: Option<String>,
}
impl RelativeVerification {
    pub fn balance_error_upper_bound(&self) -> Result<f64, String> {
        upper_bound(&self.maximum_relative_balance_error)
    }
    pub fn duality_gap_upper_bound(&self) -> Result<Option<f64>, String> {
        self.relative_duality_gap
            .as_deref()
            .map(upper_bound)
            .transpose()
    }
}
fn upper_bound(value: &str) -> Result<f64, String> {
    let exact: Rational = value
        .parse()
        .map_err(|_| "invalid relative verification value")?;
    let mut rounded = exact
        .to_f64()
        .ok_or("unrepresentable relative verification value")?;
    if !rounded.is_finite() {
        return Err("nonfinite relative verification value".into());
    }
    if rational(rounded)? < exact {
        rounded = rounded.next_up();
    }
    Ok(rounded)
}

pub struct RelativeCandidate {
    pub solution: ExactSolution,
    pub basis: Basis,
    pub certificate: Certificate,
    pub relative: RelativeVerification,
    pub rounded_relative: RelativeVerification,
    pub search_pivots: usize,
}

impl Model {
    pub fn verify_relative_with_control(
        &self,
        solution: &ExactSolution,
        balance_budget: &Rational,
        gap_budget: &Rational,
        control: &SolveControl,
    ) -> Result<RelativeVerification, String> {
        control.check()?;
        if balance_budget.is_negative() || gap_budget.is_negative() {
            return Err("negative relative verification budget".into());
        }
        let certificate = self.certify(solution)?;
        self.verify_relative_certificate(
            solution,
            &certificate,
            balance_budget,
            gap_budget,
            control,
        )
    }

    fn verify_relative_certificate(
        &self,
        solution: &ExactSolution,
        certificate: &Certificate,
        balance_budget: &Rational,
        gap_budget: &Rational,
        control: &SolveControl,
    ) -> Result<RelativeVerification, String> {
        let mut maximum = Rational::zero();
        for row in &self.rows {
            control.check()?;
            let mut activity = Rational::zero();
            let mut magnitude = Rational::zero();
            let mut coefficients = BTreeMap::<usize, Rational>::new();
            for (j, a) in &row.terms {
                if !solution.values[*j].is_zero() {
                    *coefficients.entry(*j).or_default() += rational(*a)?;
                }
            }
            for (j, a) in coefficients {
                let term = a * &solution.values[j];
                magnitude += term.abs();
                activity += term;
            }
            let rhs = rational(row.rhs)?;
            let error = if row.equality {
                (&activity - &rhs).abs()
            } else {
                (&activity - &rhs).max(Rational::zero())
            };
            let scale = magnitude.max(rhs.abs());
            if !error.is_zero() {
                // A nonzero residual implies a nonzero activity or RHS.
                if scale.is_zero() {
                    return Err("nonzero residual without a relative scale".into());
                }
                maximum = maximum.max(error / scale);
            }
        }
        let objective: Rational = certificate
            .objective
            .parse()
            .map_err(|_| "invalid exact objective")?;
        let gap = certificate
            .dual_objective
            .as_ref()
            .map(|dual| -> Result<Rational, String> {
                let dual: Rational = dual.parse().map_err(|_| "invalid exact dual bound")?;
                Ok((&objective - dual).abs() / objective.abs().max(rational(1.0)?))
            })
            .transpose()?;
        let accepted = certificate.violated_bounds == 0
            && certificate.dual_feasible
            && maximum <= *balance_budget
            && gap.as_ref().is_some_and(|g| g <= gap_budget);
        Ok(RelativeVerification {
            accepted,
            maximum_relative_balance_error: maximum.to_string(),
            relative_duality_gap: gap.map(|g| g.to_string()),
        })
    }

    /// A hint-only support search. Both its original rational values and the
    /// actual f64 output are checked against EVERY original row and column.
    pub fn relative_candidate_with_control(
        &self,
        hint: &FloatCandidate,
        balance_budget: &Rational,
        gap_budget: &Rational,
        control: &SolveControl,
    ) -> Result<Option<RelativeCandidate>, String> {
        control.check()?;
        self.check()?;
        if self
            .columns
            .iter()
            .any(|c| c.lower != 0.0 || c.upper != f64::INFINITY || c.cost < 0.0)
        {
            return Ok(None);
        }
        let original = match self.reconstruct_with_control(&hint.basis, control) {
            Ok(solution) => solution,
            Err(error) if control.is_cancelled() => return Err(error),
            Err(_) => return Ok(None),
        };
        // The fast path targets roundoff-sized primal residuals with an already
        // valid dual witness. Leave dual repair to the existing exact pipeline;
        // an unpromising compact search must not delay that repair.
        if !self.certify(&original)?.dual_feasible {
            return Ok(None);
        }
        let row_duals = original.row_duals.clone();
        let check = |mut solution: ExactSolution,
                     basis: Basis,
                     pivots|
         -> Result<Option<RelativeCandidate>, String> {
            for x in &mut solution.values {
                if x.is_negative() {
                    *x = Rational::zero();
                }
            }
            solution.row_duals.clone_from(&row_duals);
            let certificate = self.certify(&solution)?;
            let relative = self.verify_relative_certificate(
                &solution,
                &certificate,
                balance_budget,
                gap_budget,
                control,
            )?;
            if !relative.accepted {
                return Ok(None);
            }
            let rounded = ExactSolution {
                values: solution
                    .approximate()?
                    .into_iter()
                    .map(rational)
                    .collect::<Result<_, _>>()?,
                row_duals: solution.row_duals.clone(),
                basis_dimension: solution.basis_dimension,
            };
            let rounded_relative =
                self.verify_relative_with_control(&rounded, balance_budget, gap_budget, control)?;
            if !rounded_relative.accepted {
                return Ok(None);
            }
            Ok(Some(RelativeCandidate {
                solution,
                basis,
                certificate,
                relative,
                rounded_relative,
                search_pivots: pivots,
            }))
        };
        if let Some(result) = check(original, hint.basis.clone(), 0)? {
            return Ok(Some(result));
        }
        let (restricted, selected) = self.candidate_support(&hint.values, false)?;
        if selected.is_empty() {
            return Ok(None);
        }
        let compact = restricted.float_candidate_with_options(
            FloatOptions {
                ratio_tolerance: 1e-12,
                primal_tolerance: 1e-12,
            },
            20000,
            |_| !control.is_cancelled(),
        )?;
        let mut basis = Basis {
            columns: vec![BasisStatus::Lower; self.columns.len()],
            rows: compact.basis.rows,
        };
        for (j, old) in selected.iter().enumerate() {
            basis.columns[*old] = compact.basis.columns[j];
        }
        let solution = match self.reconstruct_with_control(&basis, control) {
            Ok(solution) => solution,
            Err(error) if control.is_cancelled() => return Err(error),
            Err(_) => return Ok(None),
        };
        check(solution, basis, compact.pivots)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Column, Row};
    fn model() -> Model {
        Model {
            columns: [1.0, 0.0]
                .into_iter()
                .map(|cost| Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost,
                })
                .collect(),
            rows: vec![
                Row {
                    equality: true,
                    rhs: 1.0,
                    terms: vec![(0, 1.0)],
                },
                Row {
                    equality: true,
                    rhs: 0.0,
                    terms: vec![(0, 1.0), (1, -1.0)],
                },
            ],
            offset: 0.0,
        }
    }
    fn check(m: &Model, x: [f64; 2], y: [f64; 2]) -> RelativeVerification {
        m.verify_relative_with_control(
            &ExactSolution {
                values: x.into_iter().map(|x| rational(x).unwrap()).collect(),
                row_duals: y.into_iter().map(|x| rational(x).unwrap()).collect(),
                basis_dimension: 0,
            },
            &rational(1e-12).unwrap(),
            &rational(1e-12).unwrap(),
            &SolveControl::default(),
        )
        .unwrap()
    }
    #[test]
    fn relative_balance_is_separate_from_exact_primal_feasibility() {
        let m = model();
        let verified = check(&m, [1.0, 1.0 - 1e-14], [1.0, 0.0]);
        assert!(verified.accepted);
        assert!(verified.balance_error_upper_bound().unwrap() < 1e-12);
        assert!(!check(&m, [1.0, 1.0 - 1e-10], [1.0, 0.0]).accepted);
        assert!(!check(&m, [1.0, -1e-30], [1.0, 0.0]).accepted);
        assert!(!check(&m, [1.0, 1.0], [2.0, 0.0]).accepted);
        assert!(!check(&m, [1.0, 1.0], [0.0, 0.0]).accepted);
    }
    #[test]
    fn microscopic_targets_and_suppliers_have_no_absolute_escape_hatch() {
        let mut m = model();
        m.rows[0].rhs = 1e-30;
        assert!(!check(&m, [0.0, 0.0], [1.0, 0.0]).accepted);
        let bad = check(&m, [1e-30, 0.0], [1.0, 0.0]);
        assert!(!bad.accepted);
        assert_eq!(bad.maximum_relative_balance_error, "1");
        assert!(check(&m, [1e-30, 1e-30], [1.0, 0.0]).accepted);
    }
    #[test]
    fn duplicate_cancellation_cannot_dilute_the_relative_residual() {
        let mut m = model();
        m.rows[1].terms = vec![(0, 1e20), (1, 1e-30), (0, -1e20)];
        let bad = check(&m, [1.0, 1.0], [1.0, 0.0]);
        assert!(!bad.accepted);
        assert_eq!(bad.maximum_relative_balance_error, "1");
    }

    #[test]
    fn zero_budget_requires_exact_rows_and_negative_signed_gap_is_bounded() {
        let m = model();
        let mut solution = ExactSolution {
            values: vec![rational(1.0 - 1e-14).unwrap(); 2],
            row_duals: vec![rational(1.0).unwrap(), Rational::zero()],
            basis_dimension: 0,
        };
        let certificate = m.certify(&solution).unwrap();
        assert!(!certificate.primal_feasible);
        assert!(certificate.dual_feasible);
        assert!(certificate.relative_gap().unwrap().is_none());
        let budget = rational(1e-12).unwrap();
        let control = SolveControl::default();
        assert!(
            m.verify_relative_with_control(&solution, &budget, &budget, &control)
                .unwrap()
                .accepted
        );
        assert!(
            !m.verify_relative_with_control(&solution, &Rational::zero(), &budget, &control)
                .unwrap()
                .accepted
        );
        solution.values = vec![rational(1.0).unwrap(); 2];
        assert!(
            m.verify_relative_with_control(
                &solution,
                &Rational::zero(),
                &Rational::zero(),
                &control
            )
            .unwrap()
            .accepted
        );
    }

    #[test]
    fn rounded_underflow_cannot_remove_a_required_supply() {
        let mut m = model();
        m.rows[0].rhs = 1e-300;
        m.rows[0].terms[0].1 = 1e100;
        let amount = rational(1e-300).unwrap() / rational(1e100).unwrap();
        let solution = ExactSolution {
            values: vec![amount.clone(), amount],
            row_duals: vec![Rational::zero(); 2],
            basis_dimension: 0,
        };
        let budget = rational(1e-12).unwrap();
        let control = SolveControl::default();
        assert!(
            m.verify_relative_with_control(&solution, &budget, &budget, &control)
                .unwrap()
                .accepted
        );
        let rounded = ExactSolution {
            values: solution
                .approximate()
                .unwrap()
                .into_iter()
                .map(|v| rational(v).unwrap())
                .collect(),
            ..solution
        };
        let verified = m
            .verify_relative_with_control(&rounded, &budget, &budget, &control)
            .unwrap();
        assert!(!verified.accepted);
        assert_eq!(verified.maximum_relative_balance_error, "1");
    }
}
