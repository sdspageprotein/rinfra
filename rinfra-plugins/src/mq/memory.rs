use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, Mutex};
use tracing::debug;

use rinfra_core::config::MemoryMqConfig;
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::mq::{Message, MessageBus, MessageReceiver};

pub struct InMemoryBus {
    senders: Arc<Mutex<HashMap<String, Vec<mpsc::Sender<Message>>>>>,
    capacity: usize,
    round_robin: Arc<Mutex<HashMap<String, usize>>>,
}

impl InMemoryBus {
    pub fn new(config: &MemoryMqConfig) -> Self {
        Self {
            senders: Arc::new(Mutex::new(HashMap::new())),
            capacity: config.channel_capacity,
            round_robin: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn do_publish(&self, msg: Message) -> Result<(), AppError> {
        let topic = msg.topic.clone();
        let mut senders = self.senders.lock().await;
        let mut rr = self.round_robin.lock().await;

        if let Some(topic_senders) = senders.get_mut(topic.as_str()) {
            topic_senders.retain(|s| !s.is_closed());

            if topic_senders.is_empty() {
                debug!(topic = %topic, "no subscribers for topic, message dropped");
                return Ok(());
            }

            let idx = rr.entry(topic.clone()).or_insert(0);
            *idx %= topic_senders.len();

            topic_senders[*idx]
                .send(msg)
                .await
                .map_err(|e| AppError::new(ErrorCode::MqPublishFailed, format!("failed to send message: {e}")))?;

            *idx = (*idx + 1) % topic_senders.len();
        } else {
            debug!(topic = %topic, "no subscribers for topic, message dropped");
        }
        Ok(())
    }
}

#[async_trait]
impl MessageBus for InMemoryBus {
    async fn publish(&self, topic: &str, payload: Vec<u8>) -> Result<(), AppError> {
        let result = self.do_publish(Message::new(topic, payload)).await;
        if result.is_ok() {
            metrics::counter!("mq_messages_published_total", "backend" => "memory", "topic" => topic.to_string())
                .increment(1);
        }
        result
    }

    async fn publish_with_headers(
        &self,
        topic: &str,
        payload: Vec<u8>,
        headers: std::collections::HashMap<String, String>,
    ) -> Result<(), AppError> {
        self.do_publish(Message::with_headers(topic, payload, headers))
            .await
    }

    async fn subscribe(&self, topic: &str) -> Result<MessageReceiver, AppError> {
        let (tx, rx) = mpsc::channel(self.capacity);
        let mut senders = self.senders.lock().await;
        senders.entry(topic.to_string()).or_default().push(tx);
        debug!(topic = topic, "new subscriber added");
        Ok(MessageReceiver::new(Box::new(super::MpscMessageReceiver::new(rx))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> MemoryMqConfig {
        MemoryMqConfig {
            channel_capacity: 16,
        }
    }

    #[tokio::test]
    async fn test_publish_subscribe_single() {
        let bus = InMemoryBus::new(&test_config());
        let mut rx = bus.subscribe("topic1").await.unwrap();
        bus.publish("topic1", b"hello".to_vec()).await.unwrap();
        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.topic, "topic1");
        assert_eq!(msg.payload, b"hello");
        assert!(msg.headers.is_empty());
    }

    #[tokio::test]
    async fn test_publish_with_headers_preserved() {
        let bus = InMemoryBus::new(&test_config());
        let mut rx = bus.subscribe("t").await.unwrap();
        let mut h = HashMap::new();
        h.insert("key".to_string(), "val".to_string());
        bus.publish_with_headers("t", b"data".to_vec(), h)
            .await
            .unwrap();
        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.headers.get("key").unwrap(), "val");
    }

    #[tokio::test]
    async fn test_publish_no_subscribers_ok() {
        let bus = InMemoryBus::new(&test_config());
        let result = bus.publish("empty-topic", b"data".to_vec()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_round_robin_distribution() {
        let bus = InMemoryBus::new(&test_config());
        let mut rx1 = bus.subscribe("rr").await.unwrap();
        let mut rx2 = bus.subscribe("rr").await.unwrap();

        bus.publish("rr", b"msg1".to_vec()).await.unwrap();
        bus.publish("rr", b"msg2".to_vec()).await.unwrap();

        let m1 = rx1.recv().await.unwrap();
        let m2 = rx2.recv().await.unwrap();
        assert_eq!(m1.payload, b"msg1");
        assert_eq!(m2.payload, b"msg2");
    }

    #[tokio::test]
    async fn test_message_order_preserved() {
        let bus = InMemoryBus::new(&test_config());
        let mut rx = bus.subscribe("order").await.unwrap();

        bus.publish("order", b"first".to_vec()).await.unwrap();
        bus.publish("order", b"second".to_vec()).await.unwrap();
        bus.publish("order", b"third".to_vec()).await.unwrap();

        let m1 = rx.recv().await.unwrap();
        let m2 = rx.recv().await.unwrap();
        let m3 = rx.recv().await.unwrap();
        assert_eq!(m1.payload, b"first");
        assert_eq!(m2.payload, b"second");
        assert_eq!(m3.payload, b"third");
    }

    #[tokio::test]
    async fn test_independent_topics() {
        let bus = InMemoryBus::new(&test_config());
        let mut rx_a = bus.subscribe("topicA").await.unwrap();
        let mut rx_b = bus.subscribe("topicB").await.unwrap();

        bus.publish("topicA", b"a-msg".to_vec()).await.unwrap();
        bus.publish("topicB", b"b-msg".to_vec()).await.unwrap();

        let ma = rx_a.recv().await.unwrap();
        let mb = rx_b.recv().await.unwrap();
        assert_eq!(ma.payload, b"a-msg");
        assert_eq!(mb.payload, b"b-msg");
    }

    #[tokio::test]
    async fn test_health_check_returns_true() {
        let bus = InMemoryBus::new(&test_config());
        assert!(bus.health_check().await.unwrap());
    }

    #[tokio::test]
    async fn test_ack_is_noop() {
        let bus = InMemoryBus::new(&test_config());
        bus.ack("topic", "msg-id").await.unwrap();
    }

    #[tokio::test]
    async fn test_close_is_noop() {
        let bus = InMemoryBus::new(&test_config());
        bus.close().await.unwrap();
    }
}
