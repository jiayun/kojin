//! SQS broker example — enqueue/dequeue round-trip via Amazon SQS.
//!
//! Prerequisites:
//!   # Start LocalStack
//!   docker run -d --name localstack -p 4566:4566 localstack/localstack
//!
//!   # Create a queue
//!   aws --endpoint-url http://localhost:4566 sqs create-queue --queue-name orders
//!
//! Run with:
//!   AWS_ACCESS_KEY_ID=test AWS_SECRET_ACCESS_KEY=test AWS_DEFAULT_REGION=us-east-1 \
//!     SQS_ENDPOINT=http://localhost:4566 \
//!     SQS_QUEUE_URL=http://sqs.us-east-1.localhost.localstack.cloud:4566/000000000000/orders \
//!     cargo run --example sqs --features sqs

use std::time::Duration;

use async_trait::async_trait;
use kojin::{
    Broker, KojinBuilder, MetricsMiddleware, SqsBroker, SqsConfig, Task, TaskContext, TaskMessage,
    TaskResult, TracingMiddleware,
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

    // Load AWS config — set SQS_ENDPOINT for LocalStack or other custom endpoints
    let mut config_loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
    if let Ok(endpoint) = std::env::var("SQS_ENDPOINT") {
        config_loader = config_loader.endpoint_url(&endpoint);
    }
    let sdk_config = config_loader.load().await;

    let queue_url = std::env::var("SQS_QUEUE_URL")
        .unwrap_or_else(|_| "http://localhost:4566/000000000000/orders".to_string());
    let config = SqsConfig::new(vec![queue_url.to_string()]);
    let broker = SqsBroker::new(&sdk_config, config);
    println!("Connected to SQS");

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
    let queues = vec!["orders".to_string()];

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
