[English](configuration.md)

# 配置参考

rinfra 使用 YAML 配置文件驱动所有行为。默认路径 `config/standalone.example.yaml`，可通过 `--config` 参数指定。

支持三种部署模式的配置模板：

| 文件 | 模式 | 说明 |
|------|------|------|
| `config/standalone.example.yaml` | 单机 | 不加入集群，独立运行 |
| `config/master.example.yaml` | 集群主节点 | 管理 worker 注册和任务分发 |
| `config/worker.example.yaml` | 集群工作节点 | 连接 master 接收指令 |

---

## 顶层结构

```yaml
app:                    # 应用元数据
runtime:                # 运行时行为
plugins:                # 插件配置（所有能力在此声明）
business:               # 业务自定义配置（任意 key-value）
```

---

## app — 应用元数据

```yaml
app:
  name: "my-server"     # 应用名称，显示在日志和 Admin 面板
  version: "0.1.0"      # 应用版本
```

---

## runtime — 运行时

```yaml
runtime:
  shutdown:
    grace_period_secs: 30       # 优雅关闭总超时（秒）
    component_timeout_secs: 10  # 单个组件关闭超时（秒）
```

---

## plugins — 插件配置

### plugins.log — 日志

```yaml
plugins:
  log:
    stdout:
      enabled: true          # 启用控制台日志
      level: "info"          # trace | debug | info | warn | error
      format: "pretty"       # pretty | json | compact
    file:
      enabled: false         # 启用文件日志
      path: "logs"           # 日志目录
      filename: "app.log"    # 文件名
      rotation: "daily"      # daily | hourly | never
      max_files: 7           # 保留文件数
      level: "info"
      format: "json"
```

### plugins.net — 网络监听器

```yaml
plugins:
  net:
    listeners:
      - name: "main"             # 监听器名称（唯一标识）
        protocol: "http"         # http | tcp | grpc | trpc
        bind: "0.0.0.0:8080"     # 绑定地址
        http:                    # HTTP 专属配置
          middleware:            # 中间件（声明式）
            cors:
              enabled: true
              allow_origins: ["*"]
              allow_methods: ["GET", "POST", "PUT", "DELETE"]
              allow_headers: ["*"]
              max_age_secs: 86400
            request_id:
              enabled: true      # 自动生成 X-Request-Id
            timeout:
              enabled: true
              timeout_secs: 30   # 请求超时
            auth:
              enabled: false
              jwt_secret_env: "RINFRA_JWT_SECRET"
              exclude_paths: ["/healthz", "/readyz", "/metrics"]
            rate_limit:
              enabled: false
              key_strategy: "ip"  # ip | header
          ws:                    # WebSocket 配置
            enabled: false
            ping_interval_secs: 30
            ping_timeout_secs: 10

      - name: "game-tcp"         # TCP 监听器
        protocol: "tcp"
        bind: "0.0.0.0:9100"
        tcp:
          max_frame_size: 65536   # 最大帧大小（字节）
          pipeline:               # 字节变换管道（可选）
            - transform: "lz4"   # 内置 LZ4 压缩

      - name: "game-grpc"        # gRPC 监听器
        protocol: "grpc"
        bind: "0.0.0.0:9090"
```

### plugins.store — 数据库

```yaml
plugins:
  store:
    postgres:
      enabled: false
      url: "postgres://user:pass@localhost:5432/mydb"
      max_connections: 10
      idle_timeout_secs: 300
      required: false          # true = 连接失败则启动失败
    # mysql:                   # 需要 feature = "mysql"
    #   enabled: false
    #   url: "mysql://user:pass@localhost:3306/mydb"
    # sqlite:                  # 需要 feature = "sqlite"
    #   enabled: false
    #   path: "data/app.db"
```

### plugins.cache — 缓存

```yaml
plugins:
  cache:
    memory:
      enabled: true
      max_capacity: 10000      # 最大条目数
      ttl_secs: 300            # 默认 TTL（秒）
    redis:
      enabled: false
      url: "redis://127.0.0.1:6379"
      required: false
    multilevel:
      enabled: false           # 需要 memory + redis 同时启用
      l1_max_capacity: 10000
      l1_ttl_secs: 60
```

### plugins.mq — 消息队列

```yaml
plugins:
  mq:
    backend: memory            # memory | nats | redis_streams | none
    memory:
      channel_capacity: 1024
    nats:                      # 需要 feature = "nats"
      url: "nats://localhost:4222"
      stream_name: rinfra
      consumer_group: rinfra-workers
    redis_streams:             # 需要 feature = "redis-mq"
      url: "redis://localhost:6379"
      group_name: rinfra-workers
```

### plugins.ratelimit — 限流

```yaml
plugins:
  ratelimit:
    memory:
      enabled: false
      requests_per_second: 100
      burst_size: 200
    redis:
      enabled: false
      url: "redis://127.0.0.1:6379"
      requests_per_second: 100
      burst_size: 200
      window_secs: 1
```

### plugins.crypto — 加密

```yaml
plugins:
  crypto:
    aesgcm:
      enabled: false
      key_env_var: "RINFRA_CRYPTO_KEY"   # 从环境变量读取密钥
    rotating:
      enabled: false
      rotation_interval_secs: 86400
      max_key_versions: 5
    file:
      enabled: false
      path: "keys/master.key"
```

### plugins.lock — 分布式锁

```yaml
plugins:
  lock:
    enabled: false
    backend: memory            # memory | redis
    redis:
      url: "redis://127.0.0.1:6379"
      key_prefix: "rinfra:lock:"
```

### plugins.timer — 定时器

```yaml
plugins:
  timer:
    enabled: false
    engine: simple
    simple:
      max_concurrent: 4        # 最大并发定时任务数
```

### plugins.file_store — 文件存储

```yaml
plugins:
  file_store:
    enabled: false
    backend: local
    local:
      root_dir: "data/files"
```

### plugins.http_client — HTTP 客户端

```yaml
plugins:
  http_client:
    enabled: false
    timeout_secs: 30
    user_agent: "rinfra/0.1.0"
    max_retries: 0             # 自动重试次数
    retry_delay_ms: 500        # 重试间隔（毫秒）
```

### plugins.config_watch — 配置热更新

```yaml
plugins:
  config_watch:
    enabled: false
    poll_interval_secs: 5      # 轮询间隔
```

### plugins.audit — 审计日志

```yaml
plugins:
  audit:
    enabled: false
    backend: file
    file:
      path: "logs/audit.jsonl"  # JSON Lines 格式
```

### plugins.i18n — 国际化

```yaml
plugins:
  i18n:
    enabled: false
    dir: "i18n"                # YAML 翻译文件目录
    default_locale: "en"       # 默认语言
```

### plugins.metrics — Prometheus 指标

```yaml
plugins:
  metrics:
    enabled: false
    endpoint: "/metrics"       # 指标暴露端点
```

### plugins.health — 健康检查

```yaml
plugins:
  health:
    enabled: true              # 提供 /healthz 和 /readyz 端点
```

### plugins.telemetry — OpenTelemetry

```yaml
plugins:
  telemetry:
    enabled: false             # 需要 feature = "telemetry"
    otlp_endpoint: "http://localhost:4317"
    sample_ratio: 1.0
    export_timeout_secs: 10
```

### plugins.admin — 管理面板

```yaml
plugins:
  admin:
    enabled: true
    static_dir: "rinfra-admin/frontend/dist"
    auth:
      enabled: false           # 生产环境务必开启
      token_file: "data/admin_tokens.json"
      root_token_env: "RINFRA_ROOT_TOKEN"
      exclude_paths:
        - "/api/admin/health"
```

### plugins.script — 脚本引擎

```yaml
plugins:
  script:
    wasm:
      enabled: false
      timeout_secs: 30
      fuel_limit: 1000000
    python:
      enabled: false
      work_dir: "scripts/python"
      timeout_secs: 30
    js:
      enabled: false
      work_dir: "scripts/js"
      timeout_secs: 30
```

---

## business — 业务自定义配置

```yaml
business:
  jwt_secret: "my-secret-key"
  token_expire_secs: 3600
  # 任意 key-value，通过 state.config.business 访问
```

在代码中读取：

```rust
let secret = state.config.business
    .get("jwt_secret")
    .and_then(|v| v.as_str())
    .unwrap_or("default");
```

---

## 环境变量覆盖

配置文件中的值可以被环境变量覆盖：

```bash
RINFRA_APP_NAME=prod-server cargo run
RINFRA_JWT_SECRET=xxx cargo run
RINFRA_CRYPTO_KEY=base64-encoded-key cargo run
RINFRA_ROOT_TOKEN=admin-token cargo run
```

---

## Feature Flags

某些插件需要开启对应的 Cargo feature：

| Feature | 启用的能力 |
|---------|----------|
| `mysql` | MySQL Store |
| `sqlite` | SQLite Store |
| `nats` | NATS JetStream MQ |
| `redis-mq` | Redis Streams MQ |
| `telemetry` | OpenTelemetry tracing |
