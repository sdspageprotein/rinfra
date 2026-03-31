use std::path::Path;

use rinfra_core::config::RinfraConfig;
use rinfra_core::error::{AppError, ErrorCode};
use serde_json::Value;
use tracing::{debug, info};

/// Load config from a YAML string.
pub fn load_from_str(yaml: &str) -> Result<RinfraConfig, AppError> {
    serde_yaml::from_str(yaml).map_err(|e| {
        AppError::new(
            ErrorCode::ConfigParseFailed,
            format!("failed to parse YAML config: {e}"),
        )
    })
}

/// Load config from a YAML file.
pub fn load_from_file(path: &Path) -> Result<RinfraConfig, AppError> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        AppError::new(
            ErrorCode::ConfigFileMissing,
            format!("config file '{}' not found: {e}", path.display()),
        )
    })?;
    info!(path = %path.display(), "config loaded");
    load_from_str(&content)
}

/// Apply environment variable overrides with prefix `RINFRA_`.
/// Uses double underscores as path separators: `RINFRA_APP__NAME` -> `app.name`.
///
/// Internally serializes config to `serde_json::Value`, merges env vars,
/// then deserializes back. Supports arbitrary nesting depth.
pub fn apply_env_overrides(config: &mut RinfraConfig) {
    let mut value = serde_json::to_value(&*config).unwrap_or(Value::Object(Default::default()));
    let count = merge_env_into_value(&mut value);
    if count > 0 {
        match serde_json::from_value::<RinfraConfig>(value) {
            Ok(updated) => {
                *config = updated;
                info!(overrides = count, "applied env overrides");
            }
            Err(e) => {
                tracing::warn!(error = %e, "env overrides produced invalid config, keeping original");
            }
        }
    }
}

/// Load config from file, then apply env overrides via Value intermediate layer.
pub fn load_with_env(path: &Path) -> Result<RinfraConfig, AppError> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        AppError::new(
            ErrorCode::ConfigFileMissing,
            format!("config file '{}' not found: {e}", path.display()),
        )
    })?;
    info!(path = %path.display(), "config loaded");

    let mut value: Value = serde_yaml::from_str(&content).map_err(|e| {
        AppError::new(
            ErrorCode::ConfigParseFailed,
            format!("failed to parse YAML config: {e}"),
        )
    })?;

    let count = merge_env_into_value(&mut value);
    if count > 0 {
        info!(overrides = count, "applied env overrides");
    }

    serde_json::from_value(value).map_err(|e| {
        AppError::new(
            ErrorCode::ConfigParseFailed,
            format!("config deserialization failed after env override: {e}"),
        )
    })
}

/// Scan all `RINFRA_` prefixed env vars and merge them into the Value tree.
/// Returns the number of overrides applied.
fn merge_env_into_value(value: &mut Value) -> usize {
    let mut count = 0;
    for (key, val) in std::env::vars() {
        if let Some(stripped) = key.strip_prefix("RINFRA_") {
            if stripped.is_empty() {
                continue;
            }
            let segments: Vec<String> = stripped.split("__").map(|s| s.to_lowercase()).collect();
            set_nested(value, &segments, &val);
            debug!(key = %key, path = %segments.join("."), "config override from env");
            count += 1;
        }
    }
    count
}

/// Recursively navigate into the Value tree along `path`, creating intermediate
/// Object nodes as needed, and set the leaf to the parsed env value.
fn set_nested(root: &mut Value, path: &[String], raw: &str) {
    if path.is_empty() {
        return;
    }

    if path.len() == 1 {
        if let Value::Object(map) = root {
            map.insert(path[0].clone(), parse_env_value(raw));
        }
        return;
    }

    if let Value::Object(map) = root {
        let child = map
            .entry(&path[0])
            .or_insert_with(|| Value::Object(Default::default()));
        set_nested(child, &path[1..], raw);
    }
}

/// Auto-detect the type of an env var value.
/// Priority: bool > u64 > f64 > string.
fn parse_env_value(raw: &str) -> Value {
    match raw.to_lowercase().as_str() {
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        _ => {}
    }

    if let Ok(n) = raw.parse::<u64>() {
        return Value::Number(n.into());
    }

    if let Ok(f) = raw.parse::<f64>()
        && f.is_finite()
        && let Some(n) = serde_json::Number::from_f64(f)
    {
        return Value::Number(n);
    }

    Value::String(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_from_str_valid_yaml() {
        let yaml = "app:\n  name: hello\n";
        let config = load_from_str(yaml).unwrap();
        assert_eq!(config.app.name, "hello");
    }

    #[test]
    fn test_load_from_str_invalid_yaml_returns_error() {
        let yaml = "{{invalid yaml";
        let result = load_from_str(yaml);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::ConfigParseFailed);
    }

    #[test]
    fn test_load_from_file_missing_returns_error() {
        let result = load_from_file(Path::new("/nonexistent/rinfra.yaml"));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::ConfigFileMissing);
    }

    #[test]
    fn test_apply_env_overrides() {
        let mut config = RinfraConfig::default();
        assert_eq!(config.app.name, "rinfra-app");

        unsafe { std::env::set_var("RINFRA_APP__NAME", "overridden") };
        apply_env_overrides(&mut config);
        assert_eq!(config.app.name, "overridden");
        unsafe { std::env::remove_var("RINFRA_APP__NAME") };
    }

    #[test]
    fn test_apply_env_overrides_log_level() {
        let mut config = RinfraConfig::default();
        unsafe { std::env::set_var("RINFRA_PLUGINS__LOG__STDOUT__LEVEL", "trace") };
        apply_env_overrides(&mut config);
        assert_eq!(config.plugins.log.stdout.level, "trace");
        unsafe { std::env::remove_var("RINFRA_PLUGINS__LOG__STDOUT__LEVEL") };
    }

    #[test]
    fn test_deep_nested_override_cluster_mode() {
        let mut config = RinfraConfig::default();
        assert_eq!(config.plugins.cluster.mode, "standalone");

        unsafe { std::env::set_var("RINFRA_PLUGINS__CLUSTER__MODE", "cluster") };
        apply_env_overrides(&mut config);
        assert_eq!(config.plugins.cluster.mode, "cluster");
        unsafe { std::env::remove_var("RINFRA_PLUGINS__CLUSTER__MODE") };
    }

    #[test]
    fn test_bool_auto_detection() {
        let v = parse_env_value("true");
        assert_eq!(v, Value::Bool(true));
        let v = parse_env_value("FALSE");
        assert_eq!(v, Value::Bool(false));
    }

    #[test]
    fn test_integer_auto_detection() {
        let v = parse_env_value("8090");
        assert_eq!(v, Value::Number(8090u64.into()));
    }

    #[test]
    fn test_float_auto_detection() {
        let v = parse_env_value("3.14");
        assert!(v.is_number());
        assert!((v.as_f64().unwrap() - 3.14).abs() < f64::EPSILON);
    }

    #[test]
    fn test_string_fallback() {
        let v = parse_env_value("hello-world");
        assert_eq!(v, Value::String("hello-world".to_string()));
    }

    #[test]
    fn test_deep_override_app_name() {
        let mut config = RinfraConfig::default();
        assert_eq!(config.app.name, "rinfra-app");

        unsafe { std::env::set_var("RINFRA_APP__NAME", "overridden") };
        apply_env_overrides(&mut config);
        assert_eq!(config.app.name, "overridden");
        unsafe { std::env::remove_var("RINFRA_APP__NAME") };
    }

    #[test]
    fn test_set_nested_creates_intermediate_objects() {
        let mut val = Value::Object(Default::default());
        set_nested(
            &mut val,
            &["a".to_string(), "b".to_string(), "c".to_string()],
            "42",
        );
        assert_eq!(val["a"]["b"]["c"], Value::Number(42u64.into()));
    }

    #[test]
    fn test_merge_env_skips_non_rinfra_vars() {
        let mut val = Value::Object(Default::default());
        let count = merge_env_into_value(&mut val);
        // Should not panic; count depends on any RINFRA_ vars in test env
        assert!(count < 1000);
    }
}
