from __future__ import annotations

from contextlib import closing
import json

import pytest

from wedl.compiler import DOCUMENT_GENERATION_TOKEN, cache_readiness, compile_world, connect, database_meta, require_database
from wedl.context import build_context
from wedl.errors import CompileRequired, UsageError
from wedl.query import search_world, status


def test_compile_cache_and_embedding_reuse(ash_repo) -> None:
    first = compile_world(ash_repo, force=True)
    assert first["recordCount"] == 262
    assert first["searchDocumentCount"] > 300
    second = compile_world(ash_repo)
    assert second["status"] == "cache-hit"
    third = compile_world(ash_repo, force=True)
    assert third["vectorCache"]["generated"] == 0
    assert third["vectorCache"]["reused"] == third["uniqueVectorCount"]
    assert third["vectorLinkCount"] == third["searchDocumentCount"]
    assert third["uniqueVectorCount"] < third["vectorLinkCount"]
    with connect(ash_repo.root / ".wedl" / "world.sqlite", True) as connection:
        assert connection.execute("PRAGMA integrity_check").fetchone()[0] == "ok"


def test_document_generation_token_rebuilds_stale_compiled_databases(ash_repo) -> None:
    compile_world(ash_repo, force=True, profile_name="fts")
    database = ash_repo.root / ".wedl" / "world.sqlite"
    with closing(connect(database)) as connection:
        connection.execute("UPDATE revision SET compiler_fingerprint='legacy-search-documents'")
        connection.commit()

    assert compile_world(ash_repo)["status"] == "compiled"
    with closing(connect(database)) as connection:
        connection.execute("UPDATE revision SET compiler_fingerprint='legacy-search-documents'")
        connection.commit()

    world, rebuilt = require_database(ash_repo)
    assert rebuilt == database
    assert len(world.records) == 262
    assert database_meta(database)["compiler_fingerprint"].startswith(DOCUMENT_GENERATION_TOKEN + ":")


def test_compiled_cache_readiness_and_strict_reads_do_not_mutate_missing_cache(ash_repo) -> None:
    cache = ash_repo.root / ".wedl"
    missing = cache_readiness(ash_repo)
    assert missing["state"] == "missing"
    assert missing["target"]["revision"] == ash_repo.resolve("HEAD")
    assert not cache.exists()

    with pytest.raises(CompileRequired) as failure:
        require_database(ash_repo, require_compiled=True)
    assert failure.value.as_dict()["code"] == "compile_required"
    assert failure.value.details["cache"]["state"] == "missing"
    assert not cache.exists()

    # The default remains deliberately convenient for interactive reads.
    world, database = require_database(ash_repo)
    assert len(world.records) == 262
    assert database.exists()
    assert cache_readiness(ash_repo)["state"] == "ready"
    assert status(ash_repo)["cacheReadiness"]["state"] == "ready"


def test_compiled_cache_readiness_distinguishes_stale_and_incompatible(ash_repo) -> None:
    compile_world(ash_repo, profile_name="fts")
    database = ash_repo.root / ".wedl" / "world.sqlite"
    with closing(connect(database)) as connection:
        connection.execute("UPDATE revision SET tree_oid='not-the-current-tree'")
        connection.commit()
    assert cache_readiness(ash_repo)["state"] == "stale"
    with pytest.raises(CompileRequired) as stale:
        require_database(ash_repo, require_compiled=True)
    assert stale.value.details["cache"]["state"] == "stale"

    compile_world(ash_repo, force=True, profile_name="fts")
    with closing(connect(database)) as connection:
        connection.execute("UPDATE revision SET compiler_fingerprint='old'")
        connection.commit()
    incompatible = cache_readiness(ash_repo)
    assert incompatible["state"] == "incompatible"
    assert "compilerFingerprint" in incompatible["incompatibleFields"]


def test_character_search_does_not_leak_author_markers(ash_repo) -> None:
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    scene = world.find("An Honest Absence", "scene")
    result = search_world(ash_repo, "ASH-SECRET-LETTER-CONTENTS-7F3Q", perspective="character", character_id=mara.id, scene_id=scene.id)
    assert result["results"] == []


def test_author_search_filters_future_turns_before_every_lane_ranks(ash_repo) -> None:
    compile_world(ash_repo, force=True, profile_name="hybrid")
    for mode in ("fts", "vector", "hybrid"):
        before = search_world(
            ash_repo,
            "in the name of the watch",
            perspective="author",
            mode=mode,
            tick=141,
        )
        at_turn = search_world(
            ash_repo,
            "in the name of the watch",
            perspective="author",
            mode=mode,
            tick=142,
            order=20,
        )
        assert before["protocol"] == "wedl-search/v5"
        assert before["timeScope"]["mode"] == "as-of"
        assert all("in the name of the watch" not in item.get("snippet", "").casefold() for item in before["results"])
        assert any(item["documentKind"] == "conversation-turn" for item in at_turn["results"])

    full = search_world(ash_repo, "in the name of the watch", perspective="author", mode="fts", all_time=True)
    assert full["timeScope"] == {"mode": "all-time"}
    with pytest.raises(UsageError, match="requires --tick"):
        search_world(ash_repo, "door", perspective="author", mode="fts")
    with pytest.raises(UsageError, match="only for author"):
        search_world(ash_repo, "door", perspective="character", all_time=True)


def test_author_search_time_bounds_event_turn_recollection_and_transition_in_all_lanes(ash_repo) -> None:
    compile_world(ash_repo, force=True, profile_name="hybrid")
    cases = (
        ("Ysabet enters by the service door and offers to put only one item", "entity-author", 138, 139),
        ("Open this door in the name of the watch", "conversation-turn", 141, 142),
        ("Mara surrendered the most admissible object", "conversation-recollection", 142, 143),
        ("Halver is found in the office", "transition", 151, 152),
        ("admitted-listener", "transition", 165, 166),
        ("suspended from Archive authority pending inquiry", "event-effect", 196, 197),
    )
    for phrase, kind, before_tick, at_tick in cases:
        for mode in ("fts", "vector", "hybrid"):
            before = search_world(ash_repo, f'"{phrase}"', perspective="author", mode=mode, tick=before_tick, include_text=True)
            at = search_world(ash_repo, f'"{phrase}"', perspective="author", mode=mode, tick=at_tick, include_text=True)
            assert not any(phrase.casefold() in item.get("text", "").casefold() for item in before["results"]), f"future leakage for {phrase!r} in {mode}"
            assert any(item["documentKind"] == kind and phrase.casefold() in item.get("text", "").casefold() for item in at["results"])


def test_context_is_prose_first_and_hard_bounded(ash_repo) -> None:
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    scene = world.find("An Honest Absence", "scene")
    packet = build_context(ash_repo, character_id=mara.id, scene_id=scene.id, query="register ribbon Ysabet evidence", max_characters=3000)
    serialized = json.dumps(packet, ensure_ascii=False, separators=(",", ":"))
    assert len(serialized) <= 3000
    assert packet["protocol"] == "wedl-context/v3"
    assert packet["promptText"].startswith("# Mara Vale")
    assert "## Conversation now" in packet["promptText"]
    assert "## Provenance" in packet["promptText"]
    assert "activeKnowledge" not in packet


def test_context_query_focus_explains_ranking_and_minimum_budget(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    rhea = world.find("Rhea", "character")
    scene = world.find("The Hunt Begins", "scene")
    unfocused = build_context(frontiersmen_repo, character_id=rhea.id, scene_id=scene.id, max_characters=2600)
    focused = build_context(
        frontiersmen_repo,
        character_id=rhea.id,
        scene_id=scene.id,
        query="amber reliquary Root Host",
        max_characters=2600,
    )

    assert focused["focus"] == {
        "query": "amber reliquary Root Host",
        "terms": ["amber", "host", "reliquary", "root"],
        "applied": True,
        "effect": "Matching terms raise the relevance of candidate context before it is ranked into the character packet.",
        "retrieval": {
            "requested": True,
            "mode": "hybrid",
            "eligibleCandidates": 3,
            "effect": "Only perspective-safe, accessible recall is eligible for query retrieval.",
        },
    }
    assert unfocused["focus"] == {
        "query": None,
        "terms": [],
        "applied": False,
        "effect": "No effective query terms were provided. Selection prioritizes the present scene and required writing boundaries.",
        "retrieval": {"requested": False, "eligibleCandidates": 0},
    }
    assert "With **Pip Fenlock**" in focused["promptText"]
    assert "With **Pip Fenlock**" not in unfocused["promptText"]

    minimum = focused["selection"]["minimumBudgetCharacters"]
    assert minimum <= focused["selection"]["serializedCharacters"] <= 2600
    minimum_packet = build_context(
        frontiersmen_repo,
        character_id=rhea.id,
        scene_id=scene.id,
        query="amber reliquary Root Host",
        max_characters=minimum,
    )
    assert len(json.dumps(minimum_packet, ensure_ascii=False, separators=(",", ":"))) <= minimum
    with pytest.raises(UsageError, match=rf"at least {minimum} characters.*--max-characters to {minimum}"):
        build_context(
            frontiersmen_repo,
            character_id=rhea.id,
            scene_id=scene.id,
            query="amber reliquary Root Host",
            max_characters=minimum - 1,
        )

    floor_packet = build_context(frontiersmen_repo, character_id=rhea.id, scene_id=scene.id, max_characters=1800)
    assert floor_packet["selection"]["minimumBudgetCharacters"] == 1800
    assert len(json.dumps(floor_packet, ensure_ascii=False, separators=(",", ":"))) <= 1800
    with pytest.raises(UsageError, match=r"at least 1800 characters.*--max-characters to 1800"):
        build_context(frontiersmen_repo, character_id=rhea.id, scene_id=scene.id, max_characters=1799)


def test_stopword_only_query_has_no_effective_context_focus(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    rhea = world.find("Rhea", "character")
    scene = world.find("The Hunt Begins", "scene")
    unfocused = build_context(frontiersmen_repo, character_id=rhea.id, scene_id=scene.id, max_characters=2600)
    stopword_only = build_context(
        frontiersmen_repo,
        character_id=rhea.id,
        scene_id=scene.id,
        query="the and of",
        max_characters=2600,
    )
    assert stopword_only == unfocused


def test_context_focus_is_safe_for_author_and_dramatic_irony_packets(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    rhea = world.find("Rhea", "character")
    scene = world.find("The Hunt Begins", "scene")
    author = build_context(
        frontiersmen_repo,
        character_id=rhea.id,
        scene_id=scene.id,
        perspective="author",
        query="Blackroot Compact",
        max_characters=3000,
    )
    assert author["focus"] == {
        "query": "Blackroot Compact",
        "terms": ["blackroot", "compact"],
        "applied": True,
        "effect": "Matching terms raise the relevance of canonical author candidates before they are ranked into the author margin.",
    }

    dramatic = build_context(
        frontiersmen_repo,
        character_id=rhea.id,
        scene_id=scene.id,
        perspective="dramatic-irony",
        query="Blackroot Compact",
        max_characters=8000,
    )
    minimum = dramatic["selection"]["minimumBudgetCharacters"]
    assert dramatic["focus"] == {
        "query": "Blackroot Compact",
        "terms": ["blackroot", "compact"],
        "applied": True,
        "effect": "The query is applied independently to the character packet and author margin.",
    }
    assert len(json.dumps(dramatic, ensure_ascii=False, separators=(",", ":"))) <= 8000
    exact = build_context(
        frontiersmen_repo,
        character_id=rhea.id,
        scene_id=scene.id,
        perspective="dramatic-irony",
        query="Blackroot Compact",
        max_characters=minimum,
    )
    assert len(json.dumps(exact, ensure_ascii=False, separators=(",", ":"))) <= minimum
    with pytest.raises(UsageError, match=rf"at least {minimum} characters.*--max-characters to {minimum}"):
        build_context(
            frontiersmen_repo,
            character_id=rhea.id,
            scene_id=scene.id,
            perspective="dramatic-irony",
            query="Blackroot Compact",
            max_characters=minimum - 1,
        )


def test_distinct_context_queries_change_eligible_recall_and_ranking(frontiersmen_repo) -> None:
    world = frontiersmen_repo.load_world()
    rhea = world.find("Rhea", "character")
    scene = world.find("The Hunt Begins", "scene")
    amber = build_context(
        frontiersmen_repo,
        character_id=rhea.id,
        scene_id=scene.id,
        query="amber reliquary Root Host",
        max_characters=2600,
    )
    route = build_context(
        frontiersmen_repo,
        character_id=rhea.id,
        scene_id=scene.id,
        query="water route drumline",
        max_characters=2600,
    )
    assert amber["focus"]["terms"] == ["amber", "host", "reliquary", "root"]
    assert route["focus"]["terms"] == ["drumline", "route", "water"]
    assert amber["focus"]["retrieval"]["eligibleCandidates"] == 3
    assert route["focus"]["retrieval"]["eligibleCandidates"] == 1
    assert "With **Pip Fenlock**" in amber["promptText"]
    assert "With **Brother Garran Holt**" in route["promptText"]


def test_planned_scene_and_absent_character_are_rejected(ash_repo) -> None:
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    planned = world.find("River Gate at Dawn", "scene")
    with pytest.raises(UsageError):
        build_context(ash_repo, character_id=mara.id, scene_id=planned.id)
    caldrin = world.find("Councillor Caldrin Vey", "character")
    with pytest.raises(UsageError):
        build_context(ash_repo, character_id=caldrin.id, scene_id=world.find("An Honest Absence", "scene").id)
