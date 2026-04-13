//! Specification-Driven Development (SDD) task.
//!
//! [`SddTask`] implements a gen → test → fix loop: given a specification,
//! it asks Claude to generate code, runs a test command to verify, and
//! iterates on fixes until tests pass or the attempt limit is reached.
//!
//! # Examples
//!
//! ```
//! use kojin_agent::{SddTask, RunArgs};
//!
//! let task = SddTask::new("Add a `fibonacci(n: u32) -> u64` function that returns the nth Fibonacci number")
//!     .with_test_command(vec!["cargo".into(), "test".into()])
//!     .with_cwd("/path/to/repo")
//!     .with_max_fix_attempts(3)
//!     .with_args(RunArgs::default().with_model("sonnet").with_max_turns(10));
//! ```
//!
//! # Workflow Composition
//!
//! Use [`sdd_sig`] to compose SDD tasks into parallel or sequential workflows:
//!
//! ```
//! use kojin_agent::{sdd_sig, RunArgs};
//! use kojin_core::group;
//!
//! let args = RunArgs::default().with_model("sonnet");
//!
//! // Parallel SDD for multiple modules
//! let workflow = group![
//!     sdd_sig("auth module spec", vec!["cargo".into(), "test".into(), "-p".into(), "auth".into()], args.clone()),
//!     sdd_sig("db module spec", vec!["cargo".into(), "test".into(), "-p".into(), "db".into()], args.clone()),
//! ];
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use kojin_core::context::TaskContext;
use kojin_core::error::{KojinError, TaskResult};
use kojin_core::signature::Signature;
use kojin_core::task::Task;
use serde::{Deserialize, Serialize};

use crate::runner::{ClaudeRunner, RunArgs, RunOutput};

/// A kojin task that implements the SDD gen → test → fix loop.
///
/// Given a specification, the task:
/// 1. Asks Claude to generate code that satisfies the spec
/// 2. Runs `test_command` to verify the generated code
/// 3. If tests fail, asks Claude to fix the code based on test output
/// 4. Repeats steps 2-3 up to `max_fix_attempts` times
///
/// The `cwd` field is critical — all Claude processes work in the same
/// directory, so the file system acts as shared context between steps.
///
/// # Examples
///
/// ```
/// use kojin_agent::{SddTask, RunArgs};
///
/// let task = SddTask::new("Implement a Stack<T> with push, pop, peek")
///     .with_test_command(vec!["cargo".into(), "test".into()])
///     .with_cwd("/tmp/my-project")
///     .with_args(RunArgs::default().with_model("sonnet"));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SddTask {
    /// The specification or acceptance criteria.
    pub spec: String,

    /// Command to run for verification (e.g. `["cargo", "test"]`, `["pytest", "-x"]`).
    pub test_command: Vec<String>,

    /// Working directory for both Claude and the test command.
    pub cwd: Option<PathBuf>,

    /// Maximum number of fix iterations after the initial generation.
    /// Defaults to 3.
    pub max_fix_attempts: u32,

    /// Claude runner configuration.
    pub args: RunArgs,
}

impl SddTask {
    /// Create a new SDD task with the given spec and default settings.
    ///
    /// Defaults: `test_command = ["cargo", "test"]`, `max_fix_attempts = 3`.
    pub fn new(spec: impl Into<String>) -> Self {
        Self {
            spec: spec.into(),
            test_command: vec!["cargo".into(), "test".into()],
            cwd: None,
            max_fix_attempts: 3,
            args: RunArgs::default(),
        }
    }

    /// Set the test command.
    pub fn with_test_command(mut self, cmd: Vec<String>) -> Self {
        self.test_command = cmd;
        self
    }

    /// Set the working directory.
    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Set the maximum number of fix attempts.
    pub fn with_max_fix_attempts(mut self, n: u32) -> Self {
        self.max_fix_attempts = n;
        self
    }

    /// Set the Claude runner arguments.
    pub fn with_args(mut self, args: RunArgs) -> Self {
        self.args = args;
        self
    }
}

/// Output from a completed SDD task.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SddOutput {
    /// Whether verification passed.
    pub passed: bool,

    /// Total number of test iterations (1 = passed on first try).
    pub attempts: u32,

    /// Output from the initial code generation step.
    pub gen_output: RunOutput,

    /// Outputs from each fix attempt (empty if passed on first try).
    pub fix_outputs: Vec<RunOutput>,

    /// Stdout from the last test run.
    pub last_test_stdout: String,

    /// Stderr from the last test run.
    pub last_test_stderr: String,
}

#[async_trait]
impl Task for SddTask {
    const NAME: &'static str = "sdd";
    const QUEUE: &'static str = "agents";
    const MAX_RETRIES: u32 = 0; // SDD has its own internal retry loop

    type Output = SddOutput;

    async fn run(&self, ctx: &TaskContext) -> TaskResult<Self::Output> {
        let runner = ctx.data::<Arc<dyn ClaudeRunner>>().ok_or_else(|| {
            KojinError::Other(
                "ClaudeRunner not configured — add Arc<dyn ClaudeRunner> via KojinBuilder::data()"
                    .into(),
            )
        })?;

        // Prepare args with cwd if set
        let mut args = self.args.clone();
        if let Some(ref cwd) = self.cwd {
            args = args.with_cwd(cwd.clone());
        }

        // Step 1: Generate code from spec
        let gen_prompt = format!(
            "Implement the following specification. Write the code directly to files.\n\n\
             ## Specification\n\n{}\n\n\
             ## Verification\n\n\
             The code will be verified by running: `{}`",
            self.spec,
            self.test_command.join(" ")
        );

        tracing::info!("SDD: generating code from spec");
        let gen_output = runner.run(&gen_prompt, &args).await?;

        // Step 2-3: Verify + fix loop
        let mut fix_outputs = Vec::new();
        let mut last_stdout = String::new();
        let mut last_stderr = String::new();

        for attempt in 0..=self.max_fix_attempts {
            tracing::info!(attempt = attempt + 1, "SDD: running verification");

            let test_result = run_test_command(&self.test_command, self.cwd.as_deref()).await?;

            last_stdout = String::from_utf8_lossy(&test_result.stdout).into_owned();
            last_stderr = String::from_utf8_lossy(&test_result.stderr).into_owned();

            if test_result.status.success() {
                tracing::info!(attempts = attempt + 1, "SDD: verification passed");
                return Ok(SddOutput {
                    passed: true,
                    attempts: attempt + 1,
                    gen_output,
                    fix_outputs,
                    last_test_stdout: last_stdout,
                    last_test_stderr: last_stderr,
                });
            }

            // Last iteration — don't try to fix, just report failure
            if attempt == self.max_fix_attempts {
                break;
            }

            // Fix: send test output to Claude for correction
            let fix_prompt = format!(
                "The verification command `{}` failed. Fix the code.\n\n\
                 ## Test Output\n\n\
                 ### stdout\n```\n{}\n```\n\n\
                 ### stderr\n```\n{}\n```\n\n\
                 ## Original Specification\n\n{}",
                self.test_command.join(" "),
                last_stdout.chars().take(4000).collect::<String>(),
                last_stderr.chars().take(4000).collect::<String>(),
                self.spec,
            );

            tracing::info!(attempt = attempt + 1, "SDD: fixing code");
            let fix_output = runner.run(&fix_prompt, &args).await?;
            fix_outputs.push(fix_output);
        }

        tracing::warn!(
            attempts = self.max_fix_attempts + 1,
            "SDD: verification failed after all attempts"
        );

        Ok(SddOutput {
            passed: false,
            attempts: self.max_fix_attempts + 1,
            gen_output,
            fix_outputs,
            last_test_stdout: last_stdout,
            last_test_stderr: last_stderr,
        })
    }
}

/// Run the test command and return its output.
async fn run_test_command(
    command: &[String],
    cwd: Option<&std::path::Path>,
) -> TaskResult<std::process::Output> {
    if command.is_empty() {
        return Err(KojinError::Other("test_command is empty".into()));
    }

    let mut cmd = tokio::process::Command::new(&command[0]);
    if command.len() > 1 {
        cmd.args(&command[1..]);
    }
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }

    cmd.output()
        .await
        .map_err(|e| KojinError::Broker(format!("failed to run test command: {e}")))
}

/// Build a [`Signature`] for an [`SddTask`] invocation.
///
/// # Examples
///
/// ```
/// use kojin_agent::{sdd_sig, RunArgs};
///
/// let sig = sdd_sig(
///     "Implement a binary search function",
///     vec!["cargo".into(), "test".into()],
///     RunArgs::default(),
/// );
/// assert_eq!(sig.task_name, "sdd");
/// assert_eq!(sig.queue, "agents");
/// ```
pub fn sdd_sig(spec: impl Into<String>, test_command: Vec<String>, args: RunArgs) -> Signature {
    SddTask::new(spec)
        .with_test_command(test_command)
        .with_args(args)
        .signature()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_defaults() {
        let task = SddTask::new("my spec");
        assert_eq!(task.spec, "my spec");
        assert_eq!(task.test_command, vec!["cargo", "test"]);
        assert_eq!(task.max_fix_attempts, 3);
        assert!(task.cwd.is_none());
    }

    #[test]
    fn builder_methods() {
        let task = SddTask::new("spec")
            .with_test_command(vec!["pytest".into(), "-x".into()])
            .with_cwd("/tmp/repo")
            .with_max_fix_attempts(5)
            .with_args(RunArgs::default().with_model("opus"));

        assert_eq!(task.test_command, vec!["pytest", "-x"]);
        assert_eq!(task.cwd.as_deref(), Some(std::path::Path::new("/tmp/repo")));
        assert_eq!(task.max_fix_attempts, 5);
        assert_eq!(task.args.model.as_deref(), Some("opus"));
    }

    #[test]
    fn serialization_roundtrip() {
        let task = SddTask::new("implement fibonacci")
            .with_test_command(vec!["npm".into(), "test".into()])
            .with_max_fix_attempts(2);

        let json = serde_json::to_value(&task).unwrap();
        let deserialized: SddTask = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.spec, "implement fibonacci");
        assert_eq!(deserialized.test_command, vec!["npm", "test"]);
        assert_eq!(deserialized.max_fix_attempts, 2);
    }

    #[test]
    fn task_constants() {
        assert_eq!(SddTask::NAME, "sdd");
        assert_eq!(SddTask::QUEUE, "agents");
        assert_eq!(SddTask::MAX_RETRIES, 0);
    }

    #[test]
    fn sdd_sig_produces_valid_signature() {
        let sig = sdd_sig(
            "test spec",
            vec!["cargo".into(), "test".into()],
            RunArgs::default(),
        );
        assert_eq!(sig.task_name, "sdd");
        assert_eq!(sig.queue, "agents");
    }

    #[test]
    fn signature_from_task() {
        let task = SddTask::new("my spec");
        let sig = task.signature();
        assert_eq!(sig.task_name, "sdd");
    }

    #[tokio::test]
    async fn verify_pass_on_first_try() {
        use crate::runner::RunOutput;

        struct MockRunner;

        #[async_trait]
        impl ClaudeRunner for MockRunner {
            async fn run(&self, _prompt: &str, _args: &RunArgs) -> TaskResult<RunOutput> {
                Ok(RunOutput {
                    result: "done".into(),
                    session_id: None,
                    cost_usd: None,
                    duration_ms: None,
                    num_turns: None,
                })
            }
        }

        let runner: Arc<dyn ClaudeRunner> = Arc::new(MockRunner);
        let mut ctx = TaskContext::new();
        ctx.insert(runner);

        // Use `true` as test command — always exits 0
        let task = SddTask::new("test spec")
            .with_test_command(vec!["true".into()])
            .with_max_fix_attempts(3);

        let output = task.run(&ctx).await.unwrap();
        assert!(output.passed);
        assert_eq!(output.attempts, 1);
        assert!(output.fix_outputs.is_empty());
    }

    #[tokio::test]
    async fn verify_fails_then_passes() {
        use crate::runner::RunOutput;

        struct MockRunner;

        #[async_trait]
        impl ClaudeRunner for MockRunner {
            async fn run(&self, _prompt: &str, _args: &RunArgs) -> TaskResult<RunOutput> {
                Ok(RunOutput {
                    result: "fixed".into(),
                    session_id: None,
                    cost_usd: None,
                    duration_ms: None,
                    num_turns: None,
                })
            }
        }

        let runner: Arc<dyn ClaudeRunner> = Arc::new(MockRunner);
        let mut ctx = TaskContext::new();
        ctx.insert(runner);

        // Use a temp file as state: first call fails (file missing), second passes (file exists)
        let tmp = std::env::temp_dir().join("kojin_sdd_test_marker");
        let _ = std::fs::remove_file(&tmp);

        let task = SddTask::new("test spec")
            .with_test_command(vec![
                "sh".into(),
                "-c".into(),
                format!(
                    "if [ -f '{}' ]; then exit 0; else touch '{}' && exit 1; fi",
                    tmp.display(),
                    tmp.display()
                ),
            ])
            .with_max_fix_attempts(3);

        let output = task.run(&ctx).await.unwrap();
        assert!(output.passed);
        assert_eq!(output.attempts, 2); // failed once, then passed
        assert_eq!(output.fix_outputs.len(), 1);

        // Cleanup
        let _ = std::fs::remove_file(&tmp);
    }
}
