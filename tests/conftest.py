from __future__ import annotations

from importlib import resources
import os
from pathlib import Path
import shutil
import stat
import subprocess

import pytest

from wedl.repository import Repository


def git(root: Path, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(["git", "-C", str(root), *args], check=check, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if os.name == "nt" and args and args[0] == "init":
        subprocess.run(["git", "-C", str(root), "config", "core.longpaths", "true"], check=True, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return result


@pytest.fixture()
def ash_repo(tmp_path: Path) -> Repository:
    root = tmp_path / "ash"
    root.mkdir()
    source = resources.files("wedl.data.ash_archive").joinpath("story")
    with resources.as_file(source) as source_path:
        shutil.copytree(source_path, root / "story")
    (root / ".gitignore").write_text(".wedl/\n")
    git(root, "init", "-q")
    git(root, "config", "user.name", "wedl test")
    git(root, "config", "user.email", "wedl@test.invalid")
    git(root, "add", "story", ".gitignore")
    git(root, "commit", "-qm", "seed")
    return Repository(root)


def _task91_tree(root: Path) -> dict[str, bytes]:
    """Capture the seed's complete byte image for isolation checks."""

    return {
        path.relative_to(root).as_posix(): path.read_bytes()
        for path in root.rglob("*")
        if path.is_file()
    }


def _task91_is_reparse_point(path: Path) -> bool:
    attributes = getattr(os.lstat(path), "st_file_attributes", 0)
    return bool(attributes & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0))


def _assert_task91_seed_is_clean(root: Path) -> None:
    assert (root / ".git").is_dir()
    assert not (root / ".git" / "index.lock").exists()
    assert not (root / ".wedl").exists()


def _assert_task91_copy_is_independent(seed: Path, copy: Path) -> None:
    assert not os.path.samefile(seed, copy)
    assert _task91_tree(copy) == _task91_tree(seed)
    for source in seed.rglob("*"):
        target = copy / source.relative_to(seed)
        assert target.exists()
        assert not source.is_symlink()
        assert not target.is_symlink()
        assert not _task91_is_reparse_point(source)
        assert not _task91_is_reparse_point(target)
        assert not os.path.samefile(source, target)


@pytest.fixture(scope="session")
def _task91_seed(tmp_path_factory: pytest.TempPathFactory) -> Path:
    root = tmp_path_factory.mktemp("task91-seed") / "repository"
    root.mkdir()
    world = resources.files("wedl.data.ash_archive").joinpath("story", "world.md")
    (root / "story").mkdir()
    with resources.as_file(world) as world_path:
        shutil.copyfile(world_path, root / "story" / "world.md")
    (root / ".gitignore").write_text(".wedl/\n")
    git(root, "init", "-q")
    git(root, "config", "user.name", "wedl test")
    git(root, "config", "user.email", "wedl@test.invalid")
    git(root, "add", "story", ".gitignore")
    git(root, "commit", "-qm", "seed")
    _assert_task91_seed_is_clean(root)
    return root


@pytest.fixture()
def task91_repo(tmp_path: Path, _task91_seed: Path) -> Repository:
    root = tmp_path / "task91"
    seed_before = _task91_tree(_task91_seed)
    shutil.copytree(_task91_seed, root, copy_function=shutil.copy2)
    _assert_task91_seed_is_clean(_task91_seed)
    _assert_task91_seed_is_clean(root)
    _assert_task91_copy_is_independent(_task91_seed, root)
    # A byte copy necessarily gives the worktree new filesystem timestamps.
    # Refresh only this private copy before tests take their index snapshots.
    git(root, "update-index", "--refresh")
    _assert_task91_seed_is_clean(_task91_seed)
    _assert_task91_seed_is_clean(root)
    repository = Repository(root)
    yield repository
    assert _task91_tree(_task91_seed) == seed_before
    _assert_task91_seed_is_clean(_task91_seed)


@pytest.fixture()
def frontiersmen_repo(tmp_path: Path) -> Repository:
    root = tmp_path / "frontiersmen"
    root.mkdir()
    source = resources.files("wedl.data.frontiersmen").joinpath("story")
    with resources.as_file(source) as source_path:
        shutil.copytree(source_path, root / "story")
    (root / ".gitignore").write_text(".wedl/\n", encoding="utf-8")
    git(root, "init", "-q")
    git(root, "config", "user.name", "wedl test")
    git(root, "config", "user.email", "wedl@test.invalid")
    git(root, "add", "story", ".gitignore")
    git(root, "commit", "-qm", "seed Frontiersmen")
    return Repository(root)
