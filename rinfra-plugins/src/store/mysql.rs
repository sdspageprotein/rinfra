#![cfg(feature = "mysql")]

use std::time::Instant;

use async_trait::async_trait;
use rinfra_core::config::MysqlConfig;
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::store::{DbConnection, DbExecutor, DbRow, DbValue, Store, Transaction};
use sqlx::mysql::MySqlPoolOptions;
use sqlx::{Column, MySqlPool, Row, TypeInfo};
use tokio::sync::{Mutex, RwLock};
use tracing::{info, warn};

pub struct MysqlStore {
    config: MysqlConfig,
    pool: RwLock<Option<MySqlPool>>,
}

impl MysqlStore {
    pub fn new(config: MysqlConfig) -> Self {
        Self {
            config,
            pool: RwLock::new(None),
        }
    }

    pub async fn pool(&self) -> Result<MySqlPool, AppError> {
        let guard = self.pool.read().await;
        guard.clone().ok_or_else(|| {
            AppError::new(ErrorCode::StoreConnectionFailed, "mysql store not connected")
        })
    }
}

#[async_trait]
impl Store for MysqlStore {
    async fn connect(&self) -> Result<(), AppError> {
        if !self.config.enabled {
            info!("mysql store disabled, skipping connect");
            return Ok(());
        }
        info!(url = %self.config.url, "mysql store connecting");
        let pool = MySqlPoolOptions::new()
            .max_connections(self.config.max_connections)
            .idle_timeout(std::time::Duration::from_secs(self.config.idle_timeout_secs))
            .connect(&self.config.url)
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::StoreConnectionFailed,
                    format!("mysql connect failed: {e}"),
                )
            })?;
        *self.pool.write().await = Some(pool);
        info!("mysql store connected");
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), AppError> {
        if let Some(pool) = self.pool.write().await.take() {
            pool.close().await;
            info!("mysql store disconnected");
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
        let labels = [("backend", "mysql")];
        metrics::gauge!("db_pool_size", &labels).set(pool.size() as f64);
        metrics::gauge!("db_pool_idle", &labels).set(pool.num_idle() as f64);
        match sqlx::query("SELECT 1").execute(pool).await {
            Ok(_) => Ok(true),
            Err(e) => {
                warn!(error = %e, "mysql health check failed");
                Ok(false)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// MySQL Executor
// ---------------------------------------------------------------------------

struct MysqlExecutor {
    pool: MySqlPool,
    slow_threshold_ms: u64,
}

impl MysqlExecutor {
    fn check_slow(&self, sql: &str, elapsed: std::time::Duration) {
        let ms = elapsed.as_millis() as u64;
        if self.slow_threshold_ms > 0 && ms >= self.slow_threshold_ms {
            tracing::warn!(
                elapsed_ms = ms,
                threshold_ms = self.slow_threshold_ms,
                sql = sql,
                "slow query detected (mysql)"
            );
        }
    }
}

#[async_trait]
impl DbExecutor for MysqlExecutor {
    async fn execute(&self, sql: &str, params: &[DbValue]) -> Result<u64, AppError> {
        let mut q = sqlx::query(sql);
        for p in params {
            q = mysql_bind(q, p);
        }
        let start = Instant::now();
        let result = q.execute(&self.pool).await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("mysql execute failed: {e}"))
        })?;
        self.check_slow(sql, start.elapsed());
        Ok(result.rows_affected())
    }

    async fn query(&self, sql: &str, params: &[DbValue]) -> Result<Vec<DbRow>, AppError> {
        let mut q = sqlx::query(sql);
        for p in params {
            q = mysql_bind(q, p);
        }
        let start = Instant::now();
        let rows = q.fetch_all(&self.pool).await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("mysql query failed: {e}"))
        })?;
        self.check_slow(sql, start.elapsed());
        rows.iter().map(mysql_row_to_db_row).collect()
    }

    async fn query_one(&self, sql: &str, params: &[DbValue]) -> Result<Option<DbRow>, AppError> {
        let mut q = sqlx::query(sql);
        for p in params {
            q = mysql_bind(q, p);
        }
        let start = Instant::now();
        let row = q.fetch_optional(&self.pool).await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("mysql query_one failed: {e}"))
        })?;
        self.check_slow(sql, start.elapsed());
        match row {
            Some(r) => Ok(Some(mysql_row_to_db_row(&r)?)),
            None => Ok(None),
        }
    }
}

// ---------------------------------------------------------------------------
// MySQL Transaction
// ---------------------------------------------------------------------------

struct MysqlTransaction {
    tx: Mutex<Option<sqlx::Transaction<'static, sqlx::MySql>>>,
}

#[async_trait]
impl DbExecutor for MysqlTransaction {
    async fn execute(&self, sql: &str, params: &[DbValue]) -> Result<u64, AppError> {
        let mut guard = self.tx.lock().await;
        let tx = guard.as_mut().ok_or_else(|| {
            AppError::new(ErrorCode::StoreQueryFailed, "mysql transaction already consumed")
        })?;
        let mut q = sqlx::query(sql);
        for p in params {
            q = mysql_bind(q, p);
        }
        let result = q.execute(&mut **tx).await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("mysql tx execute failed: {e}"))
        })?;
        Ok(result.rows_affected())
    }

    async fn query(&self, sql: &str, params: &[DbValue]) -> Result<Vec<DbRow>, AppError> {
        let mut guard = self.tx.lock().await;
        let tx = guard.as_mut().ok_or_else(|| {
            AppError::new(ErrorCode::StoreQueryFailed, "mysql transaction already consumed")
        })?;
        let mut q = sqlx::query(sql);
        for p in params {
            q = mysql_bind(q, p);
        }
        let rows = q.fetch_all(&mut **tx).await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("mysql tx query failed: {e}"))
        })?;
        rows.iter().map(mysql_row_to_db_row).collect()
    }

    async fn query_one(&self, sql: &str, params: &[DbValue]) -> Result<Option<DbRow>, AppError> {
        let mut guard = self.tx.lock().await;
        let tx = guard.as_mut().ok_or_else(|| {
            AppError::new(ErrorCode::StoreQueryFailed, "mysql transaction already consumed")
        })?;
        let mut q = sqlx::query(sql);
        for p in params {
            q = mysql_bind(q, p);
        }
        let row = q.fetch_optional(&mut **tx).await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("mysql tx query_one failed: {e}"))
        })?;
        match row {
            Some(r) => Ok(Some(mysql_row_to_db_row(&r)?)),
            None => Ok(None),
        }
    }
}

#[async_trait]
impl Transaction for MysqlTransaction {
    async fn commit(self: Box<Self>) -> Result<(), AppError> {
        let mut guard = self.tx.lock().await;
        let tx = guard.take().ok_or_else(|| {
            AppError::new(ErrorCode::StoreQueryFailed, "mysql transaction already consumed")
        })?;
        tx.commit().await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("mysql commit failed: {e}"))
        })
    }

    async fn rollback(self: Box<Self>) -> Result<(), AppError> {
        let mut guard = self.tx.lock().await;
        let tx = guard.take().ok_or_else(|| {
            AppError::new(ErrorCode::StoreQueryFailed, "mysql transaction already consumed")
        })?;
        tx.rollback().await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("mysql rollback failed: {e}"))
        })
    }
}

// ---------------------------------------------------------------------------
// DbConnection impl
// ---------------------------------------------------------------------------

#[async_trait]
impl DbConnection for MysqlStore {
    async fn executor(&self) -> Result<Box<dyn DbExecutor>, AppError> {
        let pool = self.pool().await?;
        Ok(Box::new(MysqlExecutor {
            pool,
            slow_threshold_ms: self.config.slow_query_threshold_ms,
        }))
    }

    async fn begin(&self) -> Result<Box<dyn Transaction>, AppError> {
        let pool = self.pool().await?;
        let tx = pool.begin().await.map_err(|e| {
            AppError::new(
                ErrorCode::StoreQueryFailed,
                format!("mysql begin failed: {e}"),
            )
        })?;
        Ok(Box::new(MysqlTransaction {
            tx: Mutex::new(Some(tx)),
        }))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn mysql_bind<'q>(
    q: sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments>,
    val: &'q DbValue,
) -> sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments> {
    match val {
        DbValue::Null => q.bind(None::<String>),
        DbValue::Bool(v) => q.bind(v),
        DbValue::Int(v) => q.bind(v),
        DbValue::Float(v) => q.bind(v),
        DbValue::Text(v) => q.bind(v.as_str()),
        DbValue::Bytes(v) => q.bind(v.as_slice()),
    }
}

fn mysql_row_to_db_row(row: &sqlx::mysql::MySqlRow) -> Result<DbRow, AppError> {
    let columns: Vec<String> = row.columns().iter().map(|c| c.name().to_string()).collect();
    let mut values = Vec::with_capacity(columns.len());
    for col in row.columns() {
        let type_name = col.type_info().name();
        let val = match type_name {
            "BOOLEAN" | "TINYINT(1)" => {
                let v: Option<bool> = row.try_get(col.ordinal()).ok();
                v.map(DbValue::Bool).unwrap_or(DbValue::Null)
            }
            "TINYINT" | "SMALLINT" | "INT" | "BIGINT" => {
                let v: Option<i64> = row.try_get(col.ordinal()).ok();
                v.map(DbValue::Int).unwrap_or(DbValue::Null)
            }
            "FLOAT" | "DOUBLE" | "DECIMAL" => {
                let v: Option<f64> = row.try_get(col.ordinal()).ok();
                v.map(DbValue::Float).unwrap_or(DbValue::Null)
            }
            "BLOB" | "BINARY" | "VARBINARY" => {
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
