use std::sync::Arc;

use async_trait::async_trait;
use rinfra_core::audit::{AuditEvent, AuditLogger, AuditOutcome};
use rinfra_core::error::AppError;
use rinfra_core::net::tcp::{TcpContext, TcpMiddleware};

/// Logs TCP connect/disconnect events to the audit trail.
pub struct AuditTcpMiddleware {
    logger: Arc<dyn AuditLogger>,
}

impl AuditTcpMiddleware {
    pub fn new(logger: Arc<dyn AuditLogger>) -> Self {
        Self { logger }
    }
}

#[async_trait]
impl TcpMiddleware for AuditTcpMiddleware {
    fn name(&self) -> &str {
        "audit"
    }
    fn order(&self) -> i32 {
        50
    }

    async fn on_connect(&self, ctx: &TcpContext) -> Result<(), AppError> {
        let event = AuditEvent::new(
            ctx.peer.to_string(),
            "tcp.connect",
            "tcp",
            AuditOutcome::Success,
        )
        .ip(ctx.peer.ip().to_string())
        .details(serde_json::json!({ "listener": ctx.listener_name }));

        if let Err(e) = self.logger.log(event).await {
            tracing::warn!(error = %e, "failed to audit tcp connect");
        }
        Ok(())
    }

    async fn on_disconnect(&self, ctx: &TcpContext) {
        let event = AuditEvent::new(
            ctx.peer.to_string(),
            "tcp.disconnect",
            "tcp",
            AuditOutcome::Success,
        )
        .ip(ctx.peer.ip().to_string())
        .details(serde_json::json!({ "listener": ctx.listener_name }));

        if let Err(e) = self.logger.log(event).await {
            tracing::warn!(error = %e, "failed to audit tcp disconnect");
        }
    }
}
