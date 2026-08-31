from __future__ import annotations

import json
from copy import deepcopy
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
from wedl.source import serialize_record, split_envelope
from wedl.transaction_recovery import Surface, TransactionJournal


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


def _v07_projection(snapshot):
    """The only authored source changes permitted by upgrade-v07."""

    projected = {}
    for path, data in snapshot.files.items():
        frontmatter, body = split_envelope(data, path)
        expected = deepcopy(frontmatter)
        expected["schema"] = "wedl/v0.7"
        if expected.get("kind") == "world":
            expected["capabilities"] = ["generational-core-v1", "spatial-core-v1"]
        else:
            expected.pop("capabilities", None)
        # v0.7 has one canonical containment spelling. This preserves the
        # authored hierarchy; it does not create a location edge.
        if expected.get("kind") == "location" and "parent" in expected:
            expected["parent_id"] = expected.pop("parent")
        projected[path] = (expected, body)
    return projected


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


def test_upgrade_v07_refuses_an_external_index_lock_without_removing_it_and_retries(tiny_repo) -> None:
    source = tiny_repo.snapshot().files
    request = _request(tiny_repo, "upgrade-v07", "external-index-lock")
    planned = preview(tiny_repo, request)
    index_lock = tiny_repo.root / ".git" / "index.lock"
    index_lock.write_text("external Git transaction\n", encoding="utf-8")

    with pytest.raises(RepositoryError, match="Git index is locked by another process"):
        apply(tiny_repo, {**request, "sourceSnapshotHash": planned["sourceSnapshotHash"]}, confirmation_token_value=planned["confirmationToken"])

    assert index_lock.read_text(encoding="utf-8") == "external Git transaction\n"
    assert tiny_repo.head() == planned["expectedHead"]
    assert tiny_repo.snapshot().files == source
    assert tiny_repo.ref(planned["backupRef"]) == planned["expectedHead"]
    index_lock.unlink()

    applied = apply(tiny_repo, {**request, "sourceSnapshotHash": planned["sourceSnapshotHash"]}, confirmation_token_value=planned["confirmationToken"])
    assert applied["status"] == "committed"


def test_upgrade_v07_rolls_back_source_and_head_when_real_index_publication_fails(tiny_repo, monkeypatch) -> None:
    source = tiny_repo.snapshot().files
    request = _request(tiny_repo, "upgrade-v07", "late-index-lock")
    planned = preview(tiny_repo, request)
    index_lock = tiny_repo.root / ".git" / "index.lock"
    actual_publish = tiny_repo._publish_real_index

    def interrupted_publish(*args, **kwargs):
        index_lock.write_text("external Git transaction\n", encoding="utf-8")
        return actual_publish(*args, **kwargs)

    monkeypatch.setattr(tiny_repo, "_publish_real_index", interrupted_publish)
    with pytest.raises(RepositoryError, match="index.lock"):
        apply(tiny_repo, {**request, "sourceSnapshotHash": planned["sourceSnapshotHash"]}, confirmation_token_value=planned["confirmationToken"])

    assert tiny_repo.head() == planned["expectedHead"]
    assert tiny_repo.snapshot().files == source
    assert index_lock.read_text(encoding="utf-8") == "external Git transaction\n"
    index_lock.unlink()
    monkeypatch.setattr(tiny_repo, "_publish_real_index", actual_publish)

    applied = apply(tiny_repo, {**request, "sourceSnapshotHash": planned["sourceSnapshotHash"]}, confirmation_token_value=planned["confirmationToken"])
    assert applied["status"] == "committed"


def test_migration_fixture_recovery_accepts_preindex_before_image_after_ref_rollback(tiny_repo) -> None:
    """Repository recovery remains compatible with migration's source fixture."""
    path = "story/world.md"; old = tiny_repo.head(); before = tiny_repo._real_index_entries([path])
    source_before = (tiny_repo.root / path).read_bytes()
    new = tiny_repo.commit_files(expected_head=old, files={path: source_before + b"\n"}, message="preindex fixture")
    after = tiny_repo._real_index_entries([path])
    tiny_repo._git(["update-ref", "refs/heads/master", old, new])
    tiny_repo._git(["update-index", "-z", "--index-info"], input_bytes=tiny_repo._index_info(before))
    (tiny_repo.root / path).write_bytes(source_before)
    journal = TransactionJournal.create(tiny_repo.root, ref="refs/heads/master", previous_head=old, committed_head=new,
                                        surfaces=[Surface(path, source_before, source_before + b"\n", "source")],
                                        index_before=before, index_after=after)
    journal.advance("ref_committed")
    tiny_repo.recover_authoring_transactions()
    assert tiny_repo.head() == old and (tiny_repo.root / path).read_bytes() == source_before
    assert tiny_repo._real_index_entries([path]) == before and not journal.path.exists()


def test_commit_files_captures_rollback_bytes_before_the_ref_compare_and_swap(tiny_repo, monkeypatch) -> None:
    source = tiny_repo.snapshot().files
    expected = tiny_repo.head()
    target = tiny_repo.root / "story" / "world.md"
    actual_read_bytes = Path.read_bytes

    def interrupted_read_bytes(path):
        if path == target:
            raise OSError("capture failed")
        return actual_read_bytes(path)

    monkeypatch.setattr(Path, "read_bytes", interrupted_read_bytes)
    with pytest.raises(OSError, match="capture failed"):
        tiny_repo.commit_files(expected_head=expected, files={"story/world.md": b"# never written\n"}, message="capture failure")
    monkeypatch.setattr(Path, "read_bytes", actual_read_bytes)

    assert tiny_repo.head() == expected
    assert tiny_repo.snapshot().files == source
    assert tiny_repo.status() == []
    assert tiny_repo._git(["diff", "--cached", "--quiet"], check=False).returncode == 0


def test_commit_files_handles_hostile_paths_with_nul_delimited_index_records(tiny_repo, monkeypatch) -> None:
    path = "story/portable-index-path.md"
    payloads = []
    actual_git = tiny_repo._git

    def observed_git(args, **kwargs):
        if list(args) == ["update-index", "-z", "--index-info"]:
            payloads.append(kwargs["input_bytes"])
        return actual_git(args, **kwargs)

    monkeypatch.setattr(tiny_repo, "_git", observed_git)
    commit = tiny_repo.commit_files(expected_head=tiny_repo.head(), files={path: b"# hostile path\n"}, message="hostile index path")

    assert tiny_repo.head() == commit
    assert tiny_repo.snapshot().files[path] == b"# hostile path\n"
    assert tiny_repo.status() == []
    assert tiny_repo._git(["diff", "--cached", "--quiet"], check=False).returncode == 0
    assert len(payloads) == 2 and all(payload.endswith(path.encode() + b"\0") for payload in payloads)
    assert Repository._index_info({"story/hostile\tpath\nname.md": "f" * 40}) == b"100644 " + b"f" * 40 + b"\tstory/hostile\tpath\nname.md\0"


def test_canonical_write_lock_is_never_auto_removed_without_operator_confirmation(tiny_repo) -> None:
    lock = tiny_repo.root / ".git" / "wedl-canonical-write.lock"
    lock.write_text("pid=999999\n", encoding="ascii")

    with pytest.raises(RepositoryError, match="another WEDL canonical write is in progress"):
        tiny_repo.commit_files(expected_head=tiny_repo.head(), files={"story/blocked.md": b"# blocked\n"}, message="blocked")

    assert lock.read_text(encoding="ascii") == "pid=999999\n"
    assert tiny_repo.status() == []
    lock.unlink()


def test_commit_files_rechecks_head_after_entering_the_canonical_write_boundary(tiny_repo, monkeypatch) -> None:
    expected = tiny_repo.head()
    actual = "f" * 40
    heads = iter((expected, actual))
    monkeypatch.setattr(tiny_repo, "head", lambda: next(heads))

    with pytest.raises(StaleRevision) as failure:
        tiny_repo.commit_files(expected_head=expected, files={"story/recheck.md": b"# never written\n"}, message="recheck")

    assert failure.value.details == {"expected": expected, "actual": actual}
    assert not (tiny_repo.root / "story" / "recheck.md").exists()
    assert Repository(tiny_repo.root).head() == expected


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


def test_upgrade_v07_is_lossless_capability_bound_and_idempotent(tiny_repo) -> None:
    original = tiny_repo.snapshot().files
    request = _request(tiny_repo, "upgrade-v07", "v07-one")
    planned, applied = _apply(tiny_repo, request)

    assert planned["targetCapabilities"] == ["generational-core-v1", "spatial-core-v1"]
    assert applied["targetCapabilities"] == planned["targetCapabilities"]
    migrated = tiny_repo.snapshot().files
    assert set(migrated) == set(original)
    for path in original:
        _before_frontmatter, before_body = split_envelope(original[path], path)
        after_frontmatter, after_body = split_envelope(migrated[path], path)
        assert after_body == before_body
        assert after_frontmatter["schema"] == "wedl/v0.7"
    assert tiny_repo.load_world().schema == "wedl/v0.7"
    replay = apply(tiny_repo, {**request, "sourceSnapshotHash": planned["sourceSnapshotHash"]}, confirmation_token_value=planned["confirmationToken"])
    assert replay["idempotentReplay"] is True
    no_op = preview(tiny_repo, _request(tiny_repo, "upgrade-v07", "v07-noop"))
    assert no_op["valid"] and no_op["noOp"]


@pytest.mark.parametrize("predecessor", ("upgrade-v03", "upgrade-v06"))
def test_upgrade_v07_accepts_each_pinned_legacy_release(tiny_repo, predecessor) -> None:
    # The staged modes remain available, but each homogeneous legacy release
    # reaches the one coordinated v0.7 source envelope.
    _apply(tiny_repo, _request(tiny_repo, predecessor, f"prepare-{predecessor}"))
    planned, applied = _apply(tiny_repo, _request(tiny_repo, "upgrade-v07", f"from-{predecessor}"))
    assert planned["valid"] and applied["status"] == "committed"
    assert tiny_repo.load_world().schema == "wedl/v0.7"


@pytest.mark.parametrize("predecessor", (None, "upgrade-v03", "upgrade-v06"))
def test_upgrade_v07_golden_projection_preserves_every_legacy_source_semantic(ash_repo, predecessor) -> None:
    """v0.3, v0.5, and v0.6 converge without rewriting authored content."""

    if predecessor is not None:
        _apply(ash_repo, _request(ash_repo, predecessor, f"prepare-golden-{predecessor}"))
    before = ash_repo.snapshot()
    expected = _v07_projection(before)

    planned, applied = _apply(ash_repo, _request(ash_repo, "upgrade-v07", f"golden-{predecessor or 'v03'}"))

    assert planned["valid"] and applied["status"] == "committed"
    after = ash_repo.snapshot()
    assert set(after.files) == set(expected)
    assert {path: split_envelope(data, path) for path, data in after.files.items()} == expected
    # The complete comparison includes paths/IDs, Markdown bodies and links,
    # provenance, thread declarations/memberships, and v0.6 chronology. The
    # archive's location parent graph is compared through parent_id above.


def test_upgrade_v07_keeps_an_existing_canonical_capability_list_on_noop(tiny_repo) -> None:
    _planned, _applied = _apply(tiny_repo, _request(tiny_repo, "upgrade-v07", "first-v07"))
    snapshot = tiny_repo.snapshot()
    files = {}
    for path, data in snapshot.files.items():
        frontmatter, body = split_envelope(data, path)
        if frontmatter.get("kind") == "world":
            frontmatter["capabilities"] = ["generational-core-v1"]
        files[path] = serialize_record(frontmatter, body)
    tiny_repo.commit_files(expected_head=tiny_repo.head(), files=files, message="fixture: canonical v07 subset")
    planned = preview(tiny_repo, _request(tiny_repo, "upgrade-v07", "v07-subset"))
    assert planned["valid"] and planned["noOp"]
    assert planned["targetCapabilities"] == ["generational-core-v1"]


def test_upgrade_v07_rejects_mixed_versions_before_writing(tiny_repo) -> None:
    snapshot = tiny_repo.snapshot()
    path = next(path for path in snapshot.files if path != "story/world.md")
    frontmatter, body = split_envelope(snapshot.files[path], path)
    frontmatter["schema"] = "wedl/v0.6"
    tiny_repo.commit_files(expected_head=tiny_repo.head(), files={path: serialize_record(frontmatter, body)}, message="fixture: mixed source")
    planned = preview(tiny_repo, _request(tiny_repo, "upgrade-v07", "mixed-v07"))
    assert planned["valid"] is False
    assert planned["diagnostics"][0]["code"] == "GEN-VERSION-001"
    assert tiny_repo.ref(planned["backupRef"]) is None
