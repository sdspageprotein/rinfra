use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;

use crate::error::{AppError, ErrorCode};

pub struct TcpContext {
    pub peer: SocketAddr,
    pub listener_name: String,
}

#[async_trait]
pub trait TcpHandler: Send + Sync + 'static {
    async fn on_connect(&self, _ctx: &TcpContext) -> Result<(), AppError> {
        Ok(())
    }

    async fn on_message(
        &self,
        ctx: &TcpContext,
        data: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, AppError>;

    async fn on_disconnect(&self, _ctx: &TcpContext) -> Result<(), AppError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// TCP Middleware
// ---------------------------------------------------------------------------

/// Middleware for TCP connections, analogous to [`HttpMiddleware`] for HTTP.
///
/// Hooks run in `order` sequence for `on_connect` / `on_inbound`,
/// reverse order for `on_outbound` / `on_disconnect`.
#[async_trait]
pub trait TcpMiddleware: Send + Sync + 'static {
    fn name(&self) -> &str;

    /// Execution order. Lower values run first for inbound, last for outbound.
    fn order(&self) -> i32;

    /// Called when a new connection is established. Return `Err` to reject.
    async fn on_connect(&self, _ctx: &TcpContext) -> Result<(), AppError> {
        Ok(())
    }

    /// Called before the handler processes a message.
    /// Return `Ok(Some(data))` to pass (possibly modified) data to the next
    /// middleware / handler. Return `Ok(None)` to silently drop the message.
    async fn on_inbound(
        &self,
        _ctx: &TcpContext,
        data: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, AppError> {
        Ok(Some(data))
    }

    /// Called after the handler produces a reply.
    /// Return `Ok(Some(data))` to send (possibly modified) reply.
    /// Return `Ok(None)` to suppress the reply.
    async fn on_outbound(
        &self,
        _ctx: &TcpContext,
        data: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, AppError> {
        Ok(Some(data))
    }

    /// Called when the connection is closed.
    async fn on_disconnect(&self, _ctx: &TcpContext) {}
}

/// Registry of named [`TcpMiddleware`] implementations.
pub struct TcpMiddlewareRegistry {
    middlewares: HashMap<String, Arc<dyn TcpMiddleware>>,
}

impl TcpMiddlewareRegistry {
    pub fn new() -> Self {
        Self {
            middlewares: HashMap::new(),
        }
    }

    pub fn register(&mut self, mw: Arc<dyn TcpMiddleware>) -> Result<(), AppError> {
        let name = mw.name().to_string();
        if self.middlewares.contains_key(&name) {
            return Err(AppError::new(
                ErrorCode::PluginAlreadyRegistered,
                format!("tcp middleware '{}' already registered", name),
            ));
        }
        self.middlewares.insert(name, mw);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn TcpMiddleware>> {
        self.middlewares.get(name)
    }

    pub fn is_empty(&self) -> bool {
        self.middlewares.is_empty()
    }

    /// Sorted ascending by order (for inbound/connect).
    pub fn sorted(&self) -> Vec<&Arc<dyn TcpMiddleware>> {
        let mut mws: Vec<_> = self.middlewares.values().collect();
        mws.sort_by_key(|m| m.order());
        mws
    }

    /// Sorted descending by order (for outbound/disconnect).
    pub fn sorted_rev(&self) -> Vec<&Arc<dyn TcpMiddleware>> {
        let mut mws: Vec<_> = self.middlewares.values().collect();
        mws.sort_by_key(|m| std::cmp::Reverse(m.order()));
        mws
    }
}

impl Default for TcpMiddlewareRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoopTcpMw {
        n: String,
        o: i32,
    }

    #[async_trait]
    impl TcpMiddleware for NoopTcpMw {
        fn name(&self) -> &str {
            &self.n
        }
        fn order(&self) -> i32 {
            self.o
        }
    }

    #[test]
    fn test_tcp_mw_register_and_get() {
        let mut reg = TcpMiddlewareRegistry::new();
        reg.register(Arc::new(NoopTcpMw {
            n: "test".into(),
            o: 10,
        }))
        .unwrap();
        assert!(reg.get("test").is_some());
        assert!(reg.get("missing").is_none());
    }

    #[test]
    fn test_tcp_mw_register_duplicate() {
        let mut reg = TcpMiddlewareRegistry::new();
        reg.register(Arc::new(NoopTcpMw {
            n: "dup".into(),
            o: 10,
        }))
        .unwrap();
        assert!(reg
            .register(Arc::new(NoopTcpMw {
                n: "dup".into(),
                o: 20
            }))
            .is_err());
    }

    #[test]
    fn test_tcp_mw_sorted_order() {
        let mut reg = TcpMiddlewareRegistry::new();
        reg.register(Arc::new(NoopTcpMw { n: "c".into(), o: 30 })).unwrap();
        reg.register(Arc::new(NoopTcpMw { n: "a".into(), o: 10 })).unwrap();
        reg.register(Arc::new(NoopTcpMw { n: "b".into(), o: 20 })).unwrap();

        let asc = reg.sorted();
        assert_eq!(asc[0].name(), "a");
        assert_eq!(asc[1].name(), "b");
        assert_eq!(asc[2].name(), "c");

        let desc = reg.sorted_rev();
        assert_eq!(desc[0].name(), "c");
        assert_eq!(desc[1].name(), "b");
        assert_eq!(desc[2].name(), "a");
    }
}
