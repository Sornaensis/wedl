from __future__ import annotations

from copy import deepcopy
import json
from pathlib import Path

import pytest

from wedl.errors import NotFound, UsageError
from wedl.model import World
from wedl import query as query_module
from wedl.api_contract import route_contracts
from wedl.api_schemas import SCHEMAS, operation_schema
from wedl.cli import main
from wedl.ids import id_from_seed
from wedl.query import causality, story_points
from wedl.repository import Repository
from wedl.validation import validate_world


def _with_records(world, *records):
    values = dict(world.records)
    values.update({record.id: record for record in records})
    return World(world.revision, world.tree_oid, values, world.root, world.source_root, world.is_worktree)


def _ash_repository() -> Repository:
    return Repository(Path("tests/fixtures/legacy_worlds/ash_archive"))


def test_event_cause_rules_retain_direct_edges_and_reject_invalid_graphs() -> None:
    original = _ash_repository().load_world("WORKTREE")
    first, second = [deepcopy(item) for item in original.by_kind("event")[:2]]
    first.frontmatter["causes"] = [second.id, second.id]
    second.frontmatter["causes"] = [first.id]
    diagnostics = validate_world(_with_records(original, first, second))
    codes = {item["code"] for item in diagnostics if item["entityId"] in {first.id, second.id}}
    assert {"WDL-EVENT-009", "WDL-EVENT-012"} <= codes
    assert any(item["code"] == "WDL-EVENT-013" for item in diagnostics)

    first.frontmatter["causes"] = "not an array"
    malformed = validate_world(_with_records(original, first))
    assert any(item["code"] == "WDL-EVENT-007" and item["entityId"] == first.id for item in malformed)


def test_causality_is_named_deterministic_and_horizon_clipped(monkeypatch) -> None:
    ash_repo = _ash_repository()
    world = ash_repo.load_world("WORKTREE")
    monkeypatch.setattr(query_module, "require_database", lambda _repository, require_compiled=False: (world, ash_repo.root / "unused.sqlite"))
    event = world.find("Ilyra Explains the Descendant Ledger", "event")
    result = causality(ash_repo, event.title, direction="upstream", tick=171, order=20)
    assert result["protocol"] == "wedl-causality/v1"
    assert result["focusEvent"]["title"] == event.title
    assert result["nodes"] == sorted(result["nodes"], key=lambda item: (item["at"]["timeline"], item["at"]["tick"], item["at"]["order"], item["event"]["title"].casefold(), item["event"]["id"]))
    assert all((item["at"]["tick"], item["at"]["order"]) <= (171, 20) for item in result["nodes"])
    with pytest.raises(NotFound):
        causality(ash_repo, event.title, tick=170)


def test_plot_trails_and_advisories_are_horizon_sliced(monkeypatch) -> None:
    ash_repo = _ash_repository()
    world = ash_repo.load_world("WORKTREE")
    monkeypatch.setattr(query_module, "require_database", lambda _repository, require_compiled=False: (world, ash_repo.root / "unused.sqlite"))
    result = story_points(ash_repo, tick=154, order=10)
    point = next(item for item in result["storyPoints"] if item["title"] == "Test Halver's Testimony")
    assert point["plotTrail"]["transitions"]
    assert all(item["at"]["tick"] <= 154 for item in point["plotTrail"]["transitions"])
    assert all(item["at"]["tick"] <= 154 for item in point["plotTrail"]["outcomeEvents"])
    assert all(item["code"].startswith("WDL-ADVISORY-") for item in result["continuityAdvisories"])


def test_typed_cause_plot_and_scene_outcome_relations_are_checked() -> None:
    original = _ash_repository().load_world("WORKTREE")
    story_point = deepcopy(original.find("Test Halver's Testimony", "story-point"))
    story_point.frontmatter["outcome_events"] = [story_point.id, story_point.id]
    knowledge = deepcopy(original.by_kind("knowledge")[0])
    knowledge.frontmatter["transitions"][0]["causing_event"] = knowledge.id
    future_cause = deepcopy(original.by_kind("knowledge")[1])
    future_event = max(original.by_kind("event"), key=lambda item: (item.frontmatter["time"]["tick"], item.frontmatter["time"].get("order", 0)))
    future_cause.frontmatter["transitions"][0]["causing_event"] = future_event.id
    scene = deepcopy(original.find("An Honest Absence", "scene"))
    scene.frontmatter["outcome_events"] = [next(item.id for item in original.by_kind("event") if item.frontmatter["time"]["tick"] < scene.frontmatter["time"]["start"]["tick"])]
    diagnostics = validate_world(_with_records(original, story_point, knowledge, future_cause, scene))
    assert any(item["code"] == "WDL-SP-006" and item["entityId"] == story_point.id for item in diagnostics)
    assert any(item["code"] == "WDL-CAUSE-001" and item["entityId"] == knowledge.id for item in diagnostics)
    assert any(item["code"] == "WDL-CAUSE-004" and item["entityId"] == future_cause.id for item in diagnostics)
    assert any(item["code"] == "WDL-SCENE-028" and item["entityId"] == scene.id for item in diagnostics)


def test_plot_trail_hides_a_future_typed_cause_even_for_invalid_source(monkeypatch) -> None:
    original = _ash_repository().load_world("WORKTREE")
    story_point = deepcopy(original.find("Test Halver's Testimony", "story-point"))
    future = original.find("Ilyra Explains the Descendant Ledger", "event")
    story_point.frontmatter["lifecycle"]["transitions"][0]["causing_event"] = future.id
    world = _with_records(original, story_point)
    ash_repo = _ash_repository()
    monkeypatch.setattr(query_module, "require_database", lambda _repository, require_compiled=False: (world, ash_repo.root / "unused.sqlite"))
    result = story_points(ash_repo, tick=152, order=20)
    point = next(item for item in result["storyPoints"] if item["storyPointId"] == story_point.id)
    assert point["plotTrail"]["transitions"][0]["causingEvent"] is None
    assert future.title not in str(point["plotTrail"])


def test_causality_rejects_noncanonical_focus_without_a_key_error(monkeypatch) -> None:
    original = _ash_repository().load_world("WORKTREE")
    target = deepcopy(original.by_kind("event")[0])
    target.frontmatter["status"] = "draft"
    world = _with_records(original, target)
    ash_repo = _ash_repository()
    monkeypatch.setattr(query_module, "require_database", lambda _repository, require_compiled=False: (world, ash_repo.root / "unused.sqlite"))
    with pytest.raises(UsageError, match="canonical focus event"):
        causality(ash_repo, target.title, tick=target.frontmatter["time"]["tick"])


def test_causal_cli_and_api_contract_expose_the_same_read_surface(capsys) -> None:
    contracts = {contract.command: contract for contract in route_contracts()}
    causal = contracts[("causal",)]
    assert causal.binding is not None and causal.binding.path == "/api/causal/{event_id}"
    assert {argument.dest for argument in causal.arguments} >= {"event", "direction", "tick", "timeline", "order"}
    assert operation_schema("story-points") == "StoryPointsResponse"
    response = SCHEMAS["StoryPointsResponse"]
    assert {"storyPoints", "continuityAdvisories"} <= set(response["required"])
    assert "plotTrail" in SCHEMAS["StoryPointView"]["properties"]
    causal_response = SCHEMAS["CausalityResponse"]
    assert causal_response["properties"]["nodes"]["items"] == SCHEMAS["CausalityNode"]
    assert causal_response["properties"]["edges"]["items"] == SCHEMAS["CausalityEdge"]
    assert causal_response["properties"]["continuityAdvisories"]["items"] == SCHEMAS["ContinuityAdvisory"]

    code = main(["--compact", "causal", "Ilyra Explains the Descendant Ledger", "--repo", "tests/fixtures/legacy_worlds/ash_archive", "--direction", "upstream", "--tick", "171"])
    payload = json.loads(capsys.readouterr().out)
    assert code == 0
    assert payload["protocol"] == "wedl-causality/v1"
    assert payload["direction"] == "upstream"


def test_causal_directions_and_same_tick_horizon_do_not_leak(monkeypatch) -> None:
    original = _ash_repository().load_world("WORKTREE")
    early, later = [deepcopy(item) for item in original.by_kind("event")[:2]]
    early.frontmatter.update({"id": id_from_seed("event", "causal-horizon-early"), "title": "Causal horizon early", "time": {"timeline": "main", "tick": 300, "order": 1}, "causes": [], "effects": []})
    later.frontmatter.update({"id": id_from_seed("event", "causal-horizon-later"), "title": "Causal horizon later", "time": {"timeline": "main", "tick": 300, "order": 2}, "causes": [early.id], "effects": []})
    world = _with_records(original, early, later)
    ash_repo = _ash_repository()
    monkeypatch.setattr(query_module, "require_database", lambda _repository, require_compiled=False: (world, ash_repo.root / "unused.sqlite"))
    clipped = causality(ash_repo, early.title, direction="downstream", tick=300, order=1)
    complete = causality(ash_repo, early.title, direction="both", tick=300, order=2)
    assert [node["event"]["title"] for node in clipped["nodes"]] == [early.title]
    assert {node["event"]["title"] for node in complete["nodes"]} >= {early.title, later.title}


def test_causal_coordinates_reject_cross_timeline_and_same_coordinate_edges() -> None:
    original = _ash_repository().load_world("WORKTREE")
    cause, effect = [deepcopy(item) for item in original.by_kind("event")[:2]]
    cause.frontmatter.update({"id": id_from_seed("event", "causal-coordinate-cause"), "title": "Coordinate cause", "time": {"timeline": "main", "tick": 301, "order": 0}, "causes": [], "effects": []})
    effect.frontmatter.update({"id": id_from_seed("event", "causal-coordinate-effect"), "title": "Coordinate effect", "time": {"timeline": "aftermath", "tick": 302, "order": 0}, "causes": [cause.id], "effects": []})
    world_record = deepcopy(original.world_record)
    world_record.frontmatter["timelines"] = [*world_record.frontmatter["timelines"], {"id": "aftermath", "label": "Aftermath"}]
    cross_timeline = _with_records(original, world_record, cause, effect)
    assert any(item["code"] == "WDL-EVENT-011" and item["entityId"] == effect.id for item in validate_world(cross_timeline))

    effect.frontmatter["time"] = {"timeline": "main", "tick": 301, "order": 0}
    same_coordinate = _with_records(original, cause, effect)
    assert any(item["code"] == "WDL-EVENT-012" and item["entityId"] == effect.id for item in validate_world(same_coordinate))


def test_disjoint_fronts_rejoin_only_through_explicit_causal_edges(monkeypatch) -> None:
    original = _ash_repository().load_world("WORKTREE")
    north, south = original.by_kind("location")[:2]
    mara = original.find("Mara Vale", "character")
    nessa = original.find("Nessa Quill", "character")
    template = original.by_kind("event")[0]

    def event(seed: str, title: str, tick: int, order: int, location: str, participants: list[str], causes: list[str]):
        value = deepcopy(template)
        value.frontmatter.update({
            "id": id_from_seed("event", seed), "title": title,
            "time": {"timeline": "main", "tick": tick, "order": order},
            "location": location, "participants": [{"character": item, "role": "present"} for item in participants],
            "causes": causes, "related_story_points": [], "effects": [],
        })
        value.source_path = f"story/events/main/{seed}.md"
        return value

    shared_parent = event("cross-front-parent", "Shared earlier signal", 309, 0, north.id, [], [])
    north_front = event("cross-front-north", "North front answers", 310, 1, north.id, [mara.id], [shared_parent.id])
    south_front = event("cross-front-south", "South front answers", 310, 1, south.id, [nessa.id], [shared_parent.id])
    reunion = event("cross-front-reunion", "The fronts reunite", 311, 0, north.id, [mara.id, nessa.id], [north_front.id, south_front.id])
    world = _with_records(original, shared_parent, north_front, south_front, reunion)
    ash_repo = _ash_repository()
    monkeypatch.setattr(query_module, "require_database", lambda _repository, require_compiled=False: (world, ash_repo.root / "unused.sqlite"))
    result = causality(ash_repo, reunion.title, direction="upstream", tick=311, order=0)
    titles = {node["event"]["title"] for node in result["nodes"]}
    assert titles == {shared_parent.title, north_front.title, south_front.title, reunion.title}
    assert len(result["edges"]) == 4
    assert all(node["status"] == "canonical" for node in result["nodes"])
