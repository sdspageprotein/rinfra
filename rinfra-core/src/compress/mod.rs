use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{AppError, ErrorCode};

/// General-purpose compression/decompression abstraction.
/// Independent of any transport layer — usable by network, storage, WAL, etc.
pub trait Compressor: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>, AppError>;
    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>, AppError>;
}

/// Registry of named `Compressor` implementations.
pub struct CompressorRegistry {
    compressors: HashMap<String, Arc<dyn Compressor>>,
}

impl CompressorRegistry {
    pub fn new() -> Self {
        Self {
            compressors: HashMap::new(),
        }
    }

    pub fn register(&mut self, compressor: Arc<dyn Compressor>) -> Result<(), AppError> {
        let name = compressor.name().to_string();
        if self.compressors.contains_key(&name) {
            return Err(AppError::new(
                ErrorCode::PluginAlreadyRegistered,
                format!("compressor '{}' already registered", name),
            ));
        }
        self.compressors.insert(name, compressor);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn Compressor>> {
        self.compressors.get(name)
    }

    pub fn names(&self) -> Vec<&str> {
        self.compressors.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for CompressorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoopCompressor;

    impl Compressor for NoopCompressor {
        fn name(&self) -> &str {
            "noop"
        }
        fn compress(&self, data: &[u8]) -> Result<Vec<u8>, AppError> {
            Ok(data.to_vec())
        }
        fn decompress(&self, data: &[u8]) -> Result<Vec<u8>, AppError> {
            Ok(data.to_vec())
        }
    }

    #[test]
    fn test_register_and_get() {
        let mut reg = CompressorRegistry::new();
        reg.register(Arc::new(NoopCompressor)).unwrap();
        assert!(reg.get("noop").is_some());
        assert!(reg.get("missing").is_none());
    }

    #[test]
    fn test_register_duplicate_errors() {
        let mut reg = CompressorRegistry::new();
        reg.register(Arc::new(NoopCompressor)).unwrap();
        assert!(reg.register(Arc::new(NoopCompressor)).is_err());
    }

    #[test]
    fn test_noop_roundtrip() {
        let c = NoopCompressor;
        let data = b"hello world".to_vec();
        let compressed = c.compress(&data).unwrap();
        let decompressed = c.decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }
}
