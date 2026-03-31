use async_trait::async_trait;
use redis::AsyncCommands;
use rinfra_core::config::RedisRateLimitConfig;
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::ratelimit::{RateLimiter, RateLimitResult};
use tokio::sync::RwLock;
use tracing::info;

/// Distributed rate limiter backed by Redis.
/// Uses a fixed-window counter per key with INCR + EXPIRE.
pub struct RedisRateLimiter {
    config: RedisRateLimitConfig,
    conn: RwLock<Option<redis::aio::ConnectionManager>>,
}

impl RedisRateLimiter {
    pub fn new(config: &RedisRateLimitConfig) -> Self {
        Self {
            config: config.clone(),
            conn: RwLock::new(None),
        }
    }

    pub async fn connect(&self) -> Result<(), AppError> {
        if !self.config.enabled {
            info!("redis rate limiter disabled, skipping connect");
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
        info!(url = %self.config.url, "redis rate limiter connected");
        Ok(())
    }

    pub async fn close(&self) {
        *self.conn.write().await = None;
        info!("redis rate limiter connection closed");
    }

    async fn get_conn(&self) -> Result<redis::aio::ConnectionManager, AppError> {
        self.conn
            .read()
            .await
            .clone()
            .ok_or_else(|| AppError::new(ErrorCode::CacheConnectionFailed, "redis not connected"))
    }

    fn rate_key(&self, key: &str) -> String {
        let window = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            / self.config.window_secs.max(1);
        format!("rinfra:rl:{}:{}", key, window)
    }
}

#[async_trait]
impl RateLimiter for RedisRateLimiter {
    async fn check(&self, key: &str) -> Result<RateLimitResult, AppError> {
        if !self.config.enabled {
            return Err(AppError::new(
                ErrorCode::RateLimitExceeded,
                "redis rate limiter not enabled",
            ));
        }

        let mut conn = self.get_conn().await?;
        let rk = self.rate_key(key);
        let count: u64 = conn.incr(&rk, 1u64).await.map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("redis INCR failed: {e}"),
            )
        })?;

        if count == 1 {
            let ttl = (self.config.window_secs.max(1) + 1) as i64;
            let _: () = conn.expire(&rk, ttl).await.unwrap_or(());
        }

        let limit = self.config.requests_per_second * self.config.window_secs.max(1);
        if count <= limit {
            Ok(RateLimitResult {
                allowed: true,
                remaining: limit.saturating_sub(count),
                retry_after_ms: None,
            })
        } else {
            Ok(RateLimitResult {
                allowed: false,
                remaining: 0,
                retry_after_ms: Some(self.config.window_secs.max(1) * 1000),
            })
        }
    }

    async fn reset(&self, key: &str) -> Result<(), AppError> {
        if !self.config.enabled {
            return Err(AppError::new(
                ErrorCode::RateLimitExceeded,
                "redis rate limiter not enabled",
            ));
        }
        let mut conn = self.get_conn().await?;
        let rk = self.rate_key(key);
        conn.del(&rk).await.map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("redis DEL failed: {e}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disabled_config() -> RedisRateLimitConfig {
        RedisRateLimitConfig::default()
    }

    fn enabled_config() -> RedisRateLimitConfig {
        RedisRateLimitConfig {
            enabled: true,
            url: "redis://127.0.0.1:6379".to_string(),
            requests_per_second: 100,
            burst_size: 200,
            window_secs: 1,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_redis_limiter_disabled() {
        let limiter = RedisRateLimiter::new(&disabled_config());
        let result = limiter.check("key").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_redis_limiter_not_connected() {
        let limiter = RedisRateLimiter::new(&enabled_config());
        let result = limiter.check("key").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::CacheConnectionFailed);
    }

    #[tokio::test]
    async fn test_redis_limiter_reset_disabled() {
        let limiter = RedisRateLimiter::new(&disabled_config());
        let result = limiter.reset("key").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_redis_limiter_connect_disabled() {
        let limiter = RedisRateLimiter::new(&disabled_config());
        assert!(limiter.connect().await.is_ok());
    }

    #[tokio::test]
    #[ignore = "requires a running Redis instance at localhost:6379"]
    async fn test_redis_limiter_check_and_reset() {
        let limiter = RedisRateLimiter::new(&enabled_config());
        limiter.connect().await.unwrap();

        let result = limiter.check("test:rl:key1").await.unwrap();
        assert!(result.allowed);

        limiter.reset("test:rl:key1").await.unwrap();
        limiter.close().await;
    }
}
