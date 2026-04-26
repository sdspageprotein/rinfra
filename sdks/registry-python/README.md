# rinfra-registry-sdk (Python)

Async Python SDK for registering non-Rust services into rinfra cluster main nodes and resolving RPC endpoints.

## Install

```bash
pip install rinfra-registry-sdk
```

## Quick Start

```python
from rinfra_registry_sdk import Endpoint, Registration, RegistryClientConfig, RegistryClient
```

## Unified RPC API (V1)

Python SDK now exposes:

- `Resolver`: endpoint filtering by protocol (`grpc` default) and metadata (`service.name`, `service.version`, `service.zone`)
- `RpcInvoker`: unary RPC invocation by endpoint or by service (resolve + invoke)
- `RpcError` / `RpcErrorCode`: normalized error model

Example:

```python
from rinfra_registry_sdk import (
    RegistryNodeProvider,
    Resolver,
    ResolveOptions,
    RpcInvoker,
)

provider = RegistryNodeProvider(registry_client)
resolver = Resolver(provider)

async def unary_call(endpoint, method, request, options):
    # Plug in grpc.aio call implementation here.
    ...

invoker = RpcInvoker(resolver, unary_call)
resp = await invoker.invoke_unary_by_service(
    ResolveOptions(service="order-api"),
    "/order.v1.OrderService/GetOrder",
    {"id": "1"},
)
```

## gRPC Status Mapping

- `DEADLINE_EXCEEDED` -> `timeout`
- `UNAVAILABLE` -> `unavailable`
- `NOT_FOUND` -> `not_found`
- `INVALID_ARGUMENT` -> `invalid_argument`
- `INTERNAL` -> `internal`
- `UNAUTHENTICATED` -> `unauthenticated`
- `PERMISSION_DENIED` -> `permission_denied`
- `CANCELLED` -> `cancelled`
- others -> `unknown`
