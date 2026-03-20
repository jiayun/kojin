use async_trait::async_trait;
use kojin_core::{KojinError, Task, TaskContext, TaskResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Test tasks
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct AddTask {
    pub a: i32,
    pub b: i32,
}

#[async_trait]
impl Task for AddTask {
    const NAME: &'static str = "test_add";
    const MAX_RETRIES: u32 = 0;
    type Output = i32;

    async fn run(&self, _ctx: &TaskContext) -> TaskResult<Self::Output> {
        Ok(self.a + self.b)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SlowTask {
    pub ms: u64,
}

#[async_trait]
impl Task for SlowTask {
    const NAME: &'static str = "test_slow";
    const MAX_RETRIES: u32 = 0;
    type Output = String;

    async fn run(&self, _ctx: &TaskContext) -> TaskResult<Self::Output> {
        tokio::time::sleep(Duration::from_millis(self.ms)).await;
        Ok("done".to_string())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FailTask;

#[async_trait]
impl Task for FailTask {
    const NAME: &'static str = "test_fail";
    const MAX_RETRIES: u32 = 0;
    type Output = ();

    async fn run(&self, _ctx: &TaskContext) -> TaskResult<Self::Output> {
        Err(KojinError::TaskFailed("intentional failure".into()))
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CountTask;

#[async_trait]
impl Task for CountTask {
    const NAME: &'static str = "test_count";
    const MAX_RETRIES: u32 = 0;
    type Output = u32;

    async fn run(&self, ctx: &TaskContext) -> TaskResult<Self::Output> {
        let counter = ctx
            .data::<Arc<AtomicU32>>()
            .expect("CountTask requires Arc<AtomicU32> in context");
        let val = counter.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(val)
    }
}

// ---------------------------------------------------------------------------
// Helper: run worker until a condition is met
// ---------------------------------------------------------------------------

/// Runs the worker in a background task, polling `check` every 50ms.
/// Cancels the worker and returns once `check()` returns `true` or `timeout` elapses.
/// Panics if the timeout is reached without `check` returning true.
pub async fn run_worker_until<B, F>(worker: kojin_core::Worker<B>, check: F, timeout: Duration)
where
    B: kojin_core::Broker,
    F: Fn() -> bool + Send + 'static,
{
    let cancel = worker.cancel_token();
    let handle = tokio::spawn(async move {
        worker.run().await;
    });

    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if check() {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            cancel.cancel();
            handle.await.unwrap();
            panic!("run_worker_until timed out after {timeout:?}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    cancel.cancel();
    handle.await.unwrap();
}

/// Like `run_worker_until` but accepts an async check and returns without panicking.
/// Returns `true` if the condition was met, `false` on timeout.
pub async fn run_worker_until_async<B, F, Fut>(
    worker: kojin_core::Worker<B>,
    check: F,
    timeout: Duration,
) -> bool
where
    B: kojin_core::Broker,
    F: Fn() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = bool> + Send,
{
    let cancel = worker.cancel_token();
    let handle = tokio::spawn(async move {
        worker.run().await;
    });

    let deadline = tokio::time::Instant::now() + timeout;
    let met = loop {
        if check().await {
            break true;
        }
        if tokio::time::Instant::now() >= deadline {
            break false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };

    cancel.cancel();
    handle.await.unwrap();
    met
}
