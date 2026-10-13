//! Complete-model feasibility and exact objective-gap certification.
use crate::{
    Basis, BasisStatus, Certificate, ExactSolution, FloatCandidate, InfeasibilityCertificate,
    Model, Rational, Row, rational,
};
use num_traits::{Signed, ToPrimitive, Zero};

pub struct CertifiedResult {
    pub solution: ExactSolution,
    /// Reconstructs the primal values; the certified dual may be independent.
    pub primal_basis: Basis,
    pub certificate: Certificate,
    pub relative_gap: Option<Rational>,
    pub accepted: bool,
    pub exact_pivots: usize,
    pub support_search_pivots: usize,
    pub support_columns: usize,
    pub stop_reason: Option<String>,
    /// Verified for the complete original model, never only a restriction.
    pub infeasibility: Option<InfeasibilityCertificate>,
}
impl Certificate {
    /// Exact normalization: gap / max(1, |primal objective|).
    pub fn relative_gap(&self) -> Result<Option<Rational>, String> {
        if !self.primal_feasible || !self.dual_feasible {
            return Ok(None);
        }
        let objective: Rational = self
            .objective
            .parse()
            .map_err(|_| "invalid exact objective")?;
        let gap: Rational = self
            .objective_gap
            .as_ref()
            .ok_or("missing exact gap")?
            .parse()
            .map_err(|_| "invalid exact gap")?;
        if gap.is_negative() {
            return Err("negative primal-dual gap".into());
        }
        Ok(Some(gap / objective.abs().max(rational(1.0)?)))
    }
    /// Round upward for reporting. Acceptance always uses the rational value.
    pub fn relative_gap_upper_bound(&self) -> Result<Option<f64>, String> {
        let Some(gap) = self.relative_gap()? else {
            return Ok(None);
        };
        let mut value = gap.to_f64().ok_or("gap cannot be represented")?;
        if !value.is_finite() {
            return Err("gap exceeds finite reporting range".into());
        }
        if rational(value)? < gap {
            value = value.next_up();
            if !value.is_finite() {
                return Err("gap has no finite upper reporting bound".into());
            }
        }
        Ok(Some(value))
    }
}

impl Model {
    /// Refine a Rust floating hint, accepting only full-model exact feasibility
    /// and an exact comparison against the caller's objective-gap budget.
    /// Requires nonnegative costs and nonnegative variables without upper bounds.
    /// Observer phases distinguish support work from full-model work. Floating
    /// tolerances and support selection never relax an original constraint.
    pub fn certify_candidate_with_observer(
        &self,
        candidate: &FloatCandidate,
        max_pivots: usize,
        gap_budget: &Rational,
        mut observer: impl FnMut(&str, usize) -> bool,
    ) -> Result<CertifiedResult, String> {
        if gap_budget.is_negative() {
            return Err("negative objective-gap budget".into());
        }
        if !observer("reconstruct", 0) {
            return Err("certified solve cancelled before reconstruction".into());
        }
        let initial = self.reconstruct(&candidate.basis)?;
        let (adjusted, _) = self.scale_to_dual_feasible(&initial)?;
        let initial_result = self.certified_result(
            adjusted.clone(),
            candidate.basis.clone(),
            gap_budget,
            0,
            self.columns.len(),
        )?;
        if initial_result.accepted {
            return Ok(initial_result);
        }
        if !observer("verify", 0) {
            return Ok(CertifiedResult {
                stop_reason: Some("certified observer requested stop".into()),
                ..initial_result
            });
        }
        let mut primal_basis = candidate.basis.clone();
        let mut exact_pivots = 0;
        let mut support_columns = self.columns.len();
        let mut support_search_pivots = 0;
        if !initial_result.certificate.primal_feasible {
            let (restricted, selected) = self.candidate_support(&candidate.values)?;
            support_columns = selected.len();
            let proposal = restricted
                .float_candidate_with_observer(20000, |p| observer("support-search", p.pivots))?;
            support_search_pivots = proposal.pivots;
            let repaired =
                restricted.refine_basis_with_observer(proposal.basis, max_pivots, |_, p| {
                    observer("support-refine", p.pivots)
                })?;
            exact_pivots += repaired.repair_pivots + repaired.optimization_pivots;
            if repaired.certificate.primal_feasible {
                let mut values = vec![Rational::zero(); self.columns.len()];
                primal_basis = Basis {
                    columns: vec![BasisStatus::Lower; self.columns.len()],
                    rows: repaired.basis.rows,
                };
                for (new, old) in selected.iter().enumerate() {
                    values[*old] = repaired.solution.values[new].clone();
                    primal_basis.columns[*old] = repaired.basis.columns[new];
                }
                let combined = ExactSolution {
                    values,
                    row_duals: adjusted.row_duals.clone(),
                    basis_dimension: repaired.solution.basis_dimension,
                };
                let mut result = self.certified_result(
                    combined,
                    primal_basis.clone(),
                    gap_budget,
                    exact_pivots,
                    support_columns,
                )?;
                result.support_search_pivots = support_search_pivots;
                if result.accepted {
                    return Ok(result);
                }
                if !observer("verify", exact_pivots) {
                    return Ok(CertifiedResult {
                        stop_reason: Some("certified observer requested stop".into()),
                        ..result
                    });
                }
            }
            // Failure of a restricted model is not an original-model proof.
            // The remaining full-model refinement retains all original columns.
        }
        let refined = self.refine_basis_with_observer(
            primal_basis,
            max_pivots.saturating_sub(exact_pivots),
            |_, p| observer("full-refine", exact_pivots + p.pivots),
        )?;
        exact_pivots += refined.repair_pivots + refined.optimization_pivots;
        let mut result = self.certified_result(
            refined.solution.clone(),
            refined.basis.clone(),
            gap_budget,
            exact_pivots,
            support_columns,
        )?;
        if !result.accepted {
            let combined = ExactSolution {
                row_duals: adjusted.row_duals,
                ..refined.solution
            };
            let other = self.certified_result(
                combined,
                refined.basis,
                gap_budget,
                exact_pivots,
                support_columns,
            )?;
            if other
                .relative_gap
                .as_ref()
                .is_some_and(|g| result.relative_gap.as_ref().is_none_or(|old| g < old))
            {
                result = other;
            }
        }
        result.stop_reason = if result.accepted || refined.infeasibility.is_some() {
            None
        } else {
            refined
                .stop_reason
                .or(Some("objective-gap budget not established".into()))
        };
        result.support_search_pivots = support_search_pivots;
        result.infeasibility = refined.infeasibility;
        Ok(result)
    }
    fn certified_result(
        &self,
        solution: ExactSolution,
        primal_basis: Basis,
        budget: &Rational,
        exact_pivots: usize,
        support_columns: usize,
    ) -> Result<CertifiedResult, String> {
        let certificate = self.certify(&solution)?;
        let relative_gap = certificate.relative_gap()?;
        let accepted = relative_gap.as_ref().is_some_and(|gap| gap <= budget);
        Ok(CertifiedResult {
            solution,
            primal_basis,
            certificate,
            relative_gap,
            accepted,
            exact_pivots,
            support_search_pivots: 0,
            support_columns,
            stop_reason: None,
            infeasibility: None,
        })
    }
    fn candidate_support(&self, values: &[f64]) -> Result<(Self, Vec<usize>), String> {
        if values.len() != self.columns.len() || values.iter().any(|x| !x.is_finite()) {
            return Err("support requires a finite full vector".into());
        }
        let mut keep: Vec<_> = values.iter().map(|x| *x != 0.0).collect();
        for row in &self.rows {
            let mut sum = 0.0f64;
            let mut correction = 0.0;
            let mut magnitude = 0.0;
            for (j, a) in &row.terms {
                let product = a * values[*j];
                correction += a.mul_add(values[*j], -product);
                let next = sum + product;
                correction += if sum.abs() >= product.abs() {
                    (sum - next) + product
                } else {
                    (product - next) + sum
                };
                sum = next;
                magnitude += product.abs();
            }
            let residual = if row.equality {
                (sum + correction - row.rhs).abs()
            } else {
                (sum + correction - row.rhs).max(0.0)
            };
            if !residual.is_finite() || residual > 1e-6 * magnitude.max(row.rhs.abs()) + 1e-8 {
                for (j, _) in &row.terms {
                    keep[*j] = true;
                }
            }
        }
        let selected: Vec<_> = keep
            .iter()
            .enumerate()
            .filter_map(|(i, k)| k.then_some(i))
            .collect();
        let mut remap = vec![None; keep.len()];
        for (new, old) in selected.iter().enumerate() {
            remap[*old] = Some(new);
        }
        Ok((
            Self {
                columns: selected.iter().map(|j| self.columns[*j].clone()).collect(),
                rows: self
                    .rows
                    .iter()
                    .map(|r| Row {
                        equality: r.equality,
                        rhs: r.rhs,
                        terms: r
                            .terms
                            .iter()
                            .filter_map(|(j, a)| remap[*j].map(|k| (k, *a)))
                            .collect(),
                    })
                    .collect(),
                offset: self.offset,
            },
            selected,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Column;
    #[test]
    fn full_infeasibility_certificate_survives_the_combined_entry_point() {
        let model = Model {
            columns: vec![],
            rows: vec![Row {
                equality: true,
                rhs: 1.0,
                terms: vec![],
            }],
            offset: 0.0,
        };
        let candidate = model.float_candidate_with_observer(20, |_| true).unwrap();
        let result = model
            .certify_candidate_with_observer(&candidate, 20, &Rational::zero(), |_, _| true)
            .unwrap();
        assert!(!result.accepted && !result.certificate.primal_feasible);
        let proof = result
            .infeasibility
            .expect("missing original-model Farkas proof");
        let mut dual = vec![Rational::zero(); model.rows.len()];
        for (i, y) in proof.row_duals {
            dual[i] = y.parse().unwrap();
        }
        assert!(model.verify_infeasibility(&dual).unwrap().is_some());
        assert!(result.stop_reason.is_none());
    }
    #[test]
    fn rational_budget_boundary_and_display_rounding_are_separate() {
        let model = Model {
            columns: [1.0, 0.125]
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
                terms: vec![(0, 7.0), (1, 1.0)],
            }],
            offset: 0.0,
        };
        let candidate = FloatCandidate {
            basis: Basis {
                columns: vec![BasisStatus::Basic, BasisStatus::Lower],
                rows: vec![BasisStatus::Upper],
            },
            values: vec![1.0 / 7.0, 0.0],
            pivots: 0,
            stop_reason: "test hint".into(),
            diagnostic: None,
        };
        let gap = Rational::new(1.into(), 56.into());
        let result = model
            .certify_candidate_with_observer(&candidate, 0, &gap, |_, _| true)
            .unwrap();
        assert!(result.accepted && !result.certificate.optimal);
        assert_eq!(result.relative_gap.as_ref(), Some(&gap));
        let reported = result
            .certificate
            .relative_gap_upper_bound()
            .unwrap()
            .unwrap();
        assert!(rational(reported).unwrap() >= gap);
        assert!(rational(reported.next_down()).unwrap() < gap);
        let tighter = &gap - Rational::new(1.into(), crate::BigInt::from(1) << 200usize);
        let result = model
            .certify_candidate_with_observer(&candidate, 0, &tighter, |_, _| true)
            .unwrap();
        assert!(!result.accepted);
        assert!(result.certificate.primal_feasible && result.certificate.dual_feasible);
    }
    #[test]
    fn gap_budget_is_exact_and_does_not_claim_zero_gap() {
        let model = Model {
            columns: [1.0, 1.0 - 2.0f64.powi(-40)]
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
                terms: vec![(0, 1.0), (1, 1.0)],
            }],
            offset: 0.0,
        };
        let candidate = FloatCandidate {
            basis: Basis {
                columns: vec![BasisStatus::Basic, BasisStatus::Lower],
                rows: vec![BasisStatus::Upper],
            },
            values: vec![1.0, 0.0],
            pivots: 0,
            stop_reason: "test hint".into(),
            diagnostic: None,
        };
        let budget = Rational::new(1.into(), 1_000_000_000_000u64.into());
        let result = model
            .certify_candidate_with_observer(&candidate, 20, &budget, |_, _| true)
            .unwrap();
        assert!(
            result.accepted
                && result.certificate.primal_feasible
                && result.certificate.dual_feasible
        );
        assert!(!result.certificate.optimal);
        assert_eq!(result.exact_pivots, 0);
        let gap = result.relative_gap.unwrap();
        assert!(gap > Rational::zero() && gap <= budget);
        assert!(
            rational(
                result
                    .certificate
                    .relative_gap_upper_bound()
                    .unwrap()
                    .unwrap()
            )
            .unwrap()
                >= gap
        );
        let result = model
            .certify_candidate_with_observer(&candidate, 20, &Rational::zero(), |_, _| true)
            .unwrap();
        assert!(result.accepted && result.certificate.optimal);
        assert_eq!(result.exact_pivots, 1);
    }
    #[test]
    fn infeasible_restriction_is_not_a_full_model_infeasibility_claim() {
        let model = Model {
            columns: [0.0, 1.0]
                .into_iter()
                .map(|cost| Column {
                    lower: 0.0,
                    upper: f64::INFINITY,
                    cost,
                })
                .collect(),
            rows: vec![Row {
                equality: true,
                rhs: 1e-15,
                terms: vec![(0, -1.0), (1, 1.0)],
            }],
            offset: 0.0,
        };
        let candidate = FloatCandidate {
            basis: Basis {
                columns: vec![BasisStatus::Basic, BasisStatus::Lower],
                rows: vec![BasisStatus::Upper],
            },
            values: vec![0.0, 0.0],
            pivots: 0,
            stop_reason: "misleading hint".into(),
            diagnostic: None,
        };
        let mut full = false;
        let result = model
            .certify_candidate_with_observer(&candidate, 20, &Rational::zero(), |phase, _| {
                full |= phase == "full-refine";
                true
            })
            .unwrap();
        assert!(full && result.accepted && result.certificate.optimal);
        assert_eq!(result.support_columns, 0);
        assert_eq!(result.solution.values[1], rational(1e-15).unwrap());
        assert!(
            model
                .certify_candidate_with_observer(&candidate, 20, &Rational::zero(), |_, _| false)
                .is_err()
        );
    }
}
