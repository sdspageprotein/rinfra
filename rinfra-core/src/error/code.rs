use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorCode {
    // Plugin
    PluginAlreadyRegistered,
    PluginNotFound,
    PluginInitFailed,
    PluginShutdownFailed,
    PluginShutdownTimeout,

    // Event
    EventSendFailed,

    // Codec
    CodecEncodeFailed,
    CodecDecodeFailed,
    CodecNotFound,

    // Config
    ConfigFileMissing,
    ConfigParseFailed,
    ConfigValidationFailed,

    // Log
    LogInitFailed,

    // Net / Server
    ServerBindFailed,
    ServerStartFailed,

    // Store
    StoreConnectionFailed,
    StoreQueryFailed,
    StoreMigrationFailed,
    StoreNotFound,

    // Cache
    CacheGetFailed,
    CacheSetFailed,
    CacheDeleteFailed,
    CacheConnectionFailed,

    // RateLimit
    RateLimitExceeded,

    // Crypto
    CryptoEncryptFailed,
    CryptoDecryptFailed,
    CryptoKeyNotFound,

    // MessageQueue
    MqPublishFailed,
    MqSubscribeFailed,

    // WebSocket
    WsUpgradeFailed,
    WsSendFailed,

    // RPC
    RpcServerFailed,
    RpcServiceError,

    // Script
    ScriptExecFailed,
    ScriptLoadFailed,
    ScriptTimeout,

    // Cluster
    ClusterNodeNotFound,
    ClusterRegisterFailed,
    ClusterMainUnreachable,
    ClusterHeartbeatFailed,
    ClusterAuthFailed,

    // Auth
    AuthTokenMissing,
    AuthTokenInvalid,
    AuthTokenExpired,

    // Timer
    TimerScheduleFailed,
    TimerTaskNotFound,
    TimerInvalidSchedule,

    // FileStore
    FileNotFound,
    FileReadFailed,
    FileWriteFailed,
    FileDeleteFailed,

    // HttpClient
    HttpRequestFailed,
    HttpTimeout,

    // Lock
    LockAcquireFailed,
    LockConflict,

    // Config Watch
    ConfigReloadFailed,

    // Resilience
    CircuitBreakerOpen,

    // Audit
    AuditLogFailed,

    // i18n
    I18nLoadFailed,

    // Generic
    Internal,
    InvalidArgument,
}

impl ErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PluginAlreadyRegistered => "PLUGIN_ALREADY_REGISTERED",
            Self::PluginNotFound => "PLUGIN_NOT_FOUND",
            Self::PluginInitFailed => "PLUGIN_INIT_FAILED",
            Self::PluginShutdownFailed => "PLUGIN_SHUTDOWN_FAILED",
            Self::PluginShutdownTimeout => "PLUGIN_SHUTDOWN_TIMEOUT",
            Self::EventSendFailed => "EVENT_SEND_FAILED",
            Self::CodecEncodeFailed => "CODEC_ENCODE_FAILED",
            Self::CodecDecodeFailed => "CODEC_DECODE_FAILED",
            Self::CodecNotFound => "CODEC_NOT_FOUND",
            Self::ConfigFileMissing => "CONFIG_FILE_MISSING",
            Self::ConfigParseFailed => "CONFIG_PARSE_FAILED",
            Self::ConfigValidationFailed => "CONFIG_VALIDATION_FAILED",
            Self::LogInitFailed => "LOG_INIT_FAILED",
            Self::ServerBindFailed => "SERVER_BIND_FAILED",
            Self::ServerStartFailed => "SERVER_START_FAILED",
            Self::StoreConnectionFailed => "STORE_CONNECTION_FAILED",
            Self::StoreQueryFailed => "STORE_QUERY_FAILED",
            Self::StoreMigrationFailed => "STORE_MIGRATION_FAILED",
            Self::StoreNotFound => "STORE_NOT_FOUND",
            Self::CacheGetFailed => "CACHE_GET_FAILED",
            Self::CacheSetFailed => "CACHE_SET_FAILED",
            Self::CacheDeleteFailed => "CACHE_DELETE_FAILED",
            Self::CacheConnectionFailed => "CACHE_CONNECTION_FAILED",
            Self::RateLimitExceeded => "RATE_LIMIT_EXCEEDED",
            Self::CryptoEncryptFailed => "CRYPTO_ENCRYPT_FAILED",
            Self::CryptoDecryptFailed => "CRYPTO_DECRYPT_FAILED",
            Self::CryptoKeyNotFound => "CRYPTO_KEY_NOT_FOUND",
            Self::MqPublishFailed => "MQ_PUBLISH_FAILED",
            Self::MqSubscribeFailed => "MQ_SUBSCRIBE_FAILED",
            Self::WsUpgradeFailed => "WS_UPGRADE_FAILED",
            Self::WsSendFailed => "WS_SEND_FAILED",
            Self::RpcServerFailed => "RPC_SERVER_FAILED",
            Self::RpcServiceError => "RPC_SERVICE_ERROR",
            Self::ScriptExecFailed => "SCRIPT_EXEC_FAILED",
            Self::ScriptLoadFailed => "SCRIPT_LOAD_FAILED",
            Self::ScriptTimeout => "SCRIPT_TIMEOUT",
            Self::ClusterNodeNotFound => "CLUSTER_NODE_NOT_FOUND",
            Self::ClusterRegisterFailed => "CLUSTER_REGISTER_FAILED",
            Self::ClusterMainUnreachable => "CLUSTER_MAIN_UNREACHABLE",
            Self::ClusterHeartbeatFailed => "CLUSTER_HEARTBEAT_FAILED",
            Self::ClusterAuthFailed => "CLUSTER_AUTH_FAILED",
            Self::AuthTokenMissing => "AUTH_TOKEN_MISSING",
            Self::AuthTokenInvalid => "AUTH_TOKEN_INVALID",
            Self::AuthTokenExpired => "AUTH_TOKEN_EXPIRED",
            Self::TimerScheduleFailed => "TIMER_SCHEDULE_FAILED",
            Self::TimerTaskNotFound => "TIMER_TASK_NOT_FOUND",
            Self::TimerInvalidSchedule => "TIMER_INVALID_SCHEDULE",
            Self::FileNotFound => "FILE_NOT_FOUND",
            Self::FileReadFailed => "FILE_READ_FAILED",
            Self::FileWriteFailed => "FILE_WRITE_FAILED",
            Self::FileDeleteFailed => "FILE_DELETE_FAILED",
            Self::HttpRequestFailed => "HTTP_REQUEST_FAILED",
            Self::HttpTimeout => "HTTP_TIMEOUT",
            Self::LockAcquireFailed => "LOCK_ACQUIRE_FAILED",
            Self::LockConflict => "LOCK_CONFLICT",
            Self::ConfigReloadFailed => "CONFIG_RELOAD_FAILED",
            Self::CircuitBreakerOpen => "CIRCUIT_BREAKER_OPEN",
            Self::AuditLogFailed => "AUDIT_LOG_FAILED",
            Self::I18nLoadFailed => "I18N_LOAD_FAILED",
            Self::Internal => "INTERNAL",
            Self::InvalidArgument => "INVALID_ARGUMENT",
        }
    }

    pub fn http_status(&self) -> u16 {
        match self {
            Self::PluginAlreadyRegistered => 409,
            Self::PluginNotFound => 404,
            Self::PluginInitFailed => 500,
            Self::PluginShutdownFailed => 500,
            Self::PluginShutdownTimeout => 504,
            Self::EventSendFailed => 500,
            Self::CodecEncodeFailed => 500,
            Self::CodecDecodeFailed => 400,
            Self::CodecNotFound => 404,
            Self::ConfigFileMissing => 404,
            Self::ConfigParseFailed => 400,
            Self::ConfigValidationFailed => 400,
            Self::LogInitFailed => 500,
            Self::ServerBindFailed => 500,
            Self::ServerStartFailed => 500,
            Self::StoreConnectionFailed => 500,
            Self::StoreQueryFailed => 500,
            Self::StoreMigrationFailed => 500,
            Self::StoreNotFound => 404,
            Self::CacheGetFailed => 500,
            Self::CacheSetFailed => 500,
            Self::CacheDeleteFailed => 500,
            Self::CacheConnectionFailed => 500,
            Self::RateLimitExceeded => 429,
            Self::CryptoEncryptFailed => 500,
            Self::CryptoDecryptFailed => 500,
            Self::CryptoKeyNotFound => 404,
            Self::MqPublishFailed => 500,
            Self::MqSubscribeFailed => 500,
            Self::WsUpgradeFailed => 400,
            Self::WsSendFailed => 500,
            Self::RpcServerFailed => 500,
            Self::RpcServiceError => 500,
            Self::ScriptExecFailed => 500,
            Self::ScriptLoadFailed => 400,
            Self::ScriptTimeout => 504,
            Self::ClusterNodeNotFound => 404,
            Self::ClusterRegisterFailed => 500,
            Self::ClusterMainUnreachable => 503,
            Self::ClusterHeartbeatFailed => 500,
            Self::ClusterAuthFailed => 401,
            Self::AuthTokenMissing => 401,
            Self::AuthTokenInvalid => 401,
            Self::AuthTokenExpired => 401,
            Self::TimerScheduleFailed => 500,
            Self::TimerTaskNotFound => 404,
            Self::TimerInvalidSchedule => 400,
            Self::FileNotFound => 404,
            Self::FileReadFailed => 500,
            Self::FileWriteFailed => 500,
            Self::FileDeleteFailed => 500,
            Self::HttpRequestFailed => 502,
            Self::HttpTimeout => 504,
            Self::LockAcquireFailed => 500,
            Self::LockConflict => 409,
            Self::ConfigReloadFailed => 500,
            Self::CircuitBreakerOpen => 503,
            Self::AuditLogFailed => 500,
            Self::I18nLoadFailed => 500,
            Self::Internal => 500,
            Self::InvalidArgument => 400,
        }
    }
}

impl ErrorCode {
    /// Returns the i18n message key for this error code.
    /// Convention: `error.{LOWER_SNAKE_CASE}` (e.g. `error.plugin_not_found`).
    pub fn i18n_key(&self) -> String {
        format!("error.{}", self.as_str().to_lowercase())
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_as_str_returns_upper_snake_case() {
        assert_eq!(ErrorCode::PluginAlreadyRegistered.as_str(), "PLUGIN_ALREADY_REGISTERED");
        assert_eq!(ErrorCode::PluginInitFailed.as_str(), "PLUGIN_INIT_FAILED");
        assert_eq!(ErrorCode::EventSendFailed.as_str(), "EVENT_SEND_FAILED");
        assert_eq!(ErrorCode::Internal.as_str(), "INTERNAL");
        assert_eq!(ErrorCode::InvalidArgument.as_str(), "INVALID_ARGUMENT");
    }

    #[test]
    fn test_error_code_http_status_mapping() {
        assert_eq!(ErrorCode::PluginAlreadyRegistered.http_status(), 409);
        assert_eq!(ErrorCode::PluginNotFound.http_status(), 404);
        assert_eq!(ErrorCode::Internal.http_status(), 500);
        assert_eq!(ErrorCode::InvalidArgument.http_status(), 400);
        assert_eq!(ErrorCode::PluginShutdownTimeout.http_status(), 504);
    }

    #[test]
    fn test_error_code_display_matches_as_str() {
        let code = ErrorCode::PluginInitFailed;
        assert_eq!(format!("{code}"), code.as_str());
    }

    #[test]
    fn test_error_code_i18n_key() {
        assert_eq!(
            ErrorCode::PluginNotFound.i18n_key(),
            "error.plugin_not_found"
        );
        assert_eq!(ErrorCode::Internal.i18n_key(), "error.internal");
        assert_eq!(
            ErrorCode::RateLimitExceeded.i18n_key(),
            "error.rate_limit_exceeded"
        );
    }

    #[test]
    fn test_error_code_serialize_json() {
        let code = ErrorCode::PluginNotFound;
        let json = serde_json::to_string(&code).unwrap();
        assert_eq!(json, "\"PluginNotFound\"");
    }

    #[test]
    fn test_error_code_deserialize_json() {
        let code: ErrorCode = serde_json::from_str("\"Internal\"").unwrap();
        assert_eq!(code, ErrorCode::Internal);
    }

    #[test]
    fn test_error_code_equality() {
        assert_eq!(ErrorCode::Internal, ErrorCode::Internal);
        assert_ne!(ErrorCode::Internal, ErrorCode::InvalidArgument);
    }
}
