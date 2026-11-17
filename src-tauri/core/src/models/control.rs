use serde::{Deserialize, Serialize};

/// An action the user can take on a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ProcessAction {
    /// Ask the process to exit (Windows: close its windows; Unix: SIGTERM).
    Terminate,
    /// End immediately without letting it save (TerminateProcess / SIGKILL).
    ForceKill,
    Suspend,
    Resume,
    SetPriority {
        priority: PriorityLevel,
    },
}

impl ProcessAction {
    /// Actions that stop or freeze the process.
    pub fn is_disruptive(self) -> bool {
        matches!(self, Self::Terminate | Self::ForceKill | Self::Suspend)
    }

    /// Actions the user confirms in a native dialog. Only restoring normal
    /// priority is exempt: an unprivileged user cannot undo a lowered
    /// priority, and resuming can restart work that was stopped on purpose.
    pub fn needs_confirmation(self) -> bool {
        self != Self::SetPriority {
            priority: PriorityLevel::Normal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_restoring_normal_priority_skips_confirmation() {
        for priority in [
            PriorityLevel::Idle,
            PriorityLevel::BelowNormal,
            PriorityLevel::AboveNormal,
            PriorityLevel::High,
        ] {
            assert!(ProcessAction::SetPriority { priority }.needs_confirmation());
        }
        for action in [
            ProcessAction::Terminate,
            ProcessAction::ForceKill,
            ProcessAction::Suspend,
            ProcessAction::Resume,
        ] {
            assert!(action.needs_confirmation(), "{action:?}");
        }
        assert!(
            !ProcessAction::SetPriority {
                priority: PriorityLevel::Normal
            }
            .needs_confirmation()
        );
    }
}

/// Portable priority levels. Real-time is intentionally not offered: it can
/// starve the whole system, including input handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum PriorityLevel {
    Idle,
    BelowNormal,
    Normal,
    AboveNormal,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum ProtectionLevel {
    Allowed,
    /// Allowed only after the user acknowledges the reasons.
    Warning,
    /// Refused regardless of confirmation.
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ProtectionAssessment {
    pub level: ProtectionLevel,
    pub reasons: Vec<String>,
}

impl ProtectionAssessment {
    pub fn allowed() -> Self {
        Self {
            level: ProtectionLevel::Allowed,
            reasons: Vec::new(),
        }
    }

    pub fn add(&mut self, level: ProtectionLevel, reason: impl Into<String>) {
        self.level = self.level.max(level);
        self.reasons.push(reason.into());
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ActionOutcome {
    /// User-facing confirmation, e.g. "Sent a close request to node (PID 42)."
    pub message: String,
}
