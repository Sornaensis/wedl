"""Reproducible spatial source-to-compiled scale probe.

Run ``--help`` for the isolated smoke and explicit large-envelope commands.
The generated Git repository is disposable; this script never writes to the
repository being measured. Compiled-projection probes are labeled separately.
"""
from __future__ import annotations

import argparse
from contextlib import closing
import ctypes
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import sqlite3
import statistics
import subprocess
import sys
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


def _git(root: Path, *args: str) -> str:
    result = subprocess.run(["git", "-C", str(root), *args],
                            capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout.strip()


def _write(root: Path, group: str, record: dict[str, Any]) -> None:
    path = root / "story" / group / (record["id"].replace(":", "-") + ".md")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(serialize_record(record, f"# {record['title']}\n"))


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
                shutil.copytree(base / ".git", clone / ".git", copy_function=shutil.copyfile)
                _git(clone, "config", "core.longpaths", "true")
                _git(clone, "sparse-checkout", "init", "--cone")
                _git(clone, "sparse-checkout", "set", "story")
                _git(clone, "reset", "--hard", base_head)
                if _git(clone, "rev-parse", "HEAD") != base_head:
                    raise RuntimeError("disposable clone revision differs from measured commit")
                status = _git(clone, "status", "--porcelain")
                if status:
                    raise RuntimeError(f"disposable sparse checkout is not clean: {status[:1000]}")
                counts = generate(clone, places=places, maps=maps, routes=routes,
                                  portals=portals, overlays=overlays)
                _git(clone, "add", "-A", "story")
                _git(clone, "-c", "user.name=spatial scale", "-c", "user.email=scale@test.invalid",
                     "commit", "-qm", "generated spatial scale fixture")
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
            repo = Repository(clone)
            fixture_head = repo.head()
            compile_runs = []
            for _ in range(0 if reuse_compiled else 2):
                started = time.perf_counter()
                built = compile_world(repo, "HEAD", force=True)
                compile_runs.append({"wallMs": round((time.perf_counter()-started)*1000, 3),
                                     "status": built["status"], "recordCount": built.get("recordCount"),
                                     "databaseBytes": built.get("databaseBytes"),
                                     "timingsMs": built.get("timingsMs"),
                                     "processMemory": _process_memory()})
                if built["status"] != "compiled":
                    raise RuntimeError(f"source compile failed: {built}")
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
                    api["childrenPage2"] = _sample(second_page, repeats)
                    page = second_page()
                    second_ids = [card["id"] for card in (page.get("result") or {}).get("places", [])]
                    if (page.get("state") != "ok" or page.get("revision") != fixture_head or
                            not second_ids or len(second_ids) > 20 or set(first_ids) & set(second_ids)):
                        raise RuntimeError("authored children second page is not exact-revision disjoint pagination")
                    rebound_body = {**page_body, "parentId": "location:scale-1"}
                    rebound = _sample(lambda: client.post(cases["children"][1], json=rebound_body).json(), repeats)
                    if rebound["outcome"]["state"] != "invalid" or rebound["outcome"]["code"] != "SPATIAL-CURSOR-001" or not rebound["outcome"]["noPartialFeatureData"]:
                        raise RuntimeError("authored cursor rebound did not close without partial data")
                    pagination = {"revision": fixture_head, "limit": 20,
                                  "firstIdsSha256": hashlib.sha256(json.dumps(first_ids).encode()).hexdigest(),
                                  "secondIdsSha256": hashlib.sha256(json.dumps(second_ids).encode()).hexdigest(),
                                  "firstCount": len(first_ids), "secondCount": len(second_ids),
                                  "disjoint": True, "cursorSha256": hashlib.sha256(cursor.encode()).hexdigest(),
                                  "rebound": rebound}
            with closing(connect(database, True)) as connection:
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
                    vm["childrenPage2"] = _vm_sql(connection, lambda: connection.execute(
                        "SELECT loc.id FROM spatial_location AS loc JOIN entity ON entity.id=loc.id "
                        "WHERE loc.parent_id=? AND loc.id>? ORDER BY loc.id LIMIT 21",
                        ("location:scale-0", first_ids[-1])).fetchall())
                if places >= 100_000 and maps >= 32:
                    vm["sparseHighBbox"] = _vm_case(connection, lambda: store.bbox(
                        "map:scale-31", BoundingBox((49_990, 49_980), (50_000, 50_000)), limit=20))
                for operation, evidence in vm.items():
                    evidence["advisoryBudget"] = VM_BUDGETS[operation]
                    evidence["withinAdvisoryBudget"] = evidence["vmStepsUpperBound"] <= VM_BUDGETS[operation]
            manifest = {"kind": "authored-source-to-API", "baseCommit": base_head,
                        "fixtureCommit": fixture_head, "seed": 0, "host": _host(),
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
                shutil.move(str(clone), str(kept))
                manifest["fixturePath"] = str(kept.resolve())
            target = output / "manifest.json"
            target.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            return manifest
        finally:
            if old_temp is None: os.environ.pop("TEMP", None)
            else: os.environ["TEMP"] = old_temp
            if old_tmp is None: os.environ.pop("TMP", None)
            else: os.environ["TMP"] = old_tmp
    finally:
        if sandbox.resolve().parent != output.resolve():
            raise RuntimeError("refusing to remove a sandbox outside the evidence directory")
        def writable_retry(action, path, error):
            os.chmod(path, 0o777)
            action(path)
        shutil.rmtree(sandbox, onexc=writable_retry)


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
