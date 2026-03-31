use std::sync::Arc;

use axum::extract::State;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post, delete};
use axum::{Extension, Json, Router};
use rinfra_core::appstate::AppState;
use rinfra_core::audit::{AuditEvent, AuditLogger, AuditOutcome};
use rinfra_core::config::RinfraConfig;
use rinfra_core::response::ApiResponse;
use serde::{Deserialize, Serialize};

use crate::auth::{AdminRole, AdminTokenInfo, AdminTokenStore};

pub fn admin_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/info", get(info_handler))
        .route("/health", get(health_handler))
        .route("/config", get(config_handler))
        .route("/plugins", get(plugins_handler))
        .route("/cluster/nodes", get(cluster_nodes_handler))
        .route("/cache/{key}", get(cache_get_handler))
        .route("/cache/flush", post(cache_flush_handler))
        .route("/metrics", get(metrics_handler))
}

#[derive(Debug, Serialize)]
struct SystemInfo {
    name: String,
    version: String,
    rust_version: String,
    os: String,
    arch: String,
}

async fn info_handler() -> Json<ApiResponse<SystemInfo>> {
    let info = SystemInfo {
        name: env!("CARGO_PKG_NAME").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        rust_version: rustc_version(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    };
    Json(ApiResponse::success(info))
}

fn rustc_version() -> String {
    option_env!("CARGO_PKG_RUST_VERSION")
        .unwrap_or("edition-2024")
        .to_string()
}

#[derive(Debug, Serialize)]
struct HealthStatus {
    status: String,
    uptime_secs: u64,
    components: ComponentHealth,
}

#[derive(Debug, Serialize)]
struct ComponentHealth {
    cache: bool,
    store: bool,
    message_bus: bool,
    ratelimiter: bool,
}

async fn health_handler(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<HealthStatus>> {
    Json(ApiResponse::success(HealthStatus {
        status: "healthy".to_string(),
        uptime_secs: state.uptime_secs(),
        components: ComponentHealth {
            cache: state.cache().is_some(),
            store: state.store().is_some(),
            message_bus: state.message_bus().is_some(),
            ratelimiter: state.ratelimiter().is_some(),
        },
    }))
}

#[derive(Debug, Serialize)]
struct ConfigSummary {
    app_name: String,
    app_version: String,
    listeners: Vec<ListenerSummary>,
    cluster_mode: String,
    cluster_role: String,
    plugins_enabled: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ListenerSummary {
    name: String,
    protocol: String,
    bind: String,
}

fn build_plugins_enabled(config: &RinfraConfig) -> Vec<String> {
    let mut enabled = Vec::new();
    if config.plugins.log.stdout.enabled {
        enabled.push("log.stdout".to_string());
    }
    if config.plugins.cache.memory.enabled {
        enabled.push("cache.memory".to_string());
    }
    if config.plugins.cache.redis.enabled {
        enabled.push("cache.redis".to_string());
    }
    if config.plugins.cache.multilevel.enabled {
        enabled.push("cache.multilevel".to_string());
    }
    for listener in &config.plugins.net.listeners {
        enabled.push(format!("net.{} ({})", listener.name, listener.bind));
    }
    if config.plugins.store.postgres.enabled {
        enabled.push("store.postgres".to_string());
    }
    if config.plugins.ratelimit.memory.enabled {
        enabled.push("ratelimit.memory".to_string());
    }
    if config.plugins.ratelimit.redis.enabled {
        enabled.push("ratelimit.redis".to_string());
    }
    if config.plugins.crypto.aesgcm.enabled {
        enabled.push("crypto.aesgcm".to_string());
    }
    if config.plugins.crypto.rotating.enabled {
        enabled.push("crypto.rotating".to_string());
    }
    if config.plugins.crypto.file.enabled {
        enabled.push("crypto.file".to_string());
    }
    match config.plugins.mq.backend {
        rinfra_core::config::MqBackend::Memory => enabled.push("mq.memory".to_string()),
        rinfra_core::config::MqBackend::Nats => enabled.push("mq.nats".to_string()),
        rinfra_core::config::MqBackend::RedisStreams => enabled.push("mq.redis_streams".to_string()),
        rinfra_core::config::MqBackend::None => {}
    }
    if config.plugins.script.wasm.enabled {
        enabled.push("script.wasm".to_string());
    }
    if config.plugins.cluster.is_cluster() {
        enabled.push("cluster".to_string());
    }
    enabled
}

async fn config_handler(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<ConfigSummary>> {
    let config = &state.config;
    let plugins_enabled = build_plugins_enabled(config);
    let listeners: Vec<ListenerSummary> = config
        .plugins
        .net
        .listeners
        .iter()
        .map(|l| ListenerSummary {
            name: l.name.clone(),
            protocol: format!("{:?}", l.protocol),
            bind: l.bind.clone(),
        })
        .collect();

    Json(ApiResponse::success(ConfigSummary {
        app_name: config.app.name.clone(),
        app_version: config.app.version.clone(),
        listeners,
        cluster_mode: config.plugins.cluster.mode.clone(),
        cluster_role: config.plugins.cluster.role.clone(),
        plugins_enabled,
    }))
}

#[derive(Debug, Serialize)]
struct PluginInfo {
    name: String,
    category: String,
    status: String,
}

async fn plugins_handler(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<Vec<PluginInfo>>> {
    let config = &state.config;
    let mut all = vec![
        ("log.stdout", "logging", config.plugins.log.stdout.enabled),
        ("store.postgres", "storage", config.plugins.store.postgres.enabled),
        ("cache.memory", "cache", config.plugins.cache.memory.enabled),
        ("cache.redis", "cache", config.plugins.cache.redis.enabled),
        ("cache.multilevel", "cache", config.plugins.cache.multilevel.enabled),
        ("ratelimit.memory", "ratelimit", config.plugins.ratelimit.memory.enabled),
        ("ratelimit.redis", "ratelimit", config.plugins.ratelimit.redis.enabled),
        ("crypto.aesgcm", "crypto", config.plugins.crypto.aesgcm.enabled),
        ("crypto.rotating", "crypto", config.plugins.crypto.rotating.enabled),
        ("crypto.file", "crypto", config.plugins.crypto.file.enabled),
        ("codec.json", "codec", true),
        ("codec.msgpack", "codec", true),
        ("codec.protobuf", "codec", true),
        ("mq.memory", "messaging", config.plugins.mq.backend == rinfra_core::config::MqBackend::Memory),
        ("script.wasm", "scripting", config.plugins.script.wasm.enabled),
        ("cluster", "cluster", config.plugins.cluster.is_cluster()),
    ];

    for listener in &config.plugins.net.listeners {
        all.push((&listener.name, "network", true));
    }

    let plugins: Vec<PluginInfo> = all
        .into_iter()
        .map(|(name, category, enabled)| PluginInfo {
            name: name.to_string(),
            category: category.to_string(),
            status: if enabled { "enabled" } else { "available" }.to_string(),
        })
        .collect();

    Json(ApiResponse::success(plugins))
}

#[derive(Debug, Serialize)]
struct ClusterNodeView {
    mode: String,
    nodes: Vec<ClusterNodeInfo>,
}

#[derive(Debug, Serialize)]
struct ClusterNodeInfo {
    id: String,
    role: String,
    status: String,
    endpoints: Vec<ClusterEndpointInfo>,
    metadata: std::collections::HashMap<String, String>,
}

#[derive(Debug, Serialize)]
struct ClusterEndpointInfo {
    protocol: String,
    address: String,
}

async fn cluster_nodes_handler(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<ClusterNodeView>> {
    let mode = state.config.plugins.cluster.mode.clone();

    let nodes = if let Some(registry) = state.node_registry() {
        match registry.list_nodes().await {
            Ok(list) => list
                .into_iter()
                .map(|n| ClusterNodeInfo {
                    id: n.id,
                    role: format!("{:?}", n.role),
                    status: format!("{:?}", n.status),
                    endpoints: n
                        .endpoints
                        .into_iter()
                        .map(|e| ClusterEndpointInfo {
                            protocol: e.protocol,
                            address: e.address,
                        })
                        .collect(),
                    metadata: n.metadata,
                })
                .collect(),
            Err(_) => vec![],
        }
    } else {
        vec![]
    };

    Json(ApiResponse::success(ClusterNodeView { mode, nodes }))
}

#[derive(Debug, Serialize)]
struct CacheGetResult {
    key: String,
    found: bool,
    value: Option<String>,
}

async fn cache_get_handler(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
) -> Json<ApiResponse<CacheGetResult>> {
    if let Some(cache) = state.cache() {
        match cache.get(&key).await {
            Ok(Some(bytes)) => {
                let value = String::from_utf8(bytes)
                    .unwrap_or_else(|e| format!("<binary {} bytes>", e.into_bytes().len()));
                Json(ApiResponse::success(CacheGetResult {
                    key,
                    found: true,
                    value: Some(value),
                }))
            }
            Ok(None) => Json(ApiResponse::success(CacheGetResult {
                key,
                found: false,
                value: None,
            })),
            Err(_) => Json(ApiResponse::success(CacheGetResult {
                key,
                found: false,
                value: None,
            })),
        }
    } else {
        Json(ApiResponse::success(CacheGetResult {
            key,
            found: false,
            value: None,
        }))
    }
}

#[derive(Debug, Serialize)]
struct CacheFlushResult {
    message: String,
}

async fn cache_flush_handler(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<CacheFlushResult>> {
    let msg = if state.cache().is_some() {
        "cache flush requested"
    } else {
        "no cache configured"
    };
    Json(ApiResponse::success(CacheFlushResult {
        message: msg.into(),
    }))
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct MetricEntry {
    name: String,
    #[serde(rename = "type")]
    metric_type: String,
    help: String,
    values: Vec<MetricValue>,
}

#[derive(Debug, Serialize)]
struct MetricValue {
    labels: std::collections::HashMap<String, String>,
    value: f64,
}

async fn metrics_handler(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<Vec<MetricEntry>>> {
    let handle = state.get::<metrics_exporter_prometheus::PrometheusHandle>();
    let Some(handle) = handle else {
        return Json(ApiResponse::success(vec![]));
    };

    let raw = handle.render();
    let entries = parse_prometheus_text(&raw);
    Json(ApiResponse::success(entries))
}

fn parse_prometheus_text(text: &str) -> Vec<MetricEntry> {
    let mut entries: Vec<MetricEntry> = Vec::new();
    let mut current_help = String::new();
    let mut current_type = String::new();
    let mut current_name = String::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("# HELP ") {
            let rest = &line[7..];
            if let Some(idx) = rest.find(' ') {
                current_name = rest[..idx].to_string();
                current_help = rest[idx + 1..].to_string();
            } else {
                current_name = rest.to_string();
                current_help.clear();
            }
        } else if line.starts_with("# TYPE ") {
            let rest = &line[7..];
            if let Some(idx) = rest.find(' ') {
                current_type = rest[idx + 1..].to_string();
            }
        } else if !line.starts_with('#') {
            let (metric_name, labels, value) = parse_metric_line(line);
            let base_name = metric_name
                .strip_suffix("_total")
                .or_else(|| metric_name.strip_suffix("_bucket"))
                .or_else(|| metric_name.strip_suffix("_sum"))
                .or_else(|| metric_name.strip_suffix("_count"))
                .unwrap_or(&metric_name);

            let lookup_name = if base_name == current_name || metric_name == current_name {
                current_name.clone()
            } else {
                base_name.to_string()
            };

            let entry = if let Some(e) = entries.iter_mut().find(|e| e.name == lookup_name) {
                e
            } else {
                entries.push(MetricEntry {
                    name: lookup_name.clone(),
                    metric_type: if lookup_name == current_name {
                        current_type.clone()
                    } else {
                        "unknown".to_string()
                    },
                    help: if lookup_name == current_name {
                        current_help.clone()
                    } else {
                        String::new()
                    },
                    values: Vec::new(),
                });
                entries.last_mut().expect("just pushed an entry")
            };

            entry.values.push(MetricValue { labels, value });
        }
    }
    entries
}

fn parse_metric_line(line: &str) -> (String, std::collections::HashMap<String, String>, f64) {
    let mut labels = std::collections::HashMap::new();

    if let Some(brace_start) = line.find('{') {
        let name = line[..brace_start].to_string();
        let brace_end = line.find('}').unwrap_or(line.len());
        let label_str = &line[brace_start + 1..brace_end];

        for pair in label_str.split(',') {
            let pair = pair.trim();
            if let Some(eq) = pair.find('=') {
                let k = pair[..eq].trim().to_string();
                let v = pair[eq + 1..].trim().trim_matches('"').to_string();
                labels.insert(k, v);
            }
        }

        let value_str = line[brace_end + 1..].trim();
        let value = value_str.parse::<f64>().unwrap_or(0.0);
        (name, labels, value)
    } else {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let name = parts.first().unwrap_or(&"").to_string();
        let value = parts.get(1).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
        (name, labels, value)
    }
}

// ---------------------------------------------------------------------------
// Token management routes (root-only)
// ---------------------------------------------------------------------------

type TokenState = (Arc<AdminTokenStore>, Option<Arc<dyn AuditLogger>>);

pub(crate) fn token_management_routes(
    store: Arc<AdminTokenStore>,
    audit: Option<Arc<dyn AuditLogger>>,
) -> Router<Arc<AppState>> {
    Router::new()
        .route("/tokens", get(list_tokens_handler).post(create_token_handler))
        .route("/tokens/{id}", delete(delete_token_handler))
        .route("/tokens/root/rotate", post(rotate_root_handler))
        .with_state((store, audit))
        .into()
}

async fn audit_token_op(
    state: &TokenState,
    actor: &str,
    action: &str,
    target_id: &str,
) {
    if let Some(ref logger) = state.1 {
        let event = AuditEvent::new(actor, action, "admin_token", AuditOutcome::Success)
            .resource_id(target_id);
        if let Err(e) = logger.log(event).await {
            tracing::warn!(error = %e, "failed to write audit log");
        }
    }
}

fn require_root(info: &Option<Extension<AdminTokenInfo>>) -> Result<&AdminTokenInfo, impl IntoResponse> {
    match info {
        Some(Extension(info)) if info.role == AdminRole::Root => Ok(info),
        Some(_) => Err((
            StatusCode::FORBIDDEN,
            Json(ApiResponse::<()> {
                code: "FORBIDDEN".to_string(),
                data: None,
                message: "root role required".to_string(),
            }),
        )),
        None => Err((
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse::<()> {
                code: "UNAUTHORIZED".to_string(),
                data: None,
                message: "admin token required".to_string(),
            }),
        )),
    }
}

async fn list_tokens_handler(
    info: Option<Extension<AdminTokenInfo>>,
    State((store, _audit)): State<TokenState>,
) -> impl IntoResponse {
    if let Err(e) = require_root(&info) {
        return e.into_response();
    }
    let tokens = store.list();
    Json(ApiResponse::success(tokens)).into_response()
}

#[derive(Debug, Deserialize)]
struct CreateTokenRequest {
    label: String,
    #[serde(default)]
    expires_in_days: Option<u64>,
}

#[derive(Debug, Serialize)]
struct CreateTokenResponse {
    token: String,
    info: AdminTokenInfo,
}

async fn create_token_handler(
    info: Option<Extension<AdminTokenInfo>>,
    State(state): State<TokenState>,
    Json(body): Json<CreateTokenRequest>,
) -> impl IntoResponse {
    let root_info = match require_root(&info) {
        Ok(i) => i,
        Err(e) => return e.into_response(),
    };

    let expires_at = body.expires_in_days.map(|days| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        now + days * 24 * 60 * 60 * 1000
    });

    match state.0.create_token(AdminRole::Admin, body.label.clone(), expires_at) {
        Ok((token, token_info)) => {
            audit_token_op(&state, &root_info.label, "token.create", &token_info.id).await;
            Json(ApiResponse::success(CreateTokenResponse {
                token,
                info: token_info,
            }))
            .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()> {
                code: "INTERNAL".to_string(),
                data: None,
                message: e,
            }),
        )
            .into_response(),
    }
}

async fn delete_token_handler(
    info: Option<Extension<AdminTokenInfo>>,
    State(state): State<TokenState>,
    Path(token_id): Path<String>,
) -> impl IntoResponse {
    let root_info = match require_root(&info) {
        Ok(i) => i,
        Err(e) => return e.into_response(),
    };

    if token_id == root_info.id {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<()> {
                code: "BAD_REQUEST".to_string(),
                data: None,
                message: "cannot delete your own root token — use rotate instead".to_string(),
            }),
        )
            .into_response();
    }

    match state.0.delete(&token_id) {
        Ok(true) => {
            audit_token_op(&state, &root_info.label, "token.delete", &token_id).await;
            Json(ApiResponse::success(serde_json::json!({
                "deleted": token_id
            })))
            .into_response()
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()> {
                code: "NOT_FOUND".to_string(),
                data: None,
                message: format!("token {token_id} not found"),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()> {
                code: "INTERNAL".to_string(),
                data: None,
                message: e,
            }),
        )
            .into_response(),
    }
}

#[derive(Debug, Serialize)]
struct RotateResponse {
    token: String,
    info: AdminTokenInfo,
    message: String,
}

async fn rotate_root_handler(
    info: Option<Extension<AdminTokenInfo>>,
    State(state): State<TokenState>,
) -> impl IntoResponse {
    if let Err(e) = require_root(&info) {
        return e.into_response();
    }

    if let Err(e) = state.0.delete_root() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()> {
                code: "INTERNAL".to_string(),
                data: None,
                message: e,
            }),
        )
            .into_response();
    }

    match state.0.create_token(AdminRole::Root, "root".into(), None) {
        Ok((token, token_info)) => {
            audit_token_op(&state, "root", "token.rotate", &token_info.id).await;
            Json(ApiResponse::success(RotateResponse {
                token,
                info: token_info,
                message: "root token rotated — old token is now invalid".to_string(),
            }))
            .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()> {
                code: "INTERNAL".to_string(),
                data: None,
                message: e,
            }),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::util::ServiceExt;

    fn test_state() -> Arc<AppState> {
        Arc::new(AppState::new(RinfraConfig::default()))
    }

    fn app() -> Router {
        Router::new()
            .nest("/api/admin", admin_routes())
            .with_state(test_state())
    }

    #[tokio::test]
    async fn test_info_endpoint() {
        let response = app()
            .oneshot(
                Request::builder()
                    .uri("/api/admin/info")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "OK");
        assert!(json["data"]["name"].is_string());
    }

    #[tokio::test]
    async fn test_health_endpoint_has_uptime() {
        let response = app()
            .oneshot(
                Request::builder()
                    .uri("/api/admin/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["data"]["status"], "healthy");
        assert!(json["data"]["uptime_secs"].is_number());
    }

    #[tokio::test]
    async fn test_config_endpoint_uses_real_config() {
        let mut config = RinfraConfig::default();
        config.app.name = "test-real-config".to_string();
        config.plugins.net.listeners.push(rinfra_core::config::ListenerConfig {
            name: "main".into(),
            protocol: rinfra_core::config::ListenerProtocol::Http,
            bind: "0.0.0.0:9999".into(),
            http: None,
            tcp: None,
        });
        let state = Arc::new(AppState::new(config));
        let app = Router::new()
            .nest("/api/admin", admin_routes())
            .with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/admin/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["data"]["app_name"], "test-real-config");
        assert_eq!(json["data"]["listeners"][0]["bind"], "0.0.0.0:9999");
    }

    #[tokio::test]
    async fn test_plugins_endpoint() {
        let response = app()
            .oneshot(
                Request::builder()
                    .uri("/api/admin/plugins")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "OK");
        assert!(json["data"].is_array());
        assert!(!json["data"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_cluster_nodes_standalone() {
        let response = app()
            .oneshot(
                Request::builder()
                    .uri("/api/admin/cluster/nodes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["data"]["mode"], "standalone");
        assert!(json["data"]["nodes"].as_array().unwrap().is_empty());
    }
}
