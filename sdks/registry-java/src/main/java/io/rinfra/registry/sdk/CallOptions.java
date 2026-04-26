package io.rinfra.registry.sdk;

import java.time.Duration;
import java.util.Collections;
import java.util.Map;

public record CallOptions(
        Duration timeout,
        RetryPolicy retryPolicy,
        Map<String, String> metadata
) {
    public CallOptions {
        if (timeout == null || timeout.isNegative() || timeout.isZero()) {
            timeout = Duration.ofSeconds(2);
        }
        if (retryPolicy == null) {
            retryPolicy = RetryPolicy.defaults();
        }
        if (metadata == null) {
            metadata = Collections.emptyMap();
        } else {
            metadata = Map.copyOf(metadata);
        }
    }

    public static CallOptions defaults() {
        return new CallOptions(Duration.ofSeconds(2), RetryPolicy.defaults(), Collections.emptyMap());
    }
}
