use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use rinfra_core::error::AppError;
use rinfra_core::net::tcp::{TcpContext, TcpHandler};
use tokio::sync::RwLock;
use tracing::warn;

use super::protocol::{Frame, FrameKind};

pub type ServiceHandlerFn = Arc<
    dyn Fn(Vec<u8>) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AppError>> + Send>>
        + Send
        + Sync,
>;

/// tRPC protocol handler that implements `TcpHandler`.
/// Each incoming frame is deserialized as a `Frame` and routed to the named service.
pub struct TrpcHandler {
    services: RwLock<HashMap<String, ServiceHandlerFn>>,
}

impl TrpcHandler {
    pub fn new() -> Self {
        Self {
            services: RwLock::new(HashMap::new()),
        }
    }

    pub async fn register<F, Fut>(&self, name: &str, handler: F)
    where
        F: Fn(Vec<u8>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Vec<u8>, AppError>> + Send + 'static,
    {
        let wrapped: ServiceHandlerFn = Arc::new(move |payload| Box::pin(handler(payload)));
        self.services
            .write()
            .await
            .insert(name.to_string(), wrapped);
        tracing::info!(service = %name, "trpc: registered service handler");
    }

    pub fn register_raw(&self, name: &str, handler: ServiceHandlerFn) {
        self.services
            .blocking_write()
            .insert(name.to_string(), handler);
    }
}

#[async_trait]
impl TcpHandler for TrpcHandler {
    async fn on_message(
        &self,
        _ctx: &TcpContext,
        data: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, AppError> {
        let frame: Frame = match serde_json::from_slice(&data) {
            Ok(f) => f,
            Err(e) => {
                warn!(error = %e, "trpc: invalid frame");
                return Ok(None);
            }
        };

        match frame.kind {
            FrameKind::Tell => {
                let handler = self.services.read().await.get(&frame.service).cloned();
                if let Some(h) = handler {
                    tokio::spawn(async move {
                        let _ = h(frame.payload).await;
                    });
                } else {
                    warn!(service = %frame.service, "trpc tell: service not found");
                }
                Ok(None)
            }
            FrameKind::Request => {
                let handler = self.services.read().await.get(&frame.service).cloned();
                let request_id = frame.request_id;

                let response = match handler {
                    Some(h) => match h(frame.payload).await {
                        Ok(result) => Frame {
                            kind: FrameKind::Response,
                            request_id,
                            service: frame.service,
                            payload: result,
                        },
                        Err(e) => Frame {
                            kind: FrameKind::Error,
                            request_id,
                            service: frame.service,
                            payload: e.to_string().into_bytes(),
                        },
                    },
                    None => Frame {
                        kind: FrameKind::Error,
                        request_id,
                        service: frame.service.clone(),
                        payload: format!("service '{}' not found", frame.service).into_bytes(),
                    },
                };

                let reply = serde_json::to_vec(&response)
                    .map_err(|e| AppError::new(rinfra_core::error::ErrorCode::Internal, format!("serialize frame: {e}")))?;
                Ok(Some(reply))
            }
            _ => {
                warn!(kind = ?frame.kind, "trpc: unexpected frame kind");
                Ok(None)
            }
        }
    }
}
