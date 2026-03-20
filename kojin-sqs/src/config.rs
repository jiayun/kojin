/// Configuration for the SQS broker.
#[derive(Debug, Clone)]
pub struct SqsConfig {
    /// SQS queue URLs (standard or FIFO).
    pub queue_urls: Vec<String>,
    /// Dead-letter queue URL (optional — prefer SQS native RedrivePolicy).
    pub dlq_url: Option<String>,
    /// Visibility timeout in seconds. Default: 30.
    pub visibility_timeout: i32,
    /// Long-poll wait time in seconds. Default: 20.
    pub wait_time_seconds: i32,
}

impl SqsConfig {
    /// Create a config with the given queue URLs.
    pub fn new(queue_urls: Vec<String>) -> Self {
        Self {
            queue_urls,
            dlq_url: None,
            visibility_timeout: 30,
            wait_time_seconds: 20,
        }
    }

    pub fn with_dlq_url(mut self, url: impl Into<String>) -> Self {
        self.dlq_url = Some(url.into());
        self
    }

    pub fn with_visibility_timeout(mut self, seconds: i32) -> Self {
        self.visibility_timeout = seconds;
        self
    }

    pub fn with_wait_time_seconds(mut self, seconds: i32) -> Self {
        self.wait_time_seconds = seconds;
        self
    }
}
