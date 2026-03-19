use std::sync::Arc;

use kojin_core::MemoryBroker;
use kojin_dashboard::{DashboardConfig, DashboardState, dashboard_router};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();

    let config = DashboardConfig::default();

    // Standalone mode uses an in-memory broker for demonstration.
    // In production, connect to your real broker.
    let broker = Arc::new(MemoryBroker::new());
    let state = DashboardState::new(broker);
    let app = dashboard_router(state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port)).await?;
    tracing::info!(port = config.port, "kojin dashboard listening");
    axum::serve(listener, app).await
}
