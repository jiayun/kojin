use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::context::TaskContext;
use crate::error::{KojinError, TaskResult};
use crate::task::Task;

/// Type-erased task handler function.
pub type TaskHandler = Arc<
    dyn Fn(
            serde_json::Value,
            Arc<TaskContext>,
        ) -> Pin<Box<dyn Future<Output = TaskResult<serde_json::Value>> + Send>>
        + Send
        + Sync,
>;

/// Registry mapping task names to type-erased handlers.
#[derive(Clone)]
pub struct TaskRegistry {
    handlers: HashMap<String, TaskHandler>,
}

impl TaskRegistry {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    /// Register a task type.
    pub fn register<T: Task>(&mut self) {
        let handler: TaskHandler = Arc::new(|payload, ctx| {
            Box::pin(async move {
                let task: T = serde_json::from_value(payload)?;
                let result = task.run(&ctx).await?;
                Ok(serde_json::to_value(result)?)
            })
        });
        self.handlers.insert(T::NAME.to_string(), handler);
    }

    /// Look up a handler by task name.
    pub fn get(&self, name: &str) -> Option<&TaskHandler> {
        self.handlers.get(name)
    }

    /// Check if a task is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.handlers.contains_key(name)
    }

    /// Execute a task by name with the given payload.
    pub async fn dispatch(
        &self,
        name: &str,
        payload: serde_json::Value,
        ctx: Arc<TaskContext>,
    ) -> TaskResult<serde_json::Value> {
        let handler = self
            .get(name)
            .ok_or_else(|| KojinError::TaskNotFound(name.to_string()))?;
        handler(payload, ctx).await
    }
}

impl Default for TaskRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    struct AddTask {
        a: i32,
        b: i32,
    }

    #[async_trait]
    impl Task for AddTask {
        const NAME: &'static str = "add";
        type Output = i32;

        async fn run(&self, _ctx: &TaskContext) -> TaskResult<Self::Output> {
            Ok(self.a + self.b)
        }
    }

    #[tokio::test]
    async fn register_and_dispatch() {
        let mut registry = TaskRegistry::new();
        registry.register::<AddTask>();

        assert!(registry.contains("add"));
        assert!(!registry.contains("unknown"));

        let ctx = Arc::new(TaskContext::new());
        let result = registry
            .dispatch("add", serde_json::json!({"a": 3, "b": 4}), ctx)
            .await
            .unwrap();
        assert_eq!(result, serde_json::json!(7));
    }

    #[tokio::test]
    async fn dispatch_not_found() {
        let registry = TaskRegistry::new();
        let ctx = Arc::new(TaskContext::new());
        let result = registry
            .dispatch("unknown", serde_json::Value::Null, ctx)
            .await;
        assert!(matches!(result, Err(KojinError::TaskNotFound(_))));
    }

    #[tokio::test]
    async fn dispatch_with_context() {
        #[derive(Debug, Serialize, Deserialize)]
        struct CtxTask;

        #[async_trait]
        impl Task for CtxTask {
            const NAME: &'static str = "ctx_task";
            type Output = String;

            async fn run(&self, ctx: &TaskContext) -> TaskResult<Self::Output> {
                let prefix = ctx.data::<String>().cloned().unwrap_or_default();
                Ok(format!("{prefix}done"))
            }
        }

        let mut registry = TaskRegistry::new();
        registry.register::<CtxTask>();

        let mut ctx = TaskContext::new();
        ctx.insert("prefix:".to_string());
        let ctx = Arc::new(ctx);

        let result = registry
            .dispatch("ctx_task", serde_json::json!(null), ctx)
            .await
            .unwrap();
        assert_eq!(result, serde_json::json!("prefix:done"));
    }
}
