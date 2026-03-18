use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use kojin::{
    KojinBuilder, MemoryBroker, Signature, Task, TaskContext, TaskResult, TracingMiddleware,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Heartbeat;

#[async_trait]
impl Task for Heartbeat {
    const NAME: &'static str = "heartbeat";
    type Output = ();

    async fn run(&self, _ctx: &TaskContext) -> TaskResult<Self::Output> {
        println!("  heartbeat at {:?}", Utc::now());
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let broker = MemoryBroker::new();

    // Build worker with cron schedule
    let worker = KojinBuilder::new(broker)
        .register_task::<Heartbeat>()
        .middleware(TracingMiddleware)
        .concurrency(2)
        .queues(vec!["default".to_string()])
        .cron(
            "heartbeat_every_10s",
            "0/10 * * * * * *", // Every 10 seconds
            Signature::new("heartbeat", "default", serde_json::json!(null)),
        )
        .build();

    let cancel = worker.cancel_token();

    println!("Starting worker with cron scheduler...");
    println!("Heartbeat will fire every 10 seconds. Press Ctrl+C to stop.\n");

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(35)).await;
        cancel.cancel();
    });

    worker.run().await;
    println!("Worker stopped");
}
