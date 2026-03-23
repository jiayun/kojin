use std::sync::Arc;

use kojin::{
    DashboardState, KojinBuilder, MetricsMiddleware, PostgresResultBackend, RedisBroker,
    RedisConfig, TracingMiddleware, spawn_dashboard,
};
use kojin_examples::{AddTask, PriorityTask, ProcessOrder};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".into());

    let config = RedisConfig::new(&redis_url);
    let broker = RedisBroker::new(config).await?;

    let mut builder = KojinBuilder::new(broker.clone())
        .register_task::<AddTask>()
        .register_task::<ProcessOrder>()
        .register_task::<PriorityTask>()
        .middleware(TracingMiddleware)
        .concurrency(4)
        .queues(vec![
            "default".into(),
            "orders".into(),
            "high".into(),
            "medium".into(),
            "low".into(),
        ]);

    // Optional: PostgreSQL result backend
    let backend = if let Ok(pg_url) = std::env::var("POSTGRES_URL") {
        tracing::info!("connecting to PostgreSQL result backend");
        let backend = PostgresResultBackend::connect(&pg_url).await?;
        backend.migrate().await?;
        let backend = Arc::new(backend);
        builder = builder.result_backend_shared(backend.clone());
        Some(backend)
    } else {
        None
    };

    // Optional: dashboard
    let metrics = MetricsMiddleware::new();
    builder = builder.middleware(metrics.clone());

    if std::env::var("DASHBOARD").is_ok() {
        let port: u16 = std::env::var("DASHBOARD_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(9090);

        let mut state = DashboardState::new(Arc::new(broker)).with_metrics(metrics);
        if let Some(ref b) = backend {
            state = state.with_result_backend(b.clone());
        }
        let _handle = spawn_dashboard(state, port);
        tracing::info!("dashboard listening on http://0.0.0.0:{port}");
    }

    let worker = builder.build();
    let cancel = worker.cancel_token();

    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("shutting down");
        cancel.cancel();
    });

    tracing::info!("worker started");
    worker.run().await;
    tracing::info!("worker stopped");

    Ok(())
}
