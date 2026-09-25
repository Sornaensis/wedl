"""Transport policy and revision-bound generational read coverage."""
from __future__ import annotations

from copy import deepcopy
from pathlib import Path
import json
import sqlite3
import subprocess

from fastapi.testclient import TestClient
import jsonschema
import pytest
from wedl.api_contract import AuthPolicy, Transport, discovery_responses, route_contracts
from wedl.api_schemas import components
from wedl.cli import main
from wedl.compiler import cache_readiness
from wedl.generational_api import OPERATIONS, bootstrap, execute, status_code
from wedl.generational_api import _resolve, _visible_name_world
from wedl.generational_query import TrustedViewerScope
from wedl.model import StoryTime
from wedl.repository import Repository
from wedl.server import create_app
from wedl.source import serialize_record
from wedl.util import canonical_json

from test_generational_compiler import _world
from test_generational_query import _former_role_world


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


def discovery_request(repository: Repository, operation: str, **fields: object) -> dict[str, object]:
    value = request(repository, operation, **fields)
    value.pop("depth")
    return value


def test_discovery_bootstrap_horizon_labels_and_cursor(
        generational_repo: tuple[Repository, dict[str, str]]) -> None:
    repository, mapping = generational_repo
    client = TestClient(create_app(repository.root))
    token = client.get("/api/session").json()["token"]
    headers = {"X-Wedl-Token": token}
    assert client.get("/api/generational/bootstrap").status_code == 401
    opened = bootstrap(repository)
    assert opened == client.get("/api/generational/bootstrap", headers=headers).json()
    assert opened["state"] == "available" and opened["revision"] == repository.head()
    assert opened["capabilities"] == ["generational-core-v1"]
    assert opened["timelines"] == ["main"]
    assert bootstrap(repository, "invalid")["state"] == "invalid"
    assert bootstrap(repository, "")["state"] == "invalid"
    assert client.get("/api/generational/bootstrap?revision=", headers=headers).json()["state"] == "invalid"

    early = {"timeline": "main", "tick": "-10", "order": "0"}
    base = discovery_request(repository, "discover", kind="character", text="character",
                             cursor=None, items=1, at=early)
    page = execute(repository, "discover", base)
    assert page["state"] == "available" and len(page["results"]) == 1
    assert page["cursor"]
    second = execute(repository, "discover", {**base, "cursor": page["cursor"]})
    assert second["state"] == "available"
    assert execute(repository, "discover", {**base, "text": "CHARACTER",
                                            "cursor": page["cursor"]}) == second
    assert page["results"][0]["id"] != second["results"][0]["id"]
    for changed in ({"at": {"timeline": "main", "tick": "-9", "order": "0"}},
                    {"text": "different"}, {"kind": "event"},
                    {"cursor": page["cursor"][:-1] + "*"}):
        assert execute(repository, "discover", {**base, "cursor": page["cursor"],
                                                **changed})["state"] == "invalid"
    assert execute(repository, "discover", {**base, "revision": "0" * 40})["state"] == "unavailable"
    assert execute(repository, "discover", {**base, "timeline": "unknown",
                                             "at": {"timeline": "unknown", "tick": "0", "order": "0"}})["state"] == "invalid"
    assert execute(repository, "discover", {**base, "capabilities": ["generational-core-v1", "spatial-core-v1"]})["state"] == "unavailable"
    labels = discovery_request(repository, "labels", ids=[mapping["character_child"],
                                                     mapping["character_future_lineage"],
                                                     mapping["character_no_lineage"]], at=early)
    answer = execute(repository, "labels", labels)
    assert answer["state"] == "available"
    assert [row["id"] for row in answer["labels"]] == [mapping["character_child"]]
    assert client.post("/api/generational/labels", json=labels, headers=headers).json() == answer
    assert execute(repository, "parents", request(repository, "parents",
        subject=mapping["character_no_lineage"], at=early))["state"] == "unknown"
    assert execute(repository, "parents", request(repository, "parents",
        subject=mapping["character_future_lineage"], at=early))["state"] == "unknown"

    alias = execute(repository, "discover", discovery_request(
        repository, "discover", kind="organization", text="Aster", cursor=None,
        at={"timeline": "main", "tick": "-20", "order": "0"}))
    assert alias["state"] == "available"
    assert any(row["matchedName"] == "Aster" for row in alias["results"])


def test_discovery_request_bounds_match_openapi(
        generational_repo: tuple[Repository, dict[str, str]]) -> None:
    repository, _mapping = generational_repo
    client = TestClient(create_app(repository.root))
    headers = {"X-Wedl-Token": client.get("/api/session").json()["token"]}
    schemas = client.get("/openapi.json").json()["components"]["schemas"]
    discover_schema = jsonschema.Draft202012Validator(schemas["GenerationalDiscoverRequest"])
    labels_schema = jsonschema.Draft202012Validator(schemas["GenerationalLabelsRequest"])
    padded = discovery_request(repository, "discover", kind="character",
                               text=" " * 64 + "A", cursor=None)
    assert not discover_schema.is_valid(padded)
    assert execute(repository, "discover", padded)["state"] == "invalid"
    assert client.post("/api/generational/discover", json=padded,
                       headers=headers).json()["state"] == "invalid"
    accepted_ids = discovery_request(repository, "labels", ids=["x" * 129])
    assert labels_schema.is_valid(accepted_ids)
    expected = execute(repository, "labels", accepted_ids)
    assert expected["state"] == "available" and expected["labels"] == []
    assert client.post("/api/generational/labels", json=accepted_ids,
                       headers=headers).json() == expected
    oversized_ids = {**accepted_ids, "ids": ["x" * 257]}
    assert not labels_schema.is_valid(oversized_ids)
    assert execute(repository, "labels", oversized_ids)["state"] == "invalid"
    assert client.post("/api/generational/labels", json=oversized_ids,
                       headers=headers).json()["state"] == "invalid"


def test_discovery_uses_compiled_rows_and_rebuilds_malformed_cache(
        generational_repo: tuple[Repository, dict[str, str]],
        monkeypatch: pytest.MonkeyPatch) -> None:
    repository, mapping = generational_repo
    payload = discovery_request(repository, "labels", ids=[mapping["character_child"]])
    expected = execute(repository, "labels", payload)
    assert expected["state"] == "available"
    revision = repository.head()
    with monkeypatch.context() as patch:
        patch.setattr(repository, "load_world", lambda *args, **kwargs: (_ for _ in ()).throw(
            AssertionError("discovery loaded whole-world source")))
        assert execute(repository, "labels", payload) == expected
        assert bootstrap(repository)["state"] == "available"
    database = cache_readiness(repository, revision)["database"]
    connection = sqlite3.connect(database)
    try:
        connection.execute("DROP INDEX generational_discovery_segment_idx")
        connection.commit()
    finally:
        connection.close()
    assert cache_readiness(repository, revision)["state"] == "incompatible"
    assert execute(repository, "labels", payload) == expected
    assert repository.head() == revision
    assert cache_readiness(repository, revision)["state"] == "ready"


def test_discovery_prefix_includes_supplementary_unicode_direct_and_http(tmp_path: Path) -> None:
    world, mapping = _world()
    subject = world.records[mapping["character_child"]]
    subject.frontmatter["title"] = "A😀"
    subject.frontmatter["aliases"] = ["A𐀀"]
    root = tmp_path / "unicode-generational"
    root.mkdir()
    for record in world.records.values():
        path = root / ("story/world.md" if record.kind == "world" else record.source_path)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(serialize_record(record.frontmatter, f"# {record.title}\n"))
    (root / ".gitignore").write_text(".wedl/\n", encoding="utf-8")
    for arguments in (("init", "-q"), ("config", "user.name", "wedl test"),
                      ("config", "user.email", "wedl@test.invalid"),
                      ("add", "story", ".gitignore"), ("commit", "-qm", "seed")):
        subprocess.run(["git", "-C", str(root), *arguments], check=True, capture_output=True)
    repository = Repository(root)
    client = TestClient(create_app(root))
    token = client.get("/api/session").json()["token"]
    headers = {"X-Wedl-Token": token}
    first_request = discovery_request(repository, "discover", kind="character", text="A",
                                      cursor=None, items=1)
    first = execute(repository, "discover", first_request)
    assert first["state"] == "available" and first["cursor"]
    assert client.post("/api/generational/discover", json=first_request,
                       headers=headers).json() == first
    second_request = {**first_request, "cursor": first["cursor"]}
    second = execute(repository, "discover", second_request)
    assert second["state"] == "available" and second["cursor"] is None
    assert client.post("/api/generational/discover", json=second_request,
                       headers=headers).json() == second
    assert {first["results"][0]["matchedName"], second["results"][0]["matchedName"]} == {
        "A😀", "A𐀀"}
    assert first["results"][0]["id"] == second["results"][0]["id"] == subject.id


def test_discovery_overlong_names_close_direct_and_http(tmp_path: Path) -> None:
    world, mapping = _world()
    subject = world.records[mapping["character_child"]]
    subject.frontmatter["title"] = "T" * 300
    subject.frontmatter["aliases"] = ["Short Alias"]
    organization = next(record for record in world.records.values()
                        if record.kind == "organization" and record.title == "House Aster")
    organization.frontmatter["transitions"][0]["payload"]["title"] = "O" * 300
    root = tmp_path / "overlong-generational"
    root.mkdir()
    for record in world.records.values():
        path = root / ("story/world.md" if record.kind == "world" else record.source_path)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(serialize_record(record.frontmatter, f"# {record.title}\n"))
    (root / ".gitignore").write_text(".wedl/\n", encoding="utf-8")
    for arguments in (("init", "-q"), ("config", "user.name", "wedl test"),
                      ("config", "user.email", "wedl@test.invalid"),
                      ("add", "story", ".gitignore"), ("commit", "-qm", "seed")):
        subprocess.run(["git", "-C", str(root), *arguments], check=True, capture_output=True)
    repository = Repository(root)
    client = TestClient(create_app(root))
    headers = {"X-Wedl-Token": client.get("/api/session").json()["token"]}
    for prefix in ("T" * 300, "short alias"):
        payload = discovery_request(repository, "discover", kind="character", text=prefix,
                                    cursor=None)
        if len(prefix) > 64:
            payload["text"] = prefix[:64]
        direct = execute(repository, "discover", payload)
        assert {key: direct[key] for key in ("state", "results", "cursor")} == {
            "state": "available", "results": [], "cursor": None}
        assert client.post("/api/generational/discover", json=payload,
                           headers=headers).json() == direct
    payload = discovery_request(repository, "labels", ids=[subject.id])
    expected_labels = execute(repository, "labels", payload)
    assert expected_labels["state"] == "available" and expected_labels["labels"] == []
    assert client.post("/api/generational/labels", json=payload,
                       headers=headers).json() == expected_labels
    early = {"timeline": "main", "tick": "-20", "order": "0"}
    later = {"timeline": "main", "tick": "-5", "order": "0"}
    for at, expected_titles in ((early, ["House Aster"]), (later, [])):
        discover = discovery_request(repository, "discover", kind="organization",
                                     text="Aster", cursor=None, at=at)
        direct = execute(repository, "discover", discover)
        assert direct["state"] == "available"
        assert [row["title"] for row in direct["results"]] == expected_titles
        assert client.post("/api/generational/discover", json=discover,
                           headers=headers).json() == direct
        labels = discovery_request(repository, "labels", ids=[organization.id], at=at)
        direct_labels = execute(repository, "labels", labels)
        assert direct_labels["state"] == "available"
        assert [row["title"] for row in direct_labels["labels"]] == expected_titles
        assert client.post("/api/generational/labels", json=labels,
                           headers=headers).json() == direct_labels


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
                {"audience": "archivist"},
                {"mode": ["author-as-of"]}):
        value = execute(repository, "parents", {**early, **bad})
        assert value["state"] == "invalid" and status_code(value) == 400
    assert execute(repository, "parents", {**early, "subject": "no such character"})["state"] == "unknown"


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
                                      else {"kind": "character", "text": "character", "cursor": None}
                                      if operation == "discover" else {"ids": [mapping["character_child"]]}
                                      if operation == "labels" else {"subject": subjects[operation]})
        if operation == "relatives":
            fields["target"] = mapping["character_alpha"]
        if operation == "context":
            fields["maxCharacters"] = 4096
        as_of = request(repository, operation, **fields)
        if operation in {"discover", "labels"}:
            as_of.pop("depth")
        assert_parity(operation, as_of)
        if operation == "parents":
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


def test_former_roles_name_first_cli_http_and_openapi(
        tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    world, organization, ended, null_role, hidden, future = _former_role_world()
    world.records.pop(hidden)
    world.records.pop(future)
    root = tmp_path / "former-roles-repo"
    root.mkdir()
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
    repository = Repository(root)
    client = TestClient(create_app(root))
    token = client.get("/api/session").json()["token"]
    headers = {"X-Wedl-Token": token}
    schemas = client.get("/openapi.json").json()["components"]["schemas"]
    request_schema = jsonschema.Draft202012Validator(schemas["GenerationalOrganizationRequest"])
    response_schema = jsonschema.Draft202012Validator(schemas["GenerationalOrganizationAvailableOutcome"])
    payload_file = tmp_path / "former-roles-request.json"
    base = request(repository, "organization", subject="Cadet House",
                   at={"timeline": "main", "tick": "0", "order": "1"})
    opted = {**base, "includeFormerRoles": True}
    assert request_schema.is_valid(base) and request_schema.is_valid(opted)
    assert not request_schema.is_valid({key: value for key, value in opted.items() if key != "at"})
    assert not request_schema.is_valid({**base, "includeFormerRoles": False})
    assert not request_schema.is_valid({**opted, "cursor": None})

    def parity(operation: str, body: dict[str, object]) -> dict[str, object]:
        result = execute(repository, operation, body)
        payload_file.write_text(json.dumps(body), encoding="utf-8")
        expected_exit = 0 if result["state"] in {"available", "unknown"} else 2
        assert main(["--compact", "generational", operation, str(payload_file),
                     "--repo", str(root)]) == expected_exit
        output = capsys.readouterr()
        assert json.loads(output.out if expected_exit == 0 else output.err) == result
        http = client.post(f"/api/generational/{operation}", json=body, headers=headers)
        assert http.status_code == status_code(result)
        assert http.json() == result
        return result

    default = parity("organization", base)
    offered = parity("organization", opted)
    response_schema.validate(default)
    response_schema.validate(offered)
    assert canonical_json(default) == canonical_json({key: value for key, value in offered.items()
                                                       if key != "formerRoles"})
    assert [row["recordId"] for row in offered["formerRoles"]] == sorted((ended, null_role))
    assert {row["value"]["role"] for row in offered["formerRoles"]} == {None, "steward"}
    assert all(row["citations"] for row in offered["formerRoles"])
    assert any(row["causes"] for row in offered["formerRoles"])
    assert parity("organization", {**opted, "items": 2}) == {
        "protocol": "wedl-generational/v1", "operation": "organization",
        "revision": repository.head(), "state": "limit", "code": "GEN-LIMIT-001"}
    for bad in ({**base, "includeFormerRoles": False},
                {**base, "includeFormerRoles": "true"},
                {**opted, "cursor": None},
                {key: value for key, value in opted.items() if key != "at"},
                {**opted, "mode": "character"},
                {**{key: value for key, value in opted.items() if key != "at"},
                 "mode": "author-all-time"}):
        assert parity("organization", bad)["code"] == "GEN-REQUEST-001"
    assert parity("union", {**request(repository, "union", subject="The Threefold Compact"),
                            "includeFormerRoles": True})["code"] == "GEN-REQUEST-001"
    assert organization in canonical_json(offered)
