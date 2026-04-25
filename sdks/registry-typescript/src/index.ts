import net from "node:net";

export type ConnectionState =
  | "DISCONNECTED"
  | "CONNECTING"
  | "REGISTERING"
  | "CONNECTED"
  | "RECONNECTING"
  | "STOPPING";

export interface Endpoint {
  protocol: string;
  address: string;
}

export interface Registration {
  nodeId: string;
  endpoints: Endpoint[];
  metadata: Record<string, string>;
}

export interface RegistryClientConfig {
  mainAddress: string;
  clusterToken: string;
  pingIntervalMs?: number;
  registerTimeoutMs?: number;
}

type AnyJson = Record<string, unknown>;
type FramePayload = AnyJson | string;

export class RegistryClient {
  private readonly config: Required<RegistryClientConfig>;
  private readonly registration: Registration;
  private socket: net.Socket | null = null;
  private stateValue: ConnectionState = "DISCONNECTED";
  private running = false;
  private authFailed = false;
  private reconnectDelayMs = 3000;
  private pingTimer: NodeJS.Timeout | null = null;
  private buffered = Buffer.alloc(0);
  private inbox: AnyJson[] = [];
  private waiters: Array<{ variant: string; resolve: (v: AnyJson) => void }> = [];

  constructor(config: RegistryClientConfig, registration: Registration) {
    this.config = {
      mainAddress: config.mainAddress,
      clusterToken: config.clusterToken,
      pingIntervalMs: config.pingIntervalMs ?? 10_000,
      registerTimeoutMs: config.registerTimeoutMs ?? 5_000
    };
    this.registration = this.validateRegistration(registration);
  }

  get state(): ConnectionState {
    return this.stateValue;
  }

  async start(): Promise<void> {
    if (this.running) return;
    this.running = true;
    this.authFailed = false;
    this.reconnectDelayMs = 3000;
    await this.connectAndRegister(true);
  }

  async stop(): Promise<void> {
    this.running = false;
    this.stateValue = "STOPPING";
    this.sendDeregister();
    this.clearPing();
    await this.closeSocket();
    this.stateValue = "DISCONNECTED";
  }

  async listNodes(): Promise<AnyJson[]> {
    this.ensureConnected();
    this.send("ListNodes");
    const body = await this.waitVariant("NodeList", 5000);
    const nodes = body.nodes;
    return Array.isArray(nodes) ? nodes as AnyJson[] : [];
  }

  private async connectAndRegister(initial: boolean): Promise<void> {
    if (!this.running || this.authFailed) return;
    this.stateValue = initial ? "CONNECTING" : "RECONNECTING";
    const [host, portText] = this.config.mainAddress.split(":");
    const port = Number(portText);
    if (!host || Number.isNaN(port)) {
      throw new Error("mainAddress must be host:port");
    }

    await new Promise<void>((resolve, reject) => {
      const socket = net.createConnection({ host, port });
      this.socket = socket;
      socket.on("connect", () => resolve());
      socket.on("data", (chunk: Buffer) => this.handleData(chunk));
      socket.on("close", () => void this.handleClose());
      socket.on("error", reject);
    });

    try {
      this.stateValue = "REGISTERING";
      this.send(this.buildRegisterMessage());
      const ack = await this.waitVariant("RegisterAck", this.config.registerTimeoutMs);
      if (!ack.success) {
        this.authFailed = true;
        throw new Error(`register failed: ${String(ack.error ?? "unknown")}`);
      }
      this.stateValue = "CONNECTED";
      this.reconnectDelayMs = 3000;
      this.startPing();
    } catch (err) {
      await this.closeSocket();
      if (!this.authFailed) this.scheduleReconnect();
      throw err;
    }
  }

  private async handleClose(): Promise<void> {
    this.clearPing();
    if (this.running && !this.authFailed && this.stateValue !== "STOPPING") {
      this.scheduleReconnect();
    }
  }

  private scheduleReconnect(): void {
    this.stateValue = "RECONNECTING";
    const jitter = this.reconnectDelayMs * (0.1 + Math.random() * 0.1);
    const delay = Math.min(30_000, this.reconnectDelayMs + jitter);
    this.reconnectDelayMs = Math.min(30_000, this.reconnectDelayMs * 2);
    setTimeout(() => {
      void this.connectAndRegister(false).catch(() => undefined);
    }, delay);
  }

  private startPing(): void {
    this.clearPing();
    this.pingTimer = setInterval(() => {
      if (this.stateValue === "CONNECTED") {
        this.send("Ping");
      }
    }, this.config.pingIntervalMs);
  }

  private clearPing(): void {
    if (this.pingTimer) {
      clearInterval(this.pingTimer);
      this.pingTimer = null;
    }
  }

  private send(payload: FramePayload): void {
    const socket = this.socket;
    if (!socket || socket.destroyed) return;
    const data = Buffer.from(JSON.stringify(payload), "utf8");
    const head = Buffer.alloc(4);
    head.writeUInt32BE(data.length, 0);
    socket.write(Buffer.concat([head, data]));
  }

  private sendDeregister(): void {
    this.send({
      Deregister: {
        node_id: this.registration.nodeId,
        trace_context: null
      }
    });
  }

  private handleData(chunk: Buffer): void {
    this.buffered = Buffer.concat([this.buffered, chunk]);
    while (this.buffered.length >= 4) {
      const size = this.buffered.readUInt32BE(0);
      if (this.buffered.length < 4 + size) break;
      const payload = this.buffered.subarray(4, 4 + size);
      this.buffered = this.buffered.subarray(4 + size);
      const parsed = JSON.parse(payload.toString("utf8")) as unknown;
      if (parsed === "Ping") {
        this.send("Pong");
        continue;
      }
      if (parsed === "Pong") {
        continue;
      }
      if (parsed && typeof parsed === "object") {
        this.inbox.push(parsed as AnyJson);
      }
      this.drainWaiters();
    }
  }

  private drainWaiters(): void {
    if (this.waiters.length === 0 || this.inbox.length === 0) return;
    const nextInbox: AnyJson[] = [];
    for (const msg of this.inbox) {
      let consumed = false;
      for (const waiter of this.waiters) {
        const body = msg[waiter.variant];
        if (body !== undefined) {
          waiter.resolve(body as AnyJson);
          consumed = true;
          this.waiters = this.waiters.filter((w) => w !== waiter);
          break;
        }
      }
      if (!consumed) nextInbox.push(msg);
    }
    this.inbox = nextInbox;
  }

  private waitVariant(variant: string, timeoutMs: number): Promise<AnyJson> {
    for (const msg of this.inbox) {
      if (msg[variant] !== undefined) {
        return Promise.resolve(msg[variant] as AnyJson);
      }
    }
    return new Promise<AnyJson>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.waiters = this.waiters.filter((w) => w.resolve !== wrappedResolve);
        reject(new Error(`timeout waiting ${variant}`));
      }, timeoutMs);
      const wrappedResolve = (body: AnyJson) => {
        clearTimeout(timer);
        resolve(body);
      };
      this.waiters.push({ variant, resolve: wrappedResolve });
    });
  }

  private async closeSocket(): Promise<void> {
    const socket = this.socket;
    this.socket = null;
    if (!socket) return;
    await new Promise<void>((resolve) => {
      socket.once("close", () => resolve());
      socket.destroy();
    });
  }

  private ensureConnected(): void {
    if (this.stateValue !== "CONNECTED") {
      throw new Error("registry client is not connected");
    }
  }

  private buildRegisterMessage(): AnyJson {
    const metadata = { ...this.registration.metadata };
    if (!metadata["meta.schema"]) metadata["meta.schema"] = "rinfra.meta.v1";
    return {
      Register: {
        node_id: this.registration.nodeId,
        role: "Worker",
        endpoints: this.registration.endpoints.map((x) => ({ protocol: x.protocol, address: x.address })),
        metadata,
        token: this.config.clusterToken,
        trace_context: null
      }
    };
  }

  private validateRegistration(reg: Registration): Registration {
    if (reg.endpoints.length === 0) {
      throw new Error("endpoints must not be empty");
    }
    for (const endpoint of reg.endpoints) {
      if (!endpoint.protocol) throw new Error("endpoint protocol must not be empty");
      if (!endpoint.address.includes(":")) throw new Error("endpoint address must be host:port");
    }
    return reg;
  }
}
