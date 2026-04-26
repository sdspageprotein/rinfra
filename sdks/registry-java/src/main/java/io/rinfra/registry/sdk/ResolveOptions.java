package io.rinfra.registry.sdk;

public record ResolveOptions(
        String service,
        String protocol,
        String serviceVersion,
        String zone
) {
    public ResolveOptions(String service) {
        this(service, "grpc", null, null);
    }

    public ResolveOptions {
        if (protocol == null || protocol.isBlank()) {
            protocol = "grpc";
        }
    }
}
