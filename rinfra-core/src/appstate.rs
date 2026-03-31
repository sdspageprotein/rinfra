use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use crate::audit::AuditLogger;
use crate::cache::Cache;
use crate::cluster::NodeRegistry;
use crate::codec::CodecRegistry;
use crate::config::RinfraConfig;
use crate::crypto::Crypto;
use crate::config::watch::ConfigWatcher;
use crate::file_store::FileStore;
use crate::http_client::HttpClient;
use crate::i18n::I18n;
use crate::lock::DistributedLock;
use crate::mq::MessageBus;
use crate::plugin::HealthCheckable;
use crate::ratelimit::RateLimiter;
use crate::script::{ScriptEngine, ScriptEngineRegistry};
use crate::store::{DbConnection, Store, StoreRegistry};

/// Registry holding all [`HealthCheckable`] probes registered by plugins.
#[derive(Default)]
pub struct HealthCheckerRegistry {
    checkers: Vec<Arc<dyn HealthCheckable>>,
}

impl HealthCheckerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, checker: Arc<dyn HealthCheckable>) {
        self.checkers.push(checker);
    }

    pub fn checkers(&self) -> &[Arc<dyn HealthCheckable>] {
        &self.checkers
    }

    pub fn is_empty(&self) -> bool {
        self.checkers.is_empty()
    }
}

/// Shared application state accessible by all routers and handlers.
///
/// All components are stored in a unified type-map. Use convenience
/// accessors (`cache()`, `store()`, …) for well-known types, or the
/// generic `get::<T>()` / `set::<T>()` for arbitrary extensions.
pub struct AppState {
    pub config: Arc<RinfraConfig>,
    pub started_at: Instant,
    extensions: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl AppState {
    pub fn new(config: RinfraConfig) -> Self {
        Self {
            config: Arc::new(config),
            started_at: Instant::now(),
            extensions: HashMap::new(),
        }
    }

    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
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

    pub fn has<T: Send + Sync + 'static>(&self) -> bool {
        self.extensions.contains_key(&TypeId::of::<T>())
    }

    pub fn extensions_mut(&mut self) -> &mut HashMap<TypeId, Box<dyn Any + Send + Sync>> {
        &mut self.extensions
    }

    // --- well-known convenience accessors ---

    pub fn cache(&self) -> Option<&Arc<dyn Cache>> {
        self.get::<Arc<dyn Cache>>()
    }

    pub fn store(&self) -> Option<&Arc<dyn Store>> {
        self.get::<Arc<dyn Store>>()
    }

    pub fn message_bus(&self) -> Option<&Arc<dyn MessageBus>> {
        self.get::<Arc<dyn MessageBus>>()
    }

    pub fn ratelimiter(&self) -> Option<&Arc<dyn RateLimiter>> {
        self.get::<Arc<dyn RateLimiter>>()
    }

    pub fn node_registry(&self) -> Option<&Arc<dyn NodeRegistry>> {
        self.get::<Arc<dyn NodeRegistry>>()
    }

    pub fn codecs(&self) -> Option<&CodecRegistry> {
        self.get::<CodecRegistry>()
    }

    pub fn crypto(&self) -> Option<&Arc<dyn Crypto>> {
        self.get::<Arc<dyn Crypto>>()
    }

    /// Access the single script engine (legacy; prefer `script_engines()`).
    pub fn script_engine(&self) -> Option<&Arc<dyn ScriptEngine>> {
        self.get::<Arc<dyn ScriptEngine>>()
    }

    pub fn script_engines(&self) -> Option<&ScriptEngineRegistry> {
        self.get::<ScriptEngineRegistry>()
    }

    pub fn db(&self) -> Option<&Arc<dyn DbConnection>> {
        self.get::<Arc<dyn DbConnection>>()
    }

    pub fn stores(&self) -> Option<&StoreRegistry> {
        self.get::<StoreRegistry>()
    }

    pub fn file_store(&self) -> Option<&Arc<dyn FileStore>> {
        self.get::<Arc<dyn FileStore>>()
    }

    pub fn http_client(&self) -> Option<&Arc<dyn HttpClient>> {
        self.get::<Arc<dyn HttpClient>>()
    }

    pub fn distributed_lock(&self) -> Option<&Arc<dyn DistributedLock>> {
        self.get::<Arc<dyn DistributedLock>>()
    }

    pub fn config_watcher(&self) -> Option<&Arc<dyn ConfigWatcher>> {
        self.get::<Arc<dyn ConfigWatcher>>()
    }

    pub fn audit_logger(&self) -> Option<&Arc<dyn AuditLogger>> {
        self.get::<Arc<dyn AuditLogger>>()
    }

    pub fn i18n(&self) -> Option<&Arc<dyn I18n>> {
        self.get::<Arc<dyn I18n>>()
    }

    pub fn health_checkers(&self) -> Option<&HealthCheckerRegistry> {
        self.get::<HealthCheckerRegistry>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_appstate_new_defaults() {
        let state = AppState::new(RinfraConfig::default());
        assert_eq!(state.config.app.name, "rinfra-app");
        assert!(state.cache().is_none());
        assert!(state.message_bus().is_none());
        assert!(state.node_registry().is_none());
        assert!(state.store().is_none());
        assert!(state.ratelimiter().is_none());
    }

    #[test]
    fn test_appstate_uptime() {
        let state = AppState::new(RinfraConfig::default());
        assert!(state.uptime_secs() < 2);
    }

    #[test]
    fn test_appstate_with_custom_config() {
        let mut config = RinfraConfig::default();
        config.app.name = "test-app".to_string();
        let state = AppState::new(config);
        assert_eq!(state.config.app.name, "test-app");
    }

    #[test]
    fn test_extension_roundtrip() {
        let mut state = AppState::new(RinfraConfig::default());
        state.set::<u64>(42);
        assert_eq!(state.get::<u64>(), Some(&42));
        assert!(state.has::<u64>());
    }

    #[test]
    fn test_extension_missing() {
        let state = AppState::new(RinfraConfig::default());
        assert!(state.get::<String>().is_none());
        assert!(!state.has::<String>());
    }
}
