use std::time::Duration;

use async_trait::async_trait;
use rinfra_core::config::RedisLockConfig;
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::lock::{DistributedLock, LockHandle};
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

/// Redis-based distributed lock using SET NX EX + Lua release.
///
/// Uses the single-instance Redis locking pattern recommended by Redis
/// documentation. For multi-master Redis setups, consider the full
/// Redlock algorithm with multiple independent Redis nodes.
///
/// Key format: `{prefix}{key}` (default prefix: `rinfra:lock:`)
/// Value: a unique token (UUID) to prove ownership on release/extend.
pub struct RedisLock {
    config: RedisLockConfig,
    conn: RwLock<Option<redis::aio::ConnectionManager>>,
}

const RELEASE_SCRIPT: &str = r#"
if redis.call("GET", KEYS[1]) == ARGV[1] then
    redis.call("DEL", KEYS[1])
    return 1
else
    return 0
end
"#;

const EXTEND_SCRIPT: &str = r#"
if redis.call("GET", KEYS[1]) == ARGV[1] then
    redis.call("PEXPIRE", KEYS[1], ARGV[2])
    return 1
else
    return 0
end
"#;

impl RedisLock {
    pub fn new(config: &RedisLockConfig) -> Self {
        Self {
            config: config.clone(),
            conn: RwLock::new(None),
        }
    }

    pub async fn connect(&self) -> Result<(), AppError> {
        let client = redis::Client::open(self.config.url.as_str()).map_err(|e| {
            AppError::new(
                ErrorCode::LockAcquireFailed,
                format!("invalid redis url: {e}"),
            )
        })?;
        let mgr = redis::aio::ConnectionManager::new(client)
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::LockAcquireFailed,
                    format!("redis connect failed: {e}"),
                )
            })?;
        *self.conn.write().await = Some(mgr);
        info!(url = %self.config.url, "redis lock connected");
        Ok(())
    }

    async fn get_conn(&self) -> Result<redis::aio::ConnectionManager, AppError> {
        self.conn
            .read()
            .await
            .clone()
            .ok_or_else(|| AppError::new(ErrorCode::LockAcquireFailed, "redis not connected"))
    }

    fn full_key(&self, key: &str) -> String {
        format!("{}{}", self.config.key_prefix, key)
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

#[async_trait]
impl DistributedLock for RedisLock {
    fn lock_name(&self) -> &str {
        "redis"
    }

    async fn try_acquire(&self, key: &str, ttl_secs: u64) -> Result<Option<LockHandle>, AppError> {
        let mut conn = self.get_conn().await?;
        let full_key = self.full_key(key);
        let token = Uuid::new_v4().to_string();

        // SET key token NX PX milliseconds
        let result: Option<String> = redis::cmd("SET")
            .arg(&full_key)
            .arg(&token)
            .arg("NX")
            .arg("PX")
            .arg(ttl_secs * 1000)
            .query_async(&mut conn)
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::LockAcquireFailed,
                    format!("redis SET NX failed: {e}"),
                )
            })?;

        if result.is_some() {
            metrics::counter!("lock_acquisitions_total", "backend" => "redis").increment(1);
            Ok(Some(LockHandle {
                key: key.to_string(),
                token,
                acquired_at_ms: Self::now_ms(),
            }))
        } else {
            metrics::counter!("lock_contention_total", "backend" => "redis").increment(1);
            Ok(None)
        }
    }

    async fn acquire(
        &self,
        key: &str,
        ttl_secs: u64,
        wait_timeout_secs: u64,
    ) -> Result<LockHandle, AppError> {
        let deadline =
            tokio::time::Instant::now() + Duration::from_secs(wait_timeout_secs);
        let poll_interval = Duration::from_millis(50).min(Duration::from_secs(wait_timeout_secs));

        loop {
            if let Some(handle) = self.try_acquire(key, ttl_secs).await? {
                return Ok(handle);
            }

            if tokio::time::Instant::now() >= deadline {
                return Err(AppError::new(
                    ErrorCode::LockAcquireFailed,
                    format!("timed out waiting for lock '{key}'"),
                ));
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    async fn release(&self, handle: &LockHandle) -> Result<bool, AppError> {
        let mut conn = self.get_conn().await?;
        let full_key = self.full_key(&handle.key);

        let result: i32 = redis::Script::new(RELEASE_SCRIPT)
            .key(&full_key)
            .arg(&handle.token)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::LockAcquireFailed,
                    format!("redis release script failed: {e}"),
                )
            })?;

        Ok(result == 1)
    }

    async fn extend(&self, handle: &LockHandle, ttl_secs: u64) -> Result<bool, AppError> {
        let mut conn = self.get_conn().await?;
        let full_key = self.full_key(&handle.key);

        let result: i32 = redis::Script::new(EXTEND_SCRIPT)
            .key(&full_key)
            .arg(&handle.token)
            .arg(ttl_secs * 1000)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::LockAcquireFailed,
                    format!("redis extend script failed: {e}"),
                )
            })?;

        Ok(result == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_name() {
        let config = RedisLockConfig::default();
        let lock = RedisLock::new(&config);
        assert_eq!(lock.lock_name(), "redis");
    }

    #[test]
    fn test_full_key() {
        let config = RedisLockConfig {
            url: "redis://localhost".into(),
            key_prefix: "myapp:lock:".into(),
        };
        let lock = RedisLock::new(&config);
        assert_eq!(lock.full_key("order-123"), "myapp:lock:order-123");
    }

    #[test]
    fn test_full_key_default_prefix() {
        let config = RedisLockConfig::default();
        let lock = RedisLock::new(&config);
        assert_eq!(lock.full_key("task"), "rinfra:lock:task");
    }

    /// Requires a running Redis instance.
    #[tokio::test]
    #[ignore]
    async fn test_redis_lock_acquire_release() {
        let config = RedisLockConfig::default();
        let lock = RedisLock::new(&config);
        lock.connect().await.unwrap();

        let handle = lock.try_acquire("test-key", 10).await.unwrap().unwrap();
        assert_eq!(handle.key, "test-key");

        // Second acquire should fail.
        assert!(lock.try_acquire("test-key", 10).await.unwrap().is_none());

        // Release and re-acquire.
        assert!(lock.release(&handle).await.unwrap());
        assert!(lock.try_acquire("test-key", 10).await.unwrap().is_some());
    }

    /// Requires a running Redis instance.
    #[tokio::test]
    #[ignore]
    async fn test_redis_lock_extend() {
        let config = RedisLockConfig::default();
        let lock = RedisLock::new(&config);
        lock.connect().await.unwrap();

        let handle = lock.try_acquire("extend-test", 2).await.unwrap().unwrap();
        assert!(lock.extend(&handle, 60).await.unwrap());
        assert!(lock.release(&handle).await.unwrap());
    }

    /// Requires a running Redis instance.
    #[tokio::test]
    #[ignore]
    async fn test_redis_lock_wrong_token() {
        let config = RedisLockConfig::default();
        let lock = RedisLock::new(&config);
        lock.connect().await.unwrap();

        let _handle = lock.try_acquire("token-test", 10).await.unwrap().unwrap();
        let fake = LockHandle {
            key: "token-test".into(),
            token: "wrong".into(),
            acquired_at_ms: 0,
        };
        assert!(!lock.release(&fake).await.unwrap());

        // Cleanup
        lock.release(&_handle).await.unwrap();
    }
}
