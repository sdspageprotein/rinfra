package io.rinfra.registry.sdk;

import java.util.List;
import java.util.Map;

public record NodeInfo(
        String id,
        List<Endpoint> endpoints,
        Map<String, String> metadata
) {
}
