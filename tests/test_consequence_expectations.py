from copy import deepcopy

import pytest

from wedl.consequence_expectations import check_time, evaluate_expectations
from wedl.event_consequences import AuthorScope, CandidateIdentity, ProjectionFailure, time_value
from wedl.model import ORDER_MAX, ORDER_MIN, TICK_MAX, TICK_MIN, StoryTime

from test_event_consequences import REVISION, _id, _record, _world


def _fixture():
    world, scope = _world()
    world.config["relationship_metrics"] = {"trust": {"minimum": -1, "maximum": 1}, "missing": {}}
    world.config["state_keys"]["object"].update({
        "null": {"type": "null"}, "nullable": {"type": "null"},
        "flag": {"type": "boolean"}, "number": {"type": "number"},
        "tokens": {"type": "array"}, "data": {"type": "object"}, "absent": {"type": "string"}})
    world.get(_id("object", "object")).frontmatter["initial_state"].update(
        flag=True, number=1, nullable=None, data={"opaque": "one", "nested": [True, 1]})
    return world, scope


def _check(kind, id="check", **fields):
    return {"id": id, "predicate": {"kind": kind, **fields}}


def _run(world, scope, items, tick=10, **kwargs):
    return evaluate_expectations(world, scope, "focus", StoryTime("main", tick), items, **kwargs)


def _failure(outcome, call):
    with pytest.raises(ProjectionFailure) as failure:
        call()
    assert failure.value.outcome == outcome
    return failure.value


def _restrict(world, scope, record, sections):
    grants = {identifier: frozenset({"frontmatter", "initial_state", "effects", "transitions", "lifecycle",
                                   "trigger", "dependencies", "related_story_points", "outcome_events", "causes"})
              for identifier in scope.record_ids}
    grants[record] = frozenset(sections)
    return AuthorScope(scope.world_id, scope.record_ids, grants, scope.complete_families)


def test_all_seven_predicates_expose_typed_actuals_and_preserve_check_order():
    world, scope = _fixture()
    checks = [
        _check("state.equals", "equals", target="object", key="condition", value="open"),
        _check("state.absent", "absent", target="object", key="absent"),
        _check("knowledge.state", "belief", knowledge="belief", state="accepted"),
        _check("relationship.matches", "relationship", relationship="relationship", values={"status": "active", "metrics": {"trust": 0.8}, "facets": []}),
        _check("story-point.state", "plot", storyPoint="plot", state="dormant"),
        _check("outcome.linked", "outcome", event="focus", target="plot"),
        _check("transition.caused", "transition", record="belief", transitionId=_id("knowledge-transition", "accept"), event="focus"),
    ]
    evaluation = _run(world, scope, checks)
    assert evaluation.apply_allowed and [value["id"] for value in evaluation.results] == [value["id"] for value in checks]
    assert all(value["outcome"] == "pass" and value["code"] == "EXPECTATION-PASS" for value in evaluation.results)
    assert [value["actual"]["kind"] for value in evaluation.results] == ["state", "state", "knowledge", "relationship", "story-point", "outcome", "transition"]
    assert evaluation.results[1]["actual"] == {"kind": "state", "observed": {"presence": "absent"}}
    assert evaluation.results[2]["actual"]["transitionId"] == _id("knowledge-transition", "accept")
    assert evaluation.results[3]["actual"]["values"]["metrics"] == {"trust": {"presence": "present", "value": 0.8}}
    assert evaluation.results[4]["actual"]["state"] == "dormant"  # Eligible is a separate derived state.
    assert evaluation.results[6]["actual"]["causingEvent"] == {"presence": "present", "value": _id("event", "focus")}
    for value in evaluation.results:
        assert value["comparison"]["world"] == {"kind": "source", "revision": REVISION}
        assert value["comparison"]["at"] == {"timeline": "main", "tick": "10", "order": "0"}
    assert evaluation.results[0]["comparison"]["predicate"]["target"] == _id("object", "object")
    assert checks[0]["predicate"]["target"] == "object"


def test_exact_json_null_absence_entity_leaves_and_declared_value_types():
    world, scope = _fixture()
    state = world.get(_id("object", "object")).frontmatter["initial_state"]
    state["holder"] = {"entity": _id("character", "one"), "opaque": "two"}
    checks = [
        _check("state.equals", "null", target="object", key="null", value=None),
        _check("state.absent", "not-null", target="object", key="null"),
        _check("state.equals", "not-absent", target="object", key="nullable", value=None),
        _check("state.equals", "number", target="object", key="number", value=1.0),
        _check("state.equals", "array", target="object", key="tokens", value=[2, 1]),
        _check("state.equals", "object", target="object", key="data", value={"nested": [1, 1], "opaque": "one"}),
        _check("state.equals", "entity", target="object", key="holder", value={"entity": "one", "opaque": "two"}),
    ]
    results = _run(world, scope, checks).results
    assert [value["outcome"] for value in results] == ["pass", "fail", "pass", "pass", "fail", "fail", "pass"]
    assert results[0]["actual"]["observed"] == {"presence": "present", "value": None}
    assert results[-1]["comparison"]["predicate"]["value"] == {"entity": _id("character", "one"), "opaque": "two"}
    for key, value in (("number", True), ("flag", 1), ("condition", None), ("holder", "one")):
        _failure("invalid", lambda: _run(world, scope, [_check("state.equals", target="object", key=key, value=value)]))
    world.config["state_keys"]["object"]["condition"].update(nullable=True, allowed_values=["closed"], allowedValues=["closed"],
        itemDefinition={"type": "entity"}, objectDefinition={"type": "entity"}, item_definition={"type": "entity"}, object_definition={"type": "entity"})
    _failure("invalid", lambda: _run(world, scope, [_check("state.equals", target="object", key="condition", value=None)]))
    world.config["state_keys"]["object"]["condition"]["extension"] = {"unrelated": "literal"}
    assert _run(world, scope, [_check("state.equals", target="object", key="condition", value="open")]).apply_allowed
    world.config["state_keys"]["object"]["data"]["properties"] = {
        "owner": {"type": "entity", "entity_kind": "character"},
        "members": {"type": "array", "items": {"type": "entity", "entity_kind": "character"}}}
    state["data"] = {"owner": {"entity": _id("character", "one"), "note": "two"},
                     "members": [{"entity": _id("character", "two")}], "extension": {"entity": "literal"}}
    expected = {"owner": {"entity": "one", "note": "two"}, "members": [{"entity": "two"}], "extension": {"entity": "literal"}}
    nested = _run(world, scope, [_check("state.equals", target="object", key="data", value=expected)]).results[0]
    assert nested["outcome"] == "pass" and nested["comparison"]["predicate"]["value"] == state["data"]
    assert nested["actual"]["observed"]["value"]["extension"] == {"entity": "literal"}
    _failure("invalid", lambda: _run(world, scope, [_check("state.equals", target="object", key="data", value={**expected, "owner": {"entity": _id("object", "object")}})]))
    hidden = AuthorScope(scope.world_id, scope.record_ids - {_id("character", "two")}, complete_families=scope.complete_families)
    unknown = _run(world, hidden, [_check("state.equals", target="object", key="data", value=expected)]).results[0]
    assert unknown["outcome"] == "unknown" and unknown["actual"] == {"kind": "redacted"} and unknown["citations"] == []
    _failure("invalid", lambda: _run(world, scope, [_check("state.equals", target="object", key="data", value={**expected, "members": "not-an-array"})]))
    world.config["state_keys"]["object"]["data"]["properties"] = "unrepresentable"
    assert _run(world, scope, [_check("state.equals", target="object", key="data", value=expected)]).results[0]["outcome"] == "unsupported"


def test_belief_status_is_not_canonical_truth_and_delayed_transition_is_literal():
    world, scope = _fixture()
    knowledge = world.get(_id("knowledge", "belief"))
    knowledge.frontmatter["claim"]["statement"] = "The object condition is closed"
    checks = [_check("knowledge.state", "belief", knowledge="belief", state="accepted"),
              _check("state.equals", "truth", target="object", key="condition", value="closed")]
    assert [value["outcome"] for value in _run(world, scope, checks).results] == ["pass", "fail"]
    for tick, state in ((20, "rejected"), (30, "forgotten")):
        result = _run(world, scope, [_check("knowledge.state", knowledge="belief", state=state)], tick).results[0]
        assert result["outcome"] == "pass" and result["actual"]["state"] == state
    delayed = _check("transition.caused", record="belief", transitionId=_id("knowledge-transition", "forget"), event="focus")
    assert _run(world, scope, [delayed], 20).results[0]["outcome"] == "unknown"
    assert _run(world, scope, [delayed], 30).results[0]["outcome"] == "pass"
    no_cause = _run(world, scope, [_check("transition.caused", record="belief", transitionId=_id("knowledge-transition", "initial"), event="focus")]).results[0]
    assert no_cause["outcome"] == "fail" and no_cause["actual"]["causingEvent"] == {"presence": "absent"}
    plot = world.get(_id("story-point", "plot"))
    plot.frontmatter["lifecycle"]["transitions"] = [{"time": StoryTime("main", 10).to_dict(), "state": "active", "causing_event": _id("event", "focus")}]
    assert _run(world, scope, [_check("transition.caused", record="plot", transitionId="invented", event="focus")]).results[0]["outcome"] == "unknown"


def test_relationship_partial_metrics_outcome_halves_and_advisory_gating():
    world, scope = _fixture()
    result = _run(world, scope, [_check("relationship.matches", relationship="relationship", values={"metrics": {"missing": 0}, "facets": ["old"]})]).results[0]
    assert result["outcome"] == "fail" and result["actual"]["values"] == {"metrics": {"missing": {"presence": "absent"}}, "facets": []}
    _failure("invalid", lambda: _run(world, scope, [_check("relationship.matches", relationship="relationship", values={"metrics": {"trust": 1.01}})]))
    world.get(_id("story-point", "plot")).frontmatter["outcome_events"] = []
    checks = [_check("outcome.linked", "plot", event="focus", target="plot"), _check("outcome.linked", "scene", event="focus", target="scene")]
    evaluation = _run(world, scope, checks)
    assert not evaluation.apply_allowed and [value["outcome"] for value in evaluation.results] == ["fail", "pass"]
    assert evaluation.results[0]["actual"] == {"kind": "outcome", "eventLinked": True, "targetLinked": False}
    assert evaluation.results[1]["actual"] == {"kind": "outcome", "eventLinked": False, "targetLinked": True}
    assert _run(world, scope, checks, policy="advisory").apply_allowed
    assert _run(world, scope, []).results == [] and _run(world, scope, []).apply_allowed


def test_unknown_unsupported_wrong_kind_and_focus_failure_are_distinct():
    world, scope = _fixture()
    checks = [_check("state.absent", "missing", target="never-authored", key="condition"),
              _check("state.absent", "unsupported", target="object", key="not-declared"),
              _check("transition.caused", "future", record="belief", transitionId=_id("knowledge-transition", "forget"), event="focus")]
    results = _run(world, scope, checks).results
    assert [value["outcome"] for value in results] == ["unknown", "unsupported", "unknown"]
    assert all(value["actual"] == {"kind": "redacted"} and value["citations"] == [] for value in results)
    assert results[0]["comparison"]["predicate"]["target"] == "never-authored"
    assert not _run(world, scope, checks).apply_allowed and _run(world, scope, checks, policy="advisory").apply_allowed
    world.config["state_keys"]["object"]["tokens"]["type"] = "integer"
    assert _run(world, scope, [_check("state.equals", target="object", key="tokens", value=[1, 2])]).results[0]["outcome"] == "unsupported"
    for check in (_check("knowledge.state", knowledge=_id("object", "object"), state="accepted"),
                  _check("outcome.linked", event="focus", target=_id("object", "object")),
                  _check("transition.caused", record=_id("scene", "scene"), transitionId="x", event="focus")):
        _failure("invalid", lambda: _run(world, scope, [check]))
    _failure("unavailable", lambda: evaluate_expectations(world, scope, "missing-focus", StoryTime("main", 10), checks))
    _failure("unavailable", lambda: _run(world, scope, [], 9))


def test_author_section_privacy_hidden_causes_and_denied_records_do_not_prove_absence():
    world, scope = _fixture()
    target = _id("object", "object")
    restricted = _restrict(world, scope, target, {"frontmatter", "initial_state.condition"})
    checks = [_check("state.absent", "null", target="object", key="null"), _check("state.equals", "condition", target="object", key="condition", value="open")]
    expected = _run(world, restricted, checks)
    assert [value["outcome"] for value in expected.results] == ["unknown", "pass"]
    world.get(target).frontmatter["initial_state"]["null"] = "secret changed"
    poison = _record("object", "secret unrelated", initial_state={"null": "secret"})
    world.records[poison.id] = poison
    assert _run(world, restricted, checks) == expected
    denied = AuthorScope(scope.world_id, scope.record_ids - {_id("event", "later")}, complete_families=scope.complete_families)
    result = _run(world, denied, [_check("transition.caused", record="belief", transitionId=_id("knowledge-transition", "reject"), event="focus")], 20).results[0]
    assert result["outcome"] == "unknown" and result["actual"] == {"kind": "redacted"} and result["citations"] == []
    plot_scope = _restrict(world, scope, _id("story-point", "plot"), {"frontmatter", "lifecycle"})
    assert _run(world, plot_scope, [_check("story-point.state", storyPoint="plot", state="dormant")]).apply_allowed
    denied_lifecycle = _restrict(world, scope, _id("story-point", "plot"), {"frontmatter", "trigger", "dependencies"})
    assert _run(world, denied_lifecycle, [_check("story-point.state", storyPoint="plot", state="dormant")]).results[0]["outcome"] == "unknown"
    denied_halves = _restrict(world, scope, _id("story-point", "plot"), {"frontmatter", "lifecycle", "trigger", "dependencies"})
    assert _run(world, denied_halves, [_check("outcome.linked", event="focus", target="plot")]).results[0]["outcome"] == "unknown"
    incomplete = AuthorScope(scope.world_id, scope.record_ids)
    assert _run(world, incomplete, [_check("state.absent", target="object", key="absent")]).results[0]["outcome"] == "unknown"
    entity_scope = AuthorScope(scope.world_id, scope.record_ids - {_id("character", "two")}, complete_families=scope.complete_families)
    world.get(_id("event", "focus")).frontmatter["effects"].append({"target": target, "key": "holder", "operation": "set", "value": {"entity": _id("character", "two")}})
    assert _run(world, entity_scope, [_check("state.absent", target="object", key="holder")]).results[0]["outcome"] == "unknown"
    world.get(_id("event", "focus")).frontmatter["effects"].append({"target": target, "key": "holder", "operation": "clear"})
    assert _run(world, entity_scope, [_check("state.absent", target="object", key="holder")]).results[0]["outcome"] == "pass"


def test_closed_grammar_finite_values_and_strict_signed_time_coordinates():
    world, scope = _fixture()
    valid = _check("state.equals", target="object", key="condition", value="open")
    malformed = [[], {"id": "x", "predicate": {"kind": "expression", "value": "eval"}},
                 {**valid, "extra": True}, {"id": "x", "predicate": {**valid["predicate"], "path": "condition"}},
                 _check("state.equals", target="object", key="condition", value=float("nan")),
                 _check("knowledge.state", knowledge="belief", state="truth"),
                 _check("story-point.state", storyPoint="plot", state="eligible"),
                 _check("relationship.matches", relationship="relationship", values={}),
                 _check("relationship.matches", relationship="relationship", values={"metrics": {"trust": True}})]
    for check in malformed:
        _failure("invalid", lambda: _run(world, scope, [check]))
    _failure("invalid", lambda: _run(world, scope, [valid, valid]))
    for tick, order in ((TICK_MIN, ORDER_MIN), (TICK_MAX, ORDER_MAX)):
        point = check_time({"timeline": "main", "tick": str(tick), "order": str(order)})
        extreme, grant = _world(point)
        assert evaluate_expectations(extreme, grant, "focus", point, [valid]).apply_allowed
        assert time_value(point) == {"timeline": "main", "tick": str(tick), "order": str(order)}
    for value in (True, 10, "-0", "+1", "01", str(TICK_MAX + 1)):
        _failure("invalid", lambda: check_time({"timeline": "main", "tick": value, "order": "0"}))
    _failure("invalid", lambda: _run(world, scope, [valid], policy="unknown"))
    _failure("invalid", lambda: evaluate_expectations(world, scope, "focus", {"timeline": "main", "tick": 10, "order": 0}, [valid]))
    failure = _failure("invalid", lambda: _run(world, scope, [valid], source_valid=False))
    assert failure.code == "CONSEQUENCE-SOURCE-001"


def test_candidate_comparison_provenance_and_world_cache_purity():
    world, scope = _fixture()
    base = deepcopy(world)
    target = world.get(_id("event", "focus"))
    target.frontmatter["effects"][-1]["value"] = "candidate"
    world._cache["sentinel"] = {"unchanged": True}
    before = deepcopy((world.records, world._cache, base.records, base._cache))
    checks = [_check("state.equals", target="object", key="condition", value="candidate")]
    identity = CandidateIdentity(REVISION, "c" * 64)
    evaluation = _run(world, scope, checks, candidate=identity, base=base)
    result = evaluation.results[0]
    assert result["comparison"]["world"] == {"kind": "candidate", "baseRevision": REVISION, "requestHash": "c" * 64}
    assert result["citations"][0]["provenance"] == result["comparison"]["world"]
    assert (world.records, world._cache, base.records, base._cache) == before
    assert _run(world, scope, checks, candidate=identity, base=base) == evaluation


def test_complete_item_and_byte_limits_never_return_partial_checks():
    world, scope = _fixture()
    checks = [_check("state.equals", str(index), target="object", key="condition", value="open") for index in range(100)]
    assert len(_run(world, scope, checks).results) == 100
    _failure("limit", lambda: _run(world, scope, checks + [_check("state.absent", "extra", target="object", key="absent")]))
    _failure("limit", lambda: _run(world, scope, checks, limit=99))
    _failure("limit", lambda: _run(world, scope, [_check("state.equals", target="object", key="condition", value="x" * 262144)]))
    _failure("invalid", lambda: _run(world, scope, checks, limit=True))
