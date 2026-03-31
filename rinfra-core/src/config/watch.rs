use std::sync::Arc;

use async_trait::async_trait;

use crate::config::RinfraConfig;
use crate::error::AppError;

/// Callback invoked when the configuration file changes.
#[async_trait]
pub trait OnConfigReload: Send + Sync + 'static {
    async fn on_reload(&self, new_config: &RinfraConfig);
}

/// Watches a configuration source and notifies registered handlers.
#[async_trait]
pub trait ConfigWatcher: Send + Sync + 'static {
    fn watcher_name(&self) -> &str;

    /// Register a handler that is called on every config change.
    fn add_handler(&self, handler: Arc<dyn OnConfigReload>);

    /// Start watching (non-blocking; spawns internal task).
    async fn start(&self) -> Result<(), AppError>;

    /// Stop watching and release resources.
    async fn stop(&self) -> Result<(), AppError>;
}
