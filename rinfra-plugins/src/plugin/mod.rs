pub mod builtin;

pub use builtin::builtin_plugins;
pub use builtin::{EnvKeyProviderPlugin, FileKeyProviderPlugin, RotatingKeyProviderPlugin};
pub use builtin::{InMemoryBusPlugin, TrpcPlugin};
pub use builtin::{JsScriptPlugin, PythonScriptPlugin, WasmScriptPlugin};
pub use builtin::JsonCodecPlugin;
pub use builtin::MemoryRateLimiterPlugin;

#[cfg(feature = "memory-cache")]
pub use builtin::MemoryCachePlugin;
#[cfg(feature = "redis")]
pub use builtin::{RedisCachePlugin, RedisRateLimiterPlugin};
#[cfg(all(feature = "memory-cache", feature = "redis"))]
pub use builtin::MultilevelCachePlugin;
#[cfg(any(feature = "postgres", feature = "mysql", feature = "sqlite"))]
pub use builtin::PostgresStorePlugin;
#[cfg(feature = "codec-msgpack")]
pub use builtin::MsgpackCodecPlugin;
#[cfg(feature = "codec-protobuf")]
pub use builtin::ProtobufCodecPlugin;
#[cfg(feature = "grpc")]
pub use builtin::GrpcPlugin;
#[cfg(feature = "crypto")]
pub use builtin::AesGcmCryptoPlugin;
