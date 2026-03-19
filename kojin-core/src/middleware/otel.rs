use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use async_trait::async_trait;
use opentelemetry::metrics::{Counter, Histogram, Meter};
use opentelemetry::{KeyValue, global};

use super::Middleware;
use crate::error::KojinError;
use crate::message::TaskMessage;
use crate::task_id::TaskId;

/// OpenTelemetry metrics middleware.
///
/// Records task execution metrics via the OpenTelemetry metrics API:
/// - `kojin.task.started` (counter) — incremented in `before()`
/// - `kojin.task.succeeded` (counter) — incremented in `after()`
/// - `kojin.task.failed` (counter) — incremented in `on_error()`
/// - `kojin.task.duration` (histogram, ms) — recorded in `after()` / `on_error()`
///
/// Attributes on every metric: `task.name`, `task.queue`.
///
/// Pair this with `TracingMiddleware` and a `tracing-opentelemetry` subscriber
/// layer to get distributed traces exported alongside metrics.
pub struct OtelMiddleware {
    started: Counter<u64>,
    succeeded: Counter<u64>,
    failed: Counter<u64>,
    duration: Histogram<f64>,
    timings: Mutex<HashMap<TaskId, Instant>>,
}

impl OtelMiddleware {
    /// Create the middleware using the global `MeterProvider`.
    pub fn new() -> Self {
        let meter = global::meter("kojin");
        Self::from_meter(&meter)
    }

    /// Create the middleware from an explicit `Meter`.
    pub fn from_meter(meter: &Meter) -> Self {
        Self {
            started: meter
                .u64_counter("kojin.task.started")
                .with_description("Number of tasks started")
                .build(),
            succeeded: meter
                .u64_counter("kojin.task.succeeded")
                .with_description("Number of tasks that succeeded")
                .build(),
            failed: meter
                .u64_counter("kojin.task.failed")
                .with_description("Number of tasks that failed")
                .build(),
            duration: meter
                .f64_histogram("kojin.task.duration")
                .with_description("Task execution duration")
                .with_unit("ms")
                .build(),
            timings: Mutex::new(HashMap::new()),
        }
    }

    fn attrs(message: &TaskMessage) -> [KeyValue; 2] {
        [
            KeyValue::new("task.name", message.task_name.clone()),
            KeyValue::new("task.queue", message.queue.clone()),
        ]
    }

    fn record_duration(&self, message: &TaskMessage) {
        if let Some(start) = self.timings.lock().unwrap().remove(&message.id) {
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
            self.duration.record(elapsed_ms, &Self::attrs(message));
        }
    }
}

impl Default for OtelMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Middleware for OtelMiddleware {
    async fn before(&self, message: &TaskMessage) -> Result<(), KojinError> {
        self.timings
            .lock()
            .unwrap()
            .insert(message.id, Instant::now());
        self.started.add(1, &Self::attrs(message));
        Ok(())
    }

    async fn after(
        &self,
        message: &TaskMessage,
        _result: &serde_json::Value,
    ) -> Result<(), KojinError> {
        self.record_duration(message);
        self.succeeded.add(1, &Self::attrs(message));
        Ok(())
    }

    async fn on_error(&self, message: &TaskMessage, _error: &KojinError) -> Result<(), KojinError> {
        self.record_duration(message);
        self.failed.add(1, &Self::attrs(message));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn otel_middleware_records_metrics() {
        let mw = OtelMiddleware::new();
        let msg = TaskMessage::new("test_task", "default", serde_json::json!({}));

        mw.before(&msg).await.unwrap();
        mw.after(&msg, &serde_json::json!("ok")).await.unwrap();

        // Timing entry should be cleaned up after after()
        assert!(!mw.timings.lock().unwrap().contains_key(&msg.id));
    }

    #[tokio::test]
    async fn otel_middleware_records_on_error() {
        let mw = OtelMiddleware::new();
        let msg = TaskMessage::new("fail_task", "default", serde_json::json!({}));

        mw.before(&msg).await.unwrap();
        mw.on_error(&msg, &KojinError::TaskFailed("boom".into()))
            .await
            .unwrap();

        assert!(!mw.timings.lock().unwrap().contains_key(&msg.id));
    }
}
