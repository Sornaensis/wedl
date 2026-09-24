"""Public ``wedl-spatial/v1`` codec over the compiled spatial projection.

This boundary deliberately owns only wire validation and serialization.  The
spatial store remains the authority for hierarchy, geometry, route, overlay,
cursor, and availability semantics.
"""
from __future__ import annotations

from contextlib import closing
from dataclasses import fields, is_dataclass, replace
import math
import re
from typing import Any

from .compiler import connect, require_database
from .errors import CompileRequired, UsageError
from .model import StoryTime
from .spatial_query import BoundingBox, MapPosition, SpatialOutcome, SpatialOutcomeKind, SpatialReason, SpatialStore
from .v07 import CAPABILITY_ORDER

PROTOCOL = "wedl-spatial/v1"
_DECIMAL = re.compile(r"^-?(0|[1-9][0-9]*)$")
_OPERATIONS = frozenset(("containment", "children", "bbox", "nearby", "adjacency", "reachability", "path", "overlay-as-of"))
_JSON_SAFE_INTEGER = 2**53 - 1
_REVISION = re.compile(r"^[0-9a-f]{40}$")


def _closed(value: Any, allowed: set[str], required: set[str], where: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value).difference(allowed) or not required.issubset(value):
        raise UsageError(f"{where} is malformed")
    return value


def _text(value: Any, field: str, *, nullable: bool = False) -> str | None:
    if value is None and nullable:
        return None
    if not isinstance(value, str) or not value.strip() or len(value) > 256:
        raise UsageError(f"{field} must be a nonblank string of at most 256 characters")
    return value


def _number(value: Any, field: str) -> int | float:
    if type(value) is int:
        if abs(value) > _JSON_SAFE_INTEGER:
            raise UsageError(f"{field} exceeds the JSON-safe numeric range")
        return value
    if type(value) is not float or not math.isfinite(value):
        raise UsageError(f"{field} must be a finite JSON number")
    # SQLite accepts wider signed integers than a JSON client can reproduce
    # exactly.  Keep public coordinate and metric operands inside JSON's exact
    # integer range before they reach the compiled projection.
    if abs(value) > _JSON_SAFE_INTEGER:
        raise UsageError(f"{field} exceeds the JSON-safe numeric range")
    return value


def _coordinates(value: Any, field: str) -> tuple[int | float, ...]:
    if not isinstance(value, list) or len(value) not in (2, 3):
        raise UsageError(f"{field} must be a 2D or 3D coordinate")
    return tuple(_number(item, field) for item in value)


def _modes(value: Any) -> list[str] | None:
    if value is None:
        return None
    if not isinstance(value, list) or not value or len(value) > 100 or any(not isinstance(mode, str) or not mode.strip() or len(mode) > 256 for mode in value) or len(set(value)) != len(value):
        raise UsageError("modes must be null or at most 100 unique nonblank mode IDs")
    return value


def _story_time(value: Any) -> StoryTime:
    item = _closed(value, {"timeline", "tick", "order"}, {"timeline", "tick", "order"}, "asOf")
    timeline = _text(item["timeline"], "asOf.timeline")
    def integer(raw: Any, field: str) -> int:
        if not isinstance(raw, str) or raw == "-0" or not _DECIMAL.fullmatch(raw):
            raise UsageError(f"{field} must be a canonical signed decimal string")
        return int(raw)
    try:
        return StoryTime(str(timeline), integer(item["tick"], "asOf.tick"), integer(item["order"], "asOf.order"))
    except ValueError as exc:
        raise UsageError("asOf is outside StoryTime bounds") from exc


def _request(request: Any, operation: str) -> dict[str, Any]:
    if operation not in _OPERATIONS:
        raise UsageError("unsupported spatial operation")
    item = _closed(request, {
        "protocol", "revision", "capabilities", "limit", "cursor", "locationId", "mapId", "bounds", "relation",
        "position", "radius", "modes", "fromLocationId", "toLocationId", "metric", "unit", "queryScope",
        "overlayId", "audience", "perspective", "asOf",
    }, {"protocol", "revision", "capabilities", "limit", "cursor"}, f"{operation} request")
    if item["protocol"] != PROTOCOL:
        raise UsageError("unsupported spatial protocol")
    if not isinstance(item["revision"], str) or not _REVISION.fullmatch(item["revision"]):
        raise UsageError("revision must be a 40-character lowercase Git SHA")
    caps = item["capabilities"]
    if not isinstance(caps, list) or any(not isinstance(value, str) for value in caps) or len(set(caps)) != len(caps) or caps != sorted(caps, key=lambda value: CAPABILITY_ORDER.index(value) if value in CAPABILITY_ORDER else len(CAPABILITY_ORDER)) or any(value not in CAPABILITY_ORDER for value in caps):
        raise UsageError("capabilities must be canonical, unique declared capability IDs")
    if type(item["limit"]) is not int or not 1 <= item["limit"] <= 100:
        raise UsageError("limit must be an integer from 1 to 100")
    if item["cursor"] is not None and (not isinstance(item["cursor"], str) or not item["cursor"] or len(item["cursor"]) > 2048):
        raise UsageError("cursor must be null or an opaque cursor")
    common = {"protocol", "revision", "capabilities", "limit", "cursor"}
    if operation in {"containment", "children"}:
        _closed(item, common | {"locationId"}, common | {"locationId"}, f"{operation} request")
        _text(item["locationId"], "locationId")
        if operation == "containment" and item["cursor"] is not None:
            raise UsageError("containment cursor must be null")
    elif operation == "bbox":
        _closed(item, common | {"mapId", "bounds", "relation"}, common | {"mapId", "bounds", "relation"}, "bbox request")
        _text(item["mapId"], "mapId")
        bounds = _closed(item["bounds"], {"min", "max"}, {"min", "max"}, "bounds")
        minimum, maximum = _coordinates(bounds["min"], "bounds.min"), _coordinates(bounds["max"], "bounds.max")
        if len(minimum) != len(maximum) or any(left > right for left, right in zip(minimum, maximum)):
            raise UsageError("bounds must be ordered coordinates of matching dimensionality")
        if not isinstance(item["relation"], str) or item["relation"] not in {"intersects", "within"}:
            raise UsageError("relation must be intersects or within")
    elif operation == "nearby":
        _closed(item, common | {"position", "radius"}, common | {"position", "radius"}, "nearby request")
        position = _closed(item["position"], {"mapId", "coordinates"}, {"mapId", "coordinates"}, "position")
        _text(position["mapId"], "position.mapId")
        _coordinates(position["coordinates"], "position.coordinates")
        if _number(item["radius"], "radius") < 0:
            raise UsageError("radius must be non-negative")
    elif operation in {"adjacency", "reachability", "path"}:
        key = "locationId" if operation == "adjacency" else "fromLocationId"
        fields = {key, "modes"} if operation != "path" else {"fromLocationId", "toLocationId", "metric", "unit", "modes"}
        required = {key} if operation != "path" else {"fromLocationId", "toLocationId", "metric"}
        _closed(item, common | fields, common | required, f"{operation} request")
        if item["cursor"] is not None:
            raise UsageError(f"{operation} cursor must be null")
        _text(item[key], key)
        _modes(item.get("modes"))
        if operation == "path":
            _text(item["toLocationId"], "toLocationId")
            if not isinstance(item["metric"], str) or item["metric"] not in {"routeDistance", "travelCost", "duration"}:
                raise UsageError("metric is invalid")
            _text(item.get("unit"), "unit", nullable=True)
    else:
        _closed(item, common | {"queryScope", "locationId", "overlayId", "audience", "perspective", "asOf"}, common | {"queryScope", "locationId", "audience", "perspective", "asOf"}, "overlay-as-of request")
        if not isinstance(item["queryScope"], str) or item["queryScope"] not in {"location", "overlay"}:
            raise UsageError("queryScope is invalid")
        for key in ("locationId", "audience", "perspective"):
            _text(item[key], key)
        if item["queryScope"] == "overlay":
            _text(item.get("overlayId"), "overlayId")
        elif "overlayId" in item:
            raise UsageError("location queryScope forbids overlayId")
        _story_time(item["asOf"])
    return item


def _encode(value: Any) -> Any:
    if isinstance(value, StoryTime):
        return {"timeline": value.timeline, "tick": str(value.tick), "order": str(value.order)}
    if is_dataclass(value):
        return {_camel(field.name): _encode(getattr(value, field.name)) for field in fields(value)}
    if isinstance(value, tuple): return [_encode(item) for item in value]
    if isinstance(value, list): return [_encode(item) for item in value]
    if isinstance(value, dict): return {_camel(str(key)): _encode(item) for key, item in value.items()}
    if type(value) is int and abs(value) > _JSON_SAFE_INTEGER:
        # A compiled projection must not leak a value a JSON client cannot
        # round-trip. StoryTime is handled above as a decimal string.
        raise RuntimeError("spatial projection produced an unsafe JSON integer")
    if type(value) is float and (not math.isfinite(value) or abs(value) > _JSON_SAFE_INTEGER):
        raise RuntimeError("spatial projection produced an unsafe JSON number")
    return value


def _camel(value: str) -> str:
    head, *tail = value.split("_")
    return head + "".join(item.capitalize() for item in tail)


def _cache(world: Any, metadata: Any) -> dict[str, Any]:
    """Return the cache facts from the *open* projection, not a new HEAD read.

    A compile atomically replaces ``world.sqlite``.  Re-reading readiness here
    could therefore report cache B after this request already opened cache A.
    Metadata read from the same SQLite connection keeps the outcome citation
    coherent with its rows and also has no filesystem-path fields to redact.
    """
    return {
        "state": "ready",
        "revision": str(metadata["head_commit"]),
        "treeOid": str(metadata["tree_oid"]),
        "sourceSchema": str(metadata["source_schema"]),
        "fingerprint": str(metadata["compiler_fingerprint"]),
    }


def _outcome(world: Any, metadata: Any, operation: str, outcome: SpatialOutcome, *, request: dict[str, Any] | None = None) -> dict[str, Any]:
    response: dict[str, Any] = {
        "protocol": PROTOCOL, "operation": operation, "revision": outcome.revision,
        "sourceSchema": world.schema, "capabilities": list(getattr(world.world_record, "frontmatter", {}).get("capabilities") or ()),
        "cache": _cache(world, metadata), "state": outcome.kind.value,
    }
    if outcome.kind is SpatialOutcomeKind.OK:
        response["result"] = _result(operation, outcome.value)
    else:
        response["code"] = outcome.reason.value if outcome.reason is not None else "SPATIAL-REQUEST-001"
        if outcome.subreason is not None: response["subreason"] = outcome.subreason.value
        if outcome.detail is not None: response["detail"] = outcome.detail
    return response


def _result(operation: str, value: Any) -> dict[str, Any]:
    """Encode each store result into its public, operation-specific shape."""

    if operation == "overlay-as-of":
        try:
            horizon = value.filters["horizon"]
            at = StoryTime(horizon["timeline"], horizon["tick"], horizon["order"])
            value = replace(value, filters={**value.filters, "horizon": at})
        except (AttributeError, KeyError, TypeError, ValueError) as exc:
            raise RuntimeError("spatial overlay result has an invalid StoryTime horizon") from exc
    result = _encode(value)
    if not isinstance(result, dict):
        raise RuntimeError("spatial store returned a non-object result")
    if operation in {"children", "bbox", "nearby", "overlay-as-of"}:
        result["nextCursor"] = result.pop("cursor", None)
    if operation == "path":
        metric = result.get("metric")
        if not isinstance(metric, dict):
            raise RuntimeError("spatial path result is missing its metric")
        metric["metric"] = {
            "routeDistance": "routeDistance", "travelCost": "travelCost", "duration": "duration",
            "route_distance": "routeDistance", "travel_cost": "travelCost",
        }.get(str(metric.get("metric")), metric.get("metric"))
        metric["computedTotal"] = metric.pop("value", None)
    return result


def execute(repository: Any, operation: str, request: dict[str, Any], *, require_compiled: bool = False) -> dict[str, Any]:
    """Validate a raw public request and invoke exactly one compiled read."""
    item = _request(request, operation)
    # ``require_database`` selects a compiled world and a replaceable cache
    # pathname.  Bind the following read to one opened SQLite file, verify its
    # revision metadata against that world, and retry a bounded number of times
    # if a concurrent compilation swapped the pathname between those steps.
    # This avoids a broad lock while never returning B rows labeled as A.
    for attempt in range(3):
        try:
            world, database = require_database(repository, require_compiled=require_compiled)
        except CompileRequired as exc:
            # CompileRequired is an ordinary WEDL error.  Its standard details
            # include raw cache readiness (and thus local paths), which are not
            # safe at the spatial HTTP/CLI boundary.
            raise CompileRequired("compiled cache is required; run `wedl compile` before this read", details={"hint": "Run `wedl compile` for this repository, or omit --require-compiled to allow an automatic rebuild."}) from None
        if item["revision"] != world.revision:
            raise UsageError("revision does not match the compiled source revision")
        connection = connect(database, True)
        metadata = connection.execute("SELECT head_commit, tree_oid, source_schema, compiler_fingerprint FROM revision LIMIT 1").fetchone()
        if (metadata is None or str(metadata["head_commit"]) != world.revision
                or str(metadata["tree_oid"]) != world.tree_oid
                or str(metadata["source_schema"]) != world.schema):
            connection.close()
            if attempt < 2:
                continue
            raise UsageError("compiled spatial projection changed during this read; retry the request")
        break
    else:  # pragma: no cover - the retry loop either breaks or raises
        raise UsageError("compiled spatial projection changed during this read; retry the request")
    with closing(connection):
        store = SpatialStore(connection, world.revision)
        if tuple(item["capabilities"]) != store.capabilities:
            raise UsageError("capabilities do not match the compiled spatial projection")
        limit, cursor = item["limit"], item["cursor"]
        if operation == "containment":
            if cursor is not None: raise UsageError("containment cursor must be null")
            _closed(item, {"protocol", "revision", "capabilities", "limit", "cursor", "locationId"}, {"protocol", "revision", "capabilities", "limit", "cursor", "locationId"}, "containment request")
            result = store.containment(str(_text(item["locationId"], "locationId")))
        elif operation == "children":
            _closed(item, {"protocol", "revision", "capabilities", "limit", "cursor", "locationId"}, {"protocol", "revision", "capabilities", "limit", "cursor", "locationId"}, "children request")
            result = store.children(str(_text(item["locationId"], "locationId")), limit=limit, cursor=cursor)
        elif operation == "bbox":
            _closed(item, {"protocol", "revision", "capabilities", "limit", "cursor", "mapId", "bounds", "relation"}, {"protocol", "revision", "capabilities", "limit", "cursor", "mapId", "bounds", "relation"}, "bbox request")
            bounds = _closed(item["bounds"], {"min", "max"}, {"min", "max"}, "bounds")
            minimum, maximum = _coordinates(bounds["min"], "bounds.min"), _coordinates(bounds["max"], "bounds.max")
            if len(minimum) != len(maximum) or any(left > right for left, right in zip(minimum, maximum)):
                raise UsageError("bounds must be ordered coordinates of matching dimensionality")
            relation = item["relation"]
            if not isinstance(relation, str) or relation not in {"intersects", "within"}: raise UsageError("relation must be intersects or within")
            result = store.bbox(str(_text(item["mapId"], "mapId")), BoundingBox(minimum, maximum), relation=relation, limit=limit, cursor=cursor)
        elif operation == "nearby":
            _closed(item, {"protocol", "revision", "capabilities", "limit", "cursor", "position", "radius"}, {"protocol", "revision", "capabilities", "limit", "cursor", "position", "radius"}, "nearby request")
            pos = _closed(item["position"], {"mapId", "coordinates"}, {"mapId", "coordinates"}, "position")
            radius = _number(item["radius"], "radius")
            if radius < 0: raise UsageError("radius must be non-negative")
            result = store.nearby(MapPosition(str(_text(pos["mapId"], "position.mapId")), _coordinates(pos["coordinates"], "position.coordinates")), radius=radius, limit=limit, cursor=cursor)
        elif operation in {"adjacency", "reachability"}:
            allowed = {"protocol", "revision", "capabilities", "limit", "cursor", "locationId", "fromLocationId", "modes"}
            required = {"protocol", "revision", "capabilities", "limit", "cursor", "locationId" if operation == "adjacency" else "fromLocationId"}
            _closed(item, allowed, required, f"{operation} request")
            if cursor is not None: raise UsageError(f"{operation} cursor must be null")
            modes = _modes(item.get("modes"))
            result = (store.adjacency(str(_text(item["locationId"], "locationId")), modes=modes, limit=limit) if operation == "adjacency" else store.reachability(str(_text(item["fromLocationId"], "fromLocationId")), modes=modes, limit=limit))
        elif operation == "path":
            _closed(item, {"protocol", "revision", "capabilities", "limit", "cursor", "fromLocationId", "toLocationId", "metric", "unit", "modes"}, {"protocol", "revision", "capabilities", "limit", "cursor", "fromLocationId", "toLocationId", "metric"}, "path request")
            if cursor is not None: raise UsageError("path cursor must be null")
            metric = item["metric"]
            if not isinstance(metric, str) or metric not in {"routeDistance", "travelCost", "duration"}: raise UsageError("metric is invalid")
            modes = _modes(item.get("modes"))
            result = store.path(str(_text(item["fromLocationId"], "fromLocationId")), str(_text(item["toLocationId"], "toLocationId")), metric={"routeDistance": "route_distance", "travelCost": "travel_cost", "duration": "duration"}[metric], unit=_text(item.get("unit"), "unit", nullable=True), modes=modes, limit=limit)
        else:
            _closed(item, {"protocol", "revision", "capabilities", "limit", "cursor", "queryScope", "locationId", "overlayId", "audience", "perspective", "asOf"}, {"protocol", "revision", "capabilities", "limit", "cursor", "queryScope", "locationId", "audience", "perspective", "asOf"}, "overlay-as-of request")
            if not isinstance(item["queryScope"], str) or item["queryScope"] not in {"location", "overlay"}: raise UsageError("queryScope is invalid")
            overlay = _text(item.get("overlayId"), "overlayId", nullable=True)
            if item["queryScope"] == "overlay" and overlay is None: raise UsageError("overlay queryScope requires overlayId")
            if item["queryScope"] == "location" and "overlayId" in item:
                raise UsageError("location queryScope forbids overlayId")
            result = store.overlay_as_of(str(_text(item["locationId"], "locationId")), _story_time(item["asOf"]), audience=str(_text(item["audience"], "audience")), perspective=str(_text(item["perspective"], "perspective")), overlay_id=overlay, limit=limit, cursor=cursor)
    try:
        return _outcome(world, metadata, operation, result, request=item)
    except (RuntimeError, TypeError, ValueError):
        # A compiled value must never turn a protocol response into a Python
        # serialization failure.  This deliberately exposes no raw value or
        # implementation detail: clients get the same closed non-ok contract
        # used when an authored metric cannot be safely supplied.
        unsafe = SpatialOutcome(
            SpatialOutcomeKind.UNAVAILABLE,
            world.revision,
            reason=SpatialReason.METRIC,
            detail="compiled spatial result cannot be represented safely",
        )
        return _outcome(world, metadata, operation, unsafe, request=item)


def status_code(value: dict[str, Any]) -> int:
    return {"ok": 200, "invalid": 400, "unavailable": 409, "forbidden": 403, "limit": 422}[value["state"]]
