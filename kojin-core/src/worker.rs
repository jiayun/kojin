use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::broker::Broker;
use crate::context::TaskContext;
use crate::error::KojinError;
use crate::message::TaskMessage;
use crate::middleware::Middleware;

use crate::registry::TaskRegistry;
use crate::state::TaskState;

/// Worker configuration.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Max concurrent tasks.
    pub concurrency: usize,
    /// Queue names to consume from.
    pub queues: Vec<String>,
    /// How long to wait for in-flight tasks during shutdown.
    pub shutdown_timeout: Duration,
    /// Dequeue poll timeout.
    pub dequeue_timeout: Duration,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            concurrency: 10,
            queues: vec!["default".to_string()],
            shutdown_timeout: Duration::from_secs(30),
            dequeue_timeout: Duration::from_secs(5),
        }
    }
}

/// The worker loop that dequeues and executes tasks.
pub struct Worker<B: Broker> {
    broker: Arc<B>,
    registry: Arc<TaskRegistry>,
    middlewares: Arc<Vec<Box<dyn Middleware>>>,
    context: Arc<TaskContext>,
    config: WorkerConfig,
    cancel: CancellationToken,
}

impl<B: Broker> Worker<B> {
    pub fn new(
        broker: B,
        registry: TaskRegistry,
        context: TaskContext,
        config: WorkerConfig,
    ) -> Self {
        Self {
            broker: Arc::new(broker),
            registry: Arc::new(registry),
            middlewares: Arc::new(Vec::new()),
            context: Arc::new(context),
            config,
            cancel: CancellationToken::new(),
        }
    }

    /// Add middleware to the worker pipeline.
    pub fn with_middleware(mut self, middleware: impl Middleware) -> Self {
        Arc::get_mut(&mut self.middlewares)
            .expect("middleware can only be added before starting")
            .push(Box::new(middleware));
        self
    }

    /// Add a boxed middleware to the worker pipeline.
    pub fn with_middleware_boxed(mut self, middleware: Box<dyn Middleware>) -> Self {
        Arc::get_mut(&mut self.middlewares)
            .expect("middleware can only be added before starting")
            .push(middleware);
        self
    }

    /// Get the cancellation token for external shutdown triggering.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Run the worker loop until shutdown.
    pub async fn run(&self) {
        let semaphore = Arc::new(Semaphore::new(self.config.concurrency));

        tracing::info!(
            concurrency = self.config.concurrency,
            queues = ?self.config.queues,
            "Worker starting"
        );

        loop {
            if self.cancel.is_cancelled() {
                break;
            }

            // Acquire a concurrency permit
            let permit = tokio::select! {
                permit = semaphore.clone().acquire_owned() => {
                    match permit {
                        Ok(p) => p,
                        Err(_) => break, // Semaphore closed
                    }
                }
                _ = self.cancel.cancelled() => break,
            };

            // Dequeue a message
            let message = tokio::select! {
                result = self.broker.dequeue(&self.config.queues, self.config.dequeue_timeout) => {
                    match result {
                        Ok(Some(msg)) => msg,
                        Ok(None) => {
                            drop(permit);
                            continue; // Timeout, try again
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to dequeue");
                            drop(permit);
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                    }
                }
                _ = self.cancel.cancelled() => {
                    drop(permit);
                    break;
                }
            };

            // Spawn task execution
            let broker = self.broker.clone();
            let registry = self.registry.clone();
            let middlewares = self.middlewares.clone();
            let context = self.context.clone();

            tokio::spawn(async move {
                let _permit = permit; // Hold permit until done
                execute_task(broker, registry, middlewares, context, message).await;
            });
        }

        // Graceful shutdown: wait for in-flight tasks to complete
        tracing::info!("Worker shutting down, waiting for in-flight tasks...");
        let drain_deadline = tokio::time::Instant::now() + self.config.shutdown_timeout;
        loop {
            // When all permits are available, no tasks are in-flight
            if semaphore.available_permits() == self.config.concurrency {
                break;
            }
            if tokio::time::Instant::now() >= drain_deadline {
                tracing::warn!("Shutdown timeout reached, some tasks may not have completed");
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        tracing::info!("Worker stopped");
    }
}

async fn execute_task<B: Broker>(
    broker: Arc<B>,
    registry: Arc<TaskRegistry>,
    middlewares: Arc<Vec<Box<dyn Middleware>>>,
    context: Arc<TaskContext>,
    mut message: TaskMessage,
) {
    let task_id = message.id;
    let task_name = message.task_name.clone();

    tracing::info!(task_id = %task_id, task_name = %task_name, "Executing task");
    message.state = TaskState::Started;

    // Run before middleware
    for mw in middlewares.iter() {
        if let Err(e) = mw.before(&message).await {
            tracing::error!(task_id = %task_id, error = %e, "Middleware before() failed");
            handle_failure(broker, middlewares, message, e).await;
            return;
        }
    }

    // Dispatch to handler
    match registry
        .dispatch(&task_name, message.payload.clone(), context)
        .await
    {
        Ok(result) => {
            // Run after middleware
            for mw in middlewares.iter() {
                if let Err(e) = mw.after(&message, &result).await {
                    tracing::warn!(task_id = %task_id, error = %e, "Middleware after() failed");
                }
            }
            message.state = TaskState::Success;
            if let Err(e) = broker.ack(&task_id).await {
                tracing::error!(task_id = %task_id, error = %e, "Failed to ack task");
            }
            tracing::info!(task_id = %task_id, task_name = %task_name, "Task completed successfully");
        }
        Err(e) => {
            tracing::error!(task_id = %task_id, task_name = %task_name, error = %e, "Task failed");
            handle_failure(broker, middlewares, message, e).await;
        }
    }
}

async fn handle_failure<B: Broker>(
    broker: Arc<B>,
    middlewares: Arc<Vec<Box<dyn Middleware>>>,
    mut message: TaskMessage,
    error: KojinError,
) {
    let task_id = message.id;

    // Run on_error middleware
    for mw in middlewares.iter() {
        if let Err(e) = mw.on_error(&message, &error).await {
            tracing::warn!(task_id = %task_id, error = %e, "Middleware on_error() failed");
        }
    }

    // Retry or dead-letter
    if message.retries < message.max_retries {
        message.retries += 1;
        message.state = TaskState::Retry;
        message.updated_at = chrono::Utc::now();

        let backoff_delay =
            crate::backoff::BackoffStrategy::default().delay_for(message.retries - 1);
        tracing::info!(
            task_id = %task_id,
            retry = message.retries,
            max_retries = message.max_retries,
            backoff = ?backoff_delay,
            "Retrying task"
        );

        // Simple sleep-based backoff (for MemoryBroker; Redis uses scheduled queue)
        tokio::time::sleep(backoff_delay).await;

        if let Err(e) = broker.nack(message).await {
            tracing::error!(task_id = %task_id, error = %e, "Failed to nack/requeue task");
        }
    } else {
        message.state = TaskState::DeadLettered;
        message.updated_at = chrono::Utc::now();
        tracing::warn!(task_id = %task_id, "Max retries exceeded, moving to DLQ");

        if let Err(e) = broker.dead_letter(message).await {
            tracing::error!(task_id = %task_id, error = %e, "Failed to dead-letter task");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_broker::MemoryBroker;
    use crate::task::Task;
    use async_trait::async_trait;
    use serde::{Deserialize, Serialize};
    use std::sync::atomic::{AtomicU32, Ordering};

    #[derive(Debug, Serialize, Deserialize)]
    struct CountTask;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    #[async_trait]
    impl Task for CountTask {
        const NAME: &'static str = "count";
        const MAX_RETRIES: u32 = 0;
        type Output = ();

        async fn run(&self, _ctx: &TaskContext) -> crate::error::TaskResult<Self::Output> {
            COUNTER.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn worker_processes_tasks() {
        COUNTER.store(0, Ordering::SeqCst);

        let broker = MemoryBroker::new();
        let mut registry = TaskRegistry::new();
        registry.register::<CountTask>();

        // Enqueue 3 tasks
        for _ in 0..3 {
            broker
                .enqueue(TaskMessage::new(
                    "count",
                    "default",
                    serde_json::json!(null),
                ))
                .await
                .unwrap();
        }

        let config = WorkerConfig {
            concurrency: 2,
            queues: vec!["default".to_string()],
            shutdown_timeout: Duration::from_secs(5),
            dequeue_timeout: Duration::from_millis(100),
        };

        let worker = Worker::new(broker.clone(), registry, TaskContext::new(), config);
        let cancel = worker.cancel_token();

        // Run worker in background
        let handle = tokio::spawn(async move {
            worker.run().await;
        });

        // Wait for tasks to be processed
        tokio::time::sleep(Duration::from_millis(500)).await;
        cancel.cancel();
        handle.await.unwrap();

        assert_eq!(COUNTER.load(Ordering::SeqCst), 3);
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct FailTask;

    #[async_trait]
    impl Task for FailTask {
        const NAME: &'static str = "fail_task";
        const MAX_RETRIES: u32 = 0;
        type Output = ();

        async fn run(&self, _ctx: &TaskContext) -> crate::error::TaskResult<Self::Output> {
            Err(KojinError::TaskFailed("intentional failure".into()))
        }
    }

    #[tokio::test]
    async fn worker_dead_letters_after_max_retries() {
        let broker = MemoryBroker::new();
        let mut registry = TaskRegistry::new();
        registry.register::<FailTask>();

        broker
            .enqueue(
                TaskMessage::new("fail_task", "default", serde_json::json!(null))
                    .with_max_retries(0),
            )
            .await
            .unwrap();

        let config = WorkerConfig {
            concurrency: 1,
            queues: vec!["default".to_string()],
            shutdown_timeout: Duration::from_secs(5),
            dequeue_timeout: Duration::from_millis(100),
        };

        let worker = Worker::new(broker.clone(), registry, TaskContext::new(), config);
        let cancel = worker.cancel_token();

        let handle = tokio::spawn(async move {
            worker.run().await;
        });

        tokio::time::sleep(Duration::from_millis(500)).await;
        cancel.cancel();
        handle.await.unwrap();

        assert_eq!(broker.dlq_len("default").await, 1);
    }

    #[tokio::test]
    async fn worker_graceful_shutdown() {
        let broker = MemoryBroker::new();
        let registry = TaskRegistry::new();

        let config = WorkerConfig {
            concurrency: 1,
            queues: vec!["default".to_string()],
            shutdown_timeout: Duration::from_secs(1),
            dequeue_timeout: Duration::from_millis(100),
        };

        let worker = Worker::new(broker, registry, TaskContext::new(), config);
        let cancel = worker.cancel_token();

        let handle = tokio::spawn(async move {
            worker.run().await;
        });

        // Cancel immediately
        cancel.cancel();
        // Should complete within shutdown timeout
        tokio::time::timeout(Duration::from_secs(3), handle)
            .await
            .expect("Worker should shutdown within timeout")
            .unwrap();
    }
}
