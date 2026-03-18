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

// Re-export Redis broker when feature is enabled
#[cfg(feature = "redis")]
pub use kojin_redis::{RedisBroker, RedisConfig};

mod builder;
pub use builder::KojinBuilder;
