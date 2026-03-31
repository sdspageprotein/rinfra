use std::collections::HashMap;

use tracing::info;

use super::Codec;
use crate::error::{AppError, ErrorCode};

pub struct CodecRegistry {
    codecs: Vec<Box<dyn Codec>>,
    name_index: HashMap<String, usize>,
    content_type_index: HashMap<String, usize>,
}

impl CodecRegistry {
    pub fn new() -> Self {
        Self {
            codecs: Vec::new(),
            name_index: HashMap::new(),
            content_type_index: HashMap::new(),
        }
    }

    pub fn register(&mut self, codec: Box<dyn Codec>) -> Result<(), AppError> {
        let name = codec.name().to_string();
        if self.name_index.contains_key(&name) {
            return Err(AppError::new(
                ErrorCode::PluginAlreadyRegistered,
                format!("codec '{name}' is already registered"),
            ));
        }
        let ct = codec.content_type().to_string();
        let idx = self.codecs.len();
        info!(codec_name = %name, content_type = %ct, "codec registered");
        self.name_index.insert(name, idx);
        self.content_type_index.insert(ct, idx);
        self.codecs.push(codec);
        Ok(())
    }

    pub fn get_by_name(&self, name: &str) -> Option<&dyn Codec> {
        self.name_index
            .get(name)
            .map(|&idx| self.codecs[idx].as_ref())
    }

    pub fn get_by_content_type(&self, ct: &str) -> Option<&dyn Codec> {
        self.content_type_index
            .get(ct)
            .map(|&idx| self.codecs[idx].as_ref())
    }

    pub fn list_names(&self) -> Vec<&str> {
        self.codecs.iter().map(|c| c.name()).collect()
    }
}

impl Default for CodecRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubCodec;
    impl Codec for StubCodec {
        fn name(&self) -> &str { "stub" }
        fn content_type(&self) -> &str { "application/stub" }
        fn encode_value(&self, _: &serde_json::Value) -> Result<Vec<u8>, AppError> { Ok(vec![]) }
        fn decode_value(&self, _: &[u8]) -> Result<serde_json::Value, AppError> { Ok(serde_json::Value::Null) }
    }

    #[test]
    fn test_register_and_get_by_name() {
        let mut reg = CodecRegistry::new();
        reg.register(Box::new(StubCodec)).unwrap();
        let codec = reg.get_by_name("stub").unwrap();
        assert_eq!(codec.name(), "stub");
    }

    #[test]
    fn test_get_by_content_type() {
        let mut reg = CodecRegistry::new();
        reg.register(Box::new(StubCodec)).unwrap();
        let codec = reg.get_by_content_type("application/stub").unwrap();
        assert_eq!(codec.name(), "stub");
    }

    #[test]
    fn test_get_nonexistent_returns_none() {
        let reg = CodecRegistry::new();
        assert!(reg.get_by_name("msgpack").is_none());
        assert!(reg.get_by_content_type("application/msgpack").is_none());
    }

    #[test]
    fn test_register_duplicate_returns_error() {
        let mut reg = CodecRegistry::new();
        reg.register(Box::new(StubCodec)).unwrap();

        struct StubCodec2;
        impl Codec for StubCodec2 {
            fn name(&self) -> &str { "stub" }
            fn content_type(&self) -> &str { "application/stub" }
            fn encode_value(&self, _: &serde_json::Value) -> Result<Vec<u8>, AppError> { Ok(vec![]) }
            fn decode_value(&self, _: &[u8]) -> Result<serde_json::Value, AppError> { Ok(serde_json::Value::Null) }
        }

        let result = reg.register(Box::new(StubCodec2));
        assert!(result.is_err());
    }
}
