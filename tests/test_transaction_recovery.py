from __future__ import annotations

from pathlib import Path
import subprocess

import pytest

from wedl.repository import DirtyManagedTree, Repository, RepositoryError


def _git(root: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", "-C", str(root), *args], check=True, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )


def _fake_wait(repo: Repository, monkeypatch: pytest.MonkeyPatch, release) -> list[float]:
    now = [0.0]
    sleeps: list[float] = []

    def clock() -> float:
        return now[0]

    def sleeper(delay: float) -> None:
        sleeps.append(delay)
        now[0] += delay
        release(len(sleeps))

    monkeypatch.setattr(repo, "_external_index_lock_clock", clock, raising=False)
    monkeypatch.setattr(repo, "_external_index_lock_sleep", sleeper, raising=False)
    monkeypatch.setattr(repo, "_external_index_lock_wait_seconds", 0.08)
    monkeypatch.setattr(repo, "_external_index_lock_initial_backoff_seconds", 0.02)
    monkeypatch.setattr(repo, "_external_index_lock_max_backoff_seconds", 0.04)
    return sleeps


def test_transient_external_lock_releases_within_the_private_window(task91_repo: Repository, monkeypatch: pytest.MonkeyPatch) -> None:
    lock = task91_repo.root / ".git" / "index.lock"
    lock.write_bytes(b"external writer")
    sleeps = _fake_wait(task91_repo, monkeypatch, lambda count: lock.unlink() if count == 1 else None)

    previous = task91_repo.head()
    commit = task91_repo.commit_files(
        expected_head=previous, files={"story/task91.md": b"published\n"}, message="wedl: task 91",
    )

    assert task91_repo.head() == commit
    assert (task91_repo.root / "story/task91.md").read_bytes() == b"published\n"
    assert not lock.exists()
    assert sleeps == [0.02]


def test_persistent_or_substituted_external_lock_never_mutates_it(task91_repo: Repository, monkeypatch: pytest.MonkeyPatch) -> None:
    lock = task91_repo.root / ".git" / "index.lock"
    lock.write_bytes(b"first external writer")
    index_before = (task91_repo.root / ".git" / "index").read_bytes()
    head_before = task91_repo.head()
    _fake_wait(task91_repo, monkeypatch, lambda _count: None)

    with pytest.raises(RepositoryError, match="index.lock"):
        task91_repo.commit_files(
            expected_head=head_before, files={"story/task91.md": b"new\n"}, message="wedl: task 91",
        )

    assert lock.read_bytes() == b"first external writer"
    assert task91_repo.head() == head_before
    assert (task91_repo.root / ".git" / "index").read_bytes() == index_before
    assert not (task91_repo.root / "story/task91.md").exists()

    def substitute(count: int) -> None:
        if count == 1:
            lock.unlink()
            lock.write_bytes(b"replacement external writer")

    _fake_wait(task91_repo, monkeypatch, substitute)
    with pytest.raises(RepositoryError, match="changed during bounded wait"):
        task91_repo._await_external_index_lock_release(deadline=[None])
    assert lock.read_bytes() == b"replacement external writer"
    assert task91_repo.head() == head_before
    assert (task91_repo.root / ".git" / "index").read_bytes() == index_before


def test_two_external_lock_seams_share_the_first_lazy_deadline(task91_repo: Repository, monkeypatch: pytest.MonkeyPatch) -> None:
    lock = task91_repo.root / ".git" / "index.lock"
    now = [0.0]
    sleeps: list[float] = []

    def sleeper(delay: float) -> None:
        sleeps.append(delay)
        now[0] += delay
        if len(sleeps) == 1:
            lock.unlink()

    monkeypatch.setattr(task91_repo, "_external_index_lock_clock", lambda: now[0], raising=False)
    monkeypatch.setattr(task91_repo, "_external_index_lock_sleep", sleeper, raising=False)
    monkeypatch.setattr(task91_repo, "_external_index_lock_wait_seconds", 0.04)
    monkeypatch.setattr(task91_repo, "_external_index_lock_initial_backoff_seconds", 0.02)
    deadline: list[float | None] = [None]
    lock.write_bytes(b"first external writer")

    assert task91_repo._await_external_index_lock_release(deadline=deadline)
    assert deadline == [0.04]
    lock.write_bytes(b"second external writer")
    with pytest.raises(RepositoryError, match="index.lock"):
        task91_repo._await_external_index_lock_release(deadline=deadline)
    assert deadline == [0.04]
    assert now[0] == 0.04
    assert lock.read_bytes() == b"second external writer"


def test_external_touched_entry_after_wait_aborts_before_ref_or_source_publish(task91_repo: Repository, monkeypatch: pytest.MonkeyPatch) -> None:
    target = task91_repo.root / "story/task91.md"
    lock = task91_repo.root / ".git" / "index.lock"
    original_git = task91_repo._git

    def wrapped_git(args, **kwargs):
        result = original_git(args, **kwargs)
        if args[:1] == ["commit-tree"] and not lock.exists():
            lock.write_bytes(b"external writer")
        return result

    def release_and_stage(count: int) -> None:
        if count == 1:
            lock.unlink()
            target.write_bytes(b"external staged value\n")
            _git(task91_repo.root, "add", "story/task91.md")

    monkeypatch.setattr(task91_repo, "_git", wrapped_git)
    _fake_wait(task91_repo, monkeypatch, release_and_stage)
    previous = task91_repo.head()

    with pytest.raises(DirtyManagedTree):
        task91_repo.commit_files(
            expected_head=previous, files={"story/task91.md": b"wedl value\n"}, message="wedl: task 91",
        )

    assert task91_repo.head() == previous
    assert target.read_bytes() == b"external staged value\n"
    assert "task91.md" in _git(task91_repo.root, "diff", "--cached", "--name-only").stdout


def test_complete_index_cas_preserves_an_unrelated_staged_entry(task91_repo: Repository, monkeypatch: pytest.MonkeyPatch) -> None:
    unrelated = task91_repo.root / "external-stage.txt"
    target = task91_repo.root / "story/task91.md"
    previous = task91_repo.head()
    raced = [False]

    def checkpoint(name: str) -> None:
        if name == "real-index-lock-link" and not raced[0]:
            raced[0] = True
            unrelated.write_text("external stage\n", encoding="utf-8")
            _git(task91_repo.root, "add", "external-stage.txt")

    monkeypatch.setattr("wedl.repository.mutation_checkpoint", checkpoint)
    with pytest.raises(RepositoryError, match="lost complete real-index ownership"):
        task91_repo.commit_files(
            expected_head=previous, files={"story/task91.md": b"wedl value\n"}, message="wedl: task 91",
        )

    assert task91_repo.head() == previous
    assert not target.exists()
    assert _git(task91_repo.root, "diff", "--cached", "--name-only").stdout.strip() == "external-stage.txt"
    assert (task91_repo.root / "external-stage.txt").read_text(encoding="utf-8") == "external stage\n"
