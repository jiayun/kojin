use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Unique task identifier backed by UUID v7 (time-ordered).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(Uuid);

impl TaskId {
    /// Create a new time-ordered task ID.
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Create from an existing UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the inner UUID.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<Uuid> for TaskId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<TaskId> for Uuid {
    fn from(id: TaskId) -> Self {
        id.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_id_ordering() {
        let id1 = TaskId::new();
        let id2 = TaskId::new();
        assert!(id1 < id2, "UUID v7 should be time-ordered");
    }

    #[test]
    fn task_id_display() {
        let id = TaskId::new();
        let s = id.to_string();
        assert!(!s.is_empty());
        // UUID format: 8-4-4-4-12
        assert_eq!(s.len(), 36);
    }

    #[test]
    fn task_id_serde_roundtrip() {
        let id = TaskId::new();
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: TaskId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }
}
