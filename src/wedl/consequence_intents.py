"""Revision-bound consequence batches; only declared references are resolved."""
from __future__ import annotations

from copy import deepcopy
import re
from typing import Any, Callable

from . import consequence_operations as typed
from .errors import StaleRevision, UsageError
from .generational_knowledge import _EVIDENCE, _FIELDS
from .ids import KIND_PREFIX, valid_id, valid_spatial_id
from .model import World
from .repository import Repository
from .spatial_validation import _SPATIAL_KINDS

_SHA = re.compile(r"[0-9a-f]{40}\Z")
_EVENT_FIELDS = {"type", "temporaryId", "title", "time", "id", "domain", "status", "tags", "aliases",
                 "section_audiences", "location", "participants", "causes", "relatedStoryPoints",
                 "related_story_points", "effects", "bodyMarkdown"}
_TARGETS = {"knowledge.transition.append": ("knowledge", "knowledge"),
            "relationship.transition.append": ("relationship", "relationship"),
            "story-point.transition.append": ("storyPoint", "story-point")}
_ENTITY_KINDS = set(KIND_PREFIX) | _SPATIAL_KINDS


def _identifier(value: Any, kind: str | None) -> bool:
    return valid_id(value, kind) or (kind in _ENTITY_KINDS and valid_spatial_id(value, kind))


def validate(intent: Any) -> None:
    typed._closed(intent, {"action", "expectedHead", "idempotencyKey", "operations"},
                  {"summary", "consequenceRequest"}, "consequence batch")
    if intent["action"] != "consequence.batch" or not isinstance(intent["expectedHead"], str) or not _SHA.fullmatch(intent["expectedHead"]):
        raise UsageError("consequence batch requires an exact expectedHead")
    typed._text(intent["idempotencyKey"], "idempotencyKey")
    if "summary" in intent:
        typed._text(intent["summary"], "summary")
    operations = intent["operations"]
    if not isinstance(operations, list) or not 1 <= len(operations) <= 1000:
        raise UsageError("consequence batch requires 1 to 1000 operations")
    from .changeset import CHANGESET_OPERATION_TYPES, _authoring_input_bytes, _AUTHORING_MAX_REQUEST
    _authoring_input_bytes(intent, _AUTHORING_MAX_REQUEST)
    event_count = items = 0
    for operation in operations:
        if not isinstance(operation, dict) or not isinstance(operation.get("type"), str) or operation["type"] not in CHANGESET_OPERATION_TYPES:
            raise UsageError("unsupported consequence batch operation")
        event_count += operation["type"] == "event.create"
        items += 1
        for field in ("effects", "storyPoints", "scenes"):
            values = operation.get(field, [])
            if not isinstance(values, list):
                raise UsageError(f"consequence {field} must be an array")
            items += len(values)
        source = operation.get("value", {})
        if isinstance(source, dict):
            frontmatter = source.get("frontmatter", source)
            if isinstance(frontmatter, dict) and isinstance(frontmatter.get("transitions"), list):
                items += len(frontmatter["transitions"])
    if event_count > 100 or items > 1000:
        raise UsageError("consequence batch event or item limit exceeded")


def _declarations(operations: list[dict[str, Any]]) -> dict[str, str]:
    declared: dict[str, str] = {}
    def add(value: Any, kind: str) -> None:
        if not typed._temp(value) or value in declared:
            raise UsageError("consequence declarations require distinct scoped temporary IDs")
        declared[value] = kind
    for operation in operations:
        tag = operation["type"]
        temporary = operation.get("temporaryId", operation.get("tempId"))
        kind = None
        if tag in {"event.create", "conversation.create", "knowledge.create", "relationship.create"}:
            kind = tag.split(".")[0]
        elif tag in {"entity.create", "entity.upsert"}:
            source = operation.get("value", {})
            frontmatter = source.get("frontmatter", source) if isinstance(source, dict) else None
            if not isinstance(frontmatter, dict):
                raise UsageError("source frontmatter must be an object")
            kind = frontmatter.get("kind")
        elif tag == "conversation.turn.append": kind = "conversation-turn"
        elif tag == "conversation.recollection.record": kind = "conversation-recollection"
        if temporary is not None:
            if not isinstance(kind, str):
                raise UsageError("consequence declaration has no known kind")
            add(temporary, kind)
        if tag in {"event.create", "knowledge.create", "relationship.create"} and temporary is None:
            raise UsageError("consequence create requires a scoped temporaryId")
        for _, auxiliary_kind, members in typed._auxiliary(operation):
            if not isinstance(members, list):
                raise UsageError("consequence auxiliary collection must be an array")
            for member in members:
                if isinstance(member, dict) and typed._temp(member.get("id")):
                    add(member["id"], auxiliary_kind)
    return declared


def _value(value: Any, definition: Any, ref: Callable) -> Any:
    if not isinstance(definition, dict): return value
    if definition.get("type") == "entity" and isinstance(value, dict) and "entity" in value:
        value["entity"] = ref(value["entity"], definition.get("entity_kind"))
    elif definition.get("type") == "array" and isinstance(value, list):
        for index, item in enumerate(value): value[index] = _value(item, definition.get("items"), ref)
    elif definition.get("type") == "object" and isinstance(value, dict):
        for key, rule in (definition.get("properties") or {}).items():
            if key in value: value[key] = _value(value[key], rule, ref)
    return value


def _field(data: dict, name: str, kind: str | None, ref: Callable) -> None:
    if name in data and data[name] is not None:
        data[name] = ref(data[name], kind)


def _list(data: dict, name: str, kind: str | None, ref: Callable) -> None:
    if name in data and data[name] is not None:
        if not isinstance(data[name], list): raise UsageError(f"{name} must be an array")
        data[name] = [ref(item, kind) for item in data[name]]


def _transition(data: dict, ref: Callable) -> None:
    if not isinstance(data, dict): raise UsageError("transition must be an object")
    _field(data, "causing_event", "event", ref)
    _field(data, "source_entity", None, ref)


def _conversation_member(data: dict, ref: Callable, auxiliary: Callable, *, recollection: bool = False) -> None:
    if not isinstance(data, dict): raise UsageError("conversation member must be an object")
    kind = "conversation-recollection" if recollection else "conversation-turn"
    if "id" in data: data["id"] = auxiliary(data["id"], kind)
    def character(value: Any) -> str:
        if isinstance(value, str) and value.startswith("char:"):
            return "char:" + ref(value[5:], "character")
        return ref(value, "character")
    for field in ("speaker", "character"):
        _field(data, field, "character", ref)
    if data.get("addressee") is not None and data["addressee"] != "participants":
        data["addressee"] = character(data["addressee"])
    if data.get("actors") is not None:
        if not isinstance(data["actors"], list): raise UsageError("actors must be an array")
        data["actors"] = [character(item) for item in data["actors"]]
    if data.get("audience") is not None:
        audience = data["audience"]
        members = audience if isinstance(audience, list) else [audience]
        members = [item if item in ("participants", "public") else character(item) for item in members]
        data["audience"] = members if isinstance(audience, list) else members[0]
    if "interrupts" in data: data["interrupts"] = auxiliary(data["interrupts"], "conversation-turn")
    if data.get("exact_turns") is not None:
        if not isinstance(data["exact_turns"], list): raise UsageError("exact_turns must be an array")
        data["exact_turns"] = [auxiliary(item, "conversation-turn") for item in data["exact_turns"]]
    for quote in data.get("remembered_quotes") or []:
        if isinstance(quote, dict):
            if quote.get("source_turn") is not None: quote["source_turn"] = auxiliary(quote["source_turn"], "conversation-turn")
            _field(quote, "speaker", "character", ref)


def _genealogy(source: dict, ref: Callable, auxiliary: Callable) -> None:
    claim = source.get("claim")
    assertion = claim.get("genealogy") if isinstance(claim, dict) else None
    if not isinstance(assertion, dict): return
    rules = _FIELDS.get(assertion.get("kind"), {}) if isinstance(assertion.get("kind"), str) else {}
    payload = assertion.get("payload")
    if isinstance(payload, dict):
        for field, kind in rules.items():
            if kind and payload.get(field) is not None:
                if isinstance(payload[field], list): _list(payload, field, kind, ref)
                else: _field(payload, field, kind, ref)
    labels = assertion.get("labels")
    if isinstance(labels, dict): assertion["labels"] = {ref(key, None): label for key, label in labels.items()}
    for evidence in assertion.get("evidence") or []:
        if isinstance(evidence, dict) and isinstance(evidence.get("kind"), str) and evidence["kind"] in _EVIDENCE:
            kind, field, auxiliary_kind = _EVIDENCE[evidence["kind"]]
            _field(evidence, "entity_id", kind, ref)
            if field in evidence: evidence[field] = auxiliary(evidence[field], auxiliary_kind)


def _source(source: dict, kind: str, world: World, ref: Callable, auxiliary: Callable) -> None:
    if not isinstance(source, dict): raise UsageError("source frontmatter must be an object")
    fields = {"knowledge": {"knower": "character"}, "relationship": {"from": "character", "to": "character", "inverse": "relationship"},
              "event": {"location": "location"}, "scene": {"location": "location"},
              "conversation": {"scene": "scene", "sceneId": "scene", "location": "location", "locationId": "location"},
              "location": {"parent": "location", "parent_id": "location"}}.get(kind, {})
    for name, target_kind in fields.items(): _field(source, name, target_kind, ref)
    arrays = {"event": {"causes": "event", "related_story_points": "story-point"},
              "scene": {"objects": "object", "environments": "environment", "conversations": "conversation", "outcome_events": "event"},
              "story-point": {"outcome_events": "event"}}.get(kind, {})
    for name, target_kind in arrays.items(): _list(source, name, target_kind, ref)
    if kind in {"event", "scene", "conversation"}:
        for participant in source.get("participants") or []:
            if isinstance(participant, dict): _field(participant, "character", "character", ref)
        for point in source.get("story_points") or []:
            if isinstance(point, dict): _field(point, "story_point", "story-point", ref)
    if kind == "story-point":
        dependencies = source.get("dependencies")
        if isinstance(dependencies, dict): _list(dependencies, "all", "story-point", ref)
        lifecycle = source.get("lifecycle")
        if isinstance(lifecycle, dict):
            for transition in lifecycle.get("transitions") or []: _transition(transition, ref)
    if kind in {"knowledge", "relationship"}:
        for transition in source.get("transitions") or []: _transition(transition, ref)
    if kind == "knowledge": _genealogy(source, ref, auxiliary)
    if kind == "conversation":
        for turn in source.get("turns") or []: _conversation_member(turn, ref, auxiliary)
        for recollection in source.get("recollections") or []: _conversation_member(recollection, ref, auxiliary, recollection=True)
    rules = (world.config.get("state_keys") or {}).get(kind, {})
    initial = source.get("initial_state")
    if isinstance(initial, dict):
        for key, value in initial.items(): initial[key] = _value(value, rules.get(key), ref)


def _walk(operation: dict, world: World, ref: Callable, auxiliary: Callable, kind_of: Callable) -> dict:
    op = deepcopy(operation)
    tag = op["type"]
    if tag in _TARGETS:
        target, kind = _TARGETS[tag]
        _field(op, target, kind, ref)
        _transition(op.get("transition"), ref)
    elif tag == "outcome.link":
        _field(op, "event", "event", ref)
        _list(op, "storyPoints", "story-point", ref); _list(op, "scenes", "scene", ref)
    elif tag == "event.create":
        _field(op, "location", "location", ref)
        for participant in op.get("participants") or []:
            if isinstance(participant, dict): _field(participant, "character", "character", ref)
        for name, kind in (("causes", "event"), ("relatedStoryPoints", "story-point"), ("related_story_points", "story-point")):
            _list(op, name, kind, ref)
        for effect in op.get("effects") or []:
            effect["target"] = ref(effect["target"], None)
            rule = ((world.config.get("state_keys") or {}).get(kind_of(effect["target"]), {}) or {}).get(effect["key"])
            if isinstance(rule, dict) and rule.get("type") == "array" and effect["operation"] in {"add-to-set", "remove-from-set"}: rule = rule.get("items")
            if "value" in effect: effect["value"] = _value(effect["value"], rule, ref)
    elif tag in {"knowledge.create", "relationship.create", "entity.create", "entity.upsert"}:
        value = op.get("value")
        if not isinstance(value, dict): raise UsageError("create value must be an object")
        source = value.get("frontmatter", value)
        if not isinstance(source, dict): raise UsageError("source frontmatter must be an object")
        if "id" in source: source["id"] = auxiliary(source["id"], source.get("kind"))
        _source(source, source.get("kind"), world, ref, auxiliary)
    elif tag in {"entity.update", "entity.delete"}:
        for field in ("entity", "entityId"): _field(op, field, None, ref)
        identifier = op.get("entity", op.get("entityId"))
        patch = op.get("frontmatterPatch")
        if isinstance(patch, dict): _source(patch, kind_of(identifier), world, ref, auxiliary)
    elif tag == "conversation.create":
        source = op.get("value", op)
        _source(source, "conversation", world, ref, auxiliary)
    elif tag in {"conversation.turn.append", "conversation.recollection.record"}:
        for field in ("conversation", "conversationId"): _field(op, field, "conversation", ref)
        source = op.get("turn", op.get("recollection", op.get("value", {})))
        if isinstance(source, dict):
            _conversation_member(source, ref, auxiliary, recollection=tag == "conversation.recollection.record")
    return op


def _event(operation: dict, world: World) -> None:
    typed._closed(operation, {"type", "temporaryId", "title", "time"}, _EVENT_FIELDS - {"type", "temporaryId", "title", "time"}, "batch event")
    typed._text(operation["title"], "event title")
    operation["time"] = typed._point(operation["time"], transport=True)
    if operation["time"]["timeline"] not in world.timeline_ids: raise UsageError("event timeline must be declared")
    if "relatedStoryPoints" in operation and "related_story_points" in operation:
        raise UsageError("batch event cannot contain both related-story-point spellings")
    if "id" in operation and not valid_id(operation["id"], "event"): raise UsageError("event ID has the wrong kind")
    for effect in operation.get("effects") or []:
        if not isinstance(effect, dict) or not isinstance(effect.get("operation"), str) or effect["operation"] not in {"set", "clear", "add-to-set", "remove-from-set"}:
            raise UsageError("unsupported batch effect operation")
        required = {"target", "key", "operation"} | ({"value"} if effect["operation"] != "clear" else set())
        typed._closed(effect, required, {"id"}, "batch effect")
        typed._text(effect["key"], "effect key")
        if "id" in effect and not (typed._temp(effect["id"]) or valid_id(effect["id"], "effect")):
            raise UsageError("effect ID has the wrong kind")


def compile_batch(repository: Repository, intent: dict[str, Any]) -> dict[str, Any]:
    validate(intent)
    expected = intent["expectedHead"]
    current = repository.head()
    if current != expected: raise StaleRevision("consequence batch expected a different HEAD", details={"expected": expected, "actual": current})
    world = repository.load_world(expected, cache_write=False)
    operations = deepcopy(intent["operations"])
    typed.normalize({"operations": operations})
    declared = _declarations(operations)
    def reference(value: Any, kind: str | None = None) -> str:
        typed._text(value, "consequence reference")
        if typed._temp(value):
            actual = declared.get(value)
            if actual is None or (kind is not None and kind != actual) or actual not in _ENTITY_KINDS:
                raise UsageError("undeclared or wrong-kind consequence temporary reference")
            return value
        return world.find(value, kind).id
    def auxiliary(value: Any, kind: str) -> str:
        if typed._temp(value):
            if declared.get(value) != kind: raise UsageError("undeclared or wrong-kind auxiliary reference")
        elif not _identifier(value, kind): raise UsageError("declaration/reference requires a matching stable ID")
        return value
    def kind_of(identifier: str) -> str:
        return declared[identifier] if typed._temp(identifier) else world.get(identifier).kind
    compiled = []
    for operation in operations:
        if operation["type"] == "event.create": _event(operation, world)
        compiled.append(_walk(operation, world, reference, auxiliary, kind_of))
    payload = {"protocol": "wedl-changeset/v1", "expectedHead": expected, "idempotencyKey": intent["idempotencyKey"],
               "summary": intent.get("summary", "Authoring: consequence.batch"), "operations": compiled}
    # Shared normalizers validate typed shapes without altering input or IDs.
    typed.normalize(payload)
    if "consequenceRequest" in intent: payload["consequenceRequest"] = deepcopy(intent["consequenceRequest"])
    return payload


def allocate(payload: dict, generated: dict[str, str]) -> dict[str, str]:
    for operation in payload["operations"]:
        if operation["type"] == "event.create" and "id" in operation:
            generated[operation["temporaryId"]] = operation["id"]
        elif operation["type"] in {"entity.create", "entity.upsert"}:
            source = operation["value"].get("frontmatter", operation["value"])
            temporary = operation.get("temporaryId", operation.get("tempId"))
            if temporary and isinstance(source, dict) and _identifier(source.get("id"), source.get("kind")):
                generated[temporary] = source["id"]
        elif operation["type"] in {"conversation.turn.append", "conversation.recollection.record"}:
            recollection = operation["type"] == "conversation.recollection.record"
            source = operation.get("recollection" if recollection else "turn", operation.get("value", {}))
            temporary = operation.get("temporaryId", operation.get("tempId"))
            kind = "conversation-recollection" if recollection else "conversation-turn"
            if temporary and isinstance(source, dict) and valid_id(source.get("id"), kind):
                generated[temporary] = source["id"]
    if len(set(generated.values())) != len(generated): raise UsageError("consequence generated IDs collide")
    return generated


def expand(operation: dict, replacements: dict[str, str], digest: str, index: int, world: World) -> dict:
    def ref(value: Any, kind: str | None = None) -> str:
        typed._text(value, "consequence reference")
        if typed._temp(value):
            if value not in replacements: raise UsageError("undeclared consequence temporary reference")
            value = replacements[value]
        if kind is None:
            if not any(_identifier(value, entity_kind) for entity_kind in _ENTITY_KINDS):
                raise UsageError("consequence entity reference must have an entity identifier")
        elif not _identifier(value, kind):
            raise UsageError("consequence reference has the wrong kind")
        return value
    def kind_of(identifier: str) -> str:
        record = world.maybe_get(identifier)
        if record: return record.kind
        return next((kind for kind in KIND_PREFIX if valid_id(identifier, kind) or valid_spatial_id(identifier, kind)), "")
    op = _walk(operation, world, ref, ref, kind_of)
    for field in ("temporaryId", "tempId"):
        if field in op: op[field] = replacements.get(op[field], op[field])
    if op["type"] in typed.TYPES or op["type"] == "event.create":
        return typed.expand(op, replacements, digest, index, world)
    if op["type"] == "entity.update" and isinstance(op.get("frontmatterPatch"), dict) and "chronology" in op["frontmatterPatch"]:
        from .changeset import _replace_chronology_patch
        op["frontmatterPatch"]["chronology"] = _replace_chronology_patch(op["frontmatterPatch"]["chronology"], replacements)
    return op
