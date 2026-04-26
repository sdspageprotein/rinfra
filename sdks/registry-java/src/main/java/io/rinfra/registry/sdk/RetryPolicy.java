package io.rinfra.registry.sdk;

import java.time.Duration;
import java.util.EnumSet;
import java.util.Set;

public record RetryPolicy(
        int maxAttempts,
        Duration baseDelay,
        Duration maxDelay,
        boolean jitter,
        Set<RpcErrorCode> retryOn
) {
    public RetryPolicy {
        if (maxAttempts <= 0) maxAttempts = 3;
        if (baseDelay == null || baseDelay.isNegative() || baseDelay.isZero()) baseDelay = Duration.ofMillis(100);
        if (maxDelay == null || maxDelay.isNegative() || maxDelay.isZero()) maxDelay = Duration.ofSeconds(1);
        if (retryOn == null || retryOn.isEmpty()) {
            retryOn = EnumSet.of(RpcErrorCode.UNAVAILABLE, RpcErrorCode.TIMEOUT);
        } else {
            retryOn = EnumSet.copyOf(retryOn);
        }
    }

    public static RetryPolicy defaults() {
        return new RetryPolicy(3, Duration.ofMillis(100), Duration.ofSeconds(1), true, null);
    }
}
