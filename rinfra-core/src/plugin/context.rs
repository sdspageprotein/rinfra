use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::appstate::{AppState, HealthCheckerRegistry};
use crate::cache::Cache;
use crate::cluster::NodeRegistry;
use crate::codec::{Codec, CodecRegistry};
use crate::config::RinfraConfig;
use crate::error::AppError;
use crate::mq::MessageBus;
use crate::plugin::HealthCheckable;
use crate::ratelimit::RateLimiter;
use crate::store::{DbConnection, Store};

use super::PluginManifest;

pub type ShutdownHookFn =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = Result<(), AppError>> + Send>> + Send>;

/// Build-phase context passed to each plugin's `build()` method.
/// Plugins register components here; the runtime later converts
/// them into `AppState`.
pub struct PluginContext {
    config: RinfraConfig,
    pub(crate) manifests: Vec<PluginManifest>,
    pub(crate) shutdown_hooks: Vec<ShutdownHookFn>,
    codecs: CodecRegistry,
    routers: Vec<Box<dyn Any + Send>>,
    extensions: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl PluginContext {
    pub fn new(config: RinfraConfig) -> Self {
        Self {
            config,
            manifests: Vec::new(),
            shutdown_hooks: Vec::new(),
            codecs: CodecRegistry::new(),
            routers: Vec::new(),
            extensions: HashMap::new(),
        }
    }

    pub fn config(&self) -> &RinfraConfig {
        &self.config
    }

    // --- generic type-map ---

    pub fn set<T: Send + Sync + 'static>(&mut self, val: T) {
        self.extensions.insert(TypeId::of::<T>(), Box::new(val));
    }

    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.extensions
            .get(&TypeId::of::<T>())
            .and_then(|b| b.downcast_ref::<T>())
    }

    pub fn get_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T> {
        self.extensions
            .get_mut(&TypeId::of::<T>())
            .and_then(|b| b.downcast_mut::<T>())
    }

    // --- well-known convenience setters/getters ---

    pub fn set_cache(&mut self, cache: Arc<dyn Cache>) {
        self.set(cache);
    }

    pub fn cache(&self) -> Option<&Arc<dyn Cache>> {
        self.get::<Arc<dyn Cache>>()
    }

    pub fn set_store(&mut self, store: Arc<dyn Store>) {
        self.set(store);
    }

    pub fn set_message_bus(&mut self, bus: Arc<dyn MessageBus>) {
        self.set(bus);
    }

    pub fn set_ratelimiter(&mut self, limiter: Arc<dyn RateLimiter>) {
        self.set(limiter);
    }

    pub fn set_node_registry(&mut self, registry: Arc<dyn NodeRegistry>) {
        self.set(registry);
    }

    pub fn set_db(&mut self, db: Arc<dyn DbConnection>) {
        self.set(db);
    }

    // --- health checkers ---

    pub fn add_health_checker(&mut self, checker: Arc<dyn HealthCheckable>) {
        let registry = self
            .get_mut::<HealthCheckerRegistry>()
            .map(|r| {
                r.register(checker.clone());
            });
        if registry.is_none() {
            let mut reg = HealthCheckerRegistry::new();
            reg.register(checker);
            self.set(reg);
        }
    }

    // --- codecs ---

    pub fn add_codec(&mut self, codec: Box<dyn Codec>) -> Result<(), AppError> {
        self.codecs.register(codec)
    }

    // --- routers (framework-agnostic via type erasure) ---

    /// Register a framework-specific router (e.g. `axum::Router`).
    /// The runtime downcasts to the expected concrete type.
    pub fn add_router<R: Any + Send + 'static>(&mut self, router: R) {
        self.routers.push(Box::new(router));
    }

    // --- shutdown hooks ---

    pub fn add_shutdown_hook<F, Fut>(&mut self, f: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), AppError>> + Send + 'static,
    {
        self.shutdown_hooks.push(Box::new(move || Box::pin(f())));
    }

    /// Consume the context and produce `(AppState, Vec<Box<dyn Any + Send>>)`.
    pub fn into_app_parts(mut self) -> (AppState, Vec<Box<dyn Any + Send>>) {
        let mut state = AppState::new(self.config);

        if !self.codecs.list_names().is_empty() {
            state.set(self.codecs);
        }

        for (k, v) in self.extensions.drain() {
            state.extensions_mut().insert(k, v);
        }

        (state, self.routers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_config_access() {
        let mut config = RinfraConfig::default();
        config.app.name = "test".to_string();
        let ctx = PluginContext::new(config);
        assert_eq!(ctx.config().app.name, "test");
    }

    #[test]
    fn test_context_extension_roundtrip() {
        let mut ctx = PluginContext::new(RinfraConfig::default());
        ctx.set::<String>("hello".to_string());
        assert_eq!(ctx.get::<String>(), Some(&"hello".to_string()));
    }

    #[test]
    fn test_context_extension_missing() {
        let ctx = PluginContext::new(RinfraConfig::default());
        assert!(ctx.get::<u64>().is_none());
    }

    #[test]
    fn test_context_defaults_are_empty() {
        let ctx = PluginContext::new(RinfraConfig::default());
        assert!(ctx.cache().is_none());
        assert!(ctx.routers.is_empty());
        assert!(ctx.shutdown_hooks.is_empty());
    }

    #[test]
    fn test_into_app_parts_transfers_extensions() {
        let mut ctx = PluginContext::new(RinfraConfig::default());
        ctx.set::<u64>(42);
        let (state, _routers) = ctx.into_app_parts();
        assert_eq!(state.get::<u64>(), Some(&42));
    }
}
