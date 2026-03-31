[中文文档](api-reference.zh-CN.md)

# Core API reference

This document lists all public traits, structs, and key functions in `rinfra-core` and `rinfra-plugins`.

---

## Table of contents

1. [Entry API](#entry-api)
2. [AppState](#appstate)
3. [Plugin system](#plugin-system)
4. [Networking](#networking)
5. [Data storage](#data-storage)
6. [Cache](#cache)
7. [Message queue](#message-queue)
8. [HTTP client](#http-client)
9. [Distributed lock](#distributed-lock)
10. [Timers](#timers)
11. [File storage](#file-storage)
12. [Audit logging](#audit-logging)
13. [Internationalization](#internationalization)
14. [Cryptography](#cryptography)
15. [Rate limiting](#rate-limiting)
16. [Resilience (circuit breaker / retry)](#resilience-circuit-breaker-retry)
17. [Script engine](#script-engine)
18. [Codecs](#codecs)
19. [Cluster](#cluster)
20. [Hot config reload](#hot-config-reload)
21. [Health checks](#health-checks)
22. [Error handling](#error-handling)
23. [API responses](#api-responses)

---

## Entry API

### `rinfra_plugins::run(opts: RunOptions)`

One-stop framework entry: parses CLI arguments, loads configuration, builds plugins, and starts all listeners.

### `RunOptions`

```rust
RunOptions::new()
    .http_router("listener-name", |state| Router)    // HTTP routing
    .tcp_handler("listener-name", Arc<dyn TcpHandler>) // TCP handler
    .ws_handler("listener-name", Arc<dyn WsHandler>)   // WebSocket handler
    .grpc_service("listener-name", |router| router)    // gRPC service
    .plugin(Box<dyn Plugin>)                            // custom plugin
    .metadata(vec![("key", "value")])                   // node metadata
    .extra_commands(|args, config, format| bool)        // custom CLI commands
```

### `Application` / `ApplicationBuilder`

Low-level builder (used inside `run()`; suitable for advanced direct use):

```rust
Application::builder()
    .config_path("config/custom.yaml")
    .plugin(Box::new(MyPlugin))
    .http_router("main", |state| router)
    .tcp_handler("tcp", handler)
    .http_middleware(Arc::new(MyMiddleware))
    .tcp_middleware(Arc::new(MyTcpMiddleware))
    .byte_transform(Arc::new(MyTransform))
    .build().await?
    .run().await?;
```

---

## AppState

Runtime access point for all framework capabilities. Injected via the Axum `State` extractor.

```rust
pub struct AppState {
    pub config: Arc<RinfraConfig>,   // full configuration
    pub started_at: Instant,         // process start time
}
```

### Convenience accessors

| Method | Return type | Related plugin |
|--------|-------------|----------------|
| `cache()` | `Option<&Arc<dyn Cache>>` | cache.* |
| `store()` | `Option<&Arc<dyn Store>>` | store.* |
| `db()` | `Option<&Arc<dyn DbConnection>>` | store.* |
| `stores()` | `Option<&StoreRegistry>` | store.* |
| `message_bus()` | `Option<&Arc<dyn MessageBus>>` | mq.* |
| `ratelimiter()` | `Option<&Arc<dyn RateLimiter>>` | ratelimit.* |
| `crypto()` | `Option<&Arc<dyn Crypto>>` | crypto.aesgcm |
| `http_client()` | `Option<&Arc<dyn HttpClient>>` | http_client |
| `distributed_lock()` | `Option<&Arc<dyn DistributedLock>>` | lock |
| `file_store()` | `Option<&Arc<dyn FileStore>>` | file_store |
| `audit_logger()` | `Option<&Arc<dyn AuditLogger>>` | audit |
| `i18n()` | `Option<&Arc<dyn I18n>>` | i18n |
| `config_watcher()` | `Option<&Arc<dyn ConfigWatcher>>` | config_watch |
| `script_engine()` | `Option<&Arc<dyn ScriptEngine>>` | script.* |
| `script_engines()` | `Option<&ScriptEngineRegistry>` | script.* |
| `codecs()` | `Option<&CodecRegistry>` | codec.* |
| `node_registry()` | `Option<&Arc<dyn NodeRegistry>>` | cluster |
| `health_checkers()` | `Option<&HealthCheckerRegistry>` | automatic |
| `uptime_secs()` | `u64` | — |

### Generic type map

```rust
// register an arbitrary type
state.set::<MyType>(value);

// read
let val: Option<&MyType> = state.get::<MyType>();

// check presence
let exists: bool = state.has::<MyType>();
```

---

## Plugin system

### `Plugin` trait

```rust
#[async_trait]
pub trait Plugin: Send + Sync + 'static {
    fn manifest(&self) -> &PluginManifest;
    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError>;
    async fn shutdown(&self) -> Result<(), AppError> { Ok(()) }
}
```

### `PluginContext`

Context during plugin `build` for registering components.

```rust
ctx.config()                        // read configuration
ctx.set::<T>(value)                 // register arbitrary component
ctx.get::<T>()                      // read registered component
ctx.set_cache(Arc<dyn Cache>)       // register cache
ctx.set_store(Arc<dyn Store>)       // register store
ctx.set_message_bus(Arc<dyn MessageBus>)
ctx.set_db(Arc<dyn DbConnection>)
ctx.add_codec(Box<dyn Codec>)
ctx.add_health_checker(Arc<dyn HealthCheckable>)
ctx.add_shutdown_hook(|| async { Ok(()) })
```

### `HealthCheckable` trait

```rust
#[async_trait]
pub trait HealthCheckable: Send + Sync + 'static {
    fn name(&self) -> &str;                  // probe name
    async fn check(&self) -> HealthCheckResult;
}

// HealthCheckResult
HealthCheckResult::healthy()
HealthCheckResult::unhealthy("reason")
HealthCheckResult::degraded("reason")
```

---

## Networking

### `TcpHandler` trait

```rust
#[async_trait]
pub trait TcpHandler: Send + Sync + 'static {
    async fn on_connect(&self, ctx: &TcpContext) -> Result<(), AppError> { Ok(()) }
    async fn on_message(&self, ctx: &TcpContext, data: Vec<u8>) -> Result<Option<Vec<u8>>, AppError>;
    async fn on_disconnect(&self, ctx: &TcpContext) -> Result<(), AppError> { Ok(()) }
}

pub struct TcpContext {
    pub peer: SocketAddr,
    pub listener_name: String,
}
```

### `TcpMiddleware` trait

```rust
#[async_trait]
pub trait TcpMiddleware: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn order(&self) -> i32;
    async fn on_connect(&self, ctx: &TcpContext) -> Result<(), AppError> { Ok(()) }
    async fn on_inbound(&self, ctx: &TcpContext, data: Vec<u8>) -> Result<Option<Vec<u8>>, AppError> { Ok(Some(data)) }
    async fn on_outbound(&self, ctx: &TcpContext, data: Vec<u8>) -> Result<Option<Vec<u8>>, AppError> { Ok(Some(data)) }
    async fn on_disconnect(&self, ctx: &TcpContext) {}
}
```

### `HttpMiddleware` trait

```rust
pub trait HttpMiddleware: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn order(&self) -> i32;        // lower values run closer to the handler
    fn apply(&self, router: Box<dyn Any>) -> Result<Box<dyn Any>, AppError>;
}
```

### `WsHandler` trait

```rust
#[async_trait]
pub trait WsHandler: Send + Sync + 'static {
    async fn on_open(&self, conn_id: u64, sender: WsSender);
    async fn on_message(&self, conn_id: u64, msg: WsMessage);
    async fn on_close(&self, conn_id: u64);
}
```

### `ByteTransform` trait

```rust
pub trait ByteTransform: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn decode(&self, data: Vec<u8>) -> Result<Vec<u8>, AppError>;
    fn encode(&self, data: Vec<u8>) -> Result<Vec<u8>, AppError>;
}
```

---

## Data storage

### `Store` trait

```rust
#[async_trait]
pub trait Store: Send + Sync + 'static {
    async fn connect(&self) -> Result<(), AppError>;
    async fn disconnect(&self) -> Result<(), AppError>;
    async fn health_check(&self) -> Result<bool, AppError>;
}
```

### `DbConnection` trait

```rust
#[async_trait]
pub trait DbConnection: Send + Sync + 'static {
    async fn execute(&self, sql: &str, params: &[DbValue]) -> Result<u64, AppError>;
    async fn query(&self, sql: &str, params: &[DbValue]) -> Result<Vec<DbRow>, AppError>;
    async fn query_one(&self, sql: &str, params: &[DbValue]) -> Result<Option<DbRow>, AppError>;
    async fn begin(&self) -> Result<Box<dyn Transaction>, AppError>;
}
```

### `Repository<E>` trait

```rust
#[async_trait]
pub trait Repository<E: Entity>: Send + Sync {
    async fn find_by_id(&self, id: &E::Id) -> Result<Option<E>, AppError>;
    async fn find_all(&self, opts: QueryOptions) -> Result<Vec<E>, AppError>;
    async fn find_by_spec(&self, spec: &dyn Specification, opts: QueryOptions) -> Result<Vec<E>, AppError>;
    async fn count(&self, spec: &dyn Specification) -> Result<i64, AppError>;
    async fn save(&self, entity: &E) -> Result<(), AppError>;
    async fn delete(&self, id: &E::Id) -> Result<bool, AppError>;
}
```

### Specification pattern

```rust
let spec = EqSpec::new("status", "active")
    .and(LikeSpec::new("name", "%alice%"))
    .and(BetweenSpec::new("age", 18, 30));

let users = repo.find_by_spec(&spec, QueryOptions::default()).await?;
```

---

## Cache

### `Cache` trait

```rust
#[async_trait]
pub trait Cache: Send + Sync + 'static {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, AppError>;
    async fn set(&self, key: &str, value: Vec<u8>) -> Result<(), AppError>;
    async fn set_with_ttl(&self, key: &str, value: Vec<u8>, ttl: Duration) -> Result<(), AppError>;
    async fn delete(&self, key: &str) -> Result<(), AppError>;
    async fn exists(&self, key: &str) -> Result<bool, AppError>;
}
```

---

## Message queue

### `MessageBus` trait

```rust
#[async_trait]
pub trait MessageBus: Send + Sync + 'static {
    async fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), AppError>;
    async fn subscribe(&self, topic: &str) -> Result<MessageReceiverImpl, AppError>;
    async fn unsubscribe(&self, topic: &str) -> Result<(), AppError>;
    async fn health_check(&self) -> Result<bool, AppError> { Ok(true) }
    async fn close(&self) -> Result<(), AppError> { Ok(()) }
}
```

---

## HTTP client

### `HttpClient` trait

```rust
#[async_trait]
pub trait HttpClient: Send + Sync + 'static {
    async fn request(&self, req: HttpRequest) -> Result<HttpResponse, AppError>;
    async fn get(&self, url: &str) -> Result<HttpResponse, AppError>;
    async fn post(&self, url: &str, body: Vec<u8>) -> Result<HttpResponse, AppError>;
}

pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
}

pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}
```

---

## Distributed lock

### `DistributedLock` trait

```rust
#[async_trait]
pub trait DistributedLock: Send + Sync + 'static {
    async fn try_acquire(&self, key: &str, ttl: Duration) -> Result<Option<LockHandle>, AppError>;
    async fn acquire(&self, key: &str, ttl: Duration) -> Result<LockHandle, AppError>;
    async fn release(&self, handle: &LockHandle) -> Result<(), AppError>;
    async fn extend(&self, handle: &LockHandle, ttl: Duration) -> Result<(), AppError>;
}

pub struct LockHandle {
    pub key: String,
    pub token: String,
}
```

---

## Timers

### `TimerEngine` trait

```rust
#[async_trait]
pub trait TimerEngine: Send + Sync + 'static {
    fn name(&self) -> &str;
    async fn schedule(&self, task: TimerTask, handler: Arc<dyn TimerHandler>) -> Result<String, AppError>;
    async fn cancel(&self, task_id: &str) -> Result<(), AppError>;
    async fn list_tasks(&self) -> Vec<TimerTaskInfo>;
    async fn shutdown(&self);
}
```

### `TimerTask`

```rust
TimerTask::new("name", TimerSchedule::interval(Duration::from_secs(60)), handler)
    .scope(TimerScope::Node)       // or TimerScope::Cluster
    .lock_ttl(30)                  // cluster lock TTL (seconds)
```

### `TimerSchedule`

```rust
TimerSchedule::delay(Duration)           // one-shot after delay
TimerSchedule::interval(Duration)        // fixed interval
TimerSchedule::cron("0 */5 * * * *")     // cron expression
```

---

## File storage

### `FileStore` trait

```rust
#[async_trait]
pub trait FileStore: Send + Sync + 'static {
    async fn put(&self, path: &str, data: Vec<u8>) -> Result<(), AppError>;
    async fn get(&self, path: &str) -> Result<Vec<u8>, AppError>;
    async fn delete(&self, path: &str) -> Result<(), AppError>;
    async fn exists(&self, path: &str) -> Result<bool, AppError>;
    async fn list(&self, prefix: &str) -> Result<Vec<FileInfo>, AppError>;
    async fn metadata(&self, path: &str) -> Result<FileInfo, AppError>;
}
```

---

## Audit logging

### `AuditLogger` trait

```rust
#[async_trait]
pub trait AuditLogger: Send + Sync + 'static {
    async fn log(&self, event: AuditEvent) -> Result<(), AppError>;
    async fn query(&self, filter: AuditFilter) -> Result<Vec<AuditEvent>, AppError>;
}
```

### `AuditEvent`

```rust
AuditEvent::new("actor", "action", "resource", AuditOutcome::Success)
    .resource_id("123")
    .ip("192.168.1.1")
    .details(serde_json::json!({ "key": "value" }))
```

---

## Internationalization

### `I18n` trait

```rust
pub trait I18n: Send + Sync + 'static {
    fn t(&self, key: &str, locale: &str) -> String;
    fn t_args(&self, key: &str, locale: &str, args: &[(&str, &str)]) -> String;
    fn available_locales(&self) -> Vec<String>;
    fn default_locale(&self) -> &str;
}
```

---

## Cryptography

### `Crypto` trait

```rust
pub trait Crypto: Send + Sync + 'static {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, AppError>;
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, AppError>;
}
```

---

## Rate limiting

### `RateLimiter` trait

```rust
#[async_trait]
pub trait RateLimiter: Send + Sync + 'static {
    async fn check(&self, key: &str) -> Result<RateLimitResult, AppError>;
    async fn reset(&self, key: &str) -> Result<(), AppError>;
}

pub enum RateLimitResult {
    Allowed,
    Limited,
}
```

---

## Resilience (circuit breaker / retry)

### `CircuitBreaker`

```rust
let breaker = CircuitBreaker::new("name", CircuitBreakerConfig {
    failure_threshold: 5,
    success_threshold: 3,
    open_duration_secs: 30,
});

if breaker.allow_request() {
    match do_work().await {
        Ok(_) => breaker.record_success(),
        Err(_) => breaker.record_failure(),
    }
}
```

### `RetryPolicy`

```rust
let policy = RetryPolicy::new(3, RetryStrategy::Exponential {
    base_ms: 100,
    max_ms: 5000,
});

let result = rinfra_plugins::resilience::with_retry(&policy, || async {
    http_client.get("https://api.example.com").await
}).await;
```

---

## Script engine

### `ScriptEngine` trait

```rust
#[async_trait]
pub trait ScriptEngine: Send + Sync + 'static {
    fn name(&self) -> &str;
    async fn execute(&self, script: &str, input: &str) -> Result<ScriptOutput, AppError>;
}

pub struct ScriptOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}
```

---

## Codecs

### `Codec` trait

```rust
pub trait Codec: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn encode(&self, data: &[u8]) -> Result<Vec<u8>, AppError>;
    fn decode(&self, data: &[u8]) -> Result<Vec<u8>, AppError>;
}
```

---

## Cluster

### `NodeRegistry` trait

```rust
#[async_trait]
pub trait NodeRegistry: Send + Sync + 'static {
    async fn list_nodes(&self) -> Vec<NodeInfo>;
    async fn get_node(&self, id: &str) -> Option<NodeInfo>;
    async fn send_to(&self, node_id: &str, msg: ClusterMessage) -> Result<(), AppError>;
    async fn broadcast(&self, msg: ClusterMessage) -> Result<(), AppError>;
}
```

---

## Hot config reload

### `ConfigWatcher` trait

```rust
#[async_trait]
pub trait ConfigWatcher: Send + Sync + 'static {
    fn add_handler(&self, handler: Arc<dyn OnConfigReload>);
    async fn start(&self) -> Result<(), AppError>;
    async fn stop(&self) -> Result<(), AppError>;
}

#[async_trait]
pub trait OnConfigReload: Send + Sync + 'static {
    fn name(&self) -> &str;
    async fn on_reload(&self, new_config: &RinfraConfig);
}
```

---

## Error handling

### `AppError`

```rust
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
}

// common ErrorCode variants
ErrorCode::Internal
ErrorCode::NotFound
ErrorCode::Unauthorized
ErrorCode::BadRequest
ErrorCode::Forbidden
ErrorCode::Conflict
ErrorCode::RateLimited
ErrorCode::CircuitBreakerOpen
// ... and more
```

In Axum handlers, `AppError` implements `IntoResponse` and returns structured JSON errors.

---

## API responses

### `ApiResponse<T>`

```rust
// success
ApiResponse::ok(data)

// error
ApiResponse::error(&app_error)

// error with i18n
ApiResponse::error_i18n(&app_error, &i18n, "zh-CN")
```

JSON shape:

```json
{
  "success": true,
  "data": { ... },
  "error": null
}

{
  "success": false,
  "data": null,
  "error": {
    "code": "NOT_FOUND",
    "message": "Resource not found"
  }
}
```
