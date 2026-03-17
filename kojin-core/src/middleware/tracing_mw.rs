use async_trait::async_trait;

use super::Middleware;
use crate::error::KojinError;
use crate::message::TaskMessage;

/// Middleware that emits tracing spans for task execution.
#[derive(Debug, Default)]
pub struct TracingMiddleware;

#[async_trait]
impl Middleware for TracingMiddleware {
    async fn before(&self, message: &TaskMessage) -> Result<(), KojinError> {
        tracing::info!(
            task_id = %message.id,
            task_name = %message.task_name,
            queue = %message.queue,
            retries = message.retries,
            "Task starting"
        );
        Ok(())
    }

    async fn after(
        &self,
        message: &TaskMessage,
        _result: &serde_json::Value,
    ) -> Result<(), KojinError> {
        tracing::info!(
            task_id = %message.id,
            task_name = %message.task_name,
            "Task completed"
        );
        Ok(())
    }

    async fn on_error(&self, message: &TaskMessage, error: &KojinError) -> Result<(), KojinError> {
        tracing::error!(
            task_id = %message.id,
            task_name = %message.task_name,
            error = %error,
            retries = message.retries,
            max_retries = message.max_retries,
            "Task failed"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn tracing_middleware_does_not_error() {
        let mw = TracingMiddleware;
        let msg = TaskMessage::new("test_task", "default", serde_json::json!({}));

        assert!(mw.before(&msg).await.is_ok());
        assert!(mw.after(&msg, &serde_json::json!("ok")).await.is_ok());
        assert!(
            mw.on_error(&msg, &KojinError::TaskFailed("err".into()))
                .await
                .is_ok()
        );
    }
}
