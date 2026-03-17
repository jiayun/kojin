use std::time::Duration;

use async_trait::async_trait;
use kojin::{
    Broker, KojinBuilder, MemoryBroker, Task, TaskContext, TaskMessage, TaskResult,
    TracingMiddleware,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct SendEmail {
    to: String,
    subject: String,
}

#[async_trait]
impl Task for SendEmail {
    const NAME: &'static str = "send_email";
    const QUEUE: &'static str = "emails";
    const MAX_RETRIES: u32 = 3;

    type Output = String;

    async fn run(&self, _ctx: &TaskContext) -> TaskResult<Self::Output> {
        println!(
            "Sending email to {} with subject: {}",
            self.to, self.subject
        );
        Ok(format!("Email sent to {}", self.to))
    }
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let broker = MemoryBroker::new();

    // Enqueue some tasks
    for i in 0..5 {
        let msg = TaskMessage::new(
            "send_email",
            "emails",
            serde_json::to_value(&SendEmail {
                to: format!("user{i}@example.com"),
                subject: format!("Hello #{i}"),
            })
            .unwrap(),
        );
        broker.enqueue(msg).await.unwrap();
    }

    println!("Enqueued 5 tasks");

    // Build and run worker
    let worker = KojinBuilder::new(broker)
        .register_task::<SendEmail>()
        .middleware(TracingMiddleware)
        .concurrency(2)
        .queues(vec!["emails".to_string()])
        .build();

    let cancel = worker.cancel_token();

    // Auto-shutdown after tasks are processed
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        cancel.cancel();
    });

    worker.run().await;
    println!("Worker stopped");
}
