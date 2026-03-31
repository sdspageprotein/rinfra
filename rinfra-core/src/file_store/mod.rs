use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Metadata about a stored file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub size: u64,
    pub last_modified: Option<u64>,
    pub content_type: Option<String>,
    pub is_dir: bool,
}

/// Pluggable file / object storage abstraction.
///
/// Covers both local filesystem and remote object stores (S3, MinIO, etc.)
/// behind a unified async API.
#[async_trait]
pub trait FileStore: Send + Sync + 'static {
    /// Backend name (e.g. `"local"`, `"s3"`).
    fn store_name(&self) -> &str;

    /// Store a file at the given path. Overwrites if exists.
    async fn put(&self, path: &str, data: &[u8]) -> Result<(), AppError>;

    /// Read a file's full content.
    async fn get(&self, path: &str) -> Result<Vec<u8>, AppError>;

    /// Delete a file. Returns `true` if the file existed.
    async fn delete(&self, path: &str) -> Result<bool, AppError>;

    /// Check whether a file exists at the given path.
    async fn exists(&self, path: &str) -> Result<bool, AppError>;

    /// List files under a prefix / directory.
    async fn list(&self, prefix: &str) -> Result<Vec<FileInfo>, AppError>;

    /// Get metadata for a single file.
    async fn metadata(&self, path: &str) -> Result<FileInfo, AppError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_info_serde() {
        let info = FileInfo {
            path: "uploads/test.txt".into(),
            size: 1024,
            last_modified: Some(1700000000),
            content_type: Some("text/plain".into()),
            is_dir: false,
        };
        let json = serde_json::to_string(&info).unwrap();
        let decoded: FileInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.path, "uploads/test.txt");
        assert_eq!(decoded.size, 1024);
        assert!(!decoded.is_dir);
    }

    #[test]
    fn test_file_info_dir() {
        let info = FileInfo {
            path: "uploads/".into(),
            size: 0,
            last_modified: None,
            content_type: None,
            is_dir: true,
        };
        assert!(info.is_dir);
    }
}
