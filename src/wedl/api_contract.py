"""Parser-derived HTTP command contracts.

This module is deliberately framework-free.  It walks the argparse tree that
drives ``wedl`` and adds only transport decisions (HTTP method/path and the
small alias map required by the established API).  Therefore a new CLI option
cannot silently become an undocumented HTTP input: it is visible here and is
either mapped or deliberately excluded.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from enum import Enum
from functools import lru_cache
from typing import Any, Iterable, Mapping

from . import command_parser
from .api_schemas import components as response_schema_components
from .api_schemas import operation_example, operation_schema
from .errors import UsageError


class ApiClass(str, Enum):
    """The externally visible class of every CLI leaf."""

    QUERY = "QUERY"
    ACTION = "ACTION"
    MUTATION = "MUTATION"
    LOCAL_ONLY = "LOCAL_ONLY"


class Transport(str, Enum):
    QUERY = "query"
    PATH = "path"
    BODY = "body"
    HEADER = "header"
    INJECT = "inject"
    OMIT = "omit"


class AuthPolicy(str, Enum):
    """Authentication required by one mounted HTTP operation."""

    PUBLIC = "public"
    SESSION = "session"

    @property
    def header_name(self) -> str | None:
        return "X-Wedl-Token" if self == AuthPolicy.SESSION else None

    @property
    def header_description(self) -> str | None:
        return "per-repository session token returned by GET /api/session" if self == AuthPolicy.SESSION else None


@dataclass(frozen=True)
class HeaderContract:
    """An HTTP header mapped to one canonical CLI parser destination."""

    dest: str
    name: str


@dataclass(frozen=True)
class DiscoveryDescriptor:
    """Stable OpenAPI discovery metadata, independent of response schemas."""

    summary: str
    description: str
    tags: tuple[str, ...]
    success_description: str
    errors: tuple[str, ...] = ("usage_error",)
    examples: tuple[dict[str, Any], ...] = ()
    success_schema: str | None = None
    success_examples: tuple[dict[str, Any], ...] = ()
    success_media_type: str = "application/json"
    request_schema: str | None = None


_SPATIAL_EXAMPLES: dict[str, dict[str, Any]] = {
    "containment": {"locationId": "location_X"},
    "children": {"locationId": "location_X"},
    "bbox": {"mapId": "map_X", "bounds": {"min": [0, 0], "max": [1, 1]}, "relation": "intersects"},
    "nearby": {"position": {"mapId": "map_X", "coordinates": [0, 0]}, "radius": 1},
    "adjacency": {"locationId": "location_X"},
    "reachability": {"fromLocationId": "location_X"},
    "path": {"fromLocationId": "location_X", "toLocationId": "location_Y", "metric": "routeDistance"},
    "overlay-as-of": {"queryScope": "location", "locationId": "location_X", "audience": "author", "perspective": "author", "asOf": {"timeline": "main", "tick": "0", "order": "0"}},
}

_GENERATIONAL_OPERATIONS = ("parents", "ancestors", "descendants", "relatives", "union",
                           "organization", "legacy", "vital", "search", "context")


def _generational_example(action: str) -> dict[str, Any]:
    body: dict[str, Any] = {"protocol": "wedl-generational/v1", "operation": action,
                            "revision": "0" * 40, "capabilities": ["generational-core-v1"],
                            "mode": "author-as-of", "timeline": "main",
                            "at": {"timeline": "main", "tick": "-7", "order": "2"},
                            "items": 20, "depth": 4}
    if action == "search":
        body["text"] = "aster"
        body["cursor"] = None
    else:
        body["subject"] = {"union": "The Threefold Compact", "organization": "House Aster",
                           "legacy": "Keeper of Keys"}.get(action, "Mara Vale")
        if action == "relatives":
            body["target"] = "Ilyra Sorn"
        if action == "context":
            body["maxCharacters"] = 4096
    return body


def _generational_response_example(action: str) -> dict[str, Any]:
    citation = {"record_id": "parentage_0123456789ABCDEFGHJKMNPQRS",
                "path": "story/parentages/parentage_0123456789ABCDEFGHJKMNPQRS.md",
                "applicability": {"applicability_kind": "instant",
                                  "point": {"timeline": "main", "tick": "-7", "order": "2"}}}
    edge = {"from": "char_1123456789ABCDEFGHJKMNPQRS",
            "to": "char_0123456789ABCDEFGHJKMNPQRS",
            "recordId": citation["record_id"], "citations": [citation]}
    parent = {"targetId": "char_0123456789ABCDEFGHJKMNPQRS",
              "label": "biological-parent", "recordId": citation["record_id"],
              "citations": [citation]}
    result: dict[str, Any] = {
        "parents": {"relations": [parent]},
        "ancestors": {"relations": [{"targetId": "char_0123456789ABCDEFGHJKMNPQRS",
                                     "generationDistance": 1, "label": "ancestor",
                                     "edges": [edge]}]},
        "descendants": {"relations": [{"targetId": "char_0123456789ABCDEFGHJKMNPQRS",
                                       "generationDistance": 1, "label": "descendant",
                                       "edges": [edge]}]},
        "relatives": {"relations": [{"targetId": "char_0123456789ABCDEFGHJKMNPQRS",
                                     "generationDistance": 1, "label": "relative-path",
                                     "edges": [edge]}]},
        "union": {"participants": ["char_0123456789ABCDEFGHJKMNPQRS",
                                   "char_1123456789ABCDEFGHJKMNPQRS"], "citations": [citation]},
        "organization": {"organization": {"recordId": "organization_0123456789ABCDEFGHJKMNPQRS",
                                            "kind": "organization", "state": "active",
                                            "value": {"organization_kind": "house", "title": "House Aster",
                                                      "aliases": []},
                                            "citations": [citation], "causes": []},
                         "parentPath": [], "roles": []},
        "legacy": {"legacy": {"recordId": "legacy_0123456789ABCDEFGHJKMNPQRS",
                              "kind": "legacy", "state": "active",
                              "value": {"legacy_kind": "office", "title": "Keeper of Keys",
                                        "aliases": []},
                              "citations": [citation], "causes": []},
                   "tenures": [], "holders": [], "claims": [], "succession": []},
        "vital": {"vital": "living", "citations": [citation]},
        "search": {"results": [{"recordId": citation["record_id"], "kind": "parentage",
                                "citations": [citation]}],
                   "cursor": None},
        "context": {"items": [{"kind": "parents", "result": {"state": "available",
                                                       "relations": [parent]}}],
                    "truncated": False},
    }[action]
    return {"protocol": "wedl-generational/v1", "operation": action,
            "revision": "0" * 40, "state": "available", **result}


def _generational_intent_examples(path: str, *, apply: bool) -> tuple[dict[str, Any], ...]:
    point = {"timeline": "main", "tick": "0", "order": "0"}
    common = {"expectedHead": "0" * 40, "idempotencyKey": "choose-a-unique-key"}
    creates = {
        "organization": ({"organization_kind": "house"}, {"title": "House Aster", "aliases": []}),
        "parentage": ({"child_id": "Mara Vale", "parent_id": "Ilyra Sorn"}, {"basis": "adoptive"}),
        "union": ({"participant_ids": ["Ilyra Sorn", "Mara Vale"]},
                  {"participant_ids": ["Ilyra Sorn", "Mara Vale"]}),
        "affiliation": ({"character_id": "Mara Vale", "organization_id": "House Aster"},
                        {"role": "scribe"}),
        "legacy": ({"legacy_kind": "office"}, {"title": "Keeper of Keys", "aliases": []}),
        "tenure": ({"legacy_id": "Keeper of Keys"}, {"holder_id": "Mara Vale", "basis": "legal"}),
        "claim": ({"legacy_id": "Keeper of Keys", "claimant_id": "Ilyra Sorn"},
                  {"competes_with": []}),
        "vital-history": ({"character_id": "Mara Vale", "disclosure": "known"}, {}),
    }
    bodies = [(f"generational.create {kind}", {"action": "generational.create", **common,
               "kind": kind, "title": f"New {kind}", "audience": ["public"],
               "perspectives": ["ordinary"], "fields": fields, "payload": payload, "at": point})
              for kind, (fields, payload) in creates.items()]
    append = {"action": "generational.append", **common, "kind": "organization",
              "record": "House Aster", "transition": "organization-rename",
              "payload": {"title": "House Aster Renewed", "aliases": []},
              "cause": "Founding decree", "at": {"timeline": "main", "tick": "1", "order": "0"}}
    correct = {**append, "action": "generational.correct",
               "replaces": "transition_00000000000000000000000000"}
    batch_item = {key: value for key, value in bodies[0][1].items()
                  if key not in {"expectedHead", "idempotencyKey"}}
    batch = {"action": "generational.batch", **common, "items": [batch_item]}
    bodies.extend((("generational.append", append), ("generational.correct", correct),
                   ("generational.batch", batch)))
    headers = {"X-Wedl-Token": "<session-token>"}
    if apply:
        headers["X-Wedl-Confirmation"] = "wedl-confirmation/v1:<proof>"
    return tuple({"summary": summary, "value": {"method": "POST", "path": path,
                                               "headers": headers, "body": body}}
                 for summary, body in bodies)


def _generational_descriptor(action: str) -> DiscoveryDescriptor:
    stem = action.capitalize()
    return DiscoveryDescriptor(
        f"Read generational {action}",
        "Read selected-revision cited generational evidence. The session is a local author principal; character mode remains closed without a trusted character identity. Semantic states use the 200/400/409/422 matrix.",
        ("Generational",), "Generational outcome.",
        ("authentication_required", "usage_error", "compile_required", "parse_error", "repository_error"),
        examples=({"summary": f"{action} request", "value": {"method": "POST",
                   "path": f"/api/generational/{action}",
                   "headers": {"X-Wedl-Token": "<session-token>"},
                   "body": _generational_example(action)}},),
        success_schema=f"Generational{stem}AvailableOutcome",
        success_examples=({"summary": f"{action} cited outcome",
                           "value": _generational_response_example(action)},),
        request_schema=f"Generational{stem}Request",
    )


def _spatial_descriptor(action: str) -> DiscoveryDescriptor:
    stem = "OverlayAsOf" if action == "overlay-as-of" else "".join(part.capitalize() for part in action.split("-"))
    body = {"protocol": "wedl-spatial/v1", "revision": "0" * 40, "capabilities": ["spatial-core-v1", "geometry-v1", "route-v1", "overlay-v1"], "limit": 10, "cursor": None, **_SPATIAL_EXAMPLES[action]}
    result_examples: dict[str, dict[str, Any]] = {
        "containment": {"ids": ["location_X"], "basis": "authored-parent-id"},
        "children": {"ids": [], "basis": "authored-parent-id", "units": "none", "filters": {"parentId": "location_X"}, "partial": False, "unknown": False, "nextCursor": "opaque-page-cursor"},
        "bbox": {"ids": [], "basis": "authored-geometry-bounds", "units": "pace", "filters": {"mapId": "map_X", "relation": "intersects"}, "partial": False, "unknown": False, "nextCursor": "opaque-page-cursor"},
        "nearby": {"ids": [], "basis": "same-map-authored-geometry", "units": "pace", "filters": {"mapId": "map_X", "radius": 1}, "partial": False, "unknown": False, "nextCursor": "opaque-page-cursor"},
        "adjacency": {"fromLocationId": "location_X", "routeIds": [], "portalIds": [], "targetLocationIds": [], "positionPortalIds": [], "filters": {"modes": [], "availability": ["open"]}, "basis": "authored-directed-route-and-portal-edges", "partial": False, "unknown": False},
        "reachability": {"fromLocationId": "location_X", "ids": ["location_X"], "expansions": 1, "filters": {"modes": [], "availability": ["open"]}, "basis": "authored-directed-route-and-location-portal-edges", "partial": False, "unknown": False},
        "path": {"ids": ["location_X", "location_Y"], "routeIds": ["route_X"], "metric": {"metric": "routeDistance", "computedTotal": 1, "unit": "pace", "complete": True, "unknownEdges": [], "partial": False, "unknown": False}, "expansions": 1, "basis": "authored-directed-routes", "filters": {"modes": [], "availability": ["open"]}, "partial": False, "unknown": False},
        "overlay-as-of": {"ids": [], "storyTime": {"timeline": "main", "tick": "0", "order": "0"}, "filters": {"audience": "author", "perspective": "author", "horizon": {"timeline": "main", "tick": "0", "order": "0"}}, "basis": "authored-overlay-membership", "partial": False, "unknown": False, "nextCursor": "opaque-page-cursor"},
    }
    result: dict[str, Any] = {"protocol": "wedl-spatial/v1", "operation": action, "revision": "0" * 40, "sourceSchema": "wedl/v0.7", "capabilities": ["spatial-core-v1", "geometry-v1", "route-v1", "overlay-v1"], "cache": {"state": "ready", "revision": "0" * 40, "treeOid": "0" * 40, "sourceSchema": "wedl/v0.7", "fingerprint": "compiler"}, "state": "ok", "result": result_examples[action]}
    return DiscoveryDescriptor(f"Read spatial {action}", "Read one strict wedl-spatial/v1 compiled-projection operation. Spatial semantic states use the documented 200/400/403/409/422 matrix; ordinary WEDL failures retain their normal envelopes.", ("Spatial",), "Spatial outcome.", ("usage_error", "compile_required", "validation_failed", "parse_error", "repository_error"), examples=({"summary": f"{action} request", "value": {"method": "POST", "path": f"/api/spatial/{action}", "body": body}},), success_schema=f"Spatial{stem}OkOutcome", success_examples=({"summary": f"{action} outcome", "value": result},), request_schema=f"Spatial{stem}Request")


@dataclass(frozen=True)
class ControlEndpoint:
    """A non-CLI endpoint that shares the discovery vocabulary."""

    method: str
    path: str
    auth: AuthPolicy
    descriptor: DiscoveryDescriptor


@dataclass(frozen=True)
class ArgumentContract:
    """One public argument as declared by argparse."""

    dest: str
    option_strings: tuple[str, ...]
    required: bool
    default: Any
    choices: tuple[Any, ...] | None
    help: str | None
    positional: bool
    repeated: bool
    value_type: str
    minimum: int | float | None
    maximum: int | float | None
    transport: Transport
    transport_name: str | None
    transport_reason: str
    transport_description: str | None
    validator: Any


@dataclass(frozen=True)
class RouteBinding:
    """HTTP transport binding for a parser leaf command."""

    method: str
    path: str
    parameters: tuple[tuple[str, str], ...] = ()
    auth: AuthPolicy = AuthPolicy.PUBLIC
    headers: tuple[HeaderContract, ...] = ()

    @property
    def parameter_names(self) -> frozenset[str]:
        return frozenset(name for _dest, name in self.parameters)


@dataclass(frozen=True)
class CommandContract:
    """A leaf CLI command and its deliberate API classification."""

    command: tuple[str, ...]
    arguments: tuple[ArgumentContract, ...]
    api_class: ApiClass
    effect: str
    availability: str
    binding: RouteBinding | None = None
    reason: str | None = None
    discovery: DiscoveryDescriptor | None = None


# This is transport policy, not a second command grammar.  Every key must
# correspond to exactly one argparse leaf; ``command_contracts`` checks that
# invariant so parser changes cannot drift from the API review surface.
_POLICY: dict[tuple[str, ...], tuple[ApiClass, str, str, RouteBinding | None, str | None]] = {
    ("completion",): (ApiClass.LOCAL_ONLY, "local", "local_only", None, "emits shell code locally"),
    ("init",): (ApiClass.LOCAL_ONLY, "write", "local_only", None, "creates a repository and may invoke Git"),
    ("migrate", "preview"): (ApiClass.LOCAL_ONLY, "write", "local_only", None, "audits local Git source snapshots without an HTTP migration surface"),
    ("migrate", "apply"): (ApiClass.LOCAL_ONLY, "write", "local_only", None, "writes a confirmed local Git source migration and rebuilds its cache"),
    ("status",): (ApiClass.QUERY, "read", "mounted", RouteBinding("GET", "/api/status"), None),
    ("validate",): (ApiClass.QUERY, "read", "mounted", RouteBinding("GET", "/api/validate"), None),
    ("compile",): (ApiClass.ACTION, "write", "mounted", RouteBinding("POST", "/api/compile", (("force", "force"), ("profile", "profile"), ("vector_provider", "vector_provider"), ("vector_model", "vector_model"), ("vector_dimensions", "vector_dimensions"), ("vector_max_features", "vector_max_features")), AuthPolicy.SESSION), None),
    ("entity", "list"): (ApiClass.QUERY, "read", "mounted", RouteBinding("GET", "/api/entities", (("kind", "kind"), ("text", "text"), ("require_compiled", "requireCompiled"))), None),
    ("entity", "show"): (ApiClass.QUERY, "read", "mounted", RouteBinding("GET", "/api/entities/{entity_id}", (("require_compiled", "requireCompiled"),)), None),
    ("state",): (ApiClass.QUERY, "read", "mounted", RouteBinding("GET", "/api/entities/{entity_id}/state", (("tick", "tick"), ("timeline", "timeline"), ("order", "order"), ("require_compiled", "requireCompiled"))), None),
    ("knowledge",): (ApiClass.QUERY, "read", "mounted", RouteBinding("GET", "/api/entities/{character_id}/knowledge", (("tick", "tick"), ("timeline", "timeline"), ("order", "order"), ("require_compiled", "requireCompiled"))), None),
    ("interactions",): (ApiClass.QUERY, "read", "mounted", RouteBinding("GET", "/api/interactions", (("first", "first"), ("second", "second"), ("require_compiled", "requireCompiled"))), None),
    ("story-points",): (ApiClass.QUERY, "read", "mounted", RouteBinding("GET", "/api/story-points", (("scene", "scene"), ("tick", "tick"), ("timeline", "timeline"), ("order", "order"), ("require_compiled", "requireCompiled"))), None),
    ("timeline",): (ApiClass.QUERY, "read", "mounted", RouteBinding("GET", "/api/timeline", (("timeline", "timeline"), ("require_compiled", "requireCompiled"))), None),
    ("chronology", "catalog"): (ApiClass.QUERY, "read", "mounted", RouteBinding("GET", "/api/chronology", (("require_compiled", "requireCompiled"),)), None),
    ("chronology", "format"): (ApiClass.QUERY, "read", "mounted", RouteBinding("POST", "/api/chronology/format", (("require_compiled", "requireCompiled"),)), None),
    ("chronology", "convert"): (ApiClass.QUERY, "read", "mounted", RouteBinding("POST", "/api/chronology/convert", (("require_compiled", "requireCompiled"),)), None),
    ("chronology", "search"): (ApiClass.QUERY, "read", "mounted", RouteBinding("POST", "/api/chronology/search", (("require_compiled", "requireCompiled"),)), None),
    ("chronology", "story-times"): (ApiClass.QUERY, "read", "mounted", RouteBinding("POST", "/api/chronology/story-times", (("require_compiled", "requireCompiled"),)), None),
    ("spatial", "containment"): (ApiClass.QUERY, "read", "mounted", RouteBinding("POST", "/api/spatial/containment", (("require_compiled", "requireCompiled"),)), None),
    ("spatial", "children"): (ApiClass.QUERY, "read", "mounted", RouteBinding("POST", "/api/spatial/children", (("require_compiled", "requireCompiled"),)), None),
    ("spatial", "bbox"): (ApiClass.QUERY, "read", "mounted", RouteBinding("POST", "/api/spatial/bbox", (("require_compiled", "requireCompiled"),)), None),
    ("spatial", "nearby"): (ApiClass.QUERY, "read", "mounted", RouteBinding("POST", "/api/spatial/nearby", (("require_compiled", "requireCompiled"),)), None),
    ("spatial", "adjacency"): (ApiClass.QUERY, "read", "mounted", RouteBinding("POST", "/api/spatial/adjacency", (("require_compiled", "requireCompiled"),)), None),
    ("spatial", "reachability"): (ApiClass.QUERY, "read", "mounted", RouteBinding("POST", "/api/spatial/reachability", (("require_compiled", "requireCompiled"),)), None),
    ("spatial", "path"): (ApiClass.QUERY, "read", "mounted", RouteBinding("POST", "/api/spatial/path", (("require_compiled", "requireCompiled"),)), None),
    ("spatial", "overlay-as-of"): (ApiClass.QUERY, "read", "mounted", RouteBinding("POST", "/api/spatial/overlay-as-of", (("require_compiled", "requireCompiled"),)), None),
    **{("generational", action): (ApiClass.QUERY, "read", "mounted", RouteBinding("POST", f"/api/generational/{action}", (("require_compiled", "requireCompiled"),), AuthPolicy.SESSION), None) for action in _GENERATIONAL_OPERATIONS},
    ("generational", "scaffold"): (ApiClass.ACTION, "write", "mounted", RouteBinding("POST", "/api/generational/scaffold", auth=AuthPolicy.SESSION), None),
    ("generational", "schema"): (ApiClass.QUERY, "read", "mounted", RouteBinding("GET", "/api/generational/schema", auth=AuthPolicy.SESSION), None),
    ("threads",): (ApiClass.QUERY, "read", "mounted", RouteBinding("GET", "/api/threads", (("require_compiled", "requireCompiled"),)), None),
    ("thread-memberships",): (ApiClass.QUERY, "read", "mounted", RouteBinding("GET", "/api/thread-memberships", (("record_ids", "recordId"), ("thread_ids", "threadId"), ("require_compiled", "requireCompiled"))), None),
    ("whereabouts",): (ApiClass.QUERY, "read", "mounted", RouteBinding("GET", "/api/whereabouts", (("character", "character"), ("tick", "tick"), ("timeline", "timeline"), ("order", "order"), ("require_compiled", "requireCompiled"))), None),
    ("hypotheses",): (ApiClass.QUERY, "read", "mounted", RouteBinding("GET", "/api/hypotheses", (("hypothesis", "hypothesis"), ("status", "status"), ("text", "text"), ("require_compiled", "requireCompiled"))), None),
    ("causal",): (ApiClass.QUERY, "read", "mounted", RouteBinding("GET", "/api/causal/{event_id}", (("direction", "direction"), ("tick", "tick"), ("timeline", "timeline"), ("order", "order"), ("require_compiled", "requireCompiled"))), None),
    ("search",): (ApiClass.QUERY, "read", "mounted", RouteBinding("GET", "/api/search", (("query", "q"), ("perspective", "perspective"), ("character", "character"), ("scene", "scene"), ("mode", "mode"), ("limit", "limit"), ("include_text", "includeText"), ("include_hypotheses", "includeHypotheses"), ("thread_ids", "threadId"), ("timeline", "timeline"), ("tick", "tick"), ("order", "order"), ("all_time", "allTime"), ("require_compiled", "requireCompiled"))), None),
    ("context",): (ApiClass.QUERY, "read", "mounted", RouteBinding("GET", "/api/context", (("character", "character"), ("scene", "scene"), ("perspective", "perspective"), ("query", "q"), ("mode", "mode"), ("max_characters", "maxCharacters"), ("max_items", "maxItems"), ("recall_thread_ids", "recallThreadId"), ("timeline", "timeline"), ("tick", "tick"), ("order", "order"), ("require_compiled", "requireCompiled"))), None),
    ("conversation", "show"): (ApiClass.QUERY, "read", "mounted", RouteBinding("GET", "/api/conversations/{conversation_id}", (("perspective", "perspective"), ("character", "character"), ("timeline", "timeline"), ("tick", "tick"), ("order", "order"), ("all_time", "allTime"), ("require_compiled", "requireCompiled"))), None),
    ("author", "current-time", "set"): (ApiClass.LOCAL_ONLY, "write", "local_only", None, "the CLI's ergonomic flags compile to the semantic authoring API request"),
    ("author", "scene", "create"): (ApiClass.LOCAL_ONLY, "write", "local_only", None, "the CLI's ergonomic flags compile to the semantic authoring API request"),
    ("author", "scene", "advance"): (ApiClass.LOCAL_ONLY, "write", "local_only", None, "the CLI's ergonomic flags compile to the semantic authoring API request"),
    ("author", "scene", "close"): (ApiClass.LOCAL_ONLY, "write", "local_only", None, "the CLI's ergonomic flags compile to the semantic authoring API request"),
    ("author", "move"): (ApiClass.LOCAL_ONLY, "write", "local_only", None, "the CLI's ergonomic flags compile to the semantic authoring API request"),
    ("author", "conversation", "create"): (ApiClass.LOCAL_ONLY, "write", "local_only", None, "the CLI's ergonomic flags compile to the semantic authoring API request"),
    ("author", "conversation", "append"): (ApiClass.LOCAL_ONLY, "write", "local_only", None, "the CLI's ergonomic flags compile to the semantic authoring API request"),
    ("author", "hypothesis", "create"): (ApiClass.LOCAL_ONLY, "write", "local_only", None, "the CLI's ergonomic flags compile to the semantic authoring API request"),
    ("author", "hypothesis", "adopt"): (ApiClass.LOCAL_ONLY, "write", "local_only", None, "the CLI's ergonomic flags compile to the semantic authoring API request"),
    ("author", "hypothesis", "reject"): (ApiClass.LOCAL_ONLY, "write", "local_only", None, "the CLI's ergonomic flags compile to the semantic authoring API request"),
    ("author", "chronology", "replace"): (ApiClass.LOCAL_ONLY, "write", "local_only", None, "the CLI's chronology replacement compiles to the semantic authoring API request"),
    ("author", "request", "preview"): (ApiClass.ACTION, "write", "mounted", RouteBinding("POST", "/api/authoring/preview", auth=AuthPolicy.SESSION), None),
    ("author", "request", "apply"): (ApiClass.MUTATION, "write", "mounted", RouteBinding("POST", "/api/authoring/apply", auth=AuthPolicy.SESSION, headers=(HeaderContract("confirm", "X-Wedl-Confirmation"),)), None),
    ("changeset", "scaffold"): (ApiClass.ACTION, "write", "mounted", RouteBinding("POST", "/api/changesets/scaffold", auth=AuthPolicy.SESSION), None),
    ("changeset", "schema"): (ApiClass.QUERY, "read", "mounted", RouteBinding("GET", "/api/changesets/schema"), None),
    ("changeset", "preview"): (ApiClass.ACTION, "write", "mounted", RouteBinding("POST", "/api/changesets/preview", auth=AuthPolicy.SESSION), None),
    ("changeset", "apply"): (ApiClass.MUTATION, "write", "mounted", RouteBinding("POST", "/api/changesets/apply", auth=AuthPolicy.SESSION, headers=(HeaderContract("confirm", "X-Wedl-Confirmation"),)), None),
    ("serve",): (ApiClass.LOCAL_ONLY, "local", "local_only", None, "owns a local process and browser lifecycle"),
}


# Discovery data is deliberately kept with the route policy.  It describes
# the stable public shape of an operation, while the parser remains the one
# source of truth for argument names, defaults, and validation.  Rich result
# schemas belong to the result producers; this small vocabulary is useful even
# for endpoints whose result shape evolves.
_DISCOVERY: dict[tuple[str, ...], DiscoveryDescriptor] = {
    ("status",): DiscoveryDescriptor("Read repository status", "Return the current repository revision, active scene, and compile-cache readiness without compiling.", ("Repository",), "Repository status.", ("parse_error", "repository_error")),
    ("validate",): DiscoveryDescriptor("Validate the world", "Return diagnostics for the current source world. An invalid world is a successful report with valid=false.", ("Repository",), "Validation report.", ("parse_error", "repository_error")),
    ("compile",): DiscoveryDescriptor("Compile the world", "Compile the current HEAD. Compilation failures are reported as source validation or repository errors.", ("Repository",), "Compilation result.", ("authentication_required", "usage_error", "validation_failed", "parse_error", "repository_error")),
    ("entity", "list"): DiscoveryDescriptor("List entities", "List canonical lore from the compiled world; author possibilities use the dedicated hypotheses read.", ("Queries",), "Entity list.", ("usage_error", "compile_required", "validation_failed", "parse_error", "repository_error")),
    ("entity", "show"): DiscoveryDescriptor("Read an entity", "Resolve canonical lore by identifier, title, alias, or slug. Author possibilities use the dedicated hypotheses read.", ("Queries",), "Entity.", ("usage_error", "not_found", "compile_required", "validation_failed", "parse_error", "repository_error")),
    ("state",): DiscoveryDescriptor("Read entity state", "Resolve an entity's state at the required tick and optional timeline/order.", ("Queries",), "Entity state.", ("usage_error", "not_found", "compile_required", "validation_failed", "parse_error", "repository_error")),
    ("knowledge",): DiscoveryDescriptor("Read character knowledge", "Resolve a character's knowledge at the required tick and optional timeline/order.", ("Queries",), "Character knowledge.", ("usage_error", "not_found", "compile_required", "validation_failed", "parse_error", "repository_error")),
    ("interactions",): DiscoveryDescriptor("Read interactions", "Resolve interactions between the two required entity references.", ("Queries",), "Interactions.", ("usage_error", "not_found", "compile_required", "validation_failed", "parse_error", "repository_error")),
    ("story-points",): DiscoveryDescriptor("List story points", "List story points scoped by an optional scene and timeline position.", ("Queries",), "Story points.", ("usage_error", "compile_required", "validation_failed", "parse_error", "repository_error")),
    ("timeline",): DiscoveryDescriptor("Read an ordinal story timeline", "Return all chronology points and inclusive spans for one declared timeline. Tick gaps are ordinal sequence only and do not represent elapsed duration.", ("Queries",), "Timeline chronology.", ("usage_error", "compile_required", "validation_failed", "parse_error", "repository_error")),
    ("chronology", "catalog"): DiscoveryDescriptor("Read chronology catalogue", "Return the public calendar catalogue and capability without exposing kernel identifiers.", ("Chronology",), "Chronology catalogue.", ("usage_error", "compile_required", "validation_failed", "parse_error", "repository_error"), success_schema="ChronologyCatalogResponse", success_examples=({"summary": "Catalogue", "value": {"protocol": "wedl-chronology/v1", "operation": "catalog", "revision": "0" * 40, "capability": {"protocol": "wedl-chronology/v1", "sourceSchema": "wedl/v0.6", "mode": "chronology-enabled", "publicReads": True, "authoring": True, "upgradeRequired": False, "upgradeAvailable": False, "durationSemantics": "none"}, "calendars": [], "eras": [], "anchors": []}},)),
    ("chronology", "format"): DiscoveryDescriptor("Format chronology date", "Format one non-conflicting chronology value from an unwrapped wedl-chronology/v1 request.", ("Chronology",), "Chronology outcome.", ("usage_error", "compile_required", "validation_failed", "parse_error", "repository_error"), examples=({"summary": "Format", "value": {"method": "POST", "path": "/api/chronology/format", "body": {"protocol": "wedl-chronology/v1", "value": {"kind": "civil", "calendarId": "calendar_X", "year": "0"}}}},), success_schema="ChronologyFormatOutcome", success_examples=({"summary": "Formatted", "value": {"protocol": "wedl-chronology/v1", "operation": "format", "revision": "0" * 40, "outcome": "ok", "advisories": [], "result": {"value": {"kind": "civil", "calendarId": "calendar_X", "year": "0"}, "formatted": "0"}}},), request_schema="ChronologyFormatRequest"),
    ("chronology", "convert"): DiscoveryDescriptor("Convert chronology date", "Convert one non-conflicting chronology value from an unwrapped wedl-chronology/v1 request.", ("Chronology",), "Chronology outcome.", ("usage_error", "compile_required", "validation_failed", "parse_error", "repository_error"), examples=({"summary": "Convert", "value": {"method": "POST", "path": "/api/chronology/convert", "body": {"protocol": "wedl-chronology/v1", "value": {"kind": "civil", "calendarId": "calendar_X", "year": "0"}, "target": {"calendarId": "calendar_X"}}}},), success_schema="ChronologyConvertOutcome", success_examples=({"summary": "Converted", "value": {"protocol": "wedl-chronology/v1", "operation": "convert", "revision": "0" * 40, "outcome": "ok", "advisories": [], "result": {"source": {"kind": "civil", "calendarId": "calendar_X", "year": "0"}, "target": {"kind": "civil", "calendarId": "calendar_X", "year": "0"}, "formatted": "0", "axisDay": "0"}}},), request_schema="ChronologyConvertRequest"),
    ("chronology", "search"): DiscoveryDescriptor("Search chronology annotations", "Search annotations from an unwrapped wedl-chronology/v1 request.", ("Chronology",), "Chronology outcome.", ("usage_error", "compile_required", "validation_failed", "parse_error", "repository_error"), examples=({"summary": "Search", "value": {"method": "POST", "path": "/api/chronology/search", "body": {"protocol": "wedl-chronology/v1", "predicate": "on_date", "value": {"kind": "civil", "calendarId": "calendar_X", "year": "0"}}}},), success_schema="ChronologySearchOutcome", success_examples=({"summary": "Matches", "value": {"protocol": "wedl-chronology/v1", "operation": "search", "revision": "0" * 40, "outcome": "ok", "advisories": [], "result": {"request": {"predicate": "on_date", "limit": 100}, "matches": []}}},), request_schema="ChronologySearchRequest"),
    ("chronology", "story-times"): DiscoveryDescriptor("Map chronology date to story times", "Map through explicit anchors only from an unwrapped wedl-chronology/v1 request.", ("Chronology",), "Chronology outcome.", ("usage_error", "compile_required", "validation_failed", "parse_error", "repository_error"), examples=({"summary": "Map", "value": {"method": "POST", "path": "/api/chronology/story-times", "body": {"protocol": "wedl-chronology/v1", "value": {"kind": "civil", "calendarId": "calendar_X", "year": "0"}}}},), success_schema="ChronologyStoryTimesOutcome", success_examples=({"summary": "Mapping", "value": {"protocol": "wedl-chronology/v1", "operation": "story-times", "revision": "0" * 40, "outcome": "ok", "advisories": [], "result": {"mapping": "none", "storyTimes": []}}},), request_schema="ChronologyStoryTimesRequest"),
    **{("spatial", action): _spatial_descriptor(action) for action in _SPATIAL_EXAMPLES},
    **{("generational", action): _generational_descriptor(action) for action in _GENERATIONAL_OPERATIONS},
    ("generational", "scaffold"): DiscoveryDescriptor("Scaffold a generational intent", "Create a current-HEAD v0.7 organization starter for explicit editing and preview.", ("Generational",), "Generational authoring starter.", ("authentication_required", "usage_error", "parse_error", "repository_error")),
    ("generational", "schema"): DiscoveryDescriptor("Get generational intent schema", "Enumerate the closed eight-kind v0.7 create, append, correct, and batch variants.", ("Generational",), "Generational intent catalogue.", ("authentication_required",)),
    ("threads",): DiscoveryDescriptor("List narrative thread labels", "Return only declared optional narrative grouping labels for the shared world; memberships, state, time, and retrieval data are not exposed.", ("Queries",), "Narrative thread catalog.", ("compile_required", "validation_failed", "parse_error", "repository_error")),
    ("thread-memberships",): DiscoveryDescriptor("Project selected narrative memberships", "Return the selected narrative-group intersection for supplied canonical records only. It exposes neither unselected membership nor state, time, or retrieval data.", ("Queries",), "Selected narrative membership projection.", ("usage_error", "compile_required", "validation_failed", "parse_error", "repository_error")),
    ("whereabouts",): DiscoveryDescriptor("Read character whereabouts", "Return canonical and retired characters' explicit location state and journey history plus a calculated, noncanonical prominence breakdown at one author horizon. The result does not infer routes, travel, group membership, or knowledge.", ("Queries",), "Character whereabouts.", ("usage_error", "not_found", "compile_required", "validation_failed", "parse_error", "repository_error")),
    ("hypotheses",): DiscoveryDescriptor("Read author possibilities", "Return explicitly non-canonical author hypotheses. They are never horizon-scoped facts and do not affect state, causality, whereabouts, scenes, or character context.", ("Queries",), "Non-canonical possibilities.", ("usage_error", "not_found", "compile_required", "validation_failed", "parse_error", "repository_error")),
    ("causal",): DiscoveryDescriptor("Trace authored event causality", "Return the explicit event.cause DAG around one event, clipped at an author horizon. No edge is inferred from proximity, shared cast, or prose.", ("Queries",), "Causal event DAG.", ("usage_error", "not_found", "compile_required", "validation_failed", "parse_error", "repository_error")),
    ("search",): DiscoveryDescriptor("Search the world", "Search indexed content with author or character perspective and optional temporal scope. Hypotheses require explicit author opt-in and are never canonical facts.", ("Queries",), "Search results.", ("usage_error", "compile_required", "validation_failed", "parse_error", "repository_error")),
    ("context",): DiscoveryDescriptor("Build context", "Build a bounded retrieval context for the selected character, scene, and query.", ("Queries",), "Context result.", ("usage_error", "compile_required", "validation_failed", "parse_error", "repository_error")),
    ("conversation", "show"): DiscoveryDescriptor("Read a conversation", "Resolve a conversation from an author or character perspective at a timeline position.", ("Queries",), "Conversation.", ("usage_error", "not_found", "compile_required", "validation_failed", "parse_error", "repository_error")),
    ("author", "request", "preview"): DiscoveryDescriptor("Preview an authoring intent", "Resolve title or alias references and compile a semantic authoring intent to the existing raw changeset protocol without writes.", ("Authoring",), "Authoring preview.", ("authentication_required", "usage_error", "not_found", "protocol_error", "upgrade_required", "parse_error", "repository_error"), examples=_generational_intent_examples("/api/authoring/preview", apply=False)),
    ("author", "request", "apply"): DiscoveryDescriptor("Apply an authoring intent", "Resolve a semantic authoring intent, require a preview confirmation, and apply its compiled raw changeset.", ("Authoring",), "Authoring application result.", ("authentication_required", "usage_error", "not_found", "protocol_error", "upgrade_required", "validation_failed", "confirmation_required", "confirmation_mismatch", "conflict", "stale_revision", "dirty_managed_tree", "parse_error", "repository_error"), examples=_generational_intent_examples("/api/authoring/apply", apply=True)),
    ("changeset", "scaffold"): DiscoveryDescriptor("Scaffold a changeset", "Create a current-HEAD-bound starter changeset; it has no request body.", ("Changesets",), "Changeset scaffold.", ("authentication_required", "protocol_error", "parse_error", "repository_error")),
    ("changeset", "schema"): DiscoveryDescriptor("Get changeset schema", "Return the public schema for raw wedl-changeset/v1 request objects.", ("Changesets",), "Changeset schema.", ()),
    ("changeset", "preview"): DiscoveryDescriptor("Preview a changeset", "Validate a raw changeset object against the current HEAD without writes. Candidate validation is returned as valid=false, not an error response.", ("Changesets",), "Changeset preview.", ("authentication_required", "protocol_error", "parse_error", "repository_error")),
    ("changeset", "apply"): DiscoveryDescriptor("Apply a changeset", "Apply a confirmed raw changeset object and compile the resulting revision. Replays may return the stored receipt.", ("Changesets",), "Changeset application result.", ("authentication_required", "protocol_error", "validation_failed", "confirmation_required", "confirmation_mismatch", "conflict", "stale_revision", "dirty_managed_tree", "parse_error", "repository_error")),
}


_CONTROL_ENDPOINTS: tuple[ControlEndpoint, ...] = (
    ControlEndpoint(
        "GET",
        "/api/session",
        AuthPolicy.PUBLIC,
        DiscoveryDescriptor("Get session information", "Return the repository-scoped token required by protected workflow endpoints and the current HEAD.", ("Session",), "Session information.", ("repository_error",)),
    ),
    ControlEndpoint(
        "GET",
        "/",
        AuthPolicy.PUBLIC,
        DiscoveryDescriptor("Open the WEDL workspace", "Serve the local browser workspace HTML; use /api/docs for the OpenAPI document.", ("Interface",), "Workspace HTML.", ()),
    ),
)


def _explorer_descriptor(operation: str) -> DiscoveryDescriptor:
    stem = operation.capitalize()
    common = {"protocol": "wedl-spatial-explorer/v1", "revision": "0" * 40,
              "capabilities": ["spatial-core-v1", "geometry-v1", "route-v1", "overlay-v1"],
              "limit": 10, "cursor": None}
    body = {
        "catalog": {"protocol": common["protocol"], "limit": 10},
        "places": {**common, "mode": "roots"},
        "viewport": {**common, "mapId": "map_X", "bounds": {"min": [0, 0], "max": [10, 10]}, "relation": "intersects"},
        "layers": {**common, "mapId": "map_X", "bounds": {"min": [0, 0], "max": [10, 10]}, "relation": "intersects", "asOf": {"timeline": "main", "tick": "0", "order": "0"}, "audience": "author", "perspective": "author"},
        "routes": {**common, "locationId": "location_X", "direction": "outgoing", "modes": ["foot"]},
    }[operation]
    results = {
        "catalog": {"spatialAvailable": True, "maps": [{"id": "map_X", "label": "Example map", "crs": "local-planar", "axes": ["x", "y"], "unit": "pace", "bounds": {"min": [0, 0], "max": [10, 10]}}], "nextCursor": None},
        "places": {"mode": "roots", "places": [{"id": "location_X", "label": "Example place", "parentId": None, "mapId": "map_X", "geometryAvailable": True, "basis": "authored-location"}], "basis": "authored-parent-id", "nextCursor": None},
        "viewport": {"mapId": "map_X", "crs": "local-planar", "axes": ["x", "y"], "unit": "pace", "relation": "intersects", "features": [{"id": "location_X", "label": "Example place", "mapId": "map_X", "geometry": {"kind": "point", "coordinates": [1, 1]}, "basis": "authored-geometry"}], "basis": "authored-geometry-bounds", "nextCursor": None},
        "layers": {"mapId": "map_X", "crs": "local-planar", "unit": "pace", "asOf": {"timeline": "main", "tick": "0", "order": "0"}, "audience": "author", "perspective": "author", "layers": [{"overlayId": "overlay_X", "label": "Example layer", "locationId": "location_X", "geometry": {"kind": "point", "coordinates": [1, 1]}, "basis": "authorized-authored-overlay-membership"}], "basis": "authorized-authored-overlay-membership", "nextCursor": None},
        "routes": {"locationId": "location_X", "direction": "outgoing", "modes": ["foot"], "routes": [{"kind": "route", "id": "route_X", "label": "Example route", "fromLocationId": "location_X", "toLocationId": "location_Y", "authoredDirection": "one-way", "reverseOfAuthored": False, "modes": ["foot"], "availability": "open", "uncertainty": "exact", "routeDistance": {"value": 1, "unit": "pace"}, "travelCost": None, "duration": None}], "basis": "authored-directed-route-and-portal-edges", "nextCursor": None},
    }
    path = f"/api/spatial/explorer/{operation}"
    request = {"method": "GET" if operation == "catalog" else "POST", "path": path,
               "query": {"limit": 10} if operation == "catalog" else None,
               "body": None if operation == "catalog" else body}
    response = {"protocol": common["protocol"], "operation": operation, "revision": common["revision"],
                "sourceSchema": "wedl/v0.7", "capabilities": common["capabilities"],
                "cache": {"state": "ready", "revision": common["revision"], "treeOid": common["revision"], "sourceSchema": "wedl/v0.7", "fingerprint": "compiler"},
                "state": "ok", "result": results[operation]}
    requests = [request]
    responses = [response]
    if operation == "places":
        for mode, fields, basis in (
            ("children", {"parentId": "location_parent"}, "authored-parent-id"),
            ("search", {"query": "Example place"}, "compiled-title-search"),
            ("select", {"ids": ["location_X"]}, "authored-location-id"),
        ):
            requests.append({"method": "POST", "path": path, "query": None,
                             "body": {**common, "mode": mode, **fields}})
            place = {**results[operation]["places"][0],
                     "parentId": "location_parent" if mode == "children" else None}
            responses.append({**response, "result": {**results[operation], "mode": mode,
                                                       "places": [place], "basis": basis}})
    return DiscoveryDescriptor(f"Explore spatial {operation}",
        "Read one bounded, revision-pinned wedl-spatial-explorer/v1 compiled projection. Selected audience and perspective are local-author presentation filters, not trusted identity.",
        ("Spatial", "Spatial Explorer"), "Spatial explorer outcome.",
        ("usage_error", "compile_required", "validation_failed", "parse_error", "repository_error"),
        examples=tuple({"summary": f"{operation} request" if index == 0 else f"{operation} {value['body']['mode']} request",
                        "value": value} for index, value in enumerate(requests)),
        success_schema=f"SpatialExplorer{stem}OkOutcome",
        success_examples=tuple({"summary": f"{operation} outcome" if index == 0 else f"{operation} {value['result']['mode']} outcome",
                                "value": value} for index, value in enumerate(responses)),
        request_schema=f"SpatialExplorer{stem}Request")


_EXPLORER_ENDPOINTS = tuple(ControlEndpoint("GET" if action == "catalog" else "POST",
    f"/api/spatial/explorer/{action}", AuthPolicy.PUBLIC, _explorer_descriptor(action))
    for action in ("catalog", "places", "viewport", "layers", "routes"))


def _request_example(command: tuple[str, ...], binding: RouteBinding) -> tuple[dict[str, Any], ...]:
    """Return one small, transport-focused example without claiming result shapes."""

    values: dict[tuple[str, ...], dict[str, Any]] = {
        ("status",): {},
        ("validate",): {},
        ("compile",): {"headers": {"X-Wedl-Token": "<session-token>"}},
        ("entity", "list"): {"query": {"kind": "character"}},
        ("entity", "show"): {"pathParams": {"entity_id": "character-mara-vale"}},
        ("state",): {"pathParams": {"entity_id": "character-mara-vale"}, "query": {"tick": 12}},
        ("knowledge",): {"pathParams": {"character_id": "character-mara-vale"}, "query": {"tick": 12}},
        ("interactions",): {"query": {"first": "character-mara-vale", "second": "character-ilyra-sorn"}},
        ("story-points",): {"query": {"scene": "scene-market-day", "tick": 12}},
        ("timeline",): {"query": {"timeline": "main"}},
        ("chronology", "catalog"): {},
        ("chronology", "format"): {"body": {"protocol": "wedl-chronology/v1", "value": {"kind": "civil", "calendarId": "calendar_X", "year": "0"}}},
        ("chronology", "convert"): {"body": {"protocol": "wedl-chronology/v1", "value": {"kind": "civil", "calendarId": "calendar_X", "year": "0"}, "target": {"calendarId": "calendar_X"}}},
        ("chronology", "search"): {"body": {"protocol": "wedl-chronology/v1", "predicate": "on_date", "value": {"kind": "civil", "calendarId": "calendar_X", "year": "0"}}},
        ("chronology", "story-times"): {"body": {"protocol": "wedl-chronology/v1", "value": {"kind": "civil", "calendarId": "calendar_X", "year": "0"}}},
        **{("spatial", action): {"body": {"protocol": "wedl-spatial/v1", "revision": "0" * 40, "capabilities": ["spatial-core-v1", "geometry-v1", "route-v1", "overlay-v1"], "limit": 10, "cursor": None, **({"locationId": "location_X"} if action in {"containment", "children", "adjacency"} else {"fromLocationId": "location_X"} if action == "reachability" else {"fromLocationId": "location_X", "toLocationId": "location_Y", "metric": "routeDistance"} if action == "path" else {"mapId": "map_X", "bounds": {"min": [0, 0], "max": [1, 1]}, "relation": "intersects"} if action == "bbox" else {"position": {"mapId": "map_X", "coordinates": [0, 0]}, "radius": 1} if action == "nearby" else {"queryScope": "location", "locationId": "location_X", "audience": "author", "perspective": "author", "asOf": {"timeline": "main", "tick": "0", "order": "0"}})}} for action in ("containment", "children", "bbox", "nearby", "adjacency", "reachability", "path", "overlay-as-of")},
        **{("generational", action): {"headers": {"X-Wedl-Token": "<session-token>"}, "body": _generational_example(action)} for action in _GENERATIONAL_OPERATIONS},
        ("generational", "scaffold"): {"headers": {"X-Wedl-Token": "<session-token>"}},
        ("generational", "schema"): {"headers": {"X-Wedl-Token": "<session-token>"}},
        ("threads",): {},
        ("thread-memberships",): {"query": {"recordId": "event_0123456789ABCDEFGHJKMNPQRS", "threadId": "thread_0123456789ABCDEFGHJKMNPQRS"}},
        ("whereabouts",): {"query": {"character": "Mara Vale", "tick": 12}},
        ("hypotheses",): {"query": {"status": "open"}},
        ("causal",): {"pathParams": {"event_id": "event-market-alarm"}, "query": {"direction": "upstream", "tick": 12}},
        ("search",): {"query": {"q": "harbour ledger", "perspective": "author"}},
        ("context",): {"query": {"character": "character-mara-vale", "scene": "scene-market-day", "q": "harbour ledger"}},
        ("conversation", "show"): {"pathParams": {"conversation_id": "conversation-market-meeting"}, "query": {"perspective": "author"}},
        ("changeset", "scaffold"): {"headers": {"X-Wedl-Token": "<session-token>"}},
        ("changeset", "schema"): {},
        ("changeset", "preview"): {"headers": {"X-Wedl-Token": "<session-token>"}, "body": {"protocol": "wedl-changeset/v1", "expectedHead": "<current-head>", "idempotencyKey": "example-preview", "summary": "Inspect a proposed change", "operations": []}},
        ("changeset", "apply"): {"headers": {"X-Wedl-Token": "<session-token>", "X-Wedl-Confirmation": "wedl-confirmation/v1:<proof>"}, "body": {"protocol": "wedl-changeset/v1", "expectedHead": "<current-head>", "idempotencyKey": "example-apply", "summary": "Apply a proposed change", "operations": []}},
        ("author", "request", "preview"): {"headers": {"X-Wedl-Token": "<session-token>"}, "body": {"action": "conversation.append", "conversation": "Market meeting", "speaker": "Mara Vale", "text": "Close the ledger."}},
        ("author", "request", "apply"): {"headers": {"X-Wedl-Token": "<session-token>", "X-Wedl-Confirmation": "wedl-confirmation/v1:<proof>"}, "body": {"action": "character.move", "characters": ["Mara Vale"], "location": "Flood Stair", "time": {"timeline": "main", "tick": 12, "order": 20}}},
    }
    value = {"method": binding.method, "path": binding.path, **values[command]}
    return ({"summary": "Example request", "value": value},)


_DISCOVERY = {
    command: DiscoveryDescriptor(
        descriptor.summary,
        descriptor.description,
        descriptor.tags,
        descriptor.success_description,
        (
            (*descriptor.errors, "v04_superseded")
            if "parse_error" in descriptor.errors
            else descriptor.errors
        ),
        descriptor.examples or _request_example(command, binding),
        descriptor.success_schema or operation_schema(" ".join(command)),
        descriptor.success_examples or operation_example(" ".join(command)),
        descriptor.success_media_type,
        descriptor.request_schema,
    )
    for command, descriptor in _DISCOVERY.items()
    for _api_class, _effect, availability, binding, _reason in (_POLICY[command],)
    if availability == "mounted" and binding is not None
}


def _control_example(endpoint: ControlEndpoint) -> ControlEndpoint:
    descriptor = endpoint.descriptor
    return ControlEndpoint(
        endpoint.method,
        endpoint.path,
        endpoint.auth,
        DiscoveryDescriptor(
            descriptor.summary,
            descriptor.description,
            descriptor.tags,
            descriptor.success_description,
            descriptor.errors,
            ({"summary": "Example request", "value": {"method": endpoint.method, "path": endpoint.path}},),
            operation_schema("session" if endpoint.path == "/api/session" else "root"),
            operation_example("session" if endpoint.path == "/api/session" else "root"),
            "text/html" if endpoint.path == "/" else "application/json",
        ),
    )


_CONTROL_ENDPOINTS = tuple(_control_example(endpoint) for endpoint in _CONTROL_ENDPOINTS)


# Error names are intentionally semantic rather than tied to a single handler
# implementation.  New errors can be added here without duplicating schema
# fragments on every route.
ERROR_VOCABULARY: Mapping[str, tuple[int, str]] = {
    "usage_error": (400, "The request does not satisfy the command contract."),
    "validation_failed": (400, "The world or changeset failed validation."),
    "confirmation_required": (400, "The confirmation header is required."),
    "confirmation_mismatch": (400, "The confirmation does not match this request and HEAD."),
    "not_found": (400, "The requested WEDL object was not found."),
    "compile_required": (400, "The requested read requires a compiled world."),
    "authentication_required": (401, "A valid repository session token is required."),
    "conflict": (409, "The repository changed or cannot accept this operation."),
    "stale_revision": (409, "The source revision no longer matches the repository."),
    "dirty_managed_tree": (409, "The managed repository tree has uncommitted changes."),
    "repository_error": (400, "The repository could not satisfy the operation."),
    "parse_error": (400, "WEDL source parsing failed."),
    "protocol_error": (400, "The changeset protocol is structurally invalid for this operation."),
    "v04_superseded": (400, "The withdrawn wedl/v0.4 source schema must be recovered before use."),
    "upgrade_required": (400, "The source schema must be upgraded before chronology authoring."),
}


def error_status(code: str) -> int:
    """Return the HTTP status for a stable WEDL error code.

    Unknown implementation errors retain the established generic 400
    envelope; discovery codes are fail-closed where descriptors are built.
    """

    return ERROR_VOCABULARY.get(code, (400, ""))[0]


# Every parser action must be deliberately classified.  RouteBinding is the
# explicit mapping for request inputs; this table records the equally
# important non-request decisions.  There is intentionally no fallback: a
# new CLI option makes contract construction fail until its HTTP disposition
# has been reviewed.
_ARGUMENT_DISPOSITIONS: dict[str, tuple[Transport, str | None, str]] = {
    "compact": (Transport.OMIT, None, "CLI output formatting has no HTTP equivalent"),
    "repo": (Transport.INJECT, None, "the server injects its configured repository"),
    "shell": (Transport.OMIT, None, "completion scripts are emitted only by the local CLI"),
    "path": (Transport.OMIT, None, "repository creation is local-only"),
    "empty": (Transport.OMIT, None, "repository creation is local-only"),
    "example": (Transport.OMIT, None, "repository creation is local-only"),
    "no_git": (Transport.OMIT, None, "repository creation is local-only"),
    "output": (Transport.OMIT, None, "file output is a local CLI concern"),
    "use_current_head": (Transport.OMIT, None, "the server always operates against its current repository HEAD"),
    "confirm": (Transport.OMIT, None, "HTTP confirmation is supplied by the X-Wedl-Confirmation header"),
    "yes": (Transport.OMIT, None, "the unsafe CLI confirmation bypass is never exposed over HTTP"),
    "host": (Transport.OMIT, None, "server process lifecycle is local-only"),
    "port": (Transport.OMIT, None, "server process lifecycle is local-only"),
    "open": (Transport.OMIT, None, "browser launching is local-only"),
    "file": (Transport.OMIT, None, "the command is not mounted by the current browser API"),
    "force": (Transport.OMIT, None, "the command is not mounted by the current browser API"),
    "profile": (Transport.OMIT, None, "the command is not mounted by the current browser API"),
    "vector_provider": (Transport.OMIT, None, "the command is not mounted by the current browser API"),
    "vector_model": (Transport.OMIT, None, "the command is not mounted by the current browser API"),
    "vector_dimensions": (Transport.OMIT, None, "the command is not mounted by the current browser API"),
    "vector_max_features": (Transport.OMIT, None, "the command is not mounted by the current browser API"),
    "author_command": (Transport.OMIT, None, "the semantic authoring action is encoded in the request body"),
    "author_subject_command": (Transport.OMIT, None, "the semantic authoring action is encoded in the request body"),
    "summary": (Transport.OMIT, None, "the CLI-only summary is encoded in its semantic intent"),
    "idempotency_key": (Transport.OMIT, None, "the CLI-only retry key is encoded in its semantic intent"),
    "mode": (Transport.OMIT, None, "source migration mode is local-only"),
    "expected_head": (Transport.OMIT, None, "source migration expected-head protection is local-only"),
    "source_snapshot_hash": (Transport.OMIT, None, "source migration snapshot binding is local-only"),
    "rollback_backup_ref": (Transport.OMIT, None, "source migration rollback ref is local-only"),
    "migration_command": (Transport.OMIT, None, "source migration action is local-only"),
    "tick": (Transport.OMIT, None, "the CLI-only time field is encoded in its semantic intent"),
    "timeline": (Transport.OMIT, None, "the CLI-only time field is encoded in its semantic intent"),
    "order": (Transport.OMIT, None, "the CLI-only time field is encoded in its semantic intent"),
    "title": (Transport.OMIT, None, "the CLI-only title is encoded in its semantic intent"),
    "location": (Transport.OMIT, None, "the CLI-only location is encoded in its semantic intent"),
    "characters": (Transport.OMIT, None, "the CLI-only characters are encoded in its semantic intent"),
    "scene": (Transport.OMIT, None, "the CLI-only scene is encoded in its semantic intent"),
    "conversation": (Transport.OMIT, None, "the CLI-only conversation is encoded in its semantic intent"),
    "text": (Transport.OMIT, None, "the CLI-only text is encoded in its semantic intent"),
    "kind": (Transport.OMIT, None, "the CLI-only beat kind is encoded in its semantic intent"),
    "speaker": (Transport.OMIT, None, "the CLI-only speaker is encoded in its semantic intent"),
    "addressee": (Transport.OMIT, None, "the CLI-only addressee is encoded in its semantic intent"),
    "actors": (Transport.OMIT, None, "the CLI-only actors are encoded in its semantic intent"),
    "interrupt_last": (Transport.OMIT, None, "the CLI-only interruption flag is encoded in its semantic intent"),
    "hypothesis": (Transport.OMIT, None, "the CLI-only possibility is encoded in its semantic intent"),
    "statement": (Transport.OMIT, None, "the CLI-only hypothesis statement is encoded in its semantic intent"),
    "subjects": (Transport.OMIT, None, "the CLI-only hypothesis subjects are encoded in its semantic intent"),
    "alternatives": (Transport.OMIT, None, "the CLI-only hypothesis alternatives are encoded in its semantic intent"),
    "context": (Transport.OMIT, None, "the CLI-only hypothesis context is encoded in its semantic intent"),
    "event": (Transport.OMIT, None, "the CLI-only hypothesis placement is encoded in its semantic intent"),
    "canonical_records": (Transport.OMIT, None, "the CLI-only canonical references are encoded in its semantic intent"),
    "note": (Transport.OMIT, None, "the CLI-only lifecycle note is encoded in its semantic intent"),
}


def _argument_contract(action: argparse.Action, binding: RouteBinding | None) -> ArgumentContract:
    value_type = getattr(action.type, "value_type", "string")
    if action.type is int:
        value_type = "integer"
    elif action.type is float:
        value_type = "number"
    elif action.nargs == 0:
        value_type = "boolean"
    aliases = dict(binding.parameters) if binding is not None else {}
    headers = {header.dest: header for header in binding.headers} if binding is not None else {}
    if action.dest in aliases:
        transport = Transport.QUERY
        name = aliases[action.dest]
        reason = "mapped from its CLI argument to the established HTTP parameter"
    elif action.dest in headers:
        transport = Transport.HEADER
        name = headers[action.dest].name
        reason = "mapped from its CLI argument to the established HTTP header"
    elif action.dest in {"entity", "character", "conversation", "event"} and binding is not None:
        transport = Transport.PATH
        name = {"entity": "entity_id", "character": "character_id", "conversation": "conversation_id", "event": "event_id"}[action.dest]
        if "{" + name + "}" not in binding.path:
            try:
                transport, name, reason = _ARGUMENT_DISPOSITIONS[action.dest]
            except KeyError as exc:
                raise RuntimeError(
                    f"HTTP argument disposition missing for {action.dest!r}; map, inject, or explicitly omit it"
                ) from exc
            return _argument_contract_with_transport(action, transport, name, reason)
        reason = "mapped to the established HTTP path parameter"
    elif action.dest == "file" and binding is not None:
        transport, name, reason = Transport.BODY, "payload", "the API accepts the parsed JSON request rather than a local file path"
    else:
        try:
            transport, name, reason = _ARGUMENT_DISPOSITIONS[action.dest]
        except KeyError as exc:
            raise RuntimeError(
                f"HTTP argument disposition missing for {action.dest!r}; map, inject, or explicitly omit it"
            ) from exc
    return _argument_contract_with_transport(action, transport, name, reason)


def _argument_contract_with_transport(
    action: argparse.Action,
    transport: Transport,
    name: str | None,
    reason: str,
) -> ArgumentContract:
    """Build the common parser metadata after a transport decision."""

    value_type = getattr(action.type, "value_type", "string")
    if action.type is int:
        value_type = "integer"
    elif action.type is float:
        value_type = "number"
    elif action.nargs == 0:
        value_type = "boolean"
    positional = not action.option_strings
    # argparse marks required positionals through ``nargs`` rather than the
    # Action.required attribute used by options.
    required = bool(action.required) or (positional and action.nargs not in ("?", "*", argparse.REMAINDER))
    transport_description = action.help
    if transport == Transport.BODY:
        if action.help and "wedl-generational/v1" in action.help:
            transport_description = (
                "Raw wedl-generational/v1 JSON object. Send the request itself, not a wrapper "
                "or local file path. CLI FILE: " + action.help
            )
        elif action.help and "wedl-spatial/v1" in action.help:
            transport_description = (
                "Raw wedl-spatial/v1 JSON object. Send the request itself, not a wrapper "
                "or local file path. CLI FILE: " + action.help
            )
        elif action.help and "authoring intent" in action.help:
            transport_description = (
                "Semantic authoring intent JSON object. Send the intent itself, not a wrapper or local file path; "
                "the server resolves names and returns its compiled wedl-changeset/v1. CLI FILE: " + action.help
            )
        else:
            transport_description = (
                "Raw wedl-changeset/v1 JSON object. Send the changeset itself, not a wrapper "
                "or local file path. CLI FILE: " + (action.help or "changeset JSON input")
            )
    return ArgumentContract(
        dest=action.dest,
        option_strings=tuple(action.option_strings),
        required=required,
        default=action.default,
        choices=tuple(action.choices) if action.choices is not None else None,
        help=action.help,
        positional=positional,
        repeated=isinstance(action, argparse._AppendAction),
        value_type=value_type,
        minimum=getattr(action.type, "minimum", None),
        maximum=getattr(action.type, "maximum", None),
        transport=transport,
        transport_name=name,
        transport_reason=reason,
        transport_description=transport_description,
        validator=action.type,
    )


def _leaf_parsers(parser: argparse.ArgumentParser, prefix: tuple[str, ...] = ()) -> Iterable[tuple[tuple[str, ...], argparse.ArgumentParser]]:
    children = [action for action in parser._actions if isinstance(action, argparse._SubParsersAction)]
    if not children:
        yield prefix, parser
        return
    for subparsers in children:
        for name, child in subparsers.choices.items():
            yield from _leaf_parsers(child, (*prefix, name))


def _global_actions(parser: argparse.ArgumentParser) -> tuple[argparse.Action, ...]:
    """Return root options inherited by every command leaf.

    Help and version terminate parsing before a command is selected, so only
    normal global inputs (currently ``--compact``) belong in leaf contracts.
    """

    return tuple(
        action
        for action in parser._actions
        if not isinstance(action, (argparse._HelpAction, argparse._VersionAction, argparse._SubParsersAction))
    )


def control_endpoints() -> tuple[ControlEndpoint, ...]:
    """Return explicit contracts for non-CLI API control endpoints."""

    expected = {("GET", "/api/session"), ("GET", "/")}
    keys = [(endpoint.method, endpoint.path) for endpoint in _CONTROL_ENDPOINTS]
    if len(keys) != len(set(keys)) or set(keys) != expected:
        raise RuntimeError(
            "control endpoint contracts must be exactly GET /api/session and GET /; "
            f"actual={keys!r}"
        )
    return _CONTROL_ENDPOINTS


def control_endpoint(method: str, path: str) -> ControlEndpoint:
    """Find one explicit control endpoint or fail closed on an accidental route."""

    for endpoint in control_endpoints():
        if endpoint.method == method and endpoint.path == path:
            return endpoint
    raise KeyError(f"no control endpoint contract for {method} {path}")


def explorer_endpoints() -> tuple[ControlEndpoint, ...]:
    """Explicit HTTP-only adjuncts; never inferred from parser leaves."""
    expected = {("GET", "/api/spatial/explorer/catalog"), *(("POST", f"/api/spatial/explorer/{name}") for name in ("places", "viewport", "layers", "routes"))}
    keys = [(endpoint.method, endpoint.path) for endpoint in _EXPLORER_ENDPOINTS]
    existing = [(endpoint.method, endpoint.path) for endpoint in control_endpoints()]
    existing.extend((contract.binding.method, contract.binding.path) for contract in route_contracts() if contract.binding is not None)
    if len(keys) != len(set(keys)) or set(keys) != expected or set(keys).intersection(existing):
        raise RuntimeError("spatial explorer endpoint registry is missing, duplicated, or conflicts with a mounted route")
    return _EXPLORER_ENDPOINTS


def explorer_request_body(descriptor: DiscoveryDescriptor) -> dict[str, Any]:
    if descriptor.request_schema is None:
        raise RuntimeError("spatial explorer endpoint has no request schema")
    value = descriptor.examples[0]["value"]["body"]
    if not isinstance(value, dict):
        raise RuntimeError("spatial explorer POST example needs a raw body")
    return {"required": True, "content": {"application/json": {
        "schema": {"$ref": f"#/components/schemas/{descriptor.request_schema}"},
        "examples": {"request": {"summary": descriptor.examples[0]["summary"], "value": value}},
    }}}


def discovery_openapi_extra(
    descriptor: DiscoveryDescriptor,
    *,
    command: tuple[str, ...] | None = None,
    auth: AuthPolicy,
) -> dict[str, Any]:
    """Return extensions shared by generated operation discovery metadata."""

    extra: dict[str, Any] = {
        "x-wedl-auth-policy": auth.value,
        "x-wedl-error-codes": list(descriptor.errors),
        "x-wedl-examples": list(descriptor.examples),
    }
    if command is not None:
        extra["x-wedl-command"] = list(command)
    return extra


def discovery_request_body(
    descriptor: DiscoveryDescriptor,
    argument: ArgumentContract,
) -> dict[str, Any]:
    """Return the standard OpenAPI request body for one raw body argument.

    The command contract's request examples include method, path, and headers
    so they are useful as complete client recipes.  OpenAPI's ``requestBody``
    examples, on the other hand, must contain only the JSON value sent on the
    wire.  Keeping the latter derived from the former makes the two forms
    unable to silently disagree.
    """

    if argument.transport != Transport.BODY or argument.transport_name != "payload":
        raise RuntimeError("only unwrapped JSON request bodies are discoverable")
    examples: dict[str, dict[str, Any]] = {}
    for index, request in enumerate(descriptor.examples, start=1):
        value = request.get("value") if isinstance(request, dict) else None
        body = value.get("body") if isinstance(value, dict) else None
        if not isinstance(body, dict):
            continue
        examples[f"request-{index}"] = {"summary": request.get("summary", "Example request"), "value": body}
    if not examples:
        raise RuntimeError(f"body descriptor {descriptor.summary!r} has no raw JSON example")
    return {
        "description": argument.transport_description,
        "required": argument.required,
        "content": {
            "application/json": {
                "schema": {"$ref": f"#/components/schemas/{descriptor.request_schema or ('AuthoringRequest' if 'Authoring' in descriptor.tags else 'ChangesetRequest')}"},
                "examples": examples,
            }
        },
    }


def discovery_responses(descriptor: DiscoveryDescriptor) -> dict[int, dict[str, Any]]:
    """Generate structural OpenAPI responses from the semantic error vocabulary."""

    if descriptor.success_schema is None:
        raise RuntimeError(f"discovery descriptor {descriptor.summary!r} has no success schema")
    responses: dict[int, dict[str, Any]] = {
        200: {
            "description": descriptor.success_description,
            "content": {
                descriptor.success_media_type: {
                    "schema": {"$ref": f"#/components/schemas/{descriptor.success_schema}"},
                    "examples": {"success": example for example in descriptor.success_examples},
                }
            },
        }
    }
    by_status: dict[int, list[str]] = {}
    for error in descriptor.errors:
        try:
            status, _description = ERROR_VOCABULARY[error]
        except KeyError as exc:
            raise RuntimeError(f"unknown WEDL discovery error {error!r}") from exc
        by_status.setdefault(status, []).append(error)
    for status, errors in by_status.items():
        responses[status] = {
            "description": f"WEDL error response ({', '.join(errors)}).",
            "x-wedl-error-codes": errors,
            "content": {
                "application/json": {
                    "schema": {"$ref": "#/components/schemas/WedlError"},
                    "examples": {
                        code: {
                            "summary": code.replace("_", " "),
                            "value": {"code": code, "message": ERROR_VOCABULARY[code][1], "details": {}},
                        }
                        for code in errors
                    }
                }
            },
        }
    if "Spatial" in descriptor.tags:
        # Spatial query results have their own closed state machine.  Keep the
        # ordinary WEDL error alternative on 400 so malformed HTTP requests
        # remain documented without relabelling a semantic `invalid` result.
        for status, state in ((400, "invalid"), (403, "forbidden"), (409, "unavailable"), (422, "limit")):
            if not descriptor.success_schema.endswith("OkOutcome"):
                raise RuntimeError(f"spatial descriptor has no status-specific success schema: {descriptor.success_schema}")
            spatial = {"$ref": f"#/components/schemas/{descriptor.success_schema.removesuffix('OkOutcome')}{state.capitalize()}Outcome"}
            existing = responses.get(status)
            schema: dict[str, Any] = spatial
            if existing is not None:
                schema = {"oneOf": [spatial, {"$ref": "#/components/schemas/WedlError"}]}
            responses[status] = {
                "description": f"Spatial `{state}` outcome." + (" Ordinary WEDL errors may also use this status." if existing is not None else ""),
                "x-wedl-spatial-state": state,
                "content": {"application/json": {"schema": schema}},
            }
            if "Spatial Explorer" in descriptor.tags:
                example = dict(descriptor.success_examples[0]["value"])
                example.pop("result")
                example["state"] = state
                example["code"] = {"invalid": "SPATIAL-CURSOR-001", "unavailable": "SPATIAL-GEOMETRY-001",
                                   "forbidden": "SPATIAL-OVERLAY-001", "limit": "SPATIAL-LIMIT-001"}[state]
                responses[status]["content"]["application/json"]["examples"] = {
                    state: {"summary": f"{state} explorer outcome", "value": example}
                }
    if "Generational" in descriptor.tags and descriptor.success_schema.endswith("AvailableOutcome"):
        for status, state in ((200, "unknown"), (400, "invalid"), (409, "unavailable"), (422, "limit")):
            suffix = descriptor.success_schema.removesuffix("AvailableOutcome")
            semantic = {"$ref": f"#/components/schemas/{suffix}{state.capitalize()}Outcome"}
            existing = responses.get(status)
            if status == 200:
                success = responses[200]["content"]["application/json"]["schema"]
                responses[200]["content"]["application/json"]["schema"] = {"oneOf": [success, semantic]}
                continue
            schema = semantic if existing is None else {"oneOf": [semantic, {"$ref": "#/components/schemas/WedlError"}]}
            responses[status] = {
                "description": f"Generational `{state}` outcome." + (" Ordinary WEDL errors may also use this status." if existing is not None else ""),
                "x-wedl-generational-state": state,
                "content": {"application/json": {"schema": schema}},
            }
    return responses


def discovery_components() -> dict[str, dict[str, Any]]:
    """Return reusable, intentionally structural WEDL error OpenAPI components."""

    error_schema = {
        "type": "object",
        "required": ["code", "message", "details"],
        "properties": {
            "code": {"type": "string", "description": "Stable WEDL error code."},
            "message": {"type": "string", "description": "Human-readable error message."},
            "details": {"type": "object", "additionalProperties": True, "description": "Optional structured error details."},
        },
        "additionalProperties": True,
    }
    responses: dict[str, dict[str, Any]] = {}
    for status in sorted({status for status, _description in ERROR_VOCABULARY.values()}):
        codes = sorted(code for code, (code_status, _description) in ERROR_VOCABULARY.items() if code_status == status)
        responses[f"Wedl{status}Error"] = {
            "description": f"WEDL error response ({', '.join(codes)}).",
            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/WedlError"}}},
            "x-wedl-error-codes": codes,
        }
    registry = response_schema_components()
    registry["schemas"]["WedlError"] = error_schema
    registry["responses"] = responses
    return registry


def normalize_discovery_openapi(document: dict[str, Any]) -> dict[str, Any]:
    """Attach WEDL components and replace FastAPI's 422 documentation.

    Request validation is handled by the server's ``RequestValidationError``
    handler and is always returned as the documented WEDL 400 envelope.  The
    framework-generated 422 response would therefore be an unreachable and
    misleading operation response.
    """

    components = document.setdefault("components", {})
    for category, values in discovery_components().items():
        components.setdefault(category, {}).update(values)
    methods = {"get", "post", "put", "patch", "delete", "head", "options", "trace"}
    for path_item in document.get("paths", {}).values():
        for method, operation in path_item.items():
            semantic_gen_read = (isinstance(operation, dict) and
                                 operation.get("operationId") in {f"generational_{action}_post" for action in _GENERATIONAL_OPERATIONS})
            if method in methods and isinstance(operation, dict) and "Spatial" not in operation.get("tags", ()) and not semantic_gen_read:
                operation.get("responses", {}).pop("422", None)
    # FastAPI merges ``openapi_extra`` with its generated body schema.  That
    # is helpful for ordinary models but would leave a misleading hybrid here
    # (a raw payload schema plus framework-generated wrapper details).  The
    # public contract owns these two raw changeset bodies outright.
    for contract in route_contracts():
        if contract.binding is None or contract.discovery is None:
            continue
        body = next((argument for argument in contract.arguments if argument.transport == Transport.BODY), None)
        if body is None:
            continue
        operation = document.get("paths", {}).get(contract.binding.path, {}).get(contract.binding.method.lower())
        if isinstance(operation, dict):
            operation["requestBody"] = discovery_request_body(contract.discovery, body)
            # FastAPI's OpenAPI encoder drops ``None`` from extension example
            # objects. Raw protocol requests use explicit JSON null (notably
            # the required spatial cursor), so restore the contract-owned
            # examples verbatim after framework normalization.
            operation["x-wedl-examples"] = list(contract.discovery.examples)
            if contract.command[0] == "generational":
                operation["responses"]["200"]["content"]["application/json"]["examples"] = {
                    "success": example for example in contract.discovery.success_examples
                }
    for endpoint in explorer_endpoints():
        operation = document.get("paths", {}).get(endpoint.path, {}).get(endpoint.method.lower())
        if isinstance(operation, dict):
            operation["x-wedl-examples"] = list(endpoint.descriptor.examples)
            operation["responses"]["200"]["content"]["application/json"]["examples"] = {
                "success": example for example in endpoint.descriptor.success_examples
            }
            if endpoint.method == "POST":
                operation["requestBody"] = explorer_request_body(endpoint.descriptor)
    # These are introduced solely by FastAPI's default 422 schema.  Removing
    # the unreachable response must also remove its unreachable components so
    # the document has one explicit structural vocabulary.
    schemas = components.get("schemas", {})
    schemas.pop("HTTPValidationError", None)
    schemas.pop("ValidationError", None)
    return document


@lru_cache(maxsize=1)
def command_contracts() -> tuple[CommandContract, ...]:
    """Return a contract for every parser leaf, failing closed on drift."""

    grammar = command_parser.parser()
    leaves = dict(_leaf_parsers(grammar))
    inherited = _global_actions(grammar)
    missing = set(leaves).difference(_POLICY)
    stale = set(_POLICY).difference(leaves)
    if missing or stale:
        raise RuntimeError(f"HTTP contract policy does not match parser leaves; missing={sorted(missing)!r}, stale={sorted(stale)!r}")
    mounted = {command for command, (_class, _effect, availability, _binding, _reason) in _POLICY.items() if availability == "mounted"}
    missing_discovery = mounted.difference(_DISCOVERY)
    stale_discovery = set(_DISCOVERY).difference(mounted)
    if missing_discovery or stale_discovery:
        raise RuntimeError(
            "HTTP discovery descriptors do not match mounted commands; "
            f"missing={sorted(missing_discovery)!r}, stale={sorted(stale_discovery)!r}"
        )
    contracts = []
    for command, leaf in leaves.items():
        api_class, effect, availability, binding, reason = _POLICY[command]
        arguments = tuple(
            _argument_contract(action, binding)
            for action in (*inherited, *leaf._actions)
            if not isinstance(action, (argparse._HelpAction, argparse._SubParsersAction))
        )
        contracts.append(CommandContract(command, arguments, api_class, effect, availability, binding, reason, _DISCOVERY.get(command)))
    return tuple(contracts)


def route_contracts() -> tuple[CommandContract, ...]:
    """Return the subset currently implemented by the HTTP server."""

    return tuple(contract for contract in command_contracts() if contract.availability == "mounted")


def validate_discovery_descriptors() -> None:
    """Fail closed when a descriptor lacks a schema, example, or known error."""

    schemas = discovery_components()["schemas"]
    descriptors = [contract.discovery for contract in route_contracts()]
    descriptors.extend(endpoint.descriptor for endpoint in control_endpoints())
    descriptors.extend(endpoint.descriptor for endpoint in explorer_endpoints())
    for descriptor in descriptors:
        if descriptor is None or not descriptor.success_schema or descriptor.success_schema not in schemas:
            raise RuntimeError("mounted discovery descriptor has no registered success schema")
        if not descriptor.examples or not descriptor.success_examples:
            raise RuntimeError(f"discovery descriptor {descriptor.summary!r} has no examples")
        for example in (*descriptor.examples, *descriptor.success_examples):
            if not isinstance(example, dict) or "value" not in example:
                raise RuntimeError(f"discovery descriptor {descriptor.summary!r} has malformed example")
        unknown = set(descriptor.errors).difference(ERROR_VOCABULARY)
        if unknown:
            raise RuntimeError(f"discovery descriptor {descriptor.summary!r} has unknown errors {sorted(unknown)!r}")


def contract_for_route(method: str, path: str) -> CommandContract | None:
    """Find a mounted binding, including routes with one path placeholder."""

    for contract in route_contracts():
        assert contract.binding is not None
        expected = contract.binding.path
        if contract.binding.method == method and _path_matches(expected, path):
            return contract
    return None


def _path_matches(template: str, path: str) -> bool:
    template_parts = template.strip("/").split("/")
    path_parts = path.strip("/").split("/")
    return len(template_parts) == len(path_parts) and all(
        expected.startswith("{") and expected.endswith("}") or expected == actual
        for expected, actual in zip(template_parts, path_parts)
    )


def validate_request_fields(method: str, path: str, fields: Iterable[str]) -> None:
    """Reject transport fields absent from the parser-derived API binding."""

    for endpoint in explorer_endpoints():
        if endpoint.method == method and endpoint.path == path:
            allowed = {"limit", "cursor", "revision", "capabilities"} if method == "GET" else set()
            unexpected = sorted(set(fields).difference(allowed))
            if unexpected:
                raise UsageError("unknown API request field", details={"unexpected": unexpected, "allowed": sorted(allowed)})
            return
    contract = contract_for_route(method, path)
    if contract is None or contract.binding is None:
        return
    unexpected = sorted(set(fields).difference(contract.binding.parameter_names))
    if unexpected:
        raise UsageError(
            "unknown API request field",
            details={
                "unexpected": unexpected,
                "allowed": sorted(contract.binding.parameter_names),
                "command": " ".join(contract.command),
            },
        )


def validate_request_values(method: str, path: str, values: Mapping[str, str]) -> None:
    """Apply parser choices and custom value validators before a handler runs."""

    validate_request_fields(method, path, values.keys())
    contract = contract_for_route(method, path)
    if contract is None: return
    for argument in contract.arguments:
        if argument.transport != Transport.QUERY or argument.transport_name is None: continue
        if argument.repeated:
            getter = getattr(values, "getlist", None)
            raw_values = getter(argument.transport_name) if getter is not None else [values.get(argument.transport_name)]
        else:
            raw_values = [values.get(argument.transport_name)]
        if raw_values == [None]:
            if argument.required:
                raise UsageError("missing required API request field", details={"field": argument.transport_name})
            continue
        for value in raw_values:
            try:
                converted = argument.validator(value) if argument.validator is not None else value
            except (TypeError, ValueError, argparse.ArgumentTypeError) as exc:
                raise UsageError("invalid API request field", details={"field": argument.transport_name, "value": value, "reason": str(exc)}) from exc
            if argument.choices is not None and converted not in argument.choices:
                raise UsageError("invalid API request field", details={"field": argument.transport_name, "value": value, "choices": list(argument.choices)})
