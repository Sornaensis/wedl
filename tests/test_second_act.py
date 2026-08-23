from __future__ import annotations

import json

from wedl.compiler import require_database
from wedl.context import build_context
from wedl.query import conversation_view, knowledge, search_world, story_points


def test_completed_story_has_substantial_conversation_provenance(ash_repo) -> None:
    world = ash_repo.load_world()
    conversations = world.by_kind("conversation")
    assert len(world.records) == 262
    assert len(conversations) == 16
    assert sum(len(record.frontmatter.get("turns") or []) for record in conversations) == 138
    assert sum(len(record.frontmatter.get("recollections") or []) for record in conversations) == 58
    assert world.active_scene() is None
    assert world.find("An Honest Absence", "scene").status == "closed"


def test_late_participants_hear_only_later_turns(ash_repo) -> None:
    world = ash_repo.load_world()
    choice = world.find("The Choice of Records", "conversation")
    ansel = world.find("Sister Ansel Marr", "character")
    ansel_view = conversation_view(ash_repo, choice.id, perspective="character", character_id=ansel.id, tick=208)
    assert len(ansel_view["heardVerbatimTurns"]) == 9
    assert ansel_view["heardVerbatimTurns"][0]["text"] == "The shutters are closing. Argue while walking."

    pursuit = world.find("River Gate Pursuit", "conversation")
    rusk = world.find("Captain Tomas Rusk", "character")
    rusk_view = conversation_view(ash_repo, pursuit.id, perspective="character", character_id=rusk.id, tick=208)
    assert len(rusk_view["heardVerbatimTurns"]) == 11
    assert rusk_view["heardVerbatimTurns"][0]["at"]["tick"] == 184
    assert rusk_view["heardVerbatimTurns"][0]["text"] == "Give me the route key."


def test_completed_arc_story_points_are_resolved(ash_repo) -> None:
    world = ash_repo.load_world()
    scene = world.find("An Honest Absence", "scene")
    result = story_points(ash_repo, scene_id=scene.id)
    states = {item["title"]: item["derivedState"] for item in result["storyPoints"]}
    for title in [
        "Build the Incomplete Archive",
        "Decide the Fate of the Descendant Ledger",
        "Survive the Vault Alarm",
        "Confront Caldrin Vey",
        "Question Sable at the River Gate",
        "Recover the Restricted-Vault Key",
    ]:
        assert states[title] == "resolved"


def test_final_context_is_budgeted_and_narratively_coherent(ash_repo) -> None:
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    scene = world.find("An Honest Absence", "scene")
    result = build_context(
        ash_repo,
        character_id=mara.id,
        scene_id=scene.id,
        perspective="character",
        query="honest absence distributed custody and the reopened Archive",
        max_characters=5000,
        max_items=18,
    )
    serialized = json.dumps(result, ensure_ascii=False, separators=(",", ":"))
    assert len(serialized) <= 5000
    assert "An Honest Absence" in result["promptText"]
    assert "records the scope, witnesses, reason, expiration" in result["promptText"]
    assert "Find Ilyra" not in result["promptText"]


def test_historical_search_does_not_index_future_knowledge_bodies(ash_repo) -> None:
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    scene = world.find("After the Exchange", "scene")
    future_ids = {
        "know_5FYZC5YK86BB7C379AWZQJVNV7",  # threefold custody
        "know_7E4FSNDHGQ09P85XYSGMQHW858",  # honest-absence rule
        "know_15SY5WFXWK2XQ9SWDWSVXMCQCA",  # Caldrin suspension
    }
    for query in ("threefold custody", "honest absence", "Caldrin suspended"):
        result = search_world(
            ash_repo,
            query,
            perspective="character",
            character_id=mara.id,
            scene_id=scene.id,
            tick=121,
            limit=30,
        )
        assert not ({item["entityId"] for item in result["results"]} & future_ids)


def test_ilyra_missing_belief_is_rejected_after_return(ash_repo) -> None:
    world = ash_repo.load_world()
    oren = world.find("Oren Thane", "character")
    items = knowledge(ash_repo, oren.id, 208)["knowledge"]
    missing = next(item for item in items if item["claimKey"] == "ilyra.status.missing")
    assert missing["state"] == "rejected"
    assert missing["causingEventId"] == "event_26DAENK1PSARN2YRVVTZ2T36SA"


def test_secret_markers_remain_author_only_in_epilogue(ash_repo) -> None:
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    scene = world.find("An Honest Absence", "scene")
    query = "ASH-SECRET-ILYRA-UNDERCROFT-9K2M ASH-SECRET-LETTER-CONTENTS-7F3Q"
    character = search_world(
        ash_repo,
        query,
        perspective="character",
        character_id=mara.id,
        scene_id=scene.id,
        limit=12,
    )
    author = search_world(ash_repo, query, perspective="author", scene_id=scene.id, limit=12, tick=208)
    assert character["results"] == []
    assert {item["metadata"]["title"] for item in author["results"]} >= {
        "Ilyra Sorn",
        "Sealed Heron Letter",
    }


def test_compiled_world_is_reused_within_process(ash_repo, monkeypatch) -> None:
    first, _database = require_database(ash_repo)
    assert len(first.records) == 262

    def fail_reload(*_args, **_kwargs):  # pragma: no cover - must not execute
        raise AssertionError("compiled world was reconstructed twice")

    monkeypatch.setattr("wedl.compiler.world_from_database", fail_reload)
    second, _database = require_database(ash_repo)
    assert second is first
