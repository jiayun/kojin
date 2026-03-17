use serde::{Serialize, de::DeserializeOwned};

use crate::error::TaskResult;

/// Serialization codec for task payloads.
pub trait Codec: Send + Sync + 'static {
    fn encode<T: Serialize>(&self, value: &T) -> TaskResult<Vec<u8>>;
    fn decode<T: DeserializeOwned>(&self, data: &[u8]) -> TaskResult<T>;
}

/// JSON codec using serde_json.
#[derive(Debug, Clone, Default)]
pub struct JsonCodec;

impl Codec for JsonCodec {
    fn encode<T: Serialize>(&self, value: &T) -> TaskResult<Vec<u8>> {
        Ok(serde_json::to_vec(value)?)
    }

    fn decode<T: DeserializeOwned>(&self, data: &[u8]) -> TaskResult<T> {
        Ok(serde_json::from_slice(data)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct TestPayload {
        name: String,
        value: i32,
    }

    #[test]
    fn json_codec_roundtrip() {
        let codec = JsonCodec;
        let payload = TestPayload {
            name: "test".to_string(),
            value: 42,
        };
        let encoded = codec.encode(&payload).unwrap();
        let decoded: TestPayload = codec.decode(&encoded).unwrap();
        assert_eq!(payload, decoded);
    }
}
