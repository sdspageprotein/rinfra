package io.rinfra.registry.sdk;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Objects;
import java.util.concurrent.ThreadLocalRandom;

public final class Resolver {
    private final NodeProvider provider;

    public Resolver(NodeProvider provider) {
        this.provider = Objects.requireNonNull(provider, "provider");
    }

    public List<Endpoint> list(ResolveOptions options) {
        List<NodeInfo> nodes = provider.listNodes();
        List<Endpoint> candidates = new ArrayList<>();
        for (NodeInfo node : nodes) {
            if (!metadataMatch(node, options)) {
                continue;
            }
            for (Endpoint endpoint : node.endpoints()) {
                if (endpoint.protocol().toLowerCase(Locale.ROOT).equals(options.protocol().toLowerCase(Locale.ROOT))) {
                    candidates.add(endpoint);
                }
            }
        }
        if (candidates.isEmpty()) {
            throw new RpcError(
                    RpcErrorCode.NOT_FOUND,
                    "no endpoint matched resolve filters",
                    null,
                    options.service(),
                    null
            );
        }
        return List.copyOf(candidates);
    }

    public Endpoint resolve(ResolveOptions options) {
        List<Endpoint> endpoints = list(options);
        return endpoints.get(ThreadLocalRandom.current().nextInt(endpoints.size()));
    }

    private boolean metadataMatch(NodeInfo node, ResolveOptions options) {
        if (options.service() != null && !options.service().isBlank()) {
            if (!options.service().equals(node.metadata().get("service.name"))) {
                return false;
            }
        }
        if (options.serviceVersion() != null && !options.serviceVersion().isBlank()) {
            if (!options.serviceVersion().equals(node.metadata().get("service.version"))) {
                return false;
            }
        }
        if (options.zone() != null && !options.zone().isBlank()) {
            if (!options.zone().equals(node.metadata().get("service.zone"))) {
                return false;
            }
        }
        return true;
    }
}
