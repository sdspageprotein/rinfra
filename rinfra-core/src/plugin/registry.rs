use std::time::Duration;

use tracing::{error, info, warn};

use super::{Plugin, PluginContext, PluginManifest};
use crate::error::{AppError, ErrorCode};

use super::context::ShutdownHookFn;

pub struct PluginRegistry {
    manifests: Vec<PluginManifest>,
    shutdown_hooks: Vec<ShutdownHookFn>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            manifests: Vec::new(),
            shutdown_hooks: Vec::new(),
        }
    }

    /// Run all registered plugins' `build()` against the given context,
    /// then absorb shutdown hooks and manifests from the context.
    pub async fn build_all(
        &mut self,
        plugins: &[Box<dyn Plugin>],
        ctx: &mut PluginContext,
    ) -> Result<(), AppError> {
        info!(count = plugins.len(), "building plugins");
        for plugin in plugins {
            let name = &plugin.manifest().name;
            info!(plugin = %name, "building plugin");
            plugin.build(ctx).await.map_err(|e| {
                error!(plugin = %name, error = %e, "plugin build failed");
                AppError::new(
                    ErrorCode::PluginInitFailed,
                    format!("plugin '{name}' build failed: {e}"),
                )
            })?;
            ctx.manifests.push(plugin.manifest().clone());
        }

        self.manifests = std::mem::take(&mut ctx.manifests);
        self.shutdown_hooks = std::mem::take(&mut ctx.shutdown_hooks);

        Ok(())
    }

    pub fn list(&self) -> &[PluginManifest] {
        &self.manifests
    }

    pub fn get(&self, name: &str) -> Option<&PluginManifest> {
        self.manifests.iter().find(|m| m.name == name)
    }

    pub fn len(&self) -> usize {
        self.manifests.len()
    }

    pub fn is_empty(&self) -> bool {
        self.manifests.is_empty()
    }

    pub async fn shutdown_all(&mut self, deadline: Duration) -> Result<(), AppError> {
        let hooks: Vec<_> = self.shutdown_hooks.drain(..).rev().collect();
        info!(count = hooks.len(), "running shutdown hooks");
        for hook in hooks {
            match tokio::time::timeout(deadline, hook()).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    error!(error = %e, "shutdown hook failed");
                    return Err(AppError::new(
                        ErrorCode::PluginShutdownFailed,
                        format!("shutdown hook failed: {e}"),
                    ));
                }
                Err(_) => {
                    warn!("shutdown hook timed out");
                    return Err(AppError::new(
                        ErrorCode::PluginShutdownTimeout,
                        "shutdown hook timed out",
                    ));
                }
            }
        }
        Ok(())
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RinfraConfig;
    use crate::plugin::Plugin;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    struct DummyPlugin {
        manifest: PluginManifest,
        build_called: Arc<AtomicBool>,
        should_fail: bool,
    }

    impl DummyPlugin {
        fn new(name: &str) -> Self {
            Self {
                manifest: PluginManifest::new(name, "0.1.0", "test"),
                build_called: Arc::new(AtomicBool::new(false)),
                should_fail: false,
            }
        }

        fn failing(name: &str) -> Self {
            Self {
                should_fail: true,
                ..Self::new(name)
            }
        }
    }

    #[async_trait]
    impl Plugin for DummyPlugin {
        fn manifest(&self) -> &PluginManifest {
            &self.manifest
        }

        async fn build(&self, _ctx: &mut PluginContext) -> Result<(), AppError> {
            if self.should_fail {
                return Err(AppError::new(ErrorCode::Internal, "forced failure"));
            }
            self.build_called.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_build_all_calls_build() {
        let mut reg = PluginRegistry::new();
        let p = DummyPlugin::new("alpha");
        let flag = p.build_called.clone();
        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(p)];
        let mut ctx = PluginContext::new(RinfraConfig::default());
        reg.build_all(&plugins, &mut ctx).await.unwrap();
        assert!(flag.load(Ordering::SeqCst));
        assert_eq!(reg.len(), 1);
        assert!(reg.get("alpha").is_some());
    }

    #[tokio::test]
    async fn test_build_all_returns_error_on_failure() {
        let mut reg = PluginRegistry::new();
        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(DummyPlugin::failing("bad"))];
        let mut ctx = PluginContext::new(RinfraConfig::default());
        let result = reg.build_all(&plugins, &mut ctx).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::PluginInitFailed);
    }

    #[tokio::test]
    async fn test_shutdown_all_runs_hooks() {
        let called = Arc::new(AtomicBool::new(false));
        let flag = called.clone();
        let mut reg = PluginRegistry::new();
        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(DummyPlugin::new("alpha"))];
        let mut ctx = PluginContext::new(RinfraConfig::default());
        ctx.add_shutdown_hook(move || {
            let flag = flag.clone();
            async move {
                flag.store(true, Ordering::SeqCst);
                Ok(())
            }
        });
        reg.build_all(&plugins, &mut ctx).await.unwrap();
        reg.shutdown_all(Duration::from_secs(5)).await.unwrap();
        assert!(called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_empty_registry() {
        let reg = PluginRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[tokio::test]
    async fn test_list_manifests() {
        let mut reg = PluginRegistry::new();
        let plugins: Vec<Box<dyn Plugin>> = vec![
            Box::new(DummyPlugin::new("a")),
            Box::new(DummyPlugin::new("b")),
        ];
        let mut ctx = PluginContext::new(RinfraConfig::default());
        reg.build_all(&plugins, &mut ctx).await.unwrap();
        assert_eq!(reg.list().len(), 2);
    }
}
