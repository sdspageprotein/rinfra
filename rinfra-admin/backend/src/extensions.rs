use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use rinfra_core::response::ApiResponse;
use serde::Serialize;

/// Metadata for a business-defined admin page.
/// The frontend reads this via `/api/admin/extensions` to dynamically
/// render sidebar menu items and route to `GenericView`.
#[derive(Debug, Clone, Serialize)]
pub struct MenuEntry {
    pub path: String,
    pub name: String,
    pub icon: String,
    pub category: String,
    pub data_url: String,
}

/// Build a standalone router for `/extensions` that serves menu metadata.
/// Returns `Router<()>` (no external state needed).
pub(crate) fn extensions_route(entries: Vec<MenuEntry>) -> Router {
    let shared = Arc::new(entries);
    Router::new()
        .route("/extensions", get(extensions_handler))
        .with_state(shared)
}

async fn extensions_handler(
    State(entries): State<Arc<Vec<MenuEntry>>>,
) -> Json<ApiResponse<Vec<MenuEntry>>> {
    Json(ApiResponse::success((*entries).clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::util::ServiceExt;

    #[tokio::test]
    async fn test_extensions_empty() {
        let app = extensions_route(vec![]);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/extensions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "OK");
        assert!(json["data"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_extensions_with_entries() {
        let entries = vec![MenuEntry {
            path: "/game/status".into(),
            name: "Game Status".into(),
            icon: "G".into(),
            category: "game".into(),
            data_url: "/api/admin/game/status".into(),
        }];
        let app = extensions_route(entries);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/extensions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["data"][0]["name"], "Game Status");
        assert_eq!(json["data"][0]["data_url"], "/api/admin/game/status");
    }
}
