from __future__ import annotations

from importlib import resources
from pathlib import Path
import shutil

import pytest
from fastapi.testclient import TestClient

from wedl.query import (
    entity_state,
    interactions_between,
    knowledge,
    search_world,
    story_points,
    thread_catalog,
    timeline,
    validation_report,
    whereabouts,
)
from wedl.server import create_app


@pytest.fixture()
def ash_worktree_repo(tmp_path: Path):
    """Keep API parity tests independent of Windows Git path limits."""

    root = tmp_path / "ash"
    source = resources.files("wedl.data.ash_archive").joinpath("story")
    with resources.as_file(source) as source_path:
        shutil.copytree(source_path, root / "story")
    from wedl.repository import Repository
    return Repository(root)


def test_api_read_routes_match_their_cli_query_functions(ash_worktree_repo) -> None:
    world = ash_worktree_repo.load_world()
    mara = world.find("Mara Vale", "character")
    caldrin = world.find("Caldrin", "character")
    scene = world.find("After the Exchange", "scene")

    with TestClient(create_app(ash_worktree_repo.root)) as client:
        assert client.get("/api/validate").json() == validation_report(ash_worktree_repo)
        assert client.get(
            f"/api/entities/{mara.id}/state",
            params={"tick": 121, "timeline": "main", "order": 0, "requireCompiled": "true"},
        ).json() == entity_state(ash_worktree_repo, mara.id, 121, "main", 0, require_compiled=True)
        assert client.get(
            f"/api/entities/{mara.id}/knowledge",
            params={"tick": 121, "timeline": "main", "order": 0, "requireCompiled": "true"},
        ).json() == knowledge(ash_worktree_repo, mara.id, 121, "main", 0, require_compiled=True)
        assert client.get(
            "/api/interactions",
            params={"first": mara.id, "second": caldrin.id, "requireCompiled": "true"},
        ).json() == interactions_between(ash_worktree_repo, mara.id, caldrin.id, require_compiled=True)
        assert client.get(
            "/api/story-points",
            params={"scene": scene.id, "tick": 121, "timeline": "main", "order": 0, "requireCompiled": "true"},
        ).json() == story_points(ash_worktree_repo, scene.id, 121, "main", 0, require_compiled=True)
        assert client.get(
            "/api/timeline",
            params={"timeline": "main", "requireCompiled": "true"},
        ).json() == timeline(ash_worktree_repo, "main", require_compiled=True)
        assert client.get(
            "/api/threads",
            params={"requireCompiled": "true"},
        ).json() == thread_catalog(ash_worktree_repo, require_compiled=True)
        assert client.get(
            "/api/whereabouts",
            params={"character": mara.id, "tick": 121, "timeline": "main", "order": 0, "requireCompiled": "true"},
        ).json() == whereabouts(ash_worktree_repo, mara.id, 121, "main", 0, require_compiled=True)


def test_api_search_include_text_and_strict_cache_flags_match_cli(ash_worktree_repo) -> None:
    world = ash_worktree_repo.load_world()
    scene = world.find("After the Exchange", "scene")
    params = {
        "q": "flood register",
        "perspective": "author",
        "scene": scene.id,
        "mode": "fts",
        "includeText": "true",
        "requireCompiled": "true",
    }

    with TestClient(create_app(ash_worktree_repo.root)) as client:
        response = client.get("/api/search", params=params)

    assert response.status_code == 200
    assert response.json() == search_world(
        ash_worktree_repo,
        "flood register",
        perspective="author",
        scene_id=scene.id,
        mode="fts",
        include_text=True,
        require_compiled=True,
    )
    assert any("text" in result for result in response.json()["results"])
