//! AWS SQS broker for the kojin task queue.
//!
//! Provides [`SqsBroker`] — a broker implementation using AWS SQS with support
//! for both standard and FIFO queues.
//!
//! # Features
//!
//! - Standard and FIFO queue auto-detection (by URL suffix `.fifo`)
//! - Long polling for efficient message consumption
//! - `DelaySeconds` for short scheduling (≤900s)
//! - Native deduplication via `MessageDeduplicationId` for FIFO queues
//!
//! # Priority
//!
//! SQS does not support message priority. Messages with `priority` set will
//! log a warning and the field is ignored.

pub mod broker;
pub mod config;

pub use broker::SqsBroker;
pub use config::SqsConfig;
