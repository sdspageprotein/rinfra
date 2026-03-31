use std::sync::Arc;

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use rinfra_core::error::ErrorCode;
use rinfra_core::ratelimit::RateLimiter;
use rinfra_core::response::ApiResponse;

pub struct RateLimitState {
    pub limiter: Arc<dyn RateLimiter>,
    pub key_strategy: String,
}

pub async fn rate_limit_middleware(
    state: axum::extract::State<Arc<RateLimitState>>,
    req: Request,
    next: Next,
) -> Response {
    let key = extract_key(&state.key_strategy, &req);

    match state.limiter.check(&key).await {
        Ok(result) if result.allowed => next.run(req).await,
        Ok(result) => {
            let retry_after = result.retry_after_ms.unwrap_or(1000);
            let body = ApiResponse::<()> {
                code: ErrorCode::RateLimitExceeded.as_str().to_string(),
                data: None,
                message: "rate limit exceeded".to_string(),
            };
            let mut resp = (StatusCode::TOO_MANY_REQUESTS, Json(body)).into_response();
            resp.headers_mut().insert(
                "retry-after",
                axum::http::HeaderValue::from_str(&format!(
                    "{}",
                    (retry_after as f64 / 1000.0).ceil() as u64
                ))
                .unwrap_or_else(|_| axum::http::HeaderValue::from_static("1")),
            );
            resp.headers_mut().insert(
                "x-ratelimit-remaining",
                axum::http::HeaderValue::from(result.remaining),
            );
            resp
        }
        Err(e) => {
            tracing::warn!(error = %e, "rate limiter check failed, allowing request");
            next.run(req).await
        }
    }
}

fn extract_key(strategy: &str, req: &Request) -> String {
    if strategy == "global" {
        return "global".to_string();
    }

    if let Some(header_name) = strategy.strip_prefix("header:") {
        if let Some(val) = req.headers().get(header_name).and_then(|v| v.to_str().ok()) {
            return val.to_string();
        }
        return "unknown".to_string();
    }

    // Default: "ip"
    req.extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| ci.0.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use axum::middleware;
    use axum::routing::get;
    use axum::Router;
    use http_body_util::BodyExt;
    use rinfra_core::error::AppError;
    use rinfra_core::ratelimit::RateLimitResult;
    use tower::util::ServiceExt;

    struct AllowLimiter;
    #[async_trait::async_trait]
    impl RateLimiter for AllowLimiter {
        async fn check(&self, _key: &str) -> Result<RateLimitResult, AppError> {
            Ok(RateLimitResult {
                allowed: true,
                remaining: 99,
                retry_after_ms: None,
            })
        }
        async fn reset(&self, _key: &str) -> Result<(), AppError> {
            Ok(())
        }
    }

    struct DenyLimiter;
    #[async_trait::async_trait]
    impl RateLimiter for DenyLimiter {
        async fn check(&self, _key: &str) -> Result<RateLimitResult, AppError> {
            Ok(RateLimitResult {
                allowed: false,
                remaining: 0,
                retry_after_ms: Some(2000),
            })
        }
        async fn reset(&self, _key: &str) -> Result<(), AppError> {
            Ok(())
        }
    }

    fn test_app(limiter: Arc<dyn RateLimiter>) -> Router {
        let state = Arc::new(RateLimitState {
            limiter,
            key_strategy: "global".to_string(),
        });
        Router::new()
            .route("/api/data", get(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(
                state.clone(),
                rate_limit_middleware,
            ))
            .with_state(state)
    }

    #[tokio::test]
    async fn test_ratelimit_allowed() {
        let app = test_app(Arc::new(AllowLimiter));
        let resp = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/data")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_ratelimit_denied() {
        let app = test_app(Arc::new(DenyLimiter));
        let resp = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/data")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(resp.headers().contains_key("retry-after"));
        assert!(resp.headers().contains_key("x-ratelimit-remaining"));
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "RATE_LIMIT_EXCEEDED");
    }

    #[test]
    fn test_extract_key_global() {
        let req = HttpRequest::builder()
            .uri("/test")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_key("global", &req), "global");
    }

    #[test]
    fn test_extract_key_header() {
        let req = HttpRequest::builder()
            .uri("/test")
            .header("x-api-key", "my-key")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_key("header:x-api-key", &req), "my-key");
    }

    #[test]
    fn test_extract_key_header_missing() {
        let req = HttpRequest::builder()
            .uri("/test")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_key("header:x-api-key", &req), "unknown");
    }
}
