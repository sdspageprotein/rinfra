[English](plugins.md)

# 插件体系

rinfra 的所有基础设施能力都以插件形式提供。插件在应用启动时自动构建，根据 YAML 配置决定是否启用。

---

## 插件生命周期

```
Application::build()
    │
    ▼
 builtin_plugins() → 30+ 内置插件列表
    │
    ▼
 PluginRegistry::build_all()
    │  for plugin in plugins:
    │    plugin.build(ctx)        ← 读取配置，创建组件，注册到 PluginContext
    │
    ▼
 PluginContext::into_app_parts()
    │  extensions → AppState     ← 所有组件注入到 AppState
    │
    ▼
 Application::run()             ← 启动监听器、定时器等
    │
    ▼
 shutdown()
    │  PluginRegistry::shutdown_all()  ← 逆序执行 shutdown hooks
```

---

## 插件一览

### 数据存储

| 插件 | AppState 访问器 | 后端 | Feature |
|------|-----------------|------|---------|
| `store.postgres` | `state.store()` / `state.db()` | PostgreSQL (sqlx) | — |
| `store.mysql` | `state.store()` / `state.db()` | MySQL (sqlx) | `mysql` |
| `store.sqlite` | `state.store()` / `state.db()` | SQLite (sqlx) | `sqlite` |

```rust
// 使用 Store trait
let store = state.store().unwrap();
let healthy = store.health_check().await?;

// 使用 DbConnection 执行原生 SQL
let db = state.db().unwrap();
let rows = db.query("SELECT * FROM users WHERE id = $1", &[&user_id]).await?;
```

### 缓存

| 插件 | AppState 访问器 | 后端 |
|------|-----------------|------|
| `cache.memory` | `state.cache()` | Moka 本地缓存 |
| `cache.redis` | `state.cache()` | Redis |
| `cache.multilevel` | `state.cache()` | L1(Memory) + L2(Redis) |

```rust
let cache = state.cache().unwrap();
cache.set("key", b"value".to_vec()).await?;
cache.set_with_ttl("key", data, Duration::from_secs(60)).await?;
let val = cache.get("key").await?;
cache.delete("key").await?;
```

### 消息队列

| 插件 | AppState 访问器 | 后端 | Feature |
|------|-----------------|------|---------|
| `mq.memory` | `state.message_bus()` | 进程内 MPSC | — |
| `mq.nats` | `state.message_bus()` | NATS JetStream | `nats` |
| `mq.redis_streams` | `state.message_bus()` | Redis Streams | `redis-mq` |

```rust
let mq = state.message_bus().unwrap();

// 发布
mq.publish("user.created", serde_json::to_vec(&event)?).await?;

// 订阅
let mut receiver = mq.subscribe("user.created").await?;
tokio::spawn(async move {
    while let Some(msg) = receiver.recv().await {
        println!("received: {:?}", msg);
    }
});
```

### 限流

| 插件 | AppState 访问器 | 后端 |
|------|-----------------|------|
| `ratelimit.memory` | `state.ratelimiter()` | 本地令牌桶 |
| `ratelimit.redis` | `state.ratelimiter()` | Redis 滑动窗口 |

```rust
let limiter = state.ratelimiter().unwrap();
match limiter.check("user:123").await? {
    RateLimitResult::Allowed => { /* 放行 */ }
    RateLimitResult::Limited => { /* 限流 */ }
}
```

### 加密

| 插件 | AppState 访问器 | 说明 |
|------|-----------------|------|
| `crypto.aesgcm` | `state.crypto()` | AES-256-GCM 加解密 |
| `crypto.env_key` | 内部注入 | 环境变量 Key Provider |
| `crypto.file_key` | 内部注入 | 文件 Key Provider |
| `crypto.rotating` | 内部注入 | Key 自动轮换 |

```rust
let crypto = state.crypto().unwrap();
let encrypted = crypto.encrypt(b"sensitive data")?;
let decrypted = crypto.decrypt(&encrypted)?;
```

### HTTP 客户端

| 插件 | AppState 访问器 | 说明 |
|------|-----------------|------|
| `http_client` | `state.http_client()` | reqwest + 熔断器 |

```rust
let client = state.http_client().unwrap();
let resp = client.get("https://api.example.com/data").await?;
println!("status: {}, body len: {}", resp.status, resp.body.len());
```

### 分布式锁

| 插件 | AppState 访问器 | 后端 |
|------|-----------------|------|
| `lock` (memory) | `state.distributed_lock()` | 进程内 Mutex |
| `lock` (redis) | `state.distributed_lock()` | Redis SET NX PX |

```rust
let lock = state.distributed_lock().unwrap();
let handle = lock.acquire("my-resource", Duration::from_secs(10)).await?;
// ... 临界区 ...
lock.release(&handle).await?;
```

### 定时器

| 插件 | AppState 访问器 | 说明 |
|------|-----------------|------|
| `timer.simple` | 通过 `TimerEngine` | 支持 Delay / Interval / Cron |

```rust
use rinfra_core::timer::*;

// 在插件 build 中注册定时任务
let engine: &Arc<dyn TimerEngine> = /* from ctx */;
engine.schedule(
    TimerTask::new("cleanup", TimerSchedule::cron("0 */5 * * * *"), handler)
        .scope(TimerScope::Cluster)  // 集群内只执行一次
).await?;
```

**TimerScope**：
- `Node` — 每个节点独立执行（默认）
- `Cluster` — 集群内只有一个节点执行（需要分布式锁）

### 文件存储

| 插件 | AppState 访问器 | 后端 |
|------|-----------------|------|
| `file_store.local` | `state.file_store()` | 本地文件系统 |

```rust
let fs = state.file_store().unwrap();
fs.put("uploads/avatar.png", data).await?;
let content = fs.get("uploads/avatar.png").await?;
let files = fs.list("uploads/").await?;
```

### 审计日志

| 插件 | AppState 访问器 | 后端 |
|------|-----------------|------|
| `audit.file` | `state.audit_logger()` | JSON Lines 文件 |

```rust
use rinfra_core::audit::*;

let logger = state.audit_logger().unwrap();
logger.log(
    AuditEvent::new("user:123", "user.delete", "user", AuditOutcome::Success)
        .details(serde_json::json!({ "reason": "requested" }))
).await?;
```

**自动集成**：启用后，HTTP 请求和 TCP 连接/断开会自动记录审计日志。

### 国际化 (i18n)

| 插件 | AppState 访问器 | 后端 |
|------|-----------------|------|
| `i18n.file` | `state.i18n()` | YAML 翻译文件 |

```rust
let i18n = state.i18n().unwrap();
let msg = i18n.t("error.not_found", "zh-CN");
let msg = i18n.t_args("welcome", "en", &[("name", "Alice")]);
```

**自动集成**：启用后，HTTP 错误响应会根据 `Accept-Language` 头自动翻译。

翻译文件结构：
```
i18n/
├── en.yaml      # error.not_found: "Not Found"
├── zh-CN.yaml   # error.not_found: "未找到"
└── ja.yaml      # error.not_found: "見つかりません"
```

### 配置热更新

| 插件 | AppState 访问器 | 说明 |
|------|-----------------|------|
| `config_watch` | `state.config_watcher()` | 文件轮询检测变更 |

启用后自动注册以下 handler：
- `LogConfigReloadHandler` — 日志记录配置变更
- `AuditConfigReloadHandler` — 审计记录配置变更（若 audit 启用）

### 脚本引擎

| 插件 | AppState 访问器 | 运行时 |
|------|-----------------|--------|
| `script.wasm` | `state.script_engines()` | Wasmtime 沙箱 |
| `script.python` | `state.script_engines()` | Python 子进程 |
| `script.js` | `state.script_engines()` | Node.js 子进程 |

```rust
let engines = state.script_engines().unwrap();
let py = engines.get("python").unwrap();
let output = py.execute("scripts/process.py", r#"{"data": 42}"#).await?;
println!("stdout: {}, exit: {}", output.stdout, output.exit_code);
```

### 编解码器

| 插件 | 说明 |
|------|------|
| `codec.json` | JSON 编解码 |
| `codec.msgpack` | MessagePack 编解码 |
| `codec.protobuf` | Protobuf 编解码 |

```rust
let codecs = state.codecs().unwrap();
let json = codecs.get("json").unwrap();
let bytes = json.encode(&data)?;
let decoded = json.decode::<MyStruct>(&bytes)?;
```

### 可观测性

| 插件 | 说明 |
|------|------|
| `metrics` | Prometheus 指标暴露 (`/metrics`) |
| `telemetry` | OpenTelemetry 分布式追踪 (OTLP) |
| `health` | 健康检查端点 (`/healthz`, `/readyz`) |

**健康检查**：`/readyz` 自动聚合所有已注册组件的健康状态（Store、Cache、MQ 等）。

### 网络中间件

**HTTP 中间件**（声明式配置，自动应用）：

| 名称 | 功能 |
|------|------|
| `cors` | 跨域资源共享 |
| `request_id` | 自动生成 X-Request-Id |
| `timeout` | 请求超时 |
| `auth` | JWT 认证 |
| `rate_limit` | 限流 |
| `trace` | 请求追踪 (tracing span) |
| `otel_propagation` | OpenTelemetry 上下文传播 |
| `audit` | HTTP 请求审计（自动） |
| `i18n_error` | 错误消息国际化（自动） |

**TCP 中间件**（插件自动注入）：

| 名称 | 功能 |
|------|------|
| `audit` | TCP 连接/断开审计（自动） |

可通过 `ApplicationBuilder::tcp_middleware()` 自定义。

---

## 自定义插件

实现 `Plugin` trait：

```rust
use rinfra_core::plugin::{Plugin, PluginContext, PluginManifest};

struct MyPlugin { manifest: PluginManifest }

#[async_trait]
impl Plugin for MyPlugin {
    fn manifest(&self) -> &PluginManifest { &self.manifest }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = ctx.config();
        // 创建组件
        let my_service = Arc::new(MyServiceImpl::new());
        ctx.set::<Arc<dyn MyService>>(my_service);

        // 注册健康检查
        ctx.add_health_checker(Arc::new(MyHealthChecker::new()));

        // 注册关闭钩子
        ctx.add_shutdown_hook(|| async { /* cleanup */ Ok(()) });

        Ok(())
    }
}
```

注册到运行时：

```rust
rinfra_plugins::run(
    RunOptions::new().plugin(Box::new(MyPlugin::new()))
).await;
```

---

## 插件执行顺序

内置插件按依赖关系排序。大致顺序：

1. Codec (JSON, Msgpack, Protobuf)
2. Cache (Memory → Redis → Multilevel)
3. MQ (InMemory / NATS / Redis Streams)
4. Store (Postgres / MySQL / SQLite)
5. Rate Limiter
6. Crypto (Key Providers → AES-GCM)
7. Script Engines
8. gRPC / tRPC
9. FileStore
10. HttpClient (+ CircuitBreaker)
11. Lock (Memory / Redis)
12. Timer (after Lock, for cluster-safe scheduling)
13. Audit
14. i18n
15. ConfigWatch (after Audit + i18n)
16. 用户自定义插件

用户通过 `RunOptions::plugin()` 注入的插件在内置插件之后执行。
