import { test } from "node:test";
import assert from "node:assert/strict";
import { RegistryClient } from "./index.js";

test("reject empty endpoints", () => {
  assert.throws(() => {
    new RegistryClient(
      { mainAddress: "127.0.0.1:7946", clusterToken: "token" },
      { nodeId: "n1", endpoints: [], metadata: {} }
    );
  });
});
