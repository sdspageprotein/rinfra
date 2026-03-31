use async_trait::async_trait;
use rinfra_core::config::WasmScriptConfig;
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::script::{ScriptEngine, ScriptOutput};
use tracing::info;

/// WASM script execution engine.
/// In P2b this is a structural stub validating the interface.
/// Full wasmtime integration will be added when the dependency is stabilized.
pub struct WasmEngine {
    config: WasmScriptConfig,
}

impl WasmEngine {
    pub fn new(config: WasmScriptConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl ScriptEngine for WasmEngine {
    async fn execute(&self, script: &[u8], input: &[u8]) -> Result<ScriptOutput, AppError> {
        if !self.config.enabled {
            return Err(AppError::new(
                ErrorCode::ScriptExecFailed,
                "WASM script engine is disabled",
            ));
        }

        if script.is_empty() {
            return Err(AppError::new(
                ErrorCode::ScriptLoadFailed,
                "empty script module",
            ));
        }

        info!(
            engine = "wasm",
            script_size = script.len(),
            input_size = input.len(),
            fuel_limit = self.config.fuel_limit,
            "executing WASM script"
        );

        // Stub: echo input as result, no actual WASM execution
        // Real implementation will use wasmtime to:
        // 1. Compile the WASM module
        // 2. Create a store with fuel limits
        // 3. Instantiate and call the entry point
        // 4. Collect stdout and return value
        Ok(ScriptOutput {
            stdout: String::new(),
            result: input.to_vec(),
            exit_code: 0,
        })
    }

    fn engine_name(&self) -> &str {
        "wasm"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disabled_config() -> WasmScriptConfig {
        WasmScriptConfig::default()
    }

    fn enabled_config() -> WasmScriptConfig {
        WasmScriptConfig {
            enabled: true,
            timeout_secs: 10,
            fuel_limit: 100_000,
        }
    }

    #[tokio::test]
    async fn test_wasm_engine_disabled() {
        let engine = WasmEngine::new(disabled_config());
        let result = engine.execute(b"module", b"input").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::ScriptExecFailed);
    }

    #[tokio::test]
    async fn test_wasm_engine_empty_script() {
        let engine = WasmEngine::new(enabled_config());
        let result = engine.execute(b"", b"input").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::ScriptLoadFailed);
    }

    #[tokio::test]
    async fn test_wasm_engine_execute_stub() {
        let engine = WasmEngine::new(enabled_config());
        let result = engine.execute(b"fake-wasm", b"hello").await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.result, b"hello");
    }

    #[test]
    fn test_wasm_engine_name() {
        let engine = WasmEngine::new(disabled_config());
        assert_eq!(engine.engine_name(), "wasm");
    }
}
