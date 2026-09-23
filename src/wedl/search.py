from __future__ import annotations

from collections import OrderedDict
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import re
import sqlite3
from typing import Any, Iterable

import numpy as np

from .audience import section_audience, sections
from .conversation import beat_kind, conversation_end, conversation_participant_at, conversation_start, remembered_quotes, scene_end, scene_start, turn_time
from .errors import UsageError
from .generational import GENERATIONAL_KINDS
from .lexical import fts_query_parts
from .model import Record, StoryTime, World
from .profiles import CompilationProfile
from .thread_filter import ThreadFilter, filter_ranked_candidates
from .vectors import embed_query, model_from_row

STOPWORDS = {
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "from", "had", "has", "have",
    "he", "her", "hers", "him", "his", "how", "i", "in", "is", "it", "its", "me", "my", "of", "on",
    "or", "our", "she", "that", "the", "their", "them", "they", "this", "to", "was", "we", "what",
    "when", "where", "which", "who", "why", "with", "you", "your",
}

_VECTOR_MATRIX_CACHE: "OrderedDict[tuple[str, int, int, str], tuple[np.ndarray, dict[int, int]]]" = OrderedDict()
_VECTOR_MATRIX_CACHE_LIMIT = 8
_QUERY_VECTOR_CACHE: "OrderedDict[tuple[str, str], np.ndarray]" = OrderedDict()
_QUERY_VECTOR_CACHE_LIMIT = 128
_SEARCH_STATE_CACHE: "OrderedDict[tuple[str, int, int], dict[str, Any]]" = OrderedDict()
_SEARCH_STATE_CACHE_LIMIT = 16


def _database_identity(connection: sqlite3.Connection) -> tuple[str, int, int]:
    row = connection.execute("PRAGMA database_list").fetchone()
    raw_path = str(row[2]) if row and len(row) > 2 else ""
    if not raw_path:
        return (f"connection:{id(connection)}", 0, 0)
    path = Path(raw_path)
    try:
        stat = path.stat()
        return (str(path.resolve()), stat.st_mtime_ns, stat.st_size)
    except OSError:
        return (str(path), 0, 0)


def _vector_matrix(
    connection: sqlite3.Connection,
    model_id: str,
    dimensions: int,
) -> tuple[np.ndarray, dict[int, int]]:
    identity = _database_identity(connection)
    key = (*identity, model_id)
    cached = _VECTOR_MATRIX_CACHE.get(key)
    if cached is not None:
        _VECTOR_MATRIX_CACHE.move_to_end(key)
        return cached
    rows = connection.execute(
        "SELECT vector_id,vector,dimensions,norm FROM vector_embedding "
        "WHERE model_id=? ORDER BY vector_id",
        (model_id,),
    ).fetchall()
    vector_ids: list[int] = []
    blobs: list[bytes] = []
    for row in rows:
        row_dimensions = int(row[2])
        if row_dimensions != dimensions:
            raise UsageError(
                f"Stored vector {row[0]} has {row_dimensions} dimensions; model declares {dimensions}"
            )
        norm = float(row[3])
        if norm > 1e-12 and abs(norm - 1.0) > 1e-3:
            raise UsageError(f"Stored vector {row[0]} is not L2-normalized")
        blob = bytes(row[1])
        if len(blob) != dimensions * 4:
            raise UsageError(f"Stored vector {row[0]} has an invalid Float32 BLOB length")
        vector_ids.append(int(row[0]))
        blobs.append(blob)
    if blobs:
        matrix = np.frombuffer(b"".join(blobs), dtype="<f4").reshape(len(blobs), dimensions)
        matrix = np.asarray(matrix, dtype=np.float32)
    else:
        matrix = np.empty((0, dimensions), dtype=np.float32)
    positions = {vector_id: index for index, vector_id in enumerate(vector_ids)}
    value = (matrix, positions)
    _VECTOR_MATRIX_CACHE[key] = value
    _VECTOR_MATRIX_CACHE.move_to_end(key)
    while len(_VECTOR_MATRIX_CACHE) > _VECTOR_MATRIX_CACHE_LIMIT:
        _VECTOR_MATRIX_CACHE.popitem(last=False)
    return value


def _query_vector(model: Any, query: str) -> np.ndarray:
    key = (str(model.model_id), query)
    cached = _QUERY_VECTOR_CACHE.get(key)
    if cached is not None:
        _QUERY_VECTOR_CACHE.move_to_end(key)
        return cached
    vector = np.asarray(embed_query(model, query), dtype=np.float32)
    _QUERY_VECTOR_CACHE[key] = vector
    _QUERY_VECTOR_CACHE.move_to_end(key)
    while len(_QUERY_VECTOR_CACHE) > _QUERY_VECTOR_CACHE_LIMIT:
        _QUERY_VECTOR_CACHE.popitem(last=False)
    return vector


@dataclass(slots=True)
class SearchDocument:
    document_id: str
    entity_id: str
    document_kind: str
    heading: str | None
    audience_kind: str
    audience_character_id: str | None
    scene_id: str | None
    valid_from: StoryTime | None
    valid_until: StoryTime | None
    text: str
    metadata: dict[str, Any]
    chunk_hash: str

    @property
    def vector_text(self) -> str:
        aliases = ", ".join(str(value) for value in self.metadata.get("aliases") or [])
        tags = ", ".join(str(value) for value in self.metadata.get("tags") or [])
        return "\n".join(
            value
            for value in (
                str(self.metadata.get("title") or ""),
                aliases,
                str(self.heading or ""),
                str(self.metadata.get("domain") or ""),
                tags,
                self.text,
            )
            if value
        )


def _chunks(text: str, max_chars: int = 900, overlap: int = 80) -> list[str]:
    text = text.strip()
    if not text:
        return []
    if len(text) <= max_chars:
        return [text]
    paragraphs = [value.strip() for value in re.split(r"\n\s*\n", text) if value.strip()]
    result: list[str] = []
    current = ""
    for paragraph in paragraphs:
        candidate = f"{current}\n\n{paragraph}".strip() if current else paragraph
        if len(candidate) <= max_chars:
            current = candidate
            continue
        if current:
            result.append(current)
        if len(paragraph) <= max_chars:
            current = paragraph
            continue
        start = 0
        while start < len(paragraph):
            end = min(len(paragraph), start + max_chars)
            result.append(paragraph[start:end])
            if end == len(paragraph):
                current = ""
                break
            start = max(start + 1, end - overlap)
    if current:
        result.append(current)
    return result


def _doc(
    record: Record,
    kind: str,
    index: int,
    text: str,
    *,
    heading: str | None = None,
    audience: str = "author",
    character: str | None = None,
    scene: str | None = None,
    start: StoryTime | None = None,
    end: StoryTime | None = None,
    metadata: dict[str, Any] | None = None,
) -> SearchDocument:
    digest = hashlib.sha256(
        "\0".join([
            record.id, kind, str(index), heading or "", audience, character or "", scene or "",
            json.dumps(start.to_dict() if start else None, sort_keys=True),
            json.dumps(end.to_dict() if end else None, sort_keys=True), text,
        ]).encode()
    ).hexdigest()
    return SearchDocument(
        f"doc:{record.id}:{kind}:{index}:{digest[:12]}", record.id, kind, heading, audience, character,
        scene, start, end, text, metadata or {}, digest,
    )


def _metadata(record: Record) -> dict[str, Any]:
    return {
        "title": record.title,
        "kind": record.kind,
        "aliases": record.aliases,
        "domain": record.domain,
        "tags": record.tags,
    }


def _owner(record: Record) -> str | None:
    if record.kind == "character":
        return record.id
    if record.kind == "knowledge":
        return str(record.frontmatter.get("knower") or "") or None
    return None


def _transition_times(record: Record, world: World) -> list[StoryTime]:
    """Return every explicitly timed state transition on a record.

    Transition lists currently occur both directly on records (knowledge and
    relationship records) and under a story point lifecycle.  Keeping this
    extraction here makes author search documents safe to filter before either
    retrieval lane ranks them.
    """
    values: list[dict[str, Any]] = []
    direct = record.frontmatter.get("transitions") or []
    lifecycle = record.frontmatter.get("lifecycle") or {}
    nested = lifecycle.get("transitions") if isinstance(lifecycle, dict) else []
    for item in [*direct, *(nested or [])]:
        if isinstance(item, dict) and item.get("time"):
            values.append(item)
    return [StoryTime.from_value(item["time"], world.default_timeline) for item in values]


def _record_temporal_bounds(record: Record, world: World) -> tuple[StoryTime | None, StoryTime | None]:
    """Return the interval in which a record's authored prose is true.

    Entity identity/metadata remains timeless.  Prose that describes an event,
    conversation, knowledge claim, or transition may not enter an author
    as-of corpus before its authored story time.
    """
    if record.kind == "event" and record.frontmatter.get("time"):
        return StoryTime.from_value(record.frontmatter["time"], world.default_timeline), None
    if record.kind == "conversation":
        # A conversation body is an aggregate summary.  It is not safe before
        # the conversation closes; live truth is supplied by bounded turns.
        return conversation_end(record, world.default_timeline), None
    if record.kind == "scene":
        # A scene body can summarize observations/constraints authored across
        # its run, so only a closed scene's completed aggregate is searchable.
        return scene_end(record, world.default_timeline), None
    if record.kind == "environment":
        start = (record.frontmatter.get("time") or {}).get("start")
        end = (record.frontmatter.get("time") or {}).get("end")
        return (
            StoryTime.from_value(start, world.default_timeline) if start else None,
            StoryTime.from_value(end, world.default_timeline) if end else None,
        )
    transition_times = _transition_times(record, world)
    return (
        min(transition_times, key=lambda point: (point.timeline, point.tick, point.order)),
        None,
    ) if transition_times else (None, None)


def _author_document(
    record: Record,
    index: int,
    text: str,
    world: World,
    *,
    kind: str = "entity-author",
    heading: str | None = None,
    start: StoryTime | None = None,
    end: StoryTime | None = None,
    metadata: dict[str, Any] | None = None,
) -> SearchDocument:
    record_start, record_end = _record_temporal_bounds(record, world)
    return _doc(
        record,
        kind,
        index,
        text,
        heading=heading,
        start=start if start is not None else record_start,
        end=end if end is not None else record_end,
        metadata=metadata or _metadata(record),
    )


def build_documents(world: World) -> list[SearchDocument]:
    result: list[SearchDocument] = []
    character_titles = {record.id: record.title for record in world.by_kind("character")}
    for record in sorted(world.records.values(), key=lambda value: value.id):
        # Hypotheses never enter the compiled FTS corpus or author vector
        # model.  Otherwise an unrelated possibility can change BM25 document
        # frequencies or LSA training and reorder canonical retrieval.  Their
        # explicit author opt-in path uses hypothesis_source_search below.
        if record.kind == "hypothesis":
            continue
        # Generational metadata and prose may describe future or withheld
        # history. Its private projection is filtered by the query owner
        # before ranking; generic FTS and vectors have no viewer policy for it.
        if record.kind in GENERATIONAL_KINDS:
            continue
        # Identity is intentionally timeless.  Keep it separate from prose so
        # an as-of author query can still resolve a known record without a
        # future event/turn/transition aggregate entering its ranking corpus.
        metadata_text = "\n".join(
            value for value in (
                record.title,
                f"Kind: {record.kind}",
                f"Domain: {record.domain}" if record.domain else "",
                f"Aliases: {', '.join(record.aliases)}" if record.aliases else "",
                f"Tags: {', '.join(record.tags)}" if record.tags else "",
            ) if value
        )
        result.append(_doc(record, "entity-author-metadata", 0, metadata_text, metadata=_metadata(record)))
        # The text intentionally excludes frontmatter aggregates: some of
        # those aggregates contain later turns/transitions.  Dynamic material
        # is indexed below as independently bounded documents instead.
        author_text = record.body
        aggregate_available = not (
            record.kind in {"conversation", "scene"}
            and _record_temporal_bounds(record, world)[0] is None
        )
        if aggregate_available:
            for index, chunk in enumerate(_chunks(author_text)):
                result.append(_author_document(record, index, chunk, world))

        if record.kind == "environment":
            interval_start, interval_end = _record_temporal_bounds(record, world)
            conditions = record.frontmatter.get("conditions") or {}
            sensory = record.frontmatter.get("sensory") or []
            detail = "\n".join(
                value for value in (
                    record.title,
                    "Conditions: " + "; ".join(
                        f"{key}: {conditions[key]}" for key in sorted(conditions)
                    ) if isinstance(conditions, dict) and conditions else "",
                    "Sensory: " + " ".join(str(value) for value in sensory) if isinstance(sensory, list) and sensory else "",
                ) if value
            )
            if detail:
                result.append(_author_document(record, 0, detail, world, kind="environment", heading="environment", start=interval_start, end=interval_end))

        for index, transition in enumerate(record.frontmatter.get("transitions") or []):
            if not isinstance(transition, dict) or not transition.get("time"):
                continue
            point = StoryTime.from_value(transition["time"], world.default_timeline)
            text = "\n".join(
                value for value in (
                    record.title,
                    f"Transition: {transition.get('state') or transition.get('relationship_status') or transition.get('key') or transition.get('id') or 'state change'}",
                    "Metrics: " + ", ".join(
                        f"{key}={transition['metrics'][key]}" for key in sorted(transition["metrics"])
                    ) if isinstance(transition.get("metrics"), dict) else "",
                    "Facets: " + ", ".join(str(value) for value in (transition.get("facets") or [])) if isinstance(transition.get("facets"), list) else "",
                    str(transition.get("note") or transition.get("statement") or ""),
                ) if value
            )
            result.append(_author_document(record, index, text, world, kind="transition", heading="transition", start=point))
        lifecycle = record.frontmatter.get("lifecycle") or {}
        for index, transition in enumerate(lifecycle.get("transitions") or [] if isinstance(lifecycle, dict) else [], start=10_000):
            if not isinstance(transition, dict) or not transition.get("time"):
                continue
            point = StoryTime.from_value(transition["time"], world.default_timeline)
            text = "\n".join(
                value for value in (
                    record.title,
                    f"Transition: {transition.get('state') or transition.get('id') or 'state change'}",
                    str(transition.get("note") or ""),
                ) if value
            )
            result.append(_author_document(record, index, text, world, kind="transition", heading="transition", start=point))

        if record.kind == "event" and record.frontmatter.get("time"):
            point = StoryTime.from_value(record.frontmatter["time"], world.default_timeline)
            for index, effect in enumerate(record.frontmatter.get("effects") or []):
                if not isinstance(effect, dict):
                    continue
                value = effect.get("value")
                rendered_value = json.dumps(value, ensure_ascii=False, sort_keys=True) if isinstance(value, (dict, list)) else str(value)
                text = "\n".join(
                    value for value in (
                        record.title,
                        "Event effect",
                        f"Target: {effect.get('target')}",
                        f"Key: {effect.get('key')}",
                        f"Operation: {effect.get('operation')}",
                        f"Value: {rendered_value}",
                    ) if value and value != "None"
                )
                result.append(_author_document(record, index, text, world, kind="event-effect", heading="effect", start=point, metadata={**_metadata(record), "effectId": effect.get("id")}))

        if record.status in {"canonical", "active", "closed"} and record.kind != "knowledge":
            section_index = 0
            for section in sections(record.body):
                audience = set(section_audience(record, section))
                for chunk in _chunks(section.text):
                    metadata = _metadata(record)
                    if "public" in audience:
                        result.append(_doc(record, "entity-public", section_index, chunk, heading=section.heading, audience="public", metadata=metadata))
                    if "self" in audience and (owner := _owner(record)):
                        result.append(_doc(record, "entity-self", section_index, chunk, heading=section.heading, audience="character", character=owner, metadata=metadata))
                    for token in audience:
                        target = token[5:] if token.startswith("char:") else token
                        if target.startswith("char_"):
                            result.append(_doc(record, "entity-private", section_index, chunk, heading=section.heading, audience="character", character=target, metadata=metadata))
                    section_index += 1

        if record.kind == "knowledge" and record.status == "canonical":
            claim = record.frontmatter.get("claim") or {}
            transitions = [item for item in record.frontmatter.get("transitions") or [] if isinstance(item, dict) and item.get("time")]
            start = min(
                (StoryTime.from_value(item["time"], world.default_timeline) for item in transitions),
                default=None,
                key=lambda point: (point.timeline, point.tick, point.order),
            )
            statement = str(claim.get("statement") or record.title)
            result.append(_author_document(record, 0, statement, world, kind="knowledge-claim", heading="claim", start=start))
            result.append(_doc(record, "knowledge", 0, statement, audience="knowledge", character=str(record.frontmatter.get("knower")), start=start, metadata={**_metadata(record), "claimKey": claim.get("key")}))

        if record.kind == "scene" and record.status in {"active", "closed"}:
            for index, observation in enumerate(record.frontmatter.get("observations") or []):
                if not isinstance(observation, dict) or not observation.get("text") or not observation.get("at"):
                    continue
                start = StoryTime.from_value(observation["at"], world.default_timeline)
                end = StoryTime.from_value(observation["until"], world.default_timeline) if observation.get("until") else None
                audience = observation.get("audience") or ["participants"]
                if isinstance(audience, str): audience = [audience]
                result.append(_author_document(record, index, str(observation["text"]), world, kind="scene-observation", heading="observation", start=start, end=end, metadata={**_metadata(record), "observationId": observation.get("id")}))
                targets: set[str] = set()
                if "participants" in audience:
                    targets.update(str(item.get("character")) for item in record.frontmatter.get("participants") or [] if isinstance(item, dict) and item.get("character"))
                for token in audience:
                    target = token[5:] if isinstance(token, str) and token.startswith("char:") else token
                    if isinstance(target, str) and target.startswith("char_"):
                        targets.add(target)
                for target in targets:
                    result.append(_doc(record, "scene-observation", index, str(observation["text"]), heading="observation", audience="scene", character=target, scene=record.id, start=start, end=end, metadata={**_metadata(record), "observationId": observation.get("id")}))

        if record.kind == "conversation" and record.status in {"active", "closed"}:
            participants = {str(item.get("character")) for item in record.frontmatter.get("participants") or [] if isinstance(item, dict) and item.get("character")}
            for index, turn in enumerate(record.frontmatter.get("turns") or []):
                if not isinstance(turn, dict) or not turn.get("text"):
                    continue
                point = turn_time(turn, record, world.default_timeline)
                audience = turn.get("audience") or ["participants"]
                if isinstance(audience, str): audience = [audience]
                targets: set[str] = set()
                if "participants" in audience:
                    targets.update(target for target in participants if conversation_participant_at(record, target, point, world.default_timeline))
                for token in audience:
                    target = token[5:] if isinstance(token, str) and token.startswith("char:") else token
                    if isinstance(target, str) and target.startswith("char_"):
                        targets.add(target)
                kind = beat_kind(turn)
                speaker = str(turn.get("speaker") or "")
                if kind == "action":
                    actors = [actor[5:] if isinstance(actor, str) and actor.startswith("char:") else actor for actor in turn.get("actors") or []]
                    actor_titles = ", ".join(character_titles.get(str(actor), str(actor)) for actor in actors)
                    text = f"Action — {actor_titles}: {turn.get('text', '')}"
                    document_kind, heading, fidelity = "conversation-action", "action-beat", "canonical"
                    metadata = {**_metadata(record), "turnId": turn.get("id"), "actors": actor_titles, "fidelity": fidelity}
                else:
                    text = f'{character_titles.get(speaker, speaker)}: “{turn.get("text", "")}”'
                    document_kind, heading, fidelity = "conversation-turn", "verbatim-turn", "verbatim"
                    metadata = {**_metadata(record), "turnId": turn.get("id"), "speaker": character_titles.get(speaker, speaker), "fidelity": fidelity}
                result.append(_author_document(record, index, text, world, kind=document_kind, heading=heading, start=point, metadata=metadata))
                for target in targets:
                    result.append(_doc(record, document_kind, index, text, heading=heading, audience="conversation", character=target, scene=str(record.frontmatter.get("scene") or "") or None, start=point, metadata=metadata))
            for index, recollection in enumerate(record.frontmatter.get("recollections") or []):
                if not isinstance(recollection, dict) or not recollection.get("character") or not recollection.get("at") or recollection.get("state") == "forgotten":
                    continue
                point = StoryTime.from_value(recollection["at"], world.default_timeline)
                parts = [str(recollection.get("summary") or "")]
                if recollection.get("interpretation"):
                    parts.append(f"Interpretation: {recollection['interpretation']}")
                if recollection.get("emotional_impression"):
                    parts.append(f"Emotional impression: {recollection['emotional_impression']}")
                for quote in remembered_quotes(record, recollection):
                    parts.append(f'{character_titles.get(str(quote.get("speaker")), quote.get("speaker"))}: “{quote.get("text", "")}” ({quote.get("fidelity")})')
                recollection_text = "\n".join(part for part in parts if part)
                result.append(_author_document(record, index, recollection_text, world, kind="conversation-recollection", heading="subjective-recollection", start=point, metadata={**_metadata(record), "recollectionId": recollection.get("id"), "state": recollection.get("state"), "confidence": recollection.get("confidence"), "fidelity": "subjective"}))
                result.append(_doc(record, "conversation-recollection", index, recollection_text, heading="subjective-recollection", audience="conversation", character=str(recollection["character"]), start=point, metadata={**_metadata(record), "recollectionId": recollection.get("id"), "state": recollection.get("state"), "confidence": recollection.get("confidence"), "fidelity": "subjective"}))

    unique: dict[str, SearchDocument] = {}
    for document in result:
        unique.setdefault(document.document_id, document)
    return [unique[key] for key in sorted(unique)]


def hypothesis_source_search(world: World, query: str, *, limit: int) -> list[dict[str, Any]]:
    """Author-only lexical search outside the shared compiled corpus.

    It intentionally has no vector/model dependency: authoring a possibility
    must not alter canonical FTS statistics, vector embeddings, or model hash.
    """
    phrases, terms = fts_query_parts(query)
    if not phrases and not terms:
        return []
    ranked: list[tuple[int, Record, str]] = []
    for record in world.by_kind("hypothesis"):
        text = "\n".join(value for value in (
            record.title, str(record.frontmatter.get("statement") or ""),
            " ".join(str(value) for value in record.frontmatter.get("alternatives") or []),
            str(record.frontmatter.get("context") or ""), record.body,
        ) if value)
        folded = text.casefold()
        phrase_hits = sum(folded.count(phrase) * 4 for phrase in phrases)
        term_hits = sum(len(re.findall(rf"(?<!\w){re.escape(term)}(?!\w)", folded)) for term in terms)
        if phrase_hits + term_hits:
            ranked.append((phrase_hits + term_hits, record, text))
    ranked.sort(key=lambda item: (-item[0], item[1].id))
    return [
        {
            "documentId": f"hypothesis:{record.id}", "entityId": record.id,
            "documentKind": "hypothesis", "heading": "possibility",
            "audienceKind": "author", "text": text,
            "metadata": {"title": record.title, "nonCanonical": True, "hypothesisStatus": record.status},
            "score": float(score), "lanes": ["hypothesis-source"], "snippet": text[:300],
        }
        for score, record, text in ranked[:limit]
    ]



def _time_clause(at: StoryTime | None) -> tuple[str, list[Any]]:
    if at is None:
        return "1=1", []
    return (
        "(d.timeline IS NULL OR (d.timeline=? AND "
        "(d.from_tick IS NULL OR d.from_tick<? OR (d.from_tick=? AND COALESCE(d.from_order,0)<=?)) "
        "AND (d.until_tick IS NULL OR d.until_tick>? OR "
        "(d.until_tick=? AND COALESCE(d.until_order,2147483647)>=?))))",
        [at.timeline, at.tick, at.tick, at.order, at.tick, at.tick, at.order],
    )


def allowed_clause(
    perspective: str,
    character_id: str | None,
    scene_id: str | None,
    at: StoryTime | None,
    accessible_entities: Iterable[str],
    active_knowledge: Iterable[str],
) -> tuple[str, list[Any]]:
    if perspective == "author":
        time_sql, time_params = _time_clause(at)
        return f"d.audience_kind='author' AND ({time_sql})", time_params
    if not character_id:
        return "0=1", []
    clauses = ["(d.audience_kind IN ('character','conversation') AND d.audience_character_id=?)"]
    params: list[Any] = [character_id]
    if scene_id:
        clauses.append("(d.audience_kind='scene' AND d.scene_id=? AND d.audience_character_id=?)")
        params.extend([scene_id, character_id])
    knowledge_ids = sorted(set(active_knowledge))
    if knowledge_ids:
        placeholders = ",".join("?" for _ in knowledge_ids)
        clauses.append(
            f"(d.audience_kind='knowledge' AND d.audience_character_id=? "
            f"AND d.entity_id IN ({placeholders}))"
        )
        params.extend([character_id, *knowledge_ids])
    entity_ids = sorted(set(accessible_entities))
    if entity_ids:
        placeholders = ",".join("?" for _ in entity_ids)
        clauses.append(f"(d.audience_kind='public' AND d.entity_id IN ({placeholders}))")
        params.extend(entity_ids)
    time_sql, time_params = _time_clause(at)
    return f"({' OR '.join(clauses)}) AND ({time_sql})", [*params, *time_params]


def _fts_expression(query: str) -> str | None:
    phrases, terms = fts_query_parts(query)
    values: list[str] = []
    for phrase in phrases[:6]:
        values.append(f'"{phrase.replace(chr(34), chr(34) * 2)}"')
    for term in terms[:20]:
        escaped = term.replace(chr(34), chr(34) * 2)
        values.append(f'"{escaped}"')
    return " OR ".join(values) if values else None


def _profile_row(connection: sqlite3.Connection) -> dict[str, Any]:
    row = connection.execute("SELECT * FROM revision LIMIT 1").fetchone()
    if row is None:
        raise UsageError("Compiled database has no revision metadata")
    return dict(row)


def _compiled_profile(connection: sqlite3.Connection) -> CompilationProfile:
    row = _profile_row(connection)
    payload = json.loads(row.get("profile_json") or "{}")
    try:
        return CompilationProfile(
            name=str(payload["name"]),
            fts_enabled=bool(payload["ftsEnabled"]),
            vector_enabled=bool(payload["vectorEnabled"]),
            vector_provider=payload.get("vectorProvider"),
            vector_model=payload.get("vectorModel"),
            vector_dimensions=int(payload.get("vectorDimensions") or 0),
            vector_max_features=int(payload.get("vectorMaxFeatures") or 0),
            fts_candidate_limit=int(payload.get("ftsCandidateLimit") or 120),
            vector_candidate_limit=int(payload.get("vectorCandidateLimit") or 120),
            hybrid_fts_weight=float(payload.get("hybridFtsWeight") or 1.0),
            hybrid_vector_weight=float(payload.get("hybridVectorWeight") or 1.0),
            hybrid_rrf_k=float(payload.get("hybridRrfK") or 60.0),
        )
    except (KeyError, TypeError, ValueError) as exc:
        raise UsageError("Compiled database has invalid search-profile metadata") from exc


def search_state(connection: sqlite3.Connection) -> dict[str, Any]:
    identity = _database_identity(connection)
    cached = _SEARCH_STATE_CACHE.get(identity)
    if cached is not None:
        _SEARCH_STATE_CACHE.move_to_end(identity)
        return dict(cached)
    profile = _compiled_profile(connection)
    model_rows = connection.execute("SELECT * FROM vector_model ORDER BY scope").fetchall()
    vector_models = [
        {
            "scope": row["scope"],
            "modelId": row["model_id"],
            "provider": row["provider"],
            "modelName": row["model_name"],
            "dimensions": int(row["dimensions"]),
            "normalized": bool(row["normalized"]),
        }
        for row in model_rows
    ]
    value = {
        "profile": profile.as_dict(),
        "fts": {
            "available": profile.fts_enabled,
            "documentCount": int(
                connection.execute("SELECT COUNT(*) FROM search_fts").fetchone()[0]
            ) if profile.fts_enabled else 0,
            "provider": "sqlite-fts5" if profile.fts_enabled else None,
        },
        "vector": {
            "available": profile.vector_enabled and bool(vector_models),
            "models": vector_models,
            "uniqueVectors": int(connection.execute("SELECT COUNT(*) FROM vector_embedding").fetchone()[0]),
            "documentLinks": int(connection.execute("SELECT COUNT(*) FROM document_vector").fetchone()[0]),
        },
    }
    _SEARCH_STATE_CACHE[identity] = value
    _SEARCH_STATE_CACHE.move_to_end(identity)
    while len(_SEARCH_STATE_CACHE) > _SEARCH_STATE_CACHE_LIMIT:
        _SEARCH_STATE_CACHE.popitem(last=False)
    return dict(value)


def _require_mode(profile: CompilationProfile, mode: str) -> None:
    if mode not in {"fts", "vector", "hybrid"}:
        raise UsageError(f"Unsupported search mode {mode!r}")
    if mode in {"fts", "hybrid"} and not profile.fts_enabled:
        raise UsageError(
            f"Search mode {mode!r} requires an FTS or hybrid compilation profile; "
            f"database was compiled with {profile.name!r}"
        )
    if mode in {"vector", "hybrid"} and not profile.vector_enabled:
        raise UsageError(
            f"Search mode {mode!r} requires a vector or hybrid compilation profile; "
            f"database was compiled with {profile.name!r}"
        )


def _fts_search(
    connection: sqlite3.Connection,
    query: str,
    *,
    clause: str,
    parameters: list[Any],
    limit: int,
) -> list[dict[str, Any]]:
    expression = _fts_expression(query)
    if not expression:
        return []
    rows = connection.execute(
        f"""
        WITH allowed(rowid) AS (SELECT d.rowid FROM search_document d WHERE {clause})
        SELECT d.document_id,d.entity_id,d.document_kind,d.heading,d.audience_kind,
               d.text,d.metadata_json,
               bm25(search_fts,10.0,6.0,4.0,1.0,2.0,2.0) AS raw_rank,
               snippet(search_fts,3,'<mark>','</mark>',' … ',24) AS snippet
        FROM allowed a
        JOIN search_fts ON search_fts.rowid=a.rowid
        JOIN search_document d ON d.rowid=a.rowid
        WHERE search_fts MATCH ?
        ORDER BY raw_rank ASC,d.document_id ASC
        LIMIT ?
        """,
        [*parameters, expression, limit],
    ).fetchall()
    result: list[dict[str, Any]] = []
    for index, row in enumerate(rows):
        raw_rank = float(row[7])
        result.append(
            {
                "documentId": row[0],
                "entityId": row[1],
                "documentKind": row[2],
                "heading": row[3],
                "audienceKind": row[4],
                "text": row[5],
                "metadata": json.loads(row[6]),
                "ftsRank": index + 1,
                "ftsBm25": raw_rank,
                "ftsScore": -raw_rank,
                "snippet": row[8] or row[5][:300],
            }
        )
    return result


def _vector_search(
    connection: sqlite3.Connection,
    query: str,
    *,
    scope: str,
    clause: str,
    parameters: list[Any],
    limit: int,
) -> list[dict[str, Any]]:
    model_row = connection.execute(
        "SELECT * FROM vector_model WHERE scope=?", (scope,)
    ).fetchone()
    if model_row is None:
        raise UsageError(f"Compiled profile has no {scope!r} vector model")
    model = model_from_row(model_row)
    if not model.normalized:
        raise UsageError("Compiled vector model is not normalized")
    query_vector = _query_vector(model, query)
    if float(query_vector @ query_vector) <= 1e-12:
        return []

    # Authorization and fictional-time filtering occur before ranking. Fetch
    # only unique vector IDs and their authorized document counts first; full
    # snippets/metadata are loaded only for the winning vectors.
    vector_rows = connection.execute(
        f"""
        SELECT dv.vector_id,COUNT(*) AS document_count
        FROM search_document d
        JOIN document_vector dv ON dv.document_id=d.document_id
        JOIN vector_embedding v ON v.vector_id=dv.vector_id
        WHERE {clause} AND v.model_id=?
        GROUP BY dv.vector_id
        """,
        [*parameters, model.model_id],
    ).fetchall()
    if not vector_rows:
        return []
    matrix, positions = _vector_matrix(connection, model.model_id, model.dimensions)
    eligible = [
        (int(row[0]), int(row[1]))
        for row in vector_rows
        if int(row[0]) in positions
    ]
    if not eligible:
        return []
    indexes = np.fromiter((positions[vector_id] for vector_id, _count in eligible), dtype=np.int64)
    scores_array = np.asarray(matrix[indexes] @ query_vector, dtype=np.float32)
    scored_vectors = sorted(
        (
            (float(scores_array[index]), vector_id, document_count)
            for index, (vector_id, document_count) in enumerate(eligible)
        ),
        key=lambda item: (-item[0], item[1]),
    )
    selected_vector_ids: list[int] = []
    selected_documents = 0
    cutoff_score: float | None = None
    for score, vector_id, document_count in scored_vectors:
        if selected_documents >= limit and cutoff_score is not None and score < cutoff_score:
            break
        selected_vector_ids.append(vector_id)
        selected_documents += document_count
        if selected_documents >= limit and cutoff_score is None:
            cutoff_score = score
    if not selected_vector_ids:
        return []
    score_by_id = {vector_id: score for score, vector_id, _count in scored_vectors}
    details: list[Any] = []
    for start in range(0, len(selected_vector_ids), 400):
        batch = selected_vector_ids[start : start + 400]
        placeholders = ",".join("?" for _ in batch)
        details.extend(
            connection.execute(
                f"""
                SELECT d.document_id,d.entity_id,d.document_kind,d.heading,d.audience_kind,
                       d.text,d.metadata_json,dv.vector_id
                FROM search_document d
                JOIN document_vector dv ON dv.document_id=d.document_id
                WHERE {clause} AND dv.vector_id IN ({placeholders})
                """,
                [*parameters, *batch],
            ).fetchall()
        )
    scored = [(score_by_id[int(row[7])], row) for row in details]
    scored.sort(key=lambda item: (-item[0], item[1][0]))
    return [
        {
            "documentId": row[0],
            "entityId": row[1],
            "documentKind": row[2],
            "heading": row[3],
            "audienceKind": row[4],
            "text": row[5],
            "metadata": json.loads(row[6]),
            "vectorRank": index + 1,
            "vectorScore": score,
            "snippet": row[5][:300],
        }
        for index, (score, row) in enumerate(scored[:limit])
    ]


def _deduplicate(ranked: list[dict[str, Any]], limit: int) -> list[dict[str, Any]]:
    counts: dict[str, int] = {}
    result: list[dict[str, Any]] = []
    for item in ranked:
        entity_id = str(item["entityId"])
        if counts.get(entity_id, 0) >= 2:
            continue
        counts[entity_id] = counts.get(entity_id, 0) + 1
        result.append(item)
        if len(result) >= limit:
            break
    return result


def search(
    connection: sqlite3.Connection,
    query: str,
    *,
    perspective: str,
    character_id: str | None,
    scene_id: str | None,
    at: StoryTime | None,
    accessible_entities: Iterable[str] = (),
    active_knowledge: Iterable[str] = (),
    mode: str = "hybrid",
    limit: int = 20,
    include_hypotheses: bool = False,
    _thread_filter: ThreadFilter | None = None,
) -> list[dict[str, Any]]:
    profile = _compiled_profile(connection)
    _require_mode(profile, mode)
    clause, parameters = allowed_clause(
        perspective, character_id, scene_id, at, accessible_entities, active_knowledge
    )
    if perspective == "author" and not include_hypotheses:
        clause = f"({clause}) AND d.document_kind != 'hypothesis'"
    fts_results: list[dict[str, Any]] = []
    vector_results: list[dict[str, Any]] = []
    if mode in {"fts", "hybrid"}:
        fts_results = _fts_search(
            connection,
            query,
            clause=clause,
            parameters=parameters,
            limit=max(limit, profile.fts_candidate_limit),
        )
    if mode in {"vector", "hybrid"}:
        # Vector retrieval is deliberately independent of the FTS candidate set.
        # This is what lets semantic similarity recover documents with little or
        # no lexical overlap.
        vector_results = _vector_search(
            connection,
            query,
            scope="author" if perspective == "author" else "character",
            clause=clause,
            parameters=parameters,
            limit=max(limit, profile.vector_candidate_limit),
        )
    if mode == "fts":
        ranked = [
            {**item, "score": 1.0 / (profile.hybrid_rrf_k + item["ftsRank"]), "lanes": ["fts"]}
            for item in fts_results
        ]
        return _finish_ranked_search(connection, ranked, limit, profile, _thread_filter)
    if mode == "vector":
        ranked = [
            {**item, "score": item["vectorScore"], "lanes": ["vector"]}
            for item in vector_results
        ]
        return _finish_ranked_search(connection, ranked, limit, profile, _thread_filter)

    combined: dict[str, dict[str, Any]] = {}
    for item in fts_results:
        entry = combined.setdefault(item["documentId"], dict(item))
        entry["score"] = float(entry.get("score", 0.0)) + (
            profile.hybrid_fts_weight / (profile.hybrid_rrf_k + item["ftsRank"])
        )
        entry.setdefault("lanes", []).append("fts")
    for item in vector_results:
        entry = combined.setdefault(item["documentId"], dict(item))
        entry.update({key: value for key, value in item.items() if key not in entry})
        entry["score"] = float(entry.get("score", 0.0)) + (
            profile.hybrid_vector_weight / (profile.hybrid_rrf_k + item["vectorRank"])
        )
        entry.setdefault("lanes", []).append("vector")
    ranked = sorted(
        combined.values(),
        key=lambda item: (-float(item["score"]), item["documentId"]),
    )
    return _finish_ranked_search(connection, ranked, limit, profile, _thread_filter)


def _finish_ranked_search(
    connection: sqlite3.Connection,
    ranked: list[dict[str, Any]],
    limit: int,
    profile: CompilationProfile,
    thread_filter: ThreadFilter | None,
) -> list[dict[str, Any]]:
    if thread_filter is None:
        return _deduplicate(ranked, limit)
    candidate_limit = max(limit, profile.fts_candidate_limit, profile.vector_candidate_limit)
    candidates = _deduplicate(ranked, candidate_limit)
    return filter_ranked_candidates(connection, candidates, thread_filter)[:limit]
