mod helpers;

use helpers::SlowTask;
use kojin_core::{
    Broker, MemoryBroker, MemoryResultBackend, ResultBackend, Task, TaskContext, TaskMessage,
    TaskRegistry, Worker, WorkerConfig,
};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn shutdown_completes_in_flight() {
    let broker = MemoryBroker::new();
    let backend = Arc::new(MemoryResultBackend::new());

    let mut registry = TaskRegistry::new();
    registry.register::<SlowTask>();

    let msg = TaskMessage::new(SlowTask::NAME, "default", serde_json::json!({"ms": 300}));
    let task_id = msg.id;
    broker.enqueue(msg).await.unwrap();

    let config = WorkerConfig {
        concurrency: 1,
        queues: vec!["default".to_string()],
        shutdown_timeout: Duration::from_secs(5),
        dequeue_timeout: Duration::from_millis(100),
    };

    let worker = Worker::new(broker, registry, TaskContext::new(), config)
        .with_result_backend(backend.clone());
    let cancel = worker.cancel_token();

    let handle = tokio::spawn(async move {
        worker.run().await;
    });

    // Wait for the task to be picked up, then cancel
    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel.cancel();

    // Worker should wait for the in-flight SlowTask(300ms) to complete
    tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .expect("Worker should shut down within timeout")
        .unwrap();

    // Result should be stored since the task completed during graceful shutdown
    let result = backend.get(&task_id).await.unwrap();
    assert_eq!(result, Some(serde_json::json!("done")));
}

#[tokio::test]
async fn shutdown_timeout_force_stops() {
    let broker = MemoryBroker::new();

    let mut registry = TaskRegistry::new();
    registry.register::<SlowTask>();

    // Task that takes 5 seconds
    broker
        .enqueue(TaskMessage::new(
            SlowTask::NAME,
            "default",
            serde_json::json!({"ms": 5000}),
        ))
        .await
        .unwrap();

    let config = WorkerConfig {
        concurrency: 1,
        queues: vec!["default".to_string()],
        shutdown_timeout: Duration::from_millis(200), // Very short shutdown timeout
        dequeue_timeout: Duration::from_millis(100),
    };

    let worker = Worker::new(broker, registry, TaskContext::new(), config);
    let cancel = worker.cancel_token();

    let handle = tokio::spawn(async move {
        worker.run().await;
    });

    // Wait for task pickup, then trigger shutdown
    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel.cancel();

    // Worker should stop within ~500ms (200ms shutdown_timeout + margin)
    let result = tokio::time::timeout(Duration::from_millis(1000), handle).await;
    assert!(
        result.is_ok(),
        "Worker should force-stop within the shutdown timeout"
    );
}
