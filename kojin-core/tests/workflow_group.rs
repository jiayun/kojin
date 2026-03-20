#[allow(dead_code)]
mod helpers;

use helpers::{AddTask, FailTask};
use kojin_core::{
    Broker, Canvas, MemoryBroker, MemoryResultBackend, ResultBackend, Signature, Task, TaskContext,
    TaskRegistry, Worker, WorkerConfig, group,
};
use std::sync::Arc;
use std::time::Duration;

fn worker_config() -> WorkerConfig {
    WorkerConfig {
        concurrency: 4,
        queues: vec!["default".to_string()],
        shutdown_timeout: Duration::from_secs(5),
        dequeue_timeout: Duration::from_millis(100),
    }
}

#[tokio::test]
async fn group_all_members_complete() {
    let broker = MemoryBroker::new();
    let backend = Arc::new(MemoryResultBackend::new());

    let mut registry = TaskRegistry::new();
    registry.register::<AddTask>();

    let canvas = group![
        Signature::new(
            AddTask::NAME,
            "default",
            serde_json::json!({"a": 1, "b": 2})
        ),
        Signature::new(
            AddTask::NAME,
            "default",
            serde_json::json!({"a": 3, "b": 4})
        ),
        Signature::new(
            AddTask::NAME,
            "default",
            serde_json::json!({"a": 5, "b": 6})
        )
    ];
    let _handle = canvas.apply(&broker, &*backend).await.unwrap();

    // Peek at group_id by checking the broker queue
    let peek_broker = broker.clone();
    let first = peek_broker
        .dequeue(&["default".to_string()], Duration::from_millis(100))
        .await
        .unwrap()
        .unwrap();
    let group_id = first.group_id.clone().unwrap();
    // nack to put it back
    peek_broker.nack(first).await.unwrap();

    let backend_clone = backend.clone();
    let worker = Worker::new(broker, registry, TaskContext::new(), worker_config())
        .with_result_backend(backend.clone());

    let gid = group_id.clone();
    let bc = backend_clone.clone();
    helpers::run_worker_until_async(
        worker,
        move || {
            let bc = bc.clone();
            let gid = gid.clone();
            async move {
                match bc.get_group_results(&gid).await {
                    Ok(results) => results.len() == 3,
                    Err(_) => false,
                }
            }
        },
        Duration::from_secs(5),
    )
    .await;

    let results = backend_clone.get_group_results(&group_id).await.unwrap();
    assert_eq!(results.len(), 3);

    // Results should be 3, 7, 11 (in some order)
    let mut values: Vec<i32> = results
        .iter()
        .map(|v| serde_json::from_value(v.clone()).unwrap())
        .collect();
    values.sort();
    assert_eq!(values, vec![3, 7, 11]);
}

#[tokio::test]
async fn group_partial_failure() {
    let broker = MemoryBroker::new();
    let backend = Arc::new(MemoryResultBackend::new());

    let mut registry = TaskRegistry::new();
    registry.register::<AddTask>();
    registry.register::<FailTask>();

    // 2 AddTask + 1 FailTask in a group
    let group_canvas = Canvas::Group(vec![
        Canvas::Single(Signature::new(
            AddTask::NAME,
            "default",
            serde_json::json!({"a": 1, "b": 2}),
        )),
        Canvas::Single(Signature::new(
            AddTask::NAME,
            "default",
            serde_json::json!({"a": 3, "b": 4}),
        )),
        Canvas::Single(Signature::new(
            FailTask::NAME,
            "default",
            serde_json::json!(null),
        )),
    ]);
    group_canvas.apply(&broker, &*backend).await.unwrap();

    // Get group_id
    let peek = broker
        .dequeue(&["default".to_string()], Duration::from_millis(100))
        .await
        .unwrap()
        .unwrap();
    let group_id = peek.group_id.clone().unwrap();
    broker.nack(peek).await.unwrap();

    let worker = Worker::new(
        broker.clone(),
        registry,
        TaskContext::new(),
        worker_config(),
    )
    .with_result_backend(backend.clone());
    let cancel = worker.cancel_token();

    let handle = tokio::spawn(async move {
        worker.run().await;
    });

    // Wait for processing
    tokio::time::sleep(Duration::from_millis(1000)).await;
    cancel.cancel();
    handle.await.unwrap();

    // Only 2 successful results (FailTask doesn't store a result)
    let results = backend.get_group_results(&group_id).await.unwrap();
    assert_eq!(results.len(), 2);
}
