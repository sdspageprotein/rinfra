[English](README.md)

# rinfra 框架文档

> **rinfra** 是一个模块化、插件化的 Rust 后端基础设施框架，面向需要高性能、可扩展的服务端应用。

---

## 文档目录

| 文档 | 说明 |
|------|------|
| [快速开始](getting-started.md) | 5 分钟搭建第一个 rinfra 应用 |
| [配置参考](configuration.md) | 所有配置项的完整说明 |
| [插件体系](plugins.md) | 30+ 内置插件的使用指南 |
| [核心 API 参考](api-reference.md) | Trait、Struct、接口一览 |

---

## 架构总览

```
┌─────────────────────────────────────────────────────────────┐
│                      你的应用代码                              │
│  main.rs → RunOptions → http_router / tcp_handler / grpc    │
├─────────────────────────────────────────────────────────────┤
│                     rinfra-plugins                           │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐   │
│  │ Runtime  │ │ Net      │ │ Store    │ │ Plugin系统    │   │
│  │ 应用生命 │ │ HTTP/TCP │ │ PG/MySQL │ │ 30+ 内置插件  │   │
│  │ 周期管理 │ │ WS/gRPC  │ │ SQLite   │ │ 声明式配置    │   │
│  └──────────┘ └──────────┘ └──────────┘ └──────────────┘   │
├─────────────────────────────────────────────────────────────┤
│                      rinfra-core                            │
│  Trait 定义 │ Config │ AppState │ Error │ 协议抽象          │
└─────────────────────────────────────────────────────────────┘
```

## Workspace 结构

| Crate | 定位 |
|-------|------|
| **rinfra-core** | 纯 trait + config + error，零外部依赖（除 tokio/serde），定义框架契约 |
| **rinfra-plugins** | 所有 trait 的实现 + 插件系统 + Runtime + CLI，引入外部依赖 |
| **rinfra-admin** | Admin 管理面板（前端 + 后端），开箱即用 |
| **rinfra-derive** | 过程宏（Entity / FromRow / ToRow） |
| **rinfra-examples** | 示例应用（web/gate/game/admin） |

## 设计原则

1. **Trait 驱动**：所有能力定义在 `rinfra-core` 的 trait 中，实现在 `rinfra-plugins`，应用不依赖具体实现
2. **YAML 声明式配置**：通过配置文件决定启用哪些插件和使用哪个后端，无需改代码
3. **按需启用**：所有插件默认 `enabled: false`，只开你需要的
4. **可扩展**：任何 trait 都可以自行实现，通过 `RunOptions::plugin()` 注入
5. **零膨胀**：框架不内置业务逻辑，只提供基础设施能力
