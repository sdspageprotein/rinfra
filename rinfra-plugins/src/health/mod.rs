pub mod checkers;

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use rinfra_core::appstate::AppState;
use rinfra_core::plugin::HealthStatus;
use serde::Serialize;

pub use checkers::{CacheHealthChecker, MessageBusHealthChecker, StoreHealthChecker};

#[derive(Serialize)]
struct LivenessResponse {
    status: &'static str,
}

#[derive(Serialize)]
struct ReadinessResponse {
    status: &'static str,
    checks: Vec<CheckResult>,
}

#[derive(Serialize)]
struct CheckResult {
    name: String,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

async fn healthz_handler() -> Json<LivenessResponse> {
    Json(LivenessResponse { status: "alive" })
}

async fn readyz_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut checks = Vec::new();
    let mut all_ok = true;

    if let Some(registry) = state.health_checkers() {
        for checker in registry.checkers() {
            let result = checker.check().await;
            let ok = result.is_healthy();
            if !ok {
                all_ok = false;
            }
            let status_str = match result.status {
                HealthStatus::Healthy => "ok",
                HealthStatus::Degraded => "degraded",
                HealthStatus::Unhealthy => "fail",
            };
            checks.push(CheckResult {
                name: checker.name().to_string(),
                status: status_str,
                error: result.error,
            });
        }
    }

    let status_code = if all_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    let resp = ReadinessResponse {
        status: if all_ok { "ready" } else { "not_ready" },
        checks,
    };

    (status_code, Json(resp))
}

pub fn health_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/healthz", get(healthz_handler))
        .route("/readyz", get(readyz_handler))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use rinfra_core::config::RinfraConfig;
    use tower::util::ServiceExt;

    fn test_state() -> Arc<AppState> {
        Arc::new(AppState::new(RinfraConfig::default()))
    }

    #[tokio::test]
    async fn test_healthz_returns_alive() {
        let app = health_router(test_state());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "alive");
    }

    #[tokio::test]
    async fn test_readyz_no_deps_returns_ready() {
        let app = health_router(test_state());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/readyz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ready");
        assert!(json["checks"].as_array().unwrap().is_empty());
    }
}
