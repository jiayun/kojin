use std::sync::Arc;

use kojin_core::broker::Broker;
use kojin_core::middleware::MetricsMiddleware;
use kojin_core::result_backend::ResultBackend;

/// Shared state for dashboard API handlers.
#[derive(Clone)]
pub struct DashboardState {
    pub broker: Arc<dyn Broker>,
    pub result_backend: Option<Arc<dyn ResultBackend>>,
    pub metrics: Option<MetricsMiddleware>,
}

impl DashboardState {
    pub fn new(broker: Arc<dyn Broker>) -> Self {
        Self {
            broker,
            result_backend: None,
            metrics: None,
        }
    }

    pub fn with_result_backend(mut self, rb: Arc<dyn ResultBackend>) -> Self {
        self.result_backend = Some(rb);
        self
    }

    pub fn with_metrics(mut self, metrics: MetricsMiddleware) -> Self {
        self.metrics = Some(metrics);
        self
    }
}
