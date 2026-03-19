//! Dashboard example — spawn a monitoring HTTP API alongside the worker.
//!
//! Run with:
//!   cargo run --example dashboard --features dashboard
//!
//! Then query the API:
//!   curl http://localhost:9090/api/queues
//!   curl http://localhost:9090/api/metrics
//!   curl http://localhost:9090/api/tasks/<task-id>

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use kojin::{
    Broker, DashboardState, KojinBuilder, MemoryBroker, MemoryResultBackend, MetricsMiddleware,
    Task, TaskContext, TaskMessage, TaskResult, TracingMiddleware, spawn_dashboard,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct EchoTask {
    message: String,
}

#[async_trait]
impl Task for EchoTask {
    const NAME: &'static str = "echo";
    type Output = String;

    async fn run(&self, _ctx: &TaskContext) -> TaskResult<Self::Output> {
        println!("  echo: {}", self.message);
        Ok(self.message.clone())
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let broker = MemoryBroker::new();
    let backend = Arc::new(MemoryResultBackend::new());
    let metrics = MetricsMiddleware::new();

    // Enqueue some tasks
    for i in 1..=5 {
        let msg = TaskMessage::new(
            "echo",
            "default",
            serde_json::to_value(&EchoTask {
                message: format!("hello #{i}"),
            })
            .unwrap(),
        );
        broker.enqueue(msg).await.unwrap();
    }

    // Spawn the dashboard on port 9090
    let dashboard_state = DashboardState::new(Arc::new(broker.clone()))
        .with_result_backend(backend.clone())
        .with_metrics(metrics.clone());

    let _handle = spawn_dashboard(dashboard_state, 9090);
    println!("Dashboard listening on http://localhost:9090");

    // Build and run worker
    let worker = KojinBuilder::new(broker)
        .register_task::<EchoTask>()
        .result_backend_shared(backend)
        .middleware(TracingMiddleware)
        .middleware(metrics)
        .concurrency(2)
        .queues(vec!["default".to_string()])
        .build();

    let cancel = worker.cancel_token();

    println!("Starting worker... (auto-stops after 10s)\n");
    println!("Try: curl http://localhost:9090/api/queues");
    println!("     curl http://localhost:9090/api/metrics\n");

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        cancel.cancel();
    });

    worker.run().await;
    println!("Worker stopped");
}
