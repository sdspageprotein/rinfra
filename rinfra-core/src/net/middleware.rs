use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{AppError, ErrorCode};

/// HTTP middleware that can be applied to a router via a registry.
///
/// Implementations wrap framework-specific middleware (e.g., axum Tower layers)
/// behind a uniform interface, enabling configuration-driven assembly
/// similar to the TCP `ByteTransform` pipeline.
pub trait HttpMiddleware: Send + Sync + 'static {
    /// Unique name matching the config key (e.g. "cors", "timeout", "auth").
    fn name(&self) -> &str;

    /// Execution order. Lower values are applied first (innermost layer).
    ///
    /// Suggested ranges:
    /// - 10-19: metrics / instrumentation (closest to handler)
    /// - 20-29: security (auth, rate-limit)
    /// - 30-39: observability (tracing, otel propagation)
    /// - 40-49: request enrichment (request-id)
    /// - 50-59: transport (timeout)
    /// - 60-69: protocol (cors)
    fn order(&self) -> i32;

    /// Apply this middleware to a router (type-erased).
    ///
    /// Implementations downcast to the framework router type (e.g. `axum::Router`),
    /// apply their layer, and return the wrapped router.
    fn apply(&self, router: Box<dyn Any>) -> Result<Box<dyn Any>, AppError>;
}

/// Registry of named `HttpMiddleware` implementations.
pub struct HttpMiddlewareRegistry {
    middlewares: HashMap<String, Arc<dyn HttpMiddleware>>,
}

impl HttpMiddlewareRegistry {
    pub fn new() -> Self {
        Self {
            middlewares: HashMap::new(),
        }
    }

    pub fn register(&mut self, mw: Arc<dyn HttpMiddleware>) -> Result<(), AppError> {
        let name = mw.name().to_string();
        if self.middlewares.contains_key(&name) {
            return Err(AppError::new(
                ErrorCode::PluginAlreadyRegistered,
                format!("http middleware '{}' already registered", name),
            ));
        }
        self.middlewares.insert(name, mw);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn HttpMiddleware>> {
        self.middlewares.get(name)
    }

    pub fn names(&self) -> Vec<&str> {
        self.middlewares.keys().map(|s| s.as_str()).collect()
    }

    /// Returns all registered middleware sorted by order (ascending).
    /// Lower order = applied first = innermost layer.
    pub fn sorted(&self) -> Vec<&Arc<dyn HttpMiddleware>> {
        let mut mws: Vec<_> = self.middlewares.values().collect();
        mws.sort_by_key(|m| m.order());
        mws
    }

    pub fn is_empty(&self) -> bool {
        self.middlewares.is_empty()
    }

    /// Apply all registered middleware to a router (type-erased), sorted by order.
    pub fn apply_all(&self, mut router: Box<dyn Any>) -> Result<Box<dyn Any>, AppError> {
        for mw in self.sorted() {
            router = mw.apply(router)?;
        }
        Ok(router)
    }
}

impl Default for HttpMiddlewareRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoopMiddleware {
        n: String,
        o: i32,
    }

    impl HttpMiddleware for NoopMiddleware {
        fn name(&self) -> &str {
            &self.n
        }
        fn order(&self) -> i32 {
            self.o
        }
        fn apply(&self, router: Box<dyn Any>) -> Result<Box<dyn Any>, AppError> {
            Ok(router)
        }
    }

    #[test]
    fn test_register_and_get() {
        let mut reg = HttpMiddlewareRegistry::new();
        reg.register(Arc::new(NoopMiddleware {
            n: "test".into(),
            o: 10,
        }))
        .unwrap();
        assert!(reg.get("test").is_some());
        assert!(reg.get("missing").is_none());
    }

    #[test]
    fn test_register_duplicate_errors() {
        let mut reg = HttpMiddlewareRegistry::new();
        reg.register(Arc::new(NoopMiddleware {
            n: "dup".into(),
            o: 10,
        }))
        .unwrap();
        let result = reg.register(Arc::new(NoopMiddleware {
            n: "dup".into(),
            o: 20,
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_sorted_by_order() {
        let mut reg = HttpMiddlewareRegistry::new();
        reg.register(Arc::new(NoopMiddleware {
            n: "c".into(),
            o: 30,
        }))
        .unwrap();
        reg.register(Arc::new(NoopMiddleware {
            n: "a".into(),
            o: 10,
        }))
        .unwrap();
        reg.register(Arc::new(NoopMiddleware {
            n: "b".into(),
            o: 20,
        }))
        .unwrap();

        let sorted = reg.sorted();
        assert_eq!(sorted[0].name(), "a");
        assert_eq!(sorted[1].name(), "b");
        assert_eq!(sorted[2].name(), "c");
    }

    #[test]
    fn test_apply_all_passthrough() {
        let mut reg = HttpMiddlewareRegistry::new();
        reg.register(Arc::new(NoopMiddleware {
            n: "one".into(),
            o: 10,
        }))
        .unwrap();
        reg.register(Arc::new(NoopMiddleware {
            n: "two".into(),
            o: 20,
        }))
        .unwrap();

        let input: Box<dyn Any> = Box::new(42_i32);
        let output = reg.apply_all(input).unwrap();
        assert_eq!(*output.downcast::<i32>().unwrap(), 42);
    }

    #[test]
    fn test_names() {
        let mut reg = HttpMiddlewareRegistry::new();
        reg.register(Arc::new(NoopMiddleware {
            n: "foo".into(),
            o: 10,
        }))
        .unwrap();
        let names = reg.names();
        assert!(names.contains(&"foo"));
    }
}
