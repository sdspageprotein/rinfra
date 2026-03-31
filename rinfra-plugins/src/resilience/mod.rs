use std::future::Future;

use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::resilience::{CircuitBreaker, RetryPolicy};
use tracing::warn;

/// Execute an async operation through a circuit breaker.
///
/// Returns `CircuitBreakerOpen` error if the breaker is open.
pub async fn with_circuit_breaker<F, Fut, T>(
    cb: &CircuitBreaker,
    f: F,
) -> Result<T, AppError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, AppError>>,
{
    if !cb.allow_request() {
        metrics::counter!("circuit_breaker_rejected_total", "name" => cb.name().to_string())
            .increment(1);
        return Err(AppError::new(
            ErrorCode::CircuitBreakerOpen,
            format!("circuit breaker '{}' is open", cb.name()),
        ));
    }

    match f().await {
        Ok(val) => {
            cb.record_success();
            Ok(val)
        }
        Err(e) => {
            let prev_state = cb.state();
            cb.record_failure();
            let new_state = cb.state();
            if prev_state != new_state {
                metrics::counter!("circuit_breaker_trips_total", "name" => cb.name().to_string())
                    .increment(1);
            }
            Err(e)
        }
    }
}

/// Execute an async operation with retry according to the given policy.
///
/// The closure is called up to `policy.max_attempts` times. Between each
/// retry, the task sleeps for the duration dictated by the strategy.
pub async fn with_retry<F, Fut, T>(
    policy: &RetryPolicy,
    mut f: F,
) -> Result<T, AppError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, AppError>>,
{
    let mut last_err = None;

    for attempt in 1..=policy.max_attempts {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                if policy.exhausted(attempt) {
                    return Err(e);
                }
                let delay = policy.delay_for_attempt(attempt - 1);
                warn!(
                    attempt,
                    max = policy.max_attempts,
                    delay_ms = delay.as_millis() as u64,
                    error = %e,
                    "retrying after failure"
                );
                tokio::time::sleep(delay).await;
                last_err = Some(e);
            }
        }
    }

    Err(last_err.unwrap_or_else(|| {
        AppError::new(ErrorCode::Internal, "retry policy exhausted".to_string())
    }))
}

/// Combine circuit breaker and retry: retries the operation, and each
/// attempt is guarded by the circuit breaker.
pub async fn with_retry_and_breaker<F, Fut, T>(
    policy: &RetryPolicy,
    cb: &CircuitBreaker,
    mut f: F,
) -> Result<T, AppError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, AppError>>,
{
    with_retry(policy, || {
        let fut = f();
        async { with_circuit_breaker(cb, || fut).await }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinfra_core::resilience::{CircuitBreakerConfig, CircuitState};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_circuit_breaker_passes_on_success() {
        let cb = CircuitBreaker::new(
            "test",
            CircuitBreakerConfig {
                failure_threshold: 3,
                success_threshold: 1,
                open_duration_secs: 60,
            },
        );
        let result = with_circuit_breaker(&cb, || async { Ok::<_, AppError>(42) }).await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_after_failures() {
        let cb = CircuitBreaker::new(
            "test",
            CircuitBreakerConfig {
                failure_threshold: 2,
                success_threshold: 1,
                open_duration_secs: 60,
            },
        );

        for _ in 0..2 {
            let _ = with_circuit_breaker(&cb, || async {
                Err::<i32, _>(AppError::new(ErrorCode::Internal, "boom"))
            })
            .await;
        }

        assert_eq!(cb.state(), CircuitState::Open);

        let err = with_circuit_breaker(&cb, || async { Ok::<_, AppError>(1) })
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::CircuitBreakerOpen);
    }

    #[tokio::test]
    async fn test_retry_succeeds_after_failures() {
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let policy = RetryPolicy::fixed(3, 10);

        let result = with_retry(&policy, || {
            let c = c.clone();
            async move {
                let n = c.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err(AppError::new(ErrorCode::Internal, "not yet"))
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_exhausted() {
        let policy = RetryPolicy::fixed(2, 10);
        let result: Result<i32, _> = with_retry(&policy, || async {
            Err(AppError::new(ErrorCode::Internal, "always fails"))
        })
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_retry_immediate_success() {
        let policy = RetryPolicy::fixed(5, 100);
        let result = with_retry(&policy, || async { Ok::<_, AppError>(99) }).await;
        assert_eq!(result.unwrap(), 99);
    }
}
