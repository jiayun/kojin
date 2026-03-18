use chrono::Utc;
use cron::Schedule;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use crate::broker::Broker;
use crate::error::{KojinError, TaskResult};
use crate::signature::Signature;

/// A single cron entry binding a schedule expression to a task signature.
#[derive(Debug, Clone)]
pub struct CronEntry {
    /// Human-readable name.
    pub name: String,
    /// Parsed cron schedule.
    pub schedule: Schedule,
    /// Task to enqueue when the schedule fires.
    pub signature: Signature,
}

impl CronEntry {
    /// Create a new cron entry.
    ///
    /// `expression` uses the standard 7-field cron format:
    /// `sec min hour day-of-month month day-of-week year`
    pub fn new(
        name: impl Into<String>,
        expression: &str,
        signature: Signature,
    ) -> TaskResult<Self> {
        let schedule = Schedule::from_str(expression)
            .map_err(|e| KojinError::Other(format!("invalid cron expression: {e}")))?;
        Ok(Self {
            name: name.into(),
            schedule,
            signature,
        })
    }
}

/// Registry of cron entries.
#[derive(Debug, Clone, Default)]
pub struct CronRegistry {
    pub entries: Vec<CronEntry>,
}

impl CronRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a cron entry.
    pub fn add(&mut self, entry: CronEntry) {
        self.entries.push(entry);
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Run the cron scheduler loop as a background task.
///
/// This checks each entry's schedule and enqueues the task when it's due.
/// It uses a simple in-memory last-run tracker (suitable for single-worker setups).
/// For multi-worker distributed locking, use Redis-based locking externally.
pub async fn scheduler_loop<B: Broker>(
    broker: Arc<B>,
    registry: CronRegistry,
    cancel: tokio_util::sync::CancellationToken,
    poll_interval: std::time::Duration,
) {
    let mut last_runs: HashMap<String, chrono::DateTime<Utc>> = HashMap::new();

    tracing::info!(entries = registry.entries.len(), "Cron scheduler started");

    loop {
        if cancel.is_cancelled() {
            break;
        }

        let now = Utc::now();

        for entry in &registry.entries {
            // Find the next scheduled time after last run (or epoch)
            let last_run = last_runs
                .get(&entry.name)
                .copied()
                .unwrap_or_else(|| now - chrono::Duration::seconds(1));

            if let Some(next) = entry.schedule.after(&last_run).next() {
                if next <= now {
                    tracing::info!(
                        cron_name = %entry.name,
                        task = %entry.signature.task_name,
                        "Cron firing"
                    );
                    let msg = entry.signature.clone().into_message();
                    if let Err(e) = broker.enqueue(msg).await {
                        tracing::error!(
                            cron_name = %entry.name,
                            error = %e,
                            "Failed to enqueue cron task"
                        );
                    }
                    last_runs.insert(entry.name.clone(), now);
                }
            }
        }

        tokio::select! {
            _ = tokio::time::sleep(poll_interval) => {}
            _ = cancel.cancelled() => break,
        }
    }

    tracing::info!("Cron scheduler stopped");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_broker::MemoryBroker;

    #[test]
    fn cron_entry_valid() {
        let sig = Signature::new("task", "default", serde_json::json!({}));
        let entry = CronEntry::new("every_minute", "0 * * * * * *", sig);
        assert!(entry.is_ok());
    }

    #[test]
    fn cron_entry_invalid() {
        let sig = Signature::new("task", "default", serde_json::json!({}));
        let entry = CronEntry::new("bad", "not a cron", sig);
        assert!(entry.is_err());
    }

    #[tokio::test]
    async fn scheduler_fires_task() {
        let broker = Arc::new(MemoryBroker::new());
        let mut registry = CronRegistry::new();

        let sig = Signature::new("periodic", "default", serde_json::json!({}));
        // Every second
        let entry = CronEntry::new("every_sec", "* * * * * * *", sig).unwrap();
        registry.add(entry);

        let cancel = tokio_util::sync::CancellationToken::new();
        let cancel2 = cancel.clone();
        let broker2 = broker.clone();

        let handle = tokio::spawn(async move {
            scheduler_loop(
                broker2,
                registry,
                cancel2,
                std::time::Duration::from_millis(200),
            )
            .await;
        });

        // Wait for at least one fire
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        cancel.cancel();
        handle.await.unwrap();

        let len = broker.queue_len("default").await.unwrap();
        assert!(len >= 1, "expected at least 1 enqueued task, got {len}");
    }
}
