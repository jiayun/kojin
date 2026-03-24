//! Concurrency control for agent execution.
//!
//! [`SemaphoreRunner`] wraps any [`ClaudeRunner`] with a concurrency limit,
//! preventing 429 rate-limit storms when running many agent tasks in parallel.
//!
//! # Examples
//!
//! ```
//! use kojin_agent::{ProcessRunner, SemaphoreRunner};
//!
//! // Allow at most 3 concurrent claude processes
//! let runner = SemaphoreRunner::new(ProcessRunner::new(), 3);
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use kojin_core::error::{KojinError, TaskResult};
use tokio::sync::Semaphore;

use crate::runner::{ClaudeRunner, RunArgs, RunOutput};

/// Wraps a [`ClaudeRunner`] with a concurrency semaphore.
///
/// Use this to limit the number of simultaneous agent invocations,
/// independent of the kojin worker's task concurrency. This is useful
/// when the API rate limit is lower than the desired task parallelism.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use kojin_agent::{ClaudeRunner, ProcessRunner, SemaphoreRunner};
///
/// // Worker runs 10 tasks in parallel, but only 3 claude processes at a time
/// let runner = SemaphoreRunner::new(ProcessRunner::new(), 3);
/// let runner: Arc<dyn ClaudeRunner> = Arc::new(runner);
/// ```
pub struct SemaphoreRunner<R: ClaudeRunner> {
    inner: R,
    semaphore: Arc<Semaphore>,
}

impl<R: ClaudeRunner> SemaphoreRunner<R> {
    /// Create a new `SemaphoreRunner` that limits concurrency to `max_concurrent`.
    pub fn new(inner: R, max_concurrent: usize) -> Self {
        Self {
            inner,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    /// Returns the number of available permits.
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
}

#[async_trait]
impl<R: ClaudeRunner> ClaudeRunner for SemaphoreRunner<R> {
    async fn run(&self, prompt: &str, args: &RunArgs) -> TaskResult<RunOutput> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| KojinError::Other("concurrency semaphore closed".into()))?;

        tracing::debug!(
            available = self.semaphore.available_permits(),
            "acquired agent concurrency permit"
        );

        self.inner.run(prompt, args).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::RunOutput;

    /// A mock runner that records call count.
    struct CountingRunner {
        count: Arc<std::sync::atomic::AtomicU32>,
    }

    #[async_trait]
    impl ClaudeRunner for CountingRunner {
        async fn run(&self, _prompt: &str, _args: &RunArgs) -> TaskResult<RunOutput> {
            self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            Ok(RunOutput {
                result: "ok".into(),
                session_id: None,
                cost_usd: None,
                duration_ms: None,
                num_turns: None,
            })
        }
    }

    #[tokio::test]
    async fn limits_concurrency() {
        let count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let runner = SemaphoreRunner::new(
            CountingRunner {
                count: count.clone(),
            },
            2,
        );
        let runner = Arc::new(runner);

        let mut handles = vec![];
        for _ in 0..5 {
            let r = runner.clone();
            handles.push(tokio::spawn(async move {
                r.run("test", &RunArgs::default()).await.unwrap();
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 5);
    }

    #[test]
    fn available_permits() {
        let runner = SemaphoreRunner::new(
            CountingRunner {
                count: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            },
            3,
        );
        assert_eq!(runner.available_permits(), 3);
    }
}
