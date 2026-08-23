from __future__ import annotations

import asyncio
import ipaddress
import json
import secrets
import socket
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Any, Callable, Sequence

from fastapi import FastAPI, Header, HTTPException, Request, WebSocket, WebSocketDisconnect
from fastapi.exceptions import RequestValidationError
from fastapi.responses import HTMLResponse
from fastapi.staticfiles import StaticFiles

from . import __version__
from .api_contract import (
    AuthPolicy,
    control_endpoint,
    discovery_openapi_extra,
    discovery_responses,
    error_status,
    normalize_discovery_openapi,
    validate_request_values,
)
from .api_router import create_api_router, openapi_contract_errors
from .compiler import compile_world
from .errors import ServeError, UsageError, WedlError
from .query import status
from .repository import Repository
from .util import atomic_write


LOOPBACK_HOSTS = frozenset({"127.0.0.1", "localhost", "::1"})


def local_server_url(host: str, port: int) -> str:
    """Return the browser URL for an accepted loopback server address."""

    display_host = f"[{host}]" if ":" in host else host
    return f"http://{display_host}:{port}"


def _local_bind_candidates(host: str, port: int) -> tuple[str, list[tuple[int, int, int, tuple[object, ...]]]]:
    """Resolve and validate every distinct loopback bind candidate."""

    url = local_server_url(host, port)
    if host not in LOOPBACK_HOSTS:
        raise ServeError(
            "local server binds to loopback hosts only",
            details={"host": host, "port": port, "url": url, "hint": "Use 127.0.0.1, localhost, or ::1."},
        )
    try:
        addresses = socket.getaddrinfo(host, port, type=socket.SOCK_STREAM)
    except OSError as exc:
        raise ServeError(
            "could not resolve the local server address",
            details={"host": host, "port": port, "url": url, "hint": "Use 127.0.0.1 or another supported loopback host.", "reason": str(exc)},
        ) from exc
    if not addresses:
        raise ServeError(
            "could not resolve the local server address",
            details={"host": host, "port": port, "url": url, "hint": "Use 127.0.0.1 or another supported loopback host."},
        )
    candidates: list[tuple[int, int, int, tuple[object, ...]]] = []
    seen: set[tuple[int, int, int, tuple[object, ...]]] = set()
    for family, socket_type, protocol, _canonical_name, address in addresses:
        # getaddrinfo returns an IP address as the first sockaddr element for
        # AF_INET and AF_INET6.  Check every result rather than trusting the
        # hostname: a local name can resolve to more than one destination.
        resolved_host = str(address[0])
        try:
            is_loopback = ipaddress.ip_address(resolved_host).is_loopback
        except ValueError:
            is_loopback = False
        if not is_loopback:
            raise ServeError(
                "local server hostname resolves outside loopback",
                details={
                    "host": host,
                    "port": port,
                    "url": url,
                    "address": resolved_host,
                    "hint": "Use 127.0.0.1 or ::1; localhost must resolve only to loopback addresses.",
                },
            )
        candidate = (family, socket_type, protocol, tuple(address))
        if candidate not in seen:
            seen.add(candidate)
            candidates.append(candidate)
    return url, candidates


def _port_unavailable_error(host: str, port: int, url: str, exc: OSError) -> ServeError:
    return ServeError(
        "local server port is unavailable",
        details={
            "host": host,
            "port": port,
            "url": url,
            "hint": f"Stop the process using port {port} or choose a free port with --port.",
            "reason": str(exc),
        },
    )


def preflight_local_server(host: str, port: int) -> str:
    """Check every repository-independent local bind before starting Uvicorn.

    The probes are released once checked.  ``run_local_server`` subsequently
    reserves the same candidates and hands those sockets to Uvicorn, so a port
    collision after this fast preflight is still reported as ``ServeError``
    rather than becoming an unstructured server-startup failure.
    """

    url, candidates = _local_bind_candidates(host, port)

    # Validate every distinct socket Uvicorn may use.  Do this only after all
    # resolutions have passed the loopback check so an unsafe resolution never
    # reaches bind probing.
    for family, socket_type, protocol, address in candidates:
        try:
            with socket.socket(family, socket_type, protocol) as probe:
                probe.bind(address)
        except OSError as exc:
            raise _port_unavailable_error(host, port, url, exc) from exc
    return url


def reserve_local_server(host: str, port: int) -> tuple[str, list[socket.socket]]:
    """Reserve validated loopback sockets until Uvicorn takes ownership.

    Keeping the sockets bound closes the gap between preflight and server
    startup.  On any failed candidate all earlier reservations are released.
    """

    url, candidates = _local_bind_candidates(host, port)
    reserved: list[socket.socket] = []
    try:
        for family, socket_type, protocol, address in candidates:
            probe = socket.socket(family, socket_type, protocol)
            try:
                probe.bind(address)
            except OSError as exc:
                probe.close()
                raise _port_unavailable_error(host, port, url, exc) from exc
            reserved.append(probe)
    except Exception:
        for probe in reserved:
            probe.close()
        raise
    return url, reserved


def open_local_browser(url: str) -> None:
    """Open a ready local URL, preserving actionable CLI failures."""

    import webbrowser

    try:
        opened = webbrowser.open(url)
    except Exception as exc:
        raise ServeError(
            "could not open the local server in a browser",
            details={"url": url, "hint": f"Open {url} manually.", "reason": str(exc)},
        ) from exc
    if not opened:
        raise ServeError(
            "could not open the local server in a browser",
            details={"url": url, "hint": f"Open {url} manually."},
        )


def run_local_server(
    root: str | Path,
    *,
    host: str,
    port: int,
    on_ready: Callable[[], None] | None = None,
) -> None:
    """Run the local app and invoke ``on_ready`` exactly once after binding."""

    _url, sockets = reserve_local_server(host, port)
    import uvicorn

    class ReadyServer(uvicorn.Server):
        def __init__(self, config: Any) -> None:
            super().__init__(config)
            self._ready_notified = False

        async def startup(self, sockets: list[socket.socket] | None = None) -> None:
            await super().startup(sockets=sockets)
            if self.should_exit or self._ready_notified or on_ready is None:
                return
            self._ready_notified = True
            try:
                on_ready()
            except Exception:
                # Uvicorn has already created listening sockets.  Close them
                # before surfacing an actionable browser/reporting failure.
                await self.shutdown(sockets=sockets)
                raise

    try:
        ReadyServer(uvicorn.Config(create_app(root), host=host, port=port)).run(sockets=sockets)
    finally:
        # Uvicorn closes sockets on normal and callback-failure shutdown.  A
        # second close is harmless and covers failures before its lifecycle.
        for bound_socket in sockets:
            bound_socket.close()


class Runtime:
    def __init__(self, root: Path) -> None:
        self.repository = Repository(root)
        self.token_path = self.repository.root / ".wedl" / "session.json"
        self.token = self._token()
        self.write_lock = asyncio.Lock()
        self.clients: set[WebSocket] = set()
        self.stop = asyncio.Event()
        self.last_head = ""

    def _token(self) -> str:
        try:
            value = json.loads(self.token_path.read_text())
            if isinstance(value.get("token"), str): return value["token"]
        except (FileNotFoundError, json.JSONDecodeError): pass
        token = secrets.token_urlsafe(32)
        atomic_write(self.token_path, (json.dumps({"token": token}, indent=2) + "\n").encode())
        return token

    async def broadcast(self, payload: dict[str, Any]) -> None:
        dead = []
        for client in self.clients:
            try: await client.send_json(payload)
            except Exception: dead.append(client)
        for client in dead: self.clients.discard(client)

    async def watcher(self) -> None:
        while not self.stop.is_set():
            try:
                async with self.write_lock:
                    # Re-read under the same lock used by every writer.  An
                    # HTTP apply updates last_head before releasing that lock,
                    # so this watcher cannot recompile or rebroadcast it.
                    head = self.repository.head()
                    if head != self.last_head:
                        report = await asyncio.to_thread(compile_world, self.repository)
                        self.last_head = head
                        await self.broadcast({"type": "revision", "head": head, "compile": report})
            except Exception as exc:
                await self.broadcast({"type": "error", "message": str(exc)})
            try: await asyncio.wait_for(self.stop.wait(), timeout=1.0)
            except asyncio.TimeoutError: pass


def create_app(root: str | Path = ".") -> FastAPI:
    runtime = Runtime(Path(root))

    @asynccontextmanager
    async def lifespan(_app: FastAPI):
        await asyncio.to_thread(compile_world, runtime.repository)
        runtime.last_head = runtime.repository.head()
        task = asyncio.create_task(runtime.watcher())
        try: yield
        finally:
            runtime.stop.set(); task.cancel()
            try: await task
            except asyncio.CancelledError: pass

    app = FastAPI(title="wedl", version=__version__, lifespan=lifespan, docs_url="/api/docs")

    async def authorize(
        x_wedl_token: str | None = Header(
            default=None,
            alias=AuthPolicy.SESSION.header_name,
            description=AuthPolicy.SESSION.header_description,
        )
    ) -> None:
        if not secrets.compare_digest(x_wedl_token or "", runtime.token):
            raise HTTPException(status_code=401, detail="invalid X-Wedl-Token")

    @app.exception_handler(WedlError)
    async def wedl_error(_request: Any, exc: WedlError):
        from fastapi.responses import JSONResponse
        return JSONResponse(exc.as_dict(), status_code=error_status(exc.code))

    @app.exception_handler(RequestValidationError)
    async def request_validation_error(_request: Request, exc: RequestValidationError):
        from fastapi.responses import JSONResponse
        return JSONResponse(
            {
                "code": "usage_error",
                "message": "invalid API request",
                "details": {"errors": exc.errors()},
            },
            status_code=400,
        )

    @app.exception_handler(HTTPException)
    async def http_error(_request: Request, exc: HTTPException):
        from fastapi.responses import JSONResponse
        if exc.status_code == 401:
            return JSONResponse(
                {"code": "authentication_required", "message": "invalid X-Wedl-Token", "details": {}},
                status_code=401,
            )
        return JSONResponse({"code": "http_error", "message": str(exc.detail), "details": {}}, status_code=exc.status_code)

    @app.middleware("http")
    async def reject_unknown_api_fields(request: Request, call_next: Any):
        try:
            validate_request_values(request.method, request.url.path, request.query_params)
        except UsageError as exc:
            from fastapi.responses import JSONResponse
            return JSONResponse(exc.as_dict(), status_code=400)
        return await call_next(request)

    app.include_router(create_api_router(runtime, authorize))

    @app.websocket("/ws")
    async def websocket(socket: WebSocket, token: str) -> None:
        if not secrets.compare_digest(token, runtime.token): await socket.close(code=4401); return
        await socket.accept(); runtime.clients.add(socket)
        try:
            await socket.send_json({"type": "status", "status": await asyncio.to_thread(status, runtime.repository)})
            while True: await socket.receive_text()
        except WebSocketDisconnect: pass
        finally: runtime.clients.discard(socket)

    static = Path(__file__).with_name("static")
    app.mount("/assets", StaticFiles(directory=static), name="assets")

    root_contract = control_endpoint("GET", "/")

    @app.get(
        "/",
        response_class=HTMLResponse,
        operation_id="root_get",
        summary=root_contract.descriptor.summary,
        description=root_contract.descriptor.description,
        tags=list(root_contract.descriptor.tags),
        responses=discovery_responses(root_contract.descriptor),
        openapi_extra=discovery_openapi_extra(root_contract.descriptor, auth=root_contract.auth),
    )
    async def root_page() -> str: return (static / "index.html").read_text()

    # FastAPI produces operation mechanics; the contract owns the reusable
    # WEDL error vocabulary.  Merge components after route registration so
    # operation response references and components stay generated together.
    fastapi_openapi = app.openapi

    def documented_openapi() -> dict[str, Any]:
        return normalize_discovery_openapi(fastapi_openapi())

    app.openapi = documented_openapi  # type: ignore[method-assign]

    # Run the same two-way comparison used in tests against the production
    # application document.  A parser/route/OpenAPI drift then prevents a
    # server with a misleading integration contract from starting.
    contract_errors = openapi_contract_errors(app.openapi())
    if contract_errors:
        raise RuntimeError("OpenAPI contract drift: " + "; ".join(contract_errors))
    return app
