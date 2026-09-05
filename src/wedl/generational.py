"""Immutable, literal accessors for the v0.7 generational source envelope.

These classes deliberately expose authored values only.  They do not fold
history, resolve relations, or infer any lineage or succession.
"""
from __future__ import annotations

from dataclasses import dataclass
from types import MappingProxyType
from typing import Any, Mapping

from .ids import valid_id
from .model import StoryTime


GENERATIONAL_KINDS = frozenset({"organization", "parentage", "union", "affiliation", "legacy", "tenure", "claim", "vital-history"})
TRANSITIONS = {
    "organization": frozenset({"organization-initialize", "organization-rename", "organization-reparent", "organization-dormant", "organization-dissolve"}),
    "parentage": frozenset({"parentage-initialize", "parentage-confirm", "parentage-end"}),
    "union": frozenset({"union-initialize", "union-form", "union-end", "union-annul", "union-reconcile"}),
    "affiliation": frozenset({"affiliation-initialize", "affiliation-role", "affiliation-end"}),
    "legacy": frozenset({"legacy-initialize", "legacy-rename", "legacy-dormant", "legacy-dissolve"}),
    "tenure": frozenset({"tenure-initialize", "tenure-designate", "tenure-hold", "tenure-vacate", "tenure-transfer", "tenure-end"}),
    "claim": frozenset({"claim-initialize", "claim-dispute", "claim-recognize", "claim-withdraw", "claim-reject"}),
    "vital-history": frozenset({"vital-initialize", "vital-birth", "vital-death", "vital-existence-start", "vital-existence-end"}),
}
COMMON_FIELDS = frozenset({"schema", "kind", "id", "title", "domain", "status", "tags", "aliases", "threads", "audience", "perspectives", "initialization", "transitions"})
SPECIFIC_FIELDS = {
    "organization": frozenset({"organization_kind", "parent_id", "location_id"}),
    "parentage": frozenset({"child_id", "parent_id"}), "union": frozenset({"participant_ids"}),
    "affiliation": frozenset({"character_id", "organization_id"}), "legacy": frozenset({"legacy_kind", "organization_id"}),
    "tenure": frozenset({"legacy_id", "predecessor_tenure_id", "successor_tenure_id"}),
    "claim": frozenset({"legacy_id", "claimant_id"}), "vital-history": frozenset({"character_id", "disclosure"}),
}
OPTIONAL_FIELDS = {
    "organization": frozenset({"parent_id", "location_id"}), "legacy": frozenset({"organization_id"}),
    "tenure": frozenset({"predecessor_tenure_id", "successor_tenure_id"}),
}


def _mapping(value: Any, name: str) -> Mapping[str, Any]:
    if not isinstance(value, dict) or not all(isinstance(key, str) for key in value):
        raise ValueError(f"{name} must be a mapping")
    return value


def _freeze(value: Any) -> Any:
    """Copy nested YAML values into an immutable literal representation."""
    if isinstance(value, dict):
        return MappingProxyType({key: _freeze(child) for key, child in value.items()})
    if isinstance(value, list):
        return tuple(_freeze(child) for child in value)
    if isinstance(value, set):
        return frozenset(_freeze(child) for child in value)
    return value


def _point(value: Any) -> StoryTime:
    data = _mapping(value, "story time")
    if set(data) != {"timeline", "tick", "order"}:
        raise ValueError("story time must contain exactly timeline, tick, and order")
    return StoryTime.from_value(data)


def _ordered_ids(value: Any, kind: str | None = None, *, nonempty: bool = False) -> bool:
    if not isinstance(value, list) or (nonempty and not value):
        return False
    if not all(isinstance(item, str) and valid_id(item, kind) for item in value):
        return False
    return value == sorted(value) and len(value) == len(set(value))


def _ordered_text(value: Any, *, nonempty: bool = False) -> bool:
    if not isinstance(value, list) or (nonempty and not value):
        return False
    if not all(isinstance(item, str) and item.strip() for item in value):
        return False
    return value == sorted(value) and len(value) == len(set(value))


def _one_of(value: Any, choices: frozenset[str]) -> bool:
    return isinstance(value, str) and value in choices


def _dotted_domain(value: Any) -> bool:
    return isinstance(value, str) and "." in value and all(part and part.replace("-", "").isalnum() for part in value.split("."))


def literal_payload_ok(kind: Any, payload: Any) -> bool:
    """Check the closed payload grammar without resolving references.

    Reference existence deliberately belongs to world validation, but accessors
    reject the exact same leaf shapes, discriminators, and ID vocabularies.
    """
    if not isinstance(kind, str) or not isinstance(payload, dict):
        return False
    empty = {"organization-dormant", "organization-dissolve", "parentage-end", "union-end", "union-annul", "affiliation-end", "legacy-dormant", "legacy-dissolve", "tenure-end", "claim-recognize", "claim-withdraw", "claim-reject", "vital-initialize", "vital-birth", "vital-death", "vital-existence-start", "vital-existence-end"}
    if kind in empty:
        return payload == {}
    if kind in {"organization-initialize", "organization-rename", "legacy-initialize", "legacy-rename"}:
        return set(payload) == {"title", "aliases"} and isinstance(payload.get("title"), str) and bool(payload["title"].strip()) and _ordered_text(payload.get("aliases"))
    if kind == "organization-reparent":
        return set(payload) == {"parent_id"} and (payload["parent_id"] is None or valid_id(payload["parent_id"], "organization"))
    if kind in {"parentage-initialize", "parentage-confirm"}:
        return set(payload) == {"basis"} and _one_of(payload.get("basis"), frozenset({"biological", "adoptive"}))
    if kind in {"union-initialize", "union-form", "union-reconcile"}:
        return set(payload) == {"participant_ids"} and _ordered_ids(payload["participant_ids"], "character", nonempty=True) and len(payload["participant_ids"]) >= 2
    if kind == "affiliation-initialize":
        return set(payload) == {"role"} and (payload["role"] is None or isinstance(payload["role"], str))
    if kind == "affiliation-role":
        return set(payload) == {"role"} and isinstance(payload.get("role"), str) and bool(payload["role"].strip())
    if kind in {"tenure-initialize", "tenure-designate", "tenure-hold"}:
        holder = payload.get("holder_id")
        return set(payload) == {"holder_id", "basis"} and _one_of(payload.get("basis"), frozenset({"legal", "de-facto"})) and ((kind == "tenure-initialize" and holder is None) or valid_id(holder, "character"))
    if kind == "tenure-vacate":
        return payload == {"holder_id": None}
    if kind == "tenure-transfer":
        return set(payload) == {"from_tenure_id", "to_tenure_id"} and valid_id(payload.get("from_tenure_id"), "tenure") and valid_id(payload.get("to_tenure_id"), "tenure") and payload["from_tenure_id"] != payload["to_tenure_id"]
    if kind in {"claim-initialize", "claim-dispute"}:
        return set(payload) == {"competes_with"} and _ordered_ids(payload.get("competes_with"), "claim")
    return False


@dataclass(frozen=True, slots=True)
class Applicability:
    kind: str
    point: StoryTime | None = None
    first: StoryTime | None = None
    last: StoryTime | None = None

    @classmethod
    def from_value(cls, value: Any) -> "Applicability":
        data = _mapping(value, "applicability")
        kind = data.get("applicability_kind")
        if kind == "static" and set(data) == {"applicability_kind"}:
            return cls("static")
        if kind == "instant" and set(data) == {"applicability_kind", "point"}:
            return cls("instant", point=_point(data["point"]))
        if kind == "inclusive-interval" and set(data) == {"applicability_kind", "first", "last"}:
            first, last = _point(data["first"]), _point(data["last"])
            if not first.not_after(last):
                raise ValueError("applicability first cannot follow last")
            return cls("inclusive-interval", first=first, last=last)
        raise ValueError("invalid closed applicability")

    def start(self) -> StoryTime | None:
        return self.point if self.kind == "instant" else self.first


@dataclass(frozen=True, slots=True)
class Transition:
    id: str
    kind: str
    applicability: Applicability
    payload: Mapping[str, Any]
    cause_event_id: str | None = None
    replaces_transition_id: str | None = None

    @classmethod
    def from_value(cls, value: Any, *, initialization: bool = False, record_kind: str | None = None) -> "Transition":
        data = _mapping(value, "transition")
        required = {"transition_id", "transition_kind", "applicability", "payload"}
        allowed = required if initialization else required | {"cause_event_id", "replaces_transition_id"}
        if set(data) - allowed or not required <= set(data):
            raise ValueError("invalid transition fields")
        identifier, kind = data["transition_id"], data["transition_kind"]
        if not valid_id(identifier, "generational-transition") or not isinstance(kind, str):
            raise ValueError("invalid transition identifier or kind")
        if initialization != kind.endswith("-initialize"):
            raise ValueError("initialization kind mismatch")
        if record_kind is not None and (not isinstance(record_kind, str) or record_kind not in TRANSITIONS or kind not in TRANSITIONS[record_kind]):
            raise ValueError("transition kind is not applicable to record kind")
        applicability = Applicability.from_value(data["applicability"])
        if initialization and applicability.kind != "instant":
            raise ValueError("initialization must be an instant")
        if not initialization and ((kind == "tenure-vacate") != (applicability.kind == "inclusive-interval")):
            raise ValueError("transition applicability is not valid for kind")
        if not literal_payload_ok(kind, data["payload"]):
            raise ValueError("transition payload is not the exact declared shape")
        cause, replacement = data.get("cause_event_id"), data.get("replaces_transition_id")
        if kind == "tenure-transfer" and "cause_event_id" not in data:
            raise ValueError("tenure transfer requires a cause event")
        if "cause_event_id" in data and (cause is None or not valid_id(cause, "event")):
            raise ValueError("invalid cause event")
        if "replaces_transition_id" in data and (replacement is None or not valid_id(replacement, "generational-transition")):
            raise ValueError("invalid replacement transition")
        return cls(identifier, kind, applicability, _freeze(_mapping(data["payload"], "payload")), cause, replacement)


@dataclass(frozen=True, slots=True)
class GenerationalRecord:
    kind: str
    id: str
    initialization: Transition
    transitions: tuple[Transition, ...]
    fields: Mapping[str, Any]

    @classmethod
    def from_frontmatter(cls, value: Any) -> "GenerationalRecord":
        data = _mapping(value, "generational record")
        kind, identifier = data.get("kind"), data.get("id")
        if not isinstance(kind, str) or kind not in GENERATIONAL_KINDS or not valid_id(identifier, kind):
            raise ValueError("invalid generational kind or identifier")
        allowed = COMMON_FIELDS | SPECIFIC_FIELDS[kind]
        required = allowed - OPTIONAL_FIELDS.get(kind, frozenset())
        if set(data) - allowed or not required <= set(data):
            raise ValueError("generational record has invalid fields")
        if data.get("schema") != "wedl/v0.7" or not _one_of(data.get("status"), frozenset({"canonical", "draft", "retired"})):
            raise ValueError("invalid generational record envelope")
        if not isinstance(data.get("title"), str) or not data["title"].strip() or not _dotted_domain(data.get("domain")):
            raise ValueError("invalid generational record envelope")
        if not _ordered_text(data.get("audience"), nonempty=True) or not _ordered_text(data.get("perspectives"), nonempty=True) or not _ordered_text(data.get("tags")) or not _ordered_text(data.get("aliases")) or not _ordered_ids(data.get("threads"), "thread"):
            raise ValueError("invalid generational list fields")
        if kind == "organization" and (not _one_of(data.get("organization_kind"), frozenset({"house", "dynasty", "clan", "institution", "other"})) or (data.get("parent_id") is not None and not valid_id(data["parent_id"], "organization")) or (data.get("location_id") is not None and not valid_id(data["location_id"], "location"))):
            raise ValueError("invalid organization fields")
        if kind == "parentage" and not (valid_id(data.get("child_id"), "character") and valid_id(data.get("parent_id"), "character") and data["child_id"] != data["parent_id"]):
            raise ValueError("invalid parentage endpoints")
        if kind == "union" and (not _ordered_ids(data.get("participant_ids"), "character", nonempty=True) or len(data["participant_ids"]) < 2):
            raise ValueError("invalid union participants")
        if kind == "affiliation" and not (valid_id(data.get("character_id"), "character") and valid_id(data.get("organization_id"), "organization")):
            raise ValueError("invalid affiliation endpoints")
        if kind == "legacy" and (not _one_of(data.get("legacy_kind"), frozenset({"office", "estate", "title", "other"})) or (data.get("organization_id") is not None and not valid_id(data["organization_id"], "organization"))):
            raise ValueError("invalid legacy fields")
        if kind == "tenure" and (not valid_id(data.get("legacy_id"), "legacy") or any(data.get(key) is not None and not valid_id(data[key], "tenure") for key in ("predecessor_tenure_id", "successor_tenure_id"))):
            raise ValueError("invalid tenure fields")
        if kind == "claim" and not (valid_id(data.get("legacy_id"), "legacy") and valid_id(data.get("claimant_id"), "character")):
            raise ValueError("invalid claim endpoints")
        if kind == "vital-history" and not (valid_id(data.get("character_id"), "character") and _one_of(data.get("disclosure"), frozenset({"known", "unknown", "withheld"}))):
            raise ValueError("invalid vital history fields")
        transitions = data.get("transitions")
        if not isinstance(transitions, list):
            raise ValueError("transitions must be an array")
        return cls(kind, identifier, Transition.from_value(data.get("initialization"), initialization=True, record_kind=kind), tuple(Transition.from_value(item, record_kind=kind) for item in transitions), _freeze(data))


def generational_record(value: Any) -> GenerationalRecord:
    """Return an immutable typed literal record, rejecting malformed leaves."""
    return GenerationalRecord.from_frontmatter(value)
