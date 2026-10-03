"""Disposable, authored-only SQLite projection for latent v0.7 spatial data.

The module deliberately has no query API.  It normalizes exact authored facts
for the later query task: parent links are adjacency rows (never a closure),
two-way routes produce two explicitly labelled traversable edges, and geometry
is reduced only to its authored coordinate bounds.  No containment, reverse
portal, proximity, map conversion, duration, or StoryTime fact is inferred.
"""
from __future__ import annotations

from dataclasses import dataclass
import math
import sqlite3
import time
from typing import Any, Iterable

from .model import ORDER_MAX, ORDER_MIN
from .spatial_validation import validate_spatial_component
from .util import canonical_json
from .validation import _location_target
from .v07 import SOURCE_SCHEMA


I64_MIN, I64_MAX = -(2**63), 2**63 - 1
_TIME_DIGIT_MAX = (1 << 24) - 1


def _story_time_digits(tick: int, order: int) -> tuple[int, int, int, int]:
    """Order-preserving, float32-exact digits for signed StoryTime bounds."""
    value = ((tick + (1 << 63)) << 32) | (order - ORDER_MIN)
    return tuple((value >> shift) & _TIME_DIGIT_MAX for shift in (72, 48, 24, 0))


def _time_rectangles(lower: tuple[int, ...], upper: tuple[int, ...]) -> Iterable[tuple[tuple[int, int], ...]]:
    """Partition one inclusive lexicographic range into disjoint boxes."""
    if not lower:
        yield ()
    elif len(lower) == 1:
        yield ((lower[0], upper[0]),)
    elif all(left == 0 and right == _TIME_DIGIT_MAX for left, right in zip(lower, upper)):
        yield ((0, _TIME_DIGIT_MAX),) * len(lower)
    elif lower == upper:
        yield tuple((value, value) for value in lower)
    elif lower[0] == upper[0]:
        for rest in _time_rectangles(lower[1:], upper[1:]):
            yield ((lower[0], lower[0]), *rest)
    else:
        tail = len(lower) - 1
        for rest in _time_rectangles(lower[1:], (_TIME_DIGIT_MAX,) * tail):
            yield ((lower[0], lower[0]), *rest)
        if lower[0] + 1 < upper[0]:
            yield ((lower[0] + 1, upper[0] - 1), *((0, _TIME_DIGIT_MAX),) * tail)
        for rest in _time_rectangles((0,) * tail, upper[1:]):
            yield ((upper[0], upper[0]), *rest)


@dataclass(frozen=True, slots=True)
class SpatialProjection:
    capabilities: tuple[tuple[Any, ...], ...]
    maps: tuple[tuple[Any, ...], ...]
    locations: tuple[tuple[Any, ...], ...]
    vertices: tuple[tuple[Any, ...], ...]
    hierarchy: tuple[tuple[Any, ...], ...]
    location_links: tuple[tuple[Any, ...], ...]
    routes: tuple[tuple[Any, ...], ...]
    route_edges: tuple[tuple[Any, ...], ...]
    route_modes: tuple[tuple[Any, ...], ...]
    anchors: tuple[tuple[Any, ...], ...]
    portals: tuple[tuple[Any, ...], ...]
    portal_modes: tuple[tuple[Any, ...], ...]
    overlays: tuple[tuple[Any, ...], ...]
    overlay_locations: tuple[tuple[Any, ...], ...]
    overlay_audiences: tuple[tuple[Any, ...], ...]
    overlay_perspectives: tuple[tuple[Any, ...], ...]


def _number(value: Any) -> int | float:
    if type(value) not in (int, float) or not math.isfinite(value):
        raise ValueError("spatial SQLite projection requires a finite number")
    # Python's SQLite adapter refuses integers outside this range before REAL
    # affinity can be applied.  Refuse explicitly so a validated component can
    # never fail halfway through a disposable-cache insert.
    if type(value) is int and not I64_MIN <= value <= I64_MAX:
        raise ValueError("spatial SQLite projection integer is outside signed i64")
    return value


def _bounds(coordinates: Any) -> tuple[float | int, float | int, float | int | None, float | int, float | int, float | int | None]:
    points = [coordinates] if isinstance(coordinates, list) and coordinates and type(coordinates[0]) in (int, float) else coordinates
    assert isinstance(points, list) and points
    xs = [_number(point[0]) for point in points]
    ys = [_number(point[1]) for point in points]
    has_z = len(points[0]) == 3
    zs = [_number(point[2]) for point in points] if has_z else []
    return min(xs), min(ys), min(zs) if zs else None, max(xs), max(ys), max(zs) if zs else None


def _metric(value: Any) -> tuple[int | float | None, str | None]:
    return (None, None) if value is None else (_number(value["value"]), value["unit"])


def _records(world: Any) -> list[Any]:
    return sorted(world.records.values(), key=lambda record: (record.source_path.casefold(), record.id))


def build_spatial_projection(world: Any, *, validated: bool = False) -> SpatialProjection:
    """Build a deterministic projection of a valid latent v0.7 component.

    This is deliberately not called by generic compilation until the migration
    task authorizes v0.7 runtime acceptance.
    """
    if getattr(world, "schema", None) != SOURCE_SCHEMA:
        return SpatialProjection((), (), (), (), (), (), (), (), (), (), (), (), (), (), (), ())
    if not validated and validate_spatial_component(_records(world)):
        raise ValueError("spatial projection requires a valid v0.7 component")
    capabilities = tuple((name, ordinal) for ordinal, name in enumerate(world.world_record.frontmatter["capabilities"]))
    maps: list[tuple[Any, ...]] = []
    locations: list[tuple[Any, ...]] = []
    vertices: list[tuple[Any, ...]] = []
    hierarchy: list[tuple[Any, ...]] = []
    location_links: list[tuple[Any, ...]] = []
    routes: list[tuple[Any, ...]] = []
    edges: list[tuple[Any, ...]] = []
    route_modes: list[tuple[Any, ...]] = []
    anchors: list[tuple[Any, ...]] = []
    portals: list[tuple[Any, ...]] = []
    portal_modes: list[tuple[Any, ...]] = []
    overlays: list[tuple[Any, ...]] = []
    overlay_locations: list[tuple[Any, ...]] = []
    overlay_audiences: list[tuple[Any, ...]] = []
    overlay_perspectives: list[tuple[Any, ...]] = []
    for ordinal, record in enumerate(_records(world)):
        value = record.frontmatter
        kind = value.get("kind")
        if kind == "map":
            bounds = value["bounds"]
            minimum, maximum = bounds["min"], bounds["max"]
            maps.append((record.id, ordinal, value["crs"], value["axis_order"][0], value["axis_order"][1], value["unit"], value.get("z_policy", "forbidden"), *_bounds([minimum, maximum]), canonical_json(value)))
        elif kind == "location":
            spatial = value.get("spatial")
            parent_id = value.get("parent_id", value.get("parent"))
            if spatial is None:
                locations.append((record.id, ordinal, 0, parent_id, None, None, None, None, None, None, None, None, None, canonical_json(value)))
            else:
                geometry = spatial["geometry"]
                locations.append((record.id, ordinal, 1, parent_id, spatial["map_id"], geometry["kind"], *_bounds(geometry["coordinates"]), canonical_json(geometry["coordinates"]), canonical_json(value)))
                raw_points = [geometry["coordinates"]] if geometry["kind"] == "point" else geometry["coordinates"]
                vertices.extend((record.id, point_ordinal, _number(point[0]), _number(point[1]), _number(point[2]) if len(point) == 3 else None) for point_ordinal, point in enumerate(raw_points))
            if parent_id is not None:
                hierarchy.append((record.id, parent_id, ordinal))
            location_links.extend(
                (record.id, _location_target(link), link_ordinal, canonical_json(link))
                for link_ordinal, link in enumerate(value.get("links", []))
            )
        elif kind == "route":
            distance, distance_unit = _metric(value.get("route_distance"))
            cost, cost_unit = _metric(value.get("travel_cost"))
            duration, duration_unit = _metric(value.get("duration"))
            routes.append((record.id, ordinal, value["from_location_id"], value["to_location_id"], value["direction"], canonical_json(value["modes"]), value.get("availability", "open"), value.get("uncertainty", "exact"), distance, distance_unit, cost, cost_unit, duration, duration_unit, canonical_json(value)))
            edges.append((record.id, value["from_location_id"], value["to_location_id"], 0))
            if value["direction"] == "two-way":
                edges.append((record.id, value["to_location_id"], value["from_location_id"], 1))
            route_modes.extend((record.id, mode, mode_ordinal) for mode_ordinal, mode in enumerate(value["modes"]))
        elif kind == "anchor":
            source, target = value["from"], value["to"]
            anchors.append((record.id, ordinal, source["map_id"], canonical_json(source["coordinates"]), target["map_id"], canonical_json(target["coordinates"]), value.get("conversion"), canonical_json(value)))
        elif kind == "portal":
            target = value["to"]
            target_kind = "location" if isinstance(target, str) else "position"
            portals.append((record.id, ordinal, value["from_location_id"], target_kind, target if isinstance(target, str) else None, None if isinstance(target, str) else target["map_id"], None if isinstance(target, str) else canonical_json(target["coordinates"]), canonical_json(value["modes"]), canonical_json(value)))
            portal_modes.extend((record.id, mode, mode_ordinal) for mode_ordinal, mode in enumerate(value["modes"]))
        elif kind == "overlay":
            interval = value.get("valid") or {}
            start, end = interval.get("start"), interval.get("end")
            overlays.append((record.id, ordinal, value["lifecycle"], canonical_json(value["audience"]), canonical_json(value["perspectives"]), None if start is None else start["timeline"], None if start is None else start["tick"], None if start is None else start["order"], None if end is None else end["tick"], None if end is None else end["order"], canonical_json(value)))
            overlay_locations.extend((record.id, location_id, member_ordinal) for member_ordinal, location_id in enumerate(value["membership"]["location_ids"]))
            overlay_audiences.extend((record.id, audience, audience_ordinal) for audience_ordinal, audience in enumerate(value["audience"]))
            overlay_perspectives.extend((record.id, perspective, perspective_ordinal) for perspective_ordinal, perspective in enumerate(value["perspectives"]))
    return SpatialProjection(tuple(capabilities), tuple(maps), tuple(locations), tuple(vertices), tuple(hierarchy), tuple(location_links), tuple(routes), tuple(edges), tuple(route_modes), tuple(anchors), tuple(portals), tuple(portal_modes), tuple(overlays), tuple(overlay_locations), tuple(overlay_audiences), tuple(overlay_perspectives))


def insert_spatial_index(connection: sqlite3.Connection, projection: SpatialProjection, *, batch_size: int = 1_000) -> dict[str, Any]:
    """Insert bounded batches so large components never need a giant payload."""
    if type(batch_size) is not int or not 1 <= batch_size <= 1_000:
        raise ValueError("spatial batch_size must be an integer in 1..1000")
    started = time.perf_counter()
    batches = 0; maximum = 0
    def insert(sql: str, rows: Iterable[tuple[Any, ...]]) -> None:
        nonlocal batches, maximum
        batch: list[tuple[Any, ...]] = []
        for row in rows:
            batch.append(row)
            if len(batch) == batch_size:
                connection.executemany(sql, batch); batches += 1; maximum = max(maximum, len(batch)); batch.clear()
        if batch:
            connection.executemany(sql, batch); batches += 1; maximum = max(maximum, len(batch))
    insert("INSERT INTO spatial_capability VALUES (?,?)", projection.capabilities)
    insert("INSERT INTO spatial_map VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)", projection.maps)
    # Deferred self-references can name parents inserted later in source order.
    # SQLite checks their earlier children whenever a parent arrives. Without
    # this index that check scans all preceding locations for every insertion.
    # Keep it build-only: the compiler installs its public query indexes later.
    connection.execute("CREATE INDEX spatial_build_parent_idx ON spatial_location(parent_id)")
    try:
        insert("INSERT INTO spatial_location VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)", projection.locations)
    finally:
        connection.execute("DROP INDEX spatial_build_parent_idx")
    insert("INSERT INTO spatial_location_vertex VALUES (?,?,?,?,?)", projection.vertices)
    insert("INSERT INTO spatial_hierarchy VALUES (?,?,?)", projection.hierarchy)
    insert("INSERT INTO spatial_location_link VALUES (?,?,?,?)", projection.location_links)
    insert("INSERT INTO spatial_route VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)", projection.routes)
    insert("INSERT INTO spatial_route_edge VALUES (?,?,?,?)", projection.route_edges)
    insert("INSERT INTO spatial_route_mode VALUES (?,?,?)", projection.route_modes)
    insert("INSERT INTO spatial_anchor VALUES (?,?,?,?,?,?,?,?)", projection.anchors)
    insert("INSERT INTO spatial_portal VALUES (?,?,?,?,?,?,?,?,?)", projection.portals)
    insert("INSERT INTO spatial_portal_mode VALUES (?,?,?)", projection.portal_modes)
    insert("INSERT INTO spatial_overlay VALUES (?,?,?,?,?,?,?,?,?,?,?)", projection.overlays)
    insert("INSERT INTO spatial_overlay_location VALUES (?,?,?)", projection.overlay_locations)
    insert("INSERT INTO spatial_overlay_audience VALUES (?,?,?)", projection.overlay_audiences)
    insert("INSERT INTO spatial_overlay_perspective VALUES (?,?,?)", projection.overlay_perspectives)
    # The selected local-author lens must be seekable from a viewport member;
    # scanning either all authorized offscreen overlays or hidden memberships
    # at one visible location is unbounded. Stream the normalized cross-product
    # into bounded batches without retaining it in the source projection.
    insert("INSERT INTO spatial_overlay_lens_location VALUES (?,?,?,?,?,?,?,?,?,?)",
           connection.execute("SELECT m.location_id,a.audience,p.perspective,m.overlay_id,"
                              "o.lifecycle,o.timeline,o.start_tick,o.start_order,o.end_tick,o.end_order "
                              "FROM spatial_overlay_location AS m "
                              "JOIN spatial_overlay_audience AS a ON a.overlay_id=m.overlay_id "
                              "JOIN spatial_overlay_perspective AS p ON p.overlay_id=m.overlay_id "
                              "JOIN spatial_overlay AS o ON o.id=m.overlay_id"))
    bounds_index = install_optional_spatial_index(connection)
    return {"spatialMapCount": len(projection.maps), "spatialLocationCount": len(projection.locations), "spatialVertexCount": len(projection.vertices), "spatialRouteCount": len(projection.routes), "spatialRouteEdgeCount": len(projection.route_edges), "spatialOverlayCount": len(projection.overlays), "spatialBoundsIndex": bounds_index, "spatialInsertBatches": batches, "spatialMaxInsertBatch": maximum, "spatialTimingMs": round((time.perf_counter() - started) * 1000, 3)}


def install_optional_spatial_index(connection: sqlite3.Connection) -> str:
    """Install an RTree when this SQLite build has it, otherwise retain Btree.

    The Btree indexes in the base schema retain the existing ordered query
    semantics. The RTree additionally scopes a geometry probe to one compiled
    map rowid, so a sparse explorer viewport does not walk all earlier bounds.
    It stores only bounds and map identity already projected above.
    SQLite RTree rounds lower bounds down and upper bounds up, making its rows
    conservative candidates; callers must still post-filter against the exact
    NUMERIC bounds in ``spatial_location``.
    """
    try:
        connection.execute("CREATE VIRTUAL TABLE spatial_location_rtree USING rtree(source_ordinal,min_x,max_x,min_y,max_y,min_map,max_map)")
        connection.executemany(
            "INSERT INTO spatial_location_rtree VALUES (?,?,?,?,?,?,?)",
            connection.execute("SELECT loc.source_ordinal,loc.min_x,loc.max_x,loc.min_y,loc.max_y,map.rowid,map.rowid "
                               "FROM spatial_location AS loc JOIN spatial_map AS map ON map.id=loc.map_id "
                               "ORDER BY loc.source_ordinal"),
        )
        connection.execute("INSERT OR IGNORE INTO spatial_overlay_scope_key(location_id,audience,perspective,timeline) "
                           "SELECT DISTINCT location_id,audience,perspective,timeline "
                           "FROM spatial_overlay_lens_location WHERE lifecycle!='static'")
        largest_scope = connection.execute("SELECT max(id) FROM spatial_overlay_scope_key").fetchone()[0]
        if largest_scope is not None and largest_scope > 1 << 24:
            raise sqlite3.OperationalError("spatial temporal scope exceeds exact RTree key range")
        connection.execute("CREATE VIRTUAL TABLE spatial_overlay_time_rtree USING rtree(segment_id,min_scope,max_scope,min_t3,max_t3,min_t2,max_t2,min_t1,max_t1,min_t0,max_t0)")

        def temporal_segments() -> Iterable[tuple[Any, ...]]:
            rows = connection.execute("SELECT lens.rowid,key.id,"
                                      "lens.start_tick,lens.start_order,lens.end_tick,lens.end_order "
                                      "FROM spatial_overlay_lens_location AS lens "
                                      "JOIN spatial_overlay_scope_key AS key ON key.location_id=lens.location_id "
                                      "AND key.audience=lens.audience AND key.perspective=lens.perspective "
                                      "AND key.timeline=lens.timeline "
                                      "WHERE lens.lifecycle!='static'")
            for membership_id, scope_key, first_tick, first_order, last_tick, last_order in rows:
                lower = _story_time_digits(first_tick, first_order)
                upper = _story_time_digits(last_tick, last_order)
                for segment, rectangle in enumerate(_time_rectangles(lower, upper)):
                    if segment >= 16:
                        raise sqlite3.OperationalError("spatial temporal interval needs too many RTree boxes")
                    yield (membership_id * 16 + segment, scope_key, scope_key,
                           *(coordinate for pair in rectangle for coordinate in pair))

        connection.executemany(
            "INSERT INTO spatial_overlay_time_rtree VALUES (?,?,?,?,?,?,?,?,?,?,?)",
            temporal_segments(),
        )
    except sqlite3.OperationalError:
        # A failed population must not leave an incomplete virtual table that
        # an explorer read could mistake for a usable candidate index.
        connection.execute("DROP TABLE IF EXISTS spatial_overlay_time_rtree")
        connection.execute("DROP TABLE IF EXISTS spatial_location_rtree")
        return "btree"
    return "rtree"
