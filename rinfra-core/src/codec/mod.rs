mod registry;

pub use registry::CodecRegistry;

use crate::error::AppError;

/// Pluggable serialization/deserialization trait.
/// Uses `serde_json::Value` as the common intermediate representation to
/// stay trait-object compatible (avoiding generic parameters on the trait).
pub trait Codec: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn content_type(&self) -> &str;
    fn encode_value(&self, value: &serde_json::Value) -> Result<Vec<u8>, AppError>;
    fn decode_value(&self, data: &[u8]) -> Result<serde_json::Value, AppError>;
}
