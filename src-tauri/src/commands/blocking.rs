//! Bound queued and active on-demand work before entering Tauri's worker pool.

use std::sync::atomic::{AtomicUsize, Ordering};
use tm_core::errors::{AppError, AppResult};

static ACTIVE: AtomicUsize = AtomicUsize::new(0);
const MAX_ACTIVE: usize = 8;

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
}
