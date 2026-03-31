use std::path::PathBuf;

use async_trait::async_trait;
use rinfra_core::audit::{AuditEvent, AuditFilter, AuditLogger};
use rinfra_core::error::{AppError, ErrorCode};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

/// Audit logger that appends JSON-line records to a file.
///
/// Each audit event is written as a single JSON line, making the log
/// easy to parse with tools like `jq` or ship to a log aggregation
/// system.
pub struct FileAuditLogger {
    path: PathBuf,
    writer: Mutex<Option<tokio::fs::File>>,
}

impl FileAuditLogger {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            writer: Mutex::new(None),
        }
    }

    async fn ensure_writer(&self) -> Result<(), AppError> {
        let mut guard = self.writer.lock().await;
        if guard.is_none() {
            if let Some(parent) = self.path.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| {
                    AppError::new(
                        ErrorCode::AuditLogFailed,
                        format!("failed to create audit log directory: {e}"),
                    )
                })?;
            }
            let file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
                .await
                .map_err(|e| {
                    AppError::new(
                        ErrorCode::AuditLogFailed,
                        format!("failed to open audit log file: {e}"),
                    )
                })?;
            *guard = Some(file);
        }
        Ok(())
    }
}

#[async_trait]
impl AuditLogger for FileAuditLogger {
    fn logger_name(&self) -> &str {
        "file"
    }

    async fn log(&self, event: AuditEvent) -> Result<(), AppError> {
        self.ensure_writer().await?;

        let mut line = serde_json::to_string(&event).map_err(|e| {
            AppError::new(
                ErrorCode::AuditLogFailed,
                format!("failed to serialize audit event: {e}"),
            )
        })?;
        line.push('\n');

        let mut guard = self.writer.lock().await;
        if let Some(ref mut file) = *guard {
            file.write_all(line.as_bytes()).await.map_err(|e| {
                AppError::new(
                    ErrorCode::AuditLogFailed,
                    format!("failed to write audit event: {e}"),
                )
            })?;
            file.flush().await.ok();
        }
        Ok(())
    }

    async fn query(&self, filter: &AuditFilter) -> Result<Vec<AuditEvent>, AppError> {
        let content = tokio::fs::read_to_string(&self.path).await.unwrap_or_default();
        let mut results = Vec::new();

        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(event) = serde_json::from_str::<AuditEvent>(line) {
                if filter.matches(&event) {
                    results.push(event);
                    if results.len() >= filter.limit {
                        break;
                    }
                }
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinfra_core::audit::AuditOutcome;

    async fn temp_logger() -> (FileAuditLogger, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");
        let logger = FileAuditLogger::new(&path);
        (logger, dir)
    }

    #[tokio::test]
    async fn test_log_and_query() {
        let (logger, _dir) = temp_logger().await;

        let event = AuditEvent::new("admin", "user.create", "user", AuditOutcome::Success)
            .resource_id("u-1");
        logger.log(event).await.unwrap();

        let event2 = AuditEvent::new("system", "config.reload", "config", AuditOutcome::Failure);
        logger.log(event2).await.unwrap();

        let all = logger.query(&AuditFilter::new()).await.unwrap();
        assert_eq!(all.len(), 2);

        let mut filter = AuditFilter::new();
        filter.actor = Some("admin".into());
        let filtered = logger.query(&filter).await.unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].action, "user.create");
    }

    #[tokio::test]
    async fn test_log_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/deep/audit.jsonl");
        let logger = FileAuditLogger::new(&path);

        let event = AuditEvent::new("test", "test.op", "test", AuditOutcome::Success);
        logger.log(event).await.unwrap();

        assert!(path.exists());
    }

    #[tokio::test]
    async fn test_query_empty_file() {
        let (logger, _dir) = temp_logger().await;
        let results = logger.query(&AuditFilter::new()).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_query_limit() {
        let (logger, _dir) = temp_logger().await;

        for i in 0..10 {
            let event = AuditEvent::new(
                format!("user-{i}"),
                "action",
                "resource",
                AuditOutcome::Success,
            );
            logger.log(event).await.unwrap();
        }

        let mut filter = AuditFilter::new();
        filter.limit = 3;
        let results = logger.query(&filter).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_logger_name() {
        let logger = FileAuditLogger::new("/tmp/audit.jsonl");
        assert_eq!(logger.logger_name(), "file");
    }
}
