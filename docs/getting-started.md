[中文文档](getting-started.zh-CN.md)

# Getting Started

This guide walks you through setting up an rinfra-based HTTP service in about five minutes.

---

## Prerequisites

- Rust 1.85+ (edition 2024)
- Cargo

## Step 1: Create a project

```bash
cargo new my-server
cd my-server
```

Add dependencies in `Cargo.toml`:

```toml
[dependencies]
rinfra-core = { path = "../rinfra/rinfra-core", features = ["axum"] }
rinfra-plugins = { path = "../rinfra/rinfra-plugins" }
tokio = { version = "1", features = ["full"] }
axum = "0.8"
serde = { version = "1", features = ["derive"] }
```

> If rinfra is published to a private registry, replace `path` with `git` or `version`.

## Step 2: Write main.rs

```rust
use std::sync::Arc;
use axum::{Json, Router, routing::get};
use rinfra_core::AppState;
use rinfra_plugins::RunOptions;
use serde::Serialize;

#[derive(Serialize)]
struct HelloResponse {
    message: String,
}

async fn hello() -> Json<HelloResponse> {
    Json(HelloResponse {
        message: "Hello from rinfra!".into(),
    })
}

fn app_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/hello", get(hello))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    rinfra_plugins::run(
        RunOptions::new()
            .http_router("main", |state| app_router(state)),
    )
    .await;
}
```

## Step 3: Create a configuration file

Create a configuration file (you can copy and adapt `config/standalone.example.yaml`):

```yaml
app:
  name: "my-server"
  version: "0.1.0"

runtime:
  shutdown:
    grace_period_secs: 10

plugins:
  log:
    stdout:
      enabled: true
      level: "info"
      format: "pretty"

  net:
    listeners:
      - name: "main"
        protocol: "http"
        bind: "0.0.0.0:8080"

  health:
    enabled: true
```

## Step 4: Run

```bash
cargo run
```

Open:

- `http://localhost:8080/api/hello` — your application API
- `http://localhost:8080/healthz` — liveness probe
- `http://localhost:8080/readyz` — readiness probe

---

## Going further: Use AppState to access framework capabilities

`AppState` is the entry point for all framework capabilities. Inject it in your handler:

```rust
use axum::extract::State;
use rinfra_core::AppState;

async fn my_handler(State(state): State<Arc<AppState>>) -> String {
    // Access cache
    if let Some(cache) = state.cache() {
        let _ = cache.set("key", b"value".to_vec()).await;
    }

    // Access database
    if let Some(store) = state.store() {
        let healthy = store.health_check().await.unwrap_or(false);
        return format!("DB healthy: {}", healthy);
    }

    // Access message queue
    if let Some(mq) = state.message_bus() {
        let _ = mq.publish("topic", b"hello").await;
    }

    // Access configuration
    let app_name = &state.config.app.name;
    format!("Hello from {}", app_name)
}
```

All capabilities are enabled via YAML configuration—no code changes required:

```yaml
plugins:
  cache:
    memory:
      enabled: true          # Enable this line to auto-inject state.cache()
  store:
    postgres:
      enabled: true          # Enable this line to auto-inject state.store()
      url: "postgres://localhost:5432/mydb"
```

---

## Going further: Add a TCP service

```rust
use rinfra_core::net::tcp::{TcpContext, TcpHandler};
use rinfra_core::error::AppError;
use async_trait::async_trait;

struct EchoHandler;

#[async_trait]
impl TcpHandler for EchoHandler {
    async fn on_message(
        &self,
        _ctx: &TcpContext,
        data: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, AppError> {
        Ok(Some(data)) // Echo unchanged
    }
}

#[tokio::main]
async fn main() {
    rinfra_plugins::run(
        RunOptions::new()
            .http_router("main", |state| app_router(state))
            .tcp_handler("game-tcp", Arc::new(EchoHandler)),
    )
    .await;
}
```

Add a TCP listener in configuration:

```yaml
plugins:
  net:
    listeners:
      - name: "main"
        protocol: "http"
        bind: "0.0.0.0:8080"
      - name: "game-tcp"
        protocol: "tcp"
        bind: "0.0.0.0:9100"
        tcp:
          max_frame_size: 65536
```

---

## Going further: Add a gRPC service

```rust
rinfra_plugins::run(
    RunOptions::new()
        .grpc_service("game-grpc", |router| {
            router.add_service(MyServiceServer::new(MyServiceImpl))
        }),
)
.await;
```

```yaml
plugins:
  net:
    listeners:
      - name: "game-grpc"
        protocol: "grpc"
        bind: "0.0.0.0:9090"
```

---

## Going further: Custom plugins

```rust
use rinfra_core::plugin::{Plugin, PluginContext, PluginManifest};
use rinfra_core::error::AppError;
use async_trait::async_trait;

struct MyPlugin {
    manifest: PluginManifest,
}

impl MyPlugin {
    fn new() -> Self {
        Self {
            manifest: PluginManifest::new("my-plugin", "1.0.0", "My custom plugin"),
        }
    }
}

#[async_trait]
impl Plugin for MyPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn build(&self, ctx: &mut PluginContext) -> Result<(), AppError> {
        // Register your components on ctx
        // ctx.set::<Arc<dyn MyTrait>>(Arc::new(MyImpl));
        Ok(())
    }
}

// Inject into Runtime
rinfra_plugins::run(
    RunOptions::new()
        .plugin(Box::new(MyPlugin::new()))
        .http_router("main", |state| app_router(state)),
)
.await;
```

---

## CLI commands

rinfra applications include a built-in CLI:

```bash
# Start the server (default)
cargo run

# List registered plugins
cargo run -- plugins

# List registered listeners
cargo run -- listeners

# Show Admin token
cargo run -- admin token

# Use a specific config file
cargo run -- --config config/my-config.yaml

# JSON output format
cargo run -- --format json plugins
```

---

## Next steps

- [Configuration reference](configuration.md) — detailed explanation of all options
- [Plugin system](plugins.md) — guide to 30+ built-in plugins
- [Core API reference](api-reference.md) — overview of traits and interfaces
