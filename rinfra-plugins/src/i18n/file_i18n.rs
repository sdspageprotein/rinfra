use std::collections::HashMap;
use std::path::Path;

use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::i18n::I18n;
use tracing::{debug, info, warn};

type Catalog = HashMap<String, HashMap<String, String>>;

/// File-based i18n implementation.
///
/// Loads `{locale}.yaml` files from a directory. Nested YAML keys are
/// flattened with `.` separators (e.g. `errors.not_found`). Lookups
/// fall back to the default locale, then return the raw key.
pub struct FileI18n {
    catalog: Catalog,
    default_locale: String,
}

impl FileI18n {
    /// Load all `*.yaml` / `*.yml` files from `dir`.
    /// The filename (without extension) becomes the locale name.
    pub fn load(dir: impl AsRef<Path>, default_locale: &str) -> Result<Self, AppError> {
        let dir = dir.as_ref();
        let mut catalog: Catalog = HashMap::new();

        if !dir.exists() {
            warn!(dir = %dir.display(), "i18n directory not found, no translations loaded");
            return Ok(Self {
                catalog,
                default_locale: default_locale.to_string(),
            });
        }

        let entries = std::fs::read_dir(dir).map_err(|e| {
            AppError::new(
                ErrorCode::I18nLoadFailed,
                format!("failed to read i18n directory: {e}"),
            )
        })?;

        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "yaml" && ext != "yml" {
                continue;
            }
            let locale = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if locale.is_empty() {
                continue;
            }

            let content = std::fs::read_to_string(&path).map_err(|e| {
                AppError::new(
                    ErrorCode::I18nLoadFailed,
                    format!("failed to read {}: {e}", path.display()),
                )
            })?;

            let value: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|e| {
                AppError::new(
                    ErrorCode::I18nLoadFailed,
                    format!("failed to parse {}: {e}", path.display()),
                )
            })?;

            let mut messages = HashMap::new();
            flatten_yaml(&value, "", &mut messages);

            let count = messages.len();
            catalog.insert(locale.clone(), messages);
            debug!(locale = %locale, keys = count, "loaded i18n catalog");
        }

        info!(
            locales = catalog.len(),
            default = default_locale,
            "i18n initialized"
        );

        Ok(Self {
            catalog,
            default_locale: default_locale.to_string(),
        })
    }
}

impl I18n for FileI18n {
    fn t(&self, key: &str, locale: &str) -> String {
        if let Some(messages) = self.catalog.get(locale) {
            if let Some(val) = messages.get(key) {
                return val.clone();
            }
        }
        if locale != self.default_locale {
            if let Some(messages) = self.catalog.get(&self.default_locale) {
                if let Some(val) = messages.get(key) {
                    return val.clone();
                }
            }
        }
        key.to_string()
    }

    fn available_locales(&self) -> Vec<String> {
        let mut locales: Vec<String> = self.catalog.keys().cloned().collect();
        locales.sort();
        locales
    }

    fn default_locale(&self) -> &str {
        &self.default_locale
    }
}

fn flatten_yaml(value: &serde_yaml::Value, prefix: &str, out: &mut HashMap<String, String>) {
    match value {
        serde_yaml::Value::Mapping(map) => {
            for (k, v) in map {
                let key_str = match k {
                    serde_yaml::Value::String(s) => s.clone(),
                    other => format!("{other:?}"),
                };
                let full_key = if prefix.is_empty() {
                    key_str
                } else {
                    format!("{prefix}.{key_str}")
                };
                flatten_yaml(v, &full_key, out);
            }
        }
        serde_yaml::Value::String(s) => {
            out.insert(prefix.to_string(), s.clone());
        }
        serde_yaml::Value::Number(n) => {
            out.insert(prefix.to_string(), n.to_string());
        }
        serde_yaml::Value::Bool(b) => {
            out.insert(prefix.to_string(), b.to_string());
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_locale(dir: &Path, name: &str, content: &str) {
        std::fs::write(dir.join(format!("{name}.yaml")), content).unwrap();
    }

    #[test]
    fn test_load_and_translate() {
        let dir = tempfile::tempdir().unwrap();
        write_locale(
            dir.path(),
            "en",
            r#"
greeting: "Hello!"
errors:
  not_found: "Not found"
  unauthorized: "Forbidden"
"#,
        );
        write_locale(
            dir.path(),
            "zh-CN",
            r#"
greeting: "你好！"
errors:
  not_found: "未找到"
  unauthorized: "无权限"
"#,
        );

        let i18n = FileI18n::load(dir.path(), "en").unwrap();

        assert_eq!(i18n.t("greeting", "en"), "Hello!");
        assert_eq!(i18n.t("greeting", "zh-CN"), "你好！");
        assert_eq!(i18n.t("errors.not_found", "en"), "Not found");
        assert_eq!(i18n.t("errors.unauthorized", "zh-CN"), "无权限");
    }

    #[test]
    fn test_fallback_to_default_locale() {
        let dir = tempfile::tempdir().unwrap();
        write_locale(dir.path(), "en", "hello: Hello\nonly_en: English only");
        write_locale(dir.path(), "fr", "hello: Bonjour");

        let i18n = FileI18n::load(dir.path(), "en").unwrap();

        assert_eq!(i18n.t("hello", "fr"), "Bonjour");
        // "only_en" not in fr, falls back to en
        assert_eq!(i18n.t("only_en", "fr"), "English only");
    }

    #[test]
    fn test_missing_key_returns_key() {
        let dir = tempfile::tempdir().unwrap();
        write_locale(dir.path(), "en", "a: value_a");

        let i18n = FileI18n::load(dir.path(), "en").unwrap();
        assert_eq!(i18n.t("nonexistent.key", "en"), "nonexistent.key");
    }

    #[test]
    fn test_missing_dir_returns_empty() {
        let i18n = FileI18n::load("/tmp/nonexistent_i18n_dir_xyz", "en").unwrap();
        assert!(i18n.available_locales().is_empty());
        assert_eq!(i18n.t("hello", "en"), "hello");
    }

    #[test]
    fn test_available_locales() {
        let dir = tempfile::tempdir().unwrap();
        write_locale(dir.path(), "en", "a: 1");
        write_locale(dir.path(), "fr", "a: 2");
        write_locale(dir.path(), "zh-CN", "a: 3");

        let i18n = FileI18n::load(dir.path(), "en").unwrap();
        let locales = i18n.available_locales();
        assert_eq!(locales.len(), 3);
        assert!(locales.contains(&"en".to_string()));
        assert!(locales.contains(&"fr".to_string()));
        assert!(locales.contains(&"zh-CN".to_string()));
    }

    #[test]
    fn test_default_locale() {
        let dir = tempfile::tempdir().unwrap();
        write_locale(dir.path(), "en", "a: 1");

        let i18n = FileI18n::load(dir.path(), "en").unwrap();
        assert_eq!(i18n.default_locale(), "en");
    }

    #[test]
    fn test_t_args() {
        let dir = tempfile::tempdir().unwrap();
        write_locale(dir.path(), "en", "welcome: \"Welcome, {user}! You have {count} messages.\"");

        let i18n = FileI18n::load(dir.path(), "en").unwrap();
        let mut args = HashMap::new();
        args.insert("user".into(), "Alice".into());
        args.insert("count".into(), "5".into());
        assert_eq!(
            i18n.t_args("welcome", "en", &args),
            "Welcome, Alice! You have 5 messages."
        );
    }

    #[test]
    fn test_nested_yaml_flattening() {
        let dir = tempfile::tempdir().unwrap();
        write_locale(
            dir.path(),
            "en",
            r#"
level1:
  level2:
    level3: "deep value"
top: "top value"
"#,
        );

        let i18n = FileI18n::load(dir.path(), "en").unwrap();
        assert_eq!(i18n.t("top", "en"), "top value");
        assert_eq!(i18n.t("level1.level2.level3", "en"), "deep value");
    }
}
