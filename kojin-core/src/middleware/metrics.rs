use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::Middleware;
use crate::error::KojinError;
use crate::message::TaskMessage;

/// Simple metrics middleware that tracks task counts.
#[derive(Debug, Clone)]
pub struct MetricsMiddleware {
    inner: Arc<MetricsInner>,
}

#[derive(Debug)]
struct MetricsInner {
    tasks_started: AtomicU64,
    tasks_succeeded: AtomicU64,
    tasks_failed: AtomicU64,
}

impl MetricsMiddleware {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(MetricsInner {
                tasks_started: AtomicU64::new(0),
                tasks_succeeded: AtomicU64::new(0),
                tasks_failed: AtomicU64::new(0),
            }),
        }
    }

    pub fn tasks_started(&self) -> u64 {
        self.inner.tasks_started.load(Ordering::Relaxed)
    }

    pub fn tasks_succeeded(&self) -> u64 {
        self.inner.tasks_succeeded.load(Ordering::Relaxed)
    }

    pub fn tasks_failed(&self) -> u64 {
        self.inner.tasks_failed.load(Ordering::Relaxed)
    }
}

impl Default for MetricsMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Middleware for MetricsMiddleware {
    async fn before(&self, _message: &TaskMessage) -> Result<(), KojinError> {
        self.inner.tasks_started.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn after(
        &self,
        _message: &TaskMessage,
        _result: &serde_json::Value,
    ) -> Result<(), KojinError> {
        self.inner.tasks_succeeded.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn on_error(
        &self,
        _message: &TaskMessage,
        _error: &KojinError,
    ) -> Result<(), KojinError> {
        self.inner.tasks_failed.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn metrics_increments() {
        let mw = MetricsMiddleware::new();
        let msg = TaskMessage::new("test", "default", serde_json::json!({}));

        mw.before(&msg).await.unwrap();
        mw.before(&msg).await.unwrap();
        mw.after(&msg, &serde_json::json!("ok")).await.unwrap();
        mw.on_error(&msg, &KojinError::TaskFailed("err".into()))
            .await
            .unwrap();

        assert_eq!(mw.tasks_started(), 2);
        assert_eq!(mw.tasks_succeeded(), 1);
        assert_eq!(mw.tasks_failed(), 1);
    }
}
