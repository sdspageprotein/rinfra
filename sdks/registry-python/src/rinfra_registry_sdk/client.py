from __future__ import annotations

import asyncio
import enum
import json
import random
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Any


class ConnectionState(str, enum.Enum):
    DISCONNECTED = "DISCONNECTED"
    CONNECTING = "CONNECTING"
    REGISTERING = "REGISTERING"
    CONNECTED = "CONNECTED"
    RECONNECTING = "RECONNECTING"
    STOPPING = "STOPPING"


@dataclass(frozen=True)
class Endpoint:
    protocol: str
    address: str


@dataclass
class Registration:
    node_id: str
    endpoints: List[Endpoint]
    metadata: Dict[str, str] = field(default_factory=dict)


@dataclass(frozen=True)
class RegistryClientConfig:
    main_address: str
    cluster_token: str
    ping_interval_secs: float = 10.0
    register_timeout_secs: float = 5.0


class RegistryClient:
    def __init__(self, config: RegistryClientConfig, registration: Registration) -> None:
        self._config = config
        self._registration = self._validate_registration(registration)
        self._state = ConnectionState.DISCONNECTED
        self._running = False
        self._auth_failed = False
        self._reader: Optional[asyncio.StreamReader] = None
        self._writer: Optional[asyncio.StreamWriter] = None
        self._incoming: "asyncio.Queue[Any]" = asyncio.Queue()
        self._reader_task: Optional[asyncio.Task] = None
        self._ping_task: Optional[asyncio.Task] = None
        self._main_task: Optional[asyncio.Task] = None
        self._reconnect_delay = 3.0

    @property
    def state(self) -> ConnectionState:
        return self._state

    async def start(self) -> None:
        if self._running:
            return
        self._running = True
        self._auth_failed = False
        self._reconnect_delay = 3.0
        self._main_task = asyncio.create_task(self._run_loop())
        await self._wait_until_connected_or_failed()

    async def stop(self) -> None:
        self._running = False
        self._state = ConnectionState.STOPPING
        await self._send_deregister()
        await self._close_stream()
        if self._reader_task:
            self._reader_task.cancel()
        if self._ping_task:
            self._ping_task.cancel()
        if self._main_task:
            self._main_task.cancel()
        self._state = ConnectionState.DISCONNECTED

    async def list_nodes(self) -> List[dict[str, Any]]:
        self._ensure_connected()
        await self._send_message("ListNodes")
        body = await self._wait_variant("NodeList", 5.0)
        return body.get("nodes", [])

    async def _run_loop(self) -> None:
        while self._running and not self._auth_failed:
            try:
                await self._connect_and_register()
                self._state = ConnectionState.CONNECTED
                self._reconnect_delay = 3.0
                self._ping_task = asyncio.create_task(self._ping_loop())
                await self._reader_task
            except Exception:
                if self._auth_failed or not self._running:
                    break
                self._state = ConnectionState.RECONNECTING
                jitter = self._reconnect_delay * (0.1 + random.random() * 0.1)
                await asyncio.sleep(min(30.0, self._reconnect_delay + jitter))
                self._reconnect_delay = min(30.0, self._reconnect_delay * 2)
            finally:
                if self._ping_task:
                    self._ping_task.cancel()
                    self._ping_task = None
                await self._close_stream()
        self._state = ConnectionState.DISCONNECTED

    async def _connect_and_register(self) -> None:
        host, port = self._parse_host_port(self._config.main_address)
        self._state = ConnectionState.CONNECTING
        self._reader, self._writer = await asyncio.open_connection(host, port)
        self._reader_task = asyncio.create_task(self._reader_loop())

        self._state = ConnectionState.REGISTERING
        await self._send_message(self._register_message())
        ack = await self._wait_variant("RegisterAck", self._config.register_timeout_secs)
        if not ack.get("success", False):
            self._auth_failed = True
            raise RuntimeError(f"register failed: {ack.get('error', 'unknown')}")

    async def _reader_loop(self) -> None:
        assert self._reader is not None
        while self._running:
            header = await self._reader.readexactly(4)
            frame_len = int.from_bytes(header, byteorder="big", signed=False)
            payload = await self._reader.readexactly(frame_len)
            msg = json.loads(payload.decode("utf-8"))
            if msg == "Ping":
                await self._send_message("Pong")
                continue
            if msg == "Pong":
                continue
            await self._incoming.put(msg)

    async def _ping_loop(self) -> None:
        while self._running and self._state == ConnectionState.CONNECTED:
            await asyncio.sleep(self._config.ping_interval_secs)
            await self._send_message("Ping")

    async def _send_message(self, msg: Any) -> None:
        writer = self._writer
        if writer is None:
            return
        raw = json.dumps(msg, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
        writer.write(len(raw).to_bytes(4, byteorder="big") + raw)
        await writer.drain()

    async def _send_deregister(self) -> None:
        if self._writer is None:
            return
        await self._send_message({
            "Deregister": {
                "node_id": self._registration.node_id,
                "trace_context": None,
            }
        })

    async def _close_stream(self) -> None:
        writer = self._writer
        self._reader = None
        self._writer = None
        if writer is not None:
            writer.close()
            await writer.wait_closed()

    async def _wait_variant(self, variant: str, timeout_secs: float) -> dict[str, Any]:
        deadline = asyncio.get_event_loop().time() + timeout_secs
        while True:
            remain = deadline - asyncio.get_event_loop().time()
            if remain <= 0:
                raise TimeoutError(f"timeout waiting variant {variant}")
            msg = await asyncio.wait_for(self._incoming.get(), timeout=remain)
            if isinstance(msg, dict) and variant in msg:
                return msg[variant]

    async def _wait_until_connected_or_failed(self) -> None:
        while self._running:
            if self._state == ConnectionState.CONNECTED:
                return
            if self._auth_failed:
                raise RuntimeError("authentication failed")
            await asyncio.sleep(0.05)
        raise RuntimeError("client not running")

    def _register_message(self) -> dict[str, Any]:
        metadata = dict(self._registration.metadata)
        metadata.setdefault("meta.schema", "rinfra.meta.v1")
        return {
            "Register": {
                "node_id": self._registration.node_id,
                "role": "Worker",
                "endpoints": [{"protocol": e.protocol, "address": e.address} for e in self._registration.endpoints],
                "metadata": metadata,
                "token": self._config.cluster_token,
                "trace_context": None,
            }
        }

    @staticmethod
    def _parse_host_port(address: str) -> tuple[str, int]:
        host, port = address.rsplit(":", 1)
        return host, int(port)

    @staticmethod
    def _validate_registration(registration: Registration) -> Registration:
        if not registration.endpoints:
            raise ValueError("endpoints must not be empty")
        for endpoint in registration.endpoints:
            if not endpoint.protocol:
                raise ValueError("endpoint protocol must not be empty")
            if ":" not in endpoint.address:
                raise ValueError("endpoint address must be host:port")
        return registration

    def _ensure_connected(self) -> None:
        if self._state != ConnectionState.CONNECTED:
            raise RuntimeError("registry client is not connected")
