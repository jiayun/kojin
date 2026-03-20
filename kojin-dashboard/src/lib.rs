//! JSON API dashboard for monitoring the kojin task queue.
//!
//! Provides both an embeddable [`axum::Router`] via [`dashboard_router()`] and a
//! standalone binary.
//!
//! # Endpoints
//!
//! | Endpoint | Description |
//! |----------|-------------|
//! | `GET /` | Web UI dashboard |
//! | `GET /healthz` | K8s liveness/readiness probe |
//! | `GET /api/queues` | List queues with lengths + DLQ lengths |
//! | `GET /api/queues/:name` | Single queue detail |
//! | `GET /api/queues/:name/dlq` | Paginated DLQ messages |
//! | `GET /api/metrics` | Throughput counters (requires `MetricsMiddleware`) |
//! | `GET /api/tasks/:id` | Task result lookup (requires `ResultBackend`) |
//!
//! # Example
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use kojin_core::MemoryBroker;
//! use kojin_dashboard::{DashboardState, dashboard_router};
//!
//! # #[tokio::main]
//! # async fn main() {
//! let broker = Arc::new(MemoryBroker::new());
//! let state = DashboardState::new(broker);
//! let app = dashboard_router(state);
//!
//! let listener = tokio::net::TcpListener::bind("0.0.0.0:9090").await.unwrap();
//! axum::serve(listener, app).await.unwrap();
//! # }
//! ```

pub mod routes;
pub mod state;

pub use state::DashboardState;

use axum::Router;
use tower_http::cors::CorsLayer;

/// Configuration for the standalone dashboard server.
#[derive(Debug, Clone)]
pub struct DashboardConfig {
    /// Port to listen on. Default: 9090.
    pub port: u16,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self { port: 9090 }
    }
}

/// Build the dashboard [`Router`] with all API routes and web UI.
pub fn dashboard_router(state: DashboardState) -> Router {
    Router::new()
        .route("/", axum::routing::get(routes::dashboard_ui))
        .route("/healthz", axum::routing::get(routes::healthz))
        .route("/api/queues", axum::routing::get(routes::list_queues))
        .route("/api/queues/{name}", axum::routing::get(routes::get_queue))
        .route(
            "/api/queues/{name}/dlq",
            axum::routing::get(routes::get_dlq),
        )
        .route("/api/metrics", axum::routing::get(routes::get_metrics))
        .route(
            "/api/tasks/{id}",
            axum::routing::get(routes::get_task_result),
        )
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Start the dashboard as a background Tokio task. Returns the join handle.
pub fn spawn_dashboard(
    state: DashboardState,
    port: u16,
) -> tokio::task::JoinHandle<std::io::Result<()>> {
    let app = dashboard_router(state);
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
        tracing::info!(port, "dashboard listening");
        axum::serve(listener, app).await
    })
}
