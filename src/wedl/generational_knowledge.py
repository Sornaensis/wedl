"""Authored character assertions, independent of canonical genealogy.

This module validates literal source and provenance. It never compares a
belief with canon, grants knowledge from presence, or derives relationships.
"""
from __future__ import annotations

from dataclasses import dataclass
import math
from types import MappingProxyType
from typing import Any, Mapping

from .conversation import (
    beat_kind, conversation_participant_at, current_recollection,
    observations_at, participant_at, scene_contains_time, turn_time, turn_visible_to,
)
from .ids import valid_id
from .model import Record, StoryTime, World
from .v07 import canonical_capabilities

CAPABILITY = "generational-knowledge-v1"
AFFIRMATIVE_STATES = frozenset({"accepted", "suspected", "uncertain", "remembered"})
MAX_REFERENCES = 32
MAX_LABEL_LENGTH = 200
_FIELDS = {
    "parentage": {"child_id": "character", "parent_id": "character", "basis": None},
    "union": {"participant_ids": "character", "state": None},
    "organization": {"organization_id": "organization", "parent_id": "organization"},
    "affiliation": {"character_id": "character", "organization_id": "organization", "role": None},
    "tenure": {"legacy_id": "legacy", "holder_id": "character", "basis": None},
    "claim": {"legacy_id": "legacy", "claimant_id": "character", "state": None},
    "vital": {"character_id": "character", "state": None},
}
_EVIDENCE = {
    "knowledge": ("knowledge", "transition_id", "knowledge-transition"),
    "observation": ("scene", "observation_id", "observation"),
    "turn": ("conversation", "turn_id", "conversation-turn"),
    "recollection": ("conversation", "recollection_id", "conversation-recollection"),
}


def _point(value: Any) -> StoryTime:
    if not isinstance(value, dict) or set(value) != {"timeline", "tick", "order"}:
        raise ValueError("time must contain exactly timeline, tick and order")
    return StoryTime.from_value(value)


def _text(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip()) and len(value) <= MAX_LABEL_LENGTH


def _freeze(value: Any) -> Any:
    if isinstance(value, dict):
        return MappingProxyType({key: _freeze(child) for key, child in value.items()})
    if isinstance(value, list):
        return tuple(_freeze(child) for child in value)
    return value


@dataclass(frozen=True, slots=True)
class GenealogyAssertion:
    kind: str
    payload: Mapping[str, Any]
    valid_from: StoryTime
    valid_until: StoryTime | None
    labels: Mapping[str, str]
    evidence: tuple[Mapping[str, str], ...]

    @classmethod
    def from_value(cls, value: Any) -> "GenealogyAssertion":
        if not isinstance(value, dict) or not {"kind", "payload", "valid"} <= set(value) or set(value) - {"kind", "payload", "valid", "labels", "evidence"}:
            raise ValueError("genealogy requires kind, payload and valid, with optional labels and evidence")
        kind, payload = value["kind"], value["payload"]
        if not isinstance(kind, str) or kind not in _FIELDS or not isinstance(payload, dict) or set(payload) != set(_FIELDS[kind]):
            raise ValueError("invalid closed genealogy payload")
        references: set[str] = set()
        for field, endpoint_kind in _FIELDS[kind].items():
            if endpoint_kind is None:
                continue
            endpoint = payload[field]
            if field == "participant_ids":
                if not isinstance(endpoint, list) or not 2 <= len(endpoint) <= MAX_REFERENCES or not all(isinstance(item, str) and valid_id(item, endpoint_kind) for item in endpoint) or endpoint != sorted(set(endpoint)):
                    raise ValueError("union participants must be bounded, sorted unique character IDs")
                references.update(endpoint)
            elif endpoint is None and (kind, field) in {("organization", "parent_id"), ("tenure", "holder_id")}:
                continue
            elif not isinstance(endpoint, str) or not valid_id(endpoint, endpoint_kind):
                raise ValueError(f"invalid {field}")
            else:
                references.add(endpoint)
        enums = {
            "parentage": ("basis", {"biological", "adoptive"}),
            "union": ("state", {"formed", "ended", "annulled"}),
            "tenure": ("basis", {"legal", "de-facto"}),
            "claim": ("state", {"proposed", "disputed", "recognized", "withdrawn", "rejected"}),
            "vital": ("state", {"living", "dead", "existing", "ended"}),
        }
        if kind in enums:
            field, choices = enums[kind]
            if not isinstance(payload[field], str) or payload[field] not in choices:
                raise ValueError(f"invalid {kind} {field}")
        if kind == "affiliation" and payload["role"] is not None and not _text(payload["role"]):
            raise ValueError("role must be null or bounded nonempty text")
        valid = value["valid"]
        if not isinstance(valid, dict) or "from" not in valid or set(valid) - {"from", "until"}:
            raise ValueError("valid requires from and optional inclusive until")
        first = _point(valid["from"])
        last = _point(valid["until"]) if "until" in valid else None
        if last is not None and not first.not_after(last):
            raise ValueError("valid until must not precede from or change timeline")
        labels = value.get("labels", {})
        if not isinstance(labels, dict) or len(labels) > MAX_REFERENCES or not all(isinstance(key, str) and key in references and _text(label) for key, label in labels.items()):
            raise ValueError("learned labels must name asserted endpoints with bounded literal text")
        evidence = value.get("evidence", [])
        if not isinstance(evidence, list) or len(evidence) > MAX_REFERENCES:
            raise ValueError("evidence must be a bounded list")
        seen: set[tuple[str, str, str]] = set()
        for item in evidence:
            tag = item.get("kind") if isinstance(item, dict) else None
            if not isinstance(tag, str) or tag not in _EVIDENCE:
                raise ValueError("invalid evidence discriminator")
            entity_kind, field, reference_kind = _EVIDENCE[tag]
            if set(item) != {"kind", "entity_id", field} or not isinstance(item["entity_id"], str) or not valid_id(item["entity_id"], entity_kind) or not isinstance(item[field], str) or not valid_id(item[field], reference_kind):
                raise ValueError("invalid exact evidence reference")
            identity = (tag, item["entity_id"], item[field])
            if identity in seen:
                raise ValueError("duplicate evidence reference")
            seen.add(identity)
        return cls(kind, _freeze(payload), first, last, _freeze(labels), _freeze(evidence))

    def references(self) -> tuple[tuple[str, str], ...]:
        result = []
        for field, kind in _FIELDS[self.kind].items():
            if kind is not None:
                value = self.payload[field]
                result.extend((item, kind) for item in (value if field == "participant_ids" else (value,)) if item is not None)
        return tuple(result)


def _evidence_admitted(world: World, item: Mapping[str, str], knower: str, learned: StoryTime) -> bool:
    target = world.maybe_get(item["entity_id"])
    if target is None:
        return False
    entity_kind, field, _ = _EVIDENCE[item["kind"]]
    if target.kind != entity_kind:
        return False
    data = target.frontmatter
    try:
        if item["kind"] == "knowledge":
            if target.status != "canonical" or data.get("knower") != knower:
                return False
            eligible = [(StoryTime.from_value(t["time"], world.default_timeline), index, t)
                        for index, t in enumerate(data.get("transitions", []))
                        if isinstance(t, dict) and "time" in t and StoryTime.from_value(t["time"], world.default_timeline).not_after(learned)]
            if not eligible:
                return False
            active = max(eligible, key=lambda entry: (entry[0].tick, entry[0].order, entry[1]))[2]
            return active.get("state") in AFFIRMATIVE_STATES and any(t.get("id") == item[field] and t.get("state") in AFFIRMATIVE_STATES for _, _, t in eligible)
        if item["kind"] == "observation":
            if target.status not in {"active", "closed"}:
                return False
            observation = next((o for o in data.get("observations", []) if isinstance(o, dict) and o.get("id") == item[field]), None)
            if observation is None:
                return False
            point = StoryTime.from_value(observation["at"], world.default_timeline)
            return point.not_after(learned) and scene_contains_time(target, point, world.default_timeline) and participant_at(target, knower, point, world.default_timeline) is not None and any(o["id"] == item[field] for o in observations_at(target, knower, point, world.default_timeline))
        if target.status not in {"active", "closed", "retired"}:
            return False
        if item["kind"] == "turn":
            turn = next((t for t in data.get("turns", []) if isinstance(t, dict) and t.get("id") == item[field]), None)
            if turn is None or beat_kind(turn) != "speech":
                return False
            point = turn_time(turn, target, world.default_timeline)
            return point.not_after(learned) and conversation_participant_at(target, knower, point, world.default_timeline) is not None and turn_visible_to(target, turn, knower, point, world.default_timeline)
        recollection = current_recollection(target, knower, learned, world.default_timeline)
        return recollection is not None and recollection.get("id") == item[field]
    except (TypeError, KeyError, ValueError, OverflowError):
        return False


def validate_genealogy_knowledge(world: World, record: Record) -> list[dict[str, Any]]:
    claim = record.frontmatter.get("claim")
    if not isinstance(claim, dict) or "genealogy" not in claim:
        return []
    result = []

    def error(code: str, message: str, field: str = "claim.genealogy") -> None:
        result.append({"code": code, "message": message, "severity": "error", "entityId": record.id, "path": record.source_path, "field": field})

    caps = canonical_capabilities(world.world_record.frontmatter.get("capabilities"))
    if set(claim) - {"key", "statement", "genealogy"}:
        error("GEN-KNOWLEDGE-002", "typed claim allows only key, statement and genealogy", "claim")
    if record.frontmatter.get("schema") != "wedl/v0.7" or world.world_record.frontmatter.get("schema") != "wedl/v0.7" or caps is None or CAPABILITY not in caps:
        error("GEN-KNOWLEDGE-001", "typed genealogy requires v0.7 and generational-knowledge-v1 with generational-core-v1")
    try:
        assertion = GenealogyAssertion.from_value(claim["genealogy"])
    except (TypeError, ValueError) as exc:
        error("GEN-KNOWLEDGE-002", str(exc))
        return result
    for identifier, kind in assertion.references():
        target = world.maybe_get(identifier)
        if target is None or target.kind != kind:
            error("GEN-KNOWLEDGE-003", f"asserted endpoint must reference an existing {kind}", "claim.genealogy.payload")
    if assertion.valid_from.timeline not in world.timeline_ids:
        error("GEN-KNOWLEDGE-004", "asserted applicability must use a declared timeline", "claim.genealogy.valid")
    knower = record.frontmatter.get("knower")
    character = world.maybe_get(knower) if isinstance(knower, str) else None
    if character is None or character.kind != "character":
        error("GEN-KNOWLEDGE-003", "knower must reference an existing character", "knower")
    learned = []
    transitions = record.frontmatter.get("transitions")
    for transition in transitions if isinstance(transitions, list) else []:
        if isinstance(transition, dict) and "confidence" in transition:
            confidence = transition["confidence"]
            if isinstance(confidence, bool) or not isinstance(confidence, (int, float)) or not 0 <= confidence <= 1 or not math.isfinite(confidence):
                error("GEN-KNOWLEDGE-002", "confidence must be a finite number from zero to one", "transitions")
        if isinstance(transition, dict) and isinstance(transition.get("state"), str) and transition["state"] in AFFIRMATIVE_STATES:
            try:
                if not isinstance(transition.get("id"), str) or not valid_id(transition["id"], "knowledge-transition"):
                    raise ValueError("learning requires its authored stable knowledge-transition ID")
                point = _point(transition.get("time"))
                if point.timeline != assertion.valid_from.timeline:
                    raise ValueError("learning and asserted applicability must share a timeline")
                learned.append(point)
            except ValueError as exc:
                error("GEN-KNOWLEDGE-004", str(exc), "transitions")
    if not learned:
        error("GEN-KNOWLEDGE-005", "typed knowledge requires an explicit affirmative learning transition", "transitions")
    elif character is not None:
        first_learning = min(learned, key=lambda point: (point.tick, point.order))
        for index, evidence in enumerate(assertion.evidence):
            if not _evidence_admitted(world, evidence, knower, first_learning):
                error("GEN-KNOWLEDGE-006", "evidence must be exact, admitted for this character and no later than learning", f"claim.genealogy.evidence[{index}]")
    return result
