use std::collections::HashMap;

use async_trait::async_trait;
use rinfra_core::cluster::{Endpoint, NodeInfo, NodeRegistry, NodeRole, NodeStatus};
use rinfra_core::error::AppError;
use tokio::sync::RwLock;
use tracing::info;

/// Main-side node registry backed by active TCP connections.
pub struct ConnectedRegistry {
    nodes: RwLock<HashMap<String, NodeInfo>>,
}

impl ConnectedRegistry {
    pub fn new() -> Self {
        Self {
            nodes: RwLock::new(HashMap::new()),
        }
    }

    pub async fn register(
        &self,
        node_id: String,
        role: NodeRole,
        endpoints: Vec<Endpoint>,
        metadata: HashMap<String, String>,
    ) {
        let node = NodeInfo {
            id: node_id.clone(),
            role,
            endpoints,
            metadata,
            status: NodeStatus::Online,
        };
        info!(node_id = %node_id, role = ?role, "node registered");
        self.nodes.write().await.insert(node_id, node);
    }

    pub async fn unregister(&self, node_id: &str) {
        if self.nodes.write().await.remove(node_id).is_some() {
            info!(node_id = %node_id, "node unregistered");
        }
    }

    pub async fn set_status(&self, node_id: &str, status: NodeStatus) {
        if let Some(node) = self.nodes.write().await.get_mut(node_id) {
            node.status = status;
        }
    }
}

#[async_trait]
impl NodeRegistry for ConnectedRegistry {
    async fn get_node(&self, node_id: &str) -> Result<Option<NodeInfo>, AppError> {
        Ok(self.nodes.read().await.get(node_id).cloned())
    }

    async fn list_nodes(&self) -> Result<Vec<NodeInfo>, AppError> {
        Ok(self.nodes.read().await.values().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_get() {
        let reg = ConnectedRegistry::new();
        reg.register("n1".into(), NodeRole::Worker, vec![], HashMap::new()).await;
        let node = reg.get_node("n1").await.unwrap().unwrap();
        assert_eq!(node.id, "n1");
        assert_eq!(node.status, NodeStatus::Online);
    }

    #[tokio::test]
    async fn test_unregister() {
        let reg = ConnectedRegistry::new();
        reg.register("n1".into(), NodeRole::Worker, vec![], HashMap::new()).await;
        reg.unregister("n1").await;
        assert!(reg.get_node("n1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_list_nodes() {
        let reg = ConnectedRegistry::new();
        reg.register("n1".into(), NodeRole::Worker, vec![], HashMap::new()).await;
        reg.register("n2".into(), NodeRole::Worker, vec![], HashMap::new()).await;
        let nodes = reg.list_nodes().await.unwrap();
        assert_eq!(nodes.len(), 2);
    }

    #[tokio::test]
    async fn test_set_status() {
        let reg = ConnectedRegistry::new();
        reg.register("n1".into(), NodeRole::Worker, vec![], HashMap::new()).await;
        reg.set_status("n1", NodeStatus::Draining).await;
        let node = reg.get_node("n1").await.unwrap().unwrap();
        assert_eq!(node.status, NodeStatus::Draining);
    }
}
