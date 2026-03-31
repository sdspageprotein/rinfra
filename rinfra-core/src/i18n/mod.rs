use std::collections::HashMap;

/// Internationalization (i18n) abstraction.
///
/// Implementations load translation catalogs and resolve message keys
/// with optional placeholder substitution (`{name}` syntax).
pub trait I18n: Send + Sync + 'static {
    /// Resolve a message key for the given locale.
    /// Falls back to `default_locale()`, then returns the key itself.
    fn t(&self, key: &str, locale: &str) -> String;

    /// Resolve a message key with named arguments.
    /// Placeholders like `{name}` are replaced by matching entries in `args`.
    fn t_args(&self, key: &str, locale: &str, args: &HashMap<String, String>) -> String {
        let mut msg = self.t(key, locale);
        for (k, v) in args {
            msg = msg.replace(&format!("{{{k}}}"), v);
        }
        msg
    }

    /// List all loaded locales.
    fn available_locales(&self) -> Vec<String>;

    /// The fallback locale.
    fn default_locale(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyI18n;

    impl I18n for DummyI18n {
        fn t(&self, key: &str, _locale: &str) -> String {
            if key == "hello" {
                "Hello, {name}!".to_string()
            } else {
                key.to_string()
            }
        }
        fn available_locales(&self) -> Vec<String> {
            vec!["en".into()]
        }
        fn default_locale(&self) -> &str {
            "en"
        }
    }

    #[test]
    fn test_t_args_substitution() {
        let i18n = DummyI18n;
        let mut args = HashMap::new();
        args.insert("name".into(), "World".into());
        assert_eq!(i18n.t_args("hello", "en", &args), "Hello, World!");
    }

    #[test]
    fn test_t_args_no_placeholder() {
        let i18n = DummyI18n;
        let args = HashMap::new();
        assert_eq!(i18n.t_args("missing", "en", &args), "missing");
    }
}
