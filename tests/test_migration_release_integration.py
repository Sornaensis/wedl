"""Real-cache release coverage for the local migration boundary."""

from __future__ import annotations

import gc
import shutil
from importlib import resources
from pathlib import Path
import subprocess
import time

import yaml
import pytest

from wedl import THREAD_SOURCE_SCHEMA, V04_SOURCE_SCHEMA
from wedl.changeset import apply as apply_changeset, preview as preview_changeset
from wedl.compiler import cache_readiness, compile_world, connect, require_database
from wedl.errors import SupersededSchemaError
from wedl.migration import PROTOCOL, apply, preview
from wedl.query import causality, entity_state, search_world, thread_catalog, thread_memberships
from wedl.repository import Repository
from wedl.source import serialize_record, split_envelope

THREAD = "thread_0123456789ABCDEFGHJKMNPQRS"


def _repo(tmp_path: Path) -> Repository:
    root = tmp_path / "release"
    (root / "story" / "characters").mkdir(parents=True)
    (root / "story" / "world.md").write_bytes(resources.files("wedl.data.ash_archive").joinpath("story/world.md").read_bytes())
    character = {"schema": "wedl/v0.3", "kind": "character", "id": "char_0VEEWF422PP4APZK5F7DPAWYWH", "title": "Release witness", "domain": "cast", "status": "canonical", "tags": [], "aliases": []}
    (root / "story" / "characters" / "witness.md").write_bytes(serialize_record(character, "# Release witness\n"))
    (root / ".gitignore").write_text(".wedl/\n", encoding="utf-8")
    for args in (("init", "-q"), ("config", "user.name", "wedl test"), ("config", "user.email", "wedl@test.invalid"), ("add", "-A"), ("commit", "-qm", "seed")):
        subprocess.run(["git", "-C", str(root), *args], check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return Repository(root)


def _request(repo: Repository, mode: str, key: str, **extra):
    return {"protocol": PROTOCOL, "mode": mode, "expectedHead": repo.head(), "idempotencyKey": key, **extra}


def _apply(repo: Repository, request: dict):
    plan = preview(repo, request); assert plan["valid"], plan["diagnostics"]
    return plan, apply(repo, {**request, "sourceSnapshotHash": plan["sourceSnapshotHash"]}, confirmation_token_value=plan["confirmationToken"])


def _real_compile(repository: Repository) -> dict:
    """Run the actual compiler, tolerating transient Windows cache replacement."""
    failure: PermissionError | None = None
    for _ in range(5):
        try:
            return compile_world(repository, force=True, profile_name="hybrid")
        except PermissionError as exc:
            failure = exc; gc.collect(); time.sleep(0.2)
    assert failure is not None
    raise failure


def _make_v04(repo: Repository) -> None:
    files = {}
    for path, data in repo.snapshot().files.items():
        fm, body = split_envelope(data, path); fm["schema"] = V04_SOURCE_SCHEMA
        if fm["kind"] == "world":
            fm.update({"continuities": [{"id": "continuity_main", "status": "primary", "timeline": "main", "strands": []}], "default_continuity": "continuity_main"})
        else: fm.update({"continuity": "continuity_main", "strands": []})
        files[path] = b"---\n" + yaml.safe_dump(fm, sort_keys=False).encode() + b"---\n" + body.encode()
    repo.commit_files(expected_head=repo.head(), files=files, message="legacy v04")


@pytest.mark.parametrize("mode", ("upgrade-v03", "recover-v04"))
def test_real_compile_release_path_and_rollback_boundary(tmp_path, mode, monkeypatch) -> None:
    repo = _repo(tmp_path)
    if mode == "recover-v04": _make_v04(repo)
    original = repo.snapshot().files
    if mode == "recover-v04":
        with pytest.raises(SupersededSchemaError): Repository(repo.root).load_world()
    plan, _ = _apply(repo, _request(repo, mode, f"real-{mode}"))
    fresh = Repository(repo.root); report = compile_world(fresh, force=True, profile_name="hybrid")
    assert report["searchDocumentCount"] > 0
    world = fresh.load_world(); assert world.schema == THREAD_SOURCE_SCHEMA and world.world_record.frontmatter["threads"] == []
    baseline = search_world(fresh, "archive", mode="hybrid", all_time=True, require_compiled=True)
    assert thread_catalog(fresh, require_compiled=True)["threads"] == []
    character = next(record for record in world if record.kind == "character")
    payload = {"protocol": "wedl-changeset/v1", "expectedHead": fresh.head(), "idempotencyKey": f"lane-{mode}", "summary": "release lane", "operations": [
        {"type": "entity.update", "entity": world.world_record.id, "frontmatterPatch": {"threads": [{"id": THREAD, "label": "Release lane"}]}},
        {"type": "entity.update", "entity": character.id, "frontmatterPatch": {"threadIds": [THREAD]}},
    ]}
    lane = preview_changeset(fresh, payload); assert lane["valid"]
    # The release gate compiles migration/restart caches for real below.  Keep
    # this changeset fixture from racing Windows' replacement of the just-read
    # SQLite file; it is only the grouping authoring step.
    monkeypatch.setattr("wedl.changeset.compile_world", lambda _repo: {"status": "deferred-to-restart"})
    apply_changeset(fresh, payload, confirmation_token_value=lane["confirmationToken"])
    del world, fresh
    gc.collect()
    restarted = Repository(repo.root); compile_world(restarted, force=True, profile_name="hybrid")
    assert thread_memberships(restarted, (character.id,), (THREAD,), require_compiled=True)["records"] == [{"recordId": character.id, "threadIds": [THREAD]}]
    assert thread_catalog(restarted, require_compiled=True)["threads"] == [{"id": THREAD, "label": "Release lane"}]
    assert search_world(restarted, "archive", mode="hybrid", all_time=True, require_compiled=True)["results"] == baseline["results"]
    selected = search_world(restarted, "release witness", mode="hybrid", all_time=True, thread_ids=(THREAD,), require_compiled=True)
    assert selected["results"] and {item["entityId"] for item in selected["results"]} == {character.id}
    unthreaded = search_world(restarted, "release witness", mode="hybrid", all_time=True, require_compiled=True)
    assert {item["entityId"] for item in unthreaded["results"]} >= {character.id}
    # The rollback is recompiled from a fresh process below; avoid a second
    # immediate Windows replacement race in the migration apply itself.
    monkeypatch.setattr("wedl.migration.compile_world", lambda _repo: {"status": "deferred-to-restart"})
    rollback, _ = _apply(restarted, _request(restarted, "rollback", f"rollback-{mode}", rollbackBackupRef=plan["backupRef"]))
    assert restarted.snapshot().files == original
    if mode == "recover-v04":
        with pytest.raises(SupersededSchemaError): Repository(restarted.root).load_world()
    else:
        root = restarted.root
        del restarted
        gc.collect()
        assert compile_world(Repository(root), force=True, profile_name="hybrid")["searchDocumentCount"] > 0


def test_v07_release_preserves_observable_state_causality_and_search_artifacts(ash_repo, monkeypatch, tmp_path) -> None:
    """Use the representative archive to compare real public/data-plane evidence."""
    compile_world(ash_repo, force=True, profile_name="hybrid")
    world = ash_repo.load_world()
    event = next(record for record in world if record.kind == "event" and record.status == "canonical")
    character = next(record for record in world if record.kind == "character" and record.status == "canonical")
    before_search = search_world(ash_repo, "archive", mode="hybrid", all_time=True, require_compiled=True)
    before_causal = causality(ash_repo, event.id, require_compiled=True)
    before_state = entity_state(ash_repo, character.id, 0, require_compiled=True)
    _world, database = require_database(ash_repo, require_compiled=True)
    with connect(database, True) as connection:
        before_documents = connection.execute("SELECT document_id,chunk_hash FROM search_document ORDER BY document_id").fetchall()
        before_vectors = connection.execute("SELECT input_hash,dimensions,vector FROM vector_embedding ORDER BY input_hash").fetchall()
        before_models = connection.execute("SELECT scope,model_id,provider,model_name,dimensions,normalized,corpus_hash,config_json FROM vector_model ORDER BY scope").fetchall()
        before_links = connection.execute("SELECT document_id,vector_id FROM document_vector ORDER BY document_id").fetchall()
    monkeypatch.setattr("wedl.migration.compile_world", lambda _repo: {"status": "deferred-to-restart"})
    _apply(ash_repo, _request(ash_repo, "upgrade-v07", "archive-observable"))
    del world, _world, database
    gc.collect()
    clone_root = tmp_path / "migrated-source"
    shutil.copytree(ash_repo.root / "story", clone_root / "story")
    fresh = Repository(clone_root); _real_compile(fresh)
    assert fresh.load_world().schema == "wedl/v0.7"
    assert fresh.load_world().world_record.frontmatter.get("threads", []) == []
    _world, database = require_database(fresh, require_compiled=True)
    with connect(database, True) as connection:
        documents = connection.execute("SELECT document_id,chunk_hash FROM search_document ORDER BY document_id").fetchall()
        assert documents == before_documents
        assert connection.execute("SELECT input_hash,dimensions,vector FROM vector_embedding ORDER BY input_hash").fetchall() == before_vectors
        assert connection.execute("SELECT scope,model_id,provider,model_name,dimensions,normalized,corpus_hash,config_json FROM vector_model ORDER BY scope").fetchall() == before_models
        assert connection.execute("SELECT document_id,vector_id FROM document_vector ORDER BY document_id").fetchall() == before_links
    assert search_world(fresh, "archive", mode="hybrid", all_time=True, require_compiled=True)["results"] == before_search["results"]
    assert causality(fresh, event.id, require_compiled=True)["edges"] == before_causal["edges"]
    assert entity_state(fresh, character.id, 0, require_compiled=True)["state"] == before_state["state"]


def test_v07_real_cache_rebuild_changes_the_capability_fingerprint_and_rollback_restores_it(ash_repo, monkeypatch, tmp_path) -> None:
    original = ash_repo.snapshot().files
    compile_world(ash_repo, force=True, profile_name="hybrid")
    _world, database = require_database(ash_repo, require_compiled=True)
    with connect(database, True) as connection:
        legacy_fingerprint = connection.execute("SELECT compiler_fingerprint FROM revision").fetchone()[0]

    monkeypatch.setattr("wedl.migration.compile_world", lambda _repo: {"status": "deferred-to-restart"})
    plan, applied = _apply(ash_repo, _request(ash_repo, "upgrade-v07", "v07-real-cache"))
    assert applied["status"] == "committed"
    restarted = Repository(ash_repo.root)
    assert cache_readiness(restarted)["state"] in {"stale", "incompatible"}
    migrated_root = tmp_path / "migrated-restart"
    shutil.copytree(ash_repo.root / "story", migrated_root / "story")
    migrated = Repository(migrated_root)
    compile_world(migrated, force=True, profile_name="hybrid")
    _world, database = require_database(migrated, require_compiled=True)
    with connect(database, True) as connection:
        schema, v07_fingerprint = connection.execute("SELECT source_schema,compiler_fingerprint FROM revision").fetchone()
    assert schema == "wedl/v0.7"
    assert v07_fingerprint != legacy_fingerprint

    _rollback, restored = _apply(restarted, _request(restarted, "rollback", "v07-real-cache-rollback", rollbackBackupRef=plan["backupRef"]))
    assert restored["status"] == "committed"
    assert restarted.snapshot().files == original
    assert cache_readiness(restarted)["state"] in {"stale", "incompatible"}
    restored_root = tmp_path / "restored-restart"
    shutil.copytree(ash_repo.root / "story", restored_root / "story")
    fresh = Repository(restored_root)
    compile_world(fresh, force=True, profile_name="hybrid")
    _world, database = require_database(fresh, require_compiled=True)
    with connect(database, True) as connection:
        schema, restored_fingerprint = connection.execute("SELECT source_schema,compiler_fingerprint FROM revision").fetchone()
    assert schema == "wedl/v0.3"
    assert restored_fingerprint == legacy_fingerprint
