use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Backoff strategy for retry delays.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetryStrategy {
    /// Fixed delay between every attempt.
    Fixed { delay_ms: u64 },
    /// Exponential backoff: `base_ms * 2^attempt`, capped at `max_ms`.
    Exponential { base_ms: u64, max_ms: u64 },
}

/// Retry policy configuration.
///
/// Pure computation — call [`RetryPolicy::delay_for_attempt`] to get the
/// sleep duration; the actual sleeping is done by the async wrappers in
/// `rinfra-plugins::resilience`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Total number of attempts (including the first call).
    pub max_attempts: u32,
    pub strategy: RetryStrategy,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            strategy: RetryStrategy::Exponential {
                base_ms: 200,
                max_ms: 5000,
            },
        }
    }
}

impl RetryPolicy {
    pub fn fixed(max_attempts: u32, delay_ms: u64) -> Self {
        Self {
            max_attempts,
            strategy: RetryStrategy::Fixed { delay_ms },
        }
    }

    pub fn exponential(max_attempts: u32, base_ms: u64, max_ms: u64) -> Self {
        Self {
            max_attempts,
            strategy: RetryStrategy::Exponential { base_ms, max_ms },
        }
    }

    /// Compute the delay before the `attempt`-th retry (0-indexed).
    /// Attempt 0 = first retry (after the initial call failed).
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        match &self.strategy {
            RetryStrategy::Fixed { delay_ms } => Duration::from_millis(*delay_ms),
            RetryStrategy::Exponential { base_ms, max_ms } => {
                let multiplier = 1u64.checked_shl(attempt).unwrap_or(u64::MAX);
                let delay = base_ms.saturating_mul(multiplier);
                Duration::from_millis(delay.min(*max_ms))
            }
        }
    }

    /// Whether `attempt` (1-indexed total count) has exhausted the budget.
    pub fn exhausted(&self, attempt: u32) -> bool {
        attempt >= self.max_attempts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_delay() {
        let p = RetryPolicy::fixed(3, 500);
        assert_eq!(p.delay_for_attempt(0), Duration::from_millis(500));
        assert_eq!(p.delay_for_attempt(1), Duration::from_millis(500));
        assert_eq!(p.delay_for_attempt(2), Duration::from_millis(500));
    }

    #[test]
    fn test_exponential_delay() {
        let p = RetryPolicy::exponential(5, 100, 2000);
        assert_eq!(p.delay_for_attempt(0), Duration::from_millis(100));
        assert_eq!(p.delay_for_attempt(1), Duration::from_millis(200));
        assert_eq!(p.delay_for_attempt(2), Duration::from_millis(400));
        assert_eq!(p.delay_for_attempt(3), Duration::from_millis(800));
        assert_eq!(p.delay_for_attempt(4), Duration::from_millis(1600));
        // capped at max_ms
        assert_eq!(p.delay_for_attempt(5), Duration::from_millis(2000));
    }

    #[test]
    fn test_exhausted() {
        let p = RetryPolicy::fixed(3, 100);
        assert!(!p.exhausted(1));
        assert!(!p.exhausted(2));
        assert!(p.exhausted(3));
        assert!(p.exhausted(4));
    }

    #[test]
    fn test_default() {
        let p = RetryPolicy::default();
        assert_eq!(p.max_attempts, 3);
        matches!(p.strategy, RetryStrategy::Exponential { .. });
    }

    #[test]
    fn test_serde() {
        let p = RetryPolicy::fixed(5, 1000);
        let json = serde_json::to_string(&p).unwrap();
        let decoded: RetryPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.max_attempts, 5);
    }
}
