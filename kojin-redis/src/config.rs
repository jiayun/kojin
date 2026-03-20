/// Redis broker configuration.
#[derive(Debug, Clone)]
pub struct RedisConfig {
    /// Redis connection URL (e.g., "redis://127.0.0.1:6379").
    pub url: String,
    /// Connection pool size.
    pub pool_size: usize,
    /// Key prefix for all Redis keys.
    pub key_prefix: String,
    /// Deduplication TTL in seconds. Default: 300 (5 minutes).
    pub dedup_ttl: u64,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: "redis://127.0.0.1:6379".to_string(),
            pool_size: 10,
            key_prefix: "kojin".to_string(),
            dedup_ttl: 300,
        }
    }
}

impl RedisConfig {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Default::default()
        }
    }

    pub fn with_pool_size(mut self, size: usize) -> Self {
        self.pool_size = size;
        self
    }

    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.key_prefix = prefix.into();
        self
    }

    pub fn with_dedup_ttl(mut self, seconds: u64) -> Self {
        self.dedup_ttl = seconds;
        self
    }
}
