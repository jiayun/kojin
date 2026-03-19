use async_trait::async_trait;

use crate::error::TaskResult;
use crate::message::TaskMessage;
use crate::task_id::TaskId;

/// Message broker responsible for enqueuing and dequeuing task messages.
#[async_trait]
pub trait Broker: Send + Sync + 'static {
    /// Push a message onto a queue.
    async fn enqueue(&self, message: TaskMessage) -> TaskResult<()>;

    /// Blocking dequeue from one of the given queues.
    /// Returns `None` if shutdown is signaled or timeout occurs.
    async fn dequeue(
        &self,
        queues: &[String],
        timeout: std::time::Duration,
    ) -> TaskResult<Option<TaskMessage>>;

    /// Acknowledge successful processing — remove from processing queue.
    async fn ack(&self, id: &TaskId) -> TaskResult<()>;

    /// Negative acknowledge — message will be re-enqueued or dead-lettered.
    async fn nack(&self, message: TaskMessage) -> TaskResult<()>;

    /// Move a message to the dead-letter queue.
    async fn dead_letter(&self, message: TaskMessage) -> TaskResult<()>;

    /// Schedule a message for future delivery.
    async fn schedule(
        &self,
        message: TaskMessage,
        eta: chrono::DateTime<chrono::Utc>,
    ) -> TaskResult<()>;

    /// Get the length of a queue.
    async fn queue_len(&self, queue: &str) -> TaskResult<usize>;

    /// Get the number of messages in the dead-letter queue for a given queue.
    async fn dlq_len(&self, queue: &str) -> TaskResult<usize> {
        let _ = queue;
        Ok(0)
    }

    /// List all known queue names.
    async fn list_queues(&self) -> TaskResult<Vec<String>> {
        Ok(Vec::new())
    }

    /// Retrieve messages from the dead-letter queue with pagination.
    async fn dlq_messages(
        &self,
        queue: &str,
        offset: usize,
        limit: usize,
    ) -> TaskResult<Vec<TaskMessage>> {
        let _ = (queue, offset, limit);
        Ok(Vec::new())
    }
}
