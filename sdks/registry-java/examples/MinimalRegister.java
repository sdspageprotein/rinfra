package io.rinfra.registry.sdk.examples;

import io.rinfra.registry.sdk.Endpoint;
import io.rinfra.registry.sdk.Registration;
import io.rinfra.registry.sdk.RegistryClient;
import io.rinfra.registry.sdk.RegistryClientConfig;

import java.util.List;
import java.util.Map;

public final class MinimalRegister {
    public static void main(String[] args) throws Exception {
        RegistryClientConfig config = new RegistryClientConfig("127.0.0.1:7946", "change-me-in-production");
        Registration registration = new Registration(
                "java-order-api-1",
                List.of(new Endpoint("http", "10.0.1.23:8080")),
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

        Runtime.getRuntime().addShutdownHook(new Thread(client::stop));
        Thread.sleep(60_000);
        client.stop();
    }
}
