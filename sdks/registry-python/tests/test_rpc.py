import unittest

from rinfra_registry_sdk import (
    CallOptions,
    Endpoint,
    NodeInfo,
    ResolveOptions,
    Resolver,
    RetryPolicy,
    RpcError,
    RpcErrorCode,
    RpcInvoker,
)


class _Provider:
    def __init__(self, nodes):
        self.nodes = nodes

    async def list_nodes(self):
        return self.nodes


class RpcTests(unittest.IsolatedAsyncioTestCase):
    async def test_resolver_filters_by_protocol_and_service(self):
        resolver = Resolver(
            _Provider(
                [
                    NodeInfo(
                        id="n1",
                        endpoints=[
                            Endpoint(protocol="http", address="10.0.0.1:8080"),
                            Endpoint(protocol="grpc", address="10.0.0.1:9090"),
                        ],
                        metadata={"service.name": "order", "service.version": "v1"},
                    )
                ]
            )
        )
        endpoint = await resolver.resolve(
            ResolveOptions(service="order", protocol="grpc", service_version="v1")
        )
        self.assertEqual(endpoint.address, "10.0.0.1:9090")

    async def test_not_found_error(self):
        resolver = Resolver(
            _Provider(
                [
                    NodeInfo(
                        id="n1",
                        endpoints=[Endpoint(protocol="http", address="10.0.0.1:8080")],
                        metadata={"service.name": "order"},
                    )
                ]
            )
        )
        with self.assertRaises(RpcError) as ctx:
            await resolver.resolve(ResolveOptions(service="order"))
        self.assertEqual(ctx.exception.code, RpcErrorCode.NOT_FOUND)

    async def test_retry_transient_unavailable(self):
        resolver = Resolver(
            _Provider(
                [
                    NodeInfo(
                        id="n1",
                        endpoints=[Endpoint(protocol="grpc", address="10.0.0.1:9090")],
                        metadata={"service.name": "order"},
                    )
                ]
            )
        )
        state = {"count": 0}

        async def unary_call(_ep, _method, _request, _opts):
            state["count"] += 1
            if state["count"] < 3:
                raise RpcError(RpcErrorCode.UNAVAILABLE, "temporary unavailable")
            return {"ok": True}

        invoker = RpcInvoker(resolver, unary_call)
        result = await invoker.invoke_unary_by_service(
            ResolveOptions(service="order"),
            "/order.v1.OrderService/GetOrder",
            {"id": "1"},
            CallOptions(retry=RetryPolicy(max_attempts=3)),
        )
        self.assertEqual(result["ok"], True)
        self.assertEqual(state["count"], 3)


if __name__ == "__main__":
    unittest.main()
