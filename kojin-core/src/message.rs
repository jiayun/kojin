use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::state::TaskState;
use crate::task_id::TaskId;

/// A task message that flows through the broker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMessage {
    /// Unique task identifier.
    pub id: TaskId,
    /// Registered task name (e.g., "send_email").
    pub task_name: String,
    /// Target queue name.
    pub queue: String,
    /// Serialized task payload.
    pub payload: serde_json::Value,
    /// Current task state.
    pub state: TaskState,
    /// Current retry count.
    pub retries: u32,
    /// Maximum allowed retries.
    pub max_retries: u32,
    /// When the message was created.
    pub created_at: DateTime<Utc>,
    /// When the message was last updated.
    pub updated_at: DateTime<Utc>,
    /// Optional ETA — earliest time the task should execute.
    pub eta: Option<DateTime<Utc>>,
    /// Arbitrary headers for middleware / tracing propagation.
    pub headers: HashMap<String, String>,
}

impl TaskMessage {
    /// Create a new task message with defaults.
    pub fn new(
        task_name: impl Into<String>,
        queue: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: TaskId::new(),
            task_name: task_name.into(),
            queue: queue.into(),
            payload,
            state: TaskState::Pending,
            retries: 0,
            max_retries: 3,
            created_at: now,
            updated_at: now,
            eta: None,
            headers: HashMap::new(),
        }
    }

    /// Set max retries.
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set ETA.
    pub fn with_eta(mut self, eta: DateTime<Utc>) -> Self {
        self.eta = Some(eta);
        self
    }

    /// Add a header.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_message_serde_roundtrip() {
        let msg = TaskMessage::new(
            "send_email",
            "default",
            serde_json::json!({"to": "a@b.com"}),
        )
        .with_max_retries(5)
        .with_header("trace_id", "abc123");

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: TaskMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(msg.id, deserialized.id);
        assert_eq!(msg.task_name, deserialized.task_name);
        assert_eq!(msg.queue, deserialized.queue);
        assert_eq!(msg.max_retries, deserialized.max_retries);
        assert_eq!(msg.headers.get("trace_id"), Some(&"abc123".to_string()));
    }

    #[test]
    fn task_message_defaults() {
        let msg = TaskMessage::new("test", "default", serde_json::Value::Null);
        assert_eq!(msg.state, TaskState::Pending);
        assert_eq!(msg.retries, 0);
        assert_eq!(msg.max_retries, 3);
        assert!(msg.eta.is_none());
        assert!(msg.headers.is_empty());
    }
}
