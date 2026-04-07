#[cfg(feature = "compress")]
mod gzip;
#[cfg(feature = "compress")]
mod lz4;

#[cfg(feature = "compress")]
pub use self::gzip::GzipCompressor;
#[cfg(feature = "compress")]
pub use self::lz4::Lz4Compressor;

use std::sync::Arc;
use rinfra_core::compress::Compressor;

pub fn builtin_compressors() -> Vec<Arc<dyn Compressor>> {
    #[allow(unused_mut)]
    let mut v: Vec<Arc<dyn Compressor>> = Vec::new();
    #[cfg(feature = "compress")]
    {
        v.push(Arc::new(Lz4Compressor));
        v.push(Arc::new(GzipCompressor));
    }
    v
}
