use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use aws_sdk_sqs::Client;
use tokio::sync::Mutex;
use tracing::warn;

use kojin_core::broker::Broker;
use kojin_core::error::{KojinError, TaskResult};
use kojin_core::message::TaskMessage;
use kojin_core::task_id::TaskId;

use crate::config::SqsConfig;

fn broker_err(e: impl std::fmt::Display) -> KojinError {
    KojinError::Broker(e.to_string())
}

/// AWS SQS broker for kojin.
///
/// Supports both standard and FIFO queues. FIFO queues are auto-detected
/// from URLs ending in `.fifo`.
///
/// # Priority
///
/// SQS does not support message priority. If a message has `priority` set,
/// a warning is logged and the field is ignored.
///
/// # Dead-letter
///
/// Prefer configuring SQS RedrivePolicy natively. The `dead_letter()` method
/// sends to the configured `dlq_url` if set, otherwise nacks the message.
#[derive(Clone)]
pub struct SqsBroker {
    inner: Arc<SqsBrokerInner>,
}

struct SqsBrokerInner {
    client: Client,
    config: SqsConfig,
    /// Maps TaskId → (queue_url, receipt_handle) for ack/nack.
    receipts: Mutex<HashMap<TaskId, (String, String)>>,
    /// Queue URL → queue name mapping (extracted from URL).
    queue_names: HashMap<String, String>,
}

impl SqsBroker {
    /// Create a new SQS broker from an AWS SDK config and SqsConfig.
    pub fn new(sdk_config: &aws_config::SdkConfig, config: SqsConfig) -> Self {
        let client = Client::new(sdk_config);

        let mut queue_names = HashMap::new();
        for url in &config.queue_urls {
            let name = queue_name_from_url(url);
            queue_names.insert(url.clone(), name);
        }

        Self {
            inner: Arc::new(SqsBrokerInner {
                client,
                config,
                receipts: Mutex::new(HashMap::new()),
                queue_names,
            }),
        }
    }

    /// Get the first queue URL matching a queue name, or fall back to the URL itself.
    fn resolve_queue_url(&self, queue: &str) -> Option<String> {
        // First try matching by name
        for (url, name) in &self.inner.queue_names {
            if name == queue {
                return Some(url.clone());
            }
        }
        // Then try if it's already a URL
        if self.inner.config.queue_urls.contains(&queue.to_string()) {
            return Some(queue.to_string());
        }
        None
    }

    fn is_fifo(url: &str) -> bool {
        url.ends_with(".fifo")
    }
}

#[async_trait]
impl Broker for SqsBroker {
    async fn enqueue(&self, message: TaskMessage) -> TaskResult<()> {
        let queue_url = self
            .resolve_queue_url(&message.queue)
            .ok_or_else(|| KojinError::QueueNotFound(message.queue.clone()))?;

        if message.priority.is_some() {
            warn!(
                task_id = %message.id,
                "SQS does not support message priority — priority field ignored"
            );
        }

        let body = serde_json::to_string(&message)?;

        let mut req = self
            .inner
            .client
            .send_message()
            .queue_url(&queue_url)
            .message_body(&body);

        // FIFO queue support
        if Self::is_fifo(&queue_url) {
            let dedup_id = message
                .dedup_key
                .clone()
                .unwrap_or_else(|| message.id.as_uuid().to_string());
            req = req
                .message_group_id(&message.queue)
                .message_deduplication_id(dedup_id);
        }

        // Schedule via DelaySeconds (max 900s per SQS limits)
        if let Some(eta) = &message.eta {
            let delay = (*eta - chrono::Utc::now()).num_seconds().clamp(0, 900) as i32;
            if delay > 0 {
                req = req.delay_seconds(delay);
            }
        }

        req.send().await.map_err(broker_err)?;
        Ok(())
    }

    async fn dequeue(
        &self,
        queues: &[String],
        timeout: Duration,
    ) -> TaskResult<Option<TaskMessage>> {
        let wait = (timeout.as_secs() as i32).min(self.inner.config.wait_time_seconds);

        for queue_name in queues {
            let queue_url = match self.resolve_queue_url(queue_name) {
                Some(url) => url,
                None => continue,
            };

            let result = self
                .inner
                .client
                .receive_message()
                .queue_url(&queue_url)
                .max_number_of_messages(1)
                .wait_time_seconds(wait)
                .visibility_timeout(self.inner.config.visibility_timeout)
                .send()
                .await
                .map_err(broker_err)?;

            if let Some(messages) = result.messages {
                if let Some(sqs_msg) = messages.into_iter().next() {
                    let body = sqs_msg.body().unwrap_or("{}");
                    let task_msg: TaskMessage = serde_json::from_str(body)?;

                    if let Some(receipt) = sqs_msg.receipt_handle() {
                        self.inner
                            .receipts
                            .lock()
                            .await
                            .insert(task_msg.id, (queue_url.clone(), receipt.to_string()));
                    }

                    return Ok(Some(task_msg));
                }
            }
        }

        Ok(None)
    }

    async fn ack(&self, id: &TaskId) -> TaskResult<()> {
        if let Some((queue_url, receipt)) = self.inner.receipts.lock().await.remove(id) {
            self.inner
                .client
                .delete_message()
                .queue_url(&queue_url)
                .receipt_handle(&receipt)
                .send()
                .await
                .map_err(broker_err)?;
        }
        Ok(())
    }

    async fn nack(&self, message: TaskMessage) -> TaskResult<()> {
        // Make immediately visible for retry
        if let Some((queue_url, receipt)) = self.inner.receipts.lock().await.remove(&message.id) {
            self.inner
                .client
                .change_message_visibility()
                .queue_url(&queue_url)
                .receipt_handle(&receipt)
                .visibility_timeout(0)
                .send()
                .await
                .map_err(broker_err)?;
        }
        Ok(())
    }

    async fn dead_letter(&self, message: TaskMessage) -> TaskResult<()> {
        // Ack from original queue first
        self.ack(&message.id).await?;

        // If a DLQ URL is configured, send there explicitly
        if let Some(dlq_url) = &self.inner.config.dlq_url {
            let body = serde_json::to_string(&message)?;
            let mut req = self
                .inner
                .client
                .send_message()
                .queue_url(dlq_url)
                .message_body(&body);

            if Self::is_fifo(dlq_url) {
                req = req
                    .message_group_id(&message.queue)
                    .message_deduplication_id(message.id.as_uuid().to_string());
            }

            req.send().await.map_err(broker_err)?;
        }
        // Otherwise rely on SQS native RedrivePolicy
        Ok(())
    }

    async fn schedule(
        &self,
        message: TaskMessage,
        eta: chrono::DateTime<chrono::Utc>,
    ) -> TaskResult<()> {
        let delay = (eta - chrono::Utc::now()).num_seconds().max(0);
        let capped_delay = delay.min(900) as i32;

        if delay > 900 {
            warn!(
                task_id = %message.id,
                delay_seconds = delay,
                "SQS max DelaySeconds is 900 — re-enqueuing with 900s delay and eta; \
                 worker will re-schedule until eta is reached"
            );
        }

        let mut msg = message;
        if delay > 900 {
            msg.eta = Some(eta);
        }

        let queue_url = self
            .resolve_queue_url(&msg.queue)
            .ok_or_else(|| KojinError::QueueNotFound(msg.queue.clone()))?;

        let body = serde_json::to_string(&msg)?;
        let mut req = self
            .inner
            .client
            .send_message()
            .queue_url(&queue_url)
            .message_body(&body)
            .delay_seconds(capped_delay);

        if Self::is_fifo(&queue_url) {
            let dedup_id = msg
                .dedup_key
                .clone()
                .unwrap_or_else(|| msg.id.as_uuid().to_string());
            req = req
                .message_group_id(&msg.queue)
                .message_deduplication_id(dedup_id);
        }

        req.send().await.map_err(broker_err)?;
        Ok(())
    }

    async fn queue_len(&self, queue: &str) -> TaskResult<usize> {
        let queue_url = self
            .resolve_queue_url(queue)
            .ok_or_else(|| KojinError::QueueNotFound(queue.to_string()))?;

        let resp = self
            .inner
            .client
            .get_queue_attributes()
            .queue_url(&queue_url)
            .attribute_names(aws_sdk_sqs::types::QueueAttributeName::ApproximateNumberOfMessages)
            .send()
            .await
            .map_err(broker_err)?;

        let count = resp
            .attributes()
            .and_then(|attrs| {
                attrs.get(&aws_sdk_sqs::types::QueueAttributeName::ApproximateNumberOfMessages)
            })
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);

        Ok(count)
    }

    async fn dlq_len(&self, _queue: &str) -> TaskResult<usize> {
        if let Some(dlq_url) = &self.inner.config.dlq_url {
            let resp = self
                .inner
                .client
                .get_queue_attributes()
                .queue_url(dlq_url)
                .attribute_names(
                    aws_sdk_sqs::types::QueueAttributeName::ApproximateNumberOfMessages,
                )
                .send()
                .await
                .map_err(broker_err)?;

            let count = resp
                .attributes()
                .and_then(|attrs| {
                    attrs.get(&aws_sdk_sqs::types::QueueAttributeName::ApproximateNumberOfMessages)
                })
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(0);

            Ok(count)
        } else {
            Ok(0)
        }
    }

    async fn list_queues(&self) -> TaskResult<Vec<String>> {
        Ok(self.inner.queue_names.values().cloned().collect())
    }
}

/// Extract queue name from SQS URL (last path segment, without .fifo suffix).
fn queue_name_from_url(url: &str) -> String {
    let segment = url.rsplit('/').next().unwrap_or(url);
    segment.strip_suffix(".fifo").unwrap_or(segment).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_queue_name() {
        assert_eq!(
            queue_name_from_url("http://localhost:4566/000000000000/my-queue"),
            "my-queue"
        );
        assert_eq!(
            queue_name_from_url("http://localhost:4566/000000000000/my-queue.fifo"),
            "my-queue"
        );
    }

    #[test]
    fn extract_queue_name_bare() {
        assert_eq!(queue_name_from_url("my-queue"), "my-queue");
    }

    #[test]
    fn extract_queue_name_trailing_fifo() {
        assert_eq!(queue_name_from_url("my-queue.fifo"), "my-queue");
    }

    #[test]
    fn is_fifo_standard() {
        assert!(!SqsBroker::is_fifo(
            "http://localhost:4566/000000000000/my-queue"
        ));
    }

    #[test]
    fn is_fifo_fifo_queue() {
        assert!(SqsBroker::is_fifo(
            "http://localhost:4566/000000000000/my-queue.fifo"
        ));
    }
}

#[cfg(all(test, feature = "integration-tests"))]
mod integration_tests {
    use super::*;
    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::localstack::LocalStack;

    async fn setup_broker() -> (SqsBroker, testcontainers::ContainerAsync<LocalStack>) {
        let container = LocalStack::default().start().await.unwrap();
        let port = container.get_host_port_ipv4(4566).await.unwrap();
        let endpoint = format!("http://127.0.0.1:{port}");

        let sdk_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .endpoint_url(&endpoint)
            .region(aws_config::Region::new("us-east-1"))
            .credentials_provider(aws_credential_types::Credentials::new(
                "test", "test", None, None, "test",
            ))
            .load()
            .await;

        let client = Client::new(&sdk_config);

        // Create test queue
        let create_resp = client
            .create_queue()
            .queue_name("test-queue")
            .send()
            .await
            .unwrap();
        let queue_url = create_resp.queue_url().unwrap().to_string();

        let config = SqsConfig::new(vec![queue_url]);
        let broker = SqsBroker::new(&sdk_config, config);

        (broker, container)
    }

    #[tokio::test]
    async fn enqueue_dequeue() {
        let (broker, _container) = setup_broker().await;

        let msg = TaskMessage::new(
            "test_task",
            "test-queue",
            serde_json::json!({"key": "value"}),
        );
        broker.enqueue(msg).await.unwrap();

        let queues = vec!["test-queue".to_string()];
        let result = broker
            .dequeue(&queues, Duration::from_secs(5))
            .await
            .unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().task_name, "test_task");
    }

    #[tokio::test]
    async fn ack_deletes_message() {
        let (broker, _container) = setup_broker().await;

        let msg = TaskMessage::new("test_task", "test-queue", serde_json::json!({}));
        broker.enqueue(msg).await.unwrap();

        let queues = vec!["test-queue".to_string()];
        let dequeued = broker
            .dequeue(&queues, Duration::from_secs(5))
            .await
            .unwrap()
            .unwrap();

        broker.ack(&dequeued.id).await.unwrap();

        // Queue should be empty
        let result = broker
            .dequeue(&queues, Duration::from_secs(1))
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn queue_len_approximate() {
        let (broker, _container) = setup_broker().await;

        broker
            .enqueue(TaskMessage::new("t", "test-queue", serde_json::json!(1)))
            .await
            .unwrap();

        // SQS queue_len is approximate, just verify it doesn't error
        let len = broker.queue_len("test-queue").await.unwrap();
        assert!(len <= 1);
    }

    #[tokio::test]
    async fn list_queues_returns_names() {
        let (broker, _container) = setup_broker().await;
        let queues = broker.list_queues().await.unwrap();
        assert!(queues.contains(&"test-queue".to_string()));
    }
}
