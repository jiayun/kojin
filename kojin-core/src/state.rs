use serde::{Deserialize, Serialize};

/// Lifecycle state of a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Pending,
    Received,
    Started,
    Success,
    Failure,
    Retry,
    Revoked,
    DeadLettered,
}

impl TaskState {
    /// Whether the task is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskState::Success | TaskState::Failure | TaskState::Revoked | TaskState::DeadLettered
        )
    }

    /// Whether the task is still in progress.
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            TaskState::Pending | TaskState::Received | TaskState::Started | TaskState::Retry
        )
    }
}

impl std::fmt::Display for TaskState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            TaskState::Pending => "pending",
            TaskState::Received => "received",
            TaskState::Started => "started",
            TaskState::Success => "success",
            TaskState::Failure => "failure",
            TaskState::Retry => "retry",
            TaskState::Revoked => "revoked",
            TaskState::DeadLettered => "dead_lettered",
        };
        f.write_str(s)
    }
}
