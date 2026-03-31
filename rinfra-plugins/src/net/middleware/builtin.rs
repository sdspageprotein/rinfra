use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use metrics_exporter_prometheus::PrometheusHandle;
use rinfra_core::config::{CorsConfig, MiddlewareConfig, MetricsConfig};
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::net::middleware::HttpMiddleware;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use super::{AuditState, AuthState, I18nErrorState, RateLimitState, RequestIdLayer};

fn downcast_router(router: Box<dyn Any>) -> Result<Router, AppError> {
    router.downcast::<Router>().map(|r| *r).map_err(|_| {
        AppError::new(
            ErrorCode::Internal,
            "middleware: expected axum::Router".to_string(),
        )
    })
}

// ── Metrics ──────────────────────────────────────────────────────────

pub struct MetricsHttpMiddleware {
    handle: PrometheusHandle,
    endpoint: String,
}

impl MetricsHttpMiddleware {
    pub fn new(handle: PrometheusHandle, endpoint: String) -> Self {
        Self { handle, endpoint }
    }
}

impl HttpMiddleware for MetricsHttpMiddleware {
    fn name(&self) -> &str {
        "metrics"
    }
    fn order(&self) -> i32 {
        10
    }
    fn apply(&self, router: Box<dyn Any>) -> Result<Box<dyn Any>, AppError> {
        let router = downcast_router(router)?;
        let metrics_route =
            crate::metrics::metrics_router(self.handle.clone(), &self.endpoint);
        let router = router
            .merge(metrics_route)
            .layer(axum::middleware::from_fn(
                crate::metrics::http_metrics_middleware,
            ));
        Ok(Box::new(router))
    }
}

// ── Auth ─────────────────────────────────────────────────────────────

pub struct AuthHttpMiddleware {
    state: Arc<AuthState>,
}

impl AuthHttpMiddleware {
    pub fn new(state: Arc<AuthState>) -> Self {
        Self { state }
    }
}

impl HttpMiddleware for AuthHttpMiddleware {
    fn name(&self) -> &str {
        "auth"
    }
    fn order(&self) -> i32 {
        20
    }
    fn apply(&self, router: Box<dyn Any>) -> Result<Box<dyn Any>, AppError> {
        let router = downcast_router(router)?;
        let router = router.layer(axum::middleware::from_fn_with_state(
            self.state.clone(),
            super::auth_middleware,
        ));
        Ok(Box::new(router))
    }
}

// ── Rate Limit ───────────────────────────────────────────────────────

pub struct RateLimitHttpMiddleware {
    state: Arc<RateLimitState>,
}

impl RateLimitHttpMiddleware {
    pub fn new(state: Arc<RateLimitState>) -> Self {
        Self { state }
    }
}

impl HttpMiddleware for RateLimitHttpMiddleware {
    fn name(&self) -> &str {
        "rate_limit"
    }
    fn order(&self) -> i32 {
        25
    }
    fn apply(&self, router: Box<dyn Any>) -> Result<Box<dyn Any>, AppError> {
        let router = downcast_router(router)?;
        let router = router.layer(axum::middleware::from_fn_with_state(
            self.state.clone(),
            super::rate_limit_middleware,
        ));
        Ok(Box::new(router))
    }
}

// ── Trace ────────────────────────────────────────────────────────────

pub struct TraceHttpMiddleware;

impl HttpMiddleware for TraceHttpMiddleware {
    fn name(&self) -> &str {
        "trace"
    }
    fn order(&self) -> i32 {
        30
    }
    fn apply(&self, router: Box<dyn Any>) -> Result<Box<dyn Any>, AppError> {
        let router = downcast_router(router)?;
        Ok(Box::new(router.layer(TraceLayer::new_for_http())))
    }
}

// ── OTel Propagation ─────────────────────────────────────────────────

#[cfg(feature = "telemetry")]
pub struct OtelHttpMiddleware;

#[cfg(feature = "telemetry")]
impl HttpMiddleware for OtelHttpMiddleware {
    fn name(&self) -> &str {
        "otel"
    }
    fn order(&self) -> i32 {
        35
    }
    fn apply(&self, router: Box<dyn Any>) -> Result<Box<dyn Any>, AppError> {
        let router = downcast_router(router)?;
        Ok(Box::new(router.layer(super::OtelPropagationLayer)))
    }
}

// ── Request ID ───────────────────────────────────────────────────────

pub struct RequestIdHttpMiddleware;

impl HttpMiddleware for RequestIdHttpMiddleware {
    fn name(&self) -> &str {
        "request_id"
    }
    fn order(&self) -> i32 {
        40
    }
    fn apply(&self, router: Box<dyn Any>) -> Result<Box<dyn Any>, AppError> {
        let router = downcast_router(router)?;
        Ok(Box::new(router.layer(RequestIdLayer)))
    }
}

// ── Timeout ──────────────────────────────────────────────────────────

pub struct TimeoutHttpMiddleware {
    timeout_secs: u64,
}

impl TimeoutHttpMiddleware {
    pub fn new(timeout_secs: u64) -> Self {
        Self { timeout_secs }
    }
}

impl HttpMiddleware for TimeoutHttpMiddleware {
    fn name(&self) -> &str {
        "timeout"
    }
    fn order(&self) -> i32 {
        50
    }
    fn apply(&self, router: Box<dyn Any>) -> Result<Box<dyn Any>, AppError> {
        let router = downcast_router(router)?;
        let router = router.layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Duration::from_secs(self.timeout_secs),
        ));
        Ok(Box::new(router))
    }
}

// ── Audit ────────────────────────────────────────────────────────────

pub struct AuditHttpMiddleware {
    state: AuditState,
}

impl AuditHttpMiddleware {
    pub fn new(state: AuditState) -> Self {
        Self { state }
    }
}

impl HttpMiddleware for AuditHttpMiddleware {
    fn name(&self) -> &str {
        "audit"
    }
    fn order(&self) -> i32 {
        55
    }
    fn apply(&self, router: Box<dyn Any>) -> Result<Box<dyn Any>, AppError> {
        let router = downcast_router(router)?;
        let router = router.layer(axum::middleware::from_fn_with_state(
            self.state.clone(),
            super::audit_middleware,
        ));
        Ok(Box::new(router))
    }
}

// ── i18n Error Translation ──────────────────────────────────────────

pub struct I18nErrorHttpMiddleware {
    state: I18nErrorState,
}

impl I18nErrorHttpMiddleware {
    pub fn new(state: I18nErrorState) -> Self {
        Self { state }
    }
}

impl HttpMiddleware for I18nErrorHttpMiddleware {
    fn name(&self) -> &str {
        "i18n_error"
    }
    fn order(&self) -> i32 {
        8
    }
    fn apply(&self, router: Box<dyn Any>) -> Result<Box<dyn Any>, AppError> {
        let router = downcast_router(router)?;
        let router = router.layer(axum::middleware::from_fn_with_state(
            self.state.clone(),
            super::i18n_error_middleware,
        ));
        Ok(Box::new(router))
    }
}

// ── CORS ─────────────────────────────────────────────────────────────

pub struct CorsHttpMiddleware {
    config: CorsConfig,
}

impl CorsHttpMiddleware {
    pub fn new(config: CorsConfig) -> Self {
        Self { config }
    }
}

impl HttpMiddleware for CorsHttpMiddleware {
    fn name(&self) -> &str {
        "cors"
    }
    fn order(&self) -> i32 {
        60
    }
    fn apply(&self, router: Box<dyn Any>) -> Result<Box<dyn Any>, AppError> {
        let router = downcast_router(router)?;
        let layer = build_cors_layer(&self.config);
        Ok(Box::new(router.layer(layer)))
    }
}

fn build_cors_layer(config: &CorsConfig) -> CorsLayer {
    let allow_origin = if config.allow_origins.len() == 1 && config.allow_origins[0] == "*" {
        AllowOrigin::any()
    } else {
        let origins: Vec<_> = config
            .allow_origins
            .iter()
            .filter_map(|s| axum::http::HeaderValue::from_str(s).ok())
            .collect();
        AllowOrigin::list(origins)
    };
    CorsLayer::new().allow_origin(allow_origin)
}

// ── Factory ──────────────────────────────────────────────────────────

/// Build a middleware registry from config, populating all enabled builtin middleware.
///
/// `metrics_handle`: pre-initialized prometheus handle (pass `None` to skip metrics middleware).
pub fn builtin_http_middlewares(
    middleware_config: &MiddlewareConfig,
    metrics_config: &MetricsConfig,
    metrics_handle: Option<PrometheusHandle>,
    telemetry_enabled: bool,
    app_state: &Arc<rinfra_core::appstate::AppState>,
) -> Vec<Arc<dyn HttpMiddleware>> {
    let mut mws: Vec<Arc<dyn HttpMiddleware>> = Vec::new();

    if let Some(handle) = metrics_handle
        && metrics_config.enabled
    {
        mws.push(Arc::new(MetricsHttpMiddleware::new(
            handle,
            metrics_config.endpoint.clone(),
        )));
    }

    if middleware_config.auth.enabled {
        let secret =
            std::env::var(&middleware_config.auth.jwt_secret_env).unwrap_or_else(|_| {
                tracing::warn!(
                    env = %middleware_config.auth.jwt_secret_env,
                    "JWT secret env not set"
                );
                String::new()
            });
        mws.push(Arc::new(AuthHttpMiddleware::new(Arc::new(AuthState {
            secret,
            exclude_paths: middleware_config.auth.exclude_paths.clone(),
        }))));
    }

    if middleware_config.rate_limit.enabled
        && let Some(limiter) = app_state.ratelimiter()
    {
        mws.push(Arc::new(RateLimitHttpMiddleware::new(Arc::new(
            RateLimitState {
                limiter: limiter.clone(),
                key_strategy: middleware_config.rate_limit.key_strategy.clone(),
            },
        ))));
    }

    mws.push(Arc::new(TraceHttpMiddleware));

    #[cfg(feature = "telemetry")]
    if telemetry_enabled {
        mws.push(Arc::new(OtelHttpMiddleware));
    }
    #[cfg(not(feature = "telemetry"))]
    let _ = telemetry_enabled;

    if middleware_config.request_id.enabled {
        mws.push(Arc::new(RequestIdHttpMiddleware));
    }

    if middleware_config.timeout.enabled {
        mws.push(Arc::new(TimeoutHttpMiddleware::new(
            middleware_config.timeout.timeout_secs,
        )));
    }

    if middleware_config.cors.enabled {
        mws.push(Arc::new(CorsHttpMiddleware::new(
            middleware_config.cors.clone(),
        )));
    }

    if let Some(audit) = app_state.audit_logger() {
        mws.push(Arc::new(AuditHttpMiddleware::new(audit.clone())));
    }

    if let Some(i18n) = app_state.i18n() {
        mws.push(Arc::new(I18nErrorHttpMiddleware::new(i18n.clone())));
    }

    mws
}
