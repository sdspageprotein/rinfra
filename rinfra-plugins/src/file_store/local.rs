use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use async_trait::async_trait;
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::file_store::{FileInfo, FileStore};

/// File store backed by the local filesystem.
///
/// All paths are resolved relative to `root_dir`. Parent directories
/// are created automatically on `put()`.
pub struct LocalFileStore {
    root: PathBuf,
}

impl LocalFileStore {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root: root_dir.into(),
        }
    }

    fn resolve(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }
}

#[async_trait]
impl FileStore for LocalFileStore {
    fn store_name(&self) -> &str {
        "local"
    }

    async fn put(&self, path: &str, data: &[u8]) -> Result<(), AppError> {
        let full = self.resolve(path);
        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                AppError::new(
                    ErrorCode::FileWriteFailed,
                    format!("failed to create directory: {e}"),
                )
            })?;
        }
        tokio::fs::write(&full, data).await.map_err(|e| {
            AppError::new(
                ErrorCode::FileWriteFailed,
                format!("failed to write '{path}': {e}"),
            )
        })
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>, AppError> {
        let full = self.resolve(path);
        tokio::fs::read(&full).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AppError::new(ErrorCode::FileNotFound, format!("file not found: '{path}'"))
            } else {
                AppError::new(
                    ErrorCode::FileReadFailed,
                    format!("failed to read '{path}': {e}"),
                )
            }
        })
    }

    async fn delete(&self, path: &str) -> Result<bool, AppError> {
        let full = self.resolve(path);
        match tokio::fs::remove_file(&full).await {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(AppError::new(
                ErrorCode::FileDeleteFailed,
                format!("failed to delete '{path}': {e}"),
            )),
        }
    }

    async fn exists(&self, path: &str) -> Result<bool, AppError> {
        let full = self.resolve(path);
        Ok(full.exists())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<FileInfo>, AppError> {
        let dir = self.resolve(prefix);
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut entries = Vec::new();
        let mut read_dir = tokio::fs::read_dir(&dir).await.map_err(|e| {
            AppError::new(
                ErrorCode::FileReadFailed,
                format!("failed to list '{prefix}': {e}"),
            )
        })?;
        while let Some(entry) = read_dir.next_entry().await.map_err(|e| {
            AppError::new(
                ErrorCode::FileReadFailed,
                format!("failed to read directory entry: {e}"),
            )
        })? {
            let meta = entry.metadata().await.map_err(|e| {
                AppError::new(
                    ErrorCode::FileReadFailed,
                    format!("failed to read metadata: {e}"),
                )
            })?;
            let relative = make_relative(&self.root, &entry.path());
            entries.push(FileInfo {
                path: relative,
                size: meta.len(),
                last_modified: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs()),
                content_type: None,
                is_dir: meta.is_dir(),
            });
        }
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(entries)
    }

    async fn metadata(&self, path: &str) -> Result<FileInfo, AppError> {
        let full = self.resolve(path);
        let meta = tokio::fs::metadata(&full).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AppError::new(ErrorCode::FileNotFound, format!("file not found: '{path}'"))
            } else {
                AppError::new(
                    ErrorCode::FileReadFailed,
                    format!("failed to read metadata for '{path}': {e}"),
                )
            }
        })?;
        Ok(FileInfo {
            path: path.to_string(),
            size: meta.len(),
            last_modified: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
            content_type: None,
            is_dir: meta.is_dir(),
        })
    }
}

fn make_relative(root: &Path, full: &Path) -> String {
    full.strip_prefix(root)
        .unwrap_or(full)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn temp_store() -> (LocalFileStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalFileStore::new(dir.path());
        (store, dir)
    }

    #[tokio::test]
    async fn test_put_and_get() {
        let (store, _dir) = temp_store().await;
        store.put("hello.txt", b"world").await.unwrap();
        let data = store.get("hello.txt").await.unwrap();
        assert_eq!(data, b"world");
    }

    #[tokio::test]
    async fn test_put_creates_dirs() {
        let (store, _dir) = temp_store().await;
        store
            .put("a/b/c/deep.txt", b"nested")
            .await
            .unwrap();
        let data = store.get("a/b/c/deep.txt").await.unwrap();
        assert_eq!(data, b"nested");
    }

    #[tokio::test]
    async fn test_get_not_found() {
        let (store, _dir) = temp_store().await;
        let err = store.get("missing.txt").await.unwrap_err();
        assert_eq!(err.code, ErrorCode::FileNotFound);
    }

    #[tokio::test]
    async fn test_delete_existing() {
        let (store, _dir) = temp_store().await;
        store.put("del.txt", b"data").await.unwrap();
        assert!(store.delete("del.txt").await.unwrap());
        assert!(!store.exists("del.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_delete_missing() {
        let (store, _dir) = temp_store().await;
        assert!(!store.delete("nope.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_exists() {
        let (store, _dir) = temp_store().await;
        assert!(!store.exists("x.txt").await.unwrap());
        store.put("x.txt", b"").await.unwrap();
        assert!(store.exists("x.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_list() {
        let (store, _dir) = temp_store().await;
        store.put("dir/a.txt", b"a").await.unwrap();
        store.put("dir/b.txt", b"bb").await.unwrap();

        let items = store.list("dir").await.unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].path, "dir/a.txt");
        assert_eq!(items[1].path, "dir/b.txt");
    }

    #[tokio::test]
    async fn test_list_empty_dir() {
        let (store, _dir) = temp_store().await;
        let items = store.list("nonexistent").await.unwrap();
        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn test_metadata() {
        let (store, _dir) = temp_store().await;
        store.put("meta.txt", b"12345").await.unwrap();
        let info = store.metadata("meta.txt").await.unwrap();
        assert_eq!(info.size, 5);
        assert!(!info.is_dir);
        assert!(info.last_modified.is_some());
    }

    #[tokio::test]
    async fn test_metadata_not_found() {
        let (store, _dir) = temp_store().await;
        let err = store.metadata("missing").await.unwrap_err();
        assert_eq!(err.code, ErrorCode::FileNotFound);
    }

    #[test]
    fn test_store_name() {
        let store = LocalFileStore::new("/tmp/test");
        assert_eq!(store.store_name(), "local");
    }
}
