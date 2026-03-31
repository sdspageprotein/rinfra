use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use rinfra_core::cluster::{ClusterMessage, NodeRegistry, NodeRole, NodeStatus};
use rinfra_plugins::cluster::codec::ClusterCodec;
use rinfra_plugins::cluster::registry::ConnectedRegistry;
use rinfra_plugins::cluster::server::ClusterServer;
use tokio::net::TcpStream;
use tokio_util::codec::Framed;
use tokio_util::sync::CancellationToken;

async fn start_server_on(token: &str) -> (Arc<ConnectedRegistry>, String, CancellationToken) {
    let registry = Arc::new(ConnectedRegistry::new());
    let cancel = CancellationToken::new();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    drop(listener);

    let server = ClusterServer::new(registry.clone(), token.to_string(), 60);
    let bind_addr = addr.clone();
    let c = cancel.clone();
    tokio::spawn(async move {
        let _ = server.run(&bind_addr, c).await;
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    (registry, addr, cancel)
}

async fn connect_raw(addr: &str) -> Framed<TcpStream, ClusterCodec> {
    let stream = TcpStream::connect(addr).await.unwrap();
    Framed::new(stream, ClusterCodec::new())
}

async fn register(
    framed: &mut Framed<TcpStream, ClusterCodec>,
    node_id: &str,
    token: &str,
) -> bool {
    framed
        .send(ClusterMessage::Register {
            node_id: node_id.to_string(),
            role: NodeRole::Worker,
            endpoints: vec![],
            metadata: HashMap::new(),
            token: token.to_string(),
            trace_context: None,
        })
        .await
        .unwrap();

    let ack = framed.next().await.unwrap().unwrap();
    matches!(ack, ClusterMessage::RegisterAck { success: true, .. })
}

#[tokio::test]
async fn test_register_and_list() {
    let (registry, addr, cancel) = start_server_on("").await;

    let mut framed = connect_raw(&addr).await;
    assert!(register(&mut framed, "w1", "").await);

    let nodes = registry.list_nodes().await.unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].id, "w1");
    assert_eq!(nodes[0].role, NodeRole::Worker);
    assert_eq!(nodes[0].status, NodeStatus::Online);

    cancel.cancel();
}

#[tokio::test]
async fn test_deregister() {
    let (registry, addr, cancel) = start_server_on("").await;

    let mut framed = connect_raw(&addr).await;
    assert!(register(&mut framed, "w1", "").await);
    assert_eq!(registry.list_nodes().await.unwrap().len(), 1);

    framed
        .send(ClusterMessage::Deregister {
            node_id: "w1".to_string(),
            trace_context: None,
        })
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(registry.list_nodes().await.unwrap().is_empty());

    cancel.cancel();
}

#[tokio::test]
async fn test_multiple_workers() {
    let (registry, addr, cancel) = start_server_on("").await;

    let mut f1 = connect_raw(&addr).await;
    let mut f2 = connect_raw(&addr).await;

    assert!(register(&mut f1, "w1", "").await);
    assert!(register(&mut f2, "w2", "").await);

    let nodes = registry.list_nodes().await.unwrap();
    assert_eq!(nodes.len(), 2);

    cancel.cancel();
}

#[tokio::test]
async fn test_auth_rejects_wrong_token() {
    let (_registry, addr, cancel) = start_server_on("correct-token").await;

    let mut framed = connect_raw(&addr).await;

    framed
        .send(ClusterMessage::Register {
            node_id: "bad".to_string(),
            role: NodeRole::Worker,
            endpoints: vec![],
            metadata: HashMap::new(),
            token: "wrong-token".to_string(),
            trace_context: None,
        })
        .await
        .unwrap();

    let ack = framed.next().await.unwrap().unwrap();
    match ack {
        ClusterMessage::RegisterAck {
            success: false,
            error,
        } => {
            assert!(error.unwrap().contains("invalid token"));
        }
        _ => panic!("expected RegisterAck with failure"),
    }

    cancel.cancel();
}

#[tokio::test]
async fn test_auth_accepts_valid_token() {
    let (registry, addr, cancel) = start_server_on("secret").await;

    let mut framed = connect_raw(&addr).await;
    assert!(register(&mut framed, "auth-w1", "secret").await);

    let nodes = registry.list_nodes().await.unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].id, "auth-w1");

    cancel.cancel();
}

#[tokio::test]
async fn test_empty_token_skips_auth() {
    let (registry, addr, cancel) = start_server_on("").await;

    let mut framed = connect_raw(&addr).await;
    assert!(register(&mut framed, "w1", "any-token").await);

    let nodes = registry.list_nodes().await.unwrap();
    assert_eq!(nodes.len(), 1);

    cancel.cancel();
}

#[tokio::test]
async fn test_disconnect_sets_offline() {
    let (registry, addr, cancel) = start_server_on("").await;

    {
        let mut framed = connect_raw(&addr).await;
        assert!(register(&mut framed, "w1", "").await);
        assert_eq!(registry.list_nodes().await.unwrap()[0].status, NodeStatus::Online);
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    let node = registry.get_node("w1").await.unwrap().unwrap();
    assert_eq!(node.status, NodeStatus::Offline);

    cancel.cancel();
}

#[tokio::test]
async fn test_ping_pong() {
    let (_registry, addr, cancel) = start_server_on("").await;

    let mut framed = connect_raw(&addr).await;
    assert!(register(&mut framed, "w1", "").await);

    framed.send(ClusterMessage::Ping).await.unwrap();
    let resp = framed.next().await.unwrap().unwrap();
    assert!(matches!(resp, ClusterMessage::Pong));

    cancel.cancel();
}
