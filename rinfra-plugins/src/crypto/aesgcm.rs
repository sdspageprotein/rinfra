use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use async_trait::async_trait;
use rinfra_core::crypto::Crypto;
use rinfra_core::error::{AppError, ErrorCode};

pub struct AesGcmCrypto {
    key: Vec<u8>,
}

impl AesGcmCrypto {
    /// Create a new AES-256-GCM crypto instance.
    /// Key must be exactly 32 bytes for AES-256.
    pub fn new(key: Vec<u8>) -> Result<Self, AppError> {
        if key.len() != 32 {
            return Err(AppError::new(
                ErrorCode::CryptoEncryptFailed,
                format!("AES-256 requires 32-byte key, got {}", key.len()),
            ));
        }
        Ok(Self { key })
    }
}

#[async_trait]
impl Crypto for AesGcmCrypto {
    async fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, AppError> {
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| AppError::new(ErrorCode::CryptoEncryptFailed, e.to_string()))?;

        // Generate a random 12-byte nonce
        let nonce_bytes: [u8; 12] = rand::random();
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| AppError::new(ErrorCode::CryptoEncryptFailed, e.to_string()))?;

        // Prepend nonce to ciphertext for storage
        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    async fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, AppError> {
        if ciphertext.len() < 12 {
            return Err(AppError::new(
                ErrorCode::CryptoDecryptFailed,
                "ciphertext too short (missing nonce)",
            ));
        }

        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| AppError::new(ErrorCode::CryptoDecryptFailed, e.to_string()))?;

        let (nonce_bytes, encrypted) = ciphertext.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        cipher
            .decrypt(nonce, encrypted)
            .map_err(|e| AppError::new(ErrorCode::CryptoDecryptFailed, e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> Vec<u8> {
        vec![0u8; 32]
    }

    #[tokio::test]
    async fn test_aesgcm_encrypt_decrypt_roundtrip() {
        let crypto = AesGcmCrypto::new(test_key()).unwrap();
        let plaintext = b"hello rinfra crypto";
        let encrypted = crypto.encrypt(plaintext).await.unwrap();
        assert_ne!(encrypted, plaintext);
        let decrypted = crypto.decrypt(&encrypted).await.unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[tokio::test]
    async fn test_aesgcm_wrong_key_fails() {
        let crypto1 = AesGcmCrypto::new(vec![0u8; 32]).unwrap();
        let crypto2 = AesGcmCrypto::new(vec![1u8; 32]).unwrap();
        let encrypted = crypto1.encrypt(b"secret").await.unwrap();
        let result = crypto2.decrypt(&encrypted).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::CryptoDecryptFailed);
    }

    #[tokio::test]
    async fn test_aesgcm_invalid_key_length() {
        let result = AesGcmCrypto::new(vec![0u8; 16]);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_aesgcm_ciphertext_too_short() {
        let crypto = AesGcmCrypto::new(test_key()).unwrap();
        let result = crypto.decrypt(b"short").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::CryptoDecryptFailed);
    }

    #[tokio::test]
    async fn test_aesgcm_different_plaintexts_different_ciphertexts() {
        let crypto = AesGcmCrypto::new(test_key()).unwrap();
        let enc1 = crypto.encrypt(b"aaa").await.unwrap();
        let enc2 = crypto.encrypt(b"bbb").await.unwrap();
        assert_ne!(enc1, enc2);
    }
}
