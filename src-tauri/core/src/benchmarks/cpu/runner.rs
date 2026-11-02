//! Runs a kernel on N worker threads with synchronized iterations.
//!
//! ```text
//! warmup   : workers run freely for the warmup time; the work rate of the
//!            second half calibrates the fixed work per iteration
//! iteration: start barrier → every worker runs the same number of chunks
//!            → end barrier. Time is measured between the barriers on the
//!            coordinating thread, so thread start-up skew is excluded and
//!            the slowest worker bounds the iteration (no partial credit).
//! ```
//!
//! Workers check cancellation between chunks; the coordinating thread
//! makes all stop decisions, so barrier participation always matches and a
//! cancelled run can never deadlock.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Barrier, Condvar, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use super::kernels::Kernel;
use crate::benchmarks::cancellation::CancelToken;
use crate::errors::{AppError, AppResult};

/// Creates a worker's kernel (called on the worker thread).
pub type KernelFactory<'a> = &'a (dyn Fn(usize) -> Box<dyn Kernel> + Sync);

#[derive(Debug, Clone)]
pub struct IterationResult {
    /// Total work of all workers divided by the iteration wall time.
    pub work_per_sec: f64,
    pub elapsed: Duration,
    /// Each worker's own work rate during the iteration.
    pub per_worker: Vec<f64>,
}

#[derive(Debug, Clone)]
pub struct RunPlan {
    pub workers: usize,
    pub warmup: Duration,
    pub iterations: u32,
    /// Target duration of one iteration; the fixed work per iteration is
    /// derived from the warmup rate.
    pub target_iteration: Duration,
}

/// Runs `plan` and calls `on_iteration` (on the calling thread) after each
/// measured iteration. Returns all iteration results in order.
pub fn run(
    plan: &RunPlan,
    make: KernelFactory<'_>,
    cancel: &CancelToken,
    mut on_iteration: impl FnMut(u32, &IterationResult),
) -> AppResult<Vec<IterationResult>> {
    let workers = plan.workers.max(1);
    let start = Barrier::new(workers + 1);
    let end = Barrier::new(workers + 1);
    let stop = AtomicBool::new(false);
    let chunks = AtomicU64::new(0);
    let warmup = plan.warmup;
    let gate = Gate::default();

    thread::scope(|scope| {
        let (tx, rx) = mpsc::channel::<(usize, u64, f64)>();
        for idx in 0..workers {
            let tx = tx.clone();
            let (start, end, stop, chunks, gate) = (&start, &end, &stop, &chunks, &gate);
            let spawned = thread::Builder::new()
                .name(format!("benchmark-cpu-{idx}"))
                .spawn_scoped(scope, move || {
                    if !gate.wait() {
                        return;
                    }
                    let mut kernel = make(idx);
                    // Warmup: run freely, report the rate of the second half.
                    start.wait();
                    let began = Instant::now();
                    let half = began + warmup / 2;
                    let (mut counted, mut measured_from) = (0u64, None);
                    while began.elapsed() < warmup && !cancel.is_cancelled() {
                        kernel.chunk();
                        let now = Instant::now();
                        if now >= half {
                            measured_from.get_or_insert(now);
                            counted += 1;
                        }
                    }
                    let secs = measured_from.map_or(0.0, |t: Instant| t.elapsed().as_secs_f64());
                    let _ = tx.send((idx, counted, secs));
                    end.wait();
                    loop {
                        start.wait();
                        if stop.load(Ordering::SeqCst) {
                            break;
                        }
                        let n = chunks.load(Ordering::SeqCst);
                        let t0 = Instant::now();
                        let mut work = 0u64;
                        for _ in 0..n {
                            if cancel.is_cancelled() {
                                break;
                            }
                            work += kernel.chunk();
                        }
                        let _ = tx.send((idx, work, t0.elapsed().as_secs_f64()));
                        end.wait();
                    }
                });
            if let Err(e) = spawned {
                // Workers already spawned are parked at the gate; let them exit.
                gate.open(false);
                return Err(AppError::SystemQueryFailed(format!(
                    "cannot start CPU worker thread: {e}"
                )));
            }
        }
        drop(tx);
        gate.open(true);

        // Warmup and calibration.
        start.wait();
        end.wait();
        // Chunk rate (chunks per second) of each worker during warmup.
        let mut rates: Vec<f64> = Vec::with_capacity(workers);
        for _ in 0..workers {
            if let Ok((_, counted, secs)) = rx.recv()
                && secs > 0.0
            {
                rates.push(counted as f64 / secs);
            }
        }
        let release = |stop: &AtomicBool| {
            stop.store(true, Ordering::SeqCst);
            start.wait();
        };
        if let Err(e) = cancel.check() {
            release(&stop);
            return Err(e);
        }
        rates.sort_by(f64::total_cmp);
        let chunk_rate = rates.get(rates.len() / 2).copied().unwrap_or(0.0);
        let n = (chunk_rate * plan.target_iteration.as_secs_f64())
            .round()
            .max(1.0);
        chunks.store(n as u64, Ordering::SeqCst);

        let mut results = Vec::with_capacity(plan.iterations as usize);
        for i in 0..plan.iterations {
            if let Err(e) = cancel.check() {
                release(&stop);
                return Err(e);
            }
            start.wait();
            let t0 = Instant::now();
            end.wait();
            let elapsed = t0.elapsed();
            let mut total = 0u64;
            let mut per_worker = vec![0.0; workers];
            for _ in 0..workers {
                if let Ok((idx, work, secs)) = rx.recv() {
                    total += work;
                    if let Some(slot) = per_worker.get_mut(idx)
                        && secs > 0.0
                    {
                        *slot = work as f64 / secs;
                    }
                }
            }
            if let Err(e) = cancel.check() {
                release(&stop);
                return Err(e);
            }
            let result = IterationResult {
                work_per_sec: total as f64 / elapsed.as_secs_f64().max(1e-9),
                elapsed,
                per_worker,
            };
            on_iteration(i + 1, &result);
            results.push(result);
        }
        release(&stop);
        Ok(results)
    })
}

/// One-shot go/no-go signal: workers park here until every thread has been
/// spawned, so a failed spawn can never strand others at a barrier.
#[derive(Default)]
struct Gate {
    state: Mutex<Option<bool>>,
    cv: Condvar,
}

impl Gate {
    fn open(&self, go: bool) {
        *self.state.lock().unwrap_or_else(PoisonError::into_inner) = Some(go);
        self.cv.notify_all();
    }

    fn wait(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(go) = *state {
                return go;
            }
            state = self.cv.wait(state).unwrap_or_else(PoisonError::into_inner);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmarks::cancellation::CancelReason;
    use crate::benchmarks::cpu::kernels::IntegerKernel;

    fn plan(workers: usize, iterations: u32) -> RunPlan {
        RunPlan {
            workers,
            warmup: Duration::from_millis(40),
            iterations,
            target_iteration: Duration::from_millis(20),
        }
    }

    fn make(idx: usize) -> Box<dyn Kernel> {
        Box::new(IntegerKernel::new(idx as u64 + 1))
    }

    #[test]
    fn runs_requested_iterations_on_all_workers() {
        let cancel = CancelToken::new(Duration::from_secs(60));
        let mut seen = Vec::new();
        let results = run(&plan(3, 4), &make, &cancel, |i, _| seen.push(i)).unwrap();
        assert_eq!(results.len(), 4);
        assert_eq!(seen, vec![1, 2, 3, 4]);
        for r in &results {
            assert_eq!(r.per_worker.len(), 3);
            assert!(r.work_per_sec > 0.0);
            assert!(r.per_worker.iter().all(|w| *w > 0.0));
        }
    }

    #[test]
    fn cancellation_returns_promptly_without_deadlock() {
        let cancel = CancelToken::new(Duration::from_secs(60));
        let started = Instant::now();
        let p = RunPlan {
            iterations: 1_000,
            ..plan(4, 1_000)
        };
        let err = run(&p, &make, &cancel, |i, _| {
            if i == 2 {
                cancel.cancel(CancelReason::User);
            }
        })
        .unwrap_err();
        assert!(matches!(err, AppError::BenchmarkCancelled));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn cancelled_during_warmup_stops() {
        let cancel = CancelToken::new(Duration::from_secs(60));
        cancel.cancel(CancelReason::User);
        assert!(run(&plan(2, 3), &make, &cancel, |_, _| {}).is_err());
    }
}
