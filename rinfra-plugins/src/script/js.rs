use async_trait::async_trait;
use rinfra_core::config::JsScriptConfig;
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::script::{ScriptEngine, ScriptOutput};
use tokio::sync::Mutex;

use super::subprocess::{self, ScriptWorker};

/// JavaScript script engine — runs scripts via a persistent `node` worker process.
///
/// The worker stays alive between calls, avoiding repeated Node.js startup.
/// Scripts receive input through the `INPUT` parameter (Buffer) and should
/// write results to `stdout` (`console.log()`). Diagnostic output goes to
/// `stderr` (`console.error()`) and is captured in `ScriptOutput::stdout`.
///
/// Top-level `await` is supported — scripts run as async function bodies.
pub struct JsEngine {
    config: JsScriptConfig,
    worker: Mutex<Option<ScriptWorker>>,
}

impl JsEngine {
    pub fn new(config: JsScriptConfig) -> Self {
        Self {
            config,
            worker: Mutex::new(None),
        }
    }
}

#[async_trait]
impl ScriptEngine for JsEngine {
    async fn execute(&self, script: &[u8], input: &[u8]) -> Result<ScriptOutput, AppError> {
        if !self.config.enabled {
            return Err(AppError::new(
                ErrorCode::ScriptExecFailed,
                "JavaScript script engine is disabled",
            ));
        }

        if script.is_empty() {
            return Err(AppError::new(
                ErrorCode::ScriptLoadFailed,
                "empty JavaScript script",
            ));
        }

        let mut guard = self.worker.lock().await;

        if guard.as_mut().map_or(true, |w| !w.is_alive()) {
            let heap_arg = format!("--max-old-space-size={}", self.config.max_heap_mb);
            tracing::info!(max_heap_mb = self.config.max_heap_mb, "spawning persistent Node.js worker");
            *guard = Some(
                ScriptWorker::spawn(
                    "node",
                    &[&heap_arg],
                    subprocess::node_harness(),
                    "_worker.js",
                )
                .await?,
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
        "js"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disabled_config() -> JsScriptConfig {
        JsScriptConfig::default()
    }

    fn enabled_config() -> JsScriptConfig {
        JsScriptConfig {
            enabled: true,
            timeout_secs: 10,
            max_heap_mb: 64,
        }
    }

    #[tokio::test]
    async fn test_js_engine_disabled() {
        let engine = JsEngine::new(disabled_config());
        let result = engine.execute(b"console.log('hi')", b"").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::ScriptExecFailed);
    }

    #[tokio::test]
    async fn test_js_engine_empty_script() {
        let engine = JsEngine::new(enabled_config());
        let result = engine.execute(b"", b"input").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::ScriptLoadFailed);
    }

    #[test]
    fn test_js_engine_name() {
        let engine = JsEngine::new(disabled_config());
        assert_eq!(engine.engine_name(), "js");
    }

    /// Requires `node` to be installed.
    #[tokio::test]
    #[ignore]
    async fn test_js_execute_console_log() {
        let engine = JsEngine::new(enabled_config());
        let output = engine.execute(b"console.log('hello')", b"").await.unwrap();
        assert_eq!(output.exit_code, 0);
        assert!(String::from_utf8_lossy(&output.result).contains("hello"));
    }

    /// Requires `node`. Verifies INPUT parameter is available.
    #[tokio::test]
    #[ignore]
    async fn test_js_execute_with_input() {
        let engine = JsEngine::new(enabled_config());
        let script = b"process.stdout.write(INPUT)";
        let output = engine.execute(script, b"hello-node").await.unwrap();
        assert_eq!(output.exit_code, 0);
        assert_eq!(output.result, b"hello-node");
    }

    /// Requires `node`. Verifies stderr capture.
    #[tokio::test]
    #[ignore]
    async fn test_js_execute_with_stderr() {
        let engine = JsEngine::new(enabled_config());
        let script = b"console.error('log-line'); console.log('result-data');";
        let output = engine.execute(script, b"").await.unwrap();
        assert_eq!(output.exit_code, 0);
        assert!(String::from_utf8_lossy(&output.result).contains("result-data"));
        assert!(output.stdout.contains("log-line"));
    }

    /// Requires `node`. Verifies error handling.
    #[tokio::test]
    #[ignore]
    async fn test_js_execute_throw_error() {
        let engine = JsEngine::new(enabled_config());
        let script = b"throw new Error('boom')";
        let output = engine.execute(script, b"").await.unwrap();
        assert_ne!(output.exit_code, 0);
        assert!(output.stdout.contains("boom"));
    }

    /// Requires `node`. Verifies worker persistence across calls.
    #[tokio::test]
    #[ignore]
    async fn test_js_worker_persistence() {
        let engine = JsEngine::new(enabled_config());

        let out1 = engine.execute(b"console.log('first')", b"").await.unwrap();
        assert_eq!(out1.exit_code, 0);
        assert!(String::from_utf8_lossy(&out1.result).contains("first"));

        let out2 = engine.execute(b"console.log('second')", b"").await.unwrap();
        assert_eq!(out2.exit_code, 0);
        assert!(String::from_utf8_lossy(&out2.result).contains("second"));

        let out3 = engine
            .execute(b"console.log(2 + 3)", b"")
            .await
            .unwrap();
        assert_eq!(out3.exit_code, 0);
        assert!(String::from_utf8_lossy(&out3.result).contains("5"));
    }

    /// Requires `node`. Verifies worker recovers after script error.
    #[tokio::test]
    #[ignore]
    async fn test_js_worker_recovery_after_error() {
        let engine = JsEngine::new(enabled_config());

        let bad = engine
            .execute(b"throw new Error('crash')", b"")
            .await
            .unwrap();
        assert_ne!(bad.exit_code, 0);

        let good = engine
            .execute(b"console.log('recovered')", b"")
            .await
            .unwrap();
        assert_eq!(good.exit_code, 0);
        assert!(String::from_utf8_lossy(&good.result).contains("recovered"));
    }

    /// Requires `node`. Verifies top-level await support.
    #[tokio::test]
    #[ignore]
    async fn test_js_execute_top_level_await() {
        let engine = JsEngine::new(enabled_config());
        let script = b"const result = await Promise.resolve(42); console.log(result);";
        let output = engine.execute(script, b"").await.unwrap();
        assert_eq!(output.exit_code, 0);
        assert!(String::from_utf8_lossy(&output.result).contains("42"));
    }
}
