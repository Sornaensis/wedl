from copy import deepcopy

import pytest

from wedl.consequence_delta import event_local_changes, semantic_changes, semantic_delta
from wedl.event_consequences import AuthorScope, CandidateIdentity, Projection, ProjectionFailure
from wedl.model import ORDER_MAX, ORDER_MIN, TICK_MAX, TICK_MIN, StoryTime
from wedl.util import canonical_json
from test_event_consequences import REVISION, _id, _record, _subject, _transition, _world


IDENTITY = CandidateIdentity(REVISION, "c" * 64)


def _delta(base, candidate, scope, targets, at=StoryTime("main", 40), **options):
    return semantic_delta(base, candidate, scope, at, IDENTITY, operation_targets=targets, **options)


def _groups(report):
    assert report["outcome"] == "ok", report
    return {value["event"]["id"]: value for value in report["eventGroups"]}


def test_composite_delta_separates_static_sections_and_literal_consequences():
    base, scope = _world()
    candidate = deepcopy(base)
    focus, belief, relationship, plot = (_id(kind, name) for kind, name in (
        ("event", "focus"), ("knowledge", "belief"), ("relationship", "relationship"), ("story-point", "plot")))
    candidate.get(focus).frontmatter["effects"][-1]["value"] = "freed"
    candidate.get(belief).frontmatter["transitions"].append(_transition("knowledge", "new-learning", 40, state="accepted", cause=focus))
    candidate.get(relationship).frontmatter["transitions"].append(_transition("relationship", "new-trust", 40, cause=focus, metrics={"trust": 0.95}))
    candidate.get(plot).frontmatter["lifecycle"]["transitions"].append(_transition("story-point", "new-resolution", 40, state="resolved", cause=focus))
    candidate.get(belief).body = "An unrelated prose correction.\n"
    candidate.get(_id("object", "object")).frontmatter["initial_state"]["null"] = "static"
    scene = _id("scene", "scene")
    candidate.get(scene).frontmatter["outcome_events"] = []
    targets = {focus: [0], belief: [1, 5], relationship: [2], plot: [3], _id("object", "object"): [6], scene: [4]}
    report = _delta(base, candidate, scope, targets)
    group = _groups(report)[focus]
    assert {value["subject"]["kind"] for value in group["changes"]} == {"state", "knowledge", "relationship", "story-point", "outcome"}
    assert any(value["subject"] == _subject("state", "object", key="null") for value in report["unattributedChanges"])
    assert any(value["subject"]["kind"] == "story-point" for value in group["changes"])
    static = {value["recordId"]: value for value in report["unattributedRecordChanges"]}
    assert static[belief]["sections"] == [{"section": "bodyMarkdown", "before": {"presence": "present", "value": base.get(belief).body}, "after": {"presence": "present", "value": "An unrelated prose correction.\n"}}]
    assert static[belief]["operationIndexes"] == [1, 5]
    assert all(citation["section"] != "bodyMarkdown" for value in group["recordChanges"] for citation in value["citations"])
    transitions = next(value for value in group["recordChanges"] if value["recordId"] == belief)
    assert transitions["sections"][0]["after"]["value"][-1]["time"] == {"timeline": "main", "tick": "40", "order": "0"}
    assert report["candidate"] == {"baseRevision": REVISION, "requestHash": "c" * 64}
    assert report["at"] == report["timeScope"]["at"] == {"timeline": "main", "tick": "40", "order": "0"}


def test_revision_horizon_delta_is_distinct_from_local_event_time_and_noop():
    base, scope = _world()
    focus, later = _id("event", "focus"), _id("event", "later")
    base.get(later).frontmatter["effects"] = [{"id": _id("effect", "close-later"), "target": _id("object", "object"), "key": "condition", "operation": "set", "value": "closed"}]
    candidate = deepcopy(base)
    candidate.get(focus).frontmatter["effects"][-1]["value"] = "temporary"
    at_event = _delta(base, candidate, scope, {focus: [0]}, StoryTime("main", 10))
    assert len(_groups(at_event)[focus]["changes"]) == 1
    at_horizon = _delta(base, candidate, scope, {focus: [0]}, StoryTime("main", 20))
    assert _groups(at_horizon)[focus]["changes"] == []
    assert _groups(at_horizon)[focus]["recordChanges"][0]["change"] == "updated"
    projected = Projection(candidate, scope, StoryTime("main", 20), candidate=IDENTITY, base=base)
    local = event_local_changes(projected, focus)
    changed = next(value for value in local if value["subject"] == _subject("state", "object", key="condition"))
    assert changed["before"]["payload"] == {"value": "closed"}
    assert changed["after"]["payload"] == {"value": "temporary"}
    # A source-authored no-op is not suppressed by a zero semantic difference.
    noop = _delta(base, base, scope, {focus: [2, 2, 1]}, focus_events=[focus])
    marker = _groups(noop)[focus]["recordChanges"][0]
    assert marker["change"] == "noop" and marker["sections"] == [] and marker["operationIndexes"] == [1, 2]
    assert marker["citations"] and _groups(noop)[focus]["changes"] == []
    # Reordering same-event writes preserves explicit ownership even when the
    # set of authored values is identical. Derived plot changes stay unlinked.
    base, scope = _world()
    reversed_writes = deepcopy(base)
    reversed_writes.get(focus).frontmatter["effects"].reverse()
    reordered = _delta(base, reversed_writes, scope, {focus: [0]})
    group = _groups(reordered)[focus]
    assert [value["subject"] for value in group["changes"]] == [_subject("state", "object", key="condition")]
    assert group["changes"][0]["before"]["payload"] == {"value": "open"}
    assert group["changes"][0]["after"]["payload"] == {"value": "closed"}
    assert any(value["subject"]["kind"] == "story-point" for value in reordered["unattributedChanges"])
    assert group["recordChanges"][0]["sections"][0]["after"]["value"] == reversed_writes.get(focus).frontmatter["effects"]
    # Independent-cell reorder has no net semantic change, while literal source
    # order remains reported. Record-map insertion order does not change folds.
    base.get(focus).frontmatter["effects"][0]["key"] = "null"
    independent = deepcopy(base)
    independent.get(focus).frontmatter["effects"].reverse()
    unchanged = _delta(base, independent, scope, {focus: [0]})
    assert _groups(unchanged)[focus]["changes"] == []
    assert _groups(unchanged)[focus]["recordChanges"][0]["change"] == "updated"
    reordered_records = deepcopy(independent)
    reordered_records.records = dict(reversed(list(reordered_records.records.items())))
    assert _delta(base, reordered_records, scope, {focus: [0]}) == unchanged


def test_created_deleted_records_and_indirect_plot_changes_are_honest():
    base, scope = _world()
    other = _id("object", "other")
    base.get(_id("event", "disjoint")).frontmatter["effects"] = []
    candidate = deepcopy(base)
    del candidate.records[other]
    created = _record("object", "new-object", initial_state={"condition": None})
    candidate.records[created.id] = created
    scope = AuthorScope(scope.world_id, scope.record_ids | {created.id}, complete_families=scope.complete_families)
    report = _delta(base, candidate, scope, {other: [0], created.id: [1]})
    assert report["focusEvents"] == []
    markers = {value["recordId"]: value for value in report["unattributedRecordChanges"]}
    assert markers[other]["change"] == "deleted" and markers[other]["after"] is None
    assert markers[created.id]["change"] == "created" and markers[created.id]["before"] is None
    semantic = {value["subject"]["recordId"]: value for value in report["unattributedChanges"]}
    assert semantic[created.id]["before"]["presence"] == "absent"
    assert semantic[created.id]["after"]["payload"] == {"value": None}
    assert semantic[other]["after"]["presence"] == "absent"
    assert {citation["provenance"]["kind"] for citation in markers[created.id]["citations"]} == {"candidate"}
    # An existing untimed empty knowledge history has a source record even
    # though its semantic snapshot is absent before its first learning.
    empty_base, empty_scope = _world()
    belief = _id("knowledge", "belief")
    empty_base.get(belief).frontmatter["transitions"] = []
    learned = deepcopy(empty_base)
    learned.get(belief).frontmatter["transitions"] = [_transition("knowledge", "first-learning", 10, state="accepted", cause=_id("event", "focus"))]
    empty_report = _delta(empty_base, learned, empty_scope, {belief: [0]})
    marker = _groups(empty_report)[_id("event", "focus")]["recordChanges"][0]
    assert marker["change"] == "updated" and marker["before"]["id"] == marker["after"]["id"] == belief
    static_new = _record("knowledge", "new-empty-belief", knower=_id("character", "one"), claim={"key": "empty", "statement": "No learning asserted."}, transitions=[])
    learned.records[static_new.id] = static_new
    empty_scope = AuthorScope(empty_scope.world_id, empty_scope.record_ids | {static_new.id}, complete_families=empty_scope.complete_families)
    empty_report = _delta(empty_base, learned, empty_scope, {static_new.id: [1]})
    assert empty_report["unattributedRecordChanges"][0]["change"] == "created"
    assert empty_report["unattributedRecordChanges"][0]["after"]["id"] == static_new.id
    # A trigger-derived eligibility change has no authored plot cause.
    candidate = deepcopy(base)
    candidate.get(_id("event", "focus")).frontmatter["effects"][-1]["value"] = "closed"
    report = _delta(base, candidate, scope, {_id("event", "focus"): [0]})
    assert any(value["subject"]["kind"] == "story-point" for value in report["unattributedChanges"])
    assert all(value["subject"]["kind"] != "story-point" for value in _groups(report)[_id("event", "focus")]["changes"])


@pytest.mark.parametrize("mode", ["single", "two-causes", "uncaused", "cause-replacement"])
def test_array_attribution_uses_only_changed_members_and_preserves_whole_sections(mode):
    base, scope = _world()
    candidate = deepcopy(base)
    belief, focus, later = _id("knowledge", "belief"), _id("event", "focus"), _id("event", "later")
    members = candidate.get(belief).frontmatter["transitions"]
    members[-1]["state"] = "accepted"
    if mode == "two-causes":
        members[2]["state"] = "suspected"
    elif mode == "uncaused":
        members[0]["state"] = "uncertain"
    elif mode == "cause-replacement":
        members[-1]["causing_event"] = later
    candidate.get(belief).body = "Separate body edit."
    report = _delta(base, candidate, scope, {belief: [0, 1]})
    groups = _groups(report)
    if mode == "single":
        assert list(groups) == [focus]  # Unchanged old cause on member 2 adds no owner.
        owned = next(value for value in groups[focus]["recordChanges"] if value["recordId"] == belief)
    else:
        owned = next(value for value in report["unattributedRecordChanges"] if value["recordId"] == belief)
        assert all(not value["recordChanges"] for value in groups.values())
    section = next(value for value in owned["sections"] if value["section"] == "transitions")
    assert len(section["before"]["value"]) == len(section["after"]["value"]) == 4
    assert section["after"]["value"][-1]["state"] == "accepted"
    assert any(value["section"] == "bodyMarkdown" for record in report["unattributedRecordChanges"] for value in record["sections"])
    if mode == "cause-replacement":
        assert list(groups) == [focus, later]
        assert all(any(value["subject"]["recordId"] == belief for value in group["changes"]) for group in groups.values())
    if mode == "single":
        # Inserting before differently caused historical members does not add
        # their owners. Match stable IDs; source ordinals remain original.
        inserted = deepcopy(base)
        inserted.get(belief).frontmatter["transitions"].insert(2, _transition("knowledge", "inserted-learning", 15, state="accepted", cause=focus))
        report = _delta(base, inserted, scope, {belief: [0]})
        groups = _groups(report)
        assert list(groups) == [focus]
        section = groups[focus]["recordChanges"][0]["sections"][0]
        assert len(section["before"]["value"]) == 4 and len(section["after"]["value"]) == 5
        assert section["after"]["value"][3]["causing_event"] == later
        assert not report["unattributedRecordChanges"]
        # Legacy ID-less plot histories align by unchanged literal value/order.
        plot = _id("story-point", "plot")
        base.get(plot).frontmatter["lifecycle"]["transitions"] = [
            {"time": StoryTime("main", 10).to_dict(), "state": "active", "causing_event": focus},
            {"time": StoryTime("main", 30).to_dict(), "state": "resolved", "causing_event": later}]
        inserted = deepcopy(base)
        inserted.get(plot).frontmatter["lifecycle"]["transitions"].insert(1, {"time": StoryTime("main", 15).to_dict(), "state": "active", "causing_event": focus})
        report = _delta(base, inserted, scope, {plot: [0]})
        groups = _groups(report)
        assert list(groups) == [focus] and groups[focus]["recordChanges"][0]["sections"][0]["section"] == "lifecycle.transitions"
        assert not report["unattributedRecordChanges"]


def test_future_and_denied_source_sections_do_not_change_delta_or_counts():
    class Poison:
        def __deepcopy__(self, memo):
            raise AssertionError("withheld source payload was copied")
    base, scope = _world()
    candidate = deepcopy(base)
    belief, focus = _id("knowledge", "belief"), _id("event", "focus")
    candidate.get(belief).frontmatter["transitions"][-1]["note"] = "Future changed note."
    report = _delta(base, candidate, scope, {belief: [0]}, StoryTime("main", 20))
    assert report["focusEvents"] == [] and report["unattributedChanges"] == []
    assert report["unattributedRecordChanges"][0]["change"] == "noop"
    denied = _record("object", "denied", initial_state={"condition": "private"})
    candidate.records[denied.id] = denied
    assert _delta(base, candidate, scope, {belief: [0], denied.id: [1]}, StoryTime("main", 20)) == report
    denied.frontmatter = {"kind": Poison(), "title": Poison()}
    denied.body = Poison()
    assert _delta(base, candidate, scope, {belief: [0], denied.id: [1]}, StoryTime("main", 20)) == report
    candidate.get(belief).frontmatter["transitions"][-1]["note"] = Poison()
    assert _delta(base, candidate, scope, {belief: [0]}, StoryTime("main", 20)) == report
    # Body permissions precede value access; a permitted semantic edit survives.
    base, scope = _world()
    candidate = deepcopy(base)
    candidate.get(focus).frontmatter["effects"][-1]["value"] = "different"
    base.get(belief).body = candidate.get(belief).body = Poison()
    sections = {identifier: frozenset({"frontmatter", "effects", "initial_state", "transitions", "lifecycle", "trigger", "dependencies", "causes", "related_story_points", "outcome_events"}) for identifier in scope.record_ids}
    restricted = AuthorScope(scope.world_id, scope.record_ids, sections, scope.complete_families)
    report = _delta(base, candidate, restricted, {focus: [0], belief: [1]}, StoryTime("main", 20))
    assert report["outcome"] == "ok" and "bodyMarkdown" not in canonical_json(report)
    incomplete = AuthorScope(scope.world_id, scope.record_ids)
    assert _delta(base, candidate, incomplete, {focus: [0]})["outcome"] == "unavailable"
    # A child grant admits exactly that source cell before any copying/folding.
    base, grant = _world()
    candidate = deepcopy(base)
    target = _id("object", "object")
    candidate.get(target).frontmatter["initial_state"]["condition"] = "different-initial"
    sections = {identifier: frozenset({"frontmatter", "effects", "initial_state", "transitions", "lifecycle", "trigger", "dependencies", "causes", "related_story_points", "outcome_events"}) for identifier in grant.record_ids}
    sections[target] = frozenset({"frontmatter.title", "initial_state.condition"})
    partial = AuthorScope(grant.world_id, grant.record_ids, sections, grant.complete_families)
    expected = _delta(base, candidate, partial, {target: [0]})
    assert expected["outcome"] == "ok"
    marker = expected["unattributedRecordChanges"][0]
    assert marker["change"] == "updated" and [value["section"] for value in marker["sections"]] == ["initial_state.condition"]
    assert marker["sections"][0]["after"] == {"presence": "present", "value": "different-initial"}
    for record in (base.get(target), candidate.get(target)):
        record.frontmatter["initial_state"]["null"] = Poison()
        record.frontmatter["initial_state"]["tokens"] = Poison()
        record.frontmatter["initial_state"]["hidden-name"] = Poison()
    assert _delta(base, candidate, partial, {target: [0]}) == expected
    assert "hidden-name" not in canonical_json(expected)


def test_delta_identity_purity_order_and_json_presence_comparison():
    base, scope = _world()
    candidate = deepcopy(base)
    target, focus = _id("object", "object"), _id("event", "focus")
    candidate.get(target).frontmatter["initial_state"]["null"] = False
    base.get(target).frontmatter["initial_state"]["null"] = 0
    base._cache["sentinel"] = object()
    candidate._cache["sentinel"] = object()
    caches = (dict(base._cache), dict(candidate._cache))
    before, after = deepcopy(base.records), deepcopy(candidate.records)
    report = _delta(base, candidate, scope, {target: [2, 0], focus: [1]}, focus_events=[focus])
    assert report["outcome"] == "ok"
    value = next(value for value in report["unattributedChanges"] if value["subject"]["key"] == "null")
    assert value["before"]["payload"] == {"value": 0} and value["after"]["payload"] == {"value": False}
    assert (base._cache, candidate._cache) == caches and base.records == before and candidate.records == after
    assert _delta(base, candidate, scope, {focus: [1], target: [0, 2]}, focus_events=[focus]) == report
    wrong = deepcopy(candidate)
    wrong.revision = "d" * 40
    assert _delta(base, wrong, scope, {})["code"] == "CONSEQUENCE-REQUEST-001"
    with pytest.raises(ProjectionFailure):
        semantic_changes(Projection(base, scope, StoryTime("main", 10)), Projection(candidate, scope, StoryTime("main", 20)))
    # Normalize installed time leaves only, including signed extrema, without
    # guessing time-shaped values in literal source extensions.
    base, scope = _world()
    scene, world = _id("scene", "scene"), scope.world_id
    minimum, maximum = StoryTime("main", TICK_MIN, ORDER_MIN), StoryTime("main", TICK_MAX, ORDER_MAX)
    base.get(world).frontmatter["current_time"] = minimum.to_dict()
    base.get(scene).frontmatter["time"] = {"start": minimum.to_dict(), "current": minimum.to_dict(), "end": maximum.to_dict()}
    base.get(world).frontmatter["extension"] = {"current_time": minimum.to_dict(), "time": {"current": minimum.to_dict()}}
    candidate = deepcopy(base)
    candidate.get(world).frontmatter["current_time"] = maximum.to_dict()
    candidate.get(scene).frontmatter["time"]["current"] = maximum.to_dict()
    candidate.get(world).frontmatter["extension"]["current_time"] = maximum.to_dict()
    report = _delta(base, candidate, scope, {world: [0], scene: [1]}, maximum)
    assert report["outcome"] == "ok"
    sections = {value["section"]: value for record in report["unattributedRecordChanges"] for value in record["sections"]}
    assert sections["frontmatter.current_time"]["before"]["value"] == {"timeline": "main", "tick": str(TICK_MIN), "order": str(ORDER_MIN)}
    assert sections["frontmatter.current_time"]["after"]["value"] == {"timeline": "main", "tick": str(TICK_MAX), "order": str(ORDER_MAX)}
    assert sections["frontmatter.time"]["after"]["value"]["current"] == sections["frontmatter.current_time"]["after"]["value"]
    assert isinstance(sections["frontmatter.time"]["before"]["value"]["start"]["tick"], str)
    assert isinstance(sections["frontmatter.time"]["after"]["value"]["end"]["order"], str)
    assert sections["frontmatter.extension"]["after"]["value"]["current_time"] == maximum.to_dict()
    assert sections["frontmatter.extension"]["after"]["value"]["time"]["current"] == minimum.to_dict()


def test_delta_bounds_and_invalid_candidate_close_without_partial_success():
    base, scope = _world()
    candidate = deepcopy(base)
    focus = _id("event", "focus")
    candidate.get(focus).frontmatter["effects"][-1]["value"] = "different"
    limited = _delta(base, candidate, scope, {focus: [0]}, limit=1)
    assert limited == {"protocol": "wedl-event-consequence-delta/v1", "outcome": "limit", "code": "CONSEQUENCE-LIMIT-001", "message": "Consequence limit exceeded."}
    candidate.get(focus).body = "x" * 262144
    assert _delta(base, candidate, scope, {focus: [0]})["outcome"] == "limit"
    events = [_record("event", "extra-" + str(index), time=StoryTime("main", index).to_dict(), effects=[], causes=[]) for index in range(101)]
    for record in events:
        candidate.records[record.id] = record
    scope = AuthorScope(scope.world_id, scope.record_ids | {record.id for record in events}, complete_families=scope.complete_families)
    assert _delta(base, candidate, scope, {}, StoryTime("main", 200), focus_events=[record.id for record in events])["outcome"] == "limit"
    for indexes in ([True], [-1], ["0"]):
        assert _delta(base, candidate, scope, {focus: indexes})["code"] == "CONSEQUENCE-REQUEST-001"
    assert _delta(base, candidate, scope, {}, base_valid=False)["code"] == "CONSEQUENCE-SOURCE-001"
    candidate.records = None  # Invalid candidates cannot be read/folded at all.
    assert _delta(base, candidate, scope, {}, candidate_valid=False)["outcome"] == "unavailable"
