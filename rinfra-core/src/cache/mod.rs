use std::time::Duration;

use async_trait::async_trait;

use crate::error::AppError;

/// Unified cache abstraction supporting multiple backends.
#[async_trait]
pub trait Cache: Send + Sync + 'static {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, AppError>;
    async fn set(&self, key: &str, value: Vec<u8>) -> Result<(), AppError>;
    async fn set_with_ttl(&self, key: &str, value: Vec<u8>, ttl: Duration) -> Result<(), AppError>;
    async fn delete(&self, key: &str) -> Result<(), AppError>;
    async fn exists(&self, key: &str) -> Result<bool, AppError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubCache;

    #[async_trait]
    impl Cache for StubCache {
        async fn get(&self, _key: &str) -> Result<Option<Vec<u8>>, AppError> {
            Ok(None)
        }
        async fn set(&self, _key: &str, _value: Vec<u8>) -> Result<(), AppError> {
            Ok(())
        }
        async fn set_with_ttl(
            &self,
            _key: &str,
            _value: Vec<u8>,
            _ttl: Duration,
        ) -> Result<(), AppError> {
            Ok(())
        }
        async fn delete(&self, _key: &str) -> Result<(), AppError> {
            Ok(())
        }
        async fn exists(&self, _key: &str) -> Result<bool, AppError> {
            Ok(false)
        }
    }

    #[tokio::test]
    async fn test_stub_cache_get_returns_none() {
        let cache = StubCache;
        assert!(cache.get("missing").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_stub_cache_exists_returns_false() {
        let cache = StubCache;
        assert!(!cache.exists("missing").await.unwrap());
    }
}
