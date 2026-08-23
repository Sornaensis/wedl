#!/usr/bin/env python3
"""Run repeatable in-process wedl compile/query benchmarks and emit JSON."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import shutil
import statistics
import time
from typing import Any, Callable

from wedl.compiler import compile_world
from wedl.context import build_context
from wedl.query import conversation_view, search_world, story_points
from wedl.repository import Repository
from wedl.validation import validate_world


def measure(function: Callable[[], Any], repeats: int) -> tuple[dict[str, float], Any]:
    samples: list[float] = []
    value: Any = None
    for _ in range(repeats):
        start = time.perf_counter()
        value = function()
        samples.append((time.perf_counter() - start) * 1000.0)
    ordered = sorted(samples)
    p95_index = min(len(ordered) - 1, max(0, int(len(ordered) * 0.95) - 1))
    return {
        "repeats": repeats,
        "minimumMs": round(min(samples), 3),
        "medianMs": round(statistics.median(samples), 3),
        "p95Ms": round(ordered[p95_index], 3),
        "maximumMs": round(max(samples), 3),
    }, value


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", required=True, type=Path)
    parser.add_argument("--character")
    parser.add_argument("--scene")
    parser.add_argument("--conversation")
    parser.add_argument("--query", default="evidence custody listening route remembered warning")
    parser.add_argument("--repeats", type=int, default=7)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    repository = Repository(args.repo)
    world = repository.load_world("HEAD")
    active = world.active_scene()
    scene_id = args.scene or (active.id if active else None)
    if args.character:
        character_id = args.character
    elif active:
        participants = active.frontmatter.get("participants") or []
        character_id = next(
            (str(item.get("character")) for item in participants if isinstance(item, dict) and item.get("point_of_view")),
            str(participants[0].get("character")) if participants else None,
        )
    else:
        character_id = world.by_kind("character")[0].id
    conversation_id = args.conversation or next(
        (
            str(value)
            for record in world.by_kind("conversation")
            for value in [record.id]
            if record.status in {"active", "closed"}
        ),
        None,
    )

    cache = repository.root / ".wedl"
    # True cold compile: remove all derived state and both content-addressed caches.
    shutil.rmtree(cache, ignore_errors=True)
    cold_start = time.perf_counter()
    cold_report = compile_world(repository, force=True)
    cold_wall = (time.perf_counter() - cold_start) * 1000.0

    warm_start = time.perf_counter()
    warm_report = compile_world(repository, force=True)
    warm_wall = (time.perf_counter() - warm_start) * 1000.0

    exact_start = time.perf_counter()
    exact_report = compile_world(repository)
    exact_wall = (time.perf_counter() - exact_start) * 1000.0

    validation_stats, validation_value = measure(
        lambda: validate_world(repository.load_world("HEAD")), args.repeats
    )
    author_search, search_author_value = measure(
        lambda: search_world(repository, args.query, perspective="author", mode="hybrid", limit=12),
        args.repeats,
    )
    character_search: dict[str, float] | None = None
    search_character_value: Any = None
    character_context: dict[str, float] | None = None
    context_character_value: Any = None
    author_context: dict[str, float] | None = None
    context_author_value: Any = None
    if character_id and scene_id:
        character_search, search_character_value = measure(
            lambda: search_world(
                repository,
                args.query,
                perspective="character",
                character_id=character_id,
                scene_id=scene_id,
                mode="hybrid",
                limit=12,
            ),
            args.repeats,
        )
        character_context, context_character_value = measure(
            lambda: build_context(
                repository,
                character_id=character_id,
                scene_id=scene_id,
                perspective="character",
                query=args.query,
                max_characters=8000,
                max_items=24,
            ),
            args.repeats,
        )
        author_context, context_author_value = measure(
            lambda: build_context(
                repository,
                character_id=character_id,
                scene_id=scene_id,
                perspective="author",
                query=args.query,
                max_characters=8000,
                max_items=24,
            ),
            args.repeats,
        )
    conversation_stats: dict[str, float] | None = None
    conversation_value: Any = None
    if conversation_id:
        conversation_stats, conversation_value = measure(
            lambda: conversation_view(repository, conversation_id, perspective="author"),
            args.repeats,
        )
    story_stats, story_value = measure(
        lambda: story_points(repository, scene_id=scene_id), args.repeats
    )

    output = {
        "repository": str(repository.root),
        "revision": repository.head(),
        "records": len(world.records),
        "kinds": {kind: len(world.by_kind(kind)) for kind in sorted({record.kind for record in world.records.values()})},
        "characterId": character_id,
        "sceneId": scene_id,
        "conversationId": conversation_id,
        "compile": {
            "coldWallMs": round(cold_wall, 3),
            "cold": cold_report,
            "warmForcedWallMs": round(warm_wall, 3),
            "warmForced": warm_report,
            "exactCacheWallMs": round(exact_wall, 3),
            "exactCache": exact_report,
        },
        "operations": {
            "validate": validation_stats,
            "authorSearch": author_search,
            "characterSearch": character_search,
            "characterContext": character_context,
            "authorContext": author_context,
            "conversationAuthorView": conversation_stats,
            "storyPoints": story_stats,
        },
        "resultSizes": {
            "validationDiagnostics": len(validation_value),
            "authorSearchResults": len(search_author_value.get("results", [])),
            "characterSearchResults": len((search_character_value or {}).get("results", [])),
            "characterContextCharacters": len(json.dumps(context_character_value, ensure_ascii=False, separators=(",", ":"))) if context_character_value else None,
            "authorContextCharacters": len(json.dumps(context_author_value, ensure_ascii=False, separators=(",", ":"))) if context_author_value else None,
            "conversationTurns": len((conversation_value or {}).get("verbatimTurns", [])),
            "storyPoints": len(story_value.get("storyPoints", [])),
        },
    }
    rendered = json.dumps(output, ensure_ascii=False, indent=2) + "\n"
    if args.output:
        args.output.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
