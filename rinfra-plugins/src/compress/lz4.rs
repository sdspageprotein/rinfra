use rinfra_core::compress::Compressor;
use rinfra_core::error::{AppError, ErrorCode};

/// LZ4 block compression — fast, low overhead.
pub struct Lz4Compressor;

impl Compressor for Lz4Compressor {
    fn name(&self) -> &str {
        "lz4"
    }

    fn compress(&self, data: &[u8]) -> Result<Vec<u8>, AppError> {
        Ok(lz4_flex::compress_prepend_size(data))
    }

    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>, AppError> {
        lz4_flex::decompress_size_prepended(data).map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("lz4 decompress failed: {e}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lz4_roundtrip() {
        let c = Lz4Compressor;
        let data = b"hello world, this is a test of lz4 compression!";
        let compressed = c.compress(data).unwrap();
        assert_ne!(compressed, data);
        let decompressed = c.decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_lz4_empty_data() {
        let c = Lz4Compressor;
        let compressed = c.compress(b"").unwrap();
        let decompressed = c.decompress(&compressed).unwrap();
        assert!(decompressed.is_empty());
    }

    #[test]
    fn test_lz4_invalid_data_errors() {
        let c = Lz4Compressor;
        assert!(c.decompress(&[0xFF, 0xFF, 0xFF, 0xFF]).is_err());
    }

    #[test]
    fn test_lz4_compression_ratio() {
        let c = Lz4Compressor;
        let data = "A".repeat(10000);
        let compressed = c.compress(data.as_bytes()).unwrap();
        assert!(compressed.len() < data.len() / 2);
    }
}
