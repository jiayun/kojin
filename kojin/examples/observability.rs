//! Observability example — MetricsMiddleware + TracingMiddleware + RateLimitMiddleware.
//!
//! Run with:
//!   cargo run --example observability --features rate-limit

use std::num::NonZeroU32;
use std::time::Duration;

use async_trait::async_trait;
use kojin::{
    Broker, KojinBuilder, MemoryBroker, MetricsMiddleware, RateLimitMiddleware, Task, TaskContext,
    TaskMessage, TaskResult, TracingMiddleware,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct PingTask {
    seq: u32,
}

#[async_trait]
impl Task for PingTask {
    const NAME: &'static str = "ping";
    type Output = String;

    async fn run(&self, _ctx: &TaskContext) -> TaskResult<Self::Output> {
        println!("  ping #{}", self.seq);
        Ok(format!("pong #{}", self.seq))
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let broker = MemoryBroker::new();

    // Enqueue a batch of tasks
    for i in 1..=10 {
        let msg = TaskMessage::new(
            "ping",
            "default",
            serde_json::to_value(&PingTask { seq: i }).unwrap(),
        );
        broker.enqueue(msg).await.unwrap();
    }

    // MetricsMiddleware exposes counters you can query after processing
    let metrics = MetricsMiddleware::new();

    let worker = KojinBuilder::new(broker)
        .register_task::<PingTask>()
        // Middleware stack — order matters (outermost first)
        .middleware(TracingMiddleware)
        .middleware(metrics.clone())
        .middleware(RateLimitMiddleware::per_second(NonZeroU32::new(5).unwrap()))
        .concurrency(4)
        .queues(vec!["default".to_string()])
        .build();

    let cancel = worker.cancel_token();

    // Auto-shutdown after tasks drain
    let m = metrics.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if m.tasks_started() >= 10
                && m.tasks_started() == m.tasks_succeeded() + m.tasks_failed()
            {
                break;
            }
        }
        cancel.cancel();
    });

    println!("Starting worker with observability middleware...\n");
    worker.run().await;

    println!("\n=== Metrics ===");
    println!("  started:   {}", metrics.tasks_started());
    println!("  succeeded: {}", metrics.tasks_succeeded());
    println!("  failed:    {}", metrics.tasks_failed());
}
