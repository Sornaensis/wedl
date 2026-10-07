from __future__ import annotations

from importlib import resources
from pathlib import Path
import shutil

from fastapi.testclient import TestClient

from wedl.repository import Repository
from wedl.server import create_app


def disposable_ash_repository(tmp_path: Path) -> Repository:
    root = tmp_path / "ash"
    source = Path(__file__).resolve().parent / "fixtures" / "legacy_worlds" / "ash_archive" / "story"
    with resources.as_file(source) as source_path:
        shutil.copytree(source_path, root / "story")
    return Repository(root)


def test_story_points_web_ui_uses_selected_scene_and_safe_static_contract(tmp_path: Path) -> None:
    repository = disposable_ash_repository(tmp_path)
    with TestClient(create_app(repository.root)) as client:
        entities = client.get("/api/entities")
        assert entities.status_code == 200
        scene = next(item for item in entities.json() if item["kind"] == "scene" and item["title"] == "An Honest Absence")

        response = client.get("/api/story-points", params={"scene": scene["id"]})
        assert response.status_code == 200
        payload = response.json()
        assert payload["sceneId"] == scene["id"]
        assert payload["evaluatedAt"] == {"timeline": "main", "tick": 208, "order": 0}
        assert payload["storyPoints"]
        assert {"storyPointId", "title", "storedState", "derivedState", "eligible", "dependenciesSatisfied", "triggerSatisfied", "priority"} <= set(payload["storyPoints"][0])
        states = {item["title"]: item["derivedState"] for item in payload["storyPoints"]}
        assert states["Build the Incomplete Archive"] == "resolved"

        blocked_scene = next(item for item in entities.json() if item["kind"] == "scene" and item["title"] == "After the Exchange")
        blocked_response = client.get("/api/story-points", params={"scene": blocked_scene["id"]})
        assert blocked_response.status_code == 200
        blocked_states = {item["title"]: item["derivedState"] for item in blocked_response.json()["storyPoints"]}
        assert blocked_states["Build the Incomplete Archive"] == "blocked"

        invalid_scene = client.get("/api/story-points", params={"scene": "not-a-real-scene"})
        assert invalid_scene.status_code == 400
        error = invalid_scene.json()
        assert error["code"] == "not_found"
        assert "could not resolve" in error["message"]
        assert isinstance(error["details"], dict)

        root = client.get("/")
        assert root.status_code == 200
        assert 'data-kind="story-point">Plot threads</button>' in root.text
        assert 'id="author-horizon"' in root.text
        assert 'id="entity-detail-content"' in root.text

        app_module = client.get("/assets/app.js")
        query_module = client.get("/assets/query.mjs")
        assert app_module.status_code == query_module.status_code == 200
        assert "buildLoreArticle" in app_module.text
        assert "Plot thread" in client.get("/assets/lore.mjs").text
        assert "Story point ID" not in app_module.text
        assert "Inspect source" not in app_module.text
        assert "innerHTML" not in app_module.text
        assert "insertAdjacentHTML" not in app_module.text
        assert "export function storyPointsRequestPath" in query_module.text
