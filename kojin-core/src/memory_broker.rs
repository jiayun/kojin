use async_trait::async_trait;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

use crate::broker::Broker;
use crate::error::TaskResult;
use crate::message::TaskMessage;
use crate::task_id::TaskId;

/// In-memory broker for testing and development.
#[derive(Clone)]
pub struct MemoryBroker {
    inner: Arc<MemoryBrokerInner>,
}

struct MemoryBrokerInner {
    queues: Mutex<HashMap<String, VecDeque<TaskMessage>>>,
    dlq: Mutex<HashMap<String, VecDeque<TaskMessage>>>,
    processing: Mutex<HashMap<TaskId, TaskMessage>>,
    notify: Notify,
}

impl MemoryBroker {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(MemoryBrokerInner {
                queues: Mutex::new(HashMap::new()),
                dlq: Mutex::new(HashMap::new()),
                processing: Mutex::new(HashMap::new()),
                notify: Notify::new(),
            }),
        }
    }

    /// Get the dead-letter queue contents for testing.
    pub async fn dlq_len(&self, queue: &str) -> usize {
        let dlq = self.inner.dlq.lock().await;
        dlq.get(queue).map_or(0, |q| q.len())
    }
}

impl Default for MemoryBroker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Broker for MemoryBroker {
    async fn enqueue(&self, message: TaskMessage) -> TaskResult<()> {
        let mut queues = self.inner.queues.lock().await;
        queues
            .entry(message.queue.clone())
            .or_default()
            .push_back(message);
        self.inner.notify.notify_one();
        Ok(())
    }

    async fn dequeue(
        &self,
        queues: &[String],
        timeout: std::time::Duration,
    ) -> TaskResult<Option<TaskMessage>> {
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            // Try to pop from any of the requested queues
            {
                let mut q = self.inner.queues.lock().await;
                for queue_name in queues {
                    if let Some(queue) = q.get_mut(queue_name) {
                        if let Some(msg) = queue.pop_front() {
                            // Track in processing
                            self.inner
                                .processing
                                .lock()
                                .await
                                .insert(msg.id, msg.clone());
                            return Ok(Some(msg));
                        }
                    }
                }
            }

            // Wait for notification or timeout
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }

            tokio::select! {
                _ = self.inner.notify.notified() => continue,
                _ = tokio::time::sleep(remaining) => return Ok(None),
            }
        }
    }

    async fn ack(&self, id: &TaskId) -> TaskResult<()> {
        self.inner.processing.lock().await.remove(id);
        Ok(())
    }

    async fn nack(&self, message: TaskMessage) -> TaskResult<()> {
        self.inner.processing.lock().await.remove(&message.id);
        // Re-enqueue
        self.enqueue(message).await
    }

    async fn dead_letter(&self, message: TaskMessage) -> TaskResult<()> {
        self.inner.processing.lock().await.remove(&message.id);
        let dlq_name = message.queue.clone();
        let mut dlq = self.inner.dlq.lock().await;
        dlq.entry(dlq_name).or_default().push_back(message);
        Ok(())
    }

    async fn schedule(
        &self,
        message: TaskMessage,
        _eta: chrono::DateTime<chrono::Utc>,
    ) -> TaskResult<()> {
        // For MemoryBroker, just enqueue immediately (no scheduled queue support)
        self.enqueue(message).await
    }

    async fn queue_len(&self, queue: &str) -> TaskResult<usize> {
        let queues = self.inner.queues.lock().await;
        Ok(queues.get(queue).map_or(0, |q| q.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn enqueue_dequeue_fifo() {
        let broker = MemoryBroker::new();
        let msg1 = TaskMessage::new("task1", "default", serde_json::json!(1));
        let msg2 = TaskMessage::new("task2", "default", serde_json::json!(2));

        broker.enqueue(msg1.clone()).await.unwrap();
        broker.enqueue(msg2.clone()).await.unwrap();

        let queues = vec!["default".to_string()];
        let out1 = broker
            .dequeue(&queues, Duration::from_secs(1))
            .await
            .unwrap()
            .unwrap();
        let out2 = broker
            .dequeue(&queues, Duration::from_secs(1))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(out1.task_name, "task1");
        assert_eq!(out2.task_name, "task2");
    }

    #[tokio::test]
    async fn dequeue_timeout_returns_none() {
        let broker = MemoryBroker::new();
        let queues = vec!["default".to_string()];
        let result = broker
            .dequeue(&queues, Duration::from_millis(50))
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn ack_removes_from_processing() {
        let broker = MemoryBroker::new();
        let msg = TaskMessage::new("task1", "default", serde_json::json!(1));
        let id = msg.id;
        broker.enqueue(msg).await.unwrap();

        let queues = vec!["default".to_string()];
        let _out = broker
            .dequeue(&queues, Duration::from_secs(1))
            .await
            .unwrap()
            .unwrap();
        assert!(broker.inner.processing.lock().await.contains_key(&id));

        broker.ack(&id).await.unwrap();
        assert!(!broker.inner.processing.lock().await.contains_key(&id));
    }

    #[tokio::test]
    async fn nack_requeues() {
        let broker = MemoryBroker::new();
        let msg = TaskMessage::new("task1", "default", serde_json::json!(1));
        broker.enqueue(msg).await.unwrap();

        let queues = vec!["default".to_string()];
        let out = broker
            .dequeue(&queues, Duration::from_secs(1))
            .await
            .unwrap()
            .unwrap();
        broker.nack(out).await.unwrap();

        assert_eq!(broker.queue_len("default").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn dead_letter() {
        let broker = MemoryBroker::new();
        let msg = TaskMessage::new("task1", "default", serde_json::json!(1));
        broker.enqueue(msg).await.unwrap();

        let queues = vec!["default".to_string()];
        let out = broker
            .dequeue(&queues, Duration::from_secs(1))
            .await
            .unwrap()
            .unwrap();
        broker.dead_letter(out).await.unwrap();

        assert_eq!(broker.queue_len("default").await.unwrap(), 0);
        assert_eq!(broker.dlq_len("default").await, 1);
    }

    #[tokio::test]
    async fn queue_len() {
        let broker = MemoryBroker::new();
        assert_eq!(broker.queue_len("default").await.unwrap(), 0);

        broker
            .enqueue(TaskMessage::new("t", "default", serde_json::json!(1)))
            .await
            .unwrap();
        broker
            .enqueue(TaskMessage::new("t", "default", serde_json::json!(2)))
            .await
            .unwrap();
        assert_eq!(broker.queue_len("default").await.unwrap(), 2);
    }
}
