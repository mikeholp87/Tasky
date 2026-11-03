//! Bound queued and active on-demand work before entering Tauri's worker pool.

use std::sync::atomic::{AtomicUsize, Ordering};
use tm_core::errors::{AppError, AppResult};

static ACTIVE: AtomicUsize = AtomicUsize::new(0);
const MAX_ACTIVE: usize = 8;

struct Permit;
impl Permit {
    fn acquire() -> AppResult<Self> {
        ACTIVE
            .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |n| {
                (n < MAX_ACTIVE).then_some(n + 1)
            })
            .map(|_| Self)
            .map_err(|_| AppError::OperationBusy)
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
