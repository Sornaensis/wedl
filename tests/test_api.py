from __future__ import annotations

from fastapi.testclient import TestClient

from wedl import query as query_module
from wedl.query import status
from wedl.server import create_app


def test_api_serves_context_and_protects_writes(ash_repo) -> None:
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    scene = world.find("An Honest Absence", "scene")
    with TestClient(create_app(ash_repo.root)) as client:
        session = client.get("/api/session").json()
        assert len(session["token"]) > 20
        api_status = client.get("/api/status").json()
        assert api_status["recordCount"] == 262
        assert api_status["timeModel"] == status(ash_repo)["timeModel"]
        timeline = client.get("/api/timeline")
        assert timeline.status_code == 200
        assert timeline.json()["protocol"] == "wedl-timeline/v1"
        assert timeline.json()["temporalSemantics"]["durationSemantics"] == "none"
        unknown_timeline = client.get("/api/timeline", params={"timeline": "unwritten"})
        assert unknown_timeline.status_code == 400
        assert unknown_timeline.json()["code"] == "usage_error"
        assert unknown_timeline.json()["details"]["availableTimelines"] == ["main"]
        context = client.get("/api/context", params={"character": mara.id, "scene": scene.id, "q": "register ribbon"})
        assert context.status_code == 200
        assert "promptText" in context.json()
        timed_context = client.get("/api/context", params={"character": mara.id, "scene": scene.id, "timeline": "main", "tick": 208, "order": 0})
        assert timed_context.status_code == 200
        assert timed_context.json()["effectiveTime"] == {"timeline": "main", "tick": 208, "order": 0}
        context_parameters = client.get("/openapi.json").json()["paths"]["/api/context"]["get"]["parameters"]
        assert {item["name"] for item in context_parameters} >= {"timeline", "tick", "order"}
        assert "allTime" not in {item["name"] for item in context_parameters}

        derived = client.get("/api/search", params={"q": "flood register", "perspective": "author", "scene": scene.id, "mode": "fts"})
        assert derived.status_code == 200
        assert derived.json()["protocol"] == "wedl-search/v5"
        assert derived.json()["timeScope"]["mode"] == "as-of"
        all_time = client.get("/api/search", params={"q": "flood register", "perspective": "author", "mode": "fts", "allTime": "true"})
        assert all_time.status_code == 200
        assert all_time.json()["timeScope"] == {"mode": "all-time"}
        assert client.get("/api/search", params={"q": "flood", "perspective": "author", "allTime": "true", "tick": -1}).status_code == 400
        assert client.get("/api/search", params={"q": "flood", "perspective": "character", "allTime": "true"}).status_code == 400

        conversation = world.find("Whispers in Flood Gallery N", "conversation")
        conversation_as_of = client.get(f"/api/conversations/{conversation.id}", params={"perspective": "author", "tick": 139, "order": 10})
        conversation_all_time = client.get(f"/api/conversations/{conversation.id}", params={"perspective": "author", "allTime": "true"})
        assert conversation_as_of.status_code == 200
        assert conversation_as_of.json()["protocol"] == "wedl-conversation/v2"
        assert conversation_as_of.json()["timeScope"]["mode"] == "as-of"
        assert len(conversation_as_of.json()["verbatimTurns"]) == 3
        assert conversation_all_time.status_code == 200
        assert conversation_all_time.json()["timeScope"] == {"mode": "all-time"}
        assert len(conversation_all_time.json()["verbatimTurns"]) > len(conversation_as_of.json()["verbatimTurns"])
        assert client.get(f"/api/conversations/{conversation.id}", params={"perspective": "author", "allTime": "true", "tick": 139}).status_code == 400
        assert client.get(f"/api/conversations/{conversation.id}", params={"perspective": "character", "character": mara.id, "allTime": "true"}).status_code == 400
        assert client.get("/api/search", params={"q": "flood", "limit": "0"}).json()["code"] == "usage_error"
        assert client.get("/api/search", params={"q": "flood", "limit": "0"}).status_code == 400
        unauthenticated = client.post("/api/compile")
        assert unauthenticated.status_code == 401
        assert unauthenticated.json() == {
            "code": "authentication_required",
            "message": "invalid X-Wedl-Token",
            "details": {},
        }
        assert client.post("/api/compile", headers={"X-Wedl-Token": session["token"]}).status_code == 200


def test_api_timeline_reports_cross_timeline_span_data_as_a_structured_error(ash_repo, monkeypatch) -> None:
    world = ash_repo.load_world()
    world.world_record.frontmatter["timelines"] = [
        {"id": "main", "label": "Main chronology"},
        {"id": "aftermath", "label": "Aftermath"},
    ]
    scene = world.by_kind("scene")[0]
    scene.frontmatter["time"]["end"] = {"timeline": "aftermath", "tick": 1, "order": 0}
    monkeypatch.setattr(query_module, "require_database", lambda _repository, require_compiled=False: (world, ash_repo.root / "timeline.sqlite"))

    with TestClient(create_app(ash_repo.root)) as client:
        response = client.get("/api/timeline", params={"timeline": "main"})

    assert response.status_code == 400
    assert response.json()["code"] == "usage_error"
    assert response.json()["details"]["field"] == "end"
    assert response.json()["details"]["startTimeline"] == "main"
