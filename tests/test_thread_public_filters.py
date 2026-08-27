from __future__ import annotations

import json
from importlib import resources
from pathlib import Path
import shutil

import pytest
from fastapi.testclient import TestClient

from wedl import V04_SOURCE_SCHEMA
from wedl.cli import main
from wedl.compiler import compile_world
from wedl.context import build_context
from wedl.errors import UsageError
from wedl.query import search_world
from wedl.repository import Repository
from wedl.server import create_app


THREAD_A = "thread_0123456789ABCDEFGHJKMNPQRS"
THREAD_B = "thread_0123456789ABCDEFGHJKMNPQRT"
THREAD_UNKNOWN = "thread_0123456789ABCDEFGHJKMNPQRV"


def _append_frontmatter(path: Path, value: str) -> None:
    frontmatter, body = path.read_text(encoding="utf-8").split("\n---\n", 1)
    path.write_text(f"{frontmatter}\n{value}\n---\n{body}", encoding="utf-8")


def _worktree_repository(tmp_path: Path) -> Repository:
    root = tmp_path / "worktree"
    source = resources.files("wedl.data.ash_archive").joinpath("story")
    with resources.as_file(source) as source_path:
        shutil.copytree(source_path, root / "story")
    return Repository(root)


def _upgrade_to_v05(repository: Repository) -> tuple[str, str, str]:
    for path in (repository.root / "story").rglob("*.md"):
        path.write_text(
            path.read_text(encoding="utf-8").replace("schema: wedl/v0.3", "schema: wedl/v0.5", 1),
            encoding="utf-8",
        )
    _append_frontmatter(
        repository.root / "story" / "world.md",
        f"threads:\n  - id: {THREAD_A}\n    label: Archive\n  - id: {THREAD_B}\n    label: Road",
    )
    member_path = sorted((repository.root / "story" / "characters").glob("*.md"))[0]
    _append_frontmatter(member_path, f"threads:\n  - {THREAD_A}")
    world = repository.load_world()
    member = next(record for record in world.records.values() if record.source_path == member_path.relative_to(repository.root).as_posix())
    character = world.find("Mara Vale", "character")
    scene = world.find("An Honest Absence", "scene")
    return member.title, character.id, scene.id


def _client_get(root: Path, path: str, params: object) -> dict[str, object]:
    client = TestClient(create_app(root))
    try:
        response = client.get(path, params=params)
    finally:
        client.close()
    assert response.status_code == 200, response.text
    return response.json()


def test_public_filter_adapters_preserve_no_filter_outputs_and_match_cli_http(tmp_path, capsys) -> None:
    repository = _worktree_repository(tmp_path)
    member_title, character_id, scene_id = _upgrade_to_v05(repository)
    search_args = {
        "perspective": "author", "all_time": True, "mode": "fts", "limit": 8,
    }
    context_args = {
        "character_id": character_id, "scene_id": scene_id, "query": member_title,
        "search_mode": "fts", "max_characters": 3000,
    }

    baseline_search = search_world(repository, member_title, **search_args)
    assert search_world(repository, member_title, thread_ids=None, **search_args) == baseline_search
    assert main(["search", member_title, "--repo", str(repository.root), "--all-time", "--mode", "fts", "--limit", "8"]) == 0
    assert json.loads(capsys.readouterr().out) == baseline_search
    assert _client_get(repository.root, "/api/search", {"q": member_title, "allTime": "true", "mode": "fts", "limit": "8"}) == baseline_search

    filtered_search = search_world(repository, member_title, thread_ids=(THREAD_A, THREAD_B), **search_args)
    assert filtered_search["protocol"] == "wedl-search/v5"
    assert "threadIds" not in filtered_search
    assert filtered_search["results"]
    assert all(item["entityId"] for item in filtered_search["results"])
    assert main(["search", member_title, "--repo", str(repository.root), "--all-time", "--mode", "fts", "--limit", "8", "--thread-id", THREAD_A, "--thread-id", THREAD_B]) == 0
    assert json.loads(capsys.readouterr().out) == filtered_search
    assert _client_get(repository.root, "/api/search", [("q", member_title), ("allTime", "true"), ("mode", "fts"), ("limit", "8"), ("threadId", THREAD_A), ("threadId", THREAD_B)]) == filtered_search

    baseline_context = build_context(repository, **context_args)
    assert build_context(repository, _thread_filter_ids=None, **context_args) == baseline_context
    assert main(["context", character_id, "--repo", str(repository.root), "--scene", scene_id, "--query", member_title, "--mode", "fts", "--max-characters", "3000"]) == 0
    assert json.loads(capsys.readouterr().out) == baseline_context
    assert _client_get(repository.root, "/api/context", {"character": character_id, "scene": scene_id, "q": member_title, "mode": "fts", "maxCharacters": "3000"}) == baseline_context

    filtered_context = build_context(repository, _thread_filter_ids=(THREAD_A, THREAD_B), **context_args)
    assert filtered_context["protocol"] == "wedl-context/v3"
    assert "recallThreadIds" not in filtered_context
    assert main(["context", character_id, "--repo", str(repository.root), "--scene", scene_id, "--query", member_title, "--mode", "fts", "--max-characters", "3000", "--recall-thread-id", THREAD_A, "--recall-thread-id", THREAD_B]) == 0
    assert json.loads(capsys.readouterr().out) == filtered_context
    assert _client_get(repository.root, "/api/context", [("character", character_id), ("scene", scene_id), ("q", member_title), ("mode", "fts"), ("maxCharacters", "3000"), ("recallThreadId", THREAD_A), ("recallThreadId", THREAD_B)]) == filtered_context


def test_public_filter_adapters_preserve_v03_errors_and_v04_quarantine(ash_repo, tmp_path, capsys) -> None:
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    scene = world.find("An Honest Absence", "scene")
    compile_world(ash_repo, profile_name="fts")

    with pytest.raises(UsageError, match="^thread filtering requires a validated wedl/v0.5 world$"):
        search_world(ash_repo, "register", all_time=True, mode="fts", thread_ids=(THREAD_A,))
    with pytest.raises(UsageError, match="^thread filtering requires a validated wedl/v0.5 world$"):
        build_context(ash_repo, character_id=mara.id, scene_id=scene.id, search_mode="fts", _thread_filter_ids=(THREAD_A,))
    assert main(["search", "register", "--repo", str(ash_repo.root), "--all-time", "--mode", "fts", "--thread-id", THREAD_A]) == 2
    assert json.loads(capsys.readouterr().err)["message"] == "thread filtering requires a validated wedl/v0.5 world"
    assert main(["context", mara.id, "--repo", str(ash_repo.root), "--scene", scene.id, "--mode", "fts", "--recall-thread-id", THREAD_A]) == 2
    assert json.loads(capsys.readouterr().err)["message"] == "thread filtering requires a validated wedl/v0.5 world"

    client = TestClient(create_app(ash_repo.root))
    try:
        search_response = client.get("/api/search", params=[("q", "register"), ("allTime", "true"), ("mode", "fts"), ("threadId", THREAD_A)])
        context_response = client.get("/api/context", params=[("character", mara.id), ("scene", scene.id), ("mode", "fts"), ("recallThreadId", THREAD_A)])
    finally:
        client.close()
    assert search_response.status_code == context_response.status_code == 400
    assert search_response.json()["message"] == context_response.json()["message"] == "thread filtering requires a validated wedl/v0.5 world"

    v04 = _worktree_repository(tmp_path / "v04")
    world_path = v04.root / "story" / "world.md"
    world_path.write_text(world_path.read_text(encoding="utf-8").replace("schema: wedl/v0.3", f"schema: {V04_SOURCE_SCHEMA}", 1), encoding="utf-8")
    assert main(["search", "register", "--repo", str(v04.root), "--all-time", "--mode", "fts", "--thread-id", THREAD_A]) == 2
    assert json.loads(capsys.readouterr().err)["code"] == "v04_superseded"
    client = TestClient(create_app(v04.root))
    try:
        response = client.get("/api/context", params=[("character", mara.id), ("scene", scene.id), ("mode", "fts"), ("recallThreadId", THREAD_A)])
    finally:
        client.close()
    assert response.status_code == 400
    assert response.json()["code"] == "v04_superseded"


def test_author_recall_selector_validates_every_supplied_id_without_changing_author_output(ash_repo, tmp_path, capsys) -> None:
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    scene = world.find("An Honest Absence", "scene")
    compile_world(ash_repo, profile_name="fts")
    base_cli = ["context", mara.id, "--repo", str(ash_repo.root), "--scene", scene.id, "--perspective", "author", "--mode", "fts"]
    base_http = [("character", mara.id), ("scene", scene.id), ("perspective", "author"), ("mode", "fts")]
    cases = [
        (("not-a-thread",), "thread filter id must use the thread_<26 Crockford> format"),
        ((THREAD_A, THREAD_A), "thread filter thread ids must be unique"),
        ((THREAD_B, THREAD_A), "thread filter thread ids must be sorted"),
        ((THREAD_A,), "thread filtering requires a validated wedl/v0.5 world"),
    ]
    client = TestClient(create_app(ash_repo.root))
    try:
        for thread_ids, message in cases:
            argv = [*base_cli, *[part for thread_id in thread_ids for part in ("--recall-thread-id", thread_id)]]
            assert main(argv) == 2
            assert json.loads(capsys.readouterr().err)["message"] == message
            response = client.get("/api/context", params=[*base_http, *[("recallThreadId", thread_id) for thread_id in thread_ids]])
            assert response.status_code == 400
            assert response.json()["message"] == message
    finally:
        client.close()

    repository = _worktree_repository(tmp_path)
    _member_title, character_id, scene_id = _upgrade_to_v05(repository)
    author_args = {"character_id": character_id, "scene_id": scene_id, "perspective": "author", "search_mode": "fts"}
    baseline = build_context(repository, **author_args)
    assert build_context(repository, _thread_filter_ids=(THREAD_A, THREAD_B), **author_args) == baseline
    assert main(["context", character_id, "--repo", str(repository.root), "--scene", scene_id, "--perspective", "author", "--mode", "fts", "--recall-thread-id", THREAD_A, "--recall-thread-id", THREAD_B]) == 0
    assert json.loads(capsys.readouterr().out) == baseline
    assert main(["context", character_id, "--repo", str(repository.root), "--scene", scene_id, "--perspective", "author", "--mode", "fts", "--recall-thread-id", THREAD_UNKNOWN]) == 2
    assert json.loads(capsys.readouterr().err)["message"] == "thread filter id is not declared by the world"
    client = TestClient(create_app(repository.root))
    try:
        undeclared = client.get("/api/context", params=[("character", character_id), ("scene", scene_id), ("perspective", "author"), ("mode", "fts"), ("recallThreadId", THREAD_UNKNOWN)])
        valid = client.get("/api/context", params=[("character", character_id), ("scene", scene_id), ("perspective", "author"), ("mode", "fts"), ("recallThreadId", THREAD_A), ("recallThreadId", THREAD_B)])
    finally:
        client.close()
    assert undeclared.status_code == 400
    assert undeclared.json()["message"] == "thread filter id is not declared by the world"
    assert valid.status_code == 200
    assert valid.json() == baseline
