from __future__ import annotations

from dataclasses import dataclass
import json
import re
from pathlib import Path
from typing import Any, Callable, Sequence

from .audience import visible_sections
from .compiler import connect, require_database
from .conversation import beat_kind, character_scene_allowed, conversations_for_character, observations_at, participant_at, remembered_quotes, scene_context_time, turn_time, visible_beats
from .errors import UsageError
from .model import Record, StoryTime, World
from .repository import Repository
from .search import STOPWORDS, search
from .semantics import active_environments, current_knowledge, evaluate_all_story_points, relationships_from, resolve_state
from .thread_filter import ThreadFilter, filter_ranked_candidates, resolve_thread_filter
from .util import TOKEN_RE


@dataclass(frozen=True, slots=True)
class Ref:
    entity_id: str
    section: str


@dataclass(slots=True)
class Atom:
    section: str
    text: str
    refs: tuple[Ref, ...]
    priority: int
    relevance: float
    key: str
    order: tuple[Any, ...] = ()


SECTION_ORDER = ["Voice and intention", "Present moment", "Conversation now", "What matters", "Remembered conversations", "Relationship pressure", "Useful recall", "Writing boundary"]
SECTION_INDEX = {value: index for index, value in enumerate(SECTION_ORDER)}
MIN_COMPONENT_BUDGET = 1800
DRAMATIC_CHARACTER_PERCENT = 64
DRAMATIC_RESERVE = 400


def _implicit_scene(world: World, character_id: str | None = None) -> Record | None:
    """Resolve the legacy omitted scene only when it is genuinely unambiguous."""
    active = world.active_scenes()
    if not active:
        return None
    if character_id is None:
        if len(active) != 1:
            raise UsageError("multiple active scenes; select --scene explicitly")
        return active[0]
    candidates = [
        scene for scene in active
        if character_scene_allowed(
            scene,
            character_id,
            world.current_time or scene_context_time(scene, world.default_timeline),
            world.default_timeline,
        )
    ]
    if len(candidates) == 1:
        return candidates[0]
    if len(candidates) > 1:
        raise UsageError("character is present in multiple active scenes; select --scene explicitly")
    return None


def _default_scene_time(world: World, scene: Record) -> StoryTime:
    return world.current_time if scene.status == "active" and world.current_time is not None else scene_context_time(scene, world.default_timeline)


class CitationBook:
    def __init__(self) -> None:
        self.sources: dict[str, int] = {}
        self.sections: dict[tuple[str, str], int] = {}
        self.by_source: dict[str, list[str]] = {}

    def marker(self, ref: Ref) -> str:
        source_number = self.sources.setdefault(ref.entity_id, len(self.sources) + 1)
        values = self.by_source.setdefault(ref.entity_id, [])
        key = (ref.entity_id, ref.section)
        if key not in self.sections:
            values.append(ref.section)
            self.sections[key] = len(values)
        return f"s{source_number}.{self.sections[key]}"

    def markers(self, refs: Sequence[Ref]) -> str:
        values: list[str] = []
        for ref in refs:
            marker = self.marker(ref)
            if marker not in values:
                values.append(marker)
        return "[" + ", ".join(values) + "]" if values else ""

    def footer(self) -> list[str]:
        if not self.sources:
            return []
        lines = ["", "## Provenance"]
        for entity_id, number in sorted(self.sources.items(), key=lambda item: item[1]):
            labels = "; ".join(f"{index} {label}" for index, label in enumerate(self.by_source[entity_id], 1))
            lines.append(f"- [s{number}] `{entity_id}` — {labels}")
        return lines


def _tokens(text: str | None) -> set[str]:
    return {match.group(0).casefold() for match in TOKEN_RE.finditer(text or "") if len(match.group(0)) > 1 and match.group(0).casefold() not in STOPWORDS}


def _relevance(text: str, query: set[str], scene: set[str]) -> float:
    values = _tokens(text)
    return len(values & query) * 4.0 + len(values & scene) * 1.4 + min(len(values), 30) / 100.0


def _plain(text: str, limit: int = 500) -> str:
    text = re.sub(r"^#{1,6}\s+", "", text.strip(), flags=re.M)
    text = re.sub(r"\s+", " ", text).strip()
    if len(text) <= limit:
        return text
    cut = text.rfind(" ", 0, limit - 1)
    return text[: cut if cut > 0 else limit].rstrip() + "…"


def _section_content(text: str, limit: int = 500) -> str:
    lines = text.strip().splitlines()
    if lines and re.match(r"^#{1,6}\s+", lines[0]):
        lines = lines[1:]
    return _plain("\n".join(lines), limit)


def _title(world: World, entity_id: str | None) -> str:
    record = world.maybe_get(entity_id)
    return record.title if record else str(entity_id or "unknown")


def _present_participants(world: World, scene: Record, at: StoryTime) -> list[dict[str, Any]]:
    return [item for item in scene.frontmatter.get("participants") or [] if isinstance(item, dict) and item.get("character") and participant_at(scene, str(item["character"]), at, world.default_timeline)]


def accessible_entities(world: World, character_id: str, scene: Record, at: StoryTime) -> set[str]:
    values = {character_id, scene.id}
    if scene.frontmatter.get("location"):
        values.add(str(scene.frontmatter["location"]))
    values.update(str(item["character"]) for item in _present_participants(world, scene, at))
    values.update(str(item) for item in scene.frontmatter.get("objects") or [])
    values.update(str(item) for item in scene.frontmatter.get("environments") or [])
    for item in current_knowledge(world, character_id, at):
        claim = item.get("claim") or {}
        if isinstance(claim.get("subject"), str):
            values.add(claim["subject"])
        obj = claim.get("object")
        if isinstance(obj, dict) and isinstance(obj.get("entity"), str):
            values.add(obj["entity"])
        if isinstance(item.get("sourceEntityId"), str):
            values.add(item["sourceEntityId"])
    for item in relationships_from(world, character_id, at):
        values.add(str(item.get("to")))
    return {value for value in values if value in world.records}


def _profile(world: World, character: Record, query: set[str], scene_tokens: set[str]) -> Atom:
    clauses: list[str] = []
    refs: list[Ref] = []
    for section in visible_sections(character, "character", character.id):
        heading = (section.heading or "").casefold()
        if heading not in {"summary", "voice", "goals", "manner", "mannerisms"}:
            continue
        content = _section_content(section.text, 320 if heading == "summary" else 250)
        if not content:
            continue
        if heading == "summary":
            clauses.append(content)
        elif heading == "goals":
            clauses.append(f"Immediate aims: {content}")
        else:
            clauses.append(f"Voice and manner: {content}")
        refs.append(Ref(character.id, section.heading or "profile"))
    text = " ".join(clauses) or f"Use {character.title}'s established characterization."
    return Atom("Voice and intention", text, tuple(refs or [Ref(character.id, "profile")]), 122, _relevance(text, query, scene_tokens), "profile")


def _present(world: World, character: Record, scene: Record, at: StoryTime, query: set[str], scene_tokens: set[str]) -> list[Atom]:
    participants = _present_participants(world, scene, at)
    location = world.maybe_get(str(scene.frontmatter.get("location")))
    companions = [_title(world, str(item["character"])) for item in participants if str(item["character"]) != character.id]
    where = location.title if location else scene.title
    lead = f"{character.title} is in **{scene.title}** at **{where}**, {at.timeline} {at.tick}:{at.order}"
    if companions:
        lead += f", with {', '.join(companions)}"
    lead += "."
    result = [Atom("Present moment", lead, (Ref(scene.id, "time-and-participants"),), 130, 10.0, "frame", (0,))]
    for index, observation in enumerate(observations_at(scene, character.id, at, world.default_timeline)):
        text = _plain(observation["text"], 340)
        result.append(Atom("Present moment", text, (Ref(scene.id, f"observation:{observation['id']}"),), 114, observation["salience"] * 3 + _relevance(text, query, scene_tokens), f"obs:{observation['id']}", (1, observation["at"]["tick"], observation["at"]["order"], index)))
    object_phrases: list[str] = []
    object_refs: list[Ref] = []
    badge_holders: list[str] = []
    badge_count = 0
    present_ids = {str(item.get("character")) for item in participants if item.get("character")}
    scene_location = str(scene.frontmatter.get("location") or "")
    state_cache: dict[str, dict[str, Any]] = {}

    def object_is_present(object_id: str, visiting: set[str] | None = None) -> bool:
        visiting = set() if visiting is None else visiting
        if object_id in visiting:
            return False
        visiting.add(object_id)
        state = state_cache.setdefault(object_id, resolve_state(world, object_id, at)[0])
        holder = state.get("holder")
        if isinstance(holder, dict) and holder.get("entity") in present_ids:
            return True
        location_value = state.get("location")
        if isinstance(location_value, dict) and location_value.get("entity") == scene_location:
            return True
        container = state.get("container")
        if isinstance(container, dict) and isinstance(container.get("entity"), str):
            return object_is_present(str(container["entity"]), visiting)
        return False

    for object_id in scene.frontmatter.get("objects") or []:
        object_id = str(object_id)
        record = world.maybe_get(object_id)
        if not record or not object_is_present(object_id):
            continue
        state = state_cache[object_id]
        placement = ""
        holder = state.get("holder")
        location_value = state.get("location")
        container = state.get("container")
        if isinstance(holder, dict) and holder.get("entity"):
            placement = f"held by {_title(world, str(holder['entity']))}"
        elif isinstance(container, dict) and container.get("entity"):
            placement = f"inside {_title(world, str(container['entity']))}"
        elif isinstance(location_value, dict) and location_value.get("entity"):
            placement = f"at {_title(world, str(location_value['entity']))}"
        condition = state.get("condition")
        if record.frontmatter.get("object_type") == "guild-badge":
            badge_count += 1
            if isinstance(holder, dict) and holder.get("entity"):
                badge_holders.append(_title(world, str(holder["entity"])))
            continue
        detail = "; ".join(value for value in [placement, f"condition {condition}" if condition else ""] if value)
        object_phrases.append(f"{record.title}" + (f" ({detail})" if detail else ""))
        object_refs.append(Ref(record.id, "current-state"))
    if badge_count:
        holder_text = ", ".join(badge_holders[:3])
        if len(badge_holders) > 3:
            holder_text += f", and {len(badge_holders) - 3} others"
        detail = f"held individually by {holder_text}" if holder_text else "carried by the company"
        object_phrases.append(f"{badge_count} Lantern Pike badges ({detail})")
        object_refs.append(Ref(scene.id, "grouped-object-inventory:guild-badge"))
    if object_phrases:
        text = "At hand — " + "; ".join(object_phrases[:5]) + "."
        result.append(Atom("Present moment", _plain(text, 520), tuple(object_refs[:5]), 111, _relevance(text, query, scene_tokens) + 2, "present:objects", (2,)))
    targets = [scene.id, str(scene.frontmatter.get("location") or "")]
    phrases: list[str] = []
    refs: list[Ref] = []
    for environment in active_environments(world, at, targets):
        sensory = [_plain(str(item), 140) for item in (environment.frontmatter.get("sensory") or [])[:2]]
        conditions = [f"{str(key).replace('_', ' ')}: {_plain(str(value), 80)}" for key, value in list((environment.frontmatter.get("conditions") or {}).items())[:2]]
        if sensory or conditions:
            phrases.append(f"{environment.title}: " + "; ".join(sensory + conditions))
            refs.append(Ref(environment.id, "active-environment"))
    if phrases:
        text = _plain("Atmosphere and pressure — " + " | ".join(phrases[:3]), 500)
        result.append(Atom("Present moment", text, tuple(refs[:3]), 84, _relevance(text, query, scene_tokens), "environment", (3,)))
    return result


def _conversation_now(world: World, character: Record, scene: Record, at: StoryTime, query: set[str], scene_tokens: set[str]) -> list[Atom]:
    result: list[Atom] = []
    for conversation_id in scene.frontmatter.get("conversations") or []:
        conversation = world.maybe_get(str(conversation_id))
        if not conversation or conversation.kind != "conversation":
            continue
        beats = visible_beats(conversation, character.id, at, world.default_timeline)
        if not beats:
            continue
        values = beats[-5:]
        lines = [f"**{conversation.title} — latest perceived beats:**"]
        refs: list[Ref] = []
        for beat in values:
            identifier = str(beat.get("id") or "")
            if beat_kind(beat) == "action":
                actors = ", ".join(_title(world, str(actor)) for actor in beat.get("actors") or [])
                lines.append(f"*Action — {actors}:* {_plain(str(beat.get('text', '')), 300)}")
                refs.append(Ref(conversation.id, f"action:{identifier}"))
                continue
            speaker = _title(world, str(beat.get("speaker")))
            delivery = f" ({_plain(str(beat.get('delivery')), 40)})" if beat.get("delivery") else ""
            lines.append(f'> **{speaker}{delivery}:** “{_plain(str(beat.get("text", "")), 300)}”')
            refs.append(Ref(conversation.id, f"turn:{identifier}"))
        text = "\n".join(lines)
        result.append(Atom("Conversation now", text, tuple(refs), 108, _relevance(text, query, scene_tokens) + 2, f"conversation:{conversation.id}"))
    return result


def _author_conversation_now(world: World, scene: Record, at: StoryTime, query: set[str], scene_tokens: set[str]) -> list[Atom]:
    """Return recent canonical dialogue *and choreography* for an author packet.

    Action beats are deliberately not quoted: only speech is verbatim.  This
    gives an author the immediate physical pressure of a scene while preserving
    the provenance boundary between words said and actions described.
    """

    result: list[Atom] = []
    for conversation_id in scene.frontmatter.get("conversations") or []:
        conversation = world.maybe_get(str(conversation_id))
        if not conversation or conversation.kind != "conversation":
            continue
        beats = [
            turn for turn in conversation.frontmatter.get("turns") or []
            if isinstance(turn, dict) and turn_time(turn, conversation, world.default_timeline).not_after(at)
        ][-5:]
        if not beats:
            continue
        lines = [f"**{conversation.title} — recent canonical beats:**"]
        refs: list[Ref] = []
        for beat in beats:
            identifier = str(beat.get("id") or "")
            if beat_kind(beat) == "action":
                actors = ", ".join(_title(world, str(actor)) for actor in beat.get("actors") or [])
                lines.append(f"*Action — {actors}:* {_plain(str(beat.get('text', '')), 300)}")
                refs.append(Ref(conversation.id, f"action:{identifier}"))
            else:
                speaker = _title(world, str(beat.get("speaker")))
                delivery = f" ({_plain(str(beat.get('delivery')), 40)})" if beat.get("delivery") else ""
                lines.append(f'> **{speaker}{delivery}:** “{_plain(str(beat.get("text", "")), 300)}”')
                refs.append(Ref(conversation.id, f"turn:{identifier}"))
        text = "\n".join(lines)
        result.append(Atom("Conversation now", text, tuple(refs), 112, _relevance(text, query, scene_tokens) + 2, f"author-conversation:{conversation.id}"))
    return result


def _belief_text(character: Record, statement: str, state: str, confidence: Any) -> str:
    subject = str((character.frontmatter.get("pronouns") or {}).get("subject", "they")).capitalize()
    prefix = {"accepted": f"{subject} believes", "suspected": f"{subject} suspects", "rejected": f"{subject} rejects", "uncertain": f"{subject} is uncertain about", "remembered": f"{subject} remembers"}.get(state, f"{subject} holds as {state}")
    statement = statement.rstrip(" .;:")
    qualifier = ""
    if confidence is not None:
        value = float(confidence)
        qualifier = "near certainty" if value >= .95 else "high confidence" if value >= .8 else "moderate confidence" if value >= .6 else "low confidence"
    return f"{prefix}: {statement}" + (f" ({qualifier})." if qualifier else ".")


def _knowledge(world: World, character: Record, at: StoryTime, query: set[str], scene_tokens: set[str], scene_entities: set[str]) -> tuple[list[Atom], int]:
    values = current_knowledge(world, character.id, at)
    result: list[Atom] = []
    for item in values:
        text = _belief_text(character, _plain(str(item.get("statement") or ""), 330), str(item.get("state")), item.get("confidence"))
        relevance = _relevance(text, query, scene_tokens)
        claim = item.get("claim") or {}
        referenced = {
            value
            for value in [claim.get("subject"), item.get("sourceEntityId")]
            if isinstance(value, str)
        }
        claim_object = claim.get("object")
        if isinstance(claim_object, dict) and isinstance(claim_object.get("entity"), str):
            referenced.add(claim_object["entity"])
        scene_match = bool(referenced & scene_entities)
        point = item.get("time") or {}
        recency = max(0.0, 4.0 - max(0, at.tick - int(point.get("tick", at.tick))) / 8.0)
        result.append(Atom(
            "What matters",
            text,
            (Ref(item["knowledgeId"], f"transition:{item.get('transitionId')}"),),
            106 if scene_match else 98 if relevance else 88,
            relevance + float(item.get("confidence") or 0) + recency + (8.0 if scene_match else 0.0),
            f"knowledge:{item['knowledgeId']}",
            (0 if scene_match else 1, -relevance, -recency, -float(item.get("confidence") or 0)),
        ))
    return result, len(values)


def _relationships(world: World, character: Record, scene: Record, at: StoryTime, query: set[str], scene_tokens: set[str]) -> tuple[list[Atom], int]:
    present = {str(item["character"]) for item in _present_participants(world, scene, at)}
    values = relationships_from(world, character.id, at)
    result: list[Atom] = []
    for item in values:
        target = str(item.get("to"))
        metrics = item.get("metrics") or {}
        phrases = []
        trust = float(metrics.get("trust", 0))
        phrases.append("deep trust" if trust >= .7 else "cautious trust" if trust >= .25 else "distrust" if trust < -.05 else "uncertain trust")
        fear = float(metrics.get("fear", 0))
        if fear >= .25:
            phrases.append("real caution")
        obligation = float(metrics.get("obligation", 0))
        if obligation >= .25:
            phrases.append("meaningful obligation")
        facets = [str(value).replace("-", " ") for value in (item.get("facets") or [])[:3]]
        text = f"With **{_title(world, target)}**: " + ", ".join(phrases)
        if facets:
            text += f"; shaped by {', '.join(facets)}"
        text += "."
        result.append(Atom("Relationship pressure", text, (Ref(item["relationshipId"], f"transition:{item.get('transitionId')}"),), 94 if target in present else 66, _relevance(text, query, scene_tokens) + (3 if target in present else 0), f"relationship:{item['relationshipId']}", (0 if target in present else 1, _title(world, target))))
    return result, len(values)


def _memories(world: World, character: Record, at: StoryTime, query: set[str], scene_tokens: set[str]) -> tuple[list[Atom], int]:
    values = conversations_for_character(world, character.id, at)
    result: list[Atom] = []
    for item in values:
        recollection = item.get("recollection")
        conversation = item["record"]
        if not recollection:
            continue
        text = f"{character.title} remembers **{conversation.title}** as: {_plain(str(recollection.get('summary') or ''), 340)}"
        if recollection.get("interpretation"):
            text += f" Interpretation: {_plain(str(recollection['interpretation']), 220).rstrip(' .;:')}."
        quotes = remembered_quotes(conversation, recollection)
        if quotes:
            quote = quotes[0]
            text += f' Retained {quote.get("fidelity")} wording from {_title(world, str(quote.get("speaker")))}: “{_plain(str(quote.get("text", "")), 190)}”'
        relevance = _relevance(text, query, scene_tokens)
        result.append(Atom("Remembered conversations", text, (Ref(conversation.id, f"recollection:{recollection['id']}"),), 91 if relevance else 76, relevance + 1, f"memory:{recollection['id']}"))
    return result, len(values)


def _retrieval(world: World, database: Path, character: Record, scene: Record, at: StoryTime, query_text: str | None, query: set[str], scene_tokens: set[str], scene_entities: set[str], mode: str) -> tuple[list[Atom], int]:
    if not query:
        return [], 0
    accessible = accessible_entities(world, character.id, scene, at)
    knowledge_ids = {item["knowledgeId"] for item in current_knowledge(world, character.id, at)}
    with connect(database, True) as connection:
        values = search(connection, query_text, perspective="character", character_id=character.id, scene_id=scene.id, at=at, accessible_entities=accessible, active_knowledge=knowledge_ids, mode=mode, limit=12)
    result: list[Atom] = []
    seen: set[str] = set()
    for item in values:
        record = world.maybe_get(str(item["entityId"]))
        if not record or record.kind in {"knowledge", "conversation", "character", "scene"}:
            continue
        snippet = _plain(re.sub(r"</?mark>", "", str(item.get("snippet") or item.get("text") or "")), 300)
        normalized = re.sub(r"\W+", "", snippet.casefold())
        if not normalized or normalized in seen:
            continue
        seen.add(normalized)
        text = f"**{record.title}:** {snippet}"
        query_overlap = len(_tokens(text) & query)
        scene_overlap = len(_tokens(text) & scene_tokens)
        current_entity = record.id in scene_entities
        # One generic word such as "ledger" should not displace current beliefs
        # or dialogue. Current-scene entities remain eligible with one strong
        # overlap; historical supporting material needs at least two query terms.
        if query and not current_entity and query_overlap < 2:
            continue
        if not query_overlap and scene_overlap < 2 and not current_entity:
            continue
        priority = 84 if current_entity else 73
        result.append(Atom("Useful recall", text, (Ref(record.id, str(item.get("heading") or item.get("documentKind"))),), priority, _relevance(text, query, scene_tokens) + float(item.get("score", 0)) * 100 + (6 if current_entity else 0), f"retrieval:{item['documentId']}"))
    return result, len(values)


def _near_duplicate(atom: Atom, selected: Sequence[Atom]) -> bool:
    left = _tokens(atom.text)
    for existing in selected:
        right = _tokens(existing.text)
        union = left | right
        if union and len(left & right) / len(union) >= .82:
            return True
    return False


def _render(title: str, intro: str, atoms: list[Atom]) -> str:
    book = CitationBook()
    grouped = {section: [] for section in SECTION_ORDER}
    for atom in atoms:
        grouped.setdefault(atom.section, []).append(atom)
    lines = [f"# {title}", "", intro]
    for section in SECTION_ORDER:
        values = sorted(grouped.get(section) or [], key=lambda atom: atom.order or (atom.key,))
        if not values:
            continue
        lines.extend(["", f"## {section}"])
        for atom in values:
            marker = book.markers(atom.refs)
            first, *rest = atom.text.splitlines()
            lines.append(f"- {first}" + (f" {marker}" if marker else ""))
            lines.extend(f"  {line}" for line in rest)
    lines.extend(book.footer())
    return "\n".join(lines).strip() + "\n"


def _size(payload: dict[str, Any]) -> int:
    return len(json.dumps(payload, ensure_ascii=False, separators=(",", ":")))


def _payload(base: dict[str, Any], title: str, intro: str, selected: list[Atom], source_count: int, budget: int, minimum_budget: int) -> dict[str, Any]:
    value = {
        **base,
        "promptText": _render(title, intro, selected),
        "selection": {
            "included": len(selected),
            "omitted": max(0, source_count - len(selected)),
            "budgetCharacters": budget,
            "minimumBudgetCharacters": minimum_budget,
            "serializedCharacters": 0,
        },
    }
    for _ in range(5):
        current = _size(value)
        if value["selection"]["serializedCharacters"] == current:
            break
        value["selection"]["serializedCharacters"] = current
    return value


def _minimum_budget(base: dict[str, Any], title: str, intro: str, required: list[Atom], source_count: int) -> int:
    """Return the smallest serialized packet that can contain required atoms.

    The budget fields themselves contribute to the serialized size, so calculate
    to a fixed point rather than relying on a global, approximate floor.
    """
    minimum = MIN_COMPONENT_BUDGET
    for _ in range(10):
        value = _payload(base, title, intro, required, source_count, minimum, minimum)
        measured = _size(value)
        if measured <= minimum:
            return minimum
        minimum = measured
    return minimum


def _focus(query: str | None, query_tokens: set[str], *, perspective: str, search_mode: str | None = None, retrieval_count: int | None = None) -> dict[str, Any]:
    """Describe how an optional writing query affected this packet's selection."""
    terms = sorted(query_tokens)
    value: dict[str, Any] = {
        "query": query if terms else None,
        "terms": terms,
        "applied": bool(terms),
    }
    if not terms:
        value["effect"] = "No effective query terms were provided. Selection prioritizes the present scene and required writing boundaries."
        if perspective == "character":
            value["retrieval"] = {"requested": False, "eligibleCandidates": 0}
        return value
    if perspective == "character":
        value["effect"] = "Matching terms raise the relevance of candidate context before it is ranked into the character packet."
        value["retrieval"] = {
            "requested": True,
            "mode": search_mode,
            "eligibleCandidates": retrieval_count or 0,
            "effect": "Only perspective-safe, accessible recall is eligible for query retrieval.",
        }
    elif perspective == "author":
        value["effect"] = "Matching terms raise the relevance of canonical author candidates before they are ranked into the author margin."
    else:
        value["effect"] = "The query is applied independently to the character packet and author margin."
    return value


def _fit(base: dict[str, Any], title: str, intro: str, required: list[Atom], groups: list[list[Atom]], candidates: list[Atom], source_count: int, budget: int, max_items: int, limits: dict[str, int], post_select: Callable[[list[Atom]], list[Atom]] | None = None) -> dict[str, Any]:
    minimum_budget = _minimum_budget(base, title, intro, required, source_count)
    if budget < minimum_budget:
        raise UsageError(
            f"context budget of {budget} characters is too small; at least {minimum_budget} characters are required "
            f"for the {len(required)} structural context items. Increase --max-characters to {minimum_budget} or higher."
        )
    if max_items < len(required):
        raise UsageError(f"context max-items of {max_items} is too small; at least {len(required)} items are required for structural context")
    selected: list[Atom] = []
    counts: dict[str, int] = {}
    keys: set[str] = set()

    def add(atom: Atom, required_atom: bool = False) -> bool:
        if atom.key in keys or len(selected) >= max_items or _near_duplicate(atom, selected):
            return False
        if not required_atom and counts.get(atom.section, 0) >= limits.get(atom.section, max_items):
            return False
        trial = [*selected, atom]
        if _size(_payload(base, title, intro, trial, source_count, budget, minimum_budget)) <= budget:
            selected.append(atom); keys.add(atom.key); counts[atom.section] = counts.get(atom.section, 0) + 1
            return True
        if required_atom:
            raise UsageError("required context exceeds budget")
        return False

    for atom in required:
        add(atom, True)
    order = lambda atom: (-atom.priority, -atom.relevance, SECTION_INDEX.get(atom.section, 999), atom.key)
    for group in groups:
        for atom in sorted(group, key=order):
            if add(atom):
                break
    for atom in sorted(candidates, key=order):
        add(atom)
    output_atoms = post_select(selected) if post_select is not None else selected
    value = _payload(base, title, intro, output_atoms, source_count, budget, minimum_budget)
    if _size(value) > budget:
        raise UsageError("context budget enforcement failed")
    return value


def build_context(repository: Repository, *, character_id: str, scene_id: str | None = None, revision: str = "HEAD", perspective: str = "character", query: str | None = None, max_characters: int = 8000, max_items: int = 24, search_mode: str = "hybrid", timeline: str | None = None, tick: int | None = None, order: int = 2_147_483_647, require_compiled: bool = False, _thread_filter_ids: tuple[str, ...] | None = None) -> dict[str, Any]:
    world, database = require_database(repository, revision, require_compiled=require_compiled)
    character = world.find(character_id, "character")
    scene = world.find(scene_id, "scene") if scene_id else _implicit_scene(world, character.id if perspective == "character" else None)
    if not scene:
        raise UsageError("no active scene and no scene selected")
    at = world.story_time(tick, timeline, order) if tick is not None else _default_scene_time(world, scene)
    thread_filter: ThreadFilter | None = None
    if _thread_filter_ids is not None:
        with connect(database, True) as connection:
            thread_filter = resolve_thread_filter(connection, _thread_filter_ids)

    if perspective == "character":
        if not character_scene_allowed(scene, character.id, at, world.default_timeline):
            raise UsageError(f"{character.title} is not present in {scene.title} at the selected time")
        query_tokens = _tokens(query)
        context_terms = [scene.title, _title(world, str(scene.frontmatter.get("location"))), query or ""]
        context_terms.extend(_title(world, str(value)) for value in scene.frontmatter.get("objects") or [])
        context_terms.extend(_title(world, str(value)) for value in scene.frontmatter.get("story_points") or [])
        for conversation_id in scene.frontmatter.get("conversations") or []:
            conversation_record = world.maybe_get(str(conversation_id))
            if conversation_record:
                context_terms.extend(
                    str(turn.get("text", ""))
                    for turn in (conversation_record.frontmatter.get("turns") or [])[-6:]
                    if isinstance(turn, dict)
                )
        scene_tokens = _tokens(" ".join(context_terms))
        profile = _profile(world, character, query_tokens, scene_tokens)
        present = _present(world, character, scene, at, query_tokens, scene_tokens)
        conversation = _conversation_now(world, character, scene, at, query_tokens, scene_tokens)
        scene_entities = {
            str(value)
            for value in [
                scene.frontmatter.get("location"),
                *(scene.frontmatter.get("objects") or []),
                *(scene.frontmatter.get("story_points") or []),
            ]
            if value
        }
        knowledge, knowledge_total = _knowledge(world, character, at, query_tokens, scene_tokens, scene_entities)
        relationships, relationship_total = _relationships(world, character, scene, at, query_tokens, scene_tokens)
        memories, memory_total = _memories(world, character, at, query_tokens, scene_tokens)
        retrieval, retrieval_total = _retrieval(world, database, character, scene, at, query, query_tokens, scene_tokens, scene_entities, search_mode)
        boundary = Atom("Writing boundary", "Write only from this character’s perceptions, remembered conversations, and explicit beliefs. Do not import canonical events, another mind, or author truth unless this packet says the character knows it.", (), 140, 0, "boundary")
        candidates = [*present[1:], *conversation, *knowledge, *memories, *relationships, *retrieval]
        def remove_disallowed_recall(selected: list[Atom]) -> list[Atom]:
            recall = [atom for atom in selected if atom.section == "Useful recall"]
            if not recall:
                return selected
            candidates = [
                {"entityId": atom.refs[0].entity_id, "atomKey": atom.key}
                for atom in recall
                if atom.refs
            ]
            with connect(database, True) as connection:
                allowed = {
                    str(item["atomKey"])
                    for item in filter_ranked_candidates(connection, candidates, thread_filter)
                }
            return [
                atom
                for atom in selected
                if atom.section != "Useful recall" or atom.key in allowed
            ]

        return _fit(
            {
                "protocol": "wedl-context/v3",
                "revision": world.revision,
                "perspective": "character",
                "characterId": character.id,
                "sceneId": scene.id,
                "effectiveTime": at.to_dict(),
                "focus": _focus(query, query_tokens, perspective="character", search_mode=search_mode, retrieval_count=len(retrieval)),
            },
            f"{character.title} — {scene.title}", f"A compact writing packet from **{character.title}’s** lived perspective.",
            [profile, present[0], boundary], [present[1:], conversation, knowledge, relationships, memories, retrieval], candidates,
            3 + len(present) - 1 + len(conversation) + knowledge_total + relationship_total + memory_total + retrieval_total,
            max_characters, max_items,
            {"Voice and intention": 1, "Present moment": 4, "Conversation now": 2, "What matters": 7, "Remembered conversations": 3, "Relationship pressure": 3, "Useful recall": 4, "Writing boundary": 1},
            remove_disallowed_recall if thread_filter is not None else None,
        )

    if perspective == "author":
        query_tokens = _tokens(query); scene_tokens = _tokens(scene.title + " " + (query or ""))
        atoms: list[Atom] = []
        conversation_atoms = _author_conversation_now(world, scene, at, query_tokens, scene_tokens)
        for record in (character, scene):
            for section in visible_sections(record, "author"):
                heading = section.heading or "source"
                if record is scene or heading.casefold() in {"author notes", "author constraints", "goals", "hidden truth", "author secrets"}:
                    text = f"**{record.title} — {heading}:** {_section_content(section.text, 440)}"
                    atoms.append(Atom("What matters", text, (Ref(record.id, heading),), 90, _relevance(text, query_tokens, scene_tokens), f"author:{record.id}:{heading}"))
        for index, constraint in enumerate(scene.frontmatter.get("author_constraints") or []):
            atoms.append(Atom("Writing boundary", _plain(str(constraint), 380), (Ref(scene.id, f"constraint:{index}"),), 110, 0, f"constraint:{index}"))
        linked_story_points = {str(value) for value in scene.frontmatter.get("story_points") or []}
        for item in evaluate_all_story_points(world, at, scene):
            if item["derivedState"] not in {"eligible", "active"}:
                continue
            record = world.get(item["storyPointId"])
            text = f"**{record.title}:** {item['derivedState']}; priority {item['priority']}."
            relevance = _relevance(text + " " + record.body, query_tokens, scene_tokens)
            query_overlap = len(_tokens(text + " " + record.body) & query_tokens)
            linked = record.id in linked_story_points
            priority = int(item.get("priority") or 0)
            if not linked and priority < 90 and query_overlap < 2:
                continue
            atoms.append(Atom("What matters", text, (Ref(record.id, "story-point-state"),), 104 if linked else 84, relevance + (5 if linked else 0), f"sp:{record.id}"))
        frame = Atom("Present moment", f"Canonical author view of **{scene.title}** at **{at.timeline} {at.tick}:{at.order}**.", (Ref(scene.id, "author-frame"),), 130, 10, "frame")
        boundary = Atom("Writing boundary", "This margin may contain canonical truth and dramatic irony. Never attribute it to the viewpoint character unless the character packet independently supplies that knowledge.", (), 140, 0, "boundary")
        return _fit(
            {
                "protocol": "wedl-context/v3",
                "revision": world.revision,
                "perspective": "author",
                "characterId": character.id,
                "sceneId": scene.id,
                "effectiveTime": at.to_dict(),
                "focus": _focus(query, query_tokens, perspective="author"),
            },
            f"Author margin — {scene.title}",
            "Canonical truth, dramatic pressure, and dialogue provenance for constructing the next beat.",
            [frame, boundary], [conversation_atoms, atoms], [*conversation_atoms, *atoms], len(atoms) + len(conversation_atoms) + 2, max_characters, max_items,
            {"Present moment": 1, "Conversation now": 3, "What matters": 12, "Writing boundary": 6},
        )

    if perspective != "dramatic-irony":
        raise UsageError(f"unsupported perspective {perspective}")

    packet_args = {
        "character_id": character.id,
        "scene_id": scene.id,
        # The outer read established this exact compiled revision.  Reuse it
        # for every nested packet so a moving HEAD cannot make the character
        # and author components disagree midway through assembly.
        "revision": world.revision,
        "query": query,
        "max_items": max_items,
        "search_mode": search_mode,
        "timeline": at.timeline,
        "tick": at.tick,
        "order": at.order,
        # A strict outer read must stay strict recursively.  In particular,
        # never let a nested component rebuild a cache which disappeared or
        # became stale after the outer read succeeded.
        "require_compiled": require_compiled,
    }
    character_packet_args = {**packet_args, "_thread_filter_ids": _thread_filter_ids}
    character_floor = build_context(repository, perspective="character", max_characters=100_000, **character_packet_args)["selection"]["minimumBudgetCharacters"]
    author_floor = build_context(repository, perspective="author", max_characters=100_000, **packet_args)["selection"]["minimumBudgetCharacters"]
    minimum_budget = (character_floor * 100 + DRAMATIC_CHARACTER_PERCENT - 1) // DRAMATIC_CHARACTER_PERCENT
    while minimum_budget - minimum_budget * DRAMATIC_CHARACTER_PERCENT // 100 - DRAMATIC_RESERVE < author_floor:
        minimum_budget += 1

    def dramatic_packet(budget: int, minimum: int) -> dict[str, Any]:
        char_budget = budget * DRAMATIC_CHARACTER_PERCENT // 100
        author_budget = budget - char_budget - DRAMATIC_RESERVE
        character_packet = build_context(repository, perspective="character", max_characters=char_budget, **character_packet_args)
        author_packet = build_context(repository, perspective="author", max_characters=author_budget, **packet_args)
        value = {
            "protocol": "wedl-context/v3",
            "revision": world.revision,
            "perspective": "dramatic-irony",
            "characterId": character.id,
            "sceneId": scene.id,
            "effectiveTime": at.to_dict(),
            "focus": _focus(query, _tokens(query), perspective="dramatic-irony"),
            "characterPrompt": character_packet["promptText"],
            "authorMargin": author_packet["promptText"],
            "selection": {"budgetCharacters": budget, "minimumBudgetCharacters": minimum, "serializedCharacters": 0},
        }
        for _ in range(5):
            value["selection"]["serializedCharacters"] = _size(value)
        return value

    # The nested packets have distinct structural minima. Include the enclosing
    # packet's own metadata in the final lower bound as well.
    for _ in range(10):
        try:
            minimum_packet = dramatic_packet(minimum_budget, minimum_budget)
        except UsageError:
            minimum_budget += 1
            continue
        measured = _size(minimum_packet)
        if measured <= minimum_budget:
            break
        minimum_budget = measured
    else:  # pragma: no cover - protects against an unexpected size-cycle
        raise UsageError("could not determine dramatic-irony context minimum budget")

    if max_characters < minimum_budget:
        raise UsageError(
            f"dramatic-irony context budget of {max_characters} characters is too small; at least {minimum_budget} characters are required "
            f"for the character packet, author margin, and enclosing metadata. Increase --max-characters to {minimum_budget} or higher."
        )
    value = dramatic_packet(max_characters, minimum_budget)
    if _size(value) > max_characters:
        raise UsageError("dramatic-irony packet exceeds budget")
    return value
