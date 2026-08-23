from __future__ import annotations

from copy import deepcopy
from pathlib import Path

from fastapi.testclient import TestClient
import pytest

import wedl.query as query_module
from wedl.ids import id_from_seed
from wedl.model import ORDER_MAX, TICK_MAX, World
from wedl.query import whereabouts
from wedl.server import create_app


def _event(world: World, *, seed: str, title: str, time: dict[str, int | str], status: str, effects: list[dict[str, object]]):
    value = deepcopy(next(record for record in world.by_kind("event") if record.status == "canonical"))
    value.frontmatter.update({
        "id": id_from_seed("event", seed), "title": title, "status": status,
        "time": time, "effects": effects, "participants": [], "causes": [], "related_story_points": [],
    })
    value.source_path = f"story/events/main/{seed}.md"
    return value


def _projection_world(frontiersmen_repo) -> tuple[World, dict[str, str]]:
    original = frontiersmen_repo.load_world()
    records = dict(original.records)
    rhea = deepcopy(original.find("Rhea", "character"))
    asha = deepcopy(original.find("Corporal Asha Pell", "character"))
    nolly = deepcopy(original.find("Nolly Dey", "character"))
    jorund = original.find("Jorund", "character")
    road = original.find("Abandoned Warden Road", "location")
    crossing = original.find("Blackwater Crossing", "location")
    rhea.frontmatter["initial_state"] = {"location": {"entity": road.id}}
    asha.frontmatter["initial_state"] = {"location": {"entity": road.id}}
    nolly.frontmatter["initial_state"] = {"location": {"entity": road.id}}
    draft = deepcopy(nolly)
    draft.frontmatter["id"] = id_from_seed("character", "whereabouts-draft")
    draft.frontmatter["title"] = "Draft scout"
    draft.frontmatter["status"] = "draft"
    draft.source_path = "story/characters/draft-scout.md"
    second_front = deepcopy(original.find("Drowned Waymark", "scene"))
    second_front.frontmatter.update({
        "id": id_from_seed("scene", "whereabouts-second-front"), "title": "The northern watch",
        "status": "active", "location": road.id,
        "time": {"start": {"timeline": "main", "tick": 210, "order": 0}, "current": {"timeline": "main", "tick": 210, "order": 0}},
        "participants": [{"character": asha.id, "role": "watch", "from": {"timeline": "main", "tick": 210, "order": 0}}],
    })
    second_front.source_path = "story/scenes/the-northern-watch.md"
    planned_front = deepcopy(second_front)
    planned_front.frontmatter.update({
        "id": id_from_seed("scene", "whereabouts-planned-front"), "title": "Planned northern watch", "status": "planned",
        "time": {"start": {"timeline": "main", "tick": 200, "order": 0}, "current": {"timeline": "main", "tick": 206, "order": 0}, "end": {"timeline": "main", "tick": 206, "order": 0}},
        "participants": [{"character": rhea.id, "role": "planned", "from": {"timeline": "main", "tick": 200, "order": 0}, "to": {"timeline": "main", "tick": 206, "order": 0}}],
    })
    planned_front.source_path = "story/scenes/planned-northern-watch.md"
    abandoned_front = deepcopy(planned_front)
    abandoned_front.frontmatter.update({"id": id_from_seed("scene", "whereabouts-abandoned-front"), "title": "Abandoned northern watch", "status": "abandoned"})
    abandoned_front.source_path = "story/scenes/abandoned-northern-watch.md"
    records.update({
        rhea.id: rhea, asha.id: asha, nolly.id: nolly, draft.id: draft, second_front.id: second_front,
        planned_front.id: planned_front, abandoned_front.id: abandoned_front,
    })
    for event in (
        _event(original, seed="whereabouts-move", title="Rhea reaches the crossing", status="canonical", time={"timeline": "main", "tick": 205, "order": 0}, effects=[{"target": rhea.id, "key": "location", "operation": "set", "value": {"entity": crossing.id}}]),
        _event(original, seed="whereabouts-reaffirm", title="Rhea confirms the crossing", status="canonical", time={"timeline": "main", "tick": 205, "order": 1}, effects=[{"target": rhea.id, "key": "location", "operation": "set", "value": {"entity": crossing.id}}]),
        _event(original, seed="whereabouts-draft", title="Unaccepted draft move", status="draft", time={"timeline": "main", "tick": 206, "order": 0}, effects=[{"target": rhea.id, "key": "location", "operation": "set", "value": {"entity": road.id}}]),
    ):
        records[event.id] = event
    world_record = deepcopy(original.world_record)
    world_record.frontmatter["current_time"] = {"timeline": "main", "tick": 210, "order": 0}
    records[world_record.id] = world_record
    return World(original.revision, original.tree_oid, records, original.root, original.source_root, original.is_worktree), {
        "rhea": rhea.id, "asha": asha.id, "nolly": nolly.id, "jorund": jorund.id,
        "road": road.id, "crossing": crossing.id,
    }


def test_whereabouts_projects_independent_locations_journeys_and_statuses(frontiersmen_repo, monkeypatch) -> None:
    world, ids = _projection_world(frontiersmen_repo)
    monkeypatch.setattr(query_module, "require_database", lambda *_args, **_kwargs: (world, Path("projection.sqlite")))

    result = whereabouts(frontiersmen_repo)

    assert result["protocol"] == "wedl-whereabouts/v1"
    assert result["effectiveTime"] == {"timeline": "main", "tick": "210", "order": "0"}
    assert result["characterPolicy"]["includedStatuses"] == ["canonical", "retired"]
    assert result["characterPolicy"]["inference"] == "none"
    by_id = {item["character"]["id"]: item for item in result["characters"]}
    assert ids["nolly"] in by_id and by_id[ids["nolly"]]["recordStatus"] == "retired"
    assert all(item["character"]["title"] != "Draft scout" for item in result["characters"])
    assert by_id[ids["asha"]]["presence"] == "active-scene"
    assert by_id[ids["nolly"]]["presence"] == "offstage"
    assert by_id[ids["jorund"]]["presence"] == "unlocated"
    assert by_id[ids["jorund"]]["location"] is None
    rhea = by_id[ids["rhea"]]
    # The real story advances Rhea again at 207; the synthetic Crossing beats
    # are asserted at their own 205:1 horizon below.
    assert rhea["location"]["id"] == ids["road"]
    assert rhea["lastKnownLocation"] is None
    assert all(item["event"] is None or item["event"]["title"] != "Unaccepted draft move" for item in rhea["journey"])
    assert {item["location"]["id"] for item in result["locations"]} >= {ids["road"], ids["crossing"]}
    assert len(result["activeScenes"]) == 2
    assert any(item["character"]["id"] == ids["nolly"] for item in result["characters"])
    assert all("knowledge" not in item for item in result["characters"])
    rhea_only = whereabouts(frontiersmen_repo, "Rhea", 210, "main", 0)
    assert [item["scene"]["title"] for item in rhea_only["activeScenes"]] == ["Southward Cut"]


def test_whereabouts_filter_horizon_clear_and_signed_transport(frontiersmen_repo, monkeypatch) -> None:
    world, ids = _projection_world(frontiersmen_repo)
    records = dict(world.records)
    clear = _event(world, seed="whereabouts-clear", title="Rhea disappears from the crossing", status="canonical", time={"timeline": "main", "tick": TICK_MAX, "order": ORDER_MAX}, effects=[{"target": ids["rhea"], "key": "location", "operation": "clear"}])
    records[clear.id] = clear
    world = World(world.revision, world.tree_oid, records, world.root, world.source_root, world.is_worktree)
    monkeypatch.setattr(query_module, "require_database", lambda *_args, **_kwargs: (world, Path("projection.sqlite")))

    before = whereabouts(frontiersmen_repo, "Rhea", 205, "main", 1)
    after = whereabouts(frontiersmen_repo, "Rhea", TICK_MAX, "main", ORDER_MAX)

    assert before["characterFilter"]["title"] == "Rhea"
    assert len(before["characters"]) == 1
    assert before["characters"][0]["location"]["id"] == ids["crossing"]
    assert before["characters"][0]["presence"] == "active-scene"
    assert before["characters"][0]["activeScene"]["title"] == "Drowned Waymark"
    assert before["characters"][0]["lastKnownLocation"] is None
    assert [item["kind"] for item in before["characters"][0]["journey"][-2:]] == ["move", "reaffirmation"]
    assert before["characters"][0]["journey"][-1]["event"]["title"] == "Rhea confirms the crossing"
    assert before["characters"][0]["journey"][-1]["at"] == {"timeline": "main", "tick": "205", "order": "1"}
    assert [item["scene"]["title"] for item in before["activeScenes"]] == ["Drowned Waymark"]
    rhea = after["characters"][0]
    assert after["effectiveTime"] == {"timeline": "main", "tick": str(TICK_MAX), "order": str(ORDER_MAX)}
    assert rhea["presence"] == "unlocated"
    assert rhea["location"] is None
    assert rhea["lastKnownLocation"]["id"] == ids["road"]
    assert rhea["journey"][-1] == {
        "kind": "clear", "at": {"timeline": "main", "tick": str(TICK_MAX), "order": str(ORDER_MAX)},
        "from": {"id": ids["road"], "kind": "location", "title": "Abandoned Warden Road"},
        "to": None, "event": {"id": clear.id, "kind": "event", "title": "Rhea disappears from the crossing"},
    }


def test_whereabouts_http_matches_the_query(ash_repo) -> None:
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    with TestClient(create_app(ash_repo.root)) as client:
        response = client.get("/api/whereabouts", params={"character": "Mara Vale", "tick": 121, "timeline": "main", "order": 0, "requireCompiled": "true"})

    assert response.status_code == 200
    assert response.json() == whereabouts(ash_repo, mara.id, 121, "main", 0, require_compiled=True)


def test_whereabouts_rejects_partial_horizons_before_repository_access(frontiersmen_repo, monkeypatch) -> None:
    monkeypatch.setattr(query_module, "require_database", lambda *_args, **_kwargs: (_ for _ in ()).throw(AssertionError("repository was opened")))
    with pytest.raises(Exception, match="require --tick"):
        whereabouts(frontiersmen_repo, timeline="main")
