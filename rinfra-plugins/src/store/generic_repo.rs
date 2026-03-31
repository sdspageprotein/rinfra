use std::marker::PhantomData;
use std::sync::Arc;

use async_trait::async_trait;
use rinfra_core::error::AppError;
use rinfra_core::store::{
    DbConnection, DbValue, Entity, FromRow, QueryOptions, Repository, SortDirection, Specification,
    ToRow,
};

/// A generic repository that auto-generates SQL CRUD based on `FromRow` + `ToRow`.
///
/// Works with any database backend implementing `DbConnection`.
pub struct GenericRepository<T: Entity + FromRow + ToRow> {
    db: Arc<dyn DbConnection>,
    _phantom: PhantomData<T>,
}

impl<T: Entity + FromRow + ToRow> GenericRepository<T> {
    pub fn new(db: Arc<dyn DbConnection>) -> Self {
        Self {
            db,
            _phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<T> Repository<T> for GenericRepository<T>
where
    T: Entity<Id = i64> + FromRow + ToRow + Send + Sync + 'static,
{
    async fn find_by_id(&self, id: &i64) -> Result<Option<T>, AppError> {
        let table = T::table_name();
        let id_col = T::id_column();
        let sql = format!("SELECT * FROM {table} WHERE {id_col} = $1");
        let exec = self.db.executor().await?;
        let row = exec.query_one(&sql, &[DbValue::Int(*id)]).await?;
        match row {
            Some(r) => Ok(Some(T::from_row(&r)?)),
            None => Ok(None),
        }
    }

    async fn find_all(&self, opts: &QueryOptions) -> Result<Vec<T>, AppError> {
        let table = T::table_name();
        let mut sql = format!("SELECT * FROM {table}");
        if !opts.order_by.is_empty() {
            let clauses: Vec<String> = opts
                .order_by
                .iter()
                .map(|o| {
                    let dir = match o.direction {
                        SortDirection::Asc => "ASC",
                        SortDirection::Desc => "DESC",
                    };
                    format!("{} {dir}", o.field)
                })
                .collect();
            sql.push_str(" ORDER BY ");
            sql.push_str(&clauses.join(", "));
        }
        sql.push_str(&format!(" LIMIT {} OFFSET {}", opts.limit, opts.offset));
        let exec = self.db.executor().await?;
        let rows = exec.query(&sql, &[]).await?;
        rows.iter().map(|r| T::from_row(r)).collect()
    }

    async fn count(&self) -> Result<u64, AppError> {
        let table = T::table_name();
        let sql = format!("SELECT count(*) AS cnt FROM {table}");
        let exec = self.db.executor().await?;
        let row = exec.query_one(&sql, &[]).await?;
        match row {
            Some(r) => {
                let cnt: i64 = r.get("cnt")?;
                Ok(cnt as u64)
            }
            None => Ok(0),
        }
    }

    async fn create(&self, entity: &T) -> Result<T, AppError> {
        let table = T::table_name();
        let params = entity.to_params();
        let cols: Vec<&str> = params.iter().map(|(c, _)| *c).collect();
        let placeholders: Vec<String> = (1..=cols.len()).map(|i| format!("${i}")).collect();
        let sql = format!(
            "INSERT INTO {table} ({}) VALUES ({}) RETURNING *",
            cols.join(", "),
            placeholders.join(", ")
        );
        let values: Vec<DbValue> = params.into_iter().map(|(_, v)| v).collect();
        let exec = self.db.executor().await?;
        let row = exec.query_one(&sql, &values).await?;
        match row {
            Some(r) => T::from_row(&r),
            None => Err(AppError::new(
                rinfra_core::error::ErrorCode::StoreQueryFailed,
                "INSERT RETURNING returned no rows",
            )),
        }
    }

    async fn update(&self, entity: &T) -> Result<T, AppError> {
        let table = T::table_name();
        let id_col = T::id_column();
        let params = entity.to_params();
        let mut set_clauses = Vec::new();
        let mut values = Vec::new();
        for (i, (col, val)) in params.into_iter().enumerate() {
            set_clauses.push(format!("{col} = ${}", i + 1));
            values.push(val);
        }
        let id_param_idx = values.len() + 1;
        values.push(DbValue::Int(*entity.id()));
        let sql = format!(
            "UPDATE {table} SET {} WHERE {id_col} = ${id_param_idx} RETURNING *",
            set_clauses.join(", ")
        );
        let exec = self.db.executor().await?;
        let row = exec.query_one(&sql, &values).await?;
        match row {
            Some(r) => T::from_row(&r),
            None => Err(AppError::new(
                rinfra_core::error::ErrorCode::StoreQueryFailed,
                "UPDATE RETURNING returned no rows",
            )),
        }
    }

    async fn delete(&self, id: &i64) -> Result<bool, AppError> {
        let table = T::table_name();
        let id_col = T::id_column();
        let sql = format!("DELETE FROM {table} WHERE {id_col} = $1");
        let exec = self.db.executor().await?;
        let affected = exec.execute(&sql, &[DbValue::Int(*id)]).await?;
        Ok(affected > 0)
    }

    async fn exists(&self, id: &i64) -> Result<bool, AppError> {
        let table = T::table_name();
        let id_col = T::id_column();
        let sql = format!("SELECT 1 FROM {table} WHERE {id_col} = $1 LIMIT 1");
        let exec = self.db.executor().await?;
        let row = exec.query_one(&sql, &[DbValue::Int(*id)]).await?;
        Ok(row.is_some())
    }

    async fn find_by(
        &self,
        spec: &dyn Specification,
        opts: &QueryOptions,
    ) -> Result<Vec<T>, AppError> {
        let table = T::table_name();
        let (where_clause, params) = spec.to_where_clause(1);
        let mut sql = format!("SELECT * FROM {table} WHERE {where_clause}");
        if !opts.order_by.is_empty() {
            let clauses: Vec<String> = opts
                .order_by
                .iter()
                .map(|o| {
                    let dir = match o.direction {
                        SortDirection::Asc => "ASC",
                        SortDirection::Desc => "DESC",
                    };
                    format!("{} {dir}", o.field)
                })
                .collect();
            sql.push_str(" ORDER BY ");
            sql.push_str(&clauses.join(", "));
        }
        sql.push_str(&format!(" LIMIT {} OFFSET {}", opts.limit, opts.offset));
        let exec = self.db.executor().await?;
        let rows = exec.query(&sql, &params).await?;
        rows.iter().map(|r| T::from_row(r)).collect()
    }

    async fn count_by(&self, spec: &dyn Specification) -> Result<u64, AppError> {
        let table = T::table_name();
        let (where_clause, params) = spec.to_where_clause(1);
        let sql = format!("SELECT count(*) AS cnt FROM {table} WHERE {where_clause}");
        let exec = self.db.executor().await?;
        let row = exec.query_one(&sql, &params).await?;
        match row {
            Some(r) => {
                let cnt: i64 = r.get("cnt")?;
                Ok(cnt as u64)
            }
            None => Ok(0),
        }
    }

    async fn delete_by(&self, spec: &dyn Specification) -> Result<u64, AppError> {
        let table = T::table_name();
        let (where_clause, params) = spec.to_where_clause(1);
        let sql = format!("DELETE FROM {table} WHERE {where_clause}");
        let exec = self.db.executor().await?;
        exec.execute(&sql, &params).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct TestUser {
        id: i64,
        name: String,
    }

    impl Entity for TestUser {
        type Id = i64;
        fn id(&self) -> &i64 {
            &self.id
        }
    }

    impl FromRow for TestUser {
        fn from_row(row: &rinfra_core::store::DbRow) -> Result<Self, AppError> {
            Ok(Self {
                id: row.get("id")?,
                name: row.get("name")?,
            })
        }
    }

    impl ToRow for TestUser {
        fn table_name() -> &'static str {
            "test_users"
        }
        fn columns() -> &'static [&'static str] {
            &["name"]
        }
        fn to_params(&self) -> Vec<(&'static str, DbValue)> {
            vec![("name", DbValue::Text(self.name.clone()))]
        }
    }

    #[test]
    fn test_generic_repo_can_be_constructed() {
        // Compile-time verification that GenericRepository builds with the trait bounds.
        fn _assert_repo_is_send_sync<T: Send + Sync>() {}
        _assert_repo_is_send_sync::<GenericRepository<TestUser>>();
    }
}
