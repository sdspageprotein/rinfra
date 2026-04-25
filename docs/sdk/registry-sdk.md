[简体中文](registry-sdk.zh-CN.md)

# Registry SDK Integration Guide

This guide explains how Java, Python, TypeScript, and Go applications can register to an rinfra main node.

## Scope

- Java: `io.rinfra:rinfra-registry-sdk` (Netty + Maven)
- Python: `rinfra-registry-sdk`
- TypeScript: `@rinfra/registry-sdk` (Node.js)
- Go: `github.com/rinfra/rinfra/sdks/registry-go/registrysdk`

## Minimal Configuration

All SDKs share the same semantic configuration:

- `mainAddress`: cluster TCP address, for example `10.0.1.10:7946`
- `clusterToken`: cluster token (the only auth model in V1)
- `nodeId`: current instance id
- `endpoints`: advertised service endpoints
- `metadata`: service metadata

## Endpoint Advertising

Multi-language SDKs do not have `plugins.net.listeners`, so `endpoints` must be explicitly provided and map directly to `Endpoint { protocol, address }`.

Example:

```json
{
  "endpoints": [
    { "protocol": "http", "address": "10.0.1.23:8080" }
  ]
}
```

Notes:

- Do not advertise `127.0.0.1` as a service endpoint.
- `0.0.0.0` is for bind only, not for final advertised address.
- Prefer addresses reachable by main/workers: internal IP, Pod DNS, or Service DNS.

## Metadata Contract v1

Recommended minimum keys:

- `meta.schema = rinfra.meta.v1`
- `service.name`
- `service.instance_id`
- `service.version`
- `service.env`

Optional keys:

- `service.zone`
- `service.region`
- `service.weight`
- `service.tags`

## Deployment Examples

### Bare Metal

- Bind: `0.0.0.0:8080`
- Advertise: `10.0.1.23:8080`

### Docker (bridge network)

- Bind: `0.0.0.0:8080`
- Advertise: container-reachable or host-mapped address
- For multi-host access, prefer internal DNS names

### Kubernetes

- Bind: `0.0.0.0:8080`
- Advertise: `<svc-name>.<ns>.svc.cluster.local:8080` (recommended)

## Lifecycle

- `start()`: connect to main, send `Register`, enter heartbeat loop
- `stop()`: send `Deregister`, close connection
- `listNodes()`/`list_nodes()`: fetch node list from main

## Compatibility

- Protocol: 4-byte big-endian length prefix + JSON (`ClusterMessage`)
- Rust SDK is not split; the built-in runtime implementation is the reference implementation
