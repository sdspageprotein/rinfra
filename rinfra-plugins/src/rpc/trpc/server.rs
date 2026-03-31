use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use rinfra_core::error::{AppError, ErrorCode};
use tokio::io::BufReader;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use super::protocol::{read_frame, write_frame, Frame, FrameKind};

type ServiceHandlerFn = Arc<
    dyn Fn(Vec<u8>) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AppError>> + Send>>
        + Send
        + Sync,
>;

pub struct TrpcServer {
    services: Arc<RwLock<HashMap<String, ServiceHandlerFn>>>,
}

impl TrpcServer {
    pub fn new() -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
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

    pub async fn serve(&self, addr: &str, shutdown: CancellationToken) -> Result<(), AppError> {
        let listener = TcpListener::bind(addr).await.map_err(|e| {
            AppError::new(
                ErrorCode::RpcServerFailed,
                format!("trpc bind {addr}: {e}"),
            )
        })?;

        tracing::info!(addr = %addr, "trpc server started");

        let services = self.services.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => {
                        tracing::info!("trpc server shutting down");
                        break;
                    }
                    result = listener.accept() => {
                        match result {
                            Ok((stream, peer)) => {
                                tracing::debug!(peer = %peer, "trpc: accepted connection");
                                let services = services.clone();
                                tokio::spawn(async move {
                                    handle_connection(stream, services).await;
                                });
                            }
                            Err(e) => {
                                tracing::error!(error = %e, "trpc: accept failed");
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }
}

impl Default for TrpcServer {
    fn default() -> Self {
        Self::new()
    }
}

async fn get_handler(
    services: &RwLock<HashMap<String, ServiceHandlerFn>>,
    name: &str,
) -> Option<ServiceHandlerFn> {
    services.read().await.get(name).cloned()
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    services: Arc<RwLock<HashMap<String, ServiceHandlerFn>>>,
) {
    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);

    loop {
        let frame = match read_frame(&mut reader).await {
            Ok(Some(f)) => f,
            Ok(None) => {
                tracing::debug!("trpc: connection closed");
                break;
            }
            Err(e) => {
                tracing::warn!(error = %e, "trpc: error reading frame");
                break;
            }
        };

        match frame.kind {
            FrameKind::Tell => {
                if let Some(handler) = get_handler(&services, &frame.service).await {
                    tokio::spawn(async move {
                        let _ = handler(frame.payload).await;
                    });
                } else {
                    tracing::warn!(service = %frame.service, "trpc tell: service not found");
                }
            }
            FrameKind::Request => {
                let handler = get_handler(&services, &frame.service).await;
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

                if let Err(e) = write_frame(&mut writer, &response).await {
                    tracing::warn!(error = %e, "trpc: error sending response");
                    break;
                }
            }
            _ => {
                tracing::warn!(kind = ?frame.kind, "trpc: unexpected frame kind");
            }
        }
    }
}
