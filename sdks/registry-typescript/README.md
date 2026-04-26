# @rinfra/registry-sdk

TypeScript/Node.js SDK for registering services into rinfra cluster and resolving RPC endpoints.

## Install

```bash
npm i @rinfra/registry-sdk
```

## Runtime Scope

- Supported: Node.js runtime
- Not supported in V1: Browser runtime

The package throws a clear runtime error for non-Node environments.

## Unified RPC API (V1)

Exports:

- `Resolver`: filters endpoints by `protocol` (default `grpc`) and metadata
- `RpcInvoker`: unary invocation by endpoint and by service
- `RpcError` / `RpcErrorCode`: normalized errors (`not_found`, `timeout`, etc.)

Example:

```ts
import { Resolver, RpcInvoker } from "@rinfra/registry-sdk";

const resolver = new Resolver({
  async listNodes() {
    return nodes;
  }
});

const invoker = new RpcInvoker(resolver, async (endpoint, method, request, options) => {
  // Plug in @grpc/grpc-js unary call here.
  return {};
});

const resp = await invoker.invokeUnaryByService(
  { service: "order-api" },
  "/order.v1.OrderService/GetOrder",
  { id: "1" }
);
```
