use std::time::Instant;

use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use rinfra_core::config::MetricsConfig;
use rinfra_core::error::{AppError, ErrorCode};
use tracing::info;

pub fn init_metrics() -> Result<PrometheusHandle, AppError> {
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .map_err(|e| AppError::new(ErrorCode::Internal, format!("failed to install Prometheus recorder: {e}")))?;
    info!("prometheus metrics recorder installed");
    Ok(handle)
}

pub fn metrics_router(handle: PrometheusHandle, endpoint: &str) -> Router {
    let endpoint = endpoint.to_string();
    Router::new().route(&endpoint, get(move || {
        let h = handle.clone();
        async move { h.render() }
    }))
}

pub async fn http_metrics_middleware(req: Request, next: Next) -> Response {
    let method = req.method().to_string();
    let path = req
        .extensions()
        .get::<MatchedPath>()
        .map(|mp| mp.as_str().to_owned())
        .unwrap_or_else(|| req.uri().path().to_owned());
    let start = Instant::now();

    let response = next.run(req).await;

    let duration = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();

    let labels = [
        ("method", method),
        ("path", path),
        ("status", status),
    ];

    metrics::counter!("http_requests_total", &labels).increment(1);
    metrics::histogram!("http_request_duration_seconds", &labels[..2]).record(duration);

    response
}

pub fn apply_metrics(
    router: Router,
    config: &MetricsConfig,
) -> Result<(Router, Option<PrometheusHandle>), AppError> {
    if !config.enabled {
        return Ok((router, None));
    }

    let handle = init_metrics()?;
    let metrics_route = metrics_router(handle.clone(), &config.endpoint);

    let router = router
        .merge(metrics_route)
        .layer(axum::middleware::from_fn(http_metrics_middleware));

    Ok((router, Some(handle)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use http_body_util::BodyExt;
    use tower::util::ServiceExt;

    #[tokio::test]
    async fn test_metrics_endpoint_returns_text() {
        let handle = PrometheusBuilder::new()
            .install_recorder()
            .unwrap();
        let app = metrics_router(handle, "/metrics");

        let resp = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8(body.to_vec()).unwrap();
        // Prometheus output may be empty (no metrics recorded) or contain # comments
        let _ = text;
    }
}
