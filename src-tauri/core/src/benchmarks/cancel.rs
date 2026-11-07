//! One cancellation primitive shared by every benchmark.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

/// Cheap to clone and to poll (one relaxed atomic load).
#[derive(Clone, Default)]
pub struct CancelToken(Arc<Cancelled>);

#[derive(Default)]
struct Cancelled {
    flag: AtomicBool,
    shutdown: Mutex<Vec<Box<dyn FnOnce() + Send>>>,
}

impl std::fmt::Debug for CancelToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CancelToken")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        let callbacks = {
            let mut callbacks = self
                .0
                .shutdown
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            self.0.flag.store(true, Ordering::Relaxed);
            std::mem::take(&mut *callbacks)
        };
        for shutdown in callbacks {
            shutdown();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.flag.load(Ordering::Relaxed)
    }

    /// Interrupts blocking resources; late registration shuts down immediately.
    pub fn on_cancel(&self, shutdown: impl FnOnce() + Send + 'static) {
        let mut callbacks = self
            .0
            .shutdown
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self.is_cancelled() {
            drop(callbacks);
            shutdown();
        } else {
            callbacks.push(Box::new(shutdown));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_state() {
        let a = CancelToken::new();
        let b = a.clone();
        assert!(!b.is_cancelled());
        a.cancel();
        assert!(b.is_cancelled());
    }

    #[test]
    fn shutdown_callbacks_run_once_including_late_and_racing_registration() {
        let token = CancelToken::new();
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let count = Arc::clone(&count);
                let token = token.clone();
                scope.spawn(move || {
                    token.on_cancel(move || {
                        count.fetch_add(1, Ordering::Relaxed);
                    })
                });
            }
            token.cancel();
        });
        let late = Arc::clone(&count);
        token.on_cancel(move || {
            late.fetch_add(1, Ordering::Relaxed);
        });
        token.cancel();
        assert_eq!(count.load(Ordering::Relaxed), 9);
    }

    #[test]
    fn shutdown_callbacks_can_register_another_callback_without_deadlocking() {
        let token = CancelToken::new();
        let nested = token.clone();
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = Arc::clone(&count);
        token.on_cancel(move || {
            nested.on_cancel(move || {
                counted.fetch_add(1, Ordering::Relaxed);
            })
        });
        token.cancel();
        assert_eq!(count.load(Ordering::Relaxed), 1);
    }
}
