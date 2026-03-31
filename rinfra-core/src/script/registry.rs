use std::collections::HashMap;
use std::sync::Arc;

use tracing::info;

use super::ScriptEngine;
use crate::error::{AppError, ErrorCode};

/// Registry that holds multiple named script engines.
/// Similar to `CodecRegistry`, enables multi-engine script execution.
pub struct ScriptEngineRegistry {
    engines: HashMap<String, Arc<dyn ScriptEngine>>,
}

impl ScriptEngineRegistry {
    pub fn new() -> Self {
        Self {
            engines: HashMap::new(),
        }
    }

    pub fn register(&mut self, engine: Arc<dyn ScriptEngine>) -> Result<(), AppError> {
        let name = engine.engine_name().to_string();
        if self.engines.contains_key(&name) {
            return Err(AppError::new(
                ErrorCode::PluginAlreadyRegistered,
                format!("script engine '{name}' is already registered"),
            ));
        }
        info!(engine = %name, "script engine registered");
        self.engines.insert(name, engine);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn ScriptEngine>> {
        self.engines.get(name)
    }

    pub fn list_names(&self) -> Vec<&str> {
        self.engines.keys().map(|s| s.as_str()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.engines.is_empty()
    }
}

impl Default for ScriptEngineRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::ScriptOutput;
    use async_trait::async_trait;

    struct StubEngine;

    #[async_trait]
    impl ScriptEngine for StubEngine {
        async fn execute(&self, _script: &[u8], input: &[u8]) -> Result<ScriptOutput, AppError> {
            Ok(ScriptOutput {
                stdout: String::new(),
                result: input.to_vec(),
                exit_code: 0,
            })
        }
        fn engine_name(&self) -> &str {
            "stub"
        }
    }

    #[test]
    fn test_register_and_get() {
        let mut reg = ScriptEngineRegistry::new();
        reg.register(Arc::new(StubEngine)).unwrap();
        assert!(reg.get("stub").is_some());
        assert!(reg.get("missing").is_none());
    }

    #[test]
    fn test_duplicate_returns_error() {
        let mut reg = ScriptEngineRegistry::new();
        reg.register(Arc::new(StubEngine)).unwrap();

        struct StubEngine2;
        #[async_trait]
        impl ScriptEngine for StubEngine2 {
            async fn execute(
                &self,
                _script: &[u8],
                _input: &[u8],
            ) -> Result<ScriptOutput, AppError> {
                unreachable!()
            }
            fn engine_name(&self) -> &str {
                "stub"
            }
        }

        let result = reg.register(Arc::new(StubEngine2));
        assert!(result.is_err());
    }

    #[test]
    fn test_list_names() {
        let mut reg = ScriptEngineRegistry::new();
        reg.register(Arc::new(StubEngine)).unwrap();
        let names = reg.list_names();
        assert_eq!(names.len(), 1);
        assert!(names.contains(&"stub"));
    }

    #[test]
    fn test_empty_registry() {
        let reg = ScriptEngineRegistry::new();
        assert!(reg.is_empty());
        assert!(reg.list_names().is_empty());
    }
}
