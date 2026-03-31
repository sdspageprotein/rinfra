use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_nats::jetstream;
use async_trait::async_trait;
use futures_util::StreamExt;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, warn};

use rinfra_core::config::NatsConfig;
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::mq::{Message, MessageBus, MessageReceiver};

pub struct NatsBus {
    client: async_nats::Client,
    jetstream: jetstream::Context,
    stream_name: String,
    consumer_group: String,
    ack_map: Arc<Mutex<HashMap<String, jetstream::Message>>>,
}

impl NatsBus {
    pub async fn connect(config: &NatsConfig) -> Result<Self, AppError> {
        let mut opts = async_nats::ConnectOptions::new()
            .connection_timeout(Duration::from_secs(config.connect_timeout_secs))
            .name("rinfra-mq");

        if let Some(max) = config.max_reconnects {
            opts = opts.max_reconnects(max);
        }

        let client = opts
            .connect(&config.url)
            .await
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("nats connect failed: {e}")))?;

        let js = jetstream::new(client.clone());

        js.get_or_create_stream(jetstream::stream::Config {
            name: config.stream_name.clone(),
            subjects: vec![format!("rinfra.>")],
            retention: jetstream::stream::RetentionPolicy::WorkQueue,
            ..Default::default()
        })
        .await
        .map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("nats stream create failed: {e}"),
            )
        })?;

        Ok(Self {
            client,
            jetstream: js,
            stream_name: config.stream_name.clone(),
            consumer_group: config.consumer_group.clone(),
            ack_map: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    fn subject(topic: &str) -> String {
        format!("rinfra.{topic}")
    }
}

#[async_trait]
impl MessageBus for NatsBus {
    async fn publish(&self, topic: &str, payload: Vec<u8>) -> Result<(), AppError> {
        self.jetstream
            .publish(Self::subject(topic), payload.into())
            .await
            .map_err(|e| AppError::new(ErrorCode::MqPublishFailed, format!("nats publish failed: {e}")))?
            .await
            .map_err(|e| AppError::new(ErrorCode::MqPublishFailed, format!("nats publish ack failed: {e}")))?;
        metrics::counter!("mq_messages_published_total", "backend" => "nats", "topic" => topic.to_string())
            .increment(1);
        Ok(())
    }

    async fn publish_with_headers(
        &self,
        topic: &str,
        payload: Vec<u8>,
        headers: HashMap<String, String>,
    ) -> Result<(), AppError> {
        let mut nats_headers = async_nats::HeaderMap::new();
        for (k, v) in &headers {
            if let Ok(name) = k.as_str().parse::<async_nats::HeaderName>() {
                nats_headers.insert(name, v.as_str());
            }
        }

        let publish = async_nats::jetstream::context::Publish::build()
            .headers(nats_headers)
            .payload(payload.into());

        self.jetstream
            .send_publish(Self::subject(topic), publish)
            .await
            .map_err(|e| AppError::new(ErrorCode::MqPublishFailed, format!("nats publish failed: {e}")))?
            .await
            .map_err(|e| AppError::new(ErrorCode::MqPublishFailed, format!("nats publish ack failed: {e}")))?;
        Ok(())
    }

    async fn subscribe(&self, topic: &str) -> Result<MessageReceiver, AppError> {
        let stream = self
            .jetstream
            .get_stream(&self.stream_name)
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::MqSubscribeFailed,
                    format!("nats get stream failed: {e}"),
                )
            })?;

        let consumer_name = format!("{}-{}", self.consumer_group, topic.replace('.', "-"));
        let consumer = stream
            .get_or_create_consumer(
                &consumer_name,
                jetstream::consumer::pull::Config {
                    durable_name: Some(consumer_name.clone()),
                    filter_subject: Self::subject(topic),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::MqSubscribeFailed,
                    format!("nats consumer create failed: {e}"),
                )
            })?;

        let (tx, rx) = mpsc::channel(256);
        let ack_map = self.ack_map.clone();

        tokio::spawn(async move {
            let messages = match consumer.messages().await {
                Ok(m) => m,
                Err(e) => {
                    error!(error = %e, "nats consumer messages stream failed");
                    return;
                }
            };

            let mut messages = messages.take_while(|_| {
                let tx_ref = &tx;
                std::future::ready(!tx_ref.is_closed())
            });

            while let Some(result) = messages.next().await {
                match result {
                    Ok(nats_msg) => {
                        let mut headers = HashMap::new();
                        if let Some(h) = nats_msg.headers.as_ref() {
                            for (key, values) in h.iter() {
                                if let Some(val) = values.iter().next() {
                                    headers
                                        .insert(key.to_string(), val.to_string());
                                }
                            }
                        }

                        let msg = Message::with_headers(
                            nats_msg.subject.as_str(),
                            nats_msg.payload.to_vec(),
                            headers,
                        );

                        let id = msg.id.clone();
                        ack_map.lock().await.insert(id, nats_msg);

                        if tx.send(msg).await.is_err() {
                            debug!("subscriber channel closed, stopping nats consumer");
                            break;
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "nats message receive error");
                    }
                }
            }
        });

        Ok(MessageReceiver::new(Box::new(super::MpscMessageReceiver::new(rx))))
    }

    async fn ack(&self, _topic: &str, msg_id: &str) -> Result<(), AppError> {
        if let Some(nats_msg) = self.ack_map.lock().await.remove(msg_id) {
            nats_msg
                .ack()
                .await
                .map_err(|e| AppError::new(ErrorCode::Internal, format!("nats ack failed: {e}")))?;
        }
        Ok(())
    }

    async fn health_check(&self) -> Result<bool, AppError> {
        match self.client.server_info() {
            info => {
                debug!(server_id = %info.server_id, "nats health check ok");
                Ok(true)
            }
        }
    }

    async fn close(&self) -> Result<(), AppError> {
        self.client.drain().await.map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("nats drain failed: {e}"))
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subject_mapping() {
        assert_eq!(NatsBus::subject("orders"), "rinfra.orders");
        assert_eq!(NatsBus::subject("events.user"), "rinfra.events.user");
    }
}
