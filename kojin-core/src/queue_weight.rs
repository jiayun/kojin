use serde::{Deserialize, Serialize};

/// Queue priority weight for weighted queue selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QueueWeight {
    High,
    Medium,
    Low,
}

impl QueueWeight {
    /// Numeric weight for weighted selection.
    pub fn weight(&self) -> u32 {
        match self {
            QueueWeight::High => 6,
            QueueWeight::Medium => 3,
            QueueWeight::Low => 1,
        }
    }
}

/// A queue with an associated weight.
#[derive(Debug, Clone)]
pub struct WeightedQueue {
    pub name: String,
    pub weight: QueueWeight,
}

impl WeightedQueue {
    pub fn new(name: impl Into<String>, weight: QueueWeight) -> Self {
        Self {
            name: name.into(),
            weight,
        }
    }
}

/// Build a weighted queue ordering: higher-weight queues appear more often.
/// Returns queue names in the order they should be polled.
pub fn build_weighted_order(queues: &[WeightedQueue]) -> Vec<String> {
    let mut order = Vec::new();
    for wq in queues {
        for _ in 0..wq.weight.weight() {
            order.push(wq.name.clone());
        }
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighted_order() {
        let queues = vec![
            WeightedQueue::new("high", QueueWeight::High),
            WeightedQueue::new("low", QueueWeight::Low),
        ];
        let order = build_weighted_order(&queues);
        let high_count = order.iter().filter(|q| *q == "high").count();
        let low_count = order.iter().filter(|q| *q == "low").count();
        assert_eq!(high_count, 6);
        assert_eq!(low_count, 1);
    }
}
