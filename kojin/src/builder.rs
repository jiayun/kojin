use kojin_core::broker::Broker;
use kojin_core::context::TaskContext;
use kojin_core::middleware::Middleware;
use kojin_core::registry::TaskRegistry;
use kojin_core::task::Task;
use kojin_core::worker::{Worker, WorkerConfig};

/// Builder for configuring and running a Kojin worker.
pub struct KojinBuilder<B: Broker> {
    broker: B,
    registry: TaskRegistry,
    context: TaskContext,
    middlewares: Vec<Box<dyn Middleware>>,
    config: WorkerConfig,
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

    /// Build the worker.
    pub fn build(self) -> Worker<B> {
        let mut worker = Worker::new(self.broker, self.registry, self.context, self.config);
        for mw in self.middlewares {
            worker = worker.with_middleware_boxed(mw);
        }
        worker
    }
}
