from __future__ import annotations

from copy import deepcopy
from itertools import combinations
from typing import Any

from .conversation import scene_context_time
from .generational_knowledge import AFFIRMATIVE_STATES
from .model import Record, StoryTime, World


def event_time(record: Record, default_timeline: str = "main") -> StoryTime:
    return StoryTime.from_value(record.frontmatter.get("time"), default_timeline)


def canonical_events(world: World, at: StoryTime | None = None) -> list[Record]:
    key = "canonical-events"
    if key not in world._cache:
        world._cache[key] = tuple(
            sorted(
                (record for record in world.by_kind("event") if record.status == "canonical"),
                key=lambda record: (*event_time(record, world.default_timeline).to_dict().values(), record.id),
            )
        )
    values: tuple[Record, ...] = world._cache[key]
    return [record for record in values if at is None or event_time(record, world.default_timeline).not_after(at)]


def resolve_state(world: World, entity_id: str, at: StoryTime) -> tuple[dict[str, Any], dict[str, Any]]:
    record = world.get(entity_id)
    state = deepcopy(record.frontmatter.get("initial_state") or {})
    citations: dict[str, Any] = {key: {"entityId": entity_id, "source": "initial-state"} for key in state}
    effect_key = "effects-by-target"
    if effect_key not in world._cache:
        grouped: dict[str, list[tuple[StoryTime, Record, int, dict[str, Any]]]] = {}
        for event in canonical_events(world):
            point = event_time(event, world.default_timeline)
            for ordinal, effect in enumerate(event.frontmatter.get("effects") or []):
                if isinstance(effect, dict) and effect.get("target"):
                    grouped.setdefault(str(effect["target"]), []).append((point, event, ordinal, effect))
        world._cache[effect_key] = {
            target: tuple(sorted(values, key=lambda item: (item[0].tick, item[0].order, item[1].id, item[2])))
            for target, values in grouped.items()
        }
    for point, event, ordinal, effect in world._cache[effect_key].get(entity_id, ()):
        if not point.not_after(at):
            continue
        key = str(effect.get("key", ""))
        operation = str(effect.get("operation", ""))
        if operation == "set":
            state[key] = deepcopy(effect.get("value"))
        elif operation == "clear":
            state.pop(key, None)
        elif operation == "add-to-set":
            values = list(state.get(key) or [])
            if effect.get("value") not in values:
                values.append(deepcopy(effect.get("value")))
            state[key] = values
        elif operation == "remove-from-set":
            state[key] = [value for value in list(state.get(key) or []) if value != effect.get("value")]
        citations[key] = {"entityId": event.id, "effectId": effect.get("id") or f"effect:{ordinal}", "time": point.to_dict()}
    return state, citations


def current_knowledge(world: World, character_id: str, at: StoryTime, include_forgotten: bool = False) -> list[dict[str, Any]]:
    key = "knowledge-by-knower"
    if key not in world._cache:
        grouped: dict[str, list[Record]] = {}
        for record in world.by_kind("knowledge"):
            if record.status == "canonical" and record.frontmatter.get("knower"):
                grouped.setdefault(str(record.frontmatter["knower"]), []).append(record)
        world._cache[key] = {knower: tuple(values) for knower, values in grouped.items()}
    result: list[dict[str, Any]] = []
    for record in world._cache[key].get(character_id, ()):
        applicable: list[tuple[StoryTime, int, dict[str, Any]]] = []
        for index, transition in enumerate(record.frontmatter.get("transitions") or []):
            if not isinstance(transition, dict) or not transition.get("time"):
                continue
            point = StoryTime.from_value(transition["time"], world.default_timeline)
            if point.not_after(at):
                applicable.append((point, index, transition))
        if not applicable:
            continue
        point, _index, active = max(applicable, key=lambda item: (item[0].tick, item[0].order, item[1]))
        state = str(active.get("state", "uncertain"))
        if state == "forgotten" and not include_forgotten:
            continue
        claim = deepcopy(record.frontmatter.get("claim") or {})
        typed_genealogy = "genealogy" in claim
        if typed_genealogy:
            if not any(transition.get("state") in AFFIRMATIVE_STATES for _, _, transition in applicable):
                continue
            assertion = claim.get("genealogy")
            claim = {
                "key": "genealogy", "statement": "An explicitly authored genealogy assertion.",
                "genealogy": {key: value for key, value in assertion.items()
                              if key in {"kind", "payload", "valid", "labels"}}
                if isinstance(assertion, dict) else {},
            }
        result.append({
            "record": record,
            "knowledgeId": record.id,
            "claimKey": claim.get("key"),
            "statement": claim.get("statement"),
            "claim": claim,
            "state": state,
            "confidence": active.get("confidence"),
            "acquisition": "authored" if typed_genealogy else active.get("acquisition"),
            "sourceEntityId": None if typed_genealogy else active.get("source_entity"),
            "causingEventId": None if typed_genealogy else active.get("causing_event"),
            "note": None if typed_genealogy else active.get("note"),
            "time": point.to_dict(),
            "transitionId": active.get("id"),
        })
    return sorted(result, key=lambda item: (str(item["claimKey"]), item["knowledgeId"]))


def current_relationship(world: World, relationship_id: str, at: StoryTime) -> dict[str, Any] | None:
    record = world.get(relationship_id)
    applicable: list[tuple[StoryTime, int, dict[str, Any]]] = []
    for index, transition in enumerate(record.frontmatter.get("transitions") or []):
        if isinstance(transition, dict) and transition.get("time"):
            point = StoryTime.from_value(transition["time"], world.default_timeline)
            if point.not_after(at):
                applicable.append((point, index, transition))
    if not applicable:
        return None
    point, _index, transition = max(applicable, key=lambda item: (item[0].tick, item[0].order, item[1]))
    return {
        "record": record,
        "relationshipId": record.id,
        "from": record.frontmatter.get("from"),
        "to": record.frontmatter.get("to"),
        "relationshipKind": record.frontmatter.get("relationship_kind"),
        "status": transition.get("relationship_status", "active"),
        "metrics": deepcopy(transition.get("metrics") or {}),
        "facets": deepcopy(transition.get("facets") or []),
        "note": transition.get("note"),
        "time": point.to_dict(),
        "transitionId": transition.get("id"),
    }


def relationships_from(world: World, character_id: str, at: StoryTime) -> list[dict[str, Any]]:
    key = "relationships-by-source"
    if key not in world._cache:
        grouped: dict[str, list[Record]] = {}
        for record in world.by_kind("relationship"):
            grouped.setdefault(str(record.frontmatter.get("from")), []).append(record)
        world._cache[key] = {source: tuple(values) for source, values in grouped.items()}
    return [value for record in world._cache[key].get(character_id, ()) if (value := current_relationship(world, record.id, at))]


def active_environments(world: World, at: StoryTime, targets: list[str]) -> list[Record]:
    target_set = set(targets)
    result: list[Record] = []
    for record in world.by_kind("environment"):
        time_range = record.frontmatter.get("time") or {}
        start = StoryTime.from_value(time_range.get("start"), world.default_timeline)
        end = StoryTime.from_value(time_range["end"], world.default_timeline) if time_range.get("end") else None
        record_targets = set(str(value) for value in record.frontmatter.get("targets") or [])
        if start.not_after(at) and (end is None or (at.tick, at.order) <= (end.tick, end.order)) and target_set & record_targets:
            result.append(record)
    return result


def story_point_state(world: World, record: Record, at: StoryTime) -> str:
    lifecycle = record.frontmatter.get("lifecycle") or {}
    state = str(lifecycle.get("initial_state", "dormant"))
    applicable: list[tuple[StoryTime, int, str]] = []
    for index, transition in enumerate(lifecycle.get("transitions") or []):
        if isinstance(transition, dict) and transition.get("time") and transition.get("state"):
            point = StoryTime.from_value(transition["time"], world.default_timeline)
            if point.not_after(at):
                applicable.append((point, index, str(transition["state"])))
    if applicable:
        state = max(applicable, key=lambda item: (item[0].tick, item[0].order, item[1]))[2]
    return state


def _trigger(world: World, value: Any, at: StoryTime, scene: Record | None) -> bool:
    if not value:
        return True
    if isinstance(value, list):
        return all(_trigger(world, item, at, scene) for item in value)
    if not isinstance(value, dict):
        return bool(value)
    if "all" in value:
        return all(_trigger(world, item, at, scene) for item in value["all"])
    if "any" in value:
        return any(_trigger(world, item, at, scene) for item in value["any"])
    if "not" in value:
        return not _trigger(world, value["not"], at, scene)
    if "knowledge" in value:
        predicate = value["knowledge"]
        items = current_knowledge(world, str(predicate.get("knower")), at)
        return any(
            item.get("claimKey") == predicate.get("claim_key")
            and item.get("state") in set(predicate.get("state_in") or ["accepted"])
            and float(item.get("confidence") or 0.0) >= float(predicate.get("minimum_confidence", 0.0))
            for item in items
        )
    if "entity_state" in value:
        predicate = value["entity_state"]
        state, _ = resolve_state(world, str(predicate.get("target")), at)
        observed = state.get(str(predicate.get("key")))
        if "equals" in predicate:
            return observed == predicate["equals"]
        return bool(observed)
    if "relationship" in value:
        predicate = value["relationship"]
        current = current_relationship(world, str(predicate.get("relationship")), at)
        if not current:
            return False
        metric = float((current.get("metrics") or {}).get(str(predicate.get("metric")), 0.0))
        if "greater_than_or_equal" in predicate:
            return metric >= float(predicate["greater_than_or_equal"])
        if "less_than_or_equal" in predicate:
            return metric <= float(predicate["less_than_or_equal"])
        return False
    if "event" in value:
        event_id = value["event"].get("event") if isinstance(value["event"], dict) else value["event"]
        return any(record.id == event_id for record in canonical_events(world, at))
    if "scene" in value:
        if scene is None:
            return False
        predicate = value["scene"]
        checks = []
        if predicate.get("location"):
            checks.append(scene.frontmatter.get("location") == predicate["location"])
        if predicate.get("participant"):
            checks.append(any(item.get("character") == predicate["participant"] for item in scene.frontmatter.get("participants") or [] if isinstance(item, dict)))
        if predicate.get("object"):
            checks.append(predicate["object"] in set(scene.frontmatter.get("objects") or []))
        return all(checks)
    return False


def evaluate_story_point(world: World, record: Record, at: StoryTime, scene: Record | None = None) -> dict[str, Any]:
    stored = story_point_state(world, record, at)
    dependencies = record.frontmatter.get("dependencies") or {}
    dependency_ok = True
    for dependency in dependencies.get("all") or []:
        target = world.maybe_get(str(dependency.get("story_point")))
        if not target or story_point_state(world, target, at) not in set(dependency.get("state_in") or []):
            dependency_ok = False
    trigger_ok = _trigger(world, record.frontmatter.get("trigger") or {}, at, scene)
    eligible = stored == "dormant" and dependency_ok and trigger_ok
    derived = "eligible" if eligible else ("blocked" if stored == "dormant" and not dependency_ok else stored)
    return {
        "storyPointId": record.id,
        "title": record.title,
        "storedState": stored,
        "derivedState": derived,
        "eligible": eligible,
        "dependenciesSatisfied": dependency_ok,
        "triggerSatisfied": trigger_ok,
        "priority": int(record.frontmatter.get("priority", 0)),
    }


def evaluate_all_story_points(world: World, at: StoryTime, scene: Record | None = None) -> list[dict[str, Any]]:
    return sorted(
        [evaluate_story_point(world, record, at, scene) for record in world.by_kind("story-point")],
        key=lambda item: (-item["priority"], item["title"].casefold()),
    )


def interactions(world: World, first: str, second: str, at: StoryTime | None = None) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    for event in canonical_events(world, at):
        participants = [item for item in event.frontmatter.get("participants") or [] if isinstance(item, dict)]
        ids = {str(item.get("character")) for item in participants}
        if {first, second}.issubset(ids):
            point = event_time(event, world.default_timeline)
            result.append({
                "eventId": event.id,
                "title": event.title,
                "time": point.to_dict(),
                "locationId": event.frontmatter.get("location"),
                "roles": {str(item.get("character")): item.get("role") for item in participants if item.get("character") in {first, second}},
                "record": event,
            })
    return result


def effective_time(world: World, scene: Record | None = None) -> StoryTime:
    if scene is not None:
        return scene_context_time(scene, world.default_timeline)
    if world.current_time is not None:
        return world.current_time
    active = world.active_scene()
    return scene_context_time(active, world.default_timeline) if active else StoryTime(world.default_timeline, 0, 0)
