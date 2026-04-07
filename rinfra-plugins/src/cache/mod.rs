#[cfg(feature = "memory-cache")]
mod memory;
mod multilevel;
#[cfg(feature = "redis")]
mod redis_cache;

#[cfg(feature = "memory-cache")]
pub use memory::MemoryCache;
pub use multilevel::MultilevelCache;
#[cfg(feature = "redis")]
pub use redis_cache::RedisCache;
