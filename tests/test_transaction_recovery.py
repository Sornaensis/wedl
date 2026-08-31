from __future__ import annotations

from pathlib import Path
import os
import subprocess
import sys
import time

import pytest
import wedl.transaction_recovery as transaction_recovery

from wedl.errors import RepositoryError
from wedl.repository import Repository
from wedl.transaction_recovery import Surface, TransactionJournal


def _repository(tmp_path: Path) -> Repository:
    root = tmp_path / "world"
    (root / "story").mkdir(parents=True)
    (root / "story" / "world.md").write_bytes(b"before\n")
    (root / ".gitignore").write_text(".wedl/\n", encoding="utf-8")
    for args in (("init", "-q"), ("config", "user.name", "wedl test"), ("config", "user.email", "wedl@test.invalid"), ("add", "story", ".gitignore"), ("commit", "-qm", "seed")):
        subprocess.run(["git", "-C", str(root), *args], check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return Repository(root)


def test_registered_surfaces_are_durable_idempotent_and_exact(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    receipt = root / ".wedl" / "receipts.json"; receipt.parent.mkdir()
    receipt.write_bytes(b"old receipt")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surface(".wedl/receipts.json", capture_before=True, after=b"new receipt")
    journal.register_surface(".wedl/revisions/new.sqlite", before=None, after=b"compiled")
    journal.publish_surfaces(); journal.publish_surfaces()
    reloaded = TransactionJournal.load(root, journal.path)
    assert receipt.read_bytes() == b"new receipt"
    assert (root / ".wedl/revisions/new.sqlite").read_bytes() == b"compiled"
    reloaded.restore_surfaces(); reloaded.restore_surfaces()
    assert receipt.read_bytes() == b"old receipt"
    assert not (root / ".wedl/revisions/new.sqlite").exists()
    reloaded.finish(); reloaded.cleanup()
    assert TransactionJournal.pending(root) == ()


def test_registered_surface_ownership_loss_is_fail_closed(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40, surfaces=[Surface(".wedl/cache.bin", None, b"ours")])
    target = root / ".wedl/cache.bin"; target.parent.mkdir(exist_ok=True); target.write_bytes(b"external")
    with pytest.raises(RepositoryError, match="lost surface ownership"):
        journal.publish_surfaces()
    assert target.read_bytes() == b"external"
    assert journal.path.exists()


def test_recovery_rolls_forward_only_when_committed_ref_is_still_owned(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    old = repository.head()
    new = repository.commit_files(expected_head=old, files={"story/world.md": b"after\n"}, message="after")
    # Recreate the crash boundary: new ref, but source materialization never
    # happened.  The journal contains the only safe after image.
    (repository.root / "story/world.md").write_bytes(b"before\n")
    old_index = {"story/world.md": {"mode": "100644", "stage": 0, "blob": repository._blob_at(old, "story/world.md")}}
    journal = TransactionJournal.create(
        repository.root,
        ref="refs/heads/master",
        previous_head=old,
        committed_head=new,
        surfaces=[Surface("story/world.md", b"before\n", b"after\n")],
        index_before=old_index,
        index_after=repository._real_index_entries(["story/world.md"]),
    )
    repository.recover_authoring_transactions()
    assert repository.head() == new
    assert (repository.root / "story/world.md").read_bytes() == b"after\n"
    assert not journal.path.exists()
    repository.recover_authoring_transactions()


def test_recovery_restores_partial_write_when_ref_never_advanced(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    old = repository.head()
    target = repository.root / "story/world.md"; target.write_bytes(b"after\n")
    journal = TransactionJournal.create(
        repository.root,
        ref="refs/heads/master",
        previous_head=old,
        committed_head="f" * 40,
        surfaces=[Surface("story/world.md", b"before\n", b"after\n")],
    )
    repository.recover_authoring_transactions()
    assert repository.head() == old
    assert target.read_bytes() == b"before\n"
    assert not journal.path.exists()


def test_recovery_never_overwrites_an_intervening_ref_or_file(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    old = repository.head()
    target = repository.root / "story/world.md"; target.write_bytes(b"external\n")
    journal = TransactionJournal.create(
        repository.root,
        ref="refs/heads/master",
        previous_head="0" * 40,
        committed_head="f" * 40,
        surfaces=[Surface("story/world.md", b"before\n", b"after\n")],
    )
    with pytest.raises(RepositoryError, match="lost ref ownership"):
        repository.recover_authoring_transactions()
    assert target.read_bytes() == b"external\n"
    assert journal.path.exists()


def test_journal_rejects_tampering_and_freezes_generic_enrollment(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surface(".wedl/receipt.json", after=b"receipt", role="receipt")
    journal.advance("ref_committed")
    with pytest.raises(RepositoryError, match="enrollment is frozen"):
        journal.register_surface(".wedl/late.sqlite", after=b"late", role="cache")
    payload = journal.path.read_text(encoding="utf-8").replace("refs/heads/main", "../foreign")
    journal.path.write_text(payload, encoding="utf-8")
    with pytest.raises(RepositoryError, match="invalid WEDL transaction journal"):
        TransactionJournal.load(root, journal.path)


@pytest.mark.parametrize("path", (".git/config", ".wedl/transactions/foreign.json", "../outside"))
def test_generic_surfaces_cannot_claim_git_or_journal_internals(tmp_path: Path, path: str) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    with pytest.raises(RepositoryError):
        journal.register_surface(path, after=b"forbidden", role="cache")


def test_casefold_aliases_cannot_be_enrolled_twice(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.register_surface(".wedl/Cache.bin", after=b"one")
    with pytest.raises(RepositoryError, match="already registered"):
        journal.register_surface(".wedl/cache.bin", after=b"two")


def test_pre_ref_enrollment_survives_commit_for_downstream_publication(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    captured: list[TransactionJournal] = []

    def enroll(journal: TransactionJournal) -> None:
        journal.register_surface(".wedl/receipt.json", after=b"receipt", role="receipt")
        captured.append(journal)

    repository.commit_files(
        expected_head=repository.head(), files={"story/world.md": b"after\n"}, message="after",
        transaction_enroll=enroll, retain_transaction=True,
    )
    assert len(captured) == 1 and captured[0].path.exists()
    repository.recover_authoring_transactions()
    assert (repository.root / ".wedl/receipt.json").read_bytes() == b"receipt"
    assert not captured[0].path.exists()


def test_source_interleaving_before_no_clobber_publish_fails_closed(tmp_path: Path) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(
        root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
        surfaces=[Surface("story/world.md", b"before", b"after", "source")],
    )
    def interleave(mutation: str) -> None:
        if mutation == "surface-publish-create":
            target.write_bytes(b"external")

    transaction_recovery.set_mutation_hook(interleave)
    try:
        with pytest.raises(RepositoryError, match="lost surface ownership"):
            journal.publish_surfaces()
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert target.read_bytes() == b"external"
    assert journal.path.exists()


def test_claim_gap_after_real_mutation_recovers_from_private_backup(tmp_path: Path) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
        surfaces=[Surface("story/world.md", b"before", b"after", "source")])

    def crash(mutation: str) -> None:
        if mutation == "surface-publish-create":
            raise RuntimeError("injected crash")

    transaction_recovery.set_mutation_hook(crash)
    try:
        with pytest.raises(RuntimeError, match="injected crash"):
            journal.publish_surfaces()
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert not target.exists() and journal.path.exists()
    journal.publish_surfaces()
    assert target.read_bytes() == b"after"
    journal.restore_surfaces()
    assert target.read_bytes() == b"before"


@pytest.mark.parametrize("after", [b"after", None])
def test_surface_backup_is_sealed_before_public_removal_and_restart_recovers(tmp_path: Path, after: bytes | None) -> None:
    """The exact backup-create crash window never loses the public before image."""
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40,
                                        committed_head="1" * 40,
                                        surfaces=[Surface("story/world.md", b"before", after, "source")])

    def crash(name: str) -> None:
        if name == "surface-backup-create":
            raise RuntimeError("crash before private backup claim")

    transaction_recovery.set_mutation_hook(crash)
    try:
        with pytest.raises(RuntimeError, match="private backup"):
            journal.publish_surfaces()
    finally:
        transaction_recovery.set_mutation_hook(None)
    backup = journal.path.parent / journal.transaction_id / "backups" / "0000.before"
    assert target.read_bytes() == b"before" and not backup.exists()
    reloaded = TransactionJournal.load(root, journal.path)
    reloaded.publish_surfaces()
    if after is None:
        assert not target.exists()
    else:
        assert target.read_bytes() == after
    reloaded.restore_surfaces()
    assert target.read_bytes() == b"before"


@pytest.mark.parametrize("after", (b"after\n", None), ids=("update", "delete"))
@pytest.mark.parametrize("roll_forward", (True, False), ids=("roll-forward", "roll-back"))
def test_real_restart_after_surface_backup_checkpoint_converges_update_and_delete(
    tmp_path: Path, after: bytes | None, roll_forward: bool,
) -> None:
    """An interrupted authenticated before-claim has no unrecoverable gap."""
    repository = _repository(tmp_path)
    path = "story/world.md"; target = repository.root / path
    old = repository.head()
    before_index = repository._real_index_entries([path])
    new = repository.commit_files(expected_head=old, files={path: after}, message="candidate")
    after_index = repository._real_index_entries([path])
    # Reconstruct exactly the source state at the backup-create checkpoint: the
    # new ref may be authoritative (roll forward) or have been CAS-rolled back
    # (roll back), but the old public bytes remain until the sealed claim exists.
    target.write_bytes(b"before\n")
    if not roll_forward:
        repository._git(["update-ref", "refs/heads/master", old, new])
        repository._git(["update-index", "-z", "--index-info"], input_bytes=repository._index_info(before_index))
    journal = TransactionJournal.create(
        repository.root, ref="refs/heads/master", previous_head=old, committed_head=new,
        surfaces=[Surface(path, b"before\n", after, "source")],
        index_before=before_index, index_after=after_index,
    )

    def crash(name: str) -> None:
        if name == "surface-backup-create":
            raise RuntimeError("crash at durable source backup boundary")

    transaction_recovery.set_mutation_hook(crash)
    try:
        with pytest.raises(RuntimeError, match="durable source backup"):
            journal.publish_surfaces()
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert target.read_bytes() == b"before\n" and journal.path.exists()

    restarted = Repository(repository.root)
    restarted.recover_authoring_transactions()
    if roll_forward:
        assert restarted.head() == new
        assert (not target.exists()) if after is None else target.read_bytes() == after
        assert restarted._real_index_entries([path]) == after_index
    else:
        assert restarted.head() == old and target.read_bytes() == b"before\n"
        assert restarted._real_index_entries([path]) == before_index
    assert not journal.path.exists()


@pytest.mark.parametrize("artifact_kind,same_bytes", [("before", True), ("before", False), ("published", True), ("published", False), ("staging", True), ("staging", False)])
def test_cleanup_final_cas_preserves_substituted_private_artifacts(tmp_path: Path, artifact_kind: str, same_bytes: bool) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40,
                                        committed_head="1" * 40,
                                        surfaces=[Surface("story/world.md", b"before", b"after", "source")])
    journal.publish_surfaces(); journal.restore_surfaces()
    staging = journal.write_staging_artifact("private.bin", b"staging"); journal.seal_staging_artifact("private.bin")
    journal.finish()
    candidates = {
        "before": journal.path.parent / journal.transaction_id / "backups" / "0000.before",
        "published": journal.path.parent / journal.transaction_id / "backups" / "0000.published",
        "staging": staging,
    }
    artifact = candidates[artifact_kind]
    original = artifact.read_bytes()
    checkpoint = {"before": "backup-cleanup", "published": "published-cleanup", "staging": "staging-cleanup"}[artifact_kind]

    def substitute(name: str) -> None:
        if name == checkpoint:
            artifact.unlink(); artifact.write_bytes(original if same_bytes else b"external private replacement")

    transaction_recovery.set_mutation_hook(substitute)
    try:
        with pytest.raises(RepositoryError, match="changed ownership"):
            journal.cleanup()
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert artifact.exists() and artifact.read_bytes() == (original if same_bytes else b"external private replacement")


@pytest.mark.parametrize("same_bytes", (True, False))
def test_cleanup_unlink_inode_claim_rejects_late_private_substitution(tmp_path: Path, same_bytes: bool) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40,
                                        committed_head="1" * 40,
                                        surfaces=[Surface("story/world.md", b"before", b"after", "source")])
    journal.publish_surfaces(); journal.restore_surfaces(); journal.finish()
    backup = journal.path.parent / journal.transaction_id / "backups" / "0000.before"
    original = backup.read_bytes()

    def substitute(name: str) -> None:
        if name == "backup-cleanup-unlink":
            backup.unlink(); backup.write_bytes(original if same_bytes else b"external replacement")

    transaction_recovery.set_mutation_hook(substitute)
    try:
        with pytest.raises(RepositoryError, match="backup changed ownership"):
            journal.cleanup()
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert backup.exists() and backup.read_bytes() == (original if same_bytes else b"external replacement")


def test_surface_modes_and_journal_reparse_containment(tmp_path: Path) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
        surfaces=[Surface("story/world.md", b"before", b"after", "source", 0o744, 0o600)])
    journal.publish_surfaces()
    assert target.read_bytes() == b"after"
    if os.name != "nt":
        assert target.stat().st_mode & 0o777 == 0o600
    journal.restore_surfaces()
    assert target.read_bytes() == b"before"
    if os.name != "nt":
        assert target.stat().st_mode & 0o777 == 0o744
    journal.finish(); journal.cleanup()

    outside = tmp_path / "outside"; outside.mkdir()
    journal_root = root / ".wedl" / "transactions"; journal_root.parent.mkdir()
    try:
        journal_root.symlink_to(outside, target_is_directory=True)
    except OSError:
        pytest.skip("Windows test host does not permit symlink fixtures")
    with pytest.raises(RepositoryError, match="reparse point"):
        TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)


def test_recovery_preserves_intervening_real_index_entry(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    old = repository.head()
    new = repository.commit_files(expected_head=old, files={"story/world.md": b"after\n"}, message="after")
    expected_after = repository._real_index_entries(["story/world.md"])
    before = {"story/world.md": {"mode": "100644", "stage": 0, "blob": repository._blob_at(old, "story/world.md")}}
    journal = TransactionJournal.create(repository.root, ref="refs/heads/master", previous_head=old, committed_head=new,
        surfaces=[Surface("story/world.md", b"after\n", b"after\n", "source")], index_before=before, index_after=expected_after)
    external = repository._git(["hash-object", "-w", "--stdin"], input_bytes=b"external\n").stdout.decode().strip()
    repository._git(["update-index", "-z", "--index-info"], input_bytes=repository._index_info({"story/world.md": {"mode": "100644", "stage": 0, "blob": external}}))
    with pytest.raises(RepositoryError, match="real-index ownership"):
        repository.recover_authoring_transactions()
    assert repository._real_index_entries(["story/world.md"])["story/world.md"]["blob"] == external
    assert journal.path.exists()


def test_recovery_uses_recorded_ref_and_reclaims_only_owned_dead_transaction_lock(tmp_path: Path, monkeypatch) -> None:
    repository = _repository(tmp_path)
    old = repository.head()
    new = repository.commit_files(expected_head=old, files={"story/world.md": b"after\n"}, message="after")
    subprocess.run(["git", "-C", str(repository.root), "branch", "recovery-side", old], check=True)
    journal = TransactionJournal.create(
        repository.root, ref="refs/heads/recovery-side", previous_head=old, committed_head=new,
        surfaces=[Surface("story/world.md", b"after\n", b"after\n")],
        index_before=repository._real_index_entries(["story/world.md"]),
        index_after=repository._real_index_entries(["story/world.md"]),
    )
    lock = repository.root / ".git" / "wedl-canonical-write.lock"
    token = "11111111-1111-1111-1111-111111111111"
    lock.write_text(f"pid=999999\ntoken={token}\n", encoding="ascii")
    journal.bind_lock(lock, pid=999999, token=token)
    monkeypatch.setattr(transaction_recovery, "_process_dead", lambda _pid: True)
    repository.recover_authoring_transactions()
    assert repository.head() == new
    assert repository.ref("refs/heads/recovery-side") == old
    assert not journal.path.exists()


def test_partial_canonical_lock_bind_is_never_reclaimed(tmp_path: Path, monkeypatch) -> None:
    repository = _repository(tmp_path)
    lock = repository.root / ".git" / "wedl-canonical-write.lock"
    token = "11111111-1111-1111-1111-111111111111"
    payload = f"pid=999999\ntoken={token}\n"
    lock.write_text(payload, encoding="ascii")
    monkeypatch.setattr(transaction_recovery.os, "kill", lambda _pid, _signal: (_ for _ in ()).throw(ProcessLookupError()))
    with pytest.raises(RepositoryError, match="another WEDL canonical write"):
        repository.recover_authoring_transactions()
    assert lock.read_text(encoding="ascii") == payload


def test_real_index_lock_claim_preserves_external_lock_and_retries(tmp_path: Path, monkeypatch) -> None:
    repository = _repository(tmp_path)
    path = "story/world.md"; before = repository._real_index_entries([path])
    blob = repository._git(["hash-object", "-w", "--stdin"], input_bytes=b"after\n").stdout.decode().strip()
    after = {path: {"mode": "100644", "stage": 0, "blob": blob}}
    journal = TransactionJournal.create(repository.root, ref="refs/heads/master", previous_head=repository.head(), committed_head="1" * 40,
        index_before=before, index_after=after)
    lock = repository.root / ".git" / "index.lock"; lock.write_bytes(b"external Git transaction")
    with pytest.raises(RepositoryError, match="index.lock"):
        repository._publish_real_index(journal, expected=before, desired=after)
    assert lock.read_bytes() == b"external Git transaction"
    lock.unlink()
    repository._publish_real_index(journal, expected=before, desired=after)
    assert repository._real_index_entries([path]) == after


def test_real_index_lock_identity_loss_preserves_replacement(tmp_path: Path, monkeypatch) -> None:
    repository = _repository(tmp_path)
    path = "story/world.md"; before = repository._real_index_entries([path])
    blob = repository._git(["hash-object", "-w", "--stdin"], input_bytes=b"after\n").stdout.decode().strip()
    after = {path: {"mode": "100644", "stage": 0, "blob": blob}}
    journal = TransactionJournal.create(repository.root, ref="refs/heads/master", previous_head=repository.head(), committed_head="1" * 40,
        index_before=before, index_after=after)
    actual_claim = journal.claim_index_lock

    def lose_lock(**kwargs) -> None:
        actual_claim(**kwargs)
        lock = repository.root / ".git" / "index.lock"
        lock.unlink(); lock.write_bytes(b"external replacement")

    monkeypatch.setattr(journal, "claim_index_lock", lose_lock)
    with pytest.raises(RepositoryError, match="real-index lock ownership"):
        repository._publish_real_index(journal, expected=before, desired=after)
    assert (repository.root / ".git" / "index.lock").read_bytes() == b"external replacement"
    assert repository._real_index_entries([path]) == before


def test_subprocess_lock_holder_is_never_removed_and_retry_is_safe(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    lock = repository.root / ".git" / "index.lock"
    script = "from pathlib import Path; import sys,time; Path(sys.argv[1]).write_bytes(b'external holder'); time.sleep(20)"
    holder = subprocess.Popen([sys.executable, "-c", script, str(lock)])
    try:
        for _ in range(100):
            if lock.exists():
                break
            time.sleep(0.02)
        with pytest.raises(RepositoryError, match="index"):
            repository.commit_files(expected_head=repository.head(), files={"story/world.md": b"after\n"}, message="locked")
        assert lock.read_bytes() == b"external holder"
    finally:
        holder.terminate(); holder.wait(timeout=10)
        lock.unlink(missing_ok=True)
    revision = repository.commit_files(expected_head=repository.head(), files={"story/world.md": b"after\n"}, message="retry")
    assert repository.head() == revision


def test_subprocess_canonical_lock_holder_survives_and_retry_is_safe(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    lock = repository.root / ".git" / "wedl-canonical-write.lock"
    script = "from pathlib import Path; import sys,time; Path(sys.argv[1]).write_bytes(b'external canonical holder'); time.sleep(20)"
    holder = subprocess.Popen([sys.executable, "-c", script, str(lock)])
    try:
        for _ in range(100):
            if lock.exists():
                break
            time.sleep(0.02)
        with pytest.raises(RepositoryError, match="canonical write"):
            repository.commit_files(expected_head=repository.head(), files={"story/world.md": b"after\n"}, message="blocked")
        assert lock.read_bytes() == b"external canonical holder"
    finally:
        holder.terminate(); holder.wait(timeout=10)
        lock.unlink(missing_ok=True)
    revision = repository.commit_files(expected_head=repository.head(), files={"story/world.md": b"after\n"}, message="retry")
    assert repository.head() == revision


def test_subprocess_death_owned_index_lock_is_adopted_from_journal(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    path = "story/world.md"; index = repository.root / ".git" / "index"; lock = repository.root / ".git" / "index.lock"
    before = repository._real_index_entries([path])
    journal = TransactionJournal.create(repository.root, ref="refs/heads/master", previous_head=repository.head(), committed_head=repository.head(),
        surfaces=[Surface(path, b"before\n", b"before\n", "source")], index_before=before, index_after=before)
    journal.write_staging_artifact("real-index-before", index.read_bytes())
    journal.seal_staging_artifact("real-index-before")
    install = journal.write_staging_artifact("real-index-install", index.read_bytes())
    journal.seal_staging_artifact("real-index-install")
    journal.bind_index_artifacts(backup="real-index-before", install="real-index-install")
    os.link(install, lock)
    status = lock.stat()
    journal.claim_index_lock(token="22222222-2222-2222-2222-222222222222", identity=f"{status.st_dev:x}:{status.st_ino:x}:{status.st_size:x}", digest=__import__("hashlib").sha256(lock.read_bytes()).hexdigest())
    repository.recover_authoring_transactions()
    assert not lock.exists() and not journal.path.exists()


def test_completed_journal_removes_registered_staging_artifact(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    artifact = journal.write_staging_artifact("private.sqlite", b"private"); journal.seal_staging_artifact("private.sqlite")
    journal.finish(); journal.cleanup()
    assert not artifact.exists() and not journal.path.exists()


def test_crash_after_completion_retains_then_retries_exact_cleanup(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    artifact = journal.write_staging_artifact("private.sqlite", b"private"); journal.seal_staging_artifact("private.sqlite")
    journal.finish()

    def crash(mutation: str) -> None:
        if mutation == "journal-cleanup":
            raise RuntimeError("crash after completed phase")

    transaction_recovery.set_mutation_hook(crash)
    try:
        with pytest.raises(RuntimeError, match="completed phase"):
            journal.cleanup()
    finally:
        transaction_recovery.set_mutation_hook(None)
    # Private artifacts are removed before the journal. A crash at the final
    # journal unlink retains only the completed ownership record for retry.
    assert journal.path.exists() and not artifact.exists()
    TransactionJournal.load(root, journal.path).cleanup()
    assert not journal.path.exists() and not artifact.exists()


def test_staging_seal_rechecks_post_write_identity_and_digest(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    artifact = journal.write_staging_artifact("compiled.sqlite", b"first")

    def interleave(mutation: str) -> None:
        if mutation == "staging-artifact-seal":
            artifact.write_bytes(b"external replacement")

    transaction_recovery.set_mutation_hook(interleave)
    try:
        with pytest.raises(RepositoryError, match="staging artifact changed ownership"):
            journal.seal_staging_artifact("compiled.sqlite")
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert artifact.read_bytes() == b"external replacement"
    assert journal.record["sealedArtifacts"] == {}
    assert journal.path.exists()


def test_private_index_build_crash_adopts_only_sealed_install(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    old = repository.head()
    new = repository.commit_files(expected_head=old, files={"story/world.md": b"after\n"}, message="after")
    path = "story/world.md"; before = repository._real_index_entries([path])
    blob = repository._git(["hash-object", "-w", "--stdin"], input_bytes=b"third\n").stdout.decode().strip()
    after = {path: {"mode": "100644", "stage": 0, "blob": blob}}
    journal = TransactionJournal.create(repository.root, ref="refs/heads/master", previous_head=old, committed_head=new, index_before=before, index_after=after)
    lock = repository.root / ".git" / "index.lock"
    observed_private_build = False

    def crash(mutation: str) -> None:
        nonlocal observed_private_build
        if mutation == "real-index-update-private":
            observed_private_build = True
            assert not lock.exists()
        if mutation == "real-index-publish":
            raise RuntimeError("crash before final index install")

    transaction_recovery.set_mutation_hook(crash)
    try:
        with pytest.raises(RuntimeError, match="final index install"):
            repository._publish_real_index(journal, expected=before, desired=after)
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert observed_private_build and lock.exists()
    assert journal.record["indexArtifacts"] == {"backup": "real-index-before", "install": "real-index-install"}
    repository.recover_authoring_transactions()
    assert not lock.exists() and repository._real_index_entries([path]) == after and not journal.path.exists()


def test_parent_reparse_after_mkdir_is_rechecked_at_actual_write(tmp_path: Path) -> None:
    root = tmp_path / "world"; root.mkdir()
    outside = tmp_path / "outside"; outside.mkdir()
    target = root / "story" / "nested" / "world.md"
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
        surfaces=[Surface("story/nested/world.md", None, b"after", "source")])

    def replace_parent(mutation: str) -> None:
        if mutation != "surface-parent-created":
            return
        try:
            target.parent.rmdir()
            target.parent.symlink_to(outside, target_is_directory=True)
        except OSError:
            pytest.skip("Windows test host does not permit reparse-point fixtures")

    transaction_recovery.set_mutation_hook(replace_parent)
    try:
        with pytest.raises(RepositoryError, match="reparse point|changed ownership"):
            journal.publish_surfaces()
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert not (outside / "world.md").exists()


@pytest.mark.skipif(os.name == "nt", reason="Windows does not expose POSIX executable mode semantics")
def test_mode_only_drift_fails_closed(tmp_path: Path) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before"); os.chmod(target, 0o600)
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
        surfaces=[Surface("story/world.md", b"before", b"after", "source", 0o744, 0o600)])
    with pytest.raises(RepositoryError, match="lost surface ownership"):
        journal.publish_surfaces()


@pytest.mark.parametrize("phase", ("prepared", "ref_committed", "sources_published", "index_published", "surfaces_published"))
def test_restart_rolls_forward_every_durable_phase_with_registered_cache_and_receipt(tmp_path: Path, phase: str) -> None:
    repository = _repository(tmp_path)
    old = repository.head()
    new = repository.commit_files(expected_head=old, files={"story/world.md": b"after\n"}, message="after")
    before_index = {"story/world.md": {"mode": "100644", "stage": 0, "blob": repository._blob_at(old, "story/world.md")}}
    after_index = repository._real_index_entries(["story/world.md"])
    (repository.root / "story/world.md").write_bytes(b"before\n")
    receipt = repository.root / ".wedl" / "receipt.json"; receipt.parent.mkdir(exist_ok=True); receipt.write_bytes(b"old")
    journal = TransactionJournal.create(
        repository.root, ref="refs/heads/master", previous_head=old, committed_head=new,
        surfaces=[Surface("story/world.md", b"before\n", b"after\n"), Surface(".wedl/receipt.json", b"old", b"new", "receipt"), Surface(".wedl/cache.sqlite", None, b"cache", "cache")],
        index_before=before_index, index_after=after_index,
    )
    if phase != "prepared":
        for name in ("ref_committed", "sources_published", "index_published", "surfaces_published"):
            journal.advance(name)
            if name == phase:
                break
    repository.recover_authoring_transactions()
    assert (repository.root / "story/world.md").read_bytes() == b"after\n"
    assert receipt.read_bytes() == b"new"
    assert (repository.root / ".wedl/cache.sqlite").read_bytes() == b"cache"
    assert repository._real_index_entries(["story/world.md"]) == after_index
    assert not journal.path.exists()


@pytest.mark.parametrize("replacement", [b"before", b"external bytes"])
def test_source_final_claim_checkpoint_preserves_identical_and_different_substitution(tmp_path: Path, replacement: bytes) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
                                        surfaces=[Surface("story/world.md", b"before", b"after", "source")])

    def substitute(name: str) -> None:
        if name == "surface-claim-rename":
            target.unlink(); target.write_bytes(replacement)

    transaction_recovery.set_mutation_hook(substitute)
    try:
        with pytest.raises(RepositoryError, match="lost surface ownership"):
            journal.publish_surfaces()
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert target.read_bytes() == replacement
    assert journal.path.exists()


@pytest.mark.parametrize("same_bytes", [True, False])
def test_real_index_final_checkpoint_preserves_identical_and_different_lock_substitution(tmp_path: Path, same_bytes: bool) -> None:
    repository = _repository(tmp_path)
    path = "story/world.md"; before = repository._real_index_entries([path])
    blob = repository._git(["hash-object", "-w", "--stdin"], input_bytes=b"after\n").stdout.decode().strip()
    after = {path: {"mode": "100644", "stage": 0, "blob": blob}}
    journal = TransactionJournal.create(repository.root, ref="refs/heads/master", previous_head=repository.head(), committed_head="1" * 40,
                                        index_before=before, index_after=after)
    lock = repository.root / ".git" / "index.lock"

    def substitute(name: str) -> None:
        if name == "real-index-publish":
            original = lock.read_bytes(); lock.unlink()
            lock.write_bytes(original if same_bytes else b"external Git lock")

    transaction_recovery.set_mutation_hook(substitute)
    try:
        with pytest.raises(RepositoryError, match="real-index lock ownership"):
            repository._publish_real_index(journal, expected=before, desired=after)
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert repository._real_index_entries([path]) == before
    assert lock.exists()
    lock.unlink()


@pytest.mark.parametrize("same_bytes", [True, False])
def test_journal_generation_cas_and_journal_last_cleanup_preserve_substitution(tmp_path: Path, same_bytes: bool) -> None:
    root = tmp_path / "world"; root.mkdir()
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    original = journal.path.read_bytes()

    def substitute_persist(name: str) -> None:
        if name == "atomic-replace":
            journal.path.unlink(); journal.path.write_bytes(original if same_bytes else b"{}\n")

    transaction_recovery.set_mutation_hook(substitute_persist)
    try:
        with pytest.raises(RepositoryError, match="lost surface ownership"):
            journal.advance("ref_committed")
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert journal.path.read_bytes() == (original if same_bytes else b"{}\n")

    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40)
    journal.finish(); final_bytes = journal.path.read_bytes()

    def substitute_cleanup(name: str) -> None:
        if name == "journal-cleanup":
            journal.path.unlink(); journal.path.write_bytes(final_bytes if same_bytes else b"external\n")

    transaction_recovery.set_mutation_hook(substitute_cleanup)
    try:
        with pytest.raises(RepositoryError, match="journal changed ownership"):
            journal.cleanup()
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert journal.path.exists()


def test_real_os_exit_dead_pid_reclaims_exact_journal_bound_lock_after_restart(tmp_path: Path) -> None:
    repository = _repository(tmp_path)
    token = "33333333-3333-3333-3333-333333333333"
    script = (
        "import os,sys; from pathlib import Path; from wedl.transaction_recovery import TransactionJournal; "
        "root=Path(sys.argv[1]); head=sys.argv[2]; token=sys.argv[3]; "
        "journal=TransactionJournal.create(root,ref='refs/heads/master',previous_head=head,committed_head=head); "
        "lock=root/'.git'/'wedl-canonical-write.lock'; "
        "lock.write_text(f'pid={os.getpid()}\\ntoken={token}\\n',encoding='ascii'); "
        "journal.bind_lock(lock,pid=os.getpid(),token=token); os._exit(0)"
    )
    child = subprocess.run([sys.executable, "-c", script, str(repository.root), repository.head(), token],
                           check=False, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert child.returncode == 0, child.stderr.decode("utf-8", "replace")
    lock = repository.root / ".git" / "wedl-canonical-write.lock"
    assert lock.exists()
    repository.recover_authoring_transactions()
    assert not lock.exists()
    journal_dir = repository.root / ".wedl" / "transactions"
    assert not journal_dir.exists() or not tuple(journal_dir.glob("*.json"))


@pytest.mark.parametrize("same_bytes", [True, False])
def test_stale_lock_reclaim_keeps_same_and_different_byte_substitutions(tmp_path: Path, monkeypatch, same_bytes: bool) -> None:
    repository = _repository(tmp_path)
    journal = TransactionJournal.create(repository.root, ref="refs/heads/master", previous_head=repository.head(), committed_head=repository.head())
    lock = repository.root / ".git" / "wedl-canonical-write.lock"
    token = "44444444-4444-4444-4444-444444444444"
    lock.write_text(f"pid=999999\ntoken={token}\n", encoding="ascii")
    journal.bind_lock(lock, pid=999999, token=token)
    original = lock.read_bytes()
    monkeypatch.setattr(transaction_recovery, "_process_dead", lambda _pid: True)

    def substitute(name: str) -> None:
        if name == "stale-lock-reclaim":
            lock.unlink(); lock.write_bytes(original if same_bytes else b"external lock\n")

    transaction_recovery.set_mutation_hook(substitute)
    try:
        assert not TransactionJournal.reclaim_owned_stale_lock(repository.root, lock)
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert lock.read_bytes() == (original if same_bytes else b"external lock\n") and journal.path.exists()


def test_restore_final_checkpoint_rejects_parent_reparse_substitution(tmp_path: Path) -> None:
    root = tmp_path / "world"; story = root / "story"; story.mkdir(parents=True)
    outside = tmp_path / "outside"; outside.mkdir()
    target = story / "world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
                                        surfaces=[Surface("story/world.md", b"before", b"after")])
    journal.publish_surfaces()

    def substitute(name: str) -> None:
        if name == "surface-restore-rename":
            target.unlink(); story.rmdir(); story.symlink_to(outside, target_is_directory=True)

    transaction_recovery.set_mutation_hook(substitute)
    try:
        with pytest.raises(RepositoryError, match="reparse point"):
            journal.restore_surfaces()
    except OSError:
        pytest.skip("Windows test host does not permit symlink fixtures")
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert not (outside / "world.md").exists() and journal.path.exists()


@pytest.mark.skipif(os.name == "nt", reason="POSIX-only liveness contract")
def test_posix_process_dead_uses_kill_zero_outcomes(monkeypatch) -> None:
    monkeypatch.setattr(transaction_recovery.os, "kill", lambda _pid, _signal: (_ for _ in ()).throw(ProcessLookupError()))
    assert transaction_recovery._process_dead(123) is True
    monkeypatch.setattr(transaction_recovery.os, "kill", lambda _pid, _signal: None)
    assert transaction_recovery._process_dead(123) is False
    monkeypatch.setattr(transaction_recovery.os, "kill", lambda _pid, _signal: (_ for _ in ()).throw(PermissionError()))
    assert transaction_recovery._process_dead(123) is None


@pytest.mark.parametrize("same_bytes", [True, False])
def test_canonical_lock_release_keeps_same_and_different_byte_substitutions(tmp_path: Path, same_bytes: bool) -> None:
    repository = _repository(tmp_path)
    lock = repository.root / ".git" / "wedl-canonical-write.lock"
    captured: list[TransactionJournal] = []
    original: list[bytes] = []

    def substitute(name: str) -> None:
        if name == "canonical-lock-release":
            original.append(lock.read_bytes()); lock.unlink(); lock.write_bytes(original[0] if same_bytes else b"external release lock\n")

    transaction_recovery.set_mutation_hook(substitute)
    try:
        with pytest.raises(RepositoryError, match="ownership changed"):
            with repository._canonical_write_lock():
                journal = TransactionJournal.create(repository.root, ref="refs/heads/master", previous_head=repository.head(), committed_head=repository.head())
                repository._bind_transaction_lock(journal); captured.append(journal)
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert lock.read_bytes() == (original[0] if same_bytes else b"external release lock\n")
    assert captured and captured[0].path.exists()


@pytest.mark.parametrize("before,after", [(None, b"created"), (b"before", b"updated")])
def test_private_after_image_is_sealed_before_public_link_and_preserves_mode(tmp_path: Path, before: bytes | None, after: bytes) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"
    if before is not None:
        target.write_bytes(before)
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
                                        surfaces=[Surface("story/world.md", before, after, "source", 0o640 if before else None, 0o600)])
    journal.publish_surfaces()
    private_after = journal.path.parent / journal.transaction_id / "backups" / "0000.after"
    assert target.read_bytes() == after and private_after.exists()
    if os.name != "nt":
        assert target.stat().st_mode & 0o777 == 0o600


@pytest.mark.parametrize("checkpoint,directory_name", [("backup-cleanup", "backups"), ("published-cleanup", "backups"), ("staging-cleanup", "staging")])
def test_cleanup_final_checkpoint_rejects_private_parent_reparse(tmp_path: Path, checkpoint: str, directory_name: str) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    outside = tmp_path / "outside"; outside.mkdir()
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
                                        surfaces=[Surface("story/world.md", b"before", b"after")])
    journal.publish_surfaces(); journal.restore_surfaces()
    artifact = journal.write_staging_artifact("private.bin", b"private"); journal.seal_staging_artifact("private.bin")
    journal.finish()
    private_directory = journal.path.parent / journal.transaction_id / directory_name

    def substitute(name: str) -> None:
        if name == checkpoint:
            moved = private_directory.with_name(f"{directory_name}-external")
            private_directory.rename(moved); private_directory.symlink_to(outside, target_is_directory=True)

    transaction_recovery.set_mutation_hook(substitute)
    try:
        with pytest.raises(RepositoryError, match="reparse point|changed ownership"):
            journal.cleanup()
    except OSError:
        pytest.skip("Windows test host does not permit symlink fixtures")
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert not tuple(outside.iterdir()) and journal.path.exists() and artifact.exists()


@pytest.mark.parametrize("checkpoint", ["surface-after-create", "surface-after-create-written", "surface-mode-publish"])
def test_private_after_create_write_and_chmod_crashes_leave_no_public_after_and_restart_rolls_back(tmp_path: Path, checkpoint: str) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    journal = TransactionJournal.create(root, ref="refs/heads/main", previous_head="0" * 40, committed_head="1" * 40,
                                        surfaces=[Surface("story/world.md", b"before", b"after", "source", 0o640, 0o600)])

    def crash(name: str) -> None:
        if name == checkpoint:
            raise RuntimeError("private after crash")

    transaction_recovery.set_mutation_hook(crash)
    try:
        with pytest.raises(RuntimeError, match="private after crash"):
            journal.publish_surfaces()
    finally:
        transaction_recovery.set_mutation_hook(None)
    assert not target.exists() and journal.path.exists()
    TransactionJournal.load(root, journal.path).restore_surfaces()
    assert target.read_bytes() == b"before"
    if os.name != "nt":
        assert target.stat().st_mode & 0o777 == 0o640


def test_real_os_exit_with_unsealed_private_after_restarts_to_before_image(tmp_path: Path) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    target = root / "story/world.md"; target.write_bytes(b"before")
    script = (
        "import os,sys; from pathlib import Path; "
        "import wedl.transaction_recovery as recovery; "
        "from wedl.transaction_recovery import Surface,TransactionJournal; "
        "root=Path(sys.argv[1]); "
        "journal=TransactionJournal.create(root,ref='refs/heads/main',previous_head='0'*40,committed_head='1'*40,"
        "surfaces=[Surface('story/world.md',b'before',b'after','source',0o640,0o600)]); "
        "recovery.set_mutation_hook(lambda name: os._exit(0) if name == 'surface-after-create-written' else None); "
        "journal.publish_surfaces()"
    )
    child = subprocess.run([sys.executable, "-c", script, str(root)], check=False,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert child.returncode == 0, child.stderr.decode("utf-8", "replace")
    journal_path, = (root / ".wedl" / "transactions").glob("*.json")
    private_after = journal_path.parent / journal_path.stem / "backups" / "0000.after"
    assert not target.exists() and private_after.read_bytes() == b"after"

    TransactionJournal.load(root, journal_path).restore_surfaces()

    assert target.read_bytes() == b"before"
    assert private_after.read_bytes() == b"after"


@pytest.mark.parametrize("checkpoint", [
    "backup-cleanup-unlink-after-primary-unlink",
    "after-cleanup-unlink-after-primary-unlink",
    "published-cleanup-unlink-after-primary-unlink",
    "staging-cleanup-unlink-after-primary-unlink",
    "journal-cleanup-unlink-after-primary-unlink",
])
def test_real_os_exit_after_each_cleanup_primary_unlink_is_restart_adopted(tmp_path: Path, checkpoint: str) -> None:
    root = tmp_path / "world"; (root / "story").mkdir(parents=True)
    (root / "story/world.md").write_bytes(b"before")
    script = (
        "import os,sys; from pathlib import Path; import wedl.transaction_recovery as recovery; "
        "from wedl.transaction_recovery import Surface,TransactionJournal; root=Path(sys.argv[1]); checkpoint=sys.argv[2]; "
        "j=TransactionJournal.create(root,ref='refs/heads/main',previous_head='0'*40,committed_head='1'*40,"
        "surfaces=[Surface('story/world.md',b'before',b'after')]); j.publish_surfaces(); j.restore_surfaces(); "
        "j.write_staging_artifact('private.bin',b'private'); j.seal_staging_artifact('private.bin'); j.finish(); "
        "recovery.set_mutation_hook(lambda name: os._exit(0) if name == checkpoint else None); j.cleanup()"
    )
    child = subprocess.run([sys.executable, "-c", script, str(root), checkpoint], check=False,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert child.returncode == 0, child.stderr.decode("utf-8", "replace")
    # Repeated restart scans must adopt any surviving exact hard-link tombstone
    # and converge without deleting a foreign/private sibling.
    for _ in range(3):
        for journal in TransactionJournal.pending(root):
            journal.cleanup()
    transactions = root / ".wedl" / "transactions"
    assert not transactions.exists() or not tuple(transactions.rglob("*"))
