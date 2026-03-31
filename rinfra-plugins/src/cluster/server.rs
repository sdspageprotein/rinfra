use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use rinfra_core::cluster::{ClusterMessage, Endpoint, NodeRole, NodeStatus};
use rinfra_core::cluster::NodeRegistry;
use rinfra_core::error::{AppError, ErrorCode};
use tokio::net::TcpListener;
use tokio_util::codec::Framed;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn, Instrument};

use super::codec::ClusterCodec;
use super::registry::ConnectedRegistry;

pub struct ClusterServer {
    registry: Arc<ConnectedRegistry>,
    token: String,
    ping_interval: Duration,
}

impl ClusterServer {
    pub fn new(
        registry: Arc<ConnectedRegistry>,
        token: String,
        ping_interval_secs: u64,
    ) -> Self {
        Self {
            registry,
            token,
            ping_interval: Duration::from_secs(ping_interval_secs),
        }
    }

    pub async fn run(self, addr: &str, shutdown: CancellationToken) -> Result<(), AppError> {
        let listener = TcpListener::bind(addr).await.map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("cluster bind failed: {e}"))
        })?;
        info!(addr = %addr, "cluster TCP server listening");

        loop {
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => {
                    info!("cluster server shutting down");
                    break;
                }
                result = listener.accept() => {
                    match result {
                        Ok((stream, peer)) => {
                            let registry = self.registry.clone();
                            let token = self.token.clone();
                            let ping_interval = self.ping_interval;
                            let cancel = shutdown.clone();
                            tokio::spawn(
                                handle_connection(
                                    stream, peer, registry, token, ping_interval, cancel,
                                )
                            );
                        }
                        Err(e) => {
                            error!(error = %e, "accept failed");
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    peer: std::net::SocketAddr,
    registry: Arc<ConnectedRegistry>,
    expected_token: String,
    ping_interval: Duration,
    shutdown: CancellationToken,
) {
    let conn_span = tracing::info_span!(
        "cluster.connection",
        peer = %peer,
        otel.kind = "server",
    );

    async {
        let mut framed = Framed::new(stream, ClusterCodec::new());

        let reg_info = match handle_register(&mut framed, &expected_token).await {
            Some(r) => r,
            None => {
                warn!("connection rejected during registration");
                return;
            }
        };

        tracing::Span::current().record("node_id", &reg_info.node_id.as_str());

        registry
            .register(
                reg_info.node_id.clone(),
                reg_info.role,
                reg_info.endpoints,
                reg_info.metadata,
            )
            .await;

        let node_id = reg_info.node_id;

        info!(node_id = %node_id, "node connected");

        let mut ping_timer = tokio::time::interval(ping_interval);
        ping_timer.tick().await;
        let mut missed_pongs: u32 = 0;

        loop {
            tokio::select! {
                biased;

                _ = shutdown.cancelled() => {
                    break;
                }

                _ = ping_timer.tick() => {
                    if missed_pongs >= 3 {
                        warn!(node_id = %node_id, "ping timeout, disconnecting");
                        break;
                    }
                    if framed.send(ClusterMessage::Ping).await.is_err() {
                        break;
                    }
                    missed_pongs += 1;
                }

                frame = framed.next() => {
                    match frame {
                        Some(Ok(msg)) => {
                            match msg {
                                ClusterMessage::Pong => {
                                    missed_pongs = 0;
                                }
                                ClusterMessage::Deregister { trace_context, .. } => {
                                    let span = crate::telemetry::extract_trace_context_span(
                                        &trace_context, "deregister",
                                    );
                                    async {
                                        info!(node_id = %node_id, "node deregistered");
                                        registry.unregister(&node_id).await;
                                    }
                                    .instrument(span)
                                    .await;
                                    return;
                                }
                                ClusterMessage::Ping => {
                                    let _ = framed.send(ClusterMessage::Pong).await;
                                }
                                ClusterMessage::ListNodes => {
                                    let nodes = registry.list_nodes().await.unwrap_or_default();
                                    let _ = framed.send(ClusterMessage::NodeList { nodes }).await;
                                }
                                _ => {}
                            }
                        }
                        Some(Err(e)) => {
                            warn!(node_id = %node_id, error = %e, "read error");
                            break;
                        }
                        None => {
                            break;
                        }
                    }
                }
            }
        }

        registry.set_status(&node_id, NodeStatus::Offline).await;
        info!(node_id = %node_id, "node disconnected");
    }
    .instrument(conn_span)
    .await;
}

struct RegisterInfo {
    node_id: String,
    role: NodeRole,
    endpoints: Vec<Endpoint>,
    metadata: std::collections::HashMap<String, String>,
}

async fn handle_register(
    framed: &mut Framed<tokio::net::TcpStream, ClusterCodec>,
    expected_token: &str,
) -> Option<RegisterInfo> {
    let timeout = tokio::time::timeout(Duration::from_secs(5), framed.next()).await;

    let msg = match timeout {
        Ok(Some(Ok(msg))) => msg,
        _ => return None,
    };

    match msg {
        ClusterMessage::Register {
            node_id,
            token,
            endpoints,
            metadata,
            role,
            trace_context,
        } => {
            let span = crate::telemetry::extract_trace_context_span(
                &trace_context, "register",
            );

            async {
                if !expected_token.is_empty() && token != expected_token {
                    warn!(node_id = %node_id, "auth failed");
                    let _ = framed
                        .send(ClusterMessage::RegisterAck {
                            success: false,
                            error: Some("invalid token".to_string()),
                        })
                        .await;
                    return None;
                }
                let _ = framed
                    .send(ClusterMessage::RegisterAck {
                        success: true,
                        error: None,
                    })
                    .await;
                Some(RegisterInfo { node_id, role, endpoints, metadata })
            }
            .instrument(span)
            .await
        }
        _ => None,
    }
}
