use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use rinfra_core::audit::{AuditEvent, AuditLogger, AuditOutcome};

pub type AuditState = Arc<dyn AuditLogger>;

pub async fn audit_middleware(
    State(logger): State<AuditState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let method = req.method().to_string();
    let uri = req.uri().path().to_string();
    let client_ip = extract_client_ip(&req);
    let start = Instant::now();

    let response = next.run(req).await;

    let status = response.status().as_u16();
    let duration_ms = start.elapsed().as_millis() as u64;
    let outcome = if response.status().is_success() {
        AuditOutcome::Success
    } else if status == 401 || status == 403 {
        AuditOutcome::Denied
    } else {
        AuditOutcome::Failure
    };

    let mut event = AuditEvent::new(
        client_ip.as_deref().unwrap_or("unknown"),
        format!("http.{}", method.to_lowercase()),
        "http",
        outcome,
    )
    .resource_id(&uri)
    .details(serde_json::json!({
        "method": method,
        "path": uri,
        "status": status,
        "duration_ms": duration_ms,
    }));

    if let Some(ip) = client_ip {
        event = event.ip(ip);
    }

    if let Err(e) = logger.log(event).await {
        tracing::warn!(error = %e, "failed to write HTTP audit event");
    }

    response
}

fn extract_client_ip(req: &Request<Body>) -> Option<String> {
    req.headers()
        .get("x-forwarded-for")
        .or_else(|| req.headers().get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
}
