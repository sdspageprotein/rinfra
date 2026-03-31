use std::io::{Read, Write};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use rinfra_core::compress::Compressor;
use rinfra_core::error::{AppError, ErrorCode};

/// Gzip compression — widely compatible, good compression ratio.
pub struct GzipCompressor;

impl Compressor for GzipCompressor {
    fn name(&self) -> &str {
        "gzip"
    }

    fn compress(&self, data: &[u8]) -> Result<Vec<u8>, AppError> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(data).map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("gzip compress failed: {e}"))
        })?;
        encoder.finish().map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("gzip compress finish failed: {e}"))
        })
    }

    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>, AppError> {
        let mut decoder = GzDecoder::new(data);
        let mut out = Vec::new();
        decoder.read_to_end(&mut out).map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("gzip decompress failed: {e}"))
        })?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gzip_roundtrip() {
        let c = GzipCompressor;
        let data = b"hello world, this is a test of gzip compression!";
        let compressed = c.compress(data).unwrap();
        assert_ne!(compressed, data.as_slice());
        let decompressed = c.decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_gzip_empty_data() {
        let c = GzipCompressor;
        let compressed = c.compress(b"").unwrap();
        let decompressed = c.decompress(&compressed).unwrap();
        assert!(decompressed.is_empty());
    }

    #[test]
    fn test_gzip_invalid_data_errors() {
        let c = GzipCompressor;
        assert!(c.decompress(&[0xFF, 0xFF, 0xFF, 0xFF]).is_err());
    }

    #[test]
    fn test_gzip_compression_ratio() {
        let c = GzipCompressor;
        let data = "A".repeat(10000);
        let compressed = c.compress(data.as_bytes()).unwrap();
        assert!(compressed.len() < data.len() / 2);
    }
}
