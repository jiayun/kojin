use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};

use crate::backoff::BackoffStrategy;
use crate::context::TaskContext;
use crate::error::TaskResult;
use crate::signature::Signature;

/// A unit of work that can be enqueued and executed by a worker.
#[async_trait]
pub trait Task: Send + Sync + Serialize + DeserializeOwned + 'static {
    /// Unique name used for routing. Must be stable across deploys.
    const NAME: &'static str;

    /// Default queue name.
    const QUEUE: &'static str = "default";

    /// Maximum number of retry attempts.
    const MAX_RETRIES: u32 = 3;

    /// Backoff strategy for retries.
    fn backoff() -> BackoffStrategy {
        BackoffStrategy::default()
    }

    /// The output type of the task.
    type Output: Serialize + DeserializeOwned + Send;

    /// Execute the task.
    async fn run(&self, ctx: &TaskContext) -> TaskResult<Self::Output>;

    /// Build a [`Signature`] from this task instance.
    fn signature(&self) -> Signature {
        Signature::new(
            Self::NAME,
            Self::QUEUE,
            serde_json::to_value(self).expect("task must be serializable"),
        )
        .with_max_retries(Self::MAX_RETRIES)
    }
}
