use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::rpc::RpcServer as RpcServerTrait;
use tokio::sync::Notify;
use tonic::transport::server::Router as TonicRouter;
use tonic::transport::Server;
use tonic_health::server::health_reporter;
use tracing::info;

pub struct GrpcServer {
    bind_addr: String,
    shutdown_signal: Arc<Notify>,
}

impl GrpcServer {
    pub fn new(bind_addr: String) -> Self {
        Self {
            bind_addr,
            shutdown_signal: Arc::new(Notify::new()),
        }
    }

    pub fn shutdown_handle(&self) -> Arc<Notify> {
        self.shutdown_signal.clone()
    }

    fn parse_addr(&self) -> Result<SocketAddr, AppError> {
        self.bind_addr.parse().map_err(|e| {
            AppError::new(
                ErrorCode::RpcServerFailed,
                format!("invalid gRPC address '{}': {e}", self.bind_addr),
            )
        })
    }

    pub async fn start_with_routes(
        &self,
        configure: impl FnOnce(TonicRouter) -> TonicRouter + Send,
    ) -> Result<(), AppError> {
        let addr = self.parse_addr()?;
        info!(addr = %addr, "gRPC server starting");

        let (mut health_reporter, health_service) = health_reporter();
        health_reporter
            .set_service_status("", tonic_health::ServingStatus::Serving)
            .await;

        let shutdown = self.shutdown_signal.clone();

        let mut server = Server::builder();
        let router = server.add_service(health_service);
        let router = configure(router);

        router
            .serve_with_shutdown(addr, async move {
                shutdown.notified().await;
                info!("gRPC server shutting down");
            })
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::RpcServerFailed,
                    format!("gRPC server error: {e}"),
                )
            })?;

        Ok(())
    }
}

#[async_trait]
impl RpcServerTrait for GrpcServer {
    async fn start(&self) -> Result<(), AppError> {
        self.start_with_routes(|router| router).await
    }

    async fn shutdown(&self) -> Result<(), AppError> {
        self.shutdown_signal.notify_one();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grpc_server_new() {
        let server = GrpcServer::new("0.0.0.0:9090".into());
        assert!(Arc::strong_count(&server.shutdown_signal) >= 1);
    }

    #[tokio::test]
    async fn test_grpc_server_parse_addr() {
        let server = GrpcServer::new("127.0.0.1:50051".into());
        let addr = server.parse_addr().unwrap();
        assert_eq!(addr.port(), 50051);
    }

    #[tokio::test]
    async fn test_grpc_server_shutdown_handle() {
        let server = GrpcServer::new("0.0.0.0:9090".into());
        let handle = server.shutdown_handle();
        server.shutdown().await.unwrap();
        assert!(Arc::strong_count(&handle) >= 1);
    }
}
