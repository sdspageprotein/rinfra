use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use async_trait::async_trait;
use rand::RngCore;
use tokio::sync::RwLock;
use tracing::info;

use rinfra_core::config::RotatingKeyConfig;
use rinfra_core::crypto::{KeyVersion, VersionedKeyProvider};
use rinfra_core::error::{AppError, ErrorCode};

struct KeyStore {
    versions: Vec<VersionedKey>,
    active_version: u32,
}

struct VersionedKey {
    version: u32,
    key: Vec<u8>,
    created_at: u64,
}

pub struct RotatingKeyProvider {
    keys: Arc<RwLock<HashMap<String, KeyStore>>>,
    config: RotatingKeyConfig,
}

impl RotatingKeyProvider {
    pub fn new(config: &RotatingKeyConfig) -> Self {
        Self {
            keys: Arc::new(RwLock::new(HashMap::new())),
            config: config.clone(),
        }
    }

    /// Seed an initial key for a given name.
    pub async fn seed_key(&self, name: &str, key: Vec<u8>) {
        let now = Self::now_secs();
        let mut keys = self.keys.write().await;
        let store = keys.entry(name.to_string()).or_insert_with(|| KeyStore {
            versions: Vec::new(),
            active_version: 0,
        });
        let version = store.versions.len() as u32 + 1;
        store.versions.push(VersionedKey {
            version,
            key,
            created_at: now,
        });
        store.active_version = version;
    }

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

#[async_trait]
impl VersionedKeyProvider for RotatingKeyProvider {
    async fn get_key_versioned(
        &self,
        name: &str,
        version: Option<u32>,
    ) -> Result<Vec<u8>, AppError> {
        let keys = self.keys.read().await;
        let store = keys.get(name).ok_or_else(|| {
            AppError::new(ErrorCode::CryptoKeyNotFound, format!("key '{name}' not found"))
        })?;
        let ver = version.unwrap_or(store.active_version);
        let vk = store.versions.iter().find(|v| v.version == ver).ok_or_else(|| {
            AppError::new(
                ErrorCode::CryptoKeyNotFound,
                format!("key '{name}' version {ver} not found"),
            )
        })?;
        Ok(vk.key.clone())
    }

    async fn get_active_version(&self, name: &str) -> Result<u32, AppError> {
        let keys = self.keys.read().await;
        let store = keys.get(name).ok_or_else(|| {
            AppError::new(ErrorCode::CryptoKeyNotFound, format!("key '{name}' not found"))
        })?;
        Ok(store.active_version)
    }

    async fn list_versions(&self, name: &str) -> Result<Vec<KeyVersion>, AppError> {
        let keys = self.keys.read().await;
        let store = keys.get(name).ok_or_else(|| {
            AppError::new(ErrorCode::CryptoKeyNotFound, format!("key '{name}' not found"))
        })?;
        Ok(store
            .versions
            .iter()
            .map(|v| KeyVersion {
                version: v.version,
                created_at: v.created_at,
                is_active: v.version == store.active_version,
            })
            .collect())
    }

    async fn rotate_key(&self, name: &str) -> Result<u32, AppError> {
        let mut keys = self.keys.write().await;
        let store = keys.get_mut(name).ok_or_else(|| {
            AppError::new(ErrorCode::CryptoKeyNotFound, format!("key '{name}' not found"))
        })?;

        // Generate a new random key (32 bytes for AES-256)
        let mut new_key = vec![0u8; 32];
        rand::thread_rng().fill_bytes(&mut new_key);

        let new_version = store
            .versions
            .iter()
            .map(|v| v.version)
            .max()
            .unwrap_or(0)
            + 1;
        store.versions.push(VersionedKey {
            version: new_version,
            key: new_key,
            created_at: Self::now_secs(),
        });
        store.active_version = new_version;

        // Trim old versions if exceeding max
        let max = self.config.max_key_versions as usize;
        if store.versions.len() > max {
            let drain_count = store.versions.len() - max;
            store.versions.drain(..drain_count);
        }

        info!(name = %name, version = new_version, "key rotated");
        Ok(new_version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> RotatingKeyConfig {
        RotatingKeyConfig {
            enabled: true,
            rotation_interval_secs: 60,
            max_key_versions: 3,
        }
    }

    #[tokio::test]
    async fn test_seed_and_get() {
        let provider = RotatingKeyProvider::new(&test_config());
        provider.seed_key("master", vec![1, 2, 3]).await;
        let key = provider.get_key_versioned("master", None).await.unwrap();
        assert_eq!(key, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_get_specific_version() {
        let provider = RotatingKeyProvider::new(&test_config());
        provider.seed_key("master", vec![1]).await;
        provider.seed_key("master", vec![2]).await;
        let v1 = provider.get_key_versioned("master", Some(1)).await.unwrap();
        assert_eq!(v1, vec![1]);
        let v2 = provider.get_key_versioned("master", Some(2)).await.unwrap();
        assert_eq!(v2, vec![2]);
    }

    #[tokio::test]
    async fn test_get_missing_key() {
        let provider = RotatingKeyProvider::new(&test_config());
        let result = provider.get_key_versioned("nope", None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rotate_key() {
        let provider = RotatingKeyProvider::new(&test_config());
        provider.seed_key("master", vec![1]).await;
        let v = provider.rotate_key("master").await.unwrap();
        assert_eq!(v, 2);
        let versions = provider.list_versions("master").await.unwrap();
        assert_eq!(versions.len(), 2);
        assert!(versions.iter().any(|kv| kv.version == 2 && kv.is_active));
    }

    #[tokio::test]
    async fn test_rotate_trims_old_versions() {
        let provider = RotatingKeyProvider::new(&test_config());
        provider.seed_key("master", vec![1]).await;
        provider.rotate_key("master").await.unwrap();
        provider.rotate_key("master").await.unwrap();
        provider.rotate_key("master").await.unwrap();
        let versions = provider.list_versions("master").await.unwrap();
        assert_eq!(versions.len(), 3); // max_key_versions = 3
    }

    #[tokio::test]
    async fn test_active_version() {
        let provider = RotatingKeyProvider::new(&test_config());
        provider.seed_key("master", vec![1]).await;
        let v = provider.get_active_version("master").await.unwrap();
        assert_eq!(v, 1);
    }
}
