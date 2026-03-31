use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::net::tcp::{TcpContext, TcpHandler, TcpMiddleware};
use rinfra_core::net::transform::ByteTransform;
use tokio::net::TcpListener;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn, Instrument};

static TCP_CONNECTIONS_ACTIVE: AtomicI64 = AtomicI64::new(0);

pub struct TcpServer {
    bind_addr: String,
    listener_name: String,
    handler: Arc<dyn TcpHandler>,
    max_frame_size: usize,
    pipeline: Vec<Arc<dyn ByteTransform>>,
    middlewares: Vec<Arc<dyn TcpMiddleware>>,
}

impl TcpServer {
    pub fn new(
        bind_addr: String,
        listener_name: String,
        handler: Arc<dyn TcpHandler>,
    ) -> Self {
        Self {
            bind_addr,
            listener_name,
            handler,
            max_frame_size: 65536,
            pipeline: Vec::new(),
            middlewares: Vec::new(),
        }
    }

    pub fn with_max_frame_size(mut self, size: usize) -> Self {
        self.max_frame_size = size;
        self
    }

    pub fn with_pipeline(mut self, pipeline: Vec<Arc<dyn ByteTransform>>) -> Self {
        self.pipeline = pipeline;
        self
    }

    pub fn with_middlewares(mut self, middlewares: Vec<Arc<dyn TcpMiddleware>>) -> Self {
        self.middlewares = middlewares;
        self.middlewares.sort_by_key(|m| m.order());
        self
    }

    pub async fn start(self, shutdown: CancellationToken) -> Result<(), AppError> {
        let listener = TcpListener::bind(&self.bind_addr).await.map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("listener \"{}\" bind to {} failed: {e}", self.listener_name, self.bind_addr),
            )
        })?;

        if self.pipeline.is_empty() {
            info!(
                listener = %self.listener_name,
                protocol = "tcp",
                bind = %self.bind_addr,
                "listener started"
            );
        } else {
            let names: Vec<&str> = self.pipeline.iter().map(|t| t.name()).collect();
            info!(
                listener = %self.listener_name,
                protocol = "tcp",
                bind = %self.bind_addr,
                pipeline = ?names,
                "listener started"
            );
        }

        let handler = self.handler;
        let name = self.listener_name;
        let max_frame = self.max_frame_size;
        let pipeline = Arc::new(self.pipeline);
        let middlewares = Arc::new(self.middlewares);

        loop {
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => {
                    info!(listener = %name, "tcp server shutting down");
                    break;
                }
                result = listener.accept() => {
                    match result {
                        Ok((stream, peer)) => {
                            let h = handler.clone();
                            let n = name.clone();
                            let cancel = shutdown.clone();
                            let pl = pipeline.clone();
                            let mws = middlewares.clone();
                            tokio::spawn(
                                handle_tcp_connection(stream, peer, n, h, max_frame, pl, mws, cancel)
                            );
                        }
                        Err(e) => {
                            error!(listener = %name, error = %e, "tcp accept failed");
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

async fn handle_tcp_connection(
    stream: tokio::net::TcpStream,
    peer: std::net::SocketAddr,
    listener_name: String,
    handler: Arc<dyn TcpHandler>,
    max_frame_size: usize,
    pipeline: Arc<Vec<Arc<dyn ByteTransform>>>,
    middlewares: Arc<Vec<Arc<dyn TcpMiddleware>>>,
    shutdown: CancellationToken,
) {
    let conn_span = tracing::info_span!(
        "tcp.connection",
        listener = %listener_name,
        peer = %peer,
        otel.kind = "server",
    );

    async {
        let ctx = TcpContext {
            peer,
            listener_name: listener_name.clone(),
        };

        let active = TCP_CONNECTIONS_ACTIVE.fetch_add(1, Ordering::Relaxed) + 1;
        let labels = [("listener", listener_name.clone())];
        metrics::gauge!("tcp_connections_active", &labels).set(active as f64);

        // Middleware on_connect (ascending order).
        for mw in middlewares.iter() {
            if let Err(e) = mw.on_connect(&ctx).await {
                warn!(middleware = %mw.name(), error = %e, "tcp middleware rejected connection");
                TCP_CONNECTIONS_ACTIVE.fetch_sub(1, Ordering::Relaxed);
                metrics::gauge!("tcp_connections_active", &labels)
                    .set(TCP_CONNECTIONS_ACTIVE.load(Ordering::Relaxed) as f64);
                return;
            }
        }

        if let Err(e) = handler.on_connect(&ctx).await {
            warn!(error = %e, "on_connect rejected");
            TCP_CONNECTIONS_ACTIVE.fetch_sub(1, Ordering::Relaxed);
            metrics::gauge!("tcp_connections_active", &labels)
                .set(TCP_CONNECTIONS_ACTIVE.load(Ordering::Relaxed) as f64);
            return;
        }

        let codec = LengthDelimitedCodec::builder()
            .max_frame_length(max_frame_size)
            .new_codec();
        let mut framed = Framed::new(stream, codec);
        let mut msg_seq: u64 = 0;

        loop {
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => break,
                frame = framed.next() => {
                    match frame {
                        Some(Ok(raw)) => {
                            msg_seq += 1;
                            let raw_len = raw.len() as u64;
                            metrics::counter!("tcp_bytes_received_total", &labels).increment(raw_len);

                            let msg_span = tracing::debug_span!(
                                "tcp.message",
                                seq = msg_seq,
                                bytes = raw_len,
                            );

                            let decoded = decode_pipeline(&pipeline, raw.to_vec(), &listener_name, &peer);
                            let data = match decoded {
                                Ok(d) => d,
                                Err(_) => break,
                            };

                            // Middleware on_inbound chain (ascending order).
                            let mut inbound_data = Some(data);
                            for mw in middlewares.iter() {
                                if let Some(d) = inbound_data {
                                    match mw.on_inbound(&ctx, d).await {
                                        Ok(next) => inbound_data = next,
                                        Err(e) => {
                                            warn!(middleware = %mw.name(), error = %e, "tcp inbound middleware error");
                                            inbound_data = None;
                                            break;
                                        }
                                    }
                                } else {
                                    break;
                                }
                            }

                            let Some(data) = inbound_data else {
                                continue;
                            };

                            let result = handler.on_message(&ctx, data)
                                .instrument(msg_span)
                                .await;

                            match result {
                                Ok(Some(reply)) => {
                                    // Middleware on_outbound chain (reverse order).
                                    let mut outbound_data = Some(reply);
                                    for mw in middlewares.iter().rev() {
                                        if let Some(d) = outbound_data {
                                            match mw.on_outbound(&ctx, d).await {
                                                Ok(next) => outbound_data = next,
                                                Err(e) => {
                                                    warn!(middleware = %mw.name(), error = %e, "tcp outbound middleware error");
                                                    outbound_data = None;
                                                    break;
                                                }
                                            }
                                        } else {
                                            break;
                                        }
                                    }

                                    if let Some(final_reply) = outbound_data {
                                        let encoded = encode_pipeline(&pipeline, final_reply, &listener_name, &peer);
                                        match encoded {
                                            Ok(out) => {
                                                metrics::counter!("tcp_bytes_sent_total", &labels)
                                                    .increment(out.len() as u64);
                                                if framed.send(Bytes::from(out)).await.is_err() {
                                                    break;
                                                }
                                            }
                                            Err(_) => break,
                                        }
                                    }
                                }
                                Ok(None) => {}
                                Err(e) => {
                                    warn!(error = %e, "on_message error");
                                    break;
                                }
                            }
                        }
                        Some(Err(e)) => {
                            warn!(error = %e, "tcp frame error");
                            break;
                        }
                        None => break,
                    }
                }
            }
        }

        // Middleware on_disconnect (reverse order).
        for mw in middlewares.iter().rev() {
            mw.on_disconnect(&ctx).await;
        }

        let _ = handler.on_disconnect(&ctx).await;
        let active = TCP_CONNECTIONS_ACTIVE.fetch_sub(1, Ordering::Relaxed) - 1;
        metrics::gauge!("tcp_connections_active", &labels).set(active as f64);
        debug!("tcp disconnected");
    }
    .instrument(conn_span)
    .await;
}

fn decode_pipeline(
    pipeline: &[Arc<dyn ByteTransform>],
    mut data: Vec<u8>,
    listener_name: &str,
    peer: &std::net::SocketAddr,
) -> Result<Vec<u8>, ()> {
    for transform in pipeline.iter() {
        match transform.decode(data) {
            Ok(d) => data = d,
            Err(e) => {
                warn!(
                    listener = %listener_name,
                    peer = %peer,
                    transform = %transform.name(),
                    error = %e,
                    "inbound transform failed"
                );
                return Err(());
            }
        }
    }
    Ok(data)
}

fn encode_pipeline(
    pipeline: &[Arc<dyn ByteTransform>],
    mut data: Vec<u8>,
    listener_name: &str,
    peer: &std::net::SocketAddr,
) -> Result<Vec<u8>, ()> {
    for transform in pipeline.iter().rev() {
        match transform.encode(data) {
            Ok(d) => data = d,
            Err(e) => {
                warn!(
                    listener = %listener_name,
                    peer = %peer,
                    transform = %transform.name(),
                    error = %e,
                    "outbound transform failed"
                );
                return Err(());
            }
        }
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::time::Duration;

    struct EchoHandler;

    #[async_trait]
    impl TcpHandler for EchoHandler {
        async fn on_message(
            &self,
            _ctx: &TcpContext,
            data: Vec<u8>,
        ) -> Result<Option<Vec<u8>>, AppError> {
            Ok(Some(data))
        }
    }

    #[tokio::test]
    async fn test_tcp_server_echo() {
        let cancel = CancellationToken::new();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        drop(listener);

        let server = TcpServer::new(addr.clone(), "test".into(), Arc::new(EchoHandler));
        let c = cancel.clone();
        tokio::spawn(async move {
            let _ = server.start(c).await;
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let stream = tokio::net::TcpStream::connect(&addr).await.unwrap();
        let mut framed = Framed::new(stream, LengthDelimitedCodec::new());

        framed.send(Bytes::from("hello")).await.unwrap();
        let resp = framed.next().await.unwrap().unwrap();
        assert_eq!(&resp[..], b"hello");

        cancel.cancel();
    }

    #[tokio::test]
    async fn test_tcp_server_echo_with_lz4_pipeline() {
        use rinfra_core::compress::Compressor;
        use rinfra_core::net::transform::CompressorTransform;
        use crate::compress::Lz4Compressor;

        let cancel = CancellationToken::new();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        drop(listener);

        let compressor = Arc::new(Lz4Compressor);
        let lz4: Arc<dyn ByteTransform> = Arc::new(CompressorTransform(compressor.clone()));
        let server = TcpServer::new(addr.clone(), "test-lz4".into(), Arc::new(EchoHandler))
            .with_pipeline(vec![lz4.clone()]);

        let c = cancel.clone();
        tokio::spawn(async move {
            let _ = server.start(c).await;
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let stream = tokio::net::TcpStream::connect(&addr).await.unwrap();
        let mut framed = Framed::new(stream, LengthDelimitedCodec::new());

        let original = b"hello lz4 pipeline test!".to_vec();
        let compressed = compressor.compress(&original).unwrap();
        framed.send(Bytes::from(compressed)).await.unwrap();

        let resp = framed.next().await.unwrap().unwrap();
        let decompressed = compressor.decompress(&resp).unwrap();
        assert_eq!(decompressed, original);

        cancel.cancel();
    }
}
