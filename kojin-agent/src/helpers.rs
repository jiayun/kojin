//! Convenience helpers for building agent workflow signatures.
//!
//! Use [`claude_sig`] to create a [`Signature`] for a [`ClaudeCodeTask`],
//! then compose with kojin's `chain!`, `group!`, and `chord` macros.
//!
//! # Examples
//!
//! ```
//! use kojin_agent::{claude_sig, RunArgs};
//!
//! let args = RunArgs::default()
//!     .with_model("sonnet")
//!     .with_max_turns(5);
//!
//! // Single signature
//! let sig = claude_sig("Review auth.rs for security issues", args);
//! assert_eq!(sig.task_name, "claude_code");
//! assert_eq!(sig.queue, "agents");
//! ```

use kojin_core::signature::Signature;
use kojin_core::task::Task;

use crate::runner::RunArgs;
use crate::task::ClaudeCodeTask;

/// Build a [`Signature`] for a [`ClaudeCodeTask`] invocation.
///
/// This is the primary entry point for composing agent workflows:
///
/// ```
/// use kojin_agent::{claude_sig, RunArgs};
/// use kojin_core::{chain, group, chord};
///
/// let args = RunArgs::default().with_model("sonnet");
///
/// // Parallel code review
/// let review = group![
///     claude_sig("Review auth.rs", args.clone()),
///     claude_sig("Review db.rs", args.clone()),
///     claude_sig("Review api.rs", args.clone()),
/// ];
///
/// // Sequential pipeline
/// let pipeline = chain![
///     claude_sig("Write tests", args.clone()),
///     claude_sig("Review tests", args.clone()),
/// ];
///
/// // Fan-out / fan-in
/// let workflow = chord(
///     vec![
///         claude_sig("Audit module A", args.clone()),
///         claude_sig("Audit module B", args.clone()),
///     ],
///     claude_sig("Summarize findings", args.clone()),
/// );
/// ```
pub fn claude_sig(prompt: impl Into<String>, args: RunArgs) -> Signature {
    ClaudeCodeTask::with_args(prompt, args).signature()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_sig_produces_valid_signature() {
        let sig = claude_sig("test prompt", RunArgs::default());
        assert_eq!(sig.task_name, "claude_code");
        assert_eq!(sig.queue, "agents");
    }

    #[test]
    fn claude_sig_with_custom_args() {
        let sig = claude_sig(
            "review code",
            RunArgs {
                model: Some("opus".into()),
                ..Default::default()
            },
        );
        // Payload should contain the model
        let payload = sig.payload.to_string();
        assert!(payload.contains("opus"));
    }
}
