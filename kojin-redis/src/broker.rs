use async_trait::async_trait;
use deadpool_redis::{Config, Pool, Runtime};
use redis::AsyncCommands;
use std::time::Duration;

use kojin_core::broker::Broker;
use kojin_core::error::{KojinError, TaskResult};
use kojin_core::message::TaskMessage;
use kojin_core::task_id::TaskId;

use crate::config::RedisConfig;
use crate::keys::KeyBuilder;

fn broker_err(e: impl std::fmt::Display) -> KojinError {
    KojinError::Broker(e.to_string())
}

/// Redis-backed message broker.
#[derive(Clone)]
pub struct RedisBroker {
    pool: Pool,
    keys: KeyBuilder,
    worker_id: String,
}

impl RedisBroker {
    /// Create a new Redis broker from config.
    pub async fn new(config: RedisConfig) -> TaskResult<Self> {
        let cfg = Config::from_url(&config.url);
        let pool = cfg
            .builder()
            .map_err(broker_err)?
            .max_size(config.pool_size)
            .runtime(Runtime::Tokio1)
            .build()
            .map_err(broker_err)?;

        // Verify connection
        let _conn = pool.get().await.map_err(broker_err)?;

        let worker_id = uuid::Uuid::now_v7().to_string();

        Ok(Self {
            pool,
            keys: KeyBuilder::new(config.key_prefix),
            worker_id,
        })
    }

    /// Get a connection from the pool.
    async fn conn(&self) -> TaskResult<deadpool_redis::Connection> {
        self.pool.get().await.map_err(broker_err)
    }

    /// Poll the scheduled set and move due items to their queues.
    pub async fn poll_scheduled(&self) -> TaskResult<usize> {
        let mut conn = self.conn().await?;
        let now = chrono::Utc::now().timestamp();
        let prefix = self.keys.scheduled().replace(":scheduled", "");

        let script = redis::Script::new(crate::scripts::POLL_SCHEDULED_SCRIPT);
        let count: usize = script
            .key(self.keys.scheduled())
            .arg(now)
            .arg(prefix)
            .invoke_async(&mut *conn)
            .await
            .map_err(broker_err)?;

        Ok(count)
    }
}

#[async_trait]
impl Broker for RedisBroker {
    async fn enqueue(&self, message: TaskMessage) -> TaskResult<()> {
        let mut conn = self.conn().await?;
        let queue_key = self.keys.queue(&message.queue);
        let serialized = serde_json::to_string(&message)?;

        conn.lpush::<_, _, ()>(&queue_key, &serialized)
            .await
            .map_err(broker_err)?;

        Ok(())
    }

    async fn dequeue(
        &self,
        queues: &[String],
        timeout: Duration,
    ) -> TaskResult<Option<TaskMessage>> {
        let mut conn = self.conn().await?;
        let processing_key = self.keys.processing(&self.worker_id);

        // Try BLMOVE from each queue in order
        for queue_name in queues {
            let queue_key = self.keys.queue(queue_name);
            let timeout_secs = timeout.as_secs_f64();

            let result: Option<String> = redis::cmd("BLMOVE")
                .arg(&queue_key)
                .arg(&processing_key)
                .arg("RIGHT")
                .arg("LEFT")
                .arg(timeout_secs)
                .query_async(&mut *conn)
                .await
                .map_err(broker_err)?;

            if let Some(data) = result {
                let message: TaskMessage = serde_json::from_str(&data)?;
                return Ok(Some(message));
            }
        }

        Ok(None)
    }

    async fn ack(&self, id: &TaskId) -> TaskResult<()> {
        let mut conn = self.conn().await?;
        let processing_key = self.keys.processing(&self.worker_id);
        let id_str = id.to_string();

        // Remove from processing list by scanning for the task ID in serialized messages
        let items: Vec<String> = conn
            .lrange(&processing_key, 0, -1)
            .await
            .map_err(broker_err)?;

        for item in items {
            if item.contains(&id_str) {
                conn.lrem::<_, _, ()>(&processing_key, 1, &item)
                    .await
                    .map_err(broker_err)?;
                break;
            }
        }

        Ok(())
    }

    async fn nack(&self, message: TaskMessage) -> TaskResult<()> {
        self.ack(&message.id).await?;
        self.enqueue(message).await
    }

    async fn dead_letter(&self, message: TaskMessage) -> TaskResult<()> {
        let mut conn = self.conn().await?;
        self.ack(&message.id).await?;

        let dlq_key = self.keys.dlq(&message.queue);
        let serialized = serde_json::to_string(&message)?;
        conn.lpush::<_, _, ()>(&dlq_key, &serialized)
            .await
            .map_err(broker_err)?;

        Ok(())
    }

    async fn schedule(
        &self,
        message: TaskMessage,
        eta: chrono::DateTime<chrono::Utc>,
    ) -> TaskResult<()> {
        let mut conn = self.conn().await?;
        let scheduled_key = self.keys.scheduled();
        let serialized = serde_json::to_string(&message)?;
        let score = eta.timestamp() as f64;

        conn.zadd::<_, _, _, ()>(&scheduled_key, &serialized, score)
            .await
            .map_err(broker_err)?;

        Ok(())
    }

    async fn queue_len(&self, queue: &str) -> TaskResult<usize> {
        let mut conn = self.conn().await?;
        let queue_key = self.keys.queue(queue);
        let len: usize = conn.llen(&queue_key).await.map_err(broker_err)?;
        Ok(len)
    }
}

#[cfg(all(test, feature = "integration-tests"))]
mod tests {
    use super::*;
    use testcontainers::{ImageExt, runners::AsyncRunner};
    use testcontainers_modules::redis::Redis;

    async fn setup_broker() -> (RedisBroker, testcontainers::ContainerAsync<Redis>) {
        let container = Redis::default().with_tag("7").start().await.unwrap();
        let port = container.get_host_port_ipv4(6379).await.unwrap();
        let config = RedisConfig::new(format!("redis://127.0.0.1:{port}")).with_prefix("test");
        let broker = RedisBroker::new(config).await.unwrap();
        (broker, container)
    }

    #[tokio::test]
    async fn enqueue_dequeue() {
        let (broker, _container) = setup_broker().await;

        let msg = TaskMessage::new("test_task", "default", serde_json::json!({"key": "value"}));
        broker.enqueue(msg.clone()).await.unwrap();

        let queues = vec!["default".to_string()];
        let result = broker
            .dequeue(&queues, Duration::from_secs(1))
            .await
            .unwrap();
        assert!(result.is_some());
        let dequeued = result.unwrap();
        assert_eq!(dequeued.task_name, "test_task");
    }

    #[tokio::test]
    async fn ack_and_nack() {
        let (broker, _container) = setup_broker().await;

        let msg = TaskMessage::new("test_task", "default", serde_json::json!({}));
        broker.enqueue(msg).await.unwrap();

        let queues = vec!["default".to_string()];
        let dequeued = broker
            .dequeue(&queues, Duration::from_secs(1))
            .await
            .unwrap()
            .unwrap();

        broker.ack(&dequeued.id).await.unwrap();
        assert_eq!(broker.queue_len("default").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn dead_letter_queue() {
        let (broker, _container) = setup_broker().await;

        let msg = TaskMessage::new("test_task", "default", serde_json::json!({}));
        broker.enqueue(msg).await.unwrap();

        let queues = vec!["default".to_string()];
        let dequeued = broker
            .dequeue(&queues, Duration::from_secs(1))
            .await
            .unwrap()
            .unwrap();

        broker.dead_letter(dequeued).await.unwrap();

        let mut conn = broker.conn().await.unwrap();
        let dlq_len: usize = conn.llen(broker.keys.dlq("default")).await.unwrap();
        assert_eq!(dlq_len, 1);
    }

    #[tokio::test]
    async fn queue_len_tracking() {
        let (broker, _container) = setup_broker().await;

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

    #[tokio::test]
    async fn schedule_and_poll() {
        let (broker, _container) = setup_broker().await;

        let msg = TaskMessage::new("scheduled_task", "default", serde_json::json!({}));
        let past = chrono::Utc::now() - chrono::Duration::seconds(10);
        broker.schedule(msg, past).await.unwrap();

        let count = broker.poll_scheduled().await.unwrap();
        assert_eq!(count, 1);
        assert_eq!(broker.queue_len("default").await.unwrap(), 1);
    }
}
