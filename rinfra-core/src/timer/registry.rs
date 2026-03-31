use std::collections::HashMap;
use std::sync::Arc;

use tracing::info;

use super::TimerEngine;
use crate::error::{AppError, ErrorCode};

/// Registry that holds named timer engines.
pub struct TimerEngineRegistry {
    engines: HashMap<String, Arc<dyn TimerEngine>>,
}

impl TimerEngineRegistry {
    pub fn new() -> Self {
        Self {
            engines: HashMap::new(),
        }
    }

    pub fn register(&mut self, engine: Arc<dyn TimerEngine>) -> Result<(), AppError> {
        let name = engine.engine_name().to_string();
        if self.engines.contains_key(&name) {
            return Err(AppError::new(
                ErrorCode::PluginAlreadyRegistered,
                format!("timer engine '{name}' is already registered"),
            ));
        }
        info!(engine = %name, "timer engine registered");
        self.engines.insert(name, engine);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn TimerEngine>> {
        self.engines.get(name)
    }

    pub fn list_names(&self) -> Vec<&str> {
        self.engines.keys().map(|s| s.as_str()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.engines.is_empty()
    }

    /// Shutdown all registered engines.
    pub async fn shutdown_all(&self) {
        for (name, engine) in &self.engines {
            if let Err(e) = engine.shutdown().await {
                tracing::error!(engine = %name, error = %e, "timer engine shutdown failed");
            }
        }
    }
}

impl Default for TimerEngineRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timer::{TimerHandler, TimerTask, TimerTaskInfo};
    use async_trait::async_trait;

    struct StubTimerEngine;

    #[async_trait]
    impl TimerEngine for StubTimerEngine {
        fn engine_name(&self) -> &str {
            "stub"
        }
        async fn schedule(
            &self,
            _task: TimerTask,
            _handler: Arc<dyn TimerHandler>,
        ) -> Result<(), AppError> {
            Ok(())
        }
        async fn cancel(&self, _task_id: &str) -> Result<bool, AppError> {
            Ok(false)
        }
        async fn list_tasks(&self) -> Vec<TimerTaskInfo> {
            vec![]
        }
        async fn shutdown(&self) -> Result<(), AppError> {
            Ok(())
        }
    }

    #[test]
    fn test_register_and_get() {
        let mut reg = TimerEngineRegistry::new();
        reg.register(Arc::new(StubTimerEngine)).unwrap();
        assert!(reg.get("stub").is_some());
        assert!(reg.get("missing").is_none());
    }

    #[test]
    fn test_duplicate_returns_error() {
        let mut reg = TimerEngineRegistry::new();
        reg.register(Arc::new(StubTimerEngine)).unwrap();
        let result = reg.register(Arc::new(StubTimerEngine));
        assert!(result.is_err());
    }

    #[test]
    fn test_list_names() {
        let mut reg = TimerEngineRegistry::new();
        reg.register(Arc::new(StubTimerEngine)).unwrap();
        let names = reg.list_names();
        assert_eq!(names.len(), 1);
        assert!(names.contains(&"stub"));
    }

    #[test]
    fn test_empty_registry() {
        let reg = TimerEngineRegistry::new();
        assert!(reg.is_empty());
        assert!(reg.list_names().is_empty());
    }
}
