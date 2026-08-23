from __future__ import annotations

import json

import pytest

from wedl.cli import initialize
from wedl.compiler import compile_world
from wedl.context import build_context
from wedl.errors import UsageError
from wedl.model import StoryTime
from wedl.query import conversation_view, entity_state, search_world, story_points
from wedl.semantics import resolve_state


def test_frontiersmen_is_a_campaign_sized_valid_world(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    assert len(world.records) == 309
    assert {kind: len(world.by_kind(kind)) for kind in [
        "character", "conversation", "environment", "event", "knowledge",
        "location", "object", "relationship", "scene", "story-point", "world",
    ]} == {
        "character": 20,
        "conversation": 19,
        "environment": 16,
        "event": 49,
        "knowledge": 64,
        "location": 31,
        "object": 29,
        "relationship": 36,
        "scene": 22,
        "story-point": 22,
        "world": 1,
    }
    conversations = world.by_kind("conversation")
    assert sum(len(item.frontmatter.get("turns") or []) for item in conversations) == 248
    assert sum(len(item.frontmatter.get("recollections") or []) for item in conversations) == 76
    active = world.active_scene()
    assert active is not None
    assert active.title == "Southward Cut"
    assert active.frontmatter["time"]["current"]["tick"] == 210


def test_hilltop_approach_moves_each_frontiersman_before_capture(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    hilltop = world.find("Hilltop Sink", "location")
    root_camp = world.find("Root-Crowned Camp", "location")
    approach = world.find("The Frontiersmen Enter Root-Crowned Camp", "event")
    capture = world.find("The Root Host Captures the Frontiersmen", "event")
    names = ("Rhea", "Syl", "Garran", "Veyra", "Pip")
    characters = [world.find(name, "character") for name in names]

    assert approach.frontmatter["time"] == {"timeline": "main", "tick": 160, "order": 1}
    approach_location_effects = [
        effect for effect in approach.frontmatter["effects"] if effect["key"] == "location"
    ]
    assert {effect["target"] for effect in approach_location_effects} == {character.id for character in characters}
    assert all(effect["value"] == {"entity": root_camp.id} for effect in approach_location_effects)

    for character in characters:
        before_approach, _ = resolve_state(world, character.id, StoryTime("main", 160, 0))
        after_approach, _ = resolve_state(world, character.id, StoryTime("main", 160, 1))
        before_capture, _ = resolve_state(world, character.id, StoryTime("main", 163, 19))
        assert before_approach["location"] == {"entity": hilltop.id}
        assert after_approach["location"] == {"entity": root_camp.id}
        assert before_capture["location"] == {"entity": root_camp.id}

    assert not any(
        effect["key"] == "location" and effect["target"] in {character.id for character in characters}
        for effect in capture.frontmatter["effects"]
    )


def test_final_party_contexts_are_distinct_and_hard_budgeted(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    scene = world.find("Southward Cut", "scene")
    packets: dict[str, str] = {}
    for alias in ("Rhea", "Syl", "Garran", "Veyra", "Pip"):
        character = world.find(alias, "character")
        result = build_context(
            frontiersmen_repo,
            character_id=character.id,
            scene_id=scene.id,
            query="follow the abandoned warden road south with bounded Blackroot evidence",
            max_characters=5000,
            max_items=18,
        )
        serialized = json.dumps(result, ensure_ascii=False, separators=(",", ":"))
        assert len(serialized) <= 5000
        assert "Southward Cut" in result["promptText"]
        assert "Write only from this character" in result["promptText"]
        assert "Lio Vane" not in result["promptText"]
        packets[alias] = result["promptText"]
    assert "caravan shield" in packets["Rhea"]
    assert "animal absence" in packets["Syl"] or "forest" in packets["Syl"]
    assert "Hearth and Road" in packets["Garran"]
    assert "sympathetic materials" in packets["Veyra"]
    assert "locksmith" in packets["Pip"]
    assert len(set(packets.values())) == 5


def test_future_relationships_and_aliases_do_not_leak_historically(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    rhea = world.find("Rhea", "character")
    saltmere = world.find("Saltmere Campfire", "scene")
    result = search_world(
        frontiersmen_repo,
        "Tree King Aldren Veyl",
        perspective="character",
        character_id=rhea.id,
        scene_id=saltmere.id,
        tick=55,
        limit=30,
    )
    titles = {item["metadata"].get("title") for item in result["results"]}
    assert "The Tree King" not in titles
    assert not any("Tree King" in str(title) for title in titles)

    southward = world.find("Southward Cut", "scene")
    for party_alias in ("Rhea", "Syl", "Garran", "Veyra", "Pip"):
        character = world.find(party_alias, "character")
        alias = search_world(
            frontiersmen_repo,
            '"Lio Vane"',
            perspective="character",
            character_id=character.id,
            scene_id=southward.id,
            tick=210,
            limit=30,
        )
        assert alias["results"] == [], party_alias


def test_character_search_normalizes_title_and_alias_to_canonical_id(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    rhea = world.find("Rhea", "character")
    scene = world.find("The Hunt Begins", "scene")
    kwargs = {
        "perspective": "character",
        "scene_id": scene.id,
        "tick": 195,
        "mode": "fts",
        "limit": 30,
    }
    canonical = search_world(frontiersmen_repo, "Northwood", character_id=rhea.id, **kwargs)
    title = search_world(frontiersmen_repo, "Northwood", character_id=rhea.title, **kwargs)
    alias = search_world(frontiersmen_repo, "Northwood", character_id="Rhea", **kwargs)

    assert canonical["results"]
    expected_ids = [item["documentId"] for item in canonical["results"]]
    for result in (canonical, title, alias):
        assert result["characterId"] == rhea.id
        assert [item["documentId"] for item in result["results"]] == expected_ids


def test_frontier_author_secret_is_filtered_before_search_ranking(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    rhea = world.find("Rhea", "character")
    scene = world.find("The Hunt Begins", "scene")
    marker = "FRONTIER-SECRET-AMBER-ROOT-MEMORY-7K4M"
    character = search_world(
        frontiersmen_repo,
        marker,
        perspective="character",
        character_id=rhea.id,
        scene_id=scene.id,
        tick=195,
        limit=30,
    )
    author = search_world(
        frontiersmen_repo,
        marker,
        perspective="author",
        scene_id=scene.id,
        tick=195,
        limit=30,
    )
    assert character["results"] == []
    assert {item["metadata"].get("title") for item in author["results"]} >= {
        "The Frontiersmen",
        "Guildmaster Halric Doss",
    }


def test_author_time_bounds_environment_observation_and_scene_aggregate_before_ranking(frontiersmen_repo) -> None:
    compile_world(frontiersmen_repo, force=True, profile_name="hybrid")
    cases = (
        (
            "Gold light moves inside the amber without changing the room's shadows",
            "environment",
            62,
            63,
            73,
        ),
        (
            "Each amber disc holds a moving gold sheen that remains level when the disc tilts",
            "scene-observation",
            22,
            23,
            26,
        ),
        (
            "The Frontier's currency arrives without a lesson",
            "entity-author",
            24,
            25,
            26,
        ),
    )
    for phrase, kind, before_tick, at_tick, after_tick in cases:
        for mode in ("fts", "vector", "hybrid"):
            before = search_world(frontiersmen_repo, f'"{phrase}"', perspective="author", mode=mode, tick=before_tick, include_text=True)
            at = search_world(frontiersmen_repo, f'"{phrase}"', perspective="author", mode=mode, tick=at_tick, include_text=True)
            after = search_world(frontiersmen_repo, f'"{phrase}"', perspective="author", mode=mode, tick=after_tick, include_text=True)
            assert not any(phrase.casefold() in item.get("text", "").casefold() for item in before["results"])
            assert any(item["documentKind"] == kind and phrase.casefold() in item.get("text", "").casefold() for item in at["results"])
            if kind == "environment":
                assert not any(phrase.casefold() in item.get("text", "").casefold() for item in after["results"])


def test_audible_pursuer_is_not_scene_present(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    scene = world.find("The Hunt Begins", "scene")
    rootjaw = world.find("Rootjaw", "character")
    conversation = world.find("Running Under the Drums", "conversation")
    view = conversation_view(
        frontiersmen_repo,
        conversation.id,
        perspective="character",
        character_id=rootjaw.id,
    )
    heard = view["heardVerbatimTurns"]
    assert len(heard) == 4
    assert heard[0]["at"] == {"timeline": "main", "tick": 193, "order": 10}
    assert heard[0]["speaker"] == "Rootjaw"
    assert "bellow" in heard[0]["text"]
    with pytest.raises(UsageError, match="not present"):
        build_context(
            frontiersmen_repo,
            character_id=rootjaw.id,
            scene_id=scene.id,
            max_characters=5000,
        )


def test_act_six_boundaries_custody_and_unresolved_threads(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    hunt = world.find("The Hunt Begins", "scene")
    pursuit = world.find("Running Under the Drums", "conversation")
    waymark = world.find("Drowned Waymark", "scene")
    southward = world.find("Southward Cut", "scene")
    waymark_conversation = world.find("Water Over the Mark", "conversation")
    southward_conversation = world.find("The Southward Cut", "conversation")
    rootjaw = world.find("Rootjaw", "character")
    pip = world.find("Pip", "character")
    rhea = world.find("Rhea", "character")
    veyra = world.find("Veyra", "character")

    assert hunt.frontmatter["status"] == "closed"
    assert "active" not in hunt.frontmatter.get("tags", [])
    assert hunt.frontmatter["time"]["end"] == {"timeline": "main", "tick": 195, "order": 99}
    assert pursuit.frontmatter["time"]["end"] == {"timeline": "main", "tick": 195, "order": 99}
    assert waymark.frontmatter["time"]["start"] == {"timeline": "main", "tick": 196, "order": 0}
    for record in (waymark, southward, waymark_conversation, southward_conversation):
        assert rootjaw.id not in {item["character"] for item in record.frontmatter["participants"]}
    for conversation in (waymark_conversation, southward_conversation):
        rootjaw_view = conversation_view(
            frontiersmen_repo,
            conversation.id,
            perspective="character",
            character_id=rootjaw.id,
            tick=210,
        )
        assert rootjaw_view["heardVerbatimTurns"] == []
        assert rootjaw_view["subjectiveRecollection"] is None
    for scene in (waymark, southward):
        with pytest.raises(UsageError, match="not present"):
            build_context(frontiersmen_repo, character_id=rootjaw.id, scene_id=scene.id, max_characters=5000)

    bone_key = world.find("Moth's Bone Key", "object")
    bone_key_at_210 = entity_state(frontiersmen_repo, bone_key.id, 210)["state"]
    assert bone_key_at_210["holder"] == {"entity": pip.id}
    assert any(
        "hooked notch" in constraint and "unused" in constraint
        for constraint in southward.frontmatter["author_constraints"]
    )

    counterfoil = world.find("Blackroot Warden Counterfoil", "object")
    counterfoil_at_210 = entity_state(frontiersmen_repo, counterfoil.id, 210)["state"]
    assert counterfoil_at_210["holder"] == {"entity": rhea.id}

    reliquary = world.find("Amber Reliquary", "object")
    reliquary_at_210 = entity_state(frontiersmen_repo, reliquary.id, 210)["state"]
    assert reliquary_at_210["holder"] == {"entity": veyra.id}
    assert reliquary_at_210["condition"].startswith("closed;")

    jorund = world.find("Jorund", "character")
    jorund_at_210 = entity_state(frontiersmen_repo, jorund.id, 210)["state"]
    assert jorund_at_210["condition"] == "separated behind collapse; fate unknown"
    assert "survival remains unresolved" in jorund.body

    states = {
        item["title"]: item["derivedState"]
        for item in story_points(frontiersmen_repo, scene_id=southward.id)["storyPoints"]
    }
    assert states["Return to Harrowcross"] == "active"
    assert southward.frontmatter["location"] != world.find("Harrowcross", "location").id


def test_frontiersmen_routes_are_authored_only_for_established_travel(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    titles = {record.title: record.id for record in world.by_kind("location")}
    expected = {
        ("Keldmouth East Gate", "East Coastal Road"): (
            "The palisade gate opens onto the coast road.",
            "Leave Keldmouth by the coast road.",
        ),
        ("East Coastal Road", "Saltmere Camp"): (
            "The caravan road passes the raised camp beside its brackish pond.",
            "Follow the coast road to Saltmere Camp.",
        ),
        ("Saltmere Camp", "Harrowcross"): (
            "The eastroad caravan continues from this camp to Harrowcross.",
            "Continue with the caravan to Harrowcross.",
        ),
        ("Harrowcross", "Harrowcross East Orchard"): (
            "The old mine track leaves town through the east orchard.",
            "Take the old mine track to the east orchard.",
        ),
        ("Harrowcross East Orchard", "Saint Orra Mine Yard"): (
            "The mine road leads from the orchard to Saint Orra's yard.",
            "Follow the mine road to Saint Orra.",
        ),
        ("Saint Orra Mine Yard", "Saint Orra Upper Drift"): (
            "The sealed upper drift descends from the mine yard.",
            "Enter the sealed upper drift.",
        ),
        ("Saint Orra Upper Drift", "Deep Amber Chamber"): (
            "The upper drift reaches the chamber where the living amber answers.",
            "Continue to the deep amber chamber.",
        ),
        ("Cave-In Pocket", "Lower Caverns"): (
            "A lower fissure carries breathable air into older passages.",
            "Follow the lower air into the caverns.",
        ),
        ("Lower Caverns", "Blackwater Crossing"): (
            "The lower route reaches a waist-deep blackwater crossing.",
            "Continue to the blackwater crossing.",
        ),
        ("Lower Caverns", "Sunken Hall Ruins"): (
            "The older passages lead onward to constructed ruins.",
            "Continue to the Sunken Hall.",
        ),
        ("Blackwater Crossing", "Lower Caverns"): (
            "The far bank opens back into the lower cavern passages.",
            "Return to the lower caverns beyond the water.",
        ),
        ("Sunken Hall Ruins", "Wretch Gallery"): (
            "A long relief-lined gallery extends from the tilted hall.",
            "Enter the Wretch Gallery.",
        ),
        ("Wretch Gallery", "Hilltop Sink"): (
            "The escape route rises to the stone throat at Hilltop Sink.",
            "Escape upward to Hilltop Sink.",
        ),
        ("Root Camp Prison Pens", "Root-Crowned Camp"): (
            "The opened pen gives onto the masked camp and its gear racks.",
            "Slip out into Root-Crowned Camp.",
        ),
        ("Root-Crowned Camp", "Northwood Pursuit Trail"): (
            "The fleeing hunters break from the camp onto a rain-black forest trail.",
            "Flee along the Northwood pursuit trail.",
        ),
        ("Northwood Pursuit Trail", "Drowned Waymark"): (
            "The pursuit trail drops into the flooded cutting at the drowned stone.",
            "Reach the Drowned Waymark.",
        ),
        ("Drowned Waymark", "Abandoned Warden Road"): (
            "A surviving warden cut continues south from the drowned road mark.",
            "Take the southward cut along the Warden Road.",
        ),
    }
    actual = {
        (record.title, world.get(link["location"]).title): (link["description"], link["summary"])
        for record in world.by_kind("location")
        for link in record.frontmatter["links"]
    }

    assert actual == expected
    assert len(actual) == 17
    assert all(description.strip() and summary.strip() for description, summary in actual.values())

    reciprocal_pairs = {
        frozenset((source, destination))
        for source, destination in actual
        if (destination, source) in actual
    }
    assert reciprocal_pairs == {frozenset(("Lower Caverns", "Blackwater Crossing"))}
    assert all(source in titles and destination in titles for source, destination in actual)

    assert ("Deep Amber Chamber", "Cave-In Pocket") not in actual
    assert ("Hilltop Sink", "Root-Crowned Camp") not in actual
    assert ("Abandoned Warden Road", "Harrowcross") not in actual
    assert not any("Blackroot Sink" in edge for edge in actual)


def test_jorund_becomes_unlocated_after_the_collapse(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    jorund = world.find("Jorund", "character")
    deep_amber = world.find("Deep Amber Chamber", "location")
    cave_in = world.find("Cave-In Pocket", "location")

    before_collapse, _ = resolve_state(world, jorund.id, StoryTime("main", 89, 99))
    state_after_collapse, _ = resolve_state(world, jorund.id, StoryTime("main", 91, 99))
    assert before_collapse["location"] == {"entity": deep_amber.id}
    assert "location" not in state_after_collapse
    assert state_after_collapse["condition"] == "separated behind collapse; fate unknown"
    assert cave_in.id not in state_after_collapse.values()
    lost_event = world.find("Jorund Is Lost Behind the Collapse", "event")
    assert jorund.id not in {item["character"] for item in lost_event.frontmatter["participants"]}


def test_tree_king_transcript_preserves_mute_acolyte(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    conversation = world.find("The Tree King's Offer", "conversation")
    author = conversation_view(frontiersmen_repo, conversation.id, perspective="author")
    speakers = [turn["speaker"] for turn in author["verbatimTurns"]]
    assert len(author["verbatimTurns"]) == 20
    assert "Moth" not in speakers
    assert any(turn["speaker"] == "Rootjaw" and "grunt" in turn["text"] for turn in author["verbatimTurns"])
    assert any("Aldren Veyl" in turn["text"] for turn in author["verbatimTurns"])


def test_frontiersmen_story_point_arc_resolves_the_chase_and_keeps_campaign_threads_active(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    scene = world.find("Southward Cut", "scene")
    states = {
        item["title"]: item["derivedState"]
        for item in story_points(frontiersmen_repo, scene_id=scene.id)["storyPoints"]
    }
    for title in [
        "Escape Before the Blood Sport",
        "Escape the Cave-In",
        "Escape the Wretch Gallery",
        "Understand the Tree King",
        "Trace the Attacks to Saint Orra's Mine",
        "Protect the Eastroad Caravan",
        "Flee the Root Host",
    ]:
        assert states[title] == "resolved"
    for title in [
        "Expose the Blackroot Compact",
        "Return to Harrowcross",
        "Identify the Silent Acolyte",
    ]:
        assert states[title] == "active"


def test_frontiersmen_is_selectable_from_init(tmp_path) -> None:
    target = tmp_path / "named-frontiersmen"
    result = initialize(
        target,
        example="frontiersmen",
        profile_name="state",
    )
    assert result["example"] == "frontiersmen"
    assert result["compile"]["recordCount"] == 309
    assert (target / "story" / "conversations").is_dir()
    assert len(list((target / "story").rglob("*.md"))) == 309
