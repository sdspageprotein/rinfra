use bytes::{Bytes, BytesMut};
use rinfra_core::cluster::ClusterMessage;
use tokio_util::codec::{Decoder, Encoder, LengthDelimitedCodec};

/// Framed codec: 4-byte big-endian length prefix + JSON payload.
pub struct ClusterCodec {
    inner: LengthDelimitedCodec,
}

impl ClusterCodec {
    pub fn new() -> Self {
        Self {
            inner: LengthDelimitedCodec::builder()
                .max_frame_length(64 * 1024)
                .new_codec(),
        }
    }
}

impl Decoder for ClusterCodec {
    type Item = ClusterMessage;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match self.inner.decode(src)? {
            Some(frame) => {
                let msg: ClusterMessage = serde_json::from_slice(&frame)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                Ok(Some(msg))
            }
            None => Ok(None),
        }
    }
}

impl Encoder<ClusterMessage> for ClusterCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: ClusterMessage, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let json = serde_json::to_vec(&item)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        self.inner.encode(Bytes::from(json), dst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinfra_core::cluster::NodeRole;
    use std::collections::HashMap;

    #[test]
    fn test_encode_decode_roundtrip() {
        let mut codec = ClusterCodec::new();
        let msg = ClusterMessage::Register {
            node_id: "w1".to_string(),
            role: NodeRole::Worker,
            endpoints: vec![],
            metadata: HashMap::new(),
            token: "tok".to_string(),
            trace_context: None,
        };

        let mut buf = BytesMut::new();
        codec.encode(msg, &mut buf).unwrap();

        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        match decoded {
            ClusterMessage::Register { node_id, .. } => assert_eq!(node_id, "w1"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_encode_decode_ping_pong() {
        let mut codec = ClusterCodec::new();
        let mut buf = BytesMut::new();

        codec.encode(ClusterMessage::Ping, &mut buf).unwrap();
        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert!(matches!(decoded, ClusterMessage::Ping));
    }
}
