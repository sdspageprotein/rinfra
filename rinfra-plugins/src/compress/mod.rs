mod gzip;
mod lz4;

pub use self::gzip::GzipCompressor;
pub use self::lz4::Lz4Compressor;

use std::sync::Arc;
use rinfra_core::compress::Compressor;

pub fn builtin_compressors() -> Vec<Arc<dyn Compressor>> {
    vec![Arc::new(Lz4Compressor), Arc::new(GzipCompressor)]
}
