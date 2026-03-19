use std::num::NonZeroU32;
use std::sync::Arc;

use async_trait::async_trait;
use governor::clock::DefaultClock;
use governor::middleware::NoOpMiddleware;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};

use super::Middleware;
use crate::error::KojinError;
use crate::message::TaskMessage;

type DirectLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>;
type KeyedLimiter =
    RateLimiter<String, governor::state::keyed::DefaultKeyedStateStore<String>, DefaultClock>;

/// Rate limiting middleware using token-bucket algorithm (via `governor`).
///
/// Supports two modes:
/// - **Global**: a single rate limit shared by all tasks.
/// - **Per-task**: independent rate limits keyed by `task_name`, with an
///   optional default quota for unregistered task names.
///
/// The middleware applies backpressure by awaiting until a token is available
/// (`until_ready`), rather than rejecting requests.
///
/// **Note:** rate limits are per-process. Multiple workers each maintain
/// independent limits. Distributed rate limiting is not yet supported.
pub struct RateLimitMiddleware {
    inner: Arc<RateLimitInner>,
}

enum RateLimitInner {
    Global(DirectLimiter),
    PerTask { limiter: KeyedLimiter },
}

impl RateLimitMiddleware {
    /// Create a global rate limiter with the given quota.
    ///
    /// All tasks share a single token bucket.
    pub fn global(quota: Quota) -> Self {
        Self {
            inner: Arc::new(RateLimitInner::Global(RateLimiter::direct(quota))),
        }
    }

    /// Create a per-task rate limiter with the given quota.
    ///
    /// Each unique `task_name` gets its own independent token bucket.
    pub fn per_task(quota: Quota) -> Self {
        Self {
            inner: Arc::new(RateLimitInner::PerTask {
                limiter: RateLimiter::keyed(quota),
            }),
        }
    }

    /// Convenience: global limiter allowing `n` tasks per second.
    pub fn per_second(n: NonZeroU32) -> Self {
        Self::global(Quota::per_second(n))
    }

    /// Convenience: per-task limiter allowing `n` tasks per second per task name.
    pub fn per_second_per_task(n: NonZeroU32) -> Self {
        Self::per_task(Quota::per_second(n))
    }
}

#[async_trait]
impl Middleware for RateLimitMiddleware {
    async fn before(&self, message: &TaskMessage) -> Result<(), KojinError> {
        match self.inner.as_ref() {
            RateLimitInner::Global(limiter) => {
                limiter.until_ready().await;
            }
            RateLimitInner::PerTask { limiter } => {
                limiter.until_key_ready(&message.task_name).await;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn global_rate_limit_applies_backpressure() {
        let mw = RateLimitMiddleware::per_second(NonZeroU32::new(10).unwrap());
        let msg = TaskMessage::new("test", "default", serde_json::json!({}));

        // First burst should be fast
        let start = Instant::now();
        for _ in 0..10 {
            mw.before(&msg).await.unwrap();
        }
        let burst_elapsed = start.elapsed();

        // 11th request should be rate-limited (takes ~100ms for 10/sec)
        let wait_start = Instant::now();
        mw.before(&msg).await.unwrap();
        let wait_elapsed = wait_start.elapsed();

        assert!(
            burst_elapsed < std::time::Duration::from_millis(50),
            "burst should be fast, took {:?}",
            burst_elapsed
        );
        assert!(
            wait_elapsed >= std::time::Duration::from_millis(50),
            "should have waited, only took {:?}",
            wait_elapsed
        );
    }

    #[tokio::test]
    async fn per_task_limits_are_independent() {
        let mw = RateLimitMiddleware::per_second_per_task(NonZeroU32::new(5).unwrap());
        let msg_a = TaskMessage::new("task_a", "default", serde_json::json!({}));
        let msg_b = TaskMessage::new("task_b", "default", serde_json::json!({}));

        // Exhaust task_a's bucket
        for _ in 0..5 {
            mw.before(&msg_a).await.unwrap();
        }

        // task_b should still be fast
        let start = Instant::now();
        mw.before(&msg_b).await.unwrap();
        assert!(
            start.elapsed() < std::time::Duration::from_millis(50),
            "task_b should not be blocked by task_a"
        );
    }
}
