use async_trait::async_trait;

use crate::error::{KojinError, TaskResult};
use crate::task_id::TaskId;

/// Backend for storing and retrieving task results.
#[async_trait]
pub trait ResultBackend: Send + Sync + 'static {
    /// Store a task result.
    async fn store(&self, id: &TaskId, result: &serde_json::Value) -> TaskResult<()>;

    /// Get a stored result.
    async fn get(&self, id: &TaskId) -> TaskResult<Option<serde_json::Value>>;

    /// Wait for a result to be available, with timeout.
    async fn wait(
        &self,
        id: &TaskId,
        timeout: std::time::Duration,
    ) -> TaskResult<serde_json::Value>;

    /// Delete a stored result.
    async fn delete(&self, id: &TaskId) -> TaskResult<()>;

    /// Initialize a group with the expected total count.
    async fn init_group(&self, _group_id: &str, _total: u32) -> TaskResult<()> {
        Err(KojinError::ResultBackend(
            "group operations not supported by this backend".into(),
        ))
    }

    /// Mark a group member as complete and return the number of completed members.
    async fn complete_group_member(
        &self,
        _group_id: &str,
        _task_id: &TaskId,
        _result: &serde_json::Value,
    ) -> TaskResult<u32> {
        Err(KojinError::ResultBackend(
            "group operations not supported by this backend".into(),
        ))
    }

    /// Get all results for a group.
    async fn get_group_results(&self, _group_id: &str) -> TaskResult<Vec<serde_json::Value>> {
        Err(KojinError::ResultBackend(
            "group operations not supported by this backend".into(),
        ))
    }
}
