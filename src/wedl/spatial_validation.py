"""Strict, component-only validation for authored ``wedl/v0.7`` spatial data.

This module is deliberately not connected to generic parsing, compilation, or
ChangeSet dispatch.  It validates an explicit candidate and reports stable,
field-addressed diagnostics without inventing any spatial facts.
"""
from __future__ import annotations

from copy import deepcopy
from dataclasses import dataclass
import math
from pathlib import Path
import re
from typing import Any, Iterable

from .ids import valid_id, valid_spatial_id
from .model import Record, StoryTime, World
from .validation import _invalid_location_link_prose_fields, _location_target
from .v07 import CAPABILITY_ORDER, canonical_capabilities

SPATIAL_SCHEMA = "wedl/v0.7"
SPATIAL_CAPABILITY = "spatial-core-v1"
SPATIAL_CAPABILITIES = CAPABILITY_ORDER
_SPATIAL_KINDS = frozenset(("map", "anchor", "portal", "route", "overlay"))
_SLUG = re.compile(r"^[a-z][a-z0-9-]*$")
I64_MIN, I64_MAX = -(2**63), 2**63 - 1
I32_MIN, I32_MAX = -(2**31), 2**31 - 1
_MESSAGES = {
    "GEN-CAPABILITY-001": "spatial capability declaration is invalid",
    "SPATIAL-REQUEST-001": "invalid spatial component envelope",
    "SPATIAL-ID-001": "spatial IDs must be stable opaque ID paths",
    "SPATIAL-REF-001": "spatial reference is missing or has the wrong kind",
    "SPATIAL-MAP-001": "map declaration is invalid",
    "SPATIAL-GEOMETRY-001": "geometry is invalid",
    "SPATIAL-METRIC-001": "route metrics must be typed non-negative finite values",
    "SPATIAL-HIERARCHY-001": "location containment must be acyclic",
    "SPATIAL-ANCHOR-001": "anchor declaration is invalid",
    "SPATIAL-PORTAL-001": "portal declaration is invalid",
    "SPATIAL-ROUTE-001": "route declaration is invalid",
    "SPATIAL-OVERLAY-001": "overlay declaration is invalid",
    "SPATIAL-TIME-001": "overlay time bounds must be ordered on one declared timeline",
    "SPATIAL-LIMIT-001": "spatial component limit exceeded",
}
PRODUCTION_DIAGNOSTICS = frozenset(_MESSAGES.items())


@dataclass(frozen=True, slots=True)
class SpatialId:
    value: str

    @property
    def kind(self) -> str:
        return self.value.partition(":")[0]

    @classmethod
    def parse(cls, value: Any, expected_kind: str | None = None) -> "SpatialId | None":
        return cls(value) if valid_spatial_id(value, expected_kind) else None


def _diag(code: str, record: Record | None, field: str) -> dict[str, Any]:
    data = _front(record) if record is not None else {}
    entity_id = data.get("id") if isinstance(data.get("id"), str) else None
    return {"code": code, "message": _MESSAGES[code], "severity": "error", "entityId": entity_id,
            "path": getattr(record, "source_path", None) if record is not None else None, "field": field}


def _front(record: Any) -> dict[str, Any]:
    value = getattr(record, "frontmatter", None)
    return value if isinstance(value, dict) else {}


def _record_id(record: Record) -> str:
    value = _front(record).get("id")
    return value if isinstance(value, str) else ""


def _record_kind(record: Record) -> str:
    value = _front(record).get("kind")
    return value if isinstance(value, str) else ""


def _closed(value: Any, required: set[str], optional: set[str] = frozenset()) -> bool:
    return isinstance(value, dict) and required <= set(value) and all(isinstance(key, str) and (key in required or key in optional or key.startswith("x-")) for key in value)


def _closed_leaf(value: Any, required: set[str], optional: set[str] = frozenset()) -> str | None:
    """Return a determinable malformed envelope member, not a broad fallback."""
    if not isinstance(value, dict):
        return ""
    missing = sorted(required - set(value))
    if missing:
        return missing[0]
    unknown = sorted(
        (key for key in value if not isinstance(key, str) or (key not in required and key not in optional and not key.startswith("x-"))),
        key=lambda item: str(item),
    )
    return str(unknown[0]) if unknown else None


def _finite(value: Any) -> bool:
    return type(value) in (int, float) and math.isfinite(value)


def _point(value: Any, dimension: int = 2) -> bool:
    return isinstance(value, list) and len(value) == dimension and all(_finite(item) for item in value)


def _policy_point(value: Any, policy: str) -> bool:
    dimensions = {"forbidden": (2,), "optional-level": (2, 3), "required": (3,)}.get(policy, ())
    return isinstance(value, list) and len(value) in dimensions and all(_finite(item) for item in value)


def _metric(value: Any) -> bool:
    return _closed(value, {"value", "unit"}) and _finite(value.get("value")) and value["value"] >= 0 and isinstance(value.get("unit"), str) and bool(value["unit"].strip())


def _ordered_strings(value: Any, *, nonempty: bool = False) -> bool:
    return isinstance(value, list) and (bool(value) or not nonempty) and all(isinstance(item, str) and item for item in value) and value == sorted(set(value))


def _time(value: Any, timelines: set[str]) -> StoryTime | None:
    if not _closed(value, {"timeline", "tick", "order"}) or not isinstance(value.get("timeline"), str) or type(value.get("tick")) is not int or type(value.get("order")) is not int:
        return None
    if value["timeline"] not in timelines or not I64_MIN <= value["tick"] <= I64_MAX or not I32_MIN <= value["order"] <= I32_MAX:
        return None
    try: return StoryTime(value["timeline"], value["tick"], value["order"])
    except (TypeError, ValueError): return None


def _location_id(value: Any) -> bool:
    return valid_id(value, "location") or valid_spatial_id(value, "location")


def _ref(value: Any, kind: str, records: dict[str, Record]) -> bool:
    shape = _location_id(value) if kind == "location" else valid_spatial_id(value, kind)
    return shape and isinstance(value, str) and value in records and records[value].kind == kind


def _geometry(value: Any, dimension: int) -> tuple[bool, str, bool]:
    """Return valid, exact failing suffix, and geometry limit exceeded."""
    if (leaf := _closed_leaf(value, {"kind", "coordinates"})) is not None:
        return False, f".{leaf}" if leaf else "", False
    kind, coords = value.get("kind"), value.get("coordinates")
    if kind not in {"point", "line", "polygon"}: return False, ".kind", False
    if kind == "point": return (_point(coords, dimension), ".coordinates", False)
    if not isinstance(coords, list): return False, ".coordinates", False
    if len(coords) > 10_000: return False, ".coordinates", True
    if not all(_point(item, dimension) for item in coords): return False, ".coordinates", False
    if len(coords) < (2 if kind == "line" else 4): return False, ".coordinates", False
    if kind == "polygon" and coords[0] != coords[-1]: return False, ".coordinates", False
    return True, "", False


def _map_shape(data: dict[str, Any]) -> tuple[bool, str, int]:
    req = {"schema", "kind", "id", "title", "crs", "axis_order", "unit", "bounds"}
    opt = {"origin", "scale", "z_policy", "domain", "status", "tags", "aliases", "threads", "section_audiences", "provenance"}
    if (leaf := _closed_leaf(data, req, opt)) is not None: return False, leaf, 0
    if not isinstance(data.get("title"), str) or not data["title"].strip(): return False, "title", 0
    crs, axes, unit = data.get("crs"), data.get("axis_order"), data.get("unit")
    if not isinstance(crs, str): return False, "crs", 0
    if not isinstance(axes, list) or len(axes) != 2: return False, "axis_order", 0
    if crs.startswith("local-planar:"):
        if not _SLUG.fullmatch(crs.partition(":")[2] or ""): return False, "crs", 0
        if axes != ["east", "north"]: return False, "axis_order", 0
        if not isinstance(unit, str) or not _SLUG.fullmatch(unit): return False, "unit", 0
    elif crs == "EPSG:4326":
        if axes != ["longitude", "latitude"]: return False, "axis_order", 0
        if unit != "degree": return False, "unit", 0
    else: return False, "crs", 0
    policy = data.get("z_policy", "forbidden")
    if policy not in {"forbidden", "optional-level", "required"}: return False, "z_policy", 0
    bounds = data.get("bounds")
    if not _closed(bounds, {"min", "max"}) or not _policy_point(bounds.get("min"), policy) or not _policy_point(bounds.get("max"), policy) or len(bounds["min"]) != len(bounds["max"]): return False, "bounds", 0
    dimension = len(bounds["min"])
    if "scale" in data and not _metric(data["scale"]): return False, "scale", 0
    if "origin" in data:
        origin = data["origin"]
        if not _closed(origin, {"label", "coordinates"}) or not isinstance(origin.get("label"), str) or not origin["label"].strip() or not _point(origin.get("coordinates"), dimension): return False, "origin", 0
    return True, "", dimension


def _in_bounds(point: list[Any], bounds: dict[str, Any]) -> bool:
    return all(low <= value <= high for value, low, high in zip(point, bounds["min"], bounds["max"]))


def _chronology_errors(world: Record) -> list[str]:
    """Validate inherited chronology with the complete v0.6 grammar.

    The temporary world supplies only the non-chronology defaults required by
    the old source envelope.  Calendar/era/anchor data is copied literally,
    so IDs, closed forms, bounds, ordering, and references remain subject to
    the established chronology validator rather than a shallow shape check.
    """
    from .chronology_validation import validate_v06_candidate

    data = deepcopy(_front(world))
    data["schema"] = "wedl/v0.6"
    data.pop("capabilities", None)
    data.setdefault("domain", "spatial")
    data.setdefault("status", "canonical")
    data.setdefault("tags", [])
    data.setdefault("aliases", [])
    data.setdefault("threads", [])
    # Timeline declaration and selected default are checked directly by this
    # component.  Give the inherited chronology pass a declared default so it
    # reaches calendar/era/anchor validation even when that separate envelope
    # error is present.
    timelines = data.get("timelines")
    if isinstance(timelines, list):
        first = next((item.get("id") for item in timelines if isinstance(item, dict) and isinstance(item.get("id"), str) and item["id"].strip()), None)
        if first is not None:
            data["default_timeline"] = first
    clone = Record(data, world.body, world.source_path, world.raw_bytes, world.blob_oid, world.revision)
    candidate = World("spatial-component", "spatial-component", {clone.id: clone}, Path("."))
    return [str(item.get("field") or "chronology") for item in validate_v06_candidate(candidate)]


def _world(world: Record, errors: list[dict[str, Any]]) -> tuple[set[str], list[str]]:
    data = _front(world)
    req = {"schema", "kind", "id", "title", "capabilities", "default_timeline", "timelines"}
    opt = {"chronology", "domain", "status", "tags", "aliases", "threads", "current_time", "section_audiences", "provenance", "state_keys", "relationship_metrics", "embedding_policy", "context_policy", "compilation_policy"}
    envelope_leaf = _closed_leaf(data, req, opt)
    if envelope_leaf is not None: errors.append(_diag("SPATIAL-REQUEST-001", world, envelope_leaf or "schema"))
    elif data.get("schema") != SPATIAL_SCHEMA: errors.append(_diag("SPATIAL-REQUEST-001", world, "schema"))
    elif data.get("kind") != "world": errors.append(_diag("SPATIAL-REQUEST-001", world, "kind"))
    elif not valid_id(data.get("id"), "world"): errors.append(_diag("SPATIAL-REQUEST-001", world, "id"))
    elif not isinstance(data.get("title"), str) or not data["title"].strip(): errors.append(_diag("SPATIAL-REQUEST-001", world, "title"))
    if "chronology" in data:
        for field in _chronology_errors(world): errors.append(_diag("SPATIAL-REQUEST-001", world, field))
    raw, ids = data.get("timelines"), []
    if not isinstance(raw, list) or not raw: errors.append(_diag("SPATIAL-REQUEST-001", world, "timelines"))
    else:
        for index, item in enumerate(raw):
            field = f"timelines[{index}]"
            if not _closed(item, {"id", "label"}, {"origin"}) or not isinstance(item.get("id"), str) or not _SLUG.fullmatch(item["id"]) or not isinstance(item.get("label"), str) or not item["label"].strip(): errors.append(_diag("SPATIAL-REQUEST-001", world, field)); continue
            origin = item.get("origin")
            if origin is not None and (not _closed(origin, {"tick", "label"}) or type(origin.get("tick")) is not int or not I64_MIN <= origin["tick"] <= I64_MAX or not isinstance(origin.get("label"), str) or not origin["label"].strip()): errors.append(_diag("SPATIAL-REQUEST-001", world, f"{field}.origin")); continue
            ids.append(item["id"])
        if len(ids) != len(set(ids)): errors.append(_diag("SPATIAL-REQUEST-001", world, "timelines"))
    if data.get("default_timeline") not in ids: errors.append(_diag("SPATIAL-REQUEST-001", world, "default_timeline"))
    caps = data.get("capabilities")
    if canonical_capabilities(caps) is None: errors.append(_diag("GEN-CAPABILITY-001", world, "capabilities")); return set(ids), []
    return set(ids), list(caps)


def validate_spatial_component(records: Iterable[Record], *, world_record: Record | None = None) -> list[dict[str, Any]]:
    """Validate an explicit v0.7 candidate without enabling generic v0.7 I/O."""
    supplied = [record for record in records if isinstance(record, Record)]
    values = sorted(supplied, key=lambda r: (str(getattr(r, "source_path", "")), _record_id(r)))
    world = world_record or next((record for record in values if _record_kind(record) == "world"), None)
    active = any(_front(record).get("schema") == SPATIAL_SCHEMA or _record_kind(record) in _SPATIAL_KINDS or "spatial" in _front(record) for record in values)
    if not active: return []
    if world is None: return [_diag("SPATIAL-REQUEST-001", None, "world")]
    errors: list[dict[str, Any]] = []
    timelines, caps = _world(world, errors)
    if any(record is not world and _front(record).get("schema") != SPATIAL_SCHEMA for record in values): errors.append(_diag("SPATIAL-REQUEST-001", world, "schema"))
    lookup: dict[str, Record] = {}
    for record in values:
        rid, kind, data = _record_id(record), _record_kind(record), _front(record)
        if not rid or not kind: errors.append(_diag("SPATIAL-REQUEST-001", record, "id")); continue
        if rid in lookup: errors.append(_diag("SPATIAL-ID-001", record, "id")); continue
        lookup[rid] = record
        if record is not world and "capabilities" in data: errors.append(_diag("GEN-CAPABILITY-001", record, "capabilities"))
        if kind in _SPATIAL_KINDS and not valid_spatial_id(rid, kind): errors.append(_diag("SPATIAL-ID-001", record, "id"))
        if kind == "location" and not _location_id(rid): errors.append(_diag("SPATIAL-ID-001", record, "id"))

    def cap(name: str, record: Record, field: str) -> bool:
        if name not in caps: errors.append(_diag("GEN-CAPABILITY-001", record, field)); return False
        return True

    maps = {rid: rec for rid, rec in lookup.items() if _record_kind(rec) == "map" and valid_spatial_id(rid, "map")}
    locations = {rid: rec for rid, rec in lookup.items() if _record_kind(rec) == "location"}
    dimensions: dict[str, int] = {}
    for record in maps.values():
        cap(SPATIAL_CAPABILITY, record, "schema")
        if (leaf := _closed_leaf(_front(record), {"schema", "kind", "id", "title", "crs", "axis_order", "unit", "bounds"}, {"origin", "scale", "z_policy", "domain", "status", "tags", "aliases", "threads", "section_audiences", "provenance"})) is not None:
            errors.append(_diag("SPATIAL-REQUEST-001", record, leaf or "schema")); continue
        ok, leaf, dim = _map_shape(_front(record))
        if not ok: errors.append(_diag("SPATIAL-MAP-001", record, leaf)); continue
        bounds = record.frontmatter["bounds"]
        if not _closed(bounds, {"min", "max"}) or not _point(bounds.get("min"), dim) or not _point(bounds.get("max"), dim) or any(a >= b for a, b in zip(bounds["min"], bounds["max"])): errors.append(_diag("SPATIAL-MAP-001", record, "bounds")); continue
        if record.frontmatter["crs"] == "EPSG:4326" and (bounds["min"][0] < -180 or bounds["max"][0] > 180 or bounds["min"][1] < -90 or bounds["max"][1] > 90): errors.append(_diag("SPATIAL-MAP-001", record, "bounds")); continue
        dimensions[record.id] = dim

    parents: dict[str, str] = {}
    for record in locations.values():
        data, spatial = _front(record), _front(record).get("spatial")
        # Keep the v0.7 component envelope closed while retaining the pinned
        # legacy location classification verbatim.  ``parent`` is normalized
        # to ``parent_id`` during serialization, so both spellings remain
        # permitted without inferring spatial placement.
        optional = {"parent_id", "parent", "links", "spatial", "location_type", "chronology", "domain", "status", "tags", "aliases", "threads", "section_audiences", "provenance"}
        envelope_leaf = _closed_leaf(data, {"schema", "kind", "id", "title"}, optional)
        if envelope_leaf is not None: errors.append(_diag("SPATIAL-REQUEST-001", record, envelope_leaf or "schema"))
        elif data.get("schema") != SPATIAL_SCHEMA: errors.append(_diag("SPATIAL-REQUEST-001", record, "schema"))
        elif data.get("kind") != "location": errors.append(_diag("SPATIAL-REQUEST-001", record, "kind"))
        elif not isinstance(data.get("title"), str) or not data["title"].strip(): errors.append(_diag("SPATIAL-REQUEST-001", record, "title"))
        if spatial is not None:
            cap(SPATIAL_CAPABILITY, record, "spatial")
            if (leaf := _closed_leaf(spatial, {"map_id", "geometry"})) is not None:
                errors.append(_diag("SPATIAL-REQUEST-001", record, f"spatial.{leaf}" if leaf else "spatial"))
            elif not _ref(spatial.get("map_id"), "map", maps): errors.append(_diag("SPATIAL-REF-001", record, "spatial.map_id"))
            else:
                dim = dimensions.get(spatial["map_id"])
                if cap("geometry-v1", record, "spatial.geometry") and dim:
                    valid, leaf, limited = _geometry(spatial.get("geometry"), dim)
                    if limited: errors.append(_diag("SPATIAL-LIMIT-001", record, "spatial.geometry.coordinates"))
                    elif not valid: errors.append(_diag("SPATIAL-GEOMETRY-001", record, "spatial.geometry" + leaf))
                    else:
                        coords = spatial["geometry"]["coordinates"]; points = [coords] if spatial["geometry"]["kind"] == "point" else coords
                        if any(not _in_bounds(point, maps[spatial["map_id"]].frontmatter["bounds"]) for point in points): errors.append(_diag("SPATIAL-GEOMETRY-001", record, "spatial.geometry.coordinates"))
        if "parent_id" in data and "parent" in data:
            errors.append(_diag("SPATIAL-REQUEST-001", record, "parent_id"))
        elif "parent_id" in data or "parent" in data:
            parent = data.get("parent_id", data.get("parent"))
            if parent is not None:
                if not _ref(parent, "location", locations) or parent == record.id: errors.append(_diag("SPATIAL-REF-001", record, "parent_id"))
                else: parents[record.id] = parent
        links = data.get("links", [])
        if not isinstance(links, list): errors.append(_diag("SPATIAL-REF-001", record, "links"))
        else:
            seen: set[str] = set()
            for index, link in enumerate(links):
                field, target = f"links[{index}]", _location_target(link)
                if target is None or not _ref(target, "location", locations) or target == record.id or target in seen: errors.append(_diag("SPATIAL-REF-001", record, field))
                if target is not None: seen.add(target)
                for prose_field in _invalid_location_link_prose_fields(link):
                    errors.append(_diag("SPATIAL-REF-001", record, f"{field}.{prose_field}"))
    for start in sorted(parents):
        seen: set[str] = set(); current = start
        while current in parents:
            if current in seen: errors.append(_diag("SPATIAL-HIERARCHY-001", locations[start], "parent_id")); break
            seen.add(current); current = parents[current]

    edges: set[tuple[str, str, tuple[str, ...]]] = set()
    def endpoint(value: Any) -> bool:
        return _closed(value, {"map_id", "coordinates"}) and _ref(value.get("map_id"), "map", maps) and value.get("map_id") in dimensions and _point(value.get("coordinates"), dimensions[value["map_id"]]) and _in_bounds(value["coordinates"], maps[value["map_id"]].frontmatter["bounds"])
    for record in values:
        data, kind = _front(record), _record_kind(record)
        if kind == "anchor":
            cap(SPATIAL_CAPABILITY, record, "schema"); cap("geometry-v1", record, "schema")
            if (leaf := _closed_leaf(data, {"schema", "kind", "id", "title", "from", "to"}, {"conversion", "domain", "status", "tags", "aliases", "threads", "provenance"})) is not None: errors.append(_diag("SPATIAL-REQUEST-001", record, leaf or "schema")); continue
            if not endpoint(data.get("from")): errors.append(_diag("SPATIAL-ANCHOR-001", record, "from"))
            if not endpoint(data.get("to")): errors.append(_diag("SPATIAL-ANCHOR-001", record, "to"))
            elif endpoint(data.get("from")) and data["from"]["map_id"] == data["to"]["map_id"]: errors.append(_diag("SPATIAL-ANCHOR-001", record, "to.map_id"))
            if "conversion" in data and not isinstance(data["conversion"], str): errors.append(_diag("SPATIAL-ANCHOR-001", record, "conversion"))
        elif kind == "portal":
            cap(SPATIAL_CAPABILITY, record, "schema"); cap("route-v1", record, "schema")
            if (leaf := _closed_leaf(data, {"schema", "kind", "id", "title", "from_location_id", "to", "modes"}, {"domain", "status", "tags", "aliases", "threads", "provenance"})) is not None: errors.append(_diag("SPATIAL-REQUEST-001", record, leaf or "schema")); continue
            target = data.get("to")
            if not _ref(data.get("from_location_id"), "location", locations): errors.append(_diag("SPATIAL-PORTAL-001", record, "from_location_id"))
            elif not (_ref(target, "location", locations) or endpoint(target)): errors.append(_diag("SPATIAL-PORTAL-001", record, "to"))
            elif target == data.get("from_location_id"): errors.append(_diag("SPATIAL-PORTAL-001", record, "to"))
            elif not _ordered_strings(data.get("modes"), nonempty=True): errors.append(_diag("SPATIAL-PORTAL-001", record, "modes"))
        elif kind == "route":
            cap(SPATIAL_CAPABILITY, record, "schema"); cap("route-v1", record, "schema")
            opt = {"route_distance", "travel_cost", "duration", "availability", "uncertainty", "domain", "status", "tags", "aliases", "threads", "provenance"}
            if (leaf := _closed_leaf(data, {"schema", "kind", "id", "title", "from_location_id", "to_location_id", "direction", "modes"}, opt)) is not None: errors.append(_diag("SPATIAL-REQUEST-001", record, leaf or "schema")); continue
            if not _ref(data.get("from_location_id"), "location", locations): errors.append(_diag("SPATIAL-ROUTE-001", record, "from_location_id"))
            elif not _ref(data.get("to_location_id"), "location", locations): errors.append(_diag("SPATIAL-ROUTE-001", record, "to_location_id"))
            elif data["from_location_id"] == data["to_location_id"]: errors.append(_diag("SPATIAL-ROUTE-001", record, "to_location_id"))
            elif data.get("direction") not in {"one-way", "two-way"}: errors.append(_diag("SPATIAL-ROUTE-001", record, "direction"))
            elif not _ordered_strings(data.get("modes"), nonempty=True): errors.append(_diag("SPATIAL-ROUTE-001", record, "modes"))
            for field in ("route_distance", "travel_cost", "duration"):
                if field in data and not _metric(data[field]): errors.append(_diag("SPATIAL-METRIC-001", record, field))
            if data.get("availability", "open") not in {"open", "closed", "restricted", "unknown"}: errors.append(_diag("SPATIAL-ROUTE-001", record, "availability"))
            if data.get("uncertainty", "exact") not in {"exact", "estimated", "unknown"}: errors.append(_diag("SPATIAL-ROUTE-001", record, "uncertainty"))
            modes = data.get("modes")
            if isinstance(modes, list) and all(isinstance(item, str) for item in modes):
                edge = (str(data.get("from_location_id")), str(data.get("to_location_id")), tuple(modes))
                if edge in edges: errors.append(_diag("SPATIAL-ROUTE-001", record, "from_location_id"))
                edges.add(edge)
        elif kind == "overlay":
            cap(SPATIAL_CAPABILITY, record, "schema"); cap("overlay-v1", record, "schema")
            opt = {"valid", "domain", "status", "tags", "aliases", "threads", "provenance", "faction", "institution", "calendar"}
            if (leaf := _closed_leaf(data, {"schema", "kind", "id", "title", "lifecycle", "membership", "audience", "perspectives"}, opt)) is not None: errors.append(_diag("SPATIAL-REQUEST-001", record, leaf or "schema")); continue
            membership = data.get("membership")
            member_ok = _closed(membership, {"location_ids"}) and _ordered_strings(membership.get("location_ids")) and all(_ref(item, "location", locations) for item in membership["location_ids"])
            if data.get("lifecycle") not in {"static", "time-bounded"}: errors.append(_diag("SPATIAL-OVERLAY-001", record, "lifecycle"))
            if not member_ok: errors.append(_diag("SPATIAL-OVERLAY-001", record, "membership.location_ids"))
            if not _ordered_strings(data.get("audience")): errors.append(_diag("SPATIAL-OVERLAY-001", record, "audience"))
            if not _ordered_strings(data.get("perspectives")): errors.append(_diag("SPATIAL-OVERLAY-001", record, "perspectives"))
            for forbidden in ("faction", "institution", "calendar"):
                if forbidden in data: errors.append(_diag("SPATIAL-REQUEST-001", record, forbidden))
            if data.get("lifecycle") == "static" and "valid" in data: errors.append(_diag("SPATIAL-OVERLAY-001", record, "valid"))
            if data.get("lifecycle") == "time-bounded":
                valid = data.get("valid"); start = _time(valid.get("start"), timelines) if isinstance(valid, dict) else None; end = _time(valid.get("end"), timelines) if isinstance(valid, dict) else None
                if start is None: errors.append(_diag("SPATIAL-TIME-001", record, "valid.start"))
                elif end is None: errors.append(_diag("SPATIAL-TIME-001", record, "valid.end"))
                elif start.timeline != end.timeline or (start.tick, start.order) > (end.tick, end.order): errors.append(_diag("SPATIAL-TIME-001", record, "valid"))
    return sorted(errors, key=lambda item: (str(item["path"]), str(item["entityId"]), str(item["field"]), item["code"]))


def validate_spatial_world(world: World) -> list[dict[str, Any]]:
    return validate_spatial_component(world.records.values(), world_record=world.world_record)


def validate_v07_spatial_candidate(records: Iterable[Record], *, world_record: Record | None = None) -> list[dict[str, Any]]:
    return validate_spatial_component(records, world_record=world_record)
