"""Small deterministic benchmark for the latent spatial projection.

It exercises only an opt-in component projection and never makes v0.7 a
runtime-accepted source schema.
"""
from __future__ import annotations

import hashlib
import json
import sqlite3
import time
from typing import Any

from wedl.compiler import DDL, INDEX_DDL
from wedl.spatial_index import insert_spatial_index


def synthetic_projection(
    *,
    locations: int = 1_000,
    maps: int = 4,
    routes: int = 2_500,
    portals: int = 10,
    overlays: int = 100,
) -> Any:
    """Create deterministic benchmark rows without claiming authored source.

    The full envelope is 100k locations, 32 maps, 250k routes, 100 portals,
    and 10k overlays. Hierarchy depth and sibling fanout are intentionally
    bounded (128 and 10k) so the fixture tests adjacency rather than closure.
    """
    from wedl.spatial_index import SpatialProjection
    if min(locations, maps, routes, portals, overlays) < 0 or maps == 0 or locations == 0:
        raise ValueError("benchmark counts must be non-negative with positive maps and locations")
    capabilities = (("spatial-core-v1", 0), ("geometry-v1", 1), ("route-v1", 2), ("overlay-v1", 3))
    map_rows = tuple((f"map:bench-{index}", index, f"local-planar:{index}", "east", "north", "pace", "forbidden", -1_000_000, -1_000_000, None, 1_000_000, 1_000_000, None, "{}") for index in range(maps))
    def parent(index: int) -> str | None:
        if index == 0:
            return None
        # First a 128-deep chain, then ten thousand direct siblings of its
        # root, then additional bounded-depth chains. This exercises adjacency
        # at both requested hierarchy extremes without materializing closure.
        if index < 128:
            return f"location:bench-{index - 1}"
        if index <= 10_127:
            return "location:bench-0"
        group_start = 10_128 + ((index - 10_128) // 128) * 128
        return "location:bench-0" if index == group_start else f"location:bench-{index - 1}"
    location_rows = tuple((f"location:bench-{index}", index, 1, parent(index), f"map:bench-{index % maps}", "point", index, index, None, index, index, None, f"[{index},{index}]", "{}") for index in range(locations))
    vertices = tuple((f"location:bench-{index}", 0, index, index, None) for index in range(locations))
    hierarchy = tuple((f"location:bench-{index}", parent(index), index) for index in range(locations) if parent(index) is not None)
    route_rows = tuple((f"route:bench-{index}", index, f"location:bench-{index % locations}", f"location:bench-{(index + 1) % locations}", "one-way", '["foot"]', "open", "exact", None, None, None, None, None, None, "{}") for index in range(routes))
    arcs = tuple((f"route:bench-{index}", f"location:bench-{index % locations}", f"location:bench-{(index + 1) % locations}", 0) for index in range(routes))
    route_modes = tuple((f"route:bench-{index}", "foot", 0) for index in range(routes))
    portal_rows = tuple((f"portal:bench-{index}", index, f"location:bench-{index % locations}", "location", f"location:bench-{(index + 1) % locations}", None, None, '["foot"]', "{}") for index in range(portals))
    portal_modes = tuple((f"portal:bench-{index}", "foot", 0) for index in range(portals))
    overlay_rows = tuple((f"overlay:bench-{index}", index, "time-bounded", '["author"]', '["author"]', "main", index, 0, index + 1, 0, "{}") for index in range(overlays))
    memberships = tuple((f"overlay:bench-{index}", f"location:bench-{index % locations}", 0) for index in range(overlays))
    audiences = tuple((f"overlay:bench-{index}", "author", 0) for index in range(overlays))
    perspectives = tuple((f"overlay:bench-{index}", "author", 0) for index in range(overlays))
    return SpatialProjection(capabilities, map_rows, location_rows, vertices, hierarchy, (), route_rows, arcs, route_modes, (), portal_rows, portal_modes, overlay_rows, memberships, audiences, perspectives)


def _insert_benchmark_entities(connection: sqlite3.Connection, projection: Any) -> None:
    kinds = ((projection.maps, "map"), (projection.locations, "location"), (projection.routes, "route"), (projection.anchors, "anchor"), (projection.portals, "portal"), (projection.overlays, "overlay"))
    rows = ((item[0], kind, item[0], "world", "canonical", f"benchmark/{ordinal:09d}.md", None, "", "{}") for ordinal, (item, kind) in enumerate((item, kind) for items, kind in kinds for item in items))
    connection.executemany("INSERT INTO entity VALUES (?,?,?,?,?,?,?,?,?)", rows)


_SPATIAL_TABLES = (
    "spatial_capability", "spatial_map", "spatial_location",
    "spatial_location_vertex", "spatial_hierarchy", "spatial_location_link",
    "spatial_route", "spatial_route_edge", "spatial_route_mode",
    "spatial_anchor", "spatial_portal", "spatial_portal_mode",
    "spatial_overlay", "spatial_overlay_location", "spatial_overlay_audience",
    "spatial_overlay_perspective",
)


def _row_digest(connection: sqlite3.Connection) -> str:
    snapshot: list[Any] = []
    for table in _SPATIAL_TABLES:
        columns = [str(row[1]) for row in connection.execute(f"PRAGMA table_info({table})")]
        order = ",".join(f'"{column}"' for column in columns)
        rows = [list(row) for row in connection.execute(f'SELECT * FROM "{table}" ORDER BY {order}')]
        snapshot.append([table, columns, rows])
    encoded = json.dumps(snapshot, ensure_ascii=False, separators=(",", ":"), allow_nan=False).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def _query_plans(connection: sqlite3.Connection) -> dict[str, list[str]]:
    cases = {
        "mapBounds": ("SELECT id FROM spatial_location WHERE map_id=? AND min_x<=? AND max_x>=?", ("map:bench-0", 0, 0)),
        "hierarchyParent": ("SELECT location_id FROM spatial_hierarchy WHERE parent_id=? ORDER BY location_id", ("location:bench-0",)),
        "locationLinkTarget": ("SELECT location_id FROM spatial_location_link WHERE target_location_id=? ORDER BY location_id,source_ordinal", ("location:bench-0",)),
        "routeAdjacency": ("SELECT route_id,to_location_id FROM spatial_route_edge WHERE from_location_id=? ORDER BY to_location_id,route_id", ("location:bench-0",)),
        "overlayCandidate": ("SELECT id FROM spatial_overlay WHERE timeline=? AND start_tick<=? AND end_tick>=? ORDER BY start_tick,start_order,id", ("main", 0, 0)),
    }
    return {
        name: [str(row[-1]) for row in connection.execute("EXPLAIN QUERY PLAN " + statement, params)]
        for name, (statement, params) in cases.items()
    }


def measure(projection: Any, repeats: int = 3) -> dict[str, Any]:
    """Return timings plus repeat-stable byte, row, and query-plan evidence."""
    if type(repeats) is not int or repeats < 1:
        raise ValueError("repeats must be a positive integer")
    timings: list[float] = []
    stats: dict[str, int] = {}
    evidence: dict[str, Any] | None = None
    for _ in range(repeats):
        connection = sqlite3.connect(":memory:")
        try:
            connection.executescript(DDL)
            connection.executescript(INDEX_DDL)
            _insert_benchmark_entities(connection, projection)
            started = time.perf_counter()
            stats = insert_spatial_index(connection, projection)
            connection.commit()
            timings.append((time.perf_counter() - started) * 1000)
            serialized = connection.serialize()
            current = {
                "databaseBytes": len(serialized),
                "databaseDigest": hashlib.sha256(serialized).hexdigest(),
                "rowDigest": _row_digest(connection),
                "queryPlans": _query_plans(connection),
            }
            if evidence is not None and current != evidence:
                raise RuntimeError("spatial benchmark projection is not repeat-deterministic")
            evidence = current
        finally:
            connection.close()
    ordered = sorted(timings)
    assert evidence is not None
    return {"runs": repeats, "minimumMs": round(ordered[0], 3), "medianMs": round(ordered[len(ordered) // 2], 3), **stats, **evidence}


def smoke() -> dict[str, Any]:
    """Run the inexpensive 1k-location benchmark and return JSON-safe data."""
    return measure(synthetic_projection(), repeats=1)


def full() -> dict[str, Any]:
    """Run the ratified stress envelope (100k locations, 250k routes)."""
    return measure(
        synthetic_projection(locations=100_000, maps=32, routes=250_000, portals=100, overlays=10_000),
        repeats=1,
    )
