import { RegistryClient } from "../src/index.js";

async function main() {
  const client = new RegistryClient(
    {
      mainAddress: "127.0.0.1:7946",
      clusterToken: "change-me-in-production",
      pingIntervalMs: 10_000
    },
    {
      nodeId: "ts-order-api-1",
      endpoints: [{ protocol: "http", address: "10.0.1.25:8080" }],
      metadata: {
        "meta.schema": "rinfra.meta.v1",
        "service.name": "order-api",
        "service.instance_id": "ts-order-api-1",
        "service.version": "0.1.0",
        "service.env": "dev"
      }
    }
  );

  await client.start();
  setTimeout(async () => {
    await client.stop();
  }, 60_000);
}

void main();
