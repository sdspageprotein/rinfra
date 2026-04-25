[English](registry-sdk.md)

# Registry SDK 接入指南

本文档说明如何使用 Java、Python、TypeScript、Go SDK 把非 Rust 应用注册到 rinfra main 节点。

## 适用范围

- Java：`io.rinfra:rinfra-registry-sdk`（Netty + Maven）
- Python：`rinfra-registry-sdk`
- TypeScript：`@rinfra/registry-sdk`（Node.js）
- Go：`github.com/rinfra/rinfra/sdks/registry-go/registrysdk`

## 最小配置

所有 SDK 共享相同语义配置：

- `mainAddress`: main 节点的 TCP 注册地址，例如 `10.0.1.10:7946`
- `clusterToken`: 集群 token（V1 唯一认证方式）
- `nodeId`: 当前实例 ID
- `endpoints`: 服务暴露地址列表
- `metadata`: 业务元信息

## endpoint 暴露规则

多语言 SDK 没有 `plugins.net.listeners`，必须显式配置 `endpoints`，与 rinfra 的 `Endpoint { protocol, address }` 保持一致。

示例：

```json
{
  "endpoints": [
    { "protocol": "http", "address": "10.0.1.23:8080" }
  ]
}
```

注意：

- 不要把 `127.0.0.1` 作为对外服务地址上报。
- `0.0.0.0` 仅用于 bind，不应作为最终 advertised address。
- 推荐使用可被 main/worker 访问的 IP、Pod DNS 或 Service DNS。

## metadata contract v1

建议至少包含：

- `meta.schema = rinfra.meta.v1`
- `service.name`
- `service.instance_id`
- `service.version`
- `service.env`

可选键：

- `service.zone`
- `service.region`
- `service.weight`
- `service.tags`

## 部署场景示例

### 裸机

- 服务监听：`0.0.0.0:8080`
- 注册上报：`10.0.1.23:8080`

### Docker（bridge 网络）

- 服务监听：`0.0.0.0:8080`
- 注册上报：容器可达地址或宿主机映射地址
- 如跨主机访问，建议通过内网 DNS 名称上报

### Kubernetes

- 服务监听：`0.0.0.0:8080`
- 注册上报：`<svc-name>.<ns>.svc.cluster.local:8080`（推荐）

## 生命周期

- `start()`：连接 main、发送 Register、进入心跳。
- `stop()`：发送 Deregister、关闭连接。
- `listNodes()`/`list_nodes()`：主动拉取当前节点列表。

## 兼容性

- 协议：4-byte big-endian length + JSON（externally tagged enum）
- Rust 不拆独立 SDK，现有 runtime 实现是参考实现。
