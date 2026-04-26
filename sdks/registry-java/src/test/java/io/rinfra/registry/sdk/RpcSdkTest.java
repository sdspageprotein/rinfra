package io.rinfra.registry.sdk;

import org.junit.jupiter.api.Test;

import java.time.Duration;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

class RpcSdkTest {
    @Test
    void resolveFiltersByProtocolAndMetadata() {
        Resolver resolver = new Resolver(() -> List.of(
                new NodeInfo(
                        "n1",
                        List.of(
                                new Endpoint("http", "10.0.0.1:8080"),
                                new Endpoint("grpc", "10.0.0.1:9090")
                        ),
                        Map.of("service.name", "order", "service.version", "v1")
                )
        ));

        Endpoint endpoint = resolver.resolve(new ResolveOptions("order", "grpc", "v1", null));
        assertEquals("10.0.0.1:9090", endpoint.address());
    }

    @Test
    void resolveReturnsNotFound() {
        Resolver resolver = new Resolver(() -> List.of(
                new NodeInfo("n1", List.of(new Endpoint("http", "10.0.0.1:8080")), Map.of("service.name", "order"))
        ));
        RpcError error = assertThrows(
                RpcError.class,
                () -> resolver.resolve(new ResolveOptions("order"))
        );
        assertEquals(RpcErrorCode.NOT_FOUND, error.code());
    }

    @Test
    void invokerRetriesTransientUnavailable() {
        Resolver resolver = new Resolver(() -> List.of(
                new NodeInfo("n1", List.of(new Endpoint("grpc", "10.0.0.1:9090")), Map.of("service.name", "order"))
        ));
        AtomicInteger calls = new AtomicInteger();
        RpcInvoker invoker = new RpcInvoker(resolver, (_endpoint, _method, _request, _options) -> {
            int count = calls.incrementAndGet();
            if (count < 3) {
                throw new RpcError(RpcErrorCode.UNAVAILABLE, "temporary unavailable");
            }
            return Map.of("ok", true);
        });
        Object result = invoker.invokeUnaryByService(
                new ResolveOptions("order"),
                "/order.v1.OrderService/GetOrder",
                Map.of("id", "1"),
                new CallOptions(
                        Duration.ofSeconds(2),
                        new RetryPolicy(3, Duration.ofMillis(1), Duration.ofMillis(5), false, null),
                        Map.of()
                )
        );
        assertEquals(Map.of("ok", true), result);
        assertEquals(3, calls.get());
    }
}
