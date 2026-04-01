[中文文档](configuration.zh-CN.md)

# Configuration Reference

rinfra is driven by YAML configuration files. The default path is `config/standalone.example.yaml`; override it with the `--config` flag.

Configuration templates for three deployment modes:

| File | Mode | Description |
|------|------|-------------|
| `config/standalone.example.yaml` | Standalone | Not in a cluster; runs independently |
| `config/main.example.yaml` | Cluster main | Manages worker registration and task dispatch |
| `config/worker.example.yaml` | Cluster worker | Connects to the main node to receive commands |

---

## Top-level structure

```yaml
app:                    # Application metadata
runtime:                # Runtime behavior
plugins:                # Plugin configuration (all capabilities declared here)
business:               # Business-specific custom config (arbitrary key-value)
```

---

## app — Application metadata

```yaml
app:
  name: "my-server"     # Application name, shown in logs and the Admin panel
  version: "0.1.0"      # Application version
```

---

## runtime — Runtime

```yaml
runtime:
  shutdown:
    grace_period_secs: 30       # Total graceful shutdown timeout (seconds)
    component_timeout_secs: 10  # Per-component shutdown timeout (seconds)
```

---

## plugins — Plugin configuration

### plugins.log — Logging

```yaml
plugins:
  log:
    stdout:
      enabled: true          # Enable console logging
      level: "info"          # trace | debug | info | warn | error
      format: "pretty"       # pretty | json | compact
    file:
      enabled: false         # Enable file logging
      path: "logs"           # Log directory
      filename: "app.log"    # Filename
      rotation: "daily"      # daily | hourly | never
      max_files: 7           # Number of retained files
      level: "info"
      format: "json"
```

### plugins.net — Network listeners

```yaml
plugins:
  net:
    listeners:
      - name: "main"             # Listener name (unique identifier)
        protocol: "http"         # http | tcp | grpc | trpc
        bind: "0.0.0.0:8080"     # Bind address
        http:                    # HTTP-specific settings
          middleware:            # Middleware (declarative)
            cors:
              enabled: true
              allow_origins: ["*"]
              allow_methods: ["GET", "POST", "PUT", "DELETE"]
              allow_headers: ["*"]
              max_age_secs: 86400
            request_id:
              enabled: true      # Auto-generate X-Request-Id
            timeout:
              enabled: true
              timeout_secs: 30   # Request timeout
            auth:
              enabled: false
              jwt_secret_env: "RINFRA_JWT_SECRET"
              exclude_paths: ["/healthz", "/readyz", "/metrics"]
            rate_limit:
              enabled: false
              key_strategy: "ip"  # ip | header
          ws:                    # WebSocket settings
            enabled: false
            ping_interval_secs: 30
            ping_timeout_secs: 10

      - name: "game-tcp"         # TCP listener
        protocol: "tcp"
        bind: "0.0.0.0:9100"
        tcp:
          max_frame_size: 65536   # Maximum frame size (bytes)
          pipeline:               # Byte transformation pipeline (optional)
            - transform: "lz4"   # Built-in LZ4 compression

      - name: "game-grpc"        # gRPC listener
        protocol: "grpc"
        bind: "0.0.0.0:9090"
```

### plugins.store — Database

```yaml
plugins:
  store:
    postgres:
      enabled: false
      url: "postgres://user:pass@localhost:5432/mydb"
      max_connections: 10
      idle_timeout_secs: 300
      required: false          # true = startup fails if connection fails
    # mysql:                   # requires feature = "mysql"
    #   enabled: false
    #   url: "mysql://user:pass@localhost:3306/mydb"
    # sqlite:                  # requires feature = "sqlite"
    #   enabled: false
    #   path: "data/app.db"
```

### plugins.cache — Cache

```yaml
plugins:
  cache:
    memory:
      enabled: true
      max_capacity: 10000      # Maximum number of entries
      ttl_secs: 300            # Default TTL (seconds)
    redis:
      enabled: false
      url: "redis://127.0.0.1:6379"
      required: false
    multilevel:
      enabled: false           # Requires both memory and redis enabled
      l1_max_capacity: 10000
      l1_ttl_secs: 60
```

### plugins.mq — Message queue

```yaml
plugins:
  mq:
    backend: memory            # memory | nats | redis_streams | none
    memory:
      channel_capacity: 1024
    nats:                      # requires feature = "nats"
      url: "nats://localhost:4222"
      stream_name: rinfra
      consumer_group: rinfra-workers
    redis_streams:             # requires feature = "redis-mq"
      url: "redis://localhost:6379"
      group_name: rinfra-workers
```

### plugins.ratelimit — Rate limiting

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

### plugins.crypto — Cryptography

```yaml
plugins:
  crypto:
    aesgcm:
      enabled: false
      key_env_var: "RINFRA_CRYPTO_KEY"   # Read key from environment variable
    rotating:
      enabled: false
      rotation_interval_secs: 86400
      max_key_versions: 5
    file:
      enabled: false
      path: "keys/main.key"
```

### plugins.lock — Distributed lock

```yaml
plugins:
  lock:
    enabled: false
    backend: memory            # memory | redis
    redis:
      url: "redis://127.0.0.1:6379"
      key_prefix: "rinfra:lock:"
```

### plugins.timer — Timer

```yaml
plugins:
  timer:
    enabled: false
    engine: simple
    simple:
      max_concurrent: 4        # Maximum concurrent scheduled tasks
```

### plugins.file_store — File storage

```yaml
plugins:
  file_store:
    enabled: false
    backend: local
    local:
      root_dir: "data/files"
```

### plugins.http_client — HTTP client

```yaml
plugins:
  http_client:
    enabled: false
    timeout_secs: 30
    user_agent: "rinfra/0.1.0"
    max_retries: 0             # Automatic retry count
    retry_delay_ms: 500        # Retry interval (milliseconds)
```

### plugins.config_watch — Hot config reload

```yaml
plugins:
  config_watch:
    enabled: false
    poll_interval_secs: 5      # Polling interval
```

### plugins.audit — Audit log

```yaml
plugins:
  audit:
    enabled: false
    backend: file
    file:
      path: "logs/audit.jsonl"  # JSON Lines format
```

### plugins.i18n — Internationalization

```yaml
plugins:
  i18n:
    enabled: false
    dir: "i18n"                # Directory of YAML translation files
    default_locale: "en"       # Default locale
```

### plugins.metrics — Prometheus metrics

```yaml
plugins:
  metrics:
    enabled: false
    endpoint: "/metrics"       # Metrics exposure endpoint
```

### plugins.health — Health checks

```yaml
plugins:
  health:
    enabled: true              # Exposes /healthz and /readyz
```

### plugins.telemetry — OpenTelemetry

```yaml
plugins:
  telemetry:
    enabled: false             # requires feature = "telemetry"
    otlp_endpoint: "http://localhost:4317"
    sample_ratio: 1.0
    export_timeout_secs: 10
```

### plugins.admin — Admin panel

```yaml
plugins:
  admin:
    enabled: true
    static_dir: "rinfra-admin/frontend/dist"
    auth:
      enabled: false           # Enable in production
      token_file: "data/admin_tokens.json"
      root_token_env: "RINFRA_ROOT_TOKEN"
      exclude_paths:
        - "/api/admin/health"
```

### plugins.script — Script engine

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

## business — Business-specific configuration

```yaml
business:
  jwt_secret: "my-secret-key"
  token_expire_secs: 3600
  # Arbitrary key-value; access via state.config.business
```

Reading in code:

```rust
let secret = state.config.business
    .get("jwt_secret")
    .and_then(|v| v.as_str())
    .unwrap_or("default");
```

---

## Environment variable overrides

Values in the configuration file can be overridden by environment variables:

```bash
RINFRA_APP_NAME=prod-server cargo run
RINFRA_JWT_SECRET=xxx cargo run
RINFRA_CRYPTO_KEY=base64-encoded-key cargo run
RINFRA_ROOT_TOKEN=admin-token cargo run
```

---

## Feature Flags

Some plugins require the corresponding Cargo feature to be enabled:

| Feature | Enabled capability |
|---------|-------------------|
| `mysql` | MySQL Store |
| `sqlite` | SQLite Store |
| `nats` | NATS JetStream MQ |
| `redis-mq` | Redis Streams MQ |
| `telemetry` | OpenTelemetry tracing |
