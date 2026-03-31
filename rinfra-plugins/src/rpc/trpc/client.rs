use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::resilience::RetryPolicy;
use tokio::io::BufReader;
use tokio::net::TcpStream;
use tokio::sync::{oneshot, Mutex};

use super::protocol::{read_frame, write_frame, Frame, FrameKind};

type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Vec<u8>, AppError>>>>>;

const MAX_RECONNECT_ATTEMPTS: u32 = 3;
const RECONNECT_BASE_DELAY_MS: u64 = 500;

pub struct TrpcClient {
    write_tx: tokio::sync::mpsc::Sender<Frame>,
    pending: PendingMap,
    next_id: Arc<AtomicU64>,
}

impl TrpcClient {
    pub async fn connect(addr: &str) -> Result<Self, AppError> {
        let stream = Self::connect_with_retry(addr).await?;

        let (reader, writer) = tokio::io::split(stream);
        let reader = BufReader::new(reader);
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));

        let (write_tx, write_rx) = tokio::sync::mpsc::channel::<Frame>(256);

        tokio::spawn(Self::write_loop(writer, write_rx));
        tokio::spawn(Self::read_loop(reader, pending.clone()));

        Ok(Self {
            write_tx,
            pending,
            next_id: Arc::new(AtomicU64::new(1)),
        })
    }

    async fn connect_with_retry(addr: &str) -> Result<TcpStream, AppError> {
        let policy = RetryPolicy::exponential(
            MAX_RECONNECT_ATTEMPTS,
            RECONNECT_BASE_DELAY_MS,
            5000,
        );
        let addr_owned = addr.to_string();
        crate::resilience::with_retry(&policy, || {
            let addr = addr_owned.clone();
            async move {
                TcpStream::connect(&addr).await.map_err(|e| {
                    AppError::new(
                        ErrorCode::RpcServerFailed,
                        format!("trpc connect {addr} failed: {e}"),
                    )
                })
            }
        })
        .await
    }

    pub async fn tell(&self, service: &str, payload: Vec<u8>) -> Result<(), AppError> {
        let frame = Frame {
            kind: FrameKind::Tell,
            request_id: 0,
            service: service.to_string(),
            payload,
        };
        self.write_tx.send(frame).await.map_err(|_| {
            AppError::new(ErrorCode::RpcServiceError, "trpc: connection closed")
        })
    }

    pub async fn ask(&self, service: &str, payload: Vec<u8>) -> Result<Vec<u8>, AppError> {
        let request_id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();

        self.pending.lock().await.insert(request_id, tx);

        let frame = Frame {
            kind: FrameKind::Request,
            request_id,
            service: service.to_string(),
            payload,
        };

        if self.write_tx.send(frame).await.is_err() {
            self.pending.lock().await.remove(&request_id);
            return Err(AppError::new(
                ErrorCode::RpcServiceError,
                "trpc: connection closed",
            ));
        }

        rx.await.map_err(|_| {
            AppError::new(
                ErrorCode::RpcServiceError,
                "trpc: connection lost while waiting for response",
            )
        })?
    }

    pub async fn ask_timeout(
        &self,
        service: &str,
        payload: Vec<u8>,
        timeout: Duration,
    ) -> Result<Vec<u8>, AppError> {
        match tokio::time::timeout(timeout, self.ask(service, payload)).await {
            Ok(result) => result,
            Err(_) => Err(AppError::new(
                ErrorCode::RpcServiceError,
                "trpc: ask timed out",
            )),
        }
    }

    async fn write_loop(
        mut writer: tokio::io::WriteHalf<TcpStream>,
        mut write_rx: tokio::sync::mpsc::Receiver<Frame>,
    ) {
        while let Some(frame) = write_rx.recv().await {
            if let Err(e) = write_frame(&mut writer, &frame).await {
                tracing::warn!(error = %e, "trpc: write error, closing connection");
                break;
            }
        }
    }

    async fn read_loop(
        mut reader: BufReader<tokio::io::ReadHalf<TcpStream>>,
        pending: PendingMap,
    ) {
        loop {
            match read_frame(&mut reader).await {
                Ok(Some(frame)) => match frame.kind {
                    FrameKind::Response => {
                        if let Some(tx) = pending.lock().await.remove(&frame.request_id) {
                            let _ = tx.send(Ok(frame.payload));
                        }
                    }
                    FrameKind::Error => {
                        if let Some(tx) = pending.lock().await.remove(&frame.request_id) {
                            let msg = String::from_utf8_lossy(&frame.payload).to_string();
                            let _ = tx.send(Err(AppError::new(
                                ErrorCode::RpcServiceError,
                                msg,
                            )));
                        }
                    }
                    _ => {
                        tracing::warn!(kind = ?frame.kind, "trpc: unexpected frame kind");
                    }
                },
                Ok(None) => {
                    tracing::debug!("trpc: connection closed by server");
                    break;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "trpc: read error");
                    break;
                }
            }
        }

        let mut guard = pending.lock().await;
        for (_, tx) in guard.drain() {
            let _ = tx.send(Err(AppError::new(
                ErrorCode::RpcServiceError,
                "trpc: connection closed",
            )));
        }
    }
}

impl Clone for TrpcClient {
    fn clone(&self) -> Self {
        Self {
            write_tx: self.write_tx.clone(),
            pending: self.pending.clone(),
            next_id: self.next_id.clone(),
        }
    }
}
