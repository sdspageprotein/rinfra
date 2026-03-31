pub mod middleware;
pub mod tcp;
pub mod transform;
pub mod ws;
pub use middleware::{HttpMiddleware, HttpMiddlewareRegistry};
pub use tcp::{TcpContext, TcpHandler, TcpMiddleware, TcpMiddlewareRegistry};
pub use transform::{ByteTransform, CompressorTransform, TransformRegistry};
pub use ws::{WsHandler, WsMessage};

#[cfg(feature = "axum")]
mod axum_compat {
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};
    use axum::Json;

    use crate::error::AppError;
    use crate::response::ApiResponse;

    impl IntoResponse for AppError {
        fn into_response(self) -> Response {
            let status = StatusCode::from_u16(self.code.http_status())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            let body = ApiResponse::<()>::error(&self);
            (status, Json(body)).into_response()
        }
    }
}
