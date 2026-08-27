from __future__ import annotations

from importlib import resources
import os
from pathlib import Path
import shutil
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
