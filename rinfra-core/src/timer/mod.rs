mod registry;

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

pub use registry::TimerEngineRegistry;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// How a timer task is scheduled.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TimerSchedule {
    /// Standard cron expression (6-field with seconds, e.g. `"0 */5 * * * *"`).
    Cron { expr: String },
    /// Fixed interval in seconds (repeating).
    Interval { secs: u64 },
    /// One-shot execution after a delay in seconds.
    Delay { secs: u64 },
    /// One-shot execution at a specific UNIX timestamp (milliseconds).
    Once { at_unix_ms: u64 },
}

/// Execution scope of a timer task.
///
/// Controls whether the task runs on every node independently or is
/// coordinated across the cluster so that only one node executes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimerScope {
    /// Every node runs this task independently (heartbeat, local cache refresh, …).
    Node,
    /// Only one node in the cluster executes this task per trigger
    /// (daily report, data cleanup, …). Requires a [`DistributedLock`] to
    /// be available; falls back to `Node` semantics otherwise.
    Cluster,
}

impl Default for TimerScope {
    fn default() -> Self {
        Self::Node
    }
}

/// A timer task definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerTask {
    pub id: String,
    pub name: String,
    pub schedule: TimerSchedule,
    /// Execution scope — `Node` (default) or `Cluster`.
    #[serde(default)]
    pub scope: TimerScope,
    /// Lock TTL in seconds for `Cluster` scope tasks.
    /// Defaults to 300 s if not set. Should be longer than the expected
    /// execution time to prevent another node from re-acquiring prematurely.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_ttl_secs: Option<u64>,
    /// Opaque payload passed to the handler (e.g. script content, JSON config).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub payload: Vec<u8>,
}

const DEFAULT_LOCK_TTL_SECS: u64 = 300;

impl TimerTask {
    pub fn new(
        name: impl Into<String>,
        schedule: TimerSchedule,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            schedule,
            scope: TimerScope::default(),
            lock_ttl_secs: None,
            payload,
        }
    }

    /// Set the execution scope.
    pub fn scope(mut self, scope: TimerScope) -> Self {
        self.scope = scope;
        self
    }

    /// Set the lock TTL for `Cluster` scope tasks.
    pub fn lock_ttl(mut self, secs: u64) -> Self {
        self.lock_ttl_secs = Some(secs);
        self
    }

    /// Effective lock TTL — custom value or default.
    pub fn effective_lock_ttl(&self) -> u64 {
        self.lock_ttl_secs.unwrap_or(DEFAULT_LOCK_TTL_SECS)
    }
}

/// Runtime status of a scheduled task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimerTaskStatus {
    Scheduled,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Read-only snapshot of a live timer task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerTaskInfo {
    pub id: String,
    pub name: String,
    pub schedule: TimerSchedule,
    pub scope: TimerScope,
    pub status: TimerTaskStatus,
    pub last_run: Option<u64>,
    pub next_run: Option<u64>,
    pub run_count: u64,
}

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

/// Callback invoked when a timer fires.
#[async_trait]
pub trait TimerHandler: Send + Sync + 'static {
    async fn handle(&self, task: &TimerTask) -> Result<(), AppError>;
}

/// Pluggable timer / scheduler engine.
///
/// Manages the lifecycle of scheduled tasks. The engine is responsible for
/// tracking fire times and invoking the associated [`TimerHandler`] when due.
#[async_trait]
pub trait TimerEngine: Send + Sync + 'static {
    /// Human-readable engine name (e.g. `"simple"`, `"distributed"`).
    fn engine_name(&self) -> &str;

    /// Register a task with its handler. Returns immediately; the engine
    /// will fire the handler according to the task's schedule.
    async fn schedule(
        &self,
        task: TimerTask,
        handler: Arc<dyn TimerHandler>,
    ) -> Result<(), AppError>;

    /// Cancel a previously scheduled task. Returns `true` if the task
    /// existed and was cancelled.
    async fn cancel(&self, task_id: &str) -> Result<bool, AppError>;

    /// Return a snapshot of all currently registered tasks.
    async fn list_tasks(&self) -> Vec<TimerTaskInfo>;

    /// Gracefully stop all running timers and release resources.
    async fn shutdown(&self) -> Result<(), AppError>;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timer_task_new() {
        let task = TimerTask::new(
            "daily-cleanup",
            TimerSchedule::Cron {
                expr: "0 0 3 * * *".into(),
            },
            b"{}".to_vec(),
        );
        assert_eq!(task.name, "daily-cleanup");
        assert!(!task.id.is_empty());
        assert_eq!(task.payload, b"{}");
        assert_eq!(task.scope, TimerScope::Node);
    }

    #[test]
    fn test_timer_task_scope_builder() {
        let task = TimerTask::new("report", TimerSchedule::Interval { secs: 3600 }, vec![])
            .scope(TimerScope::Cluster)
            .lock_ttl(600);
        assert_eq!(task.scope, TimerScope::Cluster);
        assert_eq!(task.lock_ttl_secs, Some(600));
        assert_eq!(task.effective_lock_ttl(), 600);
    }

    #[test]
    fn test_timer_task_default_lock_ttl() {
        let task = TimerTask::new("t", TimerSchedule::Delay { secs: 1 }, vec![])
            .scope(TimerScope::Cluster);
        assert_eq!(task.effective_lock_ttl(), 300);
    }

    #[test]
    fn test_timer_scope_serde() {
        let scope = TimerScope::Cluster;
        let json = serde_json::to_string(&scope).unwrap();
        assert_eq!(json, "\"cluster\"");
        let decoded: TimerScope = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, TimerScope::Cluster);

        let node: TimerScope = serde_json::from_str("\"node\"").unwrap();
        assert_eq!(node, TimerScope::Node);
    }

    #[test]
    fn test_timer_schedule_serde_cron() {
        let s = TimerSchedule::Cron {
            expr: "0 */5 * * * *".into(),
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("cron"));
        let decoded: TimerSchedule = serde_json::from_str(&json).unwrap();
        if let TimerSchedule::Cron { expr } = decoded {
            assert_eq!(expr, "0 */5 * * * *");
        } else {
            panic!("expected Cron variant");
        }
    }

    #[test]
    fn test_timer_schedule_serde_interval() {
        let s = TimerSchedule::Interval { secs: 60 };
        let json = serde_json::to_string(&s).unwrap();
        let decoded: TimerSchedule = serde_json::from_str(&json).unwrap();
        if let TimerSchedule::Interval { secs } = decoded {
            assert_eq!(secs, 60);
        } else {
            panic!("expected Interval variant");
        }
    }

    #[test]
    fn test_timer_schedule_serde_delay() {
        let s = TimerSchedule::Delay { secs: 10 };
        let json = serde_json::to_string(&s).unwrap();
        let decoded: TimerSchedule = serde_json::from_str(&json).unwrap();
        if let TimerSchedule::Delay { secs } = decoded {
            assert_eq!(secs, 10);
        } else {
            panic!("expected Delay variant");
        }
    }

    #[test]
    fn test_timer_schedule_serde_once() {
        let s = TimerSchedule::Once {
            at_unix_ms: 1700000000000,
        };
        let json = serde_json::to_string(&s).unwrap();
        let decoded: TimerSchedule = serde_json::from_str(&json).unwrap();
        if let TimerSchedule::Once { at_unix_ms } = decoded {
            assert_eq!(at_unix_ms, 1700000000000);
        } else {
            panic!("expected Once variant");
        }
    }

    #[test]
    fn test_timer_task_status() {
        let info = TimerTaskInfo {
            id: "t1".into(),
            name: "test".into(),
            schedule: TimerSchedule::Interval { secs: 30 },
            scope: TimerScope::Node,
            status: TimerTaskStatus::Scheduled,
            last_run: None,
            next_run: Some(1700000000000),
            run_count: 0,
        };
        assert_eq!(info.status, TimerTaskStatus::Scheduled);
        assert_eq!(info.run_count, 0);
    }

    #[test]
    fn test_timer_task_payload_skip_empty() {
        let task = TimerTask::new("t", TimerSchedule::Delay { secs: 1 }, vec![]);
        let json = serde_json::to_string(&task).unwrap();
        assert!(!json.contains("payload"));
    }
}
