use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::http::{Request, Response, StatusCode};
use rinfra_core::audit::{AuditEvent, AuditLogger, AuditOutcome};
use tower::{Layer, Service};

use super::store::AdminTokenStore;

/// Tower layer that enforces admin token authentication.
#[derive(Clone)]
pub struct AdminAuthLayer {
    store: Arc<AdminTokenStore>,
    exclude_paths: Vec<String>,
    audit: Option<Arc<dyn AuditLogger>>,
}

impl AdminAuthLayer {
    pub fn new(store: Arc<AdminTokenStore>, exclude_paths: Vec<String>) -> Self {
        Self {
            store,
            exclude_paths,
            audit: None,
        }
    }

    pub fn with_audit(mut self, audit: Arc<dyn AuditLogger>) -> Self {
        self.audit = Some(audit);
        self
    }
}

impl<S> Layer<S> for AdminAuthLayer {
    type Service = AdminAuthService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        AdminAuthService {
            inner,
            store: self.store.clone(),
            exclude_paths: self.exclude_paths.clone(),
            audit: self.audit.clone(),
        }
    }
}

#[derive(Clone)]
pub struct AdminAuthService<S> {
    inner: S,
    store: Arc<AdminTokenStore>,
    exclude_paths: Vec<String>,
    audit: Option<Arc<dyn AuditLogger>>,
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for AdminAuthService<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Send + 'static,
    ResBody: Default,
{
    type Response = Response<ResBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let path = req.uri().path().to_string();

        if self.exclude_paths.iter().any(|p| path.starts_with(p)) {
            let mut svc = self.inner.clone();
            return Box::pin(async move { svc.call(req).await });
        }

        let token = extract_token(&req);
        let store = self.store.clone();
        let audit = self.audit.clone();
        let mut svc = self.inner.clone();

        Box::pin(async move {
            let token = match token {
                Some(t) => t,
                None => {
                    emit_audit(&audit, "anonymous", &path, AuditOutcome::Denied).await;
                    return Ok(unauthorized("missing admin token — provide Authorization: Bearer <token> or X-Admin-Token header"));
                }
            };

            match store.verify(&token) {
                Some(info) => {
                    emit_audit(&audit, &info.label, &path, AuditOutcome::Success).await;
                    let mut req = req;
                    req.extensions_mut().insert(info);
                    svc.call(req).await
                }
                None => {
                    emit_audit(&audit, "unknown", &path, AuditOutcome::Denied).await;
                    Ok(unauthorized("invalid or expired admin token"))
                }
            }
        })
    }
}

fn extract_token<B>(req: &Request<B>) -> Option<String> {
    if let Some(val) = req.headers().get("x-admin-token") {
        if let Ok(s) = val.to_str() {
            return Some(s.to_string());
        }
    }

    if let Some(val) = req.headers().get("authorization") {
        if let Ok(s) = val.to_str() {
            if let Some(token) = s.strip_prefix("Bearer ") {
                return Some(token.to_string());
            }
        }
    }

    None
}

fn unauthorized<B: Default>(_message: &str) -> Response<B> {
    let mut resp = Response::new(B::default());
    *resp.status_mut() = StatusCode::UNAUTHORIZED;
    resp
}

async fn emit_audit(
    audit: &Option<Arc<dyn AuditLogger>>,
    actor: &str,
    path: &str,
    outcome: AuditOutcome,
) {
    if let Some(logger) = audit {
        let event = AuditEvent::new(actor, "admin.access", "admin_api", outcome)
            .details(serde_json::json!({ "path": path }));
        if let Err(e) = logger.log(event).await {
            tracing::warn!(error = %e, "failed to write audit log");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::store::AdminTokenInfo;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use axum::routing::get;
    use axum::Router;
    use tower::util::ServiceExt;

    async fn dummy_handler(
        info: Option<axum::Extension<AdminTokenInfo>>,
    ) -> axum::Json<serde_json::Value> {
        match info {
            Some(axum::Extension(info)) => {
                axum::Json(serde_json::json!({"role": info.role.to_string(), "label": info.label}))
            }
            None => axum::Json(serde_json::json!({"role": "anonymous"})),
        }
    }

    fn temp_path() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("rinfra_auth_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("tokens.json")
    }

    #[tokio::test]
    async fn test_auth_layer_rejects_no_token() {
        let path = temp_path();
        let store = Arc::new(AdminTokenStore::load(&path).unwrap());
        store
            .create_token(super::super::store::AdminRole::Root, "root".into(), None)
            .unwrap();

        let app = Router::new()
            .route("/api/admin/test", get(dummy_handler))
            .layer(AdminAuthLayer::new(store, vec![]));

        let resp = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/admin/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[tokio::test]
    async fn test_auth_layer_accepts_valid_bearer() {
        let path = temp_path();
        let store = Arc::new(AdminTokenStore::load(&path).unwrap());
        let (token, _) = store
            .create_token(super::super::store::AdminRole::Root, "root".into(), None)
            .unwrap();

        let app = Router::new()
            .route("/api/admin/test", get(dummy_handler))
            .layer(AdminAuthLayer::new(store, vec![]));

        let resp = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/admin/test")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[tokio::test]
    async fn test_auth_layer_accepts_x_admin_token() {
        let path = temp_path();
        let store = Arc::new(AdminTokenStore::load(&path).unwrap());
        let (token, _) = store
            .create_token(super::super::store::AdminRole::Admin, "admin".into(), None)
            .unwrap();

        let app = Router::new()
            .route("/api/admin/test", get(dummy_handler))
            .layer(AdminAuthLayer::new(store, vec![]));

        let resp = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/admin/test")
                    .header("x-admin-token", &token)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[tokio::test]
    async fn test_auth_layer_exclude_paths() {
        let path = temp_path();
        let store = Arc::new(AdminTokenStore::load(&path).unwrap());

        let app = Router::new()
            .route("/api/admin/health", get(dummy_handler))
            .route("/api/admin/config", get(dummy_handler))
            .layer(AdminAuthLayer::new(
                store,
                vec!["/api/admin/health".to_string()],
            ));

        let resp = app
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/admin/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let resp = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/admin/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
