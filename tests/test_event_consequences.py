from copy import deepcopy
import hashlib
from pathlib import Path

import pytest

from wedl.event_consequences import AuthorScope, CandidateIdentity, Projection, ProjectionFailure, bounded, time_value
from wedl.ids import id_from_seed
from wedl.model import ORDER_MAX, ORDER_MIN, TICK_MAX, TICK_MIN, Record, StoryTime, World
from wedl.semantics import current_knowledge, current_relationship, evaluate_story_point, resolve_state
from wedl.util import canonical_json

REVISION = "a" * 40


def _id(kind, name):
    return id_from_seed(kind, name)


def _record(kind, name, **fields):
    fm = {"schema": "wedl/v0.3", "kind": kind, "id": _id(kind, name), "title": name,
          "status": "canonical", "domain": "story", "tags": [], "aliases": [], **fields}
    raw = canonical_json(fm).encode()
    blob = hashlib.sha1(b"blob " + str(len(raw)).encode() + b"\0" + raw).hexdigest()
    return Record(fm, "Literal authored prose", f"story/{kind}/{name}.md", raw, blob, REVISION)


def _transition(kind, name, tick, *, state=None, cause=None, **fields):
    value = {"id": _id(kind + "-transition", name), "time": StoryTime("main", tick).to_dict(), **fields}
    if state is not None:
        value["state"] = state
    if cause is not None:
        value["causing_event"] = cause
    return value


def _world(point=StoryTime("main", 10)):
    world = _record("world", "world", default_timeline="main", timelines=[{"id": "main"}, {"id": "elsewhere"}],
                    state_keys={"object": {"condition": {"type": "string"}, "holder": {"type": "entity", "entity_kind": "character"}}})
    one = _record("character", "one")
    two = _record("character", "two")
    object = _record("object", "object", initial_state={"condition": "closed", "null": None, "tokens": [1, 2]})
    other = _record("object", "other", initial_state={"condition": "unaffected"})
    focus = _record("event", "focus", time=point.to_dict(), effects=[
        {"id": _id("effect", "first"), "target": object.id, "key": "condition", "operation": "set", "value": "closed"},
        {"id": _id("effect", "last"), "target": object.id, "key": "condition", "operation": "set", "value": "open"}],
        causes=[], related_story_points=[])
    disjoint = _record("event", "disjoint", time=point.to_dict(), effects=[
        {"id": _id("effect", "disjoint"), "target": other.id, "key": "condition", "operation": "set", "value": "changed"}], causes=[])
    knowledge = _record("knowledge", "belief", knower=one.id, claim={"key": "rescue", "statement": "An authored belief"}, transitions=[
        _transition("knowledge", "initial", 0, state="suspected"),
        _transition("knowledge", "accept", 10, state="accepted", cause=focus.id, confidence=0.9),
        _transition("knowledge", "reject", 20, state="rejected", cause=_id("event", "later")),
        _transition("knowledge", "forget", 30, state="forgotten", cause=focus.id)])
    relationship = _record("relationship", "relationship", **{"from": one.id, "to": two.id}, relationship_kind="trust", transitions=[
        _transition("relationship", "before", 0, metrics={"trust": 0.2}, facets=["old"]),
        _transition("relationship", "after", 10, cause=focus.id, metrics={"trust": 0.8})])
    plot = _record("story-point", "plot", lifecycle={"initial_state": "dormant", "transitions": []},
                   dependencies={}, trigger={"entity_state": {"target": object.id, "key": "condition", "equals": "open"}}, outcome_events=[focus.id])
    focus.frontmatter["related_story_points"] = [plot.id]
    scene = _record("scene", "scene", time={"start": StoryTime("main", 0).to_dict(), "end": StoryTime("main", 40).to_dict()}, outcome_events=[focus.id])
    later = _record("event", "later", time=StoryTime("main", 20).to_dict(), effects=[], causes=[focus.id])
    values = [world, one, two, object, other, focus, disjoint, knowledge, relationship, plot, scene, later]
    result = World(REVISION, "b" * 40, {record.id: record for record in values}, Path("source-only"))
    scope = AuthorScope(world.id, frozenset(result.records), complete_families=frozenset({"state", "knowledge"}))
    return result, scope


def _subject(kind, name, **fields):
    return {"kind": kind, "recordId": _id("object" if kind == "state" else kind, name), **fields}


def _unavailable(call):
    with pytest.raises(ProjectionFailure) as failure:
        call()
    assert failure.value.outcome == "unavailable"


def test_event_local_extrema_order_and_disjoint_coordinates():
    for point in (StoryTime("main", TICK_MIN, ORDER_MIN), StoryTime("main", TICK_MAX, ORDER_MAX)):
        world, scope = _world(point)
        focus = world.get(_id("event", "focus"))
        target = _id("object", "object")
        focus.frontmatter["effects"].extend([
            {"id": _id("effect", "clear"), "target": target, "key": "null", "operation": "clear"},
            {"id": _id("effect", "add"), "target": target, "key": "tokens", "operation": "add-to-set", "value": 3},
            {"id": _id("effect", "remove"), "target": target, "key": "tokens", "operation": "remove-from-set", "value": 1}])
        projection = Projection(world, scope, point)
        before, after = projection.event_views("focus")
        condition = _subject("state", "object", key="condition")
        assert before.snapshot(condition)["payload"] == {"value": "closed"}
        assert after.snapshot(condition)["payload"] == {"value": "open"}
        assert before.world.get(focus.id).frontmatter["effects"] == []
        assert after.snapshot(_subject("state", "object", key="tokens"))["payload"] == {"value": [2, 3]}
        assert before.snapshot(_subject("state", "object", key="null"))["payload"] == {"value": None}
        cleared = after.snapshot(_subject("state", "object", key="null"))
        assert cleared["presence"] == "absent" and cleared["citations"][0]["memberId"] == _id("effect", "clear")
        assert before.snapshot(_subject("state", "other", key="condition")) == after.snapshot(_subject("state", "other", key="condition"))
        assert projection.effects("focus")[0]["effect"]["value"] == "closed"
        assert time_value(point) == {"timeline": "main", "tick": str(point.tick), "order": str(point.order)}


def test_delayed_transitions_forgotten_rejected_and_canonical_folds():
    world, scope = _world()
    projection = Projection(world, scope, StoryTime("main", 20))
    before, after = projection.event_views("focus")
    knowledge = _subject("knowledge", "belief")
    assert before.snapshot(knowledge)["payload"]["state"] == "suspected"
    assert after.snapshot(knowledge)["payload"]["state"] == "accepted"
    assert projection.snapshot(knowledge)["payload"]["state"] == "rejected"
    assert projection.snapshot(knowledge)["payload"]["state"] == current_knowledge(projection.world, _id("character", "one"), projection.at, True)[0]["state"]
    caused = projection.caused_transitions("focus")
    assert len(caused) == 2 and all(value["atEventTime"] for value in caused)
    later = projection.at_time(StoryTime("main", 30))
    assert later.snapshot(knowledge)["payload"]["state"] == "forgotten"
    assert [value["atEventTime"] for value in later.caused_transitions("focus")] == [True, True, False]
    relationship = projection.snapshot(_subject("relationship", "relationship"))["payload"]
    canonical = current_relationship(projection.world, _id("relationship", "relationship"), projection.at)
    assert relationship["metrics"] == canonical["metrics"] == {"trust": 0.8}
    assert relationship["facets"] == []  # Latest snapshots do not merge history.
    plot = _subject("story-point", "plot")
    assert before.snapshot(plot)["payload"]["eligible"] is False
    assert after.snapshot(plot)["payload"]["eligible"] is True
    assert after.snapshot(plot)["payload"]["storedState"] == "dormant"
    assert any(value["recordId"] == _id("event", "focus") for value in after.snapshot(plot)["citations"])
    assert after.snapshot(plot)["payload"]["derivedState"] == evaluate_story_point(after.world, after.world.get(plot["recordId"]), after.at)["derivedState"]


def test_equal_time_uncaused_transitions_and_explicit_reverse_links():
    world, scope = _world()
    knowledge = world.get(_id("knowledge", "belief"))
    knowledge.frontmatter["transitions"][1].pop("causing_event")
    projection = Projection(world, scope, StoryTime("main", 30))
    before, after = projection.event_views("focus")
    subject = _subject("knowledge", "belief")
    assert before.snapshot(subject) == after.snapshot(subject)
    assert projection.caused_transitions("focus")[-1]["sourceOrdinal"] == 3
    assert projection.successors("focus")[0]["event"]["id"] == _id("event", "later")
    assert projection.successors("later") == []
    links = projection.outcomes("focus")
    assert [(value["target"]["kind"], value["reciprocal"]) for value in links] == [("scene", False), ("story-point", True)]
    assert projection.effects("later") == [] and projection.caused_transitions("later")[0]["kind"] == "knowledge"
    with pytest.raises(ProjectionFailure) as failure:
        Projection(world, scope, StoryTime("main", 9)).reference("focus")
    assert failure.value.outcome == "unavailable"


def test_scope_horizon_sections_and_excluded_poisoning():
    class Poison:
        def __deepcopy__(self, memo):
            raise AssertionError("excluded data was copied")
        def __str__(self):
            raise AssertionError("excluded data was named")
    # The trusted grant closes only the needed evidence families. It remains
    # identical while unrelated denied source records are added/mutated/deleted.
    clean, grant = _world()
    point = StoryTime("main", 20)
    subject = _subject("state", "other", key="condition")
    def visible():
        value = Projection(clean, grant, point)
        return {"snapshot": value.snapshot(subject), "effects": value.effects("focus"),
                "reference": value.reference("focus"), "successors": value.successors("focus"),
                "outcomes": value.outcomes("focus"), "subjects": value.subjects()}
    expected = visible()
    assert expected["snapshot"]["payload"] == {"value": "changed"}
    excluded = _record("object", "unrelated-denied", initial_state={"condition": "secret"})
    clean.records[excluded.id] = excluded
    assert visible() == expected
    excluded.frontmatter.update(title=Poison(), initial_state=Poison(), kind="knowledge", transitions=Poison())
    assert visible() == expected
    del clean.records[excluded.id]
    assert visible() == expected
    # Missing completeness is unavailable, never canonical absence; folds
    # unrelated to that family (relationship/stored plot) remain available.
    incomplete = Projection(clean, AuthorScope(grant.world_id, grant.record_ids), point)
    _unavailable(lambda: incomplete.snapshot(subject))
    assert incomplete.snapshot(_subject("relationship", "relationship"))["payload"]["metrics"] == {"trust": 0.8}
    _unavailable(lambda: incomplete.snapshot(_subject("state", "unrelated-denied", key="condition")))
    assert Projection(clean, grant, point).snapshot(_subject("state", "other", key="absent-key")) == {"presence": "absent", "payload": None, "citations": []}
    clean.get(_id("story-point", "plot")).frontmatter["trigger"] = {}
    # A stored plot with no state/knowledge predicate needs neither family.
    assert Projection(clean, AuthorScope(grant.world_id, grant.record_ids), point).snapshot(_subject("story-point", "plot"))["payload"]["storedState"] == "dormant"
    world, scope = _world()
    hidden = _record("event", "hidden", time={"bad": True}, effects=[])
    hidden.frontmatter["title"] = Poison()
    world.records[hidden.id] = hidden
    future = _record("event", "future", time=StoryTime("main", 100).to_dict(), effects=[])
    future.frontmatter["effects"] = Poison()
    world.records[future.id] = future
    other_timeline = _record("event", "foreign", time=StoryTime("elsewhere", 0).to_dict(), effects=[])
    other_timeline.frontmatter["effects"] = Poison()
    world.records[other_timeline.id] = other_timeline
    scope = AuthorScope(scope.world_id, scope.record_ids | {future.id, other_timeline.id}, complete_families=scope.complete_families)
    world.get(_id("knowledge", "belief")).frontmatter["transitions"][-1]["note"] = Poison()
    projection = Projection(world, scope, StoryTime("main", 20))
    assert projection.reference("focus")["title"] == "focus"
    for reference in ("hidden", "future", "foreign", "missing"):
        with pytest.raises(ProjectionFailure) as failure:
            projection.reference(reference)
        assert (failure.value.outcome, failure.value.message) == ("unavailable", "Consequence unavailable.")
    assert all(value["time"]["tick"] != "30" for value in projection.caused_transitions("focus"))
    # Exclude upstream records before folds; incomplete plot input never throws
    # or turns a withheld state into a derived assertion.
    restricted = AuthorScope(scope.world_id, scope.record_ids - {_id("object", "object")})
    narrowed = Projection(world, restricted, StoryTime("main", 20))
    assert narrowed.effects("focus") == []
    _unavailable(lambda: narrowed.snapshot(_subject("story-point", "plot")))
    sections = {identifier: frozenset({"frontmatter.title", "frontmatter.time", "effects", "transitions", "lifecycle.initial_state", "lifecycle.transitions", "outcome_events", "related_story_points", "causes", "trigger", "dependencies"}) for identifier in scope.record_ids}
    sliced = Projection(world, AuthorScope(scope.world_id, scope.record_ids, sections), StoryTime("main", 20))
    before, _ = sliced.event_views("focus")
    _unavailable(lambda: before.snapshot(_subject("state", "object", key="condition")))
    # Withheld event time is excluded before parsing, naming, indexing or fold
    # defaults. Changing denied values cannot change an unavailable answer.
    clean, grant = _world()
    allowed = {identifier: frozenset({"frontmatter", "effects", "initial_state", "transitions", "lifecycle", "trigger", "dependencies", "outcome_events", "related_story_points", "causes"}) for identifier in grant.record_ids}
    focus_id, later_id = _id("event", "focus"), _id("event", "later")
    allowed[focus_id] = allowed[focus_id] - {"frontmatter"} | {"frontmatter.title"}
    denied = AuthorScope(grant.world_id, grant.record_ids, allowed, grant.complete_families)
    for withheld_time in (StoryTime("main", 10).to_dict(), StoryTime("main", 100).to_dict(), Poison()):
        clean.get(focus_id).frontmatter["time"] = withheld_time
        limited = Projection(clean, denied, StoryTime("main", 20))
        assert limited.successors("later") == []
        assert limited.caused_transitions("later")[0]["transition"].get("causing_event") == later_id
        for method in (limited.reference, limited.effects, limited.successors, limited.outcomes, limited.event_views):
            with pytest.raises(ProjectionFailure) as failure:
                method("focus")
            assert failure.value.outcome == "unavailable"
        _unavailable(lambda: limited.snapshot(_subject("state", "object", key="condition")))
    clean, grant = _world()
    plot_id = _id("story-point", "plot")
    full = {identifier: frozenset({"frontmatter", "effects", "initial_state", "transitions", "lifecycle", "trigger", "dependencies", "outcome_events", "related_story_points", "causes"}) for identifier in grant.record_ids}
    for omitted in ("lifecycle.initial_state", "lifecycle.transitions", "trigger", "dependencies"):
        selected = dict(full)
        selected[plot_id] = frozenset({"frontmatter", "lifecycle.initial_state", "lifecycle.transitions", "trigger", "dependencies", "outcome_events"} - {omitted})
        for stored in ("dormant", "active", "resolved"):
            clean.get(plot_id).frontmatter["lifecycle"]["initial_state"] = stored
            _unavailable(lambda: Projection(clean, AuthorScope(grant.world_id, grant.record_ids, selected, grant.complete_families), StoryTime("main", 20)).snapshot(_subject("story-point", "plot")))
        owner = clean.get(plot_id).frontmatter["lifecycle"] if omitted.startswith("lifecycle.") else clean.get(plot_id).frontmatter
        field = omitted.rsplit(".", 1)[-1]
        original = owner[field]
        owner[field] = Poison()
        _unavailable(lambda: Projection(clean, AuthorScope(grant.world_id, grant.record_ids, selected, grant.complete_families), StoryTime("main", 20)).snapshot(_subject("story-point", "plot")))
        owner[field] = original
    # Incomplete knowledge authorization cannot turn `not learned` into true.
    clean, grant = _world()
    plot = clean.get(plot_id)
    plot.frontmatter["trigger"] = {"not": {"knowledge": {"knower": _id("character", "one"), "claim_key": "rescue", "state_in": ["accepted"]}}}
    admitted = Projection(clean, grant, StoryTime("main", 10))
    assert admitted.snapshot(_subject("story-point", "plot"))["payload"]["eligible"] is False
    assert admitted.snapshot(_subject("story-point", "plot"))["payload"]["eligible"] == evaluate_story_point(admitted.world, admitted.world.get(plot_id), admitted.at)["eligible"]
    hidden_belief = _id("knowledge", "belief")
    restricted = AuthorScope(grant.world_id, grant.record_ids - {hidden_belief})
    for state in ("accepted", "rejected", "forgotten"):
        clean.get(hidden_belief).frontmatter["transitions"][1]["state"] = state
        _unavailable(lambda: Projection(clean, restricted, StoryTime("main", 10)).snapshot(_subject("story-point", "plot")))
    for omitted in ("frontmatter.knower", "frontmatter.claim", "transitions"):
        selected = dict(full)
        selected[hidden_belief] = frozenset({"frontmatter.title", "frontmatter.aliases", "frontmatter.knower", "frontmatter.claim", "transitions"} - {omitted})
        _unavailable(lambda: Projection(clean, AuthorScope(grant.world_id, grant.record_ids, selected, grant.complete_families), StoryTime("main", 10)).snapshot(_subject("story-point", "plot")))
    # Complete admitted evidence retains the canonical negative/negated fold:
    # future learning and a genuinely absent claim are known absence at H.
    clean.get(hidden_belief).frontmatter["transitions"] = [_transition("knowledge", "future-learning", 100, state="accepted")]
    complete = Projection(clean, grant, StoryTime("main", 10))
    assert complete.snapshot(_subject("knowledge", "belief")) == {"presence": "absent", "payload": None, "citations": []}
    assert complete.snapshot(_subject("story-point", "plot"))["payload"]["eligible"] is True
    plot.frontmatter["trigger"]["not"]["knowledge"]["claim_key"] = "absent-claim"
    assert Projection(clean, grant, StoryTime("main", 10)).snapshot(_subject("story-point", "plot"))["payload"]["eligible"] is True


def test_input_cache_isolation_candidate_provenance_and_ordered_citations():
    world, scope = _world()
    world._cache["effects-by-target"] = {"poison": object()}
    world._cache["canonical-events"] = ()
    before_fm = deepcopy({key: value.frontmatter for key, value in world.records.items()})
    cache_identity = dict(world._cache)
    projection = Projection(world, scope, StoryTime("main", 20))
    before, after = projection.event_views("focus")
    assert before.world._cache is not after.world._cache and projection.world._cache is not world._cache
    subject = _subject("state", "object", key="condition")
    assert after.snapshot(subject)["payload"] == {"value": "open"}
    assert world._cache == cache_identity and {key: value.frontmatter for key, value in world.records.items()} == before_fm
    candidate = deepcopy(world)
    changed = candidate.get(_id("event", "focus"))
    changed.frontmatter["effects"][-1]["value"] = "different"
    identity = CandidateIdentity(REVISION, "c" * 64)
    projected = Projection(candidate, scope, StoryTime("main", 20), candidate=identity, base=world)
    citation = projected.snapshot(subject)["citations"][0]
    assert citation["provenance"] == {"kind": "candidate", "baseRevision": REVISION, "requestHash": "c" * 64}
    assert projected.snapshot(_subject("state", "other", key="condition"))["citations"][0]["provenance"]["kind"] == "source"
    # Ordinals bind original source positions after future links disappear.
    plot = world.get(_id("story-point", "plot"))
    plot.frontmatter["outcome_events"].insert(0, _id("event", "later"))
    links = Projection(world, scope, StoryTime("main", 10)).outcomes("focus")
    assert next(value for value in links if value["target"]["kind"] == "story-point")["citations"][-1]["sourceOrdinal"] == 1


def test_genealogy_author_occurrence_and_unlearned_fold_boundary():
    world, scope = _world()
    world.world_record.frontmatter.update(schema="wedl/v0.7", capabilities=["generational-core-v1", "generational-knowledge-v1"])
    knowledge = world.get(_id("knowledge", "belief"))
    knowledge.frontmatter["claim"]["genealogy"] = {"kind": "parentage", "payload": {"child_id": _id("character", "one"),
        "parent_id": _id("character", "two"), "basis": "biological"}, "valid": {"from": StoryTime("main", 0).to_dict()}}
    knowledge.frontmatter["transitions"][0]["state"] = "rejected"
    knowledge.frontmatter["transitions"][1].update(source_entity=_id("character", "two"), note="Author-admitted learning")
    projection = Projection(world, scope, StoryTime("main", 20))
    before, after = projection.event_views("focus")
    assert before.snapshot(_subject("knowledge", "belief"))["presence"] == "absent"
    assert after.snapshot(_subject("knowledge", "belief"))["payload"]["state"] == "accepted"
    occurrence = next(value for value in projection.caused_transitions("focus") if value["kind"] == "knowledge")
    assert occurrence["transition"]["source_entity"] == _id("character", "two")
    assert occurrence["transition"]["note"] == "Author-admitted learning"
    assert projection.snapshot(_subject("knowledge", "belief"))["payload"]["state"] == "rejected"


def test_limits_identity_and_source_only_dependency_boundary():
    world, scope = _world()
    projection = Projection(world, scope, StoryTime("main", 20), limit=1)
    with pytest.raises(ProjectionFailure) as failure:
        projection.effects("focus")
    assert (failure.value.outcome, failure.value.message) == ("limit", "Consequence limit exceeded.")
    with pytest.raises(ProjectionFailure):
        bounded({"value": "x" * 262144}, 1)
    for limit in (True, 0, 1001):
        with pytest.raises(ProjectionFailure) as failure:
            Projection(world, scope, StoryTime("main", 20), limit=limit)
        assert failure.value.outcome == "invalid"
        assert failure.value.code == "CONSEQUENCE-REQUEST-001"
    with pytest.raises(ProjectionFailure) as failure:
        Projection(world, {"mode": "author"}, StoryTime("main", 20))
    assert failure.value.outcome == "unavailable"
    world.world_record.frontmatter["schema"] = "wedl/v0.7"
    world.world_record.frontmatter["capabilities"] = ["unknown-capability"]
    with pytest.raises(ProjectionFailure) as failure:
        Projection(world, scope, StoryTime("main", 20))
    assert failure.value.outcome == "unavailable"
