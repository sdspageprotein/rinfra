package io.rinfra.registry.sdk;

import java.time.Duration;
import java.util.Objects;
import java.util.concurrent.ThreadLocalRandom;

public final class RpcInvoker {
    private final Resolver resolver;
    private final UnaryCaller caller;

    public RpcInvoker(Resolver resolver, UnaryCaller caller) {
        this.resolver = Objects.requireNonNull(resolver, "resolver");
        this.caller = Objects.requireNonNull(caller, "caller");
    }

    public Object invokeUnary(Endpoint endpoint, String method, Object request, CallOptions options) {
        CallOptions effective = options == null ? CallOptions.defaults() : options;
        try {
            return caller.call(endpoint, method, request, effective);
        } catch (RuntimeException ex) {
            throw mapError(ex, null, method);
        }
    }

    public Object invokeUnaryByService(
            ResolveOptions resolveOptions,
            String method,
            Object request,
            CallOptions options
    ) {
        CallOptions effective = options == null ? CallOptions.defaults() : options;
        RpcError lastError = null;
        int attempts = Math.max(1, effective.retryPolicy().maxAttempts());
        for (int attempt = 1; attempt <= attempts; attempt++) {
            Endpoint endpoint = resolver.resolve(resolveOptions);
            try {
                return invokeUnary(endpoint, method, request, effective);
            } catch (RpcError rpcError) {
                lastError = rpcError;
                if (!effective.retryPolicy().retryOn().contains(rpcError.code()) || attempt >= attempts) {
                    throw rpcError;
                }
                sleep(backoffDuration(effective.retryPolicy(), attempt));
            }
        }
        if (lastError == null) {
            throw new RpcError(RpcErrorCode.UNKNOWN, "rpc failed", null, resolveOptions.service(), method);
        }
        throw lastError;
    }

    private Duration backoffDuration(RetryPolicy retryPolicy, int attempt) {
        long backoffMs = retryPolicy.baseDelay().toMillis() * (1L << Math.max(0, attempt - 1));
        backoffMs = Math.min(backoffMs, retryPolicy.maxDelay().toMillis());
        if (retryPolicy.jitter()) {
            backoffMs += ThreadLocalRandom.current().nextLong(Math.max(1, backoffMs / 4));
        }
        return Duration.ofMillis(backoffMs);
    }

    private void sleep(Duration duration) {
        try {
            Thread.sleep(duration.toMillis());
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new RpcError(RpcErrorCode.CANCELLED, "rpc retry interrupted", e, null, null);
        }
    }

    public static RpcError mapError(Throwable error, String service, String method) {
        if (error instanceof RpcError rpcError) {
            return rpcError;
        }
        return new RpcError(RpcErrorCode.UNKNOWN, "rpc failed", error, service, method);
    }
}
