use std::fmt;

/// Core error type for Kojin.
#[derive(Debug, thiserror::Error)]
pub enum KojinError {
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("broker error: {0}")]
    Broker(String),

    #[error("task not found: {0}")]
    TaskNotFound(String),

    #[error("task failed: {0}")]
    TaskFailed(String),

    #[error("task timeout after {0:?}")]
    Timeout(std::time::Duration),

    #[error("task revoked: {0}")]
    Revoked(String),

    #[error("result backend error: {0}")]
    ResultBackend(String),

    #[error("queue not found: {0}")]
    QueueNotFound(String),

    #[error("codec error: {0}")]
    Codec(String),

    #[error("shutdown in progress")]
    ShutdownInProgress,

    #[error("{0}")]
    Other(String),
}

impl KojinError {
    pub fn broker(msg: impl fmt::Display) -> Self {
        Self::Broker(msg.to_string())
    }

    pub fn task_failed(msg: impl fmt::Display) -> Self {
        Self::TaskFailed(msg.to_string())
    }

    pub fn result_backend(msg: impl fmt::Display) -> Self {
        Self::ResultBackend(msg.to_string())
    }
}

/// Convenience result alias.
pub type TaskResult<T> = Result<T, KojinError>;
