#[cfg(feature = "redis")]
mod redis_limiter;
#[cfg(feature = "redis")]
pub use redis_limiter::RedisRateLimiter;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use rinfra_core::config::MemoryRateLimitConfig;
use rinfra_core::error::AppError;
use rinfra_core::ratelimit::{RateLimiter, RateLimitResult};
use tokio::sync::Mutex;

struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
    max_tokens: f64,
    refill_rate: f64,
}

impl TokenBucket {
    fn new(max_tokens: u64, refill_rate: u64) -> Self {
        Self {
            tokens: max_tokens as f64,
            last_refill: Instant::now(),
            max_tokens: max_tokens as f64,
            refill_rate: refill_rate as f64,
        }
    }

    fn try_consume(&mut self) -> RateLimitResult {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_refill = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            RateLimitResult {
                allowed: true,
                remaining: self.tokens as u64,
                retry_after_ms: None,
            }
        } else {
            let wait = (1.0 - self.tokens) / self.refill_rate;
            RateLimitResult {
                allowed: false,
                remaining: 0,
                retry_after_ms: Some((wait * 1000.0) as u64),
            }
        }
    }
}

pub struct MemoryRateLimiter {
    buckets: Arc<Mutex<HashMap<String, TokenBucket>>>,
    burst_size: u64,
    rps: u64,
}

impl MemoryRateLimiter {
    pub fn new(config: &MemoryRateLimitConfig) -> Self {
        Self {
            buckets: Arc::new(Mutex::new(HashMap::new())),
            burst_size: config.burst_size,
            rps: config.requests_per_second,
        }
    }
}

#[async_trait]
impl RateLimiter for MemoryRateLimiter {
    async fn check(&self, key: &str) -> Result<RateLimitResult, AppError> {
        let mut buckets = self.buckets.lock().await;
        let bucket = buckets
            .entry(key.to_string())
            .or_insert_with(|| TokenBucket::new(self.burst_size, self.rps));
        Ok(bucket.try_consume())
    }

    async fn reset(&self, key: &str) -> Result<(), AppError> {
        let mut buckets = self.buckets.lock().await;
        buckets.remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> MemoryRateLimitConfig {
        MemoryRateLimitConfig {
            enabled: true,
            requests_per_second: 10,
            burst_size: 5,
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_allows_within_burst() {
        let limiter = MemoryRateLimiter::new(&test_config());
        for i in 0..5 {
            let result = limiter.check("client1").await.unwrap();
            assert!(result.allowed, "request {i} should be allowed");
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_denies_over_burst() {
        let limiter = MemoryRateLimiter::new(&test_config());
        for _ in 0..5 {
            limiter.check("client1").await.unwrap();
        }
        let result = limiter.check("client1").await.unwrap();
        assert!(!result.allowed);
        assert!(result.retry_after_ms.is_some());
    }

    #[tokio::test]
    async fn test_rate_limiter_reset() {
        let limiter = MemoryRateLimiter::new(&test_config());
        for _ in 0..5 {
            limiter.check("client1").await.unwrap();
        }
        limiter.reset("client1").await.unwrap();
        let result = limiter.check("client1").await.unwrap();
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn test_rate_limiter_independent_keys() {
        let limiter = MemoryRateLimiter::new(&test_config());
        for _ in 0..5 {
            limiter.check("client1").await.unwrap();
        }
        let result = limiter.check("client2").await.unwrap();
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn test_rate_limiter_recovery_after_wait() {
        let config = MemoryRateLimitConfig {
            enabled: true,
            requests_per_second: 1000,
            burst_size: 1,
        };
        let limiter = MemoryRateLimiter::new(&config);
        let r1 = limiter.check("k").await.unwrap();
        assert!(r1.allowed);
        let r2 = limiter.check("k").await.unwrap();
        assert!(!r2.allowed);
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let r3 = limiter.check("k").await.unwrap();
        assert!(r3.allowed);
    }
}
