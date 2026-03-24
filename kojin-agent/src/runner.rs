//! [`ClaudeRunner`] trait and [`ProcessRunner`] implementation.
//!
//! The [`ClaudeRunner`] trait abstracts how Claude Code is invoked. The default
//! [`ProcessRunner`] spawns `claude` as a child process; alternative implementations
//! could run agents in Kubernetes Jobs, Docker containers, or remote machines.

use std::path::PathBuf;

use async_trait::async_trait;
use kojin_core::error::{KojinError, TaskResult};
use serde::{Deserialize, Serialize};

/// Configuration for a single Claude Code invocation.
///
/// All fields are optional — sensible defaults are used when omitted.
/// Marked `#[non_exhaustive]` so new fields can be added in minor releases.
///
/// # Examples
///
/// ```
/// use kojin_agent::RunArgs;
///
/// let args = RunArgs::default()
///     .with_model("sonnet")
///     .with_max_turns(5);
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RunArgs {
    /// Model to use (e.g. `"sonnet"`, `"opus"`, `"claude-sonnet-4-6"`).
    pub model: Option<String>,

    /// System prompt appended to the default system prompt.
    pub system_prompt: Option<String>,

    /// Maximum number of agentic turns.
    pub max_turns: Option<u32>,

    /// Spending limit in USD for this invocation.
    pub max_budget_usd: Option<f64>,

    /// Tools to auto-approve without prompting (e.g. `["Read", "Edit", "Bash(git *)"]`).
    pub allowed_tools: Option<Vec<String>>,

    /// Working directory for the claude process.
    pub cwd: Option<PathBuf>,
}

impl RunArgs {
    /// Set the model.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the system prompt.
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set the maximum number of agentic turns.
    pub fn with_max_turns(mut self, max_turns: u32) -> Self {
        self.max_turns = Some(max_turns);
        self
    }

    /// Set the spending limit in USD.
    pub fn with_max_budget_usd(mut self, budget: f64) -> Self {
        self.max_budget_usd = Some(budget);
        self
    }

    /// Set the tools to auto-approve.
    pub fn with_allowed_tools(mut self, tools: Vec<String>) -> Self {
        self.allowed_tools = Some(tools);
        self
    }

    /// Set the working directory.
    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }
}

/// Structured output from a Claude Code invocation.
///
/// Parsed from `claude --output-format json` output.
/// Marked `#[non_exhaustive]` so new fields can be added in minor releases.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RunOutput {
    /// The response text produced by the agent.
    pub result: String,

    /// Session identifier (UUID).
    pub session_id: Option<String>,

    /// Total cost of the invocation in USD.
    pub cost_usd: Option<f64>,

    /// Wall-clock duration in milliseconds.
    pub duration_ms: Option<u64>,

    /// Number of agentic turns taken.
    pub num_turns: Option<u32>,
}

/// Trait abstracting how Claude Code is executed.
///
/// Implement this trait to run agents in different environments:
///
/// - [`ProcessRunner`] — spawns a local `claude` process (included)
/// - `KubernetesRunner` — create a k8s Job per invocation (bring your own)
/// - `DockerRunner` — run in an isolated container (bring your own)
///
/// The trait is object-safe, so you can use `Arc<dyn ClaudeRunner>`.
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
/// use kojin_agent::{ClaudeRunner, ProcessRunner, RunArgs};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let runner: Arc<dyn ClaudeRunner> = Arc::new(ProcessRunner::new());
/// let output = runner.run("Explain this codebase", &RunArgs::default()).await?;
/// println!("{}", output.result);
/// # Ok(())
/// # }
/// ```
#[async_trait]
pub trait ClaudeRunner: Send + Sync + 'static {
    /// Execute a Claude Code invocation with the given prompt and arguments.
    async fn run(&self, prompt: &str, args: &RunArgs) -> TaskResult<RunOutput>;
}

/// Spawns `claude` as a local child process.
///
/// Uses `claude --bare -p <prompt> --output-format json --no-session-persistence`
/// for predictable, stateless execution.
///
/// # Examples
///
/// ```no_run
/// use kojin_agent::{ProcessRunner, RunArgs, ClaudeRunner};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let runner = ProcessRunner::new();
/// let args = RunArgs::default()
///     .with_model("sonnet")
///     .with_allowed_tools(vec!["Read".into(), "Bash(grep *)".into()]);
/// let output = runner.run("List all TODO comments", &args).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Default)]
pub struct ProcessRunner {
    /// Path to the `claude` binary. Defaults to `"claude"` (found via PATH).
    pub claude_bin: Option<String>,
}

impl ProcessRunner {
    /// Create a new `ProcessRunner` using the default `claude` binary from PATH.
    pub fn new() -> Self {
        Self { claude_bin: None }
    }

    /// Create a `ProcessRunner` with a custom path to the `claude` binary.
    pub fn with_bin(bin: impl Into<String>) -> Self {
        Self {
            claude_bin: Some(bin.into()),
        }
    }

    fn bin(&self) -> &str {
        self.claude_bin.as_deref().unwrap_or("claude")
    }
}

/// Raw JSON structure from `claude --output-format json`.
#[derive(Deserialize)]
struct RawOutput {
    result: Option<String>,
    session_id: Option<String>,
    cost_usd: Option<f64>,
    duration_ms: Option<u64>,
    num_turns: Option<u32>,
}

#[async_trait]
impl ClaudeRunner for ProcessRunner {
    async fn run(&self, prompt: &str, args: &RunArgs) -> TaskResult<RunOutput> {
        let mut cmd = tokio::process::Command::new(self.bin());

        // Core flags for predictable, stateless execution
        cmd.arg("--bare")
            .arg("-p")
            .arg(prompt)
            .arg("--output-format")
            .arg("json")
            .arg("--no-session-persistence");

        // Optional arguments
        if let Some(ref model) = args.model {
            cmd.arg("--model").arg(model);
        }
        if let Some(ref system_prompt) = args.system_prompt {
            cmd.arg("--append-system-prompt").arg(system_prompt);
        }
        if let Some(max_turns) = args.max_turns {
            cmd.arg("--max-turns").arg(max_turns.to_string());
        }
        if let Some(budget) = args.max_budget_usd {
            cmd.arg("--max-budget-usd").arg(budget.to_string());
        }
        if let Some(ref tools) = args.allowed_tools {
            cmd.arg("--allowedTools").arg(tools.join(","));
        }
        if let Some(ref cwd) = args.cwd {
            cmd.current_dir(cwd);
        }

        tracing::info!(
            prompt_len = prompt.len(),
            model = args.model.as_deref().unwrap_or("default"),
            "spawning claude process"
        );

        let output = cmd
            .output()
            .await
            .map_err(|e| KojinError::Broker(format!("failed to spawn claude process: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(KojinError::TaskFailed(format!(
                "claude exited with {}: {}{}",
                output.status,
                stderr.trim(),
                if stderr.is_empty() {
                    stdout.to_string()
                } else {
                    String::new()
                }
            )));
        }

        let raw: RawOutput = serde_json::from_slice(&output.stdout).map_err(|e| {
            KojinError::TaskFailed(format!(
                "failed to parse claude JSON output: {e}\nstdout: {}",
                String::from_utf8_lossy(&output.stdout)
            ))
        })?;

        Ok(RunOutput {
            result: raw.result.unwrap_or_default(),
            session_id: raw.session_id,
            cost_usd: raw.cost_usd,
            duration_ms: raw.duration_ms,
            num_turns: raw.num_turns,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_args_default() {
        let args = RunArgs::default();
        assert!(args.model.is_none());
        assert!(args.max_turns.is_none());
        assert!(args.cwd.is_none());
    }

    #[test]
    fn run_output_deserialize() {
        let json = r#"{"result":"hello","session_id":"abc","cost_usd":0.01,"duration_ms":500,"num_turns":2}"#;
        let output: RunOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.result, "hello");
        assert_eq!(output.cost_usd, Some(0.01));
        assert_eq!(output.num_turns, Some(2));
    }

    #[test]
    fn run_args_serialization_roundtrip() {
        let args = RunArgs {
            model: Some("opus".into()),
            max_turns: Some(5),
            allowed_tools: Some(vec!["Read".into(), "Edit".into()]),
            ..Default::default()
        };
        let json = serde_json::to_string(&args).unwrap();
        let deserialized: RunArgs = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.model.as_deref(), Some("opus"));
        assert_eq!(deserialized.max_turns, Some(5));
    }

    #[tokio::test]
    async fn process_runner_with_echo() {
        // Use `echo` as a mock claude binary to test process spawning
        let runner = ProcessRunner::with_bin("echo");
        let result = runner.run("test", &RunArgs::default()).await;
        // echo will output the args as text, which won't parse as JSON
        assert!(result.is_err());
    }
}
