from copy import deepcopy
import io
import json

from fastapi.testclient import TestClient
import jsonschema
import pytest

from wedl import authoring, changeset, cli
from wedl.api_contract import AuthPolicy, Transport, discovery_components, route_contracts, validate_request_values
from wedl.api_router import openapi_contract_errors
from wedl.command_parser import parser
from wedl.errors import UsageError
from wedl.event_consequences import AuthorScope
from wedl.repository import Repository
from wedl.server import create_app
from wedl.source import serialize_record
from test_consequence_preview import _snapshot, _world


def _validate(name, value):
    jsonschema.validate(value, {"components": discovery_components(), "$ref": "#/components/schemas/" + name})


def test_schema_static_cli_is_repository_free_and_context_grammar_is_exact(monkeypatch, capsys):
    def forbidden(*args, **kwargs): raise AssertionError("static discovery opened a repository")
    monkeypatch.setattr(cli, "Repository", forbidden)
    assert cli.main(["--compact", "changeset", "schema"]) == 0
    assert json.loads(capsys.readouterr().out) == changeset.schema()
    assert cli.main(["--compact", "changeset", "schema", "--revision", "a" * 40]) == 2
    assert json.loads(capsys.readouterr().err)["code"] == "usage_error"
    for revision in ("HEAD", "abc", "A" * 40, "a" * 41):
        with pytest.raises(UsageError): parser().parse_args(["changeset", "schema", "--repo", ".", "--revision", revision])
        with pytest.raises(UsageError): validate_request_values("GET", "/api/changesets/schema", {"revision": revision})
    for field in ("scope", "viewer", "identity", "mode", "audience", "complete_families"):
        with pytest.raises(UsageError): validate_request_values("GET", "/api/changesets/schema", {field: "author"})


def test_parser_discovery_components_and_batch_examples_share_closed_variants():
    contracts = {contract.command: contract for contract in route_contracts()}
    schema = contracts["changeset", "schema"]
    assert schema.binding.auth == AuthPolicy.CONTEXT_SESSION
    revision = next(item for item in schema.arguments if item.dest == "revision")
    assert revision.transport == Transport.QUERY and revision.transport_name == "revision"
    _validate("ChangesetSchemaDocument", changeset.schema())
    for action in ("preview", "apply"):
        contract = contracts["author", "request", action]
        example = next(item["value"] for item in contract.discovery.examples if item["value"]["body"]["action"] == "consequence.batch")
        _validate("AuthoringRequest", example["body"])
        _validate("ConsequenceBatchIntent", example["body"])
        with pytest.raises(jsonschema.ValidationError): _validate("ConsequenceBatchIntent", {**example["body"], "scope": "author"})
        assert example["headers"]["X-Wedl-Token"]
        assert ("X-Wedl-Confirmation" in example["headers"]) == (action == "apply")


def test_context_revision_and_source_refusal_preserve_loaded_cache(monkeypatch):
    world = _world(); cache = deepcopy(world._cache); calls = []
    class ContextRepository:
        def __init__(self, path): pass
        def head(self): return world.revision
        def load_world(self, revision, *, cache_write=True):
            calls.append((revision, cache_write)); return world
    monkeypatch.setattr(cli, "Repository", ContextRepository)
    args = parser().parse_args(["changeset", "schema", "--repo", "selected", "--revision", world.revision])
    result = cli.dispatch(args)
    world_record = next(record for record in world.records.values() if record.kind == "world")
    assert result == changeset.schema(world=world, scope=AuthorScope(world_record.id, frozenset(world.records)))
    assert calls == [(world.revision, False)] and world._cache == cache
    args.revision = "b" * 40
    with pytest.raises(UsageError, match="revision does not match"): cli.dispatch(args)
    args.revision = world.revision
    world_record.frontmatter["capabilities"].append("uninstalled-capability")
    from wedl.errors import ValidationFailed
    with pytest.raises(ValidationFailed): cli.dispatch(args)
    assert world._cache == cache


def test_exact_git_context_direct_cli_http_parity_and_old_revision_freshness(ash_repo, monkeypatch, capsys):
    repository = ash_repo; old = repository.head(); old_world = repository.load_world(old, cache_write=False)
    record = old_world.world_record; frontmatter = deepcopy(record.frontmatter)
    frontmatter.setdefault("state_keys", {}).setdefault("object", {})["discovery-declaration"] = {"type": "boolean"}
    repository.commit_files(expected_head=old, files={record.source_path: serialize_record(frontmatter, record.body)}, message="test schema context revision")
    new = repository.head()
    client = TestClient(create_app(repository.root)); token = client.get("/api/session").json()["token"]
    before = _snapshot(repository.root); calls = []; original = Repository.load_world
    def observed(self, revision="HEAD", *, cache_write=True):
        calls.append((revision, cache_write)); return original(self, revision, cache_write=cache_write)
    monkeypatch.setattr(Repository, "load_world", observed)
    assert client.get("/api/changesets/schema").json() == changeset.schema() and calls == []
    for revision in (old, new):
        calls.clear()
        response = client.get("/api/changesets/schema", params={"revision": revision}, headers={"X-Wedl-Token": token})
        assert response.status_code == 200, response.json()
        assert calls == [(revision, False)]
        calls.clear()
        assert cli.main(["--compact", "changeset", "schema", "--repo", str(repository.root), "--revision", revision]) == 0
        cli_value = json.loads(capsys.readouterr().out)
        assert calls == [(revision, False)] and cli_value == response.json()
        world = original(repository, revision, cache_write=False)
        direct = changeset.schema(world=world, scope=AuthorScope(world.world_record.id, frozenset(world.records)))
        assert direct == cli_value and direct["context"]["revision"] == revision
        assert any(item["key"] == "discovery-declaration" for item in direct["context"]["stateKeys"]) == (revision == new)
    assert _snapshot(repository.root) == before
    assert not openapi_contract_errors(client.get("/openapi.json").json())
    contextual = client.get("/openapi.json").json()["paths"]["/api/changesets/schema"]["get"]
    assert contextual["x-wedl-auth-policy"] == "session-if-context" and contextual["x-wedl-auth-context-query"] == "revision"


def test_http_context_auth_and_unknown_grants_refuse_before_load(ash_repo, monkeypatch):
    client = TestClient(create_app(ash_repo.root)); head = ash_repo.head()
    token = client.get("/api/session").json()["token"]; before = _snapshot(ash_repo.root)
    def forbidden(*args, **kwargs): raise AssertionError("refused schema request loaded source")
    monkeypatch.setattr(Repository, "load_world", forbidden)
    for headers in ({}, {"X-Wedl-Token": "wrong"}):
        response = client.get("/api/changesets/schema", params={"revision": head}, headers=headers)
        assert response.status_code == 401 and response.json()["code"] == "authentication_required"
    for fields in ({"revision": "HEAD"}, {"revision": head, "scope": "author"}, {"viewer": "author"}):
        response = client.get("/api/changesets/schema", params=fields, headers={"X-Wedl-Token": token})
        assert response.status_code == 400 and response.json()["code"] == "usage_error"
    assert client.get("/api/changesets/schema").status_code == 200
    assert _snapshot(ash_repo.root) == before


def test_batch_direct_cli_http_raw_body_confirmation_and_full_intent_binding(ash_repo, monkeypatch, capsys):
    repository = ash_repo; world = repository.load_world(repository.head(), cache_write=False)
    key = world.find("Brass Restricted-Vault Key")
    intent = {"action": "consequence.batch", "expectedHead": world.revision, "idempotencyKey": "transport-batch",
        "operations": [{"type": "event.create", "temporaryId": "tmp:opening", "title": "Transport opening",
            "time": {"timeline": "main", "tick": "300", "order": "0"},
            "effects": [{"target": key.title, "key": "condition", "operation": "set", "value": "open"}]},
            {"type": "expectation.check", "event": "tmp:opening", "at": {"timeline": "main", "tick": "300", "order": "0"},
                "policy": "required", "items": [{"id": "open", "predicate": {"kind": "state.equals", "target": key.title, "key": "condition", "value": "open"}}]}]}
    client = TestClient(create_app(repository.root)); token = client.get("/api/session").json()["token"]
    headers = {"X-Wedl-Token": token}; before = _snapshot(repository.root)
    direct = authoring.preview_intent(repository, intent)
    monkeypatch.setattr("sys.stdin", io.StringIO(json.dumps(intent)))
    assert cli.main(["--compact", "author", "request", "preview", "-", "--repo", str(repository.root)]) == 0
    assert json.loads(capsys.readouterr().out) == direct
    preview = client.post("/api/authoring/preview", json=intent, headers=headers)
    assert preview.status_code == 200 and preview.json() == direct and _snapshot(repository.root) == before
    confirmation = direct["preview"]["confirmationToken"]
    for body, supplied, code in ((intent, {}, "confirmation_required"),
                               ({**intent, "scope": "author"}, {"X-Wedl-Confirmation": confirmation}, "usage_error"),
                               ({**intent, "payload": intent}, {"X-Wedl-Confirmation": confirmation}, "usage_error")):
        response = client.post("/api/authoring/apply", json=body, headers={**headers, **supplied})
        assert response.json()["code"] == code and _snapshot(repository.root) == before
    for edit in ("target", "policy"):
        changed = deepcopy(intent)
        if edit == "target": changed["operations"][0]["effects"][0]["target"] = key.id
        else: changed["operations"][-1]["policy"] = "advisory"
        response = client.post("/api/authoring/apply", json=changed, headers={**headers, "X-Wedl-Confirmation": confirmation})
        assert response.json()["code"] == "confirmation_mismatch" and _snapshot(repository.root) == before
    monkeypatch.setattr("sys.stdin", io.StringIO(json.dumps(intent)))
    assert cli.main(["--compact", "author", "request", "apply", "-", "--repo", str(repository.root), "--yes"]) == 2
    assert json.loads(capsys.readouterr().err)["code"] == "usage_error" and _snapshot(repository.root) == before
    response = client.post("/api/authoring/apply", json=intent, headers={**headers, "X-Wedl-Confirmation": confirmation})
    assert response.status_code == 200 and response.json()["status"] == "committed"
    _validate("AuthoringApplyResponse", response.json())
    replay = client.post("/api/authoring/apply", json=intent, headers={**headers, "X-Wedl-Confirmation": confirmation})
    assert replay.json() == {**response.json(), "idempotentReplay": True}
