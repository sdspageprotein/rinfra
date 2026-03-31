use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use rinfra_core::config::watch::{ConfigWatcher, OnConfigReload};
use rinfra_core::config::RinfraConfig;
use rinfra_core::error::{AppError, ErrorCode};
use tracing::{debug, error, info};

/// Watches a YAML config file for changes by polling its modification time.
///
/// Uses filesystem polling instead of `inotify`/`FSEvents` for maximum
/// portability across platforms (Windows, Linux, macOS) and container
/// environments.
pub struct FileConfigWatcher {
    config_path: PathBuf,
    poll_interval: Duration,
    handlers: Arc<StdMutex<Vec<Arc<dyn OnConfigReload>>>>,
    running: AtomicBool,
    cancel: tokio_util::sync::CancellationToken,
}

impl FileConfigWatcher {
    pub fn new(config_path: impl Into<PathBuf>, poll_interval_secs: u64) -> Self {
        Self {
            config_path: config_path.into(),
            poll_interval: Duration::from_secs(poll_interval_secs.max(1)),
            handlers: Arc::new(StdMutex::new(Vec::new())),
            running: AtomicBool::new(false),
            cancel: tokio_util::sync::CancellationToken::new(),
        }
    }
}

fn read_mtime(path: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

fn parse_config(path: &PathBuf) -> Result<RinfraConfig, AppError> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        AppError::new(
            ErrorCode::ConfigReloadFailed,
            format!("failed to read config file: {e}"),
        )
    })?;
    serde_yaml::from_str::<RinfraConfig>(&content).map_err(|e| {
        AppError::new(
            ErrorCode::ConfigReloadFailed,
            format!("failed to parse config: {e}"),
        )
    })
}

#[async_trait]
impl ConfigWatcher for FileConfigWatcher {
    fn watcher_name(&self) -> &str {
        "file"
    }

    fn add_handler(&self, handler: Arc<dyn OnConfigReload>) {
        self.handlers.lock().unwrap().push(handler);
    }

    async fn start(&self) -> Result<(), AppError> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        let path = self.config_path.clone();
        let interval = self.poll_interval;
        let cancel = self.cancel.clone();
        let handlers = self.handlers.clone();

        info!(path = %path.display(), poll_secs = interval.as_secs(), "config watcher started");

        tokio::spawn(async move {
            let mut last_mtime = read_mtime(&path);

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        info!("config watcher stopped");
                        break;
                    }
                    _ = tokio::time::sleep(interval) => {
                        let current_mtime = read_mtime(&path);
                        if current_mtime != last_mtime {
                            last_mtime = current_mtime;
                            debug!(path = %path.display(), "config file changed, reloading");

                            match parse_config(&path) {
                                Ok(new_config) => {
                                    let snapshot: Vec<Arc<dyn OnConfigReload>> =
                                        handlers.lock().unwrap().clone();
                                    for h in &snapshot {
                                        h.on_reload(&new_config).await;
                                    }
                                    info!("config reloaded successfully ({} handlers notified)", snapshot.len());
                                }
                                Err(e) => {
                                    error!("config reload failed: {e}");
                                }
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    async fn stop(&self) -> Result<(), AppError> {
        self.cancel.cancel();
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    struct CountingHandler {
        count: AtomicU32,
    }

    #[async_trait]
    impl OnConfigReload for CountingHandler {
        async fn on_reload(&self, _new_config: &RinfraConfig) {
            self.count.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn test_watcher_name() {
        let w = FileConfigWatcher::new("test.yaml", 1);
        assert_eq!(w.watcher_name(), "file");
    }

    #[tokio::test]
    async fn test_add_handler() {
        let w = FileConfigWatcher::new("test.yaml", 1);
        let h: Arc<dyn OnConfigReload> = Arc::new(CountingHandler {
            count: AtomicU32::new(0),
        });
        w.add_handler(h);
        assert_eq!(w.handlers.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_stop_before_start() {
        let w = FileConfigWatcher::new("test.yaml", 1);
        w.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_detects_file_change() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("rinfra.yaml");

        let initial = RinfraConfig::default();
        let yaml = serde_yaml::to_string(&initial).unwrap();
        std::fs::write(&config_path, &yaml).unwrap();

        let handler = Arc::new(CountingHandler {
            count: AtomicU32::new(0),
        });

        let watcher = FileConfigWatcher::new(&config_path, 1);
        watcher.add_handler(handler.clone());
        watcher.start().await.unwrap();

        // Wait for the first poll to establish baseline.
        tokio::time::sleep(Duration::from_millis(1500)).await;

        // Touch the file to trigger change.
        std::fs::write(&config_path, &yaml).unwrap();
        tokio::time::sleep(Duration::from_millis(2000)).await;

        assert!(handler.count.load(Ordering::SeqCst) >= 1);

        watcher.stop().await.unwrap();
    }

    #[test]
    fn test_parse_config_valid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.yaml");
        let cfg = RinfraConfig::default();
        let yaml = serde_yaml::to_string(&cfg).unwrap();
        std::fs::write(&path, &yaml).unwrap();

        let result = parse_config(&path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_config_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.yaml");
        std::fs::write(&path, "{{{{invalid yaml").unwrap();

        let result = parse_config(&path);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::ConfigReloadFailed);
    }
}
