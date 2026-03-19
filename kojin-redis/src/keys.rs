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

    /// Get the key prefix.
    pub fn prefix(&self) -> &str {
        &self.prefix
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

    /// Result key: `{prefix}:result:{id}`
    pub fn result(&self, id: &str) -> String {
        format!("{}:result:{}", self.prefix, id)
    }

    /// Group total key: `{prefix}:group:{id}:total`
    pub fn group_total(&self, group_id: &str) -> String {
        format!("{}:group:{}:total", self.prefix, group_id)
    }

    /// Group completed counter key: `{prefix}:group:{id}:completed`
    pub fn group_completed(&self, group_id: &str) -> String {
        format!("{}:group:{}:completed", self.prefix, group_id)
    }

    /// Group results list key: `{prefix}:group:{id}:results`
    pub fn group_results(&self, group_id: &str) -> String {
        format!("{}:group:{}:results", self.prefix, group_id)
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
        assert_eq!(kb.result("abc"), "kojin:result:abc");
        assert_eq!(kb.group_total("g1"), "kojin:group:g1:total");
        assert_eq!(kb.group_completed("g1"), "kojin:group:g1:completed");
        assert_eq!(kb.group_results("g1"), "kojin:group:g1:results");
    }
}
