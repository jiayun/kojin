//! [`ClaudeCodeTask`] — a kojin [`Task`] that invokes Claude Code.
//!
//! This is the core primitive that bridges kojin's distributed task queue
//! to AI agent execution. Each task carries a prompt and configuration;
//! the actual execution is delegated to a [`ClaudeRunner`] injected via
//! [`TaskContext`].
//!
//! # Setup
//!
//! ```no_run
//! use std::sync::Arc;
//! use kojin_agent::{ClaudeCodeTask, ProcessRunner, ClaudeRunner};
//!
//! # fn example() {
//! // Inject the runner into the worker via KojinBuilder::data()
//! let runner: Arc<dyn ClaudeRunner> = Arc::new(ProcessRunner::new());
//!
//! // let worker = KojinBuilder::new(broker)
//! //     .register_task::<ClaudeCodeTask>()
//! //     .data(runner)
//! //     .build();
//! # }
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use kojin_core::context::TaskContext;
use kojin_core::error::{KojinError, TaskResult};
use kojin_core::task::Task;
use serde::{Deserialize, Serialize};

use crate::runner::{ClaudeRunner, RunArgs, RunOutput};

/// A kojin task that executes a Claude Code agent invocation.
///
/// The task is serializable — only the prompt and arguments are stored in
/// the message payload. The [`ClaudeRunner`] implementation is resolved
/// at execution time from [`TaskContext`].
///
/// # Examples
///
/// ```
/// use kojin_agent::{ClaudeCodeTask, RunArgs};
///
/// let task = ClaudeCodeTask::with_args(
///     "Find and fix security issues in auth.rs",
///     RunArgs::default()
///         .with_model("opus")
///         .with_max_turns(10),
/// );
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeCodeTask {
    /// The prompt to send to Claude Code.
    pub prompt: String,

    /// Configuration for this invocation.
    pub args: RunArgs,
}

impl ClaudeCodeTask {
    /// Create a new task with the given prompt and default arguments.
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            args: RunArgs::default(),
        }
    }

    /// Create a new task with the given prompt and arguments.
    pub fn with_args(prompt: impl Into<String>, args: RunArgs) -> Self {
        Self {
            prompt: prompt.into(),
            args,
        }
    }
}

#[async_trait]
impl Task for ClaudeCodeTask {
    const NAME: &'static str = "claude_code";
    const QUEUE: &'static str = "agents";
    const MAX_RETRIES: u32 = 1; // LLM calls are expensive; limit retries

    type Output = RunOutput;

    async fn run(&self, ctx: &TaskContext) -> TaskResult<Self::Output> {
        let runner = ctx.data::<Arc<dyn ClaudeRunner>>().ok_or_else(|| {
            KojinError::Other(
                "ClaudeRunner not configured — add Arc<dyn ClaudeRunner> via KojinBuilder::data()"
                    .into(),
            )
        })?;
        runner.run(&self.prompt, &self.args).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_name() {
        assert_eq!(ClaudeCodeTask::NAME, "claude_code");
        assert_eq!(ClaudeCodeTask::QUEUE, "agents");
        assert_eq!(ClaudeCodeTask::MAX_RETRIES, 1);
    }

    #[test]
    fn serialization_roundtrip() {
        let task = ClaudeCodeTask::with_args(
            "Review this code",
            RunArgs {
                model: Some("sonnet".into()),
                max_turns: Some(3),
                ..Default::default()
            },
        );

        let json = serde_json::to_value(&task).unwrap();
        let deserialized: ClaudeCodeTask = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.prompt, "Review this code");
        assert_eq!(deserialized.args.model.as_deref(), Some("sonnet"));
    }

    #[test]
    fn signature_produces_correct_task_name() {
        let task = ClaudeCodeTask::new("test prompt");
        let sig = task.signature();
        assert_eq!(sig.task_name, "claude_code");
        assert_eq!(sig.queue, "agents");
    }
}
