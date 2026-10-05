from copy import deepcopy
import io
import json

from fastapi.testclient import TestClient
import jsonschema
import pytest

from wedl import authoring, changeset, cli, event_consequences
from wedl.api_contract import discovery_components, route_contracts
from wedl.api_schemas import consequence_rescue_example, operation_example
from wedl.event_consequences import event_report
from wedl.model import TICK_MAX, TICK_MIN, StoryTime
from wedl.server import create_app
from test_consequence_preview import ReadRepository, _forbid_writes, _snapshot, _world


def _validate(name, value):
    validator = jsonschema.Draft202012Validator({"components": discovery_components(), "$ref": "#/components/schemas/" + name})
    validator.validate(value)


def _assert_rescue(value, intent):
    _validate("AuthoringPreviewResponse", value)
    plan = value["preview"]; delta = plan["semanticDelta"]; ids = plan["generatedIds"]
    assert plan["valid"] and delta["outcome"] == "ok", plan["diagnostics"]
    assert value["protocol"] == "wedl-author-preview/v1" and plan["protocol"] == "wedl-preview/v1"
    assert delta["candidate"] == {"baseRevision": intent["expectedHead"], "requestHash": plan["requestHash"]}
    assert delta["baseRevision"] == intent["expectedHead"] and "revision" not in delta["candidate"]
    assert delta["at"] == intent["consequenceRequest"]["at"]
    group = next(item for item in delta["eventGroups"] if item["event"]["id"] == ids["tmp:rescue"])
    assert group["eventTime"] == {"timeline": "main", "tick": "300", "order": "0"}
    assert {item["subject"]["kind"] for item in group["changes"]} == {"state", "knowledge", "relationship", "story-point", "outcome"}
    assert {item["subject"]["targetKind"] for item in group["changes"] if item["subject"]["kind"] == "outcome"} == {"story-point", "scene"}
    assert any(item["after"]["payload"]["storedState"] == "resolved" for item in group["changes"] if item["subject"]["kind"] == "story-point")
    prose = [section for item in delta["unattributedRecordChanges"] for section in item["sections"] if section["section"] == "bodyMarkdown"]
    assert any(section["after"]["value"] == "An unrelated blue-coat description.\n" for section in prose)
    assert not any(section["after"].get("value") == "An unrelated blue-coat description.\n" for item in group["recordChanges"] for section in item["sections"])
    checks = plan["expectationChecks"]
    assert checks["applyAllowed"] and all(item["outcome"] == "pass" for item in checks["groups"][0]["results"])
    assert delta["expectations"] == [item for check_group in checks["groups"] for item in check_group["results"]]
    assert delta["applyAllowed"] == checks["applyAllowed"]
    assert set(ids) >= {"tmp:ledger", "tmp:rescue", "tmp:custody", "tmp:belief", "tmp:learned", "tmp:trust", "tmp:trust-raised", "tmp:plot", "tmp:resolved", "tmp:scene"}
    def visit(node):
        if isinstance(node, dict):
            if "provenance" in node:
                provenance = node["provenance"]
                if provenance["kind"] == "source": assert provenance["revision"] == intent["expectedHead"]
                else: assert provenance == {"kind": "candidate", **delta["candidate"]}
            for item in node.values(): visit(item)
        elif isinstance(node, list):
            for item in node: visit(item)
    visit(delta)
    assert plan["files"] and plan["diff"] and plan["confirmationToken"].startswith("wedl-confirmation/v1:")
    assert "_changes" not in plan
    return group


def test_rescue_discovery_example_and_closed_preview_failure_serialization():
    contracts = {item.command: item for item in route_contracts()}
    for action in ("preview", "apply"):
        request = next(item["value"] for item in contracts["author", "request", action].discovery.examples if item["value"]["body"]["action"] == "consequence.batch")
        assert request["body"] == consequence_rescue_example()
        _validate("AuthoringRequest", request["body"])
        assert request["headers"]["X-Wedl-Token"] and ("X-Wedl-Confirmation" in request["headers"]) == (action == "apply")
    for operation, name in (("changeset preview", "ChangesetPreviewResponse"), ("author request preview", "AuthoringPreviewResponse")):
        examples = operation_example(operation)
        for example in examples:
            value = json.loads(json.dumps(example["value"]))
            _validate(name, value)
            plan = value["preview"] if "preview" in value else value
            if "semanticDelta" in plan:
                assert set(plan["semanticDelta"]) == {"protocol", "outcome", "code", "message"}
                bad = deepcopy(value); nested = bad["preview"] if "preview" in bad else bad
                nested["semanticDelta"]["eventGroups"] = []
                with pytest.raises(jsonschema.ValidationError): _validate(name, bad)


def test_actual_rescue_generated_provenance_same_h_delta_and_event_local_t(monkeypatch):
    world = _world(); intent = consequence_rescue_example(world.revision, world.schema)
    intent["operations"].append({"type": "expectation.check", "event": "tmp:rescue",
        "at": {"timeline": "main", "tick": "301", "order": "0"}, "policy": "advisory", "items": [
            {"id": "advisory-belief", "predicate": {"kind": "knowledge.state", "knowledge": "tmp:belief", "state": "rejected"}},
            {"id": "advisory-plot", "predicate": {"kind": "story-point.state", "storyPoint": "tmp:plot", "state": "resolved"}}]})
    captured = {}; projector = event_consequences.semantic_delta
    def observed(base, candidate, scope, at, identity, **options):
        captured.update(candidate=candidate, scope=scope, identity=identity)
        return projector(base, candidate, scope, at, identity, **options)
    monkeypatch.setattr(event_consequences, "semantic_delta", observed)
    _forbid_writes(monkeypatch)
    before = deepcopy((world.records, world._cache, intent))
    value = authoring.preview_intent(ReadRepository(world), intent)
    _assert_rescue(value, intent)
    checks = value["preview"]["expectationChecks"]["groups"]
    assert [(group["operationIndex"], group["policy"], group["at"]["tick"]) for group in checks] == [(11, "required", "302"), (12, "advisory", "301")]
    assert [item["id"] for item in value["preview"]["semanticDelta"]["expectations"]] == ["custody", "belief", "trust", "plot", "scene", "advisory-belief", "advisory-plot"]
    assert [item["outcome"] for item in checks[-1]["results"]] == ["fail", "pass"]
    raw = changeset.preview(ReadRepository(world), value["changeset"])
    raw.pop("_changes", None)
    _validate("ChangesetPreviewResponse", raw)
    for field in ("requestHash", "generatedIds", "semanticDelta", "expectationChecks", "files", "diff"):
        assert raw[field] == value["preview"][field]
    assert raw["confirmationToken"] != value["preview"]["confirmationToken"]  # Full original intent additionally bound.
    local = event_report(captured["candidate"], captured["scope"], value["preview"]["generatedIds"]["tmp:rescue"], StoryTime("main", 302), candidate=captured["identity"])
    _validate("ConsequenceEventReport", local)
    assert local["outcome"] == "ok" and local["eventTime"]["tick"] == "300" and local["at"]["tick"] == "302"
    assert not any(item["subject"]["kind"] == "story-point" and item["after"]["payload"]["storedState"] == "resolved" for item in local["changes"])
    assert any(item["time"]["tick"] == "301" and not item["atEventTime"] for item in local["causedTransitions"])
    assert (world.records, world._cache, intent) == before


def test_actual_preview_closed_semantic_outcomes_and_signed_horizons(monkeypatch):
    world = _world(); _forbid_writes(monkeypatch)
    for mutate, outcome in ((lambda value: value["consequenceRequest"].update(limit=1), "limit"),
                            (lambda value: value["consequenceRequest"]["at"].update(tick=302), "invalid"),
                            (lambda value: value["operations"][4]["value"]["frontmatter"].update(knower="missing"), "unavailable")):
        intent = consequence_rescue_example(world.revision, world.schema)
        # Invalid source is tested at the raw preview boundary, retaining source diagnostics.
        if outcome == "unavailable":
            raw = authoring.compile_intent(ReadRepository(world), intent); raw["operations"][10]["frontmatterPatch"] = {"initial_state": {"condition": 32}}
            value = changeset.preview(ReadRepository(world), raw); value.pop("_changes", None)
        elif outcome == "invalid":
            raw = authoring.compile_intent(ReadRepository(world), intent); mutate(raw)
            value = changeset.preview(ReadRepository(world), raw); value.pop("_changes", None)
        else:
            mutate(intent); value = authoring.preview_intent(ReadRepository(world), intent)["preview"]
        _validate("ChangesetPreviewResponse", value)
        assert value["semanticDelta"]["outcome"] == outcome
        assert set(value["semanticDelta"]) == {"protocol", "outcome", "code", "message"}
        if outcome == "unavailable": assert not value["valid"] and value["diagnostics"]
    for tick in (TICK_MIN, TICK_MAX):
        intent = consequence_rescue_example(world.revision, world.schema)
        raw = authoring.compile_intent(ReadRepository(world), intent)
        raw["consequenceRequest"]["at"]["tick"] = str(tick)
        value = changeset.preview(ReadRepository(world), raw); value.pop("_changes", None)
        _validate("ChangesetPreviewResponse", value)
        if value["semanticDelta"]["outcome"] == "ok": assert value["semanticDelta"]["at"]["tick"] == str(tick)


def test_rescue_real_direct_cli_http_schema_and_confirmation_parity(ash_repo, monkeypatch, capsys):
    repository = ash_repo; world = repository.load_world(repository.head(), cache_write=False)
    intent = consequence_rescue_example(world.revision, world.schema)
    client = TestClient(create_app(repository.root)); token = client.get("/api/session").json()["token"]
    before = _snapshot(repository.root)
    direct = authoring.preview_intent(repository, intent); _assert_rescue(direct, intent)
    monkeypatch.setattr("sys.stdin", io.StringIO(json.dumps(intent)))
    assert cli.main(["--compact", "author", "request", "preview", "-", "--repo", str(repository.root)]) == 0
    assert json.loads(capsys.readouterr().out) == direct
    headers = {"X-Wedl-Token": token}
    response = client.post("/api/authoring/preview", json=intent, headers=headers)
    assert response.status_code == 200 and response.json() == direct
    raw = direct["changeset"]; plan = changeset.preview(repository, raw); plan.pop("_changes", None)
    response = client.post("/api/changesets/preview", json=raw, headers=headers)
    assert response.status_code == 200 and response.json() == plan
    monkeypatch.setattr("sys.stdin", io.StringIO(json.dumps(raw)))
    assert cli.main(["--compact", "changeset", "preview", "-", "--repo", str(repository.root)]) == 0
    assert json.loads(capsys.readouterr().out) == plan
    for name, value in (("AuthoringPreviewResponse", direct), ("ChangesetPreviewResponse", plan)): _validate(name, value)
    for path, body in (("/api/authoring/preview", intent), ("/api/changesets/preview", raw)):
        assert client.post(path, json=body).status_code == 401
    proof = direct["preview"]["confirmationToken"]
    for body, confirmation, expected in ((intent, None, "confirmation_required"),
                                          ({**intent, "consequenceRequest": {**intent["consequenceRequest"], "at": {"timeline": "main", "tick": "303", "order": "0"}}}, proof, "confirmation_mismatch"),
                                          (intent, plan["confirmationToken"], "confirmation_mismatch")):
        supplied = {**headers, **({"X-Wedl-Confirmation": confirmation} if confirmation else {})}
        refused = client.post("/api/authoring/apply", json=body, headers=supplied)
        assert refused.json()["code"] == expected
    assert _snapshot(repository.root) == before
