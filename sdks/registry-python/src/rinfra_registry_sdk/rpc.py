from __future__ import annotations

import asyncio
import random
from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Awaitable, Callable, Dict, List, Optional, Protocol

from .client import Endpoint, RegistryClient


class RpcErrorCode(str, Enum):
    TIMEOUT = "timeout"
    UNAVAILABLE = "unavailable"
    NOT_FOUND = "not_found"
    INVALID_ARGUMENT = "invalid_argument"
    INTERNAL = "internal"
    UNAUTHENTICATED = "unauthenticated"
    PERMISSION_DENIED = "permission_denied"
    CANCELLED = "cancelled"
    UNKNOWN = "unknown"


class RpcError(RuntimeError):
    def __init__(
        self,
        code: RpcErrorCode,
        message: str,
        *,
        service: Optional[str] = None,
        method: Optional[str] = None,
        cause: Optional[BaseException] = None,
    ) -> None:
        super().__init__(message)
        self.code = code
        self.service = service
        self.method = method
        self.cause = cause


@dataclass(frozen=True)
class RetryPolicy:
    max_attempts: int = 3
    base_delay_ms: int = 100
    max_delay_ms: int = 1000
    jitter: bool = True
    retry_on: set[RpcErrorCode] = field(
        default_factory=lambda: {RpcErrorCode.UNAVAILABLE, RpcErrorCode.TIMEOUT}
    )


@dataclass(frozen=True)
class ResolveOptions:
    service: str
    protocol: str = "grpc"
    service_version: str = ""
    zone: str = ""


@dataclass(frozen=True)
class CallOptions:
    timeout_ms: int = 2000
    retry: RetryPolicy = field(default_factory=RetryPolicy)
    metadata: Dict[str, str] = field(default_factory=dict)


@dataclass(frozen=True)
class NodeInfo:
    id: str
    endpoints: List[Endpoint]
    metadata: Dict[str, str]


class NodeProvider(Protocol):
    async def list_nodes(self) -> List[NodeInfo]: ...


class RegistryNodeProvider:
    def __init__(self, registry_client: RegistryClient) -> None:
        self._registry_client = registry_client

    async def list_nodes(self) -> List[NodeInfo]:
        raw_nodes = await self._registry_client.list_nodes()
        result: List[NodeInfo] = []
        for raw in raw_nodes:
            endpoints = [
                Endpoint(protocol=e.get("protocol", ""), address=e.get("address", ""))
                for e in raw.get("endpoints", [])
                if isinstance(e, dict)
            ]
            metadata = raw.get("metadata", {})
            if not isinstance(metadata, dict):
                metadata = {}
            result.append(
                NodeInfo(
                    id=str(raw.get("id", "")),
                    endpoints=endpoints,
                    metadata={str(k): str(v) for k, v in metadata.items()},
                )
            )
        return result


class Resolver:
    def __init__(self, provider: NodeProvider) -> None:
        self._provider = provider

    async def list(self, options: ResolveOptions) -> List[Endpoint]:
        nodes = await self._provider.list_nodes()
        candidates: List[Endpoint] = []
        for node in nodes:
            if options.service and node.metadata.get("service.name") != options.service:
                continue
            if options.service_version and node.metadata.get("service.version") != options.service_version:
                continue
            if options.zone and node.metadata.get("service.zone") != options.zone:
                continue
            for endpoint in node.endpoints:
                if endpoint.protocol.lower() == options.protocol.lower():
                    candidates.append(endpoint)
        if not candidates:
            raise RpcError(
                RpcErrorCode.NOT_FOUND,
                "no endpoint matched resolve filters",
                service=options.service,
            )
        return candidates

    async def resolve(self, options: ResolveOptions) -> Endpoint:
        endpoints = await self.list(options)
        return random.choice(endpoints)


UnaryCall = Callable[[Endpoint, str, Any, CallOptions], Awaitable[Any]]


class RpcInvoker:
    def __init__(self, resolver: Resolver, unary_call: UnaryCall) -> None:
        self._resolver = resolver
        self._unary_call = unary_call

    async def invoke_unary(
        self,
        endpoint: Endpoint,
        method: str,
        request: Any,
        options: Optional[CallOptions] = None,
    ) -> Any:
        opts = options or CallOptions()
        try:
            return await asyncio.wait_for(
                self._unary_call(endpoint, method, request, opts),
                timeout=opts.timeout_ms / 1000.0,
            )
        except Exception as exc:  # noqa: BLE001
            raise map_error(exc, service=None, method=method) from exc

    async def invoke_unary_by_service(
        self,
        resolve_options: ResolveOptions,
        method: str,
        request: Any,
        call_options: Optional[CallOptions] = None,
    ) -> Any:
        opts = call_options or CallOptions()
        last_error: Optional[RpcError] = None
        for attempt in range(1, max(1, opts.retry.max_attempts) + 1):
            endpoint = await self._resolver.resolve(resolve_options)
            try:
                return await self.invoke_unary(endpoint, method, request, opts)
            except RpcError as exc:
                last_error = exc
                if exc.code not in opts.retry.retry_on or attempt >= opts.retry.max_attempts:
                    raise
                backoff_ms = min(
                    opts.retry.max_delay_ms,
                    opts.retry.base_delay_ms * (2 ** (attempt - 1)),
                )
                if opts.retry.jitter:
                    backoff_ms += int(backoff_ms * 0.25 * random.random())
                await asyncio.sleep(backoff_ms / 1000.0)
        if last_error is None:
            raise RpcError(RpcErrorCode.UNKNOWN, "rpc failed", service=resolve_options.service, method=method)
        raise last_error


def map_error(exc: BaseException, *, service: Optional[str], method: Optional[str]) -> RpcError:
    if isinstance(exc, RpcError):
        return exc
    if isinstance(exc, asyncio.TimeoutError):
        return RpcError(RpcErrorCode.TIMEOUT, "rpc timeout", service=service, method=method, cause=exc)
    if isinstance(exc, asyncio.CancelledError):
        return RpcError(RpcErrorCode.CANCELLED, "rpc cancelled", service=service, method=method, cause=exc)
    grpc_code = _map_grpc_status(exc)
    if grpc_code is not None:
        return RpcError(grpc_code, str(exc), service=service, method=method, cause=exc)
    return RpcError(RpcErrorCode.UNKNOWN, "rpc failed", service=service, method=method, cause=exc)


def _map_grpc_status(exc: BaseException) -> Optional[RpcErrorCode]:
    try:
        import grpc  # type: ignore
    except Exception:  # noqa: BLE001
        return None
    if not isinstance(exc, grpc.RpcError):
        return None
    code = exc.code()
    mapping = {
        grpc.StatusCode.DEADLINE_EXCEEDED: RpcErrorCode.TIMEOUT,
        grpc.StatusCode.UNAVAILABLE: RpcErrorCode.UNAVAILABLE,
        grpc.StatusCode.NOT_FOUND: RpcErrorCode.NOT_FOUND,
        grpc.StatusCode.INVALID_ARGUMENT: RpcErrorCode.INVALID_ARGUMENT,
        grpc.StatusCode.INTERNAL: RpcErrorCode.INTERNAL,
        grpc.StatusCode.UNAUTHENTICATED: RpcErrorCode.UNAUTHENTICATED,
        grpc.StatusCode.PERMISSION_DENIED: RpcErrorCode.PERMISSION_DENIED,
        grpc.StatusCode.CANCELLED: RpcErrorCode.CANCELLED,
    }
    return mapping.get(code, RpcErrorCode.UNKNOWN)
