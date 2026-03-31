use async_trait::async_trait;
use serde::Serialize;

use crate::error::AppError;

/// Result of a rate limit check.
#[derive(Debug, Clone, Serialize)]
pub struct RateLimitResult {
    pub allowed: bool,
    pub remaining: u64,
    pub retry_after_ms: Option<u64>,
}

/// Rate limiter abstraction.
#[async_trait]
pub trait RateLimiter: Send + Sync + 'static {
    async fn check(&self, key: &str) -> Result<RateLimitResult, AppError>;
    async fn reset(&self, key: &str) -> Result<(), AppError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_result_allowed() {
        let r = RateLimitResult {
            allowed: true,
            remaining: 99,
            retry_after_ms: None,
        };
        assert!(r.allowed);
        assert_eq!(r.remaining, 99);
    }

    #[test]
    fn test_rate_limit_result_denied() {
        let r = RateLimitResult {
            allowed: false,
            remaining: 0,
            retry_after_ms: Some(1000),
        };
        assert!(!r.allowed);
        assert_eq!(r.retry_after_ms, Some(1000));
    }
}
