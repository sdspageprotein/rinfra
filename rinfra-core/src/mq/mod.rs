use std::collections::HashMap;
use std::time::SystemTime;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// A message in the queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub topic: String,
    pub payload: Vec<u8>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
    pub created_at: u64,
}

impl Message {
    pub fn new(topic: impl Into<String>, payload: Vec<u8>) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            topic: topic.into(),
            payload,
            headers: HashMap::new(),
            created_at: timestamp,
        }
    }

    pub fn with_headers(
        topic: impl Into<String>,
        payload: Vec<u8>,
        headers: HashMap<String, String>,
    ) -> Self {
        let mut msg = Self::new(topic, payload);
        msg.headers = headers;
        msg
    }
}

/// Receiver handle for consuming messages from a topic.
/// Framework-agnostic: wraps any async receiver via trait object.
pub struct MessageReceiver {
    inner: Box<dyn MessageReceiverImpl>,
}

#[async_trait]
pub trait MessageReceiverImpl: Send {
    async fn recv(&mut self) -> Option<Message>;
}

impl MessageReceiver {
    pub fn new(inner: Box<dyn MessageReceiverImpl>) -> Self {
        Self { inner }
    }

    pub async fn recv(&mut self) -> Option<Message> {
        self.inner.recv().await
    }
}

/// Message queue abstraction with point-to-point queue semantics.
/// Unlike EventBus (broadcast to all subscribers), MessageBus delivers
/// each message to exactly one subscriber (load distribution).
#[async_trait]
pub trait MessageBus: Send + Sync + 'static {
    async fn publish(&self, topic: &str, payload: Vec<u8>) -> Result<(), AppError>;
    async fn subscribe(&self, topic: &str) -> Result<MessageReceiver, AppError>;

    /// Publish with metadata headers (e.g. trace context propagation).
    /// Default delegates to `publish`, discarding headers.
    async fn publish_with_headers(
        &self,
        topic: &str,
        payload: Vec<u8>,
        _headers: HashMap<String, String>,
    ) -> Result<(), AppError> {
        self.publish(topic, payload).await
    }

    /// Acknowledge a consumed message (at-least-once semantics).
    /// No-op for at-most-once backends like InMemoryBus.
    async fn ack(&self, _topic: &str, _msg_id: &str) -> Result<(), AppError> {
        Ok(())
    }

    /// Backend-specific health probe.
    async fn health_check(&self) -> Result<bool, AppError> {
        Ok(true)
    }

    /// Gracefully close connections and release resources.
    async fn close(&self) -> Result<(), AppError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_new() {
        let msg = Message::new("test-topic", b"hello".to_vec());
        assert_eq!(msg.topic, "test-topic");
        assert_eq!(msg.payload, b"hello");
        assert!(!msg.id.is_empty());
        assert!(msg.created_at > 0);
        assert!(msg.headers.is_empty());
    }

    #[test]
    fn test_message_with_headers() {
        let mut headers = HashMap::new();
        headers.insert("traceparent".to_string(), "00-abc-def-01".to_string());
        let msg = Message::with_headers("topic", b"data".to_vec(), headers);
        assert_eq!(msg.headers.get("traceparent").unwrap(), "00-abc-def-01");
    }

    #[test]
    fn test_message_unique_ids() {
        let m1 = Message::new("t", vec![]);
        let m2 = Message::new("t", vec![]);
        assert_ne!(m1.id, m2.id);
    }

    #[test]
    fn test_message_serde_without_headers() {
        let msg = Message::new("t", b"data".to_vec());
        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("headers"));
        let decoded: Message = serde_json::from_str(&json).unwrap();
        assert!(decoded.headers.is_empty());
    }

    #[test]
    fn test_message_serde_backward_compat() {
        let json = r#"{"id":"x","topic":"t","payload":[1,2],"created_at":100}"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert!(msg.headers.is_empty());
        assert_eq!(msg.id, "x");
    }
}
