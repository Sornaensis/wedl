from __future__ import annotations

from copy import deepcopy

import pytest

from wedl.ids import id_from_seed
from wedl.model import World
from wedl.context import _implicit_scene as context_implicit_scene
from wedl.errors import UsageError
from wedl.query import _implicit_active_scene
from wedl.repository import Repository
from wedl.validation import validate_world


def _split_frontiersmen_world(source: Repository) -> World:
    """Build a valid two-front variant from the isolated fixture."""
    original = source.load_world("HEAD")
    records = dict(original.records)
    world_record = deepcopy(original.world_record)
    world_record.frontmatter["current_time"] = {"timeline": "main", "tick": 210, "order": 0}
    records[world_record.id] = world_record

    second = deepcopy(original.find("Drowned Waymark", "scene"))
    second_id = id_from_seed("scene", "concurrent-second-front")
    second.frontmatter["id"] = second_id
    second.frontmatter["title"] = "Concurrent second front"
    second.frontmatter["status"] = "active"
    second.frontmatter["time"]["end"] = None
    second.frontmatter["time"]["current"] = {"timeline": "main", "tick": 210, "order": 0}
    second.frontmatter["participants"] = []
    second.frontmatter["objects"] = []
    second.source_path = "story/scenes/concurrent-second-front.md"
    records[second_id] = second
    return World(original.revision, original.tree_oid, records, original.root, original.source_root, original.is_worktree)


def _historical_scene(
    original: World,
    *,
    seed: str,
    title: str,
    location_id: str,
    characters: list[str],
    start: tuple[int, int],
    end: tuple[int, int],
):
    """Build one isolated closed scene without altering the fixture story."""
    scene = deepcopy(original.find("Drowned Waymark", "scene"))
    scene_id = id_from_seed("scene", seed)
    scene.frontmatter["id"] = scene_id
    scene.frontmatter["title"] = title
    scene.frontmatter["status"] = "closed"
    scene.frontmatter["time"] = {
        "start": {"timeline": "main", "tick": start[0], "order": start[1]},
        "current": {"timeline": "main", "tick": end[0], "order": end[1]},
        "end": {"timeline": "main", "tick": end[0], "order": end[1]},
    }
    scene.frontmatter["location"] = location_id
    scene.frontmatter["participants"] = [
        {
            "character": character_id,
            "role": "traveler",
            "point_of_view": False,
            "from": {"timeline": "main", "tick": start[0], "order": start[1]},
            "to": {"timeline": "main", "tick": end[0], "order": end[1]},
        }
        for character_id in characters
    ]
    scene.frontmatter["objects"] = []
    scene.frontmatter["observations"] = []
    scene.frontmatter["conversations"] = []
    scene.source_path = f"story/scenes/{seed}.md"
    return scene


def _world_with_records(original: World, *extra) -> World:
    records = dict(original.records)
    records.update({record.id: record for record in extra})
    return World(original.revision, original.tree_oid, records, original.root, original.source_root, original.is_worktree)


def _event_at(
    original: World,
    *,
    seed: str,
    title: str,
    location_id: str,
    character_id: str,
):
    event = deepcopy(next(record for record in original.by_kind("event") if record.status == "canonical"))
    event_id = id_from_seed("event", seed)
    event.frontmatter["id"] = event_id
    event.frontmatter["title"] = title
    event.frontmatter["time"] = {"timeline": "main", "tick": 340, "order": 0}
    event.frontmatter["location"] = location_id
    event.frontmatter["participants"] = [{"character": character_id, "role": "traveler"}]
    event.frontmatter["causes"] = []
    event.frontmatter["related_story_points"] = []
    event.frontmatter["effects"] = []
    event.source_path = f"story/events/main/{seed}.md"
    return event


def test_multiple_active_scenes_require_and_accept_a_shared_cursor(frontiersmen_repo: Repository) -> None:
    world = _split_frontiersmen_world(frontiersmen_repo)
    assert world.active_scene() is None
    assert [scene.title for scene in world.active_scenes()] == ["Concurrent second front", "Southward Cut"]
    errors = [item for item in validate_world(world) if item["severity"] == "error"]
    assert not [item for item in errors if item["code"].startswith("WDL-CURSOR-")]
    assert not [item for item in errors if item["code"] == "WDL-SCENE-001"]


def test_every_active_scene_cursor_must_exactly_match_the_world_cursor(frontiersmen_repo: Repository) -> None:
    world = _split_frontiersmen_world(frontiersmen_repo)
    second = world.find("Concurrent second front", "scene")
    second.frontmatter["time"]["current"] = {"timeline": "main", "tick": 210, "order": 1}

    diagnostics = validate_world(world)
    assert any(item["code"] == "WDL-CURSOR-005" and item["entityId"] == second.id for item in diagnostics)


def test_explicit_singleton_world_cursor_must_match_its_active_scene(frontiersmen_repo: Repository) -> None:
    source = frontiersmen_repo
    original = source.load_world("HEAD")
    records = dict(original.records)
    world_record = deepcopy(original.world_record)
    world_record.frontmatter["current_time"] = {"timeline": "main", "tick": 209, "order": 0}
    records[world_record.id] = world_record
    world = World(original.revision, original.tree_oid, records, original.root, original.source_root, original.is_worktree)

    diagnostics = validate_world(world)
    active = world.active_scenes()[0]
    assert any(item["code"] == "WDL-CURSOR-005" and item["entityId"] == active.id for item in diagnostics)


def test_implicit_scene_is_ambiguous_generically_but_resolves_for_a_character(frontiersmen_repo: Repository) -> None:
    world = _split_frontiersmen_world(frontiersmen_repo)
    character = world.find("Rhea", "character")
    with pytest.raises(UsageError, match="multiple active scenes"):
        _implicit_active_scene(world)
    assert _implicit_active_scene(world, character.id).title == "Southward Cut"
    assert context_implicit_scene(world, character.id).title == "Southward Cut"


def test_concurrent_scenes_reject_overlapping_present_casts(frontiersmen_repo: Repository) -> None:
    world = _split_frontiersmen_world(frontiersmen_repo)
    second = world.find("Concurrent second front", "scene")
    first = world.find("Southward Cut", "scene")
    second.frontmatter["participants"] = [deepcopy(first.frontmatter["participants"][0])]
    diagnostics = validate_world(world)
    assert any(item["code"] == "WDL-SCENE-001" for item in diagnostics)


def test_same_story_time_state_writes_are_a_validation_conflict(frontiersmen_repo: Repository) -> None:
    source = frontiersmen_repo
    original = source.load_world("HEAD")
    records = dict(original.records)
    event = next(record for record in original.by_kind("event") if record.status == "canonical" and any(isinstance(effect, dict) and effect.get("target") for effect in record.frontmatter.get("effects") or []))
    duplicate = deepcopy(event)
    duplicate_id = id_from_seed("event", "same-time-state-conflict")
    duplicate.frontmatter["id"] = duplicate_id
    duplicate.frontmatter["title"] = "Conflicting concurrent write"
    duplicate.frontmatter["effects"] = [deepcopy(next(effect for effect in event.frontmatter["effects"] if isinstance(effect, dict) and effect.get("target")))]
    duplicate.frontmatter["effects"][0]["id"] = id_from_seed("effect", "same-time-state-conflict")
    duplicate.source_path = "story/events/main/conflicting-concurrent-write.md"
    records[duplicate_id] = duplicate
    world = World(original.revision, original.tree_oid, records, original.root, original.source_root, original.is_worktree)
    diagnostics = validate_world(world)
    assert any(item["code"] == "WDL-STATE-020" for item in diagnostics)


def test_historical_split_and_reunion_with_independent_characters_is_valid(frontiersmen_repo: Repository) -> None:
    source = frontiersmen_repo
    original = source.load_world("HEAD")
    rhea = original.find("Rhea", "character")
    pip = original.find("Pip", "character")
    first_location, second_location = original.by_kind("location")[:2]
    first = _historical_scene(
        original, seed="historical-north-front", title="North front", location_id=first_location.id,
        characters=[rhea.id], start=(300, 0), end=(301, 0),
    )
    second = _historical_scene(
        original, seed="historical-south-front", title="South front", location_id=second_location.id,
        characters=[pip.id], start=(300, 0), end=(301, 0),
    )
    reunion = _historical_scene(
        original, seed="historical-reunion", title="Reunion", location_id=first_location.id,
        characters=[rhea.id, pip.id], start=(302, 0), end=(303, 0),
    )
    diagnostics = validate_world(_world_with_records(original, first, second, reunion))
    owned = [item for item in diagnostics if item["entityId"] in {first.id, second.id, reunion.id}]
    assert not [item for item in owned if item["code"] in {"WDL-SCENE-025", "WDL-SCENE-026"}]


def test_historical_double_booking_names_the_character_and_scenes(frontiersmen_repo: Repository) -> None:
    source = frontiersmen_repo
    original = source.load_world("HEAD")
    rhea = original.find("Rhea", "character")
    first_location, second_location = original.by_kind("location")[:2]
    first = _historical_scene(
        original, seed="historical-first-booking", title="First booking", location_id=first_location.id,
        characters=[rhea.id], start=(310, 0), end=(311, 0),
    )
    second = _historical_scene(
        original, seed="historical-second-booking", title="Second booking", location_id=second_location.id,
        characters=[rhea.id], start=(310, 0), end=(311, 0),
    )
    diagnostics = validate_world(_world_with_records(original, first, second))
    double_booking = next(item for item in diagnostics if item["code"] == "WDL-SCENE-025" and item["entityId"] == second.id)
    assert "Rhea" in double_booking["message"]
    assert "First booking" in double_booking["message"]
    assert "Second booking" in double_booking["message"]
    assert any(item["code"] == "WDL-SCENE-026" and item["entityId"] == second.id for item in diagnostics)


def test_same_coordinate_two_place_presence_is_invalid_but_next_order_handoff_is_valid(frontiersmen_repo: Repository) -> None:
    source = frontiersmen_repo
    original = source.load_world("HEAD")
    rhea = original.find("Rhea", "character")
    first_location, second_location = original.by_kind("location")[:2]
    first = _historical_scene(
        original, seed="same-coordinate-origin", title="Same coordinate origin", location_id=first_location.id,
        characters=[rhea.id], start=(320, 0), end=(320, 0),
    )
    conflicting = _historical_scene(
        original, seed="same-coordinate-destination", title="Same coordinate destination", location_id=second_location.id,
        characters=[rhea.id], start=(320, 0), end=(320, 1),
    )
    handoff = _historical_scene(
        original, seed="adjacent-order-destination", title="Adjacent order destination", location_id=second_location.id,
        characters=[rhea.id], start=(330, 1), end=(330, 2),
    )
    prior = _historical_scene(
        original, seed="adjacent-order-origin", title="Adjacent order origin", location_id=first_location.id,
        characters=[rhea.id], start=(330, 0), end=(330, 0),
    )
    diagnostics = validate_world(_world_with_records(original, first, conflicting, prior, handoff))
    assert any(item["code"] == "WDL-SCENE-026" and "Same coordinate origin" in item["message"] for item in diagnostics)
    assert not [
        item for item in diagnostics
        if item["code"] in {"WDL-SCENE-025", "WDL-SCENE-026"}
        and "Adjacent order origin" in item["message"]
    ]


def test_location_parents_and_routes_are_kind_safe_and_directional(frontiersmen_repo: Repository) -> None:
    source = frontiersmen_repo
    world = source.load_world("HEAD")
    location, destination = world.by_kind("location")[:2]
    character = world.find("Rhea", "character")
    location.frontmatter["parent"] = character.id
    location.frontmatter["links"] = [
        {"description": "Missing destination."},
        location.id,
        destination.id,
        {"location": destination.id, "description": "A steep, one-way deer track."},
        {"location": character.id, "summary": "Not a place."},
    ]
    diagnostics = validate_world(world)
    codes = {item["code"] for item in diagnostics if item["entityId"] == location.id}
    assert {"WDL-LOC-002", "WDL-LOC-005", "WDL-LOC-006", "WDL-LOC-007", "WDL-LOC-008"} <= codes


def test_malformed_location_parent_is_diagnostic_not_a_cycle_checker_crash(frontiersmen_repo: Repository) -> None:
    source = frontiersmen_repo
    world = source.load_world("HEAD")
    location = world.by_kind("location")[0]
    location.frontmatter["parent"] = {"location": location.id}
    diagnostics = validate_world(world)
    assert any(item["code"] == "WDL-LOC-001" and item["entityId"] == location.id for item in diagnostics)


def test_location_link_mapping_requires_exactly_one_valid_target_alias(frontiersmen_repo: Repository) -> None:
    source = frontiersmen_repo
    world = source.load_world("HEAD")
    location, destination = world.by_kind("location")[:2]
    location.frontmatter["links"] = [
        {"target": destination.id, "description": "A valid directional route."},
        {"location": destination.id, "target": destination.id},
        {"location": destination.id, "entity": ""},
        {"entity": 42},
    ]
    diagnostics = validate_world(world)
    malformed = {
        item["field"]
        for item in diagnostics
        if item["entityId"] == location.id and item["code"] == "WDL-LOC-005"
    }
    assert malformed == {"links[1]", "links[2]", "links[3]"}
    assert not [
        item for item in diagnostics
        if item["entityId"] == location.id
        and item["code"] == "WDL-LOC-005"
        and item["field"] == "links[0]"
    ]


def test_same_coordinate_event_participation_requires_one_place(frontiersmen_repo: Repository) -> None:
    source = frontiersmen_repo
    original = source.load_world("HEAD")
    rhea = original.find("Rhea", "character")
    first_location, second_location = original.by_kind("location")[:2]
    first = _event_at(
        original, seed="same-place-event-one", title="Same place event one",
        location_id=first_location.id, character_id=rhea.id,
    )
    same_place = _event_at(
        original, seed="same-place-event-two", title="Same place event two",
        location_id=first_location.id, character_id=rhea.id,
    )
    other_place = _event_at(
        original, seed="other-place-event", title="Other place event",
        location_id=second_location.id, character_id=rhea.id,
    )
    same_place_diagnostics = validate_world(_world_with_records(original, first, same_place))
    assert not [
        item for item in same_place_diagnostics
        if item["code"] == "WDL-EVENT-006" and item["entityId"] in {first.id, same_place.id}
    ]
    conflicting_diagnostics = validate_world(_world_with_records(original, first, other_place))
    assert any(
        item["code"] == "WDL-EVENT-006"
        and "Rhea" in item["message"]
        and "Same place event one" in item["message"]
        and "Other place event" in item["message"]
        for item in conflicting_diagnostics
    )
