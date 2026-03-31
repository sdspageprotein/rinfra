use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::SystemTime;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminRole {
    Root,
    Admin,
}

impl std::fmt::Display for AdminRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AdminRole::Root => write!(f, "root"),
            AdminRole::Admin => write!(f, "admin"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminTokenRecord {
    pub id: String,
    pub role: AdminRole,
    pub token_hash: String,
    pub label: String,
    pub created_at: u64,
    pub last_used_at: Option<u64>,
    pub expires_at: Option<u64>,
}

/// Public view of a token record (no hash exposed).
#[derive(Debug, Clone, Serialize)]
pub struct AdminTokenInfo {
    pub id: String,
    pub role: AdminRole,
    pub label: String,
    pub created_at: u64,
    pub last_used_at: Option<u64>,
    pub expires_at: Option<u64>,
}

impl From<&AdminTokenRecord> for AdminTokenInfo {
    fn from(r: &AdminTokenRecord) -> Self {
        Self {
            id: r.id.clone(),
            role: r.role.clone(),
            label: r.label.clone(),
            created_at: r.created_at,
            last_used_at: r.last_used_at,
            expires_at: r.expires_at,
        }
    }
}

pub struct AdminTokenStore {
    path: PathBuf,
    tokens: RwLock<Vec<AdminTokenRecord>>,
}

impl AdminTokenStore {
    /// Load or create an empty token store at the given path.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        let tokens = if path.exists() {
            let data = std::fs::read_to_string(&path)
                .map_err(|e| format!("failed to read token file {}: {e}", path.display()))?;
            serde_json::from_str::<Vec<AdminTokenRecord>>(&data)
                .map_err(|e| format!("failed to parse token file: {e}"))?
        } else {
            Vec::new()
        };
        debug!(path = %path.display(), count = tokens.len(), "admin token store loaded");
        Ok(Self {
            path,
            tokens: RwLock::new(tokens),
        })
    }

    pub fn has_root(&self) -> bool {
        self.tokens
            .read()
            .expect("token store lock poisoned")
            .iter()
            .any(|t| t.role == AdminRole::Root)
    }

    /// Generate a new random token string with role prefix.
    pub fn generate_token(role: &AdminRole) -> String {
        let mut bytes = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
        let random_hex = hex::encode(bytes);
        match role {
            AdminRole::Root => format!("rinfra_root_{random_hex}"),
            AdminRole::Admin => format!("rinfra_admin_{random_hex}"),
        }
    }

    /// Create a token record, store the hash, and return the plaintext token.
    pub fn create_token(
        &self,
        role: AdminRole,
        label: String,
        expires_at: Option<u64>,
    ) -> Result<(String, AdminTokenInfo), String> {
        let plaintext = Self::generate_token(&role);
        let info = self.create_token_with_value(role, label, &plaintext, expires_at)?;
        Ok((plaintext, info))
    }

    /// Create a token record from a known plaintext value (e.g. env variable).
    pub fn create_token_with_value(
        &self,
        role: AdminRole,
        label: String,
        plaintext: &str,
        expires_at: Option<u64>,
    ) -> Result<AdminTokenInfo, String> {
        let hash = hash_token(plaintext)?;
        let now = now_millis();

        let record = AdminTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            role,
            token_hash: hash,
            label,
            created_at: now,
            last_used_at: None,
            expires_at,
        };

        let info = AdminTokenInfo::from(&record);
        self.tokens.write().expect("token store lock poisoned").push(record);
        self.persist()?;

        Ok(info)
    }

    /// Verify a plaintext token against stored hashes.
    /// Returns matching record info on success, updating last_used_at.
    pub fn verify(&self, plaintext: &str) -> Option<AdminTokenInfo> {
        let now = now_millis();
        let mut tokens = self.tokens.write().expect("token store lock poisoned");

        for record in tokens.iter_mut() {
            if let Some(exp) = record.expires_at {
                if now > exp {
                    continue;
                }
            }
            if verify_token(plaintext, &record.token_hash) {
                record.last_used_at = Some(now);
                return Some(AdminTokenInfo::from(&*record));
            }
        }
        None
    }

    /// List all tokens (public view, no hashes).
    pub fn list(&self) -> Vec<AdminTokenInfo> {
        self.tokens
            .read()
            .expect("token store lock poisoned")
            .iter()
            .map(AdminTokenInfo::from)
            .collect()
    }

    /// Delete a token by id. Returns true if found and removed.
    pub fn delete(&self, id: &str) -> Result<bool, String> {
        let mut tokens = self.tokens.write().expect("token store lock poisoned");
        let len_before = tokens.len();
        tokens.retain(|t| t.id != id);
        let removed = tokens.len() < len_before;
        drop(tokens);
        if removed {
            self.persist()?;
        }
        Ok(removed)
    }

    /// Delete the current root token (for rotation).
    pub fn delete_root(&self) -> Result<(), String> {
        let mut tokens = self.tokens.write().expect("token store lock poisoned");
        tokens.retain(|t| t.role != AdminRole::Root);
        drop(tokens);
        self.persist()
    }

    /// Persist tokens to file atomically.
    fn persist(&self) -> Result<(), String> {
        let tokens = self.tokens.read().expect("token store lock poisoned");
        let data = serde_json::to_string_pretty(&*tokens)
            .map_err(|e| format!("serialize failed: {e}"))?;
        drop(tokens);

        if let Some(parent) = self.path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("mkdir failed: {e}"))?;
            }
        }

        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, &data)
            .map_err(|e| format!("write tmp failed: {e}"))?;
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| format!("rename failed: {e}"))?;

        Ok(())
    }
}

fn hash_token(plaintext: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(plaintext.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| format!("hash failed: {e}"))
}

fn verify_token(plaintext: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(plaintext.as_bytes(), &parsed)
            .is_ok(),
        Err(e) => {
            warn!(error = %e, "invalid stored hash");
            false
        }
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rinfra_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("tokens.json")
    }

    #[test]
    fn test_generate_token_format() {
        let root = AdminTokenStore::generate_token(&AdminRole::Root);
        assert!(root.starts_with("rinfra_root_"));
        assert_eq!(root.len(), "rinfra_root_".len() + 64);

        let admin = AdminTokenStore::generate_token(&AdminRole::Admin);
        assert!(admin.starts_with("rinfra_admin_"));
    }

    #[test]
    fn test_hash_and_verify() {
        let token = "rinfra_root_abc123";
        let hash = hash_token(token).unwrap();
        assert!(verify_token(token, &hash));
        assert!(!verify_token("wrong_token", &hash));
    }

    #[test]
    fn test_create_and_verify_token() {
        let path = temp_path();
        let store = AdminTokenStore::load(&path).unwrap();

        let (plaintext, info) = store
            .create_token(AdminRole::Root, "test-root".into(), None)
            .unwrap();

        assert!(plaintext.starts_with("rinfra_root_"));
        assert_eq!(info.role, AdminRole::Root);
        assert_eq!(info.label, "test-root");

        let verified = store.verify(&plaintext);
        assert!(verified.is_some());
        assert_eq!(verified.unwrap().id, info.id);

        assert!(store.verify("wrong_token").is_none());

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn test_has_root() {
        let path = temp_path();
        let store = AdminTokenStore::load(&path).unwrap();
        assert!(!store.has_root());

        store.create_token(AdminRole::Root, "root".into(), None).unwrap();
        assert!(store.has_root());

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn test_list_and_delete() {
        let path = temp_path();
        let store = AdminTokenStore::load(&path).unwrap();

        let (_, info1) = store.create_token(AdminRole::Admin, "a1".into(), None).unwrap();
        let (_, _info2) = store.create_token(AdminRole::Admin, "a2".into(), None).unwrap();

        assert_eq!(store.list().len(), 2);

        store.delete(&info1.id).unwrap();
        assert_eq!(store.list().len(), 1);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn test_expired_token_rejected() {
        let path = temp_path();
        let store = AdminTokenStore::load(&path).unwrap();

        let expired = now_millis() - 1000;
        let (plaintext, _) = store
            .create_token(AdminRole::Admin, "expired".into(), Some(expired))
            .unwrap();

        assert!(store.verify(&plaintext).is_none());

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn test_persistence_across_loads() {
        let path = temp_path();
        let token_plaintext;

        {
            let store = AdminTokenStore::load(&path).unwrap();
            let (pt, _) = store.create_token(AdminRole::Root, "persist-test".into(), None).unwrap();
            token_plaintext = pt;
        }

        let store2 = AdminTokenStore::load(&path).unwrap();
        assert!(store2.verify(&token_plaintext).is_some());
        assert!(store2.has_root());

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn test_delete_root_for_rotation() {
        let path = temp_path();
        let store = AdminTokenStore::load(&path).unwrap();

        store.create_token(AdminRole::Root, "root".into(), None).unwrap();
        store.create_token(AdminRole::Admin, "admin".into(), None).unwrap();

        assert!(store.has_root());
        store.delete_root().unwrap();
        assert!(!store.has_root());
        assert_eq!(store.list().len(), 1);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn test_role_display() {
        assert_eq!(AdminRole::Root.to_string(), "root");
        assert_eq!(AdminRole::Admin.to_string(), "admin");
    }
}
