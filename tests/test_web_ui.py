from __future__ import annotations

from importlib import resources
from pathlib import Path
import re
import shutil

from fastapi.testclient import TestClient
import pytest

from wedl.query import _browser_safe_frontmatter
from wedl.query import _location_context, _location_history
from wedl.model import StoryTime, TICK_MAX
from wedl.repository import Repository
from wedl.server import create_app
from wedl.source import extract_entity_refs, markdown_entity_links


@pytest.fixture()
def ui_ash_repo(tmp_path: Path) -> Repository:
    """A disposable read-model fixture; the UI has no Git-write dependency."""
    root = tmp_path / "ash"
    source = resources.files("wedl.data.ash_archive").joinpath("story")
    with resources.as_file(source) as source_path:
        shutil.copytree(source_path, root / "story")
    return Repository(root)


def test_web_ui_static_root_and_assets_are_served(ui_ash_repo) -> None:
    """The browser entry point must be able to load every bundled UI module."""
    with TestClient(create_app(ui_ash_repo.root)) as client:
        root = client.get("/")
        assert root.status_code == 200
        assert "text/html" in root.headers["content-type"]
        assert "<title>Lore compendium</title>" in root.text
        assert 'href="/assets/style.css"' in root.text
        assert '<script type="module" src="/assets/app.js"></script>' in root.text
        assert 'href="/assets/favicon.svg"' in root.text
        assert '<form id="entities-form"' in root.text
        labels = dict(re.findall(r'<label[^>]*for="([^"]+)"[^>]*>(.*?)</label>', root.text, flags=re.DOTALL))
        required_controls = ("entity-text", "search-kind", "author-horizon")
        for control_id in required_controls:
            assert f'id="{control_id}"' in root.text
            assert re.sub(r"<[^>]+>", "", labels.get(control_id, "")).strip()
        for region, css_class in (
            ("app-status", "app-status"),
            ("entities-status", "section-status"),
            ("article-status", "section-status"),
        ):
            assert f'<p id="{region}" class="{css_class}" role="status" aria-live="polite"' in root.text

        stylesheet = client.get("/assets/style.css")
        app_module = client.get("/assets/app.js")
        api_module = client.get("/assets/api.js")
        query_module = client.get("/assets/query.mjs")
        lore_module = client.get("/assets/lore.mjs")
        search_module = client.get("/assets/search.mjs")
        navigation_module = client.get("/assets/navigation.mjs")
        favicon = client.get("/assets/favicon.svg")

        assert stylesheet.status_code == app_module.status_code == api_module.status_code == query_module.status_code == lore_module.status_code == search_module.status_code == navigation_module.status_code == favicon.status_code == 200
        assert "text/css" in stylesheet.headers["content-type"]
        assert "javascript" in app_module.headers["content-type"]
        assert "javascript" in api_module.headers["content-type"]
        assert "javascript" in query_module.headers["content-type"]
        assert "createApiClient" in app_module.text
        assert 'from "./api.js"' in app_module.text
        assert 'from "./lore.mjs"' in app_module.text
        assert "export function createApiClient" in api_module.text
        assert "export function authorSearchRequestPath" in query_module.text
        assert "export function whereaboutsRequestPath" in query_module.text
        assert "export function entityStateRequestPath" in query_module.text
        assert "export function buildLoreArticle" in lore_module.text
        assert "export function presentSearchResults" in search_module.text
        assert "export function navigationSnapshot" in navigation_module.text
        assert 'from "./navigation.mjs"' in app_module.text
        assert "Timeline" in root.text
        assert 'data-view="whereabouts"' in root.text
        assert "Whereabouts" in root.text
        assert "Plot threads" in root.text
        assert "source_path" not in root.text
        for module in (app_module.text, api_module.text, query_module.text, lore_module.text, search_module.text):
            assert "innerHTML" not in module
            assert "insertAdjacentHTML" not in module
            assert "document.write" not in module
        assert "detail-open" in stylesheet.text
        assert ".meanwhile-group" in stylesheet.text
        assert ".whereabouts-person" in stylesheet.text
        assert "max-width: 719px" in stylesheet.text


def test_web_ui_serves_the_existing_bulk_whereabouts_projection(ui_ash_repo) -> None:
    """The exploratory view consumes the read-only projection rather than new UI state."""
    with TestClient(create_app(ui_ash_repo.root)) as client:
        response = client.get("/api/whereabouts")

    assert response.status_code == 200
    payload = response.json()
    assert payload["protocol"] == "wedl-whereabouts/v1"
    assert payload["characterPolicy"]["inference"] == "none"
    assert all("journey" in item and "presence" in item for item in payload["characters"])


def test_web_ui_api_supports_entity_selection_context_and_temporal_author_search(ui_ash_repo) -> None:
    """Cover the contracts used when selecting entities and working in the UI."""
    with TestClient(create_app(ui_ash_repo.root)) as client:
        entities_response = client.get("/api/entities")
        assert entities_response.status_code == 200
        entities = entities_response.json()
        mara = next(entity for entity in entities if entity["kind"] == "character" and entity["title"] == "Mara Vale")
        scene = next(entity for entity in entities if entity["kind"] == "scene" and entity["title"] == "An Honest Absence")

        filtered = client.get("/api/entities", params={"kind": "character", "text": "Mara"})
        assert filtered.status_code == 200
        assert [entity["id"] for entity in filtered.json()] == [mara["id"]]
        assert client.get("/api/entities", params={"kind": "character", "text": "not-a-real-title"}).json() == []

        # Selecting an item in the entity list drives this detail request.
        detail = client.get(f"/api/entities/{mara['id']}")
        assert detail.status_code == 200
        detail_payload = detail.json()
        assert detail_payload["id"] == mara["id"]
        assert detail_payload["title"] == "Mara Vale"
        assert detail_payload["kind"] == "character"
        assert detail_payload["status"] == "canonical"
        assert detail_payload["domain"]
        assert detail_payload["source_path"].endswith("mara.md")
        assert detail_payload["frontmatter"]["id"] == mara["id"]
        assert detail_payload["frontmatter"]["kind"] == "character"
        assert isinstance(detail_payload["bodyMarkdown"], str)

        scene_detail = client.get(f"/api/entities/{scene['id']}")
        assert scene_detail.status_code == 200
        scene_frontmatter = scene_detail.json()["frontmatter"]
        # Detail is consumed by browser-side horizon filtering, which compares
        # coordinates as BigInts.  Keep the transport exact just like timeline.
        assert isinstance(scene_frontmatter["time"]["start"]["tick"], str)
        assert isinstance(scene_frontmatter["time"]["start"]["order"], str)

        context = client.get(
            "/api/context",
            params={"character": mara["id"], "scene": scene["id"], "q": "register ribbon"},
        )
        assert context.status_code == 200
        context_payload = context.json()
        assert context_payload["protocol"] == "wedl-context/v3"
        assert context_payload["characterId"] == mara["id"]
        assert context_payload["sceneId"] == scene["id"]
        assert context_payload["perspective"] == "character"
        assert context_payload["effectiveTime"] == {"timeline": "main", "tick": 208, "order": 0}
        assert context_payload["promptText"]

        # The UI must send either a selected scene (as-of) or author-only allTime,
        # never both temporal forms in one request.
        scoped_search = client.get(
            "/api/search",
            params={"q": "flood register", "perspective": "author", "scene": scene["id"]},
        )
        all_time_search = client.get(
            "/api/search",
            params={"q": "flood register", "perspective": "author", "allTime": "true"},
        )
        assert scoped_search.status_code == all_time_search.status_code == 200
        scoped_payload = scoped_search.json()
        assert scoped_payload["protocol"] == "wedl-search/v5"
        assert scoped_payload["perspective"] == "author"
        assert scoped_payload["characterId"] is None
        assert scoped_payload["sceneId"] == scene["id"]
        assert scoped_payload["effectiveTime"] == {"timeline": "main", "tick": 208, "order": 0}
        assert scoped_payload["timeScope"]["mode"] == "as-of"
        all_time_payload = all_time_search.json()
        assert all_time_payload["protocol"] == "wedl-search/v5"
        assert all_time_payload["perspective"] == "author"
        assert all_time_payload["characterId"] is None
        assert all_time_payload["sceneId"] is None
        assert all_time_payload["effectiveTime"] is None
        assert all_time_payload["timeScope"] == {"mode": "all-time"}

        # Search is indexed full text, not only title filtering; the browser
        # binds a named author horizon through this explicit temporal scope.
        horizon_search = client.get(
            "/api/search",
            params={"q": "flood register", "perspective": "author", "timeline": "main", "tick": 208, "order": 0},
        )
        assert horizon_search.status_code == 200
        horizon_payload = horizon_search.json()
        assert horizon_payload["timeScope"] == {"mode": "as-of", "at": {"timeline": "main", "tick": 208, "order": 0}}
        assert horizon_payload["results"]

        state = client.get(f"/api/entities/{mara['id']}/state", params={"timeline": "main", "tick": 208, "order": 0})
        knowledge = client.get(f"/api/entities/{mara['id']}/knowledge", params={"timeline": "main", "tick": 208, "order": 0})
        assert state.status_code == knowledge.status_code == 200
        assert state.json()["at"] == knowledge.json()["at"] == {"timeline": "main", "tick": 208, "order": 0}


def test_web_ui_detail_serializes_nested_story_times_without_signed64_precision_loss() -> None:
    """The detail serializer supplies the exact coordinate shape consumed by lore.mjs."""
    first, next_tick = 2**53 + 1, 2**53 + 2
    frontmatter = _browser_safe_frontmatter(
        {
            "time": {
                "start": {"timeline": "main", "tick": first, "order": 2_147_483_647},
                "end": {"timeline": "main", "tick": next_tick, "order": -2_147_483_648},
            },
            "participants": [{
                "character": "char_123", "from": {"timeline": "main", "tick": next_tick, "order": -2_147_483_648},
                "to": {"timeline": "main", "tick": next_tick, "order": 0},
            }],
            "observations": [{
                "text": "The second beat arrives.", "at": {"timeline": "main", "tick": next_tick, "order": -2_147_483_648},
                "until": {"timeline": "main", "tick": next_tick, "order": 0},
            }],
        },
        "main",
    )
    assert frontmatter["time"]["start"] == {"timeline": "main", "tick": str(first), "order": "2147483647"}
    assert frontmatter["time"]["end"] == {"timeline": "main", "tick": str(next_tick), "order": "-2147483648"}
    for point in (
        frontmatter["participants"][0]["from"], frontmatter["participants"][0]["to"],
        frontmatter["observations"][0]["at"], frontmatter["observations"][0]["until"],
    ):
        assert isinstance(point["tick"], str)
        assert isinstance(point["order"], str)


def test_location_read_projections_keep_named_directional_routes_and_exact_history(ui_ash_repo) -> None:
    """Location projections are additive, directional, and safe for browser use."""
    world = ui_ash_repo.load_world()
    first, second, third = world.by_kind("location")[:3]
    first.frontmatter["links"] = [second.id]
    second.frontmatter["links"] = [first.id]
    third.frontmatter["links"] = [{"location": first.id, "description": f"A hidden route toward {second.id}.", "summary": "A narrow stone passage under the stacks."}]
    third.frontmatter["parent"] = first.id
    second.body = "A narrow reading room catches the river light."
    context = _location_context(world, first)
    assert any(item["id"] == third.id and item["title"] == third.title for item in context["children"])
    assert context["outgoing"] == [{"place": {"id": second.id, "kind": "location", "title": second.title}, "reciprocal": True, "summary": "A narrow reading room catches the river light."}]
    incoming = {item["place"]["id"]: item for item in context["incoming"]}
    assert {location_id: item["reciprocal"] for location_id, item in incoming.items()} == {second.id: True, third.id: False}
    assert incoming[third.id]["description"] == f"A hidden route toward {second.id}."
    assert incoming[third.id]["summary"] == "A narrow stone passage under the stacks."

    character = world.find("Mara Vale", "character")
    event = next(record for record in world.by_kind("event") if record.status == "canonical")
    event.frontmatter["time"] = {"timeline": "main", "tick": TICK_MAX, "order": 0}
    event.frontmatter["effects"] = [
        {"target": character.id, "key": "location", "operation": "set", "value": {"entity": second.id}},
        {"target": character.id, "key": "location", "operation": "clear"},
    ]
    world._cache.pop("canonical-events", None)
    history = _location_history(world, character, StoryTime("main", TICK_MAX, 0))
    assert history[-2:] == [
        {"operation": "set", "at": {"timeline": "main", "tick": str(TICK_MAX), "order": "0"}, "location": {"id": second.id, "kind": "location", "title": second.title}, "event": {"id": event.id, "kind": "event", "title": event.title}},
        {"operation": "clear", "at": {"timeline": "main", "tick": str(TICK_MAX), "order": "0"}, "location": None, "event": {"id": event.id, "kind": "event", "title": event.title}},
    ]


def test_location_detail_api_keeps_prose_from_incoming_authored_route(ui_ash_repo) -> None:
    """The destination view retains the source route's authored prose."""
    world = ui_ash_repo.load_world()
    destination, source = world.by_kind("location")[:2]
    source_path = ui_ash_repo.root / source.source_path
    source_path.write_bytes(
        source_path.read_bytes().decode("utf-8").replace(
            "links: []",
            "links:\n"
            f"- location: {destination.id}\n"
            f"  description: A quiet return toward {destination.id}.\n"
            "  summary: An undercroft passage lit by a single lamp.",
            1,
        ).encode("utf-8"),
    )

    # Use a new repository object after changing the disposable fixture so the
    # HTTP app reads the authored route rather than the prior in-memory world.
    with TestClient(create_app(Repository(ui_ash_repo.root).root)) as client:
        response = client.get(f"/api/entities/{destination.id}")

    assert response.status_code == 200
    incoming = next(item for item in response.json()["locationContext"]["incoming"] if item["place"]["id"] == source.id)
    assert incoming["description"] == f"A quiet return toward {destination.id}."
    assert incoming["summary"] == "An undercroft passage lit by a single lamp."


def test_web_ui_entity_backlinks_are_explicit_authored_references_not_timeline_inference(ui_ash_repo) -> None:
    """The compendium may link authored references, never records merely nearby in time."""
    world = ui_ash_repo.load_world()
    scene = world.find("Flood Gallery N", "scene")
    authored_sources = {
        record.id
        for record in world.records.values()
        if record.id != scene.id
        and scene.id in (extract_entity_refs(record.frontmatter) | markdown_entity_links(record.body))
    }
    scene_time = scene.frontmatter["time"]
    start = (scene_time["start"]["tick"], scene_time["start"].get("order", 0))
    end = (scene_time["end"]["tick"], scene_time["end"].get("order", 0))
    nearby_unlinked_event = next(
        record
        for record in world.by_kind("event")
        if (time := record.frontmatter.get("time"))
        and time.get("timeline", world.default_timeline) == scene_time["start"]["timeline"]
        and start <= (time["tick"], time.get("order", 0)) <= end
        and record.id not in authored_sources
    )

    with TestClient(create_app(ui_ash_repo.root)) as client:
        payload = client.get(f"/api/entities/{scene.id}").json()

    backlinks = payload["inboundReferences"]
    assert {item["id"] for item in backlinks} == authored_sources
    assert nearby_unlinked_event.id not in {item["id"] for item in backlinks}
    assert all(set(item) == {"id", "kind", "title"} and item["title"] for item in backlinks)


def test_web_ui_api_returns_actionable_error_contracts(ui_ash_repo) -> None:
    """UI clients receive structured errors for invalid selections and time scopes."""
    with TestClient(create_app(ui_ash_repo.root)) as client:
        missing_entity = client.get("/api/entities/not-a-real-entity")
        invalid_time_scope = client.get(
            "/api/search",
            params={"q": "flood", "perspective": "author", "allTime": "true", "tick": -1},
        )

        for response in (missing_entity, invalid_time_scope):
            assert response.status_code == 400
            assert set(response.json()) == {"code", "message", "details"}
            assert isinstance(response.json()["message"], str)
            assert response.json()["message"]
            assert response.json()["details"] == {}
        assert missing_entity.json()["code"] == "not_found"
        assert invalid_time_scope.json()["code"] == "usage_error"
