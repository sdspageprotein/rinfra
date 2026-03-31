use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Versioned key management with rotation support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyVersion {
    pub version: u32,
    pub created_at: u64,
    pub is_active: bool,
}

/// Encryption/decryption abstraction. Algorithm is pluggable.
#[async_trait]
pub trait Crypto: Send + Sync + 'static {
    async fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, AppError>;
    async fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, AppError>;
}

/// Key provider abstraction for decoupling key storage from crypto algorithms.
#[async_trait]
pub trait KeyProvider: Send + Sync + 'static {
    async fn get_key(&self, key_id: &str) -> Result<Vec<u8>, AppError>;
}

/// Key provider with rotation and versioning support.
#[async_trait]
pub trait VersionedKeyProvider: Send + Sync + 'static {
    async fn get_key_versioned(&self, name: &str, version: Option<u32>) -> Result<Vec<u8>, AppError>;
    async fn get_active_version(&self, name: &str) -> Result<u32, AppError>;
    async fn list_versions(&self, name: &str) -> Result<Vec<KeyVersion>, AppError>;
    async fn rotate_key(&self, name: &str) -> Result<u32, AppError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubCrypto;

    #[async_trait]
    impl Crypto for StubCrypto {
        async fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, AppError> {
            Ok(plaintext.to_vec())
        }
        async fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, AppError> {
            Ok(ciphertext.to_vec())
        }
    }

    #[tokio::test]
    async fn test_stub_crypto_roundtrip() {
        let crypto = StubCrypto;
        let data = b"hello world";
        let encrypted = crypto.encrypt(data).await.unwrap();
        let decrypted = crypto.decrypt(&encrypted).await.unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_key_version_serialize_deserialize() {
        use super::KeyVersion;
        let kv = KeyVersion {
            version: 1,
            created_at: 12345,
            is_active: true,
        };
        let json = serde_json::to_string(&kv).unwrap();
        let parsed: KeyVersion = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.created_at, 12345);
        assert!(parsed.is_active);
    }

    #[test]
    fn test_key_version_clone() {
        use super::KeyVersion;
        let kv = KeyVersion {
            version: 2,
            created_at: 999,
            is_active: false,
        };
        let cloned = kv.clone();
        assert_eq!(cloned.version, kv.version);
        assert_eq!(cloned.created_at, kv.created_at);
        assert_eq!(cloned.is_active, kv.is_active);
    }
}
