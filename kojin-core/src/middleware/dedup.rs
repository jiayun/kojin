use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use dashmap::DashMap;

use crate::error::KojinError;
use crate::message::TaskMessage;

use super::Middleware;

/// In-memory deduplication middleware for brokers without native dedup.
///
/// Intercepts `before()`: if a message's `dedup_key` has been seen within TTL,
/// returns `Err(Duplicate)` to skip execution. The caller (worker/broker) should
/// ack and discard the message.
///
/// For Redis, prefer the native `SET NX` approach in `RedisBroker::enqueue`.
/// For SQS FIFO, dedup is handled natively by `MessageDeduplicationId`.
pub struct DeduplicationMiddleware {
    seen: Arc<DashMap<String, Instant>>,
    ttl: Duration,
}

impl DeduplicationMiddleware {
    /// Create a new dedup middleware with the given TTL.
    pub fn new(ttl: Duration) -> Self {
        Self {
            seen: Arc::new(DashMap::new()),
            ttl,
        }
    }

    /// Remove expired entries. Call periodically for long-running workers.
    pub fn cleanup(&self) {
        let now = Instant::now();
        self.seen.retain(|_, v| now.duration_since(*v) < self.ttl);
    }
}

#[async_trait]
impl Middleware for DeduplicationMiddleware {
    async fn before(&self, message: &TaskMessage) -> Result<(), KojinError> {
        let key = match &message.dedup_key {
            Some(k) => k.clone(),
            None => return Ok(()),
        };

        let now = Instant::now();

        match self.seen.entry(key.clone()) {
            dashmap::mapref::entry::Entry::Occupied(mut entry) => {
                if now.duration_since(*entry.get()) < self.ttl {
                    tracing::debug!(dedup_key = %key, "duplicate task filtered by DeduplicationMiddleware");
                    return Err(KojinError::Duplicate(key));
                }
                // Expired — update timestamp
                entry.insert(now);
            }
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                entry.insert(now);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn allows_first_message() {
        let mw = DeduplicationMiddleware::new(Duration::from_secs(60));
        let msg = TaskMessage::new("task", "q", serde_json::json!({})).with_dedup_key("unique-key");
        assert!(mw.before(&msg).await.is_ok());
    }

    #[tokio::test]
    async fn rejects_duplicate_within_ttl() {
        let mw = DeduplicationMiddleware::new(Duration::from_secs(60));
        let msg = TaskMessage::new("task", "q", serde_json::json!({})).with_dedup_key("same-key");
        mw.before(&msg).await.unwrap();

        let msg2 = TaskMessage::new("task", "q", serde_json::json!({})).with_dedup_key("same-key");
        let result = mw.before(&msg2).await;
        assert!(matches!(result, Err(KojinError::Duplicate(_))));
    }

    #[tokio::test]
    async fn allows_without_dedup_key() {
        let mw = DeduplicationMiddleware::new(Duration::from_secs(60));
        let msg = TaskMessage::new("task", "q", serde_json::json!({}));
        assert!(mw.before(&msg).await.is_ok());
        // No dedup_key → always allowed
        assert!(mw.before(&msg).await.is_ok());
    }

    #[tokio::test]
    async fn cleanup_removes_expired() {
        let mw = DeduplicationMiddleware::new(Duration::from_millis(1));
        let msg = TaskMessage::new("task", "q", serde_json::json!({})).with_dedup_key("expire-me");
        mw.before(&msg).await.unwrap();

        tokio::time::sleep(Duration::from_millis(10)).await;
        mw.cleanup();

        // After cleanup + TTL expiry, should be allowed again
        assert!(mw.before(&msg).await.is_ok());
    }
}
