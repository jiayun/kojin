use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::message::TaskMessage;

/// A type-erased task invocation descriptor.
///
/// Signatures describe *what* to run without running it immediately.
/// They can be composed into workflows via [`Canvas`](crate::canvas::Canvas).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    /// Registered task name.
    pub task_name: String,
    /// Target queue.
    pub queue: String,
    /// Serialized task payload.
    pub payload: serde_json::Value,
    /// Maximum retries.
    pub max_retries: u32,
    /// Optional ETA.
    pub eta: Option<DateTime<Utc>>,
    /// Arbitrary headers.
    pub headers: HashMap<String, String>,
}

impl Signature {
    /// Create a new signature.
    pub fn new(
        task_name: impl Into<String>,
        queue: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            task_name: task_name.into(),
            queue: queue.into(),
            payload,
            max_retries: 3,
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

    /// Convert this signature into a [`TaskMessage`] ready for enqueuing.
    pub fn into_message(self) -> TaskMessage {
        let mut msg = TaskMessage::new(self.task_name, self.queue, self.payload)
            .with_max_retries(self.max_retries);
        msg.eta = self.eta;
        msg.headers = self.headers;
        msg
    }
}

impl std::ops::BitOr for Signature {
    type Output = crate::canvas::Canvas;

    fn bitor(self, rhs: Self) -> Self::Output {
        crate::canvas::Canvas::Chain(vec![
            crate::canvas::Canvas::Single(self),
            crate::canvas::Canvas::Single(rhs),
        ])
    }
}

impl From<Signature> for crate::canvas::Canvas {
    fn from(sig: Signature) -> Self {
        crate::canvas::Canvas::Single(sig)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_to_message() {
        let sig = Signature::new("add", "math", serde_json::json!({"a": 1, "b": 2}))
            .with_max_retries(5)
            .with_header("trace", "t1");

        let msg = sig.into_message();
        assert_eq!(msg.task_name, "add");
        assert_eq!(msg.queue, "math");
        assert_eq!(msg.max_retries, 5);
        assert_eq!(msg.headers.get("trace"), Some(&"t1".to_string()));
    }

    #[test]
    fn pipe_operator_creates_chain() {
        let s1 = Signature::new("a", "q", serde_json::json!({}));
        let s2 = Signature::new("b", "q", serde_json::json!({}));
        let canvas = s1 | s2;
        assert!(matches!(canvas, crate::canvas::Canvas::Chain(_)));
    }
}
