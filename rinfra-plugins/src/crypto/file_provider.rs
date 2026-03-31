use async_trait::async_trait;
use tracing::info;

use rinfra_core::config::FileKeyConfig;
use rinfra_core::crypto::KeyProvider;
use rinfra_core::error::{AppError, ErrorCode};

/// File-based key provider that reads keys from an encrypted key file.
/// Current implementation is a stub.
pub struct FileKeyProvider {
    config: FileKeyConfig,
}

impl FileKeyProvider {
    pub fn new(config: &FileKeyConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }
}

#[async_trait]
impl KeyProvider for FileKeyProvider {
    async fn get_key(&self, key_id: &str) -> Result<Vec<u8>, AppError> {
        if !self.config.enabled {
            return Err(AppError::new(
                ErrorCode::CryptoKeyNotFound,
                "file key provider not enabled",
            ));
        }
        if self.config.path.is_empty() {
            return Err(AppError::new(
                ErrorCode::CryptoKeyNotFound,
                "key file path not configured",
            ));
        }
        info!(
            name = %key_id,
            path = %self.config.path,
            "file key lookup (stub)"
        );
        // Stub: would read and decrypt key from file
        Err(AppError::new(
            ErrorCode::CryptoKeyNotFound,
            format!("file key provider stub: key '{key_id}' not implemented"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_disabled() {
        let provider = FileKeyProvider::new(&FileKeyConfig {
            enabled: false,
            path: String::new(),
        });
        let result = provider.get_key("test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_empty_path() {
        let provider = FileKeyProvider::new(&FileKeyConfig {
            enabled: true,
            path: String::new(),
        });
        let result = provider.get_key("test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_stub_returns_error() {
        let provider = FileKeyProvider::new(&FileKeyConfig {
            enabled: true,
            path: "/etc/keys.enc".to_string(),
        });
        let result = provider.get_key("master").await;
        assert!(result.is_err());
    }
}
