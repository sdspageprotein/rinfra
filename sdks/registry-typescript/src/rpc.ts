export type RpcErrorCode =
  | "timeout"
  | "unavailable"
  | "not_found"
  | "invalid_argument"
  | "internal"
  | "unauthenticated"
  | "permission_denied"
  | "cancelled"
  | "unknown";

export class RpcError extends Error {
  readonly code: RpcErrorCode;
  readonly service?: string;
  readonly method?: string;
  readonly cause?: unknown;

  constructor(code: RpcErrorCode, message: string, opts?: { service?: string; method?: string; cause?: unknown }) {
    super(message);
    this.name = "RpcError";
    this.code = code;
    this.service = opts?.service;
    this.method = opts?.method;
    this.cause = opts?.cause;
  }
}

export interface RpcEndpoint {
  protocol: string;
  address: string;
}

export interface RpcNodeInfo {
  id: string;
  endpoints: RpcEndpoint[];
  metadata: Record<string, string>;
}

export interface ResolveOptions {
  service: string;
  protocol?: string;
  serviceVersion?: string;
  zone?: string;
}

export interface RetryPolicy {
  maxAttempts?: number;
  baseDelayMs?: number;
  maxDelayMs?: number;
  jitter?: boolean;
  retryOn?: RpcErrorCode[];
}

export interface CallOptions {
  timeoutMs?: number;
  retry?: RetryPolicy;
  metadata?: Record<string, string>;
}

type NormalizedRetryPolicy = {
  maxAttempts: number;
  baseDelayMs: number;
  maxDelayMs: number;
  jitter: boolean;
  retryOn: RpcErrorCode[];
};

type NormalizedCallOptions = {
  timeoutMs: number;
  retry: NormalizedRetryPolicy;
  metadata: Record<string, string>;
};

export interface NodeProvider {
  listNodes(): Promise<RpcNodeInfo[]>;
}

export class Resolver {
  constructor(private readonly provider: NodeProvider) {}

  async list(options: ResolveOptions): Promise<RpcEndpoint[]> {
    const protocol = (options.protocol ?? "grpc").toLowerCase();
    const nodes = await this.provider.listNodes();
    const endpoints: RpcEndpoint[] = [];
    for (const node of nodes) {
      if (options.service && node.metadata["service.name"] !== options.service) continue;
      if (options.serviceVersion && node.metadata["service.version"] !== options.serviceVersion) continue;
      if (options.zone && node.metadata["service.zone"] !== options.zone) continue;
      for (const endpoint of node.endpoints) {
        if (endpoint.protocol.toLowerCase() === protocol) {
          endpoints.push(endpoint);
        }
      }
    }
    if (endpoints.length === 0) {
      throw new RpcError("not_found", "no endpoint matched resolve filters", { service: options.service });
    }
    return endpoints;
  }

  async resolve(options: ResolveOptions): Promise<RpcEndpoint> {
    const endpoints = await this.list(options);
    return endpoints[Math.floor(Math.random() * endpoints.length)];
  }
}

export type UnaryCall<Req, Res> = (
  endpoint: RpcEndpoint,
  method: string,
  request: Req,
  options: NormalizedCallOptions
) => Promise<Res>;

export class RpcInvoker {
  constructor(private readonly resolver: Resolver, private readonly unaryCall: UnaryCall<unknown, unknown>) {}

  async invokeUnary<Req, Res>(
    endpoint: RpcEndpoint,
    method: string,
    request: Req,
    options?: CallOptions
  ): Promise<Res> {
  const effective = withCallDefaults(options);
    try {
      return (await promiseTimeout(
        this.unaryCall(endpoint, method, request, effective),
        effective.timeoutMs
      )) as Res;
    } catch (err) {
      throw mapError(err, method);
    }
  }

  async invokeUnaryByService<Req, Res>(
    resolveOptions: ResolveOptions,
    method: string,
    request: Req,
    options?: CallOptions
  ): Promise<Res> {
  const effective = withCallDefaults(options);
    let lastError: unknown;
    for (let attempt = 1; attempt <= effective.retry.maxAttempts; attempt += 1) {
      const endpoint = await this.resolver.resolve(resolveOptions);
      try {
        return await this.invokeUnary(endpoint, method, request, effective);
      } catch (err) {
        lastError = err;
        if (!(err instanceof RpcError) || !effective.retry.retryOn.includes(err.code) || attempt >= effective.retry.maxAttempts) {
          throw err;
        }
        let backoff = Math.min(
          effective.retry.maxDelayMs,
          effective.retry.baseDelayMs * 2 ** (attempt - 1)
        );
        if (effective.retry.jitter) {
          backoff += Math.floor(backoff * 0.25 * Math.random());
        }
        await delay(backoff);
      }
    }
    throw mapError(lastError, method);
  }
}

export function assertNodeRuntime(): void {
  if (typeof process === "undefined" || !process.versions?.node) {
    throw new RpcError(
      "invalid_argument",
      "Browser runtime is not supported. Use @rinfra/registry-sdk in Node.js only."
    );
  }
}

function withCallDefaults(options?: CallOptions): NormalizedCallOptions {
  const retry: NormalizedRetryPolicy = {
    maxAttempts: Math.max(1, options?.retry?.maxAttempts ?? 3),
    baseDelayMs: options?.retry?.baseDelayMs ?? 100,
    maxDelayMs: options?.retry?.maxDelayMs ?? 1000,
    jitter: options?.retry?.jitter ?? true,
    retryOn: options?.retry?.retryOn ?? ["unavailable", "timeout"]
  };
  return {
    timeoutMs: options?.timeoutMs ?? 2000,
    retry,
    metadata: options?.metadata ?? {}
  };
}

function mapError(err: unknown, method?: string): RpcError {
  if (err instanceof RpcError) return err;
  if (err instanceof Error && err.name === "TimeoutError") {
    return new RpcError("timeout", "rpc timeout", { method, cause: err });
  }
  return new RpcError("unknown", "rpc failed", { method, cause: err });
}

async function promiseTimeout<T>(promise: Promise<T>, timeoutMs: number): Promise<T> {
  let timer: NodeJS.Timeout | undefined;
  try {
    return await Promise.race([
      promise,
      new Promise<T>((_, reject) => {
        timer = setTimeout(() => {
          const timeoutError = new Error("rpc timeout");
          timeoutError.name = "TimeoutError";
          reject(timeoutError);
        }, timeoutMs);
      })
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
