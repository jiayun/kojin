use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use kojin::{
    KojinBuilder, MemoryBroker, MemoryResultBackend, Signature, Task, TaskContext, TaskResult,
    TracingMiddleware, chain, chord, group,
};
use serde::{Deserialize, Serialize};

// -- Task definitions --

#[derive(Debug, Serialize, Deserialize)]
struct Add {
    a: i32,
    b: i32,
}

#[async_trait]
impl Task for Add {
    const NAME: &'static str = "add";
    type Output = i32;

    async fn run(&self, _ctx: &TaskContext) -> TaskResult<Self::Output> {
        let result = self.a + self.b;
        println!("  add({}, {}) = {}", self.a, self.b, result);
        Ok(result)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Multiply {
    a: i32,
    b: i32,
}

#[async_trait]
impl Task for Multiply {
    const NAME: &'static str = "multiply";
    type Output = i32;

    async fn run(&self, _ctx: &TaskContext) -> TaskResult<Self::Output> {
        let result = self.a * self.b;
        println!("  multiply({}, {}) = {}", self.a, self.b, result);
        Ok(result)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Aggregate;

#[async_trait]
impl Task for Aggregate {
    const NAME: &'static str = "aggregate";
    type Output = String;

    async fn run(&self, _ctx: &TaskContext) -> TaskResult<Self::Output> {
        println!("  aggregate() — chord callback fired!");
        Ok("aggregated".to_string())
    }
}

fn add_sig(a: i32, b: i32) -> Signature {
    Signature::new("add", "default", serde_json::json!({"a": a, "b": b}))
}

fn mul_sig(a: i32, b: i32) -> Signature {
    Signature::new("multiply", "default", serde_json::json!({"a": a, "b": b}))
}

fn agg_sig() -> Signature {
    Signature::new("aggregate", "default", serde_json::json!(null))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let broker = MemoryBroker::new();
    let backend = Arc::new(MemoryResultBackend::new());

    // --- Demo 1: Chain ---
    println!("\n=== Chain: add(1,2) -> add(3,4) -> multiply(5,6) ===");
    let chain_workflow = chain![add_sig(1, 2), add_sig(3, 4), mul_sig(5, 6)];
    let handle = chain_workflow
        .apply(&broker, backend.as_ref())
        .await
        .unwrap();
    println!("Chain workflow submitted: {}", handle.id);

    // --- Demo 2: Group ---
    println!("\n=== Group: add(1,1), add(2,2), add(3,3) in parallel ===");
    let group_workflow = group![add_sig(1, 1), add_sig(2, 2), add_sig(3, 3)];
    let handle = group_workflow
        .apply(&broker, backend.as_ref())
        .await
        .unwrap();
    println!(
        "Group workflow submitted: {} ({} tasks)",
        handle.id,
        handle.task_ids.len()
    );

    // --- Demo 3: Chord ---
    println!("\n=== Chord: group of adds -> aggregate callback ===");
    let chord_workflow = chord(vec![add_sig(10, 20), add_sig(30, 40)], agg_sig());
    let handle = chord_workflow
        .apply(&broker, backend.as_ref())
        .await
        .unwrap();
    println!("Chord workflow submitted: {}", handle.id);

    // --- Run worker to process all ---
    println!("\n=== Starting worker ===");
    let worker = KojinBuilder::new(broker)
        .register_task::<Add>()
        .register_task::<Multiply>()
        .register_task::<Aggregate>()
        .result_backend(MemoryResultBackend::new()) // worker needs its own backend for chain/chord
        .middleware(TracingMiddleware)
        .concurrency(4)
        .queues(vec!["default".to_string()])
        .build();

    let cancel = worker.cancel_token();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        cancel.cancel();
    });

    worker.run().await;
    println!("\n=== Done ===");
}
