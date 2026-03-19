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
}
