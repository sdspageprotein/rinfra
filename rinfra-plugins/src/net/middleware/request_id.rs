use axum::http::{HeaderName, HeaderValue, Request, Response};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{Layer, Service};

#[derive(Clone)]
pub struct RequestIdLayer;

impl<S> Layer<S> for RequestIdLayer {
    type Service = RequestIdService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        RequestIdService { inner }
    }
}

#[derive(Clone)]
pub struct RequestIdService<S> {
    inner: S,
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for RequestIdService<S>
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
        let mut svc = self.inner.clone();
        Box::pin(async move {
            let mut response = svc.call(req).await?;
            let request_id = uuid::Uuid::new_v4().to_string();
            response.headers_mut().insert(
                HeaderName::from_static("x-request-id"),
                HeaderValue::from_str(&request_id).unwrap(),
            );
            Ok(response)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use axum::{Json, Router};
    use tower::util::ServiceExt;

    async fn dummy_handler() -> Json<serde_json::Value> {
        Json(serde_json::json!({"ok": true}))
    }

    #[tokio::test]
    async fn test_request_id_layer_adds_header() {
        let app = Router::new()
            .route("/test", get(dummy_handler))
            .layer(RequestIdLayer);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let request_id = response.headers().get("x-request-id");
        assert!(request_id.is_some(), "x-request-id header should be present");
        let value = request_id.unwrap().to_str().unwrap();
        assert!(
            uuid::Uuid::parse_str(value).is_ok(),
            "x-request-id should be a valid UUID v4"
        );
    }
}
