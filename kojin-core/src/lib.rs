//! Core traits, types, and worker runtime for the kojin task queue.
//!
//! This crate provides the foundational abstractions: [`Task`], [`Broker`],
//! [`Middleware`], [`Worker`], and supporting types. Most users should depend
//! on the [`kojin`](https://crates.io/crates/kojin) facade crate instead.

pub mod backoff;
pub mod broker;
pub mod canvas;
pub mod codec;
pub mod context;
pub mod error;
pub mod memory_broker;
pub mod memory_result_backend;
pub mod message;
pub mod middleware;
pub mod queue_weight;
pub mod registry;
pub mod result_backend;
pub mod shutdown;
pub mod signature;
pub mod state;
pub mod task;
pub mod task_id;
pub mod worker;

#[cfg(feature = "cron")]
pub mod cron;

// Re-export key types at crate root
pub use backoff::BackoffStrategy;
pub use broker::Broker;
pub use canvas::{Canvas, WorkflowHandle, chord};
pub use codec::{Codec, JsonCodec};
pub use context::TaskContext;
pub use error::{KojinError, TaskResult};
pub use memory_broker::MemoryBroker;
pub use memory_result_backend::MemoryResultBackend;
pub use message::TaskMessage;
#[cfg(feature = "otel")]
pub use middleware::OtelMiddleware;
#[cfg(feature = "rate-limit")]
pub use middleware::RateLimitMiddleware;
pub use middleware::{MetricsMiddleware, Middleware, TracingMiddleware};
pub use queue_weight::{QueueWeight, WeightedQueue};
pub use registry::TaskRegistry;
pub use result_backend::ResultBackend;
pub use signature::Signature;
pub use state::TaskState;
pub use task::Task;
pub use task_id::TaskId;
pub use worker::{Worker, WorkerConfig};
