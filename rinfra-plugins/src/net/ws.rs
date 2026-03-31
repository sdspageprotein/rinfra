use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use rinfra_core::config::WsOptions;
use rinfra_core::net::ws::{WsConnectParams, WsHandler, WsMessage};
use tokio::sync::Notify;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

/// Tracks active WebSocket connections and supports waiting for them to drain.
pub struct WsTracker {
    active: AtomicUsize,
    all_closed: Notify,
}

impl WsTracker {
    pub fn new() -> Self {
        Self {
            active: AtomicUsize::new(0),
            all_closed: Notify::new(),
        }
    }

    fn on_connect(&self) {
        self.active.fetch_add(1, Ordering::SeqCst);
    }

    fn on_disconnect(&self) {
        let prev = self.active.fetch_sub(1, Ordering::SeqCst);
        if prev == 1 {
            self.all_closed.notify_waiters();
        }
    }

    pub fn active_count(&self) -> usize {
        self.active.load(Ordering::SeqCst)
    }

    /// Wait until all tracked connections have closed, or timeout expires.
    pub async fn wait_all_closed(&self, timeout: Duration) {
        if self.active_count() == 0 {
            return;
        }
        tokio::select! {
            _ = self.all_closed.notified() => {}
            _ = tokio::time::sleep(timeout) => {
                warn!(
                    remaining = self.active_count(),
                    "ws drain timeout, some connections still open"
                );
            }
        }
    }
}

/// Axum handler for WebSocket upgrade requests.
/// Extracts query parameters and headers into `WsConnectParams`.
/// Shutdown token, WsConfig and WsTracker are injected via `Extension` by the framework.
pub async fn ws_upgrade_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    State(handler): State<Arc<dyn WsHandler>>,
    shutdown: Option<axum::extract::Extension<CancellationToken>>,
    ws_config: Option<axum::extract::Extension<WsOptions>>,
    tracker: Option<axum::extract::Extension<Arc<WsTracker>>>,
) -> impl IntoResponse {
    let header_map: HashMap<String, String> = headers
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
        .collect();

    let params = WsConnectParams {
        query,
        headers: header_map,
    };

    let token = shutdown.map(|e| e.0).unwrap_or_else(CancellationToken::new);
    let config = ws_config.map(|e| e.0).unwrap_or_default();
    let tracker = tracker.map(|e| e.0);
    ws.on_upgrade(move |socket| handle_socket(socket, handler, params, token, config, tracker))
}

async fn handle_socket(
    mut socket: WebSocket,
    handler: Arc<dyn WsHandler>,
    params: WsConnectParams,
    shutdown: CancellationToken,
    config: WsOptions,
    tracker: Option<Arc<WsTracker>>,
) {
    if let Some(ref t) = tracker {
        t.on_connect();
    }

    if let Err(e) = handler.on_open(params).await {
        warn!(error = %e, "ws on_open failed");
        if let Some(ref t) = tracker {
            t.on_disconnect();
        }
        return;
    }

    let ping_interval = Duration::from_secs(config.ping_interval_secs);
    let ping_timeout = Duration::from_secs(config.ping_timeout_secs);
    let mut ping_timer = tokio::time::interval(ping_interval);
    ping_timer.tick().await; // skip immediate first tick
    let mut last_pong = Instant::now();
    let mut waiting_pong = false;

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                debug!("ws: shutdown signal, sending close frame");
                let _ = socket.send(Message::Close(None)).await;
                break;
            }
            _ = ping_timer.tick() => {
                if waiting_pong && last_pong.elapsed() > ping_timeout {
                    debug!("ws: pong timeout, closing connection");
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
                if socket.send(Message::Ping(vec![].into())).await.is_err() {
                    break;
                }
                waiting_pong = true;
            }
            maybe_msg = socket.recv() => {
                match maybe_msg {
                    Some(Ok(Message::Pong(_))) => {
                        last_pong = Instant::now();
                        waiting_pong = false;
                    }
                    Some(Ok(msg)) => {
                        let ws_msg = axum_to_ws_message(msg);
                        if matches!(ws_msg, WsMessage::Close) {
                            break;
                        }
                        match handler.on_message(ws_msg).await {
                            Ok(Some(response)) => {
                                let axum_msg = ws_message_to_axum(response);
                                if socket.send(axum_msg).await.is_err() {
                                    break;
                                }
                            }
                            Ok(None) => {}
                            Err(e) => {
                                warn!(error = %e, "ws on_message failed");
                                break;
                            }
                        }
                    }
                    Some(Err(e)) => {
                        debug!(error = %e, "ws receive error");
                        break;
                    }
                    None => break,
                }
            }
        }
    }

    if let Err(e) = handler.on_close().await {
        warn!(error = %e, "ws on_close failed");
    }

    if let Some(ref t) = tracker {
        t.on_disconnect();
    }
    debug!("websocket connection closed");
}

fn axum_to_ws_message(msg: Message) -> WsMessage {
    match msg {
        Message::Text(t) => WsMessage::Text(t.as_str().to_string()),
        Message::Binary(b) => WsMessage::Binary(b.to_vec()),
        Message::Ping(p) => WsMessage::Ping(p.to_vec()),
        Message::Pong(p) => WsMessage::Pong(p.to_vec()),
        Message::Close(_) => WsMessage::Close,
    }
}

fn ws_message_to_axum(msg: WsMessage) -> Message {
    match msg {
        WsMessage::Text(t) => Message::text(t),
        WsMessage::Binary(b) => Message::binary(b),
        WsMessage::Ping(p) => Message::Ping(p.into()),
        WsMessage::Pong(p) => Message::Pong(p.into()),
        WsMessage::Close => Message::Close(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct EchoHandler;

    #[async_trait]
    impl WsHandler for EchoHandler {
        async fn on_message(
            &self,
            msg: WsMessage,
        ) -> Result<Option<WsMessage>, rinfra_core::error::AppError> {
            Ok(Some(msg))
        }
    }

    #[test]
    fn test_axum_to_ws_message_text() {
        let msg = axum_to_ws_message(Message::text("hello"));
        match msg {
            WsMessage::Text(t) => assert_eq!(t, "hello"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn test_axum_to_ws_message_binary() {
        let msg = axum_to_ws_message(Message::binary(vec![1, 2, 3]));
        match msg {
            WsMessage::Binary(b) => assert_eq!(b, vec![1, 2, 3]),
            _ => panic!("expected Binary"),
        }
    }

    #[test]
    fn test_ws_message_to_axum_text() {
        let msg = ws_message_to_axum(WsMessage::Text("test".to_string()));
        match msg {
            Message::Text(t) => assert_eq!(t.as_str(), "test"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn test_ws_message_roundtrip() {
        let original = Message::text("roundtrip");
        let ws_msg = axum_to_ws_message(original);
        let back = ws_message_to_axum(ws_msg);
        match back {
            Message::Text(t) => assert_eq!(t.as_str(), "roundtrip"),
            _ => panic!("expected Text"),
        }
    }

    #[tokio::test]
    async fn test_echo_handler_echoes_message() {
        let handler = EchoHandler;
        let msg = WsMessage::Text("hello".to_string());
        let result = handler.on_message(msg).await.unwrap();
        match result {
            Some(WsMessage::Text(t)) => assert_eq!(t, "hello"),
            _ => panic!("expected Some(Text)"),
        }
    }

    #[test]
    fn test_ws_tracker_connect_disconnect() {
        let tracker = WsTracker::new();
        assert_eq!(tracker.active_count(), 0);
        tracker.on_connect();
        assert_eq!(tracker.active_count(), 1);
        tracker.on_connect();
        assert_eq!(tracker.active_count(), 2);
        tracker.on_disconnect();
        assert_eq!(tracker.active_count(), 1);
        tracker.on_disconnect();
        assert_eq!(tracker.active_count(), 0);
    }

    #[tokio::test]
    async fn test_ws_tracker_wait_all_closed_immediate() {
        let tracker = WsTracker::new();
        tracker
            .wait_all_closed(Duration::from_millis(100))
            .await;
    }

    #[tokio::test]
    async fn test_ws_tracker_wait_drain() {
        let tracker = Arc::new(WsTracker::new());
        tracker.on_connect();

        let t = tracker.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            t.on_disconnect();
        });

        tracker
            .wait_all_closed(Duration::from_secs(1))
            .await;
        assert_eq!(tracker.active_count(), 0);
    }
}
