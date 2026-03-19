use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_lite::StreamExt;
use lapin::options::*;
use lapin::types::{AMQPValue, FieldTable, ShortString};
use lapin::{BasicProperties, Channel, Connection, ConnectionProperties};
use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use kojin_core::broker::Broker;
use kojin_core::error::{KojinError, TaskResult};
use kojin_core::message::TaskMessage;
use kojin_core::task_id::TaskId;

use crate::config::AmqpConfig;
use crate::topology;

fn broker_err(e: impl std::fmt::Display) -> KojinError {
    KojinError::Broker(e.to_string())
}

/// RabbitMQ broker using AMQP 0.9.1 via `lapin`.
///
/// # Topology
///
/// - `kojin.direct` exchange → `kojin.queue.{name}` (routing key = name)
/// - `kojin.dlx` exchange → `kojin.dlq.{name}` (dead-letter)
/// - `kojin.delayed` exchange for scheduled tasks (requires `rabbitmq-delayed-message-exchange`)
///
/// # Ack/Nack
///
/// Delivery tags are tracked per-task in memory. Calling `ack()` or `nack()`
/// resolves the corresponding AMQP delivery.
#[derive(Clone)]
pub struct AmqpBroker {
    inner: Arc<AmqpBrokerInner>,
}

struct AmqpBrokerInner {
    connection: Connection,
    publish_channel: Channel,
    config: AmqpConfig,
    /// Maps TaskId → (Channel, delivery_tag) for ack/nack.
    deliveries: Arc<Mutex<HashMap<TaskId, (Channel, u64)>>>,
    /// Receiver for consumed messages, fed by background consumer tasks.
    rx: Mutex<mpsc::Receiver<TaskMessage>>,
    /// Sender side, cloned into consumer tasks.
    tx: mpsc::Sender<TaskMessage>,
    /// Cancel token for background consumers.
    cancel: CancellationToken,
    /// Tracks which queues already have consumers running.
    consuming: Mutex<Vec<String>>,
}

impl AmqpBroker {
    /// Connect to RabbitMQ and declare topology for the given queues.
    pub async fn new(config: AmqpConfig, queues: &[String]) -> TaskResult<Self> {
        let conn = Connection::connect(&config.url, ConnectionProperties::default())
            .await
            .map_err(broker_err)?;

        let publish_channel = conn.create_channel().await.map_err(broker_err)?;

        // Declare exchanges and queue pairs
        let setup_channel = conn.create_channel().await.map_err(broker_err)?;
        topology::declare_topology(&setup_channel, &config, queues)
            .await
            .map_err(broker_err)?;

        let (tx, rx) = mpsc::channel(config.prefetch_count as usize * queues.len().max(1));
        let cancel = CancellationToken::new();

        let inner = Arc::new(AmqpBrokerInner {
            connection: conn,
            publish_channel,
            config,
            deliveries: Arc::new(Mutex::new(HashMap::new())),
            rx: Mutex::new(rx),
            tx,
            cancel,
            consuming: Mutex::new(Vec::new()),
        });

        let broker = Self { inner };

        // Start consumers for each queue
        for queue in queues {
            broker.ensure_consumer(queue).await?;
        }

        Ok(broker)
    }

    /// Ensure a background consumer exists for the given queue.
    async fn ensure_consumer(&self, queue: &str) -> TaskResult<()> {
        let mut consuming = self.inner.consuming.lock().await;
        if consuming.contains(&queue.to_string()) {
            return Ok(());
        }

        let channel = self
            .inner
            .connection
            .create_channel()
            .await
            .map_err(broker_err)?;

        channel
            .basic_qos(self.inner.config.prefetch_count, BasicQosOptions::default())
            .await
            .map_err(broker_err)?;

        let amqp_queue: ShortString = format!("kojin.queue.{queue}").into();
        let consumer_tag: ShortString = format!("kojin-{}-{}", queue, uuid::Uuid::now_v7()).into();

        let consumer = channel
            .basic_consume(
                amqp_queue,
                consumer_tag,
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(broker_err)?;

        let tx = self.inner.tx.clone();
        let deliveries = self.inner.deliveries.clone();
        let cancel = self.inner.cancel.clone();
        let ch = channel.clone();

        tokio::spawn(async move {
            let mut consumer = consumer;
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        debug!("consumer shutting down");
                        break;
                    }
                    delivery = consumer.next() => {
                        match delivery {
                            Some(Ok(delivery)) => {
                                let tag = delivery.delivery_tag;
                                match serde_json::from_slice::<TaskMessage>(&delivery.data) {
                                    Ok(msg) => {
                                        let task_id = msg.id;
                                        let entry: (Channel, u64) = (ch.clone(), tag);
                                        deliveries.lock().await.insert(task_id, entry);
                                        if tx.send(msg).await.is_err() {
                                            break; // receiver dropped
                                        }
                                    }
                                    Err(e) => {
                                        warn!(error = %e, "failed to deserialize AMQP message, nacking");
                                        let _ = ch.basic_nack(tag, BasicNackOptions { requeue: false, ..Default::default() }).await;
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                warn!(error = %e, "AMQP consumer error");
                                break;
                            }
                            None => break,
                        }
                    }
                }
            }
        });

        consuming.push(queue.to_string());
        Ok(())
    }

    /// Shut down background consumers.
    pub fn shutdown(&self) {
        self.inner.cancel.cancel();
    }
}

impl Drop for AmqpBrokerInner {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[async_trait]
impl Broker for AmqpBroker {
    async fn enqueue(&self, message: TaskMessage) -> TaskResult<()> {
        let routing_key: ShortString = message.queue.clone().into();
        let payload = serde_json::to_vec(&message)?;

        self.inner
            .publish_channel
            .basic_publish(
                ShortString::from(self.inner.config.exchange.as_str()),
                routing_key,
                BasicPublishOptions::default(),
                &payload,
                BasicProperties::default()
                    .with_delivery_mode(2) // persistent
                    .with_content_type("application/json".into()),
            )
            .await
            .map_err(broker_err)?
            .await
            .map_err(broker_err)?;

        Ok(())
    }

    async fn dequeue(
        &self,
        _queues: &[String],
        timeout: Duration,
    ) -> TaskResult<Option<TaskMessage>> {
        // Messages arrive from background consumers via the mpsc channel.
        // The `queues` argument is handled at consumer setup time.
        let mut rx = self.inner.rx.lock().await;
        tokio::select! {
            msg = rx.recv() => Ok(msg),
            _ = tokio::time::sleep(timeout) => Ok(None),
        }
    }

    async fn ack(&self, id: &TaskId) -> TaskResult<()> {
        if let Some((channel, tag)) = self.inner.deliveries.lock().await.remove(id) {
            channel
                .basic_ack(tag, BasicAckOptions::default())
                .await
                .map_err(broker_err)?;
        }
        Ok(())
    }

    async fn nack(&self, message: TaskMessage) -> TaskResult<()> {
        if let Some((channel, tag)) = self.inner.deliveries.lock().await.remove(&message.id) {
            channel
                .basic_nack(
                    tag,
                    BasicNackOptions {
                        requeue: true,
                        ..Default::default()
                    },
                )
                .await
                .map_err(broker_err)?;
        }
        Ok(())
    }

    async fn dead_letter(&self, message: TaskMessage) -> TaskResult<()> {
        // nack with requeue=false triggers AMQP dead-letter routing
        if let Some((channel, tag)) = self.inner.deliveries.lock().await.remove(&message.id) {
            channel
                .basic_nack(
                    tag,
                    BasicNackOptions {
                        requeue: false,
                        ..Default::default()
                    },
                )
                .await
                .map_err(broker_err)?;
        }
        Ok(())
    }

    async fn schedule(
        &self,
        message: TaskMessage,
        eta: chrono::DateTime<chrono::Utc>,
    ) -> TaskResult<()> {
        let delay_ms = (eta - chrono::Utc::now()).num_milliseconds().max(0);
        let routing_key: ShortString = message.queue.clone().into();
        let payload = serde_json::to_vec(&message)?;

        let mut headers = FieldTable::default();
        headers.insert("x-delay".into(), AMQPValue::LongInt(delay_ms as i32));

        self.inner
            .publish_channel
            .basic_publish(
                ShortString::from(self.inner.config.delayed_exchange.as_str()),
                routing_key,
                BasicPublishOptions::default(),
                &payload,
                BasicProperties::default()
                    .with_delivery_mode(2)
                    .with_content_type("application/json".into())
                    .with_headers(headers),
            )
            .await
            .map_err(broker_err)?
            .await
            .map_err(broker_err)?;

        Ok(())
    }

    async fn queue_len(&self, queue: &str) -> TaskResult<usize> {
        let channel = self
            .inner
            .connection
            .create_channel()
            .await
            .map_err(broker_err)?;

        let amqp_queue: ShortString = format!("kojin.queue.{queue}").into();
        let q = channel
            .queue_declare(
                amqp_queue,
                QueueDeclareOptions {
                    passive: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(broker_err)?;

        Ok(q.message_count() as usize)
    }

    async fn dlq_len(&self, queue: &str) -> TaskResult<usize> {
        let channel = self
            .inner
            .connection
            .create_channel()
            .await
            .map_err(broker_err)?;

        let dlq_name: ShortString = format!("kojin.dlq.{queue}").into();
        let q = channel
            .queue_declare(
                dlq_name,
                QueueDeclareOptions {
                    passive: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(broker_err)?;

        Ok(q.message_count() as usize)
    }

    async fn list_queues(&self) -> TaskResult<Vec<String>> {
        // AMQP doesn't have a native "list queues" command.
        // Return the queues we know about from our consumer list.
        let consuming = self.inner.consuming.lock().await;
        Ok(consuming.clone())
    }

    async fn dlq_messages(
        &self,
        queue: &str,
        offset: usize,
        limit: usize,
    ) -> TaskResult<Vec<TaskMessage>> {
        // AMQP doesn't support random access. We basic_get messages for inspection
        // then nack them back (requeue=true) so they remain in the DLQ.
        let channel = self
            .inner
            .connection
            .create_channel()
            .await
            .map_err(broker_err)?;

        let dlq_name: ShortString = format!("kojin.dlq.{queue}").into();
        let mut messages = Vec::new();
        let mut skipped = 0;

        for _ in 0..(offset + limit) {
            match channel
                .basic_get(dlq_name.clone(), BasicGetOptions { no_ack: false })
                .await
                .map_err(broker_err)?
            {
                Some(delivery) => {
                    if skipped < offset {
                        skipped += 1;
                    } else if let Ok(msg) = serde_json::from_slice::<TaskMessage>(&delivery.data) {
                        messages.push(msg);
                    }
                    // Requeue so it stays in the DLQ
                    channel
                        .basic_nack(
                            delivery.delivery.delivery_tag,
                            BasicNackOptions {
                                requeue: true,
                                ..Default::default()
                            },
                        )
                        .await
                        .map_err(broker_err)?;
                }
                None => break,
            }
        }

        Ok(messages)
    }
}
