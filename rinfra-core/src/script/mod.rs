mod registry;

use async_trait::async_trait;
use crate::error::AppError;

pub use registry::ScriptEngineRegistry;

/// Result of a script execution.
#[derive(Debug, Clone)]
pub struct ScriptOutput {
    pub stdout: String,
    pub result: Vec<u8>,
    pub exit_code: i32,
}

/// Pluggable script execution engine.
#[async_trait]
pub trait ScriptEngine: Send + Sync + 'static {
    /// Execute a script from bytes with optional input data.
    async fn execute(&self, script: &[u8], input: &[u8]) -> Result<ScriptOutput, AppError>;

    /// Return the engine name (e.g. "wasm", "python", "javascript").
    fn engine_name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_output() {
        let output = ScriptOutput {
            stdout: "hello".to_string(),
            result: vec![1, 2, 3],
            exit_code: 0,
        };
        assert_eq!(output.exit_code, 0);
        assert_eq!(output.stdout, "hello");
    }
}
