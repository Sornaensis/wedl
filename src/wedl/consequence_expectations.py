"""Pure request-local checks; author grants and source validity are caller inputs.

This internal evaluator publishes no source facts or receipts. Transport derives
AuthorScope from authentication, validates the complete World, and shares the
logical item/serialized byte budget when assembling reports or writer receipts.
"""
from __future__ import annotations

from copy import deepcopy
from dataclasses import dataclass
import math
import re
from typing import Any

from .consequence_delta import _equal
from .event_consequences import (AuthorScope, CandidateIdentity, MAX_ITEMS, Projection,
                                 ProjectionFailure, bounded, citations, time_value)
from .model import StoryTime, World
from .semantics import canonical_events, story_point_state

_FIELDS = {
    "state.equals": {"target", "key", "value"}, "state.absent": {"target", "key"},
    "knowledge.state": {"knowledge", "state"},
    "relationship.matches": {"relationship", "values"},
    "story-point.state": {"storyPoint", "state"},
    "outcome.linked": {"event", "target"},
    "transition.caused": {"record", "transitionId", "event"},
}
_KNOWLEDGE = {"accepted", "suspected", "rejected", "uncertain", "remembered", "forgotten"}
_PLOT = {"dormant", "active", "resolved", "failed", "cancelled"}


@dataclass(frozen=True)
class ExpectationEvaluation:
    """Internal group for the later report/preview adapter, not a wire envelope."""
    event: dict
    at: dict
    policy: str
    results: list[dict]
    apply_allowed: bool


def check_time(value: Any) -> StoryTime:
    """Decode the closed wire Time; never coerce legacy numeric source time."""
    if not isinstance(value, dict) or value.keys() != {"timeline", "tick", "order"}:
        raise ProjectionFailure("invalid")
    if not _text(value["timeline"]) or any(not isinstance(value[key], str) or
            not re.fullmatch(r"0|-?[1-9][0-9]*", value[key]) for key in ("tick", "order")):
        raise ProjectionFailure("invalid")
    try:
        return StoryTime(value["timeline"], int(value["tick"]), int(value["order"]))
    except ValueError:
        raise ProjectionFailure("invalid") from None


def _text(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def _json(value: Any) -> bool:
    if value is None or type(value) in {str, bool, int}:
        return True
    if type(value) is float:
        return math.isfinite(value)
    if type(value) is list:
        return all(_json(item) for item in value)
    if type(value) is dict:
        return all(isinstance(key, str) and _json(item) for key, item in value.items())
    return False


def _validate(items: Any, policy: str) -> None:
    if not isinstance(policy, str) or policy not in {"required", "advisory"} or not isinstance(items, list):
        raise ProjectionFailure("invalid")
    if len(items) > 100:
        raise ProjectionFailure("limit")
    seen = set()
    for check in items:
        if not isinstance(check, dict) or check.keys() != {"id", "predicate"} or not _text(check["id"]) or check["id"] in seen:
            raise ProjectionFailure("invalid")
        seen.add(check["id"])
        predicate = check["predicate"]
        if not isinstance(predicate, dict) or not _text(predicate.get("kind")) or predicate["kind"] not in _FIELDS:
            raise ProjectionFailure("invalid")
        kind = predicate["kind"]
        if predicate.keys() != {"kind"} | _FIELDS[kind] or not _json(predicate):
            raise ProjectionFailure("invalid")
        for field in _FIELDS[kind] - {"value", "values"}:
            if not _text(predicate[field]):
                raise ProjectionFailure("invalid")
        if kind == "knowledge.state" and predicate["state"] not in _KNOWLEDGE:
            raise ProjectionFailure("invalid")
        if kind == "story-point.state" and predicate["state"] not in _PLOT:
            raise ProjectionFailure("invalid")
        if kind == "relationship.matches":
            value = predicate["values"]
            if not isinstance(value, dict) or not value or value.keys() - {"status", "metrics", "facets"}:
                raise ProjectionFailure("invalid")
            if "status" in value and not _text(value["status"]):
                raise ProjectionFailure("invalid")
            if "metrics" in value and (not isinstance(value["metrics"], dict) or any(not _text(key) or
                    type(item) not in {int, float} or (type(item) is float and not math.isfinite(item)) for key, item in value["metrics"].items())):
                raise ProjectionFailure("invalid")
            if "facets" in value and (not isinstance(value["facets"], list) or not all(isinstance(item, str) for item in value["facets"])):
                raise ProjectionFailure("invalid")


def _reference(projection: Projection, value: str, kinds: set[str] | None = None) -> dict:
    explicit = projection.world.records.get(value)
    if explicit is not None and kinds is not None and explicit.kind not in kinds:
        raise ProjectionFailure("invalid")
    if kinds is None:
        return projection.reference(value)
    matches = []
    for kind in sorted(kinds):
        try:
            matches.append(projection.reference(value, kind))
        except ProjectionFailure as failure:
            if failure.outcome != "unavailable":
                raise
    if not matches:
        raise ProjectionFailure("unavailable")
    if len(matches) != 1:
        raise ProjectionFailure("invalid")
    return matches[0]


class _Unsupported(Exception):
    pass


def _value(projection: Projection, value: Any, rule: Any, *, validate: bool = True) -> Any:
    """Basic authored types and installed items/properties reference hints only.

    This follows the schema-leaf walker used by consequence intents without
    importing writer code. Opaque metadata is neither a constraint nor a ref.
    It is not a JSON Schema engine: no required/default/enum/path inference.
    """
    if not isinstance(rule, dict) or "type" not in rule:
        return value
    kind = rule["type"]
    if not isinstance(kind, str) or kind not in {"string", "entity", "boolean", "number", "array", "object", "null"}:
        raise _Unsupported
    valid = {"string": type(value) is str, "entity": isinstance(value, dict) and _text(value.get("entity")),
             "boolean": type(value) is bool, "number": type(value) in {int, float},
             "array": type(value) is list, "object": type(value) is dict, "null": value is None}[kind]
    if validate and not valid:
        raise ProjectionFailure("invalid")
    if kind == "entity" and isinstance(value, dict) and "entity" in value:
        if rule.get("entity_kind") is not None and not _text(rule["entity_kind"]):
            raise _Unsupported
        value["entity"] = _reference(projection, value["entity"], {rule["entity_kind"]} if rule.get("entity_kind") else None)["id"]
    elif kind == "array" and isinstance(value, list):
        if rule.get("items") is not None and not isinstance(rule["items"], dict):
            raise _Unsupported
        for index, item in enumerate(value):
            value[index] = _value(projection, item, rule.get("items"), validate=validate)
    elif kind == "object" and isinstance(value, dict):
        if rule.get("properties") is not None and not isinstance(rule["properties"], dict):
            raise _Unsupported
        for key, definition in (rule.get("properties") or {}).items():
            if key in value:
                value[key] = _value(projection, value[key], definition, validate=validate)
    return value


def _state_value(projection: Projection, predicate: dict, record) -> dict:
    if not projection.scope.permits(projection.scope.world_id, "frontmatter.state_keys"):
        raise ProjectionFailure("unavailable")
    rule = ((projection.world.config.get("state_keys") or {}).get(record.kind) or {}).get(predicate["key"])
    if not isinstance(rule, dict) or "type" not in rule:
        raise _Unsupported
    kind = rule.get("type")
    if not isinstance(kind, str) or kind not in {"string", "entity", "boolean", "number", "array", "object", "null"}:
        raise _Unsupported
    if predicate["kind"] == "state.absent":
        return rule
    predicate["value"] = _value(projection, predicate["value"], rule)
    return rule


def _evaluate(projection: Projection, predicate: dict) -> tuple[bool, dict, list[dict]]:
    kind = predicate["kind"]
    if kind.startswith("state."):
        reference = _reference(projection, predicate["target"], {"character", "object", "location", "environment"})
        predicate["target"] = reference["id"]
        record = projection.world.records[reference["id"]]
        rule = _state_value(projection, predicate, record)
        snapshot = projection.snapshot({"kind": "state", "recordId": record.id, "key": predicate["key"]})
        if rule["type"] == "entity":
            # The projector drops effects whose entity leaf is withheld; do not
            # mistake that redaction for an earlier value or a clear.
            latest = None
            for event in canonical_events(projection.world, projection.at):
                for effect in projection.source.records[event.id].frontmatter.get("effects", []):
                    if effect.get("target") == record.id and effect.get("key") == predicate["key"]:
                        latest = effect
            if latest is not None and latest.get("operation") != "clear":
                _value(projection, deepcopy(latest.get("value")), rule, validate=False)
        observed = {"presence": snapshot["presence"]}
        if snapshot["presence"] == "present":
            observed["value"] = _value(projection, deepcopy(snapshot["payload"]["value"]), rule, validate=False)
        passed = observed["presence"] == "absent" if kind == "state.absent" else (
            observed["presence"] == "present" and _equal(observed["value"], predicate["value"]))
        return passed, {"kind": "state", "observed": observed}, snapshot["citations"]
    if kind in {"knowledge.state", "relationship.matches", "story-point.state"}:
        family, field = {"knowledge.state": ("knowledge", "knowledge"),
                         "relationship.matches": ("relationship", "relationship"),
                         "story-point.state": ("story-point", "storyPoint")}[kind]
        reference = _reference(projection, predicate[field], {family})
        predicate[field] = reference["id"]
        if family == "story-point":
            record = projection.world.records[reference["id"]]
            if not all(projection.scope.permits(record.id, section) for section in ("lifecycle.initial_state", "lifecycle.transitions")):
                raise ProjectionFailure("unavailable")
            state = story_point_state(projection.world, record, projection.at)
            latest = projection._latest(record, "lifecycle.transitions")
            if latest:
                ordinal, member = latest
                evidence = [projection.citation(record.id, "lifecycle.transitions", ordinal, member.get("id"),
                                               StoryTime.from_value(member["time"], projection.world.default_timeline))]
            else:
                evidence = [projection.citation(record.id, "lifecycle.initial_state")]
            return state == predicate["state"], {"kind": family, "state": state}, evidence
        snapshot = projection.snapshot({"kind": family, "recordId": reference["id"]})
        if snapshot["presence"] != "present":
            raise ProjectionFailure("unavailable")
        payload = snapshot["payload"]
        if family == "knowledge":
            return payload["state"] == predicate["state"], {"kind": family, **{key: payload[key] for key in ("state", "transitionId", "time")}}, snapshot["citations"]
        requested, actual, passed = predicate["values"], {}, True
        for key in requested:
            if key == "metrics":
                if not projection.scope.permits(projection.scope.world_id, "frontmatter.relationship_metrics"):
                    raise ProjectionFailure("unavailable")
                definitions = projection.world.config.get("relationship_metrics") or {}
                actual[key] = {}
                for name, value in requested[key].items():
                    rule = definitions.get(name)
                    if not isinstance(rule, dict):
                        raise _Unsupported
                    if (rule.get("minimum") is not None and value < rule["minimum"]) or (rule.get("maximum") is not None and value > rule["maximum"]):
                        raise ProjectionFailure("invalid")
                    observed = {"presence": "present", "value": payload[key][name]} if name in payload[key] else {"presence": "absent"}
                    actual[key][name] = observed
                    passed = passed and observed["presence"] == "present" and _equal(observed["value"], value)
            else:
                actual[key] = deepcopy(payload[key])
                passed = passed and _equal(payload[key], requested[key])
        return passed, {"kind": family, "values": actual}, snapshot["citations"]
    event = _reference(projection, predicate["event"], {"event"})
    predicate["event"] = event["id"]
    if kind == "outcome.linked":
        target = _reference(projection, predicate["target"], {"story-point", "scene"})
        predicate["target"] = target["id"]
        if not projection.scope.permits(target["id"], "outcome_events") or (target["kind"] == "story-point" and
                not projection.scope.permits(event["id"], "related_story_points")):
            raise ProjectionFailure("unavailable")
        snapshot = projection.snapshot({"kind": "outcome", "recordId": event["id"], "targetId": target["id"], "targetKind": target["kind"]})
        actual = snapshot["payload"]
        evidence = snapshot["citations"]
        # Negative membership is evidenced by the admitted complete authored list.
        evidence.append(projection.citation(target["id"], "outcome_events"))
        if target["kind"] == "story-point":
            evidence.append(projection.citation(event["id"], "related_story_points"))
        return actual["targetLinked"] and (target["kind"] == "scene" or actual["eventLinked"]), {"kind": "outcome", **actual}, citations(evidence)
    record = _reference(projection, predicate["record"], {"knowledge", "relationship", "story-point"})
    predicate["record"] = record["id"]
    section = "lifecycle.transitions" if record["kind"] == "story-point" else "transitions"
    if not projection.scope.permits(record["id"], section):
        raise ProjectionFailure("unavailable")
    source = projection.world.records[record["id"]]
    members = source.frontmatter.get("lifecycle", {}).get("transitions", []) if record["kind"] == "story-point" else source.frontmatter.get("transitions", [])
    selected = [(index, member) for index, member in enumerate(members) if member.get("id") == predicate["transitionId"]]
    if not selected:
        raise ProjectionFailure("unavailable")
    index, member = selected[0]
    ordinal = projection._ordinals[record["id"], section][index]
    original_record = projection.source.records[record["id"]]
    originals = original_record.frontmatter.get("lifecycle", {}).get("transitions", []) if record["kind"] == "story-point" else original_record.frontmatter["transitions"]
    cause = originals[ordinal].get("causing_event")
    observed = {"presence": "absent"}
    if cause is not None:
        observed = {"presence": "present", "value": _reference(projection, cause, {"event"})["id"]}
    point = StoryTime.from_value(member["time"], projection.world.default_timeline)
    actual = {"kind": "transition", "causingEvent": observed, "transitionId": member["id"], "time": time_value(point)}
    return cause == event["id"], actual, [projection.citation(record["id"], section, ordinal, member["id"], point)]


def evaluate_expectations(world: World, scope: AuthorScope, event: str, at: StoryTime,
                          items: list[dict], *, policy: str = "required", limit: int = MAX_ITEMS,
                          candidate: CandidateIdentity | None = None, base: World | None = None,
                          source_valid: bool = True) -> ExpectationEvaluation:
    """Evaluate explicit checks or raise a closed failure with no partial group.

    Standalone empty checks are supported. Operation adapters independently require
    1..100 items per operation and at most 100 logical checks across all groups.
    Results may be copied into multiple receipt fields, but every serialized copy
    must count toward the adapter's aggregate byte budget.
    """
    if not isinstance(source_valid, bool) or not _text(event):
        raise ProjectionFailure("invalid")
    if not source_valid:
        raise ProjectionFailure("invalid", source=True)
    try:
        _validate(items, policy)
    except RecursionError:
        raise ProjectionFailure("invalid") from None
    bounded(items, len(items), limit)
    projection = Projection(world, scope, at, limit=limit, candidate=candidate, base=base)
    focus = _reference(projection, event, {"event"})
    identity = {"kind": "source", "revision": world.revision} if candidate is None else {
        "kind": "candidate", "baseRevision": candidate.base_revision, "requestHash": candidate.request_hash}
    results = []
    for check in items:
        predicate = deepcopy(check["predicate"])
        actual, evidence = {"kind": "redacted"}, []
        try:
            passed, actual, evidence = _evaluate(projection, predicate)
            outcome = "pass" if passed else "fail"
        except _Unsupported:
            outcome = "unsupported"
        except ProjectionFailure as failure:
            if failure.outcome != "unavailable":
                raise
            outcome = "unknown"
        results.append({"id": check["id"], "policy": policy, "outcome": outcome,
                        "code": "EXPECTATION-" + outcome.upper(),
                        "comparison": {"world": identity.copy(), "at": time_value(at), "predicate": predicate},
                        "actual": actual, "citations": citations(evidence)})
    allowed = policy == "advisory" or all(result["outcome"] == "pass" for result in results)
    bounded({"event": focus, "at": time_value(at), "policy": policy, "results": results, "applyAllowed": allowed}, len(results), limit)
    return ExpectationEvaluation(focus, time_value(at), policy, results, allowed)
