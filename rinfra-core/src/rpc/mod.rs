use async_trait::async_trait;

use crate::error::AppError;

/// Abstraction for an RPC service that can be started and stopped.
#[async_trait]
pub trait RpcServer: Send + Sync + 'static {
    /// Start the RPC server. This should block until the server shuts down.
    async fn start(&self) -> Result<(), AppError>;

    /// Signal the RPC server to initiate graceful shutdown.
    async fn shutdown(&self) -> Result<(), AppError>;
}

/// Metadata describing an RPC service endpoint.
#[derive(Debug, Clone)]
pub struct RpcServiceInfo {
    pub name: String,
    pub methods: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_service_info() {
        let info = RpcServiceInfo {
            name: "greeter".to_string(),
            methods: vec!["say_hello".to_string(), "say_goodbye".to_string()],
        };
        assert_eq!(info.name, "greeter");
        assert_eq!(info.methods.len(), 2);
    }
}
