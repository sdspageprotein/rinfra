use async_trait::async_trait;
use rinfra_core::config::PythonScriptConfig;
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::script::{ScriptEngine, ScriptOutput};
use tokio::sync::Mutex;

use super::subprocess::{self, ScriptWorker};

/// Python script engine — runs scripts via a persistent `python` worker process.
///
/// The worker stays alive between calls, avoiding repeated interpreter startup.
/// Scripts receive input through the `INPUT` global (bytes) and should write
/// results to `stdout` (`print()`). Diagnostic / log output goes to `stderr`
/// and is captured in `ScriptOutput::stdout`.
pub struct PythonEngine {
    config: PythonScriptConfig,
    worker: Mutex<Option<ScriptWorker>>,
}

impl PythonEngine {
    pub fn new(config: PythonScriptConfig) -> Self {
        Self {
            config,
            worker: Mutex::new(None),
        }
    }

    fn resolve_command(&self) -> String {
        if !self.config.venv_path.is_empty() {
            #[cfg(unix)]
            {
                format!("{}/bin/python", self.config.venv_path)
            }
            #[cfg(windows)]
            {
                format!("{}\\Scripts\\python.exe", self.config.venv_path)
            }
        } else {
            #[cfg(unix)]
            {
                "python3".to_string()
            }
            #[cfg(windows)]
            {
                "python".to_string()
            }
        }
    }
}

#[async_trait]
impl ScriptEngine for PythonEngine {
    async fn execute(&self, script: &[u8], input: &[u8]) -> Result<ScriptOutput, AppError> {
        if !self.config.enabled {
            return Err(AppError::new(
                ErrorCode::ScriptExecFailed,
                "Python script engine is disabled",
            ));
        }

        if script.is_empty() {
            return Err(AppError::new(
                ErrorCode::ScriptLoadFailed,
                "empty Python script",
            ));
        }

        let mut guard = self.worker.lock().await;

        if guard.as_mut().map_or(true, |w| !w.is_alive()) {
            let cmd = self.resolve_command();
            tracing::info!(command = %cmd, "spawning persistent Python worker");
            *guard = Some(
                ScriptWorker::spawn(&cmd, &[], subprocess::python_harness(), "_worker.py").await?,
            );
        }

        let worker = guard.as_mut().unwrap();
        match worker
            .execute(script, input, self.config.timeout_secs)
            .await
        {
            Ok(output) => Ok(output),
            Err(e) => {
                *guard = None;
                Err(e)
            }
        }
    }

    fn engine_name(&self) -> &str {
        "python"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disabled_config() -> PythonScriptConfig {
        PythonScriptConfig::default()
    }

    fn enabled_config() -> PythonScriptConfig {
        PythonScriptConfig {
            enabled: true,
            timeout_secs: 10,
            venv_path: String::new(),
        }
    }

    #[tokio::test]
    async fn test_python_engine_disabled() {
        let engine = PythonEngine::new(disabled_config());
        let result = engine.execute(b"print('hi')", b"").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::ScriptExecFailed);
    }

    #[tokio::test]
    async fn test_python_engine_empty_script() {
        let engine = PythonEngine::new(enabled_config());
        let result = engine.execute(b"", b"input").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::ScriptLoadFailed);
    }

    #[test]
    fn test_python_engine_name() {
        let engine = PythonEngine::new(disabled_config());
        assert_eq!(engine.engine_name(), "python");
    }

    #[test]
    fn test_resolve_command_default() {
        let engine = PythonEngine::new(enabled_config());
        let cmd = engine.resolve_command();
        #[cfg(unix)]
        assert_eq!(cmd, "python3");
        #[cfg(windows)]
        assert_eq!(cmd, "python");
    }

    #[test]
    fn test_resolve_command_venv() {
        let config = PythonScriptConfig {
            enabled: true,
            timeout_secs: 10,
            venv_path: "/opt/venvs/myapp".to_string(),
        };
        let engine = PythonEngine::new(config);
        let cmd = engine.resolve_command();
        #[cfg(unix)]
        assert_eq!(cmd, "/opt/venvs/myapp/bin/python");
        #[cfg(windows)]
        assert_eq!(cmd, "/opt/venvs/myapp\\Scripts\\python.exe");
    }

    /// Requires `python3` (or `python` on Windows) to be installed.
    #[tokio::test]
    #[ignore]
    async fn test_python_execute_print() {
        let engine = PythonEngine::new(enabled_config());
        let output = engine.execute(b"print('hello')", b"").await.unwrap();
        assert_eq!(output.exit_code, 0);
        assert_eq!(
            String::from_utf8_lossy(&output.result).trim(),
            "hello"
        );
    }

    /// Requires Python. Verifies INPUT global is available.
    #[tokio::test]
    #[ignore]
    async fn test_python_execute_with_input() {
        let engine = PythonEngine::new(enabled_config());
        let script = b"import sys; sys.stdout.buffer.write(INPUT)";
        let output = engine.execute(script, b"hello-python").await.unwrap();
        assert_eq!(output.exit_code, 0);
        assert_eq!(output.result, b"hello-python");
    }

    /// Requires Python. Verifies stderr is captured.
    #[tokio::test]
    #[ignore]
    async fn test_python_execute_with_stderr() {
        let engine = PythonEngine::new(enabled_config());
        let script = b"import sys; print('log-line', file=sys.stderr); print('result-data')";
        let output = engine.execute(script, b"").await.unwrap();
        assert_eq!(output.exit_code, 0);
        assert!(String::from_utf8_lossy(&output.result).contains("result-data"));
        assert!(output.stdout.contains("log-line"));
    }

    /// Requires Python. Verifies syntax errors are reported.
    #[tokio::test]
    #[ignore]
    async fn test_python_execute_syntax_error() {
        let engine = PythonEngine::new(enabled_config());
        let output = engine.execute(b"def broken(", b"").await.unwrap();
        assert_ne!(output.exit_code, 0);
        assert!(output.stdout.contains("SyntaxError"));
    }

    /// Requires Python. Verifies worker persistence across multiple calls.
    #[tokio::test]
    #[ignore]
    async fn test_python_worker_persistence() {
        let engine = PythonEngine::new(enabled_config());

        let out1 = engine.execute(b"print('first')", b"").await.unwrap();
        assert_eq!(out1.exit_code, 0);
        assert!(String::from_utf8_lossy(&out1.result).contains("first"));

        let out2 = engine.execute(b"print('second')", b"").await.unwrap();
        assert_eq!(out2.exit_code, 0);
        assert!(String::from_utf8_lossy(&out2.result).contains("second"));

        let out3 = engine.execute(b"print(1 + 2)", b"").await.unwrap();
        assert_eq!(out3.exit_code, 0);
        assert!(String::from_utf8_lossy(&out3.result).contains("3"));
    }

    /// Requires Python. Verifies worker recovers after script error.
    #[tokio::test]
    #[ignore]
    async fn test_python_worker_recovery_after_error() {
        let engine = PythonEngine::new(enabled_config());

        let bad = engine.execute(b"raise ValueError('boom')", b"").await.unwrap();
        assert_ne!(bad.exit_code, 0);

        let good = engine.execute(b"print('recovered')", b"").await.unwrap();
        assert_eq!(good.exit_code, 0);
        assert!(String::from_utf8_lossy(&good.result).contains("recovered"));
    }
}
