use serde::{Deserialize, Serialize};

use crate::broker::Broker;
use crate::error::TaskResult;
use crate::result_backend::ResultBackend;
use crate::signature::Signature;
use crate::task_id::TaskId;

/// A composable workflow description.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Canvas {
    /// A single task invocation.
    Single(Signature),
    /// Execute tasks sequentially; each task's result is passed to the next.
    Chain(Vec<Canvas>),
    /// Execute tasks in parallel.
    Group(Vec<Canvas>),
    /// A group with a callback that fires after all members complete.
    Chord {
        group: Vec<Canvas>,
        callback: Box<Canvas>,
    },
}

/// Handle returned after submitting a workflow.
#[derive(Debug, Clone)]
pub struct WorkflowHandle {
    /// Workflow identifier (correlation ID).
    pub id: String,
    /// Task IDs of all submitted tasks.
    pub task_ids: Vec<TaskId>,
}

impl Canvas {
    /// Submit this workflow for execution.
    ///
    /// - **Single**: enqueues immediately.
    /// - **Chain**: enqueues first task with remaining steps stored in headers.
    /// - **Group**: enqueues all members in parallel with group metadata.
    /// - **Chord**: like Group but attaches a chord callback.
    pub async fn apply(
        &self,
        broker: &dyn Broker,
        backend: &dyn ResultBackend,
    ) -> TaskResult<WorkflowHandle> {
        let correlation_id = uuid::Uuid::now_v7().to_string();
        let mut task_ids = Vec::new();
        self.apply_inner(broker, backend, &correlation_id, &mut task_ids)
            .await?;
        Ok(WorkflowHandle {
            id: correlation_id,
            task_ids,
        })
    }

    async fn apply_inner(
        &self,
        broker: &dyn Broker,
        backend: &dyn ResultBackend,
        correlation_id: &str,
        task_ids: &mut Vec<TaskId>,
    ) -> TaskResult<()> {
        match self {
            Canvas::Single(sig) => {
                let mut msg = sig.clone().into_message();
                msg.correlation_id = Some(correlation_id.to_string());
                task_ids.push(msg.id);
                broker.enqueue(msg).await?;
            }
            Canvas::Chain(steps) => {
                if steps.is_empty() {
                    return Ok(());
                }
                // Flatten chain into signatures
                let sigs = Self::flatten_chain(steps);
                if sigs.is_empty() {
                    return Ok(());
                }
                // Enqueue only the first; store remaining as chain_next header
                let mut first_msg = sigs[0].clone().into_message();
                first_msg.correlation_id = Some(correlation_id.to_string());
                if sigs.len() > 1 {
                    let remaining: Vec<Signature> = sigs[1..].to_vec();
                    let remaining_json =
                        serde_json::to_string(&remaining).expect("failed to serialize chain steps");
                    first_msg
                        .headers
                        .insert("kojin.chain_next".to_string(), remaining_json);
                }
                task_ids.push(first_msg.id);
                broker.enqueue(first_msg).await?;
            }
            Canvas::Group(members) => {
                let group_id = uuid::Uuid::now_v7().to_string();
                let sigs = Self::flatten_group(members);
                let total = sigs.len() as u32;
                backend.init_group(&group_id, total).await?;

                for sig in &sigs {
                    let mut msg = sig.clone().into_message();
                    msg.correlation_id = Some(correlation_id.to_string());
                    msg.group_id = Some(group_id.clone());
                    msg.group_total = Some(total);
                    task_ids.push(msg.id);
                    broker.enqueue(msg).await?;
                }
            }
            Canvas::Chord { group, callback } => {
                let group_id = uuid::Uuid::now_v7().to_string();
                let sigs = Self::flatten_group(group);
                let total = sigs.len() as u32;
                backend.init_group(&group_id, total).await?;

                // Build the callback message
                let callback_sigs = Self::flatten_chain(&[*callback.clone()]);
                let callback_msg = if !callback_sigs.is_empty() {
                    callback_sigs[0].clone().into_message()
                } else {
                    return Ok(());
                };

                for sig in &sigs {
                    let mut msg = sig.clone().into_message();
                    msg.correlation_id = Some(correlation_id.to_string());
                    msg.group_id = Some(group_id.clone());
                    msg.group_total = Some(total);
                    msg.chord_callback = Some(Box::new(callback_msg.clone()));
                    task_ids.push(msg.id);
                    broker.enqueue(msg).await?;
                }
            }
        }
        Ok(())
    }

    /// Flatten a chain of canvases into a list of signatures.
    fn flatten_chain(steps: &[Canvas]) -> Vec<Signature> {
        let mut result = Vec::new();
        for step in steps {
            match step {
                Canvas::Single(sig) => result.push(sig.clone()),
                Canvas::Chain(inner) => result.extend(Self::flatten_chain(inner)),
                _ => {
                    // For nested group/chord in a chain, we'd need more complex handling.
                    // For now, skip non-single items.
                    tracing::warn!("Nested group/chord in chain is not yet supported, skipping");
                }
            }
        }
        result
    }

    /// Flatten a group of canvases into a list of signatures.
    fn flatten_group(members: &[Canvas]) -> Vec<Signature> {
        let mut result = Vec::new();
        for member in members {
            match member {
                Canvas::Single(sig) => result.push(sig.clone()),
                _ => {
                    tracing::warn!("Nested canvas in group is not yet supported, skipping");
                }
            }
        }
        result
    }
}

/// Create a chain of tasks.
///
/// ```ignore
/// let workflow = chain![sig_a, sig_b, sig_c];
/// ```
#[macro_export]
macro_rules! chain {
    ($($sig:expr),+ $(,)?) => {
        $crate::canvas::Canvas::Chain(vec![
            $($crate::canvas::Canvas::Single($sig)),+
        ])
    };
}

/// Create a group of tasks.
///
/// ```ignore
/// let workflow = group![sig_a, sig_b, sig_c];
/// ```
#[macro_export]
macro_rules! group {
    ($($sig:expr),+ $(,)?) => {
        $crate::canvas::Canvas::Group(vec![
            $($crate::canvas::Canvas::Single($sig)),+
        ])
    };
}

/// Create a chord: a group with a callback that fires when all members complete.
pub fn chord(group_items: Vec<Signature>, callback: Signature) -> Canvas {
    Canvas::Chord {
        group: group_items.into_iter().map(Canvas::Single).collect(),
        callback: Box::new(Canvas::Single(callback)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_broker::MemoryBroker;
    use crate::memory_result_backend::MemoryResultBackend;

    fn sig(name: &str) -> Signature {
        Signature::new(name, "default", serde_json::json!({}))
    }

    #[test]
    fn chain_macro() {
        let c = chain![sig("a"), sig("b"), sig("c")];
        match c {
            Canvas::Chain(steps) => assert_eq!(steps.len(), 3),
            _ => panic!("expected Chain"),
        }
    }

    #[test]
    fn group_macro() {
        let g = group![sig("a"), sig("b")];
        match g {
            Canvas::Group(members) => assert_eq!(members.len(), 2),
            _ => panic!("expected Group"),
        }
    }

    #[test]
    fn chord_constructor() {
        let c = chord(vec![sig("a"), sig("b")], sig("callback"));
        match c {
            Canvas::Chord { group, callback } => {
                assert_eq!(group.len(), 2);
                assert!(matches!(*callback, Canvas::Single(_)));
            }
            _ => panic!("expected Chord"),
        }
    }

    #[tokio::test]
    async fn apply_single() {
        let broker = MemoryBroker::new();
        let backend = MemoryResultBackend::new();
        let canvas = Canvas::Single(sig("task_a"));

        let handle = canvas.apply(&broker, &backend).await.unwrap();
        assert_eq!(handle.task_ids.len(), 1);
        assert_eq!(broker.queue_len("default").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn apply_chain() {
        let broker = MemoryBroker::new();
        let backend = MemoryResultBackend::new();
        let canvas = chain![sig("a"), sig("b"), sig("c")];

        let handle = canvas.apply(&broker, &backend).await.unwrap();
        // Only first task enqueued
        assert_eq!(handle.task_ids.len(), 1);
        assert_eq!(broker.queue_len("default").await.unwrap(), 1);

        // Verify chain_next header
        let msg = broker
            .dequeue(
                &["default".to_string()],
                std::time::Duration::from_millis(100),
            )
            .await
            .unwrap()
            .unwrap();
        assert!(msg.headers.contains_key("kojin.chain_next"));
        let remaining: Vec<Signature> =
            serde_json::from_str(msg.headers.get("kojin.chain_next").unwrap()).unwrap();
        assert_eq!(remaining.len(), 2);
        assert_eq!(remaining[0].task_name, "b");
        assert_eq!(remaining[1].task_name, "c");
    }

    #[tokio::test]
    async fn apply_group() {
        let broker = MemoryBroker::new();
        let backend = MemoryResultBackend::new();
        let canvas = group![sig("a"), sig("b"), sig("c")];

        let handle = canvas.apply(&broker, &backend).await.unwrap();
        assert_eq!(handle.task_ids.len(), 3);
        assert_eq!(broker.queue_len("default").await.unwrap(), 3);
    }

    #[tokio::test]
    async fn apply_chord() {
        let broker = MemoryBroker::new();
        let backend = MemoryResultBackend::new();
        let canvas = chord(vec![sig("a"), sig("b")], sig("callback"));

        let handle = canvas.apply(&broker, &backend).await.unwrap();
        assert_eq!(handle.task_ids.len(), 2);
        assert_eq!(broker.queue_len("default").await.unwrap(), 2);

        // Each member should have chord_callback set
        let msg = broker
            .dequeue(
                &["default".to_string()],
                std::time::Duration::from_millis(100),
            )
            .await
            .unwrap()
            .unwrap();
        assert!(msg.chord_callback.is_some());
        assert_eq!(msg.chord_callback.unwrap().task_name, "callback");
    }
}
