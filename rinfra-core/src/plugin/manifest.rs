use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
}

impl PluginManifest {
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            description: description.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Result of a single health check probe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub status: HealthStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl HealthCheckResult {
    pub fn healthy() -> Self {
        Self { status: HealthStatus::Healthy, error: None }
    }

    pub fn unhealthy(reason: impl Into<String>) -> Self {
        Self { status: HealthStatus::Unhealthy, error: Some(reason.into()) }
    }

    pub fn degraded(reason: impl Into<String>) -> Self {
        Self { status: HealthStatus::Degraded, error: Some(reason.into()) }
    }

    pub fn is_healthy(&self) -> bool {
        self.status == HealthStatus::Healthy
    }
}
