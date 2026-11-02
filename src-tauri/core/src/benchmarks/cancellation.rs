//! Cooperative cancellation. Workloads call [`CancelToken::check`] between
//! bounded chunks of work (never more than ~100 ms apart), so cancelling
//! stops new work promptly and lets RAII guards release resources normally.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{Duration, Instant};

use crate::errors::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelReason {
    User,
    Timeout,
    Shutdown,
}

const NONE: u8 = 0;

impl CancelReason {
    const fn code(self) -> u8 {
        match self {
            Self::User => 1,
            Self::Timeout => 2,
            Self::Shutdown => 3,
        }
    }

    const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::User),
            2 => Some(Self::Timeout),
            3 => Some(Self::Shutdown),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct CancelToken {
    reason: AtomicU8,
    skip_cooldown: AtomicBool,
    deadline: Instant,
}

impl CancelToken {
    pub fn new(timeout: Duration) -> Self {
        Self {
            reason: AtomicU8::new(NONE),
            skip_cooldown: AtomicBool::new(false),
            deadline: Instant::now() + timeout,
        }
    }

    /// Requests cancellation. The first reason wins.
    pub fn cancel(&self, reason: CancelReason) {
        let _ =
            self.reason
                .compare_exchange(NONE, reason.code(), Ordering::SeqCst, Ordering::SeqCst);
    }

    pub fn reason(&self) -> Option<CancelReason> {
        if let Some(r) = CancelReason::from_code(self.reason.load(Ordering::SeqCst)) {
            return Some(r);
        }
        if Instant::now() >= self.deadline {
            self.cancel(CancelReason::Timeout);
            return Some(CancelReason::Timeout);
        }
        None
    }

    pub fn is_cancelled(&self) -> bool {
        self.reason().is_some()
    }

    /// `Err` once cancelled or past the run deadline.
    pub fn check(&self) -> AppResult<()> {
        match self.reason() {
            None => Ok(()),
            Some(CancelReason::Timeout) => Err(AppError::InvalidBenchmarkConfiguration(
                "The benchmark exceeded its maximum duration and was stopped.".into(),
            )),
            Some(_) => Err(AppError::BenchmarkCancelled),
        }
    }

    /// Sleeps up to `duration`, waking early on cancellation.
    pub fn sleep(&self, duration: Duration) -> AppResult<()> {
        let end = Instant::now() + duration;
        loop {
            self.check()?;
            let now = Instant::now();
            if now >= end {
                return Ok(());
            }
            std::thread::sleep((end - now).min(Duration::from_millis(50)));
        }
    }

    pub fn skip_cooldown(&self) {
        self.skip_cooldown.store(true, Ordering::SeqCst);
    }

    /// Consumes a pending skip request.
    pub fn take_skip_cooldown(&self) -> bool {
        self.skip_cooldown.swap(false, Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_reason_wins() {
        let t = CancelToken::new(Duration::from_secs(60));
        assert!(t.check().is_ok());
        t.cancel(CancelReason::User);
        t.cancel(CancelReason::Shutdown);
        assert_eq!(t.reason(), Some(CancelReason::User));
        assert!(matches!(t.check(), Err(AppError::BenchmarkCancelled)));
    }

    #[test]
    fn deadline_cancels_with_timeout() {
        let t = CancelToken::new(Duration::ZERO);
        assert_eq!(t.reason(), Some(CancelReason::Timeout));
        assert!(matches!(
            t.check(),
            Err(AppError::InvalidBenchmarkConfiguration(_))
        ));
    }

    #[test]
    fn sleep_wakes_on_cancel() {
        let t = std::sync::Arc::new(CancelToken::new(Duration::from_secs(60)));
        let t2 = std::sync::Arc::clone(&t);
        let start = Instant::now();
        let h = std::thread::spawn(move || t2.sleep(Duration::from_secs(30)));
        std::thread::sleep(Duration::from_millis(100));
        t.cancel(CancelReason::User);
        assert!(h.join().unwrap().is_err());
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn skip_cooldown_is_consumed_once() {
        let t = CancelToken::new(Duration::from_secs(60));
        assert!(!t.take_skip_cooldown());
        t.skip_cooldown();
        assert!(t.take_skip_cooldown());
        assert!(!t.take_skip_cooldown());
    }
}
