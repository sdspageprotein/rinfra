use std::collections::HashMap;

use async_trait::async_trait;

use crate::error::AppError;

/// Incoming WebSocket message types.
#[derive(Debug, Clone)]
pub enum WsMessage {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close,
}

/// Parameters extracted from the WebSocket upgrade request.
#[derive(Debug, Clone, Default)]
pub struct WsConnectParams {
    /// URL query parameters (e.g. `?token=xxx` → `{"token": "xxx"}`)
    pub query: HashMap<String, String>,
    /// HTTP headers from the upgrade request
    pub headers: HashMap<String, String>,
}

/// Application-level WebSocket message handler.
/// Implement this trait to handle WebSocket connections.
#[async_trait]
pub trait WsHandler: Send + Sync + 'static {
    /// Called when a new WebSocket connection is established.
    /// `params` contains query parameters and headers from the upgrade request.
    async fn on_open(&self, _params: WsConnectParams) -> Result<(), AppError> {
        Ok(())
    }

    /// Called when a message is received. Return an optional response message.
    async fn on_message(&self, msg: WsMessage) -> Result<Option<WsMessage>, AppError>;

    /// Called when the connection is closed.
    async fn on_close(&self) -> Result<(), AppError> {
        Ok(())
    }
}
