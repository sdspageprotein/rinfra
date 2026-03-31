use super::Entity;

/// Entities with automatic audit timestamps.
pub trait Auditable: Entity {
    fn created_at(&self) -> i64;
    fn updated_at(&self) -> i64;
    fn set_created_at(&mut self, ts: i64);
    fn set_updated_at(&mut self, ts: i64);
}

/// Entities supporting soft delete (logical deletion).
pub trait SoftDeletable: Entity {
    fn deleted_at(&self) -> Option<i64>;
    fn set_deleted_at(&mut self, ts: Option<i64>);

    fn is_deleted(&self) -> bool {
        self.deleted_at().is_some()
    }
}

/// Returns the current Unix timestamp in seconds.
pub fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AuditUser {
        id: i64,
        created_at: i64,
        updated_at: i64,
        deleted_at: Option<i64>,
    }

    impl Entity for AuditUser {
        type Id = i64;
        fn id(&self) -> &i64 {
            &self.id
        }
    }

    impl Auditable for AuditUser {
        fn created_at(&self) -> i64 {
            self.created_at
        }
        fn updated_at(&self) -> i64 {
            self.updated_at
        }
        fn set_created_at(&mut self, ts: i64) {
            self.created_at = ts;
        }
        fn set_updated_at(&mut self, ts: i64) {
            self.updated_at = ts;
        }
    }

    impl SoftDeletable for AuditUser {
        fn deleted_at(&self) -> Option<i64> {
            self.deleted_at
        }
        fn set_deleted_at(&mut self, ts: Option<i64>) {
            self.deleted_at = ts;
        }
    }

    #[test]
    fn test_auditable() {
        let mut u = AuditUser {
            id: 1,
            created_at: 0,
            updated_at: 0,
            deleted_at: None,
        };
        let now = now_unix_secs();
        u.set_created_at(now);
        u.set_updated_at(now);
        assert!(u.created_at() > 0);
        assert!(u.updated_at() > 0);
    }

    #[test]
    fn test_soft_deletable() {
        let mut u = AuditUser {
            id: 1,
            created_at: 0,
            updated_at: 0,
            deleted_at: None,
        };
        assert!(!u.is_deleted());
        u.set_deleted_at(Some(now_unix_secs()));
        assert!(u.is_deleted());
    }

    #[test]
    fn test_now_unix_secs() {
        let now = now_unix_secs();
        assert!(now > 1_700_000_000);
    }
}
