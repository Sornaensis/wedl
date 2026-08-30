"""Public ``wedl-chronology/v1`` codec and read boundary.

The arithmetic and SQLite index remain internal.  This module is the small
translation layer that makes chronology safe to expose without leaking kernel
identifiers or Python integers through CLI/HTTP JSON.
"""
from __future__ import annotations

import json
import re
from copy import deepcopy
from contextlib import closing
from typing import Any

from . import CHRONOLOGY_SOURCE_SCHEMA
from .chronology import ApproximateDate, CivilDate, CivilRange, ConflictingDates, EraDate
from .chronology_index import load_chronology_projection
from .chronology_query import AnnotationQuery, ConversionRequest, ChronologyOutcome, EraMatchMode
from .errors import UsageError
from .model import ORDER_MAX, ORDER_MIN, TICK_MAX, TICK_MIN

PROTOCOL = "wedl-chronology/v1"
_DECIMAL = re.compile(r"^-?(0|[1-9][0-9]*)$")
_I64_MIN, _I64_MAX = -(2 ** 63), 2 ** 63 - 1
_MAX_CONFLICT_DEPTH = 64


def _closed(value: Any, keys: set[str], where: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != keys:
        raise UsageError(f"{where} must contain exactly {', '.join(sorted(keys))}")
    return value


def _nonblank_identifier(value: Any, field: str) -> str:
    """Accept an identifier only when it has visible, non-whitespace text."""

    if not isinstance(value, str) or not value.strip():
        raise UsageError(f"{field} must be a nonblank string")
    return value


def _construct(factory: Any, field: str, *args: Any) -> Any:
    """Translate value-object invariants into the public usage vocabulary."""

    try:
        return factory(*args)
    except (AssertionError, OverflowError, TypeError, ValueError) as exc:
        raise UsageError(f"{field} is malformed") from exc


def _decimal(value: Any, field: str, *, minimum: int = _I64_MIN, maximum: int = _I64_MAX) -> int:
    if not isinstance(value, str) or value == "-0" or not _DECIMAL.fullmatch(value):
        raise UsageError(f"{field} must be a canonical decimal string")
    result = int(value)
    if not minimum <= result <= maximum:
        raise UsageError(f"{field} is outside its signed integer range")
    return result


def _encode_decimal(value: int | None) -> str | None:
    return None if value is None else str(value)


def _endpoint(value: Any, calendar_id: str, where: str) -> CivilDate | None:
    if value is None:
        return None
    if not isinstance(value, dict):
        raise UsageError(f"{where} must be an object or null")
    allowed = {"calendarId", "year", "month", "day"}
    if set(value).difference(allowed) or "year" not in value:
        raise UsageError(f"{where} is not a closed civil endpoint")
    identifier = value.get("calendarId", calendar_id)
    if identifier != calendar_id:
        raise UsageError(f"{where}.calendarId must match the range calendar")
    _nonblank_identifier(identifier, f"{where}.calendarId")
    month = None if "month" not in value else _decimal(value["month"], f"{where}.month")
    day = None if "day" not in value else _decimal(value["day"], f"{where}.day")
    if day is not None and month is None:
        raise UsageError(f"{where}.day requires month")
    return _construct(CivilDate, where, identifier, _decimal(value["year"], f"{where}.year"), month, day)


def _tag_extensions(value: dict[str, Any], where: str) -> None:
    """Validate the optional opaque source-tag extension channel.

    Public source-shaped values have two distinct extension locations: payload
    extensions remain beside the public value members, while extensions beside
    a source tag are carried in ``tagExtensions``.  This boundary intentionally
    never interprets a value inside either location.
    """

    if "tagExtensions" not in value:
        return
    extensions = value["tagExtensions"]
    if not isinstance(extensions, dict) or any(
        not isinstance(key, str) or not key.startswith("x-")
        for key in extensions
    ):
        raise UsageError(f"{where}.tagExtensions must contain only x-* members")


def decode_date_value(value: Any, *, allow_extended: bool = True, _depth: int = 0) -> Any:
    """Decode the closed public date union into the internal typed values."""
    if not isinstance(value, dict) or not isinstance(value.get("kind"), str):
        raise UsageError("chronology value must be a closed date object")
    kind = value["kind"]
    if kind in {"civil", "era"}:
        identifier = "calendarId" if kind == "civil" else "eraId"
        allowed = {"kind", identifier, "year", "month", "day", "tagExtensions"}
        if set(value).difference(allowed) or identifier not in value or "year" not in value:
            raise UsageError(f"{kind} date is malformed")
        _tag_extensions(value, kind)
        item_id = _nonblank_identifier(value[identifier], identifier)
        month = None if "month" not in value else _decimal(value["month"], "month")
        day = None if "day" not in value else _decimal(value["day"], "day")
        if day is not None and month is None:
            raise UsageError("day requires month")
        cls = CivilDate if kind == "civil" else EraDate
        return _construct(cls, kind, item_id, _decimal(value["year"], "year"), month, day)
    if kind == "range":
        if (not isinstance(value, dict) or not {"kind", "calendarId", "lower", "upper"}.issubset(value)
                or set(value).difference({"kind", "calendarId", "lower", "upper", "tagExtensions"})):
            raise UsageError("range is malformed")
        _tag_extensions(value, "range")
        calendar_id = _nonblank_identifier(value["calendarId"], "range.calendarId")
        return _construct(CivilRange, "range", calendar_id, _endpoint(value["lower"], calendar_id, "range.lower"), _endpoint(value["upper"], calendar_id, "range.upper"))
    if kind == "approximate":
        if (not isinstance(value, dict) or not {"kind", "displayValue", "bounds"}.issubset(value)
                or set(value).difference({"kind", "displayValue", "bounds", "tagExtensions"})):
            raise UsageError("approximate date is malformed")
        _tag_extensions(value, "approximate")
        if not isinstance(value["displayValue"], str) or not isinstance(value["bounds"], dict):
            raise UsageError("approximate date is malformed")
        bounds = value["bounds"]
        _closed(bounds, {"calendarId", "lower", "upper"}, "approximate.bounds")
        calendar = _nonblank_identifier(bounds["calendarId"], "approximate.bounds.calendarId")
        if not isinstance(value["displayValue"], str):
            raise UsageError("approximate.displayValue must be a string")
        bounds_value = _construct(CivilRange, "approximate.bounds", calendar, _endpoint(bounds["lower"], calendar, "approximate.bounds.lower"), _endpoint(bounds["upper"], calendar, "approximate.bounds.upper"))
        return _construct(ApproximateDate, "approximate", value["displayValue"], bounds_value)
    if kind == "conflict" and allow_extended:
        if (not isinstance(value, dict) or not {"kind", "claims"}.issubset(value)
                or set(value).difference({"kind", "claims", "tagExtensions"})):
            raise UsageError("conflict is malformed")
        _tag_extensions(value, "conflict")
        if not isinstance(value["claims"], list) or not 2 <= len(value["claims"]) <= 64:
            raise UsageError("conflict.claims must contain 2..64 date values")
        if _depth >= _MAX_CONFLICT_DEPTH:
            raise UsageError(f"conflict nesting exceeds {_MAX_CONFLICT_DEPTH}")
        return _construct(ConflictingDates, "conflict", tuple(decode_date_value(item, _depth=_depth + 1) for item in value["claims"]))
    raise UsageError("unsupported chronology date kind")


def encode_date_value(value: Any) -> dict[str, Any]:
    if isinstance(value, CivilDate):
        result = {"kind": "civil", "calendarId": value.calendar_id, "year": str(value.year)}
        if value.month is not None: result["month"] = str(value.month)
        if value.day is not None: result["day"] = str(value.day)
        return result
    if isinstance(value, EraDate):
        result = {"kind": "era", "eraId": value.era_id, "year": str(value.year)}
        if value.month is not None: result["month"] = str(value.month)
        if value.day is not None: result["day"] = str(value.day)
        return result
    if isinstance(value, CivilRange):
        def endpoint(item: CivilDate | None) -> dict[str, Any] | None:
            if item is None: return None
            result = {"calendarId": item.calendar_id, "year": str(item.year)}
            if item.month is not None: result["month"] = str(item.month)
            if item.day is not None: result["day"] = str(item.day)
            return result
        return {"kind": "range", "calendarId": value.calendar_id, "lower": endpoint(value.lower), "upper": endpoint(value.upper)}
    if isinstance(value, ApproximateDate):
        bounds = encode_date_value(value.bounds)
        return {"kind": "approximate", "displayValue": value.display_value, "bounds": {key: bounds[key] for key in ("calendarId", "lower", "upper")}}
    if isinstance(value, ConflictingDates):
        return {"kind": "conflict", "claims": [encode_date_value(item) for item in value.claims]}
    raise UsageError("unsupported chronology date value")


def chronology_capability(world: Any) -> dict[str, Any]:
    enabled = world.schema == CHRONOLOGY_SOURCE_SCHEMA
    return {"protocol": PROTOCOL, "sourceSchema": world.schema, "mode": "chronology-enabled" if enabled else "ordinal-only", "publicReads": enabled, "authoring": enabled, "upgradeRequired": not enabled, "upgradeAvailable": False, "durationSemantics": "none"}


def _outcome(operation: str, outcome: ChronologyOutcome, result: Any = None) -> dict[str, Any]:
    response: dict[str, Any] = {"protocol": PROTOCOL, "operation": operation, "revision": outcome.revision, "outcome": outcome.kind.value, "advisories": [{"code": item.kind, "message": item.message, "count": item.count} for item in outcome.advisories]}
    if outcome.reason is not None: response["reason"] = outcome.reason.value
    if outcome.detail is not None: response["detail"] = outcome.detail
    if outcome.kind.value == "ok": response["result"] = result
    return response


def _store_outcome(repository: Any, operation: str, invoke: Any, *, require_compiled: bool) -> ChronologyOutcome:
    from .compiler import connect, require_database
    from .chronology_query import SQLiteChronologyStore
    _world, database = require_database(repository, require_compiled=require_compiled)
    with closing(connect(database, True)) as connection:
        return invoke(SQLiteChronologyStore(connection, _world.revision))


def _camel(value: Any, *, opaque: bool = False) -> Any:
    """Encode core source values without interpreting an ``x-*`` payload."""

    if opaque:
        return deepcopy(value)
    if isinstance(value, bool) or value is None or isinstance(value, str): return value
    if isinstance(value, int): return str(value)
    if isinstance(value, list): return [_camel(item) for item in value]
    if not isinstance(value, dict): return value
    return {
        key if isinstance(key, str) and key.startswith("x-") else re.sub(r"_([a-z])", lambda match: match.group(1).upper(), key):
        _camel(item, opaque=isinstance(key, str) and key.startswith("x-"))
        for key, item in value.items()
    }


def _extensions(value: Any) -> dict[str, Any]:
    """Copy extension members verbatim; they are outside the public codec."""

    return {
        key: deepcopy(item)
        for key, item in value.items()
        if isinstance(key, str) and key.startswith("x-")
    } if isinstance(value, dict) else {}


def _public_value_with_tag_extensions(result: dict[str, Any], source_value: dict[str, Any]) -> dict[str, Any]:
    """Attach only source-tag extensions to their reversible public channel."""

    extensions = _extensions(source_value)
    if extensions:
        result["tagExtensions"] = extensions
    return result


def catalog(repository: Any, *, require_compiled: bool = False) -> dict[str, Any]:
    # Catalogue data is public compiled projection data, never an independently
    # rebuilt source projection.  ``require_database`` keeps the established
    # ordinary-rebuild/strict-cache behavior shared by the other read APIs.
    from .compiler import connect, require_database
    world, database = require_database(repository, require_compiled=require_compiled)
    capability = chronology_capability(world)
    result: dict[str, Any] = {"protocol": PROTOCOL, "operation": "catalog", "revision": world.revision, "capability": capability, "calendars": [], "eras": [], "anchors": []}
    if not capability["publicReads"]: return result
    with closing(connect(database, True)) as connection:
        projection = load_chronology_projection(connection, load_annotations=False)
    result["calendars"] = [{**_camel(json.loads(row.definition_json)), "basisId": row.basis_id, "hasEpoch": row.has_epoch} for row in projection.calendars]
    result["eras"] = [{**_camel(json.loads(row.definition_json)), "basisId": row.basis_id} for row in projection.eras]
    # Anchor rows intentionally index only their core mapping.  Use the source
    # declaration solely to retain its allowed opaque extensions after the
    # compiled projection has established the public core values.
    declaration = world.world_record.frontmatter.get("chronology")
    source_anchors: dict[str, dict[str, Any]] = {}
    if isinstance(declaration, dict):
        for item in declaration.get("anchors", []):
            if isinstance(item, dict) and isinstance(item.get("id"), str):
                source_anchors[item["id"]] = item
    anchors: list[dict[str, Any]] = []
    for row in projection.anchors:
        source = source_anchors.get(row.id, {})
        anchor = _camel(source) if isinstance(source, dict) else {}
        story_time = anchor.get("storyTime")
        if not isinstance(story_time, dict): story_time = {}
        anchor.update({
            "id": row.id,
            "axisDay": str(row.axis_day),
            "storyTime": {**story_time, "timeline": row.story_time.timeline, "tick": str(row.story_time.tick), "order": str(row.story_time.order)},
            "provenance": json.loads(row.provenance_json),
        })
        anchors.append(anchor)
    result["anchors"] = anchors
    return result


def format_date(repository: Any, request: dict[str, Any], *, require_compiled: bool = False) -> dict[str, Any]:
    _closed(request, {"protocol", "value"}, "format request")
    if request["protocol"] != PROTOCOL: raise UsageError("unsupported chronology protocol")
    value = decode_date_value(request["value"], allow_extended=False)
    outcome = _store_outcome(repository, "format", lambda store: __import__("wedl.chronology_query", fromlist=["format_chronology_date"]).format_chronology_date(store, value), require_compiled=require_compiled)
    return _outcome("format", outcome, {"value": encode_date_value(value), "formatted": outcome.value})


def convert_date(repository: Any, request: dict[str, Any], *, require_compiled: bool = False) -> dict[str, Any]:
    _closed(request, {"protocol", "value", "target"}, "convert request")
    if request["protocol"] != PROTOCOL or not isinstance(request["target"], dict) or set(request["target"]) not in ({"calendarId"}, {"eraId"}): raise UsageError("convert request is malformed")
    value = decode_date_value(request["value"], allow_extended=False)
    target = request["target"]
    target_id = _nonblank_identifier(target.get("calendarId", target.get("eraId")), "convert target identifier")
    outcome = _store_outcome(repository, "convert", lambda store: __import__("wedl.chronology_query", fromlist=["convert_chronology_date"]).convert_chronology_date(store, ConversionRequest(value, target.get("calendarId"), target.get("eraId"))), require_compiled=require_compiled)
    result = None if outcome.kind.value != "ok" else {"source": encode_date_value(outcome.value.source), "target": encode_date_value(outcome.value.target), "formatted": outcome.value.formatted, "axisDay": str(outcome.value.axis_day)}
    return _outcome("convert", outcome, result)


def search_annotations(repository: Any, request: dict[str, Any], *, require_compiled: bool = False) -> dict[str, Any]:
    allowed = {"protocol", "predicate", "value", "upper", "eraFilter", "limit"}
    if not isinstance(request, dict) or set(request).difference(allowed) or request.get("protocol") != PROTOCOL: raise UsageError("search request is malformed")
    predicate = request.get("predicate")
    if predicate not in {item.value for item in __import__("wedl.chronology_query", fromlist=["ChronologyPredicate"]).ChronologyPredicate}: raise UsageError("search predicate is invalid")
    if (predicate == "between") != ("upper" in request): raise UsageError("between requires upper and other predicates forbid it")
    value = decode_date_value(request.get("value"))
    upper = decode_date_value(request["upper"]) if "upper" in request else None
    limit = request.get("limit", 100)
    if isinstance(limit, bool) or not isinstance(limit, int) or not 1 <= limit <= 10000: raise UsageError("search limit must be an integer from 1 to 10000")
    era_id = None; era_mode = EraMatchMode.AUTHORED
    if "eraFilter" in request:
        selector = request["eraFilter"]
        _closed(selector, {"eraId", "mode"}, "eraFilter")
        if selector["mode"] not in {item.value for item in EraMatchMode}: raise UsageError("eraFilter is malformed")
        era_id, era_mode = _nonblank_identifier(selector["eraId"], "eraFilter.eraId"), EraMatchMode(selector["mode"])
    Predicate = __import__("wedl.chronology_query", fromlist=["ChronologyPredicate"]).ChronologyPredicate
    query = AnnotationQuery(Predicate(predicate), value, upper, era_id, era_mode, limit)
    outcome = _store_outcome(repository, "search", lambda store: __import__("wedl.chronology_query", fromlist=["query_annotations"]).query_annotations(store, query), require_compiled=require_compiled)
    result = None
    if outcome.kind.value == "ok":
        # ``limit`` is deliberately the sole public chronology coordinate
        # represented as a JSON number, so do not run this echo through the
        # general source-to-wire integer encoder.
        result = {"request": {"predicate": predicate, "limit": limit}, "matches": [{"recordId": hit.record_id, "annotationId": hit.annotation_id, "sourceOrdinal": hit.source_ordinal, "role": hit.role, "display": hit.display, "provenance": list(hit.provenance), "valueKind": hit.value_kind, "precision": hit.precision, "basisId": hit.basis_id, "lowerDay": _encode_decimal(hit.lower_day), "upperDay": _encode_decimal(hit.upper_day), "relation": hit.relation, "value": source_value_to_public(hit.value)} for hit in outcome.value.matches]}
    return _outcome("search", outcome, result)


def source_value_to_public(value: dict[str, Any]) -> dict[str, Any]:
    """Encode one source annotation value through the public authoring union.

    ``x-*`` members remain opaque at every nesting boundary.  A qualitative
    approximation has no source calendar at all, so its public bounds are the
    one intentional calendar-free authoring shape.
    """

    kind = next(key for key in value if not key.startswith("x-"))
    payload = value[kind]
    if kind == "civil": return _public_value_with_tag_extensions({**encode_date_value(CivilDate(payload["calendar_id"], payload["year"], payload.get("month"), payload.get("day"))), **_extensions(payload)}, value)
    if kind == "era": return _public_value_with_tag_extensions({**encode_date_value(EraDate(payload["era_id"], payload["year"], payload.get("month"), payload.get("day"))), **_extensions(payload)}, value)
    if kind in {"range", "approx"}:
        bounds = payload if kind == "range" else payload["bounds"]
        if kind == "approx" and bounds["lower"] is None and bounds["upper"] is None:
            result = {
                "kind": "approximate",
                "displayValue": payload["display_value"],
                "bounds": {"lower": None, "upper": None},
            }
            result.update(_extensions(payload))
            result["bounds"].update(_extensions(bounds))
            return _public_value_with_tag_extensions(result, value)
        # Bounds extensions are opaque and may be scalars, arrays, or objects
        # that happen to resemble a civil endpoint.  Only the two declared
        # endpoints may establish the range calendar; source member order is
        # deliberately irrelevant.
        lower, upper = bounds.get("lower"), bounds.get("upper")
        calendar = bounds.get("calendar_id") or next(
            item["calendar_id"] for item in (lower, upper) if item is not None
        )
        date = CivilRange(calendar, None if bounds["lower"] is None else CivilDate(calendar, bounds["lower"]["year"], bounds["lower"].get("month"), bounds["lower"].get("day")), None if bounds["upper"] is None else CivilDate(calendar, bounds["upper"]["year"], bounds["upper"].get("month"), bounds["upper"].get("day")))
        result = encode_date_value(date if kind == "range" else ApproximateDate(payload["display_value"], date))
        result.update(_extensions(payload))
        public_bounds = result if kind == "range" else result["bounds"]
        public_bounds.update(_extensions(bounds))
        for bound in ("lower", "upper"):
            if isinstance(public_bounds[bound], dict): public_bounds[bound].update(_extensions(bounds[bound]))
        return _public_value_with_tag_extensions(result, value)
    if kind == "conflict": return _public_value_with_tag_extensions({"kind": "conflict", "claims": [source_value_to_public(item) for item in payload["claims"]], **_extensions(payload)}, value)
    if kind == "relative": return _public_value_with_tag_extensions({"kind": "relative", "relation": payload["relation"], **({"beforeId": payload["before_id"]} if "before_id" in payload else {}), **({"afterId": payload["after_id"]} if "after_id" in payload else {}), **_extensions(payload)}, value)
    if kind == "duration": return _public_value_with_tag_extensions({"kind": "duration", "unit": payload["unit"], "value": str(payload["value"]), **_extensions(payload)}, value)
    return _camel(value)


def source_annotation_to_public(value: dict[str, Any]) -> dict[str, Any]:
    """Expose a complete source annotation as a chronology.replace item."""

    if not isinstance(value, dict) or not isinstance(value.get("value"), dict):
        raise UsageError("source chronology annotation is malformed")
    result = {
        key: deepcopy(value[key])
        for key in ("id", "temporaryId", "role", "display", "provenance")
        if key in value
    }
    result["value"] = source_value_to_public(value["value"])
    result.update(_extensions(value))
    return result


def story_times(repository: Any, request: dict[str, Any], *, require_compiled: bool = False) -> dict[str, Any]:
    _closed(request, {"protocol", "value"}, "story-times request")
    if request["protocol"] != PROTOCOL: raise UsageError("unsupported chronology protocol")
    value = decode_date_value(request["value"])
    outcome = _store_outcome(repository, "story-times", lambda store: __import__("wedl.chronology_query", fromlist=["map_chronology_date_to_story_time"]).map_chronology_date_to_story_time(store, value), require_compiled=require_compiled)
    result = None
    if outcome.kind.value == "ok":
        mapped = outcome.value
        # The kernel's explicit mapping union is NoMatch / Unique(value) /
        # Ambiguous(values).  Preserve that cardinality at the public wire
        # boundary instead of assuming every successful mapping has ``values``.
        times = tuple(mapped.values) if hasattr(mapped, "values") else (mapped.value,) if hasattr(mapped, "value") else ()
        result = {"mapping": "none" if not times else "unique" if len(times) == 1 else "ambiguous", "storyTimes": [{"timeline": item.timeline, "tick": str(item.tick), "order": str(item.order)} for item in times]}
    return _outcome("story-times", outcome, result)
