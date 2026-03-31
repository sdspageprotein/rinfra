use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// A handle representing an acquired distributed lock.
///
/// The `token` field is a unique proof-of-ownership value that must be
/// presented when releasing or extending the lock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockHandle {
    pub key: String,
    pub token: String,
    pub acquired_at_ms: u64,
}

/// Pluggable distributed lock abstraction.
///
/// Implementations can range from a single-node in-memory lock (for dev/test)
/// to Redis/etcd-based locks for production clusters.
#[async_trait]
pub trait DistributedLock: Send + Sync + 'static {
    /// Backend name (e.g. `"memory"`, `"redis"`).
    fn lock_name(&self) -> &str;

    /// Try to acquire a lock without blocking.
    /// Returns `Some(handle)` on success, `None` if already held.
    async fn try_acquire(&self, key: &str, ttl_secs: u64) -> Result<Option<LockHandle>, AppError>;

    /// Acquire a lock, blocking up to `wait_timeout_secs`.
    async fn acquire(
        &self,
        key: &str,
        ttl_secs: u64,
        wait_timeout_secs: u64,
    ) -> Result<LockHandle, AppError>;

    /// Release a lock. Returns `true` if the lock was actually held and released.
    async fn release(&self, handle: &LockHandle) -> Result<bool, AppError>;

    /// Extend the TTL of a held lock. Returns `true` if still owned and extended.
    async fn extend(&self, handle: &LockHandle, ttl_secs: u64) -> Result<bool, AppError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_handle_serde() {
        let handle = LockHandle {
            key: "my-lock".into(),
            token: "abc-123".into(),
            acquired_at_ms: 1700000000000,
        };
        let json = serde_json::to_string(&handle).unwrap();
        let decoded: LockHandle = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.key, "my-lock");
        assert_eq!(decoded.token, "abc-123");
    }
}
