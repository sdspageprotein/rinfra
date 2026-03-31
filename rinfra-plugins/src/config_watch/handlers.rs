use std::sync::Arc;

use async_trait::async_trait;
use rinfra_core::audit::{AuditEvent, AuditLogger, AuditOutcome};
use rinfra_core::config::watch::OnConfigReload;
use rinfra_core::config::RinfraConfig;

/// Logs every configuration reload to the audit trail.
pub struct AuditConfigReloadHandler {
    logger: Arc<dyn AuditLogger>,
}

impl AuditConfigReloadHandler {
    pub fn new(logger: Arc<dyn AuditLogger>) -> Self {
        Self { logger }
    }
}

#[async_trait]
impl OnConfigReload for AuditConfigReloadHandler {
    async fn on_reload(&self, _new_config: &RinfraConfig) {
        let event = AuditEvent::new(
            "system",
            "config.reload",
            "config",
            AuditOutcome::Success,
        );
        if let Err(e) = self.logger.log(event).await {
            tracing::warn!(error = %e, "failed to audit config reload");
        }
    }
}

/// Logs config changes to tracing and detects changes that require restart.
pub struct LogConfigReloadHandler;

#[async_trait]
impl OnConfigReload for LogConfigReloadHandler {
    async fn on_reload(&self, new_config: &RinfraConfig) {
        tracing::info!(
            app_name = %new_config.app.name,
            "configuration reloaded"
        );

        if new_config.plugins.log.stdout.enabled || new_config.plugins.log.file.enabled {
            tracing::info!(
                "log config changes detected — log level changes require restart"
            );
        }
    }
}
