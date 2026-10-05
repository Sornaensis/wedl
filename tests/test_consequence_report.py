from copy import deepcopy

from wedl.event_consequences import AuthorScope, CandidateIdentity, event_report
from wedl.model import ORDER_MAX, ORDER_MIN, TICK_MAX, TICK_MIN, StoryTime, World
from wedl.util import canonical_json
from test_event_consequences import REVISION, _id, _record, _subject, _transition, _world


def _report(world, scope, at=StoryTime("main", 20), event="focus", **options):
    return event_report(world, scope, event, at, **options)


def _current(report):
    assert report["outcome"] == "ok", report
    return {canonical_json(value["subject"]): value for value in report["currentAtHorizon"]}


def _entry(report, subject):
    return _current(report)[canonical_json(subject)]


def test_consolidated_report_keeps_occurrence_local_changes_and_current_status_separate():
    world, scope = _world()
    focus, later = _id("event", "focus"), _id("event", "later")
    plot = world.get(_id("story-point", "plot"))
    plot.frontmatter["lifecycle"]["transitions"] = [
        _transition("story-point", "activate", 10, state="active", cause=focus),
        _transition("story-point", "resolve-later", 14, state="resolved", cause=focus),
        _transition("story-point", "fail-later", 20, state="failed", cause=later)]
    world.get(later).frontmatter["effects"] = [{"id": _id("effect", "equal-later"), "target": _id("object", "object"), "key": "condition", "operation": "set", "value": "open"}]
    report = _report(world, scope)
    assert set(report) == {"protocol", "outcome", "revision", "event", "eventTime", "at", "timeScope", "effects", "changes", "causedTransitions", "outcomes", "causalSuccessors", "currentAtHorizon", "advisories", "expectations", "applyAllowed"}
    assert report["revision"] == REVISION and report["event"]["title"] == "focus"
    assert report["eventTime"] == {"timeline": "main", "tick": "10", "order": "0"}
    assert report["at"] == report["timeScope"]["at"] == {"timeline": "main", "tick": "20", "order": "0"}
    assert len(report["effects"]) == 2
    assert not any(value["subject"].get("recordId") == _id("object", "other") for value in report["changes"] + report["currentAtHorizon"])
    assert {value["subject"]["kind"] for value in report["changes"]} == {"state", "knowledge", "relationship", "story-point"}
    belief = next(value for value in report["changes"] if value["subject"]["kind"] == "knowledge")
    assert belief["before"]["payload"]["state"] == "suspected" and belief["after"]["payload"]["state"] == "accepted"
    historical = report["causedTransitions"]
    assert {value["kind"] for value in historical} == {"knowledge", "relationship", "story-point"}
    assert [value["time"]["tick"] for value in historical] == ["10", "10", "10", "14"]
    assert historical[-1]["atEventTime"] is False
    current = _entry(report, _subject("knowledge", "belief"))
    assert current["snapshot"]["payload"]["state"] == "rejected" and current["supersession"] == "superseded"
    assert current["supersedingCitations"][0]["memberId"] == _id("knowledge-transition", "reject")
    equal_value = _entry(report, _subject("state", "object", key="condition"))
    assert equal_value["snapshot"]["payload"] == {"value": "open"} and equal_value["supersession"] == "superseded"
    assert equal_value["supersedingCitations"][0]["recordId"] == later
    assert _entry(report, _subject("story-point", "plot"))["snapshot"]["payload"]["storedState"] == "failed"
    assert _entry(report, _subject("story-point", "plot"))["supersession"] == "superseded"
    assert report["causalSuccessors"][0]["event"]["id"] == later
    assert report["advisories"] == [] and report["expectations"] == [] and report["applyAllowed"] is True


def test_later_focus_caused_learning_is_history_not_other_event_supersession():
    world, scope = _world()
    report = _report(world, scope, StoryTime("main", 30))
    belief = _entry(report, _subject("knowledge", "belief"))
    assert belief["snapshot"]["payload"]["state"] == "forgotten"
    assert belief["supersession"] == "unchanged" and belief["supersedingCitations"] == []
    transition = next(value for value in report["causedTransitions"] if value["time"]["tick"] == "30")
    assert transition["transition"]["state"] == "forgotten" and transition["atEventTime"] is False
    assert next(value for value in report["changes"] if value["subject"]["kind"] == "knowledge")["after"]["payload"]["state"] == "accepted"
    # The reader exposes direct successors, never a transitive causal closure.
    descendant = _record("event", "descendant", time=StoryTime("main", 40).to_dict(), effects=[], causes=[_id("event", "later")])
    world.records[descendant.id] = descendant
    scope = AuthorScope(scope.world_id, scope.record_ids | {descendant.id}, complete_families=scope.complete_families)
    report = _report(world, scope, StoryTime("main", 40))
    assert [value["event"]["title"] for value in report["causalSuccessors"]] == ["later"]


def test_noop_effects_null_clear_and_root_empty_report_do_not_claim_completeness():
    world, scope = _world()
    focus = world.get(_id("event", "focus"))
    target = _id("object", "object")
    focus.frontmatter["effects"].extend([
        {"id": _id("effect", "noop-add"), "target": target, "key": "tokens", "operation": "add-to-set", "value": 2},
        {"id": _id("effect", "clear-null"), "target": target, "key": "null", "operation": "clear"}])
    report = _report(world, scope)
    assert len(report["effects"]) == 4
    assert not any(value["subject"].get("key") == "tokens" for value in report["changes"])
    noop = _entry(report, _subject("state", "object", key="tokens"))
    assert noop["snapshot"]["payload"] == {"value": [1, 2]} and noop["supersession"] == "unchanged"
    cleared = _entry(report, _subject("state", "object", key="null"))
    assert cleared["snapshot"]["presence"] == "absent" and cleared["snapshot"]["citations"][0]["memberId"] == _id("effect", "clear-null")
    assert cleared["supersession"] == "unchanged"
    local = next(value for value in report["changes"] if value["subject"].get("key") == "null")
    assert local["before"]["payload"] == {"value": None} and local["after"]["presence"] == "absent"
    quiet = _record("event", "quiet", time=StoryTime("main", 5).to_dict(), effects=[], causes=[])
    world.records[quiet.id] = quiet
    scope = AuthorScope(scope.world_id, scope.record_ids | {quiet.id}, complete_families=scope.complete_families)
    report = _report(world, scope, event="quiet")
    assert report["outcome"] == "ok"
    for field in ("effects", "changes", "causedTransitions", "outcomes", "causalSuccessors", "currentAtHorizon", "advisories", "expectations"):
        assert report[field] == []
    assert "complete" not in report and "inferred" not in canonical_json(report)


def test_reciprocal_story_point_asymmetries_are_advisory_and_scene_half_is_normal():
    for missing in ("event", "plot"):
        world, scope = _world()
        focus, plot = _id("event", "focus"), _id("story-point", "plot")
        if missing == "event":
            world.get(focus).frontmatter["related_story_points"] = []
        else:
            world.get(plot).frontmatter["outcome_events"] = []
        report = _report(world, scope)
        assert report["outcome"] == "ok" and len(report["advisories"]) == 1
        advisory = report["advisories"][0]
        assert advisory["code"] == "CONSEQUENCE-LINK-001" and advisory["target"]["id"] == plot
        assert {value["section"] for value in advisory["citations"]} == {"outcome_events" if missing == "event" else "related_story_points"}
        scene = next(value for value in report["outcomes"] if value["target"]["kind"] == "scene")
        assert scene["reciprocal"] is False and len(scene["citations"]) == 1
        current = _entry(report, {"kind": "outcome", "recordId": focus, "targetId": plot, "targetKind": "story-point"})
        assert current["supersession"] == "unchanged"
    # A withheld reciprocal half cannot be called missing or become advisory.
    sections = {identifier: frozenset({"frontmatter", "effects", "initial_state", "transitions", "lifecycle", "trigger", "dependencies", "causes", "related_story_points", "outcome_events"}) for identifier in scope.record_ids}
    sections[plot] = sections[plot] - {"outcome_events"}
    partial = AuthorScope(scope.world_id, scope.record_ids, sections, scope.complete_families)
    assert _report(world, partial)["outcome"] == "unavailable"


def test_genealogy_occurrence_preserves_author_provenance_and_unlearned_is_unknown():
    world, scope = _world()
    world.world_record.frontmatter.update(schema="wedl/v0.7", capabilities=["generational-core-v1", "generational-knowledge-v1"])
    belief = world.get(_id("knowledge", "belief"))
    belief.frontmatter["claim"]["genealogy"] = {"kind": "parentage", "payload": {"child_id": _id("character", "one"), "parent_id": _id("character", "two"), "basis": "biological"}, "valid": {"from": StoryTime("main", 0).to_dict()}}
    belief.frontmatter["transitions"][0]["state"] = "rejected"
    belief.frontmatter["transitions"][1].update(source_entity=_id("character", "two"), note="Literal permitted author evidence.")
    report = _report(world, scope)
    source = next(value for value in report["causedTransitions"] if value["kind"] == "knowledge")
    assert source["transition"]["causing_event"] == _id("event", "focus")
    assert source["transition"]["source_entity"] == _id("character", "two") and source["transition"]["note"] == "Literal permitted author evidence."
    assert _entry(report, _subject("knowledge", "belief"))["supersession"] == "superseded"
    belief.frontmatter["transitions"][1]["state"] = "rejected"
    report = _report(world, scope, StoryTime("main", 10))
    current = _entry(report, _subject("knowledge", "belief"))
    assert current["snapshot"] == {"presence": "absent", "payload": None, "citations": []}
    assert current["supersession"] == "unknown" and current["supersedingCitations"] == []


def test_report_horizon_scope_privacy_and_signed_event_extrema_are_exact():
    class Poison:
        def __deepcopy__(self, memo):
            raise AssertionError("excluded data was copied")
    world, scope = _world()
    expected = _report(world, scope)
    future = world.get(_id("knowledge", "belief")).frontmatter["transitions"][-1]
    future["note"] = Poison()
    excluded = _record("event", "excluded", time=StoryTime("main", 1).to_dict(), effects=[])
    excluded.frontmatter["title"] = Poison()
    world.records[excluded.id] = excluded
    assert _report(world, scope) == expected
    for event in ("missing", "excluded"):
        assert _report(world, scope, event=event) == {"protocol": "wedl-event-consequences/v1", "outcome": "unavailable", "code": "CONSEQUENCE-UNAVAILABLE-001", "message": "Consequence unavailable."}
    assert _report(world, scope, StoryTime("main", 9))["outcome"] == "unavailable"
    sections = {identifier: frozenset({"frontmatter", "effects", "initial_state", "transitions", "lifecycle", "trigger", "dependencies", "causes", "related_story_points", "outcome_events"}) for identifier in scope.record_ids}
    focus = _id("event", "focus")
    sections[focus] = frozenset({"frontmatter.title", "effects", "related_story_points"})
    partial = AuthorScope(scope.world_id, scope.record_ids, sections, scope.complete_families)
    assert _report(world, partial)["outcome"] == "unavailable"
    assert _report(world, {"mode": "author"})["outcome"] == "unavailable"
    assert _report(world, AuthorScope(scope.world_id, scope.record_ids))["outcome"] == "unavailable"
    # Event references resolve names/aliases/slugs only within admitted events.
    # Other record kinds cannot make a unique event ambiguous.
    clean, grant = _world()
    expected_event = _report(clean, grant)
    other = clean.get(_id("object", "other"))
    other.frontmatter["title"] = "focus"
    assert _report(clean, grant) == expected_event
    other.frontmatter["title"], other.frontmatter["aliases"] = "different object", ["focus"]
    assert _report(clean, grant) == expected_event
    clean.get(_id("event", "focus")).frontmatter["title"] = "Focus Event"
    other.frontmatter["title"], other.frontmatter["aliases"] = "Focus Event", ["focus-event"]
    assert _report(clean, grant, event="focus-event")["event"]["id"] == _id("event", "focus")
    assert _report(clean, grant, event=other.id)["code"] == "CONSEQUENCE-REQUEST-001"
    hidden = _record("event", "hidden-collision", time=StoryTime("main", 5).to_dict(), effects=[])
    hidden.frontmatter["title"], hidden.frontmatter["aliases"], hidden.frontmatter["effects"] = Poison(), ["focus-event"], Poison()
    clean.records[hidden.id] = hidden
    expected = _report(clean, grant, event="focus-event")
    assert expected["outcome"] == "ok"
    assert _report(clean, grant, event=hidden.id)["outcome"] == "unavailable"
    future_collision = _record("event", "future-collision", time=StoryTime("main", 100).to_dict(), effects=[])
    future_collision.frontmatter.update(title="Focus Event", aliases=["focus-event"], effects=Poison())
    clean.records[future_collision.id] = future_collision
    grant = AuthorScope(grant.world_id, grant.record_ids | {future_collision.id}, complete_families=grant.complete_families)
    assert _report(clean, grant, event="focus-event") == expected
    assert _report(clean, grant, event=future_collision.id)["outcome"] == "unavailable"
    duplicate = _record("event", "eligible-collision", time=StoryTime("main", 5).to_dict(), effects=[], causes=[])
    duplicate.frontmatter.update(title="Focus Event", aliases=["focus-event"])
    clean.records[duplicate.id] = duplicate
    grant = AuthorScope(grant.world_id, grant.record_ids | {duplicate.id}, complete_families=grant.complete_families)
    for name in ("Focus Event", "focus-event"):
        assert _report(clean, grant, event=name) == {"protocol": "wedl-event-consequences/v1", "outcome": "invalid", "code": "CONSEQUENCE-REQUEST-001", "message": "Invalid consequence request."}
    assert _report(clean, grant, event=_id("event", "focus"))["outcome"] == "ok"
    for point in (StoryTime("main", TICK_MIN, ORDER_MIN), StoryTime("main", TICK_MAX, ORDER_MAX)):
        config = _record("world", "world", default_timeline="main", timelines=[{"id": "main"}])
        target = _record("object", "target", initial_state={"condition": "closed"})
        focus = _record("event", "focus", time=point.to_dict(), effects=[{"id": _id("effect", "extreme"), "target": target.id, "key": "condition", "operation": "set", "value": "open"}], causes=[])
        minimal = World(REVISION, "b" * 40, {record.id: record for record in (config, target, focus)}, world.root)
        grant = AuthorScope(config.id, frozenset(minimal.records), complete_families=frozenset({"state", "knowledge"}))
        report = _report(minimal, grant, point)
        assert report["outcome"] == "ok" and report["eventTime"] == {"timeline": "main", "tick": str(point.tick), "order": str(point.order)}
        assert report["changes"][0]["before"]["payload"] == {"value": "closed"}
        assert report["changes"][0]["after"]["payload"] == {"value": "open"}


def test_report_candidate_provenance_cache_isolation_and_deterministic_order():
    base, scope = _world()
    candidate = deepcopy(base)
    candidate.get(_id("event", "focus")).frontmatter["effects"][-1]["value"] = "candidate-value"
    base._cache["sentinel"] = object()
    candidate._cache["sentinel"] = object()
    caches, records = (dict(base._cache), dict(candidate._cache)), (deepcopy(base.records), deepcopy(candidate.records))
    identity = CandidateIdentity(REVISION, "c" * 64)
    report = _report(candidate, scope, candidate=identity, base=base)
    assert report["outcome"] == "ok" and report["revision"] == REVISION
    assert report["effects"][0]["citation"]["provenance"] == {"kind": "candidate", "baseRevision": REVISION, "requestHash": "c" * 64}
    assert _entry(report, _subject("knowledge", "belief"))["snapshot"]["citations"][0]["provenance"]["kind"] == "source"
    assert (base._cache, candidate._cache) == caches and (base.records, candidate.records) == records
    shuffled = deepcopy(candidate)
    shuffled.records = dict(reversed(list(shuffled.records.items())))
    assert _report(shuffled, scope, candidate=identity, base=base) == report


def test_report_aggregate_bounds_and_invalid_source_emit_only_failure():
    world, scope = _world()
    limited = _report(world, scope, limit=10)
    assert limited == {"protocol": "wedl-event-consequences/v1", "outcome": "limit", "code": "CONSEQUENCE-LIMIT-001", "message": "Consequence limit exceeded."}
    world.get(_id("knowledge", "belief")).frontmatter["transitions"][1]["note"] = "x" * 262144
    assert _report(world, scope)["outcome"] == "limit"
    for limit in (0, 1001, True):
        assert _report(world, scope, limit=limit)["code"] == "CONSEQUENCE-REQUEST-001"
    assert _report(world, scope, event="")["outcome"] == "invalid"
    assert _report(world, scope, event=_id("object", "object"))["code"] == "CONSEQUENCE-REQUEST-001"
    world.records = None
    assert _report(world, scope, source_valid=False) == {"protocol": "wedl-event-consequences/v1", "outcome": "invalid", "code": "CONSEQUENCE-SOURCE-001", "message": "Invalid consequence source."}
