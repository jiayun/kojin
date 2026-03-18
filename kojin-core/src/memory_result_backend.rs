use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use crate::error::{KojinError, TaskResult};
use crate::result_backend::ResultBackend;
use crate::task_id::TaskId;

/// In-memory result backend for testing. Stores results in a HashMap behind a Mutex.
#[derive(Debug, Default)]
pub struct MemoryResultBackend {
    results: Mutex<HashMap<String, serde_json::Value>>,
    groups: Mutex<HashMap<String, GroupState>>,
}

#[derive(Debug, Clone)]
struct GroupState {
    #[allow(dead_code)]
    total: u32,
    completed: u32,
    results: Vec<serde_json::Value>,
}

impl MemoryResultBackend {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ResultBackend for MemoryResultBackend {
    async fn store(&self, id: &TaskId, result: &serde_json::Value) -> TaskResult<()> {
        self.results
            .lock()
            .unwrap()
            .insert(id.to_string(), result.clone());
        Ok(())
    }

    async fn get(&self, id: &TaskId) -> TaskResult<Option<serde_json::Value>> {
        Ok(self.results.lock().unwrap().get(&id.to_string()).cloned())
    }

    async fn wait(&self, id: &TaskId, timeout: Duration) -> TaskResult<serde_json::Value> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if let Some(result) = self.get(id).await? {
                return Ok(result);
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(KojinError::Timeout(timeout));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    async fn delete(&self, id: &TaskId) -> TaskResult<()> {
        self.results.lock().unwrap().remove(&id.to_string());
        Ok(())
    }

    async fn init_group(&self, group_id: &str, total: u32) -> TaskResult<()> {
        self.groups.lock().unwrap().insert(
            group_id.to_string(),
            GroupState {
                total,
                completed: 0,
                results: Vec::new(),
            },
        );
        Ok(())
    }

    async fn complete_group_member(
        &self,
        group_id: &str,
        _task_id: &TaskId,
        result: &serde_json::Value,
    ) -> TaskResult<u32> {
        let mut groups = self.groups.lock().unwrap();
        let state = groups
            .get_mut(group_id)
            .ok_or_else(|| KojinError::ResultBackend(format!("group not found: {group_id}")))?;
        state.completed += 1;
        state.results.push(result.clone());
        Ok(state.completed)
    }

    async fn get_group_results(&self, group_id: &str) -> TaskResult<Vec<serde_json::Value>> {
        let groups = self.groups.lock().unwrap();
        let state = groups
            .get(group_id)
            .ok_or_else(|| KojinError::ResultBackend(format!("group not found: {group_id}")))?;
        Ok(state.results.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn store_and_get() {
        let backend = MemoryResultBackend::new();
        let id = TaskId::new();
        let value = serde_json::json!({"result": 42});

        backend.store(&id, &value).await.unwrap();
        let got = backend.get(&id).await.unwrap();
        assert_eq!(got, Some(value));
    }

    #[tokio::test]
    async fn get_missing() {
        let backend = MemoryResultBackend::new();
        let id = TaskId::new();
        assert_eq!(backend.get(&id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn delete_result() {
        let backend = MemoryResultBackend::new();
        let id = TaskId::new();
        backend.store(&id, &serde_json::json!(1)).await.unwrap();
        backend.delete(&id).await.unwrap();
        assert_eq!(backend.get(&id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn wait_for_result() {
        let backend = std::sync::Arc::new(MemoryResultBackend::new());
        let id = TaskId::new();

        let b = backend.clone();
        let id2 = id;
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            b.store(&id2, &serde_json::json!("done")).await.unwrap();
        });

        let result = backend.wait(&id, Duration::from_secs(2)).await.unwrap();
        assert_eq!(result, serde_json::json!("done"));
    }

    #[tokio::test]
    async fn wait_timeout() {
        let backend = MemoryResultBackend::new();
        let id = TaskId::new();
        let result = backend.wait(&id, Duration::from_millis(100)).await;
        assert!(matches!(result, Err(KojinError::Timeout(_))));
    }

    #[tokio::test]
    async fn group_lifecycle() {
        let backend = MemoryResultBackend::new();
        backend.init_group("g1", 3).await.unwrap();

        let id1 = TaskId::new();
        let id2 = TaskId::new();
        let id3 = TaskId::new();

        let c1 = backend
            .complete_group_member("g1", &id1, &serde_json::json!(1))
            .await
            .unwrap();
        assert_eq!(c1, 1);

        let c2 = backend
            .complete_group_member("g1", &id2, &serde_json::json!(2))
            .await
            .unwrap();
        assert_eq!(c2, 2);

        let c3 = backend
            .complete_group_member("g1", &id3, &serde_json::json!(3))
            .await
            .unwrap();
        assert_eq!(c3, 3);

        let results = backend.get_group_results("g1").await.unwrap();
        assert_eq!(results.len(), 3);
    }
}
