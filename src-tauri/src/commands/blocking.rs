//! Bound queued and active on-demand work before entering Tauri's worker pool.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, TryLockError};
use tm_core::errors::{AppError, AppResult};

static ACTIVE: AtomicUsize = AtomicUsize::new(0);
const MAX_ACTIVE: usize = 8;

/// Native dialogs wait on the user, possibly for minutes, so they do not
/// count toward [`MAX_ACTIVE`]; instead only one is open at a time.
static DIALOG: Mutex<()> = Mutex::new(());

struct Permit;
impl Permit {
    fn acquire() -> AppResult<Self> {
        // A CAS loop rather than `fetch_update`, which newer Rust deprecates
        // for `try_update` (stable only from 1.95; our minimum is 1.90).
        let mut n = ACTIVE.load(Ordering::Relaxed);
        loop {
            if n >= MAX_ACTIVE {
                return Err(AppError::OperationBusy);
            }
            match ACTIVE.compare_exchange_weak(n, n + 1, Ordering::AcqRel, Ordering::Relaxed) {
                Ok(_) => return Ok(Self),
                Err(current) => n = current,
            }
        }
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Held while a native dialog is shown. Busy if another one is open.
pub fn dialog_slot() -> AppResult<MutexGuard<'static, ()>> {
    match DIALOG.try_lock() {
        Ok(guard) => Ok(guard),
        Err(TryLockError::Poisoned(e)) => Ok(e.into_inner()),
        Err(TryLockError::WouldBlock) => Err(AppError::OperationBusy),
    }
}

pub async fn run<T: Send + 'static>(
    f: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    let permit = Permit::acquire()?;
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        f()
    })
    .await
    .map_err(|_| AppError::SystemQueryFailed("Background operation failed.".into()))?
}

/// Runs `f`, which shows a native file dialog, holding the dialog slot
/// rather than a worker permit.
pub async fn dialog<T: Send + 'static>(
    f: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || {
        let _slot = dialog_slot()?;
        f()
    })
    .await
    .map_err(|_| AppError::SystemQueryFailed("Background operation failed.".into()))?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_excess_work_and_releases_capacity_on_drop() {
        let permits: Vec<_> = (0..MAX_ACTIVE)
            .map(|_| Permit::acquire().unwrap())
            .collect();
        assert!(Permit::acquire().is_err());
        drop(permits);
        assert!(Permit::acquire().is_ok());
    }

    #[test]
    fn one_dialog_at_a_time_independent_of_the_worker_cap() {
        let permits: Vec<_> = (0..MAX_ACTIVE)
            .map(|_| Permit::acquire().unwrap())
            .collect();
        let slot = dialog_slot().expect("dialogs are not limited by busy workers");
        assert!(matches!(dialog_slot(), Err(AppError::OperationBusy)));
        drop(slot);
        assert!(dialog_slot().is_ok());
        drop(permits);
    }
}
