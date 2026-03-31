use std::sync::Arc;

use async_trait::async_trait;
use rinfra_core::cache::Cache;
use rinfra_core::crypto::{Crypto, KeyProvider, VersionedKeyProvider};
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::plugin::{Plugin, PluginContext, PluginManifest};
use rinfra_core::script::{ScriptEngine, ScriptEngineRegistry};
use rinfra_core::store::{DbConnection, Store};
use rinfra_core::audit::AuditLogger;
use rinfra_core::config::watch::ConfigWatcher;
use rinfra_core::file_store::FileStore;
use rinfra_core::http_client::HttpClient;
use rinfra_core::i18n::I18n;
use rinfra_core::lock::DistributedLock;
use rinfra_core::mq::MessageBus;
use rinfra_core::resilience::{CircuitBreaker, CircuitBreakerConfig};
use rinfra_core::timer::{TimerEngine, TimerEngineRegistry};
use tracing::{info, warn};

use crate::cache::{MemoryCache, MultilevelCache, RedisCache};
use crate::codec::{JsonCodec, MsgpackCodec, ProtobufCodec};
use crate::crypto::{AesGcmCrypto, EnvKeyProvider, FileKeyProvider, RotatingKeyProvider};
use crate::mq::InMemoryBus;
use crate::ratelimit::{MemoryRateLimiter, RedisRateLimiter};
use crate::script::{JsEngine, PythonEngine, WasmEngine};
use crate::store::PostgresStore;
#[cfg(feature = "mysql")]
use crate::store::mysql::MysqlStore;
#[cfg(feature = "sqlite")]
use crate::store::sqlite::SqliteStore;

// ---------------------------------------------------------------------------
// Cache plugins
// ---------------------------------------------------------------------------

pub struct MemoryCachePlugin {
    manifest: PluginManifest,
}

impl MemoryCachePlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("cache.memory", "0.1.0", "In-memory cache (moka)"),
        }
    }
}

#[async_trait]
impl Plugin for MemoryCachePlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = &ctx.config().plugins.cache.memory;
        if !cfg.enabled {
            return Ok(());
        }
        let cache = Arc::new(MemoryCache::new(cfg));
        ctx.set_cache(cache);
        info!("initialized memory cache");
        Ok(())
    }
}

pub struct RedisCachePlugin {
    manifest: PluginManifest,
}

impl RedisCachePlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("cache.redis", "0.1.0", "Redis cache"),
        }
    }
}

#[async_trait]
impl Plugin for RedisCachePlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = &ctx.config().plugins.cache.redis;
        if !cfg.enabled {
            return Ok(());
        }
        let cache = Arc::new(RedisCache::new(cfg));
        let c = cache.clone();
        if let Err(e) = cache.connect().await {
            if cfg.required {
                return Err(AppError::new(
                    ErrorCode::PluginInitFailed,
                    format!("redis cache connect failed (required): {e}"),
                ));
            }
            warn!(error = %e, "redis cache connect failed, continuing without redis");
        }
        let cache_dyn: Arc<dyn Cache> = cache;
        ctx.add_health_checker(Arc::new(
            crate::health::CacheHealthChecker::new("cache.redis", cache_dyn.clone()),
        ));
        ctx.set_cache(cache_dyn);
        ctx.add_shutdown_hook(move || {
            let c = c;
            async move {
                c.close().await;
                Ok(())
            }
        });
        info!("initialized redis cache");
        Ok(())
    }
}

pub struct MultilevelCachePlugin {
    manifest: PluginManifest,
}

impl MultilevelCachePlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new(
                "cache.multilevel",
                "0.1.0",
                "Multi-level cache (L1+L2)",
            ),
        }
    }
}

#[async_trait]
impl Plugin for MultilevelCachePlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = &ctx.config().plugins.cache;
        if !cfg.multilevel.enabled {
            return Ok(());
        }
        let mem_cfg = &cfg.memory;
        let redis_cfg = &cfg.redis;
        if !mem_cfg.enabled || !redis_cfg.enabled {
            tracing::warn!("multilevel cache requires both memory and redis to be enabled");
            return Ok(());
        }

        let l1 = Arc::new(MemoryCache::new(mem_cfg));
        let l2 = Arc::new(RedisCache::new(redis_cfg));
        let l2_clone = l2.clone();
        if let Err(e) = l2.connect().await {
            tracing::warn!(error = %e, "redis cache connect failed for multilevel");
        }

        let ml = MultilevelCache::new(
            l1 as Arc<dyn Cache>,
            l2.clone() as Arc<dyn Cache>,
        );
        let ml_cache: Arc<dyn Cache> = Arc::new(ml);
        ctx.add_health_checker(Arc::new(
            crate::health::CacheHealthChecker::new("cache.multilevel", ml_cache.clone()),
        ));
        ctx.set_cache(ml_cache);
        ctx.add_shutdown_hook(move || {
            let c = l2_clone;
            async move {
                c.close().await;
                Ok(())
            }
        });
        info!("initialized multilevel cache (L1=memory, L2=redis)");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Store plugin
// ---------------------------------------------------------------------------

pub struct PostgresStorePlugin {
    manifest: PluginManifest,
}

impl PostgresStorePlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("store.postgres", "0.1.0", "PostgreSQL store"),
        }
    }
}

#[async_trait]
impl Plugin for PostgresStorePlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = &ctx.config().plugins.store.postgres;
        if !cfg.enabled {
            return Ok(());
        }
        let store = Arc::new(PostgresStore::new(cfg.clone()));
        let s = store.clone();
        if let Err(e) = Store::connect(store.as_ref()).await {
            if cfg.required {
                return Err(AppError::new(
                    ErrorCode::PluginInitFailed,
                    format!("postgres store connect failed (required): {e}"),
                ));
            }
            warn!(error = %e, "postgres store connect failed, continuing without postgres");
        }
        ctx.set_store(store.clone() as Arc<dyn Store>);
        ctx.set_db(store.clone() as Arc<dyn DbConnection>);
        ctx.add_health_checker(Arc::new(
            crate::health::StoreHealthChecker::new("store.postgres", store as Arc<dyn Store>),
        ));
        ctx.add_shutdown_hook(move || {
            let s = s;
            async move { s.disconnect().await }
        });
        info!("registered postgres store (Store + DbConnection)");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// MySQL store plugin
// ---------------------------------------------------------------------------

#[cfg(feature = "mysql")]
pub struct MysqlStorePlugin {
    manifest: PluginManifest,
}

#[cfg(feature = "mysql")]
impl MysqlStorePlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("store.mysql", "0.1.0", "MySQL store"),
        }
    }
}

#[cfg(feature = "mysql")]
#[async_trait]
impl Plugin for MysqlStorePlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = &ctx.config().plugins.store.mysql;
        if !cfg.enabled {
            return Ok(());
        }
        let store = Arc::new(MysqlStore::new(cfg.clone()));
        let s = store.clone();
        if let Err(e) = Store::connect(store.as_ref()).await {
            if cfg.required {
                return Err(AppError::new(
                    ErrorCode::PluginInitFailed,
                    format!("mysql store connect failed (required): {e}"),
                ));
            }
            warn!(error = %e, "mysql store connect failed, continuing without mysql");
        }
        ctx.set_store(store.clone() as Arc<dyn Store>);
        ctx.set_db(store.clone() as Arc<dyn DbConnection>);
        ctx.add_health_checker(Arc::new(
            crate::health::StoreHealthChecker::new("store.mysql", store as Arc<dyn Store>),
        ));
        ctx.add_shutdown_hook(move || {
            let s = s;
            async move { s.disconnect().await }
        });
        info!("registered mysql store (Store + DbConnection)");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// SQLite store plugin
// ---------------------------------------------------------------------------

#[cfg(feature = "sqlite")]
pub struct SqliteStorePlugin {
    manifest: PluginManifest,
}

#[cfg(feature = "sqlite")]
impl SqliteStorePlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("store.sqlite", "0.1.0", "SQLite store"),
        }
    }
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl Plugin for SqliteStorePlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = &ctx.config().plugins.store.sqlite;
        if !cfg.enabled {
            return Ok(());
        }
        let store = Arc::new(SqliteStore::new(cfg.clone()));
        let s = store.clone();
        if let Err(e) = Store::connect(store.as_ref()).await {
            if cfg.required {
                return Err(AppError::new(
                    ErrorCode::PluginInitFailed,
                    format!("sqlite store connect failed (required): {e}"),
                ));
            }
            warn!(error = %e, "sqlite store connect failed, continuing without sqlite");
        }
        ctx.set_store(store.clone() as Arc<dyn Store>);
        ctx.set_db(store.clone() as Arc<dyn DbConnection>);
        ctx.add_health_checker(Arc::new(
            crate::health::StoreHealthChecker::new("store.sqlite", store as Arc<dyn Store>),
        ));
        ctx.add_shutdown_hook(move || {
            let s = s;
            async move { s.disconnect().await }
        });
        info!("registered sqlite store (Store + DbConnection)");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Message Queue plugins
// ---------------------------------------------------------------------------

pub struct InMemoryBusPlugin {
    manifest: PluginManifest,
}

impl InMemoryBusPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("mq.memory", "0.1.0", "In-memory message bus"),
        }
    }
}

#[async_trait]
impl Plugin for InMemoryBusPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        if ctx.config().plugins.mq.backend != rinfra_core::config::MqBackend::Memory {
            return Ok(());
        }
        let bus: Arc<dyn MessageBus> = Arc::new(InMemoryBus::new(&ctx.config().plugins.mq.memory));
        ctx.add_health_checker(Arc::new(
            crate::health::MessageBusHealthChecker::new("mq.memory", bus.clone()),
        ));
        ctx.set_message_bus(bus);
        info!("initialized in-memory message bus");
        Ok(())
    }
}

#[cfg(feature = "nats")]
pub struct NatsBusPlugin {
    manifest: PluginManifest,
}

#[cfg(feature = "nats")]
impl NatsBusPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("mq.nats", "0.1.0", "NATS JetStream message bus"),
        }
    }
}

#[cfg(feature = "nats")]
#[async_trait]
impl Plugin for NatsBusPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        if ctx.config().plugins.mq.backend != rinfra_core::config::MqBackend::Nats {
            return Ok(());
        }
        let nats_cfg = ctx.config().plugins.mq.nats.clone();
        let bus: Arc<dyn MessageBus> = Arc::new(crate::mq::NatsBus::connect(&nats_cfg).await?);
        ctx.add_health_checker(Arc::new(
            crate::health::MessageBusHealthChecker::new("mq.nats", bus.clone()),
        ));
        ctx.set_message_bus(bus);
        info!(url = %nats_cfg.url, stream = %nats_cfg.stream_name, "initialized NATS JetStream message bus");
        Ok(())
    }
}

#[cfg(feature = "redis-mq")]
pub struct RedisStreamBusPlugin {
    manifest: PluginManifest,
}

#[cfg(feature = "redis-mq")]
impl RedisStreamBusPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("mq.redis_streams", "0.1.0", "Redis Streams message bus"),
        }
    }
}

#[cfg(feature = "redis-mq")]
#[async_trait]
impl Plugin for RedisStreamBusPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        if ctx.config().plugins.mq.backend != rinfra_core::config::MqBackend::RedisStreams {
            return Ok(());
        }
        let rs_cfg = ctx.config().plugins.mq.redis_streams.clone();
        let bus: Arc<dyn MessageBus> = Arc::new(crate::mq::RedisStreamBus::connect(&rs_cfg).await?);
        ctx.add_health_checker(Arc::new(
            crate::health::MessageBusHealthChecker::new("mq.redis_streams", bus.clone()),
        ));
        ctx.set_message_bus(bus);
        info!(url = %rs_cfg.url, group = %rs_cfg.group_name, "initialized Redis Streams message bus");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Rate Limiter plugin
// ---------------------------------------------------------------------------

pub struct MemoryRateLimiterPlugin {
    manifest: PluginManifest,
}

impl MemoryRateLimiterPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new(
                "ratelimit.memory",
                "0.1.0",
                "In-memory rate limiter",
            ),
        }
    }
}

#[async_trait]
impl Plugin for MemoryRateLimiterPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = &ctx.config().plugins.ratelimit.memory;
        if !cfg.enabled {
            return Ok(());
        }
        let limiter = Arc::new(MemoryRateLimiter::new(cfg));
        ctx.set_ratelimiter(limiter);
        info!("initialized memory rate limiter");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Codec plugins
// ---------------------------------------------------------------------------

pub struct JsonCodecPlugin {
    manifest: PluginManifest,
}

impl JsonCodecPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("codec.json", "0.1.0", "JSON codec"),
        }
    }
}

#[async_trait]
impl Plugin for JsonCodecPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        ctx.add_codec(Box::new(JsonCodec::new()))?;
        Ok(())
    }
}

pub struct MsgpackCodecPlugin {
    manifest: PluginManifest,
}

impl MsgpackCodecPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("codec.msgpack", "0.1.0", "MessagePack codec"),
        }
    }
}

#[async_trait]
impl Plugin for MsgpackCodecPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        ctx.add_codec(Box::new(MsgpackCodec))?;
        Ok(())
    }
}

pub struct ProtobufCodecPlugin {
    manifest: PluginManifest,
}

impl ProtobufCodecPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("codec.protobuf", "0.1.0", "Protobuf codec (prost)"),
        }
    }
}

#[async_trait]
impl Plugin for ProtobufCodecPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        ctx.add_codec(Box::new(ProtobufCodec))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// gRPC plugin
// ---------------------------------------------------------------------------

pub struct GrpcPlugin {
    manifest: PluginManifest,
}

impl GrpcPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("net.rpc", "0.1.0", "gRPC server (tonic)"),
        }
    }
}

#[async_trait]
impl Plugin for GrpcPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, _ctx: &mut PluginContext) -> Result<(), AppError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// TRPC plugin
// ---------------------------------------------------------------------------

pub struct TrpcPlugin {
    manifest: PluginManifest,
}

impl TrpcPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("net.trpc", "0.1.0", "Lightweight TCP RPC server"),
        }
    }
}

#[async_trait]
impl Plugin for TrpcPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, _ctx: &mut PluginContext) -> Result<(), AppError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Redis Rate Limiter plugin
// ---------------------------------------------------------------------------

pub struct RedisRateLimiterPlugin {
    manifest: PluginManifest,
}

impl RedisRateLimiterPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new(
                "ratelimit.redis",
                "0.1.0",
                "Redis-backed distributed rate limiter",
            ),
        }
    }
}

#[async_trait]
impl Plugin for RedisRateLimiterPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = &ctx.config().plugins.ratelimit.redis;
        if !cfg.enabled {
            return Ok(());
        }
        let limiter = Arc::new(RedisRateLimiter::new(cfg));
        if let Err(e) = limiter.connect().await {
            if cfg.required {
                return Err(AppError::new(
                    ErrorCode::PluginInitFailed,
                    format!("redis rate limiter connect failed (required): {e}"),
                ));
            }
            warn!(error = %e, "redis rate limiter connect failed, continuing without it");
        }
        let l = limiter.clone();
        ctx.set_ratelimiter(limiter);
        ctx.add_shutdown_hook(move || {
            let l = l;
            async move {
                l.close().await;
                Ok(())
            }
        });
        info!("initialized redis rate limiter");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Crypto plugins
// ---------------------------------------------------------------------------

pub struct EnvKeyProviderPlugin {
    manifest: PluginManifest,
}

impl EnvKeyProviderPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new(
                "crypto.env_key",
                "0.1.0",
                "Environment variable key provider",
            ),
        }
    }
}

#[async_trait]
impl Plugin for EnvKeyProviderPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let provider: Arc<dyn KeyProvider> = Arc::new(EnvKeyProvider);
        ctx.set(provider);
        info!("registered env key provider");
        Ok(())
    }
}

pub struct FileKeyProviderPlugin {
    manifest: PluginManifest,
}

impl FileKeyProviderPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new(
                "crypto.file_key",
                "0.1.0",
                "File-based key provider",
            ),
        }
    }
}

#[async_trait]
impl Plugin for FileKeyProviderPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = &ctx.config().plugins.crypto.file;
        if !cfg.enabled {
            return Ok(());
        }
        let provider: Arc<dyn KeyProvider> = Arc::new(FileKeyProvider::new(cfg));
        ctx.set(provider);
        info!("registered file key provider");
        Ok(())
    }
}

pub struct RotatingKeyProviderPlugin {
    manifest: PluginManifest,
}

impl RotatingKeyProviderPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new(
                "crypto.rotating_key",
                "0.1.0",
                "Rotating versioned key provider",
            ),
        }
    }
}

#[async_trait]
impl Plugin for RotatingKeyProviderPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = &ctx.config().plugins.crypto.rotating;
        if !cfg.enabled {
            return Ok(());
        }
        let provider: Arc<dyn VersionedKeyProvider> = Arc::new(RotatingKeyProvider::new(cfg));
        ctx.set(provider);
        info!("registered rotating key provider");
        Ok(())
    }
}

pub struct AesGcmCryptoPlugin {
    manifest: PluginManifest,
}

impl AesGcmCryptoPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("crypto.aesgcm", "0.1.0", "AES-256-GCM encryption"),
        }
    }
}

#[async_trait]
impl Plugin for AesGcmCryptoPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = &ctx.config().plugins.crypto.aesgcm;
        if !cfg.enabled {
            return Ok(());
        }
        let key_provider = ctx.get::<Arc<dyn KeyProvider>>().cloned().ok_or_else(|| {
            AppError::new(
                ErrorCode::PluginInitFailed,
                "crypto.aesgcm requires a KeyProvider plugin (e.g. crypto.env_key)",
            )
        })?;
        let key = key_provider.get_key(&cfg.key_env_var).await.map_err(|e| {
            AppError::new(
                ErrorCode::PluginInitFailed,
                format!("crypto.aesgcm: failed to read key from '{}': {e}", cfg.key_env_var),
            )
        })?;
        let crypto = AesGcmCrypto::new(key)?;
        let crypto: Arc<dyn Crypto> = Arc::new(crypto);
        ctx.set(crypto);
        info!("initialized AES-GCM crypto");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Script plugins (all engines share a ScriptEngineRegistry)
// ---------------------------------------------------------------------------

/// Helper: get or create the shared `ScriptEngineRegistry` in `PluginContext`.
fn ensure_script_registry(ctx: &mut PluginContext) {
    if ctx.get::<ScriptEngineRegistry>().is_none() {
        ctx.set(ScriptEngineRegistry::new());
    }
}

pub struct WasmScriptPlugin {
    manifest: PluginManifest,
}

impl WasmScriptPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("script.wasm", "0.1.0", "WASM script engine"),
        }
    }
}

#[async_trait]
impl Plugin for WasmScriptPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = ctx.config().plugins.script.wasm.clone();
        if !cfg.enabled {
            return Ok(());
        }
        ensure_script_registry(ctx);
        let engine: Arc<dyn ScriptEngine> = Arc::new(WasmEngine::new(cfg));
        ctx.get_mut::<ScriptEngineRegistry>()
            .unwrap()
            .register(engine)?;
        info!("initialized WASM script engine");
        Ok(())
    }
}

pub struct PythonScriptPlugin {
    manifest: PluginManifest,
}

impl PythonScriptPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("script.python", "0.1.0", "Python script engine"),
        }
    }
}

#[async_trait]
impl Plugin for PythonScriptPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = ctx.config().plugins.script.python.clone();
        if !cfg.enabled {
            return Ok(());
        }
        ensure_script_registry(ctx);
        let engine: Arc<dyn ScriptEngine> = Arc::new(PythonEngine::new(cfg));
        ctx.get_mut::<ScriptEngineRegistry>()
            .unwrap()
            .register(engine)?;
        info!("initialized Python script engine");
        Ok(())
    }
}

pub struct JsScriptPlugin {
    manifest: PluginManifest,
}

impl JsScriptPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("script.js", "0.1.0", "JavaScript script engine"),
        }
    }
}

#[async_trait]
impl Plugin for JsScriptPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = ctx.config().plugins.script.js.clone();
        if !cfg.enabled {
            return Ok(());
        }
        ensure_script_registry(ctx);
        let engine: Arc<dyn ScriptEngine> = Arc::new(JsEngine::new(cfg));
        ctx.get_mut::<ScriptEngineRegistry>()
            .unwrap()
            .register(engine)?;
        info!("initialized JavaScript script engine");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Timer
// ---------------------------------------------------------------------------

fn ensure_timer_registry(ctx: &mut PluginContext) {
    if ctx.get::<TimerEngineRegistry>().is_none() {
        ctx.set(TimerEngineRegistry::new());
    }
}

pub struct SimpleTimerPlugin {
    manifest: PluginManifest,
}

impl SimpleTimerPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("timer.simple", "0.1.0", "Simple in-process timer engine"),
        }
    }
}

#[async_trait]
impl Plugin for SimpleTimerPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = ctx.config().plugins.timer.clone();
        if !cfg.enabled {
            return Ok(());
        }
        ensure_timer_registry(ctx);
        let mut engine = crate::timer::SimpleTimerEngine::new(&cfg.simple);
        if let Some(lock) = ctx.get::<Arc<dyn DistributedLock>>() {
            engine = engine.with_lock(lock.clone());
            info!("timer engine integrated with distributed lock for cluster-safe scheduling");
        }
        let engine: Arc<dyn TimerEngine> = Arc::new(engine);
        ctx.get_mut::<TimerEngineRegistry>()
            .unwrap()
            .register(engine)?;
        info!("initialized simple timer engine");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// FileStore
// ---------------------------------------------------------------------------

pub struct LocalFileStorePlugin {
    manifest: PluginManifest,
}

impl LocalFileStorePlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new(
                "file_store.local",
                "0.1.0",
                "Local filesystem file store",
            ),
        }
    }
}

#[async_trait]
impl Plugin for LocalFileStorePlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = ctx.config().plugins.file_store.clone();
        if !cfg.enabled {
            return Ok(());
        }
        let store: Arc<dyn FileStore> =
            Arc::new(crate::file_store::LocalFileStore::new(&cfg.local.root_dir));
        ctx.set(store);
        info!(root_dir = %cfg.local.root_dir, "initialized local file store");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// HttpClient
// ---------------------------------------------------------------------------

pub struct HttpClientPlugin {
    manifest: PluginManifest,
}

impl HttpClientPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("http_client", "0.1.0", "HTTP client (reqwest)"),
        }
    }
}

#[async_trait]
impl Plugin for HttpClientPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = ctx.config().plugins.http_client.clone();
        if !cfg.enabled {
            return Ok(());
        }
        let mut reqwest_client = crate::http_client::ReqwestHttpClient::new(&cfg)?;
        let breaker = Arc::new(CircuitBreaker::new(
            "http_client",
            CircuitBreakerConfig::default(),
        ));
        reqwest_client = reqwest_client.with_circuit_breaker(breaker);
        info!("HTTP client integrated with circuit breaker");
        let client: Arc<dyn HttpClient> = Arc::new(reqwest_client);
        ctx.set(client);
        info!(
            timeout_secs = cfg.timeout_secs,
            max_retries = cfg.max_retries,
            "initialized HTTP client"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// DistributedLock
// ---------------------------------------------------------------------------

pub struct LockPlugin {
    manifest: PluginManifest,
}

impl LockPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("lock", "0.1.0", "Distributed lock (memory / redis)"),
        }
    }
}

#[async_trait]
impl Plugin for LockPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = ctx.config().plugins.lock.clone();
        if !cfg.enabled {
            return Ok(());
        }

        let lock: Arc<dyn DistributedLock> = match cfg.backend.as_str() {
            "redis" => {
                let redis_lock = crate::lock::RedisLock::new(&cfg.redis);
                redis_lock.connect().await?;
                info!(url = %cfg.redis.url, "initialized redis distributed lock");
                Arc::new(redis_lock)
            }
            _ => {
                info!("initialized in-memory distributed lock");
                Arc::new(crate::lock::InMemoryLock::new())
            }
        };

        ctx.set(lock);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ConfigWatch
// ---------------------------------------------------------------------------

pub struct ConfigWatchPlugin {
    manifest: PluginManifest,
}

impl ConfigWatchPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("config_watch", "0.1.0", "Config file hot-reload"),
        }
    }
}

#[async_trait]
impl Plugin for ConfigWatchPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = ctx.config().plugins.config_watch.clone();
        if !cfg.enabled {
            return Ok(());
        }

        let config_path = ctx.config().config_path.clone();
        let watcher = Arc::new(crate::config_watch::FileConfigWatcher::new(
            config_path,
            cfg.poll_interval_secs,
        ));

        watcher.add_handler(Arc::new(
            crate::config_watch::LogConfigReloadHandler,
        ));

        if let Some(audit) = ctx.get::<Arc<dyn AuditLogger>>() {
            watcher.add_handler(Arc::new(
                crate::config_watch::AuditConfigReloadHandler::new(audit.clone()),
            ));
            info!("config watcher integrated with audit logger");
        }

        watcher.start().await?;

        let w: Arc<dyn ConfigWatcher> = watcher;
        ctx.set(w);
        info!(
            poll_secs = cfg.poll_interval_secs,
            "initialized config file watcher"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Audit
// ---------------------------------------------------------------------------

pub struct FileAuditPlugin {
    manifest: PluginManifest,
}

impl FileAuditPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("audit.file", "0.1.0", "File-based audit logger"),
        }
    }
}

#[async_trait]
impl Plugin for FileAuditPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = ctx.config().plugins.audit.clone();
        if !cfg.enabled {
            return Ok(());
        }
        let logger: Arc<dyn AuditLogger> =
            Arc::new(crate::audit::FileAuditLogger::new(&cfg.file.path));
        ctx.set(logger);
        info!(path = %cfg.file.path, "initialized file audit logger");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// i18n
// ---------------------------------------------------------------------------

pub struct I18nPlugin {
    manifest: PluginManifest,
}

impl I18nPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new("i18n", "0.1.0", "File-based i18n"),
        }
    }
}

#[async_trait]
impl Plugin for I18nPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = ctx.config().plugins.i18n.clone();
        if !cfg.enabled {
            return Ok(());
        }
        let i18n: Arc<dyn I18n> =
            Arc::new(crate::i18n::FileI18n::load(&cfg.dir, &cfg.default_locale)?);
        ctx.set(i18n);
        info!(dir = %cfg.dir, default = %cfg.default_locale, "initialized i18n");
        Ok(())
    }
}

/// Returns the default set of built-in plugins in the recommended order.
/// Ordering matters: KeyProvider plugins must come before AesGcm which depends on them.
pub fn builtin_plugins() -> Vec<Box<dyn Plugin>> {
    vec![
        // Codecs
        Box::new(JsonCodecPlugin::new()),
        Box::new(MsgpackCodecPlugin::new()),
        Box::new(ProtobufCodecPlugin::new()),
        // Cache
        Box::new(MemoryCachePlugin::new()),
        Box::new(RedisCachePlugin::new()),
        Box::new(MultilevelCachePlugin::new()),
        // Messaging
        Box::new(InMemoryBusPlugin::new()),
        #[cfg(feature = "nats")]
        Box::new(NatsBusPlugin::new()),
        #[cfg(feature = "redis-mq")]
        Box::new(RedisStreamBusPlugin::new()),
        // Store
        Box::new(PostgresStorePlugin::new()),
        #[cfg(feature = "mysql")]
        Box::new(MysqlStorePlugin::new()),
        #[cfg(feature = "sqlite")]
        Box::new(SqliteStorePlugin::new()),
        // Rate limiting
        Box::new(MemoryRateLimiterPlugin::new()),
        Box::new(RedisRateLimiterPlugin::new()),
        // Crypto (key providers first, then crypto impl)
        Box::new(EnvKeyProviderPlugin::new()),
        Box::new(FileKeyProviderPlugin::new()),
        Box::new(RotatingKeyProviderPlugin::new()),
        Box::new(AesGcmCryptoPlugin::new()),
        // Script engines
        Box::new(WasmScriptPlugin::new()),
        Box::new(PythonScriptPlugin::new()),
        Box::new(JsScriptPlugin::new()),
        // Network servers
        Box::new(GrpcPlugin::new()),
        Box::new(TrpcPlugin::new()),
        // FileStore
        Box::new(LocalFileStorePlugin::new()),
        // HttpClient
        Box::new(HttpClientPlugin::new()),
        // Lock (before Timer so timer can use distributed lock)
        Box::new(LockPlugin::new()),
        // Timer (after Lock so it can integrate cluster-safe scheduling)
        Box::new(SimpleTimerPlugin::new()),
        // Audit (before ConfigWatch so watcher can use audit logger)
        Box::new(FileAuditPlugin::new()),
        // i18n
        Box::new(I18nPlugin::new()),
        // ConfigWatch (after audit + i18n so it can integrate with them)
        Box::new(ConfigWatchPlugin::new()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinfra_core::config::RinfraConfig;

    #[tokio::test]
    async fn test_memory_cache_plugin_disabled() {
        let mut cfg = RinfraConfig::default();
        cfg.plugins.cache.memory.enabled = false;
        let mut ctx = PluginContext::new(cfg);
        MemoryCachePlugin::new().build(&mut ctx).await.unwrap();
        let (state, _) = ctx.into_app_parts();
        assert!(state.cache().is_none());
    }

    #[tokio::test]
    async fn test_memory_cache_plugin_enabled() {
        let mut cfg = RinfraConfig::default();
        cfg.plugins.cache.memory.enabled = true;
        let mut ctx = PluginContext::new(cfg);
        MemoryCachePlugin::new().build(&mut ctx).await.unwrap();
        let (state, _) = ctx.into_app_parts();
        assert!(state.cache().is_some());
    }

    #[tokio::test]
    async fn test_codec_plugins_register() {
        let mut ctx = PluginContext::new(RinfraConfig::default());
        JsonCodecPlugin::new().build(&mut ctx).await.unwrap();
        MsgpackCodecPlugin::new().build(&mut ctx).await.unwrap();
        ProtobufCodecPlugin::new().build(&mut ctx).await.unwrap();
        let (state, _) = ctx.into_app_parts();
        let codecs = state.codecs().unwrap();
        assert!(codecs.get_by_name("json").is_some());
        assert!(codecs.get_by_name("msgpack").is_some());
        assert!(codecs.get_by_name("protobuf").is_some());
    }

    #[tokio::test]
    async fn test_mq_plugin_disabled() {
        let mut cfg = RinfraConfig::default();
        cfg.plugins.mq.backend = rinfra_core::config::MqBackend::None;
        let mut ctx = PluginContext::new(cfg);
        InMemoryBusPlugin::new().build(&mut ctx).await.unwrap();
        let (state, _) = ctx.into_app_parts();
        assert!(state.message_bus().is_none());
    }

    #[tokio::test]
    async fn test_ratelimiter_plugin_enabled() {
        let mut cfg = RinfraConfig::default();
        cfg.plugins.ratelimit.memory.enabled = true;
        let mut ctx = PluginContext::new(cfg);
        MemoryRateLimiterPlugin::new().build(&mut ctx).await.unwrap();
        let (state, _) = ctx.into_app_parts();
        assert!(state.ratelimiter().is_some());
    }

    #[tokio::test]
    async fn test_grpc_plugin_noop() {
        let cfg = RinfraConfig::default();
        let mut ctx = PluginContext::new(cfg);
        GrpcPlugin::new().build(&mut ctx).await.unwrap();
    }

    #[test]
    fn test_builtin_plugins_count() {
        let plugins = builtin_plugins();
        assert_eq!(plugins.len(), 26);
    }
}
