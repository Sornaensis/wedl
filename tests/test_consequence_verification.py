from copy import deepcopy
import io
import json

from fastapi.testclient import TestClient
import jsonschema
import pytest

from wedl import cli, consequence_verification as verification, event_consequences
from wedl.api_contract import AuthPolicy, Transport, discovery_components, route_contracts, validate_request_values
from wedl.api_router import openapi_contract_errors
from wedl.consequence_expectations import evaluate_expectations
from wedl.errors import UsageError
from wedl.ids import id_from_seed
from wedl.model import StoryTime
from wedl.repository import Repository
from wedl.server import create_app
from wedl.source import serialize_record
from wedl.util import canonical_json
from test_consequence_preview import _snapshot, _world


def _request(world, **patch):
    return {"protocol": verification.PROTOCOL, "revision": world.revision,
            "event": "The Archive Exchange", "at": {"timeline": "main", "tick": "300", "order": "0"},
            "limit": 1000, **patch}


def _expect(world, *, target=None, key="condition", value="impossible", policy="required", identifier="condition"):
    return {"policy": policy, "items": [{"id": identifier, "predicate": {"kind": "state.equals",
        "target": target or world.find("Brass Restricted-Vault Key").id, "key": key, "value": value}}]}


def _closed(report, outcome):
    assert report["outcome"] == outcome, report
    assert set(report) == {"protocol", "outcome", "code", "message"}


def _validate(name, value):
    jsonschema.validate(value, {"components": discovery_components(), "$ref": "#/components/schemas/" + name})


def test_source_report_flat_checks_same_world_scope_and_cache_purity(monkeypatch):
    world = _world(); payload = _request(world, expectations=_expect(world))
    original = deepcopy(payload); cache = deepcopy(world._cache); calls = []
    report_fn = event_consequences.event_report
    evaluation_fn = verification.evaluate_expectations
    def report(*args, **kwargs):
        calls.append(("report", args, kwargs)); return report_fn(*args, **kwargs)
    def evaluation(*args, **kwargs):
        calls.append(("check", args, kwargs)); return evaluation_fn(*args, **kwargs)
    monkeypatch.setattr(event_consequences, "event_report", report)
    monkeypatch.setattr(verification, "evaluate_expectations", evaluation)
    result = verification.verify_world(world, payload)
    assert result["outcome"] == "ok" and result["applyAllowed"] is False, result
    assert len(calls) == 2 and calls[0][1][:4] == calls[1][1][:4]
    assert calls[0][1][0] is calls[1][1][0] and calls[0][1][0] is not world
    assert calls[0][1][1].complete_families == frozenset({"state", "knowledge"})
    check = result["expectations"][0]
    assert check["outcome"] == "fail"
    assert check["comparison"]["world"] == {"kind": "source", "revision": world.revision}
    assert check["comparison"]["at"] == result["at"]
    assert "candidate" not in canonical_json(result) and "operationIndex" not in canonical_json(result)
    assert world._cache == cache and payload == original
    _validate("ConsequenceEventOk", result)


def test_required_advisory_unknown_unsupported_and_empty_checks_are_read_outcomes():
    world = _world()
    for expectation, outcome in ((_expect(world), "fail"),
            (_expect(world, target="Unwritten reference"), "unknown"),
            (_expect(world, key="undeclared"), "unsupported")):
        for policy in ("required", "advisory"):
            expectation["policy"] = policy
            report = verification.verify_world(world, _request(world, expectations=expectation))
            assert report["outcome"] == "ok", report
            assert report["expectations"][0]["outcome"] == outcome
            assert report["applyAllowed"] == (policy == "advisory")
    empty = verification.verify_world(world, _request(world, expectations={"policy": "required", "items": []}))
    assert empty["expectations"] == [] and empty["applyAllowed"]


def test_closed_request_before_repository_and_invalid_source_before_scope_or_folds(monkeypatch):
    world = _world(); payload = _request(world)
    def forbidden(*args, **kwargs): raise AssertionError("invalid input accessed source or folded")
    monkeypatch.setattr("wedl.repository.Repository", forbidden)
    for field in ("viewer", "character", "identity", "audience", "perspective", "mode", "allTime", "scope", "complete_families"):
        _closed(verification.execute(None, {**payload, field: "author"}), "invalid")
    for patch in ({"revision": "HEAD"}, {"revision": "A" * 40}, {"limit": True}, {"event": ""},
            {"expectations": None}, {"expectations": {"policy": "required", "items": [], "scope": "author"}},
            {"at": {"timeline": "main", "tick": 300, "order": "0"}}):
        _closed(verification.execute(None, {**payload, **patch}), "invalid")
    broken = deepcopy(world)
    broken.world_record.frontmatter["schema"] = "unsupported"
    monkeypatch.setattr(verification, "full_author_scope", forbidden)
    monkeypatch.setattr(event_consequences, "event_report", forbidden)
    monkeypatch.setattr(verification, "evaluate_expectations", forbidden)
    result = verification.verify_world(broken, payload)
    _closed(result, "invalid")
    assert result["code"] == "CONSEQUENCE-SOURCE-001"


def test_public_combined_item_exact_limit_and_actual_serialized_byte_limits():
    world = _world(); payload = _request(world); report = verification.verify_world(world, payload)
    assert report["outcome"] == "ok", report
    count = sum(len(report[key]) for key in verification._EVENT_COLLECTIONS)
    assert count > 0
    payload["limit"] = count
    assert verification.verify_world(world, payload)["outcome"] == "ok"
    payload["expectations"] = _expect(world)
    # Each layer fits independently; combined logical results do not.
    evaluate_expectations(world, verification.full_author_scope(world), payload["event"], StoryTime("main", 300),
                          payload["expectations"]["items"], limit=count)
    _closed(verification.verify_world(world, payload), "limit")
    payload["limit"] = count + 1
    assert verification.verify_world(world, payload)["outcome"] == "ok"
    payload["limit"] = 1000
    small = verification.verify_world(world, payload)
    current_bytes = len(canonical_json(small).encode("utf8"))
    large_id = "x" * (262144 - current_bytes + len("condition") + 1)
    payload["expectations"]["items"][0]["id"] = large_id
    # The evaluator's own envelope remains within its bound.
    evaluate_expectations(world, verification.full_author_scope(world), payload["event"], StoryTime("main", 300),
                          payload["expectations"]["items"], limit=1000)
    _closed(verification.verify_world(world, payload), "limit")
    empty = {**report, **{key: [] for key in verification._EVENT_COLLECTIONS}, "expectations": []}
    assert verification.admit_semantic(empty, checks=0, event=empty, limit=1) is empty
    with pytest.raises(event_consequences.ProjectionFailure):
        verification.admit_semantic(empty, checks=0, focus_events=range(101), limit=1000)
    focus = deepcopy(world.find("Caldrin’s Archive Authority Is Suspended"))
    focus.frontmatter.update(id=id_from_seed("event", "public-empty"), title="Empty public event", aliases=[],
                            effects=[], causes=[], related_story_points=[], time={"timeline": "main", "tick": 300, "order": 0})
    focus.source_path = "story/events/main/public-empty.md"
    world.records[focus.id] = focus; world._cache.clear()
    payload = _request(world, event=focus.id, limit=1)
    actual_empty = verification.verify_world(world, payload)
    assert actual_empty["outcome"] == "ok", actual_empty
    assert all(actual_empty[key] == [] for key in verification._EVENT_COLLECTIONS)
    payload["expectations"] = _expect(world)
    assert verification.verify_world(world, payload)["outcome"] == "ok"


def test_horizon_canonical_focus_and_eligible_name_resolution():
    world = _world()
    # The original Archive case has a known-false later event trigger at its
    # local T=120 view, rather than unavailable evidence.
    hearing = world.find("Conduct the Ledger Hearing")
    trigger = world.records[hearing.frontmatter["trigger"]["event"]["event"]]
    assert trigger.frontmatter["time"] == {"timeline": "main", "tick": 156, "order": 30}
    original = verification.verify_world(world, _request(world, event="The Archive Exchange"))
    assert original["outcome"] == "ok" and original["eventTime"] == {"timeline": "main", "tick": "120", "order": "10"}, original
    _closed(verification.verify_world(world, _request(world, event="Caldrin’s Archive Authority Is Suspended",
            at={"timeline": "main", "tick": "196", "order": "0"})), "unavailable")
    focus = deepcopy(world.find("Caldrin’s Archive Authority Is Suspended"))
    focus.frontmatter.update(id=id_from_seed("event", "public-draft"), title="Unadmitted draft", aliases=[],
                            effects=[], causes=[], related_story_points=[])
    focus.source_path = "story/events/main/public-draft.md"
    focus.frontmatter["status"] = "draft"
    world.records[focus.id] = focus
    world._cache.clear()
    _closed(verification.verify_world(world, _request(world, event="Unadmitted draft")), "unavailable")


def test_parser_discovery_closed_components_statuses_and_temporal_description():
    contracts = {item.command: item for item in route_contracts()}
    contract = contracts["consequences",]
    assert contract.binding.auth == AuthPolicy.SESSION
    assert contract.binding.path == "/api/events/consequences"
    assert next(arg for arg in contract.arguments if arg.dest == "file").transport == Transport.BODY
    _validate("ConsequenceEventRequest", contract.discovery.examples[0]["value"]["body"])
    _validate("ConsequenceEventOk", contract.discovery.success_examples[0]["value"])
    for field in ("scope", "viewer", "mode", "complete_families"):
        with pytest.raises(UsageError):
            validate_request_values("POST", contract.binding.path, {field: "author"})
        with pytest.raises(jsonschema.ValidationError):
            _validate("ConsequenceEventRequest", {**contract.discovery.examples[0]["value"]["body"], field: "author"})
    assert "same explicit H" in contracts["changeset", "preview"].discovery.description
    assert "before/after at T" in contracts["changeset", "preview"].discovery.description


def test_exact_git_direct_cli_http_purity_statuses_and_deferred_cache(ash_repo, monkeypatch, capsys):
    repository = ash_repo; revision = repository.head()
    world = repository.load_world(revision, cache_write=False)
    client = TestClient(create_app(repository.root)); token = client.get("/api/session").json()["token"]
    # A damaged or deferred compiled cache is irrelevant to this source read.
    cache_dir = repository.root / ".wedl" / "compiled"; cache_dir.mkdir(parents=True, exist_ok=True)
    (cache_dir / "deferred-sentinel").write_text("uncompiled")
    before = _snapshot(repository.root); memory = deepcopy(repository._world_cache) if hasattr(repository, "_world_cache") else None
    calls = []; load = Repository.load_world
    def observed(self, selected="HEAD", *, cache_write=True):
        calls.append((self, selected, cache_write)); return load(self, selected, cache_write=cache_write)
    monkeypatch.setattr(Repository, "load_world", observed)
    requests = [_request(world), _request(world, expectations=_expect(world)),
                _request(world, revision="f" * 40), _request(world, scope="author"), _request(world, limit=1)]
    outcomes = ["ok", "ok", "unavailable", "invalid", "limit"]
    for payload, outcome in zip(requests, outcomes):
        direct = verification.execute(repository, payload)
        assert direct["outcome"] == outcome, direct
        response = client.post("/api/events/consequences", json=payload, headers={"X-Wedl-Token": token})
        assert response.status_code == verification.status_code(direct) and response.json() == direct
        monkeypatch.setattr("sys.stdin", io.StringIO(json.dumps(payload)))
        assert cli.main(["--compact", "consequences", "-", "--repo", str(repository.root)]) == (0 if outcome == "ok" else 2)
        output = capsys.readouterr()
        assert json.loads(output.out if outcome == "ok" else output.err) == direct
        _validate("ConsequenceEventReport", direct)
    assert all(instance is not repository and selected != "HEAD" and write is False for instance, selected, write in calls)
    assert _snapshot(repository.root) == before
    if memory is not None: assert repository._world_cache == memory
    document = client.get("/openapi.json").json()
    assert not openapi_contract_errors(document)
    responses = document["paths"]["/api/events/consequences"]["post"]["responses"]
    assert set(responses) == {"200", "400", "401", "409", "422"}


def test_http_auth_precedes_source_and_exact_revision_survives_head_race(ash_repo, monkeypatch):
    repository = ash_repo; old = repository.head(); world = repository.load_world(old, cache_write=False)
    client = TestClient(create_app(repository.root)); token = client.get("/api/session").json()["token"]
    payload = _request(world); before = _snapshot(repository.root)
    def forbidden(*args, **kwargs): raise AssertionError("unauthorized read opened source")
    with monkeypatch.context() as guarded:
        guarded.setattr(Repository, "load_world", forbidden)
        guarded.setattr(verification, "full_author_scope", forbidden)
        for headers in ({}, {"X-Wedl-Token": "wrong"}):
            assert client.post("/api/events/consequences", json=payload, headers=headers).status_code == 401
        for field in ("viewer", "character", "audience", "perspective", "mode", "scope", "complete_families"):
            assert client.post("/api/events/consequences", json={**payload, field: "author"},
                               headers={"X-Wedl-Token": token}).status_code == 400
            assert client.post("/api/events/consequences", json=payload, params={field: "author"},
                               headers={"X-Wedl-Token": token}).status_code == 400
    assert _snapshot(repository.root) == before
    record = world.find("The Archive Exchange")
    frontmatter = deepcopy(record.frontmatter); frontmatter["title"] = "Changed after selected revision"
    load = Repository.load_world; loads = []
    def raced(self, selected="HEAD", *, cache_write=True):
        loads.append((selected, cache_write))
        selected_world = load(self, selected, cache_write=cache_write)
        if repository.head() == old:
            repository.commit_files(expected_head=old, files={record.source_path: serialize_record(frontmatter, record.body)}, message="test revision race")
        return selected_world
    monkeypatch.setattr(Repository, "load_world", raced)
    report = client.post("/api/events/consequences", json=payload, headers={"X-Wedl-Token": token}).json()
    assert report["outcome"] == "ok" and report["revision"] == old
    assert report["event"]["title"] == "The Archive Exchange" and repository.head() != old
    assert loads == [(old, False)]
    def citations(value):
        if isinstance(value, dict):
            if value.get("provenance", {}).get("kind") == "source": yield value
            for child in value.values(): yield from citations(child)
        elif isinstance(value, list):
            for child in value: yield from citations(child)
    assert list(citations(report))
    for citation in citations(report):
        assert citation["provenance"] == {"kind": "source", "revision": old,
            "blobOid": world.records[citation["recordId"]].blob_oid}


def _duplicate_documents(payload):
    raw = json.dumps(payload)
    escaped_event = chr(92) + "u0065vent"
    escaped_tick = "ti" + chr(92) + "u0063k"
    return [raw[:1] + '"event":"private-first-event",' + raw[1:],
            raw[:1] + '"' + escaped_event + '":"private-first-event",' + raw[1:],
            raw.replace('"tick": "300"', '"tick":"299","tick":"300"'),
            raw.replace('"tick": "300"', '"' + escaped_tick + '":"299","tick":"300"'),
            raw[:-1] + ',"expectations":{"policy":"required","items":[{"id":"nested","predicate":'
                '{"kind":"state.equals","target":"literal","key":"condition","value":'
                '{"private-secret":1,"private-secret":2}}}]}}']


def test_raw_duplicate_members_cli_file_stdin_and_decoder_fail_before_source(tmp_path, monkeypatch, capsys):
    payload = _request(_world())
    assert verification.decode_request(json.dumps(payload)) == payload
    def forbidden(*args, **kwargs): raise AssertionError("duplicate members accessed source or scope")
    monkeypatch.setattr(cli, "Repository", forbidden)
    monkeypatch.setattr(verification, "full_author_scope", forbidden)
    documents = _duplicate_documents(payload)
    for raw in documents:
        path = tmp_path / "duplicate-request.json"; path.write_text(raw, encoding="utf-8")
        # Unrelated legacy command decoding retains its established behavior.
        assert isinstance(cli._json_file(str(path)), dict)
    for raw in [*documents, '{"protocol":']:
        with pytest.raises(event_consequences.ProjectionFailure) as failed:
            verification.decode_request(raw)
        assert failed.value.outcome == "invalid"
        path = tmp_path / "duplicate-request.json"; path.write_text(raw, encoding="utf-8")
        for filename in (str(path), "-"):
            monkeypatch.setattr("sys.stdin", io.StringIO(raw))
            assert cli.main(["--compact", "consequences", filename, "--repo", str(tmp_path)]) == 2
            captured = capsys.readouterr()
            assert captured.out == ""
            assert json.loads(captured.err) == {"protocol": verification.PROTOCOL, "outcome": "invalid",
                "code": "CONSEQUENCE-REQUEST-001", "message": "Invalid consequence request."}
            assert "private" not in captured.err and "299" not in captured.err


def test_raw_duplicate_http_members_authentication_redaction_and_unique_control(ash_repo, monkeypatch):
    repository = ash_repo; world = repository.load_world(repository.head(), cache_write=False)
    client = TestClient(create_app(repository.root)); token = client.get("/api/session").json()["token"]
    payload = _request(world); before = _snapshot(repository.root)
    headers = {"Content-Type": "application/json", "X-Wedl-Token": token}
    expected = verification.execute(repository, payload)
    assert expected["outcome"] == "ok" and expected["eventTime"]["tick"] == "120"
    valid = client.post("/api/events/consequences", content=json.dumps(payload), headers=headers)
    assert valid.status_code == 200 and valid.json() == expected
    def forbidden(*args, **kwargs): raise AssertionError("duplicate HTTP members accessed source or scope")
    monkeypatch.setattr(Repository, "load_world", forbidden)
    monkeypatch.setattr(verification, "full_author_scope", forbidden)
    for raw in [*_duplicate_documents(payload), '{"protocol":']:
        denied = client.post("/api/events/consequences", content=raw, headers={"Content-Type": "application/json"})
        assert denied.status_code == 401
        response = client.post("/api/events/consequences", content=raw, headers=headers)
        assert response.status_code == 400
        assert response.json() == {"protocol": verification.PROTOCOL, "outcome": "invalid",
            "code": "CONSEQUENCE-REQUEST-001", "message": "Invalid consequence request."}
        assert "private" not in response.text and "299" not in response.text
    assert _snapshot(repository.root) == before
