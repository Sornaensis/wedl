from __future__ import annotations

import json
from importlib import resources
from pathlib import Path
import shutil

import pytest
from fastapi.testclient import TestClient

from wedl import THREAD_SOURCE_SCHEMA, V04_SOURCE_SCHEMA
from wedl.cli import main
from wedl.compiler import cache_readiness
from wedl.errors import CompileRequired, SupersededSchemaError
from wedl.query import thread_catalog
from wedl.repository import Repository
from wedl.server import create_app


THREAD_ARCHIVE = "thread_0123456789ABCDEFGHJKMNPQRS"
THREAD_ARCHIVE_TWO = "thread_0123456789ABCDEFGHJKMNPQRT"
THREAD_ROAD = "thread_0123456789ABCDEFGHJKMNPQRV"


def _append_frontmatter(path: Path, value: str) -> None:
    frontmatter, body = path.read_text(encoding="utf-8").split("\n---\n", 1)
    path.write_text(f"{frontmatter}\n{value}\n---\n{body}", encoding="utf-8")


def _worktree_repository(tmp_path: Path) -> Repository:
    root = tmp_path / "worktree"
    source = resources.files("wedl.data.ash_archive").joinpath("story")
    with resources.as_file(source) as source_path:
        shutil.copytree(source_path, root / "story")
    return Repository(root)


def _upgrade_to_v05(repository) -> None:
    for path in (repository.root / "story").rglob("*.md"):
        path.write_text(
            path.read_text(encoding="utf-8").replace("schema: wedl/v0.3", f"schema: {THREAD_SOURCE_SCHEMA}", 1),
            encoding="utf-8",
        )
    _append_frontmatter(
        repository.root / "story" / "world.md",
        "threads:\n"
        f"  - id: {THREAD_ARCHIVE}\n    label: archive\n"
        f"  - id: {THREAD_ARCHIVE_TWO}\n    label: Archive\n"
        f"  - id: {THREAD_ROAD}\n    label: Road",
    )
    _append_frontmatter(
        sorted((repository.root / "story" / "characters").glob("*.md"))[0],
        f"threads:\n  - {THREAD_ROAD}",
    )


def test_v03_catalog_has_no_synthetic_grouping_and_cli_http_query_parity(ash_repo, capsys) -> None:
    expected = {
        "protocol": "wedl-threads/v1",
        "revision": ash_repo.head(),
        "sourceSchema": "wedl/v0.3",
        "groupingAvailable": False,
        "threads": [],
    }

    assert thread_catalog(ash_repo) == expected
    assert main(["threads", "--repo", str(ash_repo.root)]) == 0
    assert json.loads(capsys.readouterr().out) == expected
    with TestClient(create_app(ash_repo.root)) as client:
        response = client.get("/api/threads")
    assert response.status_code == 200
    assert response.json() == expected


def test_v05_catalog_is_sorted_and_discloses_only_public_declarations(tmp_path, capsys) -> None:
    repository = _worktree_repository(tmp_path)
    _upgrade_to_v05(repository)

    expected_threads = [
        {"id": THREAD_ARCHIVE, "label": "archive"},
        {"id": THREAD_ARCHIVE_TWO, "label": "Archive"},
        {"id": THREAD_ROAD, "label": "Road"},
    ]
    response = thread_catalog(repository)

    assert response == {
        "protocol": "wedl-threads/v1",
        "revision": "WORKTREE",
        "sourceSchema": THREAD_SOURCE_SCHEMA,
        "groupingAvailable": True,
        "threads": expected_threads,
    }
    assert set(response) == {"protocol", "revision", "sourceSchema", "groupingAvailable", "threads"}
    assert all(set(entry) == {"id", "label"} for entry in response["threads"])

    assert main(["threads", "--repo", str(repository.root)]) == 0
    cli = json.loads(capsys.readouterr().out)
    with TestClient(create_app(repository.root)) as client:
        http = client.get("/api/threads").json()
    assert cli == http == response


def test_catalog_require_compiled_never_rebuilds_when_cache_is_missing(ash_repo, capsys) -> None:
    assert cache_readiness(ash_repo)["state"] == "missing"
    client = TestClient(create_app(ash_repo.root))
    try:
        http = client.get("/api/threads", params={"requireCompiled": "true"})
    finally:
        client.close()
    assert http.status_code == 400
    assert http.json()["code"] == "compile_required"
    assert cache_readiness(ash_repo)["state"] == "missing"
    with pytest.raises(CompileRequired):
        thread_catalog(ash_repo, require_compiled=True)
    assert main(["threads", "--repo", str(ash_repo.root), "--require-compiled"]) == 2
    assert json.loads(capsys.readouterr().err)["code"] == "compile_required"
    assert cache_readiness(ash_repo)["state"] == "missing"


def test_catalog_preserves_v04_quarantine_boundary(tmp_path) -> None:
    repository = _worktree_repository(tmp_path)
    world = repository.root / "story" / "world.md"
    world.write_text(world.read_text(encoding="utf-8").replace("schema: wedl/v0.3", f"schema: {V04_SOURCE_SCHEMA}", 1), encoding="utf-8")

    with pytest.raises(SupersededSchemaError, match=r"^story/world\.md: wedl/v0\.4 is superseded;"):
        thread_catalog(repository)

    client = TestClient(create_app(repository.root))
    try:
        http = client.get("/api/threads")
    finally:
        client.close()
    assert http.status_code == 400
    assert http.json()["code"] == "v04_superseded"
