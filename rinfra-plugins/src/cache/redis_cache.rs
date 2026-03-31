use std::time::Duration;

use async_trait::async_trait;
use redis::AsyncCommands;
use rinfra_core::cache::Cache;
use rinfra_core::config::RedisCacheConfig;
use rinfra_core::error::{AppError, ErrorCode};
use tokio::sync::RwLock;
use tracing::info;

pub struct RedisCache {
    config: RedisCacheConfig,
    conn: RwLock<Option<redis::aio::ConnectionManager>>,
}

impl RedisCache {
    pub fn new(config: &RedisCacheConfig) -> Self {
        Self {
            config: config.clone(),
            conn: RwLock::new(None),
        }
    }

    pub async fn connect(&self) -> Result<(), AppError> {
        if !self.config.enabled {
            info!("redis cache disabled, skipping connect");
            return Ok(());
        }
        let client = redis::Client::open(self.config.url.as_str()).map_err(|e| {
            AppError::new(
                ErrorCode::CacheConnectionFailed,
                format!("invalid redis url: {e}"),
            )
        })?;
        let mgr = redis::aio::ConnectionManager::new(client).await.map_err(|e| {
            AppError::new(
                ErrorCode::CacheConnectionFailed,
                format!("redis connect failed: {e}"),
            )
        })?;
        *self.conn.write().await = Some(mgr);
        info!(url = %self.config.url, "redis cache connected");
        Ok(())
    }

    pub async fn close(&self) {
        *self.conn.write().await = None;
        info!("redis cache connection closed");
    }

    async fn get_conn(&self) -> Result<redis::aio::ConnectionManager, AppError> {
        self.conn
            .read()
            .await
            .clone()
            .ok_or_else(|| AppError::new(ErrorCode::CacheConnectionFailed, "redis not connected"))
    }
}

#[async_trait]
impl Cache for RedisCache {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, AppError> {
        let mut conn = self.get_conn().await?;
        let result: Option<Vec<u8>> = conn.get(key).await.map_err(|e| {
            AppError::new(ErrorCode::CacheGetFailed, format!("redis GET failed: {e}"))
        })?;
        let labels = [("backend", "redis")];
        if result.is_some() {
            metrics::counter!("cache_hits_total", &labels).increment(1);
        } else {
            metrics::counter!("cache_misses_total", &labels).increment(1);
        }
        Ok(result)
    }

    async fn set(&self, key: &str, value: Vec<u8>) -> Result<(), AppError> {
        let mut conn = self.get_conn().await?;
        conn.set(key, value).await.map_err(|e| {
            AppError::new(ErrorCode::CacheSetFailed, format!("redis SET failed: {e}"))
        })
    }

    async fn set_with_ttl(
        &self,
        key: &str,
        value: Vec<u8>,
        ttl: Duration,
    ) -> Result<(), AppError> {
        let mut conn = self.get_conn().await?;
        let ttl_secs = ttl.as_secs().max(1);
        conn.set_ex(key, value, ttl_secs).await.map_err(|e| {
            AppError::new(
                ErrorCode::CacheSetFailed,
                format!("redis SETEX failed: {e}"),
            )
        })
    }

    async fn delete(&self, key: &str) -> Result<(), AppError> {
        let mut conn = self.get_conn().await?;
        conn.del(key).await.map_err(|e| {
            AppError::new(
                ErrorCode::CacheDeleteFailed,
                format!("redis DEL failed: {e}"),
            )
        })
    }

    async fn exists(&self, key: &str) -> Result<bool, AppError> {
        let mut conn = self.get_conn().await?;
        let result: bool = conn.exists(key).await.map_err(|e| {
            AppError::new(
                ErrorCode::CacheGetFailed,
                format!("redis EXISTS failed: {e}"),
            )
        })?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_redis_cache_disabled_returns_ok_on_connect() {
        let config = RedisCacheConfig::default();
        let cache = RedisCache::new(&config);
        assert!(cache.connect().await.is_ok());
    }

    #[tokio::test]
    async fn test_redis_cache_not_connected_returns_error() {
        let mut config = RedisCacheConfig::default();
        config.enabled = true;
        let cache = RedisCache::new(&config);
        let result = cache.get("key").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::CacheConnectionFailed);
    }

    #[tokio::test]
    async fn test_redis_cache_close_when_not_connected() {
        let config = RedisCacheConfig::default();
        let cache = RedisCache::new(&config);
        cache.close().await;
    }

    #[tokio::test]
    #[ignore = "requires a running Redis instance at localhost:6379"]
    async fn test_redis_cache_set_get_roundtrip() {
        let config = RedisCacheConfig {
            enabled: true,
            url: "redis://127.0.0.1:6379".to_string(),
            ..Default::default()
        };
        let cache = RedisCache::new(&config);
        cache.connect().await.unwrap();

        cache.set("test:key1", b"hello".to_vec()).await.unwrap();
        let val = cache.get("test:key1").await.unwrap();
        assert_eq!(val, Some(b"hello".to_vec()));

        cache.delete("test:key1").await.unwrap();
        let val = cache.get("test:key1").await.unwrap();
        assert_eq!(val, None);

        cache.close().await;
    }

    #[tokio::test]
    #[ignore = "requires a running Redis instance at localhost:6379"]
    async fn test_redis_cache_exists() {
        let config = RedisCacheConfig {
            enabled: true,
            url: "redis://127.0.0.1:6379".to_string(),
            ..Default::default()
        };
        let cache = RedisCache::new(&config);
        cache.connect().await.unwrap();

        cache.delete("test:exists_key").await.ok();
        assert!(!cache.exists("test:exists_key").await.unwrap());

        cache
            .set("test:exists_key", b"val".to_vec())
            .await
            .unwrap();
        assert!(cache.exists("test:exists_key").await.unwrap());

        cache.delete("test:exists_key").await.unwrap();
        cache.close().await;
    }

    #[tokio::test]
    #[ignore = "requires a running Redis instance at localhost:6379"]
    async fn test_redis_cache_set_with_ttl() {
        let config = RedisCacheConfig {
            enabled: true,
            url: "redis://127.0.0.1:6379".to_string(),
            ..Default::default()
        };
        let cache = RedisCache::new(&config);
        cache.connect().await.unwrap();

        cache
            .set_with_ttl("test:ttl_key", b"temp".to_vec(), Duration::from_secs(10))
            .await
            .unwrap();
        let val = cache.get("test:ttl_key").await.unwrap();
        assert_eq!(val, Some(b"temp".to_vec()));

        cache.delete("test:ttl_key").await.unwrap();
        cache.close().await;
    }
}
