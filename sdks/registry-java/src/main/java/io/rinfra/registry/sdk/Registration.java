package io.rinfra.registry.sdk;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;

public final class Registration {
    private final String nodeId;
    private final List<Endpoint> endpoints;
    private final Map<String, String> metadata;

    public Registration(String nodeId, List<Endpoint> endpoints, Map<String, String> metadata) {
        this.nodeId = Objects.requireNonNull(nodeId, "nodeId");
        this.endpoints = new ArrayList<>(Objects.requireNonNull(endpoints, "endpoints"));
        this.metadata = new HashMap<>(Objects.requireNonNull(metadata, "metadata"));
    }

    public String nodeId() {
        return nodeId;
    }

    public List<Endpoint> endpoints() {
        return List.copyOf(endpoints);
    }

    public Map<String, String> metadata() {
        return Map.copyOf(metadata);
    }
}
