use std::collections::HashMap;
use std::sync::Arc;

use crate::compress::Compressor;
use crate::error::{AppError, ErrorCode};

/// Bidirectional byte-level transform for network pipelines.
/// Each implementation handles exactly one concern (compression, encryption, etc.).
///
/// - `decode`: inbound direction (received bytes → decompressed/decrypted)
/// - `encode`: outbound direction (bytes to send → compressed/encrypted)
pub trait ByteTransform: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn decode(&self, data: Vec<u8>) -> Result<Vec<u8>, AppError>;
    fn encode(&self, data: Vec<u8>) -> Result<Vec<u8>, AppError>;
}

/// Adapter that bridges a general-purpose `Compressor` into a network `ByteTransform`.
pub struct CompressorTransform(pub Arc<dyn Compressor>);

impl ByteTransform for CompressorTransform {
    fn name(&self) -> &str {
        self.0.name()
    }
    fn encode(&self, data: Vec<u8>) -> Result<Vec<u8>, AppError> {
        self.0.compress(&data)
    }
    fn decode(&self, data: Vec<u8>) -> Result<Vec<u8>, AppError> {
        self.0.decompress(&data)
    }
}

/// Registry of named `ByteTransform` implementations.
pub struct TransformRegistry {
    transforms: HashMap<String, Arc<dyn ByteTransform>>,
}

impl TransformRegistry {
    pub fn new() -> Self {
        Self {
            transforms: HashMap::new(),
        }
    }

    pub fn register(&mut self, transform: Arc<dyn ByteTransform>) -> Result<(), AppError> {
        let name = transform.name().to_string();
        if self.transforms.contains_key(&name) {
            return Err(AppError::new(
                ErrorCode::PluginAlreadyRegistered,
                format!("transform '{}' already registered", name),
            ));
        }
        self.transforms.insert(name, transform);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn ByteTransform>> {
        self.transforms.get(name)
    }

    pub fn names(&self) -> Vec<&str> {
        self.transforms.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for TransformRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoopTransform;

    impl ByteTransform for NoopTransform {
        fn name(&self) -> &str {
            "noop"
        }
        fn decode(&self, data: Vec<u8>) -> Result<Vec<u8>, AppError> {
            Ok(data)
        }
        fn encode(&self, data: Vec<u8>) -> Result<Vec<u8>, AppError> {
            Ok(data)
        }
    }

    #[test]
    fn test_register_and_get() {
        let mut reg = TransformRegistry::new();
        reg.register(Arc::new(NoopTransform)).unwrap();
        assert!(reg.get("noop").is_some());
        assert!(reg.get("missing").is_none());
    }

    #[test]
    fn test_register_duplicate_errors() {
        let mut reg = TransformRegistry::new();
        reg.register(Arc::new(NoopTransform)).unwrap();
        let result = reg.register(Arc::new(NoopTransform));
        assert!(result.is_err());
    }

    #[test]
    fn test_names() {
        let mut reg = TransformRegistry::new();
        reg.register(Arc::new(NoopTransform)).unwrap();
        assert_eq!(reg.names(), vec!["noop"]);
    }

    #[test]
    fn test_noop_roundtrip() {
        let t = NoopTransform;
        let data = b"hello world".to_vec();
        let encoded = t.encode(data.clone()).unwrap();
        let decoded = t.decode(encoded).unwrap();
        assert_eq!(decoded, data);
    }
}
