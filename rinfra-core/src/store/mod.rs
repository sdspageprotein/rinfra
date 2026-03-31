mod audit;
mod mapping;
mod registry;
mod spec;
mod value;

use async_trait::async_trait;

use crate::error::AppError;

pub use audit::{now_unix_secs, Auditable, SoftDeletable};
pub use mapping::{FromRow, ToRow};
pub use registry::StoreRegistry;
pub use spec::{AndSpec, BetweenSpec, EqSpec, InSpec, LikeSpec, OrSpec, Specification};
pub use value::{DbValue, FromDbValue, IntoDbValue};

// ---------------------------------------------------------------------------
// Lifecycle (existing Store trait — kept for backward compat)
// ---------------------------------------------------------------------------

/// Database store abstraction for connection pool lifecycle management.
#[async_trait]
pub trait Store: Send + Sync + 'static {
    async fn connect(&self) -> Result<(), AppError>;
    async fn disconnect(&self) -> Result<(), AppError>;
    async fn health_check(&self) -> Result<bool, AppError>;
}

// ---------------------------------------------------------------------------
// Query result row
// ---------------------------------------------------------------------------

/// A single row returned from a database query.
pub struct DbRow {
    columns: Vec<String>,
    values: Vec<DbValue>,
}

impl DbRow {
    pub fn new(columns: Vec<String>, values: Vec<DbValue>) -> Self {
        Self { columns, values }
    }

    /// Get a typed value by column name.
    pub fn get<T: FromDbValue>(&self, column: &str) -> Result<T, AppError> {
        let idx = self
            .columns
            .iter()
            .position(|c| c == column)
            .ok_or_else(|| {
                AppError::new(
                    crate::error::ErrorCode::StoreQueryFailed,
                    format!("column '{column}' not found in row"),
                )
            })?;
        T::from_db_value(&self.values[idx])
    }

    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    pub fn values(&self) -> &[DbValue] {
        &self.values
    }
}

// ---------------------------------------------------------------------------
// Executor / Transaction / Connection
// ---------------------------------------------------------------------------

/// Abstraction for executing SQL statements against a database.
#[async_trait]
pub trait DbExecutor: Send + Sync {
    /// Execute a statement, returning the number of affected rows.
    async fn execute(&self, sql: &str, params: &[DbValue]) -> Result<u64, AppError>;
    /// Execute a query, returning all result rows.
    async fn query(&self, sql: &str, params: &[DbValue]) -> Result<Vec<DbRow>, AppError>;
    /// Execute a query expecting at most one row.
    async fn query_one(&self, sql: &str, params: &[DbValue]) -> Result<Option<DbRow>, AppError>;
}

/// A database transaction that can be committed or rolled back.
#[async_trait]
pub trait Transaction: DbExecutor {
    async fn commit(self: Box<Self>) -> Result<(), AppError>;
    async fn rollback(self: Box<Self>) -> Result<(), AppError>;
}

/// Full database connection providing executor access and transaction support.
/// Extends `Store` to include query capabilities.
#[async_trait]
pub trait DbConnection: Store {
    /// Obtain an executor backed by the connection pool.
    async fn executor(&self) -> Result<Box<dyn DbExecutor>, AppError>;
    /// Begin a new transaction.
    async fn begin(&self) -> Result<Box<dyn Transaction>, AppError>;
}

// ---------------------------------------------------------------------------
// Entity / Repository
// ---------------------------------------------------------------------------

/// Represents a database row that can be identified by a primary key.
pub trait Entity: Send + Sync + 'static {
    type Id: Send + Sync + 'static;
    fn id(&self) -> &Self::Id;
}

/// Sort direction for query ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Asc,
    Desc,
}

/// A single ordering clause.
#[derive(Debug, Clone)]
pub struct OrderBy {
    pub field: String,
    pub direction: SortDirection,
}

/// Pagination and ordering options for queries.
#[derive(Debug, Clone)]
pub struct QueryOptions {
    pub limit: i64,
    pub offset: i64,
    pub order_by: Vec<OrderBy>,
}

impl Default for QueryOptions {
    fn default() -> Self {
        Self {
            limit: 100,
            offset: 0,
            order_by: Vec::new(),
        }
    }
}

/// Generic repository trait for basic CRUD operations.
#[async_trait]
pub trait Repository<T: Entity>: Send + Sync + 'static {
    async fn find_by_id(&self, id: &T::Id) -> Result<Option<T>, AppError>;
    async fn find_all(&self, opts: &QueryOptions) -> Result<Vec<T>, AppError>;
    async fn count(&self) -> Result<u64, AppError>;
    async fn create(&self, entity: &T) -> Result<T, AppError>;
    async fn update(&self, entity: &T) -> Result<T, AppError>;
    async fn delete(&self, id: &T::Id) -> Result<bool, AppError>;
    async fn exists(&self, id: &T::Id) -> Result<bool, AppError>;

    /// Find entities matching a specification with pagination.
    async fn find_by(
        &self,
        _spec: &dyn Specification,
        _opts: &QueryOptions,
    ) -> Result<Vec<T>, AppError> {
        Err(AppError::new(
            crate::error::ErrorCode::Internal,
            "find_by not implemented",
        ))
    }

    /// Count entities matching a specification.
    async fn count_by(&self, _spec: &dyn Specification) -> Result<u64, AppError> {
        Err(AppError::new(
            crate::error::ErrorCode::Internal,
            "count_by not implemented",
        ))
    }

    /// Delete entities matching a specification, returning rows affected.
    async fn delete_by(&self, _spec: &dyn Specification) -> Result<u64, AppError> {
        Err(AppError::new(
            crate::error::ErrorCode::Internal,
            "delete_by not implemented",
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    struct StubEntity {
        id: i64,
    }
    impl Entity for StubEntity {
        type Id = i64;
        fn id(&self) -> &i64 {
            &self.id
        }
    }

    #[test]
    fn test_entity_id() {
        let e = StubEntity { id: 42 };
        assert_eq!(*e.id(), 42);
    }

    #[test]
    fn test_query_options_default() {
        let opts = QueryOptions::default();
        assert_eq!(opts.limit, 100);
        assert_eq!(opts.offset, 0);
        assert!(opts.order_by.is_empty());
    }

    #[test]
    fn test_db_row_get_existing_column() {
        let row = DbRow::new(
            vec!["id".into(), "name".into()],
            vec![DbValue::Int(1), DbValue::Text("alice".into())],
        );
        let id: i64 = row.get("id").unwrap();
        assert_eq!(id, 1);
        let name: String = row.get("name").unwrap();
        assert_eq!(name, "alice");
    }

    #[test]
    fn test_db_row_get_missing_column() {
        let row = DbRow::new(vec!["id".into()], vec![DbValue::Int(1)]);
        let result = row.get::<i64>("missing");
        assert!(result.is_err());
    }

    #[test]
    fn test_sort_direction() {
        let asc = SortDirection::Asc;
        let desc = SortDirection::Desc;
        assert_ne!(asc, desc);
    }
}
