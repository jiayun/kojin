mod metrics;
mod tracing_mw;

pub use metrics::MetricsMiddleware;
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
