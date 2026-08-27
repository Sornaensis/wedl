from __future__ import annotations

import pytest

from wedl.changeset import apply, preview
from wedl.compiler import cache_paths, compile_world, connect
from wedl.context import build_context
from wedl.errors import ValidationFailed
from wedl.model import ORDER_MAX, ORDER_MIN, TICK_MAX, TICK_MIN, StoryTime
from wedl import query as query_module
from wedl.query import entity_state, status, timeline
from wedl.source import serialize_record
from wedl.validation import validate_world


def _errors(world) -> list[dict[str, object]]:
    return [item for item in validate_world(world) if item["severity"] == "error"]


def test_story_time_accepts_signed_boundaries_and_rejects_coercion() -> None:
    assert StoryTime("main", TICK_MIN, ORDER_MIN).to_dict() == {
        "timeline": "main",
        "tick": TICK_MIN,
        "order": ORDER_MIN,
    }
    assert StoryTime("main", TICK_MAX, ORDER_MAX).tick == TICK_MAX
    for value in (True, 1.5, "1", TICK_MAX + 1):
        with pytest.raises(ValueError, match="tick"):
            StoryTime("main", value)
    for value in (False, 0.5, "0", ORDER_MAX + 1):
        with pytest.raises(ValueError, match="order"):
            StoryTime("main", 0, value)
    with pytest.raises(ValueError, match="timeline"):
        StoryTime("", 0)


def test_negative_source_time_and_descriptive_origin_are_valid(ash_repo) -> None:
    world = ash_repo.load_world()
    world.world_record.frontmatter["timelines"] = [
        {"id": "main", "label": "Main chronology", "origin": {"tick": -100, "label": "Before the archive"}}
    ]
    # Keep the draft event unreferenced so this test isolates source-time
    # validity rather than intentionally breaking causal references.
    event = world.find("Ysabet Serves the Inventory Writ", "event")
    event.frontmatter["time"] = {"timeline": "main", "tick": -1, "order": ORDER_MIN}
    event.frontmatter["status"] = "draft"
    assert _errors(world) == []
    assert world.world_record.frontmatter["timelines"][0]["origin"] == {
        "tick": -100,
        "label": "Before the archive",
    }


def test_status_time_model_preserves_optional_descriptive_origins(ash_repo, monkeypatch) -> None:
    world = ash_repo.load_world()
    world.world_record.frontmatter["timelines"] = [
        {"id": "main", "label": "Main chronology", "origin": {"tick": -100, "label": "Before the archive"}}
    ]
    monkeypatch.setattr(ash_repo, "load_world", lambda _revision=None: world)

    model = status(ash_repo)["timeModel"]
    assert model["defaultTimeline"] == "main"
    assert model["timelineDeclarations"] == [
        {"id": "main", "label": "Main chronology", "origin": {"tick": -100, "label": "Before the archive"}}
    ]
    assert model["origin"] == {"semantics": "descriptive", "setsLowerBound": False}


def test_source_timeline_and_time_errors_are_field_aware(ash_repo) -> None:
    world = ash_repo.load_world()
    world.world_record.frontmatter["default_timeline"] = "missing"
    world.world_record.frontmatter["timelines"] = [
        {"id": "main", "label": "Main chronology", "origin": {"tick": 0, "label": ""}}
    ]
    event = world.by_kind("event")[0]
    event.frontmatter["time"] = {"timeline": "unwritten", "tick": 0, "order": True}
    errors = _errors(world)
    assert {(item["code"], item["field"]) for item in errors} >= {
        ("WDL-TIMELINE-011", "default_timeline"),
        ("WDL-TIMELINE-009", "timelines[0].origin.label"),
        ("WDL-TIME-002", "time.order"),
    }
    event.frontmatter["time"] = "not a story time"
    assert ("WDL-TIME-002", "time") in {
        (item["code"], item["field"])
        for item in _errors(world)
    }


def test_temporal_validation_ignores_non_temporal_v03_metadata(ash_repo) -> None:
    world = ash_repo.load_world()
    character = world.find("Mara Vale", "character")
    character.frontmatter["initial_state"] = {"order": "sworn"}
    diagnostics = validate_world(world)
    assert not [item for item in diagnostics if item["code"].startswith("WDL-TIME-")]
    assert any(item["code"] == "WDL-STATE-010" for item in diagnostics)


def test_invalid_timeline_declarations_and_mixed_origin_keys_are_field_aware(ash_repo) -> None:
    world = ash_repo.load_world()
    world.world_record.frontmatter["timelines"] = [
        {"id": "main", "label": "Main chronology", "origin": {"tick": 0, "label": "Origin", 7: "unexpected"}},
        {"id": "main", "label": ""},
        {"id": "", "label": "Empty timeline"},
    ]
    errors = {(item["code"], item["field"]) for item in _errors(world)}
    assert {
        ("WDL-TIMELINE-007", "timelines[0].origin.7"),
        ("WDL-TIMELINE-004", "timelines[1].id"),
        ("WDL-TIMELINE-005", "timelines[1].label"),
        ("WDL-TIMELINE-003", "timelines[2].id"),
    } <= errors


def test_origin_diagnostics_and_serialization_have_total_stable_member_order(ash_repo) -> None:
    world = ash_repo.load_world()
    origin = {"label": "Origin", "tick": 0, "z": "last", 7: "number"}
    world.world_record.frontmatter["timelines"] = [{"id": "main", "label": "Main", "origin": origin}]
    first = _errors(world)
    world.world_record.frontmatter["timelines"][0]["origin"] = {7: "number", "z": "last", "tick": 0, "label": "Origin"}
    assert _errors(world) == first
    frontmatter = world.world_record.frontmatter
    encoded = serialize_record(frontmatter, "")
    assert encoded.index(b"tick: 0") < encoded.index(b"label: Origin") < encoded.index(b"7: number") < encoded.index(b"z: last")


def test_query_uses_the_same_bounds_and_declared_timeline_contract(ash_repo) -> None:
    compile_world(ash_repo, force=True, profile_name="fts")
    character = ash_repo.load_world().find("Mara Vale", "character")
    assert entity_state(ash_repo, character.id, -1)["at"]["tick"] == -1
    with pytest.raises(ValueError, match="order"):
        entity_state(ash_repo, character.id, 0, order=ORDER_MAX + 1)
    with pytest.raises(ValueError, match="unknown timeline"):
        entity_state(ash_repo, character.id, 0, timeline="unwritten")
    scene = ash_repo.load_world().by_kind("scene")[0]
    with pytest.raises(ValueError, match="tick"):
        build_context(ash_repo, character_id=character.id, scene_id=scene.id, tick=TICK_MAX + 1)


def test_timeline_contract_preserves_ordinal_boundaries_named_references_and_statuses(ash_repo, monkeypatch) -> None:
    world = ash_repo.load_world()
    world.world_record.frontmatter["timelines"] = [
        {"id": "main", "label": "Main chronology", "origin": {"tick": -10, "label": "Archive opening"}},
        {"id": "aftermath", "label": "Aftermath"},
    ]
    events = world.by_kind("event")
    events[0].frontmatter.update({"time": {"timeline": "main", "tick": -3, "order": 2}, "status": "draft"})
    events[1].frontmatter.update({"time": {"timeline": "main", "tick": -3, "order": 4}, "status": "cancelled"})
    scene = world.by_kind("scene")[0]
    scene.frontmatter["time"] = {
        "start": {"timeline": "main", "tick": -3, "order": 0},
        "current": {"timeline": "main", "tick": -2, "order": 0},
        "end": {"timeline": "main", "tick": -1, "order": 0},
    }
    conversation = world.by_kind("conversation")[0]
    conversation.frontmatter["time"] = {
        "start": {"timeline": "main", "tick": -3, "order": 1},
        "end": {"timeline": "main", "tick": -2, "order": 1},
    }
    monkeypatch.setattr(query_module, "require_database", lambda _repository, require_compiled=False: (world, ash_repo.root / "timeline.sqlite"))

    result = timeline(ash_repo)

    assert result["protocol"] == "wedl-timeline/v1"
    assert result["timeline"] == {"id": "main", "label": "Main chronology", "origin": {"tick": "-10", "label": "Archive opening"}}
    assert result["temporalSemantics"] == {"spacing": "ordinal", "durationSemantics": "none", "intervalEndpoints": "inclusive"}
    assert [(item["at"]["tick"], item["at"]["order"]) for item in result["points"][:2]] == [("-3", "2"), ("-3", "4")]
    assert {item["status"] for item in result["points"]} >= {"draft", "cancelled"}
    assert all({"id", "kind", "title"} <= set(item["entity"]) for item in result["points"] + result["spans"])
    scene_span = next(item for item in result["spans"] if item["kind"] == "scene" and item["entity"]["id"] == scene.id)
    assert scene_span["start"]["tick"] == "-3"
    assert scene_span["current"]["tick"] == "-2"
    assert scene_span["end"]["tick"] == "-1"
    assert any(item["kind"] == "conversation" and item["end"] is not None for item in result["spans"])
    assert timeline(ash_repo, "aftermath")["timeline"]["label"] == "Aftermath"


def test_cross_timeline_scene_and_conversation_intervals_are_rejected(ash_repo) -> None:
    world = ash_repo.load_world()
    world.world_record.frontmatter["timelines"] = [
        {"id": "main", "label": "Main chronology"},
        {"id": "aftermath", "label": "Aftermath"},
    ]
    scene = world.by_kind("scene")[0]
    scene.frontmatter["time"]["end"] = {"timeline": "aftermath", "tick": 1, "order": 0}
    conversation = world.by_kind("conversation")[0]
    conversation.frontmatter["time"]["end"] = {"timeline": "aftermath", "tick": 1, "order": 0}

    errors = {(item["code"], item["field"]) for item in _errors(world)}

    assert ("WDL-SCENE-023", "time.end") in errors
    assert ("WDL-CONV-014", "time.end") in errors


def test_invalid_changeset_time_is_atomic_and_never_commits(ash_repo) -> None:
    world = ash_repo.load_world()
    item = world.find("Black Salt Vial", "object")
    payload = {
        "expectedHead": ash_repo.head(),
        "idempotencyKey": "invalid-time-is-atomic-v1",
        "summary": "must not commit a time beyond SQLite range",
        "operations": [
            {"type": "entity.update", "entity": item.id, "frontmatterPatch": {"tags": [*item.tags, "should-not-persist"]}},
            {
                "type": "event.create",
                "title": "Invalid Time",
                "time": {"timeline": "main", "tick": TICK_MAX + 1, "order": 0},
            },
            {
                "type": "event.create",
                "title": "Malformed Time",
                "time": "not a story time",
            },
        ],
    }
    plan = preview(ash_repo, payload)
    assert plan["valid"] is False
    assert any(item["field"] == "time.tick" for item in plan["diagnostics"])
    assert any(item["field"] == "time" for item in plan["diagnostics"])
    with pytest.raises(ValidationFailed):
        apply(ash_repo, payload)
    assert ash_repo.head() == payload["expectedHead"]
    assert "should-not-persist" not in ash_repo.load_world().get(item.id).tags


def test_signed_boundaries_and_same_tick_ordering_round_trip_through_sqlite(ash_repo) -> None:
    payload = {
        "expectedHead": ash_repo.head(),
        "idempotencyKey": "signed-boundary-time-v1",
        "summary": "store temporal storage boundaries and same-tick ordering",
        "operations": [
            {
                "type": "event.create",
                "title": "At the lower boundary",
                "time": {"timeline": "main", "tick": TICK_MIN, "order": ORDER_MIN},
            },
            {
                "type": "event.create",
                "title": "At the upper boundary",
                "time": {"timeline": "main", "tick": TICK_MAX, "order": ORDER_MAX},
            },
            {
                "type": "event.create",
                "title": "Same tick, first",
                "time": {"timeline": "main", "tick": 0, "order": ORDER_MIN},
            },
            {
                "type": "event.create",
                "title": "Same tick, last",
                "time": {"timeline": "main", "tick": 0, "order": ORDER_MAX},
            },
        ],
    }
    receipt = apply(ash_repo, payload, allow_unconfirmed=True)
    assert receipt["compile"]["recordCount"] == 266
    database = cache_paths(ash_repo)[1]
    with connect(database, True) as connection:
        rows = connection.execute(
            "SELECT entity.title, event.tick, event.ordering FROM event JOIN entity ON entity.id=event.entity_id "
            "WHERE entity.title IN (?,?,?,?) ORDER BY event.tick, event.ordering",
            ("At the lower boundary", "At the upper boundary", "Same tick, first", "Same tick, last"),
        ).fetchall()
    assert [(row["title"], row["tick"], row["ordering"]) for row in rows] == [
        ("At the lower boundary", TICK_MIN, ORDER_MIN),
        ("Same tick, first", 0, ORDER_MIN),
        ("Same tick, last", 0, ORDER_MAX),
        ("At the upper boundary", TICK_MAX, ORDER_MAX),
    ]


def test_frontiersmen_compiles_without_temporal_migration(frontiersmen_repo) -> None:
    report = compile_world(frontiersmen_repo, force=True, profile_name="fts")
    assert report["recordCount"] == 309
