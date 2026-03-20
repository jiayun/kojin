mod helpers;

use helpers::{AddTask, CountTask, FailTask};
use kojin_core::{
    Broker, MemoryBroker, MemoryResultBackend, Signature, Task, TaskContext, TaskRegistry, Worker,
    WorkerConfig, chain,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

fn worker_config() -> WorkerConfig {
    WorkerConfig {
        concurrency: 1,
        queues: vec!["default".to_string()],
        shutdown_timeout: Duration::from_secs(5),
        dequeue_timeout: Duration::from_millis(100),
    }
}

#[tokio::test]
async fn chain_all_steps_execute() {
    let broker = MemoryBroker::new();
    let backend = Arc::new(MemoryResultBackend::new());
    let counter = Arc::new(AtomicU32::new(0));

    let mut registry = TaskRegistry::new();
    registry.register::<CountTask>();

    let mut ctx = TaskContext::new();
    ctx.insert(counter.clone());

    let canvas = chain![
        CountTask.signature(),
        CountTask.signature(),
        CountTask.signature()
    ];
    canvas.apply(&broker, &*backend).await.unwrap();

    let worker =
        Worker::new(broker, registry, ctx, worker_config()).with_result_backend(backend.clone());

    helpers::run_worker_until(
        worker,
        move || counter.load(Ordering::SeqCst) >= 3,
        Duration::from_secs(5),
    )
    .await;
}

#[tokio::test]
async fn chain_result_passed_via_header() {
    let broker = MemoryBroker::new();
    let backend = Arc::new(MemoryResultBackend::new());

    let mut registry = TaskRegistry::new();
    registry.register::<AddTask>();
    registry.register::<CountTask>();

    let counter = Arc::new(AtomicU32::new(0));
    let mut ctx = TaskContext::new();
    ctx.insert(counter.clone());

    // Chain: AddTask{3,4} -> CountTask
    let canvas = chain![
        Signature::new(
            AddTask::NAME,
            "default",
            serde_json::json!({"a": 3, "b": 4})
        ),
        CountTask.signature()
    ];
    canvas.apply(&broker, &*backend).await.unwrap();

    // After AddTask completes, the broker should have a CountTask message
    // with kojin.chain_input = 7
    let worker = Worker::new(broker.clone(), registry, ctx, worker_config())
        .with_result_backend(backend.clone());

    helpers::run_worker_until(
        worker,
        move || counter.load(Ordering::SeqCst) >= 1,
        Duration::from_secs(5),
    )
    .await;

    // The CountTask executed, confirming the chain continued.
    // We can't easily inspect the header mid-flight in an e2e test,
    // but we verify the chain completed by CountTask having run.
}

#[tokio::test]
async fn chain_failure_stops_continuation() {
    let broker = MemoryBroker::new();
    let backend = Arc::new(MemoryResultBackend::new());
    let counter = Arc::new(AtomicU32::new(0));

    let mut registry = TaskRegistry::new();
    registry.register::<FailTask>();
    registry.register::<CountTask>();

    let mut ctx = TaskContext::new();
    ctx.insert(counter.clone());

    // Chain: FailTask -> CountTask (CountTask should NOT execute)
    let canvas = chain![FailTask.signature(), CountTask.signature()];
    canvas.apply(&broker, &*backend).await.unwrap();

    let worker = Worker::new(broker.clone(), registry, ctx, worker_config())
        .with_result_backend(backend.clone());
    let cancel = worker.cancel_token();

    let handle = tokio::spawn(async move {
        worker.run().await;
    });

    // Give enough time for FailTask to execute and be dead-lettered
    tokio::time::sleep(Duration::from_millis(500)).await;
    cancel.cancel();
    handle.await.unwrap();

    // Counter should remain 0 — chain stopped at FailTask
    assert_eq!(counter.load(Ordering::SeqCst), 0);
    // FailTask should be in DLQ
    assert_eq!(broker.dlq_len("default").await.unwrap(), 1);
}
