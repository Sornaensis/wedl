from copy import deepcopy

import jsonschema
import pytest

from wedl import api_schemas, changeset, consequence_operations as ops
from wedl.consequence_expectations import _validate, evaluate_expectations
from wedl.consequence_schemas import components, definitions, schema_context
from wedl.errors import UsageError
from wedl.event_consequences import AuthorScope, CandidateIdentity, event_report, semantic_delta
from wedl.model import ORDER_MAX, ORDER_MIN, TICK_MAX, TICK_MIN, StoryTime

from test_consequence_expectations import _check, _fixture
from test_event_consequences import REVISION, _id


def _validator(name):
    defs = definitions(); defs.pop("operationNames")
    return jsonschema.Draft202012Validator({"$defs": defs, "$ref": "#/$defs/" + name})


def _accept(name, value):
    _validator(name).validate(value)


def _reject(name, value):
    assert not _validator(name).is_valid(value)


def _append(kind="knowledge"):
    target = {"knowledge": "knowledge", "relationship": "relationship", "story-point": "storyPoint"}[kind]
    value = {"type": kind + ".transition.append", target: _id(kind, "belief" if kind == "knowledge" else "relationship" if kind == "relationship" else "plot"),
             "transition": {"time": {"timeline": "main", "tick": "30", "order": "0"}, "causing_event": _id("event", "focus")}}
    if kind != "relationship": value["transition"]["state"] = "remembered" if kind == "knowledge" else "resolved"
    return value


def test_registry_meta_refs_dispatch_and_openapi_adapters_share_one_definition():
    defs = definitions(); names = defs.pop("operationNames")
    jsonschema.Draft202012Validator.check_schema({"$defs": defs})
    assert set(names) == set(changeset.CHANGESET_OPERATION_TYPES)
    public = changeset.schema()
    assert public["$defs"] == defs and {item["type"] for item in public["operations"]} == set(names)
    http = api_schemas.components()["schemas"]
    assert all(http[name] == value for name, value in components().items())
    for value in (defs, http):
        def visit(node):
            if isinstance(node, dict):
                if "$ref" in node:
                    path = node["$ref"]
                    target = {"$defs": defs, "components": {"schemas": http}}
                    for part in path[2:].split("/"):
                        target = target[part.replace("~1", "/").replace("~0", "~")]
                for item in node.values(): visit(item)
            elif isinstance(node, list):
                for item in node: visit(item)
        visit(value)
    jsonschema.Draft202012Validator({"components": {"schemas": http}, "$ref": "#/components/schemas/ChangesetSchemaDocument"}).validate(public)
    public["$defs"]["Time"]["properties"].clear()
    assert changeset.schema()["$defs"] == defs
    assert api_schemas._STRING == {"type": "string"}


def test_closed_appends_match_normalizer_missing_null_coercion_and_auxiliary_ids():
    for kind, name in (("knowledge", "KnowledgeAppend"), ("relationship", "RelationshipAppend"), ("story-point", "PlotAppend")):
        valid = _append(kind)
        for identifier in (None, "tmp:transition", "tmp:one\ntwo", _id(kind + "-transition", "new")):
            value = deepcopy(valid)
            if identifier is not None: value["transition"]["id"] = identifier
            _accept(name, value)
            normalized = ops.normalize({"operations": [value]})
            assert normalized["operations"][0]["transition"]["time"]["tick"] == 30
            changeset._allocate({"operations": [value]})
        for mutate in (lambda v: v["transition"].pop("causing_event"),
                       lambda v: v["transition"].update(causing_event=None),
                       lambda v: v["transition"]["time"].update(tick=30),
                       lambda v: v["transition"]["time"].update(order="-0"),
                       lambda v: v["transition"].update(extra=True),
                       lambda v: v.update(extra=True)):
            value = deepcopy(valid); mutate(value)
            _reject(name, value)
            with pytest.raises(UsageError): ops.normalize({"operations": [value]})
        for identifier in (None, "tmp:", "tmp: \n\t", _id("effect", "wrong")):
            value = deepcopy(valid); value["transition"]["id"] = identifier
            _reject(name, value)
            with pytest.raises(UsageError): changeset._allocate({"operations": [value]})


def test_raw_event_classifier_is_disjoint_and_preserves_legacy_open_shapes():
    common = {"type": "event.create", "temporaryId": "tmp:legacy", "title": "Raw event", "time": {"tick": 10}, "ignoredLegacy": True}
    legacy = {**common, "effects": [{"id": _id("effect", "existing"), "target": _id("object", "object"), "key": "condition", "operation": "clear", "value": "legacy warning", "x-extra": "literal"}]}
    _accept("RawLegacyEvent", legacy); _reject("RawAllocationEvent", legacy)
    assert not ops.is_typed_event(legacy) and ops.normalize({"operations": [legacy]})["operations"][0] == legacy
    _accept("RawEventCreate", {**common, "effects": []})
    allocated = {**common, "effects": [{"target": _id("object", "object"), "key": "condition", "operation": "set", "value": "open"}]}
    for identifier in (None, "tmp:effect", "tmp:one\ntwo"):
        value = deepcopy(allocated)
        if identifier: value["effects"][0]["id"] = identifier
        assert ops.is_typed_event(value)
        _accept("RawAllocationEvent", value); _reject("RawLegacyEvent", value)
        _accept("RawEventCreate", value); ops.normalize({"operations": [value]})
    allocated["effects"][0]["extra"] = True
    _reject("RawEventCreate", allocated)
    with pytest.raises(UsageError): ops.normalize({"operations": [allocated]})


def test_create_histories_are_open_distinct_from_closed_appends_and_source_variants():
    world, _ = _fixture()
    for kind, name in (("knowledge", "KnowledgeCreate"), ("relationship", "RelationshipCreate")):
        fields = {"schema": world.schema, "kind": kind, "title": "New record", "domain": "story", "status": "canonical", "tags": [], "aliases": [], "transitions": []}
        fields.update({"knower": _id("character", "one"), "claim": {"opaque": True}} if kind == "knowledge" else
                      {"from": _id("character", "one"), "to": _id("character", "two"), "relationship_kind": "trust"})
        transition = {"time": StoryTime("main", 0).to_dict(), "causing_event": None, "x-legacy": {"literal": "tmp:label"}}
        if kind == "knowledge": transition["state"] = "suspected"
        fields["transitions"] = [transition]
        fields["x-open"] = {"entity": "literal"}
        operation = {"type": kind + ".create", "temporaryId": "tmp:new", "value": {"frontmatter": fields, "bodyMarkdown": "literal"}}
        _accept(name, operation); ops.normalize({"operations": [operation]})
        ids = changeset._allocate({"operations": [operation]})
        assert "tmp:new" in ids
        bad = deepcopy(operation); bad["value"]["frontmatter"]["schema"] = "wedl/v0.4"
        _reject(name, bad)
        bad = deepcopy(operation); bad["value"]["bodyMarkdown"] = None
        _reject(name, bad)
        with pytest.raises(UsageError): ops.normalize({"operations": [bad]})
        bad = deepcopy(operation); bad["value"]["frontmatter"]["transitions"][0]["id"] = _id("effect", "wrong")
        _reject(name, bad)
        with pytest.raises(UsageError): changeset._allocate({"operations": [bad]})
    assertion = {"kind": "parentage", "payload": {"child_id": _id("character", "one"), "parent_id": _id("character", "two"), "basis": "biological"},
                 "valid": {"from": StoryTime("main", 0).to_dict()}, "evidence": [{"kind": "knowledge", "entity_id": _id("knowledge", "belief"), "transition_id": _id("knowledge-transition", "accept")}]}
    _accept("KnowledgeClaim", {"genealogy": assertion})
    _reject("KnowledgeClaim", {"genealogy": {**assertion, "kind": "inferred"}})
    _reject("KnowledgeClaim", {"genealogy": assertion, "arbitrary": True})
    source = {"schema": "wedl/v0.7", "kind": "knowledge", "title": "Genealogy belief", "domain": "story", "status": "canonical", "tags": [], "aliases": [],
              "knower": _id("character", "one"), "claim": {"genealogy": assertion}, "transitions": []}
    _accept("KnowledgeCreateSource", source)
    _reject("KnowledgeCreateSource", {**source, "schema": "wedl/v0.3"})


def test_batch_intent_generic_op_openness_links_and_wire_time_match_runtime():
    from wedl.consequence_intents import validate, _event
    world, _ = _fixture()
    event = {"type": "event.create", "temporaryId": "tmp:new", "title": "New event", "time": {"timeline": "main", "tick": "10", "order": "0"}, "effects": []}
    intent = {"action": "consequence.batch", "expectedHead": REVISION, "idempotencyKey": "key", "operations": [event]}
    _accept("BatchIntent", intent); validate(intent); _event(deepcopy(event), world)
    http = api_schemas.components()["schemas"]
    jsonschema.Draft202012Validator({"components": {"schemas": http}, "$ref": "#/components/schemas/AuthoringRequest"}).validate(intent)
    for bad in ({**event, "extra": True}, {**event, "relatedStoryPoints": [], "related_story_points": []}, {**event, "time": {"tick": 10}}):
        _reject("BatchEventCreate", bad)
        with pytest.raises(UsageError): _event(deepcopy(bad), world)
    for kind in ("entity.update", "entity.delete"):
        _accept("Operation", {"type": kind, "entity": "map:regional/world", "x-open": {"entity": "literal"}})
    link = {"type": "outcome.link", "event": "focus", "storyPoints": ["plot"], "scenes": []}
    _accept("Operation", link); ops.normalize({"operations": [link]})
    bad = {**link, "storyPoints": []}; _reject("Operation", bad)
    with pytest.raises(UsageError): ops.normalize({"operations": [bad]})


def test_predicate_and_result_conformance_uses_real_evaluator_actuals():
    world, scope = _fixture()
    checks = [_check("state.equals", "state", target="object", key="condition", value="open"),
              _check("state.absent", "absent", target="object", key="absent"),
              _check("knowledge.state", "knowledge", knowledge="belief", state="accepted"),
              _check("relationship.matches", "relationship", relationship="relationship", values={"metrics": {"trust": 0.8}}),
              _check("story-point.state", "plot", storyPoint="plot", state="dormant"),
              _check("outcome.linked", "outcome", event="focus", target="scene"),
              _check("transition.caused", "transition", record="belief", transitionId=_id("knowledge-transition", "accept"), event="focus"),
              _check("state.absent", "unknown", target="missing", key="condition"),
              _check("state.absent", "unsupported", target="object", key="unknown")]
    for value in checks: _accept("Check", value)
    _validate(checks, "required")
    for value in evaluate_expectations(world, scope, "focus", StoryTime("main", 10), checks).results:
        _accept("CheckResult", value)
        bad = {**value, "code": "EXPECTATION-FAIL" if value["outcome"] == "pass" else "EXPECTATION-PASS"}
        _reject("CheckResult", bad)
        if value["comparison"]["predicate"]["kind"] == "state.equals":
            _reject("CheckResult", {**value, "actual": {"kind": "story-point", "state": "dormant"}})
    invalid = {"kind": "relationship.matches", "relationship": "relationship", "values": {"metrics": {"trust": True}}}
    _reject("Predicate", invalid)
    with pytest.raises(Exception) as failure: _validate([{"id": "x", "predicate": invalid}], "required")
    assert failure.value.outcome == "invalid"


def test_real_report_delta_check_only_and_failure_components_are_closed():
    from wedl.validation import _validate_state_semantics
    world, scope = _fixture()
    legacy = world.get(_id("event", "focus")).frontmatter["effects"][-1]
    legacy.update(operation="clear", value="legacy warning", **{"x-extra": {"authored": True}})
    diagnostics = _validate_state_semantics(world)
    assert any(value["code"] == "WDL-STATE-018" and value["severity"] == "warning" for value in diagnostics)
    assert not any(value["severity"] == "error" for value in diagnostics)
    report = event_report(world, scope, "focus", StoryTime("main", 10))
    assert report["outcome"] == "ok" and report["effects"][-1]["effect"] == legacy
    _accept("EventReport", report)
    http = api_schemas.components()["schemas"]
    jsonschema.Draft202012Validator({"components": {"schemas": http}, "$ref": "#/components/schemas/ConsequenceEventReport"}).validate(report)
    _reject("EffectInput", legacy)
    _reject("EffectInput", {key: value for key, value in legacy.items() if key != "x-extra"})
    _reject("EffectInput", {key: value for key, value in legacy.items() if key != "value"})
    candidate = deepcopy(world)
    candidate.get(_id("event", "focus")).frontmatter["effects"][-1]["value"] = "changed"
    delta = semantic_delta(world, candidate, scope, StoryTime("main", 10), CandidateIdentity(REVISION, "c" * 64), focus_events=["focus"], operation_targets={_id("event", "focus"): [0]})
    _accept("SemanticDelta", delta)
    failure = {"protocol": "wedl-event-consequences/v1", "outcome": "limit", "code": "CONSEQUENCE-LIMIT-001", "message": "Consequence limit exceeded."}
    _accept("EventReport", failure); _reject("EventReport", {**failure, "effects": []})
    group = {"operationIndex": 0, "event": report["event"], "at": report["at"], "policy": "required", "results": []}
    checks = {"outcome": "ok", "baseRevision": REVISION, "candidate": {"baseRevision": REVISION, "requestHash": "c" * 64}, "groups": [group], "applyAllowed": True}
    checked = {"protocol": "wedl-command-result/v1", "status": "checked", "previousHead": REVISION, "newHead": REVISION,
               "generatedIds": {}, "touchedEntityIds": [], "compile": {"status": "not-required", "revision": REVISION}, "idempotentReplay": True, "expectationChecks": checks}
    _accept("CheckOnlyApplyResult", checked); _reject("CheckOnlyApplyResult", {key: value for key, value in checked.items() if key != "expectationChecks"})


def test_context_exact_revision_sanitized_hints_metrics_and_evaluator_parity():
    world, scope = _fixture()
    rule = {"type": "object", "properties": {"owner": {"type": "entity", "entity_kind": "character", "x-private": "opaque"},
                                             "values": {"type": "array", "items": {"type": "boolean"}}},
            "nullable": True, "allowedValues": ["opaque"], "itemDefinition": {"opaque": True}, "exclusive_group": "custody"}
    world.config["state_keys"]["object"]["typed"] = rule
    world.config["relationship_metrics"]["trust"] = {"minimum": -1, "maximum": 1, "default": 0, "opaque": True}
    before = deepcopy((world.records, world._cache))
    context = schema_context(world, scope)
    _accept("SchemaContext", context)
    assert context["revision"] == REVISION and context["sourceSchema"] == world.schema and context["capabilities"] == []
    value = next(item for item in context["stateKeys"] if item["key"] == "typed")
    assert value == {"entityKind": "object", "key": "typed", "valueType": "object", "entityKindConstraint": None, "nullable": False,
                     "itemDefinition": None, "objectDefinition": {"owner": {"type": "entity", "entity_kind": "character"}, "values": {"type": "array", "items": {"type": "boolean"}}},
                     "exclusiveGroup": "custody", "allowedValues": None}
    assert next(item for item in context["stateKeys"] if item["key"] == "null")["nullable"] is True
    assert next(item for item in context["relationshipMetrics"] if item["name"] == "trust") == {"name": "trust", "minimum": -1, "maximum": 1}
    world.get(_id("object", "object")).frontmatter["initial_state"]["typed"] = {"owner": {"entity": _id("character", "one")}, "values": [True]}
    assert evaluate_expectations(world, scope, "focus", StoryTime("main", 10), [_check("state.equals", target="object", key="typed", value={"owner": {"entity": "one"}, "values": [True]})]).apply_allowed
    assert changeset.schema(world=world, scope=scope)["context"] == context
    world.get(_id("object", "object")).frontmatter["initial_state"].pop("typed")
    assert (world.records, world._cache) == before


def test_context_refuses_denied_unrepresentable_and_unsupported_capabilities_without_leaks():
    world, scope = _fixture()
    denied = AuthorScope(scope.world_id, scope.record_ids - {scope.world_id})
    with pytest.raises(UsageError, match="unavailable"): schema_context(world, denied)
    restricted = AuthorScope(scope.world_id, scope.record_ids, {scope.world_id: frozenset({"frontmatter.title"})})
    with pytest.raises(UsageError, match="unavailable"): schema_context(world, restricted)
    world.config["state_keys"]["object"]["condition"]["type"] = "integer"
    with pytest.raises(UsageError, match="unrepresentable"): schema_context(world, scope)
    world.config["state_keys"]["object"]["condition"]["type"] = "string"
    world.config["schema"] = "wedl/v0.7"; world.config["capabilities"] = ["not-installed"]
    with pytest.raises(UsageError, match="capabilities"): schema_context(world, scope)
    world.config["capabilities"] = ["generational-core-v1"]
    context = schema_context(world, scope)
    assert context["capabilities"] == ["generational-core-v1"]
    _accept("SchemaContext", context)
    _reject("SchemaContext", {**context, "capabilities": ["not-installed"]})
    world.revision = "HEAD"
    with pytest.raises(UsageError, match="exact revision"): schema_context(world, scope)
    with pytest.raises(UsageError): changeset.schema(world=world)


def test_signed_wire_time_bounds_reject_source_coercion_and_source_formats_remain_separate():
    from wedl.consequence_expectations import check_time
    for tick, order in ((TICK_MIN, ORDER_MIN), (TICK_MAX, ORDER_MAX)):
        value = {"timeline": "main", "tick": str(tick), "order": str(order)}
        _accept("Time", value); assert check_time(value) == StoryTime("main", tick, order)
        _accept("SourceTime", {"timeline": "main", "tick": tick, "order": order})
    for tick in (True, 10, "-0", "+1", "01", str(TICK_MAX + 1)):
        _reject("Time", {"timeline": "main", "tick": tick, "order": "0"})
        with pytest.raises(Exception): check_time({"timeline": "main", "tick": tick, "order": "0"})
    _reject("SourceTime", {"timeline": "main", "tick": "10", "order": "0"})
