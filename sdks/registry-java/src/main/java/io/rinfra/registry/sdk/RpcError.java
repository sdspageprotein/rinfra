package io.rinfra.registry.sdk;

public final class RpcError extends RuntimeException {
    private final RpcErrorCode code;
    private final String service;
    private final String method;

    public RpcError(RpcErrorCode code, String message) {
        this(code, message, null, null, null);
    }

    public RpcError(RpcErrorCode code, String message, Throwable cause, String service, String method) {
        super(message, cause);
        this.code = code;
        this.service = service;
        this.method = method;
    }

    public RpcErrorCode code() {
        return code;
    }

    public String service() {
        return service;
    }

    public String method() {
        return method;
    }
}
