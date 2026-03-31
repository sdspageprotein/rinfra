use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::Router;
use rinfra_core::AppState;
use rinfra_plugins::ClusterNodeList;
use tracing::{debug, error};

#[derive(Clone)]
struct ProxyState {
    node_list: ClusterNodeList,
    counter: Arc<AtomicUsize>,
    client: reqwest::Client,
}

pub fn proxy_router(state: Arc<AppState>) -> Router {
    let node_list = state
        .get::<ClusterNodeList>()
        .expect("ClusterNodeList not found in AppState; cluster mode required")
        .clone();

    let proxy_state = ProxyState {
        node_list,
        counter: Arc::new(AtomicUsize::new(0)),
        client: reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build reqwest client"),
    };

    Router::new()
        .route("/{*path}", any(proxy_handler))
        .route("/", any(proxy_handler))
        .with_state(proxy_state)
}

fn pick_web_backend(state: &ProxyState, nodes: &[rinfra_core::cluster::NodeInfo]) -> Option<String> {
    let web_endpoints: Vec<&str> = nodes
        .iter()
        .filter(|n| {
            n.status == rinfra_core::cluster::NodeStatus::Online
                && n.metadata.get("service_type").map(|v| v == "web").unwrap_or(false)
        })
        .flat_map(|n| {
            n.endpoints
                .iter()
                .filter(|e| e.protocol == "http")
                .map(|e| e.address.as_str())
        })
        .collect();

    if web_endpoints.is_empty() {
        return None;
    }

    let idx = state.counter.fetch_add(1, Ordering::Relaxed) % web_endpoints.len();
    let addr = web_endpoints[idx];
    let url = if addr.starts_with("http") {
        addr.to_string()
    } else {
        format!("http://{addr}")
    };
    Some(url)
}

async fn proxy_handler(
    State(state): State<ProxyState>,
    req: Request<Body>,
) -> Response {
    let nodes = state.node_list.0.read().await;
    let backend = match pick_web_backend(&state, &nodes) {
        Some(b) => b,
        None => {
            return (StatusCode::SERVICE_UNAVAILABLE, "no web backends available").into_response();
        }
    };
    drop(nodes);

    let (parts, body) = req.into_parts();
    let path = parts.uri.path();
    let query = parts
        .uri
        .query()
        .map(|q| format!("?{q}"))
        .unwrap_or_default();
    let upstream_url = format!("{backend}{path}{query}");

    debug!(upstream = %upstream_url, "forwarding request");

    let body_bytes = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            error!(error = %e, "failed to read request body");
            return (StatusCode::BAD_REQUEST, "failed to read body").into_response();
        }
    };

    let method = match parts.method {
        axum::http::Method::GET => reqwest::Method::GET,
        axum::http::Method::POST => reqwest::Method::POST,
        axum::http::Method::PUT => reqwest::Method::PUT,
        axum::http::Method::DELETE => reqwest::Method::DELETE,
        axum::http::Method::PATCH => reqwest::Method::PATCH,
        axum::http::Method::HEAD => reqwest::Method::HEAD,
        axum::http::Method::OPTIONS => reqwest::Method::OPTIONS,
        _ => reqwest::Method::GET,
    };

    let mut builder = state.client.request(method, &upstream_url);
    for (key, value) in &parts.headers {
        if key == "host" {
            continue;
        }
        if let Ok(v) = value.to_str() {
            builder = builder.header(key.as_str(), v);
        }
    }

    if !body_bytes.is_empty() {
        builder = builder.body(body_bytes);
    }

    let upstream_resp = match builder.send().await {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, upstream = %upstream_url, "upstream request failed");
            return (StatusCode::BAD_GATEWAY, "upstream request failed").into_response();
        }
    };

    let status = StatusCode::from_u16(upstream_resp.status().as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let mut response = Response::builder().status(status);
    for (key, value) in upstream_resp.headers() {
        if let (Ok(name), Ok(val)) = (
            HeaderName::try_from(key.as_str()),
            HeaderValue::from_bytes(value.as_bytes()),
        ) {
            response = response.header(name, val);
        }
    }

    let resp_body = match upstream_resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            error!(error = %e, "failed to read upstream response");
            return (StatusCode::BAD_GATEWAY, "failed to read upstream").into_response();
        }
    };

    response
        .body(Body::from(resp_body))
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response())
}
