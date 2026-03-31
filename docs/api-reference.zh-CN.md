[English](api-reference.md)

# 核心 API 参考

本文档列出 `rinfra-core` 和 `rinfra-plugins` 中所有公开 trait、struct 和关键函数。

---

## 目录

1. [入口 API](#入口-api)
2. [AppState](#appstate)
3. [Plugin 系统](#plugin-系统)
4. [网络层](#网络层)
5. [数据存储](#数据存储)
6. [缓存](#缓存)
7. [消息队列](#消息队列)
8. [HTTP 客户端](#http-客户端)
9. [分布式锁](#分布式锁)
10. [定时器](#定时器)
11. [文件存储](#文件存储)
12. [审计日志](#审计日志)
13. [国际化](#国际化)
14. [加密](#加密)
15. [限流](#限流)
16. [弹性（熔断/重试）](#弹性熔断重试)
17. [脚本引擎](#脚本引擎)
18. [编解码器](#编解码器)
19. [集群](#集群)
20. [配置热更新](#配置热更新)
21. [健康检查](#健康检查)
22. [错误处理](#错误处理)
23. [API 响应](#api-响应)

---

## 入口 API

### `rinfra_plugins::run(opts: RunOptions)`

框架一站式入口。解析 CLI 参数、加载配置、构建插件、启动所有 listener。

### `RunOptions`

```rust
RunOptions::new()
    .http_router("listener-name", |state| Router)    // HTTP 路由
    .tcp_handler("listener-name", Arc<dyn TcpHandler>) // TCP 处理器
    .ws_handler("listener-name", Arc<dyn WsHandler>)   // WebSocket 处理器
    .grpc_service("listener-name", |router| router)    // gRPC 服务
    .plugin(Box<dyn Plugin>)                            // 自定义插件
    .metadata(vec![("key", "value")])                   // 节点元数据
    .extra_commands(|args, config, format| bool)        // 自定义 CLI 命令
```

### `Application` / `ApplicationBuilder`

底层构建器（`run()` 内部使用，高级用法可直接使用）：

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

所有框架能力的运行时访问入口。通过 Axum `State` 提取器注入。

```rust
pub struct AppState {
    pub config: Arc<RinfraConfig>,   // 完整配置
    pub started_at: Instant,         // 启动时间
}
```

### 便捷访问器

| 方法 | 返回类型 | 对应插件 |
|------|---------|---------|
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
| `health_checkers()` | `Option<&HealthCheckerRegistry>` | 自动 |
| `uptime_secs()` | `u64` | — |

### 泛型类型映射

```rust
// 注册任意类型
state.set::<MyType>(value);

// 读取
let val: Option<&MyType> = state.get::<MyType>();

// 检查
let exists: bool = state.has::<MyType>();
```

---

## Plugin 系统

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

插件 build 阶段的上下文，用于注册组件。

```rust
ctx.config()                        // 读取配置
ctx.set::<T>(value)                 // 注册任意组件
ctx.get::<T>()                      // 读取已注册组件
ctx.set_cache(Arc<dyn Cache>)       // 注册缓存
ctx.set_store(Arc<dyn Store>)       // 注册存储
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
    fn name(&self) -> &str;                  // 探针名
    async fn check(&self) -> HealthCheckResult;
}

// HealthCheckResult
HealthCheckResult::healthy()
HealthCheckResult::unhealthy("reason")
HealthCheckResult::degraded("reason")
```

---

## 网络层

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
    fn order(&self) -> i32;        // 越小越靠近 handler
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

## 数据存储

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

### Specification 模式

```rust
let spec = EqSpec::new("status", "active")
    .and(LikeSpec::new("name", "%alice%"))
    .and(BetweenSpec::new("age", 18, 30));

let users = repo.find_by_spec(&spec, QueryOptions::default()).await?;
```

---

## 缓存

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

## 消息队列

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

## HTTP 客户端

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

## 分布式锁

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

## 定时器

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
    .scope(TimerScope::Node)       // 或 TimerScope::Cluster
    .lock_ttl(30)                  // 集群锁 TTL（秒）
```

### `TimerSchedule`

```rust
TimerSchedule::delay(Duration)           // 延迟一次
TimerSchedule::interval(Duration)        // 重复间隔
TimerSchedule::cron("0 */5 * * * *")     // Cron 表达式
```

---

## 文件存储

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

## 审计日志

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

## 国际化

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

## 加密

### `Crypto` trait

```rust
pub trait Crypto: Send + Sync + 'static {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, AppError>;
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, AppError>;
}
```

---

## 限流

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

## 弹性（熔断/重试）

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

## 脚本引擎

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

## 编解码器

### `Codec` trait

```rust
pub trait Codec: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn encode(&self, data: &[u8]) -> Result<Vec<u8>, AppError>;
    fn decode(&self, data: &[u8]) -> Result<Vec<u8>, AppError>;
}
```

---

## 集群

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

## 配置热更新

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

## 错误处理

### `AppError`

```rust
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
}

// 常用 ErrorCode
ErrorCode::Internal
ErrorCode::NotFound
ErrorCode::Unauthorized
ErrorCode::BadRequest
ErrorCode::Forbidden
ErrorCode::Conflict
ErrorCode::RateLimited
ErrorCode::CircuitBreakerOpen
// ... 更多
```

在 Axum handler 中，`AppError` 自动实现 `IntoResponse`，返回结构化 JSON 错误。

---

## API 响应

### `ApiResponse<T>`

```rust
// 成功
ApiResponse::ok(data)

// 错误
ApiResponse::error(&app_error)

// 带 i18n 错误
ApiResponse::error_i18n(&app_error, &i18n, "zh-CN")
```

JSON 格式：

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
