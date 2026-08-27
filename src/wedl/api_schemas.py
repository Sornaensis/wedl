"""Framework-free OpenAPI response schema and example registry.

The registry intentionally describes stable envelopes and important fields
without pretending that every additive result field is fixed.  HTTP routing
uses these declarations only for discovery; runtime serialization remains in
the command/query implementations.
"""

from __future__ import annotations

from copy import deepcopy
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
]}

_AUTHOR_IMPACT_ITEM = _object(
    "kind",
    properties={
        "kind": {"enum": ["conversation-created", "conversation-turn-appended", "author-horizon-advanced", "hypothesis-recorded", "hypothesis-status"]},
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


SCHEMAS: dict[str, dict[str, Any]] = {
    "ChangesetRequest": _CHANGESET_REQUEST,
    "AuthoringRequest": _AUTHORING_REQUEST,
    "AuthorImpact": _AUTHOR_IMPACT,
    "AuthoringPreviewResponse": _object("protocol", "intent", "changeset", "preview", "authorImpact", properties={"protocol": {"const": "wedl-author-preview/v1"}, "intent": {"$ref": "#/components/schemas/AuthoringRequest"}, "changeset": {"$ref": "#/components/schemas/ChangesetRequest"}, "preview": {"$ref": "#/components/schemas/ChangesetPreviewResponse"}, "authorImpact": {"$ref": "#/components/schemas/AuthorImpact"}}),
    "WedlEntity": _object("id", "kind", "title", properties={"id": _STRING, "kind": {"enum": ["world", "character", "knowledge", "event", "object", "environment", "location", "relationship", "story-point", "scene", "conversation"]}, "title": _STRING}),
    "StatusResponse": _object("repositoryRoot", "revision", "recordCount", "counts", "activeSceneId", "activeScenes", "currentTime", "cacheReadiness", properties={"repositoryRoot": _STRING, "revision": _STRING, "treeOid": _STRING, "recordCount": _INTEGER, "counts": _OBJECT, "activeSceneId": {"type": ["string", "null"]}, "activeScenes": {"type": "array", "items": _object("id", "title", properties={"id": _STRING, "title": _STRING})}, "currentTime": {"anyOf": [_STORY_TIME, {"type": "null"}]}, "cacheReadiness": _OBJECT}),
    "ValidationReportResponse": _object("valid", "revision", "recordCount", "diagnostics", properties={"valid": _BOOLEAN, "revision": _STRING, "recordCount": _INTEGER, "diagnostics": {"type": "array", "items": _OBJECT}}),
    "CompileResponse": _object("status", "revision", properties={"status": _STRING, "revision": _STRING, "treeOid": _STRING, "recordCount": _INTEGER, "timingsMs": _OBJECT}),
    "EntityListResponse": {"type": "array", "items": {"$ref": "#/components/schemas/WedlEntity"}},
    "EntityResponse": _object("id", "kind", "title", properties={"id": _STRING, "kind": _STRING, "title": _STRING, "frontmatter": _OBJECT, "locationContext": _LOCATION_CONTEXT}),
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


_CHANGESET = {
    "protocol": "wedl-changeset/v1",
    "expectedHead": "0123456789abcdef0123456789abcdef01234567",
    "idempotencyKey": "example-change",
    "summary": "Record the harbour ledger discovery",
    "operations": [],
}


_OPERATIONS: dict[str, tuple[str, dict[str, Any]]] = {
    "status": ("StatusResponse", {"repositoryRoot": "/world", "revision": "0123456789abcdef", "recordCount": 4, "counts": {"character": 2}, "activeSceneId": "scene-market-day", "activeScenes": [{"id": "scene-market-day", "title": "Market day"}], "currentTime": {"timeline": "main", "tick": 12, "order": 0}, "cacheReadiness": {"state": "ready"}}),
    "validate": ("ValidationReportResponse", {"valid": True, "revision": "0123456789abcdef", "recordCount": 4, "diagnostics": []}),
    "compile": ("CompileResponse", {"status": "compiled", "revision": "0123456789abcdef", "recordCount": 4, "timingsMs": {"total": 12.4}}),
    "entity list": ("EntityListResponse", [{"id": "character-mara-vale", "kind": "character", "title": "Mara Vale"}]),
    "entity show": ("EntityResponse", {"id": "character-mara-vale", "kind": "character", "title": "Mara Vale"}),
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
}


def operation_schema(operation: str) -> str:
    """Return the component name for one descriptor operation key."""

    return _OPERATIONS[operation][0]


def operation_example(operation: str) -> tuple[dict[str, Any], ...]:
    """Return one compact but schema-valid success response example."""

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
