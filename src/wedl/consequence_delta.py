"""Pure same-horizon deltas; resolved operation metadata comes from the caller.

Worlds have already passed complete source validation. Authorization grants are
server-derived AuthorScope objects. No request identity, loader, writer, validation
of artificial exclusion Worlds, or persistent cache participates here.
"""
from __future__ import annotations

from copy import copy, deepcopy
from difflib import SequenceMatcher
from typing import Any, Mapping, Sequence

from .event_consequences import (AuthorScope, CandidateIdentity, MAX_ITEMS, Projection,
                                 ProjectionFailure, bounded, citations, subject_order, time_value)
from .model import Record, StoryTime, World
from .semantics import canonical_events
from .util import canonical_json

PROTOCOL = "wedl-event-consequence-delta/v1"
_ABSENT = {"presence": "absent", "payload": None, "citations": []}


def _equal(left: Any, right: Any) -> bool:
    """JSON comparison: no Boolean/number coercion, numeric equality is exact."""
    if isinstance(left, bool) or isinstance(right, bool):
        return type(left) is type(right) and left == right
    if isinstance(left, (int, float)) and isinstance(right, (int, float)):
        return left == right
    if type(left) is not type(right):
        return False
    if isinstance(left, dict):
        return left.keys() == right.keys() and all(_equal(left[key], right[key]) for key in left)
    if isinstance(left, list):
        return len(left) == len(right) and all(_equal(a, b) for a, b in zip(left, right))
    return left == right


def _snapshot(projection: Projection, other: Projection, subject: dict) -> dict:
    identifier = subject["recordId"]
    # An admitted record truly absent from this revision is known absence. A
    # withheld/future record is unavailable, even if it exists in the other view.
    if identifier not in projection.world.records and identifier not in projection.source.records:
        if projection.scope.permits(identifier, "frontmatter.title") and identifier in other.world.records:
            return deepcopy(_ABSENT)
    if subject["kind"] == "outcome":
        target = subject["targetId"]
        if target not in projection.world.records:
            if target not in projection.source.records and projection.scope.permits(target, "frontmatter.title") and target in other.world.records:
                return deepcopy(_ABSENT)
            raise ProjectionFailure("unavailable")
    return projection.snapshot(subject)


def semantic_changes(before: Projection, after: Projection) -> list[dict]:
    """Compare every admitted semantic subject; citations alone never differ.

    This also supports event-local T views. The caller labels the comparison;
    same-horizon revision delta and event-local exclusion remain separate.
    """
    if before.at != after.at:
        raise ProjectionFailure("invalid")
    subjects = {canonical_json(value): value for value in before.subjects() + after.subjects()}
    changes = []
    for subject in sorted(subjects.values(), key=subject_order):
        old, new = _snapshot(before, after, subject), _snapshot(after, before, subject)
        if old["presence"] != new["presence"] or not _equal(old["payload"], new["payload"]):
            changes.append({"subject": subject, "before": old, "after": new})
            bounded(changes, len(changes), min(before.limit, after.limit))
    return changes


def event_local_changes(projection: Projection, event: str) -> list[dict]:
    before, after = projection.event_views(event)
    return semantic_changes(before, after)


def _time_leaves(value: Any, section: str, default: str, kind: str) -> Any:
    """Serialize only declared time leaves; opaque extension strings stay literal."""
    result = deepcopy(value)
    if section == "frontmatter.current_time" and kind == "world" and result is not None:
        result = time_value(StoryTime.from_value(result, default))
    elif section == "frontmatter.time" and kind in {"event", "scene", "environment", "conversation"}:
        if isinstance(result, dict) and "start" in result:
            for field in (("start", "current", "end") if kind == "scene" else ("start", "end")):
                if field in result and result[field] is not None:
                    result[field] = time_value(StoryTime.from_value(result[field], default))
        else:
            result = time_value(StoryTime.from_value(result, default))
    elif section in {"transitions", "lifecycle.transitions"}:
        for member in result:
            member["time"] = time_value(StoryTime.from_value(member["time"], default))
    return result


def _sections(projection: Projection, identifier: str) -> dict[str, Any]:
    record = projection.world.records.get(identifier)
    if record is None:
        return {}
    scope, result = projection.scope, {}
    # The projected record has already discarded future members/hidden links.
    # Select field permissions before reading/copying any source value.
    for field in record.frontmatter:
        section = field if field in {"effects", "transitions", "trigger", "dependencies", "causes", "related_story_points", "outcome_events"} else "frontmatter." + field
        if field == "initial_state":
            if scope.permits(identifier, "initial_state"):
                for key, value in (record.frontmatter[field] or {}).items():
                    result["initial_state." + key] = deepcopy(value)
            else:
                for section in (scope.sections or {}).get(identifier, ()):
                    if section.startswith("initial_state."):
                        key = section[len("initial_state."):]
                        if key in (record.frontmatter.get(field) or {}):
                            result[section] = deepcopy(record.frontmatter[field][key])
        elif field == "lifecycle":
            for key in record.frontmatter[field]:
                section = "lifecycle." + key
                if scope.permits(identifier, section):
                    result[section] = _time_leaves(record.frontmatter[field][key], section, projection.world.default_timeline, record.kind)
        elif scope.permits(identifier, section):
            result[section] = _time_leaves(record.frontmatter[field], section, projection.world.default_timeline, record.kind)
    if scope.permits(identifier, "bodyMarkdown"):
        result["bodyMarkdown"] = projection.source.records[identifier].body
    return result


def _record_view(projection: Projection) -> Projection:
    """Empty untimed histories still have authored record content.

    This separate view is used only for record deltas, never semantic folds or
    reference resolution in a public read. Future-only histories stay excluded.
    """
    view = copy(projection)
    records = dict(projection.world.records)
    for kind, identifier in projection._known_absent:
        source = projection.source.records[identifier]
        if not source.frontmatter.get("transitions"):
            fm = {key: value for key, value in source.frontmatter.items() if key != "transitions"}
            fm["transitions"] = []
            records[identifier] = Record(fm, "", source.source_path, source.raw_bytes, source.blob_oid, source.revision)
    view.world = World(projection.world.revision, projection.world.tree_oid, records,
                       projection.world.root, projection.world.source_root)
    return view


def _changed_members(old: list, new: list) -> list:
    # Stable IDs align authored history across inserts. Legacy ID-less members
    # align by their complete literal value in source order (no invented IDs).
    # Moves remain changes; an insertion merely shifting history does not.
    def key(member):
        return ("id", member["id"]) if isinstance(member, dict) and isinstance(member.get("id"), str) else ("value", canonical_json(member))
    result = []
    for tag, i, j, k, l in SequenceMatcher(None, [key(value) for value in old], [key(value) for value in new], autojunk=False).get_opcodes():
        if tag == "equal":
            for a, b in zip(old[i:j], new[k:l]):
                if canonical_json(a) != canonical_json(b):
                    result.extend((a, b))
        else:
            result.extend(old[i:j])
            result.extend(new[k:l])
    return result


def _section_events(identifier: str, kind: str, section: str, old: Any, new: Any, created_deleted: bool) -> set[str]:
    if kind == "event" and section in {"effects", "frontmatter.time"}:
        return {identifier}
    if section in {"transitions", "lifecycle.transitions"}:
        members = _changed_members(old or [], new or [])
        # A mixed caused/uncaused array remains literally represented without
        # attaching its uncaused edit to an event. Semantic subjects are grouped
        # separately by their changed explicit contributions.
        if any(not value.get("causing_event") for value in members):
            return set()
        owners = {value["causing_event"] for value in members}
        return owners if len(owners) == 1 else set()
    if section == "related_story_points" and kind == "event":
        return {identifier}
    if section == "outcome_events":
        owners = set(old or []) ^ set(new or [])
        return owners if len(owners) == 1 else set()
    return set()


def _section_citations(projection: Projection, identifier: str, section: str, value: Any) -> list[dict]:
    if section in {"effects", "transitions", "lifecycle.transitions", "causes", "related_story_points", "outcome_events"} and isinstance(value, list):
        evidence = []
        for index, member in enumerate(value):
            original = projection._ordinals.get((identifier, section), list(range(len(value))))[index]
            raw = ((projection.world.records[identifier].frontmatter.get("lifecycle") or {}).get("transitions", [])
                   if section == "lifecycle.transitions" else projection.world.records[identifier].frontmatter.get(section, []))
            point = (StoryTime.from_value(raw[index]["time"], projection.world.default_timeline)
                     if isinstance(member, dict) and "time" in member else
                     StoryTime.from_value(projection.world.records[identifier].frontmatter["time"], projection.world.default_timeline)
                     if section in {"effects", "causes"} else None)
            evidence.append(projection.citation(identifier, section, original, member.get("id") if isinstance(member, dict) else None, point))
        # Empty targeted arrays still identify the exact source section.
        return evidence or [projection.citation(identifier, section)]
    return [projection.citation(identifier, section)]


def _contributions(projection: Projection, subject: dict) -> list[tuple[Any, set[str]]]:
    kind, identifier = subject["kind"], subject["recordId"]
    if kind == "state":
        return [({"event": event.id, "time": event.frontmatter["time"], "effect": effect}, {event.id}) for event in canonical_events(projection.world, projection.at)
                for effect in event.frontmatter["effects"] if effect["target"] == identifier and effect["key"] == subject["key"]]
    if kind in {"knowledge", "relationship", "story-point"}:
        record = projection.world.records.get(identifier)
        if record is None:
            return []
        members = (record.frontmatter.get("lifecycle") or {}).get("transitions", []) if kind == "story-point" else record.frontmatter.get("transitions", [])
        return [(member, {member["causing_event"]} if member.get("causing_event") else set()) for member in members]
    return []


def _change_events(before: Projection, after: Projection, subject: dict) -> set[str]:
    if subject["kind"] == "outcome":
        return {subject["recordId"]}
    old, new = _contributions(before, subject), _contributions(after, subject)
    changed = {canonical_json(value) for value in _changed_members([value for value, _ in old], [value for value, _ in new])}
    return set().union(*(events for value, events in old + new if canonical_json(value) in changed))


def _observed(value: Any, present: bool) -> dict:
    return {"presence": "present", "value": deepcopy(value)} if present else {"presence": "absent"}


def semantic_delta(base: World, candidate: World, scope: AuthorScope, at: StoryTime, identity: CandidateIdentity, *,
                   operation_targets: Mapping[str, Sequence[int]], focus_events: Sequence[str] = (),
                   limit: int = MAX_ITEMS, base_valid: bool = True, candidate_valid: bool = True) -> dict:
    """Complete closed DeltaOk or failure, with no partial successful output.

    operation_targets supplies exact resolved target IDs and original operation
    indexes, including no-ops and all reciprocal-link targets. focus_events adds
    explicit check/outcome event IDs. Neither parameter widens AuthorScope.
    Validation flags describe the original authorized complete Worlds, never a
    scope/horizon-sliced artificial World. Invalid candidates are not folded.
    """
    try:
        if not isinstance(base_valid, bool) or not isinstance(candidate_valid, bool):
            raise ProjectionFailure("invalid")
        if not base_valid:
            raise ProjectionFailure("invalid", source=True)
        if not candidate_valid:
            raise ProjectionFailure("unavailable")
        if not isinstance(identity, CandidateIdentity) or base.revision != identity.base_revision or candidate.revision != base.revision:
            raise ProjectionFailure("invalid")
        if not isinstance(operation_targets, Mapping) or not isinstance(focus_events, (list, tuple)) or any(not isinstance(value, str) for value in focus_events):
            raise ProjectionFailure("invalid")
        targets = {}
        for identifier, indexes in operation_targets.items():
            if not isinstance(identifier, str) or not isinstance(indexes, (list, tuple)) or any(isinstance(index, bool) or not isinstance(index, int) or index < 0 for index in indexes):
                raise ProjectionFailure("invalid")
            targets[identifier] = sorted(set(indexes))
        before = Projection(base, scope, at, limit=limit)
        after = Projection(candidate, scope, at, limit=limit, candidate=identity, base=base)
        changes = semantic_changes(before, after)
        old_view, new_view = _record_view(before), _record_view(after)
        admitted = set(old_view.world.records) | set(new_view.world.records)
        focus = set(focus_events)
        records = []
        section_owners: dict[tuple[str, str], set[str]] = {}
        for identifier in sorted(set(targets) & admitted):
            old, new = _sections(old_view, identifier), _sections(new_view, identifier)
            old_record, new_record = old_view.world.records.get(identifier), new_view.world.records.get(identifier)
            # A source record outside H is excluded, not created/deleted at H.
            if ((old_record is None and identifier in base.records)
                    or (new_record is None and identifier in candidate.records)):
                continue
            kind = (new_record or old_record).kind
            status = "created" if identifier not in base.records else "deleted" if identifier not in candidate.records else "updated" if canonical_json(old) != canonical_json(new) else "noop"
            sections, evidence = [], []
            for section in sorted(set(old) | set(new), key=lambda value: (value == "bodyMarkdown", value)):
                if section in old and section in new and canonical_json(old[section]) == canonical_json(new[section]):
                    continue
                sections.append({"section": section, "before": _observed(old.get(section), section in old), "after": _observed(new.get(section), section in new)})
                owners = _section_events(identifier, kind, section, old.get(section), new.get(section), status in {"created", "deleted"})
                section_owners[identifier, section] = owners
                focus.update(owners)
                # Focus also includes causes in mixed arrays kept unattributed.
                if section in {"transitions", "lifecycle.transitions"}:
                    focus.update(member["causing_event"] for member in _changed_members(old.get(section, []), new.get(section, [])) if member.get("causing_event"))
                for projection, values in ((old_view, old), (new_view, new)):
                    if section in values:
                        evidence.extend(_section_citations(projection, identifier, section, values[section]))
            if kind == "event" and status != "noop":
                focus.add(identifier)
            if not evidence:
                for projection, record in ((old_view, old_record), (new_view, new_record)):
                    if record is not None:
                        evidence.append(projection.citation(identifier, "frontmatter.title"))
            records.append({"recordId": identifier, "kind": kind, "change": status,
                            "before": old_view.reference(identifier) if old_record else None,
                            "after": new_view.reference(identifier) if new_record else None,
                            "sections": sections, "operationIndexes": targets[identifier], "citations": citations(evidence)})
            if len(records) > limit:
                raise ProjectionFailure("limit")
        owners_by_change = []
        for change in changes:
            owners = _change_events(before, after, change["subject"])
            owners_by_change.append(owners)
            focus.update(owners)
        eligible = {}
        for identifier in focus & admitted:
            projection = new_view if identifier in new_view.world.records else old_view
            record = projection.world.records[identifier]
            if record.kind == "event":
                point = StoryTime.from_value(record.frontmatter["time"], projection.world.default_timeline)
                eligible[identifier] = (point, projection.reference(identifier))
        ordered = sorted(eligible, key=lambda identifier: (eligible[identifier][0].tick, eligible[identifier][0].order, identifier))
        if len(ordered) > 100:
            raise ProjectionFailure("limit")
        groups = {identifier: {"event": eligible[identifier][1], "eventTime": time_value(eligible[identifier][0]), "changes": [], "recordChanges": []} for identifier in ordered}
        unattributed = []
        for change, owners in zip(changes, owners_by_change):
            linked = owners & groups.keys()
            if not linked:
                unattributed.append(change)
            for identifier in linked:
                groups[identifier]["changes"].append(change)
        unattributed_records = []
        for record in sorted(records, key=lambda value: (value["kind"], value["recordId"])):
            partitions: dict[str | None, list[dict]] = {}
            for section in record["sections"]:
                linked = section_owners[record["recordId"], section["section"]] & groups.keys()
                for owner in linked or {None}:
                    partitions.setdefault(owner, []).append(section)
            if not partitions:
                partitions[record["recordId"] if record["kind"] == "event" and record["recordId"] in groups else None] = []
            for owner, sections in partitions.items():
                value = {**record, "sections": sections}
                # Each partition cites only its represented sections; a caused
                # transition must not drag unrelated body/title evidence along.
                if sections:
                    names = {section["section"] for section in sections}
                    value["citations"] = [citation for citation in record["citations"] if citation["section"] in names]
                (unattributed_records if owner is None else groups[owner]["recordChanges"]).append(value)
        count = len(ordered) + len(changes) + len(records) + sum(len(record["sections"]) for record in records)
        report = {"protocol": PROTOCOL, "outcome": "ok", "baseRevision": base.revision,
                  "candidate": {"baseRevision": identity.base_revision, "requestHash": identity.request_hash},
                  "at": time_value(at), "timeScope": {"mode": "author-as-of", "at": time_value(at)},
                  "focusEvents": [eligible[identifier][1] for identifier in ordered],
                  "eventGroups": [groups[identifier] for identifier in ordered],
                  "unattributedChanges": unattributed, "unattributedRecordChanges": unattributed_records,
                  "expectations": [], "applyAllowed": True}
        return bounded(report, count, limit)
    except ProjectionFailure as failure:
        return {"protocol": PROTOCOL, "outcome": failure.outcome, "code": failure.code, "message": failure.message}
