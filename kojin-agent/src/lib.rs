//! AI agent orchestration for kojin.
//!
//! `kojin-agent` bridges kojin's distributed task queue to Claude Code,
//! enabling multi-agent orchestration with workflows, concurrency control,
//! and pluggable execution backends.
//!
//! # Overview
//!
//! | Component | Purpose |
//! |-----------|---------|
//! | [`ClaudeRunner`] | Trait for executing Claude Code (implement for your environment) |
//! | [`ProcessRunner`] | Default implementation — spawns `claude` as a child process |
//! | [`SemaphoreRunner`] | Concurrency limiter wrapping any `ClaudeRunner` |
//! | [`ClaudeCodeTask`] | Kojin [`Task`] that delegates to a `ClaudeRunner` |
//! | [`claude_sig`] | Helper to build workflow [`Signature`]s for agent tasks |
//!
//! # Quick Start
//!
//! ```no_run
//! use std::sync::Arc;
//! use kojin_agent::{
//!     ClaudeCodeTask, ClaudeRunner, ProcessRunner, SemaphoreRunner, RunArgs,
//! };
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // 1. Create a runner (with concurrency limit)
//! let runner = SemaphoreRunner::new(ProcessRunner::new(), 3);
//! let runner: Arc<dyn ClaudeRunner> = Arc::new(runner);
//!
//! // 2. Build a worker that processes agent tasks
//! // let worker = KojinBuilder::new(broker)
//! //     .register_task::<ClaudeCodeTask>()
//! //     .data(runner)
//! //     .queues(vec!["agents".into()])
//! //     .build();
//!
//! // 3. Enqueue agent tasks (from producer or another worker)
//! // let sig = claude_sig("Review this PR", RunArgs::default());
//! // sig.into_message();
//! # Ok(())
//! # }
//! ```
//!
//! # Workflow Composition
//!
//! Agent tasks produce standard [`Signature`]s, so they compose with
//! kojin's `chain!`, `group!`, and `chord` macros:
//!
//! ```
//! use kojin_agent::{claude_sig, RunArgs};
//! use kojin_core::{chain, group, chord};
//!
//! let args = RunArgs::default();
//!
//! // Parallel review
//! let review = group![
//!     claude_sig("Review auth module", args.clone()),
//!     claude_sig("Review data module", args.clone()),
//! ];
//!
//! // Sequential pipeline
//! let pipeline = chain![
//!     claude_sig("Write implementation", args.clone()),
//!     claude_sig("Write tests", args.clone()),
//! ];
//! ```
//!
//! [`Task`]: kojin_core::task::Task
//! [`Signature`]: kojin_core::signature::Signature

pub mod concurrency;
pub mod helpers;
pub mod runner;
pub mod task;

// Re-export primary types for convenience
pub use concurrency::SemaphoreRunner;
pub use helpers::claude_sig;
pub use runner::{ClaudeRunner, ProcessRunner, RunArgs, RunOutput};
pub use task::ClaudeCodeTask;
