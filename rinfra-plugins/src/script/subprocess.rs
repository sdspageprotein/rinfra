use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::script::ScriptOutput;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

// ---------------------------------------------------------------------------
// Persistent worker process
// ---------------------------------------------------------------------------

const PYTHON_HARNESS: &str = include_str!("workers/python_worker.py");
const NODE_HARNESS: &str = include_str!("workers/node_worker.js");

pub(crate) fn python_harness() -> &'static str {
    PYTHON_HARNESS
}
pub(crate) fn node_harness() -> &'static str {
    NODE_HARNESS
}

/// A long-running interpreter process that accepts script-execution requests
/// via a line-delimited JSON protocol on stdin/stdout.
pub(crate) struct ScriptWorker {
    child: Child,
    writer: BufWriter<ChildStdin>,
    reader: BufReader<ChildStdout>,
    harness_path: PathBuf,
}

impl ScriptWorker {
    /// Spawn a new persistent worker process.
    ///
    /// `harness` is the embedded worker script (Python or Node) that will be
    /// written to a temp file and passed as the first positional argument.
    pub async fn spawn(
        command: &str,
        extra_args: &[&str],
        harness: &str,
        harness_ext: &str,
    ) -> Result<Self, AppError> {
        let harness_path = write_temp_file(harness.as_bytes(), harness_ext).await?;

        let mut cmd = Command::new(command);
        for arg in extra_args {
            cmd.arg(arg);
        }
        cmd.arg(&harness_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|e| {
            AppError::new(
                ErrorCode::ScriptExecFailed,
                format!("failed to start '{command}': {e} — is it installed and in PATH?"),
            )
        })?;

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::debug!(target: "rinfra::script_worker", "{}", line);
            }
        });

        Ok(Self {
            child,
            writer: BufWriter::new(stdin),
            reader: BufReader::new(stdout),
            harness_path,
        })
    }

    /// Send a script to the worker and wait for the result.
    pub async fn execute(
        &mut self,
        script: &[u8],
        input: &[u8],
        timeout_secs: u64,
    ) -> Result<ScriptOutput, AppError> {
        let request = serde_json::json!({
            "s": BASE64.encode(script),
            "i": BASE64.encode(input),
        });

        let mut req_line = serde_json::to_string(&request).map_err(|e| {
            AppError::new(
                ErrorCode::ScriptExecFailed,
                format!("serialize request: {e}"),
            )
        })?;
        req_line.push('\n');

        self.writer
            .write_all(req_line.as_bytes())
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::ScriptExecFailed,
                    format!("write to worker stdin: {e}"),
                )
            })?;
        self.writer.flush().await.map_err(|e| {
            AppError::new(
                ErrorCode::ScriptExecFailed,
                format!("flush worker stdin: {e}"),
            )
        })?;

        let mut resp_line = String::new();
        let timeout = Duration::from_secs(timeout_secs);

        match tokio::time::timeout(timeout, self.reader.read_line(&mut resp_line)).await {
            Ok(Ok(0)) => Err(AppError::new(
                ErrorCode::ScriptExecFailed,
                "worker process terminated unexpectedly",
            )),
            Ok(Ok(_)) => parse_worker_response(&resp_line),
            Ok(Err(e)) => Err(AppError::new(
                ErrorCode::ScriptExecFailed,
                format!("read from worker stdout: {e}"),
            )),
            Err(_) => {
                let _ = self.child.kill().await;
                Err(AppError::new(
                    ErrorCode::ScriptExecFailed,
                    format!("script timed out after {timeout_secs}s"),
                ))
            }
        }
    }

    /// Check whether the underlying process is still running.
    pub fn is_alive(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_none()
    }
}

impl Drop for ScriptWorker {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.harness_path);
    }
}

fn parse_worker_response(line: &str) -> Result<ScriptOutput, AppError> {
    let resp: serde_json::Value = serde_json::from_str(line).map_err(|e| {
        AppError::new(
            ErrorCode::ScriptExecFailed,
            format!("parse worker response: {e}"),
        )
    })?;

    let result_b64 = resp["r"].as_str().unwrap_or("");
    let stderr_text = resp["o"].as_str().unwrap_or("").to_string();
    let exit_code = resp["c"].as_i64().unwrap_or(-1) as i32;

    let result = BASE64.decode(result_b64).map_err(|e| {
        AppError::new(
            ErrorCode::ScriptExecFailed,
            format!("decode result payload: {e}"),
        )
    })?;

    Ok(ScriptOutput {
        stdout: stderr_text,
        result,
        exit_code,
    })
}

// ---------------------------------------------------------------------------
// One-shot subprocess (kept as fallback / used by WASM stub tests)
// ---------------------------------------------------------------------------

/// Run a script as a one-shot subprocess (fallback for non-persistent scenarios).
#[allow(dead_code)]
pub(crate) async fn run_script(
    command: &str,
    extra_args: &[&str],
    script: &[u8],
    input: &[u8],
    file_ext: &str,
    timeout_secs: u64,
) -> Result<ScriptOutput, AppError> {
    let script_path = write_temp_file(script, file_ext).await?;
    let result = spawn_oneshot(command, extra_args, &script_path, input, timeout_secs).await;
    let _ = tokio::fs::remove_file(&script_path).await;
    result
}

#[allow(dead_code)]
async fn spawn_oneshot(
    command: &str,
    extra_args: &[&str],
    script_path: &PathBuf,
    input: &[u8],
    timeout_secs: u64,
) -> Result<ScriptOutput, AppError> {
    let mut cmd = Command::new(command);
    for arg in extra_args {
        cmd.arg(arg);
    }
    cmd.arg(script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd.spawn().map_err(|e| {
        AppError::new(
            ErrorCode::ScriptExecFailed,
            format!("failed to start '{command}': {e} — is it installed and in PATH?"),
        )
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        if !input.is_empty() {
            let _ = stdin.write_all(input).await;
        }
        drop(stdin);
    }

    let timeout = Duration::from_secs(timeout_secs);
    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(output)) => {
            let exit_code = output.status.code().unwrap_or(-1);
            let stderr_text = String::from_utf8_lossy(&output.stderr).to_string();

            if exit_code != 0 {
                tracing::warn!(
                    command,
                    exit_code,
                    stderr = %stderr_text,
                    "script exited with non-zero code"
                );
            }

            Ok(ScriptOutput {
                stdout: stderr_text,
                result: output.stdout,
                exit_code,
            })
        }
        Ok(Err(e)) => Err(AppError::new(
            ErrorCode::ScriptExecFailed,
            format!("process I/O error: {e}"),
        )),
        Err(_) => Err(AppError::new(
            ErrorCode::ScriptExecFailed,
            format!("script timed out after {timeout_secs}s"),
        )),
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

async fn write_temp_file(content: &[u8], ext: &str) -> Result<PathBuf, AppError> {
    let id = uuid::Uuid::new_v4();
    let path = std::env::temp_dir().join(format!("rinfra_{id}{ext}"));
    tokio::fs::write(&path, content).await.map_err(|e| {
        AppError::new(
            ErrorCode::ScriptLoadFailed,
            format!("failed to write temp file: {e}"),
        )
    })?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_write_and_cleanup_temp_file() {
        let path = write_temp_file(b"hello", ".txt").await.unwrap();
        assert!(path.exists());
        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "hello");
        tokio::fs::remove_file(&path).await.unwrap();
    }

    #[tokio::test]
    async fn test_nonexistent_command_returns_error() {
        let result = run_script(
            "rinfra_nonexistent_interpreter_xyz",
            &[],
            b"code",
            b"",
            ".txt",
            5,
        )
        .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, ErrorCode::ScriptExecFailed);
        assert!(err.message.contains("rinfra_nonexistent_interpreter_xyz"));
    }

    #[tokio::test]
    async fn test_worker_spawn_bad_command() {
        let result =
            ScriptWorker::spawn("rinfra_nonexistent_xyz", &[], "# no-op", ".py").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_parse_worker_response_ok() {
        let line = r#"{"r":"aGVsbG8=","o":"some log","c":0}"#;
        let output = parse_worker_response(line).unwrap();
        assert_eq!(output.result, b"hello");
        assert_eq!(output.stdout, "some log");
        assert_eq!(output.exit_code, 0);
    }

    #[tokio::test]
    async fn test_parse_worker_response_error() {
        let line = r#"{"r":"","o":"traceback...","c":1}"#;
        let output = parse_worker_response(line).unwrap();
        assert_eq!(output.exit_code, 1);
        assert!(output.stdout.contains("traceback"));
    }
}
