mod memory;
#[cfg(feature = "redis")]
mod redis_lock;

pub use memory::InMemoryLock;
#[cfg(feature = "redis")]
pub use redis_lock::RedisLock;
