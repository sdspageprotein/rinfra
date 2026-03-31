# rinfra

**A modular, plugin-driven backend infrastructure framework for Rust.**

Batteries-included HTTP / TCP / WebSocket / gRPC services, 30+ built-in plugins, cluster management, and an Admin console — all declaratively configured via YAML, no code changes required.

[**中文文档**](README.zh-CN.md)

📖 **[Documentation](docs/README.md)** | 🚀 **[Getting Started](docs/getting-started.md)** | ⚙️ **[Configuration](docs/configuration.md)** | 🔌 **[Plugins](docs/plugins.md)** | 📋 **[API Reference](docs/api-reference.md)**

---

## Feature Matrix

| Category | Capability | Backend Options |
|----------|-----------|-----------------|
| **Networking** | HTTP / TCP / WebSocket / gRPC / tRPC | Axum, Tonic |
| **HTTP Middleware** | CORS, Auth, Rate-limit, Timeout, RequestId, Audit, i18n, OTel | Declarative config |
| **TCP Middleware** | Audit, Custom | Programmatic injection |
| **Database** | SQL Store + ORM (Repository pattern) | PostgreSQL, MySQL, SQLite |
| **Cache** | Single / Multi-level | Memory (Moka), Redis, L1+L2 |
| **Message Queue** | Pub/Sub + Consumer groups | InMemory, NATS, Redis Streams |
| **Rate Limiting** | Token bucket / Sliding window | Memory, Redis |
| **Encryption** | AES-256-GCM + Key rotation | Env / File / Rotating Provider |
| **Distributed Lock** | try_acquire / acquire / extend | Memory, Redis (SET NX PX) |
| **HTTP Client** | Timeout + Retry + Circuit breaker | Reqwest |
| **Scheduled Tasks** | Delay / Interval / Cron | Node-level / Cluster-level |
| **File Storage** | CRUD + Metadata | Local filesystem |
| **Audit Log** | Automatic HTTP/TCP auditing | JSON Lines file |
| **i18n** | Multi-language translation + HTTP error auto-translation | YAML files |
| **Script Engine** | WASM sandbox / Python / JavaScript | Wasmtime, subprocess |
| **Observability** | Logging + Metrics + Tracing | Tracing, Prometheus, OTLP |
| **Health Check** | /healthz + /readyz (dynamic probes) | Auto-aggregation |
| **Hot Reload** | Config file change detection + audit notification | File polling |
| **Cluster** | Master/Worker mode, node registration | TCP |
| **Admin Panel** | System info, plugin management, cluster status | Vue 3 + Axum |
| **Stateful Entities** | Actor + Entity + WAL + async persistence | rinfra-live (coming soon) |

## Project Structure

```
rinfra/
├── rinfra-core/          # Core abstractions: traits + config + error + AppState
├── rinfra-plugins/       # Implementation: 30+ plugins + Runtime + CLI
├── rinfra-derive/        # Proc macros (Entity / FromRow / ToRow)
├── rinfra-admin/         # Admin panel (Vue 3 frontend + Axum backend)
├── rinfra-examples/      # Example apps (web / gate / game / admin)
├── config/               # Config templates (standalone / master / worker)
├── docs/                 # Documentation
└── docker-compose.yml    # Docker deployment
```

## Getting Started

### Prerequisites

- Rust 1.85+ (edition 2024)

### Minimal Example

```rust
use std::sync::Arc;
use axum::{Json, Router, routing::get};
use rinfra_core::AppState;
use rinfra_plugins::RunOptions;

fn app_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/hello", get(|| async { Json("Hello from rinfra!") }))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    rinfra_plugins::run(
        RunOptions::new().http_router("main", |state| app_router(state)),
    ).await;
}
```

Create a config file (copy and modify from `config/standalone.example.yaml`):

```yaml
app:
  name: "my-server"

plugins:
  log:
    stdout: { enabled: true, level: "info", format: "pretty" }
  net:
    listeners:
      - name: "main"
        protocol: "http"
        bind: "0.0.0.0:8080"
  health:
    enabled: true
```

```bash
cargo run
# → http://localhost:8080/api/hello
# → http://localhost:8080/healthz
```

### Enable More Capabilities

Just add a few lines to YAML — no code changes needed:

```yaml
plugins:
  cache:
    memory: { enabled: true }           # → state.cache() available
  store:
    postgres:
      enabled: true                      # → state.store() / state.db() available
      url: "postgres://localhost/mydb"
  mq:
    backend: memory                      # → state.message_bus() available
  lock:
    enabled: true                        # → state.distributed_lock() available
  audit:
    enabled: true                        # → auto HTTP/TCP request auditing
  metrics:
    enabled: true                        # → /metrics endpoint
```

See [Configuration Reference](docs/configuration.md) and [Plugin Guide](docs/plugins.md) for details.

## Running Tests

```bash
cargo test --workspace
```

## Docker Deployment

```bash
# Standalone (single node + Postgres + Redis)
docker compose --profile standalone up -d

# Cluster (1 Main + 2 Workers + Postgres)
docker compose --profile cluster up -d
```

## Feature Flags

| Feature | Enables |
|---------|---------|
| `mysql` | MySQL Store |
| `sqlite` | SQLite Store |
| `nats` | NATS JetStream MQ |
| `redis-mq` | Redis Streams MQ |
| `telemetry` | OpenTelemetry Tracing |

```toml
[dependencies]
rinfra-plugins = { path = "rinfra-plugins", features = ["mysql", "telemetry"] }
```

## Documentation

| Document | Description |
|----------|-------------|
| [docs/README.md](docs/README.md) | Framework overview |
| [docs/getting-started.md](docs/getting-started.md) | 5-minute quick start |
| [docs/configuration.md](docs/configuration.md) | Full configuration reference |
| [docs/plugins.md](docs/plugins.md) | 30+ plugin usage guide |
| [docs/api-reference.md](docs/api-reference.md) | Core Trait / API reference |

## License

MIT
