use async_trait::async_trait;
use sqlx::PgPool;
use std::time::Duration;

use kojin_core::error::{KojinError, TaskResult};
use kojin_core::result_backend::ResultBackend;
use kojin_core::task_id::TaskId;

fn backend_err(e: impl std::fmt::Display) -> KojinError {
    KojinError::ResultBackend(e.to_string())
}

/// PostgreSQL-backed result storage.
pub struct PostgresResultBackend {
    pool: PgPool,
    ttl: Duration,
}

impl PostgresResultBackend {
    /// Create a new PostgreSQL result backend from a connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            ttl: Duration::from_secs(86400), // 24h default
        }
    }

    /// Connect to PostgreSQL and create the backend.
    pub async fn connect(url: &str) -> TaskResult<Self> {
        let pool = PgPool::connect(url).await.map_err(backend_err)?;
        Ok(Self::new(pool))
    }

    /// Set result TTL.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    /// Run migrations to create the required tables.
    pub async fn migrate(&self) -> TaskResult<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS kojin_results (
                task_id TEXT PRIMARY KEY,
                result JSONB NOT NULL,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                expires_at TIMESTAMPTZ
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(backend_err)?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS kojin_groups (
                group_id TEXT NOT NULL,
                task_id TEXT NOT NULL,
                result JSONB,
                completed BOOLEAN DEFAULT FALSE,
                PRIMARY KEY (group_id, task_id)
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(backend_err)?;

        // Index for cleanup of expired results
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_kojin_results_expires
            ON kojin_results (expires_at)
            WHERE expires_at IS NOT NULL
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(backend_err)?;

        Ok(())
    }
}

#[async_trait]
impl ResultBackend for PostgresResultBackend {
    async fn store(&self, id: &TaskId, result: &serde_json::Value) -> TaskResult<()> {
        let expires_at = chrono::Utc::now() + chrono::Duration::seconds(self.ttl.as_secs() as i64);

        sqlx::query(
            r#"
            INSERT INTO kojin_results (task_id, result, expires_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (task_id) DO UPDATE SET result = $2, expires_at = $3
            "#,
        )
        .bind(id.to_string())
        .bind(result)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(backend_err)?;

        Ok(())
    }

    async fn get(&self, id: &TaskId) -> TaskResult<Option<serde_json::Value>> {
        let row: Option<(serde_json::Value,)> = sqlx::query_as(
            r#"
            SELECT result FROM kojin_results
            WHERE task_id = $1 AND (expires_at IS NULL OR expires_at > NOW())
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(backend_err)?;

        Ok(row.map(|(r,)| r))
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
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn delete(&self, id: &TaskId) -> TaskResult<()> {
        sqlx::query("DELETE FROM kojin_results WHERE task_id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(backend_err)?;
        Ok(())
    }

    async fn init_group(&self, group_id: &str, total: u32) -> TaskResult<()> {
        // Pre-create placeholder rows for the group
        // We use a single INSERT with generate_series for efficiency
        for i in 0..total {
            let placeholder_id = format!("{group_id}:placeholder:{i}");
            sqlx::query(
                r#"
                INSERT INTO kojin_groups (group_id, task_id, completed)
                VALUES ($1, $2, FALSE)
                ON CONFLICT (group_id, task_id) DO NOTHING
                "#,
            )
            .bind(group_id)
            .bind(&placeholder_id)
            .execute(&self.pool)
            .await
            .map_err(backend_err)?;
        }
        Ok(())
    }

    async fn complete_group_member(
        &self,
        group_id: &str,
        task_id: &TaskId,
        result: &serde_json::Value,
    ) -> TaskResult<u32> {
        // Upsert the actual task result
        sqlx::query(
            r#"
            INSERT INTO kojin_groups (group_id, task_id, result, completed)
            VALUES ($1, $2, $3, TRUE)
            ON CONFLICT (group_id, task_id) DO UPDATE SET result = $3, completed = TRUE
            "#,
        )
        .bind(group_id)
        .bind(task_id.to_string())
        .bind(result)
        .execute(&self.pool)
        .await
        .map_err(backend_err)?;

        // Count completed members (only those with completed = TRUE and actual results)
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM kojin_groups
            WHERE group_id = $1 AND completed = TRUE AND result IS NOT NULL
            "#,
        )
        .bind(group_id)
        .fetch_one(&self.pool)
        .await
        .map_err(backend_err)?;

        Ok(count as u32)
    }

    async fn get_group_results(&self, group_id: &str) -> TaskResult<Vec<serde_json::Value>> {
        let rows: Vec<(serde_json::Value,)> = sqlx::query_as(
            r#"
            SELECT result FROM kojin_groups
            WHERE group_id = $1 AND completed = TRUE AND result IS NOT NULL
            ORDER BY task_id
            "#,
        )
        .bind(group_id)
        .fetch_all(&self.pool)
        .await
        .map_err(backend_err)?;

        Ok(rows.into_iter().map(|(r,)| r).collect())
    }
}

#[cfg(all(test, feature = "integration-tests"))]
mod tests {
    use super::*;
    use testcontainers::{ImageExt, runners::AsyncRunner};
    use testcontainers_modules::postgres::Postgres;

    async fn setup_backend() -> (
        PostgresResultBackend,
        testcontainers::ContainerAsync<Postgres>,
    ) {
        let container = Postgres::default().with_tag("16").start().await.unwrap();
        let port = container.get_host_port_ipv4(5432).await.unwrap();
        let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

        let backend = PostgresResultBackend::connect(&url).await.unwrap();
        backend.migrate().await.unwrap();
        (backend, container)
    }

    #[tokio::test]
    async fn store_and_get() {
        let (backend, _container) = setup_backend().await;
        let id = TaskId::new();
        let value = serde_json::json!({"result": 42});

        backend.store(&id, &value).await.unwrap();
        let got = backend.get(&id).await.unwrap();
        assert_eq!(got, Some(value));
    }

    #[tokio::test]
    async fn get_missing() {
        let (backend, _container) = setup_backend().await;
        let id = TaskId::new();
        assert_eq!(backend.get(&id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn delete_result() {
        let (backend, _container) = setup_backend().await;
        let id = TaskId::new();
        backend.store(&id, &serde_json::json!(1)).await.unwrap();
        backend.delete(&id).await.unwrap();
        assert_eq!(backend.get(&id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn group_completion() {
        let (backend, _container) = setup_backend().await;
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
