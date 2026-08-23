from __future__ import annotations

from copy import deepcopy
from typing import Any

from .model import Record, StoryTime, World


def time_from(value: Any, default_timeline: str = "main") -> StoryTime:
    return StoryTime.from_value(value, default_timeline)


def interval_contains(start: StoryTime | None, end: StoryTime | None, at: StoryTime) -> bool:
    if start and (start.timeline != at.timeline or not start.not_after(at)):
        return False
    if end and (end.timeline != at.timeline or (at.tick, at.order) > (end.tick, end.order)):
        return False
    return True


def scene_start(scene: Record, default_timeline: str = "main") -> StoryTime:
    return time_from((scene.frontmatter.get("time") or {}).get("start"), default_timeline)


def scene_end(scene: Record, default_timeline: str = "main") -> StoryTime | None:
    value = (scene.frontmatter.get("time") or {}).get("end")
    return time_from(value, default_timeline) if value else None


def scene_context_time(scene: Record, default_timeline: str = "main") -> StoryTime:
    value = scene.frontmatter.get("time") or {}
    selected = value.get("current")
    if selected is None and scene.status == "closed":
        selected = value.get("end")
    return time_from(selected or value.get("start"), default_timeline)


def scene_contains_time(scene: Record, at: StoryTime, default_timeline: str = "main") -> bool:
    return interval_contains(scene_start(scene, default_timeline), scene_end(scene, default_timeline), at)


def participant_at(scene: Record, character_id: str, at: StoryTime, default_timeline: str = "main") -> dict[str, Any] | None:
    for participant in scene.frontmatter.get("participants") or []:
        if not isinstance(participant, dict) or str(participant.get("character")) != character_id:
            continue
        start_value = participant.get("from") or (scene.frontmatter.get("time") or {}).get("start")
        end_value = participant.get("to") or (scene.frontmatter.get("time") or {}).get("end")
        start = time_from(start_value, default_timeline) if start_value else None
        end = time_from(end_value, default_timeline) if end_value else None
        if interval_contains(start, end, at):
            return participant
    return None


def character_scene_allowed(scene: Record, character_id: str, at: StoryTime, default_timeline: str = "main") -> bool:
    return scene.kind == "scene" and scene.status in {"active", "closed"} and scene_contains_time(scene, at, default_timeline) and participant_at(scene, character_id, at, default_timeline) is not None


def observations_at(scene: Record, character_id: str, at: StoryTime, default_timeline: str = "main") -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    for observation in scene.frontmatter.get("observations") or []:
        if not isinstance(observation, dict) or not observation.get("text") or not observation.get("at"):
            continue
        point = time_from(observation["at"], default_timeline)
        end = time_from(observation["until"], default_timeline) if observation.get("until") else None
        if not interval_contains(point, end, at):
            continue
        audience = observation.get("audience") or ["participants"]
        if isinstance(audience, str):
            audience = [audience]
        allowed = "public" in audience or character_id in audience or f"char:{character_id}" in audience
        if "participants" in audience:
            allowed = participant_at(scene, character_id, at, default_timeline) is not None
        if allowed:
            result.append({
                "id": str(observation.get("id")),
                "text": str(observation["text"]),
                "at": point.to_dict(),
                "salience": float(observation.get("salience", 1.0)),
                "audience": list(audience),
            })
    return sorted(result, key=lambda item: (item["at"]["tick"], item["at"]["order"], item["id"]))


def conversation_start(record: Record, default_timeline: str = "main") -> StoryTime:
    return time_from((record.frontmatter.get("time") or {}).get("start"), default_timeline)


def conversation_end(record: Record, default_timeline: str = "main") -> StoryTime | None:
    value = (record.frontmatter.get("time") or {}).get("end")
    return time_from(value, default_timeline) if value else None


def conversation_context_time(record: Record, world: World) -> StoryTime:
    linked = world.maybe_get(str(record.frontmatter.get("scene") or ""))
    if linked and linked.kind == "scene":
        current = scene_context_time(linked, world.default_timeline)
        start = conversation_start(record, world.default_timeline)
        end = conversation_end(record, world.default_timeline)
        if current.timeline == start.timeline and (current.tick, current.order) >= (start.tick, start.order):
            if end is None or (current.tick, current.order) <= (end.tick, end.order):
                return current
    end = conversation_end(record, world.default_timeline)
    if record.status == "closed" and end:
        return end
    turns = [turn_time(turn, record, world.default_timeline) for turn in record.frontmatter.get("turns") or [] if isinstance(turn, dict) and turn.get("at")]
    return max(turns, default=conversation_start(record, world.default_timeline), key=lambda point: (point.tick, point.order))


def conversation_participant_at(record: Record, character_id: str, at: StoryTime, default_timeline: str = "main") -> dict[str, Any] | None:
    for participant in record.frontmatter.get("participants") or []:
        if not isinstance(participant, dict) or str(participant.get("character")) != character_id:
            continue
        start_value = participant.get("from") or (record.frontmatter.get("time") or {}).get("start")
        end_value = participant.get("to") or (record.frontmatter.get("time") or {}).get("end")
        start = time_from(start_value, default_timeline) if start_value else None
        end = time_from(end_value, default_timeline) if end_value else None
        if interval_contains(start, end, at):
            return participant
    return None


def turn_time(turn: dict[str, Any], record: Record, default_timeline: str = "main") -> StoryTime:
    return time_from(turn.get("at") or (record.frontmatter.get("time") or {}).get("start"), default_timeline)


def beat_kind(turn: dict[str, Any]) -> str:
    """Return the additive conversation-beat kind.

    Older stories recorded only spoken turns.  Treating an omitted kind as
    speech keeps those records, and the v2 transcript fields derived from
    them, exactly useful without a source migration.
    """
    # ``kind`` is additive: its absence is the legacy spoken-turn form.  An
    # explicitly malformed value must not quietly become speech, however; the
    # validator needs to be able to reject it as a bad discriminant.
    value = turn.get("kind", "speech")
    return value if isinstance(value, str) else ""


def turn_visible_to(record: Record, turn: dict[str, Any], character_id: str, at: StoryTime, default_timeline: str = "main") -> bool:
    point = turn_time(turn, record, default_timeline)
    if not point.not_after(at):
        return False
    audience = turn.get("audience") or ["participants"]
    if isinstance(audience, str):
        audience = [audience]
    if "public" in audience or character_id in audience or f"char:{character_id}" in audience:
        return True
    return "participants" in audience and conversation_participant_at(record, character_id, point, default_timeline) is not None


def visible_beats(record: Record, character_id: str, at: StoryTime, default_timeline: str = "main") -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    for index, turn in enumerate(record.frontmatter.get("turns") or []):
        if not isinstance(turn, dict) or not turn_visible_to(record, turn, character_id, at, default_timeline):
            continue
        value = deepcopy(turn)
        value["id"] = str(turn.get("id") or f"turn:{record.id}:{index}")
        value["at"] = turn_time(turn, record, default_timeline).to_dict()
        result.append(value)
    return sorted(result, key=lambda item: (item["at"]["tick"], item["at"]["order"], item["id"]))


def visible_turns(record: Record, character_id: str, at: StoryTime, default_timeline: str = "main") -> list[dict[str, Any]]:
    """Return the legacy audible *spoken* transcript.

    Physical action beats have the same visibility rules, but do not become
    verbatim dialogue merely because they occur in a conversation.
    """
    return [
        beat for beat in visible_beats(record, character_id, at, default_timeline)
        if beat_kind(beat) == "speech"
    ]


def current_recollection(record: Record, character_id: str, at: StoryTime, default_timeline: str = "main") -> dict[str, Any] | None:
    applicable: list[tuple[StoryTime, int, dict[str, Any]]] = []
    for index, recollection in enumerate(record.frontmatter.get("recollections") or []):
        if not isinstance(recollection, dict) or str(recollection.get("character")) != character_id or not recollection.get("at"):
            continue
        point = time_from(recollection["at"], default_timeline)
        if point.not_after(at):
            applicable.append((point, index, recollection))
    if not applicable:
        return None
    point, index, value = max(applicable, key=lambda item: (item[0].tick, item[0].order, item[1]))
    if value.get("state", "remembered") == "forgotten":
        return None
    result = deepcopy(value)
    result["id"] = str(value.get("id") or f"recol:{record.id}:{character_id}:{index}")
    result["at"] = point.to_dict()
    return result


def remembered_quotes(record: Record, recollection: dict[str, Any] | None) -> list[dict[str, Any]]:
    if not recollection:
        return []
    turns = {str(turn.get("id")): turn for turn in record.frontmatter.get("turns") or [] if isinstance(turn, dict) and turn.get("id")}
    result: list[dict[str, Any]] = []
    for turn_id in recollection.get("exact_turns") or []:
        turn = turns.get(str(turn_id))
        if turn and beat_kind(turn) == "speech":
            result.append({"sourceTurn": str(turn_id), "speaker": turn.get("speaker"), "text": turn.get("text"), "fidelity": "exact"})
    for quote in recollection.get("remembered_quotes") or []:
        if isinstance(quote, dict):
            result.append({
                "sourceTurn": quote.get("source_turn"),
                "speaker": quote.get("speaker"),
                "text": quote.get("text", ""),
                "fidelity": quote.get("fidelity", "approximate"),
            })
    return result


def conversations_for_character(world: World, character_id: str, at: StoryTime) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    for record in world.by_kind("conversation"):
        recollection = current_recollection(record, character_id, at, world.default_timeline)
        turns = visible_turns(record, character_id, at, world.default_timeline)
        if recollection or turns:
            result.append({"record": record, "recollection": recollection, "turns": turns})
    return sorted(result, key=lambda item: item["record"].title.casefold())
