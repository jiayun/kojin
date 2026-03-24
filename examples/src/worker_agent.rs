use std::sync::Arc;

use kojin::{
    ClaudeCodeTask, ClaudeRunner, KojinBuilder, ProcessRunner, RedisBroker, RedisConfig,
    SemaphoreRunner, TracingMiddleware,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".into());

    let config = RedisConfig::new(&redis_url);
    let broker = RedisBroker::new(config).await?;

    // Create runner with concurrency limit (max 3 simultaneous claude processes)
    let max_concurrent: usize = std::env::var("MAX_CONCURRENT_AGENTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);

    let runner = SemaphoreRunner::new(ProcessRunner::new(), max_concurrent);
    let runner: Arc<dyn ClaudeRunner> = Arc::new(runner);

    let worker = KojinBuilder::new(broker)
        .register_task::<ClaudeCodeTask>()
        .data(runner)
        .middleware(TracingMiddleware)
        .concurrency(max_concurrent)
        .queues(vec!["agents".into()])
        .build();

    let cancel = worker.cancel_token();

    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("shutting down");
        cancel.cancel();
    });

    tracing::info!(
        max_concurrent,
        "agent worker started (polling 'agents' queue)"
    );
    worker.run().await;
    tracing::info!("agent worker stopped");

    Ok(())
}
