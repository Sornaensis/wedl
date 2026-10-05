"""Closed generational intents compiled to the existing atomic changeset writer."""
from __future__ import annotations

import hashlib
import re
from copy import deepcopy
from typing import Any

from . import V07_SOURCE_SCHEMA
from .errors import ChronologyUpgradeRequired, StaleRevision, UsageError
from .generational import GENERATIONAL_KINDS, SPECIFIC_FIELDS, TRANSITIONS
from .generational_knowledge import AFFIRMATIVE_STATES, CAPABILITY as KNOWLEDGE_CAPABILITY, GenealogyAssertion
from .ids import id_from_seed, valid_id
from .model import StoryTime, World
from .repository import Repository
from .util import canonical_json
from .v07 import CAPABILITY_ORDER


_SHA = re.compile(r"[0-9a-f]{40}\Z")
_DECIMAL = re.compile(r"(?:0|-?[1-9][0-9]*)\Z")
_FIELDS = {
    "organization": ({"organization_kind"}, {"parent_id", "location_id"}),
    "parentage": ({"child_id", "parent_id"}, set()),
    "union": ({"participant_ids"}, set()),
    "affiliation": ({"character_id", "organization_id"}, set()),
    "legacy": ({"legacy_kind"}, {"organization_id"}),
    "tenure": ({"legacy_id"}, {"predecessor_tenure_id", "successor_tenure_id"}),
    "claim": ({"legacy_id", "claimant_id"}, set()),
    "vital-history": ({"character_id", "disclosure"}, set()),
}
_REF_KINDS = {"parent_id": "organization", "location_id": "location",
              "child_id": "character", "participant_ids": "character",
              "character_id": "character", "organization_id": "organization",
              "legacy_id": "legacy", "predecessor_tenure_id": "tenure",
              "successor_tenure_id": "tenure", "claimant_id": "character",
              "holder_id": "character", "competes_with": "claim",
              "from_tenure_id": "tenure", "to_tenure_id": "tenure"}
_CREATE_KEYS = {"action", "kind", "id", "title", "domain", "tags", "aliases", "threads",
                "audience", "perspectives", "fields", "payload", "at"}
_APPEND_KEYS = {"action", "kind", "record", "transition", "payload", "at", "interval",
                "cause", "transitionId"}
_CORRECT_KEYS = _APPEND_KEYS | {"replaces"}
_OUTER_KEYS = {"expectedHead", "idempotencyKey", "summary"}
_DOMAINS = {"organization": "history.organizations", "parentage": "history.kinship",
            "union": "history.unions", "affiliation": "history.affiliations",
            "legacy": "history.legacies", "tenure": "history.tenures",
            "claim": "history.claims", "vital-history": "history.vitals"}
KNOWLEDGE_ACTIONS = ("generational.knowledge.opt-in", "generational.knowledge.create",
                     "generational.knowledge.state", "generational.knowledge.replace")
_KNOWLEDGE_CREATE = {"title", "knower", "assertion", "at", "state", "confidence", "id", "transitionId"}


def _knowledge_transition(world: World, item: dict[str, Any], seed: str, *, initial: bool = False) -> dict[str, Any]:
    states = AFFIRMATIVE_STATES if initial else AFFIRMATIVE_STATES | {"rejected", "forgotten"}
    if not isinstance(item["state"], str) or item["state"] not in states:
        raise UsageError("knowledge state is not an admitted variant")
    confidence = item.get("confidence")
    if "confidence" in item and (type(confidence) not in (int, float) or not 0 <= confidence <= 1):
        raise UsageError("knowledge confidence must be a finite number from zero to one")
    identifier = item.get("transitionId", id_from_seed("knowledge-transition", seed))
    if not valid_id(identifier, "knowledge-transition"):
        raise UsageError("transitionId must be a stable knowledge transition ID")
    result = {"id": identifier, "time": _time(item["at"], world), "state": item["state"], "acquisition": "learned"}
    if "confidence" in item:
        result["confidence"] = confidence
    return result


def _knowledge_create(world: World, item: dict[str, Any], seed: str) -> dict[str, Any]:
    _closed(item, {"title", "knower", "assertion", "at", "state"}, _KNOWLEDGE_CREATE, "knowledge create")
    assertion = deepcopy(item["assertion"])
    if not isinstance(assertion, dict) or not isinstance(assertion.get("valid"), dict):
        raise UsageError("assertion requires a closed genealogy literal")
    assertion["valid"] = {key: _time(value, world) for key, value in assertion["valid"].items()}
    try:
        GenealogyAssertion.from_value(assertion)
    except (TypeError, ValueError) as exc:
        raise UsageError("assertion requires a closed typed genealogy literal") from exc
    identifier = item.get("id", id_from_seed("knowledge", seed))
    if not valid_id(identifier, "knowledge") or identifier in world.records:
        raise UsageError("knowledge create ID is invalid or already used")
    frontmatter = {"schema": world.schema, "kind": "knowledge", "id": identifier,
                   "title": _text(item["title"], "title"), "domain": "knowledge.genealogy",
                   "status": "canonical", "tags": [], "aliases": [],
                   "knower": _reference(world, {}, item["knower"], "character"),
                   "claim": {"key": "genealogy", "statement": "Authored genealogy assertion", "genealogy": assertion},
                   "transitions": [_knowledge_transition(world, item, seed, initial=True)]}
    return {"type": "entity.create", "value": {"frontmatter": frontmatter}}


def _knowledge_operations(world: World, intent: dict[str, Any], seed: str) -> list[dict[str, Any]]:
    action = intent["action"]
    required = {"action", "expectedHead", "idempotencyKey"}
    if action == "generational.knowledge.opt-in":
        _closed(intent, required, required | {"summary"}, "knowledge opt-in")
        capabilities = world.world_record.frontmatter.get("capabilities") or []
        enabled = sorted(set(capabilities) | {"generational-core-v1", KNOWLEDGE_CAPABILITY}, key=CAPABILITY_ORDER.index)
        return [{"type": "entity.update", "entity": world.world_record.id, "frontmatterPatch": {"capabilities": enabled}}]
    if KNOWLEDGE_CAPABILITY not in (world.world_record.frontmatter.get("capabilities") or []):
        raise ChronologyUpgradeRequired("knowledge authoring requires an explicit generational-knowledge-v1 opt-in")
    if action == "generational.knowledge.create":
        _closed(intent, required | {"title", "knower", "assertion", "at", "state"}, required | {"summary"} | _KNOWLEDGE_CREATE, "knowledge create intent")
        return [_knowledge_create(world, {key: value for key, value in intent.items() if key in _KNOWLEDGE_CREATE}, seed)]
    if action not in {"generational.knowledge.state", "generational.knowledge.replace"}:
        raise UsageError("unsupported knowledge intent variant")
    fields = {"record", "at", "state", "transitionId", "confidence"} if action.endswith(".state") else {"record", "at", "retireState", "transitionId", "replacement"}
    _closed(intent, required | {"record", "at"} | ({"state"} if action.endswith(".state") else {"retireState", "replacement"}), required | {"summary"} | fields, "knowledge transition intent")
    identifier = _reference(world, {}, intent["record"], "knowledge")
    record = world.get(identifier)
    if not isinstance(record.frontmatter.get("claim"), dict) or "genealogy" not in record.frontmatter["claim"]:
        raise UsageError("knowledge transition requires a typed genealogy assertion")
    state_item = intent if action.endswith(".state") else {"at": intent["at"], "state": intent["retireState"], **({"transitionId": intent["transitionId"]} if "transitionId" in intent else {})}
    if action.endswith(".replace") and (not isinstance(intent["retireState"], str) or intent["retireState"] not in {"rejected", "forgotten"}):
        raise UsageError("replacement must explicitly reject or forget the old assertion")
    transition = _knowledge_transition(world, state_item, seed + ":state")
    history = deepcopy(record.frontmatter["transitions"])
    if history and not StoryTime.from_value(history[-1]["time"]).not_after(StoryTime.from_value(transition["time"])):
        raise UsageError("knowledge state must append at or after its last authored transition")
    history.append(transition)
    result = [{"type": "entity.update", "entity": identifier, "frontmatterPatch": {"transitions": history}}]
    if action.endswith(".replace"):
        replacement = _closed(intent["replacement"], {"title", "assertion", "state"}, _KNOWLEDGE_CREATE - {"knower", "at"}, "knowledge replacement")
        result.append(_knowledge_create(world, {**replacement, "knower": record.frontmatter["knower"], "at": intent["at"]}, seed + ":replacement"))
    return result


def _closed(value: Any, required: set[str], allowed: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or any(not isinstance(key, str) for key in value) or required - set(value) or set(value) - allowed:
        raise UsageError(f"{label} has missing or unsupported fields")
    return value


def _text(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value.strip() or len(value) > 256:
        raise UsageError(f"{label} must be a nonblank string of at most 256 characters")
    return value


def _strings(value: Any, label: str, *, nonempty: bool = False) -> list[str]:
    if not isinstance(value, list) or (nonempty and not value) or len(value) > 100 or any(not isinstance(item, str) or not item.strip() or len(item) > 256 for item in value) or value != sorted(set(value)):
        raise UsageError(f"{label} must be a sorted, unique string array")
    return value


def _time(value: Any, world: World) -> dict[str, Any]:
    point = _closed(value, {"timeline", "tick", "order"}, {"timeline", "tick", "order"}, "StoryTime")
    if not isinstance(point["timeline"], str) or point["timeline"] not in world.timeline_ids:
        raise UsageError("StoryTime timeline is not declared")
    for name in ("tick", "order"):
        if not isinstance(point[name], str) or not _DECIMAL.fullmatch(point[name]):
            raise UsageError(f"StoryTime {name} must be a canonical signed decimal string")
    try:
        return StoryTime(point["timeline"], int(point["tick"]), int(point["order"])).to_dict()
    except ValueError as exc:
        raise UsageError("StoryTime coordinate is outside supported bounds") from exc


def _reference(world: World, planned: dict[str, tuple[str, dict[str, Any] | None]],
               value: Any, kind: str) -> str:
    reference = _text(value, f"{kind} reference")
    if reference in planned:
        if planned[reference][0] != kind:
            raise UsageError("generational reference has the wrong kind")
        return reference
    record = world.find(reference, kind)
    if record.status != "canonical":
        raise UsageError("generational reference must name a canonical record")
    return record.id


def _references(world: World, planned: dict[str, tuple[str, dict[str, Any] | None]],
                fields: Any, required: set[str], optional: set[str], label: str,
                owner_kind: str) -> dict[str, Any]:
    raw = _closed(fields, required, required | optional, label)
    result = deepcopy(raw)
    for key, kind in _REF_KINDS.items():
        if key not in result:
            continue
        value = result[key]
        if key == "parent_id" and owner_kind == "parentage":
            kind = "character"
        if value is None and key in {"parent_id", "location_id", "organization_id", "predecessor_tenure_id", "successor_tenure_id", "holder_id"}:
            continue
        if key in {"participant_ids", "competes_with"}:
            if not isinstance(value, list) or (key == "participant_ids" and len(value) < 2) or len(value) > 100:
                raise UsageError(f"{label}.{key} must be a bounded reference array")
            result[key] = sorted(_reference(world, planned, item, kind) for item in value)
            if len(set(result[key])) != len(result[key]):
                raise UsageError(f"{label}.{key} contains duplicate references")
        else:
            result[key] = _reference(world, planned, value, kind)
    return result


def _payload(world: World, planned: dict[str, tuple[str, dict[str, Any] | None]],
             transition: str, raw: Any) -> dict[str, Any]:
    empty = {"organization-dormant", "organization-dissolve", "parentage-end", "union-end",
             "union-annul", "affiliation-end", "legacy-dormant", "legacy-dissolve",
             "tenure-end", "claim-recognize", "claim-withdraw", "claim-reject",
             "vital-initialize", "vital-birth", "vital-death", "vital-existence-start",
             "vital-existence-end"}
    fields = {
        "organization-initialize": {"title", "aliases"}, "organization-rename": {"title", "aliases"},
        "organization-reparent": {"parent_id"}, "parentage-initialize": {"basis"},
        "parentage-confirm": {"basis"}, "union-initialize": {"participant_ids"},
        "union-form": {"participant_ids"}, "union-reconcile": {"participant_ids"},
        "affiliation-initialize": {"role"}, "affiliation-role": {"role"},
        "legacy-initialize": {"title", "aliases"}, "legacy-rename": {"title", "aliases"},
        "tenure-initialize": {"holder_id", "basis"}, "tenure-designate": {"holder_id", "basis"},
        "tenure-hold": {"holder_id", "basis"}, "tenure-vacate": {"holder_id"},
        "tenure-transfer": {"from_tenure_id", "to_tenure_id"},
        "claim-initialize": {"competes_with"}, "claim-dispute": {"competes_with"},
    }.get(transition, set())
    if transition not in empty and not fields:
        raise UsageError("unsupported generational transition")
    value = _references(world, planned, raw, fields, set(), "payload", transition.split("-", 1)[0])
    if transition in {"organization-initialize", "organization-rename", "legacy-initialize", "legacy-rename"}:
        _text(value["title"], "payload.title")
        _strings(value["aliases"], "payload.aliases")
    if transition in {"parentage-initialize", "parentage-confirm"} and (not isinstance(value["basis"], str) or value["basis"] not in {"biological", "adoptive"}):
        raise UsageError("parentage basis is invalid")
    if transition.startswith("tenure-") and "basis" in value and (not isinstance(value["basis"], str) or value["basis"] not in {"legal", "de-facto"}):
        raise UsageError("tenure basis is invalid")
    if transition == "tenure-vacate" and value["holder_id"] is not None:
        raise UsageError("tenure vacancy must explicitly use null holder_id")
    if transition in {"tenure-designate", "tenure-hold"} and value["holder_id"] is None:
        raise UsageError("tenure designation or hold requires a holder")
    if transition == "tenure-transfer" and value["from_tenure_id"] == value["to_tenure_id"]:
        raise UsageError("tenure transfer endpoints must differ")
    if transition == "affiliation-role" and not _text(value["role"], "role"):
        raise UsageError("affiliation role is required")
    if transition == "affiliation-initialize" and value["role"] is not None:
        _text(value["role"], "role")
    return value


def _cause(world: World, planned: dict[str, tuple[str, dict[str, Any] | None]],
           value: Any, at: dict[str, Any]) -> str:
    identifier = _reference(world, planned, value, "event")
    event = world.get(identifier)
    point = StoryTime.from_value(event.frontmatter.get("time"))
    if point.timeline != at["timeline"] or (point.tick, point.order) >= (at["tick"], at["order"]):
        raise UsageError("non-initial generational transition requires an earlier canonical event cause")
    return identifier


def _items(intent: dict[str, Any]) -> list[dict[str, Any]]:
    action = intent.get("action")
    if action == "generational.batch":
        _closed(intent, {"action", "expectedHead", "idempotencyKey", "items"},
                {"action", "expectedHead", "idempotencyKey", "summary", "items"}, "generational batch")
        items = intent["items"]
        if not isinstance(items, list) or not 1 <= len(items) <= 32:
            raise UsageError("generational batch requires 1 to 32 explicit intents")
        return items
    _closed(intent, {"action", "expectedHead", "idempotencyKey"},
            (_CREATE_KEYS | _CORRECT_KEYS | _OUTER_KEYS), "generational intent")
    return [{key: value for key, value in intent.items() if key not in _OUTER_KEYS}]


def compile_operations(repository: Repository, intent: dict[str, Any]) -> list[dict[str, Any]]:
    if not isinstance(intent, dict):
        raise UsageError("generational intent must be an object")
    expected, key = intent.get("expectedHead"), intent.get("idempotencyKey")
    if not isinstance(expected, str) or not _SHA.fullmatch(expected):
        raise UsageError("generational intent requires an exact expectedHead")
    if not isinstance(key, str) or not key.strip() or len(key) > 256:
        raise UsageError("generational intent requires a nonblank idempotencyKey")
    if "summary" in intent:
        _text(intent["summary"], "summary")
    if repository.head() != expected:
        raise StaleRevision("generational intent expected a different HEAD",
                            details={"expected": expected, "actual": repository.head()})
    world = repository.load_world(expected, cache_write=False)
    if str(intent["action"]).startswith("generational.knowledge."):
        if world.schema != V07_SOURCE_SCHEMA:
            raise ChronologyUpgradeRequired("knowledge authoring requires wedl/v0.7; migration defaults are unchanged")
        return _knowledge_operations(world, intent, hashlib.sha256(canonical_json(intent).encode()).hexdigest())
    if world.schema != V07_SOURCE_SCHEMA or "generational-core-v1" not in (world.world_record.frontmatter.get("capabilities") or []):
        raise ChronologyUpgradeRequired("generational authoring requires wedl/v0.7 with generational-core-v1")
    items = _items(intent)
    digest = hashlib.sha256(canonical_json(intent).encode()).hexdigest()
    planned: dict[str, tuple[str, dict[str, Any] | None]] = {}
    for index, item in enumerate(items):
        if (not isinstance(item, dict) or not isinstance(item.get("action"), str)
                or item["action"] not in {"generational.create", "generational.append", "generational.correct"}):
            raise UsageError("unsupported generational intent variant")
        kind = item.get("kind")
        if not isinstance(kind, str) or kind not in GENERATIONAL_KINDS:
            raise UsageError("unsupported generational kind")
        if item["action"] == "generational.create":
            identifier = item.get("id", id_from_seed(kind, f"{digest}:{index}:record"))
            if not valid_id(identifier, kind) or identifier in world.records or identifier in planned:
                raise UsageError("generational create ID is invalid or already used")
            planned[identifier] = (kind, None)
    operations: list[dict[str, Any]] = []
    histories: dict[str, list[dict[str, Any]]] = {}
    for index, item in enumerate(items):
        action, kind = item["action"], item["kind"]
        if action == "generational.create":
            _closed(item, {"action", "kind", "title", "audience", "perspectives", "fields", "payload", "at"},
                    _CREATE_KEYS, "generational create")
            title = _text(item["title"], "title")
            tags = _strings(item.get("tags", []), "tags")
            aliases = _strings(item.get("aliases", []), "aliases")
            threads = _strings(item.get("threads", []), "threads")
            audience = _strings(item["audience"], "audience", nonempty=True)
            perspectives = _strings(item["perspectives"], "perspectives", nonempty=True)
            required, optional = _FIELDS[kind]
            fields = _references(world, planned, item["fields"], required, optional, "fields", kind)
            if kind == "organization" and (not isinstance(fields["organization_kind"], str) or fields["organization_kind"] not in {"house", "dynasty", "clan", "institution", "other"}):
                raise UsageError("organization_kind is invalid")
            if kind == "legacy" and (not isinstance(fields["legacy_kind"], str) or fields["legacy_kind"] not in {"office", "estate", "title", "other"}):
                raise UsageError("legacy_kind is invalid")
            if kind == "vital-history" and (not isinstance(fields["disclosure"], str) or fields["disclosure"] not in {"known", "unknown", "withheld"}):
                raise UsageError("vital disclosure is invalid")
            at = _time(item["at"], world)
            payload = _payload(world, planned, ("vital" if kind == "vital-history" else kind) + "-initialize", item["payload"])
            identifier = item.get("id", id_from_seed(kind, f"{digest}:{index}:record"))
            init = {"transition_id": id_from_seed("generational-transition", f"{digest}:{index}:initial"),
                    "transition_kind": ("vital" if kind == "vital-history" else kind) + "-initialize",
                    "applicability": {"applicability_kind": "instant", "point": at}, "payload": payload}
            frontmatter = {"schema": world.schema, "kind": kind, "id": identifier,
                           "title": title, "domain": item.get("domain", _DOMAINS[kind]),
                           "status": "canonical", "tags": tags, "aliases": aliases,
                           "threads": threads, "audience": audience, "perspectives": perspectives,
                           **fields, "initialization": init, "transitions": []}
            _text(frontmatter["domain"], "domain")
            planned[identifier] = (kind, frontmatter)
            histories[identifier] = []
            operations.append({"type": "entity.create", "value": {"frontmatter": frontmatter}})
            continue
        _closed(item, {"action", "kind", "record", "transition", "payload", "cause"},
                _CORRECT_KEYS if action == "generational.correct" else _APPEND_KEYS,
                "generational transition")
        identifier = _reference(world, planned, item["record"], kind)
        if identifier in planned:
            source = planned[identifier][1]
            if source is None:
                raise UsageError("batch transitions must follow their create intent")
        else:
            source = world.get(identifier).frontmatter
        transition = item["transition"]
        if not isinstance(transition, str) or transition not in TRANSITIONS[kind] or transition.endswith("-initialize"):
            raise UsageError("transition is not a closed variant for this kind")
        if transition == "tenure-vacate":
            if "at" in item or "interval" not in item:
                raise UsageError("tenure-vacate requires only an inclusive interval")
            interval = _closed(item["interval"], {"first", "last"}, {"first", "last"}, "interval")
            first, last = _time(interval["first"], world), _time(interval["last"], world)
            if first["timeline"] != last["timeline"] or (first["tick"], first["order"]) > (last["tick"], last["order"]):
                raise UsageError("tenure-vacate interval endpoints are invalid")
            applicability = {"applicability_kind": "inclusive-interval", "first": first, "last": last}
            at = first
        else:
            if "interval" in item or "at" not in item:
                raise UsageError("transition requires one exact StoryTime")
            at = _time(item["at"], world)
            applicability = {"applicability_kind": "instant", "point": at}
        payload = _payload(world, planned, transition, item["payload"])
        cause = _cause(world, planned, item["cause"], at)
        transition_id = item.get("transitionId", id_from_seed("generational-transition", f"{digest}:{index}:transition"))
        if not valid_id(transition_id, "generational-transition"):
            raise UsageError("transitionId must be a stable transition ID")
        entry = {"transition_id": transition_id, "transition_kind": transition,
                 "applicability": applicability, "payload": payload, "cause_event_id": cause}
        if action == "generational.correct":
            replaces = item.get("replaces")
            if not valid_id(replaces, "generational-transition"):
                raise UsageError("correction requires replaces transition ID")
            entry["replaces_transition_id"] = replaces
        current = histories.setdefault(identifier, deepcopy(source["transitions"]))
        current.append(entry)
        operations.append({"type": "entity.update", "entity": identifier,
                           "frontmatterPatch": {"transitions": deepcopy(current)}})
    return operations


def scaffold(repository: Repository) -> dict[str, Any]:
    """Current-HEAD starter; replace named choices before preview."""
    world = repository.load_world(repository.head(), cache_write=False)
    if world.schema != V07_SOURCE_SCHEMA or "generational-core-v1" not in (world.world_record.frontmatter.get("capabilities") or []):
        raise ChronologyUpgradeRequired("generational starter requires wedl/v0.7 with generational-core-v1")
    timeline = world.default_timeline
    cursor = world.current_time
    at = {"timeline": timeline, "tick": str(cursor.tick if cursor and cursor.timeline == timeline else 0),
          "order": str(cursor.order if cursor and cursor.timeline == timeline else 0)}
    return {"action": "generational.create", "expectedHead": repository.head(),
            "idempotencyKey": "generational-starter-choose-a-unique-key",
            "kind": "organization", "title": "New organization",
            "audience": ["public"], "perspectives": ["ordinary"],
            "fields": {"organization_kind": "house"},
            "payload": {"title": "New organization", "aliases": []}, "at": at}


def schema() -> dict[str, Any]:
    """Small machine-readable variant catalogue for authoring clients."""
    return {"protocol": "wedl-generational-authoring-schema/v1",
            "sourceSchema": V07_SOURCE_SCHEMA,
            "kinds": sorted(GENERATIONAL_KINDS),
            "variants": ["generational.create", "generational.append",
                         "generational.correct", "generational.batch", *KNOWLEDGE_ACTIONS],
            "fields": {kind: {"required": sorted(required), "optional": sorted(optional),
                              "transitions": sorted(TRANSITIONS[kind])}
                       for kind, (required, optional) in _FIELDS.items()},
            "confirmation": "author request preview -> author request apply --confirm TOKEN",
            "batchLimit": 32}
