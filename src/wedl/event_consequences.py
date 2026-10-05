"""Pure, scoped World projections shared by consequence reads and previews.

The caller establishes author authorization before creating AuthorScope. This
module accepts no request identity and never loads a repository or publishes a
cache. Report assembly and the shared report-wide budget belong to its callers.
"""
from __future__ import annotations

from copy import deepcopy
from dataclasses import dataclass
import re
from types import MappingProxyType
from typing import Any, Mapping

from . import COMPILED_SOURCE_SCHEMAS
from .model import Record, StoryTime, World
from .semantics import current_knowledge, current_relationship, evaluate_story_point, resolve_state
from .util import canonical_json, slugify
from .v07 import canonical_capabilities

MAX_ITEMS = 1000
MAX_BYTES = 262144
_SHA = re.compile(r"[0-9a-f]{40}\Z")
_HASH = re.compile(r"[0-9a-f]{64}\Z")
_KINDS = ("state", "knowledge", "relationship", "story-point", "outcome")


class ProjectionFailure(Exception):
    def __init__(self, outcome: str):
        self.outcome = outcome
        self.code = {"invalid": "CONSEQUENCE-REQUEST-001", "unavailable": "CONSEQUENCE-UNAVAILABLE-001",
                     "limit": "CONSEQUENCE-LIMIT-001"}[outcome]
        self.message = {"invalid": "Invalid consequence request.", "unavailable": "Consequence unavailable.",
                        "limit": "Consequence limit exceeded."}[outcome]
        super().__init__(self.message)


@dataclass(frozen=True)
class AuthorScope:
    """A trusted internal grant, not a JSON request or an audience selector.

    A missing section map grants that record's author sections. An explicit map
    is a whitelist of exact semantic sections and frontmatter.title.
    complete_families is an authorization-layer guarantee that all writers of
    state, or all knowledge evidence, are admitted. It must not be inferred from
    source record counts. Without that guarantee the corresponding fold is
    unavailable; this never turns withheld evidence into semantic absence.
    Future transport adapters must derive this grant from authenticated server
    context; request JSON cannot set record, section or completeness grants.
    """
    world_id: str
    record_ids: frozenset[str]
    sections: Mapping[str, frozenset[str]] | None = None
    complete_families: frozenset[str] = frozenset()

    def __post_init__(self):
        object.__setattr__(self, "record_ids", frozenset(self.record_ids))
        object.__setattr__(self, "complete_families", frozenset(self.complete_families))
        if not self.complete_families <= {"state", "knowledge"}:
            raise ProjectionFailure("invalid")
        if self.sections is not None:
            object.__setattr__(self, "sections", MappingProxyType({key: frozenset(value) for key, value in self.sections.items()}))

    def permits(self, identifier: str, section: str) -> bool:
        return identifier in self.record_ids and (self.sections is None or any(
            section == permitted or section.startswith(permitted + ".") for permitted in self.sections.get(identifier, ())))


@dataclass(frozen=True)
class CandidateIdentity:
    base_revision: str
    request_hash: str

    def __post_init__(self):
        if not _SHA.fullmatch(self.base_revision) or not _HASH.fullmatch(self.request_hash):
            raise ProjectionFailure("invalid")


def time_value(point: StoryTime) -> dict[str, str]:
    return {"timeline": point.timeline, "tick": str(point.tick), "order": str(point.order)}


def subject_order(subject: Mapping[str, Any]) -> tuple:
    return (_KINDS.index(subject["kind"]), subject["recordId"], subject.get("key", ""),
            subject.get("targetKind", ""), subject.get("targetId", ""))


def bounded(value: Any, count: int, limit: int = MAX_ITEMS) -> Any:
    """Check a complete primitive/report, never return a truncated assertion."""
    if isinstance(limit, bool) or not isinstance(limit, int) or not 1 <= limit <= MAX_ITEMS:
        raise ProjectionFailure("invalid")
    if count > limit:
        raise ProjectionFailure("limit")
    try:
        encoded = canonical_json(value).encode("utf-8")
    except (TypeError, ValueError):
        raise ProjectionFailure("invalid") from None
    if len(encoded) > MAX_BYTES:
        raise ProjectionFailure("limit")
    return value


def citations(values: list[dict[str, Any]]) -> list[dict[str, Any]]:
    unique = {canonical_json(value): value for value in values}
    def order(value):
        point = value["time"]
        return (value["recordId"], value["section"], -1 if value["sourceOrdinal"] is None else value["sourceOrdinal"],
                value["memberId"] or "", (0, "", 0, 0) if point is None else
                (1, point["timeline"], int(point["tick"]), int(point["order"])), value["provenance"]["kind"], canonical_json(value))
    return sorted(unique.values(), key=order)


class Projection:
    """A fresh authorized, horizon-sliced World with explicit reverse indexes.

    Inputs must be structurally validated Worlds. Filtering precedes names,
    copying member payloads, folds, indexes, counts and serialized output. Each
    projection owns its World/cache; neither source nor base caches are touched.
    """
    def __init__(self, world: World, scope: AuthorScope, at: StoryTime, *, limit: int = MAX_ITEMS,
                 candidate: CandidateIdentity | None = None, base: World | None = None,
                 exclude_event: str | None = None):
        if not isinstance(scope, AuthorScope):
            raise ProjectionFailure("unavailable")
        if not isinstance(at, StoryTime) or not _SHA.fullmatch(world.revision):
            raise ProjectionFailure("invalid")
        bounded({}, 0, limit)
        config_record = world.records.get(scope.world_id) if scope.world_id in scope.record_ids else None
        if config_record is None or config_record.kind != "world":
            raise ProjectionFailure("unavailable")
        config = config_record.frontmatter
        if config.get("schema") not in COMPILED_SOURCE_SCHEMAS or (config.get("schema") == "wedl/v0.7"
                and canonical_capabilities(config.get("capabilities")) is None):
            raise ProjectionFailure("unavailable")
        timelines = {item["id"] for item in config.get("timelines") or [] if isinstance(item, dict) and "id" in item}
        if at.timeline not in timelines:
            raise ProjectionFailure("invalid")
        if candidate is not None and (candidate.base_revision != world.revision or (base is not None and base.revision != world.revision)):
            raise ProjectionFailure("invalid")
        self.source, self.scope, self.at, self.limit = world, scope, at, limit
        self.candidate, self.base = candidate, base
        self._incomplete_plots: set[str] = set()
        # Completeness is trusted scope metadata, never a comparison with the
        # denied source universe. Unrelated denied records cannot affect folds.
        self._knowledge_complete = "knowledge" in scope.complete_families
        self._state_complete = "state" in scope.complete_families
        self._known_absent: set[tuple[str, str]] = set()
        self._ordinals: dict[tuple[str, str], list[int]] = {}
        default = str(config.get("default_timeline", "main"))
        # The focus is resolved only after scope/horizon selection. Exclusion
        # retains its record so event-existence triggers keep canonical meaning.
        focus = world.records.get(exclude_event) if exclude_event is not None and scope.permits(exclude_event, "frontmatter.time") else None
        focus_point = StoryTime.from_value(focus.frontmatter.get("time"), default) if focus and focus.kind == "event" and focus.status == "canonical" else None
        records: dict[str, Record] = {}
        for identifier in scope.record_ids:
            record = world.records.get(identifier)
            if record is None or (identifier != scope.world_id and not scope.permits(identifier, "frontmatter.title")):
                continue
            fm = record.frontmatter
            if record.kind == "event":
                if not scope.permits(identifier, "frontmatter.time"):
                    self._state_complete = False
                    continue
                if record.status != "canonical" or not StoryTime.from_value(fm.get("time"), default).not_after(at):
                    continue
                if not scope.permits(identifier, "effects"):
                    self._state_complete = False
            elif record.kind in {"scene", "environment", "conversation"}:
                if not scope.permits(identifier, "frontmatter.time"):
                    continue
                value = fm.get("time") or {}
                start = value.get("start") if isinstance(value, dict) and "start" in value else value
                if not StoryTime.from_value(start, default).not_after(at):
                    continue
            projected = dict(fm)
            if not scope.permits(identifier, "frontmatter.aliases"):
                projected["aliases"] = []
            if record.kind == "knowledge":
                for field in ("knower", "claim"):
                    if not scope.permits(identifier, "frontmatter." + field):
                        projected.pop(field, None)
            if record.kind in {"knowledge", "relationship", "story-point"}:
                section = "lifecycle.transitions" if record.kind == "story-point" else "transitions"
                owner = fm.get("lifecycle") or {} if record.kind == "story-point" else fm
                members, ordinals = [], []
                if scope.permits(identifier, section):
                    for ordinal, member in enumerate(owner.get("transitions") or []):
                        point = StoryTime.from_value(member.get("time"), default)
                        if not point.not_after(at) or (exclude_event is not None and member.get("causing_event") == exclude_event and point == focus_point):
                            continue
                        members.append(deepcopy(member))
                        ordinals.append(ordinal)
                self._ordinals[identifier, section] = ordinals
                if record.kind == "story-point":
                    if not all(scope.permits(identifier, required) for required in (
                            "lifecycle.initial_state", "lifecycle.transitions", "trigger", "dependencies")):
                        self._incomplete_plots.add(identifier)
                    projected["lifecycle"] = {key: deepcopy(value) for key, value in owner.items()
                                              if key != "transitions" and scope.permits(identifier, "lifecycle." + key)}
                    projected["lifecycle"]["transitions"] = members
                    if not scope.permits(identifier, "lifecycle.initial_state"):
                        projected["lifecycle"].pop("initial_state", None)
                else:
                    required = (("frontmatter.knower", "frontmatter.claim", "transitions")
                                if record.kind == "knowledge" else ("transitions",))
                    admitted = all(scope.permits(identifier, section) for section in required)
                    if record.kind == "knowledge" and not admitted:
                        self._knowledge_complete = False
                    if not members:
                        if admitted:
                            self._known_absent.add((record.kind, identifier))
                        continue
                    projected["transitions"] = members
            if record.kind == "event":
                projected["effects"] = []
                ordinals = []
                if scope.permits(identifier, "effects") and identifier != exclude_event:
                    for ordinal, member in enumerate(fm.get("effects") or []):
                        if member.get("target") in scope.record_ids:
                            projected["effects"].append(deepcopy(member))
                            ordinals.append(ordinal)
                self._ordinals[identifier, "effects"] = ordinals
            for section in ("initial_state", "trigger", "dependencies", "related_story_points", "outcome_events", "causes"):
                if section in projected and not scope.permits(identifier, section):
                    if record.kind == "story-point" and section in {"trigger", "dependencies"} and projected[section]:
                        self._incomplete_plots.add(identifier)
                    projected.pop(section)
            # Copy only after temporal members were sliced; future opaque
            # payloads cannot poison an otherwise admitted source projection.
            records[identifier] = Record(deepcopy(projected), "", record.source_path, record.raw_bytes,
                                         record.blob_oid, record.revision)
        if scope.world_id not in records:
            raise ProjectionFailure("unavailable")
        self.world = World(world.revision, world.tree_oid, records, world.root, world.source_root)
        self._filter_links()
        self._caused: dict[str, list[tuple[Record, str, int, dict, StoryTime]]] = {}
        self._successors: dict[str, list[tuple[Record, int, StoryTime]]] = {}
        self._outcomes: dict[str, set[str]] = {}
        for record in self.world:
            section = "lifecycle.transitions" if record.kind == "story-point" else "transitions"
            members = (record.frontmatter.get("lifecycle") or {}).get("transitions", []) if record.kind == "story-point" else record.frontmatter.get("transitions", [])
            if record.kind in {"knowledge", "relationship", "story-point"}:
                for index, member in enumerate(members):
                    cause = member.get("causing_event")
                    if cause in records and records[cause].kind == "event":
                        self._caused.setdefault(cause, []).append((record, section, self._ordinals[record.id, section][index], member, StoryTime.from_value(member["time"], default)))
            if record.kind == "event":
                point = StoryTime.from_value(record.frontmatter["time"], default)
                for ordinal, cause in enumerate(record.frontmatter.get("causes") or []):
                    self._successors.setdefault(cause, []).append((record, self._ordinals[record.id, "causes"][ordinal], point))
                for target in record.frontmatter.get("related_story_points") or []:
                    self._outcomes.setdefault(record.id, set()).add(target)
            if record.kind in {"story-point", "scene"}:
                for cause in record.frontmatter.get("outcome_events") or []:
                    self._outcomes.setdefault(cause, set()).add(record.id)

    def _filter_links(self) -> None:
        records = self.world.records
        for record in self.world:
            fm = record.frontmatter
            for field in ("causes", "related_story_points", "outcome_events"):
                if field in fm:
                    self._ordinals[record.id, field] = [index for index, identifier in enumerate(fm[field]) if identifier in records]
                    fm[field] = [identifier for identifier in fm[field] if identifier in records]
            if record.kind == "event":
                # Only entity-typed state values are reference leaves.
                kept, ordinals = [], []
                for index, effect in enumerate(fm["effects"]):
                    target = records.get(effect.get("target"))
                    if target is None:
                        continue
                    rule = ((self.world.config.get("state_keys") or {}).get(target.kind) or {}).get(effect.get("key")) or {}
                    value = effect.get("value")
                    if rule.get("type") == "entity" and isinstance(value, dict) and value.get("entity") not in records:
                        continue
                    kept.append(effect)
                    ordinals.append(self._ordinals[record.id, "effects"][index])
                fm["effects"] = kept
                self._ordinals[record.id, "effects"] = ordinals
            section = "lifecycle.transitions" if record.kind == "story-point" else "transitions"
            members = (fm.get("lifecycle") or {}).get("transitions", []) if record.kind == "story-point" else fm.get("transitions", [])
            for member in members:
                for field in ("causing_event", "source_entity"):
                    if member.get(field) is not None and member[field] not in records:
                        member.pop(field)

    def _plot_references(self, record: Record) -> list[dict[str, Any]] | None:
        """Return only schema-declared fold evidence, closing incomplete inputs."""
        if record.id in self._incomplete_plots:
            return None
        subjects = []
        missing = False
        def visit(value):
            nonlocal missing
            if isinstance(value, list):
                for item in value:
                    visit(item)
            elif isinstance(value, dict):
                for field in ("all", "any", "not"):
                    if field in value:
                        visit(value[field])
                for field, target_field, kind in (("entity_state", "target", "state"), ("relationship", "relationship", "relationship")):
                    if field in value:
                        predicate = value[field]
                        target = self.world.records.get(predicate.get(target_field))
                        if target is None or (kind == "state" and (not self._state_complete or not self.scope.permits(target.id, "initial_state." + str(predicate.get("key"))))):
                            missing = True
                        else:
                            subject = {"kind": kind, "recordId": target.id}
                            if kind == "state":
                                subject["key"] = predicate.get("key")
                            subjects.append(subject)
                if "knowledge" in value:
                    predicate = value["knowledge"]
                    if not self._knowledge_complete or predicate.get("knower") not in self.world.records:
                        missing = True
                    else:
                        subjects.extend({"kind": "knowledge", "recordId": item["knowledgeId"]}
                                        for item in current_knowledge(self.world, predicate["knower"], self.at, include_forgotten=True)
                                        if item["claimKey"] == predicate.get("claim_key"))
                if "event" in value:
                    event = value["event"].get("event") if isinstance(value["event"], dict) else value["event"]
                    if event not in self.world.records:
                        missing = True
                    elif self.scope.permits(event, "frontmatter.time"):
                        point = StoryTime.from_value(self.world.records[event].frontmatter["time"], self.world.default_timeline)
                        subjects.append({"citation": self.citation(event, "frontmatter.time", None, None, point)})
                    else:
                        missing = True
        visit(record.frontmatter.get("trigger") or {})
        for dependency in (record.frontmatter.get("dependencies") or {}).get("all") or []:
            identifier = dependency.get("story_point")
            if identifier not in self.world.records or identifier in self._incomplete_plots:
                missing = True
            else:
                target = self.world.records[identifier]
                section = "lifecycle.transitions"
                latest = self._latest(target, section)
                if latest:
                    ordinal, member = latest
                    subjects.append({"citation": self.citation(target.id, section, ordinal, member.get("id"), StoryTime.from_value(member["time"], self.world.default_timeline))})
                elif self.scope.permits(target.id, "lifecycle.initial_state"):
                    subjects.append({"citation": self.citation(target.id, "lifecycle.initial_state")})
        return None if missing else subjects

    def reference(self, value: str, kind: str | None = None) -> dict[str, str]:
        # This World contains only eligible records; no raw World.find or
        # permissive audience helper can widen the trusted grant.
        records = [record for record in self.world if self.scope.permits(record.id, "frontmatter.title") and (kind is None or record.kind == kind)]
        exact = [record for record in records if record.id == value]
        matches = exact or [record for record in records if value == record.title or value in (record.frontmatter.get("aliases") or [])
                           or value == slugify(record.title) or any(value == slugify(alias) for alias in record.frontmatter.get("aliases") or [])]
        if not matches:
            raise ProjectionFailure("unavailable")
        if len(matches) != 1:
            raise ProjectionFailure("invalid")
        record = matches[0]
        return {"id": record.id, "kind": record.kind, "title": record.title}

    def citation(self, identifier: str, section: str, ordinal: int | None = None,
                 member_id: str | None = None, point: StoryTime | None = None) -> dict[str, Any]:
        if identifier not in self.world.records or not self.scope.permits(identifier, section):
            raise ProjectionFailure("unavailable")
        record = self.source.records[identifier]
        original = self.base.records.get(identifier) if self.base is not None else None
        unchanged = original is not None and (record.frontmatter, record.body, record.source_path, record.raw_bytes) == (
            original.frontmatter, original.body, original.source_path, original.raw_bytes)
        if self.candidate is not None and not unchanged:
            provenance = {"kind": "candidate", "baseRevision": self.candidate.base_revision, "requestHash": self.candidate.request_hash}
        else:
            source = original if unchanged else record
            if not isinstance(source.blob_oid, str) or not _SHA.fullmatch(source.blob_oid):
                raise ProjectionFailure("unavailable")
            provenance = {"kind": "source", "revision": self.source.revision, "blobOid": source.blob_oid}
        path = record.source_path.replace("\\", "/")
        if path.startswith("/") or ":" in path or ".." in path.split("/"):
            raise ProjectionFailure("unavailable")
        return {"recordId": identifier, "sourcePath": path, "section": section, "sourceOrdinal": ordinal,
                "memberId": member_id, "time": time_value(point) if point is not None else None, "provenance": provenance}

    def _latest(self, record: Record, section: str) -> tuple[int, dict] | None:
        members = (record.frontmatter.get("lifecycle") or {}).get("transitions", []) if section == "lifecycle.transitions" else record.frontmatter.get(section, [])
        if not members:
            return None
        index, member = max(enumerate(members), key=lambda item: (StoryTime.from_value(item[1]["time"], self.world.default_timeline).tick,
                                                                 StoryTime.from_value(item[1]["time"], self.world.default_timeline).order, item[0]))
        return self._ordinals[record.id, section][index], member

    def snapshot(self, subject: Mapping[str, Any]) -> dict[str, Any]:
        """Return canonical presence only when available; withheld inputs raise.

        Callers map ProjectionFailure('unavailable') to their redacted outcome,
        never to an absent snapshot or a successful absence expectation.
        """
        record = self.world.records.get(subject["recordId"])
        if record is None:
            if (subject["kind"], subject["recordId"]) in self._known_absent:
                return {"presence": "absent", "payload": None, "citations": []}
            raise ProjectionFailure("unavailable")
        kind = subject["kind"]
        payload, evidence = None, []
        if kind == "state":
            if not self._state_complete or not self.scope.permits(record.id, "initial_state." + str(subject["key"])):
                raise ProjectionFailure("unavailable")
            key = subject["key"]
            state, sources = resolve_state(self.world, record.id, self.at)
            if key in state:
                payload = {"value": state[key]}
            source = sources.get(key)
            if source and source.get("source") == "initial-state":
                evidence.append(self.citation(record.id, "initial_state." + key))
            elif source:
                event = self.world.records[source["entityId"]]
                index = next(index for index, effect in reversed(list(enumerate(event.frontmatter["effects"]))) if effect["key"] == key and effect["target"] == record.id)
                effect = event.frontmatter["effects"][index]
                evidence.append(self.citation(event.id, "effects", self._ordinals[event.id, "effects"][index], effect.get("id"), StoryTime.from_value(event.frontmatter["time"], self.world.default_timeline)))
        elif kind in {"knowledge", "relationship"}:
            if record.kind != kind:
                raise ProjectionFailure("invalid")
            required = (("frontmatter.knower", "frontmatter.claim", "transitions")
                        if kind == "knowledge" else ("transitions",))
            if not all(self.scope.permits(record.id, section) for section in required):
                raise ProjectionFailure("unavailable")
            value = (next((value for value in current_knowledge(self.world, str(record.frontmatter.get("knower")), self.at, include_forgotten=True)
                           if value["knowledgeId"] == record.id), None) if kind == "knowledge" else current_relationship(self.world, record.id, self.at))
            latest = self._latest(record, "transitions")
            if value is not None and latest is not None:
                ordinal, member = latest
                payload = ({"state": value["state"], "confidence": value["confidence"]} if kind == "knowledge" else
                           {"status": value["status"], "metrics": value["metrics"], "facets": value["facets"]})
                point = StoryTime.from_value(member["time"], self.world.default_timeline)
                payload.update(transitionId=member.get("id"), time=time_value(point))
                evidence.append(self.citation(record.id, "transitions", ordinal, member.get("id"), point))
        elif kind == "story-point":
            if record.kind != kind:
                raise ProjectionFailure("invalid")
            support = self._plot_references(record)
            if support is None:
                raise ProjectionFailure("unavailable")
            value = evaluate_story_point(self.world, record, self.at)
            payload = {key: value[key] for key in ("storedState", "derivedState", "eligible", "dependenciesSatisfied", "triggerSatisfied")}
            latest = self._latest(record, "lifecycle.transitions")
            if latest:
                ordinal, member = latest
                evidence.append(self.citation(record.id, "lifecycle.transitions", ordinal, member.get("id"), StoryTime.from_value(member["time"], self.world.default_timeline)))
            elif self.scope.permits(record.id, "lifecycle.initial_state"):
                evidence.append(self.citation(record.id, "lifecycle.initial_state"))
            for section in ("trigger", "dependencies"):
                if record.frontmatter.get(section) and self.scope.permits(record.id, section):
                    evidence.append(self.citation(record.id, section))
            for subject in support:
                if "citation" in subject:
                    evidence.append(subject["citation"])
                else:
                    evidence.extend(self.snapshot(subject)["citations"])
        elif kind == "outcome":
            target = self.world.records.get(subject["targetId"])
            if record.kind != "event" or target is None or target.kind != subject["targetKind"]:
                return {"presence": "absent", "payload": None, "citations": []}
            event_linked = target.kind == "story-point" and target.id in (record.frontmatter.get("related_story_points") or [])
            target_linked = record.id in (target.frontmatter.get("outcome_events") or [])
            payload = {"eventLinked": event_linked, "targetLinked": target_linked}
            if event_linked:
                evidence.append(self.citation(record.id, "related_story_points", self._ordinals[record.id, "related_story_points"][record.frontmatter["related_story_points"].index(target.id)]))
            if target_linked:
                evidence.append(self.citation(target.id, "outcome_events", self._ordinals[target.id, "outcome_events"][target.frontmatter["outcome_events"].index(record.id)]))
        else:
            raise ProjectionFailure("invalid")
        return {"presence": "present" if payload is not None else "absent", "payload": payload, "citations": citations(evidence)}

    def subjects(self) -> list[dict[str, Any]]:
        values = []
        for record in self.world:
            if record.kind in {"character", "object", "location", "environment"}:
                state, _ = resolve_state(self.world, record.id, self.at)
                keys = set(state)
                for event in self.world.by_kind("event"):
                    keys.update(effect["key"] for effect in event.frontmatter["effects"] if effect["target"] == record.id)
                values.extend({"kind": "state", "recordId": record.id, "key": key} for key in keys)
            elif record.kind in {"knowledge", "relationship", "story-point"}:
                values.append({"kind": record.kind, "recordId": record.id})
        for event, targets in self._outcomes.items():
            values.extend({"kind": "outcome", "recordId": event, "targetId": target, "targetKind": self.world.records[target].kind} for target in targets)
        return sorted(values, key=subject_order)

    def at_time(self, at: StoryTime, *, exclude_event: str | None = None) -> Projection:
        return Projection(self.source, self.scope, at, limit=self.limit, candidate=self.candidate, base=self.base, exclude_event=exclude_event)

    def event_views(self, event: str) -> tuple[Projection, Projection]:
        reference = self.reference(event, "event")
        record = self.world.records[reference["id"]]
        point = StoryTime.from_value(record.frontmatter["time"], self.world.default_timeline)
        return self.at_time(point, exclude_event=record.id), self.at_time(point)

    def effects(self, event: str) -> list[dict[str, Any]]:
        record = self.world.records[self.reference(event, "event")["id"]]
        point = StoryTime.from_value(record.frontmatter["time"], self.world.default_timeline)
        values = [{"effect": deepcopy(effect), "citation": self.citation(record.id, "effects", self._ordinals[record.id, "effects"][index], effect.get("id"), point)}
                  for index, effect in enumerate(record.frontmatter["effects"])]
        return bounded(values, len(values), self.limit)

    def caused_transitions(self, event: str) -> list[dict[str, Any]]:
        event_id = self.reference(event, "event")["id"]
        focus_point = StoryTime.from_value(self.world.records[event_id].frontmatter["time"], self.world.default_timeline)
        values = []
        for record, section, ordinal, member, point in self._caused.get(event_id, []):
            keys = {"id", "time", "causing_event", "note"} | ({"state", "confidence", "acquisition", "source_entity"} if record.kind == "knowledge" else
                    {"relationship_status", "metrics", "facets"} if record.kind == "relationship" else {"state"})
            transition = {key: deepcopy(value) for key, value in member.items() if key in keys}
            transition["time"] = time_value(point)
            values.append({"kind": record.kind, "record": self.reference(record.id), "transitionId": member.get("id"), "sourceOrdinal": ordinal,
                           "time": time_value(point), "atEventTime": point == focus_point, "transition": transition,
                           "citation": self.citation(record.id, section, ordinal, member.get("id"), point)})
        values.sort(key=lambda value: (int(value["time"]["tick"]), int(value["time"]["order"]), value["kind"], value["record"]["id"], value["sourceOrdinal"], value["transitionId"] or ""))
        return bounded(values, len(values), self.limit)

    def successors(self, event: str) -> list[dict[str, Any]]:
        event_id = self.reference(event, "event")["id"]
        first = StoryTime.from_value(self.world.records[event_id].frontmatter["time"], self.world.default_timeline)
        values = [{"event": self.reference(record.id), "time": time_value(point), "citation": self.citation(record.id, "causes", ordinal, None, point)}
                  for record, ordinal, point in self._successors.get(event_id, []) if first.not_after(point) and first != point]
        values.sort(key=lambda value: (int(value["time"]["tick"]), int(value["time"]["order"]), value["event"]["id"]))
        return bounded(values, len(values), self.limit)

    def outcomes(self, event: str) -> list[dict[str, Any]]:
        event_id = self.reference(event, "event")["id"]
        values = []
        for target_id in self._outcomes.get(event_id, set()):
            target = self.world.records[target_id]
            snapshot = self.snapshot({"kind": "outcome", "recordId": event_id, "targetId": target_id, "targetKind": target.kind})
            payload = snapshot["payload"]
            values.append({"event": self.reference(event_id), "target": self.reference(target_id),
                           "reciprocal": target.kind == "story-point" and payload["eventLinked"] and payload["targetLinked"], "citations": snapshot["citations"]})
        values.sort(key=lambda value: (value["target"]["kind"], value["target"]["id"]))
        return bounded(values, len(values), self.limit)
