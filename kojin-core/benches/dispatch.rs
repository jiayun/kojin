use async_trait::async_trait;
use criterion::{Criterion, criterion_group, criterion_main};
use kojin_core::{Task, TaskContext, TaskRegistry, TaskResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::runtime::Runtime;

#[derive(Debug, Serialize, Deserialize)]
struct TrivialTask;

#[async_trait]
impl Task for TrivialTask {
    const NAME: &'static str = "bench_trivial";
    const MAX_RETRIES: u32 = 0;
    type Output = i32;

    async fn run(&self, _ctx: &TaskContext) -> TaskResult<Self::Output> {
        Ok(42)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ComputeTask {
    a: i32,
    b: i32,
}

#[async_trait]
impl Task for ComputeTask {
    const NAME: &'static str = "bench_compute";
    const MAX_RETRIES: u32 = 0;
    type Output = i32;

    async fn run(&self, _ctx: &TaskContext) -> TaskResult<Self::Output> {
        Ok(self.a + self.b)
    }
}

fn registry_dispatch(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut registry = TaskRegistry::new();
    registry.register::<TrivialTask>();
    registry.register::<ComputeTask>();

    let ctx = Arc::new(TaskContext::new());

    let mut group = c.benchmark_group("dispatch");

    group.bench_function("trivial", |b| {
        let registry = &registry;
        let ctx = ctx.clone();
        b.to_async(&rt).iter(|| {
            let ctx = ctx.clone();
            async move {
                registry
                    .dispatch("bench_trivial", serde_json::json!(null), ctx)
                    .await
                    .unwrap()
            }
        });
    });

    group.bench_function("compute", |b| {
        let registry = &registry;
        let ctx = ctx.clone();
        b.to_async(&rt).iter(|| {
            let ctx = ctx.clone();
            async move {
                registry
                    .dispatch(
                        "bench_compute",
                        serde_json::json!({"a": 100, "b": 200}),
                        ctx,
                    )
                    .await
                    .unwrap()
            }
        });
    });

    group.finish();
}

criterion_group!(benches, registry_dispatch);
criterion_main!(benches);
