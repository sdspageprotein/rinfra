use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rinfra_core::cache::Cache;
use rinfra_core::error::AppError;

/// Multi-level cache: L1 (fast, in-memory) + L2 (shared, e.g. Redis).
/// On get: L1 hit → return; L1 miss → query L2 → backfill L1 on hit.
/// On set: write both L1 and L2.
/// On delete: remove from both L1 and L2.
pub struct MultilevelCache {
    l1: Arc<dyn Cache>,
    l2: Arc<dyn Cache>,
}

impl MultilevelCache {
    pub fn new(l1: Arc<dyn Cache>, l2: Arc<dyn Cache>) -> Self {
        Self { l1, l2 }
    }
}

#[async_trait]
impl Cache for MultilevelCache {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, AppError> {
        if let Some(val) = self.l1.get(key).await? {
            return Ok(Some(val));
        }
        if let Some(val) = self.l2.get(key).await? {
            // Backfill L1
            let _ = self.l1.set(key, val.clone()).await;
            return Ok(Some(val));
        }
        Ok(None)
    }

    async fn set(&self, key: &str, value: Vec<u8>) -> Result<(), AppError> {
        self.l1.set(key, value.clone()).await?;
        self.l2.set(key, value).await?;
        Ok(())
    }

    async fn set_with_ttl(
        &self,
        key: &str,
        value: Vec<u8>,
        ttl: Duration,
    ) -> Result<(), AppError> {
        self.l1.set_with_ttl(key, value.clone(), ttl).await?;
        self.l2.set_with_ttl(key, value, ttl).await?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), AppError> {
        self.l1.delete(key).await?;
        self.l2.delete(key).await?;
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, AppError> {
        if self.l1.exists(key).await? {
            return Ok(true);
        }
        self.l2.exists(key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinfra_core::config::MemoryCacheConfig;

    use crate::cache::MemoryCache;

    fn make_l1() -> Arc<dyn Cache> {
        let config = MemoryCacheConfig {
            enabled: true,
            max_capacity: 100,
            ttl_secs: 60,
        };
        Arc::new(MemoryCache::new(&config))
    }

    // Use a second MemoryCache as a stand-in for L2 (since RedisCache is a stub)
    fn make_l2() -> Arc<dyn Cache> {
        let config = MemoryCacheConfig {
            enabled: true,
            max_capacity: 1000,
            ttl_secs: 300,
        };
        Arc::new(MemoryCache::new(&config))
    }

    #[tokio::test]
    async fn test_set_writes_both_levels() {
        let l1 = make_l1();
        let l2 = make_l2();
        let ml = MultilevelCache::new(l1.clone(), l2.clone());
        ml.set("k", b"v".to_vec()).await.unwrap();
        assert_eq!(l1.get("k").await.unwrap(), Some(b"v".to_vec()));
        assert_eq!(l2.get("k").await.unwrap(), Some(b"v".to_vec()));
    }

    #[tokio::test]
    async fn test_get_l1_hit() {
        let l1 = make_l1();
        let l2 = make_l2();
        l1.set("k", b"v1".to_vec()).await.unwrap();
        let ml = MultilevelCache::new(l1, l2);
        let val = ml.get("k").await.unwrap();
        assert_eq!(val, Some(b"v1".to_vec()));
    }

    #[tokio::test]
    async fn test_get_l1_miss_l2_hit_backfills_l1() {
        let l1 = make_l1();
        let l2 = make_l2();
        l2.set("k", b"v2".to_vec()).await.unwrap();
        let ml = MultilevelCache::new(l1.clone(), l2);
        let val = ml.get("k").await.unwrap();
        assert_eq!(val, Some(b"v2".to_vec()));
        // L1 should now have it
        assert_eq!(l1.get("k").await.unwrap(), Some(b"v2".to_vec()));
    }

    #[tokio::test]
    async fn test_get_miss_both() {
        let ml = MultilevelCache::new(make_l1(), make_l2());
        let val = ml.get("nope").await.unwrap();
        assert!(val.is_none());
    }

    #[tokio::test]
    async fn test_delete_from_both() {
        let l1 = make_l1();
        let l2 = make_l2();
        let ml = MultilevelCache::new(l1.clone(), l2.clone());
        ml.set("k", b"v".to_vec()).await.unwrap();
        ml.delete("k").await.unwrap();
        assert!(l1.get("k").await.unwrap().is_none());
        assert!(l2.get("k").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_exists() {
        let l1 = make_l1();
        let l2 = make_l2();
        l2.set("k", b"v".to_vec()).await.unwrap();
        let ml = MultilevelCache::new(l1, l2);
        assert!(ml.exists("k").await.unwrap());
        assert!(!ml.exists("nope").await.unwrap());
    }
}
