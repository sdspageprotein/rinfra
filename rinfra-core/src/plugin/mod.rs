mod context;
mod manifest;
mod registry;

pub use context::PluginContext;
pub use manifest::{HealthCheckResult, HealthStatus, PluginManifest};
pub use registry::PluginRegistry;

use crate::error::AppError;
use async_trait::async_trait;

/// Core plugin trait. Plugins create and register components during `build`.
#[async_trait]
pub trait Plugin: Send + Sync + 'static {
    fn manifest(&self) -> &PluginManifest;

    /// Build phase: create components and register them into `ctx`.
    /// The plugin reads its own config from `ctx.config()`.
    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError>;

    /// Cleanup phase: release external connections, flush buffers, etc.
    async fn shutdown(&self) -> Result<(), AppError> {
        Ok(())
    }
}

/// A named component that can report its health status asynchronously.
///
/// Plugins register `Arc<dyn HealthCheckable>` during `build()`; the health
/// endpoint iterates over all registered checkers at request time.
#[async_trait]
pub trait HealthCheckable: Send + Sync + 'static {
    /// Short identifier shown in the readiness response (e.g. "store.postgres", "cache.redis").
    fn name(&self) -> &str;

    /// Probe the component and report current health.
    async fn check(&self) -> HealthCheckResult;
}
