from __future__ import annotations

from collections import Counter
from copy import deepcopy
from typing import Any

from . import CHRONOLOGY_SOURCE_SCHEMA, SOURCE_SCHEMA, SUPPORTED_SOURCE_SCHEMAS, THREAD_SOURCE_SCHEMA, V04_RECOVERY_CONTRACT, V04_SOURCE_SCHEMA, V07_SOURCE_SCHEMA
from .conversation import beat_kind, conversation_participant_at, scene_contains_time, participant_at, turn_time, turn_visible_to
from .errors import SupersededSchemaError
from .ids import KIND_PREFIX, AUX_PREFIX, valid_id, valid_spatial_id
from .model import Record, StoryTime, World
from .source import extract_entity_refs, markdown_entity_links
from .semantics import canonical_events, effective_time, event_time, resolve_state

KINDS = set(KIND_PREFIX)
VALID_STATUS = {
    "world": {"canonical"},
    "character": {"canonical", "retired"},
    "knowledge": {"canonical", "retired"},
    "event": {"canonical", "draft", "cancelled", "retconned"},
    "object": {"canonical", "retired"},
    "environment": {"canonical", "retired"},
    "location": {"canonical", "retired"},
    "relationship": {"canonical", "retired"},
    "story-point": {"canonical", "retired"},
    "scene": {"planned", "active", "closed", "abandoned"},
    "conversation": {"draft", "active", "closed", "retired"},
    # A hypothesis is author workspace material, never a canonical fact or
    # an operational draft.  Its lifecycle is deliberately distinct from the
    # ordinary record lifecycle.
    "hypothesis": {"open", "adopted", "rejected"},
}

_V05_WITHDRAWN_MEMBERS = (
    "default_continuity",
    "continuities",
    "continuity",
    "membership",
    "strands",
    "synchronization",
    "current_horizon",
    "retcon",
    "causal_handoffs",
    "presentation_frames",
)


def is_adoptable_canonical_record(record: Record) -> bool:
    """Whether a pre-existing record may evidence an adopted possibility."""
    if record.kind == "hypothesis":
        return False
    if record.kind in {"scene", "conversation"}:
        return record.status in {"active", "closed", "retired"}
    return record.status in {"canonical", "retired"}

def diagnostic(code: str, message: str, record: Record | None = None, field: str | None = None, severity: str = "error") -> dict[str, Any]:
    return {
        "code": code,
        "message": message,
        "severity": severity,
        "entityId": record.id if record else None,
        "path": record.source_path if record else None,
        "field": field,
    }


def _point(value: Any, default_timeline: str) -> StoryTime | None:
    try:
        return StoryTime.from_value(value, default_timeline)
    except (TypeError, KeyError, ValueError):
        return None


def _strictly_ordered(values: list[dict[str, Any]], default_timeline: str) -> bool:
    points = [_point(item.get("time") or item.get("at"), default_timeline) for item in values]
    if any(point is None for point in points):
        return False
    return all(
        left.timeline == right.timeline and (left.tick, left.order) < (right.tick, right.order)
        for left, right in zip(points, points[1:])  # type: ignore[arg-type]
    )


def _validate_timelines(world: World) -> tuple[set[str], list[dict[str, Any]]]:
    """Validate the world-level timeline declarations before any storage work."""
    result: list[dict[str, Any]] = []
    record = world.world_record
    timelines = record.frontmatter.get("timelines")
    declared: set[str] = set()
    if not isinstance(timelines, list) or not timelines:
        result.append(diagnostic("WDL-TIMELINE-001", "timelines must be a non-empty array", record, "timelines"))
        if world.schema == THREAD_SOURCE_SCHEMA:
            result.append(diagnostic("WDL-TIMELINE-012", "wedl/v0.5 requires exactly one timeline declaration", record, "timelines"))
        return declared, result
    for index, value in enumerate(timelines):
        field = f"timelines[{index}]"
        if not isinstance(value, dict):
            result.append(diagnostic("WDL-TIMELINE-002", "timeline declaration must be a mapping", record, field))
            continue
        identifier = value.get("id")
        if not isinstance(identifier, str) or not identifier.strip():
            result.append(diagnostic("WDL-TIMELINE-003", "timeline id must be a non-empty string", record, f"{field}.id"))
        elif identifier in declared:
            result.append(diagnostic("WDL-TIMELINE-004", f"duplicate timeline id {identifier!r}", record, f"{field}.id"))
        else:
            declared.add(identifier)
        label = value.get("label")
        if not isinstance(label, str) or not label.strip():
            result.append(diagnostic("WDL-TIMELINE-005", "timeline label must be a non-empty string", record, f"{field}.label"))
        if "origin" not in value:
            continue
        origin = value["origin"]
        origin_field = f"{field}.origin"
        if not isinstance(origin, dict):
            result.append(diagnostic("WDL-TIMELINE-006", "timeline origin must be a mapping with tick and label", record, origin_field))
            continue
        unexpected_keys = set(origin) - {"tick", "label"}
        if unexpected_keys:
            unexpected = sorted(unexpected_keys, key=lambda key: (type(key).__module__, type(key).__qualname__, repr(key)))[0]
            result.append(diagnostic("WDL-TIMELINE-007", "timeline origin only permits tick and label", record, f"{origin_field}.{unexpected}"))
        try:
            StoryTime(identifier if isinstance(identifier, str) else "main", origin.get("tick"), 0)
        except ValueError as exc:
            result.append(diagnostic("WDL-TIMELINE-008", str(exc), record, f"{origin_field}.tick"))
        if not isinstance(origin.get("label"), str) or not origin["label"].strip():
            result.append(diagnostic("WDL-TIMELINE-009", "timeline origin label must be a non-empty string", record, f"{origin_field}.label"))
    if world.schema == THREAD_SOURCE_SCHEMA and len(timelines) != 1:
        result.append(diagnostic("WDL-TIMELINE-012", "wedl/v0.5 requires exactly one timeline declaration", record, "timelines"))
    default = record.frontmatter.get("default_timeline", "main")
    if not isinstance(default, str) or not default.strip():
        result.append(diagnostic("WDL-TIMELINE-010", "default_timeline must be a non-empty string", record, "default_timeline"))
    elif default not in declared:
        result.append(diagnostic("WDL-TIMELINE-011", f"default timeline {default!r} is not declared", record, "default_timeline"))
    return declared, result


def _time_error(value: Any, default_timeline: str) -> tuple[str, str]:
    if not isinstance(value, dict):
        return "", "story time must be a mapping"
    if "tick" not in value:
        return ".tick", "tick is required"
    if "timeline" in value and (not isinstance(value["timeline"], str) or not value["timeline"].strip()):
        return ".timeline", "timeline must be a non-empty string"
    if isinstance(value.get("tick"), bool) or not isinstance(value.get("tick"), int):
        return ".tick", "tick must be an integer"
    if isinstance(value.get("order", 0), bool) or not isinstance(value.get("order", 0), int):
        return ".order", "order must be an integer"
    try:
        StoryTime.from_value(value, default_timeline)
    except ValueError as exc:
        field = ".tick" if "tick" in str(exc) else ".order" if "order" in str(exc) else ".timeline"
        return field, str(exc)
    return "", ""


def _time_values(record: Record) -> list[tuple[str, Any]]:
    """Return only schema-defined story-time fields for one record.

    Story data can legitimately use words such as ``order`` in its state and
    metadata. Temporal validation must therefore never infer a StoryTime from
    arbitrary mappings.
    """
    data = record.frontmatter
    result: list[tuple[str, Any]] = []

    def add(field: str, value: Any) -> None:
        result.append((field, value))

    def add_range(field: str, value: Any, *, current: bool = False) -> None:
        if not isinstance(value, dict):
            add(field, value)
            return
        add(f"{field}.start", value.get("start"))
        if current:
            add(f"{field}.current", value.get("current"))
        if value.get("end") is not None:
            add(f"{field}.end", value["end"])

    if record.kind == "event":
        add("time", data.get("time"))
    elif record.kind == "environment":
        add_range("time", data.get("time"))
    elif record.kind == "scene":
        add_range("time", data.get("time"), current=True)
        for index, participant in enumerate(data.get("participants") or []):
            if isinstance(participant, dict):
                for key in ("from", "to"):
                    if key in participant:
                        add(f"participants[{index}].{key}", participant[key])
        for index, observation in enumerate(data.get("observations") or []):
            if isinstance(observation, dict):
                for key in ("at", "until"):
                    if key in observation:
                        add(f"observations[{index}].{key}", observation[key])
    elif record.kind == "conversation":
        add_range("time", data.get("time"))
        for index, participant in enumerate(data.get("participants") or []):
            if isinstance(participant, dict):
                for key in ("from", "to"):
                    if key in participant:
                        add(f"participants[{index}].{key}", participant[key])
        for index, turn in enumerate(data.get("turns") or []):
            if isinstance(turn, dict) and "at" in turn:
                add(f"turns[{index}].at", turn["at"])
        for index, recollection in enumerate(data.get("recollections") or []):
            if isinstance(recollection, dict) and "at" in recollection:
                add(f"recollections[{index}].at", recollection["at"])
    elif record.kind in {"knowledge", "relationship"}:
        for index, transition in enumerate(data.get("transitions") or []):
            if isinstance(transition, dict) and "time" in transition:
                add(f"transitions[{index}].time", transition["time"])
    elif record.kind == "story-point":
        lifecycle = data.get("lifecycle") or {}
        if isinstance(lifecycle, dict):
            for index, transition in enumerate(lifecycle.get("transitions") or []):
                if isinstance(transition, dict) and "time" in transition:
                    add(f"lifecycle.transitions[{index}].time", transition["time"])
    return result


def _validate_time_points(world: World, declared: set[str]) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    for record in world:
        if record.kind == "world":
            continue
        for field, value in _time_values(record):
            suffix, message = _time_error(value, world.default_timeline)
            if message:
                result.append(diagnostic("WDL-TIME-002", message, record, f"{field}{suffix}"))
                continue
            point = StoryTime.from_value(value, world.default_timeline)
            if point.timeline not in declared:
                result.append(diagnostic("WDL-TIME-003", f"timeline {point.timeline!r} is not declared", record, f"{field}.timeline"))
    return result


def _schema_member_paths(record: Record, schema: str) -> list[str]:
    data = record.frontmatter
    if schema == SOURCE_SCHEMA:
        return ["threads"] if "threads" in data else []
    if schema != THREAD_SOURCE_SCHEMA:
        return []
    result = [field for field in _V05_WITHDRAWN_MEMBERS if field in data]
    for field, value in _time_values(record):
        if isinstance(value, dict) and "domain" in value:
            result.append(f"{field}.domain")
    current_time = data.get("current_time") if record.kind == "world" else None
    if isinstance(current_time, dict) and "domain" in current_time:
        result.append("current_time.domain")
    return result


def _validate_threads(world: World) -> list[dict[str, Any]]:
    """Validate the v0.5 grouping-only declaration and memberships in source order."""
    result: list[dict[str, Any]] = []
    world_record = world.world_record
    declarations = world_record.frontmatter.get("threads")
    declared: set[str] = set()
    if not isinstance(declarations, list):
        return [diagnostic("WDL-THREAD-001", "wedl/v0.5 world threads must be an array", world_record, "threads")]

    declaration_ids: list[str] = []
    canonical_declarations = True
    for index, value in enumerate(declarations):
        field = f"threads[{index}]"
        if not isinstance(value, dict) or set(value) != {"id", "label"}:
            result.append(diagnostic("WDL-THREAD-002", "thread declaration must be exactly an id and label mapping", world_record, field))
            canonical_declarations = False
            continue
        identifier = value["id"]
        label = value["label"]
        if not valid_id(identifier, "thread"):
            result.append(diagnostic("WDL-THREAD-003", "thread id must use the thread_<26 Crockford> format", world_record, f"{field}.id"))
            canonical_declarations = False
        elif identifier in declared:
            result.append(diagnostic("WDL-THREAD-004", "thread declaration id is duplicated", world_record, f"{field}.id"))
        else:
            declared.add(identifier)
        if not isinstance(label, str) or not label.strip():
            result.append(diagnostic("WDL-THREAD-005", "thread label must be a non-empty string", world_record, f"{field}.label"))
            canonical_declarations = False
        if isinstance(identifier, str):
            declaration_ids.append(identifier)
        else:
            canonical_declarations = False
    if canonical_declarations and declaration_ids != sorted(declaration_ids):
        result.append(diagnostic("WDL-THREAD-006", "thread declarations must be sorted by id", world_record, "threads"))

    for record in sorted(world.records.values(), key=lambda item: (item.source_path.casefold(), item.id)):
        if record.kind in {"world", "hypothesis"} or "threads" not in record.frontmatter:
            continue
        memberships = record.frontmatter["threads"]
        if not isinstance(memberships, list):
            result.append(diagnostic("WDL-THREAD-007", "record thread membership must be an array of thread ids", record, "threads"))
            continue
        invalid_index = next(
            (
                index
                for index, identifier in enumerate(memberships)
                if not valid_id(identifier, "thread")
            ),
            None,
        )
        if invalid_index is not None:
            result.append(diagnostic("WDL-THREAD-007", "record thread membership must be an array of thread ids", record, f"threads[{invalid_index}]"))
            continue
        if (
            len(memberships) != len(set(memberships))
            or memberships != sorted(memberships)
        ):
            result.append(diagnostic("WDL-THREAD-008", "record thread memberships must be unique and sorted by id", record, "threads"))
        for index, identifier in enumerate(memberships):
            if identifier not in declared:
                result.append(diagnostic("WDL-THREAD-009", "record thread membership is not declared by the world", record, f"threads[{index}]"))
    return result


def _validate_v07_inherited_chronology(world: World) -> list[dict[str, Any]]:
    """Apply the complete v0.6 chronology grammar to v0.7's inherited records."""
    from .chronology_validation import validate_v06_candidate

    records: dict[str, Record] = {}
    for record in world:
        data = deepcopy(record.frontmatter)
        data["schema"] = CHRONOLOGY_SOURCE_SCHEMA
        if record.kind == "world":
            data.pop("capabilities", None)
            # v0.7 allows a coordinate/time-only world.  Supply an inert
            # declaration solely to run the inherited annotation/nesting
            # checks; it is never written back or treated as an authored fact.
            data.setdefault("chronology", {"calendars": [], "eras": [], "anchors": []})
        records[record.id] = Record(data, record.body, record.source_path, record.raw_bytes, record.blob_oid, record.revision)
    candidate = World(world.revision, world.tree_oid, records, world.root, world.source_root, world.is_worktree)
    return validate_v06_candidate(candidate, validate_inherited=False)


def validate_world(world: World) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    schemas: list[Any] = []
    for record in world:
        candidate = record.frontmatter.get("schema")
        if not any(candidate == existing for existing in schemas):
            schemas.append(candidate)
    if any(candidate == V04_SOURCE_SCHEMA for candidate in schemas):
        raise SupersededSchemaError(
            f"{V04_SOURCE_SCHEMA} is superseded; see {V04_RECOVERY_CONTRACT}",
            details={"schema": V04_SOURCE_SCHEMA, "recoveryContract": V04_RECOVERY_CONTRACT},
        )
    if any(candidate == CHRONOLOGY_SOURCE_SCHEMA for candidate in schemas):
        from .chronology_validation import validate_v06_candidate
        return validate_v06_candidate(world)
    if any(candidate == V07_SOURCE_SCHEMA for candidate in schemas):
        from .spatial_validation import validate_spatial_world
        if len(schemas) != 1:
            world_records = world.by_kind("world")
            return [diagnostic("WDL-SRC-008", "source records must use one homogeneous schema", world_records[0] if world_records else None, "schema")]
        # v0.7's closed capability envelope is additive: its component
        # diagnostics must not bypass the generic record, chronology, time,
        # reference, and state validation that still governs its inherited
        # records.  Spatial-only kinds remain owned by the component pass.
        result.extend(validate_spatial_world(world))
        result.extend(_validate_v07_inherited_chronology(world))
    world_records = world.by_kind("world")
    if len(schemas) > 1:
        # Preserve the homogeneous-source invariant, but do not suppress
        # record-kind diagnostics. Changeset previews construct a synthetic
        # hypothesis before its schema is materialized; callers still need the
        # established nonoperative-field diagnostics for that candidate.
        result.append(diagnostic("WDL-SRC-008", "source records must use one homogeneous schema", world_records[0] if world_records else None, "schema"))
    schema = world_records[0].frontmatter.get("schema") if len(world_records) == 1 else (schemas[0] if schemas else None)
    if len(world_records) != 1:
        result.append(diagnostic("WDL-WORLD-001", "world must contain exactly one world record"))
    path_counts = Counter(record.source_path.casefold() for record in world)
    for record in world:
        data = record.frontmatter
        if not isinstance(schema, str) or schema not in SUPPORTED_SOURCE_SCHEMAS:
            result.append(diagnostic("WDL-SRC-001", f"expected schema {SOURCE_SCHEMA}", record, "schema"))
        incompatible_members = _schema_member_paths(record, str(schema))
        for field in incompatible_members:
            result.append(diagnostic("WDL-SRC-009", "member is incompatible with this source schema", record, field))
        if "importance" in data:
            result.append(diagnostic("WDL-SRC-007", "importance is calculated read-model output and is never canonical frontmatter", record, "importance"))
        if schema == V07_SOURCE_SCHEMA and record.kind in {"map", "anchor", "portal", "route", "overlay"}:
            continue
        if record.kind not in KINDS:
            result.append(diagnostic("WDL-SRC-002", f"unsupported entity kind {record.kind!r}", record, "kind"))
            continue
        if not valid_id(record.id, record.kind) and not (schema == V07_SOURCE_SCHEMA and valid_spatial_id(record.id, record.kind)):
            result.append(diagnostic("WDL-ID-001", f"invalid ID {record.id!r} for {record.kind}", record, "id"))
        if record.status not in VALID_STATUS[record.kind]:
            result.append(diagnostic("WDL-STATUS-001", f"invalid status {record.status!r} for {record.kind}", record, "status"))
        for key in ("title", "domain"):
            if schema == V07_SOURCE_SCHEMA and key == "domain":
                continue
            if not isinstance(data.get(key), str) or not data.get(key).strip():
                result.append(diagnostic("WDL-SRC-003", f"{key} must be non-empty", record, key))
        for key in ("tags", "aliases"):
            values = data.get(key, [])
            if not isinstance(values, list) or not all(isinstance(item, str) for item in values):
                result.append(diagnostic("WDL-SRC-004", f"{key} must be an array of strings", record, key))
            elif len(values) != len(set(values)):
                result.append(diagnostic("WDL-SRC-006", f"{key} must not contain duplicates", record, key))
        if path_counts[record.source_path.casefold()] > 1:
            result.append(diagnostic("WDL-SRC-005", "case-folding source path collision", record))

        if not incompatible_members:
            refs = extract_entity_refs(data) | markdown_entity_links(record.body)
            refs.discard(record.id)
            for entity_id in sorted(refs):
                if entity_id not in world.records:
                    result.append(diagnostic("WDL-REF-001", f"unknown referenced entity {entity_id}", record))

    if len(world_records) != 1:
        return result
    declared, timeline_diagnostics = _validate_timelines(world)
    result.extend(timeline_diagnostics)
    result.extend(_validate_time_points(world, declared))
    if any(item["severity"] == "error" and item["code"].startswith(("WDL-TIME-", "WDL-TIMELINE-")) for item in result):
        return result

    if schema == THREAD_SOURCE_SCHEMA or (schema == V07_SOURCE_SCHEMA and "threads" in world.world_record.frontmatter):
        result.extend(_validate_threads(world))

    result.extend(_validate_current_active_scenes(world))

    for record in world:
        if record.kind == "scene":
            result.extend(_validate_scene(world, record))
        elif record.kind == "location":
            result.extend(_validate_location(world, record))
        elif record.kind == "conversation":
            result.extend(_validate_conversation(world, record))
        elif record.kind == "story-point":
            result.extend(_validate_story_point(world, record))
        elif record.kind == "knowledge":
            result.extend(_validate_transitions(world, record, "transitions", "kt", {"accepted", "suspected", "rejected", "uncertain", "remembered", "forgotten"}))
        elif record.kind == "relationship":
            result.extend(_validate_transitions(world, record, "transitions", "rt", None))
        elif record.kind == "event":
            point = _point(record.frontmatter.get("time"), world.default_timeline)
            if point is None:
                result.append(diagnostic("WDL-EVENT-001", "event requires a valid time", record, "time"))
            seen_effects: set[str] = set()
            for index, effect in enumerate(record.frontmatter.get("effects") or []):
                if not isinstance(effect, dict):
                    result.append(diagnostic("WDL-EVENT-002", "event effect must be a mapping", record, f"effects[{index}]"))
                    continue
                effect_id = str(effect.get("id") or "")
                if not valid_id(effect_id, "effect"):
                    result.append(diagnostic("WDL-EVENT-003", "invalid effect ID", record, f"effects[{index}].id"))
                if effect_id in seen_effects:
                    result.append(diagnostic("WDL-EVENT-004", "duplicate effect ID", record, f"effects[{index}].id"))
                seen_effects.add(effect_id)
                if effect.get("operation") not in {"set", "clear", "add-to-set", "remove-from-set"}:
                    result.append(diagnostic("WDL-EVENT-005", "unsupported effect operation", record, f"effects[{index}].operation"))
        elif record.kind == "hypothesis":
            result.extend(_validate_hypothesis(world, record))

    result.extend(_validate_event_causality(world))
    result.extend(_validate_typed_causes(world))
    result.extend(_validate_state_semantics(world))
    result.extend(_validate_active_scene_continuity(world))
    result.extend(_validate_historical_scene_presence(world))
    result.extend(_validate_concurrent_event_participation(world))
    result.extend(_cycle_checks(world))
    return result


def _validate_hypothesis(world: World, record: Record) -> list[dict[str, Any]]:
    """Validate the deliberately small, nonoperative uncertainty record.

    Placement is authored context, not StoryTime.  This function therefore
    validates only shape and references and intentionally never calls state,
    causality, or time validation helpers for a hypothesis.
    """
    data = record.frontmatter
    result: list[dict[str, Any]] = []
    # A closed vocabulary is essential here.  A permissive metadata bucket
    # would let a source smuggle in occurrence coordinates or operational
    # fields that later code might accidentally start honoring.
    allowed_fields = {
        "schema", "kind", "id", "title", "domain", "status", "tags", "aliases",
        "section_audiences", "provenance", "statement", "subjects", "alternatives",
        "context", "placement", "resolution",
    }
    for field in sorted(set(data).difference(allowed_fields)):
        result.append(diagnostic("WDL-HYP-010", "hypotheses permit only their nonoperative frontmatter fields", record, field))
    for field in ("statement", "context"):
        if not isinstance(data.get(field), str) or not data[field].strip():
            result.append(diagnostic("WDL-HYP-001", f"hypothesis requires a non-empty {field}", record, field))
    subjects = data.get("subjects")
    if not isinstance(subjects, list) or not subjects or not all(isinstance(value, str) and value in world.records for value in subjects):
        result.append(diagnostic("WDL-HYP-002", "hypothesis subjects must be a non-empty array of record references", record, "subjects"))
    elif len(subjects) != len(set(subjects)):
        result.append(diagnostic("WDL-HYP-003", "hypothesis subjects must not contain duplicates", record, "subjects"))
    alternatives = data.get("alternatives")
    if not isinstance(alternatives, list) or not alternatives or not all(isinstance(value, str) and value.strip() for value in alternatives):
        result.append(diagnostic("WDL-HYP-004", "hypothesis alternatives must be a non-empty array of non-empty strings", record, "alternatives"))
    placement = data.get("placement")
    if not isinstance(placement, dict) or not placement:
        result.append(diagnostic("WDL-HYP-005", "hypothesis requires placement context", record, "placement"))
    else:
        allowed = {"timeline", "scene", "event", "location", "context"}
        for key in placement:
            if key not in allowed:
                result.append(diagnostic("WDL-HYP-006", "hypothesis placement permits only timeline, scene, event, location, and context", record, f"placement.{key}"))
        if not any(key in placement for key in allowed):
            result.append(diagnostic("WDL-HYP-005", "hypothesis requires placement context", record, "placement"))
        if "timeline" in placement:
            timeline = placement["timeline"]
            if not isinstance(timeline, str) or not timeline.strip() or timeline not in world.timeline_ids:
                result.append(diagnostic("WDL-HYP-007", "hypothesis placement timeline must be a declared timeline name", record, "placement.timeline"))
        for key, kind in (("scene", "scene"), ("event", "event"), ("location", "location")):
            value = placement.get(key)
            if value is not None and (not isinstance(value, str) or world.maybe_get(value) is None or world.get(value).kind != kind):
                result.append(diagnostic("WDL-HYP-008", f"hypothesis placement {key} must reference a {kind}", record, f"placement.{key}"))
        if "context" in placement and (not isinstance(placement["context"], str) or not placement["context"].strip()):
            result.append(diagnostic("WDL-HYP-009", "hypothesis placement context must be non-empty text", record, "placement.context"))
    resolution = data.get("resolution")
    if record.status == "open":
        if resolution is not None:
            result.append(diagnostic("WDL-HYP-011", "an open hypothesis must not have a resolution", record, "resolution"))
    elif not isinstance(resolution, dict):
        result.append(diagnostic("WDL-HYP-011", "a resolved hypothesis requires a resolution mapping", record, "resolution"))
    elif record.status == "adopted":
        for field in sorted(set(resolution).difference({"canonical_entities", "note"})):
            result.append(diagnostic("WDL-HYP-014", "an adopted hypothesis resolution permits only canonical_entities and note", record, f"resolution.{field}"))
        records = resolution.get("canonical_entities")
        if "note" in resolution and (not isinstance(resolution["note"], str) or not resolution["note"].strip()):
            result.append(diagnostic("WDL-HYP-013", "an adopted hypothesis note must be non-empty text when present", record, "resolution.note"))
        if not isinstance(records, list) or not records:
            result.append(diagnostic("WDL-HYP-011", "an adopted hypothesis requires one or more canonical entities", record, "resolution.canonical_entities"))
        else:
            for index, value in enumerate(records):
                target = world.maybe_get(value) if isinstance(value, str) else None
                if target is None or not is_adoptable_canonical_record(target):
                    result.append(diagnostic("WDL-HYP-012", "adoption references must be existing settled canonical records", record, f"resolution.canonical_entities[{index}]"))
            if len(records) != len(set(value for value in records if isinstance(value, str))):
                result.append(diagnostic("WDL-HYP-012", "adoption references must not contain duplicates", record, "resolution.canonical_entities"))
    elif record.status == "rejected":
        for field in sorted(set(resolution).difference({"note"})):
            result.append(diagnostic("WDL-HYP-014", "a rejected hypothesis resolution permits only a note", record, f"resolution.{field}"))
        note = resolution.get("note")
        if not isinstance(note, str) or not note.strip():
            result.append(diagnostic("WDL-HYP-013", "a rejected hypothesis requires a non-empty resolution note", record, "resolution.note"))
    return result


def _state_schema(world: World, record: Record, key: str) -> dict[str, Any] | None:
    state_keys = world.config.get("state_keys") or {}
    by_kind = state_keys.get(record.kind) or {}
    value = by_kind.get(key)
    return value if isinstance(value, dict) else None


def _validate_state_value(
    world: World,
    owner: Record,
    key: str,
    value: Any,
    field: str,
) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    schema = _state_schema(world, owner, key)
    if schema is None:
        result.append(diagnostic("WDL-STATE-010", f"unknown state key {key!r} for {owner.kind}", owner, field))
        return result
    value_type = schema.get("type")
    if value_type == "string":
        if not isinstance(value, str):
            result.append(diagnostic("WDL-STATE-011", f"state {key!r} must be a string", owner, field))
    elif value_type == "entity":
        if not isinstance(value, dict) or not isinstance(value.get("entity"), str):
            result.append(diagnostic("WDL-STATE-012", f"state {key!r} must be an entity reference", owner, field))
        else:
            target = world.maybe_get(str(value["entity"]))
            expected = schema.get("entity_kind")
            if target is None:
                result.append(diagnostic("WDL-STATE-013", f"state {key!r} references unknown entity {value['entity']}", owner, field))
            elif expected and target.kind != expected:
                result.append(
                    diagnostic(
                        "WDL-STATE-014",
                        f"state {key!r} requires {expected}, got {target.kind} {target.id}",
                        owner,
                        field,
                    )
                )
    return result


def _apply_validation_effect(state: dict[str, Any], effect: dict[str, Any]) -> None:
    key = str(effect.get("key") or "")
    operation = effect.get("operation")
    if operation == "set":
        state[key] = effect.get("value")
    elif operation == "clear":
        state.pop(key, None)
    elif operation == "add-to-set":
        values = list(state.get(key) or []) if isinstance(state.get(key), list) else []
        if effect.get("value") not in values:
            values.append(effect.get("value"))
        state[key] = values
    elif operation == "remove-from-set":
        values = list(state.get(key) or []) if isinstance(state.get(key), list) else []
        state[key] = [item for item in values if item != effect.get("value")]


def _exclusive_group_errors(world: World, owner: Record, state: dict[str, Any], field: str) -> list[dict[str, Any]]:
    groups: dict[str, list[str]] = {}
    for key in state:
        schema = _state_schema(world, owner, str(key)) or {}
        group = schema.get("exclusive_group")
        if group:
            groups.setdefault(str(group), []).append(str(key))
    return [
        diagnostic(
            "WDL-STATE-015",
            f"exclusive state group {group!r} has multiple active keys: {', '.join(sorted(keys))}",
            owner,
            field,
        )
        for group, keys in groups.items()
        if len(keys) > 1
    ]


def _validate_current_active_scenes(world: World) -> list[dict[str, Any]]:
    """Validate the shared cursor contract used when a party splits.

    A singleton active scene deliberately remains compatible with older worlds:
    its scene cursor is the effective world cursor.  Two or more active fronts
    must instead opt into an explicit world cursor so no read accidentally
    depends on record ordering.
    """
    result: list[dict[str, Any]] = []
    active = world.active_scenes()
    raw_cursor = world.config.get("current_time")
    cursor = world.current_time
    if raw_cursor is not None and cursor is None:
        result.append(diagnostic("WDL-CURSOR-001", "current_time must be a valid declared story time", world.world_record, "current_time"))
    # A singleton may omit the cursor for legacy compatibility.  Once an
    # author explicitly declares it, however, it is the world cursor and must
    # agree exactly with every live front just as it does for a split party.
    if cursor is not None:
        for scene in active:
            current = _point((scene.frontmatter.get("time") or {}).get("current"), world.default_timeline)
            if current != cursor:
                result.append(diagnostic(
                    "WDL-CURSOR-005",
                    "active scene time.current must equal world.current_time",
                    scene,
                    "time.current",
                ))
    if len(active) <= 1:
        return result
    if raw_cursor is None:
        result.append(diagnostic("WDL-CURSOR-002", "world.current_time is required when more than one scene is active", world.world_record, "current_time"))
        return result
    if cursor is None:
        return result

    present_by_character: dict[str, Record] = {}
    objects_by_id: dict[str, Record] = {}
    for scene in active:
        if not scene_contains_time(scene, cursor, world.default_timeline):
            result.append(diagnostic("WDL-CURSOR-003", "active scene does not contain world.current_time", scene, "time"))
            continue
        time_range = scene.frontmatter.get("time") or {}
        start = _point(time_range.get("start"), world.default_timeline)
        current = _point(time_range.get("current"), world.default_timeline)
        if start is None or current is None or start.timeline != cursor.timeline or current.timeline != cursor.timeline:
            result.append(diagnostic("WDL-CURSOR-004", "active scenes must use the world.current_time timeline", scene, "time"))
        for participant in scene.frontmatter.get("participants") or []:
            if not isinstance(participant, dict) or not participant.get("character"):
                continue
            character_id = str(participant["character"])
            if participant_at(scene, character_id, cursor, world.default_timeline) is None:
                continue
            previous = present_by_character.setdefault(character_id, scene)
            if previous is not scene:
                result.append(diagnostic("WDL-SCENE-001", f"active participant {character_id!r} is present in both {previous.title!r} and {scene.title!r}", scene, "participants"))
        for value in scene.frontmatter.get("objects") or []:
            object_id = str(value)
            previous = objects_by_id.setdefault(object_id, scene)
            if previous is not scene:
                result.append(diagnostic("WDL-SCENE-024", f"active scene object {object_id!r} is listed in both {previous.title!r} and {scene.title!r}", scene, "objects"))
    return result


def _validate_state_semantics(world: World) -> list[dict[str, Any]]:
    """Validate state key types and replay exclusive placement groups once."""
    result: list[dict[str, Any]] = []
    states: dict[str, dict[str, Any]] = {}
    for record in world:
        initial = record.frontmatter.get("initial_state") or {}
        if not isinstance(initial, dict):
            result.append(diagnostic("WDL-STATE-016", "initial_state must be a mapping", record, "initial_state"))
            continue
        states[record.id] = dict(initial)
        for key, value in initial.items():
            result.extend(_validate_state_value(world, record, str(key), value, f"initial_state.{key}"))
        result.extend(_exclusive_group_errors(world, record, states[record.id], "initial_state"))

    writes_at: dict[tuple[str, int, int, str, str], Record] = {}
    for event in canonical_events(world):
        point = event_time(event, world.default_timeline)
        for index, effect in enumerate(event.frontmatter.get("effects") or []):
            if not isinstance(effect, dict):
                continue
            target_id = str(effect.get("target") or "")
            owner = world.maybe_get(target_id)
            if owner is None:
                result.append(diagnostic("WDL-STATE-017", f"effect targets unknown entity {target_id}", event, f"effects[{index}].target"))
                continue
            key = str(effect.get("key") or "")
            operation = effect.get("operation")
            # A shared cursor has no arbitrary "event id wins" ordering.  Two
            # canonical writes to the same state cell at the exact coordinate
            # are therefore an authoring conflict, even if their values happen
            # to be equal.
            write_key = (point.timeline, point.tick, point.order, target_id, key)
            previous = writes_at.setdefault(write_key, event)
            if previous is not event:
                result.append(diagnostic(
                    "WDL-STATE-020",
                    f"state key {key!r} for {target_id!r} is also written by {previous.title!r} at the same story time",
                    event,
                    f"effects[{index}]",
                ))
            if operation in {"set", "add-to-set", "remove-from-set"}:
                result.extend(_validate_state_value(world, owner, key, effect.get("value"), f"effects[{index}].value"))
            elif operation == "clear" and "value" in effect:
                result.append(diagnostic("WDL-STATE-018", "clear effect must not carry a value", event, f"effects[{index}].value", severity="warning"))
            state = states.setdefault(owner.id, dict(owner.frontmatter.get("initial_state") or {}))
            _apply_validation_effect(state, effect)
            result.extend(_exclusive_group_errors(world, owner, state, f"event:{event.id}.effects[{index}]"))
    return result


def _validate_active_scene_continuity(world: World) -> list[dict[str, Any]]:
    """Require every live front to agree with replayed shared-cursor state."""
    scenes = world.active_scenes()
    if not scenes:
        return []
    result: list[dict[str, Any]] = []
    at = world.current_time or effective_time(world, scenes[0])
    for scene in scenes:
        location_id = str(scene.frontmatter.get("location") or "")
        present = {
            str(item.get("character"))
            for item in scene.frontmatter.get("participants") or []
            if isinstance(item, dict)
            and item.get("character")
            and participant_at(scene, str(item["character"]), at, world.default_timeline)
        }
        for character_id in sorted(present):
            character = world.maybe_get(character_id)
            if character is None:
                continue
            state, _citations = resolve_state(world, character_id, at)
            observed = state.get("location")
            actual = observed.get("entity") if isinstance(observed, dict) else None
            if actual != location_id:
                result.append(
                    diagnostic(
                        "WDL-SCENE-020",
                        f"active participant {character.title} resolves to location {actual!r}, not scene location {location_id!r}",
                        scene,
                        "participants",
                    )
                )

        object_states = {object_id: resolve_state(world, str(object_id), at)[0] for object_id in scene.frontmatter.get("objects") or []}

        def accessible(object_id: str, visiting: set[str] | None = None) -> bool:
            visiting = set() if visiting is None else visiting
            if object_id in visiting:
                return False
            visiting.add(object_id)
            state = object_states.get(object_id) or resolve_state(world, object_id, at)[0]
            holder = state.get("holder")
            if isinstance(holder, dict) and holder.get("entity") in present:
                return True
            location = state.get("location")
            if isinstance(location, dict) and location.get("entity") == location_id:
                return True
            container = state.get("container")
            if isinstance(container, dict) and isinstance(container.get("entity"), str):
                return accessible(str(container["entity"]), visiting)
            return False

        for object_id in scene.frontmatter.get("objects") or []:
            object_id = str(object_id)
            record = world.maybe_get(object_id)
            if record and not accessible(object_id):
                result.append(
                    diagnostic(
                        "WDL-SCENE-021",
                        f"active scene object {record.title} is not located in the scene, held by a present participant, or contained by an accessible object",
                        scene,
                        "objects",
                    )
                )
    return result


def _point_label(point: StoryTime) -> str:
    """Render a coordinate for author-facing validation diagnostics."""
    return f"{point.timeline} {point.tick}:{point.order}"


def _presence_window(scene: Record, participant: dict[str, Any], default_timeline: str) -> tuple[StoryTime, StoryTime] | None:
    """Return the known portion of one scene-presence interval.

    An active scene is deliberately open-ended in the source model.  Validation
    can only make historical claims through its current cursor, so its window
    ends there until an author advances or closes the scene.
    """
    time_range = scene.frontmatter.get("time") or {}
    scene_start = _point(time_range.get("start"), default_timeline)
    scene_end = _point(time_range.get("end") or time_range.get("current"), default_timeline)
    if scene_start is None or scene_end is None or scene_start.timeline != scene_end.timeline:
        return None
    start = _point(participant.get("from") or time_range.get("start"), default_timeline)
    end = _point(participant.get("to") or time_range.get("end") or time_range.get("current"), default_timeline)
    if start is None or end is None or start.timeline != scene_start.timeline or end.timeline != scene_start.timeline:
        return None
    # Do not claim a presence beyond the portion of the scene which exists in
    # the current source revision.  A malformed participant range is handled
    # by the scene's own temporal diagnostics; clamping here prevents it from
    # manufacturing a false double-booking in a later scene.
    start = max(start, scene_start)
    end = min(end, scene_end)
    return (start, end) if start <= end else None


def _interval_overlap(
    first: tuple[StoryTime, StoryTime], second: tuple[StoryTime, StoryTime]
) -> tuple[StoryTime, StoryTime] | None:
    """Return an inclusive intersection, or ``None`` for an adjacent handoff."""
    first_start, first_end = first
    second_start, second_end = second
    if first_start.timeline != second_start.timeline:
        return None
    start = max(first_start, second_start)
    end = min(first_end, second_end)
    return (start, end) if start <= end else None


def _validate_historical_scene_presence(world: World) -> list[dict[str, Any]]:
    """Reject character double-booking throughout the known shared chronology.

    Active-scene validation protects the author horizon.  This complementary
    pass covers closed scenes and the known portions of active scenes, so a
    later split/reunion cannot conceal an earlier incompatible placement.
    Presence endpoints are inclusive: handing a character from one scene to
    another at the same coordinate is still two-place participation.  Moving
    the receiving scene to the next order is the explicit, safe handoff.
    """
    by_character: dict[str, list[tuple[Record, tuple[StoryTime, StoryTime]]]] = {}
    for scene in world.by_kind("scene"):
        if scene.status not in {"active", "closed"}:
            continue
        for participant in scene.frontmatter.get("participants") or []:
            if not isinstance(participant, dict) or not isinstance(participant.get("character"), str):
                continue
            window = _presence_window(scene, participant, world.default_timeline)
            if window is not None:
                by_character.setdefault(participant["character"], []).append((scene, window))

    result: list[dict[str, Any]] = []
    for character_id, presences in by_character.items():
        character = world.maybe_get(character_id)
        # Invalid references have their own general reference diagnostic.  Do
        # not add a misleading double-booking error for a missing character.
        if character is None:
            continue
        for index, (first_scene, first_window) in enumerate(presences):
            for second_scene, second_window in presences[index + 1:]:
                overlap = _interval_overlap(first_window, second_window)
                if overlap is None:
                    continue
                start, end = overlap
                first_location = world.maybe_get(str(first_scene.frontmatter.get("location") or ""))
                second_location = world.maybe_get(str(second_scene.frontmatter.get("location") or ""))
                first_place = first_location.title if first_location and first_location.kind == "location" else "an unresolved place"
                second_place = second_location.title if second_location and second_location.kind == "location" else "an unresolved place"
                interval = _point_label(start) if start == end else f"{_point_label(start)} through {_point_label(end)}"
                result.append(diagnostic(
                    "WDL-SCENE-025",
                    f"{character.title} is present in both {first_scene.title!r} and {second_scene.title!r} at {interval}; end one presence before the other begins",
                    second_scene,
                    "participants",
                ))
                if first_scene.frontmatter.get("location") != second_scene.frontmatter.get("location"):
                    result.append(diagnostic(
                        "WDL-SCENE-026",
                        f"{character.title} cannot be at both {first_place!r} in {first_scene.title!r} and {second_place!r} in {second_scene.title!r} at {interval}",
                        second_scene,
                        "participants",
                    ))
    return result


def _validate_concurrent_event_participation(world: World) -> list[dict[str, Any]]:
    """Reject a character's same-coordinate canonical events at two places.

    Events are instants, so unlike scene presence there is no interval to
    hand off: a later order is already a different coordinate.  Several event
    records can describe facets of the same occurrence at one place; that is
    intentionally permitted.
    """
    seen: dict[tuple[str, int, int, str], Record] = {}
    result: list[dict[str, Any]] = []
    for event in canonical_events(world):
        point = event_time(event, world.default_timeline)
        location_id = str(event.frontmatter.get("location") or "")
        if not location_id:
            continue
        for participant in event.frontmatter.get("participants") or []:
            if not isinstance(participant, dict) or not isinstance(participant.get("character"), str):
                continue
            character_id = participant["character"]
            key = (point.timeline, point.tick, point.order, character_id)
            previous = seen.setdefault(key, event)
            if previous is event or previous.frontmatter.get("location") == location_id:
                continue
            character = world.maybe_get(character_id)
            previous_location = world.maybe_get(str(previous.frontmatter.get("location") or ""))
            current_location = world.maybe_get(location_id)
            character_name = character.title if character and character.kind == "character" else "the referenced character"
            previous_place = previous_location.title if previous_location and previous_location.kind == "location" else "an unresolved place"
            current_place = current_location.title if current_location and current_location.kind == "location" else "an unresolved place"
            result.append(diagnostic(
                "WDL-EVENT-006",
                f"{character_name} participates in both {previous.title!r} at {previous_place!r} and {event.title!r} at {current_place!r} at {_point_label(point)}; use a later order for a handoff",
                event,
                "participants",
            ))
    return result


def _validate_transitions(world: World, record: Record, field: str, prefix: str, allowed_states: set[str] | None) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    values = record.frontmatter.get(field) or []
    if not isinstance(values, list) or not _strictly_ordered([item for item in values if isinstance(item, dict)], world.default_timeline):
        if values:
            result.append(diagnostic("WDL-TIME-001", f"{field} must be strictly ordered", record, field))
    seen: set[str] = set()
    for index, transition in enumerate(values):
        if not isinstance(transition, dict):
            continue
        identifier = str(transition.get("id") or "")
        if not valid_id(identifier, prefix):
            result.append(diagnostic("WDL-ID-002", f"invalid transition ID {identifier!r}", record, f"{field}[{index}].id"))
        if identifier in seen:
            result.append(diagnostic("WDL-ID-003", "duplicate transition ID", record, f"{field}[{index}].id"))
        seen.add(identifier)
        if allowed_states and transition.get("state") not in allowed_states:
            result.append(diagnostic("WDL-STATE-001", "invalid transition state", record, f"{field}[{index}].state"))
    return result


def _location_target(value: Any) -> str | None:
    """Read one location target without collapsing its authored route prose."""
    if isinstance(value, str) and value.strip():
        return value
    if not isinstance(value, dict):
        return None
    aliases = [key for key in ("location", "entity", "target") if key in value]
    if len(aliases) != 1:
        return None
    target = value[aliases[0]]
    return target if isinstance(target, str) and target.strip() else None


def _invalid_location_link_prose_fields(value: Any) -> tuple[str, ...]:
    """Return malformed detailed-link prose fields for every source envelope.

    The spatial component retains ordinary directional location links, so this
    narrow legacy rule is shared rather than re-specified by a second parser.
    """
    if not isinstance(value, dict):
        return ()
    return tuple(
        prose_key
        for prose_key in ("description", "label", "summary")
        if prose_key in value and (not isinstance(value[prose_key], str) or not value[prose_key].strip())
    )


def _validate_location_reference(
    world: World,
    owner: Record,
    target_id: str,
    field: str,
    *,
    parent: bool = False,
) -> list[dict[str, Any]]:
    target = world.maybe_get(target_id)
    if target is None:
        # The general reference walk reports the unknown entity when it is an
        # entity-shaped ID.  This diagnostic also covers malformed plain text
        # while keeping the location-specific field actionable.
        return [diagnostic(
            "WDL-LOC-002" if parent else "WDL-LOC-006",
            f"{'location parent' if parent else 'location link'} must reference an existing location",
            owner,
            field,
        )]
    if target.kind != "location":
        return [diagnostic(
            "WDL-LOC-002" if parent else "WDL-LOC-006",
            f"{'location parent' if parent else 'location link'} must reference a location, not {target.kind} {target.title!r}",
            owner,
            field,
        )]
    return []


def _validate_location(world: World, record: Record) -> list[dict[str, Any]]:
    """Validate containment and authored, directional location routes."""
    result: list[dict[str, Any]] = []
    parent = record.frontmatter.get("parent")
    if parent is not None:
        if not isinstance(parent, str) or not parent.strip():
            result.append(diagnostic("WDL-LOC-001", "location parent must be null or a location reference", record, "parent"))
        elif parent == record.id:
            result.append(diagnostic("WDL-LOC-003", "location cannot be its own parent", record, "parent"))
        else:
            result.extend(_validate_location_reference(world, record, parent, "parent", parent=True))

    links = record.frontmatter.get("links", [])
    if not isinstance(links, list):
        return result + [diagnostic("WDL-LOC-004", "location links must be an array", record, "links")]
    seen: set[str] = set()
    for index, link in enumerate(links):
        field = f"links[{index}]"
        target_id = _location_target(link)
        if target_id is None:
            result.append(diagnostic("WDL-LOC-005", "location link must be a location reference or a mapping with exactly one location, entity, or target reference", record, field))
            continue
        for prose_key in _invalid_location_link_prose_fields(link):
            result.append(diagnostic("WDL-LOC-009", f"location link {prose_key} must be non-empty text", record, f"{field}.{prose_key}"))
        if target_id == record.id:
            result.append(diagnostic("WDL-LOC-007", "location cannot link to itself", record, field))
        if target_id in seen:
            result.append(diagnostic("WDL-LOC-008", f"duplicate directional link to {target_id!r}", record, field))
        seen.add(target_id)
        result.extend(_validate_location_reference(world, record, target_id, field))
    return result


def _validate_scene(world: World, record: Record) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    time_range = record.frontmatter.get("time") or {}
    start = _point(time_range.get("start"), world.default_timeline)
    current = _point(time_range.get("current"), world.default_timeline)
    end = _point(time_range.get("end"), world.default_timeline) if time_range.get("end") else None
    if start is None or current is None:
        result.append(diagnostic("WDL-SCENE-002", "scene requires start and current time", record, "time"))
        return result
    if start.timeline != current.timeline:
        result.append(diagnostic("WDL-SCENE-022", "scene start and current time must share a timeline", record, "time.current"))
    elif (current.tick, current.order) < (start.tick, start.order):
        result.append(diagnostic("WDL-SCENE-003", "scene current time precedes start", record, "time.current"))
    if record.status == "closed" and end is None:
        result.append(diagnostic("WDL-SCENE-004", "closed scene requires an end time", record, "time.end"))
    if record.status == "active" and end is not None:
        result.append(diagnostic("WDL-SCENE-005", "active scene must not have an end time", record, "time.end"))
    if end:
        if end.timeline != start.timeline:
            result.append(diagnostic("WDL-SCENE-023", "scene start and end time must share a timeline", record, "time.end"))
        elif current.timeline == end.timeline and (current.tick, current.order) > (end.tick, end.order):
            result.append(diagnostic("WDL-SCENE-006", "scene current time follows end", record, "time.current"))
    participant_ids: set[str] = set()
    for index, participant in enumerate(record.frontmatter.get("participants") or []):
        if not isinstance(participant, dict) or not participant.get("character"):
            result.append(diagnostic("WDL-SCENE-007", "participant must be a mapping with character", record, f"participants[{index}]"))
            continue
        character_id = str(participant["character"])
        if character_id in participant_ids:
            result.append(diagnostic("WDL-SCENE-008", "duplicate scene participant", record, f"participants[{index}]"))
        participant_ids.add(character_id)
    observations = record.frontmatter.get("observations") or []
    if not isinstance(observations, list):
        result.append(diagnostic("WDL-SCENE-009", "observations must be timed audience records", record, "observations"))
        return result
    seen: set[str] = set()
    for index, observation in enumerate(observations):
        if not isinstance(observation, dict):
            result.append(diagnostic("WDL-SCENE-010", "observation must be a mapping", record, f"observations[{index}]"))
            continue
        identifier = str(observation.get("id") or "")
        if not valid_id(identifier, "observation"):
            result.append(diagnostic("WDL-SCENE-011", "invalid observation ID", record, f"observations[{index}].id"))
        if identifier in seen:
            result.append(diagnostic("WDL-SCENE-012", "duplicate observation ID", record, f"observations[{index}].id"))
        seen.add(identifier)
        point = _point(observation.get("at"), world.default_timeline)
        if point is None or not scene_contains_time(record, point, world.default_timeline):
            result.append(diagnostic("WDL-SCENE-013", "observation time lies outside scene", record, f"observations[{index}].at"))
        audience = observation.get("audience") or ["participants"]
        if isinstance(audience, str):
            audience = [audience]
        for token in audience:
            target = token[5:] if isinstance(token, str) and token.startswith("char:") else token
            if isinstance(target, str) and target.startswith("char_") and target not in participant_ids:
                result.append(diagnostic("WDL-SCENE-014", "private observation targets a non-participant", record, f"observations[{index}].audience"))
            if point and isinstance(target, str) and target.startswith("char_") and participant_at(record, target, point, world.default_timeline) is None:
                result.append(diagnostic("WDL-SCENE-015", "observation targets character outside their presence interval", record, f"observations[{index}].audience"))
    outcomes = record.frontmatter.get("outcome_events")
    if outcomes is None:
        return result
    if not isinstance(outcomes, list):
        result.append(diagnostic("WDL-SCENE-027", "scene outcome_events must be an array of event references", record, "outcome_events"))
        return result
    seen_outcomes: set[str] = set()
    boundary = end or current
    for index, event_id in enumerate(outcomes):
        field = f"outcome_events[{index}]"
        event = world.maybe_get(event_id) if isinstance(event_id, str) else None
        if event is None or event.kind != "event" or event_id in seen_outcomes:
            result.append(diagnostic("WDL-SCENE-027", "scene outcome_events must contain unique event references", record, field))
            continue
        seen_outcomes.add(event_id)
        event_point = _point(event.frontmatter.get("time"), world.default_timeline)
        in_scene = (
            event.status == "canonical" and event_point is not None and
            event_point.timeline == start.timeline and
            (start.tick, start.order) <= (event_point.tick, event_point.order) <= (boundary.tick, boundary.order)
        )
        if not in_scene:
            result.append(diagnostic("WDL-SCENE-028", "scene outcome event must be canonical and occur within the scene interval", record, field))
    return result


def _validate_conversation(world: World, record: Record) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    time_range = record.frontmatter.get("time") or {}
    start = _point(time_range.get("start"), world.default_timeline)
    end = _point(time_range.get("end"), world.default_timeline) if time_range.get("end") else None
    if start is None:
        result.append(diagnostic("WDL-CONV-001", "conversation requires a start time", record, "time.start"))
        return result
    if end and end.timeline != start.timeline:
        result.append(diagnostic("WDL-CONV-014", "conversation start and end time must share a timeline", record, "time.end"))
    elif end and (end.tick, end.order) < (start.tick, start.order):
        result.append(diagnostic("WDL-CONV-015", "conversation end time precedes start", record, "time.end"))
    if record.status == "closed" and end is None:
        result.append(diagnostic("WDL-CONV-002", "closed conversation requires an end time", record, "time.end"))
    participants = {str(item.get("character")) for item in record.frontmatter.get("participants") or [] if isinstance(item, dict) and item.get("character")}
    turns = record.frontmatter.get("turns") or []
    if record.status == "closed" and not turns:
        result.append(diagnostic("WDL-CONV-003", "closed conversation requires verbatim turns", record, "turns"))
    if turns and not _strictly_ordered([item for item in turns if isinstance(item, dict)], world.default_timeline):
        result.append(diagnostic("WDL-CONV-004", "conversation turns must be strictly ordered", record, "turns"))
    turn_map: dict[str, dict[str, Any]] = {}
    earlier_speech_ids: set[str] = set()
    for index, turn in enumerate(turns):
        if not isinstance(turn, dict):
            continue
        identifier = str(turn.get("id") or "")
        if not valid_id(identifier, "conversation-turn"):
            result.append(diagnostic("WDL-CONV-005", "invalid turn ID", record, f"turns[{index}].id"))
        turn_map[identifier] = turn
        point = _point(turn.get("at"), world.default_timeline)
        kind = beat_kind(turn)
        if kind not in {"speech", "action"}:
            result.append(diagnostic("WDL-CONV-016", "turn kind must be speech or action", record, f"turns[{index}].kind"))
        allowed_fields = {
            "speech": {"id", "kind", "at", "speaker", "addressee", "text", "delivery", "audience", "interrupts"},
            "action": {"id", "kind", "at", "actors", "text", "audience"},
        }
        if kind in allowed_fields:
            for field in sorted(set(turn) - allowed_fields[kind]):
                result.append(diagnostic(
                    "WDL-CONV-028",
                    f"{kind} beat does not permit field {field!r}",
                    record,
                    f"turns[{index}].{field}",
                ))
        if kind == "speech":
            speaker = str(turn.get("speaker") or "")
            if speaker not in participants:
                result.append(diagnostic("WDL-CONV-006", "turn speaker is not a participant", record, f"turns[{index}].speaker"))
            if point and conversation_participant_at(record, speaker, point, world.default_timeline) is None:
                result.append(diagnostic("WDL-CONV-007", "turn speaker is outside presence interval", record, f"turns[{index}].at"))
            if "addressee" in turn:
                addressee = turn.get("addressee")
                target = addressee[5:] if isinstance(addressee, str) and addressee.startswith("char:") else addressee
                if target == "participants":
                    pass
                elif not isinstance(target, str) or target not in participants:
                    result.append(diagnostic("WDL-CONV-017", "speech addressee must be a conversation participant or participants", record, f"turns[{index}].addressee"))
                elif point and conversation_participant_at(record, target, point, world.default_timeline) is None:
                    result.append(diagnostic("WDL-CONV-018", "speech addressee is outside presence interval", record, f"turns[{index}].addressee"))
            if "interrupts" in turn:
                interrupted = turn.get("interrupts")
                if not isinstance(interrupted, str) or interrupted not in earlier_speech_ids:
                    result.append(diagnostic("WDL-CONV-021", "speech interruption must target an earlier spoken turn", record, f"turns[{index}].interrupts"))
        elif kind == "action":
            actors = turn.get("actors")
            if not isinstance(actors, list) or not actors:
                result.append(diagnostic("WDL-CONV-022", "action beat requires a non-empty actors list", record, f"turns[{index}].actors"))
            else:
                actor_ids = [item[5:] if isinstance(item, str) and item.startswith("char:") else item for item in actors]
                string_actor_ids = [actor for actor in actor_ids if isinstance(actor, str)]
                if len(actor_ids) != len(string_actor_ids) or len(string_actor_ids) != len(set(string_actor_ids)):
                    result.append(diagnostic("WDL-CONV-023", "action beat actors must be unique", record, f"turns[{index}].actors"))
                for actor_index, actor in enumerate(actor_ids):
                    if not isinstance(actor, str) or actor not in participants:
                        result.append(diagnostic("WDL-CONV-024", "action actor is not a participant", record, f"turns[{index}].actors[{actor_index}]"))
                    elif point and conversation_participant_at(record, actor, point, world.default_timeline) is None:
                        result.append(diagnostic("WDL-CONV-025", "action actor is outside presence interval", record, f"turns[{index}].actors[{actor_index}]"))
            for forbidden in ("effects", "state", "mutations", "state_changes"):
                if forbidden in turn:
                    result.append(diagnostic("WDL-CONV-026", "action beats are choreography and cannot mutate durable state", record, f"turns[{index}].{forbidden}"))
        if not isinstance(turn.get("text"), str) or not turn.get("text", "").strip():
            result.append(diagnostic("WDL-CONV-008", "turn text must be non-empty", record, f"turns[{index}].text"))
        audience = turn.get("audience") or ["participants"]
        if isinstance(audience, str):
            audience = [audience]
        if not isinstance(audience, list) or not audience:
            result.append(diagnostic("WDL-CONV-019", "turn audience must be a non-empty audience list", record, f"turns[{index}].audience"))
        else:
            for audience_index, audience_member in enumerate(audience):
                target = audience_member[5:] if isinstance(audience_member, str) and audience_member.startswith("char:") else audience_member
                if target in {"participants", "public"}:
                    continue
                if not isinstance(target, str) or target not in participants:
                    result.append(diagnostic("WDL-CONV-019", "turn audience target is not a conversation participant", record, f"turns[{index}].audience[{audience_index}]"))
                elif point and conversation_participant_at(record, target, point, world.default_timeline) is None:
                    result.append(diagnostic("WDL-CONV-020", "turn audience target is outside presence interval", record, f"turns[{index}].audience[{audience_index}]"))
        if kind == "speech":
            earlier_speech_ids.add(identifier)
    recollections = record.frontmatter.get("recollections") or []
    for index, recollection in enumerate(recollections):
        if not isinstance(recollection, dict):
            continue
        identifier = str(recollection.get("id") or "")
        if not valid_id(identifier, "conversation-recollection"):
            result.append(diagnostic("WDL-CONV-009", "invalid recollection ID", record, f"recollections[{index}].id"))
        character_id = str(recollection.get("character") or "")
        if character_id not in participants:
            result.append(diagnostic("WDL-CONV-010", "recollection owner was not a participant/listener", record, f"recollections[{index}].character"))
        point = _point(recollection.get("at"), world.default_timeline)
        if point is None or not start.not_after(point):
            result.append(diagnostic("WDL-CONV-011", "recollection time precedes conversation", record, f"recollections[{index}].at"))
            continue
        for turn_id in recollection.get("exact_turns") or []:
            turn = turn_map.get(str(turn_id))
            if turn is None:
                result.append(diagnostic("WDL-CONV-012", f"unknown exact turn {turn_id}", record, f"recollections[{index}].exact_turns"))
            elif beat_kind(turn) != "speech":
                result.append(diagnostic("WDL-CONV-027", "exact remembered quotes may cite spoken turns only", record, f"recollections[{index}].exact_turns"))
            elif not turn_visible_to(record, turn, character_id, point, world.default_timeline):
                result.append(diagnostic("WDL-CONV-013", "character cannot exactly remember an inaudible turn", record, f"recollections[{index}].exact_turns"))
    return result


def _validate_story_point(world: World, record: Record) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    lifecycle = record.frontmatter.get("lifecycle") or {}
    if "state" in lifecycle:
        result.append(diagnostic("WDL-SP-001", "lifecycle.state is forbidden; use initial_state and timed transitions", record, "lifecycle.state"))
    if lifecycle.get("initial_state") not in {"dormant", "active", "resolved", "failed", "cancelled"}:
        result.append(diagnostic("WDL-SP-002", "invalid initial story-point state", record, "lifecycle.initial_state"))
    transitions = lifecycle.get("transitions") or []
    if transitions and not _strictly_ordered([item for item in transitions if isinstance(item, dict)], world.default_timeline):
        result.append(diagnostic("WDL-SP-003", "story-point transitions must be strictly ordered", record, "lifecycle.transitions"))
    for index, transition in enumerate(transitions):
        if isinstance(transition, dict) and transition.get("state") == "eligible":
            result.append(diagnostic("WDL-SP-004", "eligibility is derived and cannot be a stored transition", record, f"lifecycle.transitions[{index}].state"))
    outcomes = record.frontmatter.get("outcome_events", [])
    if not isinstance(outcomes, list):
        result.append(diagnostic("WDL-SP-005", "outcome_events must be an array of event references", record, "outcome_events"))
        return result
    seen: set[str] = set()
    reference_timeline = next(
        (_point(item.get("time"), world.default_timeline).timeline for item in transitions
         if isinstance(item, dict) and _point(item.get("time"), world.default_timeline) is not None),
        None,
    )
    previous_outcome: StoryTime | None = None
    for index, event_id in enumerate(outcomes):
        field = f"outcome_events[{index}]"
        if not isinstance(event_id, str) or not event_id:
            result.append(diagnostic("WDL-SP-006", "outcome event must be an event reference", record, field))
            continue
        if event_id in seen:
            result.append(diagnostic("WDL-SP-007", "outcome_events must not contain duplicates", record, field))
            continue
        seen.add(event_id)
        event = world.maybe_get(event_id)
        if event is None or event.kind != "event":
            result.append(diagnostic("WDL-SP-006", "outcome event must reference an event", record, field))
            continue
        if event.status != "canonical":
            result.append(diagnostic("WDL-SP-008", "outcome event must be canonical", record, field))
        point = _point(event.frontmatter.get("time"), world.default_timeline)
        if reference_timeline is not None and point is not None:
            if point.timeline != reference_timeline:
                result.append(diagnostic("WDL-SP-009", "outcome event must share the story point timeline", record, field))
        if point is not None and previous_outcome is not None and (
            point.timeline != previous_outcome.timeline or (point.tick, point.order) <= (previous_outcome.tick, previous_outcome.order)
        ):
            result.append(diagnostic("WDL-SP-010", "outcome_events must be in strictly increasing story-time order", record, field))
        if point is not None:
            previous_outcome = point
    return result


def _validate_event_causality(world: World) -> list[dict[str, Any]]:
    """Validate the intentionally direct event-to-event causal graph.

    ``causes`` stays an untyped source list: it does not claim physical
    mechanism or force a narrative interpretation.  It merely records an
    authored event edge, which is enough to make chronology-safe DAG reads.
    """
    result: list[dict[str, Any]] = []
    graph: dict[str, list[str]] = {}
    for event in world.by_kind("event"):
        causes = event.frontmatter.get("causes", [])
        if not isinstance(causes, list):
            result.append(diagnostic("WDL-EVENT-007", "event causes must be an array of event references", event, "causes"))
            continue
        seen: set[str] = set()
        event_point = _point(event.frontmatter.get("time"), world.default_timeline)
        for index, cause_id in enumerate(causes):
            field = f"causes[{index}]"
            if not isinstance(cause_id, str) or not cause_id:
                result.append(diagnostic("WDL-EVENT-008", "event cause must reference an event", event, field))
                continue
            cause = world.maybe_get(cause_id)
            if cause is None or cause.kind != "event":
                result.append(diagnostic("WDL-EVENT-008", "event cause must reference an event", event, field))
                continue
            if cause_id == event.id or cause_id in seen:
                result.append(diagnostic("WDL-EVENT-009", "event causes must be unique and cannot reference the event itself", event, field))
                continue
            seen.add(cause_id)
            graph.setdefault(event.id, []).append(cause_id)
            if cause.status != "canonical":
                result.append(diagnostic("WDL-EVENT-010", "event cause must be canonical", event, field))
            cause_point = _point(cause.frontmatter.get("time"), world.default_timeline)
            if event_point is not None and cause_point is not None:
                if cause_point.timeline != event_point.timeline:
                    result.append(diagnostic("WDL-EVENT-011", "event cause must share the caused event's timeline", event, field))
                elif (cause_point.tick, cause_point.order) >= (event_point.tick, event_point.order):
                    result.append(diagnostic("WDL-EVENT-012", "event cause must be strictly earlier than the caused event", event, field))
        related = event.frontmatter.get("related_story_points", [])
        if not isinstance(related, list):
            result.append(diagnostic("WDL-EVENT-014", "related_story_points must be an array of story-point references", event, "related_story_points"))
        else:
            related_seen: set[str] = set()
            for index, story_point_id in enumerate(related):
                field = f"related_story_points[{index}]"
                target = world.maybe_get(story_point_id) if isinstance(story_point_id, str) else None
                if target is None or target.kind != "story-point" or story_point_id in related_seen:
                    result.append(diagnostic("WDL-EVENT-014", "related_story_points must contain unique story-point references", event, field))
                if isinstance(story_point_id, str):
                    related_seen.add(story_point_id)
    visiting: set[str] = set()
    visited: set[str] = set()
    def visit(event_id: str) -> bool:
        if event_id in visiting:
            return True
        if event_id in visited:
            return False
        visiting.add(event_id)
        found = any(visit(cause_id) for cause_id in graph.get(event_id, []))
        visiting.remove(event_id)
        visited.add(event_id)
        return found
    for event_id in sorted(graph):
        if visit(event_id):
            result.append(diagnostic("WDL-EVENT-013", "event causal graph contains a cycle", world.maybe_get(event_id), "causes"))
            break
    return result


def _validate_typed_causes(world: World) -> list[dict[str, Any]]:
    """Check causal event citations attached to state and plot transitions."""
    result: list[dict[str, Any]] = []
    records: list[tuple[Record, str, list[Any]]] = []
    for record in world.by_kind("story-point"):
        lifecycle = record.frontmatter.get("lifecycle") or {}
        records.append((record, "lifecycle.transitions", lifecycle.get("transitions") or []))
    for kind in ("knowledge", "relationship"):
        for record in world.by_kind(kind):
            records.append((record, "transitions", record.frontmatter.get("transitions") or []))
    for record, field_root, transitions in records:
        for index, transition in enumerate(transitions):
            if not isinstance(transition, dict) or transition.get("causing_event") is None:
                continue
            field = f"{field_root}[{index}].causing_event"
            event_id = transition.get("causing_event")
            event = world.maybe_get(event_id) if isinstance(event_id, str) else None
            if event is None or event.kind != "event":
                result.append(diagnostic("WDL-CAUSE-001", "causing_event must reference an event", record, field))
                continue
            if event.status != "canonical":
                result.append(diagnostic("WDL-CAUSE-002", "causing_event must reference a canonical event", record, field))
            transition_point = _point(transition.get("time"), world.default_timeline)
            event_point = _point(event.frontmatter.get("time"), world.default_timeline)
            if transition_point is None:
                result.append(diagnostic("WDL-CAUSE-004", "a transition with causing_event requires a valid transition time", record, field))
            elif event_point is not None:
                if transition_point.timeline != event_point.timeline:
                    result.append(diagnostic("WDL-CAUSE-003", "causing_event must share the transition timeline", record, field))
                elif (event_point.tick, event_point.order) > (transition_point.tick, transition_point.order):
                    result.append(diagnostic("WDL-CAUSE-004", "causing_event cannot follow the transition it explains", record, field))
    return result
    return result


def _cycle_checks(world: World) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []

    def location_parent(record: Record) -> list[str]:
        parent = record.frontmatter.get("parent")
        return [parent] if isinstance(parent, str) and parent else []

    for kind, edge in (
        ("location", location_parent),
        ("story-point", lambda record: [str(item.get("story_point")) for family in (record.frontmatter.get("dependencies") or {}).values() for item in family or [] if isinstance(item, dict) and item.get("story_point")]),
    ):
        graph = {record.id: [target for target in edge(record) if target] for record in world.by_kind(kind)}
        visiting: set[str] = set()
        visited: set[str] = set()
        def visit(node: str) -> bool:
            if node in visiting:
                return True
            if node in visited:
                return False
            visiting.add(node)
            if any(target in graph and visit(target) for target in graph.get(node, [])):
                return True
            visiting.remove(node)
            visited.add(node)
            return False
        for node in graph:
            if visit(node):
                result.append(diagnostic("WDL-CYCLE-001", f"{kind} dependency cycle detected at {node}", world.maybe_get(node)))
                break
    return result
