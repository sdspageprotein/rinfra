use std::sync::Arc;

use rinfra_core::net::middleware::HttpMiddlewareRegistry;
use rinfra_plugins::net::HttpServer;

#[tokio::test]
async fn test_standalone_health_endpoint() {
    let server = HttpServer::new("127.0.0.1:0".into());
    let router = server.router().unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let url = format!("http://{addr}/health");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["code"], "OK");
    assert_eq!(body["data"]["status"], "healthy");

    handle.abort();
}

#[tokio::test]
async fn test_standalone_health_with_middleware() {
    use rinfra_plugins::net::middleware::builtin::RequestIdHttpMiddleware;

    let mut registry = HttpMiddlewareRegistry::new();
    registry
        .register(Arc::new(RequestIdHttpMiddleware))
        .unwrap();

    let server =
        HttpServer::new("127.0.0.1:0".into()).with_middleware_registry(registry);
    let router = server.router().unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let url = format!("http://{addr}/health");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), 200);

    let has_request_id = resp.headers().get("x-request-id").is_some();
    assert!(has_request_id, "request_id middleware should inject X-Request-Id header");

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["code"], "OK");
    assert_eq!(body["data"]["status"], "healthy");

    handle.abort();
}

#[tokio::test]
async fn test_standalone_404_for_unknown_route() {
    let server = HttpServer::new("127.0.0.1:0".into());
    let router = server.router().unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let url = format!("http://{addr}/nonexistent");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), 404);

    handle.abort();
}
