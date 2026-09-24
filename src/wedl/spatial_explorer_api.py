"""Bounded HTTP-only spatial explorer projections over one compiled revision."""

from __future__ import annotations

import base64
from contextlib import closing
import hashlib
import json
import re
import sqlite3
from typing import Any

from .compiler import connect, require_database
from .errors import CompileRequired, RepositoryError, UsageError
from .spatial_api import _cache, _closed, _coordinates, _number, _story_time, _text
from .spatial_index import _story_time_digits
from .spatial_query import (BoundingBox, MAX_GEOMETRY_CANDIDATES, MAX_OVERLAY_CANDIDATES, SpatialOutcomeKind, SpatialStore,
                            _cursor as _store_cursor, _cursor_payload as _store_cursor_payload,
                            _request_binding as _store_request_binding)
from .util import canonical_json
from .v07 import CAPABILITY_ORDER


PROTOCOL = "wedl-spatial-explorer/v1"
OPERATIONS = ("catalog", "places", "viewport", "layers")
_SHA = re.compile(r"^[0-9a-f]{40}$")
_MAX_VERTICES = 10_000
_MAX_SEARCH_CANDIDATES = 2_000
_STATUSES = {"ok": 200, "invalid": 400, "unavailable": 409, "forbidden": 403, "limit": 422}


def status_code(value: dict[str, Any]) -> int:
    return _STATUSES[value["state"]]


def _cursor(binding: str, last: tuple[int, str]) -> str:
    raw = canonical_json({"binding": binding, "last": list(last)}).encode("utf-8")
    return base64.urlsafe_b64encode(raw).decode("ascii").rstrip("=")


def _page(request: dict[str, Any], revision: str) -> tuple[str, tuple[int, str] | None] | None:
    normalized = {key: value for key, value in request.items() if key != "cursor"}
    binding = hashlib.sha256(canonical_json({"revision": revision, "request": normalized}).encode("utf-8")).hexdigest()
    cursor = request.get("cursor")
    if cursor is None:
        return binding, None
    try:
        raw = base64.urlsafe_b64decode(cursor + "=" * (-len(cursor) % 4))
        value = json.loads(raw)
        last = value["last"]
        if (set(value) != {"binding", "last"} or value["binding"] != binding
                or not isinstance(last, list) or len(last) != 2
                or type(last[0]) is not int or not 0 <= last[0] <= 2**63 - 1
                or not isinstance(last[1], str) or not last[1]):
            return None
        last[1].encode("utf-8")
        return binding, (last[0], last[1])
    except (ValueError, KeyError, TypeError, UnicodeDecodeError):
        return None


def _common(request: Any, operation: str) -> dict[str, Any]:
    if operation not in OPERATIONS:
        raise UsageError("unsupported spatial explorer operation")
    common = {"protocol", "revision", "capabilities", "limit", "cursor"}
    fields = {
        "catalog": set(), "places": {"mode", "parentId", "query", "ids"},
        "viewport": {"mapId", "bounds", "relation"},
        "layers": {"mapId", "bounds", "relation", "asOf", "audience", "perspective", "overlayId"},
    }[operation]
    required = {"protocol", "limit"} if operation == "catalog" else common
    item = _closed(request, common | fields, required, f"{operation} explorer request")
    if item["protocol"] != PROTOCOL:
        raise UsageError("unsupported spatial explorer protocol")
    if type(item["limit"]) is not int or not 1 <= item["limit"] <= 100:
        raise UsageError("limit must be an integer from 1 to 100")
    cursor = item.get("cursor")
    if cursor is not None and (not isinstance(cursor, str) or not cursor or len(cursor) > 2048):
        raise UsageError("cursor must be null or a bounded opaque string")
    revision = item.get("revision")
    capabilities = item.get("capabilities")
    if operation == "catalog" and cursor is None and (revision is not None or capabilities is not None):
        raise UsageError("first catalog page must bootstrap without revision or capabilities")
    if operation != "catalog" or cursor is not None:
        if not isinstance(revision, str) or not _SHA.fullmatch(revision):
            raise UsageError("revision must be a lowercase Git SHA")
        if (not isinstance(capabilities, list) or any(not isinstance(value, str) or value not in CAPABILITY_ORDER for value in capabilities)
                or capabilities != sorted(set(capabilities), key=CAPABILITY_ORDER.index)):
            raise UsageError("capabilities must be in canonical order without duplicates")
    if operation == "catalog":
        return item
    if operation == "places":
        mode = item.get("mode")
        variants = {
            "roots": (set(), set()),
            "children": ({"parentId"}, {"parentId"}),
            "search": ({"query"}, {"query"}),
            "select": ({"ids"}, {"ids"}),
        }
        if not isinstance(mode, str) or mode not in variants:
            raise UsageError("places mode is invalid")
        allowed, required_mode = variants[mode]
        _closed(item, common | {"mode"} | allowed, common | {"mode"} | required_mode, "places request")
        if mode == "children":
            _text(item["parentId"], "parentId")
        if mode == "search":
            _text(item["query"], "query")
        if mode == "select":
            ids = item["ids"]
            if not isinstance(ids, list) or not 1 <= len(ids) <= 100 or any(not isinstance(value, str) or not value.strip() or len(value) > 256 for value in ids) or len(ids) != len(set(ids)):
                raise UsageError("ids must contain 1..100 unique place IDs")
        return item
    for key in ("mapId", "relation", "bounds"):
        if key not in item:
            raise UsageError(f"{key} is required")
    _text(item["mapId"], "mapId")
    if item["relation"] not in ("within", "intersects"):
        raise UsageError("relation must be within or intersects")
    bounds = _closed(item["bounds"], {"min", "max"}, {"min", "max"}, "bounds")
    minimum, maximum = _coordinates(bounds["min"], "bounds.min"), _coordinates(bounds["max"], "bounds.max")
    if len(minimum) != len(maximum) or any(left > right for left, right in zip(minimum, maximum)):
        raise UsageError("bounds must be ordered finite same-dimensional coordinates")
    if operation == "layers":
        for key in ("asOf", "audience", "perspective"):
            if key not in item:
                raise UsageError(f"{key} is required")
        _story_time(item["asOf"])
        _text(item["audience"], "audience")
        _text(item["perspective"], "perspective")
        if "overlayId" in item:
            _text(item["overlayId"], "overlayId")
    return item


def _outcome(world: Any, metadata: Any, operation: str, state: str, *, result: Any = None,
             code: str = "SPATIAL-REQUEST-001", detail: str | None = None) -> dict[str, Any]:
    value = {"protocol": PROTOCOL, "operation": operation, "revision": world.revision,
             "sourceSchema": world.schema,
             "capabilities": list(getattr(world.world_record, "frontmatter", {}).get("capabilities") or ()),
             "cache": _cache(world, metadata), "state": state}
    if state == "ok":
        value["result"] = result
    else:
        value["code"] = code
        if detail is not None:
            value["detail"] = detail
    return value


def _map(row: Any) -> dict[str, Any]:
    try:
        lower = [_number(value, "compiled map bound") for value in (row[6], row[7], row[8]) if value is not None]
        upper = [_number(value, "compiled map bound") for value in (row[9], row[10], row[11]) if value is not None]
    except UsageError as exc:
        raise ValueError("unsafe compiled map bound") from exc
    return {"id": row[0], "label": row[1], "crs": row[2], "axes": [row[3], row[4]], "unit": row[5],
            "bounds": {"min": lower, "max": upper}}


def _card(row: Any) -> dict[str, Any]:
    return {"id": row[0], "label": row[1], "parentId": row[2], "mapId": row[3],
            "geometryAvailable": bool(row[4]), "basis": "authored-location"}


def _features(connection: Any, ids: list[str]) -> list[dict[str, Any]] | None:
    if not ids:
        return []
    rows = connection.execute(
        "SELECT loc.id,entity.title,loc.map_id,loc.geometry_kind,loc.geometry_json "
        "FROM spatial_location AS loc JOIN entity ON entity.id=loc.id "
        "WHERE loc.id IN (" + ",".join("?" for _ in ids) + ")", ids).fetchall()
    by_id = {row[0]: row for row in rows}
    result: list[dict[str, Any]] = []
    vertices = 0
    for ident in ids:
        row = by_id[ident]
        coordinates = _geometry(row[3], row[4])
        vertices += 1 if row[3] == "point" else len(coordinates)
        if vertices > _MAX_VERTICES:
            return None
        result.append({"id": row[0], "label": row[1], "mapId": row[2],
                       "geometry": {"kind": row[3], "coordinates": coordinates},
                       "basis": "authored-geometry"})
    return result


def _geometry(kind: str, encoded: str) -> Any:
    try:
        coordinates = json.loads(encoded)
        if kind == "point":
            return list(_coordinates(coordinates, "compiled geometry"))
        if kind not in {"line", "polygon"} or not isinstance(coordinates, list) or len(coordinates) > _MAX_VERTICES:
            raise ValueError("invalid compiled geometry")
        return [list(_coordinates(point, "compiled geometry")) for point in coordinates]
    except (UsageError, TypeError, ValueError) as exc:
        raise ValueError("unsafe compiled geometry") from exc


def _map_row(connection: Any, map_id: str, bounds: dict[str, Any]) -> Any:
    row = connection.execute("SELECT id,crs,axis_first,axis_second,unit,z_policy,min_z,max_z FROM spatial_map WHERE id=?", (map_id,)).fetchone()
    if row is None:
        return None
    dimensions = 3 if row[5] == "required" or (row[5] == "optional-level" and row[6] is not None) else 2
    if len(bounds["min"]) != dimensions or len(bounds["max"]) != dimensions:
        return False
    return row


def _catalog(connection: Any, item: dict[str, Any], revision: str) -> tuple[str, Any, str, str | None]:
    page = _page(item, revision)
    if page is None:
        return "invalid", None, "SPATIAL-CURSOR-001", "cursor is not bound to this catalog page"
    binding, last = page
    params: list[Any] = []
    clause = ""
    if last is not None:
        # source_ordinal is unique, so the single range preserves order and
        # lets even an empty late page seek into its index.
        clause = "WHERE map.source_ordinal>?"
        params = [last[0]]
    rows = connection.execute("SELECT map.id,entity.title,map.crs,map.axis_first,map.axis_second,map.unit,"
        "map.min_x,map.min_y,map.min_z,map.max_x,map.max_y,map.max_z,map.source_ordinal "
        "FROM spatial_map AS map JOIN entity ON entity.id=map.id " + clause +
        " ORDER BY map.source_ordinal,map.id LIMIT ?", (*params, item["limit"] + 1)).fetchall()
    records = rows[:item["limit"]]
    next_cursor = _cursor(binding, (records[-1][12], records[-1][0])) if len(rows) > item["limit"] else None
    available = connection.execute("SELECT 1 FROM spatial_capability WHERE name='spatial-core-v1'").fetchone() is not None
    return "ok", {"spatialAvailable": available, "maps": [_map(row) for row in records], "nextCursor": next_cursor}, "", None


def _places(connection: Any, item: dict[str, Any], revision: str) -> tuple[str, Any, str, str | None]:
    page = _page(item, revision)
    if page is None:
        return "invalid", None, "SPATIAL-CURSOR-001", "cursor is not bound to this places request"
    binding, last = page
    mode = item["mode"]
    params: list[Any] = []
    clauses: list[str] = []
    if mode == "roots":
        clauses.append("loc.parent_id IS NULL")
    elif mode == "children":
        if connection.execute("SELECT 1 FROM spatial_location WHERE id=?", (item["parentId"],)).fetchone() is None:
            return "unavailable", None, "SPATIAL-REQUEST-001", "parent place is unavailable"
        clauses.append("loc.parent_id=?"); params.append(item["parentId"])
    elif mode == "select":
        ids = item["ids"]
        clauses.append("loc.id IN (" + ",".join("?" for _ in ids) + ")"); params.extend(ids)
    else:
        # Bound title documents before the place join. Otherwise a matching
        # non-place corpus can make an empty search scan the entire FTS set.
        # A quoted phrase avoids FTS query syntax from hostile input.
        query = item["query"].strip().replace('"', '""')
        if not query:
            return "invalid", None, "SPATIAL-REQUEST-001", "query must be nonblank"
        match = 'title : "' + query + '"'
        candidates = connection.execute(
            "SELECT doc.entity_id FROM search_fts JOIN search_document AS doc ON doc.rowid=search_fts.rowid "
            "WHERE search_fts MATCH ? LIMIT ?",
            (match, _MAX_SEARCH_CANDIDATES + 1)).fetchall()
        if len(candidates) > _MAX_SEARCH_CANDIDATES:
            return "limit", None, "SPATIAL-LIMIT-001", "place search candidate budget exceeded"
        if not candidates:
            return "ok", {"mode": mode, "places": [], "basis": "compiled-title-search", "nextCursor": None}, "", None
        ids = list(dict.fromkeys(row[0] for row in candidates))
        clauses.append("loc.id IN (" + ",".join("?" for _ in ids) + ")"); params.extend(ids)
    if last is not None:
        if mode in {"roots", "children"}:
            clauses.append("loc.id>?")
            params.append(last[1])
        else:
            clauses.append("loc.source_ordinal>?")
            params.append(last[0])
    order = "loc.id" if mode in {"roots", "children"} else "loc.source_ordinal,loc.id"
    sql = "SELECT loc.id,entity.title,loc.parent_id,loc.map_id,loc.has_spatial,loc.source_ordinal " \
          "FROM spatial_location AS loc JOIN entity ON entity.id=loc.id WHERE " + " AND ".join(clauses) + \
          " ORDER BY " + order + " LIMIT ?"
    rows = connection.execute(sql, (*params, item["limit"] + 1)).fetchall()
    records = rows[:item["limit"]]
    key = None if not records else ((0, records[-1][0]) if mode in {"roots", "children"} else (records[-1][5], records[-1][0]))
    next_cursor = _cursor(binding, key) if len(rows) > item["limit"] and key is not None else None
    basis = "compiled-title-search" if mode == "search" else "authored-parent-id" if mode in {"roots", "children"} else "authored-location-id"
    return "ok", {"mode": mode, "places": [_card(row) for row in records], "basis": basis, "nextCursor": next_cursor}, "", None


def _viewport(connection: Any, item: dict[str, Any], revision: str) -> tuple[str, Any, str, str | None]:
    page = _page(item, revision)
    if page is None:
        return "invalid", None, "SPATIAL-CURSOR-001", "cursor is not bound to this viewport request"
    binding, last = page
    map_row = _map_row(connection, item["mapId"], item["bounds"])
    if map_row is None:
        return "unavailable", None, "SPATIAL-GEOMETRY-001", "map is unavailable"
    if map_row is False:
        return "unavailable", None, "SPATIAL-GEOMETRY-001", "bounds dimensionality is incompatible with map"
    store = SpatialStore(connection, revision)
    store_binding = _store_request_binding(revision, "bbox", {
        "map_id": item["mapId"], "bounds": [item["bounds"]["min"], item["bounds"]["max"]],
        "relation": item["relation"], "limit": item["limit"]})
    store_page = _store_cursor(store_binding, last) if last is not None else None
    outcome = store.bbox(item["mapId"], BoundingBox(tuple(item["bounds"]["min"]), tuple(item["bounds"]["max"])),
                         relation=item["relation"], limit=item["limit"], cursor=store_page,
                         candidate_budget=MAX_GEOMETRY_CANDIDATES)
    if outcome.kind is not SpatialOutcomeKind.OK:
        return outcome.kind.value, None, outcome.reason.value if outcome.reason else "SPATIAL-REQUEST-001", outcome.detail
    features = _features(connection, list(outcome.value.ids))
    if features is None:
        return "limit", None, "SPATIAL-LIMIT-001", "viewport vertex budget exceeded"
    store_next = _store_cursor_payload(outcome.value.cursor) if outcome.value.cursor is not None else None
    next_cursor = _cursor(binding, tuple(store_next["last"])) if store_next is not None else None
    return "ok", {"mapId": item["mapId"], "crs": map_row[1], "axes": [map_row[2], map_row[3]],
                  "unit": map_row[4], "relation": item["relation"], "features": features,
                  "basis": "authored-geometry-bounds", "nextCursor": next_cursor}, "", None


def _layers(connection: Any, item: dict[str, Any], revision: str) -> tuple[str, Any, str, str | None]:
    page = _page(item, revision)
    if page is None:
        return "invalid", None, "SPATIAL-CURSOR-001", "cursor is not bound to this layers request"
    binding, last = page
    map_row = _map_row(connection, item["mapId"], item["bounds"])
    if map_row is None:
        return "unavailable", None, "SPATIAL-GEOMETRY-001", "map is unavailable"
    if map_row is False:
        return "unavailable", None, "SPATIAL-GEOMETRY-001", "bounds dimensionality is incompatible with map"
    overlay_id = item.get("overlayId")
    audience, perspective = item["audience"], item["perspective"]
    if overlay_id is not None:
        authorized = connection.execute(
            "SELECT 1 FROM spatial_overlay_audience AS a JOIN spatial_overlay_perspective AS p ON p.overlay_id=a.overlay_id "
            "WHERE a.overlay_id=? AND a.audience=? AND p.perspective=?", (overlay_id, audience, perspective)).fetchone()
        if authorized is None:
            return "forbidden", None, "SPATIAL-OVERLAY-001", "explicit overlay is not authorized in this scope"
    at = _story_time(item["asOf"])
    bounds, relation = item["bounds"], item["relation"]
    if relation == "intersects":
        geometry = "loc.min_x<=? AND loc.max_x>=? AND loc.min_y<=? AND loc.max_y>=?"
        values: list[Any] = [bounds["max"][0], bounds["min"][0], bounds["max"][1], bounds["min"][1]]
    else:
        geometry = "loc.min_x>=? AND loc.max_x<=? AND loc.min_y>=? AND loc.max_y<=?"
        values = [bounds["min"][0], bounds["max"][0], bounds["min"][1], bounds["max"][1]]
    if len(bounds["min"]) == 3:
        geometry += " AND loc.min_z<=? AND loc.max_z>=?" if relation == "intersects" else " AND loc.min_z>=? AND loc.max_z<=?"
        values.extend((bounds["max"][2], bounds["min"][2]) if relation == "intersects" else (bounds["min"][2], bounds["max"][2]))
    # Probe public viewport geometry first, then seek only selected-lens
    # memberships at those locations. Neither authorized overlays elsewhere
    # nor hidden memberships at a visible location enter this work budget.
    if connection.execute("SELECT count(*) FROM sqlite_master WHERE name IN "
                          "('spatial_location_rtree','spatial_overlay_time_rtree') AND type='table'").fetchone()[0] != 2:
        return "unavailable", None, "SPATIAL-GEOMETRY-001", "compiled spatial bounds index is unavailable"
    map_key = connection.execute("SELECT rowid FROM spatial_map WHERE id=?", (item["mapId"],)).fetchone()[0]
    visible_locations = connection.execute(
        "SELECT loc.id FROM spatial_location_rtree AS box "
        "CROSS JOIN spatial_location AS loc ON loc.source_ordinal=box.source_ordinal "
        "WHERE box.min_map<=? AND box.max_map>=? AND box.min_x<=? AND box.max_x>=? "
        "AND box.min_y<=? AND box.max_y>=? AND loc.map_id=? AND " + geometry + " LIMIT ?",
        (map_key, map_key, bounds["max"][0], bounds["min"][0], bounds["max"][1], bounds["min"][1],
         item["mapId"], *values, MAX_GEOMETRY_CANDIDATES + 1),
    ).fetchall()
    if len(visible_locations) > MAX_GEOMETRY_CANDIDATES:
        return "limit", None, "SPATIAL-LIMIT-001", "viewport geometry candidate budget exceeded"
    visible_json = json.dumps([row[0] for row in visible_locations])
    columns = "SELECT o.id,o.source_ordinal,loc.id,entity.title,loc.geometry_kind,loc.geometry_json,loc.source_ordinal "
    explicit = " AND o.id=?" if overlay_id is not None else ""
    explicit_params = (overlay_id,) if overlay_id is not None else ()
    static_sql = columns + \
        "FROM json_each(?) AS visible " \
        "CROSS JOIN spatial_overlay_lens_location AS lens INDEXED BY spatial_overlay_static_lens_idx " \
        "ON lens.location_id=visible.value AND lens.audience=? AND lens.perspective=? AND lens.lifecycle='static' " \
        "CROSS JOIN spatial_overlay AS o ON o.id=lens.overlay_id " \
        "CROSS JOIN spatial_location AS loc ON loc.id=visible.value " \
        "JOIN entity ON entity.id=o.id WHERE 1=1" + explicit + " LIMIT ?"
    rows = connection.execute(static_sql, (visible_json, audience, perspective,
                                           *explicit_params, MAX_OVERLAY_CANDIDATES + 1)).fetchall()
    if len(rows) > MAX_OVERLAY_CANDIDATES:
        return "limit", None, "SPATIAL-LIMIT-001", "authorized layer candidate budget exceeded"
    # Each of the four 24-bit StoryTime digits is exactly representable in
    # SQLite's float32 RTree, including signed extreme ticks and orders.
    digits = _story_time_digits(at.tick, at.order)
    temporal_sql = columns + \
        "FROM json_each(?) AS visible " \
        "CROSS JOIN spatial_overlay_scope_key AS scope ON scope.location_id=visible.value " \
        "AND scope.audience=? AND scope.perspective=? AND scope.timeline=? " \
        "CROSS JOIN spatial_overlay_time_rtree AS box ON " \
        "box.min_scope<=scope.id AND box.max_scope>=scope.id " \
        "AND box.min_t3<=? AND box.max_t3>=? AND box.min_t2<=? AND box.max_t2>=? " \
        "AND box.min_t1<=? AND box.max_t1>=? AND box.min_t0<=? AND box.max_t0>=? " \
        "CROSS JOIN spatial_overlay_lens_location AS lens ON lens.rowid=CAST(box.segment_id/16 AS INTEGER) " \
        "CROSS JOIN spatial_overlay AS o ON o.id=lens.overlay_id " \
        "CROSS JOIN spatial_location AS loc ON loc.id=visible.value " \
        "JOIN entity ON entity.id=o.id WHERE lens.location_id=scope.location_id AND lens.audience=? " \
        "AND lens.perspective=? AND lens.timeline=? AND lens.lifecycle!='static' " \
        "AND (lens.start_tick<? OR (lens.start_tick=? AND lens.start_order<=?)) " \
        "AND (lens.end_tick>? OR (lens.end_tick=? AND lens.end_order>=?))" + explicit + " LIMIT ?"
    rows += connection.execute(temporal_sql, (visible_json, audience, perspective, at.timeline,
        *(value for digit in digits for value in (digit, digit)),
        audience, perspective, at.timeline, at.tick, at.tick, at.order,
        at.tick, at.tick, at.order, *explicit_params,
        MAX_OVERLAY_CANDIDATES + 1 - len(rows))).fetchall()
    if len(rows) > MAX_OVERLAY_CANDIDATES:
        return "limit", None, "SPATIAL-LIMIT-001", "authorized layer candidate budget exceeded"
    # Cursor keys include overlay plus member identity, preserving stable
    # pagination across multiple members of one overlay.
    def key(row: Any) -> tuple[int, str]:
        return row[1], f"{row[0]}\x00{row[6]:020d}\x00{row[2]}"
    visible = sorted((row for row in rows if last is None or key(row) > last), key=key)
    selected = visible[:item["limit"]]
    geometries = [_geometry(row[4], row[5]) for row in selected]
    vertices = sum(1 if row[4] == "point" else len(geometry) for row, geometry in zip(selected, geometries))
    if vertices > _MAX_VERTICES:
        return "limit", None, "SPATIAL-LIMIT-001", "layer vertex budget exceeded"
    next_cursor = _cursor(binding, key(selected[-1])) if len(visible) > item["limit"] else None
    layers = [{"overlayId": row[0], "label": row[3], "locationId": row[2],
               "geometry": {"kind": row[4], "coordinates": geometry},
               "basis": "authorized-authored-overlay-membership"} for row, geometry in zip(selected, geometries)]
    return "ok", {"mapId": item["mapId"], "crs": map_row[1], "unit": map_row[4],
                  "asOf": item["asOf"], "audience": audience, "perspective": perspective,
                  "layers": layers, "basis": "authorized-authored-overlay-membership", "nextCursor": next_cursor}, "", None


def execute(repository: Any, operation: str, request: dict[str, Any], *, require_compiled: bool = False) -> dict[str, Any]:
    """Execute one selected, compiled-only explorer projection."""
    item = _common(request, operation)
    for attempt in range(3):
        try:
            world, database = require_database(repository, require_compiled=require_compiled)
        except CompileRequired:
            raise CompileRequired("compiled cache is required; run `wedl compile` before this read",
                                  details={"hint": "Run `wedl compile` for this repository."}) from None
        except (OSError, RepositoryError):
            raise RepositoryError("compiled spatial projection is unavailable for this read") from None
        if item.get("revision") is not None and item["revision"] != world.revision:
            raise UsageError("revision does not match the compiled source revision")
        connection = None
        try:
            connection = connect(database, True)
            metadata = connection.execute("SELECT head_commit,tree_oid,source_schema,compiler_fingerprint FROM revision LIMIT 1").fetchone()
        except (OSError, sqlite3.Error, RepositoryError):
            if connection is not None:
                connection.close()
            raise RepositoryError("compiled spatial projection is unavailable for this read") from None
        if metadata is not None and metadata["head_commit"] == world.revision and metadata["tree_oid"] == world.tree_oid and metadata["source_schema"] == world.schema:
            break
        connection.close()
        if attempt == 2:
            raise UsageError("compiled spatial projection changed during this read; retry the request")
    with closing(connection):
        store = SpatialStore(connection, world.revision)
        if item.get("capabilities") is not None and tuple(item["capabilities"]) != store.capabilities:
            raise UsageError("capabilities do not match the compiled spatial projection")
        if operation != "catalog" and "spatial-core-v1" not in store.capabilities:
            return _outcome(world, metadata, operation, "unavailable", detail="compiled projection lacks spatial-core-v1")
        if operation in {"viewport", "layers"} and "geometry-v1" not in store.capabilities:
            return _outcome(world, metadata, operation, "unavailable", detail="compiled projection lacks geometry-v1")
        if operation == "layers" and "overlay-v1" not in store.capabilities:
            return _outcome(world, metadata, operation, "unavailable", detail="compiled projection lacks overlay-v1")
        resolved = dict(item)
        if operation == "catalog":
            resolved["revision"] = world.revision
            resolved["capabilities"] = list(store.capabilities)
        try:
            state, result, code, detail = {
                "catalog": _catalog, "places": _places, "viewport": _viewport, "layers": _layers,
            }[operation](connection, resolved, world.revision)
        except (ValueError, TypeError, KeyError):
            return _outcome(world, metadata, operation, "unavailable", code="SPATIAL-GEOMETRY-001",
                            detail="compiled spatial projection cannot be represented safely")
        return _outcome(world, metadata, operation, state, result=result, code=code, detail=detail)
