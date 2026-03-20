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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let cfg = SqsConfig::new(vec!["http://localhost/queue".into()]);
        assert_eq!(cfg.queue_urls, vec!["http://localhost/queue"]);
        assert_eq!(cfg.dlq_url, None);
        assert_eq!(cfg.visibility_timeout, 30);
        assert_eq!(cfg.wait_time_seconds, 20);
    }

    #[test]
    fn with_dlq_url() {
        let cfg = SqsConfig::new(vec![]).with_dlq_url("http://localhost/dlq");
        assert_eq!(cfg.dlq_url, Some("http://localhost/dlq".into()));
    }

    #[test]
    fn with_visibility_timeout() {
        let cfg = SqsConfig::new(vec![]).with_visibility_timeout(60);
        assert_eq!(cfg.visibility_timeout, 60);
    }

    #[test]
    fn with_wait_time_seconds() {
        let cfg = SqsConfig::new(vec![]).with_wait_time_seconds(10);
        assert_eq!(cfg.wait_time_seconds, 10);
    }

    #[test]
    fn builder_chain() {
        let cfg = SqsConfig::new(vec!["http://localhost/q".into()])
            .with_dlq_url("http://localhost/dlq")
            .with_visibility_timeout(45)
            .with_wait_time_seconds(5);
        assert_eq!(cfg.dlq_url, Some("http://localhost/dlq".into()));
        assert_eq!(cfg.visibility_timeout, 45);
        assert_eq!(cfg.wait_time_seconds, 5);
    }
}
