"""Reproducible spatial source-to-compiled scale probe.

Run ``--help`` for the isolated smoke and explicit large-envelope commands.
The generated Git repository is disposable; this script never writes to the
repository being measured. Compiled-projection probes are labeled separately.
"""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
from contextlib import closing, contextmanager, nullcontext
import ctypes
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import signal
import sqlite3
import statistics
import stat
import subprocess
import sys
import threading
import time
from typing import Any
from uuid import uuid4

from fastapi.testclient import TestClient

from wedl.compiler import compile_world, connect, require_database
from wedl.repository import Repository
from wedl.server import create_app
from wedl.source import serialize_record
from wedl.spatial_query import MAX_GEOMETRY_CANDIDATES, MAX_OVERLAY_CANDIDATES, MAX_ROUTE_EXPANSIONS
from wedl.spatial_query import BoundingBox, SpatialStore
from wedl.spatial_explorer_api import PROTOCOL as EXPLORER_PROTOCOL
from wedl.spatial_api import PROTOCOL as SPATIAL_PROTOCOL
from wedl.spatial_index import insert_spatial_index
from wedl.model import StoryTime

from benchmark_spatial_index import _insert_benchmark_entities
from benchmark_spatial_query import _projection


CAPABILITIES = ["spatial-core-v1", "geometry-v1", "route-v1", "overlay-v1"]
DEFAULT_BASE = Path(__file__).resolve().parents[1]
VM_BUDGETS = {"children": 50_000, "sparseBbox": 500_000,
              "childrenPage2": 50_000, "sparseHighBbox": 500_000,
              "routesIncoming": 500_000, "routesOutgoing": 500_000,
              "searchBroad": 500_000, "path": 2_000_000, "overlay": 500_000}


class PhaseTrace:
    """Write completed source-stage phases before a long run can be interrupted."""

    def __init__(self, output: Path):
        self.path = output / "phase-events.jsonl"
        if self.path.exists() or (output / "manifest.json").exists():
            raise FileExistsError(f"source-stage evidence already exists: {output}")
        self.phases: list[dict[str, Any]] = []

    @contextmanager
    def measure(self, name: str):
        started = time.perf_counter()
        cpu_started = time.process_time()
        before = _process_memory()
        peak = [before["workingSetBytes"]]
        stop = threading.Event()

        def sample():
            while not stop.wait(0.1):
                observed = _process_memory()["workingSetBytes"]
                if observed is not None:
                    peak[0] = observed if peak[0] is None else max(peak[0], observed)

        sampler = threading.Thread(target=sample, daemon=True)
        sampler.start()
        try:
            yield
        finally:
            stop.set()
            sampler.join()
            after = _process_memory()
            if after["workingSetBytes"] is not None:
                peak[0] = (after["workingSetBytes"] if peak[0] is None
                           else max(peak[0], after["workingSetBytes"]))
            event = {"phase": name, "wallMs": round((time.perf_counter() - started) * 1000, 3),
                     "parentCpuMs": round((time.process_time() - cpu_started) * 1000, 3),
                     "parentWorkingSetStartBytes": before["workingSetBytes"],
                     "parentWorkingSetEndBytes": after["workingSetBytes"],
                     "parentPeakWorkingSetBytes": peak[0],
                     "parentLifetimePeakWorkingSetBytes": after["peakWorkingSetBytes"]}
            self.phases.append(event)
            with self.path.open("a", encoding="utf-8") as stream:
                stream.write(json.dumps(event, sort_keys=True) + "\n")


def _finalize_run(trace: PhaseTrace, sandbox: Path, output: Path,
                  manifest: dict[str, Any] | None, successful: bool) -> None:
    if sandbox.resolve().parent != output.resolve():
        raise RuntimeError("refusing to remove a sandbox outside the evidence directory")

    def writable_retry(action, path, error):
        os.chmod(path, 0o777)
        action(path)

    with trace.measure("sandbox-teardown"):
        shutil.rmtree(sandbox, onexc=writable_retry)
    if successful:
        if manifest is None:
            raise RuntimeError("successful source stage has no manifest")
        manifest["phaseTrace"] = list(trace.phases)
        temporary = output / f"manifest-{uuid4().hex}.tmp"
        try:
            temporary.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n",
                                 encoding="utf-8")
            os.replace(temporary, output / "manifest.json")
        finally:
            temporary.unlink(missing_ok=True)


class CommandCleanupError(RuntimeError):
    """An owned command may still hold the fixture; manual reaping is required."""


def _run_command(command: list[str], *, input: bytes | None = None,
                 stdin=None, timeout: float = 60, check: bool = True) -> subprocess.CompletedProcess:
    """Bound both command execution and descendant/pipe cleanup."""
    process = subprocess.Popen(command, stdin=subprocess.PIPE if input is not None else stdin,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               start_new_session=os.name != "nt")
    try:
        stdout, stderr = process.communicate(input, timeout=timeout)
    except subprocess.TimeoutExpired as failure:
        try:
            if process.poll() is None:
                if os.name == "nt":
                    subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                                   capture_output=True, check=True, timeout=10)
                else:
                    os.killpg(process.pid, signal.SIGKILL)
            stdout, stderr = process.communicate(timeout=10)
            failure.output, failure.stderr = stdout[-32768:], stderr[-32768:]
        except Exception as cleanup:
            raise CommandCleanupError(f"command cleanup failed; retain owned fixture and report: {cleanup}") from failure
        raise
    result = subprocess.CompletedProcess(command, process.returncode, stdout, stderr)
    if check:
        result.check_returncode()
    return result


def _git(root: Path, *args: str) -> str:
    result = _run_command(["git", "-C", str(root), *args], check=False)
    if result.returncode:
        raise RuntimeError(f"git {' '.join(args)} failed: {result.stderr.decode().strip()}")
    return result.stdout.decode().strip()


def _write(root: Path, group: str, record: dict[str, Any]) -> None:
    path = root / "story" / group / (record["id"].replace(":", "-") + ".md")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(serialize_record(record, f"# {record['title']}\n"))


def _commit_generated(root: Path, sandbox: Path, parent: str,
                      trace: PhaseTrace | None = None) -> dict[str, Any]:
    """Commit fresh authored files in one Git stream, then prove tree parity.

    This operates only on the newly copied disposable repository. It refuses
    attributes/filters rather than bypassing Git's normal source conversion.
    The source files remain the input, and every blob and mode is verified.
    """
    if root.resolve(strict=True).parent != sandbox.resolve(strict=True):
        raise RuntimeError("generated fixture is outside its sandbox")

    def regular(path: Path, *, directory: bool) -> os.stat_result:
        info = path.lstat()
        if (stat.S_ISLNK(info.st_mode) or
                getattr(info, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT or
                (not stat.S_ISDIR(info.st_mode) if directory else not stat.S_ISREG(info.st_mode))):
            raise RuntimeError(f"generated fixture contains an alias or special file: {path}")
        return info

    for directory in (sandbox, root, root / ".git", root / "story"):
        regular(directory, directory=True)
    if _git(root, "rev-parse", "HEAD") != parent:
        raise RuntimeError("generated fixture parent changed")
    branch = _git(root, "symbolic-ref", "HEAD")
    if not branch.startswith("refs/heads/"):
        raise RuntimeError("generated fixture needs its copied local branch")
    files: list[Path] = []
    file_states: dict[Path, tuple[int, int, int, int, int]] = {}
    directory_states: dict[Path, tuple[int, int, int, int, int]] = {}

    def state(info: os.stat_result) -> tuple[int, int, int, int, int]:
        return info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, info.st_ctime_ns

    def read_authored(path: Path) -> bytes:
        if state(regular(path, directory=False)) != file_states[path]:
            raise RuntimeError("generated fixture source changed before reading")
        data = path.read_bytes()
        if state(regular(path, directory=False)) != file_states[path]:
            raise RuntimeError("generated fixture source changed while reading")
        return data

    for directory, subdirectories, names in os.walk(root / "story", followlinks=False):
        directory_states[Path(directory)] = state(regular(Path(directory), directory=True))
        for name in subdirectories:
            regular(Path(directory) / name, directory=True)
        for name in names:
            path = Path(directory) / name
            file_states[path] = state(regular(path, directory=False))
            relative = path.relative_to(root).as_posix()
            if not relative.isascii() or any(character in relative for character in '\n\r"\\'):
                raise RuntimeError("generated fixture path cannot be streamed literally")
            files.append(path)
    files.sort(key=lambda path: path.relative_to(root).as_posix())
    paths = b"".join(path.relative_to(root).as_posix().encode() + b"\0" for path in files)
    attributes = _run_command(["git", "-C", str(root), "check-attr", "-z", "--all", "--stdin"], input=paths)
    if attributes.stdout:
        raise RuntimeError("generated fixture attributes require conventional Git staging")
    ignored = _run_command(["git", "-C", str(root), "check-ignore", "-z", "--no-index", "--stdin"],
                           input=paths, check=False)
    if ignored.returncode not in (0, 1) or ignored.stdout:
        raise RuntimeError("generated fixture ignore rules require conventional Git staging")
    autocrlf = _run_command(["git", "-C", str(root), "config", "--get", "core.autocrlf"],
                            check=False, timeout=10)
    if autocrlf.returncode not in (0, 1) or autocrlf.stdout.strip().lower() not in (b"", b"false", b"input"):
        raise RuntimeError("generated fixture line conversion is not supported by stream staging")
    algorithm = _git(root, "rev-parse", "--show-object-format")
    if algorithm not in {"sha1", "sha256"}:
        raise RuntimeError("unsupported Git object format")

    def tree(revision: str) -> dict[bytes, bytes]:
        result = _run_command(["git", "-C", str(root), "ls-tree", "-rz", revision])
        return dict((entry.split(b"\t", 1)[1], entry.split(b"\t", 1)[0])
                    for entry in result.stdout.split(b"\0") if entry)

    before = tree(parent)
    expected = {path: value for path, value in before.items() if not path.startswith(b"story/")}
    stream_path = sandbox / "generated-import.fi"
    created_stream = False
    measure = trace.measure if trace is not None else lambda _: nullcontext()
    try:
        with measure("git-authored-read-and-blob-stream"), stream_path.open("xb") as stream:
            created_stream = True
            # Cold per-file opens dominate this authored Windows workload.
            # Read a bounded batch concurrently, retaining source order and
            # hashing the actual file bytes rather than generator records.
            with ThreadPoolExecutor(max_workers=16) as readers:
                for offset in range(0, len(files), 64):
                    batch = files[offset:offset + 64]
                    for path, data in zip(batch, readers.map(read_authored, batch)):
                        if b"\r" in data:
                            raise RuntimeError("generated fixture needs conventional line conversion")
                        relative = path.relative_to(root).as_posix().encode()
                        oid = hashlib.new(algorithm, f"blob {len(data)}\0".encode() + data).hexdigest().encode()
                        expected[relative] = b"100644 blob " + oid
                        stream.write(b"blob\n")
                        stream.write(f"data {len(data)}\n".encode() + data + b"\n")
            stream.write(b"done\n")
        with measure("git-blob-import"), stream_path.open("rb") as stream:
            _run_command(["git", "-C", str(root), "fast-import", "--quiet", "--done"],
                         stdin=stream, timeout=300)
        with measure("git-native-tree-and-proof"):
            # Build trees in bulk, avoiding fast-import's per-path tree updates for
            # the hundreds of thousands of siblings in the authored route folder.
            directories: dict[bytes, list[bytes]] = {b"story": []}
            for path, value in expected.items():
                if path.startswith(b"story/"):
                    directory, name = path.rsplit(b"/", 1)
                    directories.setdefault(directory, []).append(value + b"\t" + name + b"\0")
                    while directory != b"story":
                        directory = directory.rsplit(b"/", 1)[0]
                        directories.setdefault(directory, [])
            story_oid = b""
            for directory in sorted(directories, key=lambda path: path.count(b"/"), reverse=True):
                oid = _run_command(["git", "-C", str(root), "mktree", "-z"],
                                   input=b"".join(directories[directory])).stdout.strip()
                if directory == b"story":
                    story_oid = oid
                else:
                    ancestor, name = directory.rsplit(b"/", 1)
                    directories[ancestor].append(b"040000 tree " + oid + b"\t" + name + b"\0")
            parent_entries = _run_command(["git", "-C", str(root), "ls-tree", "-z", parent]).stdout
            root_entries = [entry + b"\0" for entry in parent_entries.split(b"\0")
                            if entry and entry.split(b"\t", 1)[1] != b"story"]
            root_entries.append(b"040000 tree " + story_oid + b"\tstory\0")
            root_oid = _run_command(["git", "-C", str(root), "mktree", "-z"],
                                    input=b"".join(root_entries)).stdout.decode().strip()
            revision = _run_command(["git", "-C", str(root), "-c", "user.name=spatial scale",
                                     "-c", "user.email=scale@test.invalid", "commit-tree", root_oid,
                                     "-p", parent], input=b"generated spatial scale fixture\n").stdout.decode().strip()
            if _git(root, "rev-parse", f"{revision}^") != parent or tree(revision) != expected:
                raise RuntimeError("generated Git tree differs from exact authored bytes/modes/base tree")
            for path, snapshot in file_states.items():
                if state(regular(path, directory=False)) != snapshot:
                    raise RuntimeError("generated fixture source changed after reading")
            for path, snapshot in directory_states.items():
                if state(regular(path, directory=True)) != snapshot:
                    raise RuntimeError("generated fixture source set changed after reading")
            if _git(root, "symbolic-ref", "HEAD") != branch:
                raise RuntimeError("generated fixture branch changed")
            _git(root, "update-ref", branch, revision, parent)
            # Populate the disposable index directly; a worktree refresh would walk
            # every generated file again. The checked Git tree is authoritative.
            _git(root, "read-tree", "HEAD")
            return {"kind": "fresh-authored-git-stream", "sourceFiles": len(files),
                    "parent": parent, "treeOid": _git(root, "rev-parse", "HEAD^{tree}"),
                    "verifiedBlobModes": True, "verifiedNonStoryTree": True}
    finally:
        if created_stream and not isinstance(sys.exc_info()[1], CommandCleanupError):
            stream_path.unlink(missing_ok=True)


def generate(root: Path, *, places: int, maps: int, routes: int,
             portals: int, overlays: int) -> dict[str, int]:
    """Generate authored v0.7 records, including deep/wide topology."""
    if places < 128 or maps < 1 or min(routes, portals, overlays) < 0:
        raise ValueError("places >=128, maps >=1 and nonnegative edge counts required")
    story = root / "story"
    if story.resolve().parent != root.resolve():
        raise RuntimeError("refusing to replace source outside the disposable repository")
    if story.exists():
        shutil.rmtree(story)
    _write(root, "", {"schema": "wedl/v0.7", "kind": "world",
        "id": "world_00000000000000000000000000", "title": "Spatial scale world",
        "capabilities": CAPABILITIES, "default_timeline": "main",
        "timelines": [{"id": "main", "label": "Main"}],
        "chronology": {"calendars": [], "eras": [], "anchors": []}})
    for i in range(maps):
        three_dimensional = i % 4 == 1
        _write(root, "maps", {"schema": "wedl/v0.7", "kind": "map",
            "id": f"map:scale-{i}", "title": f"Scale map {i}",
            "crs": f"local-planar:scale-{i}", "axis_order": ["east", "north"],
            "unit": "pace" if i % 2 == 0 else "tile",
            "z_policy": "required" if three_dimensional else "forbidden",
            "bounds": {"min": [-1_000_000, -1_000_000, -100] if three_dimensional else [-1_000_000, -1_000_000],
                       "max": [1_000_000, 1_000_000, 100] if three_dimensional else [1_000_000, 1_000_000]}})
    for i in range(places):
        if i == 0:
            parent = None
        elif i < 128:
            parent = f"location:scale-{i-1}"
        elif i < 10_128:
            parent = "location:scale-0"
        else:
            parent = f"location:scale-{i-1}" if (i-10_128) % 128 else "location:scale-0"
        record: dict[str, Any] = {"schema": "wedl/v0.7", "kind": "location",
            "id": f"location:scale-{i}", "title": f"Scale place {i:06d}"}
        if parent:
            record["parent_id"] = parent
        if i % 17 != 0:
            x = (i % 100_001) - 50_000
            y = (i * 7 % 100_001) - 50_000
            map_index = i % maps
            coordinates = [x, y, (i % 101) - 50] if map_index % 4 == 1 else [x, y]
            record["spatial"] = {"map_id": f"map:scale-{map_index}",
                "geometry": {"kind": "point", "coordinates": coordinates}}
        _write(root, "locations", record)
    for i in range(routes):
        _write(root, "routes", {"schema": "wedl/v0.7", "kind": "route",
            "id": f"route:scale-{i}", "title": f"Scale route {i:06d}",
            "from_location_id": f"location:scale-{i % places}",
            "to_location_id": f"location:scale-{(i % places + 1 + i // places) % places}",
            "direction": "one-way", "modes": ["foot"],
            "route_distance": {"value": 1, "unit": "pace"}})
    for i in range(portals):
        _write(root, "portals", {"schema": "wedl/v0.7", "kind": "portal",
            "id": f"portal:scale-{i}", "title": f"Scale portal {i}",
            "from_location_id": f"location:scale-{i % places}",
            "to": f"location:scale-{(i+1) % places}", "modes": ["foot"]})
    for i in range(overlays):
        _write(root, "overlays", {"schema": "wedl/v0.7", "kind": "overlay",
            "id": f"overlay:scale-{i}", "title": f"Scale overlay {i}",
            "lifecycle": "time-bounded",
            "membership": {"location_ids": [f"location:scale-{i % places}"]},
            "audience": ["author"], "perspectives": ["author"],
            "valid": {"start": {"timeline": "main", "tick": i, "order": 0},
                      "end": {"timeline": "main", "tick": i+1, "order": 0}}})
    return {"places": places, "hierarchyDepth": min(128, places),
            "rootSiblings": max(0, min(places-128, 10_000)), "maps": maps,
            "threeDimensionalMaps": sum(i % 4 == 1 for i in range(maps)),
            "routes": routes, "directedEdges": routes, "portals": portals,
            "overlays": overlays, "coordinateFreePlaces": (places+16)//17}


def _sample(action, repeats: int) -> dict[str, Any]:
    samples = []
    result_hash = None
    outcome = None
    for _ in range(repeats):
        start = time.perf_counter()
        result = action()
        samples.append(round((time.perf_counter()-start)*1000, 3))
        encoded = json.dumps(result, sort_keys=True, separators=(",", ":"),
                             ensure_ascii=False, default=str).encode("utf-8")
        digest = hashlib.sha256(encoded).hexdigest()
        if result_hash is not None and result_hash != digest:
            raise RuntimeError("repeated query result changed")
        result_hash = digest
        payload = result.get("result")
        outcome = {"state": result.get("state"), "code": result.get("code"),
                   "resultKeys": sorted(payload or {}),
                   "recordCounts": {key: len(value) for key, value in (payload or {}).items()
                                    if isinstance(value, list)},
                   "hasCursor": bool((payload or {}).get("nextCursor")),
                   "noPartialFeatureData": payload is None if result.get("state") in {"limit", "invalid"} else None}
    return {"samplesMs": samples, "medianMs": statistics.median(samples),
            "worstMs": max(samples), "resultSha256": result_hash, "outcome": outcome}


def _vm_case(connection: sqlite3.Connection, action) -> dict[str, Any]:
    steps = [0]
    def count_steps() -> int:
        steps[0] += 100
        return 0
    connection.set_progress_handler(count_steps, 100)
    try:
        started = time.perf_counter()
        result = action()
        elapsed = round((time.perf_counter()-started)*1000, 3)
    finally:
        connection.set_progress_handler(None, 0)
    return {"state": result.kind.value, "vmStepsUpperBound": steps[0] + 99,
            "wallMs": elapsed}


def _vm_sql(connection: sqlite3.Connection, action) -> dict[str, Any]:
    """Measure the bounded SQL seek used by an explorer API operation."""
    steps = [0]
    def count_steps() -> int:
        steps[0] += 100
        return 0
    connection.set_progress_handler(count_steps, 100)
    try:
        started = time.perf_counter()
        rows = action()
        elapsed = round((time.perf_counter()-started)*1000, 3)
    finally:
        connection.set_progress_handler(None, 0)
    return {"state": "ok", "rows": len(rows), "vmStepsUpperBound": steps[0] + 99,
            "wallMs": elapsed}


def _children_page_two_vm(store: SpatialStore, parent: str, limit: int) -> dict[str, Any]:
    # Cursor preparation is separate from the operation-level page-two cost.
    first = store.children(parent, limit=limit)
    if first.value is None or not first.value.cursor:
        raise RuntimeError("children page-two measurement requires a production cursor")
    return _vm_case(store.connection, lambda: store.children(
        parent, limit=limit, cursor=first.value.cursor))


def _host() -> dict[str, Any]:
    return {"platform": platform.platform(), "python": sys.version,
            "sqlite": sqlite3.sqlite_version, "cpuCount": os.cpu_count()}


def _implementation_provenance(base: Path, *, required: bool) -> dict[str, Any]:
    """Pin imported WEDL code/assets to exact committed bytes for release runs."""
    configured = os.environ.get("WEDL_CODE_ROOT")
    if not configured:
        if required:
            raise RuntimeError("full envelope requires WEDL_CODE_ROOT exact-HEAD archive")
        return {"kind": "worktree-unpinned", "executable": sys.executable}
    code_root = Path(configured).resolve(strict=True)
    if (code_root / ".git").exists() or code_root == base:
        raise RuntimeError("WEDL_CODE_ROOT must be a separate exact-HEAD source archive")
    source_root = (code_root / "src").resolve(strict=True)
    paths: dict[str, dict[str, str]] = {}
    for name, module in sorted(sys.modules.items()):
        if name != "wedl" and not name.startswith("wedl."):
            continue
        file_name = getattr(module, "__file__", None)
        if not file_name or not file_name.endswith(".py"):
            continue
        source = Path(file_name).resolve(strict=True)
        try:
            relative = source.relative_to(source_root).as_posix()
        except ValueError as exc:
            raise RuntimeError(f"WEDL module imported outside clean archive: {name}: {source}") from exc
        committed = subprocess.run(["git", "-C", str(base), "show", f"HEAD:src/{relative}"],
                                   check=True, capture_output=True).stdout
        actual = source.read_bytes()
        if actual != committed:
            raise RuntimeError(f"WEDL module differs from exact HEAD: {name}: {relative}")
        paths[name] = {"path": str(source), "sha256": hashlib.sha256(actual).hexdigest()}
    for asset in ("spatial.html", "spatial_app.mjs", "spatial_api.mjs", "spatial_renderer.mjs",
                  "spatial.css", "favicon.svg", "index.html", "app.js"):
        relative = f"wedl/static/{asset}"
        source = source_root / relative
        committed = subprocess.run(["git", "-C", str(base), "show", f"HEAD:src/{relative}"],
                                   check=True, capture_output=True).stdout
        actual = source.read_bytes()
        if actual != committed:
            raise RuntimeError(f"browser asset differs from exact HEAD: {relative}")
        paths[f"asset:{asset}"] = {"path": str(source), "sha256": hashlib.sha256(actual).hexdigest()}
    if not paths:
        raise RuntimeError("no WEDL modules were imported from clean archive")
    return {"kind": "exact-HEAD-archive", "codeRoot": str(code_root),
            "executable": sys.executable, "pythonPath": os.environ.get("PYTHONPATH", ""),
            "sysPath": sys.path, "files": paths}


def _process_memory() -> dict[str, int | None]:
    """Process working set and process-lifetime peak where the host exposes it."""
    if os.name != "nt":
        return {"workingSetBytes": None, "peakWorkingSetBytes": None}
    class Counters(ctypes.Structure):
        _fields_ = [("cb", ctypes.c_ulong), ("pageFaultCount", ctypes.c_ulong),
                    ("peakWorkingSetSize", ctypes.c_size_t), ("workingSetSize", ctypes.c_size_t),
                    ("quotaPeakPagedPoolUsage", ctypes.c_size_t),
                    ("quotaPagedPoolUsage", ctypes.c_size_t),
                    ("quotaPeakNonPagedPoolUsage", ctypes.c_size_t),
                    ("quotaNonPagedPoolUsage", ctypes.c_size_t),
                    ("pagefileUsage", ctypes.c_size_t),
                    ("peakPagefileUsage", ctypes.c_size_t)]
    value = Counters()
    value.cb = ctypes.sizeof(value)
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    psapi = ctypes.WinDLL("psapi", use_last_error=True)
    kernel.GetCurrentProcess.restype = ctypes.c_void_p
    psapi.GetProcessMemoryInfo.argtypes = [ctypes.c_void_p, ctypes.POINTER(Counters), ctypes.c_ulong]
    psapi.GetProcessMemoryInfo.restype = ctypes.c_int
    if not psapi.GetProcessMemoryInfo(kernel.GetCurrentProcess(), ctypes.byref(value), value.cb):
        return {"workingSetBytes": None, "peakWorkingSetBytes": None}
    return {"workingSetBytes": value.workingSetSize,
            "peakWorkingSetBytes": value.peakWorkingSetSize}


def compiled_browser_fixture(source_fixture: Path, output: Path) -> dict[str, Any]:
    """Build a 100k *compiled-only* browser fixture from a small authored seed.

    This deliberately modifies a disposable cache and cannot prove source
    parsing, validation, or rebuild at the 100k envelope.
    """
    source_fixture = source_fixture.resolve(strict=True)
    output.mkdir(parents=True, exist_ok=True)
    fixture = output / "fixture"
    if fixture.exists():
        raise FileExistsError(f"compiled browser fixture already exists: {fixture}")
    shutil.copytree(source_fixture, fixture, copy_function=shutil.copyfile)
    database = fixture / ".wedl" / "world.sqlite"
    projection = _projection(locations=100_000, maps=32,
        routes=250_000, portals=100, overlays=10_000)
    started = time.perf_counter()
    with closing(sqlite3.connect(database)) as connection:
        for table in ("spatial_location_rtree", "spatial_overlay_time_rtree"):
            connection.execute(f"DROP TABLE IF EXISTS {table}")
        tables = [str(row[0]) for row in connection.execute(
            "SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'spatial_%' "
            "AND name NOT LIKE '%_rtree_%'")]
        for table in tables:
            connection.execute(f'DELETE FROM "{table}"')
        connection.execute("DELETE FROM entity WHERE kind IN ('map','location','route','anchor','portal','overlay')")
        _insert_benchmark_entities(connection, projection)
        connection.executemany("UPDATE entity SET title=? WHERE id=?",
            ((f"Bench place {i:06d}", f"location:bench-{i}") for i in range(100_000)))
        connection.executemany("UPDATE entity SET title=? WHERE id=?",
            ((f"Bench map {i}", f"map:bench-{i}") for i in range(32)))
        connection.executemany(
            "INSERT INTO search_document VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            ((f"benchmark:place:{i}", f"location:bench-{i}", "entity", None,
              "public", None, None, None, None, None, None, None, "", "{}", "")
             for i in range(100_000)),
        )
        connection.executemany(
            "INSERT INTO search_fts(rowid,title,aliases,heading,text,domain,tags) VALUES (?,?,?,?,?,?,?)",
            ((rowid, f"Bench place {int(entity_id.rsplit('-', 1)[1]):06d}", "", "", "", "", "")
             for rowid, entity_id in connection.execute(
                 "SELECT rowid,entity_id FROM search_document WHERE document_id LIKE 'benchmark:place:%'")),
        )
        stats = insert_spatial_index(connection, projection)
        connection.commit()
        rows = {table: connection.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
                for table in ("spatial_map", "spatial_location", "spatial_route_edge",
                              "spatial_portal", "spatial_overlay")}
    expected = {"spatial_map": 32, "spatial_location": 100_000,
                "spatial_route_edge": 250_000, "spatial_portal": 100,
                "spatial_overlay": 10_000}
    if rows != expected:
        raise RuntimeError(f"compiled browser rows mismatch: {rows}")
    result = {"kind": "compiled-projection-browser-only", "sourceFixtureCommit": _git(source_fixture, "rev-parse", "HEAD"),
              "fixturePath": str(fixture.resolve()), "compiledRows": rows,
              "insert": stats, "wallMs": round((time.perf_counter()-started)*1000, 3),
              "databaseBytes": database.stat().st_size,
              "databaseSha256": hashlib.sha256(database.read_bytes()).hexdigest(),
              "host": _host(), "processMemory": _process_memory(),
              "limitation": "Cache rows are synthetic and intentionally disagree with authored source; browser scaling only, not source-to-browser proof."}
    (output / "manifest.json").write_text(json.dumps(result, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    return result


def run(*, base: Path, output: Path, places: int = 256, maps: int = 2,
        routes: int = 128, portals: int = 2, overlays: int = 8,
        repeats: int = 3, retain_fixture: bool = False,
        retained_fixture: Path | None = None,
        reuse_compiled: bool = False) -> dict[str, Any]:
    """Measure generated or retained authored source using pinned implementation."""
    if repeats < 1:
        raise ValueError("repeats must be positive")
    if retained_fixture is not None and retain_fixture:
        raise ValueError("retained fixture cannot be retained again")
    if reuse_compiled and retained_fixture is None:
        raise ValueError("compiled-cache preflight requires a retained authored fixture")
    base = base.resolve(strict=True)
    base_head = _git(base, "rev-parse", "HEAD")
    provenance = _implementation_provenance(base, required=retained_fixture is not None or places >= 100_000)
    output.mkdir(parents=True, exist_ok=True)
    trace = PhaseTrace(output)
    successful = False
    manifest: dict[str, Any] | None = None
    preparation: dict[str, Any] | None = None
    sandbox = output / f"run-{uuid4().hex}"
    sandbox.mkdir()
    try:
        env_temp = sandbox / "temp"
        env_temp.mkdir()
        old_temp, old_tmp = os.environ.get("TEMP"), os.environ.get("TMP")
        os.environ["TEMP"] = os.environ["TMP"] = str(env_temp)
        try:
            if retained_fixture is None:
                clone = sandbox / "repository"
                if clone.resolve().parent != sandbox.resolve():
                    raise RuntimeError("disposable clone is outside the benchmark scratch directory")
                clone.mkdir()
                with trace.measure("git-copy"):
                    shutil.copytree(base / ".git", clone / ".git", copy_function=shutil.copyfile)
                with trace.measure("git-sparse-reset"):
                    _git(clone, "config", "core.longpaths", "true")
                    _git(clone, "sparse-checkout", "init", "--cone")
                    _git(clone, "sparse-checkout", "set", "story")
                    _git(clone, "reset", "--hard", base_head)
                if _git(clone, "rev-parse", "HEAD") != base_head:
                    raise RuntimeError("disposable clone revision differs from measured commit")
                status = _git(clone, "status", "--porcelain")
                if status:
                    raise RuntimeError(f"disposable sparse checkout is not clean: {status[:1000]}")
                with trace.measure("source-generation"):
                    counts = generate(clone, places=places, maps=maps, routes=routes,
                                      portals=portals, overlays=overlays)
                with trace.measure("git-stream-and-tree-verification"):
                    preparation = _commit_generated(clone, sandbox, base_head, trace)
            else:
                clone = retained_fixture.resolve(strict=True)
                owned = (base / "output" / "spatial-release-scale").resolve(strict=True)
                if owned not in clone.parents or not (clone / ".git").exists():
                    raise RuntimeError("retained fixture must be an owned disposable Git fixture")
                source_manifest = json.loads((clone.parent / "manifest.json").read_text(encoding="utf-8"))
                if source_manifest.get("baseCommit") != base_head or source_manifest.get("fixtureCommit") != _git(clone, "rev-parse", "HEAD"):
                    raise RuntimeError("retained authored fixture commit does not match source manifest")
                if _git(clone, "status", "--porcelain", "--", "story"):
                    raise RuntimeError("retained authored source has uncommitted changes")
                counts = source_manifest["counts"]
                places, maps, routes, portals, overlays = (
                    counts[key] for key in ("places", "maps", "routes", "portals", "overlays"))
            with trace.measure("source-repository-open"):
                repo = Repository(clone)
                fixture_head = repo.head()
            compile_runs = []
            for run_number in range(0 if reuse_compiled else 2):
                started = time.perf_counter()
                with trace.measure(f"forced-compile-{run_number + 1}"):
                    built = compile_world(repo, "HEAD", force=True)
                compile_runs.append({"wallMs": round((time.perf_counter()-started)*1000, 3),
                                     "status": built["status"], "recordCount": built.get("recordCount"),
                                     "databaseBytes": built.get("databaseBytes"),
                                     "timingsMs": built.get("timingsMs"),
                                     "processMemory": _process_memory()})
                if built["status"] != "compiled":
                    raise RuntimeError(f"source compile failed: {built}")
            with trace.measure("compiled-database-open"):
                compiled, database = require_database(repo, require_compiled=True)
            common = {"protocol": EXPLORER_PROTOCOL, "revision": fixture_head,
                      "capabilities": CAPABILITIES, "limit": 20, "cursor": None}
            bounds = {"min": [-50000, -50000], "max": [-49990, -49900]}
            cases = {
                "catalog": ("GET", "/api/spatial/explorer/catalog", {"limit": 20}),
                "roots": ("POST", "/api/spatial/explorer/places", {**common, "mode": "roots"}),
                "children": ("POST", "/api/spatial/explorer/places", {**common, "mode": "children", "parentId": "location:scale-0"}),
                "search": ("POST", "/api/spatial/explorer/places", {**common, "mode": "search", "query": "Scale place"}),
                "viewport": ("POST", "/api/spatial/explorer/viewport", {**common, "mapId": "map:scale-0", "bounds": bounds, "relation": "intersects"}),
                "routes": ("POST", "/api/spatial/explorer/routes", {**common, "locationId": "location:scale-0", "direction": "outgoing"}),
                "routesIncoming": ("POST", "/api/spatial/explorer/routes", {**common, "locationId": "location:scale-1", "direction": "incoming"}),
                "layers": ("POST", "/api/spatial/explorer/layers", {**common, "mapId": "map:scale-0", "bounds": bounds, "relation": "intersects", "asOf": {"timeline": "main", "tick": "0", "order": "0"}, "audience": "author", "perspective": "author"}),
                "path": ("POST", "/api/spatial/path", {**common, "protocol": SPATIAL_PROTOCOL,
                    "fromLocationId": "location:scale-0", "toLocationId": "location:scale-1",
                    "metric": "routeDistance"}),
            }
            if places > 2_000 and routes > 1_000:
                cases["path-beyond-cap"] = ("POST", "/api/spatial/path", {
                    **common, "protocol": SPATIAL_PROTOCOL,
                    "fromLocationId": "location:scale-0",
                    "toLocationId": f"location:scale-{min(places-1, 6_000)}",
                    "metric": "routeDistance"})
            if places >= 100_000 and maps >= 32:
                cases["viewportSparseHigh"] = ("POST", "/api/spatial/explorer/viewport", {
                    **common, "mapId": "map:scale-31",
                    "bounds": {"min": [49_990, 49_980], "max": [50_000, 50_000]},
                    "relation": "intersects"})
            api: dict[str, Any] = {}
            pagination: dict[str, Any] = {}
            with TestClient(create_app(clone)) as client:
                for name, (method, path, body) in cases.items():
                    def invoke(method=method, path=path, body=body):
                        response = client.get(path, params=body) if method == "GET" else client.post(path, json=body)
                        return response.json()
                    with trace.measure(f"api-{name}"):
                        api[name] = _sample(invoke, repeats)
                    expected_state = "limit" if (places > 2_000 and name == "search") or (
                        routes > 1_000 and name in {"path", "path-beyond-cap"}) else "ok"
                    if api[name]["outcome"]["state"] != expected_state:
                        raise RuntimeError(f"{name} returned {api[name]['outcome']}")
                    if expected_state == "limit" and (api[name]["outcome"]["code"] != "SPATIAL-LIMIT-001"
                                                      or not api[name]["outcome"]["noPartialFeatureData"]):
                        raise RuntimeError(f"{name} did not close without partial data")
                if counts["rootSiblings"] >= 40:
                    first = client.post(cases["children"][1], json=cases["children"][2]).json()
                    first_result = first.get("result") or {}
                    cursor = first_result.get("nextCursor")
                    first_ids = [card["id"] for card in first_result.get("places", [])]
                    if first.get("state") != "ok" or first.get("revision") != fixture_head or not cursor or len(first_ids) != 20:
                        raise RuntimeError("authored children first page lacks exact revision or full bounded cursor")
                    page_body = {**cases["children"][2], "cursor": cursor}
                    def second_page():
                        return client.post(cases["children"][1], json=page_body).json()
                    with trace.measure("api-childrenPage2"):
                        api["childrenPage2"] = _sample(second_page, repeats)
                    page = second_page()
                    second_ids = [card["id"] for card in (page.get("result") or {}).get("places", [])]
                    if (page.get("state") != "ok" or page.get("revision") != fixture_head or
                            not second_ids or len(second_ids) > 20 or set(first_ids) & set(second_ids)):
                        raise RuntimeError("authored children second page is not exact-revision disjoint pagination")
                    rebound_body = {**page_body, "parentId": "location:scale-1"}
                    with trace.measure("api-childrenCursorRebound"):
                        rebound = _sample(lambda: client.post(cases["children"][1], json=rebound_body).json(), repeats)
                    if rebound["outcome"]["state"] != "invalid" or rebound["outcome"]["code"] != "SPATIAL-CURSOR-001" or not rebound["outcome"]["noPartialFeatureData"]:
                        raise RuntimeError("authored cursor rebound did not close without partial data")
                    pagination = {"revision": fixture_head, "limit": 20,
                                  "firstIdsSha256": hashlib.sha256(json.dumps(first_ids).encode()).hexdigest(),
                                  "secondIdsSha256": hashlib.sha256(json.dumps(second_ids).encode()).hexdigest(),
                                  "firstCount": len(first_ids), "secondCount": len(second_ids),
                                  "disjoint": True, "cursorSha256": hashlib.sha256(cursor.encode()).hexdigest(),
                                  "rebound": rebound}
            with trace.measure("query-vm-and-counts"), closing(connect(database, True)) as connection:
                rows = {table: connection.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
                        for table in ("spatial_map", "spatial_location", "spatial_route_edge",
                                      "spatial_portal", "spatial_overlay")}
                store = SpatialStore(connection, fixture_head)
                vm = {
                    "children": _vm_case(connection, lambda: store.children("location:scale-0", limit=20)),
                    "sparseBbox": _vm_case(connection, lambda: store.bbox(
                        "map:scale-0", BoundingBox((-50_000, -50_000), (-49_990, -49_900)), limit=20)),
                    "routesOutgoing": _vm_sql(connection, lambda: connection.execute(
                        "SELECT e.to_location_id,e.route_id,e.reverse_of_authored "
                        "FROM spatial_route_edge AS e INDEXED BY spatial_route_edge_from_idx "
                        "WHERE e.from_location_id=? ORDER BY e.to_location_id,e.route_id,e.reverse_of_authored LIMIT 21",
                        ("location:scale-0",)).fetchall()),
                    "routesIncoming": _vm_sql(connection, lambda: connection.execute(
                        "SELECT e.from_location_id,e.route_id,e.reverse_of_authored "
                        "FROM spatial_route_edge AS e INDEXED BY spatial_route_edge_to_idx "
                        "WHERE e.to_location_id=? ORDER BY e.from_location_id,e.route_id,e.reverse_of_authored LIMIT 21",
                        ("location:scale-1",)).fetchall()),
                    "searchBroad": _vm_sql(connection, lambda: connection.execute(
                        "SELECT doc.entity_id FROM search_fts JOIN search_document AS doc ON doc.rowid=search_fts.rowid "
                        "WHERE search_fts MATCH ? LIMIT 2001", ('title : "Scale place"',)).fetchall()),
                    "path": _vm_case(connection, lambda: store.path(
                        "location:scale-0", "location:scale-1", metric="route_distance", limit=20)),
                    "overlay": _vm_case(connection, lambda: store.overlay_as_of(
                        "location:scale-0", StoryTime("main", 0, 0),
                        audience="author", perspective="author", limit=20)),
                }
                if pagination:
                    vm["childrenPage2"] = _children_page_two_vm(store, "location:scale-0", 20)
                if places >= 100_000 and maps >= 32:
                    vm["sparseHighBbox"] = _vm_case(connection, lambda: store.bbox(
                        "map:scale-31", BoundingBox((49_990, 49_980), (50_000, 50_000)), limit=20))
                for operation, evidence in vm.items():
                    evidence["advisoryBudget"] = VM_BUDGETS[operation]
                    evidence["withinAdvisoryBudget"] = evidence["vmStepsUpperBound"] <= VM_BUDGETS[operation]
            manifest = {"kind": "authored-source-to-API", "baseCommit": base_head,
                        "fixtureCommit": fixture_head, "seed": 0, "host": _host(),
                        "fixturePreparation": preparation,
                        "implementationProvenance": provenance,
                        "counts": counts, "compiledRows": rows,
                        "budgets": {"maxPage": 100, "routeExpansions": MAX_ROUTE_EXPANSIONS,
                                    "overlayCandidates": MAX_OVERLAY_CANDIDATES,
                                    "geometryCandidates": MAX_GEOMETRY_CANDIDATES,
                                    "geometryVertices": 10_000, "advisoryCompileMs": 120_000,
                                    "advisoryPeakRssBytes": 2_000_000_000},
                        "compile": {"runs": compile_runs,
                                    "reusedCompiledCache": reuse_compiled,
                                    "medianMs": statistics.median(run["wallMs"] for run in compile_runs) if compile_runs else None,
                                    "worstMs": max(run["wallMs"] for run in compile_runs) if compile_runs else None},
                        "api": api,
                        "pagination": pagination,
                        "queryVm": vm,
                        "coverage": {"source": counts, "compiled": rows,
                                     "browser": "unmeasured by this command",
                                     "compiledProjectionEnvelope": "separate existing benchmark only"}}
            if retain_fixture:
                kept = output / "fixture"
                if kept.exists():
                    raise FileExistsError(f"retained fixture already exists: {kept}")
                if clone.resolve().parent != sandbox.resolve() or kept.resolve().parent != output.resolve():
                    raise RuntimeError("retained fixture move is outside benchmark evidence scope")
                with trace.measure("fixture-retention"):
                    shutil.move(str(clone), str(kept))
                manifest["fixturePath"] = str(kept.resolve())
            successful = True
            return manifest
        finally:
            if old_temp is None: os.environ.pop("TEMP", None)
            else: os.environ["TEMP"] = old_temp
            if old_tmp is None: os.environ.pop("TMP", None)
            else: os.environ["TMP"] = old_tmp
    finally:
        if isinstance(sys.exc_info()[1], CommandCleanupError):
            (output / "cleanup-unresolved.json").write_text(json.dumps({
                "sandbox": str(sandbox), "reason": "owned command cleanup unresolved",
                "disposition": "retain until owner verifies descendants reaped; then finite contained cleanup"}) + "\n")
        else:
            _finalize_run(trace, sandbox, output, manifest, successful)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", type=Path, default=DEFAULT_BASE)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--places", type=int, default=256)
    parser.add_argument("--maps", type=int, default=2)
    parser.add_argument("--routes", type=int, default=128)
    parser.add_argument("--portals", type=int, default=2)
    parser.add_argument("--overlays", type=int, default=8)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--retain-fixture", action="store_true")
    parser.add_argument("--retained-fixture", type=Path,
                        help="recompile and query an existing immutable authored fixture under output/spatial-release-scale")
    parser.add_argument("--reuse-compiled", action="store_true",
                        help="query-only preflight of a retained authored fixture's existing compiled cache")
    parser.add_argument("--compiled-browser-from", type=Path)
    args = parser.parse_args()
    if args.compiled_browser_from is not None:
        result = compiled_browser_fixture(args.compiled_browser_from, args.output)
        print(json.dumps({"manifest": str(args.output / "manifest.json"),
                          "compiledRows": result["compiledRows"]}, sort_keys=True))
        return
    result = run(base=args.base, output=args.output, places=args.places,
                 maps=args.maps, routes=args.routes, portals=args.portals,
                 overlays=args.overlays, repeats=args.repeats,
                 retain_fixture=args.retain_fixture, retained_fixture=args.retained_fixture,
                 reuse_compiled=args.reuse_compiled)
    print(json.dumps({"manifest": str(args.output / "manifest.json"),
                      "counts": result["counts"], "compile": result["compile"]}, sort_keys=True))


if __name__ == "__main__":
    main()
