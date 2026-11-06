//! Hand-off point between the WebView provider and the benchmark runner.
//!
//! The runner opens the inbox with a fresh random token and sends it to the
//! WebView in the provider request. Only messages carrying that token are
//! accepted, and only while the run is waiting, so stale or forged
//! submissions are rejected.

use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::benchmarks::models::{NetworkProgress, NetworkSubmission};
use crate::errors::{AppError, AppResult};

pub enum InboxMessage {
    Progress(NetworkProgress),
    Done(Box<NetworkSubmission>),
}

#[derive(Default)]
pub struct NetworkInbox {
    slot: Mutex<Option<(String, SyncSender<InboxMessage>)>>,
}

impl NetworkInbox {
    fn slot(&self) -> MutexGuard<'_, Option<(String, SyncSender<InboxMessage>)>> {
        self.slot.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts waiting for a provider; replaces any previous wait.
    pub fn open(&self) -> (String, Receiver<InboxMessage>) {
        let token = uuid::Uuid::new_v4().to_string();
        // Progress is expendable. Bound queued updates even if a WebView sends
        // faster than the runner can consume them.
        let (tx, rx) = mpsc::sync_channel(8);
        *self.slot() = Some((token.clone(), tx));
        (token, rx)
    }

    pub fn close(&self) {
        *self.slot() = None;
    }

    fn send(&self, token: &str, msg: InboxMessage, last: bool) -> AppResult<()> {
        let tx = {
            let mut slot = self.slot();
            let Some((_, tx)) = slot.as_ref().filter(|(expected, _)| expected == token) else {
                return Err(AppError::InvalidArgument(
                    "No internet test is waiting for these results.".into(),
                ));
            };
            let tx = tx.clone();
            if last {
                *slot = None;
            }
            tx
        };
        // Never hold the inbox lock while waiting for space: close/cancellation
        // must be able to drop the receiver and wake a final submission.
        if last {
            tx.send(msg).map_err(|_| {
                AppError::ProviderUnavailable(
                    "The internet test is no longer receiving results.".into(),
                )
            })
        } else {
            match tx.try_send(msg) {
                Ok(()) | Err(TrySendError::Full(_)) => Ok(()),
                Err(TrySendError::Disconnected(_)) => Err(AppError::ProviderUnavailable(
                    "The internet test is no longer receiving progress.".into(),
                )),
            }
        }
    }

    pub fn progress(&self, token: &str, progress: NetworkProgress) -> AppResult<()> {
        let mut progress = progress;
        progress.fraction = if progress.fraction.is_finite() {
            progress.fraction.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mut end = progress.label.len().min(100);
        while !progress.label.is_char_boundary(end) {
            end -= 1;
        }
        progress.label.truncate(end);
        progress.live.truncate(64);
        progress
            .live
            .retain(|v| v.value.is_finite() && v.series.len() <= 64);
        self.send(token, InboxMessage::Progress(progress), false)
    }

    pub fn submit(&self, token: &str, submission: NetworkSubmission) -> AppResult<()> {
        self.send(token, InboxMessage::Done(Box::new(submission)), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_progress_is_bounded_without_panicking() {
        let inbox = NetworkInbox::default();
        let (token, rx) = inbox.open();
        inbox
            .progress(
                &token,
                NetworkProgress {
                    fraction: 0.5,
                    label: "界".repeat(40),
                    live: Vec::new(),
                },
            )
            .unwrap();
        let InboxMessage::Progress(progress) = rx.recv().unwrap() else {
            panic!("expected progress");
        };
        assert!(progress.label.len() <= 100);
        assert!(progress.label.chars().all(|c| c == '界'));
    }

    #[test]
    fn accepts_only_the_current_token_once() {
        let inbox = NetworkInbox::default();
        assert!(inbox.submit("nope", NetworkSubmission::default()).is_err());
        let (token, rx) = inbox.open();
        assert!(
            inbox
                .submit("forged", NetworkSubmission::default())
                .is_err()
        );
        inbox
            .progress(
                &token,
                NetworkProgress {
                    fraction: 7.0,
                    label: "x".repeat(500),
                    live: Vec::new(),
                },
            )
            .unwrap();
        inbox.submit(&token, NetworkSubmission::default()).unwrap();
        assert!(
            inbox.submit(&token, NetworkSubmission::default()).is_err(),
            "single use"
        );
        match rx.recv().unwrap() {
            InboxMessage::Progress(p) => {
                assert_eq!(p.fraction, 1.0);
                assert_eq!(p.label.len(), 100);
            }
            InboxMessage::Done(_) => panic!("progress first"),
        }
        assert!(matches!(rx.recv().unwrap(), InboxMessage::Done(_)));
    }

    #[test]
    fn close_rejects_late_submissions() {
        let inbox = NetworkInbox::default();
        let (token, _rx) = inbox.open();
        inbox.close();
        assert!(inbox.submit(&token, NetworkSubmission::default()).is_err());
    }

    #[test]
    fn progress_flood_is_bounded_and_final_submission_survives() {
        let inbox = NetworkInbox::default();
        let (token, rx) = inbox.open();
        for _ in 0..10_000 {
            inbox
                .progress(
                    &token,
                    NetworkProgress {
                        fraction: f32::NAN,
                        label: String::new(),
                        live: Vec::new(),
                    },
                )
                .unwrap();
        }
        let queued: Vec<_> = rx.try_iter().collect();
        assert_eq!(queued.len(), 8);
        assert!(
            queued
                .iter()
                .all(|m| matches!(m, InboxMessage::Progress(p) if p.fraction == 0.0))
        );
        inbox.submit(&token, NetworkSubmission::default()).unwrap();
        assert!(matches!(rx.recv().unwrap(), InboxMessage::Done(_)));
    }
}
