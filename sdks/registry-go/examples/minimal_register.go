package main

import (
	"context"
	"log"
	"time"

	registrysdk "github.com/rinfra/rinfra/sdks/registry-go/registrysdk"
)

func main() {
	client, err := registrysdk.NewClient(
		registrysdk.Config{
			MainAddress:  "127.0.0.1:7946",
			ClusterToken: "change-me-in-production",
			PingInterval: 2 * time.Second,
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
	if err != nil {
		log.Fatalf("new client: %v", err)
	}

	if err := client.Start(context.Background()); err != nil {
		log.Fatalf("start client: %v", err)
	}

	time.Sleep(20 * time.Second)

	nodes, err := client.ListNodes(context.Background())
	if err == nil {
		log.Printf("node count: %d", len(nodes))
	}

	if err := client.Stop(context.Background()); err != nil {
		log.Fatalf("stop client: %v", err)
	}
}
