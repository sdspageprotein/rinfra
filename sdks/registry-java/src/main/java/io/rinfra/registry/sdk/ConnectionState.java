package io.rinfra.registry.sdk;

public enum ConnectionState {
    DISCONNECTED,
    CONNECTING,
    REGISTERING,
    CONNECTED,
    RECONNECTING,
    STOPPING
}
