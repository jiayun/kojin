mod helpers;

use helpers::{AddTask, CountTask};
use kojin_core::{
    MemoryBroker, MemoryResultBackend, Signature, Task, TaskContext, TaskRegistry, Worker,
    WorkerConfig, chord,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

#[tokio::test]
async fn chord_callback_fires_after_group() {
    let broker = MemoryBroker::new();
    let backend = Arc::new(MemoryResultBackend::new());
    let counter = Arc::new(AtomicU32::new(0));

    let mut registry = TaskRegistry::new();
    registry.register::<AddTask>();
    registry.register::<CountTask>();

    let mut ctx = TaskContext::new();
    ctx.insert(counter.clone());

    // Chord: group of 3 AddTasks, callback = CountTask
    let canvas = chord(
        vec![
            Signature::new(
                AddTask::NAME,
                "default",
                serde_json::json!({"a": 1, "b": 2}),
            ),
            Signature::new(
                AddTask::NAME,
                "default",
                serde_json::json!({"a": 3, "b": 4}),
            ),
            Signature::new(
                AddTask::NAME,
                "default",
                serde_json::json!({"a": 5, "b": 6}),
            ),
        ],
        CountTask.signature(),
    );
    canvas.apply(&broker, &*backend).await.unwrap();

    let config = WorkerConfig {
        concurrency: 4,
        queues: vec!["default".to_string()],
        shutdown_timeout: Duration::from_secs(5),
        dequeue_timeout: Duration::from_millis(100),
    };

    let worker =
        Worker::new(broker.clone(), registry, ctx, config).with_result_backend(backend.clone());

    // Wait until the callback (CountTask) fires
    helpers::run_worker_until(
        worker,
        move || counter.load(Ordering::SeqCst) >= 1,
        Duration::from_secs(5),
    )
    .await;
}
