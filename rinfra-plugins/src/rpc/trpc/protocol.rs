use rinfra_core::error::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameKind {
    Tell,
    Request,
    Response,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    pub kind: FrameKind,
    pub request_id: u64,
    pub service: String,
    pub payload: Vec<u8>,
}

const MAX_FRAME_SIZE: u32 = 16 * 1024 * 1024; // 16 MB

pub async fn write_frame<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    frame: &Frame,
) -> Result<(), AppError> {
    let data = serde_json::to_vec(frame).map_err(|e| {
        AppError::new(ErrorCode::RpcServiceError, format!("serialize frame: {e}"))
    })?;

    let len = data.len() as u32;
    writer
        .write_all(&len.to_be_bytes())
        .await
        .map_err(|e| AppError::new(ErrorCode::RpcServiceError, e.to_string()))?;
    writer
        .write_all(&data)
        .await
        .map_err(|e| AppError::new(ErrorCode::RpcServiceError, e.to_string()))?;
    writer
        .flush()
        .await
        .map_err(|e| AppError::new(ErrorCode::RpcServiceError, e.to_string()))?;
    Ok(())
}

pub async fn read_frame<R: AsyncReadExt + Unpin>(
    reader: &mut R,
) -> Result<Option<Frame>, AppError> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => {
            return Err(AppError::new(
                ErrorCode::RpcServerFailed,
                e.to_string(),
            ))
        }
    }

    let len = u32::from_be_bytes(len_buf);
    if len > MAX_FRAME_SIZE {
        return Err(AppError::new(
            ErrorCode::RpcServerFailed,
            format!("frame too large: {len} bytes"),
        ));
    }

    let mut buf = vec![0u8; len as usize];
    reader
        .read_exact(&mut buf)
        .await
        .map_err(|e| AppError::new(ErrorCode::RpcServerFailed, e.to_string()))?;

    let frame: Frame = serde_json::from_slice(&buf).map_err(|e| {
        AppError::new(
            ErrorCode::RpcServerFailed,
            format!("deserialize frame: {e}"),
        )
    })?;

    Ok(Some(frame))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_frame_roundtrip() {
        let frame = Frame {
            kind: FrameKind::Request,
            request_id: 42,
            service: "test-svc".to_string(),
            payload: b"hello world".to_vec(),
        };

        let mut buf = Vec::new();
        write_frame(&mut buf, &frame).await.unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let decoded = read_frame(&mut cursor).await.unwrap().unwrap();

        assert_eq!(decoded.kind, FrameKind::Request);
        assert_eq!(decoded.request_id, 42);
        assert_eq!(decoded.service, "test-svc");
        assert_eq!(decoded.payload, b"hello world");
    }

    #[tokio::test]
    async fn test_read_frame_eof_returns_none() {
        let mut cursor = std::io::Cursor::new(Vec::<u8>::new());
        let result = read_frame(&mut cursor).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_tell_frame_roundtrip() {
        let frame = Frame {
            kind: FrameKind::Tell,
            request_id: 0,
            service: "fire-forget".to_string(),
            payload: vec![1, 2, 3],
        };

        let mut buf = Vec::new();
        write_frame(&mut buf, &frame).await.unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let decoded = read_frame(&mut cursor).await.unwrap().unwrap();
        assert_eq!(decoded.kind, FrameKind::Tell);
        assert_eq!(decoded.request_id, 0);
    }
}
