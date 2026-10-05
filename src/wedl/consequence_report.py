"""Bounded event reports from immutable, validated, internally authorized Worlds."""
from __future__ import annotations

from typing import Any

from .event_consequences import (AuthorScope, CandidateIdentity, MAX_ITEMS, Projection,
                                 ProjectionFailure, bounded, citations, subject_order, time_value)
from .model import StoryTime, World
from .util import canonical_json

PROTOCOL = "wedl-event-consequences/v1"


def _affected(effects: list[dict], caused: list[dict], outcomes: list[dict]) -> list[dict]:
    values = [{"kind": "state", "recordId": value["effect"]["target"], "key": value["effect"]["key"]} for value in effects]
    values.extend({"kind": value["kind"], "recordId": value["record"]["id"]} for value in caused)
    values.extend({"kind": "outcome", "recordId": value["event"]["id"],
                   "targetId": value["target"]["id"], "targetKind": value["target"]["kind"]} for value in outcomes)
    return sorted({canonical_json(value): value for value in values}.values(), key=subject_order)


def _outcome_sections(projection: Projection, event: str, target: dict) -> None:
    if not projection.scope.permits(target["id"], "outcome_events") or (target["kind"] == "story-point"
            and not projection.scope.permits(event, "related_story_points")):
        raise ProjectionFailure("unavailable")


def _current(projection: Projection, focus: str, subject: dict) -> dict:
    snapshot = projection.snapshot(subject)
    status, superseding = "unknown", []
    kind = subject["kind"]
    if kind == "state":
        # A clear is an absent value with a real latest writer, not unknown.
        writers = [value for value in snapshot["citations"] if value["section"] == "effects"]
        if writers:
            latest = writers[-1]
            if latest["recordId"] == focus:
                status = "unchanged"
            else:
                focus_point = StoryTime.from_value(projection.world.records[focus].frontmatter["time"], projection.world.default_timeline)
                point = latest["time"]
                if point is not None and (int(point["tick"]), int(point["order"])) > (focus_point.tick, focus_point.order):
                    status, superseding = "superseded", writers
    elif kind in {"knowledge", "relationship", "story-point"}:
        record = projection.world.records.get(subject["recordId"])
        section = "lifecycle.transitions" if kind == "story-point" else "transitions"
        latest = projection._latest(record, section) if record is not None else None
        focus_members = [(point, ordinal) for source, member_section, ordinal, member, point in projection._caused.get(focus, [])
                         if source.id == subject["recordId"] and member_section == section]
        # Canonical unlearned genealogy is absent, despite an authored rejected
        # occurrence. There is no applicable learned contribution to compare.
        if snapshot["presence"] == "present" and latest is not None and focus_members:
            ordinal, member = latest
            point = StoryTime.from_value(member["time"], projection.world.default_timeline)
            focus_point, focus_ordinal = max(focus_members, key=lambda value: (value[0].tick, value[0].order, value[1]))
            if member.get("causing_event") == focus:
                status = "unchanged"
            elif (point.tick, point.order, ordinal) > (focus_point.tick, focus_point.order, focus_ordinal):
                status = "superseded"
                superseding = [projection.citation(record.id, section, ordinal, member.get("id"), point)]
    elif kind == "outcome" and snapshot["presence"] == "present" and snapshot["citations"]:
        # These static explicit links have no invented lifecycle/activation time.
        status = "unchanged"
    return {"subject": subject, "at": time_value(projection.at), "snapshot": snapshot,
            "supersession": status, "supersedingCitations": citations(superseding)}


def event_report(world: World, scope: AuthorScope, event: str, at: StoryTime, *, limit: int = MAX_ITEMS,
                 candidate: CandidateIdentity | None = None, base: World | None = None,
                 source_valid: bool = True) -> dict[str, Any]:
    """Return complete EventOk or a redacted failure, without repository I/O.

    The outer adapter authenticates the author and validates the complete source
    before calling. Request-local expectations are added by the shared evaluator
    at its later integration boundary; an empty collection asserts no checks.
    """
    try:
        if not isinstance(source_valid, bool) or not isinstance(event, str) or not event.strip():
            raise ProjectionFailure("invalid")
        if not source_valid:
            raise ProjectionFailure("invalid", source=True)
        projection = Projection(world, scope, at, limit=limit, candidate=candidate, base=base)
        explicit = projection.world.records.get(event)
        if explicit is not None and explicit.kind != "event":
            raise ProjectionFailure("invalid")
        reference = projection.reference(event, "event")
        identifier = reference["id"]
        event_time = StoryTime.from_value(projection.world.records[identifier].frontmatter["time"], projection.world.default_timeline)
        effects = projection.effects(identifier)
        caused = projection.caused_transitions(identifier)
        successors = projection.successors(identifier)
        outcomes = projection.outcomes(identifier)
        for value in outcomes:
            _outcome_sections(projection, identifier, value["target"])
        from .consequence_delta import event_local_changes
        changes = event_local_changes(projection, identifier)
        count = sum(len(value) for value in (effects, caused, successors, outcomes, changes))
        bounded({}, count, limit)
        current, advisories = [], []
        for subject in _affected(effects, caused, outcomes):
            current.append(_current(projection, identifier, subject))
            count += 1
            bounded(current, count, limit)
        for value in outcomes:
            if value["target"]["kind"] == "story-point" and not value["reciprocal"]:
                advisories.append({"code": "CONSEQUENCE-LINK-001", "event": reference,
                                   "target": value["target"], "citations": value["citations"]})
                count += 1
                bounded(advisories, count, limit)
        report = {"protocol": PROTOCOL, "outcome": "ok", "revision": world.revision,
                  "event": reference, "eventTime": time_value(event_time), "at": time_value(at),
                  "timeScope": {"mode": "author-as-of", "at": time_value(at)},
                  "effects": effects, "changes": changes, "causedTransitions": caused,
                  "outcomes": outcomes, "causalSuccessors": successors,
                  "currentAtHorizon": current, "advisories": advisories,
                  "expectations": [], "applyAllowed": True}
        return bounded(report, count, limit)
    except ProjectionFailure as failure:
        return {"protocol": PROTOCOL, "outcome": failure.outcome, "code": failure.code, "message": failure.message}
