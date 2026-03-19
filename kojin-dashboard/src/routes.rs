use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

use kojin_core::task_id::TaskId;

use crate::state::DashboardState;

#[derive(Serialize)]
struct QueueInfo {
    name: String,
    length: usize,
    dlq_length: usize,
}

#[derive(Serialize)]
struct MetricsResponse {
    tasks_started: u64,
    tasks_succeeded: u64,
    tasks_failed: u64,
}

#[derive(Deserialize)]
pub struct PaginationParams {
    #[serde(default)]
    offset: usize,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    20
}

/// `GET /api/queues` — list all queues with lengths.
pub async fn list_queues(
    State(state): State<DashboardState>,
) -> Result<impl IntoResponse, StatusCode> {
    let queues = state
        .broker
        .list_queues()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut result = Vec::with_capacity(queues.len());
    for name in queues {
        let length = state.broker.queue_len(&name).await.unwrap_or(0);
        let dlq_length = state.broker.dlq_len(&name).await.unwrap_or(0);
        result.push(QueueInfo {
            name,
            length,
            dlq_length,
        });
    }

    Ok(Json(result))
}

/// `GET /api/queues/:name` — single queue detail.
pub async fn get_queue(
    State(state): State<DashboardState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let length = state
        .broker
        .queue_len(&name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let dlq_length = state.broker.dlq_len(&name).await.unwrap_or(0);

    Ok(Json(QueueInfo {
        name,
        length,
        dlq_length,
    }))
}

/// `GET /api/queues/:name/dlq` — paginated dead-letter queue messages.
pub async fn get_dlq(
    State(state): State<DashboardState>,
    Path(name): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Result<impl IntoResponse, StatusCode> {
    let messages = state
        .broker
        .dlq_messages(&name, params.offset, params.limit)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(messages))
}

/// `GET /api/metrics` — throughput counters.
pub async fn get_metrics(
    State(state): State<DashboardState>,
) -> Result<impl IntoResponse, StatusCode> {
    match &state.metrics {
        Some(m) => Ok(Json(MetricsResponse {
            tasks_started: m.tasks_started(),
            tasks_succeeded: m.tasks_succeeded(),
            tasks_failed: m.tasks_failed(),
        })),
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// `GET /api/tasks/:id` — look up a task result.
pub async fn get_task_result(
    State(state): State<DashboardState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let backend = state.result_backend.as_ref().ok_or(StatusCode::NOT_FOUND)?;

    let task_id: TaskId = id
        .parse::<uuid::Uuid>()
        .map(TaskId::from_uuid)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    match backend.get(&task_id).await {
        Ok(Some(value)) => Ok(Json(value)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use kojin_core::broker::Broker;
    use kojin_core::message::TaskMessage;
    use kojin_core::middleware::{MetricsMiddleware, Middleware};
    use kojin_core::result_backend::ResultBackend;
    use kojin_core::{MemoryBroker, MemoryResultBackend};

    use crate::{DashboardState, dashboard_router};

    fn app(state: DashboardState) -> axum::Router {
        dashboard_router(state)
    }

    async fn get_json(app: axum::Router, uri: &str) -> (axum::http::StatusCode, serde_json::Value) {
        let resp = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value =
            serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    #[tokio::test]
    async fn list_queues_empty() {
        let broker = Arc::new(MemoryBroker::new());
        let state = DashboardState::new(broker);
        let (status, json) = get_json(app(state), "/api/queues").await;
        assert_eq!(status, 200);
        assert_eq!(json, serde_json::json!([]));
    }

    #[tokio::test]
    async fn list_queues_with_data() {
        let broker = Arc::new(MemoryBroker::new());
        broker
            .enqueue(TaskMessage::new("t", "default", serde_json::json!(1)))
            .await
            .unwrap();
        let state = DashboardState::new(broker);
        let (status, json) = get_json(app(state), "/api/queues").await;
        assert_eq!(status, 200);
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"], "default");
        assert_eq!(arr[0]["length"], 1);
    }

    #[tokio::test]
    async fn get_queue_detail() {
        let broker = Arc::new(MemoryBroker::new());
        broker
            .enqueue(TaskMessage::new("t", "default", serde_json::json!(1)))
            .await
            .unwrap();
        broker
            .enqueue(TaskMessage::new("t", "default", serde_json::json!(2)))
            .await
            .unwrap();
        let state = DashboardState::new(broker);
        let (status, json) = get_json(app(state), "/api/queues/default").await;
        assert_eq!(status, 200);
        assert_eq!(json["name"], "default");
        assert_eq!(json["length"], 2);
        assert_eq!(json["dlq_length"], 0);
    }

    #[tokio::test]
    async fn get_dlq_messages() {
        let broker = Arc::new(MemoryBroker::new());
        let msg = TaskMessage::new("failing_task", "default", serde_json::json!({}));
        broker.enqueue(msg).await.unwrap();
        let out = broker
            .dequeue(&["default".to_string()], std::time::Duration::from_secs(1))
            .await
            .unwrap()
            .unwrap();
        broker.dead_letter(out).await.unwrap();

        let state = DashboardState::new(broker);
        let (status, json) = get_json(app(state), "/api/queues/default/dlq").await;
        assert_eq!(status, 200);
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["task_name"], "failing_task");
    }

    #[tokio::test]
    async fn get_metrics_with_middleware() {
        let broker = Arc::new(MemoryBroker::new());
        let metrics = MetricsMiddleware::new();
        let msg = TaskMessage::new("test", "default", serde_json::json!({}));
        metrics.before(&msg).await.unwrap();
        metrics.after(&msg, &serde_json::json!("ok")).await.unwrap();

        let state = DashboardState::new(broker).with_metrics(metrics);
        let (status, json) = get_json(app(state), "/api/metrics").await;
        assert_eq!(status, 200);
        assert_eq!(json["tasks_started"], 1);
        assert_eq!(json["tasks_succeeded"], 1);
        assert_eq!(json["tasks_failed"], 0);
    }

    #[tokio::test]
    async fn get_metrics_without_middleware() {
        let broker = Arc::new(MemoryBroker::new());
        let state = DashboardState::new(broker);
        let (status, _) = get_json(app(state), "/api/metrics").await;
        assert_eq!(status, 404);
    }

    #[tokio::test]
    async fn get_task_result_found() {
        let broker = Arc::new(MemoryBroker::new());
        let backend = Arc::new(MemoryResultBackend::new());
        let task_id = kojin_core::TaskId::new();
        backend
            .store(&task_id, &serde_json::json!({"answer": 42}))
            .await
            .unwrap();

        let state = DashboardState::new(broker).with_result_backend(backend);
        let uri = format!("/api/tasks/{}", task_id.as_uuid());
        let (status, json) = get_json(app(state), &uri).await;
        assert_eq!(status, 200);
        assert_eq!(json["answer"], 42);
    }

    #[tokio::test]
    async fn get_task_result_not_found() {
        let broker = Arc::new(MemoryBroker::new());
        let backend = Arc::new(MemoryResultBackend::new());
        let state = DashboardState::new(broker).with_result_backend(backend);
        let random_id = uuid::Uuid::now_v7();
        let uri = format!("/api/tasks/{random_id}");
        let (status, _) = get_json(app(state), &uri).await;
        assert_eq!(status, 404);
    }

    #[tokio::test]
    async fn get_task_result_no_backend() {
        let broker = Arc::new(MemoryBroker::new());
        let state = DashboardState::new(broker); // no result_backend
        let random_id = uuid::Uuid::now_v7();
        let uri = format!("/api/tasks/{random_id}");
        let (status, _) = get_json(app(state), &uri).await;
        assert_eq!(status, 404);
    }
}
