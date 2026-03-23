use kojin::{AmqpBroker, AmqpConfig, KojinBuilder, MetricsMiddleware, TracingMiddleware};
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

    let metrics = MetricsMiddleware::new();

    let worker = KojinBuilder::new(broker)
        .register_task::<PriorityTask>()
        .middleware(TracingMiddleware)
        .middleware(metrics)
        .concurrency(1) // single consumer to clearly show priority ordering
        .queues(queues)
        .build();

    let cancel = worker.cancel_token();

    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("shutting down");
        cancel.cancel();
    });

    tracing::info!("worker started (concurrency=1 to demonstrate priority ordering)");
    worker.run().await;
    tracing::info!("worker stopped");

    Ok(())
}
