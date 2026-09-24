from __future__ import annotations

import json
import socket
import sys
import types
import webbrowser
from pathlib import Path

import pytest

from wedl.cli import dispatch, main, parser
from wedl.errors import NotFound, ServeError
from wedl.model import World
from wedl.server import local_server_url, open_local_browser, preflight_local_server, run_local_server
from wedl.source import parse_record


class _Repository:
    def __init__(self, path: str) -> None:
        self.root = Path(path)

    def load_world(self, *, cache_write: bool = True) -> object:
        assert cache_write is False
        record = parse_record(
            b"""---
schema: wedl/v0.3
kind: world
id: world_00000000000000000000000000
title: Test world
domain: world
status: canonical
tags: []
aliases: []
default_timeline: main
timelines:
  - id: main
    label: Main chronology
---
# Test world
""",
            "story/world.md",
        )
        return World("WORKTREE", "tree", {record.id: record}, self.root)


class _GitRootWithoutWedlWorld:
    def __init__(self, path: str) -> None:
        self.root = Path(path)
        self.cache_write_values: list[bool] = []

    def load_world(self, *, cache_write: bool = True) -> object:
        self.cache_write_values.append(cache_write)

        class _EmptyWorld:
            @property
            def world_record(self) -> object:
                raise NotFound("expected exactly one world record, found 0")

        return _EmptyWorld()


def test_local_server_url_brackets_ipv6() -> None:
    assert local_server_url("127.0.0.1", 8765) == "http://127.0.0.1:8765"
    assert local_server_url("::1", 8765) == "http://[::1]:8765"


def test_serve_dispatch_announces_ready_without_opening_browser(monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]) -> None:
    monkeypatch.setattr("wedl.cli.Repository", _Repository)
    monkeypatch.setattr("wedl.cli.preflight_local_server", lambda host, port: "http://127.0.0.1:8765")
    opened: list[str] = []
    monkeypatch.setattr("wedl.cli.open_local_browser", opened.append)

    def server(_root: Path, *, host: str, port: int, on_ready: object) -> None:
        assert (host, port) == ("127.0.0.1", 8765)
        assert callable(on_ready)
        on_ready()

    monkeypatch.setattr("wedl.cli.run_local_server", server)
    assert dispatch(parser().parse_args(["serve", "--repo", "world"])) is None
    assert opened == []
    assert capsys.readouterr().out == "Ready: http://127.0.0.1:8765 (press Ctrl+C to stop)\n"


def test_serve_ready_output_flushes_for_piped_callers(monkeypatch: pytest.MonkeyPatch) -> None:
    class _Pipe:
        def __init__(self) -> None:
            self.output = ""
            self.flushes = 0

        def write(self, value: str) -> int:
            self.output += value
            return len(value)

        def flush(self) -> None:
            self.flushes += 1

    pipe = _Pipe()
    monkeypatch.setattr("wedl.cli.Repository", _Repository)
    monkeypatch.setattr("wedl.cli.preflight_local_server", lambda host, port: "http://127.0.0.1:8765")
    monkeypatch.setattr("wedl.cli.run_local_server", lambda _root, *, host, port, on_ready: on_ready())
    monkeypatch.setattr(sys, "stdout", pipe)

    assert dispatch(parser().parse_args(["serve", "--repo", "world"])) is None
    assert pipe.output == "Ready: http://127.0.0.1:8765 (press Ctrl+C to stop)\n"
    assert pipe.flushes == 1


def test_serve_dispatch_opens_once_after_ready_and_compact_announces_url(monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]) -> None:
    monkeypatch.setattr("wedl.cli.Repository", _Repository)
    monkeypatch.setattr("wedl.cli.preflight_local_server", lambda host, port: "http://[::1]:8765")
    opened: list[str] = []
    monkeypatch.setattr("wedl.cli.open_local_browser", opened.append)
    monkeypatch.setattr("wedl.cli.run_local_server", lambda _root, *, host, port, on_ready: on_ready())

    assert dispatch(parser().parse_args(["--compact", "serve", "--repo", "world", "--host", "::1", "--open"])) is None
    assert opened == ["http://[::1]:8765"]
    assert json.loads(capsys.readouterr().out) == {"url": "http://[::1]:8765"}


def test_serve_port_preflight_failure_is_structured_and_does_not_start_server(monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]) -> None:
    monkeypatch.setattr("wedl.cli.Repository", _Repository)
    monkeypatch.setattr("wedl.cli.preflight_local_server", lambda host, port: (_ for _ in ()).throw(ServeError("local server port is unavailable", details={"host": host, "port": port, "hint": "Choose a free port with --port."})))
    monkeypatch.setattr("wedl.cli.run_local_server", lambda *_args, **_kwargs: pytest.fail("server should not start"))

    assert main(["serve", "--repo", "world"]) == 2
    payload = json.loads(capsys.readouterr().err)
    assert payload["code"] == "serve_error"
    assert payload["details"]["hint"] == "Choose a free port with --port."


def test_serve_repository_preflight_failure_is_structured_and_does_not_bind(capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr("wedl.cli.preflight_local_server", lambda *_args: pytest.fail("port should not be checked before repository"))

    assert main(["serve", "--repo", "not-a-wedl-repository"]) == 2
    payload = json.loads(capsys.readouterr().err)
    assert payload["code"] == "repository_error"
    assert "--repo" in payload["details"]["hint"]


def test_serve_rejects_git_root_without_a_wedl_world_before_socket_activity(
    capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    instances: list[_GitRootWithoutWedlWorld] = []

    def repository(path: str) -> _GitRootWithoutWedlWorld:
        instance = _GitRootWithoutWedlWorld(path)
        instances.append(instance)
        return instance

    monkeypatch.setattr("wedl.cli.Repository", repository)
    monkeypatch.setattr("wedl.cli.preflight_local_server", lambda *_args: pytest.fail("socket preflight must follow WEDL validation"))
    monkeypatch.setattr("wedl.cli.run_local_server", lambda *_args, **_kwargs: pytest.fail("server must not reserve sockets"))

    assert main(["serve", "--repo", "plain-git-root"]) == 2
    payload = json.loads(capsys.readouterr().err)
    assert payload["code"] == "repository_error"
    assert "WEDL world" in payload["message"]
    assert instances[0].cache_write_values == [False]


def test_serve_rejects_an_invalid_parseable_world_before_socket_activity(
    capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    record = parse_record(
        b"""---
schema: wedl/v0.3
kind: world
id: world_00000000000000000000000000
title: Invalid but parseable world
domain: world
status: canonical
tags: []
aliases: []
default_timeline: missing
timelines:
  - id: main
    label: Main chronology
---
# Invalid but parseable world
""",
        "story/world.md",
    )
    world = World("WORKTREE", "tree", {record.id: record}, Path("plain-git-root"))

    class _InvalidWedlRepository:
        root = Path("plain-git-root")

        def load_world(self, *, cache_write: bool = True) -> World:
            assert cache_write is False
            return world

    monkeypatch.setattr("wedl.cli.Repository", lambda _path: _InvalidWedlRepository())
    monkeypatch.setattr("wedl.cli.preflight_local_server", lambda *_args: pytest.fail("socket preflight must follow semantic validation"))
    monkeypatch.setattr("wedl.cli.run_local_server", lambda *_args, **_kwargs: pytest.fail("server must not reserve sockets"))

    assert main(["serve", "--repo", "plain-git-root"]) == 2
    payload = json.loads(capsys.readouterr().err)
    assert payload["code"] == "validation_failed"
    assert payload["details"]["diagnostics"] == [
        {
            "code": "WDL-TIMELINE-011",
            "message": "default timeline 'missing' is not declared",
            "severity": "error",
            "entityId": record.id,
            "path": "story/world.md",
            "field": "default_timeline",
        }
    ]


def test_serve_open_failure_is_structured_after_readiness(monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]) -> None:
    monkeypatch.setattr("wedl.cli.Repository", _Repository)
    monkeypatch.setattr("wedl.cli.preflight_local_server", lambda host, port: "http://127.0.0.1:8765")
    monkeypatch.setattr("wedl.cli.open_local_browser", lambda _url: (_ for _ in ()).throw(ServeError("could not open the local server in a browser", details={"hint": "Open it manually."})))
    monkeypatch.setattr("wedl.cli.run_local_server", lambda _root, *, host, port, on_ready: on_ready())

    assert main(["serve", "--repo", "world", "--open"]) == 2
    captured = capsys.readouterr()
    assert captured.out == ""
    assert json.loads(captured.err) == {
        "code": "serve_error",
        "message": "could not open the local server in a browser",
        "details": {"hint": "Open it manually."},
    }


def test_preflight_reports_port_conflicts_with_actionable_details(monkeypatch: pytest.MonkeyPatch) -> None:
    class _Probe:
        def __enter__(self) -> _Probe:
            return self

        def __exit__(self, *_args: object) -> None:
            return None

        def bind(self, _address: object) -> None:
            raise OSError("already in use")

    monkeypatch.setattr(socket, "getaddrinfo", lambda *_args, **_kwargs: [(socket.AF_INET, socket.SOCK_STREAM, 0, "", ("127.0.0.1", 8765))])
    monkeypatch.setattr(socket, "socket", lambda *_args, **_kwargs: _Probe())

    with pytest.raises(ServeError) as raised:
        preflight_local_server("127.0.0.1", 8765)
    assert raised.value.code == "serve_error"
    assert raised.value.details["url"] == "http://127.0.0.1:8765"
    assert "--port" in raised.value.details["hint"]


def test_preflight_checks_every_distinct_resolved_bind_candidate(monkeypatch: pytest.MonkeyPatch) -> None:
    attempted: list[object] = []

    class _Probe:
        def __enter__(self) -> _Probe:
            return self

        def __exit__(self, *_args: object) -> None:
            return None

        def bind(self, address: object) -> None:
            attempted.append(address)
            if address == ("::1", 8765, 0, 0):
                raise OSError("already in use")

    monkeypatch.setattr(
        socket,
        "getaddrinfo",
        lambda *_args, **_kwargs: [
            (socket.AF_INET, socket.SOCK_STREAM, 0, "", ("127.0.0.1", 8765)),
            (socket.AF_INET6, socket.SOCK_STREAM, 0, "", ("::1", 8765, 0, 0)),
            (socket.AF_INET, socket.SOCK_STREAM, 0, "", ("127.0.0.1", 8765)),
        ],
    )
    monkeypatch.setattr(socket, "socket", lambda *_args, **_kwargs: _Probe())

    with pytest.raises(ServeError) as raised:
        preflight_local_server("localhost", 8765)

    assert raised.value.code == "serve_error"
    assert attempted == [("127.0.0.1", 8765), ("::1", 8765, 0, 0)]


def test_preflight_rejects_a_nonloopback_hostname_resolution_before_binding(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(
        socket,
        "getaddrinfo",
        lambda *_args, **_kwargs: [
            (socket.AF_INET, socket.SOCK_STREAM, 0, "", ("127.0.0.1", 8765)),
            (socket.AF_INET, socket.SOCK_STREAM, 0, "", ("192.0.2.1", 8765)),
        ],
    )
    monkeypatch.setattr(socket, "socket", lambda *_args, **_kwargs: pytest.fail("unsafe resolution must not bind"))

    with pytest.raises(ServeError) as raised:
        preflight_local_server("localhost", 8765)

    assert raised.value.code == "serve_error"
    assert raised.value.details["address"] == "192.0.2.1"
    assert "loopback" in raised.value.details["hint"]


def test_serve_translates_a_port_collision_after_preflight_before_uvicorn_starts(monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]) -> None:
    attempts: list[str] = []

    class _Probe:
        def __init__(self, name: str) -> None:
            self.name = name

        def __enter__(self) -> _Probe:
            return self

        def __exit__(self, *_args: object) -> None:
            self.close()

        def bind(self, _address: object) -> None:
            attempts.append(self.name)
            if self.name == "reservation":
                raise OSError("already in use")

        def close(self) -> None:
            return None

    probes = iter([_Probe("preflight"), _Probe("reservation")])
    monkeypatch.setattr("wedl.cli.Repository", _Repository)
    monkeypatch.setattr(
        socket,
        "getaddrinfo",
        lambda *_args, **_kwargs: [(socket.AF_INET, socket.SOCK_STREAM, 0, "", ("127.0.0.1", 8765))],
    )
    monkeypatch.setattr(socket, "socket", lambda *_args, **_kwargs: next(probes))
    monkeypatch.setattr(
        "wedl.server.create_app",
        lambda _root: pytest.fail("Uvicorn must not start after a collision"),
    )

    assert main(["serve", "--repo", "world"]) == 2
    payload = json.loads(capsys.readouterr().err)
    assert payload["code"] == "serve_error"
    assert "--port" in payload["details"]["hint"]
    assert attempts == ["preflight", "reservation"]


def test_ready_server_calls_back_once_across_repeated_startups(monkeypatch: pytest.MonkeyPatch) -> None:
    callbacks: list[str] = []
    lifecycle: list[str] = []

    class _Config:
        def __init__(self, *_args: object, **_kwargs: object) -> None:
            return None

    class _Server:
        def __init__(self, _config: object) -> None:
            self.should_exit = False

        async def startup(self, sockets: object = None) -> None:
            lifecycle.append("startup")

        async def shutdown(self, sockets: object = None) -> None:
            lifecycle.append("shutdown")

        def run(self, sockets: object = None) -> None:
            import asyncio

            asyncio.run(self.startup(sockets=sockets))
            asyncio.run(self.startup(sockets=sockets))

    class _Socket:
        def close(self) -> None:
            lifecycle.append("close")

    monkeypatch.setattr("wedl.server.reserve_local_server", lambda *_args: ("http://127.0.0.1:8765", [_Socket()]))
    monkeypatch.setattr("wedl.server.create_app", lambda _root: object())
    monkeypatch.setitem(sys.modules, "uvicorn", types.SimpleNamespace(Config=_Config, Server=_Server))

    run_local_server("world", host="127.0.0.1", port=8765, on_ready=lambda: callbacks.append("ready"))

    assert callbacks == ["ready"]
    assert lifecycle == ["startup", "startup", "close"]


def test_ready_server_shuts_down_when_the_ready_callback_fails(monkeypatch: pytest.MonkeyPatch) -> None:
    lifecycle: list[str] = []

    class _Config:
        def __init__(self, *_args: object, **_kwargs: object) -> None:
            return None

    class _Server:
        def __init__(self, _config: object) -> None:
            self.should_exit = False

        async def startup(self, sockets: object = None) -> None:
            lifecycle.append("startup")

        async def shutdown(self, sockets: object = None) -> None:
            lifecycle.append("shutdown")

        def run(self, sockets: object = None) -> None:
            import asyncio

            asyncio.run(self.startup(sockets=sockets))

    class _Socket:
        def close(self) -> None:
            lifecycle.append("close")

    monkeypatch.setattr("wedl.server.reserve_local_server", lambda *_args: ("http://127.0.0.1:8765", [_Socket()]))
    monkeypatch.setattr("wedl.server.create_app", lambda _root: object())
    monkeypatch.setitem(sys.modules, "uvicorn", types.SimpleNamespace(Config=_Config, Server=_Server))

    with pytest.raises(RuntimeError, match="browser unavailable"):
        run_local_server("world", host="127.0.0.1", port=8765, on_ready=lambda: (_ for _ in ()).throw(RuntimeError("browser unavailable")))

    assert lifecycle == ["startup", "shutdown", "close"]


def test_preflight_keeps_the_server_loopback_only() -> None:
    with pytest.raises(ServeError) as raised:
        preflight_local_server("0.0.0.0", 8765)
    assert raised.value.details["host"] == "0.0.0.0"
    assert "127.0.0.1" in raised.value.details["hint"]


def test_browser_open_failure_has_a_manual_url_fallback(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(webbrowser, "open", lambda _url: False)

    with pytest.raises(ServeError) as raised:
        open_local_browser("http://127.0.0.1:8765")
    assert raised.value.code == "serve_error"
    assert raised.value.details["url"] == "http://127.0.0.1:8765"
    assert "manually" in raised.value.details["hint"]


def test_serve_help_explains_readiness_and_opt_in_opening() -> None:
    command = next(action for action in parser()._actions if action.dest == "command").choices["serve"]
    help_text = " ".join(command.format_help().split())
    assert "checks the repository and local port before it blocks" in help_text
    assert "--open" in help_text
    assert "do not open a browser" in help_text


def test_generational_cli_preserves_semantic_exit_matrix(monkeypatch: pytest.MonkeyPatch,
                                                        capsys: pytest.CaptureFixture[str]) -> None:
    monkeypatch.setattr("wedl.cli.Repository", _Repository)
    monkeypatch.setattr("wedl.cli._json_file", lambda _path: {"protocol": "wedl-generational/v1"})
    for state, expected_exit in (("available", 0), ("unknown", 0),
                                 ("invalid", 2), ("unavailable", 2), ("limit", 2)):
        outcome = {"protocol": "wedl-generational/v1", "operation": "parents",
                   "revision": "0" * 40, "state": state}
        monkeypatch.setattr("wedl.cli.generational_execute",
                            lambda _repo, _operation, _request, *, require_compiled: outcome)
        assert main(["generational", "parents", "-", "--repo", "world"]) == expected_exit
        captured = capsys.readouterr()
        assert json.loads(captured.out if expected_exit == 0 else captured.err) == outcome
        assert (captured.err if expected_exit == 0 else captured.out) == ""
