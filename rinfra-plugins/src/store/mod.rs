mod executor;
mod generic_repo;
#[cfg(feature = "mysql")]
pub mod mysql;
#[cfg(feature = "sqlite")]
pub mod sqlite;
mod transaction;

use std::path::Path;

use async_trait::async_trait;
use rinfra_core::config::PostgresConfig;
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::store::{DbConnection, DbExecutor, Store, Transaction};
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tokio::sync::RwLock;
use tracing::{info, warn};

pub use executor::PgExecutor;
pub use generic_repo::GenericRepository;
pub use transaction::PgTransaction;

pub struct PostgresStore {
    config: PostgresConfig,
    pool: RwLock<Option<PgPool>>,
}

impl PostgresStore {
    pub fn new(config: PostgresConfig) -> Self {
        Self {
            config,
            pool: RwLock::new(None),
        }
    }

    /// Get a reference to the underlying connection pool.
    /// Returns an error if not connected.
    pub async fn pool(&self) -> Result<PgPool, AppError> {
        let guard = self.pool.read().await;
        guard.clone().ok_or_else(|| {
            AppError::new(
                ErrorCode::StoreConnectionFailed,
                "postgres store not connected",
            )
        })
    }
}

#[async_trait]
impl Store for PostgresStore {
    async fn connect(&self) -> Result<(), AppError> {
        if !self.config.enabled {
            info!("postgres store disabled, skipping connect");
            return Ok(());
        }
        info!(
            url = %self.config.url,
            max_connections = self.config.max_connections,
            "postgres store connecting"
        );
        let pg_pool = PgPoolOptions::new()
            .max_connections(self.config.max_connections)
            .idle_timeout(std::time::Duration::from_secs(self.config.idle_timeout_secs))
            .connect(&self.config.url)
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::StoreConnectionFailed,
                    format!("postgres connect failed: {e}"),
                )
            })?;
        if let Some(ref migrations_path) = self.config.migrations_path {
            let migrator =
                Migrator::new(Path::new(migrations_path))
                    .await
                    .map_err(|e| {
                        AppError::new(
                            ErrorCode::Internal,
                            format!("failed to load migrations from '{migrations_path}': {e}"),
                        )
                    })?;
            migrator.run(&pg_pool).await.map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("migration failed: {e}"),
                )
            })?;
            info!(path = %migrations_path, "database migrations applied");
        }

        *self.pool.write().await = Some(pg_pool);
        info!("postgres store connected");
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), AppError> {
        if let Some(pool) = self.pool.write().await.take() {
            pool.close().await;
            info!("postgres store disconnected");
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
        let labels = [("backend", "postgres")];
        metrics::gauge!("db_pool_size", &labels).set(pool.size() as f64);
        metrics::gauge!("db_pool_idle", &labels).set(pool.num_idle() as f64);
        match sqlx::query("SELECT 1").execute(pool).await {
            Ok(_) => Ok(true),
            Err(e) => {
                warn!(error = %e, "postgres health check failed");
                Ok(false)
            }
        }
    }
}

impl PostgresStore {
    /// Pool statistics (active, idle, total size).
    pub async fn pool_stats(&self) -> Option<(u32, u32, u32)> {
        let guard = self.pool.read().await;
        guard.as_ref().map(|p| (p.size(), p.num_idle() as u32, p.size()))
    }
}

#[async_trait]
impl DbConnection for PostgresStore {
    async fn executor(&self) -> Result<Box<dyn DbExecutor>, AppError> {
        let pool = self.pool().await?;
        let threshold_ms = self.config.slow_query_threshold_ms;
        Ok(Box::new(PgExecutor::new(pool).with_slow_threshold_ms(threshold_ms)))
    }

    async fn begin(&self) -> Result<Box<dyn Transaction>, AppError> {
        let pool = self.pool().await?;
        let tx = pool.begin().await.map_err(|e| {
            AppError::new(
                ErrorCode::StoreQueryFailed,
                format!("failed to begin transaction: {e}"),
            )
        })?;
        Ok(Box::new(PgTransaction::new(tx)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_postgres_store_disabled() {
        let config = PostgresConfig::default();
        let store = PostgresStore::new(config);
        store.connect().await.unwrap();
        assert!(store.health_check().await.unwrap());
        store.disconnect().await.unwrap();
    }

    #[tokio::test]
    async fn test_postgres_store_not_connected_health_false() {
        let config = PostgresConfig {
            enabled: true,
            url: "postgres://localhost:5432/test".to_string(),
            max_connections: 5,
            idle_timeout_secs: 60,
            migrations_path: None,
            slow_query_threshold_ms: 200,
            ..Default::default()
        };
        let store = PostgresStore::new(config);
        assert!(!store.health_check().await.unwrap());
    }

    #[tokio::test]
    async fn test_postgres_disconnect_when_not_connected() {
        let config = PostgresConfig::default();
        let store = PostgresStore::new(config);
        assert!(store.disconnect().await.is_ok());
    }

    #[tokio::test]
    async fn test_pool_not_connected_returns_error() {
        let config = PostgresConfig::default();
        let store = PostgresStore::new(config);
        // disabled store won't connect, pool() should error
        store.connect().await.unwrap();
        let result = store.pool().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore = "requires a running Postgres instance at localhost:5432"]
    async fn test_postgres_store_connect_and_health() {
        let config = PostgresConfig {
            enabled: true,
            url: "postgres://rinfra:rinfra@localhost:5432/rinfra".to_string(),
            max_connections: 2,
            idle_timeout_secs: 60,
            migrations_path: None,
            slow_query_threshold_ms: 200,
            ..Default::default()
        };
        let store = PostgresStore::new(config);
        store.connect().await.unwrap();
        assert!(store.health_check().await.unwrap());
        store.disconnect().await.unwrap();
        assert!(!store.health_check().await.unwrap());
    }
}
