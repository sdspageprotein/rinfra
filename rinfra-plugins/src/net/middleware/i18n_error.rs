use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use http_body_util::BodyExt;
use rinfra_core::i18n::I18n;

pub type I18nErrorState = Arc<dyn I18n>;

pub async fn i18n_error_middleware(
    State(i18n): State<I18nErrorState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let locale = parse_accept_language(req.headers());

    let response = next.run(req).await;

    if response.status().is_success() {
        return response;
    }

    let is_json = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.contains("application/json"))
        .unwrap_or(false);

    if !is_json {
        return response;
    }

    let (parts, body) = response.into_parts();
    let bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => return (parts, Body::empty()).into_response(),
    };

    let mut json: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => {
            return (parts, Body::from(bytes)).into_response();
        }
    };

    if let Some(code) = json.get("code").and_then(|c| c.as_str()) {
        let i18n_key = format!("error.{}", code.to_lowercase());
        let translated = i18n.t(&i18n_key, &locale);
        if translated != i18n_key {
            json["message"] = serde_json::Value::String(translated);
        }
    }

    let new_body = serde_json::to_vec(&json).unwrap_or_else(|_| bytes.to_vec());
    (parts, Body::from(new_body)).into_response()
}

fn parse_accept_language(headers: &axum::http::HeaderMap) -> String {
    headers
        .get("accept-language")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.split(';').next().unwrap_or(s).trim().to_string())
        .unwrap_or_else(|| "en".to_string())
}
