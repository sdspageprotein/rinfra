import { test } from "node:test";
import assert from "node:assert/strict";
import { RegistryClient, Resolver, RpcInvoker, RpcError, type RpcNodeInfo } from "./index.js";

test("reject empty endpoints", () => {
  assert.throws(() => {
    new RegistryClient(
      { mainAddress: "127.0.0.1:7946", clusterToken: "token" },
      { nodeId: "n1", endpoints: [], metadata: {} }
    );
  });
});

test("resolver filters grpc endpoint by metadata", async () => {
  const nodes: RpcNodeInfo[] = [
    {
      id: "n1",
      endpoints: [
        { protocol: "http", address: "10.0.0.1:8080" },
        { protocol: "grpc", address: "10.0.0.1:9090" }
      ],
      metadata: { "service.name": "order", "service.version": "v1" }
    }
  ];
  const resolver = new Resolver({
    async listNodes() {
      return nodes;
    }
  });
  const endpoint = await resolver.resolve({ service: "order", protocol: "grpc", serviceVersion: "v1" });
  assert.equal(endpoint.address, "10.0.0.1:9090");
});

test("invoker retries unavailable errors", async () => {
  let calls = 0;
  const resolver = new Resolver({
    async listNodes() {
      return [
        {
          id: "n1",
          endpoints: [{ protocol: "grpc", address: "10.0.0.1:9090" }],
          metadata: { "service.name": "order" }
        }
      ];
    }
  });
  const invoker = new RpcInvoker(resolver, async () => {
    calls += 1;
    if (calls < 3) {
      throw new RpcError("unavailable", "temporary unavailable");
    }
    return { ok: true };
  });
  const result = await invoker.invokeUnaryByService(
    { service: "order" },
    "/order.v1.OrderService/GetOrder",
    { id: "1" },
    { retry: { maxAttempts: 3 } }
  );
  assert.deepEqual(result, { ok: true });
  assert.equal(calls, 3);
});
