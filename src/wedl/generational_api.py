"""Name-first, revision-bound transport for the private generational query service.

The loopback session proves access to this repository, not a character's
identity.  Only the local author scope can currently be constructed here.
"""
from __future__ import annotations

import json
import re
from contextlib import closing
from typing import Any

from .errors import NotFound, RepositoryError, ValidationFailed
from .compiler import cache_readiness, compile_world, connect, require_database
from .conversation import scene_context_time
from .generational_context import build_generational_context
from .generational_query import CAPABILITY, PROTOCOL, TrustedViewerScope, _VisibleCandidates, discovery_connection, query_generational
from .model import ORDER_MAX, TICK_MAX, StoryTime, World
from .repository import Repository
from .util import canonical_json
from .v07 import CAPABILITY_ORDER


OPERATIONS = ("parents", "ancestors", "descendants", "relatives", "union",
              "organization", "legacy", "vital", "search", "context", "discover", "labels")
_KINDS = {"parents": "character", "ancestors": "character", "descendants": "character",
          "relatives": "character", "vital": "character", "context": "character",
          "union": "union", "organization": "organization", "legacy": "legacy"}
_SHA = re.compile(r"[0-9a-f]{40}\Z")
_DECIMAL = re.compile(r"(?:0|-?[1-9][0-9]*)\Z")
_WORD = re.compile(r"\w{1,32}\Z", re.UNICODE)
_COMMON = {"protocol", "operation", "revision", "capabilities", "mode", "timeline",
           "at", "items", "depth", "cursor"}
_FIELDS = {"search": {"text"}, "discover": {"kind", "text"}, "labels": {"ids"},
           "relatives": {"subject", "target"},
           "context": {"subject", "maxCharacters"}}


def _outcome(operation: str, revision: str | None, state: str, **details: Any) -> dict[str, Any]:
    return {"protocol": PROTOCOL, "operation": operation, "revision": revision,
            "state": state, **details}


def status_code(value: dict[str, Any]) -> int:
    return {"invalid": 400, "unavailable": 409, "limit": 422}.get(value["state"], 200)


def _point(value: Any) -> StoryTime | None:
    if not isinstance(value, dict) or set(value) != {"timeline", "tick", "order"}:
        return None
    if (not isinstance(value["timeline"], str) or not value["timeline"]
            or any(not isinstance(value[key], str) or not _DECIMAL.fullmatch(value[key])
                   for key in ("tick", "order"))):
        return None
    try:
        return StoryTime(value["timeline"], int(value["tick"]), int(value["order"]))
    except ValueError:
        return None


def _closed_request(operation: str, request: Any) -> tuple[dict[str, Any] | None, str | None]:
    if operation not in OPERATIONS or not isinstance(request, dict):
        return None, "GEN-REQUEST-001"
    fields = _FIELDS.get(operation, {"subject"})
    allowed = (_COMMON if operation in {"search", "discover"} else _COMMON - {"cursor"}) | fields
    if operation in {"discover", "labels"}:
        allowed -= {"depth"}
    if operation == "organization":
        allowed = allowed | {"includeFormerRoles"}
    required = {"protocol", "operation", "revision", "capabilities", "mode", "timeline"} | fields
    if set(request) - allowed or required - set(request):
        return None, "GEN-REQUEST-001"
    if request["protocol"] != PROTOCOL or request["operation"] != operation:
        return None, "GEN-REQUEST-001"
    revision = request["revision"]
    if not isinstance(revision, str) or not _SHA.fullmatch(revision):
        return None, "GEN-REQUEST-001"
    caps = request["capabilities"]
    if (not isinstance(caps, list) or not caps or any(not isinstance(item, str) for item in caps)
            or len(caps) != len(set(caps)) or any(item not in CAPABILITY_ORDER for item in caps)
            or caps != sorted(caps, key=CAPABILITY_ORDER.index) or CAPABILITY not in caps):
        return None, "GEN-REQUEST-001"
    mode, timeline = request["mode"], request["timeline"]
    if (not isinstance(mode, str) or mode not in {"author-as-of", "author-all-time", "character"}
            or not isinstance(timeline, str) or not timeline or len(timeline) > 256):
        return None, "GEN-REQUEST-001"
    if any(key in request for key in ("viewer", "characterId", "audience", "perspective")):
        return None, "GEN-REQUEST-001"
    if operation in {"discover", "labels"} and (mode != "author-as-of" or "at" not in request):
        return None, "GEN-REQUEST-001"
    if mode == "author-all-time":
        if "at" in request:
            return None, "GEN-REQUEST-001"
    elif "at" in request:
        point = _point(request["at"])
        if point is None or point.timeline != timeline:
            return None, "GEN-TIME-001"
    elif mode == "character":
        return None, "GEN-REQUEST-001"
    if "includeFormerRoles" in request and (request["includeFormerRoles"] is not True
                                            or mode != "author-as-of" or "at" not in request
                                            or "cursor" in request):
        return None, "GEN-REQUEST-001"
    for key in fields - {"maxCharacters", "ids"}:
        if not isinstance(request[key], str) or not request[key].strip() or len(request[key]) > 256:
            return None, "GEN-REQUEST-001"
    if operation == "discover" and (
        request["kind"] not in {"character", "organization", "legacy", "event"}
        or len(request["text"]) > 64
        or any(ord(char) < 32 for char in request["text"])
    ):
        return None, "GEN-REQUEST-001"
    if operation == "labels" and (
        not isinstance(request["ids"], list) or not 1 <= len(request["ids"]) <= 100
        or len(set(item for item in request["ids"] if isinstance(item, str))) != len(request["ids"])
        or any(not isinstance(item, str) or not item or len(item) > 256 for item in request["ids"])
    ):
        return None, "GEN-REQUEST-001"
    if operation == "search" and not _WORD.fullmatch(request["text"].casefold()):
        return None, "GEN-REQUEST-001"
    items, depth = request.get("items", 100), request.get("depth", 8)
    if type(items) is not int or not 1 <= items <= 500 or type(depth) is not int or not 0 <= depth <= 32:
        return None, "GEN-REQUEST-001"
    cursor = request.get("cursor")
    if cursor is not None and (operation not in {"search", "discover"} or not isinstance(cursor, str) or not cursor or len(cursor) > 1024):
        return None, "GEN-REQUEST-001"
    if operation in {"discover", "labels"} and items > 100:
        return None, "GEN-REQUEST-001"
    if operation == "context":
        budget = request["maxCharacters"]
        if type(budget) is not int or not 80 <= budget <= 65536 or items > 100 or depth > 16:
            return None, "GEN-REQUEST-001"
    return request, None


def _wire(value: Any) -> Any:
    """Keep every nested StoryTime coordinate exact for JSON clients."""
    if isinstance(value, list):
        return [_wire(item) for item in value]
    if isinstance(value, dict):
        result = {key: _wire(item) for key, item in value.items()}
        if {"timeline", "tick", "order"} <= set(result):
            for key in ("tick", "order"):
                if type(result[key]) is int:
                    result[key] = str(result[key])
        return result
    return value


def _discovery_database(repository: Repository, revision: str, *, require_compiled: bool) -> Any:
    """Use cache metadata and indexed rows without constructing a World."""
    readiness = cache_readiness(repository, revision)
    if readiness["state"] != "ready":
        if require_compiled:
            from .errors import CompileRequired
            raise CompileRequired("compiled cache is required; run `wedl compile` before this read",
                                  details={"cache": readiness})
        compile_world(repository, revision)
        readiness = cache_readiness(repository, revision)
        if readiness["state"] != "ready":
            raise OSError("compiled cache unavailable")
    return readiness["database"]


def _compiled_context(connection: Any) -> tuple[list[str], list[str]]:
    row = connection.execute("SELECT frontmatter_json FROM entity WHERE kind='world' LIMIT 1").fetchone()
    if row is None:
        raise OSError("compiled world metadata missing")
    source = json.loads(row[0])
    capabilities = source.get("capabilities") or []
    timelines = [item["id"] for item in source.get("timelines", ())]
    return capabilities, timelines


def bootstrap(repository: Repository, revision: str | None = None, *,
              require_compiled: bool = False) -> dict[str, Any]:
    """Expose the bounded revision envelope needed before an author read."""
    selected = repository.head() if revision is None else revision
    if not isinstance(selected, str) or not _SHA.fullmatch(selected):
        return _outcome("bootstrap", None, "invalid", code="GEN-REQUEST-001")
    try:
        database = _discovery_database(repository, selected, require_compiled=require_compiled)
        with closing(connect(database, True)) as connection:
            capabilities, timelines = _compiled_context(connection)
        if CAPABILITY not in capabilities:
            return _outcome("bootstrap", selected, "unavailable")
        return _outcome("bootstrap", selected, "available", capabilities=capabilities,
                        timelines=timelines)
    except (OSError, ValueError, RepositoryError, ValidationFailed):
        return _outcome("bootstrap", selected, "unavailable")


def _resolve(world: Any, reference: str, kind: str) -> tuple[str | None, dict[str, Any] | None]:
    try:
        record = world.find(reference, kind)
    except NotFound as exc:
        details = {key: value[:8] for key, value in exc.details.items() if key in {"candidates", "suggestions"} and isinstance(value, list)}
        return None, {"code": "GEN-REFERENCE-001", **details}
    if record.status != "canonical":
        return None, {"code": "GEN-REFERENCE-001"}
    return record.id, None


def _visible_name_world(repository: Repository, world: World, scope: TrustedViewerScope,
                        kind: str, *, require_compiled: bool) -> World | None:
    """Give name lookup only records admitted by the compiled horizon filter."""
    if kind not in {"character", "union", "organization", "legacy"}:
        return world
    at = scope.at
    if scope.mode == "author-all-time":
        at = StoryTime(scope.timeline, TICK_MAX, ORDER_MAX)
    elif at is None:
        at = world.current_time
        if at is None:
            scene = world.active_scene()
            at = scene_context_time(scene, world.default_timeline) if scene else None
    if at is None or at.timeline != scope.timeline:
        return None
    _compiled_world, database = require_database(repository, scope.revision,
                                                 require_compiled=require_compiled)
    with closing(connect(database, True)) as connection:
        visible = _VisibleCandidates(connection, scope, at)
        records = {world.world_record.id: world.world_record}
        if kind == "character":
            admitted = set()
            for audience in scope.audiences:
                for perspective in scope.perspectives:
                    admitted.update(row[0] for row in connection.execute(
                        "SELECT entity_id FROM generational_discovery_name "
                        "WHERE audience=? AND perspective=? AND timeline=? AND kind='character' "
                        "AND name=title AND (start_tick<? OR (start_tick=? AND start_order<=?))",
                        (audience, perspective, scope.timeline, at.tick, at.tick, at.order)))
            records.update({record.id: record for record in world.by_kind(kind)
                            if record.id in admitted})
        else:
            records.update({record.id: record for record in world.by_kind(kind)
                            if record.id in visible})
    return World(world.revision, world.tree_oid, records, world.root)


def execute(repository: Repository, operation: str, request: Any, *,
            require_compiled: bool = False) -> dict[str, Any]:
    """Validate first, then resolve author references at the selected revision."""
    parsed, code = _closed_request(operation, request)
    raw_revision = request.get("revision") if isinstance(request, dict) else None
    revision = raw_revision if isinstance(raw_revision, str) and _SHA.fullmatch(raw_revision) else None
    if code:
        return _outcome(operation, revision, "invalid", code=code)
    assert parsed is not None and isinstance(revision, str)
    # The repository session is not a trusted character principal.  Do not
    # load source or resolve even a caller-supplied name in that mode.
    if parsed["mode"] == "character":
        return _outcome(operation, revision, "unknown")
    if operation in {"discover", "labels"}:
        try:
            database = _discovery_database(repository, revision, require_compiled=require_compiled)
            with closing(connect(database, True)) as connection:
                capabilities, timelines = _compiled_context(connection)
                if parsed["timeline"] not in timelines:
                    return _outcome(operation, revision, "invalid", code="GEN-TIME-001")
                if capabilities != parsed["capabilities"] or CAPABILITY not in capabilities:
                    return _outcome(operation, revision, "unavailable")
                lens_rows = connection.execute(
                    "SELECT audience,perspective FROM generational_discovery_lens"
                ).fetchall()
                audiences = frozenset(row[0] for row in lens_rows)
                perspectives = frozenset(row[1] for row in lens_rows)
                if len(audiences) > 16 or len(perspectives) > 16:
                    return _outcome(operation, revision, "limit", code="GEN-LIMIT-001")
                scope = TrustedViewerScope(revision, "author-as-of", parsed["timeline"],
                                           _point(parsed["at"]), audiences, perspectives,
                                           frozenset(capabilities))
                selector = {"operation": operation}
                if operation == "discover":
                    selector.update(kind=parsed["kind"], text=parsed["text"].strip().casefold(),
                                    items=parsed.get("items", 20), cursor=parsed.get("cursor"))
                else:
                    selector["ids"] = parsed["ids"]
                result = discovery_connection(connection, scope, selector)
            return _wire(_outcome(operation, revision, result["state"],
                                  **{key: value for key, value in result.items() if key != "state"}))
        except (OSError, ValueError, RepositoryError, ValidationFailed):
            return _outcome(operation, revision, "unavailable")
    try:
        if repository.resolve(revision) != revision:
            return _outcome(operation, revision, "invalid", code="GEN-REQUEST-001")
        world = repository.load_world(revision, cache_write=False)
    except (OSError, ValueError):
        return _outcome(operation, revision, "unavailable")
    if world.revision != revision or parsed["timeline"] not in world.timeline_ids:
        return _outcome(operation, revision, "invalid", code="GEN-TIME-001")
    if world.world_record.frontmatter.get("capabilities") != parsed["capabilities"]:
        return _outcome(operation, revision, "unavailable")
    if require_compiled:
        # Preserve the ordinary WEDL compile_required error envelope. The
        # API-neutral query service deliberately maps it to unavailable.
        require_database(repository, revision, require_compiled=True)
    audiences = {"public"}
    perspectives = {"ordinary"}
    for record in world.records.values():
        if record.kind in {"organization", "parentage", "union", "affiliation", "legacy", "tenure", "claim", "vital-history"}:
            audiences.update(record.frontmatter.get("audience") or [])
            perspectives.update(record.frontmatter.get("perspectives") or [])
    # The API-neutral query limits trusted lanes to 16 each. An author cannot
    # safely receive an incomplete projection in a larger world.
    if len(audiences) > 16 or len(perspectives) > 16:
        return _outcome(operation, revision, "limit", code="GEN-LIMIT-001")
    scope = TrustedViewerScope(revision, parsed["mode"], parsed["timeline"],
                               _point(parsed["at"]) if "at" in parsed else None,
                               frozenset(audiences), frozenset(perspectives),
                               frozenset(parsed["capabilities"]))
    if operation == "search":
        selector = {"operation": "search", "text": parsed["text"]}
    else:
        name_world = _visible_name_world(repository, world, scope, _KINDS[operation],
                                         require_compiled=require_compiled)
        if name_world is None:
            return _outcome(operation, revision, "invalid", code="GEN-TIME-001")
        subject, failure = _resolve(name_world, parsed["subject"], _KINDS[operation])
        if failure:
            if _KINDS[operation] == "character":
                return _outcome(operation, revision, "unknown")
            return _outcome(operation, revision, "invalid", **failure)
        selector = {"operation": operation, "subject_id": subject}
        if operation == "relatives":
            target, failure = _resolve(name_world, parsed["target"], "character")
            if failure:
                return _outcome(operation, revision, "unknown")
            selector["target_id"] = target
    selector.update({"items": parsed.get("items", 100), "depth": parsed.get("depth", 8)})
    if "includeFormerRoles" in parsed:
        selector["includeFormerRoles"] = True
    if operation == "search":
        selector["cursor"] = parsed.get("cursor")
    if operation == "context":
        result = build_generational_context(repository, scope, selector["subject_id"],
                                            max_characters=parsed["maxCharacters"],
                                            max_items=selector["items"], max_depth=selector["depth"],
                                            require_compiled=require_compiled)
    else:
        result = query_generational(repository, scope, selector, require_compiled=require_compiled)
    outcome = _wire(_outcome(operation, revision, result["state"],
                             **{key: value for key, value in result.items() if key != "state"}))
    if operation == "context" and outcome["state"] in {"available", "unknown"}:
        budget = parsed["maxCharacters"]
        while len(canonical_json(outcome)) > budget and outcome.get("items"):
            outcome["items"].pop()
            outcome["truncated"] = True
        if len(canonical_json(outcome)) > budget:
            return _outcome(operation, revision, "limit", code="GEN-LIMIT-001")
    return outcome
