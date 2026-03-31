[中文文档](README.zh-CN.md)

# rinfra framework documentation

> **rinfra** is a modular, pluggable Rust backend infrastructure framework for server applications that need high performance and scalability.

---

## Documentation index

| Document | Description |
|------|------|
| [Quick start](getting-started.md) | Set up your first rinfra app in 5 minutes |
| [Configuration reference](configuration.md) | Full description of all configuration options |
| [Plugin system](plugins.md) | Guide to 30+ built-in plugins |
| [Core API reference](api-reference.md) | Traits, structs, and interfaces at a glance |

---

## Architecture overview

```
┌─────────────────────────────────────────────────────────────┐
│                     Your application code                   │
│  main.rs → RunOptions → http_router / tcp_handler / grpc    │
├─────────────────────────────────────────────────────────────┤
│                     rinfra-plugins                           │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐   │
│  │ Runtime  │ │ Net      │ │ Store    │ │Plugin system │   │
│  │ app life │ │ HTTP/TCP │ │ PG/MySQL │ │ 30+ built-in │   │
│  │  cycle   │ │ WS/gRPC  │ │ SQLite   │ │ declarative  │   │
│  └──────────┘ └──────────┘ └──────────┘ └──────────────┘   │
├─────────────────────────────────────────────────────────────┤
│                      rinfra-core                            │
│  Trait defs │ Config │ AppState │ Error │ Protocol abs.   │
└─────────────────────────────────────────────────────────────┘
```

## Workspace layout

| Crate | Role |
|-------|------|
| **rinfra-core** | Pure traits + config + error, no external deps except tokio/serde; defines framework contracts |
| **rinfra-plugins** | Trait implementations + plugin system + Runtime + CLI; pulls in external dependencies |
| **rinfra-admin** | Admin dashboard (frontend + backend), ready to use |
| **rinfra-derive** | Procedural macros (Entity / FromRow / ToRow) |
| **rinfra-examples** | Example apps (web/gate/game/admin) |

## Design principles

1. **Trait-driven**: Capabilities are defined as traits in `rinfra-core` and implemented in `rinfra-plugins`; apps do not depend on concrete implementations
2. **Declarative YAML configuration**: Choose which plugins to enable and which backends to use via config files, without code changes
3. **Opt-in by default**: Every plugin defaults to `enabled: false`; enable only what you need
4. **Extensible**: You can implement any trait yourself and inject it with `RunOptions::plugin()`
5. **No bloat**: The framework does not ship business logic; it only provides infrastructure
