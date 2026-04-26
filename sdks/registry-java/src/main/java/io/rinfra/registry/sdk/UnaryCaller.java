package io.rinfra.registry.sdk;

@FunctionalInterface
public interface UnaryCaller {
    Object call(Endpoint endpoint, String method, Object request, CallOptions options);
}
