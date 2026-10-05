"""Closed additive operations; source validation and transactions stay shared."""
from __future__ import annotations

from copy import deepcopy
import math
import re
from typing import Any

from .errors import UsageError
from .generational_knowledge import _EVIDENCE, _FIELDS
from .ids import KIND_PREFIX, id_from_seed, valid_id, valid_spatial_id
from .model import Record, StoryTime, World
from .source import generated_path, serialize_record


TYPES = (
    "knowledge.create", "relationship.create", "knowledge.transition.append",
    "relationship.transition.append", "story-point.transition.append", "outcome.link",
)
_APPENDS = {"knowledge.transition.append": ("knowledge", "knowledge", "transitions"),
            "relationship.transition.append": ("relationship", "relationship", "transitions"),
            "story-point.transition.append": ("story-point", "storyPoint", "lifecycle.transitions")}
_STATES = {"accepted", "suspected", "rejected", "uncertain", "remembered", "forgotten"}
_PLOT_STATES = {"dormant", "active", "resolved", "failed", "cancelled"}
_DECIMAL = re.compile(r"(?:0|-?[1-9][0-9]*)\Z")


def _closed(value: Any, required: set[str], optional: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or required - value.keys() or value.keys() - required - optional:
        raise UsageError(f"{label} has missing or unsupported fields")
    return value


def _text(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise UsageError(f"{label} must be a nonblank string")
    return value


def _temp(value: Any) -> bool:
    return isinstance(value, str) and value.startswith("tmp:") and bool(value[4:].strip())


def _finite(value: Any) -> None:
    if isinstance(value, float) and not math.isfinite(value):
        raise UsageError("consequence values must be finite JSON")
    if isinstance(value, dict):
        if any(not isinstance(key, str) for key in value):
            raise UsageError("consequence object keys must be strings")
        for item in value.values():
            _finite(item)
    elif isinstance(value, list):
        for item in value:
            _finite(item)
    elif value is not None and not isinstance(value, (str, int, float, bool)):
        raise UsageError("consequence values must be JSON")


def _point(value: Any, *, transport: bool) -> dict[str, Any]:
    point = _closed(value, {"timeline", "tick", "order"}, set(), "consequence time")
    _text(point["timeline"], "timeline")
    if transport:
        if any(not isinstance(point[name], str) or not _DECIMAL.fullmatch(point[name]) for name in ("tick", "order")):
            raise UsageError("consequence time requires canonical signed decimal strings")
    try:
        if transport:
            point = {"timeline": point["timeline"], "tick": int(point["tick"]), "order": int(point["order"])}
        return StoryTime.from_value(point).to_dict()
    except ValueError as exc:
        raise UsageError("consequence time is outside supported bounds") from exc


def _transition(value: Any, kind: str, *, initial: bool) -> dict[str, Any]:
    required = {"time"} | ({"state"} if kind != "relationship" else set())
    optional = {"id", "causing_event", "note"}
    if not initial:
        required.add("causing_event")
    if kind == "knowledge":
        optional |= {"confidence", "acquisition", "source_entity"}
    elif kind == "relationship":
        optional |= {"relationship_status", "metrics", "facets"}
    if initial:
        if not isinstance(value, dict) or required - value.keys():
            raise UsageError("created transition is missing source fields")
    else:
        _closed(value, required, optional, "typed transition")
    result = deepcopy(value)
    result["time"] = _point(result["time"], transport=not initial)
    if kind != "relationship" and (not isinstance(result["state"], str) or result["state"] not in (_STATES if kind == "knowledge" else _PLOT_STATES)):
        raise UsageError("unsupported consequence transition state")
    for field in ("causing_event", "source_entity", "acquisition", "relationship_status"):
        if field in result and (result[field] is not None or not initial):
            _text(result[field], field)
    if "note" in result and not isinstance(result["note"], str):
        raise UsageError("transition note must be a string")
    if "confidence" in result:
        confidence = result["confidence"]
        if isinstance(confidence, bool) or not isinstance(confidence, (int, float)) or not 0 <= confidence <= 1:
            raise UsageError("confidence must be a finite number from zero to one")
    if "metrics" in result and (not isinstance(result["metrics"], dict) or any(
            not isinstance(name, str) or not name.strip() or isinstance(metric, bool)
            or not isinstance(metric, (int, float)) or not math.isfinite(metric)
            for name, metric in result["metrics"].items())):
        raise UsageError("relationship metrics must be finite numeric values")
    if "facets" in result and (not isinstance(result["facets"], list)
                               or any(not isinstance(item, str) or not item.strip() for item in result["facets"])):
        raise UsageError("relationship facets must be a string array")
    return result


def is_typed_event(operation: dict[str, Any]) -> bool:
    """New auxiliary leaves opt in; legacy entity labels never select a codec."""
    effects = operation.get("effects")
    return operation.get("type") == "event.create" and isinstance(effects, list) and any(
        isinstance(effect, dict) and ("id" not in effect or _temp(effect.get("id"))) for effect in effects)


def normalize(payload: dict[str, Any]) -> dict[str, Any]:
    """Normalize only new transport leaves, without allocating or changing input."""
    result = deepcopy(payload)
    for operation in result.get("operations") or []:
        operation_type = operation.get("type") if isinstance(operation, dict) else None
        if isinstance(operation, dict) and is_typed_event(operation):
            # event.create remains the existing raw operation. The batch
            # adapter closes its transport; auxiliary allocation changes no
            # raw event fields or defaulted integer-time rules.
            _finite(operation)
            for effect in operation.get("effects") or []:
                if not isinstance(effect, dict) or not isinstance(effect.get("operation"), str) or effect["operation"] not in {"set", "clear", "add-to-set", "remove-from-set"}:
                    raise UsageError("unsupported typed effect operation")
                required = {"target", "key", "operation"} | ({"value"} if effect["operation"] != "clear" else set())
                _closed(effect, required, {"id"}, "typed effect")
                _text(effect["target"], "effect target")
                _text(effect["key"], "effect key")
        if operation_type not in TYPES:
            continue
        _finite(operation)
        if operation_type.endswith(".create"):
            _closed(operation, {"type", "temporaryId", "value"}, set(), "typed create")
            if not _temp(operation["temporaryId"]):
                raise UsageError("typed create requires a scoped temporaryId")
            value = _closed(operation["value"], {"frontmatter", "bodyMarkdown"}, set(), "typed create value")
            kind = operation_type.split(".")[0]
            fm = value["frontmatter"]
            required = {"schema", "kind", "title", "domain", "status", "tags", "aliases", "transitions"}
            required |= {"knower", "claim"} if kind == "knowledge" else {"from", "to", "relationship_kind"}
            if not isinstance(fm, dict) or required - fm.keys() or fm["kind"] != kind or not isinstance(value["bodyMarkdown"], str):
                raise UsageError("typed create requires a complete matching source envelope")
            for field in ("schema", "title", "domain", "status"):
                _text(fm[field], field)
            for field in ("tags", "aliases"):
                if not isinstance(fm[field], list) or any(not isinstance(item, str) for item in fm[field]):
                    raise UsageError("created tags and aliases must be string arrays")
            if not isinstance(fm["transitions"], list):
                raise UsageError("created transitions must be an array")
            fm["transitions"] = [_transition(item, kind, initial=True) for item in fm["transitions"]]
        elif operation_type in _APPENDS:
            kind, target, _ = _APPENDS[operation_type]
            _closed(operation, {"type", target, "transition"}, set(), "typed append")
            _text(operation[target], target)
            operation["transition"] = _transition(operation["transition"], kind, initial=False)
        else:
            _closed(operation, {"type", "event", "storyPoints", "scenes"}, set(), "outcome link")
            _text(operation["event"], "outcome event")
            for field in ("storyPoints", "scenes"):
                values = operation[field]
                if not isinstance(values, list) or any(not isinstance(item, str) or not item.strip() for item in values) or len(values) != len(set(values)):
                    raise UsageError("outcome targets must be unique reference arrays")
            if not operation["storyPoints"] and not operation["scenes"]:
                raise UsageError("outcome link needs an expressly supplied target")
    return result


def _auxiliary(operation: dict[str, Any]) -> list[tuple[str, str, list[dict[str, Any]]]]:
    operation_type = operation.get("type")
    if operation_type in _APPENDS:
        kind, _, collection = _APPENDS[operation_type]
        return [(collection, f"{kind}-transition", [operation["transition"]])]
    if operation_type in {"knowledge.create", "relationship.create"}:
        kind = operation_type.split(".")[0]
        return [("transitions", f"{kind}-transition", operation["value"]["frontmatter"]["transitions"])]
    if operation_type == "event.create":
        return [("effects", "effect", operation.get("effects") or [])]
    return []


def allocate(payload: dict[str, Any], generated: dict[str, str], digest: str) -> dict[str, str]:
    """Extend the shared allocator with closed declarations and auxiliary leaves."""
    normalized = normalize(payload)
    seen: set[str] = set()
    for index, operation in enumerate(normalized.get("operations") or []):
        temporary = operation.get("temporaryId") or operation.get("tempId")
        if temporary:
            _text(temporary, "temporary declaration")
            if temporary in seen:
                raise UsageError("duplicate consequence temporary declaration")
            seen.add(temporary)
        if operation.get("type") in {"knowledge.create", "relationship.create"}:
            kind = operation["type"].split(".")[0]
            supplied = operation["value"]["frontmatter"].get("id")
            if supplied is not None and not valid_id(supplied, kind):
                raise UsageError("typed create ID has the wrong kind")
            generated[temporary] = supplied or id_from_seed(kind, f"{digest}:{index}:{temporary}")
        auxiliaries = [] if operation.get("type") == "event.create" and not is_typed_event(operation) else _auxiliary(operation)
        for collection, kind, members in auxiliaries:
            if not isinstance(members, list):
                raise UsageError("consequence auxiliary collection must be an array")
            for ordinal, member in enumerate(members):
                if not isinstance(member, dict):
                    raise UsageError("consequence auxiliary member must be an object")
                identifier = member.get("id")
                if identifier is None:
                    if "id" in member:
                        raise UsageError("consequence ID cannot be null")
                elif _temp(identifier):
                    if identifier in seen:
                        raise UsageError("duplicate consequence temporary declaration")
                    seen.add(identifier)
                    generated[identifier] = id_from_seed(kind, f"{digest}:{index}:{collection}:{ordinal}")
                elif not valid_id(identifier, kind):
                    raise UsageError("consequence auxiliary ID has the wrong kind")
    return generated


def _ref(value: Any, replacements: dict[str, str], kind: str | None = None) -> str:
    _text(value, "consequence reference")
    if value.startswith("tmp:"):
        if value not in replacements:
            raise UsageError("undeclared consequence temporary reference")
        value = replacements[value]
    if kind is None and not (any(valid_id(value, entity_kind) for entity_kind in KIND_PREFIX) or valid_spatial_id(value)):
        raise UsageError("consequence entity reference must have an entity identifier")
    if kind is not None and not valid_id(value, kind) and not (kind == "location" and valid_spatial_id(value, kind)):
        raise UsageError("consequence reference has the wrong kind")
    return value


def expand(operation: dict[str, Any], replacements: dict[str, str], digest: str, index: int, world: World) -> dict[str, Any]:
    """Replace schema-declared leaves only, leaving prose/extensions literal."""
    result = deepcopy(operation)
    operation_type = result.get("type")
    if "temporaryId" in result:
        result["temporaryId"] = replacements.get(result["temporaryId"], result["temporaryId"])
    for collection, kind, members in _auxiliary(result):
        for ordinal, member in enumerate(members):
            identifier = member.get("id")
            member["id"] = (_ref(identifier, replacements, kind) if identifier is not None
                            else id_from_seed(kind, f"{digest}:{index}:{collection}:{ordinal}"))
            if collection != "effects":
                for field, ref_kind in (("causing_event", "event"), ("source_entity", None)):
                    if member.get(field) is not None:
                        member[field] = _ref(member[field], replacements, ref_kind)
    if operation_type in _APPENDS:
        kind, target, _ = _APPENDS[operation_type]
        result[target] = _ref(result[target], replacements, kind)
    elif operation_type == "outcome.link":
        result["event"] = _ref(result["event"], replacements, "event")
        for field, kind in (("storyPoints", "story-point"), ("scenes", "scene")):
            result[field] = [_ref(item, replacements, kind) for item in result[field]]
            if len(result[field]) != len(set(result[field])):
                raise UsageError("outcome references resolve to duplicate targets")
    elif operation_type in {"knowledge.create", "relationship.create"}:
        fm = result["value"]["frontmatter"]
        fm["id"] = result["temporaryId"]
        for field in ("knower", "from", "to"):
            if field in fm:
                fm[field] = _ref(fm[field], replacements, "character")
        if fm.get("inverse") is not None:
            fm["inverse"] = _ref(fm["inverse"], replacements, "relationship")
        assertion = (fm.get("claim") or {}).get("genealogy") if isinstance(fm.get("claim"), dict) else None
        if isinstance(assertion, dict):
            fields = _FIELDS.get(assertion.get("kind"), {}) if isinstance(assertion.get("kind"), str) else {}
            for field, ref_kind in fields.items():
                payload = assertion.get("payload")
                if ref_kind and isinstance(payload, dict) and payload.get(field) is not None:
                    value = payload[field]
                    payload[field] = [_ref(item, replacements, ref_kind) for item in value] if isinstance(value, list) else _ref(value, replacements, ref_kind)
            for evidence in assertion.get("evidence") or []:
                if isinstance(evidence, dict) and isinstance(evidence.get("kind"), str) and evidence["kind"] in _EVIDENCE:
                    record_kind, member_field, member_kind = _EVIDENCE[evidence["kind"]]
                    evidence["entity_id"] = _ref(evidence.get("entity_id"), replacements, record_kind)
                    evidence[member_field] = _ref(evidence.get(member_field), replacements, member_kind)
            if isinstance(assertion.get("labels"), dict):
                assertion["labels"] = {_ref(key, replacements): value for key, value in assertion["labels"].items()}
    elif operation_type == "event.create":
        if result.get("location") is not None:
            result["location"] = _ref(result["location"], replacements, "location")
        for participant in result.get("participants") or []:
            if isinstance(participant, dict) and "character" in participant:
                participant["character"] = _ref(participant["character"], replacements, "character")
        for field, kind in (("causes", "event"), ("relatedStoryPoints", "story-point"), ("related_story_points", "story-point")):
            if field in result:
                result[field] = [_ref(item, replacements, kind) for item in result[field]]
        for effect in result.get("effects") or []:
            effect["target"] = _ref(effect.get("target"), replacements)
            schemas = world.config.get("state_keys") or {}
            kind = next((kind for kind in schemas if valid_id(effect["target"], kind)
                         or valid_spatial_id(effect["target"], kind)), None)
            rule = (schemas.get(kind) or {}).get(effect["key"]) or {}
            if rule.get("type") == "entity" and isinstance(effect.get("value"), dict) and "entity" in effect["value"]:
                effect["value"]["entity"] = _ref(effect["value"]["entity"], replacements, rule.get("entity_kind"))
    return result


def _record(records: dict[str, Record], identifier: str, kind: str) -> Record:
    record = records.get(identifier)
    if record is None or record.kind != kind:
        raise UsageError(f"consequence operation requires an existing {kind}")
    return record


def admit_references(operation: dict[str, Any], records: dict[str, Record], world: World) -> None:
    """Admit declared leaves against the complete candidate, including forward refs."""
    def entity(identifier: Any, kind: str | None = None) -> Record:
        identifier = _ref(identifier, {}, kind)
        record = records.get(identifier)
        if record is None or (kind is not None and record.kind != kind):
            raise UsageError("consequence reference requires an existing matching entity")
        if not (valid_id(identifier, record.kind) and record.kind in KIND_PREFIX) and not valid_spatial_id(identifier, record.kind):
            raise UsageError("consequence reference cannot name an auxiliary record")
        return record

    def transitions(members: list[dict[str, Any]]) -> None:
        for member in members:
            for field, kind in (("causing_event", "event"), ("source_entity", None)):
                if member.get(field) is not None:
                    entity(member[field], kind)

    operation_type = operation["type"]
    if operation_type in _APPENDS:
        kind, target, _ = _APPENDS[operation_type]
        entity(operation[target], kind)
        transitions([operation["transition"]])
    elif operation_type in {"knowledge.create", "relationship.create"}:
        fm = operation["value"]["frontmatter"]
        entity(fm["id"], fm["kind"])
        for field in ("knower", "from", "to"):
            if field in fm:
                entity(fm[field], "character")
        if fm.get("inverse") is not None:
            entity(fm["inverse"], "relationship")
        transitions(fm["transitions"])
        assertion = (fm.get("claim") or {}).get("genealogy") if isinstance(fm.get("claim"), dict) else None
        if isinstance(assertion, dict):
            fields = _FIELDS.get(assertion.get("kind"), {}) if isinstance(assertion.get("kind"), str) else {}
            for field, kind in fields.items():
                payload = assertion.get("payload")
                if kind and isinstance(payload, dict) and payload.get(field) is not None:
                    values = payload[field] if isinstance(payload[field], list) else [payload[field]]
                    for identifier in values:
                        entity(identifier, kind)
            for evidence in assertion.get("evidence") or []:
                if not isinstance(evidence, dict) or not isinstance(evidence.get("kind"), str) or evidence["kind"] not in _EVIDENCE:
                    continue
                kind, field, auxiliary_kind = _EVIDENCE[evidence["kind"]]
                owner = entity(evidence.get("entity_id"), kind)
                identifier = _ref(evidence.get(field), {}, auxiliary_kind)
                collection = {"transition_id": "transitions", "observation_id": "observations",
                              "turn_id": "turns", "recollection_id": "recollections"}[field]
                if not any(isinstance(item, dict) and item.get("id") == identifier for item in owner.frontmatter.get(collection) or []):
                    raise UsageError("consequence evidence must name a member of its declared owner")
            for identifier in assertion.get("labels") or {}:
                entity(identifier)
    elif operation_type == "outcome.link":
        entity(operation["event"], "event")
        for field, kind in (("storyPoints", "story-point"), ("scenes", "scene")):
            for identifier in operation[field]:
                entity(identifier, kind)
    elif operation_type == "event.create":
        if operation.get("location") is not None:
            entity(operation["location"], "location")
        for participant in operation.get("participants") or []:
            if isinstance(participant, dict) and "character" in participant:
                entity(participant["character"], "character")
        for field, kind in (("causes", "event"), ("relatedStoryPoints", "story-point"), ("related_story_points", "story-point")):
            for identifier in operation.get(field) or []:
                entity(identifier, kind)
        for effect in operation.get("effects") or []:
            target = entity(effect["target"])
            rule = ((world.config.get("state_keys") or {}).get(target.kind) or {}).get(effect["key"]) or {}
            if rule.get("type") == "entity" and isinstance(effect.get("value"), dict) and "entity" in effect["value"]:
                entity(effect["value"]["entity"], rule.get("entity_kind"))


def _metrics(transition: dict[str, Any], world: World) -> None:
    definitions = world.config.get("relationship_metrics") or {}
    for name, value in (transition.get("metrics") or {}).items():
        rule = definitions.get(name)
        if not isinstance(rule, dict) or (rule.get("minimum") is not None and value < rule["minimum"]) or (rule.get("maximum") is not None and value > rule["maximum"]):
            raise UsageError("relationship metric is undeclared or outside its bounds")


def apply_operation(operation: dict[str, Any], records: dict[str, Record], world: World) -> set[str]:
    operation_type = operation["type"]
    if operation_type.endswith(".create"):
        value = operation["value"]
        fm = value["frontmatter"]
        identifier = fm["id"]
        if identifier in records or fm["schema"] != world.schema:
            raise UsageError("typed create cannot replace an existing record or change source schema")
        if fm["kind"] == "relationship":
            for transition in fm["transitions"]:
                _metrics(transition, world)
        path = generated_path(world.source_root, fm["kind"], fm["title"], identifier, fm)
        records[identifier] = Record(fm, value["bodyMarkdown"], path, serialize_record(fm, value["bodyMarkdown"]))
        return {identifier}
    if operation_type in _APPENDS:
        kind, target, collection = _APPENDS[operation_type]
        record = _record(records, operation[target], kind)
        owner = record.frontmatter.setdefault("lifecycle", {}) if collection == "lifecycle.transitions" else record.frontmatter
        history = owner.setdefault("transitions", [])
        transition = operation["transition"]
        point = StoryTime.from_value(transition["time"])
        if point.timeline not in world.timeline_ids:
            raise UsageError("transition timeline is not declared")
        if not isinstance(history, list):
            raise UsageError("transition history must be an array")
        if any(item.get("id") == transition["id"] for item in history if isinstance(item, dict)):
            raise UsageError("transition ID collides with existing history")
        if history:
            previous = StoryTime.from_value(history[-1]["time"], world.default_timeline)
            if previous.timeline != point.timeline or (previous.tick, previous.order) >= (point.tick, point.order):
                raise UsageError("typed transition must strictly follow its existing history")
        if kind == "relationship":
            _metrics(transition, world)
        history.append(transition)
        record.raw_bytes = serialize_record(record.frontmatter, record.body)
        return {record.id}
    event = _record(records, operation["event"], "event")
    if event.status != "canonical":
        raise UsageError("outcome event must be canonical")
    touched: set[str] = set()
    for field, kind in (("storyPoints", "story-point"), ("scenes", "scene")):
        for identifier in operation[field]:
            target = _record(records, identifier, kind)
            outcomes = target.frontmatter.setdefault("outcome_events", [])
            if not isinstance(outcomes, list):
                raise UsageError("outcome history must be an array")
            if event.id not in outcomes:
                if kind == "story-point" and outcomes:
                    previous = StoryTime.from_value(_record(records, outcomes[-1], "event").frontmatter["time"], world.default_timeline)
                    point = StoryTime.from_value(event.frontmatter["time"], world.default_timeline)
                    if previous.timeline != point.timeline or (previous.tick, previous.order) >= (point.tick, point.order):
                        raise UsageError("outcome must strictly follow the existing outcome history")
                outcomes.append(event.id)
                touched.add(target.id)
            if kind == "story-point":
                related = event.frontmatter.setdefault("related_story_points", [])
                if not isinstance(related, list):
                    raise UsageError("related story points must be an array")
                if target.id not in related:
                    related.append(target.id)
                    touched.add(event.id)
    for identifier in touched:
        record = records[identifier]
        record.raw_bytes = serialize_record(record.frontmatter, record.body)
    return touched
