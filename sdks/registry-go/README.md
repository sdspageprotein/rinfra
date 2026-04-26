# rinfra-registry-sdk (Go)

Go SDK for registering non-Rust services to an rinfra main node and resolving RPC endpoints.

## Requirements

- Go 1.22+

## Quick Start (Registry)

```go
package main

import (
	"context"
	"time"
	registrysdk "github.com/rinfra/rinfra/sdks/registry-go/registrysdk"
)

func main() {
	client, _ := registrysdk.NewClient(
		registrysdk.Config{
			MainAddress:  "127.0.0.1:7946",
			ClusterToken: "change-me-in-production",
		},
		registrysdk.Registration{
			NodeID: "go-order-api-1",
			Endpoints: []registrysdk.Endpoint{
				{Protocol: "grpc", Address: "10.0.1.27:9090"},
			},
			Metadata: map[string]string{
				"service.name":        "order-api",
				"service.instance_id": "go-order-api-1",
				"service.version":     "0.1.0",
				"service.env":         "dev",
			},
		},
	)
	_ = client.Start(context.Background())
	time.Sleep(30 * time.Second)
	_ = client.Stop(context.Background())
}
```

## Unified RPC API (V1)

Go SDK provides unified primitives:

- `Resolver`: filter endpoints by protocol (`grpc` by default) and metadata (`service.name`, `service.version`, `service.zone`).
- `RpcInvoker`: invoke unary RPC directly by endpoint or by service (resolve + invoke).
- `RpcError`: normalized error type with `RpcErrorCode`.

Example:

```go
provider := registrysdk.NewRegistryNodeProvider(client)
resolver := registrysdk.NewResolver(provider)

invoker := registrysdk.NewRpcInvoker(resolver, myUnaryCaller)
err := invoker.InvokeUnaryByService(
	context.Background(),
	registrysdk.ResolveOptions{Service: "order-api"},
	"/order.v1.OrderService/GetOrder",
	req,
	&resp,
	registrysdk.CallOptions{},
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
