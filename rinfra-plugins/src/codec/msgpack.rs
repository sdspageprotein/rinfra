use rinfra_core::codec::Codec;
use rinfra_core::error::{AppError, ErrorCode};

pub struct MsgpackCodec;

impl Codec for MsgpackCodec {
    fn name(&self) -> &str {
        "msgpack"
    }

    fn content_type(&self) -> &str {
        "application/msgpack"
    }

    fn encode_value(&self, value: &serde_json::Value) -> Result<Vec<u8>, AppError> {
        rmp_serde::to_vec(value)
            .map_err(|e| AppError::new(ErrorCode::CodecEncodeFailed, format!("msgpack encode failed: {e}")))
    }

    fn decode_value(&self, data: &[u8]) -> Result<serde_json::Value, AppError> {
        rmp_serde::from_slice(data)
            .map_err(|e| AppError::new(ErrorCode::CodecDecodeFailed, format!("msgpack decode failed: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_msgpack_name_and_content_type() {
        let codec = MsgpackCodec;
        assert_eq!(codec.name(), "msgpack");
        assert_eq!(codec.content_type(), "application/msgpack");
    }

    #[test]
    fn test_msgpack_encode_decode_roundtrip() {
        let codec = MsgpackCodec;
        let value = serde_json::json!({"hello": "world", "num": 42});
        let encoded = codec.encode_value(&value).unwrap();
        let decoded = codec.decode_value(&encoded).unwrap();
        assert_eq!(decoded["hello"], "world");
        assert_eq!(decoded["num"], 42);
    }

    #[test]
    fn test_msgpack_encode_array() {
        let codec = MsgpackCodec;
        let value = serde_json::json!([1, 2, 3]);
        let encoded = codec.encode_value(&value).unwrap();
        let decoded = codec.decode_value(&encoded).unwrap();
        assert_eq!(decoded, serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn test_msgpack_decode_invalid_data() {
        let codec = MsgpackCodec;
        // Truncated/invalid msgpack: fixstr with missing payload
        let result = codec.decode_value(&[0xBF]);
        assert!(result.is_err());
    }

    #[test]
    fn test_msgpack_encode_null() {
        let codec = MsgpackCodec;
        let encoded = codec.encode_value(&serde_json::Value::Null).unwrap();
        let decoded = codec.decode_value(&encoded).unwrap();
        assert!(decoded.is_null());
    }

    #[test]
    fn test_msgpack_codec_registry() {
        use rinfra_core::codec::CodecRegistry;
        let mut registry = CodecRegistry::new();
        registry.register(Box::new(MsgpackCodec)).unwrap();
        let codec = registry.get_by_name("msgpack");
        assert!(codec.is_some());
        let codec = registry.get_by_content_type("application/msgpack");
        assert!(codec.is_some());
    }
}
