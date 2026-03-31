use std::sync::Arc;

use async_trait::async_trait;
use rinfra_core::cache::Cache;
use rinfra_core::mq::MessageBus;
use rinfra_core::plugin::{HealthCheckResult, HealthCheckable};
use rinfra_core::store::Store;

/// Health checker for [`Store`] (database connections).
pub struct StoreHealthChecker {
    name: String,
    store: Arc<dyn Store>,
}

impl StoreHealthChecker {
    pub fn new(name: impl Into<String>, store: Arc<dyn Store>) -> Self {
        Self {
            name: name.into(),
            store,
        }
    }
}

#[async_trait]
impl HealthCheckable for StoreHealthChecker {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check(&self) -> HealthCheckResult {
        match self.store.health_check().await {
            Ok(true) => HealthCheckResult::healthy(),
            Ok(false) => HealthCheckResult::unhealthy("health check returned false"),
            Err(e) => HealthCheckResult::unhealthy(e.to_string()),
        }
    }
}

/// Health checker for [`Cache`] backends.
pub struct CacheHealthChecker {
    name: String,
    cache: Arc<dyn Cache>,
}

impl CacheHealthChecker {
    pub fn new(name: impl Into<String>, cache: Arc<dyn Cache>) -> Self {
        Self {
            name: name.into(),
            cache,
        }
    }
}

#[async_trait]
impl HealthCheckable for CacheHealthChecker {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check(&self) -> HealthCheckResult {
        match self.cache.exists("__health_probe__").await {
            Ok(_) => HealthCheckResult::healthy(),
            Err(e) => HealthCheckResult::unhealthy(e.to_string()),
        }
    }
}

/// Health checker for [`MessageBus`] backends.
pub struct MessageBusHealthChecker {
    name: String,
    bus: Arc<dyn MessageBus>,
}

impl MessageBusHealthChecker {
    pub fn new(name: impl Into<String>, bus: Arc<dyn MessageBus>) -> Self {
        Self {
            name: name.into(),
            bus,
        }
    }
}

#[async_trait]
impl HealthCheckable for MessageBusHealthChecker {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check(&self) -> HealthCheckResult {
        match self.bus.health_check().await {
            Ok(true) => HealthCheckResult::healthy(),
            Ok(false) => HealthCheckResult::unhealthy("health check returned false"),
            Err(e) => HealthCheckResult::unhealthy(e.to_string()),
        }
    }
}
