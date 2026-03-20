mod helpers;

use async_trait::async_trait;
use helpers::{AddTask, FailTask};
use kojin_core::{
    Broker, KojinError, MemoryBroker, MemoryResultBackend, Middleware, Task, TaskContext,
    TaskMessage, TaskRegistry, Worker, WorkerConfig,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Recording middleware for ordering tests
// ---------------------------------------------------------------------------

struct RecordingMiddleware {
    name: String,
    log: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl Middleware for RecordingMiddleware {
    async fn before(&self, _message: &TaskMessage) -> Result<(), KojinError> {
        self.log
            .lock()
            .unwrap()
            .push(format!("before_{}", self.name));
        Ok(())
    }

    async fn after(
        &self,
        _message: &TaskMessage,
        _result: &serde_json::Value,
    ) -> Result<(), KojinError> {
        self.log
            .lock()
            .unwrap()
            .push(format!("after_{}", self.name));
        Ok(())
    }

    async fn on_error(
        &self,
        _message: &TaskMessage,
        _error: &KojinError,
    ) -> Result<(), KojinError> {
        self.log
            .lock()
            .unwrap()
            .push(format!("error_{}", self.name));
        Ok(())
    }
}

fn worker_config() -> WorkerConfig {
    WorkerConfig {
        concurrency: 1,
        queues: vec!["default".to_string()],
        shutdown_timeout: Duration::from_secs(5),
        dequeue_timeout: Duration::from_millis(100),
    }
}

#[tokio::test]
async fn middleware_ordering() {
    let broker = MemoryBroker::new();
    let backend = Arc::new(MemoryResultBackend::new());
    let log = Arc::new(Mutex::new(Vec::<String>::new()));

    let mut registry = TaskRegistry::new();
    registry.register::<AddTask>();

    broker
        .enqueue(TaskMessage::new(
            AddTask::NAME,
            "default",
            serde_json::json!({"a": 1, "b": 2}),
        ))
        .await
        .unwrap();

    let log_clone = log.clone();
    let worker = Worker::new(broker, registry, TaskContext::new(), worker_config())
        .with_result_backend(backend)
        .with_middleware(RecordingMiddleware {
            name: "A".to_string(),
            log: log.clone(),
        })
        .with_middleware(RecordingMiddleware {
            name: "B".to_string(),
            log: log.clone(),
        });

    helpers::run_worker_until(
        worker,
        move || log_clone.lock().unwrap().len() >= 4,
        Duration::from_secs(5),
    )
    .await;

    let entries = log.lock().unwrap().clone();
    // before hooks run in order A, B; after hooks also run in order A, B
    assert_eq!(entries, vec!["before_A", "before_B", "after_A", "after_B"]);
}

#[tokio::test]
async fn metrics_middleware_e2e() {
    let broker = MemoryBroker::new();
    let backend = Arc::new(MemoryResultBackend::new());

    let mut registry = TaskRegistry::new();
    registry.register::<AddTask>();
    registry.register::<FailTask>();

    // 2 success + 1 fail
    for payload in [
        serde_json::json!({"a": 1, "b": 2}),
        serde_json::json!({"a": 3, "b": 4}),
    ] {
        broker
            .enqueue(TaskMessage::new(AddTask::NAME, "default", payload))
            .await
            .unwrap();
    }
    broker
        .enqueue(TaskMessage::new(
            FailTask::NAME,
            "default",
            serde_json::json!(null),
        ))
        .await
        .unwrap();

    let metrics = kojin_core::MetricsMiddleware::new();
    let metrics_clone = metrics.clone();

    let worker = Worker::new(broker, registry, TaskContext::new(), worker_config())
        .with_result_backend(backend)
        .with_middleware(metrics.clone());

    helpers::run_worker_until(
        worker,
        move || metrics_clone.tasks_started() >= 3,
        Duration::from_secs(5),
    )
    .await;

    assert_eq!(metrics.tasks_started(), 3);
    assert_eq!(metrics.tasks_succeeded(), 2);
    assert_eq!(metrics.tasks_failed(), 1);
}
