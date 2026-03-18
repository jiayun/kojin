use async_trait::async_trait;
use deadpool_redis::Pool;
use redis::AsyncCommands;
use std::time::Duration;

use kojin_core::error::{KojinError, TaskResult};
use kojin_core::result_backend::ResultBackend;
use kojin_core::task_id::TaskId;

use crate::config::RedisConfig;
use crate::keys::KeyBuilder;

fn backend_err(e: impl std::fmt::Display) -> KojinError {
    KojinError::ResultBackend(e.to_string())
}

/// Lua script: atomically increment completed count and push result.
/// Returns the new completed count.
const GROUP_COMPLETE_SCRIPT: &str = r#"
local completed_key = KEYS[1]
local results_key = KEYS[2]
local result = ARGV[1]
local count = redis.call('INCR', completed_key)
redis.call('RPUSH', results_key, result)
return count
"#;

/// Redis-backed result storage.
///
/// Results are stored as JSON strings with a configurable TTL (default 24 hours).
/// Group operations (`complete_group_member`) use a Lua script to atomically
/// increment the completed count and push the result, preventing races when
/// multiple workers finish group members concurrently.
pub struct RedisResultBackend {
    pool: Pool,
    keys: KeyBuilder,
    ttl: Duration,
}

impl RedisResultBackend {
    /// Create a new Redis result backend.
    ///
    /// Builds a `deadpool_redis` connection pool from the given config and
    /// verifies connectivity by acquiring one connection. The default result
    /// TTL is 24 hours; override with [`with_ttl`](Self::with_ttl).
    pub async fn new(config: RedisConfig) -> TaskResult<Self> {
        let cfg = deadpool_redis::Config::from_url(&config.url);
        let pool = cfg
            .builder()
            .map_err(backend_err)?
            .max_size(config.pool_size)
            .runtime(deadpool_redis::Runtime::Tokio1)
            .build()
            .map_err(backend_err)?;

        // Verify connection
        let _conn = pool.get().await.map_err(backend_err)?;

        Ok(Self {
            pool,
            keys: KeyBuilder::new(config.key_prefix),
            ttl: Duration::from_secs(86400), // 24h default
        })
    }

    /// Override the result TTL (time-to-live).
    ///
    /// Results older than this duration are automatically expired by Redis.
    /// Defaults to 24 hours if not called.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    async fn conn(&self) -> TaskResult<deadpool_redis::Connection> {
        self.pool.get().await.map_err(backend_err)
    }
}

#[async_trait]
impl ResultBackend for RedisResultBackend {
    async fn store(&self, id: &TaskId, result: &serde_json::Value) -> TaskResult<()> {
        let mut conn = self.conn().await?;
        let key = self.keys.result(&id.to_string());
        let serialized = serde_json::to_string(result)?;
        let ttl_secs = self.ttl.as_secs() as i64;

        redis::cmd("SET")
            .arg(&key)
            .arg(&serialized)
            .arg("EX")
            .arg(ttl_secs)
            .query_async::<()>(&mut *conn)
            .await
            .map_err(backend_err)?;

        Ok(())
    }

    async fn get(&self, id: &TaskId) -> TaskResult<Option<serde_json::Value>> {
        let mut conn = self.conn().await?;
        let key = self.keys.result(&id.to_string());
        let result: Option<String> = conn.get(&key).await.map_err(backend_err)?;

        match result {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
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
        let mut conn = self.conn().await?;
        let key = self.keys.result(&id.to_string());
        conn.del::<_, ()>(&key).await.map_err(backend_err)?;
        Ok(())
    }

    async fn init_group(&self, group_id: &str, total: u32) -> TaskResult<()> {
        let mut conn = self.conn().await?;
        let key = self.keys.group_total(group_id);
        let ttl_secs = self.ttl.as_secs() as i64;

        redis::cmd("SET")
            .arg(&key)
            .arg(total)
            .arg("EX")
            .arg(ttl_secs)
            .query_async::<()>(&mut *conn)
            .await
            .map_err(backend_err)?;

        Ok(())
    }

    async fn complete_group_member(
        &self,
        group_id: &str,
        _task_id: &TaskId,
        result: &serde_json::Value,
    ) -> TaskResult<u32> {
        let mut conn = self.conn().await?;
        let completed_key = self.keys.group_completed(group_id);
        let results_key = self.keys.group_results(group_id);
        let serialized = serde_json::to_string(result)?;

        let script = redis::Script::new(GROUP_COMPLETE_SCRIPT);
        let count: u32 = script
            .key(&completed_key)
            .key(&results_key)
            .arg(&serialized)
            .invoke_async(&mut *conn)
            .await
            .map_err(backend_err)?;

        Ok(count)
    }

    async fn get_group_results(&self, group_id: &str) -> TaskResult<Vec<serde_json::Value>> {
        let mut conn = self.conn().await?;
        let results_key = self.keys.group_results(group_id);
        let items: Vec<String> = conn
            .lrange(&results_key, 0, -1)
            .await
            .map_err(backend_err)?;

        items
            .into_iter()
            .map(|s| serde_json::from_str(&s).map_err(Into::into))
            .collect()
    }
}

#[cfg(all(test, feature = "integration-tests"))]
mod tests {
    use super::*;
    use testcontainers::{ImageExt, runners::AsyncRunner};
    use testcontainers_modules::redis::Redis;

    async fn setup_backend() -> (RedisResultBackend, testcontainers::ContainerAsync<Redis>) {
        let container = Redis::default().with_tag("7").start().await.unwrap();
        let port = container.get_host_port_ipv4(6379).await.unwrap();
        let config = RedisConfig::new(format!("redis://127.0.0.1:{port}")).with_prefix("test");
        let backend = RedisResultBackend::new(config).await.unwrap();
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
