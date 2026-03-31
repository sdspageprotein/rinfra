use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{AppError, ErrorCode};

use super::DbConnection;

/// Registry for managing multiple database connections (data sources).
///
/// Supports a default connection and named connections for multi-database setups.
pub struct StoreRegistry {
    stores: HashMap<String, Arc<dyn DbConnection>>,
    default_name: String,
}

impl StoreRegistry {
    pub fn new() -> Self {
        Self {
            stores: HashMap::new(),
            default_name: "default".to_string(),
        }
    }

    /// Register a named database connection.
    pub fn register(&mut self, name: &str, db: Arc<dyn DbConnection>) {
        if self.stores.is_empty() {
            self.default_name = name.to_string();
        }
        self.stores.insert(name.to_string(), db);
    }

    /// Get a connection by name.
    pub fn get(&self, name: &str) -> Option<&Arc<dyn DbConnection>> {
        self.stores.get(name)
    }

    /// Get the default connection.
    pub fn default(&self) -> Option<&Arc<dyn DbConnection>> {
        self.stores.get(&self.default_name)
    }

    /// Set which connection is the default.
    pub fn set_default(&mut self, name: &str) -> Result<(), AppError> {
        if !self.stores.contains_key(name) {
            return Err(AppError::new(
                ErrorCode::StoreConnectionFailed,
                format!("store '{name}' not found in registry"),
            ));
        }
        self.default_name = name.to_string();
        Ok(())
    }

    /// List all registered connection names.
    pub fn list_names(&self) -> Vec<&str> {
        self.stores.keys().map(|s| s.as_str()).collect()
    }

    /// Number of registered connections.
    pub fn len(&self) -> usize {
        self.stores.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stores.is_empty()
    }
}

impl Default for StoreRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_registry() {
        let reg = StoreRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
        assert!(reg.default().is_none());
        assert!(reg.list_names().is_empty());
    }

    #[test]
    fn test_set_default_missing() {
        let mut reg = StoreRegistry::new();
        assert!(reg.set_default("nope").is_err());
    }
}
