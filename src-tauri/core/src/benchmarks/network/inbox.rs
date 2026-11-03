//! Hand-off point between the WebView provider and the benchmark runner.
//!
//! The runner opens the inbox with a fresh random token and sends it to the
//! WebView in the provider request. Only messages carrying that token are
//! accepted, and only while the run is waiting, so stale or forged
//! submissions are rejected.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::benchmarks::models::{NetworkProgress, NetworkSubmission};
use crate::errors::{AppError, AppResult};

pub enum InboxMessage {
    Progress(NetworkProgress),
    Done(Box<NetworkSubmission>),
}

#[derive(Default)]
pub struct NetworkInbox {
    slot: Mutex<Option<(String, Sender<InboxMessage>)>>,
}

impl NetworkInbox {
    fn slot(&self) -> MutexGuard<'_, Option<(String, Sender<InboxMessage>)>> {
        self.slot.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts waiting for a provider; replaces any previous wait.
    pub fn open(&self) -> (String, Receiver<InboxMessage>) {
        let token = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::channel();
        *self.slot() = Some((token.clone(), tx));
        (token, rx)
    }

    pub fn close(&self) {
        *self.slot() = None;
    }

    fn send(&self, token: &str, msg: InboxMessage, last: bool) -> AppResult<()> {
        let mut slot = self.slot();
        match slot.as_ref() {
            Some((expected, tx)) if expected == token => {
                let _ = tx.send(msg);
                if last {
                    *slot = None;
                }
                Ok(())
            }
            _ => Err(AppError::InvalidArgument(
                "No internet test is waiting for these results.".into(),
            )),
        }
    }

    pub fn progress(&self, token: &str, progress: NetworkProgress) -> AppResult<()> {
        let mut progress = progress;
        progress.fraction = progress.fraction.clamp(0.0, 1.0);
        progress.label.truncate(100);
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
}
