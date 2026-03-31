use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use async_trait::async_trait;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, warn};

use rinfra_core::config::RedisStreamMqConfig;
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::mq::{Message, MessageBus, MessageReceiver};

pub struct RedisStreamBus {
    conn: redis::aio::ConnectionManager,
    config: RedisStreamMqConfig,
    created_groups: Arc<Mutex<std::collections::HashSet<String>>>,
}

impl RedisStreamBus {
    pub async fn connect(config: &RedisStreamMqConfig) -> Result<Self, AppError> {
        let client = redis::Client::open(config.url.as_str()).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("redis client open failed: {e}"),
            )
        })?;

        let conn = redis::aio::ConnectionManager::new(client).await.map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("redis connection manager failed: {e}"),
            )
        })?;

        Ok(Self {
            conn,
            config: config.clone(),
            created_groups: Arc::new(Mutex::new(std::collections::HashSet::new())),
        })
    }

    fn stream_key(topic: &str) -> String {
        format!("rinfra:mq:{topic}")
    }

    async fn ensure_group(&self, stream_key: &str) -> Result<(), AppError> {
        {
            let groups = self.created_groups.lock().await;
            if groups.contains(stream_key) {
                return Ok(());
            }
        }

        let mut conn = self.conn.clone();
        let result: Result<String, redis::RedisError> = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(stream_key)
            .arg(&self.config.group_name)
            .arg("0")
            .arg("MKSTREAM")
            .query_async(&mut conn)
            .await;

        match result {
            Ok(_) => {
                debug!(stream = stream_key, group = %self.config.group_name, "consumer group created");
            }
            Err(e) => {
                let msg = e.to_string();
                if !msg.contains("BUSYGROUP") {
                    return Err(AppError::new(
                        ErrorCode::Internal,
                        format!("xgroup create failed: {e}"),
                    ));
                }
            }
        }

        self.created_groups
            .lock()
            .await
            .insert(stream_key.to_string());
        Ok(())
    }
}

#[async_trait]
impl MessageBus for RedisStreamBus {
    async fn publish(&self, topic: &str, payload: Vec<u8>) -> Result<(), AppError> {
        let stream_key = Self::stream_key(topic);
        self.ensure_group(&stream_key).await?;

        let mut conn = self.conn.clone();
        let mut cmd = redis::cmd("XADD");
        cmd.arg(&stream_key);

        if let Some(max_len) = self.config.max_len {
            cmd.arg("MAXLEN").arg("~").arg(max_len);
        }

        cmd.arg("*")
            .arg("topic")
            .arg(topic)
            .arg("payload")
            .arg(&payload);

        let _: String = cmd.query_async(&mut conn).await.map_err(|e| {
            AppError::new(
                ErrorCode::MqPublishFailed,
                format!("redis xadd failed: {e}"),
            )
        })?;
        metrics::counter!("mq_messages_published_total", "backend" => "redis_stream", "topic" => topic.to_string())
            .increment(1);
        Ok(())
    }

    async fn publish_with_headers(
        &self,
        topic: &str,
        payload: Vec<u8>,
        headers: HashMap<String, String>,
    ) -> Result<(), AppError> {
        let stream_key = Self::stream_key(topic);
        self.ensure_group(&stream_key).await?;

        let mut conn = self.conn.clone();
        let mut cmd = redis::cmd("XADD");
        cmd.arg(&stream_key);

        if let Some(max_len) = self.config.max_len {
            cmd.arg("MAXLEN").arg("~").arg(max_len);
        }

        cmd.arg("*")
            .arg("topic")
            .arg(topic)
            .arg("payload")
            .arg(&payload);

        let headers_json = serde_json::to_string(&headers).unwrap_or_default();
        cmd.arg("headers").arg(&headers_json);

        let _: String = cmd.query_async(&mut conn).await.map_err(|e| {
            AppError::new(
                ErrorCode::MqPublishFailed,
                format!("redis xadd failed: {e}"),
            )
        })?;
        Ok(())
    }

    async fn subscribe(&self, topic: &str) -> Result<MessageReceiver, AppError> {
        let stream_key = Self::stream_key(topic);
        self.ensure_group(&stream_key).await?;

        let (tx, rx) = mpsc::channel(256);
        let mut conn = self.conn.clone();
        let group_name = self.config.group_name.clone();
        let consumer_name = self.config.consumer_name.clone();
        let block_ms = self.config.block_ms;
        let batch_size = self.config.batch_size;
        let stream_key_owned = stream_key.clone();

        tokio::spawn(async move {
            loop {
                if tx.is_closed() {
                    debug!(stream = %stream_key_owned, "subscriber channel closed, stopping redis stream consumer");
                    break;
                }

                let result: Result<redis::streams::StreamReadReply, redis::RedisError> =
                    redis::cmd("XREADGROUP")
                        .arg("GROUP")
                        .arg(&group_name)
                        .arg(&consumer_name)
                        .arg("COUNT")
                        .arg(batch_size)
                        .arg("BLOCK")
                        .arg(block_ms)
                        .arg("STREAMS")
                        .arg(&stream_key_owned)
                        .arg(">")
                        .query_async(&mut conn)
                        .await;

                match result {
                    Ok(reply) => {
                        for stream in reply.keys {
                            for entry in stream.ids {
                                let msg = parse_stream_entry(&entry, &stream_key_owned);
                                if tx.send(msg).await.is_err() {
                                    return;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        if msg.contains("NOGROUP") {
                            warn!(stream = %stream_key_owned, "consumer group disappeared, will retry");
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                            continue;
                        }
                        error!(error = %e, stream = %stream_key_owned, "xreadgroup failed");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        });

        Ok(MessageReceiver::new(Box::new(super::MpscMessageReceiver::new(rx))))
    }

    async fn ack(&self, topic: &str, msg_id: &str) -> Result<(), AppError> {
        let stream_key = Self::stream_key(topic);
        let mut conn = self.conn.clone();

        let _: i64 = redis::cmd("XACK")
            .arg(&stream_key)
            .arg(&self.config.group_name)
            .arg(msg_id)
            .query_async(&mut conn)
            .await
            .map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("redis xack failed: {e}"))
            })?;
        Ok(())
    }

    async fn health_check(&self) -> Result<bool, AppError> {
        let mut conn = self.conn.clone();
        let pong: Result<String, _> = redis::cmd("PING").query_async(&mut conn).await;
        match pong {
            Ok(ref s) if s == "PONG" => Ok(true),
            Ok(_) => Ok(true),
            Err(e) => {
                warn!(error = %e, "redis streams health check failed");
                Ok(false)
            }
        }
    }

    async fn close(&self) -> Result<(), AppError> {
        Ok(())
    }
}

fn parse_stream_entry(
    entry: &redis::streams::StreamId,
    _stream_key: &str,
) -> Message {
    let topic = entry
        .get::<String>("topic")
        .unwrap_or_default();

    let payload: Vec<u8> = entry
        .get::<Vec<u8>>("payload")
        .unwrap_or_default();

    let mut headers = HashMap::new();
    if let Some(headers_json) = entry.get::<String>("headers") {
        if let Ok(parsed) = serde_json::from_str::<HashMap<String, String>>(&headers_json) {
            headers = parsed;
        }
    }

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    Message {
        id: entry.id.clone(),
        topic,
        payload,
        headers,
        created_at: timestamp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_key_mapping() {
        assert_eq!(RedisStreamBus::stream_key("orders"), "rinfra:mq:orders");
        assert_eq!(
            RedisStreamBus::stream_key("events.user"),
            "rinfra:mq:events.user"
        );
    }
}
