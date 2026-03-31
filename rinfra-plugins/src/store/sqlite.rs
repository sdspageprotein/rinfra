#![cfg(feature = "sqlite")]

use std::time::Instant;

use async_trait::async_trait;
use rinfra_core::config::SqliteConfig;
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::store::{DbConnection, DbExecutor, DbRow, DbValue, Store, Transaction};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Column, Row, SqlitePool, TypeInfo};
use tokio::sync::{Mutex, RwLock};
use tracing::{info, warn};

pub struct SqliteStore {
    config: SqliteConfig,
    pool: RwLock<Option<SqlitePool>>,
}

impl SqliteStore {
    pub fn new(config: SqliteConfig) -> Self {
        Self {
            config,
            pool: RwLock::new(None),
        }
    }

    pub async fn pool(&self) -> Result<SqlitePool, AppError> {
        let guard = self.pool.read().await;
        guard.clone().ok_or_else(|| {
            AppError::new(ErrorCode::StoreConnectionFailed, "sqlite store not connected")
        })
    }
}

#[async_trait]
impl Store for SqliteStore {
    async fn connect(&self) -> Result<(), AppError> {
        if !self.config.enabled {
            info!("sqlite store disabled, skipping connect");
            return Ok(());
        }
        let url = format!("sqlite:{}", self.config.path);
        info!(path = %self.config.path, "sqlite store connecting");
        let pool = SqlitePoolOptions::new()
            .max_connections(self.config.max_connections)
            .connect(&url)
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::StoreConnectionFailed,
                    format!("sqlite connect failed: {e}"),
                )
            })?;
        *self.pool.write().await = Some(pool);
        info!("sqlite store connected");
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), AppError> {
        if let Some(pool) = self.pool.write().await.take() {
            pool.close().await;
            info!("sqlite store disconnected");
        }
        Ok(())
    }

    async fn health_check(&self) -> Result<bool, AppError> {
        let guard = self.pool.read().await;
        let Some(pool) = guard.as_ref() else {
            if !self.config.enabled {
                return Ok(true);
            }
            return Ok(false);
        };
        let labels = [("backend", "sqlite")];
        metrics::gauge!("db_pool_size", &labels).set(pool.size() as f64);
        metrics::gauge!("db_pool_idle", &labels).set(pool.num_idle() as f64);
        match sqlx::query("SELECT 1").execute(pool).await {
            Ok(_) => Ok(true),
            Err(e) => {
                warn!(error = %e, "sqlite health check failed");
                Ok(false)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// SQLite Executor
// ---------------------------------------------------------------------------

struct SqliteExecutor {
    pool: SqlitePool,
    slow_threshold_ms: u64,
}

impl SqliteExecutor {
    fn check_slow(&self, sql: &str, elapsed: std::time::Duration) {
        let ms = elapsed.as_millis() as u64;
        if self.slow_threshold_ms > 0 && ms >= self.slow_threshold_ms {
            tracing::warn!(
                elapsed_ms = ms,
                threshold_ms = self.slow_threshold_ms,
                sql = sql,
                "slow query detected (sqlite)"
            );
        }
    }
}

#[async_trait]
impl DbExecutor for SqliteExecutor {
    async fn execute(&self, sql: &str, params: &[DbValue]) -> Result<u64, AppError> {
        let mut q = sqlx::query(sql);
        for p in params {
            q = sqlite_bind(q, p);
        }
        let start = Instant::now();
        let result = q.execute(&self.pool).await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("sqlite execute failed: {e}"))
        })?;
        self.check_slow(sql, start.elapsed());
        Ok(result.rows_affected())
    }

    async fn query(&self, sql: &str, params: &[DbValue]) -> Result<Vec<DbRow>, AppError> {
        let mut q = sqlx::query(sql);
        for p in params {
            q = sqlite_bind(q, p);
        }
        let start = Instant::now();
        let rows = q.fetch_all(&self.pool).await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("sqlite query failed: {e}"))
        })?;
        self.check_slow(sql, start.elapsed());
        rows.iter().map(sqlite_row_to_db_row).collect()
    }

    async fn query_one(&self, sql: &str, params: &[DbValue]) -> Result<Option<DbRow>, AppError> {
        let mut q = sqlx::query(sql);
        for p in params {
            q = sqlite_bind(q, p);
        }
        let start = Instant::now();
        let row = q.fetch_optional(&self.pool).await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("sqlite query_one failed: {e}"))
        })?;
        self.check_slow(sql, start.elapsed());
        match row {
            Some(r) => Ok(Some(sqlite_row_to_db_row(&r)?)),
            None => Ok(None),
        }
    }
}

// ---------------------------------------------------------------------------
// SQLite Transaction
// ---------------------------------------------------------------------------

struct SqliteTransaction {
    tx: Mutex<Option<sqlx::Transaction<'static, sqlx::Sqlite>>>,
}

#[async_trait]
impl DbExecutor for SqliteTransaction {
    async fn execute(&self, sql: &str, params: &[DbValue]) -> Result<u64, AppError> {
        let mut guard = self.tx.lock().await;
        let tx = guard.as_mut().ok_or_else(|| {
            AppError::new(ErrorCode::StoreQueryFailed, "sqlite transaction already consumed")
        })?;
        let mut q = sqlx::query(sql);
        for p in params {
            q = sqlite_bind(q, p);
        }
        let result = q.execute(&mut **tx).await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("sqlite tx execute failed: {e}"))
        })?;
        Ok(result.rows_affected())
    }

    async fn query(&self, sql: &str, params: &[DbValue]) -> Result<Vec<DbRow>, AppError> {
        let mut guard = self.tx.lock().await;
        let tx = guard.as_mut().ok_or_else(|| {
            AppError::new(ErrorCode::StoreQueryFailed, "sqlite transaction already consumed")
        })?;
        let mut q = sqlx::query(sql);
        for p in params {
            q = sqlite_bind(q, p);
        }
        let rows = q.fetch_all(&mut **tx).await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("sqlite tx query failed: {e}"))
        })?;
        rows.iter().map(sqlite_row_to_db_row).collect()
    }

    async fn query_one(&self, sql: &str, params: &[DbValue]) -> Result<Option<DbRow>, AppError> {
        let mut guard = self.tx.lock().await;
        let tx = guard.as_mut().ok_or_else(|| {
            AppError::new(ErrorCode::StoreQueryFailed, "sqlite transaction already consumed")
        })?;
        let mut q = sqlx::query(sql);
        for p in params {
            q = sqlite_bind(q, p);
        }
        let row = q.fetch_optional(&mut **tx).await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("sqlite tx query_one failed: {e}"))
        })?;
        match row {
            Some(r) => Ok(Some(sqlite_row_to_db_row(&r)?)),
            None => Ok(None),
        }
    }
}

#[async_trait]
impl Transaction for SqliteTransaction {
    async fn commit(self: Box<Self>) -> Result<(), AppError> {
        let mut guard = self.tx.lock().await;
        let tx = guard.take().ok_or_else(|| {
            AppError::new(ErrorCode::StoreQueryFailed, "sqlite transaction already consumed")
        })?;
        tx.commit().await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("sqlite commit failed: {e}"))
        })
    }

    async fn rollback(self: Box<Self>) -> Result<(), AppError> {
        let mut guard = self.tx.lock().await;
        let tx = guard.take().ok_or_else(|| {
            AppError::new(ErrorCode::StoreQueryFailed, "sqlite transaction already consumed")
        })?;
        tx.rollback().await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("sqlite rollback failed: {e}"))
        })
    }
}

// ---------------------------------------------------------------------------
// DbConnection impl
// ---------------------------------------------------------------------------

#[async_trait]
impl DbConnection for SqliteStore {
    async fn executor(&self) -> Result<Box<dyn DbExecutor>, AppError> {
        let pool = self.pool().await?;
        Ok(Box::new(SqliteExecutor {
            pool,
            slow_threshold_ms: self.config.slow_query_threshold_ms,
        }))
    }

    async fn begin(&self) -> Result<Box<dyn Transaction>, AppError> {
        let pool = self.pool().await?;
        let tx = pool.begin().await.map_err(|e| {
            AppError::new(
                ErrorCode::StoreQueryFailed,
                format!("sqlite begin failed: {e}"),
            )
        })?;
        Ok(Box::new(SqliteTransaction {
            tx: Mutex::new(Some(tx)),
        }))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sqlite_bind<'q>(
    q: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    val: &'q DbValue,
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
    match val {
        DbValue::Null => q.bind(None::<String>),
        DbValue::Bool(v) => q.bind(v),
        DbValue::Int(v) => q.bind(v),
        DbValue::Float(v) => q.bind(v),
        DbValue::Text(v) => q.bind(v.as_str()),
        DbValue::Bytes(v) => q.bind(v.as_slice()),
    }
}

fn sqlite_row_to_db_row(row: &sqlx::sqlite::SqliteRow) -> Result<DbRow, AppError> {
    let columns: Vec<String> = row.columns().iter().map(|c| c.name().to_string()).collect();
    let mut values = Vec::with_capacity(columns.len());
    for col in row.columns() {
        let type_name = col.type_info().name();
        let val = match type_name {
            "BOOLEAN" => {
                let v: Option<bool> = row.try_get(col.ordinal()).ok();
                v.map(DbValue::Bool).unwrap_or(DbValue::Null)
            }
            "INTEGER" => {
                let v: Option<i64> = row.try_get(col.ordinal()).ok();
                v.map(DbValue::Int).unwrap_or(DbValue::Null)
            }
            "REAL" => {
                let v: Option<f64> = row.try_get(col.ordinal()).ok();
                v.map(DbValue::Float).unwrap_or(DbValue::Null)
            }
            "BLOB" => {
                let v: Option<Vec<u8>> = row.try_get(col.ordinal()).ok();
                v.map(DbValue::Bytes).unwrap_or(DbValue::Null)
            }
            _ => {
                let v: Option<String> = row.try_get(col.ordinal()).ok();
                v.map(DbValue::Text).unwrap_or(DbValue::Null)
            }
        };
        values.push(val);
    }
    Ok(DbRow::new(columns, values))
}
