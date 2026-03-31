use axum::http::{Request, Response};
use opentelemetry::global;
use opentelemetry::propagation::Extractor;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{Layer, Service};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// Tower layer that extracts W3C TraceContext from incoming HTTP request headers
/// and sets it as the parent of the current tracing span.
#[derive(Clone)]
pub struct OtelPropagationLayer;

impl<S> Layer<S> for OtelPropagationLayer {
    type Service = OtelPropagationService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        OtelPropagationService { inner }
    }
}

#[derive(Clone)]
pub struct OtelPropagationService<S> {
    inner: S,
}

struct HeaderExtractor<'a, B>(&'a Request<B>);

impl<B> Extractor for HeaderExtractor<'_, B> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.headers().get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0
            .headers()
            .keys()
            .map(|k| k.as_str())
            .collect()
    }
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for OtelPropagationService<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Send + 'static,
{
    type Response = Response<ResBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let parent_cx = global::get_text_map_propagator(|propagator| {
            propagator.extract(&HeaderExtractor(&req))
        });

        Span::current().set_parent(parent_cx);

        let mut svc = self.inner.clone();
        Box::pin(async move { svc.call(req).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request as HttpRequest, StatusCode};
    use axum::routing::get;
    use axum::{Json, Router};
    use tower::util::ServiceExt;

    async fn dummy_handler() -> Json<serde_json::Value> {
        Json(serde_json::json!({"ok": true}))
    }

    #[tokio::test]
    async fn test_otel_propagation_layer_passes_through() {
        let app = Router::new()
            .route("/test", get(dummy_handler))
            .layer(OtelPropagationLayer);

        let response = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_otel_propagation_with_traceparent_header() {
        let app = Router::new()
            .route("/test", get(dummy_handler))
            .layer(OtelPropagationLayer);

        let response = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/test")
                    .header(
                        "traceparent",
                        "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
