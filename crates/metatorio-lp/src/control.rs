//! Optional cooperative cancellation, separate from nonterminal pivot observers.
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

/// Interrupt expensive exact operations without treating unfinished work as a
/// feasibility or optimality proof. Defaults never interrupt. Checks occur
/// between elimination rows and triangular solve rows; an individual rational
/// operation and memory allocation are not preemptible.
#[derive(Clone, Default)]
pub struct SolveControl {
    deadline: Option<Instant>,
    cancellation: Option<Arc<AtomicBool>>,
}

impl SolveControl {
    pub fn with_deadline(deadline: Instant) -> Self {
        Self {
            deadline: Some(deadline),
            cancellation: None,
        }
    }

    /// An external flag allows a caller to cancel from another thread.
    pub fn with_cancellation(mut self, cancellation: Arc<AtomicBool>) -> Self {
        self.cancellation = Some(cancellation);
        self
    }

    /// Bound a child search without extending the parent's deadline or losing
    /// its external cancellation flag.
    pub fn limited_to(&self, deadline: Instant) -> Self {
        Self {
            deadline: Some(
                self.deadline
                    .map_or(deadline, |parent| parent.min(deadline)),
            ),
            cancellation: self.cancellation.clone(),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
            || self
                .deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
    }

    pub(crate) fn check(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err("exact operation cancelled; no terminal status established".into())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Basis, BasisStatus, Column, FloatCandidate, Model, Rational, Row};

    #[test]
    fn child_deadline_retains_parent_deadline_and_cancellation() {
        let flag = Arc::new(AtomicBool::new(false));
        let parent = SolveControl::with_deadline(Instant::now()).with_cancellation(flag.clone());
        assert!(
            parent
                .limited_to(Instant::now() + std::time::Duration::from_secs(60))
                .is_cancelled()
        );
        let parent = SolveControl::default().with_cancellation(flag.clone());
        let child = parent.limited_to(Instant::now() + std::time::Duration::from_secs(60));
        flag.store(true, Ordering::Relaxed);
        assert!(child.is_cancelled());
    }

    fn model() -> (Model, Basis) {
        (
            Model {
                columns: [1.0, 2.0]
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
            },
            Basis {
                columns: vec![BasisStatus::Lower, BasisStatus::Basic],
                rows: vec![BasisStatus::Upper],
            },
        )
    }

    #[test]
    fn expired_reconstruction_returns_no_solution() {
        let (model, basis) = model();
        let control = SolveControl::with_deadline(Instant::now());
        assert!(
            model
                .reconstruct_with_control(&basis, &control)
                .unwrap_err()
                .contains("cancelled")
        );
        let solution = model.reconstruct(&basis).unwrap();
        assert!(model.certify(&solution).unwrap().primal_feasible);
    }

    #[test]
    fn cancellation_during_optimization_does_not_claim_a_result_and_can_resume() {
        let (model, basis) = model();
        let flag = Arc::new(AtomicBool::new(false));
        let control = SolveControl::default().with_cancellation(flag.clone());
        let mut observed = false;
        let stopped = model.refine_basis_with_control(
            basis.clone(),
            20,
            |repair, progress| {
                assert!(!repair && progress.pivots == 0);
                observed = true;
                flag.store(true, Ordering::Relaxed);
                true
            },
            &control,
        );
        assert!(observed && stopped.unwrap_err().contains("cancelled"));
        flag.store(false, Ordering::Relaxed);
        let resumed = model
            .refine_basis_with_control(basis, 20, |_, _| true, &control)
            .unwrap();
        let independent = model.solve_dual_with_observer(20, |_| true).unwrap();
        assert!(resumed.certificate.optimal);
        assert_eq!(
            resumed.certificate.objective,
            model.certify(&independent.solution).unwrap().objective
        );
        assert_eq!(resumed.solution.values, independent.solution.values);
    }

    #[test]
    fn cancelled_certification_cannot_accept_even_an_optimal_hint() {
        let (model, _) = model();
        let candidate = FloatCandidate {
            basis: Basis {
                columns: vec![BasisStatus::Basic, BasisStatus::Lower],
                rows: vec![BasisStatus::Upper],
            },
            values: vec![1.0, 0.0],
            pivots: 0,
            stop_reason: "hint".into(),
            diagnostic: None,
        };
        let flag = Arc::new(AtomicBool::new(false));
        let control = SolveControl::default().with_cancellation(flag.clone());
        let result = model.certify_candidate_with_control(
            &candidate,
            20,
            &Rational::from_integer(0.into()),
            |phase, _| {
                assert_eq!(phase, "reconstruct");
                flag.store(true, Ordering::Relaxed);
                true
            },
            &control,
        );
        assert!(matches!(result, Err(message) if message.contains("cancelled")));
        flag.store(false, Ordering::Relaxed);
        let verified = model
            .certify_candidate_with_control(
                &candidate,
                20,
                &Rational::from_integer(0.into()),
                |_, _| true,
                &control,
            )
            .unwrap();
        assert!(verified.accepted && verified.certificate.optimal);
    }
}
