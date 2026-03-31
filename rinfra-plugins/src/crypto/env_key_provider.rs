use async_trait::async_trait;
use rinfra_core::crypto::KeyProvider;
use rinfra_core::error::{AppError, ErrorCode};

pub struct EnvKeyProvider;

#[async_trait]
impl KeyProvider for EnvKeyProvider {
    async fn get_key(&self, key_id: &str) -> Result<Vec<u8>, AppError> {
        std::env::var(key_id)
            .map(|v| v.into_bytes())
            .map_err(|_| AppError::new(ErrorCode::CryptoKeyNotFound, format!("env var '{key_id}' not set")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_env_key_provider_missing_key() {
        let provider = EnvKeyProvider;
        let result = provider.get_key("RINFRA_TEST_NONEXISTENT_KEY_12345").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::CryptoKeyNotFound);
    }
}
