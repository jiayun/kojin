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

    // -- Workflow metadata (Phase 2) --
    /// Parent task ID for workflow tracking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<TaskId>,
    /// Correlation ID for tracing an entire workflow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    /// Group ID this task belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    /// Total number of tasks in the group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_total: Option<u32>,
    /// Chord callback to enqueue when all group members complete.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chord_callback: Option<Box<TaskMessage>>,
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
            parent_id: None,
            correlation_id: None,
            group_id: None,
            group_total: None,
            chord_callback: None,
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

    /// Set parent task ID for workflow tracking.
    pub fn with_parent_id(mut self, parent_id: TaskId) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    /// Set correlation ID for tracing an entire workflow.
    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }

    /// Set group metadata.
    pub fn with_group(mut self, group_id: impl Into<String>, group_total: u32) -> Self {
        self.group_id = Some(group_id.into());
        self.group_total = Some(group_total);
        self
    }

    /// Set chord callback.
    pub fn with_chord_callback(mut self, callback: TaskMessage) -> Self {
        self.chord_callback = Some(Box::new(callback));
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
        assert!(msg.parent_id.is_none());
        assert!(msg.correlation_id.is_none());
        assert!(msg.group_id.is_none());
        assert!(msg.group_total.is_none());
        assert!(msg.chord_callback.is_none());
    }

    #[test]
    fn backward_compat_deserialization() {
        // Simulate a v0.1.0 message without workflow fields
        let old_json = serde_json::json!({
            "id": "01234567-89ab-cdef-0123-456789abcdef",
            "task_name": "send_email",
            "queue": "default",
            "payload": {"to": "a@b.com"},
            "state": "pending",
            "retries": 0,
            "max_retries": 3,
            "created_at": "2025-01-01T00:00:00Z",
            "updated_at": "2025-01-01T00:00:00Z",
            "eta": null,
            "headers": {}
        });
        let msg: TaskMessage = serde_json::from_value(old_json).unwrap();
        assert_eq!(msg.task_name, "send_email");
        assert!(msg.parent_id.is_none());
        assert!(msg.correlation_id.is_none());
        assert!(msg.group_id.is_none());
        assert!(msg.group_total.is_none());
        assert!(msg.chord_callback.is_none());
    }

    #[test]
    fn workflow_metadata_roundtrip() {
        let callback = TaskMessage::new("callback", "default", serde_json::json!({}));
        let msg = TaskMessage::new("task", "default", serde_json::json!({}))
            .with_parent_id(TaskId::new())
            .with_correlation_id("corr-123")
            .with_group("group-1", 5)
            .with_chord_callback(callback);

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: TaskMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(msg.parent_id, deserialized.parent_id);
        assert_eq!(msg.correlation_id, deserialized.correlation_id);
        assert_eq!(msg.group_id, deserialized.group_id);
        assert_eq!(msg.group_total, deserialized.group_total);
        assert!(deserialized.chord_callback.is_some());
        assert_eq!(deserialized.chord_callback.unwrap().task_name, "callback");
    }
}
