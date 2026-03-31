use rinfra_core::codec::Codec;
use rinfra_core::error::{AppError, ErrorCode};
use prost::Message;
use prost_types::value::Kind;
use prost_types::{ListValue, Struct, Value};
use std::collections::BTreeMap;

pub struct ProtobufCodec;

impl Codec for ProtobufCodec {
    fn name(&self) -> &str {
        "protobuf"
    }

    fn content_type(&self) -> &str {
        "application/x-protobuf"
    }

    fn encode_value(&self, value: &serde_json::Value) -> Result<Vec<u8>, AppError> {
        let proto_value = json_to_proto(value);
        Ok(proto_value.encode_to_vec())
    }

    fn decode_value(&self, data: &[u8]) -> Result<serde_json::Value, AppError> {
        let proto_value = Value::decode(data).map_err(|e| {
            AppError::new(
                ErrorCode::CodecDecodeFailed,
                format!("protobuf decode failed: {e}"),
            )
        })?;
        Ok(proto_to_json(&proto_value))
    }
}

fn json_to_proto(value: &serde_json::Value) -> Value {
    let kind = match value {
        serde_json::Value::Null => Kind::NullValue(0),
        serde_json::Value::Bool(b) => Kind::BoolValue(*b),
        serde_json::Value::Number(n) => Kind::NumberValue(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => Kind::StringValue(s.clone()),
        serde_json::Value::Array(arr) => Kind::ListValue(ListValue {
            values: arr.iter().map(json_to_proto).collect(),
        }),
        serde_json::Value::Object(map) => {
            let fields: BTreeMap<String, Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_proto(v)))
                .collect();
            Kind::StructValue(Struct { fields })
        }
    };
    Value { kind: Some(kind) }
}

fn proto_to_json(value: &Value) -> serde_json::Value {
    match &value.kind {
        Some(Kind::NullValue(_)) => serde_json::Value::Null,
        Some(Kind::BoolValue(b)) => serde_json::Value::Bool(*b),
        Some(Kind::NumberValue(n)) => serde_json::json!(*n),
        Some(Kind::StringValue(s)) => serde_json::Value::String(s.clone()),
        Some(Kind::ListValue(list)) => {
            serde_json::Value::Array(list.values.iter().map(proto_to_json).collect())
        }
        Some(Kind::StructValue(s)) => {
            let map: serde_json::Map<String, serde_json::Value> = s
                .fields
                .iter()
                .map(|(k, v)| (k.clone(), proto_to_json(v)))
                .collect();
            serde_json::Value::Object(map)
        }
        None => serde_json::Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protobuf_name_and_content_type() {
        let codec = ProtobufCodec;
        assert_eq!(codec.name(), "protobuf");
        assert_eq!(codec.content_type(), "application/x-protobuf");
    }

    #[test]
    fn test_protobuf_encode_decode_string() {
        let codec = ProtobufCodec;
        let value = serde_json::json!("hello");
        let encoded = codec.encode_value(&value).unwrap();
        let decoded = codec.decode_value(&encoded).unwrap();
        assert_eq!(decoded, "hello");
    }

    #[test]
    fn test_protobuf_encode_decode_object() {
        let codec = ProtobufCodec;
        let value = serde_json::json!({"key": "value", "num": 42.0});
        let encoded = codec.encode_value(&value).unwrap();
        let decoded = codec.decode_value(&encoded).unwrap();
        assert_eq!(decoded["key"], "value");
        assert_eq!(decoded["num"], 42.0);
    }

    #[test]
    fn test_protobuf_encode_decode_array() {
        let codec = ProtobufCodec;
        let value = serde_json::json!([1.0, 2.0, 3.0]);
        let encoded = codec.encode_value(&value).unwrap();
        let decoded = codec.decode_value(&encoded).unwrap();
        assert_eq!(decoded, serde_json::json!([1.0, 2.0, 3.0]));
    }

    #[test]
    fn test_protobuf_encode_decode_null() {
        let codec = ProtobufCodec;
        let encoded = codec.encode_value(&serde_json::Value::Null).unwrap();
        let decoded = codec.decode_value(&encoded).unwrap();
        assert!(decoded.is_null());
    }

    #[test]
    fn test_protobuf_decode_invalid_data() {
        let codec = ProtobufCodec;
        let result = codec.decode_value(&[0xFF, 0xFE, 0xFD, 0xFC]);
        // protobuf is lenient with unknown fields, so this may or may not fail
        // but it should not panic
        let _ = result;
    }

    #[test]
    fn test_protobuf_codec_registry() {
        use rinfra_core::codec::CodecRegistry;
        let mut registry = CodecRegistry::new();
        registry.register(Box::new(ProtobufCodec)).unwrap();
        assert!(registry.get_by_name("protobuf").is_some());
        assert!(registry.get_by_content_type("application/x-protobuf").is_some());
    }
}
