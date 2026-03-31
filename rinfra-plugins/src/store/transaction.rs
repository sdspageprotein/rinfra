use async_trait::async_trait;
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::store::{DbExecutor, DbRow, DbValue, Transaction};
use sqlx::{Column, Postgres, Row, TypeInfo};
use tokio::sync::Mutex;

/// PostgreSQL transaction wrapper.
pub struct PgTransaction {
    tx: Mutex<Option<sqlx::Transaction<'static, Postgres>>>,
}

impl PgTransaction {
    pub fn new(tx: sqlx::Transaction<'static, Postgres>) -> Self {
        Self {
            tx: Mutex::new(Some(tx)),
        }
    }
}

#[async_trait]
impl DbExecutor for PgTransaction {
    async fn execute(&self, sql: &str, params: &[DbValue]) -> Result<u64, AppError> {
        let mut guard = self.tx.lock().await;
        let tx = guard.as_mut().ok_or_else(|| {
            AppError::new(ErrorCode::StoreQueryFailed, "transaction already consumed")
        })?;
        let mut q = sqlx::query(sql);
        for p in params {
            q = bind_param(q, p);
        }
        let result = q.execute(&mut **tx).await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("tx execute failed: {e}"))
        })?;
        Ok(result.rows_affected())
    }

    async fn query(&self, sql: &str, params: &[DbValue]) -> Result<Vec<DbRow>, AppError> {
        let mut guard = self.tx.lock().await;
        let tx = guard.as_mut().ok_or_else(|| {
            AppError::new(ErrorCode::StoreQueryFailed, "transaction already consumed")
        })?;
        let mut q = sqlx::query(sql);
        for p in params {
            q = bind_param(q, p);
        }
        let rows = q.fetch_all(&mut **tx).await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("tx query failed: {e}"))
        })?;
        rows.iter().map(pg_row_to_db_row).collect()
    }

    async fn query_one(&self, sql: &str, params: &[DbValue]) -> Result<Option<DbRow>, AppError> {
        let mut guard = self.tx.lock().await;
        let tx = guard.as_mut().ok_or_else(|| {
            AppError::new(ErrorCode::StoreQueryFailed, "transaction already consumed")
        })?;
        let mut q = sqlx::query(sql);
        for p in params {
            q = bind_param(q, p);
        }
        let row = q.fetch_optional(&mut **tx).await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("tx query_one failed: {e}"))
        })?;
        match row {
            Some(r) => Ok(Some(pg_row_to_db_row(&r)?)),
            None => Ok(None),
        }
    }
}

#[async_trait]
impl Transaction for PgTransaction {
    async fn commit(self: Box<Self>) -> Result<(), AppError> {
        let mut guard = self.tx.lock().await;
        let tx = guard.take().ok_or_else(|| {
            AppError::new(ErrorCode::StoreQueryFailed, "transaction already consumed")
        })?;
        tx.commit().await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("commit failed: {e}"))
        })
    }

    async fn rollback(self: Box<Self>) -> Result<(), AppError> {
        let mut guard = self.tx.lock().await;
        let tx = guard.take().ok_or_else(|| {
            AppError::new(ErrorCode::StoreQueryFailed, "transaction already consumed")
        })?;
        tx.rollback().await.map_err(|e| {
            AppError::new(ErrorCode::StoreQueryFailed, format!("rollback failed: {e}"))
        })
    }
}

fn bind_param<'q>(
    q: sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>,
    val: &'q DbValue,
) -> sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments> {
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
