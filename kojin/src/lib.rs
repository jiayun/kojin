//! # kojin
//!
//! Async distributed task queue for Rust — the equivalent of Celery (Python),
//! BullMQ (Node.js), Sidekiq (Ruby), and Machinery (Go).
//!
//! This is the main facade crate. It re-exports types from [`kojin_core`],
//! the `#[task]` proc-macro from [`kojin_macros`], and optionally the Redis
//! broker from [`kojin_redis`].
//!
//! ## Features
//!
//! - Async-first worker runtime built on Tokio
//! - `#[kojin::task]` proc-macro for defining tasks
//! - Pluggable broker trait (Redis included)
//! - Composable middleware (tracing, metrics)
//! - Workflow orchestration: chain, group, chord
//! - Result backends (memory, Redis, PostgreSQL)
//! - Graceful shutdown, weighted queues, configurable retries
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use kojin::{KojinBuilder, MemoryBroker};
//!
//! # #[tokio::main]
//! # async fn main() {
//! let broker = MemoryBroker::new();
//! let worker = KojinBuilder::new(broker)
//!     .concurrency(4)
//!     .queues(vec!["default".into()])
//!     .build();
//! worker.run().await;
//! # }
//! ```

// Re-export everything from kojin-core
pub use kojin_core::*;

// Re-export the task proc-macro
pub use kojin_macros::task;

// Re-export Redis broker + result backend when feature is enabled
#[cfg(feature = "redis")]
pub use kojin_redis::{RedisBroker, RedisConfig, RedisResultBackend};

// Re-export PostgreSQL result backend when feature is enabled
#[cfg(feature = "postgres")]
pub use kojin_postgres::PostgresResultBackend;

// Re-export AMQP broker when feature is enabled
#[cfg(feature = "amqp")]
pub use kojin_amqp::{AmqpBroker, AmqpConfig};

// Re-export SQS broker when feature is enabled
#[cfg(feature = "sqs")]
pub use kojin_sqs::{SqsBroker, SqsConfig};

// Re-export dashboard when feature is enabled
#[cfg(feature = "dashboard")]
pub use kojin_dashboard::{DashboardConfig, DashboardState, dashboard_router, spawn_dashboard};

// Re-export agent orchestration when feature is enabled
#[cfg(feature = "agent")]
pub use kojin_agent::{
    ClaudeCodeTask, ClaudeRunner, ProcessRunner, RunArgs, RunOutput, SddOutput, SddTask,
    SemaphoreRunner, claude_sig, sdd_sig,
};

mod builder;
pub use builder::KojinBuilder;
