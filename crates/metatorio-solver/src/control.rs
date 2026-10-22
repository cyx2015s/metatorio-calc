//! Cancellation for a synchronous solve call tree on one worker thread.
//!
//! The scope is deliberately thread local: spawned threads must install their
//! own cloned token. It is restored on return and on panic.
use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

thread_local! {
    static CURRENT: RefCell<Option<CancellationToken>> = const { RefCell::new(None) };
}

#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    pub fn scope<T>(&self, compute: impl FnOnce() -> T) -> T {
        struct Restore(Option<CancellationToken>);
        impl Drop for Restore {
            fn drop(&mut self) {
                CURRENT.with(|slot| {
                    slot.replace(self.0.take());
                });
            }
        }
        let _restore = Restore(CURRENT.with(|slot| slot.replace(Some(self.clone()))));
        compute()
    }
}

/// Cancellation is unfinished computation, never an infeasibility proof.
pub fn check_cancelled() -> Result<(), String> {
    if CURRENT.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
    }) {
        Err("solve cancelled; no terminal status established".into())
    } else {
        Ok(())
    }
}

#[cfg(feature = "rational-lp")]
pub(crate) fn exact_control(deadline: std::time::Instant) -> crate::lp_rational::SolveControl {
    let control = crate::lp_rational::SolveControl::with_deadline(deadline);
    CURRENT.with(|slot| match slot.borrow().as_ref() {
        Some(token) => control.with_cancellation(token.0.clone()),
        None => control,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_scopes_restore_after_return_and_panic() {
        let cancelled = CancellationToken::default();
        cancelled.cancel();
        cancelled.scope(|| {
            assert!(check_cancelled().is_err());
            CancellationToken::default().scope(|| assert!(check_cancelled().is_ok()));
            assert!(check_cancelled().is_err());
            let _ =
                std::panic::catch_unwind(|| CancellationToken::default().scope(|| panic!("test")));
            assert!(check_cancelled().is_err());
        });
        assert!(check_cancelled().is_ok());
    }

    #[cfg(feature = "rational-lp")]
    #[test]
    fn exact_control_observes_later_external_cancellation() {
        let token = CancellationToken::default();
        let control = token.scope(|| {
            exact_control(std::time::Instant::now() + std::time::Duration::from_secs(60))
        });
        assert!(!control.is_cancelled());
        token.cancel();
        assert!(control.is_cancelled());
    }
}
