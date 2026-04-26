# rinfra-registry-sdk (Java)

Java SDK for registering non-Rust services into rinfra cluster and resolving RPC endpoints.

## Requirements

- Java 17+
- Maven 3.9+

## Unified RPC API (V1)

The SDK includes:

- `Resolver`: endpoint filtering by protocol (`grpc` default) and metadata
- `RpcInvoker`: unary invocation by endpoint or by service (resolve + invoke)
- `RpcError` / `RpcErrorCode`: normalized SDK error model

Example:

```java
Resolver resolver = new Resolver(new RegistryNodeProvider(registryClient));
RpcInvoker invoker = new RpcInvoker(
        resolver,
        (endpoint, method, request, options) -> {
            // Plug in grpc-java unary call here.
            return Map.of();
        }
);

Object response = invoker.invokeUnaryByService(
        new ResolveOptions("order-api"),
        "/order.v1.OrderService/GetOrder",
        Map.of("id", "1"),
        CallOptions.defaults()
);
```

## gRPC Status Mapping (Common Contract)

- `DEADLINE_EXCEEDED` -> `TIMEOUT`
- `UNAVAILABLE` -> `UNAVAILABLE`
- `NOT_FOUND` -> `NOT_FOUND`
- `INVALID_ARGUMENT` -> `INVALID_ARGUMENT`
- `INTERNAL` -> `INTERNAL`
- `UNAUTHENTICATED` -> `UNAUTHENTICATED`
- `PERMISSION_DENIED` -> `PERMISSION_DENIED`
- `CANCELLED` -> `CANCELLED`
- others -> `UNKNOWN`
