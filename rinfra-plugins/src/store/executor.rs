use std::time::Instant;

use async_trait::async_trait;
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::store::{DbExecutor, DbRow, DbValue};
use sqlx::{Column, PgPool, Row, TypeInfo};

/// PostgreSQL executor backed by a connection pool.
pub struct PgExecutor {
    pool: PgPool,
    slow_threshold_ms: u64,
}

impl PgExecutor {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            slow_threshold_ms: 200,
        }
    }

    pub fn with_slow_threshold_ms(mut self, ms: u64) -> Self {
        self.slow_threshold_ms = ms;
        self
    }

    fn check_slow(&self, sql: &str, elapsed: std::time::Duration) {
        let ms = elapsed.as_millis() as u64;
        if self.slow_threshold_ms > 0 && ms >= self.slow_threshold_ms {
            tracing::warn!(
                elapsed_ms = ms,
                threshold_ms = self.slow_threshold_ms,
                sql = sql,
                "slow query detected"
            );
        }
    }
}

#[async_trait]
impl DbExecutor for PgExecutor {
    async fn execute(&self, sql: &str, params: &[DbValue]) -> Result<u64, AppError> {
        let mut q = sqlx::query(sql);
        for p in params {
            q = bind_param(q, p);
        }
        let start = Instant::now();
        let result = q.execute(&self.pool).await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("execute failed: {e}"))
        })?;
        self.check_slow(sql, start.elapsed());
        Ok(result.rows_affected())
    }

    async fn query(&self, sql: &str, params: &[DbValue]) -> Result<Vec<DbRow>, AppError> {
        let mut q = sqlx::query(sql);
        for p in params {
            q = bind_param(q, p);
        }
        let start = Instant::now();
        let rows = q.fetch_all(&self.pool).await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("query failed: {e}"))
        })?;
        self.check_slow(sql, start.elapsed());
        rows.iter().map(pg_row_to_db_row).collect()
    }

    async fn query_one(&self, sql: &str, params: &[DbValue]) -> Result<Option<DbRow>, AppError> {
        let mut q = sqlx::query(sql);
        for p in params {
            q = bind_param(q, p);
        }
        let start = Instant::now();
        let row = q.fetch_optional(&self.pool).await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("query_one failed: {e}"))
        })?;
        self.check_slow(sql, start.elapsed());
        match row {
            Some(r) => Ok(Some(pg_row_to_db_row(&r)?)),
            None => Ok(None),
        }
    }
}

fn bind_param<'q>(
    q: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    val: &'q DbValue,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    match val {
        DbValue::Null => q.bind(None::<String>),
        DbValue::Bool(v) => q.bind(v),
        DbValue::Int(v) => q.bind(v),
        DbValue::Float(v) => q.bind(v),
        DbValue::Text(v) => q.bind(v.as_str()),
        DbValue::Bytes(v) => q.bind(v.as_slice()),
    }
}

fn pg_row_to_db_row(row: &sqlx::postgres::PgRow) -> Result<DbRow, AppError> {
    let columns: Vec<String> = row.columns().iter().map(|c| c.name().to_string()).collect();
    let mut values = Vec::with_capacity(columns.len());
    for col in row.columns() {
        let type_name = col.type_info().name();
        let val = match type_name {
            "BOOL" => {
                let v: Option<bool> = row.try_get(col.ordinal()).ok();
                v.map(DbValue::Bool).unwrap_or(DbValue::Null)
            }
            "INT2" | "INT4" | "INT8" => {
                let v: Option<i64> = row.try_get(col.ordinal()).ok();
                v.map(DbValue::Int).unwrap_or(DbValue::Null)
            }
            "FLOAT4" | "FLOAT8" | "NUMERIC" => {
                let v: Option<f64> = row.try_get(col.ordinal()).ok();
                v.map(DbValue::Float).unwrap_or(DbValue::Null)
            }
            "BYTEA" => {
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
