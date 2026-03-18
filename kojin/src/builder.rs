use std::sync::Arc;

use kojin_core::broker::Broker;
use kojin_core::context::TaskContext;
use kojin_core::middleware::Middleware;
use kojin_core::registry::TaskRegistry;
use kojin_core::result_backend::ResultBackend;
use kojin_core::task::Task;
use kojin_core::worker::{Worker, WorkerConfig};

/// Builder for configuring and running a Kojin worker.
pub struct KojinBuilder<B: Broker> {
    broker: B,
    registry: TaskRegistry,
    context: TaskContext,
    middlewares: Vec<Box<dyn Middleware>>,
    config: WorkerConfig,
    result_backend: Option<Arc<dyn ResultBackend>>,
    #[cfg(feature = "cron")]
    cron_registry: kojin_core::cron::CronRegistry,
}

impl<B: Broker> KojinBuilder<B> {
    /// Create a new builder with the given broker.
    pub fn new(broker: B) -> Self {
        Self {
            broker,
            registry: TaskRegistry::new(),
            context: TaskContext::new(),
            middlewares: Vec::new(),
            config: WorkerConfig::default(),
            result_backend: None,
            #[cfg(feature = "cron")]
            cron_registry: kojin_core::cron::CronRegistry::new(),
        }
    }

    /// Register a task type.
    pub fn register_task<T: Task>(mut self) -> Self {
        self.registry.register::<T>();
        self
    }

    /// Add shared data accessible via `ctx.data::<T>()`.
    pub fn data<T: Send + Sync + 'static>(mut self, value: T) -> Self {
        self.context.insert(value);
        self
    }

    /// Add a middleware.
    pub fn middleware(mut self, mw: impl Middleware) -> Self {
        self.middlewares.push(Box::new(mw));
        self
    }

    /// Set concurrency (max parallel tasks).
    pub fn concurrency(mut self, n: usize) -> Self {
        self.config.concurrency = n;
        self
    }

    /// Set queues to consume.
    pub fn queues(mut self, queues: Vec<String>) -> Self {
        self.config.queues = queues;
        self
    }

    /// Set shutdown timeout.
    pub fn shutdown_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.config.shutdown_timeout = timeout;
        self
    }

    /// Set the result backend for storing task results and workflow coordination.
    pub fn result_backend(mut self, rb: impl ResultBackend) -> Self {
        self.result_backend = Some(Arc::new(rb));
        self
    }

    /// Set a shared result backend (already wrapped in `Arc`).
    ///
    /// Use this when the same backend must be shared between
    /// `Canvas::apply()` and the worker.
    pub fn result_backend_shared(mut self, rb: Arc<dyn ResultBackend>) -> Self {
        self.result_backend = Some(rb);
        self
    }

    /// Add a cron schedule entry.
    #[cfg(feature = "cron")]
    pub fn cron(
        mut self,
        name: impl Into<String>,
        expression: &str,
        signature: kojin_core::signature::Signature,
    ) -> Self {
        match kojin_core::cron::CronEntry::new(name, expression, signature) {
            Ok(entry) => self.cron_registry.add(entry),
            Err(e) => panic!("Invalid cron expression: {e}"),
        }
        self
    }

    /// Build the worker.
    pub fn build(self) -> Worker<B> {
        let mut worker = Worker::new(self.broker, self.registry, self.context, self.config);
        for mw in self.middlewares {
            worker = worker.with_middleware_boxed(mw);
        }
        if let Some(rb) = self.result_backend {
            worker = worker.with_result_backend(rb);
        }
        #[cfg(feature = "cron")]
        if !self.cron_registry.is_empty() {
            worker = worker.with_cron_registry(self.cron_registry);
        }
        worker
    }
}
