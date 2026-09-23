from __future__ import annotations

import pytest

from wedl import migration
from wedl.repository import Repository, RepositoryError


def _prepared_upgrade(repository: Repository) -> tuple[dict[str, object], dict[str, object]]:
    """Keep the migration consumer focused on its Repository transaction seam."""

    expected = repository.head()
    request: dict[str, object] = {
        "protocol": migration.PROTOCOL,
        "mode": "upgrade-v03",
        "expectedHead": expected,
        "sourceSnapshotHash": "task91-source-snapshot",
        "idempotencyKey": "task91-migration",
    }
    plan: dict[str, object] = {
        **request,
        "valid": True,
        "noOp": False,
        "confirmationToken": "task91-confirmation",
        "backupRef": "refs/wedl/backups/migration/task91",
        "requestHash": "task91-request",
        "_request": {**request, "backupRef": "refs/wedl/backups/migration/task91"},
        # This represents a validated migration result while avoiding cache
        # compilation and schema planning in this bounded lock test.
        "_changes": {"story/world.md": (repository.root / "story/world.md").read_bytes()},
    }
    return request, plan


def test_migration_consumer_waits_for_a_transient_lock(task91_repo: Repository, monkeypatch: pytest.MonkeyPatch) -> None:
    request, plan = _prepared_upgrade(task91_repo)
    lock = task91_repo.root / ".git" / "index.lock"
    lock.write_bytes(b"external migration writer")
    now = [0.0]

    def sleeper(delay: float) -> None:
        now[0] += delay
        lock.unlink()

    monkeypatch.setattr(task91_repo, "_external_index_lock_clock", lambda: now[0], raising=False)
    monkeypatch.setattr(task91_repo, "_external_index_lock_sleep", sleeper, raising=False)
    monkeypatch.setattr("wedl.migration.preview", lambda _repository, _request: plan)
    monkeypatch.setattr("wedl.migration.compile_world", lambda _repository: {"status": "stubbed-disposable-cache"})

    result = migration.apply(task91_repo, request, confirmation_token_value=str(plan["confirmationToken"]))

    assert result["status"] == "committed"
    assert result["compile"] == {"status": "stubbed-disposable-cache"}
    assert task91_repo.ref(str(plan["backupRef"])) == str(plan["expectedHead"])
    assert not lock.exists()


def test_migration_consumer_persistent_lock_keeps_head_source_and_index(task91_repo: Repository, monkeypatch: pytest.MonkeyPatch) -> None:
    request, plan = _prepared_upgrade(task91_repo)
    head_before = task91_repo.head()
    backup_ref = str(plan["backupRef"])
    assert task91_repo.ref(backup_ref) is None
    source_before = {path: data for path, data in task91_repo.snapshot("HEAD").files.items()}
    index_before = (task91_repo.root / ".git" / "index").read_bytes()
    lock = task91_repo.root / ".git" / "index.lock"
    lock.write_bytes(b"persistent migration writer")
    now = [0.0]

    monkeypatch.setattr(task91_repo, "_external_index_lock_clock", lambda: now[0], raising=False)
    monkeypatch.setattr(task91_repo, "_external_index_lock_sleep", lambda delay: now.__setitem__(0, now[0] + delay), raising=False)
    monkeypatch.setattr("wedl.migration.preview", lambda _repository, _request: plan)

    with pytest.raises(RepositoryError, match="index.lock"):
        migration.apply(task91_repo, request, confirmation_token_value=str(plan["confirmationToken"]))

    assert task91_repo.head() == head_before
    assert task91_repo.snapshot("HEAD").files == source_before
    assert (task91_repo.root / ".git" / "index").read_bytes() == index_before
    assert task91_repo.ref(backup_ref) is None
    assert lock.read_bytes() == b"persistent migration writer"


def test_released_migration_lock_rechecks_head_before_creating_absent_backup(task91_repo: Repository, monkeypatch: pytest.MonkeyPatch) -> None:
    request, plan = _prepared_upgrade(task91_repo)
    backup_ref = str(plan["backupRef"])
    source_before = task91_repo.snapshot("HEAD").files
    index_before = (task91_repo.root / ".git" / "index").read_bytes()
    monkeypatch.setattr("wedl.migration.preview", lambda _repository, _request: plan)
    monkeypatch.setattr(task91_repo, "_await_external_index_lock_release", lambda *, deadline: True)
    monkeypatch.setattr(task91_repo, "head", lambda: "external-head")

    with pytest.raises(Exception, match="expected a different HEAD"):
        migration.apply(task91_repo, request, confirmation_token_value=str(plan["confirmationToken"]))

    assert task91_repo.ref(backup_ref) is None
    assert task91_repo.snapshot("HEAD").files == source_before
    assert (task91_repo.root / ".git" / "index").read_bytes() == index_before


def test_migration_admission_and_commit_share_one_deadline_cell(task91_repo: Repository, monkeypatch: pytest.MonkeyPatch) -> None:
    request, plan = _prepared_upgrade(task91_repo)
    seen: list[list[float | None]] = []
    original = task91_repo._await_external_index_lock_release

    def observe(*, deadline):
        seen.append(deadline)
        return original(deadline=deadline)

    monkeypatch.setattr("wedl.migration.preview", lambda _repository, _request: plan)
    monkeypatch.setattr("wedl.migration.compile_world", lambda _repository: {"status": "stubbed-disposable-cache"})
    monkeypatch.setattr(task91_repo, "_await_external_index_lock_release", observe)
    migration.apply(task91_repo, request, confirmation_token_value=str(plan["confirmationToken"]))
    assert len(seen) >= 2
    assert all(cell is seen[0] for cell in seen)
