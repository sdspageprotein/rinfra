# rinfra-registry-sdk (Go)

Go SDK for registering non-Rust services to an rinfra main node.

## Requirements

- Go 1.22+

## Quick Start

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
                {Protocol: "http", Address: "10.0.1.27:8080"},
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
