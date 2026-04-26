package io.rinfra.registry.sdk;

import com.fasterxml.jackson.databind.JsonNode;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

public final class RegistryNodeProvider implements NodeProvider {
    private final RegistryClient registryClient;

    public RegistryNodeProvider(RegistryClient registryClient) {
        this.registryClient = registryClient;
    }

    @Override
    public List<NodeInfo> listNodes() {
        List<JsonNode> rawNodes = registryClient.listNodes();
        List<NodeInfo> result = new ArrayList<>(rawNodes.size());
        for (JsonNode node : rawNodes) {
            String id = node.path("id").asText("");
            List<Endpoint> endpoints = new ArrayList<>();
            JsonNode endpointNodes = node.path("endpoints");
            if (endpointNodes.isArray()) {
                for (JsonNode endpointNode : endpointNodes) {
                    endpoints.add(
                            new Endpoint(
                                    endpointNode.path("protocol").asText(""),
                                    endpointNode.path("address").asText("")
                            )
                    );
                }
            }
            Map<String, String> metadata = new HashMap<>();
            JsonNode metadataNode = node.path("metadata");
            if (metadataNode.isObject()) {
                metadataNode.fields().forEachRemaining(e -> metadata.put(e.getKey(), e.getValue().asText("")));
            }
            result.add(new NodeInfo(id, List.copyOf(endpoints), Map.copyOf(metadata)));
        }
        return result;
    }
}
