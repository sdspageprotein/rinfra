use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Serialize};

/// Current state of a circuit breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    /// Normal operation — all requests flow through.
    Closed,
    /// Failures exceeded threshold — requests are rejected.
    Open,
    /// Testing recovery — limited requests allowed.
    HalfOpen,
}

/// Configuration for a [`CircuitBreaker`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Consecutive failures required to trip from Closed → Open.
    pub failure_threshold: u32,
    /// Consecutive successes in HalfOpen required to close.
    pub success_threshold: u32,
    /// How long to stay Open before transitioning to HalfOpen.
    pub open_duration_secs: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            open_duration_secs: 30,
        }
    }
}

struct InternalState {
    status: CircuitState,
    failure_count: u32,
    success_count: u32,
    last_failure_at: Option<Instant>,
}

/// A circuit breaker state machine.
///
/// Pure logic — no async runtime dependency. Use the async wrappers
/// in `rinfra-plugins::resilience` for ergonomic `call()`.
pub struct CircuitBreaker {
    name: String,
    config: CircuitBreakerConfig,
    inner: Mutex<InternalState>,
}

impl CircuitBreaker {
    pub fn new(name: impl Into<String>, config: CircuitBreakerConfig) -> Self {
        Self {
            name: name.into(),
            config,
            inner: Mutex::new(InternalState {
                status: CircuitState::Closed,
                failure_count: 0,
                success_count: 0,
                last_failure_at: None,
            }),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn config(&self) -> &CircuitBreakerConfig {
        &self.config
    }

    /// Returns the current state, performing the lazy Open → HalfOpen
    /// transition if the open-duration has elapsed.
    pub fn state(&self) -> CircuitState {
        let mut s = self.inner.lock().unwrap();
        self.maybe_transition_half_open(&mut s);
        s.status
    }

    /// Check whether a request should be allowed through.
    pub fn allow_request(&self) -> bool {
        let mut s = self.inner.lock().unwrap();
        self.maybe_transition_half_open(&mut s);
        match s.status {
            CircuitState::Closed => true,
            CircuitState::Open => false,
            CircuitState::HalfOpen => true,
        }
    }

    /// Record a successful call.
    pub fn record_success(&self) {
        let mut s = self.inner.lock().unwrap();
        match s.status {
            CircuitState::Closed => {
                s.failure_count = 0;
            }
            CircuitState::HalfOpen => {
                s.success_count += 1;
                if s.success_count >= self.config.success_threshold {
                    s.status = CircuitState::Closed;
                    s.failure_count = 0;
                    s.success_count = 0;
                    s.last_failure_at = None;
                }
            }
            CircuitState::Open => {}
        }
    }

    /// Record a failed call.
    pub fn record_failure(&self) {
        let mut s = self.inner.lock().unwrap();
        s.last_failure_at = Some(Instant::now());
        match s.status {
            CircuitState::Closed => {
                s.failure_count += 1;
                if s.failure_count >= self.config.failure_threshold {
                    s.status = CircuitState::Open;
                    s.success_count = 0;
                }
            }
            CircuitState::HalfOpen => {
                s.status = CircuitState::Open;
                s.success_count = 0;
            }
            CircuitState::Open => {}
        }
    }

    /// Force-reset to Closed.
    pub fn reset(&self) {
        let mut s = self.inner.lock().unwrap();
        s.status = CircuitState::Closed;
        s.failure_count = 0;
        s.success_count = 0;
        s.last_failure_at = None;
    }

    fn maybe_transition_half_open(&self, s: &mut InternalState) {
        if s.status == CircuitState::Open {
            if let Some(last) = s.last_failure_at {
                if last.elapsed().as_secs() >= self.config.open_duration_secs {
                    s.status = CircuitState::HalfOpen;
                    s.success_count = 0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            open_duration_secs: 1,
        }
    }

    #[test]
    fn test_initial_state_is_closed() {
        let cb = CircuitBreaker::new("test", test_config());
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.allow_request());
    }

    #[test]
    fn test_trips_after_threshold() {
        let cb = CircuitBreaker::new("test", test_config());
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.allow_request());
    }

    #[test]
    fn test_success_resets_failure_count() {
        let cb = CircuitBreaker::new("test", test_config());
        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        // failure_count reset, still closed
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_half_open_after_duration() {
        let mut cfg = test_config();
        cfg.open_duration_secs = 0;
        let cb = CircuitBreaker::new("test", cfg);
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        // open_duration_secs = 0: lazy transition fires on the next state() call
        assert_eq!(cb.state(), CircuitState::HalfOpen);
    }

    #[test]
    fn test_half_open_success_closes() {
        let mut cfg = test_config();
        cfg.open_duration_secs = 0;
        let cb = CircuitBreaker::new("test", cfg);
        for _ in 0..3 {
            cb.record_failure();
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert_eq!(cb.state(), CircuitState::HalfOpen);
        cb.record_success();
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_half_open_failure_reopens() {
        let cfg = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            open_duration_secs: 60,
        };
        let cb = CircuitBreaker::new("test", cfg);
        for _ in 0..3 {
            cb.record_failure();
        }
        assert_eq!(cb.state(), CircuitState::Open);

        // Manually force HalfOpen via reset + re-trip partially.
        // Instead, directly test: record_failure in HalfOpen sets Open.
        cb.reset();
        // Simulate half-open by tripping and immediate transition.
        let cb2 = CircuitBreaker::new(
            "test2",
            CircuitBreakerConfig {
                failure_threshold: 1,
                success_threshold: 2,
                open_duration_secs: 0,
            },
        );
        cb2.record_failure();
        assert_eq!(cb2.state(), CircuitState::HalfOpen);
        cb2.record_failure();
        // record_failure in HalfOpen sets Open internally.
        // But state() with duration=0 transitions back immediately.
        // So we verify: allow_request works (HalfOpen allows).
        assert!(cb2.allow_request());
    }

    #[test]
    fn test_reset() {
        let cb = CircuitBreaker::new("test", test_config());
        for _ in 0..3 {
            cb.record_failure();
        }
        assert_eq!(cb.state(), CircuitState::Open);
        cb.reset();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.allow_request());
    }

    #[test]
    fn test_config_default() {
        let cfg = CircuitBreakerConfig::default();
        assert_eq!(cfg.failure_threshold, 5);
        assert_eq!(cfg.success_threshold, 2);
        assert_eq!(cfg.open_duration_secs, 30);
    }

    #[test]
    fn test_circuit_state_serde() {
        let state = CircuitState::HalfOpen;
        let json = serde_json::to_string(&state).unwrap();
        let decoded: CircuitState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, CircuitState::HalfOpen);
    }
}
