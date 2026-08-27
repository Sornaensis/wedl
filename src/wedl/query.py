from __future__ import annotations

import json
import math
from pathlib import Path
from typing import Any

from . import THREAD_SOURCE_SCHEMA
from .compiler import cache_readiness, connect, require_database
from .context import accessible_entities, build_context
from .conversation import beat_kind, character_scene_allowed, conversation_context_time, current_recollection, remembered_quotes, scene_contains_time, scene_context_time, turn_time, visible_beats, visible_turns
from .errors import NotFound, UsageError
from .model import ORDER_MAX, ORDER_MIN, TICK_MAX, TICK_MIN, StoryTime, World
from .repository import Repository
from .search import hypothesis_source_search, search, search_state
from .semantics import canonical_events, current_knowledge, evaluate_all_story_points, event_time, interactions, resolve_state
from .thread_filter import project_thread_memberships, resolve_membership_record_ids, resolve_thread_filter
from .util import slugify
from .validation import validate_world


def _implicit_active_scene(world: World, character_id: str | None = None) -> Any:
    """Keep omitted-scene reads deterministic in a multi-front world."""
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


def _default_scene_time(world: World, scene: Any) -> StoryTime:
    return world.current_time if scene is not None and scene.status == "active" and world.current_time is not None else scene_context_time(scene, world.default_timeline)


def time_model(world: World) -> dict[str, Any]:
    """Return the repository's explicit, non-calendar story-time contract.

    This stays in status rather than source schema output so clients can learn
    the effective contract without interpreting omitted optional origins.
    """
    declarations: list[dict[str, Any]] = []
    for value in world.config.get("timelines") or []:
        if not isinstance(value, dict):
            continue
        declaration = {"id": value.get("id"), "label": value.get("label")}
        if "origin" in value:
            declaration["origin"] = value["origin"]
        declarations.append(declaration)
    return {
        "coordinate": ["timeline", "tick", "order"],
        "tick": {"semantics": "unitless-ordinal", "minimum": TICK_MIN, "maximum": TICK_MAX},
        "order": {"semantics": "same-tick-order", "minimum": ORDER_MIN, "maximum": ORDER_MAX},
        "durationSemantics": "none",
        "intervalEndpoints": "inclusive",
        "origin": {"semantics": "descriptive", "setsLowerBound": False},
        "defaultTimeline": world.default_timeline,
        "timelineDeclarations": declarations,
    }


def status(repository: Repository) -> dict[str, Any]:
    world = repository.load_world("HEAD" if repository.is_git else "WORKTREE")
    readiness = cache_readiness(repository, world.revision)
    database = Path(readiness["database"])
    counts = {kind: len(world.by_kind(kind)) for kind in sorted({record.kind for record in world})}
    active = world.active_scenes()
    singleton = world.active_scene()
    current = world.current_time or (scene_context_time(singleton, world.default_timeline) if singleton else None)
    result = {
        "repositoryRoot": str(repository.root),
        "revision": world.revision,
        "treeOid": world.tree_oid,
        "recordCount": len(world.records),
        "counts": counts,
        "activeSceneId": singleton.id if singleton else None,
        "activeScenes": [{"id": scene.id, "title": scene.title} for scene in active],
        "currentTime": None if current is None else current.to_dict(),
        "managedDirty": repository.status(),
        "timeModel": time_model(world),
        "cacheReadiness": readiness,
    }
    if readiness["state"] == "ready":
        with connect(database, True) as connection:
            row = connection.execute("SELECT * FROM revision LIMIT 1").fetchone()
            if row:
                result["compiled"] = dict(row)
            result["compileMetrics"] = [dict(row) for row in connection.execute("SELECT * FROM compile_metric ORDER BY stage")]
    return result


def validation_report(repository: Repository) -> dict[str, Any]:
    """Return the source-validation report shared by CLI and HTTP reads."""

    world = repository.load_world()
    diagnostics = validate_world(world)
    return {
        "valid": not any(item["severity"] == "error" for item in diagnostics),
        "revision": world.revision,
        "recordCount": len(world.records),
        "diagnostics": diagnostics,
    }


def thread_catalog(repository: Repository, *, require_compiled: bool = False) -> dict[str, Any]:
    """Return the public narrative-group catalog without exposing membership.

    Threads are optional labels in one shared world.  This deliberately reads
    no record memberships or temporal/state/search data: callers receive only
    the declared identifiers and display labels.
    """

    world, _database = require_database(repository, require_compiled=require_compiled)
    grouping_available = world.schema == THREAD_SOURCE_SCHEMA
    threads = (
        [
            {"id": thread.id, "label": thread.label}
            for thread in sorted(world.threads, key=lambda thread: (thread.label.casefold(), thread.id))
        ]
        if grouping_available
        else []
    )
    return {
        "protocol": "wedl-threads/v1",
        "revision": world.revision,
        "sourceSchema": world.schema,
        "groupingAvailable": grouping_available,
        "threads": threads,
    }


def thread_memberships(
    repository: Repository,
    record_ids: tuple[str, ...],
    thread_ids: tuple[str, ...],
    *,
    require_compiled: bool = False,
) -> dict[str, Any]:
    """Project selected narrative-group membership for supplied public records."""

    world, database = require_database(repository, require_compiled=require_compiled)
    normalized_records = resolve_membership_record_ids(record_ids)
    if len(thread_ids) > 32:
        raise UsageError("thread membership projection accepts at most 32 thread ids")
    with connect(database, True) as connection:
        thread_filter = resolve_thread_filter(connection, thread_ids)
        assert thread_filter is not None
        records = project_thread_memberships(connection, normalized_records, thread_filter)
    return {
        "protocol": "wedl-thread-memberships/v1",
        "revision": world.revision,
        "sourceSchema": world.schema,
        "selectedThreadIds": list(thread_filter.thread_ids),
        "records": records,
    }


def list_entities(repository: Repository, kind: str | None = None, text: str | None = None, *, require_compiled: bool = False) -> list[dict[str, Any]]:
    world, database = require_database(repository, require_compiled=require_compiled)
    if kind == "hypothesis":
        raise UsageError("hypotheses are author-only possibilities; use `wedl hypotheses` or GET /api/hypotheses")
    clauses = []
    params: list[Any] = []
    if kind:
        clauses.append("kind=?"); params.append(kind)
    else:
        # Possibilities are intentionally opt-in.  They remain directly
        # addressable for the read-only compendium but never join the default
        # canonical lore catalogue.
        clauses.append("kind != 'hypothesis'")
    if text:
        folded = text.casefold()
        matching_ids = [
            record.id
            for record in world
            if (kind is None or record.kind == kind)
            and any(
                folded in reference.casefold() or folded in slugify(reference).casefold()
                for reference in (record.id, record.title, *record.aliases)
            )
        ]
        if not matching_ids:
            return []
        clauses.append(f"id IN ({','.join('?' for _ in matching_ids)})")
        params.extend(matching_ids)
    where = " WHERE " + " AND ".join(clauses) if clauses else ""
    with connect(database, True) as connection:
        return [dict(row) for row in connection.execute(f"SELECT id,kind,title,domain,status,source_path FROM entity{where} ORDER BY kind,lower(title)", params)]


def _hypothesis_reference(world: World, value: Any, kind: str | None = None) -> dict[str, str] | None:
    if not isinstance(value, str):
        return None
    record = world.maybe_get(value)
    if record is None or (kind is not None and record.kind != kind):
        return None
    # Possibilities are author-facing notes.  Unlike ordinary entity links,
    # these projections are not navigation handles and deliberately carry no
    # opaque IDs.  Name-first reads remain usable through title/alias/slug.
    return {"kind": record.kind, "title": record.title}


def _hypothesis_projection(world: World, record: Any) -> dict[str, Any]:
    """Return author-facing uncertainty without representing it as a fact."""
    placement = record.frontmatter.get("placement") or {}
    result: dict[str, Any] = {
        "title": record.title, "status": record.status,
        "nonCanonical": True, "statement": record.frontmatter.get("statement"),
        "subjects": [item for item in (_hypothesis_reference(world, value) for value in record.frontmatter.get("subjects") or []) if item],
        "alternatives": list(record.frontmatter.get("alternatives") or []),
        "context": record.frontmatter.get("context"),
        "placement": {"context": placement.get("context")} if isinstance(placement, dict) and placement.get("context") else {},
    }
    if isinstance(placement, dict):
        if isinstance(placement.get("timeline"), str): result["placement"]["timeline"] = placement["timeline"]
        for key, kind in (("scene", "scene"), ("event", "event"), ("location", "location")):
            reference = _hypothesis_reference(world, placement.get(key), kind)
            if reference: result["placement"][key] = reference
    resolution = record.frontmatter.get("resolution")
    if isinstance(resolution, dict):
        result["resolution"] = {"note": resolution.get("note")}
        if record.status == "adopted":
            result["resolution"]["canonicalRecords"] = [item for item in (_hypothesis_reference(world, value) for value in resolution.get("canonical_entities") or []) if item]
    return result


def hypotheses(repository: Repository, hypothesis_id: str | None = None, *, status: str | None = None, text: str | None = None, require_compiled: bool = False) -> dict[str, Any]:
    """List or read author possibilities; never consult an author horizon."""
    world, _database = require_database(repository, require_compiled=require_compiled)
    if status is not None and status not in {"open", "adopted", "rejected"}:
        raise UsageError("hypothesis status must be open, adopted, or rejected")
    records = world.by_kind("hypothesis")
    if hypothesis_id:
        record = world.find(hypothesis_id, "hypothesis")
        return {"protocol": "wedl-hypotheses/v1", "revision": world.revision, "nonCanonical": True, "hypothesis": _hypothesis_projection(world, record)}
    folded = text.casefold() if text else ""
    records = [record for record in records if (status is None or record.status == status) and (not folded or folded in (record.title + "\n" + str(record.frontmatter.get("statement") or "") + "\n" + record.body).casefold())]
    return {"protocol": "wedl-hypotheses/v1", "revision": world.revision, "nonCanonical": True, "hypotheses": [_hypothesis_projection(world, record) for record in records]}


def show_entity(repository: Repository, entity_id: str, *, require_compiled: bool = False) -> dict[str, Any]:
    world, database = require_database(repository, require_compiled=require_compiled)
    record = world.find(entity_id)
    if record.kind == "hypothesis":
        raise UsageError("hypotheses are author-only possibilities; use `wedl hypotheses NAME` or GET /api/hypotheses")
    with connect(database, True) as connection:
        row = connection.execute("SELECT * FROM entity WHERE id=?", (record.id,)).fetchone()
    assert row is not None
    value = dict(row)
    # Detail frontmatter is read by browser-side horizon filtering.  StoryTime
    # ticks are signed64, so ordinary JSON numbers would silently collapse
    # adjacent beats above JavaScript's safe-integer range.  Match the timeline
    # endpoint's decimal-string transport for every authored time coordinate.
    value["frontmatter"] = _browser_safe_frontmatter(
        json.loads(value.pop("frontmatter_json")), world.default_timeline
    )
    value["bodyMarkdown"] = value.pop("body_markdown")
    if record.kind == "location":
        value["locationContext"] = _location_context(world, record)
    # entity_ref is populated only from authored frontmatter references and
    # explicit Markdown story links.  Do not manufacture related lore from
    # temporal overlap, shared locations, or other inferred associations.
    with connect(database, True) as connection:
        value["inboundReferences"] = [
            dict(reference)
            for reference in connection.execute(
                """
                SELECT source.id, source.kind, source.title
                FROM entity_ref AS ref
                JOIN entity AS source ON source.id = ref.source_id
                WHERE ref.target_id = ? AND source.kind != 'hypothesis'
                GROUP BY source.id, source.kind, source.title
                ORDER BY lower(source.title), source.id
                """,
                (record.id,),
            )
        ]
    return value


_STORY_TIME_FIELDS = frozenset({"at", "current", "end", "from", "start", "time", "to", "until"})


def _browser_safe_frontmatter(value: Any, default_timeline: str, *, time_field: bool = False) -> Any:
    """Copy detail frontmatter with exact decimal coordinates for browser use.

    The source model deliberately keeps StoryTime values as Python integers.
    Only objects reached through authored time fields are converted, leaving
    unrelated user metadata untouched.  This includes nested scene presence
    intervals and observation ``at``/``until`` bounds.
    """

    if isinstance(value, list):
        return [_browser_safe_frontmatter(item, default_timeline, time_field=time_field) for item in value]
    if not isinstance(value, dict):
        return value
    if time_field and "tick" in value:
        try:
            point = StoryTime.from_value(value, default_timeline)
        except (TypeError, ValueError):
            pass
        else:
            return _timeline_coordinate(point)
    return {
        key: _browser_safe_frontmatter(item, default_timeline, time_field=key in _STORY_TIME_FIELDS)
        for key, item in value.items()
    }


def _timeline_reference(world: World, value: Any, fallback_kind: str = "unknown") -> dict[str, str] | None:
    """Return a named navigation reference without exposing source details."""

    if not isinstance(value, str) or not value:
        return None
    record = world.maybe_get(value)
    if record is None:
        return {"id": value, "kind": fallback_kind, "title": "Unavailable reference"}
    return {"id": record.id, "kind": record.kind, "title": record.title}


def _timeline_references(world: World, values: Any, key: str | None = None) -> list[dict[str, str]]:
    result: list[dict[str, str]] = []
    for value in values or []:
        reference = value.get(key) if key and isinstance(value, dict) else value
        resolved = _timeline_reference(world, reference)
        if resolved is not None:
            result.append(resolved)
    return result


def _timeline_participants(
    world: World,
    values: Any,
    *,
    start: StoryTime,
    end: StoryTime | None,
) -> list[dict[str, Any]]:
    """Project named presence records without exposing source frontmatter.

    Timeline lanes need the effective inclusive ``from``/``to`` interval to
    decide who is present at a selected horizon.  Keep that small, derived
    projection alongside the existing safe entity reference rather than making
    the browser reconstruct defaults from private source data.
    """

    result: list[dict[str, Any]] = []
    for value in values or []:
        if not isinstance(value, dict):
            continue
        reference = _timeline_reference(world, value.get("character"), "character")
        if reference is None:
            continue
        from_value = value.get("from")
        to_value = value.get("to")
        from_point = (
            _timeline_point(from_value, world.default_timeline, record_id=reference["id"], field="participants.from")
            if from_value is not None else start
        )
        to_point = (
            _timeline_point(to_value, world.default_timeline, record_id=reference["id"], field="participants.to")
            if to_value is not None else end
        )
        result.append({
            **reference,
            "from": _timeline_coordinate(from_point),
            "to": None if to_point is None else _timeline_coordinate(to_point),
        })
    return result


def _location_id(value: Any) -> str | None:
    """Read an authored location reference without broadening its meaning."""

    if isinstance(value, str) and value:
        return value
    if isinstance(value, dict):
        for key in ("location", "entity", "target"):
            candidate = value.get(key)
            if isinstance(candidate, str) and candidate:
                return candidate
    return None


def _location_link(world: World, value: Any, *, reciprocal: bool = False, place_id: str | None = None) -> dict[str, Any] | None:
    """Project one authored route, optionally naming its source place.

    An outgoing route names the route target.  An incoming route is read from
    its destination, so it instead needs to name the source location while
    still using the source's link object for its authored prose.
    """

    target = place_id or _location_id(value)
    reference = _timeline_reference(world, target, "location")
    if reference is None:
        return None
    result: dict[str, Any] = {"place": reference, "reciprocal": reciprocal}
    destination = world.maybe_get(target)
    destination_summary = _timeline_summary(destination) if destination is not None else None
    if isinstance(value, dict):
        description = value.get("description") or value.get("label")
        if isinstance(description, str) and description.strip():
            result["description"] = description.strip()
        summary = value.get("summary")
        if isinstance(summary, str) and summary.strip():
            result["summary"] = summary.strip()
    if "summary" not in result and destination_summary:
        result["summary"] = destination_summary
    return result


def _location_context(world: World, record: Any) -> dict[str, Any]:
    """Project only explicitly authored containment and directional routes."""

    locations = world.by_kind("location")
    parent = _timeline_reference(world, _location_id(record.frontmatter.get("parent")), "location")
    children = [
        _timeline_reference(world, child.id, "location")
        for child in locations
        if _location_id(child.frontmatter.get("parent")) == record.id
    ]
    children = sorted((item for item in children if item is not None), key=lambda item: (item["title"].casefold(), item["id"]))

    def authored_link(source: Any, target_id: str) -> Any | None:
        """Return the actual authored route from ``source`` to ``target_id``.

        Incoming routes are displayed from the destination's point of view,
        but their descriptive prose belongs to the source's link object.  Do
        not reduce it to the source id before projecting it for the browser.
        """

        return next((link for link in source.frontmatter.get("links") or [] if _location_id(link) == target_id), None)

    def has_authored_link(source: Any, target_id: str) -> bool:
        return authored_link(source, target_id) is not None

    outgoing: list[dict[str, Any]] = []
    for link in record.frontmatter.get("links") or []:
        target_id = _location_id(link)
        target = world.maybe_get(target_id)
        item = _location_link(world, link, reciprocal=bool(target and has_authored_link(target, record.id)))
        if item is not None:
            outgoing.append(item)
    incoming = [
        _location_link(world, link, reciprocal=has_authored_link(record, source.id), place_id=source.id)
        for source in locations
        if source.id != record.id
        if (link := authored_link(source, record.id)) is not None
    ]
    sort_links = lambda item: (item["place"]["title"].casefold(), item["place"]["id"])
    return {
        "parent": parent,
        "children": children,
        "outgoing": sorted(outgoing, key=sort_links),
        "incoming": sorted((item for item in incoming if item is not None), key=sort_links),
    }


IMPORTANCE_ALGORITHM = "wedl-character-importance/v1"
IMPORTANCE_WEIGHTS = {"scenes": 40, "pointOfViewScenes": 25, "events": 20, "relationshipNeighbors": 15}


def _character_projection_index(world: World) -> dict[str, Any]:
    """Build read-only, revision-local evidence indexes once per World.

    The index deliberately stores source evidence rather than derived scores.
    It is attached to this immutable revision's ``World`` cache, so replacing a
    world/database cannot accidentally reuse evidence from a previous revision.
    """
    key = "character-projection-index/v1"
    if key in world._cache:
        return world._cache[key]
    locations: list[tuple[StoryTime, str, int, Any, dict[str, Any]]] = []
    events: list[tuple[StoryTime, set[str]]] = []
    for event in canonical_events(world):
        point = event_time(event, world.default_timeline)
        involved = {
            str(item.get("character")) for item in event.frontmatter.get("participants") or []
            if isinstance(item, dict) and item.get("character")
        }
        for ordinal, effect in enumerate(event.frontmatter.get("effects") or []):
            if not isinstance(effect, dict):
                continue
            if effect.get("key") == "location" and str(effect.get("operation") or "") in {"set", "clear"} and effect.get("target"):
                locations.append((point, event.id, ordinal, event, effect))
            # An event contributes at most once to a character, even if that
            # character is both a participant and the target of several effects.
            if effect.get("target") and world.maybe_get(str(effect["target"])) and world.maybe_get(str(effect["target"])).kind == "character":
                involved.add(str(effect["target"]))
        events.append((point, involved))
    locations.sort(key=lambda item: (item[0].timeline, item[0].tick, item[0].order, item[1], item[2]))

    scenes: list[tuple[StoryTime, str, bool, str]] = []
    for scene in world.by_kind("scene"):
        if scene.status not in {"active", "closed"}:
            continue
        value = scene.frontmatter.get("time") or {}
        start = _timeline_point(value.get("start"), world.default_timeline, record_id=scene.id, field="time.start")
        for participant in scene.frontmatter.get("participants") or []:
            if not isinstance(participant, dict) or not participant.get("character"):
                continue
            # A participant does not become evidence before their authored
            # entrance; later exits do not erase an already authored scene.
            entered = _timeline_point(participant.get("from") or start.to_dict(), world.default_timeline, record_id=scene.id, field="participants.from")
            scenes.append((entered, str(participant["character"]), bool(participant.get("point_of_view")), scene.id))

    earliest_neighbors: dict[tuple[str, str, str], tuple[StoryTime, str, str]] = {}
    for relationship in world.by_kind("relationship"):
        if relationship.status != "canonical":
            continue
        source, target = str(relationship.frontmatter.get("from") or ""), str(relationship.frontmatter.get("to") or "")
        if not source or not target or source == target:
            continue
        for transition in relationship.frontmatter.get("transitions") or []:
            if not isinstance(transition, dict) or not transition.get("time"):
                continue
            point = _timeline_point(transition["time"], world.default_timeline, record_id=relationship.id, field="transitions.time")
            # Timelines are independent ordinal coordinate spaces. A pair's
            # first connection on one cannot suppress another timeline.
            neighbor_key = (point.timeline, *sorted((source, target)))
            candidate = (point, source, target)
            previous = earliest_neighbors.get(neighbor_key)
            if previous is None or (point.timeline, point.tick, point.order, source, target) < (previous[0].timeline, previous[0].tick, previous[0].order, previous[1], previous[2]):
                earliest_neighbors[neighbor_key] = candidate
    relationships = sorted(earliest_neighbors.values(), key=lambda item: (item[0].timeline, item[0].tick, item[0].order, item[1], item[2]))
    value = {"locations": tuple(locations), "events": tuple(events), "scenes": tuple(scenes), "relationships": tuple(relationships)}
    world._cache[key] = value
    return value


def character_importance(world: World, at: StoryTime) -> dict[str, dict[str, Any]]:
    """Calculate disposable prominence for the complete eligible cohort.

    It is a relative navigation aid, not authored canon.  Each signal is
    log-normalized against the unfiltered canonical-and-retired cohort, then
    weighted 40/25/20/15. Contributions are rounded to two decimals before
    their final score is summed and rounded, preserving deterministic ties and
    zero-maxima behavior.
    """
    characters = sorted((item for item in world.by_kind("character") if item.status in {"canonical", "retired"}), key=lambda item: (item.title.casefold(), item.id))
    raw = {item.id: {"scenes": 0, "pointOfViewScenes": 0, "events": 0, "relationshipNeighbors": 0} for item in characters}
    index = _character_projection_index(world)
    for point, character_id, pov, _scene_id in index["scenes"]:
        if point.not_after(at) and character_id in raw:
            raw[character_id]["scenes"] += 1
            if pov:
                raw[character_id]["pointOfViewScenes"] += 1
    for point, involved in index["events"]:
        if point.not_after(at):
            for character_id in involved:
                if character_id in raw:
                    raw[character_id]["events"] += 1
    neighbors = {item.id: set() for item in characters}
    for point, source, target in index["relationships"]:
        if point.not_after(at):
            # Directional relationship records are authored separately, but
            # their prominence evidence is one reciprocal neighbor connection.
            if source in neighbors and target in neighbors:
                neighbors[source].add(target)
                neighbors[target].add(source)
    for character_id, values in raw.items():
        values["relationshipNeighbors"] = len(neighbors[character_id])
    maxima = {signal: max((values[signal] for values in raw.values()), default=0) for signal in IMPORTANCE_WEIGHTS}
    result: dict[str, dict[str, Any]] = {}
    for character in characters:
        counts = raw[character.id]
        normalized = {signal: (math.log1p(counts[signal]) / math.log1p(maxima[signal]) if maxima[signal] else 0.0) for signal in IMPORTANCE_WEIGHTS}
        contributions = {signal: round(IMPORTANCE_WEIGHTS[signal] * normalized[signal], 2) for signal in IMPORTANCE_WEIGHTS}
        score = round(sum(contributions.values()), 2)
        result[character.id] = {
            "algorithm": IMPORTANCE_ALGORITHM,
            "score": score,
            "raw": counts,
            "normalized": {signal: round(value, 6) for signal, value in normalized.items()},
            "contributions": contributions,
            "explanation": "Calculated from authored scene appearances, point-of-view appearances, canonical event involvement, and distinct relationship neighbors through this horizon.",
        }
    return result


def _location_history(world: World, record: Any, at: StoryTime) -> list[dict[str, Any]]:
    """Return an exact, horizon-bounded history from the shared transition index."""
    initial = _location_id((record.frontmatter.get("initial_state") or {}).get("location"))
    result: list[dict[str, Any]] = []
    if initial:
        result.append({"operation": "initial", "at": None, "location": _timeline_reference(world, initial, "location")})
    for point, event_id, _ordinal, _event, effect in _character_projection_index(world)["locations"]:
        if point.not_after(at) and effect.get("target") == record.id:
            operation = str(effect["operation"])
            location = _location_id(effect.get("value")) if operation == "set" else None
            result.append({"operation": operation, "at": _timeline_coordinate(point), "location": _timeline_reference(world, location, "location"), "event": _timeline_reference(world, event_id, "event")})
    return result


def _timeline_summary(record: Any) -> str:
    """Return authored prose as plain chronology copy, not a Markdown document."""

    lines = [line.strip() for line in record.body.splitlines() if line.strip() and not line.lstrip().startswith("#")]
    return " ".join(lines)


def _timeline_point(value: Any, default_timeline: str, *, record_id: str, field: str) -> StoryTime:
    try:
        return StoryTime.from_value(value, default_timeline)
    except (TypeError, ValueError) as exc:
        raise UsageError(
            "timeline data contains an invalid story coordinate",
            details={"entityId": record_id, "field": field},
        ) from exc


def _timeline_coordinate(point: StoryTime) -> dict[str, str]:
    """Serialize a browser-safe exact ordinal coordinate.

    ``tick`` is signed64, which JSON/JavaScript numbers cannot represent
    losslessly. Decimal strings retain an exact value for BigInt-capable
    browser clients and URL query construction.
    """

    return {"timeline": point.timeline, "tick": str(point.tick), "order": str(point.order)}


def _timeline_span(
    start: StoryTime,
    current: StoryTime | None,
    end: StoryTime | None,
    *,
    selected_timeline: str,
    record_id: str,
) -> bool:
    points = [("start", start)]
    if current is not None:
        points.append(("current", current))
    if end is not None:
        points.append(("end", end))
    for field, point in points:
        if point.timeline != start.timeline:
            raise UsageError(
                "timeline span endpoints must share a timeline",
                details={"entityId": record_id, "field": field, "startTimeline": start.timeline, "actualTimeline": point.timeline},
            )
    if current is not None and (current.tick, current.order) < (start.tick, start.order):
        raise UsageError("timeline span current boundary precedes its start", details={"entityId": record_id, "field": "current"})
    if end is not None and (end.tick, end.order) < (start.tick, start.order):
        raise UsageError("timeline span end boundary precedes its start", details={"entityId": record_id, "field": "end"})
    return start.timeline == selected_timeline


def timeline(repository: Repository, timeline_id: str | None = None, *, require_compiled: bool = False) -> dict[str, Any]:
    """Return the complete ordinal chronology contract for one timeline.

    Coordinates remain exact for internal comparisons.  Consumers must use the
    declared ordinal spacing and may not infer duration from tick gaps.
    """

    world, _database = require_database(repository, require_compiled=require_compiled)
    selected_timeline = timeline_id or world.default_timeline
    declarations = [item for item in world.config.get("timelines") or [] if isinstance(item, dict) and item.get("id") == selected_timeline]
    if not declarations:
        raise UsageError("unknown timeline", details={"timeline": selected_timeline, "availableTimelines": sorted(world.timeline_ids)})
    declaration = declarations[0]
    selected: dict[str, Any] = {"id": str(declaration["id"]), "label": str(declaration.get("label") or declaration["id"])}
    if "origin" in declaration:
        origin = declaration["origin"]
        if isinstance(origin, dict):
            selected["origin"] = {"tick": str(origin.get("tick")), "label": str(origin.get("label") or "")}

    points: list[dict[str, Any]] = []
    spans: list[dict[str, Any]] = []
    for record in world.by_kind("event"):
        at = _timeline_point(record.frontmatter.get("time") or {}, world.default_timeline, record_id=record.id, field="time")
        if at.timeline != selected_timeline:
            continue
        points.append({
            "kind": "event", "at": _timeline_coordinate(at), "status": record.status, "entity": _timeline_reference(world, record.id),
            "summary": _timeline_summary(record), "location": _timeline_reference(world, record.frontmatter.get("location"), "location"),
            "participants": _timeline_references(world, record.frontmatter.get("participants"), "character"),
            "causes": _timeline_references(world, record.frontmatter.get("causes")),
            "plotThreads": _timeline_references(world, record.frontmatter.get("related_story_points")),
        })
    for record in world.by_kind("story-point"):
        lifecycle = record.frontmatter.get("lifecycle") or {}
        for transition in lifecycle.get("transitions") or []:
            if not isinstance(transition, dict):
                continue
            at = _timeline_point(transition.get("time"), world.default_timeline, record_id=record.id, field="lifecycle.transitions.time")
            if at.timeline != selected_timeline:
                continue
            points.append({
                "kind": "plot-transition", "at": _timeline_coordinate(at), "status": record.status,
                "lifecycleState": transition.get("state"), "entity": _timeline_reference(world, record.id),
                "summary": str(transition.get("note") or _timeline_summary(record)),
                "causingEvent": _timeline_reference(world, transition.get("causing_event")),
                "outcomeEvents": _timeline_references(world, record.frontmatter.get("outcome_events")),
                "dependencies": _timeline_references(world, (record.frontmatter.get("dependencies") or {}).get("all"), "story_point"),
            })
    for record in world.by_kind("scene"):
        value = record.frontmatter.get("time") or {}
        start = _timeline_point(value.get("start"), world.default_timeline, record_id=record.id, field="time.start")
        current = _timeline_point(value.get("current"), world.default_timeline, record_id=record.id, field="time.current")
        end = _timeline_point(value["end"], world.default_timeline, record_id=record.id, field="time.end") if value.get("end") else None
        if not _timeline_span(start, current, end, selected_timeline=selected_timeline, record_id=record.id):
            continue
        spans.append({
            "kind": "scene", "start": _timeline_coordinate(start), "current": _timeline_coordinate(current), "end": None if end is None else _timeline_coordinate(end),
            "status": record.status, "entity": _timeline_reference(world, record.id), "summary": _timeline_summary(record),
            "location": _timeline_reference(world, record.frontmatter.get("location"), "location"),
            "participants": _timeline_participants(world, record.frontmatter.get("participants"), start=start, end=end),
            "conversations": _timeline_references(world, record.frontmatter.get("conversations")),
            "plotThreads": _timeline_references(world, record.frontmatter.get("story_points")),
        })
    for record in world.by_kind("conversation"):
        value = record.frontmatter.get("time") or {}
        start = _timeline_point(value.get("start"), world.default_timeline, record_id=record.id, field="time.start")
        end = _timeline_point(value["end"], world.default_timeline, record_id=record.id, field="time.end") if value.get("end") else None
        if not _timeline_span(start, None, end, selected_timeline=selected_timeline, record_id=record.id):
            continue
        spans.append({
            "kind": "conversation", "start": _timeline_coordinate(start), "end": None if end is None else _timeline_coordinate(end),
            "status": record.status, "entity": _timeline_reference(world, record.id), "summary": _timeline_summary(record),
            "scene": _timeline_reference(world, record.frontmatter.get("scene"), "scene"),
            "location": _timeline_reference(world, record.frontmatter.get("location"), "location"),
            "participants": _timeline_participants(world, record.frontmatter.get("participants"), start=start, end=end),
        })

    points.sort(key=lambda item: (int(item["at"]["tick"]), int(item["at"]["order"]), item["kind"], item["entity"]["title"].casefold(), item["entity"]["id"]))
    spans.sort(key=lambda item: (int(item["start"]["tick"]), int(item["start"]["order"]), item["kind"], item["entity"]["title"].casefold(), item["entity"]["id"]))
    return {
        "protocol": "wedl-timeline/v1", "revision": world.revision, "timeline": selected,
        "temporalSemantics": {"spacing": "ordinal", "durationSemantics": "none", "intervalEndpoints": "inclusive"},
        "points": points, "spans": spans,
    }


def _time_scope(
    *,
    perspective: str,
    all_time: bool,
    tick: int | None,
    timeline: str | None,
    order: int | None,
    has_scene_cursor: bool = False,
) -> None:
    """Validate the public temporal read contract before resolving defaults."""
    if all_time:
        if perspective != "author":
            raise UsageError("--all-time is available only for author views")
        if tick is not None or timeline is not None or order is not None:
            raise UsageError("--all-time cannot be combined with --tick, --timeline, or --order")
        return
    if perspective == "author" and tick is None:
        if timeline is not None or order is not None:
            raise UsageError("author as-of search requires --tick; --timeline and --order cannot be used alone")
        if not has_scene_cursor:
            raise UsageError("author search requires --tick when no selected or active scene provides a cursor, or --all-time")


def _scope_value(at: Any, all_time: bool) -> dict[str, Any]:
    return {"mode": "all-time"} if all_time else {"mode": "as-of", "at": at.to_dict()}


def search_world(repository: Repository, query: str, *, perspective: str = "author", character_id: str | None = None, scene_id: str | None = None, mode: str = "hybrid", limit: int = 20, timeline: str | None = None, tick: int | None = None, order: int | None = None, include_text: bool = False, all_time: bool = False, include_hypotheses: bool = False, thread_ids: tuple[str, ...] | None = None, require_compiled: bool = False) -> dict[str, Any]:
    world, database = require_database(repository, require_compiled=require_compiled)
    if include_hypotheses and perspective != "author":
        raise UsageError("hypothesis search is available only to authors")
    canonical_character = world.find(character_id, "character") if perspective == "character" and character_id else None
    scene = world.find(scene_id, "scene") if scene_id else _implicit_active_scene(world, canonical_character.id if canonical_character else None)
    _time_scope(
        perspective=perspective,
        all_time=all_time,
        tick=tick,
        timeline=timeline,
        order=order,
        has_scene_cursor=scene is not None,
    )
    effective_order = ORDER_MAX if order is None else order
    at = None if all_time else (world.story_time(tick, timeline, effective_order) if tick is not None else (_default_scene_time(world, scene) if scene else world.current_time))
    if at is None and not all_time:
        at = _default_scene_time(world, scene) if scene else world.story_time(0, order=effective_order)
    accessible: set[str] = set()
    knowledge_ids: set[str] = set()
    canonical_character_id: str | None = None
    if perspective == "character":
        if not character_id or not scene:
            raise UsageError("character search requires character and scene")
        character = canonical_character
        assert character is not None
        canonical_character_id = character.id
        accessible = accessible_entities(world, character.id, scene, at)
        knowledge_ids = {item["knowledgeId"] for item in current_knowledge(world, character.id, at)}
    with connect(database, True) as connection:
        thread_filter = resolve_thread_filter(connection, thread_ids)
        results = search(connection, query, perspective=perspective, character_id=canonical_character_id, scene_id=scene.id if scene else None, at=at, accessible_entities=accessible, active_knowledge=knowledge_ids, mode=mode, limit=limit, _thread_filter=thread_filter)
        state = search_state(connection)
    if include_hypotheses and thread_filter is None:
        # This separate source projection preserves the canonical compiled
        # corpus/model while keeping an explicit author discovery path.
        results.extend(hypothesis_source_search(world, query, limit=limit))
        results.sort(key=lambda item: (-float(item.get("score", 0.0)), str(item["documentId"])))
        results = results[:limit]
    for item in results:
        record = world.maybe_get(item["entityId"])
        if record:
            item["citation"] = {
                "entityId": record.id,
                "sourcePath": record.source_path,
                "section": str(item.get("heading") or item.get("documentKind")),
            }
        if not include_text:
            # The ranked snippet is the public search result. Returning the
            # complete indexed chunk as well doubled response size and made
            # interactive search noticeably more expensive on larger worlds.
            item.pop("text", None)
    return {"protocol": "wedl-search/v5", "revision": world.revision, "perspective": perspective, "characterId": canonical_character_id, "sceneId": scene.id if scene else None, "effectiveTime": None if at is None else at.to_dict(), "timeScope": _scope_value(at, all_time), "mode": mode, "includeHypotheses": include_hypotheses, "searchState": state, "results": results}


def conversation_view(repository: Repository, conversation_id: str, *, perspective: str = "author", character_id: str | None = None, timeline: str | None = None, tick: int | None = None, order: int | None = None, all_time: bool = False, require_compiled: bool = False) -> dict[str, Any]:
    if all_time and perspective != "author":
        raise UsageError("--all-time is available only for author views")
    if all_time and (tick is not None or timeline is not None or order is not None):
        raise UsageError("--all-time cannot be combined with --tick, --timeline, or --order")
    world, _database = require_database(repository, require_compiled=require_compiled)
    conversation = world.find(conversation_id, "conversation")
    effective_order = ORDER_MAX if order is None else order
    if tick is not None:
        at = world.story_time(tick, timeline, effective_order)
    elif perspective == "character" and world.current_time is not None:
        # Character views answer "how does this character remember it now?".
        # Memory may evolve long after the canonical exchange ended, so use the
        # current narrative cursor while reporting the conversation interval
        # separately below.
        at = world.current_time
    elif perspective == "character" and world.active_scene() is not None:
        at = scene_context_time(world.active_scene(), world.default_timeline)
    else:
        at = conversation_context_time(conversation, world)
    titles = {record.id: record.title for record in world.by_kind("character")}
    interval = {
        "start": (conversation.frontmatter.get("time") or {}).get("start"),
        "end": (conversation.frontmatter.get("time") or {}).get("end"),
    }
    def beat_value(turn: dict[str, Any]) -> dict[str, Any]:
        point = turn_time(turn, conversation, world.default_timeline)
        kind = beat_kind(turn)
        speaker = str(turn.get("speaker") or "")
        value = {"id": turn.get("id"), "kind": kind, "at": point.to_dict(), "text": turn.get("text"), "audience": turn.get("audience") or ["participants"], "citation": conversation.citation(world.revision, f"turn:{turn.get('id')}")}
        if kind == "speech":
            addressee_id = turn.get("addressee")
            if isinstance(addressee_id, str) and addressee_id.startswith("char:"):
                addressee_id = addressee_id[5:]
            value.update({"speakerId": speaker, "speaker": titles.get(speaker, speaker), "delivery": turn.get("delivery"), "addresseeId": addressee_id, "addressee": "participants" if addressee_id == "participants" else titles.get(str(addressee_id), addressee_id), "interrupts": turn.get("interrupts")})
        else:
            actor_ids = [actor[5:] if isinstance(actor, str) and actor.startswith("char:") else actor for actor in turn.get("actors") or []]
            value.update({"actorIds": actor_ids, "actors": [titles.get(str(actor), str(actor)) for actor in actor_ids]})
        return value
    if perspective == "author":
        turns = [turn for turn in conversation.frontmatter.get("turns") or [] if isinstance(turn, dict)]
        recollections = [item for item in conversation.frontmatter.get("recollections") or [] if isinstance(item, dict)]
        if not all_time:
            turns = [turn for turn in turns if turn_time(turn, conversation, world.default_timeline).not_after(at)]
            recollections = [
                item for item in recollections
                if item.get("at") and StoryTime.from_value(item["at"], world.default_timeline).not_after(at)
            ]
        speech_turns = [turn for turn in turns if beat_kind(turn) == "speech"]
        return {"protocol": "wedl-conversation/v2", "revision": world.revision, "perspective": "author", "conversationId": conversation.id, "title": conversation.title, "conversationTime": interval, "effectiveTime": None if all_time else at.to_dict(), "timeScope": _scope_value(at, all_time), "verbatimTurns": [beat_value(turn) for turn in speech_turns], "beats": [beat_value(turn) for turn in turns], "recollections": recollections}
    if not character_id:
        raise UsageError("character conversation view requires character")
    character = world.find(character_id, "character")
    recollection = current_recollection(conversation, character.id, at, world.default_timeline)
    perceived_beats = visible_beats(conversation, character.id, at, world.default_timeline)
    return {"protocol": "wedl-conversation/v2", "revision": world.revision, "perspective": "character", "conversationId": conversation.id, "title": conversation.title, "characterId": character.id, "conversationTime": interval, "effectiveTime": at.to_dict(), "timeScope": _scope_value(at, False), "heardVerbatimTurns": [beat_value(turn) for turn in perceived_beats if beat_kind(turn) == "speech"], "perceivedBeats": [beat_value(turn) for turn in perceived_beats], "subjectiveRecollection": None if recollection is None else {**recollection, "rememberedQuotes": remembered_quotes(conversation, recollection)}, "provenanceBoundary": "heardVerbatimTurns are canonical audible speech; perceivedBeats also includes witnessed canonical action choreography. subjectiveRecollection is separately authored memory and may disagree."}


def entity_state(repository: Repository, entity_id: str, tick: int, timeline: str | None = None, order: int = 2_147_483_647, *, require_compiled: bool = False) -> dict[str, Any]:
    world, _database = require_database(repository, require_compiled=require_compiled)
    record = world.find(entity_id)
    at = world.story_time(tick, timeline, order)
    state, citations = resolve_state(world, record.id, at)
    return {"revision": world.revision, "entityId": record.id, "at": at.to_dict(), "state": state, "citations": citations, "locationHistory": _location_history(world, record, at)}


def _whereabouts_time(world: World, *, tick: int | None, timeline: str | None, order: int | None) -> StoryTime:
    """Resolve the author horizon for the bulk whereabouts projection."""

    if tick is None:
        if timeline is not None or order is not None:
            raise UsageError("--timeline and --order require --tick for whereabouts")
        if world.current_time is not None:
            return world.current_time
        singleton = world.active_scene()
        if singleton is not None:
            return _default_scene_time(world, singleton)
        return world.story_time(0, order=ORDER_MAX)
    return world.story_time(tick, timeline, ORDER_MAX if order is None else order)


def _whereabouts_journeys(
    world: World, characters: list[Any], at: StoryTime,
) -> tuple[dict[str, list[dict[str, Any]]], dict[str, str | None], dict[str, str | None]]:
    """Project explicit canonical location state changes in one event pass.

    Scene presence and place links intentionally never manufacture a journey.
    This is a state-history view: a location exists here only when a character
    has an initial location or a canonical event sets/clears that state key.
    """

    character_ids = {character.id for character in characters}
    journeys: dict[str, list[dict[str, Any]]] = {character.id: [] for character in characters}
    current: dict[str, str | None] = {}
    last_known: dict[str, str | None] = {}
    for character in characters:
        initial = _location_id((character.frontmatter.get("initial_state") or {}).get("location"))
        current[character.id] = initial
        last_known[character.id] = initial
        if initial is not None:
            journeys[character.id].append({
                "kind": "initial", "at": None, "from": None,
                "to": _timeline_reference(world, initial, "location"), "event": None,
            })

    transitions = [
        item for item in _character_projection_index(world)["locations"]
        if item[0].not_after(at) and item[4].get("target") in character_ids
    ]

    for point, _event_id, _ordinal, event, effect in transitions:
        character_id = str(effect["target"])
        previous = current[character_id]
        operation = str(effect["operation"])
        next_location = _location_id(effect.get("value")) if operation == "set" else None
        if operation == "clear":
            kind = "clear"
        elif next_location == previous:
            kind = "reaffirmation"
        else:
            kind = "move"
        journeys[character_id].append({
            "kind": kind,
            "at": _timeline_coordinate(point),
            "from": _timeline_reference(world, previous, "location"),
            "to": _timeline_reference(world, next_location, "location"),
            "event": _timeline_reference(world, event.id, "event"),
        })
        current[character_id] = next_location
        if next_location is not None:
            last_known[character_id] = next_location
    # Returning final values beside the histories avoids a separate
    # ``resolve_state`` pass for every character.
    return journeys, current, last_known


def whereabouts(
    repository: Repository,
    character_id: str | None = None,
    tick: int | None = None,
    timeline: str | None = None,
    order: int | None = None,
    *,
    require_compiled: bool = False,
) -> dict[str, Any]:
    """Return an author-only, horizon-bounded projection of every character.

    It is deliberately a bulk read rather than a wrapper around ``state``:
    canonical event effects are scanned once, and the result contains neither
    private knowledge nor inferred routes, travel, or collective entities.
    """

    # Reject a malformed horizon before opening or compiling a repository.
    # This keeps CLI/API diagnostics deterministic even for an uncompiled or
    # otherwise unavailable world.
    if tick is None and (timeline is not None or order is not None):
        raise UsageError("--timeline and --order require --tick for whereabouts")
    world, _database = require_database(repository, require_compiled=require_compiled)
    at = _whereabouts_time(world, tick=tick, timeline=timeline, order=order)
    selected = world.find(character_id, "character") if character_id else None
    if selected is not None and selected.status not in {"canonical", "retired"}:
        raise UsageError("whereabouts includes canonical and retired characters only")
    cohort = sorted((character for character in world.by_kind("character") if character.status in {"canonical", "retired"}), key=lambda character: (character.title.casefold(), character.id))
    # Calculate before a name filter so a focused read retains the same
    # relative score and cohort maxima as the bulk projection.
    importance = character_importance(world, at)
    characters = [character for character in cohort if selected is None or character.id == selected.id]
    journeys, current_locations, last_known_locations = _whereabouts_journeys(world, characters, at)

    # A whereabouts read is an as-of projection.  A scene now closed can be
    # the recorded front at an earlier horizon, so closed and active records
    # use their authored interval and participant interval here. Planned and
    # abandoned records never establish a front. ``activeScenes`` therefore
    # means active *at this horizon*.
    candidate_scenes = [
        scene for scene in world.by_kind("scene")
        if scene.status in {"active", "closed"}
        and scene_contains_time(scene, at, world.default_timeline)
    ]
    candidate_scenes.sort(key=lambda scene: (scene.title.casefold(), scene.id))
    active_by_character: dict[str, Any] = {}
    members_by_scene: dict[str, list[Any]] = {}
    for scene in candidate_scenes:
        for character in characters:
            if character_scene_allowed(scene, character.id, at, world.default_timeline):
                members_by_scene.setdefault(scene.id, []).append(character)
                # The validator owns double-booking diagnostics.  Keep a
                # deterministic per-character reference here while the scene
                # list below preserves every directly authored membership.
                active_by_character.setdefault(character.id, scene)
    # Do not emit empty fronts, especially for a name-filtered read.  Scene
    # cards are evidence about the included characters, never an index of
    # unrelated planned or simultaneous story work.
    active_scenes = [scene for scene in candidate_scenes if scene.id in members_by_scene]

    entries: list[dict[str, Any]] = []
    location_groups: dict[str, dict[str, Any]] = {}
    offstage: list[dict[str, str]] = []
    unlocated: list[dict[str, str]] = []
    for character in characters:
        journey = journeys[character.id]
        location_id = current_locations[character.id]
        last_known_id = last_known_locations[character.id]
        if location_id is None:
            presence = "unlocated"
        elif character.id in active_by_character:
            presence = "active-scene"
        else:
            presence = "offstage"
        reference = _timeline_reference(world, character.id, "character")
        assert reference is not None
        role = character.frontmatter.get("role")
        entry = {
            "character": reference,
            "recordStatus": character.status,
            "role": str(role).strip() if isinstance(role, str) and role.strip() else None,
            "importance": importance[character.id],
            "presence": presence,
            "location": _timeline_reference(world, location_id, "location"),
            # This is useful historical context only after an explicit clear.
            # Repeating the current location obscures the distinction between
            # a confirmed whereabouts and a last reported one.
            "lastKnownLocation": _timeline_reference(world, last_known_id, "location") if location_id is None else None,
            "activeScene": _timeline_reference(world, active_by_character[character.id].id, "scene") if character.id in active_by_character else None,
            "journey": journey,
        }
        entries.append(entry)
        if location_id is not None:
            group = location_groups.setdefault(location_id, {
                "location": _timeline_reference(world, location_id, "location"), "characters": [],
            })
            group["characters"].append(reference)
        if presence == "offstage":
            offstage.append(reference)
        elif presence == "unlocated":
            unlocated.append(reference)

    scene_entries: list[dict[str, Any]] = []
    for scene in active_scenes:
        present = [
            _timeline_reference(world, character.id, "character")
            for character in members_by_scene[scene.id]
        ]
        # A filtered character read never exposes the rest of the cast.  In
        # the unfiltered bulk read this remains direct scene-presence data,
        # not a state or route inference.
        scene_reference = _timeline_reference(world, scene.id, "scene")
        assert scene_reference is not None
        scene_entries.append({
            "scene": scene_reference,
            "location": _timeline_reference(world, _location_id(scene.frontmatter.get("location")), "location"),
            "characters": [value for value in present if value is not None],
        })

    locations = sorted(
        location_groups.values(),
        key=lambda item: (str(item["location"]["title"]).casefold(), str(item["location"]["id"])),
    )
    return {
        "protocol": "wedl-whereabouts/v2",
        "revision": world.revision,
        "effectiveTime": _timeline_coordinate(at),
        "timeScope": {"mode": "as-of", "at": _timeline_coordinate(at)},
        "characterPolicy": {
            "includedStatuses": ["canonical", "retired"],
            "excludedStatuses": ["draft"],
            "locationEvidence": "initial state and canonical location effects only",
            "inference": "none",
        },
        "importancePolicy": {
            "algorithm": IMPORTANCE_ALGORITHM,
            "calculated": True,
            "nonCanonical": True,
            "cohort": "all canonical and retired characters before filtering",
            "normalization": "per-signal log1p(raw) / log1p(cohort maximum); zero maximum contributes zero",
            "weights": IMPORTANCE_WEIGHTS,
            "evidence": "scene appearances and POV subset; canonical event participants/effect targets deduplicated per event; distinct reciprocal relationship neighbors",
            "exclusions": "No prose, tags, inferred travel, co-presence, knowledge, or manual overrides are used.",
        },
        "characterFilter": None if selected is None else _timeline_reference(world, selected.id, "character"),
        "characters": entries,
        "locations": locations,
        "activeScenes": scene_entries,
        "offstageCharacters": offstage,
        "unlocatedCharacters": unlocated,
    }


def knowledge(repository: Repository, character_id: str, tick: int, timeline: str | None = None, order: int = 2_147_483_647, *, require_compiled: bool = False) -> dict[str, Any]:
    world, _database = require_database(repository, require_compiled=require_compiled)
    character = world.find(character_id, "character")
    at = world.story_time(tick, timeline, order)
    return {"revision": world.revision, "characterId": character.id, "at": at.to_dict(), "knowledge": [{key: value for key, value in item.items() if key != "record"} for item in current_knowledge(world, character.id, at)]}


def story_points(repository: Repository, scene_id: str | None = None, tick: int | None = None, timeline: str | None = None, order: int = 2_147_483_647, *, require_compiled: bool = False) -> dict[str, Any]:
    world, _database = require_database(repository, require_compiled=require_compiled)
    scene = world.find(scene_id, "scene") if scene_id else _implicit_active_scene(world)
    from .conversation import scene_context_time
    if tick is None and timeline is not None:
        raise UsageError("--timeline requires --tick for story-points")
    at = world.story_time(tick, timeline, order) if tick is not None else (_default_scene_time(world, scene) if scene else (world.current_time or world.story_time(0, None, order)))
    values = evaluate_all_story_points(world, at, scene)
    for value in values:
        record = world.get(str(value["storyPointId"]))
        value["plotTrail"] = _plot_trail(world, record, at)
    return {
        "revision": world.revision, "sceneId": scene.id if scene else None,
        "evaluatedAt": at.to_dict(), "storyPoints": values,
        "continuityAdvisories": _continuity_advisories(world, at),
    }


def _plot_trail(world: World, story_point: Any, at: StoryTime) -> dict[str, Any]:
    """Return only authored plot links visible through ``at``.

    The trail deliberately does not guess which nearby scene or event advanced
    a plot.  Transitions, their optional causal citations, and declared
    outcomes are the complete source relation.
    """
    lifecycle = story_point.frontmatter.get("lifecycle") or {}
    transitions: list[dict[str, Any]] = []
    for item in lifecycle.get("transitions") or []:
        if not isinstance(item, dict):
            continue
        point = StoryTime.from_value(item.get("time"), world.default_timeline)
        if not point.not_after(at):
            continue
        cause = world.maybe_get(str(item.get("causing_event") or ""))
        # Invalid source must not turn a horizon-sliced read into a future
        # disclosure. Validation reports the bad causal citation separately.
        causing_event = None
        if cause is not None and cause.kind == "event":
            cause_point = event_time(cause, world.default_timeline)
            if cause_point.not_after(point):
                causing_event = _timeline_reference(world, cause.id, "event")
        transitions.append({
            "at": point.to_dict(), "state": item.get("state"), "note": item.get("note"),
            "causingEvent": causing_event,
        })
    transitions.sort(key=lambda item: (item["at"]["timeline"], item["at"]["tick"], item["at"]["order"], str(item.get("state"))))
    outcomes: list[dict[str, Any]] = []
    for event_id in story_point.frontmatter.get("outcome_events") or []:
        event = world.maybe_get(str(event_id))
        if event is None or event.kind != "event":
            continue
        point = event_time(event, world.default_timeline)
        if point.not_after(at):
            outcomes.append({"event": _timeline_reference(world, event.id, "event"), "at": point.to_dict()})
    outcomes.sort(key=lambda item: (item["at"]["timeline"], item["at"]["tick"], item["at"]["order"], item["event"]["title"].casefold(), item["event"]["id"]))
    return {"transitions": transitions, "outcomeEvents": outcomes}


def _continuity_advisories(world: World, at: StoryTime) -> list[dict[str, Any]]:
    """Surface authored-link gaps without converting them into canon or errors."""
    advisories: list[dict[str, Any]] = []
    for story_point in world.by_kind("story-point"):
        for event_id in story_point.frontmatter.get("outcome_events") or []:
            event = world.maybe_get(str(event_id))
            if event is None or event.kind != "event" or not event_time(event, world.default_timeline).not_after(at):
                continue
            if story_point.id not in (event.frontmatter.get("related_story_points") or []):
                advisories.append({
                    "code": "WDL-ADVISORY-001", "kind": "plot-link-asymmetry",
                    "storyPoint": _timeline_reference(world, story_point.id, "story-point"),
                    "event": _timeline_reference(world, event.id, "event"),
                    "message": "The story point declares this outcome event, but the event does not name the story point as related.",
                })
    for event in world.by_kind("event"):
        point = event_time(event, world.default_timeline)
        if not point.not_after(at):
            continue
        for cause_id in event.frontmatter.get("causes") or []:
            cause = world.maybe_get(str(cause_id))
            if cause is None or cause.kind != "event" or not event_time(cause, world.default_timeline).not_after(at):
                continue
            if cause.frontmatter.get("location") and event.frontmatter.get("location") and cause.frontmatter.get("location") != event.frontmatter.get("location"):
                advisories.append({
                    "code": "WDL-ADVISORY-002", "kind": "cross-location-causality",
                    "cause": _timeline_reference(world, cause.id, "event"), "effect": _timeline_reference(world, event.id, "event"),
                    "message": "This authored causal edge crosses locations; record any needed handoff explicitly if it matters to the story.",
                })
    return sorted(advisories, key=lambda item: (item["code"], *(str(value.get("title", "")).casefold() for value in item.values() if isinstance(value, dict))))


def causality(
    repository: Repository,
    event_id: str,
    *,
    direction: str = "both",
    tick: int | None = None,
    timeline: str | None = None,
    order: int = 2_147_483_647,
    require_compiled: bool = False,
) -> dict[str, Any]:
    """Read an authored causal event DAG, clipped at one author horizon."""
    world, _database = require_database(repository, require_compiled=require_compiled)
    if tick is None and timeline is not None:
        raise UsageError("--timeline requires --tick for causality")
    target = world.find(event_id, "event")
    if target.status != "canonical":
        raise UsageError(
            "causality requires a canonical focus event",
            details={"event": target.title, "status": target.status},
        )
    all_events = [event for event in canonical_events(world)]
    default_at = world.current_time
    if default_at is None and all_events:
        default_at = max((event_time(event, world.default_timeline) for event in all_events), key=lambda point: (point.timeline, point.tick, point.order))
    at = world.story_time(tick, timeline, order) if tick is not None else (default_at or world.story_time(0, None, order))
    target_time = event_time(target, world.default_timeline)
    if not target_time.not_after(at):
        raise NotFound("event is beyond the selected author horizon")
    visible = {event.id: event for event in all_events if event_time(event, world.default_timeline).not_after(at)}
    upstream: dict[str, list[str]] = {event.id: [] for event in visible.values()}
    downstream: dict[str, list[str]] = {event.id: [] for event in visible.values()}
    for event in visible.values():
        for cause_id in event.frontmatter.get("causes") or []:
            if cause_id in visible:
                upstream[event.id].append(cause_id)
                downstream[cause_id].append(event.id)
    selected: set[str] = {target.id}
    frontier = [target.id]
    while frontier:
        current = frontier.pop()
        neighbours: list[str] = []
        if direction in {"upstream", "both"}:
            neighbours.extend(upstream[current])
        if direction in {"downstream", "both"}:
            neighbours.extend(downstream[current])
        for neighbour in neighbours:
            if neighbour not in selected:
                selected.add(neighbour); frontier.append(neighbour)
    def node_value(event: Any) -> dict[str, Any]:
        return {"event": _timeline_reference(world, event.id, "event"), "at": event_time(event, world.default_timeline).to_dict(), "status": event.status}
    nodes = sorted((node_value(visible[event_id]) for event_id in selected), key=lambda item: (item["at"]["timeline"], item["at"]["tick"], item["at"]["order"], item["event"]["title"].casefold(), item["event"]["id"]))
    edges = [
        {"cause": _timeline_reference(world, cause_id, "event"), "effect": _timeline_reference(world, event_id, "event")}
        for event_id in selected for cause_id in upstream[event_id] if cause_id in selected
    ]
    edges.sort(key=lambda item: (item["cause"]["title"].casefold(), item["cause"]["id"], item["effect"]["title"].casefold(), item["effect"]["id"]))
    return {
        "protocol": "wedl-causality/v1", "revision": world.revision,
        "direction": direction, "effectiveTime": at.to_dict(), "timeScope": _scope_value(at, False),
        "focusEvent": _timeline_reference(world, target.id, "event"), "nodes": nodes, "edges": edges,
        "continuityAdvisories": _continuity_advisories(world, at),
    }


def interactions_between(repository: Repository, first: str, second: str, *, require_compiled: bool = False) -> dict[str, Any]:
    world, _database = require_database(repository, require_compiled=require_compiled)
    a = world.find(first, "character"); b = world.find(second, "character")
    return {"revision": world.revision, "firstCharacterId": a.id, "secondCharacterId": b.id, "interactions": [{key: value for key, value in item.items() if key != "record"} for item in interactions(world, a.id, b.id)]}
