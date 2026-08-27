from __future__ import annotations

from pathlib import Path

import pytest

from wedl.compiler import compile_world
from wedl.context import build_context
from wedl.errors import UsageError


THREAD_A = "thread_0123456789ABCDEFGHJKMNPQRS"


def _append_frontmatter(path: Path, value: str) -> None:
    frontmatter, body = path.read_text(encoding="utf-8").split("\n---\n", 1)
    path.write_text(f"{frontmatter}\n{value}\n---\n{body}", encoding="utf-8")


def _upgrade_to_v05_with_empty_thread(repository) -> None:
    for path in (repository.root / "story").rglob("*.md"):
        path.write_text(
            path.read_text(encoding="utf-8").replace("schema: wedl/v0.3", "schema: wedl/v0.5", 1),
            encoding="utf-8",
        )
    _append_frontmatter(
        repository.root / "story" / "world.md",
        f"threads:\n  - id: {THREAD_A}\n    label: Archive",
    )


def _section(prompt: str, heading: str) -> str:
    marker = f"## {heading}\n"
    return prompt.split(marker, 1)[1].split("\n## ", 1)[0]


@pytest.mark.parametrize(("max_characters", "max_items"), [(5000, 12), (5000, 10)])
def test_context_private_thread_filter_only_removes_useful_recall_and_preserves_required_atoms(
    frontiersmen_repo, max_characters: int, max_items: int,
) -> None:
    world = frontiersmen_repo.load_world()
    mara = world.find("Rhea", "character")
    scene = world.find("The Hunt Begins", "scene")
    _upgrade_to_v05_with_empty_thread(frontiersmen_repo)
    compile_world(frontiersmen_repo, revision="WORKTREE", profile_name="hybrid")
    arguments = {
        "character_id": mara.id,
        "scene_id": scene.id,
        "revision": "WORKTREE",
        "query": "amber reliquary Root Host",
        "max_characters": max_characters,
        "max_items": max_items,
        "search_mode": "hybrid",
    }

    baseline = build_context(frontiersmen_repo, **arguments)
    filtered = build_context(frontiersmen_repo, _thread_filter_ids=(THREAD_A,), **arguments)

    assert baseline["focus"]["retrieval"]["eligibleCandidates"] > 0
    assert filtered["focus"]["retrieval"] == baseline["focus"]["retrieval"]
    assert "## Useful recall" in baseline["promptText"]
    assert "## Useful recall" not in filtered["promptText"]
    for heading in (
        "Voice and intention", "Present moment", "Conversation now", "What matters",
        "Remembered conversations", "Relationship pressure", "Writing boundary",
    ):
        assert _section(filtered["promptText"], heading) == _section(baseline["promptText"], heading)
    assert filtered["selection"]["included"] < baseline["selection"]["included"]
    assert filtered["selection"]["omitted"] > baseline["selection"]["omitted"]
    assert filtered["selection"]["serializedCharacters"] < baseline["selection"]["serializedCharacters"]


def test_context_none_filter_and_author_margin_are_byte_identical(ash_repo) -> None:
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    scene = world.find("An Honest Absence", "scene")
    _upgrade_to_v05_with_empty_thread(ash_repo)
    arguments = {
        "character_id": mara.id,
        "scene_id": scene.id,
        "revision": "WORKTREE",
        "query": "register ribbon Ysabet evidence",
        "max_characters": 3000,
        "search_mode": "fts",
    }

    baseline = build_context(ash_repo, **arguments)
    assert build_context(ash_repo, _thread_filter_ids=None, **arguments) == baseline
    author = build_context(ash_repo, perspective="author", **arguments)
    assert build_context(ash_repo, perspective="author", _thread_filter_ids=(THREAD_A,), **arguments) == author


def test_context_thread_filter_preserves_character_authorization_and_dramatic_author_margin(ash_repo) -> None:
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    scene = world.find("An Honest Absence", "scene")
    _upgrade_to_v05_with_empty_thread(ash_repo)
    compile_world(ash_repo, revision="WORKTREE", profile_name="fts")
    arguments = {
        "character_id": mara.id,
        "scene_id": scene.id,
        "revision": "WORKTREE",
        "query": "ASH-SECRET-LETTER-CONTENTS-7F3Q",
        "search_mode": "fts",
    }

    filtered = build_context(ash_repo, max_characters=3000, _thread_filter_ids=(THREAD_A,), **arguments)
    baseline_dramatic = build_context(ash_repo, perspective="dramatic-irony", max_characters=8000, **arguments)
    filtered_dramatic = build_context(
        ash_repo, perspective="dramatic-irony", max_characters=8000,
        _thread_filter_ids=(THREAD_A,), **arguments,
    )

    assert "ASH-SECRET-LETTER-CONTENTS-7F3Q" not in filtered["promptText"]
    assert filtered_dramatic["authorMargin"] == baseline_dramatic["authorMargin"]


def test_context_thread_filter_rejects_v03_after_cache_resolution(ash_repo) -> None:
    world = ash_repo.load_world()
    mara = world.find("Mara Vale", "character")
    scene = world.find("An Honest Absence", "scene")
    compile_world(ash_repo, profile_name="fts")

    with pytest.raises(UsageError, match="^thread filtering requires a validated wedl/v0.5 world$"):
        build_context(
            ash_repo, character_id=mara.id, scene_id=scene.id,
            _thread_filter_ids=(THREAD_A,), search_mode="fts",
        )
