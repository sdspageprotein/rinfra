package io.rinfra.registry.sdk;

import java.util.Objects;

public final class Endpoint {
    private final String protocol;
    private final String address;

    public Endpoint(String protocol, String address) {
        this.protocol = Objects.requireNonNull(protocol, "protocol");
        this.address = Objects.requireNonNull(address, "address");
    }

    public String protocol() {
        return protocol;
    }

    public String address() {
        return address;
    }
}
