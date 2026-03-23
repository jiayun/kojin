use kojin::{Task, TaskContext, TaskResult};
use serde::{Deserialize, Serialize};

// -- AddTask: simple math for fan-out demos --

#[derive(Debug, Serialize, Deserialize)]
pub struct AddTask {
    pub a: i32,
    pub b: i32,
}

#[async_trait::async_trait]
impl Task for AddTask {
    const NAME: &'static str = "add";
    type Output = i32;

    async fn run(&self, _ctx: &TaskContext) -> TaskResult<Self::Output> {
        let result = self.a + self.b;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        tracing::info!("add({}, {}) = {}", self.a, self.b, result);
        Ok(result)
    }
}

// -- ProcessOrder: simulates work with configurable delay --

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessOrder {
    pub order_id: String,
    pub delay_ms: u64,
}

#[async_trait::async_trait]
impl Task for ProcessOrder {
    const NAME: &'static str = "process_order";
    const QUEUE: &'static str = "orders";
    const MAX_RETRIES: u32 = 5;
    type Output = String;

    async fn run(&self, _ctx: &TaskContext) -> TaskResult<Self::Output> {
        tracing::info!(
            "processing order {} ({}ms delay)",
            self.order_id,
            self.delay_ms
        );
        tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        tracing::info!("order {} completed", self.order_id);
        Ok(format!("order:{}:done", self.order_id))
    }
}

// -- PriorityTask: for weighted queues demo --

#[derive(Debug, Serialize, Deserialize)]
pub struct PriorityTask {
    pub label: String,
    pub priority: String,
}

#[async_trait::async_trait]
impl Task for PriorityTask {
    const NAME: &'static str = "priority_task";
    type Output = String;

    async fn run(&self, _ctx: &TaskContext) -> TaskResult<Self::Output> {
        tracing::info!("[{}] processing: {}", self.priority, self.label);
        Ok(format!("{}:{}", self.priority, self.label))
    }
}
