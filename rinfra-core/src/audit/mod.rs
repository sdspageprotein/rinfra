use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Outcome of an audited operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditOutcome {
    Success,
    Failure,
    Denied,
}

/// A structured audit event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: String,
    pub timestamp_ms: u64,
    /// Who performed the action (user-id, admin name, "system").
    pub actor: String,
    /// Dot-separated action name (e.g. `"user.create"`, `"config.update"`).
    pub action: String,
    /// Target resource type (e.g. `"user"`, `"config"`).
    pub resource: String,
    /// Optional resource identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    pub outcome: AuditOutcome,
    /// Client IP or source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    /// Arbitrary extra data.
    #[serde(default)]
    pub details: serde_json::Value,
}

impl AuditEvent {
    pub fn new(
        actor: impl Into<String>,
        action: impl Into<String>,
        resource: impl Into<String>,
        outcome: AuditOutcome,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp_ms: now,
            actor: actor.into(),
            action: action.into(),
            resource: resource.into(),
            resource_id: None,
            outcome,
            ip: None,
            details: serde_json::Value::Null,
        }
    }

    pub fn resource_id(mut self, id: impl Into<String>) -> Self {
        self.resource_id = Some(id.into());
        self
    }

    pub fn ip(mut self, ip: impl Into<String>) -> Self {
        self.ip = Some(ip.into());
        self
    }

    pub fn details(mut self, details: serde_json::Value) -> Self {
        self.details = details;
        self
    }
}

/// Filter for querying audit events.
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    pub actor: Option<String>,
    pub action: Option<String>,
    pub resource: Option<String>,
    pub from_timestamp_ms: Option<u64>,
    pub to_timestamp_ms: Option<u64>,
    pub limit: usize,
}

impl AuditFilter {
    pub fn new() -> Self {
        Self {
            limit: 100,
            ..Default::default()
        }
    }

    pub fn matches(&self, event: &AuditEvent) -> bool {
        if let Some(ref a) = self.actor {
            if &event.actor != a {
                return false;
            }
        }
        if let Some(ref a) = self.action {
            if &event.action != a {
                return false;
            }
        }
        if let Some(ref r) = self.resource {
            if &event.resource != r {
                return false;
            }
        }
        if let Some(from) = self.from_timestamp_ms {
            if event.timestamp_ms < from {
                return false;
            }
        }
        if let Some(to) = self.to_timestamp_ms {
            if event.timestamp_ms > to {
                return false;
            }
        }
        true
    }
}

/// Pluggable audit logging abstraction.
#[async_trait]
pub trait AuditLogger: Send + Sync + 'static {
    fn logger_name(&self) -> &str;

    /// Record an audit event.
    async fn log(&self, event: AuditEvent) -> Result<(), AppError>;

    /// Query recent audit events (optional — defaults to empty).
    async fn query(&self, filter: &AuditFilter) -> Result<Vec<AuditEvent>, AppError> {
        let _ = filter;
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_event_builder() {
        let event = AuditEvent::new("admin", "user.create", "user", AuditOutcome::Success)
            .resource_id("u-123")
            .ip("192.168.1.1")
            .details(serde_json::json!({"email": "test@test.com"}));

        assert_eq!(event.actor, "admin");
        assert_eq!(event.action, "user.create");
        assert_eq!(event.resource_id.as_deref(), Some("u-123"));
        assert_eq!(event.ip.as_deref(), Some("192.168.1.1"));
        assert!(event.timestamp_ms > 0);
        assert!(!event.id.is_empty());
    }

    #[test]
    fn test_audit_event_serde() {
        let event = AuditEvent::new("system", "config.reload", "config", AuditOutcome::Failure);
        let json = serde_json::to_string(&event).unwrap();
        let decoded: AuditEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.actor, "system");
        assert_eq!(decoded.outcome, AuditOutcome::Failure);
    }

    #[test]
    fn test_audit_filter_matches() {
        let event = AuditEvent::new("admin", "user.delete", "user", AuditOutcome::Success);

        let mut filter = AuditFilter::new();
        assert!(filter.matches(&event));

        filter.actor = Some("admin".into());
        assert!(filter.matches(&event));

        filter.actor = Some("other".into());
        assert!(!filter.matches(&event));

        filter.actor = None;
        filter.action = Some("user.delete".into());
        assert!(filter.matches(&event));

        filter.action = Some("user.create".into());
        assert!(!filter.matches(&event));
    }

    #[test]
    fn test_audit_filter_timestamp() {
        let event = AuditEvent::new("a", "b", "c", AuditOutcome::Success);
        let ts = event.timestamp_ms;

        let mut filter = AuditFilter::new();
        filter.from_timestamp_ms = Some(ts - 1000);
        filter.to_timestamp_ms = Some(ts + 1000);
        assert!(filter.matches(&event));

        filter.from_timestamp_ms = Some(ts + 1);
        assert!(!filter.matches(&event));
    }

    #[test]
    fn test_audit_outcome_serde() {
        let json = serde_json::to_string(&AuditOutcome::Denied).unwrap();
        assert_eq!(json, "\"Denied\"");
        let decoded: AuditOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, AuditOutcome::Denied);
    }
}
