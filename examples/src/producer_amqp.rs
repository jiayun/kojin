use kojin::{AmqpBroker, AmqpConfig, Broker, TaskMessage};
use kojin_examples::PriorityTask;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let amqp_url = std::env::var("AMQP_URL")
        .unwrap_or_else(|_| "amqp://guest:guest@localhost:5672/%2f".into());

    let config = AmqpConfig::new(&amqp_url).with_max_priority(9);
    let queues = vec!["priority".to_string()];
    let broker = AmqpBroker::new(config, &queues).await?;

    tracing::info!("connected to RabbitMQ");

    // Enqueue LOW priority first (100 tasks) — these should be processed LAST
    tracing::info!("enqueuing 100 low-priority tasks (priority=1)");
    for i in 0..100 {
        let msg = TaskMessage::new(
            "priority_task",
            "priority",
            serde_json::to_value(&PriorityTask {
                label: format!("low-{i}"),
                priority: "low".into(),
            })
            .unwrap(),
        )
        .with_priority(1);
        broker.enqueue(msg).await?;
    }

    // Enqueue MEDIUM priority (5 tasks)
    tracing::info!("enqueuing 5 medium-priority tasks (priority=5)");
    for i in 0..5 {
        let msg = TaskMessage::new(
            "priority_task",
            "priority",
            serde_json::to_value(&PriorityTask {
                label: format!("medium-{i}"),
                priority: "medium".into(),
            })
            .unwrap(),
        )
        .with_priority(5);
        broker.enqueue(msg).await?;
    }

    // Enqueue HIGH priority last (3 tasks) — these should be processed FIRST
    tracing::info!("enqueuing 3 high-priority tasks (priority=9)");
    for i in 0..3 {
        let msg = TaskMessage::new(
            "priority_task",
            "priority",
            serde_json::to_value(&PriorityTask {
                label: format!("high-{i}"),
                priority: "high".into(),
            })
            .unwrap(),
        )
        .with_priority(9);
        broker.enqueue(msg).await?;
    }

    tracing::info!(
        "producer done — enqueued 108 tasks (100 low, 5 medium, 3 high) in reverse priority order"
    );

    Ok(())
}
