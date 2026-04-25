package io.rinfra.registry.sdk;

import org.junit.jupiter.api.Test;

import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertThrows;

class RegistryClientConfigTest {
    @Test
    void endpoint_must_not_be_empty() {
        Registration reg = new Registration("node-1", List.of(), Map.of());
        RegistryClientConfig cfg = new RegistryClientConfig("127.0.0.1:7946", "token");
        assertThrows(IllegalArgumentException.class, () -> new RegistryClient(cfg, reg));
    }
}
