use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Strategy for calculating retry backoff delays.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackoffStrategy {
    /// Fixed delay between retries.
    Fixed { delay_secs: u64 },
    /// Exponential backoff: base_secs * 2^retry, capped at max_secs.
    Exponential { base_secs: u64, max_secs: u64 },
}

impl BackoffStrategy {
    /// Calculate the delay for the given retry attempt (0-indexed).
    pub fn delay_for(&self, retry: u32) -> Duration {
        match self {
            BackoffStrategy::Fixed { delay_secs } => Duration::from_secs(*delay_secs),
            BackoffStrategy::Exponential {
                base_secs,
                max_secs,
            } => {
                let exp = 1u64.checked_shl(retry).unwrap_or(u64::MAX);
                let delay = base_secs.saturating_mul(exp);
                Duration::from_secs(delay.min(*max_secs))
            }
        }
    }
}

impl Default for BackoffStrategy {
    fn default() -> Self {
        BackoffStrategy::Exponential {
            base_secs: 1,
            max_secs: 300,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_backoff() {
        let strategy = BackoffStrategy::Fixed { delay_secs: 5 };
        assert_eq!(strategy.delay_for(0), Duration::from_secs(5));
        assert_eq!(strategy.delay_for(1), Duration::from_secs(5));
        assert_eq!(strategy.delay_for(10), Duration::from_secs(5));
    }

    #[test]
    fn exponential_backoff() {
        let strategy = BackoffStrategy::Exponential {
            base_secs: 1,
            max_secs: 60,
        };
        assert_eq!(strategy.delay_for(0), Duration::from_secs(1));
        assert_eq!(strategy.delay_for(1), Duration::from_secs(2));
        assert_eq!(strategy.delay_for(2), Duration::from_secs(4));
        assert_eq!(strategy.delay_for(3), Duration::from_secs(8));
        // Should cap at max_secs
        assert_eq!(strategy.delay_for(10), Duration::from_secs(60));
    }

    #[test]
    fn exponential_backoff_overflow() {
        let strategy = BackoffStrategy::Exponential {
            base_secs: 1,
            max_secs: 300,
        };
        // Very high retry count should not panic
        assert_eq!(strategy.delay_for(100), Duration::from_secs(300));
    }

    #[test]
    fn backoff_serde_roundtrip() {
        let strategy = BackoffStrategy::Exponential {
            base_secs: 2,
            max_secs: 120,
        };
        let json = serde_json::to_string(&strategy).unwrap();
        let deserialized: BackoffStrategy = serde_json::from_str(&json).unwrap();
        assert_eq!(strategy.delay_for(3), deserialized.delay_for(3));
    }
}
