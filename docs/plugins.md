[中文文档](plugins.zh-CN.md)

# Plugin system

All infrastructure capabilities in rinfra are provided as plugins. Plugins are built automatically at application startup and are enabled or disabled according to YAML configuration.

---

## Plugin lifecycle

```
Application::build()
    │
    ▼
 builtin_plugins() → 30+ builtin plugin list
    │
    ▼
 PluginRegistry::build_all()
    │  for plugin in plugins:
    │    plugin.build(ctx)        ← read config, create components, register on PluginContext
    │
    ▼
 PluginContext::into_app_parts()
    │  extensions → AppState     ← inject all components into AppState
    │
    ▼
 Application::run()             ← start listeners, timers, etc.
    │
    ▼
 shutdown()
    │  PluginRegistry::shutdown_all()  ← run shutdown hooks in reverse order
```

---

## Plugin reference

### Data store

| Plugin | AppState accessor | Backend | Feature |
|--------|-------------------|---------|---------|
| `store.postgres` | `state.store()` / `state.db()` | PostgreSQL (sqlx) | — |
| `store.mysql` | `state.store()` / `state.db()` | MySQL (sqlx) | `mysql` |
| `store.sqlite` | `state.store()` / `state.db()` | SQLite (sqlx) | `sqlite` |

```rust
// Use Store trait
let store = state.store().unwrap();
let healthy = store.health_check().await?;

// Use DbConnection for raw SQL
let db = state.db().unwrap();
let rows = db.query("SELECT * FROM users WHERE id = $1", &[&user_id]).await?;
```

### Cache

| Plugin | AppState accessor | Backend |
|--------|-------------------|---------|
| `cache.memory` | `state.cache()` | Moka in-process cache |
| `cache.redis` | `state.cache()` | Redis |
| `cache.multilevel` | `state.cache()` | L1(Memory) + L2(Redis) |

```rust
let cache = state.cache().unwrap();
cache.set("key", b"value".to_vec()).await?;
cache.set_with_ttl("key", data, Duration::from_secs(60)).await?;
let val = cache.get("key").await?;
cache.delete("key").await?;
```

### Message queue

| Plugin | AppState accessor | Backend | Feature |
|--------|-------------------|---------|---------|
| `mq.memory` | `state.message_bus()` | In-process MPSC | — |
| `mq.nats` | `state.message_bus()` | NATS JetStream | `nats` |
| `mq.redis_streams` | `state.message_bus()` | Redis Streams | `redis-mq` |

```rust
let mq = state.message_bus().unwrap();

// Publish
mq.publish("user.created", serde_json::to_vec(&event)?).await?;

// Subscribe
let mut receiver = mq.subscribe("user.created").await?;
tokio::spawn(async move {
    while let Some(msg) = receiver.recv().await {
        println!("received: {:?}", msg);
    }
});
```

### Rate limiting

| Plugin | AppState accessor | Backend |
|--------|-------------------|---------|
| `ratelimit.memory` | `state.ratelimiter()` | Local token bucket |
| `ratelimit.redis` | `state.ratelimiter()` | Redis sliding window |

```rust
let limiter = state.ratelimiter().unwrap();
match limiter.check("user:123").await? {
    RateLimitResult::Allowed => { /* allow */ }
    RateLimitResult::Limited => { /* rate limited */ }
}
```

### Crypto

| Plugin | AppState accessor | Description |
|--------|-------------------|-------------|
| `crypto.aesgcm` | `state.crypto()` | AES-256-GCM encrypt/decrypt |
| `crypto.env_key` | Internal injection | Environment variable key provider |
| `crypto.file_key` | Internal injection | File key provider |
| `crypto.rotating` | Internal injection | Automatic key rotation |

```rust
let crypto = state.crypto().unwrap();
let encrypted = crypto.encrypt(b"sensitive data")?;
let decrypted = crypto.decrypt(&encrypted)?;
```

### HTTP client

| Plugin | AppState accessor | Description |
|--------|-------------------|-------------|
| `http_client` | `state.http_client()` | reqwest + circuit breaker |

```rust
let client = state.http_client().unwrap();
let resp = client.get("https://api.example.com/data").await?;
println!("status: {}, body len: {}", resp.status, resp.body.len());
```

### Distributed lock

| Plugin | AppState accessor | Backend |
|--------|-------------------|---------|
| `lock` (memory) | `state.distributed_lock()` | In-process mutex |
| `lock` (redis) | `state.distributed_lock()` | Redis SET NX PX |

```rust
let lock = state.distributed_lock().unwrap();
let handle = lock.acquire("my-resource", Duration::from_secs(10)).await?;
// ... critical section ...
lock.release(&handle).await?;
```

### Timer

| Plugin | AppState accessor | Description |
|--------|-------------------|-------------|
| `timer.simple` | Via `TimerEngine` | Supports Delay / Interval / Cron |

```rust
use rinfra_core::timer::*;

// Register scheduled task in plugin build
let engine: &Arc<dyn TimerEngine> = /* from ctx */;
engine.schedule(
    TimerTask::new("cleanup", TimerSchedule::cron("0 */5 * * * *"), handler)
        .scope(TimerScope::Cluster)  // run once per cluster
).await?;
```

**TimerScope**:
- `Node` — each node runs independently (default)
- `Cluster` — only one node in the cluster runs the task (requires distributed lock)

### File storage

| Plugin | AppState accessor | Backend |
|--------|-------------------|---------|
| `file_store.local` | `state.file_store()` | Local filesystem |

```rust
let fs = state.file_store().unwrap();
fs.put("uploads/avatar.png", data).await?;
let content = fs.get("uploads/avatar.png").await?;
let files = fs.list("uploads/").await?;
```

### Audit log

| Plugin | AppState accessor | Backend |
|--------|-------------------|---------|
| `audit.file` | `state.audit_logger()` | JSON Lines file |

```rust
use rinfra_core::audit::*;

let logger = state.audit_logger().unwrap();
logger.log(
    AuditEvent::new("user:123", "user.delete", "user", AuditOutcome::Success)
        .details(serde_json::json!({ "reason": "requested" }))
).await?;
```

**Automatic integration**: When enabled, HTTP requests and TCP connect/disconnect events are recorded to the audit log automatically.

### Internationalization (i18n)

| Plugin | AppState accessor | Backend |
|--------|-------------------|---------|
| `i18n.file` | `state.i18n()` | YAML translation files |

```rust
let i18n = state.i18n().unwrap();
let msg = i18n.t("error.not_found", "zh-CN");
let msg = i18n.t_args("welcome", "en", &[("name", "Alice")]);
```

**Automatic integration**: When enabled, HTTP error responses are translated using the `Accept-Language` header.

Translation file layout:
```
i18n/
├── en.yaml      # error.not_found: "Not Found"
├── zh-CN.yaml   # error.not_found: "未找到"
└── ja.yaml      # error.not_found: "見つかりません"
```

### Hot config reload

| Plugin | AppState accessor | Description |
|--------|-------------------|-------------|
| `config_watch` | `state.config_watcher()` | File polling for changes |

When enabled, the following handlers are registered automatically:
- `LogConfigReloadHandler` — reload logging configuration
- `AuditConfigReloadHandler` — reload audit configuration (if audit is enabled)

### Script engines

| Plugin | AppState accessor | Runtime |
|--------|-------------------|---------|
| `script.wasm` | `state.script_engines()` | Wasmtime sandbox |
| `script.python` | `state.script_engines()` | Python subprocess |
| `script.js` | `state.script_engines()` | Node.js subprocess |

```rust
let engines = state.script_engines().unwrap();
let py = engines.get("python").unwrap();
let output = py.execute("scripts/process.py", r#"{"data": 42}"#).await?;
println!("stdout: {}, exit: {}", output.stdout, output.exit_code);
```

### Codecs

| Plugin | Description |
|--------|-------------|
| `codec.json` | JSON encode/decode |
| `codec.msgpack` | MessagePack encode/decode |
| `codec.protobuf` | Protobuf encode/decode |

```rust
let codecs = state.codecs().unwrap();
let json = codecs.get("json").unwrap();
let bytes = json.encode(&data)?;
let decoded = json.decode::<MyStruct>(&bytes)?;
```

### Observability

| Plugin | Description |
|--------|-------------|
| `metrics` | Prometheus metrics (`/metrics`) |
| `telemetry` | OpenTelemetry distributed tracing (OTLP) |
| `health` | Health endpoints (`/healthz`, `/readyz`) |

**Health checks**: `/readyz` aggregates health from all registered components (Store, Cache, MQ, etc.).

### Network middleware

**HTTP middleware** (declarative config, applied automatically):

| Name | Role |
|------|------|
| `cors` | Cross-origin resource sharing |
| `request_id` | Auto-generate `X-Request-Id` |
| `timeout` | Request timeout |
| `auth` | JWT authentication |
| `rate_limit` | Rate limiting |
| `trace` | Request tracing (tracing span) |
| `otel_propagation` | OpenTelemetry context propagation |
| `audit` | HTTP request audit (automatic) |
| `i18n_error` | Localized error messages (automatic) |

**TCP middleware** (injected by plugins):

| Name | Role |
|------|------|
| `audit` | TCP connect/disconnect audit (automatic) |

Customize via `ApplicationBuilder::tcp_middleware()`.

---

## Custom plugins

Implement the `Plugin` trait:

```rust
use rinfra_core::plugin::{Plugin, PluginContext, PluginManifest};

struct MyPlugin { manifest: PluginManifest }

#[async_trait]
impl Plugin for MyPlugin {
    fn manifest(&self) -> &PluginManifest { &self.manifest }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        let cfg = ctx.config();
        // Create component
        let my_service = Arc::new(MyServiceImpl::new());
        ctx.set::<Arc<dyn MyService>>(my_service);

        // Register health check
        ctx.add_health_checker(Arc::new(MyHealthChecker::new()));

        // Register shutdown hook
        ctx.add_shutdown_hook(|| async { /* cleanup */ Ok(()) });

        Ok(())
    }
}
```

Register with the runtime:

```rust
rinfra_plugins::run(
    RunOptions::new().plugin(Box::new(MyPlugin::new()))
).await;
```

---

## Plugin execution order

Builtin plugins are ordered by dependency. Approximate order:

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
16. User-defined plugins

Plugins added with `RunOptions::plugin()` run after all builtin plugins.
