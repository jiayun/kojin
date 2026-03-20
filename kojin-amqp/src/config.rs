/// Configuration for the AMQP broker.
#[derive(Debug, Clone)]
pub struct AmqpConfig {
    /// AMQP connection URI (e.g. `amqp://guest:guest@localhost:5672/%2f`).
    pub url: String,
    /// Direct exchange name for task routing. Default: `kojin.direct`.
    pub exchange: String,
    /// Dead-letter exchange name. Default: `kojin.dlx`.
    pub dlx_exchange: String,
    /// Delayed message exchange name (requires `rabbitmq-delayed-message-exchange` plugin).
    /// Default: `kojin.delayed`.
    pub delayed_exchange: String,
    /// Consumer prefetch count. Default: 10.
    pub prefetch_count: u16,
    /// Maximum priority level for queues (0 = disabled, max 10).
    /// Requires deleting and recreating existing queues when changing this value.
    pub max_priority: u8,
}

impl AmqpConfig {
    /// Create config with the given AMQP URL and sensible defaults.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            exchange: "kojin.direct".into(),
            dlx_exchange: "kojin.dlx".into(),
            delayed_exchange: "kojin.delayed".into(),
            prefetch_count: 10,
            max_priority: 0,
        }
    }

    pub fn with_exchange(mut self, exchange: impl Into<String>) -> Self {
        self.exchange = exchange.into();
        self
    }

    pub fn with_prefetch_count(mut self, count: u16) -> Self {
        self.prefetch_count = count;
        self
    }

    /// Enable priority queues (max 10). Changing this on existing queues requires
    /// deleting and recreating them (RabbitMQ `PRECONDITION_FAILED`).
    pub fn with_max_priority(mut self, max_priority: u8) -> Self {
        self.max_priority = max_priority.min(10);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let cfg = AmqpConfig::new("amqp://localhost");
        assert_eq!(cfg.url, "amqp://localhost");
        assert_eq!(cfg.exchange, "kojin.direct");
        assert_eq!(cfg.dlx_exchange, "kojin.dlx");
        assert_eq!(cfg.delayed_exchange, "kojin.delayed");
        assert_eq!(cfg.prefetch_count, 10);
        assert_eq!(cfg.max_priority, 0);
    }

    #[test]
    fn with_exchange() {
        let cfg = AmqpConfig::new("amqp://localhost").with_exchange("custom.exchange");
        assert_eq!(cfg.exchange, "custom.exchange");
    }

    #[test]
    fn with_prefetch_count() {
        let cfg = AmqpConfig::new("amqp://localhost").with_prefetch_count(50);
        assert_eq!(cfg.prefetch_count, 50);
    }

    #[test]
    fn with_max_priority_clamps_to_10() {
        let cfg = AmqpConfig::new("amqp://localhost").with_max_priority(255);
        assert_eq!(cfg.max_priority, 10);
    }

    #[test]
    fn with_max_priority_normal() {
        let cfg = AmqpConfig::new("amqp://localhost").with_max_priority(5);
        assert_eq!(cfg.max_priority, 5);
    }
}
