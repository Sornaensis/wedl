from __future__ import annotations

from copy import deepcopy
from pathlib import Path

from fastapi.testclient import TestClient
import pytest

import wedl.query as query_module
from wedl.changeset import ProtocolError, preview
from wedl.ids import id_from_seed
from wedl.model import ORDER_MAX, TICK_MAX, TICK_MIN, StoryTime, World
from wedl.query import character_importance, whereabouts
from wedl.server import create_app
from wedl.source import serialize_record
from wedl.validation import validate_world


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

    assert result["protocol"] == "wedl-whereabouts/v2"
    assert result["importancePolicy"]["algorithm"] == "wedl-character-importance/v1"
    assert result["importancePolicy"]["weights"] == {"scenes": 40, "pointOfViewScenes": 25, "events": 20, "relationshipNeighbors": 15}
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
    assert rhea["role"] == "shield-veteran"
    assert rhea["importance"]["algorithm"] == "wedl-character-importance/v1"
    assert set(rhea["importance"]["raw"]) == {"scenes", "pointOfViewScenes", "events", "relationshipNeighbors"}
    # The real story advances Rhea again at 207; the synthetic Crossing beats
    # are asserted at their own 205:1 horizon below.
    assert rhea["location"]["id"] == ids["road"]
    assert rhea["lastKnownLocation"] is None
    assert all(item["event"] is None or item["event"]["title"] != "Unaccepted draft move" for item in rhea["journey"])
    assert ids["road"] in {item["location"]["id"] for item in result["locations"]}
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
    full_before = whereabouts(frontiersmen_repo, None, 205, "main", 1)

    assert before["characterFilter"]["title"] == "Rhea Marrow"
    assert len(before["characters"]) == 1
    # Filtering happens after cohort normalization, not against a singleton.
    assert before["characters"][0]["importance"] == next(item["importance"] for item in full_before["characters"] if item["character"]["id"] == ids["rhea"])
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


def test_importance_golden_formula_and_zero_maxima(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    rhea = world.find("Rhea", "character")
    golden = character_importance(world, StoryTime("main", 210, 0))[rhea.id]
    assert golden["raw"] == {"scenes": 22, "pointOfViewScenes": 9, "events": 43, "relationshipNeighbors": 8}
    assert golden["normalized"] == {"scenes": 1.0, "pointOfViewScenes": 1.0, "events": 0.98839, "relationshipNeighbors": 1.0}
    assert golden["contributions"] == {"scenes": 40.0, "pointOfViewScenes": 25.0, "events": 19.77, "relationshipNeighbors": 15.0}
    assert golden["score"] == 99.77
    assert all(item["score"] == 0.0 for item in character_importance(world, StoryTime("main", -1, 0)).values())


def test_relationship_neighbors_are_reciprocal_and_deduplicated_per_timeline(frontiersmen_repo) -> None:
    original = frontiersmen_repo.load_world()
    relationship = deepcopy(next(record for record in original.by_kind("relationship") if record.frontmatter.get("from") and record.frontmatter.get("to")))
    relationship.frontmatter["transitions"] = [{"id": "rt_main", "time": {"timeline": "main", "tick": 1, "order": 0}}, {"id": "rt_branch", "time": {"timeline": "branch", "tick": 1, "order": 0}}]
    reciprocal = deepcopy(relationship); reciprocal.frontmatter["id"] = id_from_seed("relationship", "importance-reciprocal")
    reciprocal.frontmatter["from"], reciprocal.frontmatter["to"] = relationship.frontmatter["to"], relationship.frontmatter["from"]
    reciprocal.frontmatter["transitions"] = [{"id": "rt_recip_main", "time": {"timeline": "main", "tick": 2, "order": 0}}, {"id": "rt_recip_branch", "time": {"timeline": "branch", "tick": 2, "order": 0}}]
    baseline = character_importance(original, StoryTime("main", 2, 0))
    world = World(original.revision, original.tree_oid, {**original.records, relationship.id: relationship, reciprocal.id: reciprocal}, original.root, original.source_root, original.is_worktree)
    main = character_importance(world, StoryTime("main", 2, 0)); branch = character_importance(world, StoryTime("branch", 2, 0))
    source, target = str(relationship.frontmatter["from"]), str(relationship.frontmatter["to"])
    assert main[source]["raw"]["relationshipNeighbors"] == baseline[source]["raw"]["relationshipNeighbors"]
    assert main[target]["raw"]["relationshipNeighbors"] == baseline[target]["raw"]["relationshipNeighbors"]
    assert branch[source]["raw"]["relationshipNeighbors"] == 1
    assert branch[target]["raw"]["relationshipNeighbors"] == 1


def test_calculated_importance_never_mutates_or_enters_canonical_frontmatter(frontiersmen_repo, monkeypatch) -> None:
    world, _ids = _projection_world(frontiersmen_repo)
    original = {record.id: (deepcopy(record.frontmatter), record.raw_bytes) for record in world}
    monkeypatch.setattr(query_module, "require_database", lambda *_args, **_kwargs: (world, Path("projection.sqlite")))
    whereabouts(frontiersmen_repo, tick=210, timeline="main", order=0)
    assert {record.id: (record.frontmatter, record.raw_bytes) for record in world} == original

    invalid_character = deepcopy(world.find("Rhea", "character"))
    invalid_character.frontmatter["importance"] = 100
    with pytest.raises(Exception, match="cannot be serialized"):
        serialize_record(invalid_character.frontmatter, invalid_character.body)
    invalid = World(world.revision, world.tree_oid, {**world.records, invalid_character.id: invalid_character}, world.root, world.source_root, world.is_worktree)
    assert any(item["code"] == "WDL-SRC-007" for item in validate_world(invalid))


def test_importance_signals_horizons_and_shared_index(frontiersmen_repo, monkeypatch) -> None:
    original = frontiersmen_repo.load_world(); people = [deepcopy(original.find(name, "character")) for name in ("Rhea", "Corporal Asha Pell", "Nolly Dey")]
    for person in people: person.frontmatter["status"] = "canonical"
    people[2].frontmatter["status"] = "retired"
    first, second, third = people
    draft = deepcopy(third); draft.frontmatter.update({"id": id_from_seed("character", "importance-draft"), "status": "draft"})
    scene = deepcopy(next(item for item in original.by_kind("scene") if item.status == "active")); scene.frontmatter.update({"id": id_from_seed("scene", "importance-active"), "status": "active", "time": {"start": {"timeline": "main", "tick": 5, "order": 0}, "current": {"timeline": "main", "tick": 8, "order": 0}}, "participants": [{"character": first.id, "from": {"timeline": "main", "tick": 5, "order": 0}, "point_of_view": True}]})
    closed = deepcopy(scene); closed.frontmatter.update({"id": id_from_seed("scene", "importance-closed"), "status": "closed", "time": {"start": {"timeline": "main", "tick": 7, "order": 0}, "current": {"timeline": "main", "tick": 7, "order": 0}, "end": {"timeline": "main", "tick": 7, "order": 0}}, "participants": [{"character": second.id, "from": {"timeline": "main", "tick": 7, "order": 0}}]})
    planned = deepcopy(closed); planned.frontmatter.update({"id": id_from_seed("scene", "importance-planned"), "status": "planned", "participants": [{"character": third.id, "from": {"timeline": "main", "tick": 1, "order": 0}, "point_of_view": True}]})
    draft_scene = deepcopy(scene); draft_scene.frontmatter.update({"id": id_from_seed("scene", "importance-draft-one"), "participants": [{"character": draft.id, "from": {"timeline": "main", "tick": 5, "order": 0}}]})
    draft_scene_two = deepcopy(draft_scene); draft_scene_two.frontmatter["id"] = id_from_seed("scene", "importance-draft-two")
    event = _event(original, seed="importance-event-one", title="One involvement", status="canonical", time={"timeline": "main", "tick": 6, "order": 0}, effects=[{"target": first.id, "key": "flag", "operation": "set", "value": True}, {"target": first.id, "key": "rank", "operation": "set", "value": 1}, {"target": second.id, "key": "flag", "operation": "set", "value": True}]); event.frontmatter["participants"] = [{"character": first.id}]
    second_event = _event(original, seed="importance-event-two", title="A second involvement", status="canonical", time={"timeline": "main", "tick": 8, "order": 0}, effects=[]); second_event.frontmatter["participants"] = [{"character": first.id}]
    relationship = deepcopy(next(item for item in original.by_kind("relationship") if item.status == "canonical")); relationship.frontmatter.update({"id": id_from_seed("relationship", "importance-neighbor"), "status": "canonical", "from": first.id, "to": second.id, "transitions": [{"id": "rt_00000000000000000000000000", "time": {"timeline": "main", "tick": 8, "order": 0}, "relationship_status": "active"}]})
    world_record = deepcopy(original.world_record); world = World(original.revision, original.tree_oid, {world_record.id: world_record, **{person.id: person for person in people}, draft.id: draft, scene.id: scene, closed.id: closed, planned.id: planned, draft_scene.id: draft_scene, draft_scene_two.id: draft_scene_two, event.id: event, second_event.id: second_event, relationship.id: relationship}, original.root, original.source_root, original.is_worktree)
    early = character_importance(world, StoryTime("main", 6, 0)); late = character_importance(world, StoryTime("main", 8, 0))
    assert early[first.id]["raw"] == {"scenes": 1, "pointOfViewScenes": 1, "events": 1, "relationshipNeighbors": 0}
    assert early[second.id]["raw"] == {"scenes": 0, "pointOfViewScenes": 0, "events": 1, "relationshipNeighbors": 0}
    assert late[first.id]["raw"] == {"scenes": 1, "pointOfViewScenes": 1, "events": 2, "relationshipNeighbors": 1}
    assert late[second.id]["raw"] == {"scenes": 1, "pointOfViewScenes": 0, "events": 1, "relationshipNeighbors": 1}
    assert late[third.id]["raw"] == {"scenes": 0, "pointOfViewScenes": 0, "events": 0, "relationshipNeighbors": 0}
    assert draft.id not in late and late[first.id]["normalized"]["scenes"] == 1.0, "draft evidence cannot alter canonical/retired cohort maxima"
    assert all(value["score"] == 0.0 for value in character_importance(world, StoryTime("main", 4, 0)).values())
    calls = 0; canonical = query_module.canonical_events
    def counted_events(indexed_world, at=None):
        nonlocal calls; calls += 1; return canonical(indexed_world, at)
    monkeypatch.setattr(query_module, "canonical_events", counted_events)
    fresh = World("replacement-revision", world.tree_oid, world.records, world.root, world.source_root, world.is_worktree)
    character_importance(fresh, StoryTime("main", 8, 0)); character_importance(fresh, StoryTime("main", 8, 0))
    assert calls == 1, "the shared evidence index prevents repeated event scans per character/read"


def test_importance_uses_exact_signed_tick_and_order_boundaries(frontiersmen_repo) -> None:
    original = frontiersmen_repo.load_world(); first, second = [deepcopy(original.find(name, "character")) for name in ("Rhea", "Corporal Asha Pell")]
    first.frontmatter["status"] = second.frontmatter["status"] = "canonical"
    event = _event(original, seed="importance-signed", title="Signed boundary", status="canonical", time={"timeline": "main", "tick": TICK_MIN, "order": 0}, effects=[]); event.frontmatter["participants"] = [{"character": first.id}]
    world_record = deepcopy(original.world_record); world = World(original.revision, original.tree_oid, {world_record.id: world_record, first.id: first, second.id: second, event.id: event}, original.root, original.source_root, original.is_worktree)
    assert character_importance(world, StoryTime("main", TICK_MIN, -1))[first.id]["raw"]["events"] == 0
    assert character_importance(world, StoryTime("main", TICK_MIN, 0))[first.id]["raw"]["events"] == 1


@pytest.mark.parametrize("operation", [
    {"type": "entity.create", "value": {"importance": 1}},
    {"type": "entity.upsert", "value": {"frontmatter": {"importance": 1}}},
    {"type": "entity.update", "frontmatterPatch": {"importance": 1}},
])
def test_changesets_reject_calculated_importance(frontiersmen_repo, operation) -> None:
    payload = {"protocol": "wedl-changeset/v1", "expectedHead": frontiersmen_repo.head(), "idempotencyKey": f"importance-rejection-{operation['type']}", "operations": [operation]}
    with pytest.raises(ProtocolError, match="importance is calculated output"):
        preview(frontiersmen_repo, payload)
