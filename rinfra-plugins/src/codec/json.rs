use rinfra_core::codec::Codec;
use rinfra_core::error::{AppError, ErrorCode};

pub struct JsonCodec;

impl JsonCodec {
    pub fn new() -> Self {
        Self
    }
}

impl Default for JsonCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl Codec for JsonCodec {
    fn name(&self) -> &str {
        "json"
    }

    fn content_type(&self) -> &str {
        "application/json"
    }

    fn encode_value(&self, value: &serde_json::Value) -> Result<Vec<u8>, AppError> {
        serde_json::to_vec(value).map_err(|e| {
            AppError::new(
                ErrorCode::CodecEncodeFailed,
                format!("json encode failed: {e}"),
            )
        })
    }

    fn decode_value(&self, data: &[u8]) -> Result<serde_json::Value, AppError> {
        serde_json::from_slice(data).map_err(|e| {
            AppError::new(
                ErrorCode::CodecDecodeFailed,
                format!("json decode failed: {e}"),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinfra_core::codec::Codec;
    use serde_json::json;

    #[test]
    fn test_json_codec_name_and_content_type() {
        let codec = JsonCodec::new();
        assert_eq!(codec.name(), "json");
        assert_eq!(codec.content_type(), "application/json");
    }

    #[test]
    fn test_json_codec_encode_decode_roundtrip() {
        let codec = JsonCodec::new();
        let value = json!({"name": "alice", "age": 30});
        let bytes = codec.encode_value(&value).unwrap();
        let decoded = codec.decode_value(&bytes).unwrap();
        assert_eq!(value, decoded);
    }

    #[test]
    fn test_json_codec_decode_invalid_data() {
        let codec = JsonCodec::new();
        let result = codec.decode_value(b"not json {{{");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::CodecDecodeFailed);
    }

    #[test]
    fn test_json_codec_encode_primitives() {
        let codec = JsonCodec::new();
        let bytes = codec.encode_value(&json!(42)).unwrap();
        assert_eq!(bytes, b"42");
    }

    #[test]
    fn test_json_codec_encode_null() {
        let codec = JsonCodec::new();
        let bytes = codec.encode_value(&json!(null)).unwrap();
        assert_eq!(bytes, b"null");
    }

    #[test]
    fn test_json_codec_decode_array() {
        let codec = JsonCodec::new();
        let value = codec.decode_value(b"[1,2,3]").unwrap();
        assert_eq!(value, json!([1, 2, 3]));
    }

    #[test]
    fn test_codec_registry_with_json() {
        use rinfra_core::codec::CodecRegistry;
        let mut reg = CodecRegistry::new();
        reg.register(Box::new(JsonCodec::new())).unwrap();
        let codec = reg.get_by_name("json").unwrap();
        assert_eq!(codec.name(), "json");

        let val = json!({"key": "value"});
        let encoded = codec.encode_value(&val).unwrap();
        let decoded = codec.decode_value(&encoded).unwrap();
        assert_eq!(val, decoded);
    }
}
