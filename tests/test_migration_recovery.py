from __future__ import annotations

import json
from importlib import resources
from pathlib import Path
import subprocess

import yaml

import pytest

from wedl import THREAD_SOURCE_SCHEMA, V04_SOURCE_SCHEMA
from wedl.cli import main
from wedl.errors import ConfirmationMismatch, ConfirmationRequired, DirtyManagedTree, RepositoryError, StaleRevision, SupersededSchemaError
from wedl.migration import BACKUP_PREFIX, PROTOCOL, apply, preview
from wedl.repository import Repository
from wedl.source import serialize_record


@pytest.fixture(autouse=True)
def _fast_disposable_cache_rebuild(monkeypatch):
    """The migration tests assert the source transaction, not vector runtime."""

    monkeypatch.setattr("wedl.migration.compile_world", lambda _repository: {"rebuilt": True})


@pytest.fixture()
def tiny_repo(tmp_path: Path) -> Repository:
    """A valid two-record Git world keeps transaction tests focused."""

    root = tmp_path / "world"; (root / "story" / "characters").mkdir(parents=True)
    world = resources.files("wedl.data.ash_archive").joinpath("story/world.md")
    (root / "story" / "world.md").write_bytes(world.read_bytes())
    character = {"schema": "wedl/v0.3", "kind": "character", "id": "char_0VEEWF422PP4APZK5F7DPAWYWH", "title": "Migration witness", "domain": "cast", "status": "canonical", "tags": [], "aliases": []}
    (root / "story" / "characters" / "witness.md").write_bytes(serialize_record(character, "# Migration witness\n"))
    (root / ".gitignore").write_text(".wedl/\n", encoding="utf-8")
    for args in (("init", "-q"), ("config", "user.name", "wedl test"), ("config", "user.email", "wedl@test.invalid"), ("add", "story", ".gitignore"), ("commit", "-qm", "seed")):
        subprocess.run(["git", "-C", str(root), *args], check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return Repository(root)


def _request(repository, mode: str, key: str, **extra):
    return {"protocol": PROTOCOL, "mode": mode, "expectedHead": repository.head(), "idempotencyKey": key, **extra}


def _apply(repository, request):
    planned = preview(repository, request)
    assert planned["valid"], planned["diagnostics"]
    applied_request = {**request, "sourceSnapshotHash": planned["sourceSnapshotHash"]}
    return planned, apply(repository, applied_request, confirmation_token_value=planned["confirmationToken"])


def _commit_raw(repository, mutate) -> None:
    snapshot = repository.snapshot()
    files = {}
    for path, data in snapshot.files.items():
        from wedl.source import split_envelope

        frontmatter, body = split_envelope(data, path)
        mutate(path, frontmatter)
        if frontmatter.get("schema") == V04_SOURCE_SCHEMA:
            files[path] = b"---\n" + yaml.safe_dump(frontmatter, allow_unicode=True, sort_keys=False).encode("utf-8") + b"---\n" + body.encode("utf-8")
        else:
            files[path] = serialize_record(frontmatter, body)
    repository.commit_files(expected_head=repository.head(), files=files, message="fixture: legacy source")


def _make_v04(repository, *, mutate=None) -> None:
    def edit(path, frontmatter):
        frontmatter["schema"] = V04_SOURCE_SCHEMA
        if frontmatter.get("kind") == "world":
            frontmatter["continuities"] = [{"id": "continuity_main", "status": "primary", "timeline": "main", "strands": []}]
            frontmatter["default_continuity"] = "continuity_main"
        else:
            frontmatter["continuity"] = "continuity_main"; frontmatter["strands"] = []
        if mutate:
            mutate(path, frontmatter)
    _commit_raw(repository, edit)


def test_upgrade_v03_preview_apply_is_confirmed_idempotent_and_leaves_memberships_omitted(tiny_repo) -> None:
    original = tiny_repo.snapshot().files
    planned, applied = _apply(tiny_repo, _request(tiny_repo, "upgrade-v03", "upgrade-one"))

    assert planned["protocol"] == PROTOCOL
    assert planned["files"]
    assert planned["backupRef"].startswith(BACKUP_PREFIX)
    assert applied["status"] == "committed"
    assert tiny_repo.ref(planned["backupRef"]) == planned["expectedHead"]
    world = tiny_repo.load_world()
    assert world.schema == THREAD_SOURCE_SCHEMA
    assert world.world_record.frontmatter["threads"] == []
    assert all("threads" not in record.frontmatter for record in world if record.kind != "world")
    assert tiny_repo.snapshot(planned["backupRef"]).files == original
    replay = apply(tiny_repo, {**_request(tiny_repo, "upgrade-v03", "upgrade-one"), "expectedHead": planned["expectedHead"], "sourceSnapshotHash": planned["sourceSnapshotHash"]}, confirmation_token_value=planned["confirmationToken"])
    assert replay["newHead"] == applied["newHead"]
    assert replay["idempotentReplay"] is True

    no_op = preview(tiny_repo, _request(tiny_repo, "upgrade-v03", "upgrade-two"))
    assert no_op["valid"] and no_op["noOp"] and no_op["files"] == []


def test_upgrade_release_path_preserves_one_global_cursor_and_forward_rollback(tiny_repo) -> None:
    """The release path changes schema/grouping metadata, not shared-world facts."""

    original = tiny_repo.snapshot().files
    before = tiny_repo.load_world()
    before_time = before.world_record.frontmatter.get("current_time")
    upgrade, _ = _apply(tiny_repo, _request(tiny_repo, "upgrade-v03", "release-upgrade"))
    migrated = tiny_repo.load_world()
    assert migrated.schema == THREAD_SOURCE_SCHEMA
    assert migrated.world_record.frontmatter.get("current_time") == before_time
    assert migrated.world_record.frontmatter["threads"] == []
    assert all("threads" not in record.frontmatter for record in migrated if record.kind != "world")

    rollback_request = _request(tiny_repo, "rollback", "release-rollback", rollbackBackupRef=upgrade["backupRef"])
    _rollback, receipt = _apply(tiny_repo, rollback_request)
    assert receipt["status"] == "committed"
    assert tiny_repo.snapshot().files == original
    assert tiny_repo.load_world().schema == "wedl/v0.3"


def test_invalid_candidate_never_creates_a_backup_or_partial_source(tiny_repo) -> None:
    def multitimeline(_path, frontmatter):
        if frontmatter.get("kind") == "world":
            frontmatter["timelines"].append({"id": "later", "label": "Later"})
    _commit_raw(tiny_repo, multitimeline)
    invalid = preview(tiny_repo, _request(tiny_repo, "upgrade-v03", "invalid"))
    assert invalid["valid"] is False
    assert tiny_repo.ref(invalid["backupRef"]) is None

def test_commit_failure_keeps_source_atomic_and_preserves_its_backup(tiny_repo, monkeypatch) -> None:
    source = tiny_repo.snapshot().files
    request = _request(tiny_repo, "upgrade-v03", "commit-failure")
    planned = preview(tiny_repo, request)
    monkeypatch.setattr(tiny_repo, "commit_files", lambda **_kwargs: (_ for _ in ()).throw(RuntimeError("commit failed")))
    with pytest.raises(RuntimeError, match="commit failed"):
        apply(tiny_repo, {**request, "sourceSnapshotHash": planned["sourceSnapshotHash"]}, confirmation_token_value=planned["confirmationToken"])
    assert tiny_repo.snapshot().files == source
    assert tiny_repo.ref(planned["backupRef"]) == planned["expectedHead"]


def test_compile_failure_leaves_one_valid_forward_source_commit_and_backup(tiny_repo, monkeypatch) -> None:
    request = _request(tiny_repo, "upgrade-v03", "compile-failure")
    planned = preview(tiny_repo, request)
    monkeypatch.setattr("wedl.migration.compile_world", lambda _repository: (_ for _ in ()).throw(RuntimeError("compile failed")))
    with pytest.raises(RuntimeError, match="compile failed"):
        apply(tiny_repo, {**request, "sourceSnapshotHash": planned["sourceSnapshotHash"]}, confirmation_token_value=planned["confirmationToken"])
    assert tiny_repo.load_world().schema == THREAD_SOURCE_SCHEMA
    assert tiny_repo.ref(planned["backupRef"]) == planned["expectedHead"]


def test_upgrade_v03_blocks_multi_timeline_mixed_dirty_stale_and_unconfirmed(tiny_repo) -> None:
    def multitimeline(_path, frontmatter):
        if frontmatter.get("kind") == "world":
            frontmatter["timelines"].append({"id": "later", "label": "Later", "origin": {"tick": 0, "label": "Later"}})
    _commit_raw(tiny_repo, multitimeline)
    blocked = preview(tiny_repo, _request(tiny_repo, "upgrade-v03", "multi"))
    assert blocked["valid"] is False
    assert blocked["diagnostics"][0]["code"] == "WDL-MIG-003"

    stale = _request(tiny_repo, "upgrade-v03", "stale"); stale["expectedHead"] = "0" * 40
    with pytest.raises(StaleRevision): preview(tiny_repo, stale)
    dirty_path = tiny_repo.root / "story" / "world.md"; dirty_path.write_text(dirty_path.read_text(encoding="utf-8") + "\n", encoding="utf-8")
    with pytest.raises(DirtyManagedTree): preview(tiny_repo, _request(tiny_repo, "upgrade-v03", "dirty"))


def test_apply_binds_source_hash_and_confirmation(tiny_repo) -> None:
    request = _request(tiny_repo, "upgrade-v03", "bound")
    planned = preview(tiny_repo, request)
    bound = {**request, "sourceSnapshotHash": planned["sourceSnapshotHash"]}
    with pytest.raises(ConfirmationRequired):
        apply(tiny_repo, bound)
    with pytest.raises(ConfirmationMismatch):
        apply(tiny_repo, bound, confirmation_token_value="wrong")
    with pytest.raises(StaleRevision):
        preview(tiny_repo, {**request, "sourceSnapshotHash": "0" * 64})
    with pytest.raises(RepositoryError, match="sourceSnapshotHash"):
        apply(tiny_repo, request, confirmation_token_value=planned["confirmationToken"])


def test_raw_v04_recovery_never_uses_normal_loader_and_maps_only_scalar_horizon(tiny_repo) -> None:
    _make_v04(tiny_repo, mutate=lambda _path, fm: fm.update({"horizon": 44}) if fm.get("kind") == "world" else None)
    with pytest.raises(SupersededSchemaError): tiny_repo.load_world()

    planned, applied = _apply(tiny_repo, _request(tiny_repo, "recover-v04", "recover-one"))
    assert applied["status"] == "committed"
    world = tiny_repo.load_world()
    assert world.schema == THREAD_SOURCE_SCHEMA
    assert world.world_record.frontmatter["current_time"] == {"timeline": "main", "tick": 44, "order": 0}
    assert "continuities" not in world.world_record.frontmatter
    assert all("continuity" not in record.frontmatter and "strands" not in record.frontmatter for record in world)
    assert planned["backupRef"].startswith(BACKUP_PREFIX)


@pytest.mark.parametrize("field,value", [("fork", {"at": 1}), ("retcon", "event_x"), ("sync", True), ("handoff", "scene_x"), ("vector", [1]), ("synchronized_horizon", 2), ("presentation", "flashback")])
def test_v04_recovery_blocks_every_withdrawn_topology_field(tiny_repo, field, value) -> None:
    _make_v04(tiny_repo, mutate=lambda _path, fm: fm.update({field: value}) if fm.get("kind") == "world" else None)
    planned = preview(tiny_repo, _request(tiny_repo, "recover-v04", f"blocked-{field}"))
    assert planned["valid"] is False
    assert planned["diagnostics"][0]["code"] == "WDL-MIG-012"


def test_recovery_blocks_topology_mismatches_and_never_reads_archived_vectors(tiny_repo) -> None:
    _make_v04(tiny_repo, mutate=lambda _path, fm: fm["continuities"][0].update({"status": "alternate"}) if fm.get("kind") == "world" else None)
    planned = preview(tiny_repo, _request(tiny_repo, "recover-v04", "alternate"))
    assert planned["valid"] is False
    assert planned["diagnostics"][0]["code"] == "WDL-MIG-014"
    assert not (tiny_repo.root / ".wedl").exists(), "raw audit cannot create a cache or inspect archived vectors"


def test_rollback_is_a_forward_confirmed_commit_restoring_the_backup_source(tiny_repo) -> None:
    original = tiny_repo.snapshot().files
    planned, applied = _apply(tiny_repo, _request(tiny_repo, "upgrade-v03", "upgrade-for-rollback"))
    rollback_request = _request(tiny_repo, "rollback", "rollback-one", rollbackBackupRef=planned["backupRef"])
    rollback_preview, rollback = _apply(tiny_repo, rollback_request)

    assert rollback["status"] == "committed"
    assert rollback["newHead"] != applied["newHead"]
    assert tiny_repo.snapshot().files == original
    assert rollback_preview["backupRef"] != planned["backupRef"]
    assert tiny_repo.ref(rollback_preview["backupRef"]) == applied["newHead"]


def test_rollback_preview_binds_the_backup_oid_and_bytes(tiny_repo) -> None:
    planned, _applied = _apply(tiny_repo, _request(tiny_repo, "upgrade-v03", "upgrade-bound-backup"))
    request = _request(tiny_repo, "rollback", "rollback-bound", rollbackBackupRef=planned["backupRef"])
    rollback_preview = preview(tiny_repo, request)
    assert rollback_preview["rollbackBackupOid"] == tiny_repo.ref(planned["backupRef"])
    before = tiny_repo.head()
    tiny_repo._git(["update-ref", planned["backupRef"], before])
    bound = {**request, "sourceSnapshotHash": rollback_preview["sourceSnapshotHash"]}
    with pytest.raises((StaleRevision, ConfirmationMismatch)):
        apply(tiny_repo, bound, confirmation_token_value=rollback_preview["confirmationToken"])
    assert tiny_repo.head() == before


@pytest.mark.parametrize("suffix", ("~1", "^{tree}", "@{1}"))
def test_rollback_rejects_revision_expressions_before_resolving_backup(tiny_repo, suffix) -> None:
    planned, _ = _apply(tiny_repo, _request(tiny_repo, "upgrade-v03", f"literal-backup-{suffix}"))
    request = _request(tiny_repo, "rollback", f"literal-rollback-{suffix}", rollbackBackupRef=planned["backupRef"] + suffix)
    with pytest.raises(RepositoryError, match="rollbackBackupRef"):
        preview(tiny_repo, request)


def test_preview_does_not_expose_internal_compile_flag(tiny_repo) -> None:
    planned = preview(tiny_repo, _request(tiny_repo, "upgrade-v03", "public-envelope"))
    assert "_skipCompile" not in planned


def test_recover_v04_rollback_restores_raw_quarantined_backup_without_compile(tiny_repo, monkeypatch) -> None:
    _make_v04(tiny_repo)
    original = tiny_repo.snapshot().files
    recovered, _ = _apply(tiny_repo, _request(tiny_repo, "recover-v04", "recover-rollback"))
    called = []
    monkeypatch.setattr("wedl.migration.compile_world", lambda _repo: called.append(True))
    rollback_request = _request(tiny_repo, "rollback", "restore-v04", rollbackBackupRef=recovered["backupRef"])
    rollback_preview, rollback = _apply(tiny_repo, rollback_request)
    assert rollback_preview["rollbackBackupSourceSnapshotHash"]
    assert rollback["compileSkipped"] is True and rollback["compile"] is None and called == []
    assert tiny_repo.snapshot().files == original
    with pytest.raises(SupersededSchemaError):
        tiny_repo.load_world()


def test_receipt_replay_requires_complete_identity_and_atomic_receipt_write(tiny_repo, monkeypatch) -> None:
    request = _request(tiny_repo, "upgrade-v03", "receipt-integrity")
    planned, applied = _apply(tiny_repo, request)
    with pytest.raises(RepositoryError, match="sourceSnapshotHash"):
        apply(tiny_repo, request, confirmation_token_value=planned["confirmationToken"])
    receipt_path = tiny_repo.root / ".wedl" / "migration-receipts.json"
    previous = receipt_path.read_bytes()
    request2 = _request(tiny_repo, "rollback", "receipt-write-failure", rollbackBackupRef=applied["backupRef"])
    planned2 = preview(tiny_repo, request2)
    monkeypatch.setattr("wedl.migration.atomic_write", lambda _path, _data: (_ for _ in ()).throw(OSError("receipt write failed")))
    with pytest.raises(OSError, match="receipt write failed"):
        apply(tiny_repo, {**request2, "sourceSnapshotHash": planned2["sourceSnapshotHash"]}, confirmation_token_value=planned2["confirmationToken"])
    assert receipt_path.read_bytes() == previous
    assert tiny_repo.snapshot().files == tiny_repo.snapshot(applied["backupRef"]).files


def test_cli_preview_and_apply_use_the_same_local_protocol(tiny_repo, capsys) -> None:
    head = tiny_repo.head()
    assert main(["--compact", "migrate", "preview", "--repo", str(tiny_repo.root), "--mode", "upgrade-v03", "--expected-head", head, "--idempotency-key", "cli-one"]) == 0
    planned = json.loads(capsys.readouterr().out)
    assert planned["protocol"] == PROTOCOL
    assert main(["--compact", "migrate", "apply", "--repo", str(tiny_repo.root), "--mode", "upgrade-v03", "--expected-head", head, "--source-snapshot-hash", planned["sourceSnapshotHash"], "--idempotency-key", "cli-one", "--confirm", planned["confirmationToken"]]) == 0
    assert json.loads(capsys.readouterr().out)["status"] == "committed"
