use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use kojin_core::{Broker, MemoryBroker, TaskMessage};
use std::time::Duration;
use tokio::runtime::Runtime;

fn make_payload(size: usize) -> serde_json::Value {
    if size == 0 {
        serde_json::json!(null)
    } else {
        serde_json::json!({"data": "x".repeat(size)})
    }
}

fn enqueue_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("enqueue");

    for (label, size) in [("empty", 0), ("1KB", 1024), ("100KB", 100_000)] {
        group.bench_with_input(BenchmarkId::new("payload", label), &size, |b, &size| {
            let payload = make_payload(size);
            b.to_async(&rt).iter(|| {
                let broker = MemoryBroker::new();
                let payload = payload.clone();
                async move {
                    broker
                        .enqueue(TaskMessage::new("bench_task", "default", payload))
                        .await
                        .unwrap();
                }
            });
        });
    }
    group.finish();
}

fn dequeue_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("dequeue");

    for (label, size) in [("empty", 0), ("1KB", 1024), ("100KB", 100_000)] {
        group.bench_with_input(BenchmarkId::new("payload", label), &size, |b, &size| {
            let payload = make_payload(size);
            b.to_async(&rt).iter_custom(|iters| {
                let payload = payload.clone();
                async move {
                    let broker = MemoryBroker::new();
                    // Pre-fill
                    for _ in 0..iters {
                        broker
                            .enqueue(TaskMessage::new("bench_task", "default", payload.clone()))
                            .await
                            .unwrap();
                    }
                    let start = std::time::Instant::now();
                    for _ in 0..iters {
                        broker
                            .dequeue(&["default".to_string()], Duration::from_millis(100))
                            .await
                            .unwrap();
                    }
                    start.elapsed()
                }
            });
        });
    }
    group.finish();
}

criterion_group!(benches, enqueue_throughput, dequeue_throughput);
criterion_main!(benches);
