from __future__ import annotations

from importlib import resources
import json
from pathlib import Path
import shutil
import statistics
import subprocess
import tempfile
import time
from typing import Any, Callable

from wedl.compiler import compile_world
from wedl.context import build_context
from wedl.query import conversation_view, search_world, story_points
from wedl.repository import Repository


def git(root: Path, *args: str) -> None:
    subprocess.run(["git", "-C", str(root), *args], check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def repository_at(root: Path) -> Repository:
    root.mkdir(parents=True, exist_ok=True)
    source = resources.files("wedl.data.frontiersmen").joinpath("story")
    with resources.as_file(source) as source_path:
        shutil.copytree(source_path, root / "story")
    (root / ".gitignore").write_text(".wedl/\n", encoding="utf-8")
    git(root, "init", "-q")
    git(root, "config", "user.name", "wedl benchmark")
    git(root, "config", "user.email", "benchmark@wedl.invalid")
    git(root, "add", "story", ".gitignore")
    git(root, "commit", "-qm", "seed Frontiersmen")
    return Repository(root)


def measure(fn: Callable[[], Any], repeats: int = 15) -> dict[str, Any]:
    values: list[float] = []
    result: Any = None
    for _ in range(repeats):
        start = time.perf_counter()
        result = fn()
        values.append((time.perf_counter() - start) * 1000.0)
    ordered = sorted(values)
    p95 = ordered[min(len(ordered) - 1, int(len(ordered) * 0.95))]
    return {
        "medianMs": round(statistics.median(values), 3),
        "meanMs": round(statistics.mean(values), 3),
        "p95Ms": round(p95, 3),
        "minimumMs": round(min(values), 3),
        "maximumMs": round(max(values), 3),
        "repeats": repeats,
        "sample": result,
    }


def main(output: Path) -> None:
    payload: dict[str, Any] = {"profiles": {}, "queries": {}, "context": {}, "conversation": {}, "storyPoints": {}}
    with tempfile.TemporaryDirectory(prefix="wedl-frontiersmen-bench-") as temporary:
        base = Path(temporary)
        repositories: dict[str, Repository] = {}
        for profile in ("state", "fts", "vector", "hybrid"):
            repository = repository_at(base / profile)
            repositories[profile] = repository
            cold_start = time.perf_counter()
            cold = compile_world(repository, force=True, profile_name=profile, vector_provider="lsa" if profile in {"vector", "hybrid"} else None)
            cold_ms = (time.perf_counter() - cold_start) * 1000.0
            warm_start = time.perf_counter()
            warm = compile_world(repository, force=True, profile_name=profile, vector_provider="lsa" if profile in {"vector", "hybrid"} else None)
            warm_ms = (time.perf_counter() - warm_start) * 1000.0
            exact_start = time.perf_counter()
            exact = compile_world(repository)
            exact_ms = (time.perf_counter() - exact_start) * 1000.0
            payload["profiles"][profile] = {
                "coldMs": round(cold_ms, 3),
                "warmForcedMs": round(warm_ms, 3),
                "exactReuseMs": round(exact_ms, 3),
                "databaseBytes": cold["databaseBytes"],
                "recordCount": cold["recordCount"],
                "searchProfile": cold.get("searchProfile"),
                "vectorModels": cold.get("vectorModels"),
                "searchProjection": cold.get("searchProjection"),
                "coldTimingsMs": cold.get("timingsMs"),
                "warmTimingsMs": warm.get("timingsMs"),
                "warmEmbeddingCache": warm.get("embeddingCache"),
                "exactStatus": exact.get("status"),
            }

        world = repositories["hybrid"].load_world()
        rhea = world.find("Rhea", "character")
        scene = world.find("The Hunt Begins", "scene")
        conversation = world.find("Running Under the Drums", "conversation")
        rootjaw = world.find("Rootjaw", "character")

        probes = {
            "exactAmber": "blood quickens amber",
            "semanticWretch": "memory of wounded animals taking shape through stone",
            "treeKing": "former guild hunter using terror to strengthen masked followers",
            "escape": "bone key reliquary rain pursuit through spruce",
        }
        for name, query in probes.items():
            payload["queries"][name] = {}
            for mode, profile in (("fts", "fts"), ("vector", "vector"), ("hybrid", "hybrid")):
                benchmark = measure(
                    lambda repository=repositories[profile], query=query, mode=mode: search_world(
                        repository,
                        query,
                        perspective="author",
                        scene_id=scene.id,
                        tick=195,
                        mode=mode,
                        limit=20,
                    ),
                    repeats=20,
                )
                sample = benchmark.pop("sample")
                benchmark["resultCount"] = len(sample["results"])
                benchmark["topResults"] = [
                    {
                        "title": item["metadata"].get("title"),
                        "documentKind": item["documentKind"],
                        "lanes": item.get("lanes"),
                        "ftsRank": item.get("ftsRank"),
                        "vectorRank": item.get("vectorRank"),
                        "vectorScore": item.get("vectorScore"),
                        "score": item.get("score"),
                    }
                    for item in sample["results"][:8]
                ]
                payload["queries"][name][mode] = benchmark

        payload["context"]["rheaHybrid5000"] = measure(
            lambda: build_context(
                repositories["hybrid"],
                character_id=rhea.id,
                scene_id=scene.id,
                query="escape the masked host through spruce with the bone key and reliquary",
                max_characters=5000,
                max_items=18,
                search_mode="hybrid",
            ),
            repeats=20,
        )
        context_sample = payload["context"]["rheaHybrid5000"].pop("sample")
        payload["context"]["rheaHybrid5000"]["serializedCharacters"] = len(json.dumps(context_sample, ensure_ascii=False, separators=(",", ":")))
        payload["context"]["rheaHybrid5000"]["selection"] = context_sample["selection"]

        payload["conversation"]["rootjawPursuit"] = measure(
            lambda: conversation_view(
                repositories["hybrid"],
                conversation.id,
                perspective="character",
                character_id=rootjaw.id,
            ),
            repeats=20,
        )
        conversation_sample = payload["conversation"]["rootjawPursuit"].pop("sample")
        payload["conversation"]["rootjawPursuit"]["heardTurns"] = len(conversation_sample["heardVerbatimTurns"])

        payload["storyPoints"]["activePursuit"] = measure(
            lambda: story_points(repositories["hybrid"], scene_id=scene.id),
            repeats=20,
        )
        story_sample = payload["storyPoints"]["activePursuit"].pop("sample")
        payload["storyPoints"]["activePursuit"]["states"] = {
            item["title"]: item["derivedState"] for item in story_sample["storyPoints"]
        }

    output.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    arguments = parser.parse_args()
    main(arguments.output)
