use serde::{Deserialize, Serialize};

fn default_app_name() -> String {
    "rinfra-app".to_string()
}

fn default_version() -> String {
    "0.1.0".to_string()
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "pretty".to_string()
}

fn default_log_json_format() -> String {
    "json".to_string()
}

fn default_log_dir() -> String {
    "logs".to_string()
}

fn default_log_filename() -> String {
    "app.log".to_string()
}

fn default_log_rotation() -> String {
    "daily".to_string()
}

fn default_log_max_files() -> u32 {
    7
}

fn default_grace_period_secs() -> u64 {
    30
}

fn default_component_timeout_secs() -> u64 {
    10
}

fn default_pg_url() -> String {
    "postgres://localhost:5432/rinfra".to_string()
}

fn default_pg_max_connections() -> u32 {
    10
}

fn default_pg_idle_timeout_secs() -> u64 {
    300
}

fn default_cache_max_capacity() -> u64 {
    10_000
}

fn default_cache_ttl_secs() -> u64 {
    300
}

fn default_l1_max_capacity() -> u64 {
    10_000
}

fn default_l1_ttl_secs() -> u64 {
    60
}

fn default_redis_url() -> String {
    "redis://127.0.0.1:6379".to_string()
}

fn default_cors_allow_origins() -> Vec<String> {
    vec!["*".to_string()]
}

fn default_timeout_secs() -> u64 {
    30
}

fn default_rate_limit_rps() -> u64 {
    100
}

fn default_rate_limit_burst() -> u64 {
    200
}

fn default_ratelimit_window_secs() -> u64 {
    1
}

fn default_mq_channel_capacity() -> usize {
    1024
}

fn default_nats_url() -> String {
    "nats://localhost:4222".to_string()
}

fn default_nats_stream_name() -> String {
    "rinfra".to_string()
}

fn default_nats_consumer_group() -> String {
    "rinfra-workers".to_string()
}

fn default_nats_connect_timeout_secs() -> u64 {
    5
}

fn default_redis_stream_group() -> String {
    "rinfra-workers".to_string()
}

fn default_redis_stream_consumer() -> String {
    hostname_or_uuid()
}

fn hostname_or_uuid() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("POD_NAME"))
        .unwrap_or_else(|_| uuid::Uuid::new_v4().to_string())
}

fn default_redis_stream_block_ms() -> u64 {
    2000
}

fn default_redis_stream_batch_size() -> usize {
    10
}

fn default_script_timeout_secs() -> u64 {
    30
}

fn default_wasm_fuel_limit() -> u64 {
    1_000_000
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RinfraConfig {
    /// Path to the config file that loaded this config (set at runtime, not serialized).
    #[serde(skip)]
    pub config_path: String,
    #[serde(default)]
    pub app: AppConfig,
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub plugins: PluginConfigs,
    /// Extension point for business-specific configuration.
    /// Framework does not interpret this field; business projects
    /// can deserialize it into their own config struct via
    /// `serde_json::from_value(config.business.clone())`.
    #[serde(default)]
    pub business: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeConfig {
    #[serde(default)]
    pub shutdown: ShutdownConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownConfig {
    #[serde(default = "default_grace_period_secs")]
    pub grace_period_secs: u64,
    #[serde(default = "default_component_timeout_secs")]
    pub component_timeout_secs: u64,
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        Self {
            grace_period_secs: default_grace_period_secs(),
            component_timeout_secs: default_component_timeout_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_app_name")]
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            name: default_app_name(),
            version: default_version(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginConfigs {
    #[serde(default)]
    pub log: LogPluginConfigs,
    #[serde(default)]
    pub net: NetPluginConfigs,
    #[serde(default)]
    pub store: StorePluginConfigs,
    #[serde(default)]
    pub cache: CachePluginConfigs,
    #[serde(default)]
    pub ratelimit: RateLimitPluginConfigs,
    #[serde(default)]
    pub crypto: CryptoPluginConfigs,
    #[serde(default)]
    pub mq: MqPluginConfigs,
    #[serde(default)]
    pub script: ScriptPluginConfigs,
    #[serde(default)]
    pub cluster: ClusterPluginConfigs,
    #[serde(default)]
    pub admin: AdminPluginConfigs,
    #[serde(default)]
    pub timer: TimerPluginConfigs,
    #[serde(default)]
    pub file_store: FileStorePluginConfigs,
    #[serde(default)]
    pub http_client: HttpClientConfig,
    #[serde(default)]
    pub lock: LockPluginConfigs,
    #[serde(default)]
    pub config_watch: ConfigWatchConfig,
    #[serde(default)]
    pub audit: AuditPluginConfigs,
    #[serde(default)]
    pub i18n: I18nConfig,
    #[serde(default)]
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub health: HealthConfig,
    #[serde(default)]
    pub telemetry: TelemetryConfig,
}

fn default_metrics_endpoint() -> String {
    "/metrics".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_metrics_endpoint")]
    pub endpoint: String,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            endpoint: default_metrics_endpoint(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
        }
    }
}

fn default_otlp_endpoint() -> String {
    "http://localhost:4317".to_string()
}

fn default_sample_ratio() -> f64 {
    1.0
}

fn default_export_timeout_secs() -> u64 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_otlp_endpoint")]
    pub otlp_endpoint: String,
    #[serde(default = "default_sample_ratio")]
    pub sample_ratio: f64,
    #[serde(default = "default_export_timeout_secs")]
    pub export_timeout_secs: u64,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            otlp_endpoint: default_otlp_endpoint(),
            sample_ratio: default_sample_ratio(),
            export_timeout_secs: default_export_timeout_secs(),
        }
    }
}

fn default_admin_static_dir() -> String {
    "rinfra-admin/frontend/dist".to_string()
}

fn default_admin_token_file() -> String {
    "data/admin_tokens.json".to_string()
}

fn default_admin_root_token_env() -> String {
    "RINFRA_ROOT_TOKEN".to_string()
}

fn default_admin_auth_exclude_paths() -> Vec<String> {
    vec!["/api/admin/health".to_string()]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminPluginConfigs {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_admin_static_dir")]
    pub static_dir: String,
    #[serde(default)]
    pub auth: AdminAuthConfig,
}

impl Default for AdminPluginConfigs {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            static_dir: default_admin_static_dir(),
            auth: AdminAuthConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminAuthConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_admin_token_file")]
    pub token_file: String,
    #[serde(default = "default_admin_root_token_env")]
    pub root_token_env: String,
    #[serde(default = "default_admin_auth_exclude_paths")]
    pub exclude_paths: Vec<String>,
}

impl Default for AdminAuthConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            token_file: default_admin_token_file(),
            root_token_env: default_admin_root_token_env(),
            exclude_paths: default_admin_auth_exclude_paths(),
        }
    }
}

fn default_cluster_mode() -> String {
    "standalone".to_string()
}

fn default_cluster_role() -> String {
    "worker".to_string()
}

fn default_ping_interval_secs() -> u64 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterPluginConfigs {
    #[serde(default = "default_cluster_mode")]
    pub mode: String,
    #[serde(default = "default_cluster_role")]
    pub role: String,
    #[serde(default)]
    pub node_id: String,
    #[serde(default)]
    pub main_address: String,
    #[serde(default)]
    pub cluster_token: String,
    #[serde(default = "default_ping_interval_secs")]
    pub ping_interval_secs: u64,
}

impl Default for ClusterPluginConfigs {
    fn default() -> Self {
        Self {
            mode: default_cluster_mode(),
            role: default_cluster_role(),
            node_id: String::new(),
            main_address: String::new(),
            cluster_token: String::new(),
            ping_interval_secs: default_ping_interval_secs(),
        }
    }
}

impl ClusterPluginConfigs {
    pub fn is_cluster(&self) -> bool {
        self.mode == "cluster"
    }

    pub fn is_main(&self) -> bool {
        self.role == "main"
    }

    pub fn is_worker(&self) -> bool {
        self.role == "worker"
    }
}

fn default_rotation_interval_secs() -> u64 {
    86400
}

fn default_max_key_versions() -> u32 {
    5
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CryptoPluginConfigs {
    #[serde(default)]
    pub aesgcm: AesGcmConfig,
    #[serde(default)]
    pub rotating: RotatingKeyConfig,
    #[serde(default)]
    pub file: FileKeyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotatingKeyConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_rotation_interval_secs")]
    pub rotation_interval_secs: u64,
    #[serde(default = "default_max_key_versions")]
    pub max_key_versions: u32,
}

impl Default for RotatingKeyConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            rotation_interval_secs: default_rotation_interval_secs(),
            max_key_versions: default_max_key_versions(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileKeyConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default)]
    pub path: String,
}

impl Default for FileKeyConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            path: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AesGcmConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_crypto_key_env_var")]
    pub key_env_var: String,
}

fn default_crypto_key_env_var() -> String {
    "RINFRA_CRYPTO_KEY".to_string()
}

impl Default for AesGcmConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            key_env_var: default_crypto_key_env_var(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RateLimitPluginConfigs {
    #[serde(default)]
    pub memory: MemoryRateLimitConfig,
    #[serde(default)]
    pub redis: RedisRateLimitConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRateLimitConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_rate_limit_rps")]
    pub requests_per_second: u64,
    #[serde(default = "default_rate_limit_burst")]
    pub burst_size: u64,
}

impl Default for MemoryRateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            requests_per_second: default_rate_limit_rps(),
            burst_size: default_rate_limit_burst(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisRateLimitConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_false")]
    pub required: bool,
    #[serde(default = "default_redis_url")]
    pub url: String,
    #[serde(default = "default_rate_limit_rps")]
    pub requests_per_second: u64,
    #[serde(default = "default_rate_limit_burst")]
    pub burst_size: u64,
    #[serde(default = "default_ratelimit_window_secs")]
    pub window_secs: u64,
}

impl Default for RedisRateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            required: default_false(),
            url: default_redis_url(),
            requests_per_second: default_rate_limit_rps(),
            burst_size: default_rate_limit_burst(),
            window_secs: default_ratelimit_window_secs(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CachePluginConfigs {
    #[serde(default)]
    pub memory: MemoryCacheConfig,
    #[serde(default)]
    pub redis: RedisCacheConfig,
    #[serde(default)]
    pub multilevel: MultilevelCacheConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultilevelCacheConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_l1_max_capacity")]
    pub l1_max_capacity: u64,
    #[serde(default = "default_l1_ttl_secs")]
    pub l1_ttl_secs: u64,
}

impl Default for MultilevelCacheConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            l1_max_capacity: default_l1_max_capacity(),
            l1_ttl_secs: default_l1_ttl_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCacheConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_cache_max_capacity")]
    pub max_capacity: u64,
    #[serde(default = "default_cache_ttl_secs")]
    pub ttl_secs: u64,
}

impl Default for MemoryCacheConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            max_capacity: default_cache_max_capacity(),
            ttl_secs: default_cache_ttl_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisCacheConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_false")]
    pub required: bool,
    #[serde(default = "default_redis_url")]
    pub url: String,
}

impl Default for RedisCacheConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            required: default_false(),
            url: default_redis_url(),
        }
    }
}

fn default_ws_ping_interval_secs() -> u64 {
    30
}

fn default_ws_ping_timeout_secs() -> u64 {
    10
}

fn default_max_frame_size() -> usize {
    65536
}

// ── Unified Listener Config ──

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetPluginConfigs {
    #[serde(default)]
    pub listeners: Vec<ListenerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListenerConfig {
    pub name: String,
    pub protocol: ListenerProtocol,
    pub bind: String,
    #[serde(default)]
    pub http: Option<HttpListenerOptions>,
    #[serde(default)]
    pub tcp: Option<TcpListenerOptions>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ListenerProtocol {
    Http,
    Tcp,
    Grpc,
    Trpc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpListenerOptions {
    #[serde(default)]
    pub middleware: MiddlewareConfig,
    #[serde(default)]
    pub ws: WsOptions,
}

impl Default for HttpListenerOptions {
    fn default() -> Self {
        Self {
            middleware: MiddlewareConfig::default(),
            ws: WsOptions::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsOptions {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_ws_ping_interval_secs")]
    pub ping_interval_secs: u64,
    #[serde(default = "default_ws_ping_timeout_secs")]
    pub ping_timeout_secs: u64,
}

impl Default for WsOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            ping_interval_secs: default_ws_ping_interval_secs(),
            ping_timeout_secs: default_ws_ping_timeout_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpListenerOptions {
    #[serde(default)]
    pub codec: Option<String>,
    #[serde(default = "default_max_frame_size")]
    pub max_frame_size: usize,
    #[serde(default)]
    pub pipeline: Vec<PipelineStep>,
}

impl Default for TcpListenerOptions {
    fn default() -> Self {
        Self {
            codec: None,
            max_frame_size: default_max_frame_size(),
            pipeline: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStep {
    pub transform: String,
    #[serde(default)]
    pub options: serde_json::Value,
}

fn default_jwt_secret_env() -> String {
    "RINFRA_JWT_SECRET".to_string()
}

fn default_auth_exclude_paths() -> Vec<String> {
    vec![
        "/health".to_string(),
        "/healthz".to_string(),
        "/readyz".to_string(),
        "/metrics".to_string(),
    ]
}

fn default_ratelimit_key_strategy() -> String {
    "ip".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MiddlewareConfig {
    #[serde(default)]
    pub cors: CorsConfig,
    #[serde(default)]
    pub request_id: RequestIdConfig,
    #[serde(default)]
    pub timeout: TimeoutConfig,
    #[serde(default)]
    pub auth: AuthMiddlewareConfig,
    #[serde(default)]
    pub rate_limit: RateLimitMiddlewareConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthMiddlewareConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_jwt_secret_env")]
    pub jwt_secret_env: String,
    #[serde(default = "default_auth_exclude_paths")]
    pub exclude_paths: Vec<String>,
}

impl Default for AuthMiddlewareConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            jwt_secret_env: default_jwt_secret_env(),
            exclude_paths: default_auth_exclude_paths(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitMiddlewareConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_ratelimit_key_strategy")]
    pub key_strategy: String,
}

impl Default for RateLimitMiddlewareConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            key_strategy: default_ratelimit_key_strategy(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_cors_allow_origins")]
    pub allow_origins: Vec<String>,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            allow_origins: default_cors_allow_origins(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestIdConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for RequestIdConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            timeout_secs: default_timeout_secs(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorePluginConfigs {
    #[serde(default)]
    pub postgres: PostgresConfig,
    #[serde(default)]
    pub mysql: MysqlConfig,
    #[serde(default)]
    pub sqlite: SqliteConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    /// If true, application fails to start when connection fails.
    #[serde(default = "default_false")]
    pub required: bool,
    #[serde(default = "default_pg_url")]
    pub url: String,
    #[serde(default = "default_pg_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_pg_idle_timeout_secs")]
    pub idle_timeout_secs: u64,
    /// Path to SQL migration files. If set, migrations run automatically on connect.
    #[serde(default)]
    pub migrations_path: Option<String>,
    /// Queries slower than this threshold (ms) are logged at WARN level.
    /// Defaults to 200ms. Set to 0 to disable.
    #[serde(default = "default_slow_query_threshold_ms")]
    pub slow_query_threshold_ms: u64,
}

fn default_slow_query_threshold_ms() -> u64 {
    200
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            required: default_false(),
            url: default_pg_url(),
            max_connections: default_pg_max_connections(),
            idle_timeout_secs: default_pg_idle_timeout_secs(),
            migrations_path: None,
            slow_query_threshold_ms: default_slow_query_threshold_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MysqlConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_false")]
    pub required: bool,
    #[serde(default = "default_mysql_url")]
    pub url: String,
    #[serde(default = "default_pg_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_pg_idle_timeout_secs")]
    pub idle_timeout_secs: u64,
    #[serde(default = "default_slow_query_threshold_ms")]
    pub slow_query_threshold_ms: u64,
}

fn default_mysql_url() -> String {
    "mysql://root:root@localhost:3306/rinfra".to_string()
}

impl Default for MysqlConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            required: default_false(),
            url: default_mysql_url(),
            max_connections: default_pg_max_connections(),
            idle_timeout_secs: default_pg_idle_timeout_secs(),
            slow_query_threshold_ms: default_slow_query_threshold_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqliteConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_false")]
    pub required: bool,
    #[serde(default = "default_sqlite_path")]
    pub path: String,
    #[serde(default = "default_pg_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_slow_query_threshold_ms")]
    pub slow_query_threshold_ms: u64,
}

fn default_sqlite_path() -> String {
    "rinfra.db".to_string()
}

impl Default for SqliteConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            required: default_false(),
            path: default_sqlite_path(),
            max_connections: default_pg_max_connections(),
            slow_query_threshold_ms: default_slow_query_threshold_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqPluginConfigs {
    #[serde(default)]
    pub backend: MqBackend,
    #[serde(default)]
    pub memory: MemoryMqConfig,
    #[serde(default)]
    pub nats: NatsConfig,
    #[serde(default)]
    pub redis_streams: RedisStreamMqConfig,
}

impl Default for MqPluginConfigs {
    fn default() -> Self {
        Self {
            backend: MqBackend::default(),
            memory: MemoryMqConfig::default(),
            nats: NatsConfig::default(),
            redis_streams: RedisStreamMqConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MqBackend {
    #[default]
    Memory,
    Nats,
    RedisStreams,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMqConfig {
    #[serde(default = "default_mq_channel_capacity")]
    pub channel_capacity: usize,
}

impl Default for MemoryMqConfig {
    fn default() -> Self {
        Self {
            channel_capacity: default_mq_channel_capacity(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatsConfig {
    #[serde(default = "default_nats_url")]
    pub url: String,
    #[serde(default = "default_nats_stream_name")]
    pub stream_name: String,
    #[serde(default = "default_nats_consumer_group")]
    pub consumer_group: String,
    #[serde(default)]
    pub max_reconnects: Option<usize>,
    #[serde(default = "default_nats_connect_timeout_secs")]
    pub connect_timeout_secs: u64,
}

impl Default for NatsConfig {
    fn default() -> Self {
        Self {
            url: default_nats_url(),
            stream_name: default_nats_stream_name(),
            consumer_group: default_nats_consumer_group(),
            max_reconnects: None,
            connect_timeout_secs: default_nats_connect_timeout_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisStreamMqConfig {
    #[serde(default = "default_redis_url")]
    pub url: String,
    #[serde(default = "default_redis_stream_group")]
    pub group_name: String,
    #[serde(default = "default_redis_stream_consumer")]
    pub consumer_name: String,
    #[serde(default)]
    pub max_len: Option<usize>,
    #[serde(default = "default_redis_stream_block_ms")]
    pub block_ms: u64,
    #[serde(default = "default_redis_stream_batch_size")]
    pub batch_size: usize,
}

impl Default for RedisStreamMqConfig {
    fn default() -> Self {
        Self {
            url: default_redis_url(),
            group_name: default_redis_stream_group(),
            consumer_name: default_redis_stream_consumer(),
            max_len: None,
            block_ms: default_redis_stream_block_ms(),
            batch_size: default_redis_stream_batch_size(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScriptPluginConfigs {
    #[serde(default)]
    pub wasm: WasmScriptConfig,
    #[serde(default)]
    pub python: PythonScriptConfig,
    #[serde(default)]
    pub js: JsScriptConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmScriptConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_script_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_wasm_fuel_limit")]
    pub fuel_limit: u64,
}

impl Default for WasmScriptConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            timeout_secs: default_script_timeout_secs(),
            fuel_limit: default_wasm_fuel_limit(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonScriptConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_script_timeout_secs")]
    pub timeout_secs: u64,
    /// Optional virtualenv path. Empty means system Python.
    #[serde(default)]
    pub venv_path: String,
}

impl Default for PythonScriptConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            timeout_secs: default_script_timeout_secs(),
            venv_path: String::new(),
        }
    }
}

fn default_js_max_heap_mb() -> u64 {
    64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsScriptConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_script_timeout_secs")]
    pub timeout_secs: u64,
    /// Max heap size in MB for the JS runtime.
    #[serde(default = "default_js_max_heap_mb")]
    pub max_heap_mb: u64,
}

impl Default for JsScriptConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            timeout_secs: default_script_timeout_secs(),
            max_heap_mb: default_js_max_heap_mb(),
        }
    }
}

// ---------------------------------------------------------------------------
// Timer
// ---------------------------------------------------------------------------

fn default_timer_engine() -> String {
    "simple".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerPluginConfigs {
    #[serde(default = "default_false")]
    pub enabled: bool,
    /// Which timer engine to use (e.g. `"simple"`).
    #[serde(default = "default_timer_engine")]
    pub engine: String,
    #[serde(default)]
    pub simple: SimpleTimerConfig,
}

impl Default for TimerPluginConfigs {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            engine: default_timer_engine(),
            simple: SimpleTimerConfig::default(),
        }
    }
}

fn default_timer_thread_pool_size() -> usize {
    4
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleTimerConfig {
    /// Max concurrent timer tasks.
    #[serde(default = "default_timer_thread_pool_size")]
    pub max_concurrent: usize,
}

impl Default for SimpleTimerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: default_timer_thread_pool_size(),
        }
    }
}

// ---------------------------------------------------------------------------
// FileStore
// ---------------------------------------------------------------------------

fn default_file_store_backend() -> String {
    "local".to_string()
}

fn default_file_store_root_dir() -> String {
    "data/files".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStorePluginConfigs {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_file_store_backend")]
    pub backend: String,
    #[serde(default)]
    pub local: LocalFileStoreConfig,
}

impl Default for FileStorePluginConfigs {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            backend: default_file_store_backend(),
            local: LocalFileStoreConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalFileStoreConfig {
    #[serde(default = "default_file_store_root_dir")]
    pub root_dir: String,
}

impl Default for LocalFileStoreConfig {
    fn default() -> Self {
        Self {
            root_dir: default_file_store_root_dir(),
        }
    }
}

// ---------------------------------------------------------------------------
// HttpClient
// ---------------------------------------------------------------------------

fn default_http_client_timeout_secs() -> u64 {
    30
}

fn default_http_client_user_agent() -> String {
    "rinfra/0.1.0".to_string()
}

fn default_http_client_max_retries() -> u32 {
    0
}

fn default_http_client_retry_delay_ms() -> u64 {
    500
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpClientConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_http_client_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_http_client_user_agent")]
    pub user_agent: String,
    #[serde(default = "default_http_client_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_http_client_retry_delay_ms")]
    pub retry_delay_ms: u64,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            timeout_secs: default_http_client_timeout_secs(),
            user_agent: default_http_client_user_agent(),
            max_retries: default_http_client_max_retries(),
            retry_delay_ms: default_http_client_retry_delay_ms(),
        }
    }
}

// ---------------------------------------------------------------------------
// DistributedLock
// ---------------------------------------------------------------------------

fn default_lock_backend() -> String {
    "memory".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockPluginConfigs {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_lock_backend")]
    pub backend: String,
    #[serde(default)]
    pub redis: RedisLockConfig,
}

impl Default for LockPluginConfigs {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            backend: default_lock_backend(),
            redis: RedisLockConfig::default(),
        }
    }
}

fn default_lock_key_prefix() -> String {
    "rinfra:lock:".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisLockConfig {
    #[serde(default = "default_redis_url")]
    pub url: String,
    #[serde(default = "default_lock_key_prefix")]
    pub key_prefix: String,
}

impl Default for RedisLockConfig {
    fn default() -> Self {
        Self {
            url: default_redis_url(),
            key_prefix: default_lock_key_prefix(),
        }
    }
}

// ---------------------------------------------------------------------------
// ConfigWatch
// ---------------------------------------------------------------------------

fn default_config_watch_poll_secs() -> u64 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigWatchConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_config_watch_poll_secs")]
    pub poll_interval_secs: u64,
}

impl Default for ConfigWatchConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            poll_interval_secs: default_config_watch_poll_secs(),
        }
    }
}

// ---------------------------------------------------------------------------
// Audit
// ---------------------------------------------------------------------------

fn default_audit_backend() -> String {
    "file".to_string()
}

fn default_audit_file_path() -> String {
    "logs/audit.jsonl".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditPluginConfigs {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_audit_backend")]
    pub backend: String,
    #[serde(default)]
    pub file: AuditFileConfig,
}

impl Default for AuditPluginConfigs {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            backend: default_audit_backend(),
            file: AuditFileConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditFileConfig {
    #[serde(default = "default_audit_file_path")]
    pub path: String,
}

impl Default for AuditFileConfig {
    fn default() -> Self {
        Self {
            path: default_audit_file_path(),
        }
    }
}

// ---------------------------------------------------------------------------
// i18n
// ---------------------------------------------------------------------------

fn default_i18n_dir() -> String {
    "i18n".to_string()
}

fn default_i18n_locale() -> String {
    "en".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct I18nConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_i18n_dir")]
    pub dir: String,
    #[serde(default = "default_i18n_locale")]
    pub default_locale: String,
}

impl Default for I18nConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            dir: default_i18n_dir(),
            default_locale: default_i18n_locale(),
        }
    }
}

// ---------------------------------------------------------------------------
// Logging
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogPluginConfigs {
    #[serde(default)]
    pub stdout: StdoutLogConfig,
    #[serde(default)]
    pub file: FileLogConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdoutLogConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: String,
}

impl Default for StdoutLogConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            level: default_log_level(),
            format: default_log_format(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileLogConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default = "default_log_dir")]
    pub path: String,
    #[serde(default = "default_log_filename")]
    pub filename: String,
    #[serde(default = "default_log_rotation")]
    pub rotation: String,
    #[serde(default = "default_log_max_files")]
    pub max_files: u32,
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_json_format")]
    pub format: String,
}

impl Default for FileLogConfig {
    fn default() -> Self {
        Self {
            enabled: default_false(),
            path: default_log_dir(),
            filename: default_log_filename(),
            rotation: default_log_rotation(),
            max_files: default_log_max_files(),
            level: default_log_level(),
            format: default_log_json_format(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_has_expected_values() {
        let config = RinfraConfig::default();
        assert_eq!(config.app.name, "rinfra-app");
        assert_eq!(config.app.version, "0.1.0");
        assert!(config.plugins.log.stdout.enabled);
        assert_eq!(config.plugins.log.stdout.level, "info");
        assert_eq!(config.plugins.log.stdout.format, "pretty");
    }

    #[test]
    fn test_deserialize_partial_json() {
        let json = r#"{"app":{"name":"my-service"}}"#;
        let config: RinfraConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.app.name, "my-service");
        assert_eq!(config.app.version, "0.1.0");
    }

    #[test]
    fn test_deserialize_full_json() {
        let json = r#"{
            "app": { "name": "test-svc", "version": "2.0.0" },
            "plugins": {
                "log": { "stdout": { "enabled": false, "level": "debug", "format": "json" } }
            }
        }"#;
        let config: RinfraConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.app.name, "test-svc");
        assert_eq!(config.app.version, "2.0.0");
        assert!(!config.plugins.log.stdout.enabled);
        assert_eq!(config.plugins.log.stdout.level, "debug");
        assert_eq!(config.plugins.log.stdout.format, "json");
    }

    #[test]
    fn test_serialize_roundtrip() {
        let config = RinfraConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: RinfraConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.app.name, config.app.name);
        assert_eq!(parsed.app.version, config.app.version);
    }

    #[test]
    fn test_shutdown_config_defaults() {
        let config = RinfraConfig::default();
        assert_eq!(config.runtime.shutdown.grace_period_secs, 30);
        assert_eq!(config.runtime.shutdown.component_timeout_secs, 10);
    }

    #[test]
    fn test_net_config_default_empty_listeners() {
        let config = RinfraConfig::default();
        assert!(config.plugins.net.listeners.is_empty());
    }

    #[test]
    fn test_listener_config_deserialize() {
        let json = r#"{
            "name": "main",
            "protocol": "http",
            "bind": "0.0.0.0:8080"
        }"#;
        let cfg: ListenerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.name, "main");
        assert_eq!(cfg.protocol, ListenerProtocol::Http);
        assert_eq!(cfg.bind, "0.0.0.0:8080");
        assert!(cfg.http.is_none());
        assert!(cfg.tcp.is_none());
    }

    #[test]
    fn test_middleware_config_defaults() {
        let mw = MiddlewareConfig::default();
        assert!(mw.cors.enabled);
        assert_eq!(mw.cors.allow_origins, vec!["*"]);
        assert!(mw.request_id.enabled);
        assert!(mw.timeout.enabled);
        assert_eq!(mw.timeout.timeout_secs, 30);
    }

    #[test]
    fn test_business_config_default_is_null() {
        let config = RinfraConfig::default();
        assert!(config.business.is_null());
    }

    #[test]
    fn test_business_config_deserialized() {
        let json = r#"{
            "app": { "name": "biz-svc" },
            "business": {
                "payment_gateway": "https://pay.example.com",
                "max_retry": 3
            }
        }"#;
        let config: RinfraConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.app.name, "biz-svc");
        assert_eq!(config.business["payment_gateway"], "https://pay.example.com");
        assert_eq!(config.business["max_retry"], 3);
    }

    #[test]
    fn test_business_config_to_custom_struct() {
        #[derive(serde::Deserialize)]
        struct BizConfig {
            payment_gateway: String,
            max_retry: u32,
        }

        let json = r#"{
            "business": {
                "payment_gateway": "https://pay.example.com",
                "max_retry": 5
            }
        }"#;
        let config: RinfraConfig = serde_json::from_str(json).unwrap();
        let biz: BizConfig = serde_json::from_value(config.business).unwrap();
        assert_eq!(biz.payment_gateway, "https://pay.example.com");
        assert_eq!(biz.max_retry, 5);
    }
}
