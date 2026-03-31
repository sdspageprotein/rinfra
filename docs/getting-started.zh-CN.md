[English](getting-started.md)

# 快速开始

本指南带你在 5 分钟内搭建一个基于 rinfra 的 HTTP 服务。

---

## 前置条件

- Rust 1.85+（edition 2024）
- Cargo

## 第一步：创建项目

```bash
cargo new my-server
cd my-server
```

在 `Cargo.toml` 中添加依赖：

```toml
[dependencies]
rinfra-core = { path = "../rinfra/rinfra-core", features = ["axum"] }
rinfra-plugins = { path = "../rinfra/rinfra-plugins" }
tokio = { version = "1", features = ["full"] }
axum = "0.8"
serde = { version = "1", features = ["derive"] }
```

> 如果 rinfra 发布到私有 registry，替换 `path` 为 `git` 或 `version`。

## 第二步：编写 main.rs

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

## 第三步：创建配置文件

创建配置文件（可从 `config/standalone.example.yaml` 复制修改）：

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

## 第四步：运行

```bash
cargo run
```

访问：
- `http://localhost:8080/api/hello` — 你的业务接口
- `http://localhost:8080/healthz` — 存活探针
- `http://localhost:8080/readyz` — 就绪探针

---

## 进阶：使用 AppState 访问框架能力

`AppState` 是你访问所有框架能力的入口。在 handler 中注入：

```rust
use axum::extract::State;
use rinfra_core::AppState;

async fn my_handler(State(state): State<Arc<AppState>>) -> String {
    // 访问缓存
    if let Some(cache) = state.cache() {
        let _ = cache.set("key", b"value".to_vec()).await;
    }

    // 访问数据库
    if let Some(store) = state.store() {
        let healthy = store.health_check().await.unwrap_or(false);
        return format!("DB healthy: {}", healthy);
    }

    // 访问消息队列
    if let Some(mq) = state.message_bus() {
        let _ = mq.publish("topic", b"hello").await;
    }

    // 访问配置
    let app_name = &state.config.app.name;
    format!("Hello from {}", app_name)
}
```

所有能力都通过 YAML 配置启用，无需改代码：

```yaml
plugins:
  cache:
    memory:
      enabled: true          # 加上这行就自动注入 state.cache()
  store:
    postgres:
      enabled: true          # 加上这行就自动注入 state.store()
      url: "postgres://localhost:5432/mydb"
```

---

## 进阶：添加 TCP 服务

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
        Ok(Some(data)) // 原样回显
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

配置中添加 TCP listener：

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

## 进阶：添加 gRPC 服务

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

## 进阶：自定义插件

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
        // 注册你的组件到 ctx
        // ctx.set::<Arc<dyn MyTrait>>(Arc::new(MyImpl));
        Ok(())
    }
}

// 注入到 Runtime
rinfra_plugins::run(
    RunOptions::new()
        .plugin(Box::new(MyPlugin::new()))
        .http_router("main", |state| app_router(state)),
)
.await;
```

---

## CLI 命令

rinfra 应用自带 CLI：

```bash
# 启动服务（默认）
cargo run

# 查看已注册的插件
cargo run -- plugins

# 查看已注册的 listener
cargo run -- listeners

# 查看 Admin token
cargo run -- admin token

# 指定配置文件
cargo run -- --config config/my-config.yaml

# JSON 输出格式
cargo run -- --format json plugins
```

---

## 下一步

- [配置参考](configuration.md) — 所有配置项详解
- [插件体系](plugins.md) — 30+ 内置插件指南
- [核心 API 参考](api-reference.md) — Trait 和接口一览
