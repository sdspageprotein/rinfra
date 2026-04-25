package io.rinfra.registry.sdk;

import java.time.Duration;
import java.util.Objects;

public final class RegistryClientConfig {
    private final String mainAddress;
    private final String clusterToken;
    private final Duration pingInterval;
    private final Duration registerTimeout;

    public RegistryClientConfig(String mainAddress, String clusterToken) {
        this(mainAddress, clusterToken, Duration.ofSeconds(10), Duration.ofSeconds(5));
    }

    public RegistryClientConfig(
            String mainAddress,
            String clusterToken,
            Duration pingInterval,
            Duration registerTimeout
    ) {
        this.mainAddress = Objects.requireNonNull(mainAddress, "mainAddress");
        this.clusterToken = Objects.requireNonNull(clusterToken, "clusterToken");
        this.pingInterval = Objects.requireNonNull(pingInterval, "pingInterval");
        this.registerTimeout = Objects.requireNonNull(registerTimeout, "registerTimeout");
    }

    public String mainAddress() {
        return mainAddress;
    }

    public String clusterToken() {
        return clusterToken;
    }

    public Duration pingInterval() {
        return pingInterval;
    }

    public Duration registerTimeout() {
        return registerTimeout;
    }
}
