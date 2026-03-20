#[cfg(feature = "dedup")]
pub mod dedup;
mod metrics;
#[cfg(feature = "otel")]
pub mod otel;
#[cfg(feature = "rate-limit")]
pub mod rate_limit;
mod tracing_mw;

#[cfg(feature = "dedup")]
pub use dedup::DeduplicationMiddleware;
pub use metrics::MetricsMiddleware;
#[cfg(feature = "otel")]
pub use otel::OtelMiddleware;
#[cfg(feature = "rate-limit")]
pub use rate_limit::RateLimitMiddleware;
pub use tracing_mw::TracingMiddleware;

use async_trait::async_trait;

use crate::error::KojinError;
use crate::message::TaskMessage;

/// Middleware hook for task execution pipeline.
#[async_trait]
pub trait Middleware: Send + Sync + 'static {
    /// Called before task execution. Return `Err` to abort.
    async fn before(&self, message: &TaskMessage) -> Result<(), KojinError> {
        let _ = message;
        Ok(())
    }

    /// Called after successful task execution.
    async fn after(
        &self,
        message: &TaskMessage,
        result: &serde_json::Value,
    ) -> Result<(), KojinError> {
        let _ = (message, result);
        Ok(())
    }

    /// Called when task execution fails.
    async fn on_error(&self, message: &TaskMessage, error: &KojinError) -> Result<(), KojinError> {
        let _ = (message, error);
        Ok(())
    }
}
