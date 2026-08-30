"""Deterministic benchmark evidence for compiled-only spatial query reads."""
from __future__ import annotations

import hashlib
import json
import sqlite3
import time
from dataclasses import replace
from typing import Any

from wedl.compiler import DDL, INDEX_DDL
from wedl.spatial_index import SpatialProjection, insert_spatial_index
from wedl.spatial_query import BoundingBox, MapPosition, SQLiteSpatialStore

from benchmark_spatial_index import synthetic_projection


def _projection(locations: int, routes: int, maps: int, portals: int, overlays: int) -> SpatialProjection:
    source = synthetic_projection(locations=locations, maps=maps, routes=routes, portals=portals, overlays=overlays)
    typed_routes = tuple((*row[:8], 1, "pace", 1, "fatigue", None, None, row[-1]) for row in source.routes)
    return replace(source, routes=typed_routes)


def _connection(projection: SpatialProjection) -> sqlite3.Connection:
    connection = sqlite3.connect(":memory:")
    connection.executescript(DDL); connection.executescript(INDEX_DDL)
    groups = ((projection.maps, "map"), (projection.locations, "location"), (projection.routes, "route"), (projection.portals, "portal"), (projection.overlays, "overlay"))
    connection.executemany("INSERT INTO entity VALUES (?,?,?,?,?,?,?,?,?)", ((row[0], kind, row[0], "world", "canonical", f"benchmark/{ordinal:09d}.md", None, "", "{}") for ordinal, (row, kind) in enumerate((row, kind) for rows, kind in groups for row in rows)))
    insert_spatial_index(connection, projection)
    return connection


def _evidence(result: Any) -> dict[str, Any]:
    value = result.value
    item: dict[str, Any] = {"state": result.kind.value, "reason": result.reason.value if result.reason else None}
    if value is not None and hasattr(value, "ids"):
        ids = tuple(value.ids)
        item.update({
            "count": len(ids), "first": ids[0] if ids else None, "last": ids[-1] if ids else None,
            "idsDigest": hashlib.sha256("\0".join(ids).encode("utf-8")).hexdigest(),
        })
    if value is not None and hasattr(value, "route_ids"):
        route_ids = tuple(value.route_ids)
        item["routeCount"] = len(route_ids)
        item["routeIdsDigest"] = hashlib.sha256("\0".join(route_ids).encode("utf-8")).hexdigest()
        item["metric"] = {"value": value.metric.value, "unit": value.metric.unit}
    return item


def measure(*, locations: int = 100_000, routes: int = 250_000, maps: int = 32,
            portals: int = 100, overlays: int = 10_000, repeats: int = 3) -> dict[str, Any]:
    counts = (locations, routes, maps, portals, overlays, repeats)
    if any(type(value) is not int for value in counts) or locations < 128 or routes < 0 or maps < 1 or portals < 0 or overlays < 0 or repeats < 1:
        raise ValueError("benchmark requires at least 128 locations and non-negative envelope counts")
    connection = _connection(_projection(locations, routes, maps, portals, overlays))
    try:
        store = SQLiteSpatialStore(connection, "benchmark-revision")
        timings: dict[str, list[float]] = {"containment": [], "bbox": [], "nearby": [], "path": [], "overlay": []}
        outcomes: dict[str, str] = {}
        stable_results: dict[str, Any] | None = None
        for _ in range(repeats):
            cases = {
                "containment": lambda: store.containment("location:bench-127"),
                "bbox": lambda: store.bbox("map:bench-0", BoundingBox((0, 0), (100, 100))),
                "nearby": lambda: store.nearby(MapPosition("map:bench-0", (0, 0)), radius=100),
                "path": lambda: store.path("location:bench-0", f"location:bench-{min(locations - 1, 99)}", metric="route_distance"),
                "overlay": lambda: store.overlay_as_of("location:bench-0", {"timeline": "main", "tick": 0, "order": 0}, audience="author", perspective="author"),
            }
            current_results: dict[str, Any] = {}
            for name, operation in cases.items():
                started = time.perf_counter(); result = operation(); timings[name].append((time.perf_counter() - started) * 1000)
                outcomes[name] = result.kind.value; current_results[name] = _evidence(result)
            if stable_results is not None and current_results != stable_results:
                raise RuntimeError("spatial query benchmark results are not repeat-deterministic")
            stable_results = current_results
        assert stable_results is not None
        encoded = json.dumps(stable_results, ensure_ascii=False, separators=(",", ":"), sort_keys=True, allow_nan=False).encode("utf-8")
        return {
            "protocol": "wedl-spatial/v1", "revision": "benchmark-revision",
            "locations": locations, "hierarchyDepth": 128, "maps": maps,
            "routes": routes, "portals": portals, "overlays": overlays,
            "runs": repeats, "outcomes": outcomes,
            "resultDigest": hashlib.sha256(encoded).hexdigest(),
            "results": stable_results,
            "medianMs": {name: round(sorted(values)[len(values) // 2], 3) for name, values in timings.items()},
        }
    finally:
        connection.close()


def smoke() -> dict[str, Any]:
    return measure(locations=1_000, routes=2_500, maps=4, portals=10, overlays=100, repeats=1)


if __name__ == "__main__":
    print(json.dumps(measure(), indent=2, sort_keys=True))
