use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use rinfra_core::cluster::{ClusterMessage, Endpoint, NodeInfo, NodeRole};
use rinfra_core::config::ClusterPluginConfigs;
use rinfra_core::error::{AppError, ErrorCode};
use tokio::net::TcpStream;
use tokio_util::codec::Framed;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn, Instrument};

use super::codec::ClusterCodec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConnectionState {
    Connected = 0,
    Disconnected = 1,
    Reconnecting = 2,
}

impl From<u8> for ConnectionState {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::Connected,
            1 => Self::Disconnected,
            _ => Self::Reconnecting,
        }
    }
}

#[derive(Clone)]
pub struct ConnectionHandle {
    state: Arc<AtomicU8>,
}

impl ConnectionHandle {
    pub fn state(&self) -> ConnectionState {
        self.state.load(Ordering::SeqCst).into()
    }

    pub fn is_connected(&self) -> bool {
        self.state() == ConnectionState::Connected
    }
}

pub struct ClusterConnection {
    node_id: String,
    main_address: String,
    token: String,
    endpoints: Vec<Endpoint>,
    metadata: HashMap<String, String>,
    ping_interval: Duration,
    node_list: Arc<tokio::sync::RwLock<Vec<NodeInfo>>>,
}

impl ClusterConnection {
    pub fn new(
        config: &ClusterPluginConfigs,
        endpoints: Vec<Endpoint>,
        metadata: HashMap<String, String>,
    ) -> Self {
        let node_id = if config.node_id.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            config.node_id.clone()
        };

        Self {
            node_id,
            main_address: config.main_address.clone(),
            token: config.cluster_token.clone(),
            endpoints,
            metadata,
            ping_interval: Duration::from_secs(config.ping_interval_secs),
            node_list: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        }
    }

    pub fn node_list(&self) -> Arc<tokio::sync::RwLock<Vec<NodeInfo>>> {
        self.node_list.clone()
    }

    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// First-time connect + register. Fails fast if Main is unreachable.
    pub async fn connect_once(&self) -> Result<(), AppError> {
        let mut framed = self.tcp_connect().await?;
        self.do_register(&mut framed).await?;
        Ok(())
    }

    /// Spawn background task: maintain connection, auto-reconnect on disconnect.
    pub fn spawn_background(self, shutdown: CancellationToken) -> ConnectionHandle {
        let state = Arc::new(AtomicU8::new(ConnectionState::Connected as u8));
        let handle = ConnectionHandle {
            state: state.clone(),
        };
        let span = tracing::info_span!(
            "cluster.worker",
            node_id = %self.node_id,
            main = %self.main_address,
        );
        tokio::spawn(
            self.run_loop(state, shutdown).instrument(span)
        );
        handle
    }

    async fn run_loop(self, state: Arc<AtomicU8>, shutdown: CancellationToken) {
        let mut backoff = Duration::from_secs(3);
        let max_backoff = Duration::from_secs(30);

        loop {
            let mut framed = match self.tcp_connect().await {
                Ok(f) => f,
                Err(e) => {
                    warn!(error = %e, "reconnect tcp failed");
                    state.store(ConnectionState::Reconnecting as u8, Ordering::SeqCst);
                    tokio::select! {
                        _ = shutdown.cancelled() => return,
                        _ = tokio::time::sleep(backoff) => {}
                    }
                    backoff = (backoff * 2).min(max_backoff);
                    continue;
                }
            };

            if let Err(e) = self.do_register(&mut framed).await {
                warn!(error = %e, "register failed");
                state.store(ConnectionState::Reconnecting as u8, Ordering::SeqCst);
                tokio::select! {
                    _ = shutdown.cancelled() => return,
                    _ = tokio::time::sleep(backoff) => {}
                }
                backoff = (backoff * 2).min(max_backoff);
                continue;
            }

            state.store(ConnectionState::Connected as u8, Ordering::SeqCst);
            backoff = Duration::from_secs(3);
            info!(main = %self.main_address, "connected to main");

            if !self.message_loop(&mut framed, &shutdown).await {
                return; // shutdown requested
            }

            warn!("disconnected from main, reconnecting");
            state.store(ConnectionState::Reconnecting as u8, Ordering::SeqCst);
        }
    }

    /// Returns `true` if disconnected (should reconnect), `false` if shutdown.
    async fn message_loop(
        &self,
        framed: &mut Framed<TcpStream, ClusterCodec>,
        shutdown: &CancellationToken,
    ) -> bool {
        let mut ping_timer = tokio::time::interval(self.ping_interval);
        ping_timer.tick().await;

        let mut sync_timer = tokio::time::interval(Duration::from_secs(5));
        sync_timer.tick().await;

        loop {
            tokio::select! {
                biased;

                _ = shutdown.cancelled() => {
                    let trace_ctx = crate::telemetry::inject_trace_context();
                    let _ = framed.send(ClusterMessage::Deregister {
                        node_id: self.node_id.clone(),
                        trace_context: trace_ctx,
                    }).await;
                    return false;
                }

                _ = ping_timer.tick() => {
                    if framed.send(ClusterMessage::Ping).await.is_err() {
                        return true;
                    }
                }

                _ = sync_timer.tick() => {
                    if framed.send(ClusterMessage::ListNodes).await.is_err() {
                        return true;
                    }
                }

                frame = framed.next() => {
                    match frame {
                        Some(Ok(ClusterMessage::Ping)) => {
                            if framed.send(ClusterMessage::Pong).await.is_err() {
                                return true;
                            }
                        }
                        Some(Ok(ClusterMessage::Pong)) => {}
                        Some(Ok(ClusterMessage::NodeList { nodes })) => {
                            *self.node_list.write().await = nodes;
                        }
                        Some(Ok(_)) => {}
                        Some(Err(e)) => {
                            warn!(error = %e, "read error from main");
                            return true;
                        }
                        None => return true,
                    }
                }
            }
        }
    }

    async fn tcp_connect(&self) -> Result<Framed<TcpStream, ClusterCodec>, AppError> {
        let stream = TcpStream::connect(&self.main_address).await.map_err(|e| {
            AppError::new(
                ErrorCode::ClusterMainUnreachable,
                format!("tcp connect to {} failed: {e}", self.main_address),
            )
        })?;
        Ok(Framed::new(stream, ClusterCodec::new()))
    }

    async fn do_register(
        &self,
        framed: &mut Framed<TcpStream, ClusterCodec>,
    ) -> Result<(), AppError> {
        let span = tracing::info_span!(
            "cluster.register",
            node_id = %self.node_id,
            otel.kind = "client",
        );

        async {
            let trace_ctx = crate::telemetry::inject_trace_context();

            framed
                .send(ClusterMessage::Register {
                    node_id: self.node_id.clone(),
                    role: NodeRole::Worker,
                    endpoints: self.endpoints.clone(),
                    metadata: self.metadata.clone(),
                    token: self.token.clone(),
                    trace_context: trace_ctx,
                })
                .await
                .map_err(|e| {
                    AppError::new(ErrorCode::ClusterRegisterFailed, format!("send register: {e}"))
                })?;

            let ack = tokio::time::timeout(Duration::from_secs(5), framed.next())
                .await
                .map_err(|_| AppError::new(ErrorCode::ClusterRegisterFailed, "register ack timeout"))?
                .ok_or_else(|| {
                    AppError::new(ErrorCode::ClusterRegisterFailed, "connection closed before ack")
                })?
                .map_err(|e| {
                    AppError::new(ErrorCode::ClusterRegisterFailed, format!("read ack: {e}"))
                })?;

            match ack {
                ClusterMessage::RegisterAck { success: true, .. } => Ok(()),
                ClusterMessage::RegisterAck {
                    success: false,
                    error,
                    ..
                } => Err(AppError::new(
                    ErrorCode::ClusterAuthFailed,
                    error.unwrap_or_else(|| "register rejected".to_string()),
                )),
                _ => Err(AppError::new(
                    ErrorCode::ClusterRegisterFailed,
                    "unexpected response",
                )),
            }
        }
        .instrument(span)
        .await
    }
}
