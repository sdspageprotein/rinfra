use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::lock::{DistributedLock, LockHandle};
use tokio::sync::{Mutex, Notify};
use uuid::Uuid;

struct LockEntry {
    token: String,
    expires_at: Instant,
}

/// Single-node in-memory distributed lock implementation.
///
/// **Important**: this lock only works within a single process. It does NOT
/// provide cross-node coordination in cluster (master/worker) mode.
///
/// Suitable for:
/// - `standalone` deployments
/// - Development and testing
/// - Intra-process concurrency control (e.g. preventing duplicate timer jobs)
///
/// For distributed locking across multiple nodes, use `RedisLock` instead.
pub struct InMemoryLock {
    locks: Mutex<HashMap<String, LockEntry>>,
    notify: Arc<Notify>,
}

impl InMemoryLock {
    pub fn new() -> Self {
        Self {
            locks: Mutex::new(HashMap::new()),
            notify: Arc::new(Notify::new()),
        }
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

impl Default for InMemoryLock {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DistributedLock for InMemoryLock {
    fn lock_name(&self) -> &str {
        "memory"
    }

    async fn try_acquire(&self, key: &str, ttl_secs: u64) -> Result<Option<LockHandle>, AppError> {
        let mut locks = self.locks.lock().await;
        let now = Instant::now();

        if let Some(entry) = locks.get(key) {
            if now >= entry.expires_at {
                locks.remove(key);
            }
        }

        if locks.contains_key(key) {
            metrics::counter!("lock_contention_total", "backend" => "memory").increment(1);
            return Ok(None);
        }

        let token = Uuid::new_v4().to_string();
        locks.insert(
            key.to_string(),
            LockEntry {
                token: token.clone(),
                expires_at: now + Duration::from_secs(ttl_secs),
            },
        );

        metrics::counter!("lock_acquisitions_total", "backend" => "memory").increment(1);
        Ok(Some(LockHandle {
            key: key.to_string(),
            token,
            acquired_at_ms: Self::now_ms(),
        }))
    }

    async fn acquire(
        &self,
        key: &str,
        ttl_secs: u64,
        wait_timeout_secs: u64,
    ) -> Result<LockHandle, AppError> {
        let deadline = Instant::now() + Duration::from_secs(wait_timeout_secs);

        loop {
            if let Some(handle) = self.try_acquire(key, ttl_secs).await? {
                return Ok(handle);
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(AppError::new(
                    ErrorCode::LockAcquireFailed,
                    format!("timed out waiting for lock '{key}'"),
                ));
            }

            // Wait for a release notification or timeout.
            tokio::select! {
                _ = self.notify.notified() => {}
                _ = tokio::time::sleep(remaining) => {}
            }
        }
    }

    async fn release(&self, handle: &LockHandle) -> Result<bool, AppError> {
        let mut locks = self.locks.lock().await;
        if let Some(entry) = locks.get(&handle.key) {
            if entry.token == handle.token {
                locks.remove(&handle.key);
                self.notify.notify_waiters();
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn extend(&self, handle: &LockHandle, ttl_secs: u64) -> Result<bool, AppError> {
        let mut locks = self.locks.lock().await;
        if let Some(entry) = locks.get_mut(&handle.key) {
            if entry.token == handle.token {
                entry.expires_at = Instant::now() + Duration::from_secs(ttl_secs);
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_try_acquire_and_release() {
        let lock = InMemoryLock::new();
        let h = lock.try_acquire("key1", 10).await.unwrap().unwrap();
        assert_eq!(h.key, "key1");

        // Second acquire should fail.
        assert!(lock.try_acquire("key1", 10).await.unwrap().is_none());

        // Release and re-acquire.
        assert!(lock.release(&h).await.unwrap());
        assert!(lock.try_acquire("key1", 10).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_different_keys() {
        let lock = InMemoryLock::new();
        let _h1 = lock.try_acquire("a", 10).await.unwrap().unwrap();
        let _h2 = lock.try_acquire("b", 10).await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_expired_lock_reacquirable() {
        let lock = InMemoryLock::new();
        let _h = lock.try_acquire("exp", 0).await.unwrap().unwrap();
        // TTL=0 means it expires immediately.
        tokio::time::sleep(Duration::from_millis(10)).await;
        let h2 = lock.try_acquire("exp", 10).await.unwrap();
        assert!(h2.is_some());
    }

    #[tokio::test]
    async fn test_acquire_waits_for_release() {
        let lock = Arc::new(InMemoryLock::new());
        let h = lock.try_acquire("wait", 10).await.unwrap().unwrap();

        let lock2 = lock.clone();
        let join = tokio::spawn(async move {
            lock2.acquire("wait", 10, 5).await.unwrap()
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        lock.release(&h).await.unwrap();

        let h2 = join.await.unwrap();
        assert_eq!(h2.key, "wait");
    }

    #[tokio::test]
    async fn test_acquire_timeout() {
        let lock = InMemoryLock::new();
        let _h = lock.try_acquire("block", 60).await.unwrap().unwrap();
        let err = lock.acquire("block", 10, 0).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::LockAcquireFailed);
    }

    #[tokio::test]
    async fn test_release_wrong_token() {
        let lock = InMemoryLock::new();
        let _h = lock.try_acquire("owned", 10).await.unwrap().unwrap();
        let fake = LockHandle {
            key: "owned".into(),
            token: "wrong-token".into(),
            acquired_at_ms: 0,
        };
        assert!(!lock.release(&fake).await.unwrap());
    }

    #[tokio::test]
    async fn test_extend() {
        let lock = InMemoryLock::new();
        let h = lock.try_acquire("ext", 1).await.unwrap().unwrap();
        assert!(lock.extend(&h, 60).await.unwrap());

        let fake = LockHandle {
            key: "ext".into(),
            token: "wrong".into(),
            acquired_at_ms: 0,
        };
        assert!(!lock.extend(&fake, 60).await.unwrap());
    }

    #[test]
    fn test_lock_name() {
        let lock = InMemoryLock::new();
        assert_eq!(lock.lock_name(), "memory");
    }
}
