# rinfra

**模块化、插件化的 Rust 后端基础设施框架。**

提供开箱即用的 HTTP/TCP/WebSocket/gRPC 服务、30+ 内置插件、集群管理和 Admin 控制台。通过 YAML 配置声明式启用所有能力，无需改代码。

[**English**](README.md)

📖 **[完整文档](docs/README.zh-CN.md)** | 🚀 **[快速开始](docs/getting-started.zh-CN.md)** | ⚙️ **[配置参考](docs/configuration.zh-CN.md)** | 🔌 **[插件指南](docs/plugins.zh-CN.md)** | 📋 **[API 参考](docs/api-reference.zh-CN.md)**

---

## 能力矩阵

| 分类 | 能力 | 后端选项 |
|------|------|---------|
| **网络协议** | HTTP / TCP / WebSocket / gRPC / tRPC | Axum, Tonic |
| **HTTP 中间件** | CORS, Auth, 限流, 超时, RequestId, 审计, i18n, OTel | 声明式配置 |
| **TCP 中间件** | 审计, 自定义 | 编程注入 |
| **数据库** | SQL 存储 + ORM (Repository 模式) | PostgreSQL, MySQL, SQLite |
| **缓存** | 单级 / 多级 | Memory (Moka), Redis, L1+L2 |
| **消息队列** | 发布/订阅 + 消费组 | InMemory, NATS, Redis Streams |
| **限流** | 令牌桶 / 滑动窗口 | Memory, Redis |
| **加密** | AES-256-GCM + Key 轮换 | Env / File / Rotating Provider |
| **分布式锁** | try_acquire / acquire / extend | Memory, Redis (SET NX PX) |
| **HTTP 客户端** | 超时 + 重试 + 熔断器 | Reqwest |
| **定时任务** | Delay / Interval / Cron | Node 级 / Cluster 级 |
| **文件存储** | CRUD + 元数据 | 本地文件系统 |
| **审计日志** | 自动 HTTP/TCP 审计 | JSON Lines 文件 |
| **国际化** | 多语言翻译 + HTTP 错误自动翻译 | YAML 文件 |
| **脚本引擎** | WASM 沙箱 / Python / JavaScript | Wasmtime, 子进程 |
| **可观测性** | 日志 + Metrics + Tracing | Tracing, Prometheus, OTLP |
| **健康检查** | /healthz + /readyz (动态探针) | 自动聚合 |
| **配置热更新** | 文件变更检测 + 审计通知 | 文件轮询 |
| **集群** | Master/Worker 模式, 节点注册 | TCP |
| **Admin 面板** | 系统信息, 插件管理, 集群状态 | Vue 3 + Axum |
| **有状态实体** | Actor + Entity + WAL + 异步落库 | rinfra-live (即将开源) |

## 项目结构

```
rinfra/
├── rinfra-core/          # 核心抽象：trait + config + error + AppState
├── rinfra-plugins/       # 实现层：30+ 插件 + Runtime + CLI
├── rinfra-derive/        # 过程宏（Entity/FromRow/ToRow）
├── rinfra-admin/         # Admin 管理面板（前端 Vue 3 + 后端 Axum）
├── rinfra-examples/      # 示例应用（web/gate/game/admin）
├── config/               # 配置文件模板（standalone/master/worker）
├── docs/                 # 框架文档
└── docker-compose.yml    # Docker 部署
```

## 快速开始

### 前置条件

- Rust 1.85+（edition 2024）

### 最小示例

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

创建配置文件（可从 `config/standalone.example.yaml` 复制修改）：

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

### 启用更多能力

只需在 YAML 中加几行，无需改代码：

```yaml
plugins:
  cache:
    memory: { enabled: true }           # → state.cache() 可用
  store:
    postgres:
      enabled: true                      # → state.store() / state.db() 可用
      url: "postgres://localhost/mydb"
  mq:
    backend: memory                      # → state.message_bus() 可用
  lock:
    enabled: true                        # → state.distributed_lock() 可用
  audit:
    enabled: true                        # → HTTP/TCP 请求自动审计
  metrics:
    enabled: true                        # → /metrics 端点
```

详见 [配置参考](docs/configuration.zh-CN.md) 和 [插件指南](docs/plugins.zh-CN.md)。

## 运行测试

```bash
cargo test --workspace
```

## Docker 部署

```bash
# Standalone（单节点 + Postgres + Redis）
docker compose --profile standalone up -d

# Cluster（1 Main + 2 Worker + Postgres）
docker compose --profile cluster up -d
```

## Feature Flags

| Feature | 启用能力 |
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

## 文档

| 文档 | 说明 |
|------|------|
| [docs/README.zh-CN.md](docs/README.zh-CN.md) | 框架总览 |
| [docs/getting-started.zh-CN.md](docs/getting-started.zh-CN.md) | 5 分钟快速开始 |
| [docs/configuration.zh-CN.md](docs/configuration.zh-CN.md) | 所有配置项参考 |
| [docs/plugins.zh-CN.md](docs/plugins.zh-CN.md) | 30+ 插件使用指南 |
| [docs/api-reference.zh-CN.md](docs/api-reference.zh-CN.md) | 核心 Trait/API 参考 |

## License

MIT
