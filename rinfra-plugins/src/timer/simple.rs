use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use chrono::Utc;
use cron::Schedule;
use rinfra_core::config::SimpleTimerConfig;
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::lock::DistributedLock;
use rinfra_core::timer::{
    TimerEngine, TimerHandler, TimerSchedule, TimerScope, TimerTask, TimerTaskInfo,
    TimerTaskStatus,
};
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinHandle;

/// In-process timer engine backed by `tokio::time` + `cron` crate.
///
/// Each scheduled task spawns a lightweight tokio task that sleeps until the
/// next fire time, invokes the handler, and repeats (for recurring schedules).
///
/// When a [`DistributedLock`] is provided, each execution acquires a
/// cluster-wide lock keyed by `timer:{task_id}` before running the handler.
/// This prevents duplicate execution across nodes in a cluster.
pub struct SimpleTimerEngine {
    tasks: Mutex<HashMap<String, TaskEntry>>,
    concurrency: Arc<Semaphore>,
    lock: Option<Arc<dyn DistributedLock>>,
}

struct TaskEntry {
    task: TimerTask,
    state: Arc<Mutex<TaskState>>,
    handle: JoinHandle<()>,
}

struct TaskState {
    status: TimerTaskStatus,
    last_run: Option<u64>,
    next_run: Option<u64>,
    run_count: u64,
}

impl SimpleTimerEngine {
    pub fn new(config: &SimpleTimerConfig) -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
            concurrency: Arc::new(Semaphore::new(config.max_concurrent)),
            lock: None,
        }
    }

    pub fn with_lock(mut self, lock: Arc<dyn DistributedLock>) -> Self {
        self.lock = Some(lock);
        self
    }
}

#[async_trait]
impl TimerEngine for SimpleTimerEngine {
    fn engine_name(&self) -> &str {
        "simple"
    }

    async fn schedule(
        &self,
        task: TimerTask,
        handler: Arc<dyn TimerHandler>,
    ) -> Result<(), AppError> {
        let task_id = task.id.clone();
        let task_name = task.name.clone();

        let initial_next = compute_next_fire(&task.schedule)?;
        let state = Arc::new(Mutex::new(TaskState {
            status: TimerTaskStatus::Scheduled,
            last_run: None,
            next_run: initial_next.map(|d| now_ms() + d.as_millis() as u64),
            run_count: 0,
        }));

        let handle = spawn_timer_loop(
            task.clone(),
            handler,
            state.clone(),
            self.concurrency.clone(),
            self.lock.clone(),
        );

        tracing::info!(
            task_id = %task_id,
            task_name = %task_name,
            schedule = ?task.schedule,
            "timer task scheduled"
        );

        let mut tasks = self.tasks.lock().await;
        tasks.insert(
            task_id,
            TaskEntry {
                task,
                state,
                handle,
            },
        );
        Ok(())
    }

    async fn cancel(&self, task_id: &str) -> Result<bool, AppError> {
        let mut tasks = self.tasks.lock().await;
        if let Some(entry) = tasks.remove(task_id) {
            entry.handle.abort();
            let mut state = entry.state.lock().await;
            state.status = TimerTaskStatus::Cancelled;
            tracing::info!(task_id = %task_id, "timer task cancelled");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn list_tasks(&self) -> Vec<TimerTaskInfo> {
        let tasks = self.tasks.lock().await;
        let mut result = Vec::with_capacity(tasks.len());
        for entry in tasks.values() {
            let state = entry.state.lock().await;
            result.push(TimerTaskInfo {
                id: entry.task.id.clone(),
                name: entry.task.name.clone(),
                schedule: entry.task.schedule.clone(),
                scope: entry.task.scope,
                status: state.status,
                last_run: state.last_run,
                next_run: state.next_run,
                run_count: state.run_count,
            });
        }
        result
    }

    async fn shutdown(&self) -> Result<(), AppError> {
        let mut tasks = self.tasks.lock().await;
        for (id, entry) in tasks.drain() {
            entry.handle.abort();
            tracing::debug!(task_id = %id, "timer task stopped on shutdown");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Internal scheduling logic
// ---------------------------------------------------------------------------

fn spawn_timer_loop(
    task: TimerTask,
    handler: Arc<dyn TimerHandler>,
    state: Arc<Mutex<TaskState>>,
    semaphore: Arc<Semaphore>,
    lock: Option<Arc<dyn DistributedLock>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let recurring = matches!(
            task.schedule,
            TimerSchedule::Cron { .. } | TimerSchedule::Interval { .. }
        );

        loop {
            let delay = match compute_next_fire(&task.schedule) {
                Ok(Some(d)) => d,
                Ok(None) => break,
                Err(e) => {
                    tracing::error!(task_id = %task.id, error = %e, "failed to compute next fire time");
                    let mut s = state.lock().await;
                    s.status = TimerTaskStatus::Failed;
                    break;
                }
            };

            {
                let mut s = state.lock().await;
                s.next_run = Some(now_ms() + delay.as_millis() as u64);
                s.status = TimerTaskStatus::Scheduled;
            }

            tokio::time::sleep(delay).await;

            let _permit = match semaphore.acquire().await {
                Ok(p) => p,
                Err(_) => break,
            };

            let need_lock = task.scope == TimerScope::Cluster && lock.is_some();
            let lock_handle = if need_lock {
                let dlock = lock.as_ref().unwrap();
                let lock_key = format!("timer:{}", task.id);
                let ttl = task.effective_lock_ttl();
                match dlock.try_acquire(&lock_key, ttl).await {
                    Ok(Some(h)) => Some(h),
                    Ok(None) => {
                        tracing::debug!(
                            task_id = %task.id,
                            "skipping cluster timer — another node holds the lock"
                        );
                        if !recurring {
                            break;
                        }
                        continue;
                    }
                    Err(e) => {
                        tracing::warn!(
                            task_id = %task.id,
                            error = %e,
                            "failed to acquire cluster timer lock, running locally"
                        );
                        None
                    }
                }
            } else {
                None
            };

            {
                let mut s = state.lock().await;
                s.status = TimerTaskStatus::Running;
            }

            let timer_start = std::time::Instant::now();
            let result = handler.handle(&task).await;
            let elapsed = timer_start.elapsed().as_secs_f64();

            let labels = [
                ("task_id", task.id.clone()),
                ("task_name", task.name.clone()),
            ];
            metrics::histogram!("timer_execution_duration_seconds", &labels).record(elapsed);

            if let (Some(dlock), Some(handle)) = (&lock, &lock_handle) {
                if let Err(e) = dlock.release(handle).await {
                    tracing::warn!(task_id = %task.id, error = %e, "failed to release cluster timer lock");
                }
            }

            {
                let mut s = state.lock().await;
                s.run_count += 1;
                s.last_run = Some(now_ms());

                match result {
                    Ok(()) => {
                        metrics::counter!("timer_executions_total", &labels).increment(1);
                        s.status = if recurring {
                            TimerTaskStatus::Scheduled
                        } else {
                            TimerTaskStatus::Completed
                        };
                    }
                    Err(e) => {
                        metrics::counter!("timer_execution_errors_total", &labels).increment(1);
                        tracing::warn!(
                            task_id = %task.id,
                            task_name = %task.name,
                            error = %e,
                            "timer handler returned error"
                        );
                        s.status = if recurring {
                            TimerTaskStatus::Scheduled
                        } else {
                            TimerTaskStatus::Failed
                        };
                    }
                }
            }

            if !recurring {
                break;
            }
        }
    })
}

fn compute_next_fire(schedule: &TimerSchedule) -> Result<Option<Duration>, AppError> {
    match schedule {
        TimerSchedule::Cron { expr } => {
            let sched: Schedule = expr.parse().map_err(|e| {
                AppError::new(
                    ErrorCode::TimerInvalidSchedule,
                    format!("invalid cron expression '{expr}': {e}"),
                )
            })?;
            let next = sched.upcoming(Utc).next().ok_or_else(|| {
                AppError::new(
                    ErrorCode::TimerInvalidSchedule,
                    format!("cron expression '{expr}' has no upcoming fire time"),
                )
            })?;
            let now = Utc::now();
            let duration = (next - now)
                .to_std()
                .unwrap_or(Duration::from_millis(100));
            Ok(Some(duration))
        }
        TimerSchedule::Interval { secs } => Ok(Some(Duration::from_secs(*secs))),
        TimerSchedule::Delay { secs } => Ok(Some(Duration::from_secs(*secs))),
        TimerSchedule::Once { at_unix_ms } => {
            let now = now_ms();
            if *at_unix_ms <= now {
                Ok(Some(Duration::ZERO))
            } else {
                Ok(Some(Duration::from_millis(at_unix_ms - now)))
            }
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn default_config() -> SimpleTimerConfig {
        SimpleTimerConfig { max_concurrent: 4 }
    }

    struct CountHandler {
        count: Arc<AtomicU32>,
    }

    #[async_trait]
    impl TimerHandler for CountHandler {
        async fn handle(&self, _task: &TimerTask) -> Result<(), AppError> {
            self.count.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_schedule_delay_fires_once() {
        let engine = SimpleTimerEngine::new(&default_config());
        let counter = Arc::new(AtomicU32::new(0));
        let handler = Arc::new(CountHandler {
            count: counter.clone(),
        });

        let task = TimerTask::new("test-delay", TimerSchedule::Delay { secs: 0 }, vec![]);
        engine.schedule(task, handler).await.unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_schedule_interval_fires_multiple() {
        let engine = SimpleTimerEngine::new(&default_config());
        let counter = Arc::new(AtomicU32::new(0));
        let handler = Arc::new(CountHandler {
            count: counter.clone(),
        });

        let task = TimerTask::new(
            "test-interval",
            TimerSchedule::Interval { secs: 0 },
            vec![],
        );
        let task_id = task.id.clone();
        engine.schedule(task, handler).await.unwrap();

        tokio::time::sleep(Duration::from_millis(300)).await;
        let count = counter.load(Ordering::Relaxed);
        assert!(count >= 2, "expected at least 2 fires, got {count}");

        engine.cancel(&task_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_cancel_stops_task() {
        let engine = SimpleTimerEngine::new(&default_config());
        let counter = Arc::new(AtomicU32::new(0));
        let handler = Arc::new(CountHandler {
            count: counter.clone(),
        });

        let task = TimerTask::new(
            "test-cancel",
            TimerSchedule::Delay { secs: 100 },
            vec![],
        );
        let task_id = task.id.clone();
        engine.schedule(task, handler).await.unwrap();

        let cancelled = engine.cancel(&task_id).await.unwrap();
        assert!(cancelled);
        assert_eq!(counter.load(Ordering::Relaxed), 0);

        let cancelled_again = engine.cancel(&task_id).await.unwrap();
        assert!(!cancelled_again);
    }

    #[tokio::test]
    async fn test_list_tasks() {
        let engine = SimpleTimerEngine::new(&default_config());
        let handler = Arc::new(CountHandler {
            count: Arc::new(AtomicU32::new(0)),
        });

        let task = TimerTask::new(
            "list-me",
            TimerSchedule::Delay { secs: 100 },
            vec![],
        );
        let task_id = task.id.clone();
        engine.schedule(task, handler).await.unwrap();

        let infos = engine.list_tasks().await;
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].id, task_id);
        assert_eq!(infos[0].name, "list-me");

        engine.shutdown().await.unwrap();
        let infos = engine.list_tasks().await;
        assert!(infos.is_empty());
    }

    #[tokio::test]
    async fn test_shutdown_clears_all() {
        let engine = SimpleTimerEngine::new(&default_config());
        let handler: Arc<dyn TimerHandler> = Arc::new(CountHandler {
            count: Arc::new(AtomicU32::new(0)),
        });

        for i in 0..5 {
            let task = TimerTask::new(
                format!("task-{i}"),
                TimerSchedule::Delay { secs: 100 },
                vec![],
            );
            engine.schedule(task, handler.clone()).await.unwrap();
        }

        assert_eq!(engine.list_tasks().await.len(), 5);
        engine.shutdown().await.unwrap();
        assert!(engine.list_tasks().await.is_empty());
    }

    #[tokio::test]
    async fn test_handler_error_does_not_crash() {
        let engine = SimpleTimerEngine::new(&default_config());

        struct FailHandler;
        #[async_trait]
        impl TimerHandler for FailHandler {
            async fn handle(&self, _task: &TimerTask) -> Result<(), AppError> {
                Err(AppError::new(ErrorCode::Internal, "boom"))
            }
        }

        let task = TimerTask::new("fail-task", TimerSchedule::Delay { secs: 0 }, vec![]);
        let task_id = task.id.clone();
        engine.schedule(task, Arc::new(FailHandler)).await.unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;

        let infos = engine.list_tasks().await;
        let info = infos.iter().find(|t| t.id == task_id).unwrap();
        assert_eq!(info.status, TimerTaskStatus::Failed);
        assert_eq!(info.run_count, 1);
    }

    #[test]
    fn test_compute_next_fire_invalid_cron() {
        let result = compute_next_fire(&TimerSchedule::Cron {
            expr: "bad expr".into(),
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_compute_next_fire_interval() {
        let result = compute_next_fire(&TimerSchedule::Interval { secs: 60 }).unwrap();
        assert_eq!(result, Some(Duration::from_secs(60)));
    }

    #[test]
    fn test_compute_next_fire_delay() {
        let result = compute_next_fire(&TimerSchedule::Delay { secs: 5 }).unwrap();
        assert_eq!(result, Some(Duration::from_secs(5)));
    }

    #[test]
    fn test_compute_next_fire_once_past() {
        let result = compute_next_fire(&TimerSchedule::Once { at_unix_ms: 0 }).unwrap();
        assert_eq!(result, Some(Duration::ZERO));
    }

    #[test]
    fn test_compute_next_fire_once_future() {
        let future_ms = now_ms() + 10_000;
        let result = compute_next_fire(&TimerSchedule::Once {
            at_unix_ms: future_ms,
        })
        .unwrap();
        let dur = result.unwrap();
        assert!(dur.as_millis() <= 10_000);
        assert!(dur.as_millis() >= 9_000);
    }

    #[test]
    fn test_engine_name() {
        let engine = SimpleTimerEngine::new(&default_config());
        assert_eq!(engine.engine_name(), "simple");
    }
}
