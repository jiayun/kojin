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
