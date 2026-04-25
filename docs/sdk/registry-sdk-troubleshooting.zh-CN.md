[English](registry-sdk-troubleshooting.md)

# Registry SDK 故障排查

## 连接失败

现象：

- `connection refused`
- `timeout`

排查：

1. 确认 main 节点 cluster TCP 服务已启动。
2. 确认 `mainAddress` 指向 cluster 地址而不是 admin HTTP 地址。
3. 确认防火墙/安全组/网络策略放通端口。

## 认证失败

现象：

- `RegisterAck.success = false`
- 日志显示 `invalid token`

排查：

1. 检查 SDK `clusterToken` 与 main 的 `cluster_token` 是否一致。
2. 确认 token 前后无空格。
3. 认证失败属于 fail-fast，不会持续重试，需修复配置后重启。

## endpoint 不可达

现象：

- main 能看到节点在线，但调用方访问 endpoint 失败。

排查：

1. 检查是否误上报 `127.0.0.1` 或 `0.0.0.0`。
2. 检查上报地址是否对 main/worker 可路由。
3. Kubernetes 场景优先使用 Service DNS。

## 心跳/重连异常

现象：

- 节点频繁 Offline/Online 抖动
- 连接断开后未恢复

排查：

1. 检查网络稳定性与 TCP 空闲连接策略。
2. 检查 SDK 日志中的 `Reconnecting` 状态与退避间隔。
3. 检查 main 是否频繁重启或负载过高。
