"""Transport policy and revision-bound generational read coverage."""
from __future__ import annotations

from copy import deepcopy
from pathlib import Path
import json
import subprocess

from fastapi.testclient import TestClient
import jsonschema
import pytest
from wedl.api_contract import AuthPolicy, Transport, discovery_responses, route_contracts
from wedl.api_schemas import components
from wedl.cli import main
from wedl.generational_api import OPERATIONS, execute, status_code
from wedl.generational_api import _resolve, _visible_name_world
from wedl.generational_query import TrustedViewerScope
from wedl.model import StoryTime
from wedl.repository import Repository
from wedl.server import create_app
from wedl.source import serialize_record
from wedl.util import canonical_json

from test_generational_compiler import _world


def seed_generational_repository(repository: Repository) -> tuple[Repository, dict[str, str]]:
    world, mapping = _world()
    files = {path.relative_to(repository.root).as_posix(): None
             for path in (repository.root / "story").rglob("*.md")}
    files.update({("story/world.md" if record.kind == "world" else record.source_path):
                  serialize_record(record.frontmatter, f"# {record.title}\n")
                  for record in world.records.values()})
    repository.commit_files(expected_head=repository.head(), files=files,
                            message="seed generational vector")
    return repository, mapping


@pytest.fixture(scope="module")
def generational_repo(tmp_path_factory: pytest.TempPathFactory) -> tuple[Repository, dict[str, str]]:
    root = tmp_path_factory.mktemp("generational-transport") / "repo"
    root.mkdir()
    world, mapping = _world()
    for record in world.records.values():
        path = root / ("story/world.md" if record.kind == "world" else record.source_path)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(serialize_record(record.frontmatter, f"# {record.title}\n"))
    (root / ".gitignore").write_text(".wedl/\n", encoding="utf-8")
    for args in (("init", "-q"), ("config", "core.longpaths", "true"),
                 ("config", "user.name", "wedl test"),
                 ("config", "user.email", "wedl@test.invalid"),
                 ("add", "story", ".gitignore"), ("commit", "-qm", "seed")):
        subprocess.run(["git", "-C", str(root), *args], check=True, capture_output=True)
    return Repository(root), mapping


def request(repository: Repository, operation: str, **fields: object) -> dict[str, object]:
    return {"protocol": "wedl-generational/v1", "operation": operation,
            "revision": repository.head(), "capabilities": ["generational-core-v1"],
            "mode": "author-as-of", "timeline": "main",
            "at": {"timeline": "main", "tick": "0", "order": "0"},
            "items": 20, "depth": 4, **fields}


def test_name_first_parents_horizons_and_character_controls(generational_repo: tuple[Repository, dict[str, str]]) -> None:
    repository, mapping = generational_repo
    child = mapping["character_child"]
    early = request(repository, "parents", subject="character_child",
                    at={"timeline": "main", "tick": "-10", "order": "0"})
    before = execute(repository, "parents", early)
    assert before["state"] == "available"
    assert [row["label"] for row in before["relations"]] == ["biological-parent"]
    assert before["relations"][0]["citations"][0]["applicability"]["point"]["tick"] == "-10"
    assert execute(repository, "parents", {**early, "subject": child}) == before
    later = execute(repository, "parents", request(repository, "parents", subject=child))
    assert {row["label"] for row in later["relations"]} == {"biological-parent", "adoptive-parent"}
    future = request(repository, "parents", subject="character_future_lineage")
    assert execute(repository, "parents", future)["state"] == "unknown"
    all_time = {key: value for key, value in future.items() if key != "at"}
    all_time["mode"] = "author-all-time"
    assert execute(repository, "parents", all_time)["state"] == "available"
    assert execute(repository, "parents", {**all_time, "at": early["at"]})["state"] == "invalid"
    controls = ("character_no_lineage", "character_future_lineage", "character_secret")
    character_outcomes = [execute(repository, "parents", {**request(repository, "parents", subject=name),
                                                           "mode": "character"}) for name in controls]
    assert character_outcomes[0] == character_outcomes[1] == character_outcomes[2]
    assert character_outcomes[0]["state"] == "unknown"
    for bad in ({"at": {"timeline": "main", "tick": -1, "order": "0"}},
                {"at": {"timeline": "main", "tick": "-0", "order": "0"}},
                {"audience": "archivist"}, {"subject": "no such character"},
                {"mode": ["author-as-of"]}):
        value = execute(repository, "parents", {**early, **bad})
        assert value["state"] == "invalid" and status_code(value) == 400


def test_read_operations_and_scope_bound_cursor(generational_repo: tuple[Repository, dict[str, str]]) -> None:
    repository, mapping = generational_repo
    world = repository.load_world(repository.head())
    union = next(record.title for record in world.records.values() if record.kind == "union")
    organization = next(record.title for record in world.records.values() if record.title == "Cadet House")
    legacy = next(record.title for record in world.records.values() if record.kind == "legacy")
    early = {"timeline": "main", "tick": "-10", "order": "0"}
    for operation, hidden in (("union", union), ("organization", organization)):
        for reference in (hidden, next(record.id for record in world.records.values()
                                       if record.title == hidden)):
            outcome = execute(repository, operation, request(repository, operation,
                                                           subject=reference, at=early))
            assert outcome == {"protocol": "wedl-generational/v1", "operation": operation,
                               "revision": repository.head(), "state": "invalid",
                               "code": "GEN-REFERENCE-001"}
    suggestions = execute(repository, "organization", request(repository, "organization",
                                                                subject="Cadet Hous", at=early))
    assert organization not in canonical_json(suggestions)
    ambiguous_world = deepcopy(world)
    ambiguous_world.find("Cadet House", "organization").frontmatter["aliases"] = ["House Aster"]
    scope = TrustedViewerScope(repository.head(), "author-as-of", "main",
                               StoryTime("main", -10, 0), frozenset({"public"}),
                               frozenset({"ordinary"}), frozenset({"generational-core-v1"}))
    filtered = _visible_name_world(repository, ambiguous_world, scope, "organization",
                                   require_compiled=False)
    assert filtered is not None
    assert _resolve(filtered, "House Aster", "organization")[0] == world.find(
        "House Aster", "organization").id
    assert len(execute(repository, "union", request(repository, "union", subject=union))["participants"]) == 3
    assert execute(repository, "organization", request(repository, "organization", subject=organization))["roles"]
    legacy_value = execute(repository, "legacy", request(repository, "legacy", subject=legacy))
    assert len(legacy_value["holders"]) == 2 and len(legacy_value["claims"]) == 2
    assert legacy_value["succession"]
    assert execute(repository, "context", request(repository, "context", subject=mapping["character_child"], maxCharacters=4096))["state"] in {"available", "unknown"}
    for budget in (80, 200, 4096):
        context = execute(repository, "context", request(repository, "context",
                                                         subject=mapping["character_child"],
                                                         maxCharacters=budget))
        assert context["state"] == "limit" or len(canonical_json(context)) <= budget
    search = execute(repository, "search", request(repository, "search", text="basis", items=1, cursor=None))
    assert search["state"] == "available" and search["cursor"]
    next_page = request(repository, "search", text="basis", items=1, cursor=search["cursor"])
    assert execute(repository, "search", next_page)["state"] == "available"
    changed_scope = deepcopy(next_page)
    changed_scope["at"] = {"timeline": "main", "tick": "1", "order": "0"}
    assert execute(repository, "search", changed_scope)["state"] == "invalid"


def test_http_cli_contract_and_session_are_identical(
        generational_repo: tuple[Repository, dict[str, str]], tmp_path: Path,
        capsys: pytest.CaptureFixture[str]) -> None:
    repository, mapping = generational_repo
    contracts = [contract for contract in route_contracts() if contract.command[0] == "generational"]
    assert tuple(contract.command[1] for contract in contracts) == (*OPERATIONS, "scaffold", "schema")
    for contract in contracts[:len(OPERATIONS)]:
        assert contract.binding is not None and contract.binding.auth is AuthPolicy.SESSION
        assert contract.binding.path == f"/api/generational/{contract.command[1]}"
        assert any(arg.transport is Transport.BODY for arg in contract.arguments)
        responses = discovery_responses(contract.discovery)
        assert {200, 400, 409, 422} <= set(responses)
    client = TestClient(create_app(repository.root))
    body = request(repository, "parents", subject="character_child")
    assert client.post("/api/generational/parents", json=body).status_code == 401
    session = client.get("/api/session").json()["token"]
    response = client.post("/api/generational/parents", json=body,
                           headers={"X-Wedl-Token": session})
    assert response.status_code == 200
    assert response.json() == execute(repository, "parents", body)
    malformed = client.post("/api/generational/parents", json={**body, "audience": "author"},
                            headers={"X-Wedl-Token": session})
    assert malformed.status_code == 400 and malformed.json()["state"] == "invalid"

    world = repository.load_world(repository.head(), cache_write=False)
    subjects = {
        "parents": "character_child", "ancestors": "character_child",
        "descendants": "character_alpha", "relatives": "character_child",
        "union": next(record.title for record in world.records.values() if record.kind == "union"),
        "organization": "House Aster",
        "legacy": next(record.title for record in world.records.values() if record.kind == "legacy"),
        "vital": "character_child", "context": "character_child",
    }
    payload_file = tmp_path / "generational-request.json"
    headers = {"X-Wedl-Token": session}
    response_schemas = components()["schemas"]
    response_validators: dict[str, jsonschema.Draft202012Validator] = {}

    def assert_parity(operation: str, payload: dict[str, object]) -> dict[str, object]:
        direct = execute(repository, operation, payload)
        schema_name = f"Generational{operation.capitalize()}{direct['state'].capitalize()}Outcome"
        validator = response_validators.setdefault(
            schema_name, jsonschema.Draft202012Validator(response_schemas[schema_name]))
        validator.validate(direct)
        payload_file.write_text(json.dumps(payload), encoding="utf-8")
        expected_exit = 0 if direct["state"] in {"available", "unknown"} else 2
        assert main(["--compact", "generational", operation, str(payload_file),
                     "--repo", str(repository.root)]) == expected_exit
        captured = capsys.readouterr()
        assert json.loads(captured.out if expected_exit == 0 else captured.err) == direct
        response = client.post(f"/api/generational/{operation}", json=payload, headers=headers)
        assert response.status_code == status_code(direct)
        assert response.json() == direct
        return direct

    for operation in OPERATIONS:
        fields: dict[str, object] = ({"text": "basis", "cursor": None} if operation == "search"
                                      else {"subject": subjects[operation]})
        if operation == "relatives":
            fields["target"] = mapping["character_alpha"]
        if operation == "context":
            fields["maxCharacters"] = 4096
        as_of = request(repository, operation, **fields)
        assert_parity(operation, as_of)
        all_time = {key: value for key, value in as_of.items() if key != "at"}
        all_time["mode"] = "author-all-time"
        assert_parity(operation, all_time)

    matrix = (
        ("parents", request(repository, "parents", subject="character_child"), "available"),
        ("parents", request(repository, "parents", subject="character_no_lineage"), "unknown"),
        ("parents", {**body, "mode": "character"}, "unknown"),
        ("parents", {**body, "viewer": "Mara Vale"}, "invalid"),
        ("parents", {**body, "capabilities": ["generational-core-v1", "spatial-core-v1"]},
         "unavailable"),
        ("context", request(repository, "context", subject="character_child",
                            maxCharacters=80), "limit"),
    )
    for operation, payload, expected in matrix:
        assert assert_parity(operation, payload)["state"] == expected
    malformed_revision = assert_parity("parents", {**body, "revision": "bad"})
    assert malformed_revision == {"protocol": "wedl-generational/v1", "operation": "parents",
                                  "revision": None, "state": "invalid", "code": "GEN-REQUEST-001"}
    non_search_cursor = {**body, "cursor": None}
    assert not jsonschema.Draft202012Validator(
        response_schemas["GenerationalParentsRequest"]).is_valid(non_search_cursor)
    assert assert_parity("parents", non_search_cursor) == {
        "protocol": "wedl-generational/v1", "operation": "parents",
        "revision": repository.head(), "state": "invalid", "code": "GEN-REQUEST-001"}
