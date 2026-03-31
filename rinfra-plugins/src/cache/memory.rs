use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rinfra_core::cache::Cache;
use rinfra_core::config::MemoryCacheConfig;

pub struct MemoryCache {
    inner: Arc<moka::future::Cache<String, Vec<u8>>>,
}

impl MemoryCache {
    pub fn new(config: &MemoryCacheConfig) -> Self {
        let cache = moka::future::Cache::builder()
            .max_capacity(config.max_capacity)
            .time_to_live(Duration::from_secs(config.ttl_secs))
            .build();
        Self {
            inner: Arc::new(cache),
        }
    }
}

#[async_trait]
impl Cache for MemoryCache {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, rinfra_core::error::AppError> {
        let result = self.inner.get(key).await;
        let labels = [("backend", "memory")];
        if result.is_some() {
            metrics::counter!("cache_hits_total", &labels).increment(1);
        } else {
            metrics::counter!("cache_misses_total", &labels).increment(1);
        }
        Ok(result)
    }

    async fn set(&self, key: &str, value: Vec<u8>) -> Result<(), rinfra_core::error::AppError> {
        self.inner.insert(key.to_string(), value).await;
        Ok(())
    }

    async fn set_with_ttl(
        &self,
        key: &str,
        value: Vec<u8>,
        _ttl: Duration,
    ) -> Result<(), rinfra_core::error::AppError> {
        // moka uses cache-level TTL; per-entry TTL would require a different approach.
        // For now, insert with the cache's default TTL.
        self.inner.insert(key.to_string(), value).await;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), rinfra_core::error::AppError> {
        self.inner.invalidate(key).await;
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, rinfra_core::error::AppError> {
        Ok(self.inner.get(key).await.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> MemoryCacheConfig {
        MemoryCacheConfig {
            enabled: true,
            max_capacity: 100,
            ttl_secs: 60,
        }
    }

    #[tokio::test]
    async fn test_memory_cache_set_and_get() {
        let cache = MemoryCache::new(&test_config());
        cache.set("key1", b"value1".to_vec()).await.unwrap();
        let result = cache.get("key1").await.unwrap();
        assert_eq!(result, Some(b"value1".to_vec()));
    }

    #[tokio::test]
    async fn test_memory_cache_get_miss() {
        let cache = MemoryCache::new(&test_config());
        let result = cache.get("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_memory_cache_delete() {
        let cache = MemoryCache::new(&test_config());
        cache.set("key1", b"value1".to_vec()).await.unwrap();
        cache.delete("key1").await.unwrap();
        let result = cache.get("key1").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_memory_cache_exists() {
        let cache = MemoryCache::new(&test_config());
        assert!(!cache.exists("key1").await.unwrap());
        cache.set("key1", b"value1".to_vec()).await.unwrap();
        assert!(cache.exists("key1").await.unwrap());
    }

    #[tokio::test]
    async fn test_memory_cache_overwrite() {
        let cache = MemoryCache::new(&test_config());
        cache.set("key1", b"v1".to_vec()).await.unwrap();
        cache.set("key1", b"v2".to_vec()).await.unwrap();
        let result = cache.get("key1").await.unwrap();
        assert_eq!(result, Some(b"v2".to_vec()));
    }
}
