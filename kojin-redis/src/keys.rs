/// Helper for constructing Redis key names with a common prefix.
#[derive(Debug, Clone)]
pub struct KeyBuilder {
    prefix: String,
}

impl KeyBuilder {
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }

    /// Queue key: `{prefix}:queue:{name}`
    pub fn queue(&self, name: &str) -> String {
        format!("{}:queue:{}", self.prefix, name)
    }

    /// Processing queue key: `{prefix}:processing:{worker_id}`
    pub fn processing(&self, worker_id: &str) -> String {
        format!("{}:processing:{}", self.prefix, worker_id)
    }

    /// Scheduled set key: `{prefix}:scheduled`
    pub fn scheduled(&self) -> String {
        format!("{}:scheduled", self.prefix)
    }

    /// Dead-letter queue key: `{prefix}:dlq:{name}`
    pub fn dlq(&self, name: &str) -> String {
        format!("{}:dlq:{}", self.prefix, name)
    }

    /// Message data key: `{prefix}:msg:{id}`
    pub fn message(&self, id: &str) -> String {
        format!("{}:msg:{}", self.prefix, id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_naming() {
        let kb = KeyBuilder::new("kojin");
        assert_eq!(kb.queue("default"), "kojin:queue:default");
        assert_eq!(kb.processing("w1"), "kojin:processing:w1");
        assert_eq!(kb.scheduled(), "kojin:scheduled");
        assert_eq!(kb.dlq("default"), "kojin:dlq:default");
        assert_eq!(kb.message("abc"), "kojin:msg:abc");
    }
}
