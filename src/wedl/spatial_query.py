"""Closed, compiled-only reads for the latent spatial SQLite projection.

The module is intentionally an internal boundary: it has no repository,
compiler, HTTP, CLI, or source-loader integration.  Callers supply the exact
compiled revision and an SQLite connection populated by ``spatial_index``.
Every result carries that revision and only reads authored projection rows.
"""
from __future__ import annotations

import base64
import hashlib
import heapq
import json
import math
import sqlite3
from dataclasses import dataclass
from decimal import Decimal
from enum import StrEnum
from typing import Any, Iterable

from .model import StoryTime
from .util import canonical_json
from .v07 import CAPABILITY_ORDER


PROTOCOL = "wedl-spatial/v1"
MIN_LIMIT, MAX_LIMIT = 1, 100
MAX_ROUTE_EXPANSIONS = 1_000
MAX_HIERARCHY_EXPANSIONS = 1_000
MAX_OVERLAY_CANDIDATES = 2_000
MAX_GEOMETRY_CANDIDATES = 10_000


class SpatialOutcomeKind(StrEnum):
    OK = "ok"
    INVALID = "invalid"
    UNAVAILABLE = "unavailable"
    FORBIDDEN = "forbidden"
    LIMIT = "limit"


class SpatialReason(StrEnum):
    REQUEST = "SPATIAL-REQUEST-001"
    GEOMETRY = "SPATIAL-GEOMETRY-001"
    METRIC = "SPATIAL-METRIC-001"
    PATH = "SPATIAL-PATH-001"
    OVERLAY = "SPATIAL-OVERLAY-001"
    LIMIT = "SPATIAL-LIMIT-001"
    CURSOR = "SPATIAL-CURSOR-001"


class SpatialSubreason(StrEnum):
    """Machine-readable internal distinctions beneath the ratified reason set.

    These values deliberately do not introduce new public diagnostic codes.
    Callers that persist or transport a diagnostic must continue to use
    ``SpatialReason``; a subreason only makes a closed outcome more precise.
    """

    UNKNOWN_COORDINATE = "unknown-coordinate"
    UNKNOWN_METRIC = "unknown-metric"
    INCOMPATIBLE_UNIT = "incompatible-unit"
    AMBIGUOUS_UNIT = "ambiguous-unit"
    UNAVAILABLE_EDGE = "unavailable-edge"
    CLOSED_EDGE = "closed-edge"
    CROSS_MAP_DISCONTINUITY = "cross-map-discontinuity"
    UNREACHABLE = "unreachable"


# Public names make the internal boundary explicit without coupling it to the
# existing narrative-query vocabulary.
SpatialState = SpatialOutcomeKind
SpatialCode = SpatialReason


@dataclass(frozen=True, slots=True)
class SpatialOutcome:
    kind: SpatialOutcomeKind
    revision: str
    value: Any = None
    reason: SpatialReason | None = None
    subreason: SpatialSubreason | None = None
    detail: str | None = None


@dataclass(frozen=True, slots=True)
class Citation:
    revision: str
    basis: str


@dataclass(frozen=True, slots=True)
class QueryContext:
    protocol: str
    revision: str
    capabilities: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class MapPosition:
    map_id: str
    coordinates: tuple[int | float, ...]


@dataclass(frozen=True, slots=True)
class BoundingBox:
    minimum: tuple[int | float, ...]
    maximum: tuple[int | float, ...]


@dataclass(frozen=True, slots=True)
class CataloguePage:
    ids: tuple[str, ...]
    basis: str
    units: str | None
    filters: dict[str, Any]
    partial: bool = False
    unknown: bool = False
    cursor: str | None = None


@dataclass(frozen=True, slots=True)
class HierarchyPath:
    ids: tuple[str, ...]
    basis: str = "authored-parent-id"


@dataclass(frozen=True, slots=True)
class MetricSummary:
    metric: str
    value: int | float | None
    unit: str | None
    complete: bool
    unknown_edges: tuple[str, ...] = ()
    partial: bool = False
    unknown: bool = False


@dataclass(frozen=True, slots=True)
class PathResult:
    ids: tuple[str, ...]
    route_ids: tuple[str, ...]
    metric: MetricSummary
    expansions: int
    basis: str = "authored-directed-routes"
    filters: dict[str, Any] | None = None
    partial: bool = False
    unknown: bool = False


@dataclass(frozen=True, slots=True)
class Adjacency:
    from_location_id: str
    route_ids: tuple[str, ...]
    portal_ids: tuple[str, ...]
    target_location_ids: tuple[str, ...]
    position_portal_ids: tuple[str, ...]
    filters: dict[str, Any]
    basis: str = "authored-directed-route-and-portal-edges"
    partial: bool = False
    unknown: bool = False


@dataclass(frozen=True, slots=True)
class OverlayResult:
    ids: tuple[str, ...]
    story_time: StoryTime
    filters: dict[str, Any]
    basis: str = "authored-overlay-membership"
    partial: bool = False
    unknown: bool = False
    cursor: str | None = None


@dataclass(frozen=True, slots=True)
class Reachability:
    from_location_id: str
    ids: tuple[str, ...]
    expansions: int
    filters: dict[str, Any]
    basis: str = "authored-directed-route-and-location-portal-edges"
    partial: bool = False
    unknown: bool = False


def _finite_numbers(value: Any) -> tuple[int | float, ...] | None:
    if not isinstance(value, (tuple, list)) or len(value) not in (2, 3):
        return None
    if any(type(item) not in (int, float) or not math.isfinite(item) for item in value):
        return None
    return tuple(value)


def _cursor_payload(cursor: str) -> dict[str, Any] | None:
    if not isinstance(cursor, str) or not cursor or len(cursor) > 2_048:
        return None
    try:
        padded = cursor + "=" * (-len(cursor) % 4)
        raw = base64.urlsafe_b64decode(padded.encode("ascii"))
        value = json.loads(raw)
    except (UnicodeError, ValueError, TypeError):
        return None
    return value if isinstance(value, dict) and set(value) == {"binding", "last"} else None


def _cursor(binding: str, last: tuple[int, str]) -> str:
    raw = canonical_json({"binding": binding, "last": [last[0], last[1]]}).encode("utf-8")
    return base64.urlsafe_b64encode(raw).decode("ascii").rstrip("=")


def _request_binding(revision: str, kind: str, value: dict[str, Any]) -> str:
    payload = canonical_json({"protocol": PROTOCOL, "revision": revision, "kind": kind, "request": value}).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def _metric_number(value: Any) -> int | Decimal | None:
    """Return an exact, non-negative weight from a compiled NUMERIC cell.

    SQLite preserves integral NUMERIC values as Python integers but returns
    non-integral values as binary floats.  Decimal.from_float retains the exact
    stored float for comparisons, so large integers and close mixed numeric
    paths are never collapsed by an eager float coercion.
    """
    if type(value) is int:
        return value if value >= 0 else None
    if type(value) is float and math.isfinite(value) and value >= 0:
        return Decimal.from_float(value)
    return None


def _public_metric_number(value: int | Decimal) -> int | float | None:
    if type(value) is int or value == value.to_integral_value():
        return int(value)
    converted = float(value)
    return converted if math.isfinite(converted) else None


def _squared_point_segment_distance(point: tuple[int | float, ...], start: tuple[int | float, ...], end: tuple[int | float, ...]) -> float:
    direction = tuple(right - left for left, right in zip(start, end))
    denominator = sum(value * value for value in direction)
    if denominator == 0:
        return sum((left - right) ** 2 for left, right in zip(point, start))
    progress = sum((coordinate - origin) * delta for coordinate, origin, delta in zip(point, start, direction)) / denominator
    progress = min(1.0, max(0.0, progress))
    return sum((coordinate - (origin + progress * delta)) ** 2 for coordinate, origin, delta in zip(point, start, direction))


def _inside_polygon_2d(point: tuple[int | float, ...], vertices: tuple[tuple[int | float, ...], ...]) -> bool:
    """Return whether a 2D point is inside a non-degenerate authored polygon."""
    inside = False
    x, y = point
    for start, end in zip(vertices, vertices[1:] + vertices[:1]):
        if (start[1] > y) != (end[1] > y):
            crossing = (end[0] - start[0]) * (y - start[1]) / (end[1] - start[1]) + start[0]
            if x < crossing:
                inside = not inside
    return inside


def _squared_point_polygon_distance(point: tuple[int | float, ...], vertices: tuple[tuple[int | float, ...], ...]) -> float:
    segments = tuple(zip(vertices, vertices[1:] + vertices[:1]))
    boundary = min(_squared_point_segment_distance(point, start, end) for start, end in segments)
    if len(point) == 2 and _inside_polygon_2d(point, vertices):
        return 0.0
    if len(point) != 3:
        return boundary
    origin = vertices[0]
    normal: tuple[float, float, float] | None = None
    for first in vertices[1:]:
        first_vector = tuple(value - base for value, base in zip(first, origin))
        for second in vertices[2:]:
            second_vector = tuple(value - base for value, base in zip(second, origin))
            candidate = (
                first_vector[1] * second_vector[2] - first_vector[2] * second_vector[1],
                first_vector[2] * second_vector[0] - first_vector[0] * second_vector[2],
                first_vector[0] * second_vector[1] - first_vector[1] * second_vector[0],
            )
            if sum(value * value for value in candidate) != 0:
                normal = candidate
                break
        if normal is not None:
            break
    if normal is None:
        return boundary
    normal_squared = sum(value * value for value in normal)
    offset = sum((coordinate - base) * normal_value for coordinate, base, normal_value in zip(point, origin, normal))
    projected = tuple(coordinate - offset * normal_value / normal_squared for coordinate, normal_value in zip(point, normal))
    dropped = max(range(3), key=lambda index: abs(normal[index]))
    projected_2d = tuple(value for index, value in enumerate(projected) if index != dropped)
    polygon_2d = tuple(tuple(value for index, value in enumerate(vertex) if index != dropped) for vertex in vertices)
    return offset * offset / normal_squared if _inside_polygon_2d(projected_2d, polygon_2d) else boundary


def _squared_geometry_distance(point: tuple[int | float, ...], kind: Any, encoded: Any) -> float | None:
    """Calculate the exact point-to-authored-geometry distance after Btree culling."""
    if not isinstance(kind, str) or not isinstance(encoded, str):
        return None
    try:
        coordinates = json.loads(encoded)
    except (TypeError, ValueError):
        return None
    if kind == "point":
        vertices = (_finite_numbers(coordinates),)
    elif kind in {"line", "polygon"} and isinstance(coordinates, list):
        vertices = tuple(_finite_numbers(item) for item in coordinates)
    else:
        return None
    if not vertices or any(vertex is None or len(vertex) != len(point) for vertex in vertices):
        return None
    typed_vertices = tuple(vertex for vertex in vertices if vertex is not None)
    if kind == "point":
        return sum((left - right) ** 2 for left, right in zip(point, typed_vertices[0]))
    if len(typed_vertices) < 2:
        return None
    if kind == "polygon":
        return _squared_point_polygon_distance(point, typed_vertices)
    return min(_squared_point_segment_distance(point, start, end) for start, end in zip(typed_vertices, typed_vertices[1:]))


def _route_filters(modes: frozenset[str] | None) -> dict[str, Any]:
    return {"modes": sorted(modes) if modes is not None else None, "availability": ["open"]}


class SpatialStore:
    """A revision-citable reader over one already-compiled spatial projection.

    The constructor deliberately does not build a projection and makes no
    attempt to recognise a source world.  That prevents latent v0.7 query code
    from becoming an accidental generic runtime entry point.
    """

    def __init__(self, connection: sqlite3.Connection, revision: str):
        if not isinstance(connection, sqlite3.Connection):
            raise TypeError("connection must be sqlite3.Connection")
        if not isinstance(revision, str) or not revision:
            raise ValueError("revision must be non-empty")
        self.connection, self.revision = connection, revision
        rows = connection.execute("SELECT name,source_ordinal FROM spatial_capability ORDER BY source_ordinal").fetchall()
        capabilities = tuple(str(row[0]) for row in rows)
        ordinals = tuple(row[1] for row in rows)
        if (
            any(name not in CAPABILITY_ORDER for name in capabilities)
            or capabilities != tuple(sorted(capabilities, key=CAPABILITY_ORDER.index))
            or any(type(ordinal) is not int or ordinal < 0 for ordinal in ordinals)
            or ordinals != tuple(sorted(ordinals))
        ):
            raise ValueError("compiled capability registry order is invalid")
        self.capabilities = capabilities
        self._capability_set = frozenset(capabilities)

    @property
    def context(self) -> QueryContext:
        return QueryContext(PROTOCOL, self.revision, self.capabilities)

    def _available(self, capability: str) -> SpatialOutcome | None:
        if "spatial-core-v1" not in self._capability_set or capability not in self._capability_set:
            return SpatialOutcome(SpatialOutcomeKind.UNAVAILABLE, self.revision, reason=SpatialReason.REQUEST, detail=f"compiled projection lacks {capability}")
        return None

    def _limit(self, limit: Any) -> SpatialOutcome | None:
        if type(limit) is not int or not MIN_LIMIT <= limit <= MAX_LIMIT:
            return SpatialOutcome(SpatialOutcomeKind.LIMIT, self.revision, value=(), reason=SpatialReason.LIMIT, detail=f"limit must be {MIN_LIMIT}..{MAX_LIMIT}")
        return None

    def _page_cursor(self, *, kind: str, request: dict[str, Any], cursor: str | None) -> tuple[str, tuple[int, str] | None] | SpatialOutcome:
        binding = _request_binding(self.revision, kind, request)
        if cursor is None:
            return binding, None
        parsed = _cursor_payload(cursor)
        if parsed is None or parsed["binding"] != binding:
            return SpatialOutcome(SpatialOutcomeKind.INVALID, self.revision, reason=SpatialReason.CURSOR, detail="cursor is not bound to this normalized request and revision")
        last = parsed["last"]
        if not isinstance(last, list) or len(last) != 2 or type(last[0]) is not int or not isinstance(last[1], str):
            return SpatialOutcome(SpatialOutcomeKind.INVALID, self.revision, reason=SpatialReason.CURSOR, detail="cursor key is invalid")
        return binding, (last[0], last[1])

    def containment(self, location_id: str) -> SpatialOutcome:
        unavailable = self._available("spatial-core-v1")
        if unavailable: return unavailable
        if not isinstance(location_id, str) or not location_id:
            return SpatialOutcome(SpatialOutcomeKind.INVALID, self.revision, reason=SpatialReason.REQUEST, detail="location_id must be a non-empty id")
        # Follow the primary-key lookup one parent at a time. This remains
        # proportional to hierarchy depth even in a 100k-location catalogue.
        path: list[str] = []
        seen: set[str] = set()
        current = location_id
        for _ in range(MAX_HIERARCHY_EXPANSIONS):
            if current in seen:
                return SpatialOutcome(SpatialOutcomeKind.UNAVAILABLE, self.revision, reason=SpatialReason.REQUEST, detail="compiled hierarchy is cyclic")
            row = self.connection.execute("SELECT parent_id FROM spatial_location WHERE id=?", (current,)).fetchone()
            if row is None:
                detail = "location is absent from compiled projection" if not path else "compiled hierarchy reference is unavailable"
                return SpatialOutcome(SpatialOutcomeKind.UNAVAILABLE, self.revision, reason=SpatialReason.REQUEST, detail=detail)
            seen.add(current); path.append(current)
            if row[0] is None:
                return SpatialOutcome(SpatialOutcomeKind.OK, self.revision, HierarchyPath(tuple(reversed(path))))
            current = row[0]
        return SpatialOutcome(SpatialOutcomeKind.LIMIT, self.revision, reason=SpatialReason.LIMIT, detail="hierarchy expansion budget exceeded")

    def children(self, location_id: str, *, limit: int = 100, cursor: str | None = None) -> SpatialOutcome:
        unavailable = self._available("spatial-core-v1")
        if unavailable: return unavailable
        bad_limit = self._limit(limit)
        if bad_limit: return bad_limit
        if not isinstance(location_id, str) or not location_id:
            return SpatialOutcome(SpatialOutcomeKind.INVALID, self.revision, reason=SpatialReason.REQUEST, detail="location_id must be a non-empty id")
        if self.connection.execute("SELECT 1 FROM spatial_location WHERE id=?", (location_id,)).fetchone() is None:
            return SpatialOutcome(SpatialOutcomeKind.UNAVAILABLE, self.revision, reason=SpatialReason.REQUEST, detail="location is absent from compiled projection")
        request = {"location_id": location_id, "limit": limit}
        page = self._page_cursor(kind="children", request=request, cursor=cursor)
        if isinstance(page, SpatialOutcome): return page
        binding, last = page
        params: list[Any] = [location_id]
        clause = "parent_id=?"
        if last is not None:
            clause += " AND (source_ordinal>? OR (source_ordinal=? AND id>?))"; params += [last[0], last[0], last[1]]
        rows = self.connection.execute("SELECT id,source_ordinal FROM spatial_location WHERE " + clause + " ORDER BY source_ordinal,id LIMIT ?", (*params, limit + 1)).fetchall()
        records = rows[:limit]
        next_cursor = _cursor(binding, (records[-1][1], records[-1][0])) if len(rows) > limit and records else None
        return SpatialOutcome(SpatialOutcomeKind.OK, self.revision, CataloguePage(tuple(row[0] for row in records), "authored-parent-id", None, {"parent_id": location_id}, cursor=next_cursor))

    def bbox(self, map_id: str, bounds: BoundingBox | Any, *, relation: str = "intersects", limit: int = 100, cursor: str | None = None) -> SpatialOutcome:
        unavailable = self._available("geometry-v1")
        if unavailable: return unavailable
        bad_limit = self._limit(limit)
        if bad_limit: return bad_limit
        if not isinstance(map_id, str) or not map_id:
            return SpatialOutcome(SpatialOutcomeKind.INVALID, self.revision, reason=SpatialReason.REQUEST, detail="map_id must be a non-empty id")
        if isinstance(bounds, BoundingBox): minimum, maximum = bounds.minimum, bounds.maximum
        elif isinstance(bounds, dict): minimum, maximum = bounds.get("min"), bounds.get("max")
        else: minimum = maximum = None
        minimum, maximum = _finite_numbers(minimum), _finite_numbers(maximum)
        if minimum is None or maximum is None or len(minimum) != len(maximum) or any(left > right for left, right in zip(minimum, maximum)):
            return SpatialOutcome(SpatialOutcomeKind.INVALID, self.revision, reason=SpatialReason.GEOMETRY, detail="bounds must be finite ordered 2D or 3D coordinates")
        map_row = self.connection.execute("SELECT unit,z_policy FROM spatial_map WHERE id=?", (map_id,)).fetchone()
        if map_row is None:
            return SpatialOutcome(SpatialOutcomeKind.UNAVAILABLE, self.revision, reason=SpatialReason.GEOMETRY, detail="map is absent from compiled projection")
        dimensions = 3 if map_row[1] == "required" else 2
        if map_row[1] == "optional-level":
            row = self.connection.execute("SELECT min_z IS NOT NULL FROM spatial_map WHERE id=?", (map_id,)).fetchone()
            dimensions = 3 if row and row[0] else 2
        if len(minimum) != dimensions:
            return SpatialOutcome(SpatialOutcomeKind.UNAVAILABLE, self.revision, reason=SpatialReason.GEOMETRY, detail="bounds dimensionality is incompatible with map")
        if relation not in {"intersects", "within"}:
            return SpatialOutcome(SpatialOutcomeKind.INVALID, self.revision, reason=SpatialReason.REQUEST, detail="bbox relation must be intersects or within")
        request = {"map_id": map_id, "bounds": [list(minimum), list(maximum)], "relation": relation, "limit": limit}
        page = self._page_cursor(kind="bbox", request=request, cursor=cursor)
        if isinstance(page, SpatialOutcome): return page
        binding, last = page
        if relation == "intersects":
            clauses = ["map_id=?", "min_x<=?", "max_x>=?", "min_y<=?", "max_y>=?"]
            params: list[Any] = [map_id, maximum[0], minimum[0], maximum[1], minimum[1]]
        else:
            clauses = ["map_id=?", "min_x>=?", "max_x<=?", "min_y>=?", "max_y<=?"]
            params = [map_id, minimum[0], maximum[0], minimum[1], maximum[1]]
        if dimensions == 3:
            clauses += ["min_z<=?", "max_z>=?"] if relation == "intersects" else ["min_z>=?", "max_z<=?"]
            params += [maximum[2], minimum[2]] if relation == "intersects" else [minimum[2], maximum[2]]
        if last is not None:
            clauses.append("(source_ordinal>? OR (source_ordinal=? AND id>?))"); params += [last[0], last[0], last[1]]
        rows = self.connection.execute("SELECT id,source_ordinal FROM spatial_location WHERE " + " AND ".join(clauses) + " ORDER BY source_ordinal,id LIMIT ?", (*params, limit + 1)).fetchall()
        records = rows[:limit]
        next_cursor = _cursor(binding, (records[-1][1], records[-1][0])) if len(rows) > limit and records else None
        return SpatialOutcome(SpatialOutcomeKind.OK, self.revision, CataloguePage(tuple(item[0] for item in records), "authored-geometry-bounds", map_row[0], {"map_id": map_id, "relation": relation}, cursor=next_cursor))

    def nearby(self, target: MapPosition | Any = None, *, radius: int | float | None = None, position: MapPosition | Any = None, maximum: int | float | None = None, limit: int = 100, cursor: str | None = None) -> SpatialOutcome:
        unavailable = self._available("geometry-v1")
        if unavailable: return unavailable
        bad_limit = self._limit(limit)
        if bad_limit: return bad_limit
        operand = target if target is not None else position
        radius = maximum if radius is None else radius
        if isinstance(operand, MapPosition): map_id, coordinates = operand.map_id, operand.coordinates
        elif isinstance(operand, dict): map_id, coordinates = operand.get("map_id"), operand.get("coordinates")
        else: map_id = coordinates = None
        point = _finite_numbers(coordinates)
        if not isinstance(map_id, str) or point is None or type(radius) not in (int, float) or not math.isfinite(radius) or radius < 0:
            return SpatialOutcome(SpatialOutcomeKind.INVALID, self.revision, reason=SpatialReason.REQUEST, detail="nearby requires a finite map position and non-negative radius")
        # Nearby is a same-map geometric candidate read.  It does not create a
        # route or rank by an undocumented travel estimate.
        box = BoundingBox(tuple(value - radius for value in point), tuple(value + radius for value in point))
        map_row = self.connection.execute("SELECT unit,z_policy FROM spatial_map WHERE id=?", (map_id,)).fetchone()
        if map_row is None:
            return SpatialOutcome(SpatialOutcomeKind.UNAVAILABLE, self.revision, reason=SpatialReason.GEOMETRY, detail="map is absent from compiled projection")
        expected_dimensions = 3 if map_row[1] == "required" else 2
        if map_row[1] == "optional-level":
            expected_dimensions = 3 if self.connection.execute("SELECT min_z IS NOT NULL FROM spatial_map WHERE id=?", (map_id,)).fetchone()[0] else 2
        if len(point) != expected_dimensions:
            return SpatialOutcome(SpatialOutcomeKind.UNAVAILABLE, self.revision, reason=SpatialReason.GEOMETRY, detail="nearby position dimensionality is incompatible with map")
        request = {"map_id": map_id, "coordinates": list(point), "radius": radius, "limit": limit}
        cursor_data = self._page_cursor(kind="nearby", request=request, cursor=cursor)
        if isinstance(cursor_data, SpatialOutcome): return cursor_data
        binding, last = cursor_data
        candidates = self.connection.execute(
            "SELECT id,source_ordinal,geometry_kind,geometry_json FROM spatial_location "
            "WHERE map_id=? AND min_x<=? AND max_x>=? AND min_y<=? AND max_y>=? ORDER BY source_ordinal,id LIMIT ?",
            (map_id, box.maximum[0], box.minimum[0], box.maximum[1], box.minimum[1], MAX_GEOMETRY_CANDIDATES + 1),
        ).fetchall()
        if len(candidates) > MAX_GEOMETRY_CANDIDATES:
            return SpatialOutcome(SpatialOutcomeKind.LIMIT, self.revision, reason=SpatialReason.LIMIT, detail="geometry candidate budget exceeded")
        selected: list[tuple[str, int]] = []
        for ident, ordinal, kind, geometry in candidates:
            squared = _squared_geometry_distance(point, kind, geometry)
            if squared is None:
                return SpatialOutcome(
                    SpatialOutcomeKind.UNAVAILABLE,
                    self.revision,
                    reason=SpatialReason.GEOMETRY,
                    subreason=SpatialSubreason.UNKNOWN_COORDINATE,
                    detail="compiled candidate has unavailable authored coordinates",
                )
            if squared <= radius * radius:
                selected.append((ident, ordinal))
        if last is not None: selected = [item for item in selected if (item[1], item[0]) > last]
        rows = selected[:limit]
        next_cursor = _cursor(binding, (rows[-1][1], rows[-1][0])) if len(selected) > limit and rows else None
        return SpatialOutcome(SpatialOutcomeKind.OK, self.revision, CataloguePage(tuple(item[0] for item in rows), "same-map-authored-geometry", map_row[0], {"map_id": map_id, "radius": radius}, cursor=next_cursor))

    def adjacency(self, location_id: str, *, modes: Iterable[str] | None = None, limit: int = 100) -> SpatialOutcome:
        """Return only authored outbound route/portal facts, never a closure."""
        unavailable = self._available("route-v1")
        if unavailable: return unavailable
        bad_limit = self._limit(limit)
        if bad_limit: return bad_limit
        if not isinstance(location_id, str) or not location_id:
            return SpatialOutcome(SpatialOutcomeKind.INVALID, self.revision, reason=SpatialReason.REQUEST, detail="location_id must be a non-empty id")
        if self.connection.execute("SELECT 1 FROM spatial_location WHERE id=?", (location_id,)).fetchone() is None:
            return SpatialOutcome(SpatialOutcomeKind.UNAVAILABLE, self.revision, reason=SpatialReason.PATH, detail="endpoint is unavailable")
        wanted_modes = None if modes is None else frozenset(modes)
        if wanted_modes is not None and (not wanted_modes or any(not isinstance(mode, str) or not mode for mode in wanted_modes)):
            return SpatialOutcome(SpatialOutcomeKind.INVALID, self.revision, reason=SpatialReason.REQUEST, detail="modes must be non-empty authored mode ids")
        route_rows = self.connection.execute(
            "SELECT edge.route_id,edge.to_location_id,route.source_ordinal FROM spatial_route_edge AS edge "
            "JOIN spatial_route AS route ON route.id=edge.route_id WHERE edge.from_location_id=? AND route.availability='open' "
            "ORDER BY route.source_ordinal,edge.route_id,edge.to_location_id", (location_id,)).fetchall()
        portal_rows = self.connection.execute(
            "SELECT id,target_kind,target_location_id,source_ordinal FROM spatial_portal WHERE from_location_id=? ORDER BY source_ordinal,id", (location_id,)).fetchall()
        def accepted(table: str, column: str, ident: str) -> bool:
            return wanted_modes is None or bool(wanted_modes.intersection(row[0] for row in self.connection.execute(f"SELECT mode FROM {table} WHERE {column}=?", (ident,))))
        routes = [(ident, target) for ident, target, _ordinal in route_rows if accepted("spatial_route_mode", "route_id", ident)]
        portals = [(ident, target_kind, target) for ident, target_kind, target, _ordinal in portal_rows if accepted("spatial_portal_mode", "portal_id", ident)]
        if len(routes) + len(portals) > limit:
            return SpatialOutcome(SpatialOutcomeKind.LIMIT, self.revision, reason=SpatialReason.LIMIT, detail="adjacency result budget exceeded")
        return SpatialOutcome(SpatialOutcomeKind.OK, self.revision, Adjacency(location_id, tuple(item[0] for item in routes), tuple(item[0] for item in portals), tuple(item[1] for item in routes) + tuple(item[2] for item in portals if item[1] == "location"), tuple(item[0] for item in portals if item[1] == "position"), _route_filters(wanted_modes)))

    def reachability(self, from_location_id: str, *, modes: Iterable[str] | None = None, limit: int = 100) -> SpatialOutcome:
        """Bounded directed traversal of authored routes and location portals."""
        unavailable = self._available("route-v1")
        if unavailable: return unavailable
        bad_limit = self._limit(limit)
        if bad_limit: return bad_limit
        if not isinstance(from_location_id, str) or not from_location_id:
            return SpatialOutcome(SpatialOutcomeKind.INVALID, self.revision, reason=SpatialReason.REQUEST, detail="from_location_id must be a non-empty id")
        if self.connection.execute("SELECT 1 FROM spatial_location WHERE id=?", (from_location_id,)).fetchone() is None:
            return SpatialOutcome(SpatialOutcomeKind.UNAVAILABLE, self.revision, reason=SpatialReason.PATH, detail="endpoint is unavailable")
        wanted_modes = None if modes is None else frozenset(modes)
        if wanted_modes is not None and (not wanted_modes or any(not isinstance(mode, str) or not mode for mode in wanted_modes)):
            return SpatialOutcome(SpatialOutcomeKind.INVALID, self.revision, reason=SpatialReason.REQUEST, detail="modes must be non-empty authored mode ids")
        seen, queue, expansions = {from_location_id}, [from_location_id], 0
        while queue:
            node = queue.pop(0); expansions += 1
            if expansions > MAX_ROUTE_EXPANSIONS:
                return SpatialOutcome(SpatialOutcomeKind.LIMIT, self.revision, reason=SpatialReason.LIMIT, detail="route expansion budget exceeded")
            adjacency = self.adjacency(node, modes=wanted_modes, limit=MAX_LIMIT)
            if adjacency.kind is not SpatialOutcomeKind.OK: return adjacency
            next_ids = adjacency.value.target_location_ids
            for ident in next_ids:
                if ident not in seen:
                    seen.add(ident); queue.append(ident)
                    if len(seen) > limit:
                        return SpatialOutcome(SpatialOutcomeKind.LIMIT, self.revision, reason=SpatialReason.LIMIT, detail="reachable result budget exceeded")
        ordered = tuple(row[0] for row in self.connection.execute("SELECT id FROM spatial_location WHERE id IN (" + ",".join("?" for _ in seen) + ") ORDER BY source_ordinal,id", tuple(seen)))
        return SpatialOutcome(SpatialOutcomeKind.OK, self.revision, Reachability(from_location_id, ordered, expansions, _route_filters(wanted_modes)))

    def _path_subreason(self, from_location_id: str, to_location_id: str, modes: frozenset[str] | None) -> SpatialSubreason:
        """Classify a closed path without changing its ratified PATH reason."""
        # Walk the bounded open frontier and inspect every authored outbound
        # edge it reaches. A closed edge after an open first leg is still the
        # observable obstruction; looking only beside the source mislabels it
        # as generic unreachable.
        seen, queue, expansions = {from_location_id}, [from_location_id], 0
        saw_closed = saw_unavailable = False
        while queue and expansions < MAX_ROUTE_EXPANSIONS:
            node = queue.pop(0); expansions += 1
            rows = self.connection.execute(
                "SELECT edge.to_location_id,route.id,route.availability FROM spatial_route_edge AS edge "
                "JOIN spatial_route AS route ON route.id=edge.route_id WHERE edge.from_location_id=?",
                (node,),
            ).fetchall()
            for target, route_id, availability in rows:
                route_modes = frozenset(item[0] for item in self.connection.execute("SELECT mode FROM spatial_route_mode WHERE route_id=?", (route_id,)))
                if modes is not None and not modes.intersection(route_modes):
                    continue
                if availability == "open":
                    if target not in seen:
                        seen.add(target); queue.append(target)
                elif availability == "closed":
                    saw_closed = True
                else:
                    saw_unavailable = True
        if saw_closed:
            return SpatialSubreason.CLOSED_EDGE
        if saw_unavailable:
            return SpatialSubreason.UNAVAILABLE_EDGE
        endpoint_maps = self.connection.execute(
            "SELECT has_spatial,map_id FROM spatial_location WHERE id IN (?,?) ORDER BY id",
            (from_location_id, to_location_id),
        ).fetchall()
        if len(endpoint_maps) == 2 and all(row[0] and row[1] is not None for row in endpoint_maps) and endpoint_maps[0][1] != endpoint_maps[1][1]:
            return SpatialSubreason.CROSS_MAP_DISCONTINUITY
        return SpatialSubreason.UNREACHABLE

    def path(self, from_location_id: str, to_location_id: str, *, metric: str, unit: str | None = None, modes: Iterable[str] | None = None, limit: int = 100) -> SpatialOutcome:
        unavailable = self._available("route-v1")
        if unavailable: return unavailable
        bad_limit = self._limit(limit)
        if bad_limit: return bad_limit
        if not isinstance(from_location_id, str) or not isinstance(to_location_id, str) or not from_location_id or not to_location_id or metric not in {"route_distance", "travel_cost", "duration"}:
            return SpatialOutcome(SpatialOutcomeKind.INVALID, self.revision, reason=SpatialReason.REQUEST, detail="path requires ids and one declared metric")
        wanted_modes = None if modes is None else frozenset(modes)
        if wanted_modes is not None and (not wanted_modes or any(not isinstance(mode, str) or not mode for mode in wanted_modes)):
            return SpatialOutcome(SpatialOutcomeKind.INVALID, self.revision, reason=SpatialReason.REQUEST, detail="modes must be non-empty authored mode ids")
        if self.connection.execute("SELECT 1 FROM spatial_location WHERE id=?", (from_location_id,)).fetchone() is None or self.connection.execute("SELECT 1 FROM spatial_location WHERE id=?", (to_location_id,)).fetchone() is None:
            return SpatialOutcome(SpatialOutcomeKind.UNAVAILABLE, self.revision, reason=SpatialReason.PATH, detail="endpoint is unavailable")
        if unit is not None and (not isinstance(unit, str) or not unit):
            return SpatialOutcome(SpatialOutcomeKind.INVALID, self.revision, reason=SpatialReason.REQUEST, detail="metric unit must be a non-empty authored label")
        if from_location_id == to_location_id:
            return SpatialOutcome(SpatialOutcomeKind.OK, self.revision, PathResult((from_location_id,), (), MetricSummary(metric, 0, unit, True), 0, filters=_route_filters(wanted_modes)))
        field, unit_field = (metric, metric + "_unit")
        # Stream each expansion from the indexed directed adjacency table.
        # A missing metric is never substituted; each distinct authored unit is
        # its own graph, so incompatible units can never be added together.
        # Unit is ordered before cost so unitless alternatives can share a
        # bounded traversal without ever comparing pace against miles. Only
        # costs in the same unit participate in a Dijkstra ordering.
        frontier: list[tuple[str, int | Decimal, tuple[tuple[int, str], ...], str, str | None, tuple[str, ...], tuple[str, ...]]] = [(unit or "", 0, (), from_location_id, unit, (from_location_id,), ())]
        best: dict[tuple[str, str | None], tuple[int | Decimal, tuple[tuple[int, str], ...]]] = {(from_location_id, unit): (0, ())}
        expansions = 0
        unknown = False
        incompatible_unit = False
        completed: dict[str, tuple[int | Decimal, tuple[tuple[int, str], ...], tuple[str, ...], tuple[str, ...], int]] = {}
        while frontier:
            _unit_key, cost, tie, node, current_unit, ids, route_ids = heapq.heappop(frontier)
            if best.get((node, current_unit)) != (cost, tie): continue
            if node == to_location_id:
                assert current_unit is not None
                completed.setdefault(current_unit, (cost, tie, ids, route_ids, expansions))
                continue
            expansions += 1
            if expansions > MAX_ROUTE_EXPANSIONS:
                return SpatialOutcome(SpatialOutcomeKind.LIMIT, self.revision, reason=SpatialReason.LIMIT, detail="route expansion budget exceeded")
            rows = self.connection.execute(
                f"SELECT edge.to_location_id,route.id,route.source_ordinal,route.{field},route.{unit_field} "
                "FROM spatial_route_edge AS edge JOIN spatial_route AS route ON route.id=edge.route_id "
                "WHERE edge.from_location_id=? AND route.availability='open' ORDER BY route.source_ordinal,route.id,edge.to_location_id", (node,)).fetchall()
            for target, route_id, ordinal, value, edge_unit in rows:
                route_modes = frozenset(item[0] for item in self.connection.execute("SELECT mode FROM spatial_route_mode WHERE route_id=?", (route_id,)))
                if wanted_modes is not None and not wanted_modes.intersection(route_modes): continue
                if value is None or edge_unit is None:
                    unknown = True; continue
                if not isinstance(edge_unit, str) or not edge_unit:
                    unknown = True; continue
                if current_unit is not None and edge_unit != current_unit:
                    # This edge is a valid authored graph in a different typed
                    # metric, not evidence for an unavailable same-unit path.
                    incompatible_unit = True
                    continue
                weight = _metric_number(value)
                if weight is None:
                    unknown = True; continue
                candidate = (cost + weight, tie + ((ordinal, route_id),))
                state = (target, edge_unit)
                if state not in best or candidate < best[state]:
                    best[state] = candidate
                    heapq.heappush(frontier, (edge_unit, candidate[0], candidate[1], target, edge_unit, ids + (target,), route_ids + (route_id,)))
        if len(completed) == 1:
            resolved_unit, (cost, _tie, ids, route_ids, result_expansions) = next(iter(completed.items()))
            if len(ids) > limit:
                return SpatialOutcome(SpatialOutcomeKind.LIMIT, self.revision, reason=SpatialReason.LIMIT, detail="path result budget exceeded")
            numeric = _public_metric_number(cost)
            if numeric is None:
                return SpatialOutcome(SpatialOutcomeKind.UNAVAILABLE, self.revision, reason=SpatialReason.METRIC, detail="authored metric total is not representable")
            return SpatialOutcome(SpatialOutcomeKind.OK, self.revision, PathResult(ids, route_ids, MetricSummary(metric, numeric, resolved_unit, True), result_expansions, filters=_route_filters(wanted_modes)))
        if len(completed) > 1:
            return SpatialOutcome(SpatialOutcomeKind.UNAVAILABLE, self.revision, reason=SpatialReason.METRIC, subreason=SpatialSubreason.AMBIGUOUS_UNIT, detail="multiple authored metric units reach the endpoint")
        reason = SpatialReason.METRIC if unknown or incompatible_unit else SpatialReason.PATH
        return SpatialOutcome(
            SpatialOutcomeKind.UNAVAILABLE,
            self.revision,
            reason=reason,
            subreason=SpatialSubreason.UNKNOWN_METRIC if unknown else (SpatialSubreason.INCOMPATIBLE_UNIT if incompatible_unit else self._path_subreason(from_location_id, to_location_id, wanted_modes)),
            detail="authored route has unknown requested metric" if unknown else ("authored metric units are incompatible" if incompatible_unit else "no authored directed path"),
        )

    def overlay_as_of(self, location_id: str, at: StoryTime | Any, *, audience: str, perspective: str, overlay_id: str | None = None, limit: int = 100, cursor: str | None = None) -> SpatialOutcome:
        unavailable = self._available("overlay-v1")
        if unavailable: return unavailable
        bad_limit = self._limit(limit)
        if bad_limit: return bad_limit
        if not isinstance(at, StoryTime):
            try: at = StoryTime.from_value(at)
            except ValueError:
                return SpatialOutcome(SpatialOutcomeKind.INVALID, self.revision, reason=SpatialReason.REQUEST, detail="as-of requires exact signed StoryTime")
        if not all(isinstance(value, str) and value for value in (location_id, audience, perspective)) or (overlay_id is not None and (not isinstance(overlay_id, str) or not overlay_id)):
            return SpatialOutcome(SpatialOutcomeKind.INVALID, self.revision, reason=SpatialReason.REQUEST, detail="overlay ids and scopes must be non-empty opaque ids")
        if self.connection.execute("SELECT 1 FROM spatial_location WHERE id=?", (location_id,)).fetchone() is None:
            return SpatialOutcome(SpatialOutcomeKind.UNAVAILABLE, self.revision, reason=SpatialReason.OVERLAY, detail="location is unavailable")
        request = {"location_id": location_id, "at": at.to_dict(), "audience": audience, "perspective": perspective, "overlay_id": overlay_id, "limit": limit}
        page = self._page_cursor(kind="overlay-as-of", request=request, cursor=cursor)
        if isinstance(page, SpatialOutcome): return page
        binding, last = page
        # The two forms are deliberately closed modes.  The catalogue form
        # only sees authorized memberships; it never probes an overlay ID.
        # An explicit ID first proves authorization without consulting
        # applicability.  That makes an absent ID and an unauthorized ID the
        # same stable forbidden outcome, while an authorized but inapplicable
        # overlay remains a valid empty result below.
        if overlay_id is not None:
            authorized = self.connection.execute(
                """SELECT 1
                     FROM spatial_overlay_audience AS audience
                     JOIN spatial_overlay_perspective AS perspective
                       ON perspective.overlay_id=audience.overlay_id
                    WHERE audience.overlay_id=?
                      AND audience.audience=?
                      AND perspective.perspective=?""",
                (overlay_id, audience, perspective),
            ).fetchone()
            if authorized is None:
                return SpatialOutcome(
                    SpatialOutcomeKind.FORBIDDEN,
                    self.revision,
                    reason=SpatialReason.OVERLAY,
                    detail="explicit overlay is not authorized in this scope",
                )

        # Authorization is inside the candidate query and therefore precedes
        # the 2k budget, ordering, pagination, and all serialization. Catalogue
        # callers never see a forbidden result for hidden records.
        time_clause = "(overlay.lifecycle='static' OR (overlay.timeline=? AND (overlay.start_tick<? OR (overlay.start_tick=? AND overlay.start_order<=?)) AND (overlay.end_tick>? OR (overlay.end_tick=? AND overlay.end_order>=?))))"
        base = """SELECT overlay.id,overlay.source_ordinal
                    FROM spatial_overlay AS overlay
                    JOIN spatial_overlay_location AS membership ON membership.overlay_id=overlay.id
                   WHERE membership.location_id=? AND """ + time_clause
        params: list[Any] = [location_id, at.timeline, at.tick, at.tick, at.order, at.tick, at.tick, at.order]
        if overlay_id is not None:
            base += " AND overlay.id=?"; params.append(overlay_id)
        base += " AND EXISTS (SELECT 1 FROM spatial_overlay_audience AS authorized_audience WHERE authorized_audience.overlay_id=overlay.id AND authorized_audience.audience=?)"
        base += " AND EXISTS (SELECT 1 FROM spatial_overlay_perspective AS authorized_perspective WHERE authorized_perspective.overlay_id=overlay.id AND authorized_perspective.perspective=?)"
        candidates = self.connection.execute(base + " ORDER BY overlay.source_ordinal,overlay.id LIMIT ?", (*params, audience, perspective, MAX_OVERLAY_CANDIDATES + 1)).fetchall()
        if len(candidates) > MAX_OVERLAY_CANDIDATES:
            return SpatialOutcome(SpatialOutcomeKind.LIMIT, self.revision, reason=SpatialReason.LIMIT, detail="overlay candidate budget exceeded")
        visible = candidates
        if last is not None:
            visible = [item for item in visible if (item[1], item[0]) > last]
        page_rows = visible[:limit]
        next_cursor = _cursor(binding, (page_rows[-1][1], page_rows[-1][0])) if len(visible) > limit and page_rows else None
        return SpatialOutcome(SpatialOutcomeKind.OK, self.revision, OverlayResult(tuple(item[0] for item in page_rows), at, {"audience": audience, "perspective": perspective, "horizon": at.to_dict()}, cursor=next_cursor))


# The explicit name is preferred by new callers.  The shorter name remains a
# source-compatible internal spelling while this latent component is reviewed.
SQLiteSpatialStore = SpatialStore
