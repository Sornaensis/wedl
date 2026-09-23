from __future__ import annotations

from pathlib import Path
import subprocess
import os
import stat

import pytest

from wedl.repository import DirtyManagedTree, Repository, RepositoryError
from wedl import repository as repository_module


def _git(root: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", "-C", str(root), *args], check=True, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )


def test_bounded_commit_before_exact_limit_and_growth(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    target = tmp_path / "before.md"
    assert repository_module._bounded_commit_before(target, 3) == (None, None, None)
    target.write_bytes(b"123")
    before, mode, identity = repository_module._bounded_commit_before(target, 3)
    assert before == b"123" and mode is not None and identity is not None
    with pytest.raises(RepositoryError, match="exceeds byte limit"):
        repository_module._bounded_commit_before(target, 2)

    original_read = os.read
    grown = [False]

    def grow_during_read(descriptor: int, count: int) -> bytes:
        chunk = original_read(descriptor, count)
        if not grown[0]:
            grown[0] = True
            with target.open("ab") as output:
                output.write(b"4")
        return chunk

    monkeypatch.setattr(repository_module.os, "read", grow_during_read)
    with pytest.raises(RepositoryError, match="changed during bounded capture"):
        repository_module._bounded_commit_before(target, 4)


def test_bounded_commit_before_rejects_nonregular_path(tmp_path: Path) -> None:
    directory = tmp_path / "before.md"
    directory.mkdir()
    with pytest.raises(RepositoryError, match="exceeds byte limit"):
        repository_module._bounded_commit_before(directory, 64)


def test_bounded_commit_before_revalidates_named_path(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    target = tmp_path / "before.md"
    replacement = tmp_path / "replacement.md"
    target.write_bytes(b"original")
    replacement.write_bytes(b"replacement")
    original_lstat = os.lstat
    calls = [0]

    def substituted_lstat(path):
        if Path(path) == target:
            calls[0] += 1
            if calls[0] >= 2:
                return original_lstat(replacement)
        return original_lstat(path)

    monkeypatch.setattr(repository_module.os, "lstat", substituted_lstat)
    with pytest.raises(RepositoryError, match="changed during bounded capture"):
        repository_module._bounded_commit_before(target, 64)


def test_bounded_capture_rejects_restored_mtime_content_change(tmp_path: Path) -> None:
    target = tmp_path / "before.md"
    target.write_bytes(b"original")
    before, _, identity = repository_module._bounded_commit_before(target, 64)
    parents = repository_module._source_parent_identities(target, tmp_path)
    original = target.stat()
    target.write_bytes(b"modified")
    os.utime(target, ns=(original.st_atime_ns, original.st_mtime_ns))
    with pytest.raises(RepositoryError, match="changed during bounded capture"):
        repository_module._verify_source_identity(target, identity, before=before, parents=parents)


def test_bounded_capture_rejects_parent_reparse_substitution(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    parent = tmp_path / "story"
    parent.mkdir()
    target = parent / "before.md"
    target.write_bytes(b"original")
    before, _, identity = repository_module._bounded_commit_before(target, 64, root=tmp_path)
    parents = repository_module._source_parent_identities(target, tmp_path)
    original_lstat = os.lstat

    class ReparsedParent:
        def __init__(self, original):
            self.__dict__.update((name, getattr(original, name)) for name in (
                "st_dev", "st_ino", "st_size", "st_mtime_ns", "st_nlink", "st_mode",
            ))
            self.st_file_attributes = getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)

    def substituted_lstat(path):
        metadata = original_lstat(path)
        return ReparsedParent(metadata) if Path(path) == parent else metadata

    monkeypatch.setattr(repository_module.os, "lstat", substituted_lstat)
    with pytest.raises(RepositoryError, match="parent changed during bounded capture"):
        repository_module._verify_source_identity(target, identity, before=before, parents=parents)


def test_commit_before_limit_preserves_ref_index_and_source(task91_repo: Repository) -> None:
    target = task91_repo.root / "story" / "world.md"
    original = target.read_bytes()
    index = (task91_repo.root / ".git" / "index").read_bytes()
    head = task91_repo.head()
    with pytest.raises(RepositoryError, match="exceeds byte limit"):
        task91_repo.commit_files(
            expected_head=head, files={"story/world.md": b"replacement\n"},
            message="bounded capture", max_before_bytes=len(original) - 1,
        )
    assert task91_repo.head() == head
    assert target.read_bytes() == original
    assert (task91_repo.root / ".git" / "index").read_bytes() == index


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
