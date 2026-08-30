"""Small, name-oriented authoring commands compiled to raw changesets.

This module intentionally does not add another persistence protocol.  It
resolves author-facing references against a world and produces the existing
``wedl-changeset/v1`` document, so preview, confirmation, idempotency and the
normal compiler remain the single write path.
"""

from __future__ import annotations

import hashlib
import json
import re
from copy import deepcopy
from typing import Any

from .errors import ConfirmationMismatch, ConfirmationRequired, ConflictError, UsageError
from .errors import ChronologyUpgradeRequired
from . import CHRONOLOGY_SOURCE_SCHEMA
from .conversation import participant_at
from .ids import id_from_seed
from .model import ORDER_MAX, ORDER_MIN, StoryTime, World
from .repository import Repository
from .semantics import resolve_state
from .util import canonical_json
from .validation import is_adoptable_canonical_record


def _time(world: World, value: dict[str, Any] | None, *, fallback: StoryTime | None = None) -> dict[str, Any]:
    if value is None:
        if fallback is None:
            raise UsageError("a story time is required; provide --tick or select a scene with a current time")
        return fallback.to_dict()
    try:
        tick = value["tick"]
        order = value.get("order", 0)
        if isinstance(tick, bool) or not isinstance(tick, int) or isinstance(order, bool) or not isinstance(order, int):
            raise ValueError("tick and order must be integers")
        return world.story_time(tick, value.get("timeline"), order).to_dict()
    except (KeyError, TypeError, ValueError) as exc:
        raise UsageError("story time must contain a valid tick, optional timeline, and optional order") from exc


def _reference(world: World, value: str, kind: str | None = None) -> str:
    return world.find(value, kind).id


def _validate_intent_types(intent: dict[str, Any]) -> None:
    """Enforce the semantic request JSON types before any name resolution.

    HTTP bodies arrive as untyped dictionaries, so OpenAPI is documentation,
    not runtime validation. Refuse coercion here to match the published shape.
    """

    text_fields = {"action", "summary", "idempotencyKey", "title", "location", "scene", "event", "conversation", "hypothesis", "text", "kind", "speaker", "addressee", "statement", "context", "note", "timeline"}
    for field in text_fields:
        if field in intent and intent[field] is not None and not isinstance(intent[field], str):
            raise UsageError(f"authoring field {field!r} must be a string")
    for field in ("characters", "actors", "subjects", "alternatives", "canonicalRecords"):
        if field in intent and (not isinstance(intent[field], list) or any(not isinstance(value, str) for value in intent[field])):
            raise UsageError(f"authoring field {field!r} must be an array of strings")
    if "interruptLast" in intent and not isinstance(intent["interruptLast"], bool):
        raise UsageError("authoring field 'interruptLast' must be a boolean")
    if "time" in intent and intent["time"] is not None and not isinstance(intent["time"], dict):
        raise UsageError("authoring field 'time' must be an object")


def _stable_key(intent: dict[str, Any]) -> str:
    digest = hashlib.sha256(canonical_json(intent).encode()).hexdigest()[:24]
    return f"author-{intent['action'].replace('.', '-')}-{digest}"


def _intent_hash(intent: dict[str, Any]) -> str:
    """Stable identity for a semantic retry, independent of repository HEAD."""

    return hashlib.sha256(canonical_json(intent).encode()).hexdigest()


def _envelope(repository: Repository, intent: dict[str, Any], operations: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "protocol": "wedl-changeset/v1",
        "expectedHead": str(intent.get("expectedHead") or repository.head()),
        "idempotencyKey": str(intent.get("idempotencyKey") or _stable_key(intent)),
        "summary": str(intent.get("summary") or f"Authoring: {intent['action']}"),
        "operations": operations,
    }


def apply_intent(
    repository: Repository,
    intent: dict[str, Any],
    *,
    confirmation_token_value: str | None = None,
    allow_unconfirmed: bool = False,
) -> dict[str, Any]:
    """Apply an intent, recognizing an exact semantic retry across HEAD changes.

    The first request always flows through the ordinary raw changeset expected-
    HEAD and confirmation checks. Once committed, the receipt's semantic hash
    permits only the same intent/key to replay without compiling a second copy.
    """

    if not isinstance(intent, dict) or not isinstance(intent.get("action"), str):
        raise UsageError("authoring request requires an action")
    _validate_intent_types(intent)
    key = str(intent.get("idempotencyKey") or _stable_key(intent))
    receipt_path = repository.root / ".wedl" / "idempotency.json"
    if receipt_path.exists():
        receipts = json.loads(receipt_path.read_text(encoding="utf-8"))
        receipt = receipts.get(key)
        if receipt and receipt.get("authoringIntentHash") is not None:
            if receipt["authoringIntentHash"] != _intent_hash(intent):
                raise ConflictError("idempotency key was used for a different authoring intent")
            if not allow_unconfirmed:
                if not confirmation_token_value:
                    raise ConfirmationRequired("authoring apply requires a preview confirmation token")
                if confirmation_token_value != receipt.get("confirmationToken"):
                    raise ConfirmationMismatch("authoring confirmation token does not match the previewed intent")
            impact = receipt.get("authorImpact")
            if not isinstance(impact, dict):
                # Receipts created before authorImpact was introduced remain
                # valid semantic retries. Do not recompile their intent against
                # a newer head just to reconstruct a summary: that could
                # allocate a different coordinate. Return a deliberately
                # minimal, name-free author-facing acknowledgement instead.
                impact = {"summary": f"Replayed authoring: {intent['action']}.", "items": []}
            return {
                **receipt["result"],
                "authorImpact": impact,
                "idempotentReplay": True,
            }
    from .changeset import apply as apply_changeset
    payload = compile_intent(repository, intent)
    impact = author_impact(repository, intent, payload)
    result = apply_changeset(
        repository, payload,
        confirmation_token_value=confirmation_token_value, allow_unconfirmed=allow_unconfirmed,
        authoring_intent_hash=_intent_hash(intent),
        authoring_impact=impact,
    )
    return {**result, "authorImpact": impact}


def _active_cursor_operations(world: World, point: dict[str, Any]) -> list[dict[str, Any]]:
    """Move the shared cursor and every live front as one atomic change."""

    operations = [{"type": "entity.update", "entity": world.world_record.id, "frontmatterPatch": {"current_time": point}}]
    for scene in world.active_scenes():
        time = deepcopy(scene.frontmatter.get("time") or {})
        time["current"] = point
        operations.append({"type": "entity.update", "entity": scene.id, "frontmatterPatch": {"time": time}})
    return operations


def _scene_participants(character_ids: list[str], point: dict[str, Any]) -> list[dict[str, Any]]:
    return [{"character": character_id, "from": point} for character_id in character_ids]


def _next_order_at_cursor(world: World, intent: dict[str, Any], point: dict[str, Any]) -> dict[str, Any]:
    """Allocate a safe next order whenever the author leaves order implicit."""

    supplied = intent.get("time") or {}
    if "order" not in supplied:
        orders: list[int] = []
        for record in world:
            candidates: list[dict[str, Any]] = []
            if record.kind == "event" and isinstance(record.frontmatter.get("time"), dict):
                candidates.append(record.frontmatter["time"])
            if record.kind == "scene":
                candidates.extend(value for value in (record.frontmatter.get("time") or {}).values() if isinstance(value, dict))
            if record.kind == "conversation":
                candidates.extend(turn.get("at") for turn in record.frontmatter.get("turns") or [] if isinstance(turn, dict) and isinstance(turn.get("at"), dict))
            for candidate in candidates:
                if candidate.get("timeline", world.default_timeline) == point["timeline"] and candidate.get("tick") == point["tick"]:
                    order = candidate.get("order", 0)
                    if isinstance(order, int):
                        orders.append(order)
        cursor = world.current_time
        if cursor and (cursor.timeline, cursor.tick) == (point["timeline"], point["tick"]):
            orders.append(cursor.order)
        next_order = max(orders, default=-1) + 1
        if next_order > ORDER_MAX:
            raise UsageError("cannot allocate a story order after the maximum same-tick order")
        return {**point, "order": next_order}
    return point


def _detach_from_active_scenes(world: World, character_ids: list[str], point: dict[str, Any], *, exclude: str | None = None) -> list[dict[str, Any]]:
    """End selected independent presences before they enter another live front."""

    if point["order"] <= -2**31:
        raise UsageError("cannot reconcile scene presence at the minimum same-tick order; choose a later order")
    before = {**point, "order": point["order"] - 1}
    operations: list[dict[str, Any]] = []
    for scene in world.active_scenes():
        if scene.id == exclude:
            continue
        changed = False
        participants = deepcopy(scene.frontmatter.get("participants") or [])
        at_before = StoryTime.from_value(before, world.default_timeline)
        for participant in participants:
            character_id = str(participant.get("character")) if isinstance(participant, dict) else ""
            if character_id not in character_ids or participant_at(scene, character_id, at_before, world.default_timeline) is None:
                continue
            existing_to = participant.get("to")
            if existing_to is not None:
                try:
                    end = StoryTime.from_value(existing_to, world.default_timeline)
                except (TypeError, ValueError):
                    continue
                if (end.timeline, end.tick, end.order) <= (at_before.timeline, at_before.tick, at_before.order):
                    continue
            participant["to"] = before
            changed = True
        if changed:
            operations.append({"type": "entity.update", "entity": scene.id, "frontmatterPatch": {"participants": participants}})
    return operations


def _state_immediately_before(point: dict[str, Any]) -> StoryTime:
    """Return the state horizon directly before an authored effect point."""

    order = point["order"]
    return StoryTime(point["timeline"], point["tick"], order - 1 if order > ORDER_MIN else order)


def _transfer_held_objects(world: World, character_ids: list[str], point: dict[str, Any], *, exclude: str | None = None) -> tuple[list[dict[str, Any]], list[str]]:
    """Keep scene object continuity when an independently moving character carries gear."""

    before = _state_immediately_before(point)
    operations: list[dict[str, Any]] = []
    moved: list[str] = []
    for scene in world.active_scenes():
        if scene.id == exclude:
            continue
        present_characters = {
            character_id for character_id in character_ids
            if participant_at(scene, character_id, before, world.default_timeline) is not None
        }
        if not present_characters:
            continue
        object_ids = [str(value) for value in scene.frontmatter.get("objects") or []]
        states = {object_id: resolve_state(world, object_id, before)[0] for object_id in object_ids}
        transferred = {
            object_id for object_id, state in states.items()
            if isinstance(state.get("holder") if isinstance(state, dict) else None, dict)
            and str(state["holder"].get("entity")) in present_characters
        }
        # A listed object inside a transferred container travels with that
        # container. This remains object-level continuity, never a group
        # inventory: objects not listed in the source scene are untouched.
        changed_membership = True
        while changed_membership:
            changed_membership = False
            for object_id, state in states.items():
                container = state.get("container") if isinstance(state, dict) else None
                if isinstance(container, dict) and str(container.get("entity")) in transferred and object_id not in transferred:
                    transferred.add(object_id); changed_membership = True
        kept = [object_id for object_id in object_ids if object_id not in transferred]
        changed = bool(transferred)
        moved.extend(sorted(transferred))
        if changed:
            operations.append({"type": "entity.update", "entity": scene.id, "frontmatterPatch": {"objects": kept}})
    return operations, moved


def _stationary_scene_objects(world: World, scene: Any, character_ids: list[str], at: StoryTime) -> list[str]:
    """Return listed props at the old location that will not travel with cast."""

    states = {str(value): resolve_state(world, str(value), at)[0] for value in scene.frontmatter.get("objects") or []}

    def carried(object_id: str, visiting: set[str] | None = None) -> bool:
        visiting = set() if visiting is None else visiting
        if object_id in visiting:
            return False
        visiting.add(object_id)
        state = states.get(object_id)
        if state is None:
            state = resolve_state(world, object_id, at)[0]
        holder = state.get("holder") if isinstance(state, dict) else None
        if isinstance(holder, dict) and str(holder.get("entity")) in character_ids:
            return True
        container = state.get("container") if isinstance(state, dict) else None
        return isinstance(container, dict) and isinstance(container.get("entity"), str) and carried(str(container["entity"]), visiting)

    location_id = str(scene.frontmatter.get("location") or "")
    blocked = []
    for object_id, state in states.items():
        location = state.get("location") if isinstance(state, dict) else None
        if isinstance(location, dict) and str(location.get("entity")) == location_id and not carried(object_id):
            blocked.append(world.get(object_id).title)
    return sorted(blocked)


def _scene_characters_at(world: World, scene: Any, at: StoryTime) -> list[str]:
    """Return only characters whose authored scene interval includes ``at``."""

    return [
        str(item.get("character")) for item in scene.frontmatter.get("participants") or []
        if isinstance(item, dict) and item.get("character")
        and participant_at(scene, str(item["character"]), at, world.default_timeline) is not None
    ]


def _active_scene_cursor(world: World, scene: Any, action: str) -> StoryTime:
    """Return the live cursor a scene action must not move behind."""

    if scene.status != "active":
        raise UsageError(f"{action} requires an active scene; closed scenes are historical and cannot be changed")
    cursor = world.current_time
    if cursor is not None:
        return cursor
    try:
        return StoryTime.from_value((scene.frontmatter.get("time") or {}).get("current"), world.default_timeline)
    except (TypeError, ValueError) as exc:
        raise UsageError("active scene has no valid current time") from exc


def _require_not_before_world_cursor(world: World, point: dict[str, Any], action: str) -> StoryTime:
    """Protect shared-cursor updates, except for explicit current-time set."""

    requested = StoryTime.from_value(point, world.default_timeline)
    cursor = world.current_time
    if cursor is not None and (requested.timeline != cursor.timeline or (requested.tick, requested.order) < (cursor.tick, cursor.order)):
        raise UsageError(f"{action} cannot move the shared current cursor backward or onto another timeline; use author current-time set only for an explicit cursor change")
    return requested


def _require_not_before_active_cursor(world: World, scene: Any, point: dict[str, Any], action: str) -> StoryTime:
    cursor = _active_scene_cursor(world, scene, action)
    requested = StoryTime.from_value(point, world.default_timeline)
    if requested.timeline != cursor.timeline or (requested.tick, requested.order) < (cursor.tick, cursor.order):
        raise UsageError(f"{action} cannot move an active scene before the shared current cursor or onto another timeline; use author current-time set only for an explicit cursor change")
    return requested


def _validate_scene_move_reconciliation(world: World, scene: Any, location_id: str, character_ids: list[str], point: dict[str, Any]) -> None:
    """Keep scene presence intervals and scene location semantically coherent."""

    _require_not_before_active_cursor(world, scene, point, "character.move --scene")
    if str(scene.frontmatter.get("location") or "") != location_id:
        raise UsageError("character.move --scene destination must match the active scene location; use author scene advance --location to move the scene")
    requested = StoryTime.from_value(point, world.default_timeline)
    listed = {
        str(participant.get("character"))
        for participant in scene.frontmatter.get("participants") or []
        if isinstance(participant, dict) and participant.get("character")
    }
    for character_id in character_ids:
        if character_id in listed and participant_at(scene, character_id, requested, world.default_timeline) is None:
            raise UsageError("character.move --scene cannot re-enter a previously ended scene presence; create a new scene or use a raw changeset to author a distinct interval")


def _effect(seed: str, character_id: str, location_id: str) -> dict[str, Any]:
    return {
        "id": id_from_seed("effect", f"{seed}:{character_id}:location"),
        "target": character_id,
        "key": "location",
        "operation": "set",
        "value": {"entity": location_id},
    }


def _movement_event(intent: dict[str, Any], point: dict[str, Any], location_id: str, character_ids: list[str]) -> dict[str, Any]:
    title = str(intent.get("title") or f"Characters move to {intent['location']}")
    seed = canonical_json({"action": intent["action"], "time": point, "characters": character_ids, "location": location_id})
    return {
        "type": "event.create",
        "temporaryId": "$author.move.event",
        "title": title,
        "time": point,
        "location": location_id,
        "participants": [{"character": character_id, "role": "moving character"} for character_id in character_ids],
        "effects": [_effect(seed, character_id, location_id) for character_id in character_ids],
        "bodyMarkdown": f"# {title}\n",
    }


def _scene_patch(scene: Any, *, point: dict[str, Any], status: str | None = None, location_id: str | None = None, add_characters: list[str] | None = None, add_objects: list[str] | None = None) -> dict[str, Any]:
    time = deepcopy(scene.frontmatter.get("time") or {})
    time["current"] = point
    if status == "closed":
        time["end"] = point
    patch: dict[str, Any] = {"time": time}
    if status:
        patch["status"] = status
    if location_id:
        patch["location"] = location_id
    if add_characters:
        existing = {str(item.get("character")) for item in scene.frontmatter.get("participants") or [] if isinstance(item, dict)}
        patch["participants"] = [*deepcopy(scene.frontmatter.get("participants") or []), *_scene_participants([value for value in add_characters if value not in existing], point)]
    if add_objects:
        patch["objects"] = [*deepcopy(scene.frontmatter.get("objects") or []), *[value for value in add_objects if value not in (scene.frontmatter.get("objects") or [])]]
    return {"type": "entity.update", "entity": scene.id, "frontmatterPatch": patch}


def _conversation_point(world: World, conversation: Any, intent: dict[str, Any]) -> dict[str, Any]:
    linked = world.maybe_get(str(conversation.frontmatter.get("scene") or ""))
    fallback = None
    if linked and linked.kind == "scene":
        raw = (linked.frontmatter.get("time") or {}).get("current")
        if raw:
            fallback = StoryTime.from_value(raw, world.default_timeline)
    if fallback is None:
        raw = (conversation.frontmatter.get("time") or {}).get("end") or (conversation.frontmatter.get("time") or {}).get("start")
        fallback = StoryTime.from_value(raw, world.default_timeline)
    supplied = intent.get("time")
    point = _time(world, supplied, fallback=fallback)
    if supplied is None or "order" not in supplied:
        # Never collide with a prior beat at the inherited cursor.
        same_tick = [
            int((turn.get("at") or {}).get("order", 0))
            for turn in conversation.frontmatter.get("turns") or []
            if isinstance(turn, dict)
            and (turn.get("at") or {}).get("timeline", world.default_timeline) == point["timeline"]
            and (turn.get("at") or {}).get("tick") == point["tick"]
        ]
        if same_tick:
            if max(same_tick) >= ORDER_MAX:
                raise UsageError("cannot allocate a conversation order after the maximum same-tick order")
            point["order"] = max(same_tick) + 1
    return _next_order_at_cursor(world, intent, point)


def _scene_for_conversation(world: World, reference: str | None) -> Any:
    """Resolve a conversation scene without inventing a collective actor.

    A name is optional only when precisely one active scene supplies the
    natural authoring context.  Concurrent fronts deliberately require the
    author to name the scene.
    """

    if reference and reference.strip():
        scene = world.find(reference.strip(), "scene")
    else:
        active = world.active_scenes()
        if len(active) != 1:
            raise UsageError("conversation.create requires --scene when there is not exactly one active scene")
        scene = active[0]
    if scene.status != "active":
        raise UsageError("conversation.create requires an active scene")
    return scene


def _name(world: World, entity_id: str | None) -> str | None:
    record = world.maybe_get(str(entity_id or ""))
    return record.title if record else None


def _scene_impact_items(world: World, point: dict[str, Any]) -> list[dict[str, Any]]:
    """Describe a shared horizon update without surfacing source identifiers."""

    at = StoryTime.from_value(point, world.default_timeline)
    items: list[dict[str, Any]] = []
    for scene in world.active_scenes():
        characters = [
            world.get(character_id).title
            for character_id in _scene_characters_at(world, scene, at)
        ]
        items.append({
            "kind": "author-horizon-advanced",
            "scene": scene.title,
            "location": _name(world, str(scene.frontmatter.get("location") or "")),
            "characters": characters,
            "at": deepcopy(point),
        })
    return items


def _append_advances_horizon(world: World, conversation: Any, intent: dict[str, Any], point: dict[str, Any]) -> bool:
    """Whether an implicit appended order becomes the shared author horizon.

    Legacy single-front worlds may not yet store ``world.current_time``. Their
    conversation's active scene still supplies the inherited cursor, and the
    resulting changeset materializes the shared world cursor for them.
    """

    supplied = intent.get("time") or {}
    if isinstance(supplied, dict) and "order" in supplied:
        return False
    cursor = world.current_time
    if cursor is None:
        linked = world.maybe_get(str(conversation.frontmatter.get("scene") or ""))
        if linked and linked.kind == "scene" and linked.status == "active":
            cursor = _active_scene_cursor(world, linked, "conversation.append")
    return cursor is not None and point["timeline"] == cursor.timeline and (point["tick"], point["order"]) > (cursor.tick, cursor.order)


def author_impact(repository: Repository, intent: dict[str, Any], payload: dict[str, Any] | None = None) -> dict[str, Any]:
    """Return the small, name-only result intended for human authoring flows.

    Raw changeset fields remain available alongside this helper.  This summary
    intentionally contains no source IDs, temporary IDs, paths, or revisions.
    """

    world = repository.load_world(repository.head())
    action = str(intent.get("action") or "")
    payload = payload or compile_intent(repository, intent)
    operations = payload.get("operations") or []
    if action == "chronology.replace":
        catalog = sum(1 for operation in operations if operation.get("entity") == world.world_record.id)
        records = [world.get(str(operation["entity"])).title for operation in operations if operation.get("entity") != world.world_record.id]
        items: list[dict[str, Any]] = []
        if catalog: items.append({"kind": "chronology-catalog-replaced"})
        items.extend({"kind": "chronology-annotations-replaced", "record": title} for title in records)
        return {"summary": "Replaced chronology catalogue and annotations." if catalog and records else "Replaced chronology catalogue." if catalog else "Replaced chronology annotations.", "items": items}
    if action == "hypothesis.create":
        operation = next(operation for operation in operations if operation.get("type") == "entity.create")
        value = operation.get("value") or {}; frontmatter = value.get("frontmatter") or {}
        return {"summary": f"Recorded non-canonical possibility {frontmatter.get('title') or ''}.", "items": [{"kind": "hypothesis-recorded", "hypothesis": frontmatter.get("title"), "nonCanonical": True}]}
    if action in {"hypothesis.adopt", "hypothesis.reject"}:
        reference = world.find(str(intent.get("hypothesis") or ""), "hypothesis")
        status = "adopted" if action.endswith("adopt") else "rejected"
        message = f"Marked possibility {reference.title} {status}."
        if status == "adopted": message += " Canon is unchanged; the referenced settled records already exist."
        return {"summary": message, "items": [{"kind": "hypothesis-status", "hypothesis": reference.title, "status": status, "nonCanonical": True}]}
    if action == "conversation.create":
        operation = next(operation for operation in operations if operation.get("type") == "conversation.create")
        value = operation["value"]
        point = deepcopy((value.get("time") or {}).get("start") or {})
        characters = [
            _name(world, str(participant.get("character") or ""))
            for participant in value.get("participants") or [] if isinstance(participant, dict)
        ]
        item = {
            "kind": "conversation-created", "conversation": str(value.get("title") or ""),
            "scene": _name(world, str(value.get("scene") or "")),
            "location": _name(world, str(value.get("location") or "")),
            "characters": [name for name in characters if name], "at": point,
        }
        return {"summary": f"Created conversation {item['conversation']}.", "items": [item]}
    if action == "conversation.append":
        operation = next(operation for operation in operations if operation.get("type") == "conversation.turn.append")
        conversation = world.get(str(operation.get("conversation") or ""))
        turn = operation["turn"]
        actor_ids = [turn["speaker"]] if turn.get("kind", "speech") == "speech" else list(turn.get("actors") or [])
        item = {
            "kind": "conversation-turn-appended", "conversation": conversation.title,
            "scene": _name(world, str(conversation.frontmatter.get("scene") or "")),
            "location": _name(world, str(conversation.frontmatter.get("location") or "")),
            "characters": [name for name in (_name(world, str(value)) for value in actor_ids) if name],
            "at": deepcopy(turn["at"]),
        }
        # Only automatic order allocation is allowed to advance the shared
        # horizon. Explicit author coordinates retain their exact semantics.
        advanced = _append_advances_horizon(world, conversation, intent, item["at"])
        items = [item]
        if advanced:
            items.extend(_scene_impact_items(world, item["at"]))
        summary = f"Appended a {turn.get('kind', 'speech')} beat to {conversation.title}."
        if advanced:
            summary += " Advanced the author horizon."
        return {"summary": summary, "items": items}
    return {"summary": str(intent.get("summary") or f"Authoring: {action}"), "items": []}


def preview_intent(repository: Repository, intent: dict[str, Any]) -> dict[str, Any]:
    """Preview an authoring request while retaining the raw changeset payload."""

    from .changeset import preview as preview_changeset

    payload = compile_intent(repository, intent)
    plan = preview_changeset(repository, payload)
    plan.pop("_changes", None)
    return {
        "protocol": "wedl-author-preview/v1", "intent": intent, "changeset": payload,
        "preview": plan, "authorImpact": author_impact(repository, intent, payload),
    }


_CHRONOLOGY_INT_FIELDS = frozenset({"year", "month", "day", "number", "days", "period", "residue", "target_month", "delta_days", "axis_day", "tick", "order", "value", "display_year", "machine_year"})
_CHRONOLOGY_CAMEL = {"calendarId": "calendar_id", "eraId": "era_id", "displayYear": "display_year", "machineYear": "machine_year", "displayYearZero": "display_year_zero", "axisDay": "axis_day", "storyTime": "story_time", "displayEpoch": "display_epoch", "displayValue": "display_value", "targetMonth": "target_month", "deltaDays": "delta_days", "intercalaryMonth": "intercalary_month", "beforeId": "before_id", "afterId": "after_id", "temporaryId": "temporaryId"}
_CHRONOLOGY_EXTENSION = re.compile(r"^x-[A-Za-z0-9_.-]+$")
_MAX_CONFLICT_DEPTH = 64


def _chronology_extensions(value: Any, allowed: set[str], required: set[str], where: str) -> tuple[dict[str, Any], dict[str, Any]]:
    """Split a closed public object from its explicitly permitted ``x-*`` data."""

    if not isinstance(value, dict) or not required.issubset(value):
        raise UsageError(f"{where} is malformed")
    unknown = set(value).difference(allowed)
    if any(not isinstance(key, str) or not _CHRONOLOGY_EXTENSION.fullmatch(key) for key in unknown):
        raise UsageError(f"{where} contains unsupported fields")
    return ({key: item for key, item in value.items() if key in allowed}, {key: deepcopy(item) for key, item in value.items() if key in unknown})


def _chronology_tag_extensions(value: Any, where: str) -> dict[str, Any]:
    """Copy source-tag extensions without interpreting their opaque JSON."""

    if not isinstance(value, dict) or any(
        not isinstance(key, str) or not _CHRONOLOGY_EXTENSION.fullmatch(key)
        for key in value
    ):
        raise UsageError(f"{where}.tagExtensions must contain only x-* members")
    return deepcopy(value)


def _chronology_value_parts(
    value: Any, allowed: set[str], required: set[str], where: str,
) -> tuple[dict[str, Any], dict[str, Any], dict[str, Any]]:
    """Split public value core, payload extensions, and tag extensions.

    Direct ``x-*`` members belong to the source value payload.  The optional
    ``tagExtensions`` object reversibly represents only ``x-*`` members beside
    the source value's discriminator tag, including deliberately colliding
    names.  Neither channel is traversed or coerced.
    """

    core, payload_extensions = _chronology_extensions(
        value, allowed | {"tagExtensions"}, required, where,
    )
    tag_extensions = (
        _chronology_tag_extensions(core.pop("tagExtensions"), where)
        if "tagExtensions" in core
        else {}
    )
    return core, payload_extensions, tag_extensions


def _nonblank_chronology_string(value: Any, where: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise UsageError(f"{where} must be a nonblank string")
    return value


def _chronology_source(value: Any, key: str | None = None) -> Any:
    """Transcode only public chronology camelCase/decimal leaves to source."""
    if isinstance(value, list): return [_chronology_source(item) for item in value]
    if not isinstance(value, dict):
        if key in _CHRONOLOGY_INT_FIELDS:
            if not isinstance(value, str) or value == "-0" or not __import__("re").fullmatch(r"-?(0|[1-9][0-9]*)", value):
                raise UsageError(f"chronology field {key} must be a canonical decimal string")
            return int(value)
        return value
    result: dict[str, Any] = {}
    for raw_key, raw_value in value.items():
        if isinstance(raw_key, str) and raw_key.startswith("x-"):
            # Extensions are opaque source-compatible data.  In particular, an
            # extension with a member named ``year`` is not a chronology
            # coordinate and must not be coerced during public transcoding.
            result[raw_key] = deepcopy(raw_value)
            continue
        source_key = _CHRONOLOGY_CAMEL.get(raw_key, raw_key)
        result[source_key] = _chronology_source(raw_value, source_key)
    return result


def _decode_authoring_chronology_value(value: Any, *, _depth: int = 0) -> dict[str, Any]:
    """Decode the closed public authoring value union recursively.

    This is intentionally separate from the public read codec: relative and
    duration values are source-valid annotation values but are not read-query
    operands.  Recursing here also leaves scoped ``beforeId``/``afterId``
    leaves intact for the existing narrowly scoped changeset substitution.
    """

    if not isinstance(value, dict) or not isinstance(value.get("kind"), str):
        raise UsageError("chronology value is malformed")
    kind = value["kind"]
    if kind == "relative":
        core, extensions, tag_extensions = _chronology_value_parts(value, {"kind", "relation", "beforeId", "afterId"}, {"kind", "relation"}, "relative chronology value")
        if not isinstance(core["relation"], str) or not any(name in core and isinstance(core[name], str) and core[name].strip() for name in ("beforeId", "afterId")):
            raise UsageError("relative chronology value is malformed")
        result = {"relation": core["relation"]}
        if "beforeId" in core: result["before_id"] = core["beforeId"]
        if "afterId" in core: result["after_id"] = core["afterId"]
        return {"relative": {**result, **extensions}, **tag_extensions}
    if kind == "duration":
        core, extensions, tag_extensions = _chronology_value_parts(value, {"kind", "unit", "value"}, {"kind", "unit", "value"}, "duration chronology value")
        if core.get("unit") not in {"year", "month", "day"}:
            raise UsageError("duration chronology value is malformed")
        from .chronology_api import _decimal
        return {"duration": {"unit": core["unit"], "value": _decimal(core["value"], "duration.value"), **extensions}, **tag_extensions}
    if kind == "conflict":
        core, extensions, tag_extensions = _chronology_value_parts(value, {"kind", "claims"}, {"kind", "claims"}, "conflict chronology value")
        claims = core["claims"]
        if not isinstance(claims, list) or not 2 <= len(claims) <= 64:
            raise UsageError("conflict chronology value is malformed")
        if _depth >= _MAX_CONFLICT_DEPTH:
            raise UsageError(f"conflict nesting exceeds {_MAX_CONFLICT_DEPTH}")
        return {"conflict": {"claims": [_decode_authoring_chronology_value(claim, _depth=_depth + 1) for claim in claims], **extensions}, **tag_extensions}

    def endpoint(item: Any, calendar_id: str, where: str) -> tuple[Any, dict[str, Any]]:
        if item is None:
            return None, {}
        core, extensions = _chronology_extensions(item, {"calendarId", "year", "month", "day"}, {"year"}, where)
        if "calendarId" in core and _nonblank_chronology_string(core["calendarId"], f"{where}.calendarId") != calendar_id:
            raise UsageError(f"{where}.calendarId must match the range calendar")
        return core, extensions

    extensions: dict[str, Any]
    tag_extensions: dict[str, Any]
    endpoint_extensions: tuple[dict[str, Any], dict[str, Any]] | None = None
    bounds_extensions: dict[str, Any] | None = None
    if kind in {"civil", "era"}:
        identifier = "calendarId" if kind == "civil" else "eraId"
        core, extensions, tag_extensions = _chronology_value_parts(value, {"kind", identifier, "year", "month", "day"}, {"kind", identifier, "year"}, f"{kind} chronology value")
        _nonblank_chronology_string(core[identifier], identifier)
    elif kind == "range":
        core, extensions, tag_extensions = _chronology_value_parts(value, {"kind", "calendarId", "lower", "upper"}, {"kind", "calendarId", "lower", "upper"}, "range chronology value")
        calendar_id = _nonblank_chronology_string(core["calendarId"], "range.calendarId")
        lower, lower_extensions = endpoint(core["lower"], calendar_id, "range.lower")
        upper, upper_extensions = endpoint(core["upper"], calendar_id, "range.upper")
        core = {**core, "lower": lower, "upper": upper}
        endpoint_extensions = (lower_extensions, upper_extensions)
    elif kind == "approximate":
        core, extensions, tag_extensions = _chronology_value_parts(value, {"kind", "displayValue", "bounds"}, {"kind", "displayValue", "bounds"}, "approximate chronology value")
        if not isinstance(core["displayValue"], str):
            raise UsageError("approximate.displayValue must be a string")
        bounds, bounds_extensions = _chronology_extensions(core["bounds"], {"calendarId", "lower", "upper"}, {"lower", "upper"}, "approximate.bounds")
        if bounds["lower"] is None and bounds["upper"] is None:
            if "calendarId" in bounds:
                raise UsageError("qualitative approximate.bounds must omit calendarId")
            # A display-only approximation has no source basis.  Preserve its
            # calendar-free bounds verbatim instead of creating an internal
            # CivilRange with an invented identifier.
            return {
                "approx": {
                    "display_value": core["displayValue"],
                    "bounds": {"lower": None, "upper": None, **bounds_extensions},
                    **extensions,
                },
                **tag_extensions,
            }
        if "calendarId" not in bounds:
            raise UsageError("approximate.bounds.calendarId is required when a bound is present")
        calendar_id = _nonblank_chronology_string(bounds["calendarId"], "approximate.bounds.calendarId")
        lower, lower_extensions = endpoint(bounds["lower"], calendar_id, "approximate.bounds.lower")
        upper, upper_extensions = endpoint(bounds["upper"], calendar_id, "approximate.bounds.upper")
        core = {**core, "bounds": {**bounds, "lower": lower, "upper": upper}}
        endpoint_extensions = (lower_extensions, upper_extensions)
    else:
        raise UsageError("unsupported chronology date kind")

    from .chronology_api import decode_date_value
    decoded = decode_date_value(core)
    def date(item: Any) -> dict[str, Any]:
        from .chronology import ApproximateDate, CivilDate, CivilRange, ConflictingDates, EraDate
        if isinstance(item, CivilDate):
            result = {"calendar_id": item.calendar_id, "year": item.year}
            if item.month is not None: result["month"] = item.month
            if item.day is not None: result["day"] = item.day
            return {"civil": result}
        if isinstance(item, EraDate):
            result = {"era_id": item.era_id, "year": item.year}
            if item.month is not None: result["month"] = item.month
            if item.day is not None: result["day"] = item.day
            return {"era": result}
        if isinstance(item, CivilRange):
            endpoint = lambda bound: None if bound is None else {"calendar_id": bound.calendar_id, "year": bound.year, **({"month": bound.month} if bound.month is not None else {}), **({"day": bound.day} if bound.day is not None else {})}
            return {"range": {"calendar_id": item.calendar_id, "lower": endpoint(item.lower), "upper": endpoint(item.upper)}}
        if isinstance(item, ApproximateDate):
            bounds = date(item.bounds)["range"]
            # Source ``approx.bounds`` deliberately carries only endpoints;
            # the range-level calendar is represented by each non-null civil
            # endpoint, not repeated at this container level.
            return {"approx": {"display_value": item.display_value, "bounds": {"lower": bounds["lower"], "upper": bounds["upper"]}}}
        raise UsageError("unsupported chronology value")
    result = date(decoded)
    payload = next(iter(result.values()))
    payload.update(extensions)
    if endpoint_extensions is not None:
        lower_extensions, upper_extensions = endpoint_extensions
        bounds = payload if kind == "range" else payload["bounds"]
        if bounds["lower"] is not None: bounds["lower"].update(lower_extensions)
        if bounds["upper"] is not None: bounds["upper"].update(upper_extensions)
    if bounds_extensions is not None:
        payload["bounds"].update(bounds_extensions)
    result.update(tag_extensions)
    return result


def _chronology_date(value: Any) -> dict[str, Any]:
    """Backward-compatible internal name for the recursive authoring decoder."""

    return _decode_authoring_chronology_value(value)


def _identifier(value: dict[str, Any], prefix: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value).difference({"id", "temporaryId"}) or len(value) != 1:
        raise UsageError(f"chronology {prefix} requires exactly id or temporaryId")
    field, identifier = next(iter(value.items()))
    _nonblank_chronology_string(identifier, f"chronology {prefix} identifier")
    if field == "temporaryId" and not identifier.startswith(f"${prefix}."):
        raise UsageError(f"chronology temporary ID must use ${prefix}. scope")
    return {field: identifier}


def _chronology_catalog(value: Any) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != {"calendars", "eras", "anchors"}:
        raise UsageError("chronology catalog must fully replace calendars, eras, and anchors")
    result: dict[str, Any] = {}
    fields = {
        "calendars": {"id", "temporaryId", "label", "months", "rule", "epoch"},
        "eras": {"id", "temporaryId", "calendarId", "label", "aliases", "displayYearZero", "displayEpoch", "bounds", "provenance"},
        "anchors": {"id", "temporaryId", "axisDay", "storyTime", "provenance"},
    }
    for collection, prefix in (("calendars", "calendar"), ("eras", "era"), ("anchors", "chronology")):
        values = value[collection]
        if not isinstance(values, list) or len(values) > 500: raise UsageError(f"chronology catalog {collection} must be an array of at most 500")
        converted = []
        for item in values:
            if not isinstance(item, dict): raise UsageError(f"chronology catalog {collection} entries must be objects")
            core, extensions = _chronology_extensions(item, fields[collection], {"id"} if "id" in item else {"temporaryId"}, f"chronology catalog {collection} entry")
            identifier = _identifier({key: core[key] for key in ("id", "temporaryId") if key in core}, prefix)
            copy = _chronology_source({key: data for key, data in core.items() if key not in {"id", "temporaryId"}})
            converted.append({**identifier, **copy, **extensions})
        # Stable permanent IDs have source-required ordering; temporary IDs use
        # their literal scoped token until changeset allocation canonicalizes.
        result[collection] = sorted(converted, key=lambda item: str(item.get("id") or item.get("temporaryId")))
    return result


def _chronology_annotation(value: Any) -> dict[str, Any]:
    core, extensions = _chronology_extensions(value, {"id", "temporaryId", "role", "display", "provenance", "value"}, {"provenance", "value"}, "chronology annotation")
    result = _identifier({key: core[key] for key in ("id", "temporaryId") if key in core}, "chronology")
    if not isinstance(core["provenance"], list) or any(not isinstance(item, str) for item in core["provenance"]): raise UsageError("chronology annotation provenance must be strings")
    if any(key in core and not isinstance(core[key], str) for key in ("role", "display")):
        raise UsageError("chronology annotation role and display must be strings")
    result.update({key: deepcopy(core[key]) for key in ("role", "display", "provenance") if key in core})
    result["value"] = _chronology_date(core["value"])
    result.update(extensions)
    return result


def compile_intent(repository: Repository, intent: dict[str, Any]) -> dict[str, Any]:
    """Resolve a small author intent into a canonical raw changeset.

    ``intent`` is also the HTTP request body.  It is deliberately compact and
    contains titles/aliases/slugs, never required canonical IDs.
    """

    if not isinstance(intent, dict) or not isinstance(intent.get("action"), str):
        raise UsageError("authoring request requires an action")
    _validate_intent_types(intent)
    action = intent["action"]
    world = repository.load_world(repository.head())

    if action == "chronology.replace":
        if set(intent).difference({"action", "expectedHead", "change", "summary", "idempotencyKey"}):
            raise UsageError("chronology.replace contains unsupported fields")
        if any(field in intent and intent[field] is None for field in ("expectedHead", "change", "summary", "idempotencyKey")):
            raise UsageError("chronology.replace does not accept null fields")
        if any(field in intent and not isinstance(intent[field], str) for field in ("summary", "idempotencyKey")):
            raise UsageError("chronology.replace summary and idempotencyKey must be strings")
        if world.schema != CHRONOLOGY_SOURCE_SCHEMA:
            raise ChronologyUpgradeRequired("chronology authoring requires wedl/v0.6", details={"code": "WDL-MIG-V06-004", "sourceSchema": world.schema, "upgradeAvailable": False})
        expected = intent.get("expectedHead")
        if not isinstance(expected, str) or not __import__("re").fullmatch(r"[0-9a-f]{40}", expected):
            raise UsageError("chronology.replace requires a 40-character expectedHead")
        change = intent.get("change")
        if not isinstance(change, dict) or set(change).difference({"catalog", "records"}) or not change or not any(key in change for key in ("catalog", "records")):
            raise UsageError("chronology.replace requires a closed change with catalog and/or records")
        operations: list[dict[str, Any]] = []
        if "catalog" in change:
            catalog = _chronology_catalog(change["catalog"])
            operations.append({"type": "entity.update", "entity": world.world_record.id, "frontmatterPatch": {"chronology": catalog}})
        if "records" in change:
            if not isinstance(change["records"], list) or not change["records"]:
                raise UsageError("chronology.replace records must be a non-empty array")
            resolved: set[str] = set()
            for item in change["records"]:
                if not isinstance(item, dict) or set(item) != {"record", "annotations"} or not isinstance(item["record"], str) or not item["record"].strip() or not isinstance(item["annotations"], list):
                    raise UsageError("each chronology record replacement requires record and annotations")
                record = world.find(item["record"])
                if record.kind == "world" or record.id in resolved:
                    raise UsageError("chronology record replacements must name distinct non-world records")
                resolved.add(record.id)
                operations.append({"type": "entity.update", "entity": record.id, "frontmatterPatch": {"chronology": [_chronology_annotation(value) for value in item["annotations"]]}})
        return _envelope(repository, intent, operations)

    if action == "current-time.set":
        point = _time(world, intent.get("time"))
        return _envelope(repository, intent, _active_cursor_operations(world, point))

    if action == "hypothesis.create":
        title = str(intent.get("title") or "").strip()
        statement = str(intent.get("statement") or "").strip()
        context = str(intent.get("context") or "").strip()
        if not title or not statement or not context:
            raise UsageError("hypothesis.create requires title, statement, and context")
        subject_values = intent.get("subjects") or []
        alternatives = intent.get("alternatives") or []
        if not subject_values or not alternatives:
            raise UsageError("hypothesis.create requires one or more --subject and --alternative values")
        subjects = [_reference(world, value) for value in subject_values]
        if len(set(subjects)) != len(subjects): raise UsageError("hypothesis subjects must be distinct")
        if not all(str(value).strip() for value in alternatives): raise UsageError("hypothesis alternatives must be non-empty")
        placement: dict[str, Any] = {"context": context}
        if intent.get("timeline"):
            if str(intent["timeline"]) not in world.timeline_ids: raise UsageError("hypothesis timeline must be declared")
            placement["timeline"] = str(intent["timeline"])
        for field, kind in (("scene", "scene"), ("event", "event"), ("location", "location")):
            if intent.get(field): placement[field] = _reference(world, str(intent[field]), kind)
        value = {"frontmatter": {"kind": "hypothesis", "title": title, "domain": "author.possibilities", "status": "open", "statement": statement, "subjects": subjects, "alternatives": list(alternatives), "context": context, "placement": placement}, "bodyMarkdown": f"# {title}\n\n{statement}\n"}
        return _envelope(repository, intent, [{"type": "entity.create", "temporaryId": "$author.hypothesis", "value": value}])

    if action in {"hypothesis.adopt", "hypothesis.reject"}:
        hypothesis = world.find(str(intent.get("hypothesis") or ""), "hypothesis")
        patch: dict[str, Any] = {"status": "adopted" if action.endswith("adopt") else "rejected"}
        if action.endswith("adopt"):
            values = intent.get("canonicalRecords") or []
            if not values: raise UsageError("hypothesis.adopt requires one or more already settled records")
            records = [_reference(world, value) for value in values]
            if len(set(records)) != len(records): raise UsageError("supporting records must be distinct")
            for record_id in records:
                record = world.get(record_id)
                if not is_adoptable_canonical_record(record): raise UsageError("hypothesis.adopt can reference only already settled canonical records")
            patch["resolution"] = {"canonical_entities": records, **({"note": str(intent["note"])} if intent.get("note") else {})}
        elif intent.get("note"):
            patch["resolution"] = {"note": str(intent["note"])}
        else:
            raise UsageError("hypothesis.reject requires a non-empty note")
        return _envelope(repository, intent, [{"type": "entity.update", "entity": hypothesis.id, "frontmatterPatch": patch}])

    if action in {"scene.create", "scene.advance", "scene.close"}:
        point = _time(world, intent.get("time"), fallback=world.current_time)
        location = intent.get("location")
        location_id = _reference(world, str(location), "location") if location else None
        if action == "scene.create":
            if not location_id:
                raise UsageError("scene.create requires a location")
            characters = intent.get("characters") or []
            if not isinstance(characters, list) or not characters:
                raise UsageError("scene.create requires one or more characters")
            character_ids = [_reference(world, str(value), "character") for value in characters]
            if len(set(character_ids)) != len(character_ids):
                raise UsageError("scene.create characters must be distinct")
            title = str(intent.get("title") or "").strip()
            if not title:
                raise UsageError("scene.create requires a title")
            point = _next_order_at_cursor(world, intent, point)
            _require_not_before_world_cursor(world, point, "scene.create")
            operations = _detach_from_active_scenes(world, character_ids, point)
            object_operations, moved_objects = _transfer_held_objects(world, character_ids, point)
            operations.extend(object_operations)
            operations.extend(_active_cursor_operations(world, point))
            operations.append({
                "type": "entity.create", "temporaryId": "$author.scene", "value": {"frontmatter": {
                    "kind": "scene", "title": title, "domain": "scenes.authoring", "status": "active", "location": location_id,
                    "time": {"start": point, "current": point, "end": None},
                    "participants": _scene_participants(character_ids, point), "objects": moved_objects, "environments": [],
                    "story_points": [], "observations": [], "conversations": [],
                }, "bodyMarkdown": f"# {title}\n"},
            })
            # Individual effects preserve independent characters even when a
            # single author command moves several of them together.
            operations.append(_movement_event(intent, point, location_id, character_ids))
            return _envelope(repository, intent, operations)
        scene_reference = str(intent.get("scene") or "").strip()
        if scene_reference:
            scene = world.find(scene_reference, "scene")
        else:
            if action == "scene.close":
                raise UsageError("scene.close requires an explicit nonblank scene reference")
            active = world.active_scenes()
            if len(active) != 1:
                raise UsageError("scene.advance requires a scene when there is not exactly one active scene")
            scene = active[0]
        if action == "scene.advance":
            if location_id:
                point = _next_order_at_cursor(world, intent, point)
            requested = _require_not_before_active_cursor(world, scene, point, "scene.advance")
            if intent.get("characters"):
                raise UsageError("scene.advance cannot add characters; use author move --scene to reconcile independent character presence")
            present = _scene_characters_at(world, scene, requested)
            if location_id:
                # Classify inventory and dropped props at the state just before
                # this new movement beat, not at the old scene cursor.  A
                # handoff/drop authored between those points must be honored.
                stationary = _stationary_scene_objects(world, scene, present, _state_immediately_before(point))
                if stationary:
                    names = ", ".join(stationary)
                    raise UsageError(f"scene.advance cannot change location while stationary scene objects remain at the old location: {names}. Record their object state changes in a raw changeset before advancing.")
            operations = _active_cursor_operations(world, point) if scene.status == "active" else []
            operations.append(_scene_patch(scene, point=point, location_id=location_id, add_characters=[_reference(world, str(value), "character") for value in intent.get("characters") or []]))
            if location_id and scene.status == "active":
                if present:
                    operations.append(_movement_event(intent, point, location_id, present))
            return _envelope(repository, intent, operations)
        _require_not_before_active_cursor(world, scene, point, "scene.close")
        if location_id and location_id != str(scene.frontmatter.get("location") or ""):
            raise UsageError("scene.close --location must match the current scene location; use author scene advance --location before closing")
        # Closing removes this front from the shared cursor set.  Do not
        # advance unrelated fronts as an accidental side effect.
        return _envelope(repository, intent, [_scene_patch(scene, point=point, status="closed", location_id=location_id)])

    if action == "character.move":
        characters = intent.get("characters") or []
        if not isinstance(characters, list) or not characters:
            raise UsageError("character.move requires one or more characters")
        character_ids = [_reference(world, str(value), "character") for value in characters]
        if len(set(character_ids)) != len(character_ids):
            raise UsageError("character.move characters must be distinct")
        location_id = _reference(world, str(intent.get("location") or ""), "location")
        point = _next_order_at_cursor(world, intent, _time(world, intent.get("time"), fallback=world.current_time))
        operations: list[dict[str, Any]] = []
        scene_name = intent.get("scene")
        if scene_name:
            scene = world.find(str(scene_name), "scene")
            _validate_scene_move_reconciliation(world, scene, location_id, character_ids, point)
            operations.extend(_detach_from_active_scenes(world, character_ids, point, exclude=scene.id))
            object_operations, moved_objects = _transfer_held_objects(world, character_ids, point, exclude=scene.id)
            operations.extend(object_operations)
            operations.extend(_active_cursor_operations(world, point))
            operations.append(_scene_patch(scene, point=point, add_characters=character_ids, add_objects=moved_objects))
        operations.append(_movement_event(intent, point, location_id, character_ids))
        return _envelope(repository, intent, operations)

    if action == "conversation.create":
        title = str(intent.get("title") or "").strip()
        if not title:
            raise UsageError("conversation.create requires a title")
        scene = _scene_for_conversation(world, intent.get("scene"))
        fallback = _active_scene_cursor(world, scene, "conversation.create")
        point = _next_order_at_cursor(world, intent, _time(world, intent.get("time"), fallback=fallback))
        _require_not_before_active_cursor(world, scene, point, "conversation.create")
        requested_characters = intent.get("characters")
        at = StoryTime.from_value(point, world.default_timeline)
        present = _scene_characters_at(world, scene, at)
        if requested_characters is None:
            character_ids = present
        else:
            if not isinstance(requested_characters, list) or not requested_characters:
                raise UsageError("conversation.create --character must name one or more independently present characters")
            character_ids = [_reference(world, str(value), "character") for value in requested_characters]
            if len(set(character_ids)) != len(character_ids):
                raise UsageError("conversation.create characters must be distinct")
            if any(character_id not in present for character_id in character_ids):
                raise UsageError("conversation.create characters must be present in the selected scene at the conversation time")
        if not character_ids:
            raise UsageError("conversation.create selected scene has no present characters at the conversation time")
        conversation_operation = {
            "type": "conversation.create", "temporaryId": "$author.conversation", "value": {
                "title": title, "status": "active", "scene": scene.id,
                "location": scene.frontmatter.get("location"),
                "time": {"start": point},
                "participants": _scene_participants(character_ids, point),
                "topics": [], "turns": [], "recollections": [],
            },
        }
        conversations = list(scene.frontmatter.get("conversations") or [])
        if "$author.conversation" not in conversations:
            conversations.append("$author.conversation")
        # Scene views, timeline spans, and context packets intentionally read
        # the scene-side list. Keep that reciprocal association in this same
        # changeset rather than leaving a newly created exchange orphaned from
        # its selected scene.
        scene_operation = {
            "type": "entity.update", "entity": scene.id,
            "frontmatterPatch": {"conversations": conversations},
        }
        return _envelope(repository, intent, [conversation_operation, scene_operation])

    if action == "conversation.append":
        conversation = world.find(str(intent.get("conversation") or ""), "conversation")
        if conversation.status != "active":
            raise UsageError(
                "conversation.append requires an active conversation; use a raw changeset to edit historical or closed conversations"
            )
        kind = str(intent.get("kind") or "speech")
        if kind not in {"speech", "action"}:
            raise UsageError("conversation.append kind must be speech or action")
        if kind == "action" and any(intent.get(field) is not None and intent.get(field) is not False for field in ("speaker", "addressee", "interruptLast")):
            raise UsageError("action beats cannot carry speaker, addressee, or interrupt-last; use --actor")
        if kind == "speech" and intent.get("actors"):
            raise UsageError("speech beats cannot carry action actors; use --speaker")
        point = _conversation_point(world, conversation, intent)
        participants = {
            str(item.get("character")) for item in conversation.frontmatter.get("participants") or [] if isinstance(item, dict)
        }
        turn: dict[str, Any] = {"kind": kind, "at": point, "text": str(intent.get("text") or ""), "audience": ["participants"]}
        if not turn["text"].strip():
            raise UsageError("conversation.append requires text")
        if kind == "speech":
            speaker = _reference(world, str(intent.get("speaker") or ""), "character")
            if speaker not in participants:
                raise UsageError("speech speaker must be a conversation participant")
            turn["speaker"] = speaker
            if intent.get("addressee"):
                addressee = _reference(world, str(intent["addressee"]), "character")
                if addressee not in participants:
                    raise UsageError("speech addressee must be a conversation participant")
                turn["addressee"] = addressee
            if intent.get("interruptLast"):
                speeches = [turn for turn in conversation.frontmatter.get("turns") or [] if isinstance(turn, dict) and turn.get("kind", "speech") == "speech"]
                if not speeches:
                    raise UsageError("there is no earlier spoken line to interrupt")
                turn["interrupts"] = str(speeches[-1].get("id"))
        else:
            actors = intent.get("actors") or []
            if not isinstance(actors, list) or not actors:
                raise UsageError("action beats require one or more actors")
            actor_ids = [_reference(world, str(value), "character") for value in actors]
            if len(set(actor_ids)) != len(actor_ids):
                raise UsageError("action actors must be distinct")
            if any(value not in participants for value in actor_ids):
                raise UsageError("action actors must be conversation participants")
            turn["actors"] = actor_ids
        operations: list[dict[str, Any]] = []
        if _append_advances_horizon(world, conversation, intent, point):
            # A newly allocated beat is now the author horizon. Advance every
            # active front atomically, so later scene defaults do not lag the
            # conversation that just happened.
            operations.extend(_active_cursor_operations(world, point))
        operations.append({"type": "conversation.turn.append", "conversation": conversation.id, "turn": turn})
        return _envelope(repository, intent, operations)

    raise UsageError(f"unsupported authoring action {action!r}")
