use async_trait::async_trait;
use rinfra_core::mq::{Message, MessageReceiverImpl};
use tokio::sync::mpsc;

/// tokio mpsc-backed implementation of MessageReceiverImpl.
pub struct MpscMessageReceiver {
    rx: mpsc::Receiver<Message>,
}

impl MpscMessageReceiver {
    pub fn new(rx: mpsc::Receiver<Message>) -> Self {
        Self { rx }
    }
}

#[async_trait]
impl MessageReceiverImpl for MpscMessageReceiver {
    async fn recv(&mut self) -> Option<Message> {
        let msg = self.rx.recv().await;
        if let Some(ref m) = msg {
            metrics::counter!("mq_messages_consumed_total", "topic" => m.topic.clone())
                .increment(1);
        }
        msg
    }
}
