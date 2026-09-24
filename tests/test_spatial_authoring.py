"""Spatial authoring compiles to the confirmed, atomic changeset path."""

from __future__ import annotations

from copy import deepcopy
from pathlib import Path

import pytest
import yaml
from fastapi.testclient import TestClient

from wedl.authoring import _spatial_intent, apply_intent, preview_intent
from wedl.changeset import _apply
from wedl.compiler import AuthoringByteResult
from wedl.errors import ChronologyUpgradeRequired, ConfirmationMismatch, ConfirmationRequired, ConflictError, DirtyManagedTree, StaleRevision, UsageError
from wedl.model import Record, World
from wedl.repository import Repository
from wedl.query import show_entity, whereabouts
from wedl.server import create_app
from wedl.source import generated_path, serialize_record
from wedl.transaction_recovery import TransactionJournal
from wedl import changeset


ROOT = Path(__file__).resolve().parents[1]
HEAD = "a" * 40


def _world() -> World:
    fixture = yaml.safe_load((ROOT / "tests/fixtures/spatial_v07/valid-multimap.yaml").read_text(encoding="utf-8"))
    values = [fixture["world"], *fixture["maps"], *fixture["locations"], fixture["anchor"], fixture["portal"], fixture["route"], fixture["overlay"]]
    records = []
    for value in values:
        body = f"# {value['title']}\n\nOpaque prose.  \n"
        value = deepcopy(value)
        value["provenance"] = [{"source": "test"}]
        value["x-note"] = {"opaque_key": "keep"}
        path = generated_path("story", value["kind"], value["title"], value["id"], value)
        records.append(Record(value, body, path, serialize_record(value, body)))
    return World(HEAD, "b" * 40, {record.id: record for record in records}, ROOT)


def _intent(action: str, payload: dict) -> dict:
    return {"action": action, "expectedHead": HEAD, "idempotencyKey": "spatial-test", "payload": payload}


CREATE_CASES = [
    ("spatial.map.create", {"id": "map:new", "title": "New map", "crs": "local-planar:new", "axisOrder": ["east", "north"], "unit": "pace", "bounds": {"min": [-10, -10], "max": [10, 10]}}),
    ("spatial.route.create", {"id": "route:new", "title": "New route", "fromLocationId": "location:gate", "toLocationId": "loc_00000000000000000000000000", "direction": "one-way", "modes": ["foot"]}),
    ("spatial.overlay.create", {"id": "overlay:new", "title": "New overlay", "lifecycle": "time-bounded", "membership": {"locationIds": ["location:gate"]}, "audience": ["author"], "perspectives": ["author"], "valid": {"start": {"timeline": "main", "tick": "-1", "order": "0"}, "end": {"timeline": "main", "tick": "1", "order": "0"}}}),
]
UPDATE_CASES = [
    ("spatial.map.update", {"id": "map:town", "unit": "league"}),
    ("spatial.location.update", {"id": "location:gate", "parentId": None, "spatial": None}),
    ("spatial.route.update", {"id": "route:place-gate", "modes": ["horse"]}),
    ("spatial.overlay.update", {"id": "overlay:ward", "audience": ["public"]}),
]


@pytest.mark.parametrize(("action", "payload"), CREATE_CASES + UPDATE_CASES)
def test_all_seven_intents_compile_to_exact_source_only_changeset(action: str, payload: dict) -> None:
    world = _world()
    operation, = _spatial_intent(world, _intent(action, payload), action)
    assert operation["type"] == ("entity.create" if action.endswith(".create") else "entity.update")
    assert "bodyMarkdown" not in operation or action.endswith(".create")
    if action.endswith(".update"):
        assert operation["entity"] == payload["id"]
        assert "title" not in operation["frontmatterPatch"]
        before = world.records[payload["id"]]
        candidate, touched = _apply({"protocol": "wedl-changeset/v1", "expectedHead": HEAD, "idempotencyKey": "spatial-test", "operations": [operation]}, world, {})
        after = candidate[payload["id"]]
        assert touched == {payload["id"]}
        assert after.body == before.body
        assert after.frontmatter["provenance"] == before.frontmatter["provenance"]
        assert after.frontmatter["x-note"] == before.frontmatter["x-note"]
        if action == "spatial.location.update":
            assert "parent_id" not in after.frontmatter and "spatial" not in after.frontmatter
    else:
        value = operation["value"]["frontmatter"]
        assert value["id"] == payload["id"] and value["kind"] == action.split(".")[1]
        assert value["schema"] == "wedl/v0.7"
        assert value["title"] == payload["title"]
        if action == "spatial.overlay.create":
            assert value["valid"]["start"] == {"timeline": "main", "tick": -1, "order": 0}


@pytest.mark.parametrize("action,payload", UPDATE_CASES)
def test_updates_reject_title_rewrites(action: str, payload: dict) -> None:
    if action == "spatial.location.update":
        return
    with pytest.raises(UsageError, match="cannot change title"):
        _spatial_intent(_world(), _intent(action, {**payload, "title": "Renamed"}), action)


def test_intent_payloads_and_version_gate_are_closed() -> None:
    world = _world()
    for bad in (
        {"id": "map:town", "unit": "pace", "unknown": True},
        {"id": "map:town", "unit": "pace", "bounds": {"min": [0, 0], "max": [float("inf"), 1]}},
        {"id": "map:town", "unit": "pace", "title": "No rewrite"},
        {"id": "map:town", "unit": "x" * 257},
    ):
        with pytest.raises(UsageError):
            _spatial_intent(world, _intent("spatial.map.update", bad), "spatial.map.update")
    legacy = _world()
    legacy.world_record.frontmatter["schema"] = "wedl/v0.6"
    with pytest.raises(ChronologyUpgradeRequired):
        _spatial_intent(legacy, _intent("spatial.map.create", CREATE_CASES[0][1]), "spatial.map.create")


def test_spatial_apply_refuses_unconfirmed_bypass_before_repository_access() -> None:
    with pytest.raises(ConfirmationRequired):
        apply_intent(None, _intent("spatial.map.create", CREATE_CASES[0][1]), allow_unconfirmed=True)


def test_map_create_preview_apply_and_exact_replay_use_real_journal(ash_repo: Repository) -> None:
    world = _world()
    changes: dict[str, bytes | None] = {
        path.relative_to(ash_repo.root).as_posix(): None
        for path in (ash_repo.root / "story").rglob("*.md")
    }
    changes.update({record.source_path: record.raw_bytes for record in world.records.values()})
    ash_repo.commit_files(expected_head=ash_repo.head(), files=changes, message="seed v0.7 spatial authoring")
    request = _intent("spatial.map.create", CREATE_CASES[0][1])
    request["expectedHead"] = ash_repo.head()
    before = ash_repo.head()
    preview = preview_intent(ash_repo, request)
    assert preview["preview"]["valid"] is True
    assert ash_repo.head() == before
    invalid_route = _intent("spatial.route.create", {**CREATE_CASES[1][1], "toLocationId": "location:missing"})
    invalid_route["expectedHead"] = before
    assert preview_intent(ash_repo, invalid_route)["preview"]["valid"] is False
    assert ash_repo.head() == before
    with pytest.raises(ConfirmationRequired):
        apply_intent(ash_repo, request)
    token = preview["preview"]["confirmationToken"]
    with pytest.raises(ConfirmationMismatch):
        apply_intent(ash_repo, request, confirmation_token_value="wrong-token")
    assert ash_repo.head() == before
    source = ash_repo.root / ash_repo.load_world().records["map:town"].source_path
    original = source.read_bytes()
    source.write_bytes(original + b"\n")
    try:
        with pytest.raises(DirtyManagedTree):
            apply_intent(ash_repo, request, confirmation_token_value=token)
        assert ash_repo.head() == before
    finally:
        source.write_bytes(original)
    result = apply_intent(ash_repo, request, confirmation_token_value=token)
    assert result["idempotentReplay"] is False
    assert ash_repo.head() != before
    assert ash_repo.load_world().records["map:new"].frontmatter["crs"] == "local-planar:new"
    replay = apply_intent(ash_repo, request, confirmation_token_value=token)
    assert replay["idempotentReplay"] is True
    assert ash_repo.head() == result["newHead"] == replay["newHead"]
    with pytest.raises(ConflictError):
        apply_intent(ash_repo, {**request, "payload": {**request["payload"], "unit": "mile"}}, confirmation_token_value=token)
    stale = _intent("spatial.map.create", {**CREATE_CASES[0][1], "id": "map:after"})
    stale["expectedHead"] = before
    with pytest.raises(StaleRevision):
        preview_intent(ash_repo, stale)


def test_all_seven_spatial_intents_preview_apply_and_replay_through_real_journal(
    ash_repo: Repository, monkeypatch: pytest.MonkeyPatch,
) -> None:
    world = _world()
    changes: dict[str, bytes | None] = {
        path.relative_to(ash_repo.root).as_posix(): None
        for path in (ash_repo.root / "story").rglob("*.md")
    }
    changes.update({record.source_path: record.raw_bytes for record in world.records.values()})
    ash_repo.commit_files(expected_head=ash_repo.head(), files=changes, message="seed v0.7 spatial journal matrix")

    # Optional cache publication may defer, while source and receipt still commit
    # together through the actual journal and every candidate is validated.
    monkeypatch.setattr(changeset, "compile_world_bytes", lambda *_args, **_kwargs: AuthoringByteResult(
        "deferred-to-restart", {"status": "deferred-to-restart", "reason": "sqlite-serialize-unavailable"},
    ))
    for index, (action, payload) in enumerate(CREATE_CASES + UPDATE_CASES):
        request = {
            "action": action, "expectedHead": ash_repo.head(),
            "idempotencyKey": f"spatial-journal-{index}", "payload": deepcopy(payload),
        }
        before = ash_repo.head()
        prior = ash_repo.load_world().records.get(payload["id"])
        preview = preview_intent(ash_repo, request)
        assert preview["preview"]["valid"] is True, (action, preview)
        assert ash_repo.head() == before
        token = preview["preview"]["confirmationToken"]
        result = apply_intent(ash_repo, request, confirmation_token_value=token)
        assert result["idempotentReplay"] is False
        assert result["compile"]["status"] == "deferred-to-restart"
        assert result["compile"]["reason"] == "sqlite-serialize-unavailable"
        assert result["newHead"] == ash_repo.head() != before
        current = ash_repo.load_world().records[payload["id"]]
        if action.endswith(".create"):
            assert current.frontmatter["kind"] == action.split(".")[1]
        else:
            assert prior is not None
            assert current.body == prior.body
            assert current.frontmatter["provenance"] == prior.frontmatter["provenance"]
            assert current.frontmatter["x-note"] == prior.frontmatter["x-note"]
        if action == "spatial.location.update":
            assert "parent_id" not in current.frontmatter and "spatial" not in current.frontmatter
        replay = apply_intent(ash_repo, request, confirmation_token_value=token)
        assert replay["idempotentReplay"] is True
        assert replay["newHead"] == result["newHead"] == ash_repo.head()


def test_spatial_source_and_receipt_roll_back_on_journal_publication_failure(
    ash_repo: Repository, monkeypatch: pytest.MonkeyPatch,
) -> None:
    world = _world()
    changes: dict[str, bytes | None] = {
        path.relative_to(ash_repo.root).as_posix(): None
        for path in (ash_repo.root / "story").rglob("*.md")
    }
    changes.update({record.source_path: record.raw_bytes for record in world.records.values()})
    ash_repo.commit_files(expected_head=ash_repo.head(), files=changes, message="seed v0.7 spatial rollback")
    monkeypatch.setattr(changeset, "compile_world_bytes", lambda *_args, **_kwargs: AuthoringByteResult(
        "deferred-to-restart", {"status": "deferred-to-restart", "reason": "sqlite-serialize-unavailable"},
    ))
    request = {"action": "spatial.map.create", "expectedHead": ash_repo.head(),
               "idempotencyKey": "spatial-rollback", "payload": deepcopy(CREATE_CASES[0][1])}
    preview = preview_intent(ash_repo, request)
    assert preview["preview"]["valid"] is True
    token = preview["preview"]["confirmationToken"]
    before = ash_repo.head()
    receipt_path = ash_repo.root / ".wedl" / "idempotency.json"
    receipts_before = receipt_path.read_bytes() if receipt_path.exists() else None
    original = TransactionJournal.publish_surfaces
    def fail_once(self):
        raise RuntimeError("injected spatial publication failure")
    monkeypatch.setattr(TransactionJournal, "publish_surfaces", fail_once)
    with pytest.raises(RuntimeError, match="injected spatial publication failure"):
        apply_intent(ash_repo, request, confirmation_token_value=token)
    assert ash_repo.head() == before
    assert "map:new" not in ash_repo.load_world().records
    assert (receipt_path.read_bytes() if receipt_path.exists() else None) == receipts_before
    monkeypatch.setattr(TransactionJournal, "publish_surfaces", original)
    result = apply_intent(ash_repo, request, confirmation_token_value=token)
    assert result["idempotentReplay"] is False
    assert result["newHead"] == ash_repo.head() != before


def test_malformed_spatial_authoring_enums_and_numbers_are_wedl_http_errors(ash_repo: Repository) -> None:
    world = _world()
    changes: dict[str, bytes | None] = {
        path.relative_to(ash_repo.root).as_posix(): None
        for path in (ash_repo.root / "story").rglob("*.md")
    }
    changes.update({record.source_path: record.raw_bytes for record in world.records.values()})
    ash_repo.commit_files(expected_head=ash_repo.head(), files=changes, message="seed v0.7 malformed spatial requests")
    huge = 10**400
    cases = (
        ("spatial.map.update", {"id": "map:town", "zPolicy": ["required"]}),
        ("spatial.location.update", {"id": "location:gate", "spatial": {"mapId": "map:town", "geometry": {"kind": ["point"], "coordinates": [0, 0]}}}),
        ("spatial.map.update", {"id": "map:town", "bounds": {"min": [-huge, 0], "max": [huge, 1]}}),
        ("spatial.location.update", {"id": "location:gate", "spatial": {"mapId": "map:town", "geometry": {"kind": "point", "coordinates": [huge, 0]}}}),
        ("spatial.route.update", {"id": "route:place-gate", "routeDistance": {"value": huge, "unit": "pace"}}),
    )
    with TestClient(create_app(ash_repo.root)) as client:
        token = client.get("/api/session").json()["token"]
        for action, payload in cases:
            request = {"action": action, "expectedHead": ash_repo.head(), "idempotencyKey": action, "payload": payload}
            with pytest.raises(UsageError):
                _spatial_intent(world, request, action)
            response = client.post("/api/authoring/preview", json=request, headers={"X-Wedl-Token": token})
            assert response.status_code == 400
            assert response.json()["code"] == "usage_error"


def test_spatial_preview_is_read_only_for_source_cache_receipt_and_head(ash_repo: Repository) -> None:
    world = _world()
    changes: dict[str, bytes | None] = {
        path.relative_to(ash_repo.root).as_posix(): None
        for path in (ash_repo.root / "story").rglob("*.md")
    }
    changes.update({record.source_path: record.raw_bytes for record in world.records.values()})
    ash_repo.commit_files(expected_head=ash_repo.head(), files=changes, message="seed v0.7 preview-read-only")
    repository = Repository(ash_repo.root)
    request = {"action": "spatial.map.create", "expectedHead": repository.head(),
               "idempotencyKey": "spatial-read-only", "payload": deepcopy(CREATE_CASES[0][1])}
    source = ash_repo.root / next(iter(world.records.values())).source_path
    source_before = source.read_bytes()
    cache = ash_repo.root / ".wedl" / "source-cache.sqlite"
    receipt = ash_repo.root / ".wedl" / "idempotency.json"
    before_cache = cache.read_bytes() if cache.exists() else None
    before_receipt = receipt.read_bytes() if receipt.exists() else None
    head = repository.head()
    first = preview_intent(repository, request)
    second = preview_intent(Repository(ash_repo.root), request)
    assert first["preview"]["valid"] and first["preview"] == second["preview"]
    assert repository.head() == head and source.read_bytes() == source_before
    assert (cache.read_bytes() if cache.exists() else None) == before_cache
    assert (receipt.read_bytes() if receipt.exists() else None) == before_receipt


def test_spatial_location_update_keeps_detail_context_and_whereabouts_compatible(ash_repo: Repository) -> None:
    world = _world()
    root = world.world_record
    root_frontmatter = deepcopy(root.frontmatter)
    root_frontmatter["state_keys"] = {"character": {"location": {"type": "entity", "entity_kind": "location", "exclusive": True}}}
    world.records[root.id] = Record(root_frontmatter, root.body, root.source_path, serialize_record(root_frontmatter, root.body))
    character = {"schema": "wedl/v0.7", "kind": "character", "id": "char_00000000000000000000000000",
                 "title": "Spatial watch", "domain": "cast.spatial", "status": "canonical",
                 "initial_state": {"location": {"entity": "location:gate"}}}
    body = "# Spatial watch\n\nA watcher.\n"
    path = generated_path("story", "character", character["title"], character["id"], character)
    world.records[character["id"]] = Record(character, body, path, serialize_record(character, body))
    changes: dict[str, bytes | None] = {
        path.relative_to(ash_repo.root).as_posix(): None
        for path in (ash_repo.root / "story").rglob("*.md")
    }
    changes.update({record.source_path: record.raw_bytes for record in world.records.values()})
    ash_repo.commit_files(expected_head=ash_repo.head(), files=changes, message="seed v0.7 compatibility")
    request = {"action": "spatial.location.update", "expectedHead": ash_repo.head(),
               "idempotencyKey": "spatial-compat", "payload": deepcopy(UPDATE_CASES[1][1])}
    preview = preview_intent(ash_repo, request)
    assert preview["preview"]["valid"] is True
    result = apply_intent(ash_repo, request, confirmation_token_value=preview["preview"]["confirmationToken"])
    assert result["newHead"] == ash_repo.head()
    after_detail = show_entity(ash_repo, "location:gate")
    after_whereabouts = whereabouts(ash_repo, character["id"])
    assert after_detail["bodyMarkdown"] == world.records["location:gate"].body
    assert after_detail["locationContext"] == {"parent": None, "children": [], "outgoing": [], "incoming": []}
    assert after_whereabouts["characters"][0]["location"]["id"] == "location:gate"
    assert after_whereabouts["characters"][0]["journey"][0]["kind"] == "initial"
    assert after_whereabouts["locations"][0]["location"]["id"] == "location:gate"
