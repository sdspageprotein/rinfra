use axum::routing::get;
use axum::{Json, Router};
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::net::middleware::HttpMiddlewareRegistry;
use rinfra_core::response::ApiResponse;
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use tracing::info;

#[derive(Debug, Serialize)]
pub struct HealthData {
    pub status: String,
}

pub struct HttpServer {
    bind_addr: String,
    extra_router: Option<Router>,
    middleware_registry: HttpMiddlewareRegistry,
}

impl HttpServer {
    pub fn new(bind_addr: String) -> Self {
        Self {
            bind_addr,
            extra_router: None,
            middleware_registry: HttpMiddlewareRegistry::new(),
        }
    }

    pub fn merge_router(mut self, router: Router) -> Self {
        match self.extra_router {
            Some(existing) => self.extra_router = Some(existing.merge(router)),
            None => self.extra_router = Some(router),
        }
        self
    }

    pub fn with_middleware_registry(mut self, registry: HttpMiddlewareRegistry) -> Self {
        self.middleware_registry = registry;
        self
    }

    pub fn router(self) -> Result<Router, AppError> {
        let mut router = Router::new().route("/health", get(health_handler));

        if let Some(extra) = self.extra_router {
            router = router.merge(extra);
        }

        if !self.middleware_registry.is_empty() {
            let router_any: Box<dyn std::any::Any> = Box::new(router);
            let router_any = self.middleware_registry.apply_all(router_any)?;
            router = *router_any.downcast::<Router>().map_err(|_| {
                AppError::new(
                    ErrorCode::Internal,
                    "middleware pipeline did not return axum::Router".to_string(),
                )
            })?;
        }

        Ok(router)
    }

    pub async fn start(self, shutdown: CancellationToken) -> Result<(), AppError> {
        let addr = self.bind_addr.clone();
        let router = self.router()?;

        info!(addr = %addr, "HTTP server starting");

        let listener = tokio::net::TcpListener::bind(&addr).await.map_err(|e| {
            AppError::new(
                ErrorCode::ServerBindFailed,
                format!("failed to bind to {addr}: {e}"),
            )
        })?;

        info!(addr = %addr, "HTTP server listening");

        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::ServerStartFailed,
                    format!("HTTP server error: {e}"),
                )
            })
    }
}

async fn health_handler() -> Json<ApiResponse<HealthData>> {
    Json(ApiResponse::success(HealthData {
        status: "healthy".to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::util::ServiceExt;

    use super::super::middleware::builtin::RequestIdHttpMiddleware;

    fn default_server() -> HttpServer {
        HttpServer::new("0.0.0.0:8080".into())
    }

    fn server_with_request_id() -> HttpServer {
        let mut registry = HttpMiddlewareRegistry::new();
        registry
            .register(Arc::new(RequestIdHttpMiddleware))
            .unwrap();
        HttpServer::new("0.0.0.0:8080".into()).with_middleware_registry(registry)
    }

    #[tokio::test]
    async fn test_health_endpoint_returns_ok() {
        let server = default_server();
        let app = server.router().unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "OK");
        assert_eq!(json["data"]["status"], "healthy");
        assert_eq!(json["message"], "ok");
    }

    #[tokio::test]
    async fn test_unknown_route_returns_404() {
        let server = default_server();
        let app = server.router().unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_request_id_header_present() {
        let server = server_with_request_id();
        let app = server.router().unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let request_id = response.headers().get("x-request-id");
        assert!(request_id.is_some(), "x-request-id header should be present");
        let value = request_id.unwrap().to_str().unwrap();
        assert!(
            uuid::Uuid::parse_str(value).is_ok(),
            "x-request-id should be a valid UUID"
        );
    }
}
