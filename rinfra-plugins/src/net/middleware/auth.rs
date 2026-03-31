use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use jsonwebtoken::{decode, DecodingKey, Validation};
use rinfra_core::error::ErrorCode;
use rinfra_core::response::ApiResponse;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JwtClaims {
    pub sub: String,
    pub exp: u64,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

pub struct AuthState {
    pub secret: String,
    pub exclude_paths: Vec<String>,
}

pub async fn auth_middleware(
    state: axum::extract::State<Arc<AuthState>>,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();

    if state.exclude_paths.iter().any(|p| path.starts_with(p)) {
        return next.run(req).await;
    }

    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    let token = match auth_header {
        Some(h) if h.starts_with("Bearer ") => &h[7..],
        _ => {
            return make_error_response(ErrorCode::AuthTokenMissing, "missing or invalid Authorization header");
        }
    };

    let key = DecodingKey::from_secret(state.secret.as_bytes());
    let mut validation = Validation::default();
    validation.validate_exp = true;

    match decode::<JwtClaims>(token, &key, &validation) {
        Ok(data) => {
            let mut req = req;
            req.extensions_mut().insert(data.claims);
            next.run(req).await
        }
        Err(e) => {
            let code = if e.kind() == &jsonwebtoken::errors::ErrorKind::ExpiredSignature {
                ErrorCode::AuthTokenExpired
            } else {
                ErrorCode::AuthTokenInvalid
            };
            make_error_response(code, &format!("token validation failed: {e}"))
        }
    }
}

fn make_error_response(code: ErrorCode, message: &str) -> Response {
    let status = StatusCode::from_u16(code.http_status()).unwrap_or(StatusCode::UNAUTHORIZED);
    let body = ApiResponse::<()> {
        code: code.as_str().to_string(),
        data: None,
        message: message.to_string(),
    };
    (status, Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use axum::middleware;
    use axum::routing::get;
    use axum::Router;
    use http_body_util::BodyExt;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use tower::util::ServiceExt;

    fn test_state() -> Arc<AuthState> {
        Arc::new(AuthState {
            secret: "test-secret".to_string(),
            exclude_paths: vec!["/health".to_string(), "/public".to_string()],
        })
    }

    fn make_token(secret: &str, exp_offset: i64) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let claims = JwtClaims {
            sub: "user-1".to_string(),
            exp: (now as i64 + exp_offset) as u64,
            extra: serde_json::Value::Null,
        };
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    fn test_app(state: Arc<AuthState>) -> Router {
        Router::new()
            .route("/protected", get(|| async { "ok" }))
            .route("/health", get(|| async { "healthy" }))
            .route("/public/info", get(|| async { "info" }))
            .layer(middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
            ))
            .with_state(state)
    }

    #[tokio::test]
    async fn test_auth_missing_token_returns_401() {
        let app = test_app(test_state());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "AUTH_TOKEN_MISSING");
    }

    #[tokio::test]
    async fn test_auth_invalid_token_returns_401() {
        let app = test_app(test_state());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header("authorization", "Bearer invalid.token.here")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "AUTH_TOKEN_INVALID");
    }

    #[tokio::test]
    async fn test_auth_expired_token_returns_401() {
        let token = make_token("test-secret", -120);
        let app = test_app(test_state());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "AUTH_TOKEN_EXPIRED");
    }

    #[tokio::test]
    async fn test_auth_valid_token_passes() {
        let token = make_token("test-secret", 3600);
        let app = test_app(test_state());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_excluded_path_skips_auth() {
        let app = test_app(test_state());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_excluded_prefix_skips_auth() {
        let app = test_app(test_state());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/public/info")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_wrong_secret_returns_invalid() {
        let token = make_token("wrong-secret", 3600);
        let app = test_app(test_state());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
