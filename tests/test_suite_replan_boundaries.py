"""Small functional boundaries retained when large matrices run in performance."""

from __future__ import annotations

import json

import jsonschema
import pytest
from fastapi.testclient import TestClient

from test_generational_api import generational_repo, request as generational_request
from test_spatial_api import CAPABILITIES, _spatial_seed, spatial_repository
from test_thread_search_filter import THREAD_A, _selector_connection
from wedl import migration
from wedl.cli import main
from wedl.compiler import compile_world, require_database
from wedl.errors import StaleRevision
from wedl.api_schemas import components
from wedl.generational_api import OPERATIONS, execute as generational_execute
from wedl.generational_api import status_code as generational_status_code
from wedl.repository import Repository
from wedl.profiles import CompilationProfile
from wedl.search import _finish_ranked_search
from wedl.server import create_app
from wedl.source import generated_path, serialize_record, split_envelope
from wedl.spatial_api import PROTOCOL, execute, status_code
from wedl.thread_filter import resolve_thread_filter


def _migration_request(repository: Repository, mode: str, key: str) -> dict:
    return {"protocol": migration.PROTOCOL, "mode": mode,
            "expectedHead": repository.head(), "idempotencyKey": key}


def _apply_upgrade(repository: Repository, mode: str, key: str) -> tuple[dict, dict]:
    request = _migration_request(repository, mode, key)
    plan = migration.preview(repository, request)
    assert plan["valid"]
    result = migration.apply(
        repository, {**request, "sourceSnapshotHash": plan["sourceSnapshotHash"]},
        confirmation_token_value=plan["confirmationToken"],
    )
    assert result["status"] == "committed"
    return plan, result


@pytest.mark.parametrize("schema", ("wedl/v0.3", "wedl/v0.5", "wedl/v0.6"))
def test_small_legacy_upgrade_lifecycle_preserves_body_and_rollback(
    task91_repo: Repository, schema: str,
) -> None:
    repository = task91_repo
    if schema in {"wedl/v0.5", "wedl/v0.6"}:
        _apply_upgrade(repository, "upgrade-v03", "small-prep-v05")
    if schema == "wedl/v0.6":
        _apply_upgrade(repository, "upgrade-v06", "small-prep-v06")
    record = {"schema": schema, "kind": "location", "id": "loc_00000000000000000000000098",
              "title": "Small legacy place", "domain": "setting.release", "status": "canonical"}
    body = "# Small legacy place\nPreserved verbatim.\n"
    path = generated_path("story", "location", record["title"], record["id"], record)
    repository.commit_files(expected_head=repository.head(),
                            files={path: serialize_record(record, body)},
                            message="add small legacy place")
    before = repository.snapshot("HEAD").files
    old_head = repository.head()
    request = _migration_request(repository, "upgrade-v07", f"small-{schema}")
    plan = migration.preview(repository, request)
    assert plan["valid"] and not plan["noOp"]
    assert repository.head() == old_head and repository.snapshot("HEAD").files == before
    bound = {**request, "sourceSnapshotHash": plan["sourceSnapshotHash"]}
    applied = migration.apply(repository, bound,
                              confirmation_token_value=plan["confirmationToken"])
    assert applied["status"] == "committed" and applied["previousHead"] == old_head
    replay = migration.apply(repository, bound,
                             confirmation_token_value=plan["confirmationToken"])
    assert replay["idempotentReplay"] is True and replay["newHead"] == applied["newHead"]
    new_record, new_body = split_envelope(repository.snapshot("HEAD").files[path], path)
    old_record, old_body = split_envelope(before[path], path)
    assert new_body == old_body == body
    assert new_record["schema"] == "wedl/v0.7"
    assert {key: value for key, value in new_record.items() if key != "schema"} == {
        key: value for key, value in old_record.items() if key != "schema"
    }
    assert migration.preview(repository, _migration_request(repository, "upgrade-v07", "small-noop"))["noOp"]
    with pytest.raises(StaleRevision):
        migration.preview(repository, {**_migration_request(repository, "upgrade-v07", "small-stale"),
                                       "expectedHead": old_head})
    rollback = {**_migration_request(repository, "rollback", "small-rollback"),
                "rollbackBackupRef": plan["backupRef"]}
    rollback_plan = migration.preview(repository, rollback)
    assert rollback_plan["valid"]
    reversed_result = migration.apply(
        repository, {**rollback, "sourceSnapshotHash": rollback_plan["sourceSnapshotHash"]},
        confirmation_token_value=rollback_plan["confirmationToken"],
    )
    assert reversed_result["status"] == "committed"
    assert repository.snapshot("HEAD").files == before


def test_small_spatial_source_compiled_rebuild_and_closed_transport(
    spatial_repository: Repository, tmp_path, capsys: pytest.CaptureFixture[str],
) -> None:
    world = spatial_repository.load_world()
    request = {"protocol": PROTOCOL, "revision": world.revision,
               "capabilities": CAPABILITIES, "limit": 10, "cursor": None,
               "locationId": "loc_00000000000000000000000000"}
    source = execute(spatial_repository, "children", request, require_compiled=False)
    compiled = execute(spatial_repository, "children", request, require_compiled=True)
    assert source == compiled and compiled["state"] == "ok"
    _, database = require_database(spatial_repository, require_compiled=True)
    database.unlink()
    assert compile_world(spatial_repository)["status"] == "compiled"
    assert execute(spatial_repository, "children", request, require_compiled=True) == compiled
    request_file = tmp_path / "small-spatial-request.json"
    request_file.write_text(json.dumps(request), encoding="utf-8")
    assert main(["--compact", "spatial", "children", str(request_file),
                 "--repo", str(spatial_repository.root), "--require-compiled"]) == 0
    assert json.loads(capsys.readouterr().out) == compiled
    with TestClient(create_app(spatial_repository.root)) as client:
        response = client.post("/api/spatial/children?requireCompiled=true", json=request)
        assert response.status_code == status_code(compiled)
        assert response.json() == compiled
        invalid = client.post("/api/spatial/children?requireCompiled=true",
                              json={**request, "cursor": "forged"})
        assert invalid.status_code == 400 and invalid.json()["state"] == "invalid"


def test_small_generational_transport_matrix_and_closed_revision(
    generational_repo: tuple[Repository, dict[str, str]], tmp_path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    repository, mapping = generational_repo
    world = repository.load_world(repository.head(), cache_write=False)
    subject = {
        "parents": "character_child", "ancestors": "character_child",
        "descendants": "character_alpha", "relatives": "character_child",
        "union": next(record.title for record in world.records.values() if record.kind == "union"),
        "organization": "House Aster",
        "legacy": next(record.title for record in world.records.values() if record.kind == "legacy"),
        "character-unions": "character_alpha", "organization-legacies": "House Aster",
        "vital": "character_child", "context": "character_child",
    }
    schemas = components()["schemas"]
    path = tmp_path / "small-generational-request.json"
    with TestClient(create_app(repository.root)) as client:
        unauthorized = client.post("/api/generational/parents", json=generational_request(
            repository, "parents", subject="character_child"))
        assert unauthorized.status_code == 401
        token = client.get("/api/session").json()["token"]
        headers = {"X-Wedl-Token": token}
        for operation in OPERATIONS:
            fields: dict[str, object] = (
                {"text": "basis", "cursor": None} if operation == "search" else
                {"kind": "character", "text": "character", "cursor": None}
                if operation == "discover" else
                {"ids": [mapping["character_child"]]} if operation == "labels" else
                {"subject": subject[operation]}
            )
            if operation == "relatives":
                fields["target"] = mapping["character_alpha"]
            if operation == "context":
                fields["maxCharacters"] = 4096
            payload = generational_request(repository, operation, **fields)
            if operation in {"discover", "labels"}:
                payload.pop("depth")
            outcome = generational_execute(repository, operation, payload)
            stem = "".join(part.capitalize() for part in operation.split("-"))
            schema_name = f"Generational{stem}{outcome['state'].capitalize()}Outcome"
            jsonschema.Draft202012Validator(schemas[schema_name]).validate(outcome)
            path.write_text(json.dumps(payload), encoding="utf-8")
            expected_exit = 0 if outcome["state"] in {"available", "unknown"} else 2
            assert main(["--compact", "generational", operation, str(path),
                         "--repo", str(repository.root)]) == expected_exit
            captured = capsys.readouterr()
            assert json.loads(captured.out if expected_exit == 0 else captured.err) == outcome
            response = client.post(f"/api/generational/{operation}", json=payload, headers=headers)
            assert response.status_code == generational_status_code(outcome)
            assert response.json() == outcome
            invalid = {**payload, "revision": "invalid"}
            closed = generational_execute(repository, operation, invalid)
            assert closed["state"] == "invalid" and "results" not in closed
            response = client.post(f"/api/generational/{operation}", json=invalid, headers=headers)
            assert response.status_code == generational_status_code(closed)
            assert response.json() == closed


def test_small_thread_filter_preserves_rank_and_never_backfills() -> None:
    connection = _selector_connection()
    connection.execute("INSERT INTO record_thread VALUES (?,?)", ("char_b", THREAD_A))
    connection.execute("INSERT INTO record_thread VALUES (?,?)", ("char_c", THREAD_A))
    ranked = [
        {"documentId": "a", "entityId": "char_a"},
        {"documentId": "b", "entityId": "char_b"},
        {"documentId": "c", "entityId": "char_c"},
    ]
    profile = CompilationProfile(
        name="fts", fts_enabled=True, vector_enabled=False, vector_provider=None,
        vector_model=None, vector_dimensions=0, vector_max_features=0,
        fts_candidate_limit=2, vector_candidate_limit=1,
        hybrid_fts_weight=1.0, hybrid_vector_weight=1.0, hybrid_rrf_k=60.0,
    )
    selected = resolve_thread_filter(connection, (THREAD_A,))
    assert _finish_ranked_search(connection, ranked, 1, profile, selected) == [ranked[1]]
    connection.execute("DELETE FROM record_thread WHERE record_id='char_b'")
    assert _finish_ranked_search(connection, ranked, 1, profile, selected) == []
    assert _finish_ranked_search(connection, ranked, 3, profile, selected) == [ranked[2]]
