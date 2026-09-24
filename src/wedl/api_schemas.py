"""Framework-free OpenAPI response schema and example registry.

The registry intentionally describes stable envelopes and important fields
without pretending that every additive result field is fixed.  HTTP routing
uses these declarations only for discovery; runtime serialization remains in
the command/query implementations.
"""

from __future__ import annotations

from copy import deepcopy
from itertools import combinations
from typing import Any


def _object(*required: str, properties: dict[str, Any] | None = None) -> dict[str, Any]:
    return {
        "type": "object",
        "required": list(required),
        "properties": properties or {},
        "additionalProperties": True,
    }


def _strict_object(*required: str, properties: dict[str, Any] | None = None) -> dict[str, Any]:
    """Describe a closed, discriminated response object."""

    return {
        "type": "object",
        "required": list(required),
        "properties": properties or {},
        "additionalProperties": False,
    }


def _extension_object(*required: str, properties: dict[str, Any] | None = None) -> dict[str, Any]:
    """A closed source-shaped object which preserves documented ``x-*`` data."""

    result = _strict_object(*required, properties=properties)
    result["patternProperties"] = {"^x-[A-Za-z0-9_.-]+$": {}}
    return result


def _less_than_or_equal_pattern(bound: int) -> str:
    """Return an exact canonical-positive-decimal pattern through ``bound``."""

    digits = str(bound)
    alternatives = ["[1-9][0-9]{0," + str(len(digits) - 2) + "}"]
    for index, character in enumerate(digits):
        digit = int(character)
        minimum = 1 if index == 0 else 0
        if digit > minimum:
            suffix = "[0-9]{" + str(len(digits) - index - 1) + "}" if index + 1 < len(digits) else ""
            alternatives.append(digits[:index] + "[" + str(minimum) + "-" + str(digit - 1) + "]" + suffix)
    alternatives.append(digits)
    return "(?:" + "|".join(alternatives) + ")"


def _bounded_decimal_string(maximum: int, negative_magnitude: int) -> dict[str, Any]:
    """JSON Schema for a canonical string whose numeric value is bounded."""

    positive = _less_than_or_equal_pattern(maximum)
    negative = _less_than_or_equal_pattern(negative_magnitude)
    return {"type": "string", "pattern": "^(?:0|" + positive + "|-" + negative + ")$"}


def _identified_author_item(prefix: str, *required: str, properties: dict[str, Any]) -> dict[str, Any]:
    """Return a closed source object with exactly one permanent or scoped ID."""

    shared = {
        **properties,
        "id": _CHRONOLOGY_NONBLANK_STRING,
        "temporaryId": {"type": "string", "pattern": "^\\$" + prefix + "\\.[A-Za-z0-9_.-]+$"},
    }
    return {
        "oneOf": [
            _extension_object(*required, "id", properties=shared),
            _extension_object(*required, "temporaryId", properties=shared),
        ]
    }


_STRING = {"type": "string"}
_BOOLEAN = {"type": "boolean"}
_INTEGER = {"type": "integer"}
_OBJECT = {"type": "object", "additionalProperties": True}
_TIMELINE_INTEGER = {"type": "string", "pattern": "^-?[0-9]+$"}
_TIMELINE_COORDINATE = _object("timeline", "tick", "order", properties={"timeline": _STRING, "tick": _TIMELINE_INTEGER, "order": _TIMELINE_INTEGER})
_STORY_TIME = _object("timeline", "tick", "order", properties={"timeline": _STRING, "tick": _INTEGER, "order": _INTEGER})
_TIMELINE_REFERENCE = _object("id", "kind", "title", properties={"id": _STRING, "kind": _STRING, "title": _STRING})
_TIMELINE_ORIGIN = _object("tick", "label", properties={"tick": _TIMELINE_INTEGER, "label": _STRING})
_TIMELINE_NULLABLE_REFERENCE = {"anyOf": [_TIMELINE_REFERENCE, {"type": "null"}]}
_TIMELINE_NULLABLE_COORDINATE = {"anyOf": [_TIMELINE_COORDINATE, {"type": "null"}]}
_HYPOTHESIS_REFERENCE = _strict_object("kind", "title", properties={"kind": _STRING, "title": _STRING})
_HYPOTHESIS_PLACEMENT = _strict_object(properties={"timeline": _STRING, "scene": _HYPOTHESIS_REFERENCE, "event": _HYPOTHESIS_REFERENCE, "location": _HYPOTHESIS_REFERENCE, "context": _STRING})
_HYPOTHESIS_RESOLUTION = _object(properties={"canonicalRecords": {"type": "array", "items": _HYPOTHESIS_REFERENCE}, "note": {"type": ["string", "null"]}})
_HYPOTHESIS = _strict_object("title", "status", "nonCanonical", "statement", "subjects", "alternatives", "context", "placement", properties={
    "title": _STRING, "status": {"enum": ["open", "adopted", "rejected"]}, "nonCanonical": {"const": True},
    "statement": _STRING, "subjects": {"type": "array", "items": _HYPOTHESIS_REFERENCE},
    "alternatives": {"type": "array", "items": _STRING}, "context": _STRING,
    "placement": _HYPOTHESIS_PLACEMENT, "resolution": _HYPOTHESIS_RESOLUTION,
})
_TIMELINE_PARTICIPANT = _strict_object(
    "id", "kind", "title", "from", "to",
    properties={
        "id": _STRING, "kind": _STRING, "title": _STRING,
        "from": _TIMELINE_COORDINATE, "to": _TIMELINE_NULLABLE_COORDINATE,
    },
)
_LOCATION_HISTORY_ITEM = _object("operation", "at", "location", properties={"operation": {"enum": ["initial", "set", "clear"]}, "at": {"anyOf": [_TIMELINE_COORDINATE, {"type": "null"}]}, "location": _TIMELINE_NULLABLE_REFERENCE, "event": _TIMELINE_REFERENCE})
_WHEREABOUTS_JOURNEY = _strict_object(
    "kind", "at", "from", "to", "event",
    properties={
        "kind": {"enum": ["initial", "move", "reaffirmation", "clear"]},
        "at": _TIMELINE_NULLABLE_COORDINATE,
        "from": _TIMELINE_NULLABLE_REFERENCE,
        "to": _TIMELINE_NULLABLE_REFERENCE,
        "event": _TIMELINE_NULLABLE_REFERENCE,
    },
)
_IMPORTANCE_BREAKDOWN = _strict_object(
    "algorithm", "score", "raw", "normalized", "contributions", "explanation",
    properties={
        "algorithm": {"const": "wedl-character-importance/v1"}, "score": {"type": "number"},
        "raw": _strict_object("scenes", "pointOfViewScenes", "events", "relationshipNeighbors", properties={"scenes": _INTEGER, "pointOfViewScenes": _INTEGER, "events": _INTEGER, "relationshipNeighbors": _INTEGER}),
        "normalized": _strict_object("scenes", "pointOfViewScenes", "events", "relationshipNeighbors", properties={"scenes": {"type": "number"}, "pointOfViewScenes": {"type": "number"}, "events": {"type": "number"}, "relationshipNeighbors": {"type": "number"}}),
        "contributions": _strict_object("scenes", "pointOfViewScenes", "events", "relationshipNeighbors", properties={"scenes": {"type": "number"}, "pointOfViewScenes": {"type": "number"}, "events": {"type": "number"}, "relationshipNeighbors": {"type": "number"}}),
        "explanation": _STRING,
    },
)
_IMPORTANCE_POLICY = _strict_object("algorithm", "calculated", "nonCanonical", "cohort", "normalization", "weights", "evidence", "exclusions", properties={
    "algorithm": {"const": "wedl-character-importance/v1"}, "calculated": {"const": True}, "nonCanonical": {"const": True}, "cohort": _STRING, "normalization": _STRING,
    "weights": _strict_object("scenes", "pointOfViewScenes", "events", "relationshipNeighbors", properties={"scenes": _INTEGER, "pointOfViewScenes": _INTEGER, "events": _INTEGER, "relationshipNeighbors": _INTEGER}), "evidence": _STRING, "exclusions": _STRING,
})
_WHEREABOUTS_CHARACTER = _strict_object(
    "character", "recordStatus", "role", "importance", "presence", "location", "lastKnownLocation", "activeScene", "journey",
    properties={
        "character": _TIMELINE_REFERENCE,
        "recordStatus": {"enum": ["canonical", "retired"]},
        "role": {"type": ["string", "null"]}, "importance": _IMPORTANCE_BREAKDOWN,
        "presence": {"enum": ["active-scene", "offstage", "unlocated"]},
        "location": _TIMELINE_NULLABLE_REFERENCE,
        "lastKnownLocation": _TIMELINE_NULLABLE_REFERENCE,
        "activeScene": _TIMELINE_NULLABLE_REFERENCE,
        "journey": {"type": "array", "items": _WHEREABOUTS_JOURNEY},
    },
)
_WHEREABOUTS_LOCATION = _strict_object(
    "location", "characters",
    properties={"location": _TIMELINE_REFERENCE, "characters": {"type": "array", "items": _TIMELINE_REFERENCE}},
)
_WHEREABOUTS_ACTIVE_SCENE = _strict_object(
    "scene", "location", "characters",
    properties={
        "scene": _TIMELINE_REFERENCE,
        "location": _TIMELINE_NULLABLE_REFERENCE,
        "characters": {"type": "array", "items": _TIMELINE_REFERENCE},
    },
)
_LOCATION_LINK = _object("place", "reciprocal", properties={"place": _TIMELINE_REFERENCE, "reciprocal": _BOOLEAN, "description": _STRING, "summary": _STRING})
_LOCATION_CONTEXT = _object("parent", "children", "outgoing", "incoming", properties={"parent": _TIMELINE_NULLABLE_REFERENCE, "children": {"type": "array", "items": _TIMELINE_REFERENCE}, "outgoing": {"type": "array", "items": _LOCATION_LINK}, "incoming": {"type": "array", "items": _LOCATION_LINK}})
_TIMELINE_EVENT_POINT = _object("kind", "at", "status", "entity", "summary", properties={"kind": {"const": "event"}, "at": _TIMELINE_COORDINATE, "status": _STRING, "entity": _TIMELINE_REFERENCE, "summary": _STRING, "location": _TIMELINE_NULLABLE_REFERENCE, "participants": {"type": "array", "items": _TIMELINE_REFERENCE}, "causes": {"type": "array", "items": _TIMELINE_REFERENCE}, "plotThreads": {"type": "array", "items": _TIMELINE_REFERENCE}})
_TIMELINE_PLOT_POINT = _object("kind", "at", "status", "lifecycleState", "entity", "summary", properties={"kind": {"const": "plot-transition"}, "at": _TIMELINE_COORDINATE, "status": _STRING, "lifecycleState": _STRING, "entity": _TIMELINE_REFERENCE, "summary": _STRING, "causingEvent": _TIMELINE_NULLABLE_REFERENCE, "outcomeEvents": {"type": "array", "items": _TIMELINE_REFERENCE}, "dependencies": {"type": "array", "items": _TIMELINE_REFERENCE}})
_TIMELINE_POINT = {"oneOf": [_TIMELINE_EVENT_POINT, _TIMELINE_PLOT_POINT]}
_TIMELINE_SCENE_SPAN = _object("kind", "start", "current", "end", "status", "entity", "summary", properties={"kind": {"const": "scene"}, "start": _TIMELINE_COORDINATE, "current": _TIMELINE_COORDINATE, "end": {"anyOf": [_TIMELINE_COORDINATE, {"type": "null"}]}, "status": _STRING, "entity": _TIMELINE_REFERENCE, "summary": _STRING, "location": _TIMELINE_NULLABLE_REFERENCE, "participants": {"type": "array", "items": _TIMELINE_PARTICIPANT}, "conversations": {"type": "array", "items": _TIMELINE_REFERENCE}, "plotThreads": {"type": "array", "items": _TIMELINE_REFERENCE}})
_TIMELINE_CONVERSATION_SPAN = _object("kind", "start", "end", "status", "entity", "summary", properties={"kind": {"const": "conversation"}, "start": _TIMELINE_COORDINATE, "end": {"anyOf": [_TIMELINE_COORDINATE, {"type": "null"}]}, "status": _STRING, "entity": _TIMELINE_REFERENCE, "summary": _STRING, "scene": _TIMELINE_NULLABLE_REFERENCE, "location": _TIMELINE_NULLABLE_REFERENCE, "participants": {"type": "array", "items": _TIMELINE_PARTICIPANT}})
_TIMELINE_SPAN = {"oneOf": [_TIMELINE_SCENE_SPAN, _TIMELINE_CONVERSATION_SPAN]}
_PLOT_TRANSITION = _object("at", "state", "causingEvent", properties={"at": _STORY_TIME, "state": _STRING, "note": {"type": ["string", "null"]}, "causingEvent": _TIMELINE_NULLABLE_REFERENCE})
_PLOT_OUTCOME = _object("event", "at", properties={"event": _TIMELINE_REFERENCE, "at": _STORY_TIME})
_PLOT_TRAIL = _strict_object("transitions", "outcomeEvents", properties={"transitions": {"type": "array", "items": _PLOT_TRANSITION}, "outcomeEvents": {"type": "array", "items": _PLOT_OUTCOME}})
_CONTINUITY_ADVISORY = _object("code", "kind", "message", properties={"code": _STRING, "kind": _STRING, "message": _STRING, "storyPoint": _TIMELINE_REFERENCE, "event": _TIMELINE_REFERENCE, "cause": _TIMELINE_REFERENCE, "effect": _TIMELINE_REFERENCE})
_STORY_POINT_VIEW = _object("storyPointId", "title", "storedState", "derivedState", "eligible", "dependenciesSatisfied", "triggerSatisfied", "priority", "plotTrail", properties={"storyPointId": _STRING, "title": _STRING, "storedState": _STRING, "derivedState": _STRING, "eligible": _BOOLEAN, "dependenciesSatisfied": _BOOLEAN, "triggerSatisfied": _BOOLEAN, "priority": _INTEGER, "plotTrail": _PLOT_TRAIL})
_CAUSALITY_NODE = _strict_object("event", "at", "status", properties={"event": _TIMELINE_REFERENCE, "at": _STORY_TIME, "status": _STRING})
_CAUSALITY_EDGE = _strict_object("cause", "effect", properties={"cause": _TIMELINE_REFERENCE, "effect": _TIMELINE_REFERENCE})

_BEAT_COMMON_PROPERTIES = {
    "id": _STRING, "kind": _STRING, "at": _STORY_TIME, "text": _STRING,
    "audience": {"type": "array", "items": _STRING}, "citation": _OBJECT,
}
_SPEECH_BEAT = _strict_object(
    "id", "kind", "at", "text", "audience", "citation", "speakerId", "speaker",
    properties={
        **_BEAT_COMMON_PROPERTIES,
        "kind": {"const": "speech"}, "speakerId": _STRING, "speaker": _STRING,
        "delivery": {"type": ["string", "null"]},
        "addresseeId": {"type": ["string", "null"]},
        "addressee": {"type": ["string", "null"]},
        "interrupts": {"type": ["string", "null"]},
    },
)
_ACTION_BEAT = _strict_object(
    "id", "kind", "at", "text", "audience", "citation", "actorIds", "actors",
    properties={
        **_BEAT_COMMON_PROPERTIES,
        "kind": {"const": "action"},
        "actorIds": {"type": "array", "items": _STRING},
        "actors": {"type": "array", "items": _STRING},
    },
)
_CONVERSATION_BEAT = {"oneOf": [_SPEECH_BEAT, _ACTION_BEAT]}


# The HTTP changeset endpoints deliberately accept the protocol document
# itself.  Keep this component separate from the schema *description* returned
# by ``GET /api/changesets/schema``: one is a JSON Schema-shaped discovery
# contract, the other is WEDL's concise operation vocabulary document.
_CHANGESET_REQUEST = _object(
    "protocol", "expectedHead", "idempotencyKey", "summary", "operations",
    properties={
        "protocol": {"const": "wedl-changeset/v1"},
        "expectedHead": _STRING,
        "idempotencyKey": _STRING,
        "requestId": _STRING,
        "summary": _STRING,
        "operations": {"type": "array", "items": _OBJECT},
    },
)

_AUTHORING_TIME = _object("tick", properties={"timeline": _STRING, "tick": _INTEGER, "order": _INTEGER})
_NONBLANK_STRING = {"type": "string", "minLength": 1}
_SPATIAL_NONBLANK_STRING = {"type": "string", "minLength": 1, "maxLength": 256, "pattern": "\\S"}
_SPATIAL_SUMMARY = {"type": "string", "minLength": 1, "maxLength": 1024, "pattern": "\\S"}
_CHRONOLOGY_NONBLANK_STRING = {"type": "string", "minLength": 1, "pattern": ".*\\S.*"}
_AUTHORING_COMMON = {
    "time": _AUTHORING_TIME, "title": _STRING, "location": _STRING, "scene": _STRING,
    "conversation": _STRING, "characters": {"type": "array", "items": _STRING}, "text": _STRING,
    "kind": {"enum": ["speech", "action"]}, "speaker": _STRING, "addressee": _STRING,
    "actors": {"type": "array", "items": _STRING}, "interruptLast": _BOOLEAN,
    "summary": _STRING, "idempotencyKey": _STRING,
    "hypothesis": _STRING, "statement": _STRING, "subjects": {"type": "array", "items": _STRING},
    "alternatives": {"type": "array", "items": _STRING}, "context": _STRING, "event": _STRING,
    "timeline": _STRING, "canonicalRecords": {"type": "array", "items": _STRING}, "note": _STRING,
}
_SPATIAL_SAFE_NUMBER = {"type": "number", "minimum": -(2 ** 53 - 1), "maximum": 2 ** 53 - 1}
_SPATIAL_COORDINATES = {"type": "array", "minItems": 2, "maxItems": 3, "items": _SPATIAL_SAFE_NUMBER}
_SPATIAL_BOUNDS = _strict_object("min", "max", properties={"min": _SPATIAL_COORDINATES, "max": _SPATIAL_COORDINATES})
_SPATIAL_TIME = _strict_object("timeline", "tick", "order", properties={"timeline": _SPATIAL_NONBLANK_STRING, "tick": _bounded_decimal_string(2 ** 63 - 1, 2 ** 63), "order": _bounded_decimal_string(2 ** 31 - 1, 2 ** 31)})
_SPATIAL_METRIC = _strict_object("value", "unit", properties={"value": {**_SPATIAL_SAFE_NUMBER, "minimum": 0}, "unit": _SPATIAL_NONBLANK_STRING})
_SPATIAL_VERTEX_LIST = {"type": "array", "minItems": 2, "maxItems": 10000, "items": _SPATIAL_COORDINATES}
_SPATIAL_GEOMETRY = {"oneOf": [
    _strict_object("kind", "coordinates", properties={"kind": {"const": "point"}, "coordinates": _SPATIAL_COORDINATES}),
    _strict_object("kind", "coordinates", properties={"kind": {"const": "line"}, "coordinates": _SPATIAL_VERTEX_LIST}),
    _strict_object("kind", "coordinates", properties={"kind": {"const": "polygon"}, "coordinates": {**_SPATIAL_VERTEX_LIST, "minItems": 4}}),
]}
_SPATIAL_PLACEMENT = _strict_object("mapId", "geometry", properties={"mapId": _SPATIAL_NONBLANK_STRING, "geometry": _SPATIAL_GEOMETRY})
_SPATIAL_MEMBERSHIP = _strict_object("locationIds", properties={"locationIds": {"type": "array", "minItems": 1, "maxItems": 10000, "uniqueItems": True, "items": _SPATIAL_NONBLANK_STRING}})
_SPATIAL_VALID = _strict_object("start", "end", properties={"start": _SPATIAL_TIME, "end": _SPATIAL_TIME})
_SPATIAL_MAP_FIELDS = {"id": _SPATIAL_NONBLANK_STRING, "title": _SPATIAL_NONBLANK_STRING, "crs": _SPATIAL_NONBLANK_STRING, "axisOrder": {"type": "array", "minItems": 2, "maxItems": 2, "uniqueItems": True, "items": _SPATIAL_NONBLANK_STRING}, "unit": _SPATIAL_NONBLANK_STRING, "bounds": _SPATIAL_BOUNDS, "zPolicy": {"enum": ["forbidden", "optional-level", "required"]}}
_SPATIAL_LOCATION_FIELDS = {"id": _SPATIAL_NONBLANK_STRING, "parentId": {"anyOf": [_SPATIAL_NONBLANK_STRING, {"type": "null"}]}, "spatial": {"oneOf": [_SPATIAL_PLACEMENT, {"type": "null"}]}}
_SPATIAL_ROUTE_FIELDS = {"id": _SPATIAL_NONBLANK_STRING, "title": _SPATIAL_NONBLANK_STRING, "fromLocationId": _SPATIAL_NONBLANK_STRING, "toLocationId": _SPATIAL_NONBLANK_STRING, "direction": {"enum": ["one-way", "two-way"]}, "modes": {"type": "array", "minItems": 1, "maxItems": 100, "uniqueItems": True, "items": _SPATIAL_NONBLANK_STRING}, "routeDistance": _SPATIAL_METRIC, "travelCost": _SPATIAL_METRIC, "duration": _SPATIAL_METRIC, "availability": {"enum": ["open", "closed", "restricted", "unknown"]}, "uncertainty": {"enum": ["exact", "estimated", "unknown"]}}
_SPATIAL_OVERLAY_FIELDS = {"id": _SPATIAL_NONBLANK_STRING, "title": _SPATIAL_NONBLANK_STRING, "lifecycle": {"enum": ["static", "time-bounded"]}, "membership": _SPATIAL_MEMBERSHIP, "audience": {"type": "array", "minItems": 1, "maxItems": 100, "uniqueItems": True, "items": _SPATIAL_NONBLANK_STRING}, "perspectives": {"type": "array", "minItems": 1, "maxItems": 100, "uniqueItems": True, "items": _SPATIAL_NONBLANK_STRING}, "valid": _SPATIAL_VALID}


def _spatial_authoring(action: str, required: tuple[str, ...], fields: dict[str, Any]) -> dict[str, Any]:
    return _strict_object("action", "expectedHead", "idempotencyKey", "payload", properties={"action": {"const": action}, "expectedHead": {"type": "string", "pattern": "^[0-9a-f]{40}$"}, "idempotencyKey": _SPATIAL_NONBLANK_STRING, "summary": _SPATIAL_SUMMARY, "payload": _strict_object(*required, properties=fields)})


def _spatial_update_authoring(action: str, fields: dict[str, Any]) -> dict[str, Any]:
    payload = _strict_object("id", properties=fields)
    payload["anyOf"] = [{"required": [name]} for name in fields if name != "id"]
    return _strict_object("action", "expectedHead", "idempotencyKey", "payload", properties={"action": {"const": action}, "expectedHead": {"type": "string", "pattern": "^[0-9a-f]{40}$"}, "idempotencyKey": _SPATIAL_NONBLANK_STRING, "summary": _SPATIAL_SUMMARY, "payload": payload})


_SPATIAL_OVERLAY_CREATE = {"oneOf": [
    _strict_object("id", "title", "lifecycle", "membership", "audience", "perspectives", properties={**{key: value for key, value in _SPATIAL_OVERLAY_FIELDS.items() if key != "valid"}, "lifecycle": {"const": "static"}}),
    _strict_object("id", "title", "lifecycle", "membership", "audience", "perspectives", "valid", properties={**_SPATIAL_OVERLAY_FIELDS, "lifecycle": {"const": "time-bounded"}}),
]}


_SPATIAL_AUTHORING = [
    _spatial_authoring("spatial.map.create", ("id", "title", "crs", "axisOrder", "unit", "bounds"), _SPATIAL_MAP_FIELDS),
    _spatial_update_authoring("spatial.map.update", {key: value for key, value in _SPATIAL_MAP_FIELDS.items() if key != "title"}),
    _spatial_update_authoring("spatial.location.update", _SPATIAL_LOCATION_FIELDS),
    _spatial_authoring("spatial.route.create", ("id", "title", "fromLocationId", "toLocationId", "direction", "modes"), _SPATIAL_ROUTE_FIELDS),
    _spatial_update_authoring("spatial.route.update", {key: value for key, value in _SPATIAL_ROUTE_FIELDS.items() if key != "title"}),
    _strict_object("action", "expectedHead", "idempotencyKey", "payload", properties={"action": {"const": "spatial.overlay.create"}, "expectedHead": {"type": "string", "pattern": "^[0-9a-f]{40}$"}, "idempotencyKey": _SPATIAL_NONBLANK_STRING, "summary": _SPATIAL_SUMMARY, "payload": _SPATIAL_OVERLAY_CREATE}),
    _spatial_update_authoring("spatial.overlay.update", {key: value for key, value in _SPATIAL_OVERLAY_FIELDS.items() if key != "title"}),
]
_GEN_TIME = _strict_object("timeline", "tick", "order", properties={
    "timeline": _SPATIAL_NONBLANK_STRING,
    "tick": _bounded_decimal_string(2 ** 63 - 1, 2 ** 63),
    "order": _bounded_decimal_string(2 ** 31 - 1, 2 ** 31),
})
_GEN_SHA = {"type": "string", "pattern": "^[0-9a-f]{40}$"}
_GEN_REF = _SPATIAL_NONBLANK_STRING
_GEN_REFS = {"type": "array", "minItems": 2, "maxItems": 100, "uniqueItems": True, "items": _GEN_REF}
_GEN_WORDS = {"type": "array", "maxItems": 100, "uniqueItems": True, "items": _SPATIAL_NONBLANK_STRING}
_GEN_FIELD_SCHEMAS = {
    "organization": (("organization_kind",), {"organization_kind": {"enum": ["house", "dynasty", "clan", "institution", "other"]}, "parent_id": {"anyOf": [_GEN_REF, {"type": "null"}]}, "location_id": {"anyOf": [_GEN_REF, {"type": "null"}]}}),
    "parentage": (("child_id", "parent_id"), {"child_id": _GEN_REF, "parent_id": _GEN_REF}),
    "union": (("participant_ids",), {"participant_ids": _GEN_REFS}),
    "affiliation": (("character_id", "organization_id"), {"character_id": _GEN_REF, "organization_id": _GEN_REF}),
    "legacy": (("legacy_kind",), {"legacy_kind": {"enum": ["office", "estate", "title", "other"]}, "organization_id": {"anyOf": [_GEN_REF, {"type": "null"}]}}),
    "tenure": (("legacy_id",), {"legacy_id": _GEN_REF, "predecessor_tenure_id": {"anyOf": [_GEN_REF, {"type": "null"}]}, "successor_tenure_id": {"anyOf": [_GEN_REF, {"type": "null"}]}}),
    "claim": (("legacy_id", "claimant_id"), {"legacy_id": _GEN_REF, "claimant_id": _GEN_REF}),
    "vital-history": (("character_id", "disclosure"), {"character_id": _GEN_REF, "disclosure": {"enum": ["known", "unknown", "withheld"]}}),
}
_GEN_TRANSITIONS = {
    "organization": ("rename", "reparent", "dormant", "dissolve"),
    "parentage": ("confirm", "end"),
    "union": ("form", "reconcile", "end", "annul"),
    "affiliation": ("role", "end"),
    "legacy": ("rename", "dormant", "dissolve"),
    "tenure": ("designate", "hold", "vacate", "transfer", "end"),
    "claim": ("dispute", "recognize", "withdraw", "reject"),
    "vital-history": ("birth", "death", "existence-start", "existence-end"),
}
_GEN_EMPTY = _strict_object(properties={})
_GEN_PAYLOAD = {
    "organization-initialize": _strict_object("title", "aliases", properties={"title": _GEN_REF, "aliases": _GEN_WORDS}),
    "organization-rename": _strict_object("title", "aliases", properties={"title": _GEN_REF, "aliases": _GEN_WORDS}),
    "organization-reparent": _strict_object("parent_id", properties={"parent_id": {"anyOf": [_GEN_REF, {"type": "null"}]}}),
    "parentage-initialize": _strict_object("basis", properties={"basis": {"enum": ["biological", "adoptive"]}}),
    "parentage-confirm": _strict_object("basis", properties={"basis": {"enum": ["biological", "adoptive"]}}),
    "union-initialize": _strict_object("participant_ids", properties={"participant_ids": _GEN_REFS}),
    "union-form": _strict_object("participant_ids", properties={"participant_ids": _GEN_REFS}),
    "union-reconcile": _strict_object("participant_ids", properties={"participant_ids": _GEN_REFS}),
    "affiliation-initialize": _strict_object("role", properties={"role": {"type": ["string", "null"]}}),
    "affiliation-role": _strict_object("role", properties={"role": _GEN_REF}),
    "legacy-initialize": _strict_object("title", "aliases", properties={"title": _GEN_REF, "aliases": _GEN_WORDS}),
    "legacy-rename": _strict_object("title", "aliases", properties={"title": _GEN_REF, "aliases": _GEN_WORDS}),
    "tenure-initialize": _strict_object("holder_id", "basis", properties={"holder_id": {"anyOf": [_GEN_REF, {"type": "null"}]}, "basis": {"enum": ["legal", "de-facto"]}}),
    "tenure-designate": _strict_object("holder_id", "basis", properties={"holder_id": _GEN_REF, "basis": {"enum": ["legal", "de-facto"]}}),
    "tenure-hold": _strict_object("holder_id", "basis", properties={"holder_id": _GEN_REF, "basis": {"enum": ["legal", "de-facto"]}}),
    "tenure-vacate": _strict_object("holder_id", properties={"holder_id": {"type": "null"}}),
    "tenure-transfer": _strict_object("from_tenure_id", "to_tenure_id", properties={"from_tenure_id": _GEN_REF, "to_tenure_id": _GEN_REF}),
    "claim-initialize": _strict_object("competes_with", properties={"competes_with": {"type": "array", "maxItems": 100, "uniqueItems": True, "items": _GEN_REF}}),
    "claim-dispute": _strict_object("competes_with", properties={"competes_with": {"type": "array", "maxItems": 100, "uniqueItems": True, "items": _GEN_REF}}),
}
_GEN_OUTER = {"expectedHead": _GEN_SHA, "idempotencyKey": _GEN_REF, "summary": _SPATIAL_SUMMARY}


def _gen_author_item(kind: str, action: str, transition: str | None = None, *, outer: bool = False) -> dict[str, Any]:
    common = {"action": {"const": action}, "kind": {"const": kind}}
    required = ["action", "kind"]
    if action == "generational.create":
        field_required, field_properties = _GEN_FIELD_SCHEMAS[kind]
        initial = ("vital" if kind == "vital-history" else kind) + "-initialize"
        common.update({"id": _GEN_REF, "title": _GEN_REF, "domain": _GEN_REF,
                       "tags": _GEN_WORDS, "aliases": _GEN_WORDS, "threads": _GEN_WORDS,
                       "audience": {**_GEN_WORDS, "minItems": 1}, "perspectives": {**_GEN_WORDS, "minItems": 1},
                       "fields": _strict_object(*field_required, properties=field_properties),
                       "payload": _GEN_PAYLOAD.get(initial, _GEN_EMPTY), "at": _GEN_TIME})
        required.extend(("title", "audience", "perspectives", "fields", "payload", "at"))
    else:
        assert transition is not None
        common.update({"record": _GEN_REF, "transition": {"const": transition},
                       "payload": _GEN_PAYLOAD.get(transition, _GEN_EMPTY),
                       "cause": _GEN_REF, "transitionId": _GEN_REF})
        required.extend(("record", "transition", "payload", "cause"))
        if transition == "tenure-vacate":
            common["interval"] = _strict_object("first", "last", properties={"first": _GEN_TIME, "last": _GEN_TIME})
            required.append("interval")
        else:
            common["at"] = _GEN_TIME
            required.append("at")
        if action == "generational.correct":
            common["replaces"] = _GEN_REF
            required.append("replaces")
    if outer:
        common.update(_GEN_OUTER)
        required.extend(("expectedHead", "idempotencyKey"))
    return _strict_object(*required, properties=common)


_GEN_AUTHOR_SINGLE = [_gen_author_item(kind, "generational.create", outer=True) for kind in _GEN_FIELD_SCHEMAS]
_GEN_AUTHOR_ITEMS = [_gen_author_item(kind, "generational.create") for kind in _GEN_FIELD_SCHEMAS]
for _kind, _verbs in _GEN_TRANSITIONS.items():
    for _verb in _verbs:
        _transition = ("vital" if _kind == "vital-history" else _kind) + "-" + _verb
        for _action in ("generational.append", "generational.correct"):
            _GEN_AUTHOR_SINGLE.append(_gen_author_item(_kind, _action, _transition, outer=True))
            _GEN_AUTHOR_ITEMS.append(_gen_author_item(_kind, _action, _transition))
_GEN_BATCH = _strict_object("action", "expectedHead", "idempotencyKey", "items", properties={
    "action": {"const": "generational.batch"}, **_GEN_OUTER,
    "items": {"type": "array", "minItems": 1, "maxItems": 32, "items": {"oneOf": _GEN_AUTHOR_ITEMS}},
})
_GEN_AUTHORING_REQUEST = {"oneOf": [*_GEN_AUTHOR_SINGLE, _GEN_BATCH]}
_AUTHORING_REQUEST = {"oneOf": [
    _object("action", "time", properties={**_AUTHORING_COMMON, "action": {"const": "current-time.set"}}),
    _object("action", "title", "location", "characters", properties={**_AUTHORING_COMMON, "action": {"const": "scene.create"}}),
    _object("action", properties={**_AUTHORING_COMMON, "action": {"const": "scene.advance"}}),
    _object("action", "scene", properties={**_AUTHORING_COMMON, "action": {"const": "scene.close"}, "scene": _NONBLANK_STRING}),
    _object("action", "location", "characters", properties={**_AUTHORING_COMMON, "action": {"const": "character.move"}}),
    _object("action", "title", properties={**_AUTHORING_COMMON, "action": {"const": "conversation.create"}}),
    _object("action", "conversation", "text", properties={**_AUTHORING_COMMON, "action": {"const": "conversation.append"}}),
    _object("action", "title", "statement", "subjects", "alternatives", "context", properties={**_AUTHORING_COMMON, "action": {"const": "hypothesis.create"}}),
    _object("action", "hypothesis", "canonicalRecords", properties={**_AUTHORING_COMMON, "action": {"const": "hypothesis.adopt"}}),
    _object("action", "hypothesis", "note", properties={**_AUTHORING_COMMON, "action": {"const": "hypothesis.reject"}, "note": _NONBLANK_STRING}),
    {"$ref": "#/components/schemas/ChronologyAuthoringRequest"},
    *_SPATIAL_AUTHORING,
    {"$ref": "#/components/schemas/GenerationalAuthoringRequest"},
]}

_AUTHOR_IMPACT_ITEM = _object(
    "kind",
    properties={
        "kind": {"enum": ["conversation-created", "conversation-turn-appended", "author-horizon-advanced", "hypothesis-recorded", "hypothesis-status", "chronology-catalog-replaced", "chronology-annotations-replaced"]},
        "record": {"type": ["string", "null"]},
        "conversation": {"type": ["string", "null"]},
        "scene": {"type": ["string", "null"]}, "location": {"type": ["string", "null"]},
        "characters": {"type": "array", "items": _STRING},
        # Hypotheses are repository notes rather than story occurrences.  An
        # absent (or explicitly null) coordinate is therefore meaningful and
        # must not be replaced with a made-up chronology point.
        "at": {"anyOf": [_STORY_TIME, {"type": "null"}]},
        "hypothesis": {"type": ["string", "null"]},
        "status": {"enum": ["open", "adopted", "rejected"]},
        "nonCanonical": {"type": "boolean"},
    },
)
_AUTHOR_IMPACT = _object(
    "summary", "items",
    properties={"summary": _STRING, "items": {"type": "array", "items": _AUTHOR_IMPACT_ITEM}},
)


def _chronology_outcome(operation: str, result: str) -> dict[str, Any]:
    """Closed semantic outcomes shared by each public chronology read."""
    common = {"protocol": {"const": "wedl-chronology/v1"}, "operation": {"const": operation}, "revision": _STRING, "advisories": {"type": "array", "items": {"$ref": "#/components/schemas/ChronologyAdvisory"}}}
    return {"oneOf": [
        _strict_object("protocol", "operation", "revision", "outcome", "advisories", "result", properties={**common, "outcome": {"const": "ok"}, "result": {"$ref": f"#/components/schemas/{result}"}}),
        _strict_object("protocol", "operation", "revision", "outcome", "reason", "detail", "advisories", properties={**common, "outcome": {"const": "invalid"}, "reason": {"enum": ["definition", "date", "range", "era", "anchor", "overflow", "invalid_request"]}, "detail": _STRING}),
        _strict_object("protocol", "operation", "revision", "outcome", "reason", "detail", "advisories", properties={**common, "outcome": {"const": "unavailable"}, "reason": {"enum": ["no_epoch", "table_gap", "disconnected_table", "no_shared_axis", "approximate_only", "conflicting_claims", "no_chronology", "conversion_exactness"]}, "detail": _STRING}),
    ]}


_CAPABILITY_ORDER = ("generational-core-v1", "spatial-core-v1", "geometry-v1", "route-v1", "overlay-v1")
_CANONICAL_CAPABILITY_LISTS = [[]] + [list(choice) for size in range(1, len(_CAPABILITY_ORDER) + 1) for choice in combinations(_CAPABILITY_ORDER, size) if ({"generational-core-v1", "spatial-core-v1"} & set(choice)) and all("spatial-core-v1" in choice for item in ("geometry-v1", "route-v1", "overlay-v1") if item in choice)]
_SPATIAL_COMMON_FIELDS = {
    "protocol": {"const": "wedl-spatial/v1"},
    "revision": {"type": "string", "pattern": "^[0-9a-f]{40}$"},
    "capabilities": {"oneOf": [{"const": value} for value in _CANONICAL_CAPABILITY_LISTS]},
    "limit": {"type": "integer", "minimum": 1, "maximum": 100},
    "cursor": {"type": ["string", "null"], "minLength": 1, "maxLength": 2048},
}


def _spatial_request(*required: str, properties: dict[str, Any]) -> dict[str, Any]:
    return _strict_object("protocol", "revision", "capabilities", "limit", "cursor", *required, properties={**_SPATIAL_COMMON_FIELDS, **properties})


def _spatial_outcome(operation: str, result: str, state: str) -> dict[str, Any]:
    common = {"protocol": {"const": "wedl-spatial/v1"}, "operation": {"const": operation}, "revision": {"type": "string", "pattern": "^[0-9a-f]{40}$"}, "sourceSchema": _STRING, "capabilities": {"oneOf": [{"const": value} for value in _CANONICAL_CAPABILITY_LISTS]}, "cache": {"$ref": "#/components/schemas/SpatialCache"}}
    if state == "ok":
        return _strict_object("protocol", "operation", "revision", "sourceSchema", "capabilities", "cache", "state", "result", properties={**common, "state": {"const": "ok"}, "result": {"$ref": f"#/components/schemas/{result}"}})
    return _strict_object("protocol", "operation", "revision", "sourceSchema", "capabilities", "cache", "state", "code", properties={**common, "state": {"const": state}, "code": {"enum": ["SPATIAL-REQUEST-001", "SPATIAL-GEOMETRY-001", "SPATIAL-METRIC-001", "SPATIAL-PATH-001", "SPATIAL-OVERLAY-001", "SPATIAL-LIMIT-001", "SPATIAL-CURSOR-001"]}, "subreason": {"enum": ["unknown-coordinate", "unknown-metric", "incompatible-unit", "ambiguous-unit", "unavailable-edge", "closed-edge", "cross-map-discontinuity", "unreachable"]}, "detail": _STRING})


def _spatial_outcome_schemas(operation: str, result: str) -> dict[str, dict[str, Any]]:
    stem = "OverlayAsOf" if operation == "overlay-as-of" else "".join(part.capitalize() for part in operation.split("-"))
    return {f"Spatial{stem}{state.capitalize()}Outcome": _spatial_outcome(operation, result, state) for state in ("ok", "invalid", "unavailable", "forbidden", "limit")}


SCHEMAS: dict[str, dict[str, Any]] = {
    "SpatialCache": _strict_object("state", "revision", "treeOid", "sourceSchema", "fingerprint", properties={"state": {"enum": ["missing", "stale", "incompatible", "ready"]}, "revision": _STRING, "treeOid": {"type": ["string", "null"]}, "sourceSchema": _STRING, "fingerprint": {"type": ["string", "null"]}}),
    "SpatialContainmentRequest": _spatial_request("locationId", properties={"locationId": _SPATIAL_NONBLANK_STRING, "cursor": {"const": None}}),
    "SpatialChildrenRequest": _spatial_request("locationId", properties={"locationId": _SPATIAL_NONBLANK_STRING}),
    "SpatialBboxRequest": _spatial_request("mapId", "bounds", "relation", properties={"mapId": _SPATIAL_NONBLANK_STRING, "bounds": _SPATIAL_BOUNDS, "relation": {"enum": ["intersects", "within"]}}),
    "SpatialNearbyRequest": _spatial_request("position", "radius", properties={"position": _strict_object("mapId", "coordinates", properties={"mapId": _SPATIAL_NONBLANK_STRING, "coordinates": _SPATIAL_COORDINATES}), "radius": {**_SPATIAL_SAFE_NUMBER, "minimum": 0}}),
    "SpatialAdjacencyRequest": _spatial_request("locationId", properties={"locationId": _SPATIAL_NONBLANK_STRING, "cursor": {"const": None}, "modes": {"type": ["array", "null"], "minItems": 1, "maxItems": 100, "uniqueItems": True, "items": _SPATIAL_NONBLANK_STRING}}),
    "SpatialReachabilityRequest": _spatial_request("fromLocationId", properties={"fromLocationId": _SPATIAL_NONBLANK_STRING, "cursor": {"const": None}, "modes": {"type": ["array", "null"], "minItems": 1, "maxItems": 100, "uniqueItems": True, "items": _SPATIAL_NONBLANK_STRING}}),
    "SpatialPathRequest": _spatial_request("fromLocationId", "toLocationId", "metric", properties={"fromLocationId": _SPATIAL_NONBLANK_STRING, "toLocationId": _SPATIAL_NONBLANK_STRING, "metric": {"enum": ["routeDistance", "travelCost", "duration"]}, "unit": {"anyOf": [_SPATIAL_NONBLANK_STRING, {"type": "null"}]}, "modes": {"type": ["array", "null"], "minItems": 1, "maxItems": 100, "uniqueItems": True, "items": _SPATIAL_NONBLANK_STRING}, "cursor": {"const": None}}),
    "SpatialOverlayAsOfRequest": {"oneOf": [
        _spatial_request("queryScope", "locationId", "audience", "perspective", "asOf", properties={"queryScope": {"const": "location"}, "locationId": _SPATIAL_NONBLANK_STRING, "audience": _SPATIAL_NONBLANK_STRING, "perspective": _SPATIAL_NONBLANK_STRING, "asOf": _SPATIAL_TIME}),
        _spatial_request("queryScope", "locationId", "overlayId", "audience", "perspective", "asOf", properties={"queryScope": {"const": "overlay"}, "locationId": _SPATIAL_NONBLANK_STRING, "overlayId": _SPATIAL_NONBLANK_STRING, "audience": _SPATIAL_NONBLANK_STRING, "perspective": _SPATIAL_NONBLANK_STRING, "asOf": _SPATIAL_TIME}),
    ]},
    "SpatialContainmentResult": _strict_object("ids", "basis", properties={"ids": {"type": "array", "items": _STRING}, "basis": _STRING}),
    "SpatialCatalogueFilters": {"oneOf": [
        _strict_object("parentId", properties={"parentId": _NONBLANK_STRING}),
        _strict_object("mapId", "relation", properties={"mapId": _NONBLANK_STRING, "relation": {"enum": ["intersects", "within"]}}),
        _strict_object("mapId", "radius", properties={"mapId": _NONBLANK_STRING, "radius": {**_SPATIAL_SAFE_NUMBER, "minimum": 0}}),
    ]},
    # FastAPI omits null members from generated OpenAPI examples. ``modes``
    # therefore remains closed and nullable when present, but is not required;
    # ``availability`` is the invariant filter every route result carries.
    "SpatialRouteFilters": _strict_object("availability", properties={"modes": {"type": ["array", "null"], "items": _NONBLANK_STRING}, "availability": {"const": ["open"]}}),
    "SpatialOverlayFilters": _strict_object("audience", "perspective", "horizon", properties={"audience": _NONBLANK_STRING, "perspective": _NONBLANK_STRING, "horizon": _SPATIAL_TIME}),
    "SpatialCatalogueResult": _strict_object("ids", "basis", "units", "filters", "partial", "unknown", "nextCursor", properties={"ids": {"type": "array", "items": _STRING}, "basis": _STRING, "units": {"type": ["string", "null"]}, "filters": {"$ref": "#/components/schemas/SpatialCatalogueFilters"}, "partial": _BOOLEAN, "unknown": _BOOLEAN, "nextCursor": {"type": ["string", "null"]}}),
    "SpatialAdjacencyResult": _strict_object("fromLocationId", "routeIds", "portalIds", "targetLocationIds", "positionPortalIds", "filters", "basis", "partial", "unknown", properties={"fromLocationId": _STRING, "routeIds": {"type": "array", "items": _STRING}, "portalIds": {"type": "array", "items": _STRING}, "targetLocationIds": {"type": "array", "items": _STRING}, "positionPortalIds": {"type": "array", "items": _STRING}, "filters": {"$ref": "#/components/schemas/SpatialRouteFilters"}, "basis": _STRING, "partial": _BOOLEAN, "unknown": _BOOLEAN}),
    "SpatialReachabilityResult": _strict_object("fromLocationId", "ids", "expansions", "filters", "basis", "partial", "unknown", properties={"fromLocationId": _STRING, "ids": {"type": "array", "items": _STRING}, "expansions": {"type": "integer"}, "filters": {"$ref": "#/components/schemas/SpatialRouteFilters"}, "basis": _STRING, "partial": _BOOLEAN, "unknown": _BOOLEAN}),
    "SpatialPathMetric": _strict_object("metric", "computedTotal", "unit", "complete", "unknownEdges", "partial", "unknown", properties={"metric": {"enum": ["routeDistance", "travelCost", "duration"]}, "computedTotal": {"anyOf": [_SPATIAL_SAFE_NUMBER, {"type": "null"}]}, "unit": {"type": ["string", "null"]}, "complete": _BOOLEAN, "unknownEdges": {"type": "array", "items": _STRING}, "partial": _BOOLEAN, "unknown": _BOOLEAN}),
    "SpatialPathResult": _strict_object("ids", "routeIds", "metric", "expansions", "basis", "filters", "partial", "unknown", properties={"ids": {"type": "array", "items": _STRING}, "routeIds": {"type": "array", "items": _STRING}, "metric": {"$ref": "#/components/schemas/SpatialPathMetric"}, "expansions": {"type": "integer"}, "basis": _STRING, "filters": {"anyOf": [{"$ref": "#/components/schemas/SpatialRouteFilters"}, {"type": "null"}]}, "partial": _BOOLEAN, "unknown": _BOOLEAN}),
    "SpatialOverlayResult": _strict_object("ids", "storyTime", "filters", "basis", "partial", "unknown", "nextCursor", properties={"ids": {"type": "array", "items": _STRING}, "storyTime": _SPATIAL_TIME, "filters": {"$ref": "#/components/schemas/SpatialOverlayFilters"}, "basis": _STRING, "partial": _BOOLEAN, "unknown": _BOOLEAN, "nextCursor": {"type": ["string", "null"]}}),
    **_spatial_outcome_schemas("containment", "SpatialContainmentResult"),
    **_spatial_outcome_schemas("children", "SpatialCatalogueResult"),
    **_spatial_outcome_schemas("bbox", "SpatialCatalogueResult"),
    **_spatial_outcome_schemas("nearby", "SpatialCatalogueResult"),
    **_spatial_outcome_schemas("adjacency", "SpatialAdjacencyResult"),
    **_spatial_outcome_schemas("reachability", "SpatialReachabilityResult"),
    **_spatial_outcome_schemas("path", "SpatialPathResult"),
    **_spatial_outcome_schemas("overlay-as-of", "SpatialOverlayResult"),
    "ChronologyDecimalI64": _bounded_decimal_string(2 ** 63 - 1, 2 ** 63),
    "ChronologyDecimalI32": _bounded_decimal_string(2 ** 31 - 1, 2 ** 31),
    "ChronologyTagExtensions": _extension_object(),
    "ChronologyCivilDate": {"allOf": [_strict_object("kind", "calendarId", "year", properties={"kind": {"const": "civil"}, "calendarId": _CHRONOLOGY_NONBLANK_STRING, "year": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "month": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "day": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "tagExtensions": {"$ref": "#/components/schemas/ChronologyTagExtensions"}}), {"if": {"required": ["day"]}, "then": {"required": ["month"]}}]},
    "ChronologyEraDate": {"allOf": [_strict_object("kind", "eraId", "year", properties={"kind": {"const": "era"}, "eraId": _CHRONOLOGY_NONBLANK_STRING, "year": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "month": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "day": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "tagExtensions": {"$ref": "#/components/schemas/ChronologyTagExtensions"}}), {"if": {"required": ["day"]}, "then": {"required": ["month"]}}]},
    "ChronologyCivilEndpoint": {"allOf": [_strict_object("year", properties={"calendarId": _CHRONOLOGY_NONBLANK_STRING, "year": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "month": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "day": {"$ref": "#/components/schemas/ChronologyDecimalI64"}}), {"if": {"required": ["day"]}, "then": {"required": ["month"]}}]},
    "ChronologyRange": _strict_object("kind", "calendarId", "lower", "upper", properties={"kind": {"const": "range"}, "calendarId": _CHRONOLOGY_NONBLANK_STRING, "lower": {"anyOf": [{"$ref": "#/components/schemas/ChronologyCivilEndpoint"}, {"type": "null"}]}, "upper": {"anyOf": [{"$ref": "#/components/schemas/ChronologyCivilEndpoint"}, {"type": "null"}]}, "tagExtensions": {"$ref": "#/components/schemas/ChronologyTagExtensions"}}),
    "ChronologyApproximate": _strict_object("kind", "displayValue", "bounds", properties={"kind": {"const": "approximate"}, "displayValue": _STRING, "bounds": _strict_object("calendarId", "lower", "upper", properties={"calendarId": _CHRONOLOGY_NONBLANK_STRING, "lower": {"anyOf": [{"$ref": "#/components/schemas/ChronologyCivilEndpoint"}, {"type": "null"}]}, "upper": {"anyOf": [{"$ref": "#/components/schemas/ChronologyCivilEndpoint"}, {"type": "null"}]}}), "tagExtensions": {"$ref": "#/components/schemas/ChronologyTagExtensions"}}),
    "ChronologyNonConflictDateValue": {"oneOf": [{"$ref": "#/components/schemas/ChronologyCivilDate"}, {"$ref": "#/components/schemas/ChronologyEraDate"}, {"$ref": "#/components/schemas/ChronologyRange"}, {"$ref": "#/components/schemas/ChronologyApproximate"}]},
    "ChronologyConflict": _strict_object("kind", "claims", properties={"kind": {"const": "conflict"}, "claims": {"type": "array", "minItems": 2, "maxItems": 64, "items": {"$ref": "#/components/schemas/ChronologyDateValueDepth1"}}, "tagExtensions": {"$ref": "#/components/schemas/ChronologyTagExtensions"}}),
    "ChronologyDateValue": {"oneOf": [{"$ref": "#/components/schemas/ChronologyNonConflictDateValue"}, {"$ref": "#/components/schemas/ChronologyConflict"}]},
    "ChronologyFormatRequest": _strict_object("protocol", "value", properties={"protocol": {"const": "wedl-chronology/v1"}, "value": {"$ref": "#/components/schemas/ChronologyNonConflictDateValue"}}),
    "ChronologyConversionTarget": {"oneOf": [_strict_object("calendarId", properties={"calendarId": _CHRONOLOGY_NONBLANK_STRING}), _strict_object("eraId", properties={"eraId": _CHRONOLOGY_NONBLANK_STRING})]},
    "ChronologyEraFilter": _strict_object("eraId", "mode", properties={"eraId": _CHRONOLOGY_NONBLANK_STRING, "mode": {"enum": ["authored", "overlaps_bounds"]}}),
    "ChronologyConvertRequest": _strict_object("protocol", "value", "target", properties={"protocol": {"const": "wedl-chronology/v1"}, "value": {"$ref": "#/components/schemas/ChronologyNonConflictDateValue"}, "target": {"$ref": "#/components/schemas/ChronologyConversionTarget"}}),
    "ChronologySearchRequest": {"oneOf": [_strict_object("protocol", "predicate", "value", "upper", properties={"protocol": {"const": "wedl-chronology/v1"}, "predicate": {"const": "between"}, "value": {"$ref": "#/components/schemas/ChronologyDateValue"}, "upper": {"$ref": "#/components/schemas/ChronologyDateValue"}, "eraFilter": {"$ref": "#/components/schemas/ChronologyEraFilter"}, "limit": {"type": "integer", "minimum": 1, "maximum": 10000}}), _strict_object("protocol", "predicate", "value", properties={"protocol": {"const": "wedl-chronology/v1"}, "predicate": {"enum": ["on_date", "overlaps", "before", "after"]}, "value": {"$ref": "#/components/schemas/ChronologyDateValue"}, "eraFilter": {"$ref": "#/components/schemas/ChronologyEraFilter"}, "limit": {"type": "integer", "minimum": 1, "maximum": 10000}})]},
    "ChronologyStoryTimesRequest": _strict_object("protocol", "value", properties={"protocol": {"const": "wedl-chronology/v1"}, "value": {"$ref": "#/components/schemas/ChronologyDateValue"}}),
    "ChronologyCapability": _strict_object("protocol", "sourceSchema", "mode", "publicReads", "authoring", "upgradeRequired", "upgradeAvailable", "durationSemantics", properties={"protocol": {"const": "wedl-chronology/v1"}, "sourceSchema": _STRING, "mode": {"enum": ["ordinal-only", "chronology-enabled"]}, "publicReads": _BOOLEAN, "authoring": _BOOLEAN, "upgradeRequired": _BOOLEAN, "upgradeAvailable": {"const": False}, "durationSemantics": {"const": "none"}}),
    "ChronologyCatalogResponse": _strict_object("protocol", "operation", "revision", "capability", "calendars", "eras", "anchors", properties={"protocol": {"const": "wedl-chronology/v1"}, "operation": {"const": "catalog"}, "revision": _STRING, "capability": {"$ref": "#/components/schemas/ChronologyCapability"}, "calendars": {"type": "array", "items": {"$ref": "#/components/schemas/ChronologyCalendar"}}, "eras": {"type": "array", "items": {"$ref": "#/components/schemas/ChronologyEra"}}, "anchors": {"type": "array", "items": {"$ref": "#/components/schemas/ChronologyAnchor"}}}),
    "ChronologyMonth": _extension_object("number", "days", properties={"number": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "days": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "label": _STRING}),
    "ChronologyIntercalaryMonth": _extension_object("number", "days", properties={"number": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "days": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "label": _STRING}),
    "ChronologyCycleTargetOverride": _extension_object("residue", "targetMonth", "deltaDays", properties={"residue": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "targetMonth": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "deltaDays": {"$ref": "#/components/schemas/ChronologyDecimalI64"}}),
    "ChronologyCycleIntercalaryOverride": _extension_object("residue", "intercalaryMonth", properties={"residue": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "intercalaryMonth": {"$ref": "#/components/schemas/ChronologyIntercalaryMonth"}}),
    "ChronologyTableTargetOverride": _extension_object("targetMonth", "deltaDays", properties={"targetMonth": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "deltaDays": {"$ref": "#/components/schemas/ChronologyDecimalI64"}}),
    "ChronologyTableIntercalaryOverride": _extension_object("intercalaryMonth", properties={"intercalaryMonth": {"$ref": "#/components/schemas/ChronologyIntercalaryMonth"}}),
    "ChronologyCycleRule": _extension_object("kind", "period", "overrides", properties={"kind": {"const": "cycle"}, "period": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "overrides": {"type": "array", "items": {"oneOf": [{"$ref": "#/components/schemas/ChronologyCycleTargetOverride"}, {"$ref": "#/components/schemas/ChronologyCycleIntercalaryOverride"}]}}}),
    "ChronologyTableYear": _extension_object("year", "overrides", properties={"year": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "overrides": {"type": "array", "items": {"oneOf": [{"$ref": "#/components/schemas/ChronologyTableTargetOverride"}, {"$ref": "#/components/schemas/ChronologyTableIntercalaryOverride"}]}}}),
    "ChronologyTableRule": _extension_object("kind", "years", properties={"kind": {"const": "table"}, "years": {"type": "array", "items": {"$ref": "#/components/schemas/ChronologyTableYear"}}}),
    "ChronologyCalendarEpochCivil": _extension_object("year", "month", "day", properties={"year": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "month": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "day": {"$ref": "#/components/schemas/ChronologyDecimalI64"}}),
    "ChronologyCalendarEpoch": _extension_object("civil", "axisDay", properties={"civil": {"$ref": "#/components/schemas/ChronologyCalendarEpochCivil"}, "axisDay": {"$ref": "#/components/schemas/ChronologyDecimalI64"}}),
    "ChronologyCalendar": _extension_object("id", "label", "months", "rule", "epoch", "basisId", "hasEpoch", properties={"id": _CHRONOLOGY_NONBLANK_STRING, "label": _STRING, "months": {"type": "array", "items": {"$ref": "#/components/schemas/ChronologyMonth"}}, "rule": {"oneOf": [{"$ref": "#/components/schemas/ChronologyCycleRule"}, {"$ref": "#/components/schemas/ChronologyTableRule"}]}, "epoch": {"anyOf": [{"$ref": "#/components/schemas/ChronologyCalendarEpoch"}, {"type": "null"}]}, "basisId": _CHRONOLOGY_NONBLANK_STRING, "hasEpoch": _BOOLEAN}),
    "ChronologyEraEndpoint": _extension_object("year", "month", "day", properties={"year": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "month": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "day": {"$ref": "#/components/schemas/ChronologyDecimalI64"}}),
    "ChronologyEraBounds": _extension_object("lower", "upper", properties={"lower": {"$ref": "#/components/schemas/ChronologyEraEndpoint"}, "upper": {"$ref": "#/components/schemas/ChronologyEraEndpoint"}}),
    "ChronologyEraDisplayEpoch": _extension_object("displayYear", "machineYear", properties={"displayYear": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "machineYear": {"$ref": "#/components/schemas/ChronologyDecimalI64"}}),
    "ChronologyEra": _extension_object("id", "calendarId", "label", "aliases", "displayYearZero", "displayEpoch", "provenance", "basisId", properties={"id": _CHRONOLOGY_NONBLANK_STRING, "calendarId": _CHRONOLOGY_NONBLANK_STRING, "label": _STRING, "aliases": {"type": "array", "items": _STRING}, "displayYearZero": _BOOLEAN, "displayEpoch": {"$ref": "#/components/schemas/ChronologyEraDisplayEpoch"}, "bounds": {"$ref": "#/components/schemas/ChronologyEraBounds"}, "provenance": {"type": "array", "items": _NONBLANK_STRING}, "basisId": _CHRONOLOGY_NONBLANK_STRING}),
    "ChronologyAnchor": _extension_object("id", "axisDay", "storyTime", "provenance", properties={"id": _CHRONOLOGY_NONBLANK_STRING, "axisDay": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "storyTime": _extension_object("timeline", "tick", "order", properties={"timeline": _CHRONOLOGY_NONBLANK_STRING, "tick": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "order": {"$ref": "#/components/schemas/ChronologyDecimalI32"}}), "provenance": {"type": "array", "items": _NONBLANK_STRING}}),
    "ChronologyAdvisory": _strict_object("code", "message", "count", properties={"code": {"enum": ["approximate-overlap-included", "approximate-relation-excluded", "noncomparable-excluded", "result-limit"]}, "message": _STRING, "count": _INTEGER}),
    "ChronologyHit": _strict_object("recordId", "annotationId", "sourceOrdinal", "role", "display", "provenance", "valueKind", "precision", "basisId", "lowerDay", "upperDay", "relation", "value", properties={"recordId": _STRING, "annotationId": _STRING, "sourceOrdinal": _INTEGER, "role": {"type": ["string", "null"]}, "display": {"type": ["string", "null"]}, "provenance": {"type": "array", "items": _STRING}, "valueKind": _STRING, "precision": _STRING, "basisId": {"type": ["string", "null"]}, "lowerDay": {"anyOf": [{"$ref": "#/components/schemas/ChronologyDecimalI64"}, {"type": "null"}]}, "upperDay": {"anyOf": [{"$ref": "#/components/schemas/ChronologyDecimalI64"}, {"type": "null"}]}, "relation": _STRING, "value": {"$ref": "#/components/schemas/ChronologyAuthorValue"}}),
    "ChronologyFormatResult": _strict_object("value", "formatted", properties={"value": {"$ref": "#/components/schemas/ChronologyDateValue"}, "formatted": _STRING}),
    "ChronologyConversionResult": _strict_object("source", "target", "formatted", "axisDay", properties={"source": {"$ref": "#/components/schemas/ChronologyDateValue"}, "target": {"$ref": "#/components/schemas/ChronologyDateValue"}, "formatted": _STRING, "axisDay": {"$ref": "#/components/schemas/ChronologyDecimalI64"}}),
    "ChronologySearchResult": _strict_object("request", "matches", properties={"request": _strict_object("predicate", "limit", properties={"predicate": {"enum": ["on_date", "overlaps", "before", "after", "between"]}, "limit": {"type": "integer", "minimum": 1, "maximum": 10000}}), "matches": {"type": "array", "items": {"$ref": "#/components/schemas/ChronologyHit"}}}),
    "ChronologyStoryTimeMapping": {"oneOf": [_strict_object("mapping", "storyTimes", properties={"mapping": {"const": "none"}, "storyTimes": {"type": "array", "maxItems": 0}}), _strict_object("mapping", "storyTimes", properties={"mapping": {"const": "unique"}, "storyTimes": {"type": "array", "minItems": 1, "maxItems": 1, "items": {"$ref": "#/components/schemas/ChronologyAnchor/properties/storyTime"}}}), _strict_object("mapping", "storyTimes", properties={"mapping": {"const": "ambiguous"}, "storyTimes": {"type": "array", "minItems": 2, "items": {"$ref": "#/components/schemas/ChronologyAnchor/properties/storyTime"}}})]},
    "ChronologyOutcome": _object("protocol", "operation", "revision", "outcome", "advisories", properties={"protocol": {"const": "wedl-chronology/v1"}, "operation": _STRING, "revision": _STRING, "outcome": {"enum": ["ok", "invalid", "unavailable"]}, "advisories": {"type": "array", "items": {"$ref": "#/components/schemas/ChronologyAdvisory"}}}),
    "ChronologyFormatOutcome": _chronology_outcome("format", "ChronologyFormatResult"),
    "ChronologyConvertOutcome": _chronology_outcome("convert", "ChronologyConversionResult"),
    "ChronologySearchOutcome": _chronology_outcome("search", "ChronologySearchResult"),
    "ChronologyStoryTimesOutcome": _chronology_outcome("story-times", "ChronologyStoryTimeMapping"),
    "ChronologyRelativeValue": {"allOf": [_extension_object("kind", "relation", properties={"kind": {"const": "relative"}, "relation": _NONBLANK_STRING, "beforeId": _CHRONOLOGY_NONBLANK_STRING, "afterId": _CHRONOLOGY_NONBLANK_STRING, "tagExtensions": {"$ref": "#/components/schemas/ChronologyTagExtensions"}}), {"anyOf": [{"required": ["beforeId"]}, {"required": ["afterId"]}]}]},
    "ChronologyDurationValue": _extension_object("kind", "unit", "value", properties={"kind": {"const": "duration"}, "unit": {"enum": ["year", "month", "day"]}, "value": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "tagExtensions": {"$ref": "#/components/schemas/ChronologyTagExtensions"}}),
    "ChronologyAuthorCivilDate": {"allOf": [_extension_object("kind", "calendarId", "year", properties={"kind": {"const": "civil"}, "calendarId": _CHRONOLOGY_NONBLANK_STRING, "year": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "month": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "day": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "tagExtensions": {"$ref": "#/components/schemas/ChronologyTagExtensions"}}), {"if": {"required": ["day"]}, "then": {"required": ["month"]}}]},
    "ChronologyAuthorEraDate": {"allOf": [_extension_object("kind", "eraId", "year", properties={"kind": {"const": "era"}, "eraId": _CHRONOLOGY_NONBLANK_STRING, "year": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "month": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "day": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "tagExtensions": {"$ref": "#/components/schemas/ChronologyTagExtensions"}}), {"if": {"required": ["day"]}, "then": {"required": ["month"]}}]},
    "ChronologyAuthorCivilEndpoint": {"allOf": [_extension_object("year", properties={"calendarId": _CHRONOLOGY_NONBLANK_STRING, "year": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "month": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "day": {"$ref": "#/components/schemas/ChronologyDecimalI64"}}), {"if": {"required": ["day"]}, "then": {"required": ["month"]}}]},
    "ChronologyAuthorRange": _extension_object("kind", "calendarId", "lower", "upper", properties={"kind": {"const": "range"}, "calendarId": _CHRONOLOGY_NONBLANK_STRING, "lower": {"anyOf": [{"$ref": "#/components/schemas/ChronologyAuthorCivilEndpoint"}, {"type": "null"}]}, "upper": {"anyOf": [{"$ref": "#/components/schemas/ChronologyAuthorCivilEndpoint"}, {"type": "null"}]}, "tagExtensions": {"$ref": "#/components/schemas/ChronologyTagExtensions"}}),
    "ChronologyAuthorApproximateBounds": {"oneOf": [
        {"allOf": [
            _extension_object("calendarId", "lower", "upper", properties={"calendarId": _CHRONOLOGY_NONBLANK_STRING, "lower": {"anyOf": [{"$ref": "#/components/schemas/ChronologyAuthorCivilEndpoint"}, {"type": "null"}]}, "upper": {"anyOf": [{"$ref": "#/components/schemas/ChronologyAuthorCivilEndpoint"}, {"type": "null"}]}}),
            {"anyOf": [{"properties": {"lower": {"$ref": "#/components/schemas/ChronologyAuthorCivilEndpoint"}}}, {"properties": {"upper": {"$ref": "#/components/schemas/ChronologyAuthorCivilEndpoint"}}}]},
        ]},
        _extension_object("lower", "upper", properties={"lower": {"type": "null"}, "upper": {"type": "null"}}),
    ]},
    "ChronologyAuthorApproximate": _extension_object("kind", "displayValue", "bounds", properties={"kind": {"const": "approximate"}, "displayValue": _STRING, "bounds": {"$ref": "#/components/schemas/ChronologyAuthorApproximateBounds"}, "tagExtensions": {"$ref": "#/components/schemas/ChronologyTagExtensions"}}),
    "ChronologyAuthorNonConflictValue": {"oneOf": [{"$ref": "#/components/schemas/ChronologyAuthorCivilDate"}, {"$ref": "#/components/schemas/ChronologyAuthorEraDate"}, {"$ref": "#/components/schemas/ChronologyAuthorRange"}, {"$ref": "#/components/schemas/ChronologyAuthorApproximate"}, {"$ref": "#/components/schemas/ChronologyRelativeValue"}, {"$ref": "#/components/schemas/ChronologyDurationValue"}]},
    "ChronologyAuthorConflict": _extension_object("kind", "claims", properties={"kind": {"const": "conflict"}, "claims": {"type": "array", "minItems": 2, "maxItems": 64, "items": {"$ref": "#/components/schemas/ChronologyAuthorValueDepth1"}}, "tagExtensions": {"$ref": "#/components/schemas/ChronologyTagExtensions"}}),
    "ChronologyAuthorValue": {"oneOf": [{"$ref": "#/components/schemas/ChronologyAuthorNonConflictValue"}, {"$ref": "#/components/schemas/ChronologyAuthorConflict"}]},
    "ChronologyAnnotation": _identified_author_item("chronology", "provenance", "value", properties={"role": _STRING, "display": _STRING, "provenance": {"type": "array", "items": _STRING}, "value": {"$ref": "#/components/schemas/ChronologyAuthorValue"}}),
    "ChronologyRecordReplacement": _strict_object("record", "annotations", properties={"record": _CHRONOLOGY_NONBLANK_STRING, "annotations": {"type": "array", "items": {"$ref": "#/components/schemas/ChronologyAnnotation"}}}),
    "ChronologyAuthorCalendar": _identified_author_item("calendar", "label", "months", "rule", "epoch", properties={"label": _STRING, "months": {"type": "array", "items": {"$ref": "#/components/schemas/ChronologyMonth"}}, "rule": {"oneOf": [{"$ref": "#/components/schemas/ChronologyCycleRule"}, {"$ref": "#/components/schemas/ChronologyTableRule"}]}, "epoch": {"anyOf": [{"$ref": "#/components/schemas/ChronologyCalendarEpoch"}, {"type": "null"}]}}),
    "ChronologyAuthorEra": _identified_author_item("era", "calendarId", "label", "aliases", "displayYearZero", "displayEpoch", "provenance", properties={"calendarId": _CHRONOLOGY_NONBLANK_STRING, "label": _STRING, "aliases": {"type": "array", "items": _STRING}, "displayYearZero": _BOOLEAN, "displayEpoch": {"$ref": "#/components/schemas/ChronologyEraDisplayEpoch"}, "bounds": {"$ref": "#/components/schemas/ChronologyEraBounds"}, "provenance": {"type": "array", "items": _NONBLANK_STRING}}),
    "ChronologyAuthorAnchor": _identified_author_item("chronology", "axisDay", "storyTime", "provenance", properties={"axisDay": {"$ref": "#/components/schemas/ChronologyDecimalI64"}, "storyTime": {"$ref": "#/components/schemas/ChronologyAnchor/properties/storyTime"}, "provenance": {"type": "array", "items": _NONBLANK_STRING}}),
    "ChronologyCatalogReplacement": _strict_object("calendars", "eras", "anchors", properties={"calendars": {"type": "array", "maxItems": 500, "items": {"$ref": "#/components/schemas/ChronologyAuthorCalendar"}}, "eras": {"type": "array", "maxItems": 500, "items": {"$ref": "#/components/schemas/ChronologyAuthorEra"}}, "anchors": {"type": "array", "maxItems": 500, "items": {"$ref": "#/components/schemas/ChronologyAuthorAnchor"}}}),
    "ChronologyChange": {"allOf": [_strict_object(properties={"catalog": {"$ref": "#/components/schemas/ChronologyCatalogReplacement"}, "records": {"type": "array", "minItems": 1, "items": {"$ref": "#/components/schemas/ChronologyRecordReplacement"}}}), {"anyOf": [{"required": ["catalog"]}, {"required": ["records"]}]}]},
    "ChronologyAuthoringRequest": _strict_object("action", "expectedHead", "change", properties={"action": {"const": "chronology.replace"}, "expectedHead": {"type": "string", "pattern": "^[0-9a-f]{40}$"}, "change": {"$ref": "#/components/schemas/ChronologyChange"}, "summary": _STRING, "idempotencyKey": _STRING}),
    "ChangesetRequest": _CHANGESET_REQUEST,
    "AuthoringRequest": _AUTHORING_REQUEST,
    "AuthorImpact": _AUTHOR_IMPACT,
    "AuthoringPreviewResponse": _object("protocol", "intent", "changeset", "preview", "authorImpact", properties={"protocol": {"const": "wedl-author-preview/v1"}, "intent": {"$ref": "#/components/schemas/AuthoringRequest"}, "changeset": {"$ref": "#/components/schemas/ChangesetRequest"}, "preview": {"$ref": "#/components/schemas/ChangesetPreviewResponse"}, "authorImpact": {"$ref": "#/components/schemas/AuthorImpact"}}),
    "WedlEntity": _object("id", "kind", "title", properties={"id": _STRING, "kind": {"enum": ["world", "character", "knowledge", "event", "object", "environment", "location", "relationship", "story-point", "scene", "conversation"]}, "title": _STRING}),
    "StatusResponse": _object("repositoryRoot", "revision", "recordCount", "counts", "activeSceneId", "activeScenes", "currentTime", "cacheReadiness", "chronologyCapability", properties={"repositoryRoot": _STRING, "revision": _STRING, "treeOid": _STRING, "recordCount": _INTEGER, "counts": _OBJECT, "activeSceneId": {"type": ["string", "null"]}, "activeScenes": {"type": "array", "items": _object("id", "title", properties={"id": _STRING, "title": _STRING})}, "currentTime": {"anyOf": [_STORY_TIME, {"type": "null"}]}, "cacheReadiness": _OBJECT, "chronologyCapability": {"$ref": "#/components/schemas/ChronologyCapability"}}),
    "ValidationReportResponse": _object("valid", "revision", "recordCount", "diagnostics", properties={"valid": _BOOLEAN, "revision": _STRING, "recordCount": _INTEGER, "diagnostics": {"type": "array", "items": _OBJECT}}),
    "CompileResponse": _object("status", "revision", properties={"status": _STRING, "revision": _STRING, "treeOid": _STRING, "recordCount": _INTEGER, "timingsMs": _OBJECT}),
    "EntityListResponse": {"type": "array", "items": {"$ref": "#/components/schemas/WedlEntity"}},
    "EntityResponse": _strict_object("id", "kind", "title", "chronologyAnnotations", properties={"id": _STRING, "kind": _STRING, "title": _STRING, "domain": _STRING, "status": _STRING, "source_path": _STRING, "blob_oid": {"type": ["string", "null"]}, "bodyMarkdown": _STRING, "frontmatter": _OBJECT, "locationContext": _LOCATION_CONTEXT, "inboundReferences": {"type": "array", "items": _TIMELINE_REFERENCE}, "chronologyAnnotations": {"type": "array", "items": {"$ref": "#/components/schemas/ChronologyAnnotation"}}}),
    "StateResponse": _object("revision", "entityId", "at", "state", "citations", "locationHistory", properties={"revision": _STRING, "entityId": _STRING, "at": _OBJECT, "state": _OBJECT, "citations": _OBJECT, "locationHistory": {"type": "array", "items": _LOCATION_HISTORY_ITEM}}),
    "WhereaboutsJourney": _WHEREABOUTS_JOURNEY,
    "ImportanceBreakdown": _IMPORTANCE_BREAKDOWN,
    "ImportancePolicy": _IMPORTANCE_POLICY,
    "WhereaboutsCharacter": _WHEREABOUTS_CHARACTER,
    "WhereaboutsLocation": _WHEREABOUTS_LOCATION,
    "WhereaboutsActiveScene": _WHEREABOUTS_ACTIVE_SCENE,
    "WhereaboutsResponse": _object("protocol", "revision", "effectiveTime", "timeScope", "characterPolicy", "importancePolicy", "characters", "locations", "activeScenes", "offstageCharacters", "unlocatedCharacters", properties={
        "protocol": {"const": "wedl-whereabouts/v2"}, "revision": _STRING,
        "effectiveTime": _TIMELINE_COORDINATE, "timeScope": _object("mode", "at", properties={"mode": {"const": "as-of"}, "at": _TIMELINE_COORDINATE}),
        "characterPolicy": _object("includedStatuses", "excludedStatuses", "locationEvidence", "inference", properties={"includedStatuses": {"type": "array", "items": _STRING}, "excludedStatuses": {"type": "array", "items": _STRING}, "locationEvidence": _STRING, "inference": {"const": "none"}}),
        "importancePolicy": _IMPORTANCE_POLICY,
        "characterFilter": _TIMELINE_NULLABLE_REFERENCE,
        "characters": {"type": "array", "items": _WHEREABOUTS_CHARACTER},
        "locations": {"type": "array", "items": _WHEREABOUTS_LOCATION},
        "activeScenes": {"type": "array", "items": _WHEREABOUTS_ACTIVE_SCENE},
        "offstageCharacters": {"type": "array", "items": _TIMELINE_REFERENCE},
        "unlocatedCharacters": {"type": "array", "items": _TIMELINE_REFERENCE},
    }),
    "HypothesisReference": _HYPOTHESIS_REFERENCE,
    "Hypothesis": _HYPOTHESIS,
    "HypothesesResponse": _strict_object("protocol", "revision", "nonCanonical", properties={"protocol": {"const": "wedl-hypotheses/v1"}, "revision": _STRING, "nonCanonical": {"const": True}, "hypotheses": {"type": "array", "items": _HYPOTHESIS}, "hypothesis": _HYPOTHESIS}),
    "KnowledgeResponse": _object("revision", "characterId", "at", "knowledge", properties={"revision": _STRING, "characterId": _STRING, "at": _OBJECT, "knowledge": {"type": "array", "items": _OBJECT}}),
    "InteractionsResponse": _object("revision", "firstCharacterId", "secondCharacterId", "interactions", properties={"revision": _STRING, "firstCharacterId": _STRING, "secondCharacterId": _STRING, "interactions": {"type": "array", "items": _OBJECT}}),
    "PlotTrail": _PLOT_TRAIL,
    "ContinuityAdvisory": _CONTINUITY_ADVISORY,
    "StoryPointView": _STORY_POINT_VIEW,
    "CausalityNode": _CAUSALITY_NODE,
    "CausalityEdge": _CAUSALITY_EDGE,
    "StoryPointsResponse": _object("revision", "sceneId", "evaluatedAt", "storyPoints", "continuityAdvisories", properties={"revision": _STRING, "sceneId": {"type": ["string", "null"]}, "evaluatedAt": _STORY_TIME, "storyPoints": {"type": "array", "items": _STORY_POINT_VIEW}, "continuityAdvisories": {"type": "array", "items": _CONTINUITY_ADVISORY}}),
    "TimelineCoordinate": _TIMELINE_COORDINATE,
    "TimelineOrigin": _TIMELINE_ORIGIN,
    "TimelineReference": _TIMELINE_REFERENCE,
    "TimelineParticipant": _TIMELINE_PARTICIPANT,
    "TimelinePoint": _TIMELINE_POINT,
    "TimelineSpan": _TIMELINE_SPAN,
    "TimelineResponse": _object("protocol", "revision", "timeline", "temporalSemantics", "points", "spans", properties={"protocol": {"const": "wedl-timeline/v1"}, "revision": _STRING, "timeline": _object("id", "label", properties={"id": _STRING, "label": _STRING, "origin": _TIMELINE_ORIGIN}), "temporalSemantics": _object("spacing", "durationSemantics", "intervalEndpoints", properties={"spacing": {"const": "ordinal"}, "durationSemantics": {"const": "none"}, "intervalEndpoints": {"const": "inclusive"}}), "points": {"type": "array", "items": _TIMELINE_POINT}, "spans": {"type": "array", "items": _TIMELINE_SPAN}}),
    "ThreadCatalogEntry": _strict_object("id", "label", properties={"id": _STRING, "label": _STRING}),
    "ThreadCatalogResponse": _strict_object("protocol", "revision", "sourceSchema", "groupingAvailable", "threads", properties={"protocol": {"const": "wedl-threads/v1"}, "revision": _STRING, "sourceSchema": _STRING, "groupingAvailable": _BOOLEAN, "threads": {"type": "array", "items": {"$ref": "#/components/schemas/ThreadCatalogEntry"}}}),
    "ThreadMembershipEntry": _strict_object("recordId", "threadIds", properties={"recordId": _STRING, "threadIds": {"type": "array", "items": _STRING}}),
    "ThreadMembershipResponse": _strict_object("protocol", "revision", "sourceSchema", "selectedThreadIds", "records", properties={"protocol": {"const": "wedl-thread-memberships/v1"}, "revision": _STRING, "sourceSchema": _STRING, "selectedThreadIds": {"type": "array", "items": _STRING}, "records": {"type": "array", "items": {"$ref": "#/components/schemas/ThreadMembershipEntry"}}}),
    "CausalityResponse": _object("protocol", "revision", "direction", "effectiveTime", "timeScope", "focusEvent", "nodes", "edges", "continuityAdvisories", properties={"protocol": {"const": "wedl-causality/v1"}, "revision": _STRING, "direction": {"enum": ["upstream", "downstream", "both"]}, "effectiveTime": _STORY_TIME, "timeScope": _OBJECT, "focusEvent": _TIMELINE_REFERENCE, "nodes": {"type": "array", "items": _CAUSALITY_NODE}, "edges": {"type": "array", "items": _CAUSALITY_EDGE}, "continuityAdvisories": {"type": "array", "items": _CONTINUITY_ADVISORY}}),
    "SearchResponse": _object("protocol", "revision", "perspective", "characterId", "sceneId", "effectiveTime", "timeScope", "mode", "searchState", "results", properties={"protocol": _STRING, "revision": _STRING, "perspective": _STRING, "characterId": {"type": ["string", "null"]}, "sceneId": {"type": ["string", "null"]}, "effectiveTime": {"type": ["object", "null"]}, "timeScope": _OBJECT, "mode": _STRING, "searchState": _OBJECT, "results": {"type": "array", "items": _OBJECT}}),
    "ContextResponse": {
        "oneOf": [
            _object("protocol", "revision", "perspective", "characterId", "sceneId", "effectiveTime", "focus", "promptText", "selection", properties={"protocol": _STRING, "revision": _STRING, "perspective": {"const": "author"}, "characterId": _STRING, "sceneId": _STRING, "effectiveTime": _OBJECT, "focus": _OBJECT, "promptText": _STRING, "selection": _OBJECT}),
            _object("protocol", "revision", "perspective", "characterId", "sceneId", "effectiveTime", "focus", "promptText", "selection", properties={"protocol": _STRING, "revision": _STRING, "perspective": {"const": "character"}, "characterId": _STRING, "sceneId": _STRING, "effectiveTime": _OBJECT, "focus": _OBJECT, "promptText": _STRING, "selection": _OBJECT}),
            _object("protocol", "revision", "perspective", "characterId", "sceneId", "effectiveTime", "focus", "characterPrompt", "authorMargin", "selection", properties={"protocol": _STRING, "revision": _STRING, "perspective": {"const": "dramatic-irony"}, "characterId": _STRING, "sceneId": _STRING, "effectiveTime": _OBJECT, "focus": _OBJECT, "characterPrompt": _STRING, "authorMargin": _STRING, "selection": _OBJECT}),
        ]
    },
    "SpeechBeat": _SPEECH_BEAT,
    "ActionBeat": _ACTION_BEAT,
    "ConversationBeat": _CONVERSATION_BEAT,
    "ConversationResponse": {"oneOf": [_object("protocol", "revision", "perspective", "conversationId", "title", "conversationTime", "effectiveTime", "timeScope", "verbatimTurns", "beats", "recollections", properties={"protocol": _STRING, "revision": _STRING, "perspective": {"const": "author"}, "conversationId": _STRING, "title": _STRING, "conversationTime": _OBJECT, "effectiveTime": {"type": ["object", "null"]}, "timeScope": _OBJECT, "verbatimTurns": {"type": "array", "items": _SPEECH_BEAT}, "beats": {"type": "array", "items": _CONVERSATION_BEAT}, "recollections": {"type": "array", "items": _OBJECT}}), _object("protocol", "revision", "perspective", "conversationId", "title", "characterId", "conversationTime", "effectiveTime", "timeScope", "heardVerbatimTurns", "perceivedBeats", "subjectiveRecollection", "provenanceBoundary", properties={"protocol": _STRING, "revision": _STRING, "perspective": {"const": "character"}, "conversationId": _STRING, "title": _STRING, "characterId": _STRING, "conversationTime": _OBJECT, "effectiveTime": _OBJECT, "timeScope": _OBJECT, "heardVerbatimTurns": {"type": "array", "items": _SPEECH_BEAT}, "perceivedBeats": {"type": "array", "items": _CONVERSATION_BEAT}, "subjectiveRecollection": {"type": ["object", "null"]}, "provenanceBoundary": _STRING})]},
    "ChangesetSchemaResponse": _object("protocol", "expectedHead", "idempotencyKey", "summary", "operations", properties={"protocol": _STRING, "expectedHead": _STRING, "idempotencyKey": _STRING, "summary": _STRING, "operations": {"type": "array", "items": _OBJECT}}),
    "ChangesetSchemaDocument": _object("protocol", "changesetProtocol", "required", "operations", properties={"protocol": {"const": "wedl-changeset-schema/v1"}, "changesetProtocol": {"const": "wedl-changeset/v1"}, "required": {"type": "array", "items": _STRING}, "operations": {"type": "array", "items": _OBJECT}}),
    "ChangesetPreviewResponse": {
        "oneOf": [
            _object(
                "protocol", "valid", "expectedHead", "requestHash", "confirmationToken",
                "generatedIds", "touchedEntityIds", "diagnostics", "files", "diff",
                properties={
                    "protocol": {"const": "wedl-preview/v1"}, "valid": {"const": True},
                    "expectedHead": _STRING, "requestHash": _STRING, "confirmationToken": _STRING,
                    "generatedIds": _OBJECT, "touchedEntityIds": {"type": "array", "items": _STRING},
                    "diagnostics": {"type": "array", "items": _OBJECT},
                    "files": {"type": "array", "items": _STRING}, "diff": _STRING,
                },
            ),
            _object(
                "protocol", "valid", "expectedHead", "requestHash", "confirmationToken",
                "generatedIds", "touchedEntityIds", "diagnostics", "files", "diff",
                properties={
                    "protocol": {"const": "wedl-preview/v1"}, "valid": {"const": False},
                    "expectedHead": _STRING, "requestHash": _STRING, "confirmationToken": _STRING,
                    "generatedIds": _OBJECT, "touchedEntityIds": {"type": "array", "items": _STRING},
                    "diagnostics": {"type": "array", "items": _OBJECT},
                    "files": {"type": "array", "items": _STRING}, "diff": _STRING,
                },
            ),
        ]
    },
    "ChangesetApplyResponse": _object("protocol", "status", "previousHead", "newHead", "generatedIds", "touchedEntityIds", "compile", "idempotentReplay", properties={"protocol": _STRING, "status": _STRING, "previousHead": _STRING, "newHead": _STRING, "generatedIds": _OBJECT, "touchedEntityIds": {"type": "array", "items": _STRING}, "compile": {"$ref": "#/components/schemas/CompileResponse"}, "idempotentReplay": _BOOLEAN}),
    "AuthoringApplyResponse": _object("protocol", "status", "previousHead", "newHead", "generatedIds", "touchedEntityIds", "compile", "idempotentReplay", "authorImpact", properties={"protocol": _STRING, "status": _STRING, "previousHead": _STRING, "newHead": _STRING, "generatedIds": _OBJECT, "touchedEntityIds": {"type": "array", "items": _STRING}, "compile": {"$ref": "#/components/schemas/CompileResponse"}, "idempotentReplay": _BOOLEAN, "authorImpact": {"$ref": "#/components/schemas/AuthorImpact"}}),
    "SessionResponse": _object("token", "head", properties={"token": _STRING, "head": _STRING}),
    "WorkspaceHtmlResponse": {"type": "string"},
}


# JSON Schema recursive references cannot express a maximum nesting depth on
# their own.  Materialize the source grammar's 64 conflict levels so clients
# get the same bounded contract before the runtime decoder is involved.
SCHEMAS["ChronologyConflict"]["description"] = "A conflict may nest through at most 64 conflict objects."
SCHEMAS["ChronologyConflict"]["x-wedl-max-conflict-depth"] = 64
for _depth in range(64, 0, -1):
    _branches: list[dict[str, Any]] = [{"$ref": "#/components/schemas/ChronologyNonConflictDateValue"}]
    if _depth < 64:
        _conflict_name = f"ChronologyConflictDepth{_depth}"
        SCHEMAS[_conflict_name] = _strict_object(
            "kind", "claims",
            properties={
                "kind": {"const": "conflict"},
                "claims": {"type": "array", "minItems": 2, "maxItems": 64, "items": {"$ref": f"#/components/schemas/ChronologyDateValueDepth{_depth + 1}"}},
                "tagExtensions": {"$ref": "#/components/schemas/ChronologyTagExtensions"},
            },
        )
        SCHEMAS[_conflict_name]["description"] = "Bounded nested conflict value."
        _branches.append({"$ref": f"#/components/schemas/{_conflict_name}"})
    SCHEMAS[f"ChronologyDateValueDepth{_depth}"] = {"oneOf": _branches}

SCHEMAS["ChronologyAuthorConflict"]["description"] = "A conflict may nest through at most 64 conflict objects."
SCHEMAS["ChronologyAuthorConflict"]["x-wedl-max-conflict-depth"] = 64
for _depth in range(64, 0, -1):
    _branches = [{"$ref": "#/components/schemas/ChronologyAuthorNonConflictValue"}]
    if _depth < 64:
        _conflict_name = f"ChronologyAuthorConflictDepth{_depth}"
        SCHEMAS[_conflict_name] = _extension_object(
            "kind", "claims",
            properties={
                "kind": {"const": "conflict"},
                "claims": {"type": "array", "minItems": 2, "maxItems": 64, "items": {"$ref": f"#/components/schemas/ChronologyAuthorValueDepth{_depth + 1}"}},
                "tagExtensions": {"$ref": "#/components/schemas/ChronologyTagExtensions"},
            },
        )
        SCHEMAS[_conflict_name]["description"] = "Bounded nested authoring conflict value."
        _branches.append({"$ref": f"#/components/schemas/{_conflict_name}"})
    SCHEMAS[f"ChronologyAuthorValueDepth{_depth}"] = {"oneOf": _branches}


SCHEMAS["GenerationalAuthoringRequest"] = _GEN_AUTHORING_REQUEST
SCHEMAS["GenerationalScaffoldResponse"] = _GEN_AUTHORING_REQUEST
_GEN_SCHEMA_FIELD = _strict_object("required", "optional", "transitions", properties={
    key: {"type": "array", "uniqueItems": True, "items": _GEN_REF}
    for key in ("required", "optional", "transitions")})
_GEN_SCHEMA_FIELDS = _strict_object(*_GEN_FIELD_SCHEMAS, properties={
    kind: _GEN_SCHEMA_FIELD for kind in _GEN_FIELD_SCHEMAS})
SCHEMAS["GenerationalSchemaResponse"] = _strict_object(
    "protocol", "sourceSchema", "kinds", "variants", "fields", "confirmation", "batchLimit",
    properties={"protocol": {"const": "wedl-generational-authoring-schema/v1"},
                "sourceSchema": {"const": "wedl/v0.7"}, "kinds": {"type": "array", "items": _STRING},
                "variants": {"type": "array", "items": _STRING},
                "fields": _GEN_SCHEMA_FIELDS, "confirmation": _STRING,
                "batchLimit": {"const": 32}},
)
_GEN_READ_ACTIONS = ("parents", "ancestors", "descendants", "relatives", "union",
                     "organization", "legacy", "vital", "search", "context")
_GEN_APPLICABILITY = {"oneOf": [
    _strict_object("applicability_kind", "point", properties={
        "applicability_kind": {"const": "instant"}, "point": _GEN_TIME}),
    _strict_object("applicability_kind", "first", "last", properties={
        "applicability_kind": {"const": "inclusive-interval"},
        "first": _GEN_TIME, "last": _GEN_TIME}),
]}
_GEN_CITATION = _strict_object("record_id", "path", "applicability", properties={
    "record_id": _GEN_REF, "path": _GEN_REF, "applicability": _GEN_APPLICABILITY})
_GEN_CITATIONS = {"type": "array", "minItems": 1, "items": _GEN_CITATION}
_GEN_HISTORY_PAYLOAD = _strict_object(properties={
    "title": _GEN_REF, "aliases": _GEN_WORDS, "parent_id": {"type": ["string", "null"]},
    "basis": {"enum": ["biological", "adoptive", "legal", "de-facto"]},
    "participant_ids": {"type": "array", "items": _GEN_REF},
    "role": {"type": ["string", "null"]},
    "holder_id": {"type": ["string", "null"]},
    "from_tenure_id": _GEN_REF, "to_tenure_id": _GEN_REF,
    "competes_with": {"type": "array", "items": _GEN_REF},
})
_GEN_HISTORY = _strict_object("transitionId", "kind", "payload", "citation", properties={
    "transitionId": _GEN_REF, "kind": _GEN_REF, "payload": _GEN_HISTORY_PAYLOAD,
    "causeEventId": _GEN_REF, "citation": _GEN_CITATION})
_GEN_FOLD_COMMON = {"recordId": _GEN_REF, "state": _GEN_REF,
                    "citations": _GEN_CITATIONS,
                    "causes": {"type": "array", "items": _strict_object(
                        "eventId", "citation", properties={
                            "eventId": _GEN_REF, "citation": _GEN_CITATION})},
                    "history": {"type": "array", "items": _GEN_HISTORY}}
_GEN_FOLD_VALUES = {
    "organization": {"organization_kind": _GEN_REF, "parent_id": {"type": ["string", "null"]},
                     "location_id": {"type": ["string", "null"]}, "title": _GEN_REF,
                     "aliases": _GEN_WORDS},
    "parentage": {"child_id": _GEN_REF, "parent_id": _GEN_REF,
                   "timeline": _GEN_REF, "start_tick": _INTEGER,
                   "start_order": _INTEGER, "source_ordinal": _INTEGER,
                   "basis": {"enum": ["biological", "adoptive"]}},
    "union": {"participant_ids": {"type": "array", "items": _GEN_REF}},
    "affiliation": {"character_id": _GEN_REF, "organization_id": _GEN_REF,
                    "role": {"type": ["string", "null"]}},
    "legacy": {"legacy_kind": _GEN_REF, "organization_id": {"type": ["string", "null"]},
               "title": _GEN_REF, "aliases": _GEN_WORDS},
    "tenure": {"legacy_id": _GEN_REF, "predecessor_tenure_id": {"type": ["string", "null"]},
               "successor_tenure_id": {"type": ["string", "null"]},
               "holder_id": {"type": ["string", "null"]},
               "basis": {"enum": ["legal", "de-facto"]},
               "from_tenure_id": _GEN_REF, "to_tenure_id": _GEN_REF},
    "claim": {"legacy_id": _GEN_REF, "claimant_id": _GEN_REF,
              "competes_with": {"type": "array", "items": _GEN_REF}},
    "vital-history": {"character_id": _GEN_REF,
                      "disclosure": {"enum": ["known", "unknown", "withheld"]}},
}
_GEN_FOLD_REQUIRED = {
    "organization": ("organization_kind",),
    "parentage": ("child_id", "parent_id", "timeline", "start_tick", "start_order",
                   "source_ordinal", "basis"),
    "union": ("participant_ids",),
    "affiliation": ("character_id", "organization_id", "role"),
    "legacy": ("legacy_kind",),
    "tenure": ("legacy_id", "holder_id", "basis"),
    "claim": ("legacy_id", "claimant_id", "competes_with"),
    "vital-history": ("character_id", "disclosure"),
}
_GEN_FOLD = {"oneOf": [_strict_object(
    "recordId", "kind", "state", "value", "citations", "causes",
    properties={**_GEN_FOLD_COMMON, "kind": {"const": kind},
                "value": _strict_object(*_GEN_FOLD_REQUIRED[kind], properties=fields)})
    for kind, fields in _GEN_FOLD_VALUES.items()]}
_GEN_EDGE = _strict_object("from", "to", "recordId", "citations", properties={
    "from": _GEN_REF, "to": _GEN_REF, "recordId": _GEN_REF,
    "citations": _GEN_CITATIONS})
_GEN_PATH = _strict_object("targetId", "edges", properties={
    "targetId": _GEN_REF, "edges": {"type": "array", "items": _GEN_EDGE}})
_GEN_RELATION = _strict_object("targetId", "generationDistance", "label", "edges", properties={
    "targetId": _GEN_REF, "generationDistance": {"type": "integer", "minimum": 0},
    "label": {"enum": ["ancestor", "descendant", "relative-path"]},
    "edges": {"type": "array", "items": _GEN_EDGE}})
_GEN_PARENT = _strict_object("targetId", "label", "recordId", "citations", properties={
    "targetId": _GEN_REF, "label": {"enum": ["biological-parent", "adoptive-parent"]},
    "recordId": _GEN_REF, "citations": _GEN_CITATIONS,
    "history": {"type": "array", "items": _GEN_HISTORY}})
_GEN_SUCCESSION = _strict_object("from", "to", "citations", "causes", properties={
    "from": _GEN_REF, "to": _GEN_REF, "citations": _GEN_CITATIONS,
    "causes": _GEN_FOLD_COMMON["causes"]})
_GEN_SEARCH_RESULT = _strict_object("recordId", "kind", "citations", properties={
    "recordId": _GEN_REF, "kind": {"enum": list(_GEN_FOLD_VALUES)},
    "citations": _GEN_CITATIONS})
_GEN_READ_RESULTS = {
    "parents": {"relations": {"type": "array", "items": _GEN_PARENT}},
    "ancestors": {"relations": {"type": "array", "items": _GEN_RELATION}},
    "descendants": {"relations": {"type": "array", "items": _GEN_RELATION}},
    "relatives": {"relations": {"type": "array", "items": _GEN_RELATION}},
    "union": {"participants": {"type": "array", "items": _STRING},
              "citations": _GEN_CITATIONS,
              "history": {"type": "array", "items": _GEN_HISTORY}},
    "organization": {"organization": _GEN_FOLD, "parentPath": {"type": "array", "items": _GEN_PATH},
                     "roles": {"type": "array", "items": _GEN_FOLD}},
    "legacy": {"legacy": _GEN_FOLD, "tenures": {"type": "array", "items": _GEN_FOLD},
               "holders": {"type": "array", "items": _GEN_FOLD},
               "claims": {"type": "array", "items": _GEN_FOLD},
               "succession": {"type": "array", "items": _GEN_SUCCESSION}},
    "vital": {"vital": {"enum": ["living", "dead", "existing", "ended"]},
              "citations": _GEN_CITATIONS,
              "history": {"type": "array", "items": _GEN_HISTORY}},
    "search": {"results": {"type": "array", "items": _GEN_SEARCH_RESULT},
               "cursor": {"type": ["string", "null"]}},
    "context": {"items": {"type": "array", "items": {"oneOf": [
        _strict_object("kind", "result", properties={
            "kind": {"const": kind},
            "result": _strict_object("state", *required, properties={
                "state": {"const": "available"}, **result})})
        for kind, required, result in (
            ("parents", ("relations",), {"relations": {"type": "array", "items": _GEN_PARENT}}),
            ("ancestors", ("relations",), {"relations": {"type": "array", "items": _GEN_RELATION}}),
            ("descendants", ("relations",), {"relations": {"type": "array", "items": _GEN_RELATION}}),
            ("vital", ("vital", "citations"), {"vital": {"enum": ["living", "dead", "existing", "ended"]},
                                               "citations": _GEN_CITATIONS,
                                               "history": {"type": "array", "items": _GEN_HISTORY}}),
        )]}}, "truncated": _BOOLEAN},
}


def _generational_request(action: str) -> dict[str, Any]:
    common = {"protocol": {"const": "wedl-generational/v1"}, "operation": {"const": action},
              "revision": _GEN_SHA, "capabilities": {"oneOf": [{"const": value} for value in _CANONICAL_CAPABILITY_LISTS if "generational-core-v1" in value]},
              "timeline": _GEN_REF, "items": {"type": "integer", "minimum": 1, "maximum": 100 if action == "context" else 500},
              "depth": {"type": "integer", "minimum": 0, "maximum": 16 if action == "context" else 32}}
    required = ["protocol", "operation", "revision", "capabilities", "mode", "timeline"]
    if action == "search":
        common["text"] = {"type": "string", "pattern": "^\\w{1,32}$"}
        common["cursor"] = {"type": ["string", "null"], "minLength": 1, "maxLength": 1024}
        required.append("text")
    else:
        common["subject"] = _GEN_REF
        required.append("subject")
    if action == "relatives":
        common["target"] = _GEN_REF
        required.append("target")
    if action == "context":
        common["maxCharacters"] = {"type": "integer", "minimum": 80, "maximum": 65536}
        required.append("maxCharacters")
    return {"oneOf": [
        _strict_object(*required, properties={**common, "mode": {"const": "author-as-of"}, "at": _GEN_TIME}),
        _strict_object(*required, properties={**common, "mode": {"const": "author-all-time"}}),
        _strict_object(*required, "at", properties={**common, "mode": {"const": "character"}, "at": _GEN_TIME}),
    ]}


_GEN_REFERENCE_DETAIL = _strict_object("id", "kind", "title", "reference", properties={
    "id": _GEN_REF, "kind": {"enum": ["character", "union", "organization", "legacy"]},
    "title": _GEN_REF, "reference": _GEN_REF})


for _action in _GEN_READ_ACTIONS:
    _stem = _action.capitalize()
    SCHEMAS[f"Generational{_stem}Request"] = _generational_request(_action)
    _base = {"protocol": {"const": "wedl-generational/v1"}, "operation": {"const": _action},
             "revision": {"anyOf": [_GEN_SHA, {"type": "null"}]}}
    _result = _GEN_READ_RESULTS[_action]
    _required_result = {"parents": ("relations",), "ancestors": ("relations",),
                        "descendants": ("relations",), "relatives": ("relations",),
                        "union": ("participants", "citations"),
                        "organization": ("organization", "parentPath", "roles"),
                        "legacy": ("legacy", "tenures", "holders", "claims", "succession"),
                        "vital": ("vital", "citations"), "search": ("results", "cursor"),
                        "context": ("items", "truncated")}[_action]
    SCHEMAS[f"Generational{_stem}AvailableOutcome"] = _strict_object(
        "protocol", "operation", "revision", "state", *_required_result,
        properties={**_base, "state": {"const": "available"}, **_result})
    SCHEMAS[f"Generational{_stem}UnknownOutcome"] = _strict_object(
        "protocol", "operation", "revision", "state",
        properties={**_base, "state": {"const": "unknown"},
                    **({"items": {"type": "array", "maxItems": 0}, "truncated": {"const": False}}
                       if _action == "context" else {})})
    SCHEMAS[f"Generational{_stem}InvalidOutcome"] = _strict_object(
        "protocol", "operation", "revision", "state", "code",
        properties={**_base, "state": {"const": "invalid"},
                    "code": {"enum": ["GEN-REQUEST-001", "GEN-TIME-001", "GEN-REFERENCE-001"]},
                    "candidates": {"type": "array", "maxItems": 8, "items": _GEN_REFERENCE_DETAIL},
                    "suggestions": {"type": "array", "maxItems": 8, "items": _GEN_REFERENCE_DETAIL}})
    SCHEMAS[f"Generational{_stem}UnavailableOutcome"] = _strict_object(
        "protocol", "operation", "revision", "state",
        properties={**_base, "state": {"const": "unavailable"}})
    SCHEMAS[f"Generational{_stem}LimitOutcome"] = _strict_object(
        "protocol", "operation", "revision", "state", "code",
        properties={**_base, "state": {"const": "limit"}, "code": {"const": "GEN-LIMIT-001"}})


_CHANGESET = {
    "protocol": "wedl-changeset/v1",
    "expectedHead": "0123456789abcdef0123456789abcdef01234567",
    "idempotencyKey": "example-change",
    "summary": "Record the harbour ledger discovery",
    "operations": [],
}


_OPERATIONS: dict[str, tuple[str, dict[str, Any]]] = {
    "status": ("StatusResponse", {"repositoryRoot": "/world", "revision": "0123456789abcdef", "recordCount": 4, "counts": {"character": 2}, "activeSceneId": "scene-market-day", "activeScenes": [{"id": "scene-market-day", "title": "Market day"}], "currentTime": {"timeline": "main", "tick": 12, "order": 0}, "cacheReadiness": {"state": "ready"}, "chronologyCapability": {"protocol": "wedl-chronology/v1", "sourceSchema": "wedl/v0.6", "mode": "chronology-enabled", "publicReads": True, "authoring": True, "upgradeRequired": False, "upgradeAvailable": False, "durationSemantics": "none"}}),
    "validate": ("ValidationReportResponse", {"valid": True, "revision": "0123456789abcdef", "recordCount": 4, "diagnostics": []}),
    "compile": ("CompileResponse", {"status": "compiled", "revision": "0123456789abcdef", "recordCount": 4, "timingsMs": {"total": 12.4}}),
    "entity list": ("EntityListResponse", [{"id": "character-mara-vale", "kind": "character", "title": "Mara Vale"}]),
    "entity show": ("EntityResponse", {"id": "character-mara-vale", "kind": "character", "title": "Mara Vale", "chronologyAnnotations": []}),
    "state": ("StateResponse", {"revision": "0123456789abcdef", "entityId": "character-mara-vale", "at": {"tick": 12}, "state": {}, "citations": {}, "locationHistory": []}),
    "whereabouts": ("WhereaboutsResponse", {"protocol": "wedl-whereabouts/v2", "revision": "0123456789abcdef", "effectiveTime": {"timeline": "main", "tick": "12", "order": "0"}, "timeScope": {"mode": "as-of", "at": {"timeline": "main", "tick": "12", "order": "0"}}, "characterPolicy": {"includedStatuses": ["canonical", "retired"], "excludedStatuses": ["draft"], "locationEvidence": "initial state and canonical location effects only", "inference": "none"}, "importancePolicy": {"algorithm": "wedl-character-importance/v1", "calculated": True, "nonCanonical": True, "cohort": "all canonical and retired characters before filtering", "normalization": "per-signal log1p(raw) / log1p(cohort maximum); zero maximum contributes zero", "weights": {"scenes": 40, "pointOfViewScenes": 25, "events": 20, "relationshipNeighbors": 15}, "evidence": "scene appearances and POV subset; canonical event participants/effect targets deduplicated per event; distinct reciprocal relationship neighbors", "exclusions": "No prose, tags, inferred travel, co-presence, knowledge, or manual overrides are used."}, "characters": [], "locations": [], "activeScenes": [], "offstageCharacters": [], "unlocatedCharacters": []}),
    "hypotheses": ("HypothesesResponse", {"protocol": "wedl-hypotheses/v1", "revision": "0123456789abcdef", "nonCanonical": True, "hypotheses": []}),
    "knowledge": ("KnowledgeResponse", {"revision": "0123456789abcdef", "characterId": "character-mara-vale", "at": {"tick": 12}, "knowledge": []}),
    "interactions": ("InteractionsResponse", {"revision": "0123456789abcdef", "firstCharacterId": "character-mara-vale", "secondCharacterId": "character-ilyra-sorn", "interactions": []}),
    "story-points": ("StoryPointsResponse", {"revision": "0123456789abcdef", "sceneId": "scene-market-day", "evaluatedAt": {"timeline": "main", "tick": 12, "order": 0}, "storyPoints": [], "continuityAdvisories": []}),
    "timeline": ("TimelineResponse", {"protocol": "wedl-timeline/v1", "revision": "0123456789abcdef", "timeline": {"id": "main", "label": "Main chronology"}, "temporalSemantics": {"spacing": "ordinal", "durationSemantics": "none", "intervalEndpoints": "inclusive"}, "points": [], "spans": []}),
    "threads": ("ThreadCatalogResponse", {"protocol": "wedl-threads/v1", "revision": "0123456789abcdef", "sourceSchema": "wedl/v0.5", "groupingAvailable": True, "threads": [{"id": "thread_0123456789ABCDEFGHJKMNPQRS", "label": "Archive"}]}),
    "thread-memberships": ("ThreadMembershipResponse", {"protocol": "wedl-thread-memberships/v1", "revision": "0123456789abcdef", "sourceSchema": "wedl/v0.5", "selectedThreadIds": ["thread_0123456789ABCDEFGHJKMNPQRS"], "records": [{"recordId": "event_0123456789ABCDEFGHJKMNPQRS", "threadIds": ["thread_0123456789ABCDEFGHJKMNPQRS"]}]}),
    "causal": ("CausalityResponse", {"protocol": "wedl-causality/v1", "revision": "0123456789abcdef", "direction": "both", "effectiveTime": {"timeline": "main", "tick": 12, "order": 0}, "timeScope": {"mode": "as-of", "at": {"timeline": "main", "tick": 12, "order": 0}}, "focusEvent": {"id": "event-market-alarm", "kind": "event", "title": "Market alarm"}, "nodes": [{"event": {"id": "event-market-alarm", "kind": "event", "title": "Market alarm"}, "at": {"timeline": "main", "tick": 12, "order": 0}, "status": "canonical"}], "edges": [], "continuityAdvisories": []}),
    "search": ("SearchResponse", {"protocol": "wedl-search/v5", "revision": "0123456789abcdef", "perspective": "character", "characterId": "character-mara-vale", "sceneId": "scene-market-day", "effectiveTime": {"tick": 12}, "timeScope": {"mode": "as-of", "at": {"tick": 12}}, "mode": "hybrid", "searchState": {}, "results": []}),
    "context": ("ContextResponse", {"protocol": "wedl-context/v3", "revision": "0123456789abcdef", "perspective": "author", "characterId": "character-mara-vale", "sceneId": "scene-market-day", "effectiveTime": {"tick": 12}, "focus": {}, "promptText": "Use only canonical evidence.", "selection": {}}),
    "conversation show": ("ConversationResponse", {"protocol": "wedl-conversation/v2", "revision": "0123456789abcdef", "perspective": "author", "conversationId": "conversation-market-meeting", "title": "Market meeting", "conversationTime": {}, "effectiveTime": {"tick": 12}, "timeScope": {"mode": "as-of", "at": {"tick": 12}}, "verbatimTurns": [], "beats": [], "recollections": []}),
    "author request preview": ("AuthoringPreviewResponse", {"protocol": "wedl-author-preview/v1", "intent": {"action": "conversation.append", "conversation": "Market meeting", "speaker": "Mara Vale", "text": "Close the ledger."}, "changeset": _CHANGESET, "preview": {"protocol": "wedl-preview/v1", "valid": True, "expectedHead": "0123456789abcdef0123456789abcdef01234567", "requestHash": "a" * 64, "confirmationToken": "wedl-confirmation/v1:" + "b" * 64, "generatedIds": {}, "touchedEntityIds": [], "diagnostics": [], "files": [], "diff": ""}, "authorImpact": {"summary": "Appended a speech beat to Market meeting.", "items": [{"kind": "conversation-turn-appended", "conversation": "Market meeting", "scene": "Market day", "location": "Flood Stair", "characters": ["Mara Vale"], "at": {"timeline": "main", "tick": 12, "order": 1}}]}}),
    "author request apply": ("AuthoringApplyResponse", {"protocol": "wedl-command-result/v1", "status": "committed", "previousHead": "0123456789abcdef0123456789abcdef01234567", "newHead": "fedcba9876543210fedcba9876543210fedcba98", "generatedIds": {}, "touchedEntityIds": [], "compile": {"status": "compiled", "revision": "fedcba9876543210fedcba9876543210fedcba98"}, "idempotentReplay": False, "authorImpact": {"summary": "Appended a speech beat to Market meeting.", "items": [{"kind": "conversation-turn-appended", "conversation": "Market meeting", "scene": "Market day", "location": "Flood Stair", "characters": ["Mara Vale"], "at": {"timeline": "main", "tick": 12, "order": 1}}]}}),
    "changeset scaffold": ("ChangesetSchemaResponse", {**_CHANGESET, "operations": [{"type": "entity.update", "entity": "character-mara-vale", "frontmatterPatch": {}}]}),
    "changeset schema": ("ChangesetSchemaDocument", {"protocol": "wedl-changeset-schema/v1", "changesetProtocol": "wedl-changeset/v1", "required": ["protocol", "expectedHead", "idempotencyKey", "summary", "operations"], "operations": []}),
    "changeset preview": ("ChangesetPreviewResponse", {"protocol": "wedl-preview/v1", "valid": True, "expectedHead": _CHANGESET["expectedHead"], "requestHash": "a" * 64, "confirmationToken": "wedl-confirmation/v1:" + "b" * 64, "generatedIds": {}, "touchedEntityIds": [], "diagnostics": [], "files": [], "diff": ""}),
    "changeset apply": ("ChangesetApplyResponse", {"protocol": "wedl-command-result/v1", "status": "committed", "previousHead": _CHANGESET["expectedHead"], "newHead": "fedcba9876543210fedcba9876543210fedcba98", "generatedIds": {}, "touchedEntityIds": [], "compile": {"status": "compiled", "revision": "fedcba9876543210fedcba9876543210fedcba98"}, "idempotentReplay": False}),
    "session": ("SessionResponse", {"token": "session-token", "head": _CHANGESET["expectedHead"]}),
    "root": ("WorkspaceHtmlResponse", "<!doctype html><title>WEDL</title>"),
    **{f"generational {action}": (f"Generational{action.capitalize()}AvailableOutcome", {
        "protocol": "wedl-generational/v1", "operation": action,
        "revision": "0" * 40, "state": "available",
        **({"relations": []} if action in {"parents", "ancestors", "descendants", "relatives"}
           else {"participants": [], "citations": []} if action == "union"
           else {"organization": {}, "parentPath": [], "roles": []} if action == "organization"
           else {"legacy": {}, "tenures": [], "holders": [], "claims": [], "succession": []} if action == "legacy"
           else {"vital": "living", "citations": []} if action == "vital"
           else {"results": [], "cursor": None} if action == "search"
           else {"items": [], "truncated": False}),
    }) for action in _GEN_READ_ACTIONS},
    "generational scaffold": ("GenerationalScaffoldResponse", {"action": "generational.create", "expectedHead": "0" * 40, "idempotencyKey": "generational-starter-choose-a-unique-key", "kind": "organization", "title": "New organization", "audience": ["public"], "perspectives": ["ordinary"], "fields": {"organization_kind": "house"}, "payload": {"title": "New organization", "aliases": []}, "at": {"timeline": "main", "tick": "0", "order": "0"}}),
    "generational schema": ("GenerationalSchemaResponse", {"protocol": "wedl-generational-authoring-schema/v1", "sourceSchema": "wedl/v0.7", "kinds": sorted(_GEN_FIELD_SCHEMAS), "variants": ["generational.create", "generational.append", "generational.correct", "generational.batch"], "fields": {}, "confirmation": "author request preview -> author request apply --confirm TOKEN", "batchLimit": 32}),
}


def operation_schema(operation: str) -> str:
    """Return the component name for one descriptor operation key."""

    return _OPERATIONS[operation][0]


def operation_example(operation: str) -> tuple[dict[str, Any], ...]:
    """Return one compact but schema-valid success response example."""

    if operation == "generational schema":
        from .generational_authoring import schema as generational_schema

        return ({"summary": "Current generational intent schema", "value": generational_schema()},)
    if operation == "context":
        base = deepcopy(_OPERATIONS[operation][1])
        dramatic = {**base, "perspective": "dramatic-irony", "characterPrompt": "Known evidence.", "authorMargin": "Withheld evidence."}
        dramatic.pop("promptText")
        return (
            {"summary": "Author context", "value": base},
            {"summary": "Dramatic-irony context", "value": dramatic},
        )
    if operation == "conversation show":
        author = deepcopy(_OPERATIONS[operation][1])
        speech = {
            "id": "turn-mara-001", "kind": "speech",
            "at": {"timeline": "main", "tick": 12, "order": 10},
            "text": "Keep your hand off the latch.", "audience": ["participants"], "citation": {},
            "speakerId": "character-mara-vale", "speaker": "Mara Vale", "delivery": "urgent",
            "addresseeId": "character-nessa-quill", "addressee": "Nessa Quill",
        }
        action = {
            "id": "turn-nessa-002", "kind": "action",
            "at": {"timeline": "main", "tick": 12, "order": 20},
            "text": "Nessa steps between Mara and the flooded stair.", "audience": ["participants"], "citation": {},
            "actorIds": ["character-nessa-quill"], "actors": ["Nessa Quill"],
        }
        interruption = {
            "id": "turn-nessa-003", "kind": "speech",
            "at": {"timeline": "main", "tick": 12, "order": 30},
            "text": "Too late.", "audience": ["participants"], "citation": {},
            "speakerId": "character-nessa-quill", "speaker": "Nessa Quill", "interrupts": "turn-mara-001",
        }
        # ``verbatimTurns`` deliberately remains the speech-only compatibility
        # projection.  ``beats`` is the ordered modern transcript, including
        # choreography and its explicit interruption link.
        author["verbatimTurns"] = [speech, interruption]
        author["beats"] = [speech, action, interruption]
        character = {
            "protocol": "wedl-conversation/v2", "revision": "0123456789abcdef",
            "perspective": "character", "conversationId": "conversation-market-meeting", "title": "Market meeting",
            "characterId": "character-mara-vale", "conversationTime": {}, "effectiveTime": {"tick": 12},
            "timeScope": {"mode": "as-of", "at": {"tick": 12}}, "heardVerbatimTurns": [speech, interruption], "perceivedBeats": [speech, action, interruption],
            "subjectiveRecollection": {"id": "recollection-market"}, "provenanceBoundary": "Canonical audible lines are separate from memory.",
        }
        return ({"summary": "Author transcript", "value": author}, {"summary": "Character recollection", "value": character})
    return ({"summary": "Successful response", "value": deepcopy(_OPERATIONS[operation][1])},)


def components() -> dict[str, dict[str, Any]]:
    """Return a copy suitable for incorporation into an OpenAPI document."""

    return {"schemas": deepcopy(SCHEMAS)}
