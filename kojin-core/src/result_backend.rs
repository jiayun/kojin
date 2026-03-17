use async_trait::async_trait;

use crate::error::TaskResult;
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
}
