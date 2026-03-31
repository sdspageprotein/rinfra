use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClusterMode {
    Standalone,
    Cluster,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeRole {
    Main,
    Worker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    Online,
    Offline,
    Draining,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    pub protocol: String,
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub role: NodeRole,
    pub endpoints: Vec<Endpoint>,
    pub metadata: HashMap<String, String>,
    pub status: NodeStatus,
}

/// TCP cluster protocol messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClusterMessage {
    Register {
        node_id: String,
        role: NodeRole,
        endpoints: Vec<Endpoint>,
        metadata: HashMap<String, String>,
        token: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        trace_context: Option<HashMap<String, String>>,
    },
    RegisterAck {
        success: bool,
        error: Option<String>,
    },
    Deregister {
        node_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        trace_context: Option<HashMap<String, String>>,
    },
    ListNodes,
    NodeList {
        nodes: Vec<NodeInfo>,
    },
    Ping,
    Pong,
}

/// Read-only registry for querying cluster nodes.
#[async_trait]
pub trait NodeRegistry: Send + Sync + 'static {
    async fn get_node(&self, node_id: &str) -> Result<Option<NodeInfo>, AppError>;
    async fn list_nodes(&self) -> Result<Vec<NodeInfo>, AppError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_mode() {
        assert_eq!(ClusterMode::Standalone, ClusterMode::Standalone);
        assert_ne!(ClusterMode::Standalone, ClusterMode::Cluster);
    }

    #[test]
    fn test_node_role() {
        assert_eq!(NodeRole::Main, NodeRole::Main);
        assert_ne!(NodeRole::Main, NodeRole::Worker);
    }

    #[test]
    fn test_node_info() {
        let node = NodeInfo {
            id: "node-1".to_string(),
            role: NodeRole::Worker,
            endpoints: vec![Endpoint {
                protocol: "http".into(),
                address: "10.0.0.1:8081".into(),
            }],
            metadata: HashMap::new(),
            status: NodeStatus::Online,
        };
        assert_eq!(node.id, "node-1");
        assert_eq!(node.role, NodeRole::Worker);
        assert_eq!(node.status, NodeStatus::Online);
        assert_eq!(node.endpoints.len(), 1);
        assert_eq!(node.endpoints[0].protocol, "http");
    }

    #[test]
    fn test_cluster_mode_serde_roundtrip() {
        let json = serde_json::to_string(&ClusterMode::Cluster).unwrap();
        let parsed: ClusterMode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ClusterMode::Cluster);
    }

    #[test]
    fn test_node_role_serde_roundtrip() {
        let json = serde_json::to_string(&NodeRole::Worker).unwrap();
        let parsed: NodeRole = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, NodeRole::Worker);
    }

    #[test]
    fn test_cluster_message_serde() {
        let msg = ClusterMessage::Register {
            node_id: "w1".to_string(),
            role: NodeRole::Worker,
            endpoints: vec![],
            metadata: HashMap::new(),
            token: "secret".to_string(),
            trace_context: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ClusterMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ClusterMessage::Register { node_id, .. } => assert_eq!(node_id, "w1"),
            _ => panic!("wrong variant"),
        }
    }
}
