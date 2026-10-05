"""Closed source verification and shared pure semantic report admission."""
from __future__ import annotations

import json
import re
from typing import Any

from .consequence_expectations import _validate, check_time, evaluate_expectations
from .event_consequences import AuthorScope, MAX_ITEMS, ProjectionFailure, bounded
from .model import World
from .util import canonical_json

PROTOCOL = "wedl-event-consequences/v1"
_EVENT_COLLECTIONS = ("effects", "changes", "causedTransitions", "outcomes", "causalSuccessors", "currentAtHorizon", "advisories")


def full_author_scope(world: World, *additional: World) -> AuthorScope:
    """Internal local-author grant; callers authenticate before invoking it."""
    identifiers = frozenset(world.records).union(*(frozenset(value.records) for value in additional))
    return AuthorScope(world.world_record.id, identifiers, complete_families=frozenset({"state", "knowledge"}))


def admit_semantic(value: Any, *, checks: int, focus_events=(), delta: dict | None = None,
                   event: dict | None = None, limit: int = MAX_ITEMS) -> Any:
    """Count logical items once, while pricing every actual serialized copy."""
    if checks > 100:
        raise ProjectionFailure("limit")
    count = checks
    focus = set(focus_events)
    if event is not None:
        count += sum(len(event[field]) for field in _EVENT_COLLECTIONS)
    if delta is not None:
        if delta["outcome"] == "limit": raise ProjectionFailure("limit")
        if delta["outcome"] == "ok":
            focus.update(reference["id"] for reference in delta["focusEvents"])
            changes, records, sections = set(), set(), set()
            for group in [*delta["eventGroups"], {"changes": delta["unattributedChanges"], "recordChanges": delta["unattributedRecordChanges"]}]:
                changes.update(canonical_json(change) for change in group["changes"])
                for record in group["recordChanges"]:
                    records.add(record["recordId"])
                    sections.update(canonical_json([record["recordId"], section]) for section in record["sections"])
            count += len(changes) + len(records) + len(sections)
    if len(focus) > 100: raise ProjectionFailure("limit")
    return bounded(value, count + len(focus), limit)


def failure(failed: ProjectionFailure) -> dict[str, str]:
    return {"protocol": PROTOCOL, "outcome": failed.outcome, "code": failed.code, "message": failed.message}


def decode_request(raw: str | bytes) -> dict[str, Any]:
    """Decode this protocol's raw JSON without losing duplicate members."""
    def unique(pairs):
        value = {}
        for key, item in pairs:
            if key in value:
                raise ProjectionFailure("invalid")
            value[key] = item
        return value

    def non_json_constant(value):
        raise ProjectionFailure("invalid")

    try:
        text = raw.decode("utf-8") if isinstance(raw, bytes) else raw
        payload = json.loads(text, object_pairs_hook=unique, parse_constant=non_json_constant)
        _request(payload)
        return payload
    except (ValueError, UnicodeDecodeError, RecursionError):
        raise ProjectionFailure("invalid") from None


def _request(payload: Any):
    if not isinstance(payload, dict) or payload.keys() - {"protocol", "revision", "event", "at", "limit", "expectations"} or not {"protocol", "revision", "event", "at", "limit"} <= payload.keys():
        raise ProjectionFailure("invalid")
    if payload["protocol"] != PROTOCOL or not isinstance(payload["revision"], str) or not re.fullmatch(r"[0-9a-f]{40}", payload["revision"]):
        raise ProjectionFailure("invalid")
    if not isinstance(payload["event"], str) or not payload["event"].strip(): raise ProjectionFailure("invalid")
    if type(payload["limit"]) is not int or not 1 <= payload["limit"] <= MAX_ITEMS: raise ProjectionFailure("invalid")
    point = check_time(payload["at"])
    expectations = payload.get("expectations")
    if "expectations" in payload:
        if not isinstance(expectations, dict) or expectations.keys() != {"policy", "items"}: raise ProjectionFailure("invalid")
        _validate(expectations["items"], expectations["policy"])
    return point, expectations


def verify_world(world: World, payload: Any) -> dict[str, Any]:
    """Compose one report/check result at one validated immutable source SHA."""
    from .event_consequences import event_report
    from .validation import validate_world
    try:
        point, expectations = _request(payload)
        if world.revision != payload["revision"]: raise ProjectionFailure("unavailable")
        local = World(world.revision, world.tree_oid, world.records, world.root, world.source_root)
        if any(item["severity"] == "error" for item in validate_world(local)):
            raise ProjectionFailure("invalid", source=True)
        scope = full_author_scope(local)
        report = event_report(local, scope, payload["event"], point, limit=payload["limit"], source_valid=True)
        if report["outcome"] != "ok": return report
        if expectations is not None:
            evaluation = evaluate_expectations(local, scope, payload["event"], point, expectations["items"],
                                                policy=expectations["policy"], limit=payload["limit"], source_valid=True)
            report["expectations"] = evaluation.results
            report["applyAllowed"] = evaluation.apply_allowed
        return admit_semantic(report, checks=len(report["expectations"]), event=report, limit=payload["limit"])
    except (ProjectionFailure, RecursionError) as failed:
        return failure(failed if isinstance(failed, ProjectionFailure) else ProjectionFailure("invalid"))


def execute(repository, payload: Any) -> dict[str, Any]:
    """Read only source, using a fresh Repository to isolate parser caches.

    HTTP authentication is a dependency before this adapter. Local CLI callers
    are authorized by their repository access; request JSON supplies no grants.
    """
    from .errors import ParseError, RepositoryError, SupersededSchemaError
    from .repository import Repository
    try:
        _request(payload)  # Refuse grammar before repository/source access.
        source = Repository(repository.root)
        if not source.is_git: raise ProjectionFailure("unavailable")
        try:
            world = source.load_world(payload["revision"], cache_write=False)
        except RepositoryError:
            raise ProjectionFailure("unavailable") from None
        except (ParseError, SupersededSchemaError):
            raise ProjectionFailure("invalid", source=True) from None
        return verify_world(world, payload)
    except (ProjectionFailure, RecursionError) as failed:
        return failure(failed if isinstance(failed, ProjectionFailure) else ProjectionFailure("invalid"))


def status_code(report: dict) -> int:
    return {"ok": 200, "invalid": 400, "unavailable": 409, "limit": 422}[report["outcome"]]
