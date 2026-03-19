//! AMQP broker example — enqueue/dequeue round-trip via RabbitMQ.
//!
//! Prerequisites:
//!   docker run -d --name rabbitmq -p 5672:5672 -p 15672:15672 rabbitmq:3-management
//!
//! Run with:
//!   cargo run --example amqp --features amqp

use std::time::Duration;

use async_trait::async_trait;
use kojin::{
    AmqpBroker, AmqpConfig, Broker, KojinBuilder, MetricsMiddleware, Task, TaskContext,
    TaskMessage, TaskResult, TracingMiddleware,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct ProcessOrder {
    order_id: u64,
    amount: f64,
}

#[async_trait]
impl Task for ProcessOrder {
    const NAME: &'static str = "process_order";
    const QUEUE: &'static str = "orders";
    type Output = String;

    async fn run(&self, _ctx: &TaskContext) -> TaskResult<Self::Output> {
        println!(
            "  Processing order #{} (${:.2})",
            self.order_id, self.amount
        );
        Ok(format!("order_{}_confirmed", self.order_id))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    // Connect to RabbitMQ — topology (exchanges, queues, DLQ) is declared automatically
    let config = AmqpConfig::new("amqp://guest:guest@localhost:5672/%2f");
    let queues = vec!["orders".to_string()];
    let broker = AmqpBroker::new(config, &queues).await?;
    println!("Connected to RabbitMQ");

    // Enqueue tasks
    for i in 1..=5 {
        let msg = TaskMessage::new(
            "process_order",
            "orders",
            serde_json::to_value(&ProcessOrder {
                order_id: i,
                amount: i as f64 * 29.99,
            })?,
        );
        broker.enqueue(msg).await?;
    }
    println!("Enqueued 5 orders\n");

    let metrics = MetricsMiddleware::new();

    let worker = KojinBuilder::new(broker)
        .register_task::<ProcessOrder>()
        .middleware(TracingMiddleware)
        .middleware(metrics.clone())
        .concurrency(2)
        .queues(queues)
        .build();

    let cancel = worker.cancel_token();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        cancel.cancel();
    });

    println!("Starting worker...\n");
    worker.run().await;

    println!("\n=== Metrics ===");
    println!("  started:   {}", metrics.tasks_started());
    println!("  succeeded: {}", metrics.tasks_succeeded());
    println!("  failed:    {}", metrics.tasks_failed());

    Ok(())
}
