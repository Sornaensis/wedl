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


def test_conversation_web_ui_uses_author_scopes_and_safe_static_contract(tmp_path: Path) -> None:
    repository = disposable_ash_repository(tmp_path)
    with TestClient(create_app(repository.root)) as client:
        entities = client.get("/api/entities")
        assert entities.status_code == 200
        conversation = next(item for item in entities.json() if item["kind"] == "conversation" and item["title"] == "Whispers in Flood Gallery N")

        as_of = client.get(f"/api/conversations/{conversation['id']}", params={"perspective": "author"})
        all_time = client.get(f"/api/conversations/{conversation['id']}", params={"perspective": "author", "allTime": "true"})
        assert as_of.status_code == all_time.status_code == 200
        as_of_payload = as_of.json()
        all_time_payload = all_time.json()
        assert as_of_payload["protocol"] == all_time_payload["protocol"] == "wedl-conversation/v2"
        assert as_of_payload["perspective"] == all_time_payload["perspective"] == "author"
        assert as_of_payload["timeScope"]["mode"] == "as-of"
        assert as_of_payload["effectiveTime"] == as_of_payload["conversationTime"]["end"]
        assert as_of_payload["conversationTime"] == {
            "start": {"timeline": "main", "tick": 138, "order": 50},
            "end": {"timeline": "main", "tick": 142, "order": 40},
        }
        assert all_time_payload["timeScope"] == {"mode": "all-time"}
        assert all_time_payload["effectiveTime"] is None
        assert as_of_payload["verbatimTurns"]
        turn = as_of_payload["verbatimTurns"][0]
        assert {"id", "at", "speakerId", "speaker", "text", "citation"} <= set(turn)
        assert {"entityId", "sourcePath", "section"} <= set(turn["citation"])
        coordinates = [(item["at"]["timeline"], item["at"]["tick"], item["at"]["order"]) for item in as_of_payload["verbatimTurns"]]
        assert coordinates == sorted(coordinates)
        assert isinstance(as_of_payload["recollections"], list)

        invalid_scope = client.get(
            f"/api/conversations/{conversation['id']}",
            params={"perspective": "author", "allTime": "true", "tick": 139},
        )
        assert invalid_scope.status_code == 400
        assert invalid_scope.json()["code"] == "usage_error"
        missing = client.get("/api/conversations/not-a-real-conversation", params={"perspective": "author"})
        assert missing.status_code == 400
        assert missing.json()["code"] == "not_found"
        assert isinstance(missing.json()["message"], str)
        assert isinstance(missing.json()["details"], dict)

        root = client.get("/")
        assert root.status_code == 200
        assert 'data-kind="conversation">Conversations</button>' in root.text
        assert 'id="author-horizon"' in root.text
        assert 'id="back-to-timeline"' in root.text

        app_module = client.get("/assets/app.js")
        query_module = client.get("/assets/query.mjs")
        assert app_module.status_code == query_module.status_code == 200
        assert "buildLoreArticle" in app_module.text
        assert "openTimeline" in app_module.text
        assert "Turn ID" not in app_module.text
        assert "Citation:" not in app_module.text
        assert "innerHTML" not in app_module.text
        assert "insertAdjacentHTML" not in app_module.text
        assert "export function conversationRequestPath" in query_module.text
        assert '''export function conversationRequestPath(conversationId, scope, at = null) {
  const parameters = new URLSearchParams({ perspective: "author" });
  if (scope === "all-time") parameters.set("allTime", "true");
  else if (at) {
    parameters.set("timeline", at.timeline);
    parameters.set("tick", String(at.tick));
    parameters.set("order", String(at.order ?? 0));
  }
  return `/api/conversations/${encodeURIComponent(conversationId)}?${parameters}`;
}''' in query_module.text
