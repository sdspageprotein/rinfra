package io.rinfra.registry.sdk;

import java.util.List;
import java.util.Map;

public final class IntegrationProbe {
    private IntegrationProbe() {}

    public static void main(String[] args) throws Exception {
        String token = args.length > 0 ? args[0] : "change-me-in-production";
        RegistryClientConfig config = new RegistryClientConfig("127.0.0.1:7946", token);
        Registration registration = new Registration(
                "java-order-api-1",
                List.of(new Endpoint("http", "10.0.1.26:8080")),
                Map.of(
                        "meta.schema", "rinfra.meta.v1",
                        "service.name", "order-api",
                        "service.instance_id", "java-order-api-1",
                        "service.version", "0.1.0",
                        "service.env", "dev"
                )
        );

        RegistryClient client = new RegistryClient(config, registration);
        client.start();
        Thread.sleep(2_000);
        try {
            System.out.println(client.listNodes().size());
        } catch (Exception ignored) {
            // Intentionally ignored for auth failure checks.
        }
        Thread.sleep(3_000);
        client.stop();
    }
}
