#!/usr/bin/env python3
"""Benchmark state/FTS/vector/hybrid compilation and retrieval independently."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import shutil
import statistics
import time
from typing import Any, Callable

from wedl.compiler import compile_world, connect
from wedl.query import search_world
from wedl.repository import Repository


def measure(function: Callable[[], Any], repeats: int) -> tuple[dict[str, float], Any]:
    samples: list[float] = []
    value: Any = None
    for _ in range(repeats):
        started = time.perf_counter()
        value = function()
        samples.append((time.perf_counter() - started) * 1000.0)
    ordered = sorted(samples)
    return {
        "repeats": repeats,
        "minimumMs": round(ordered[0], 3),
        "medianMs": round(statistics.median(ordered), 3),
        "p95Ms": round(ordered[min(len(ordered) - 1, int(len(ordered) * 0.95))], 3),
        "maximumMs": round(ordered[-1], 3),
    }, value


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", required=True, type=Path)
    parser.add_argument("--query", default="legal authenticity coercion")
    parser.add_argument("--repeats", type=int, default=9)
    parser.add_argument("--output", type=Path)
    parser.add_argument(
        "--profiles",
        nargs="+",
        default=["state", "fts", "vector", "hybrid"],
        choices=["state", "fts", "vector", "hybrid"],
    )
    args = parser.parse_args()
    repository = Repository(args.repo)
    output: dict[str, Any] = {
        "repository": str(repository.root),
        "revision": repository.head(),
        "query": args.query,
        "profiles": {},
    }
    for profile in args.profiles:
        cache = repository.root / ".wedl"
        (cache / "world.sqlite").unlink(missing_ok=True)
        shutil.rmtree(cache / "revisions", ignore_errors=True)
        if profile in {"vector", "hybrid"}:
            (cache / "vector-cache-v2.sqlite").unlink(missing_ok=True)
            (cache / "vector-cache-v2.sqlite-wal").unlink(missing_ok=True)
            (cache / "vector-cache-v2.sqlite-shm").unlink(missing_ok=True)
        started = time.perf_counter()
        cold = compile_world(repository, force=True, profile_name=profile)
        cold_wall = (time.perf_counter() - started) * 1000.0
        started = time.perf_counter()
        warm = compile_world(repository, force=True, profile_name=profile)
        warm_wall = (time.perf_counter() - started) * 1000.0
        started = time.perf_counter()
        exact = compile_world(repository, profile_name=profile)
        exact_wall = (time.perf_counter() - started) * 1000.0
        searches: dict[str, Any] = {}
        if profile in {"fts", "hybrid"}:
            searches["fts"], fts_result = measure(
                lambda: search_world(repository, args.query, mode="fts", limit=20),
                args.repeats,
            )
            searches["fts"]["results"] = len(fts_result["results"])
        if profile in {"vector", "hybrid"}:
            searches["vector"], vector_result = measure(
                lambda: search_world(repository, args.query, mode="vector", limit=20),
                args.repeats,
            )
            searches["vector"]["results"] = len(vector_result["results"])
        if profile == "hybrid":
            searches["hybrid"], hybrid_result = measure(
                lambda: search_world(repository, args.query, mode="hybrid", limit=20),
                args.repeats,
            )
            searches["hybrid"]["results"] = len(hybrid_result["results"])
            searches["hybrid"]["bothLanes"] = sum(
                set(item.get("lanes") or []) == {"fts", "vector"}
                for item in hybrid_result["results"]
            )
            searches["hybrid"]["vectorOnly"] = sum(
                item.get("lanes") == ["vector"]
                for item in hybrid_result["results"]
            )
        database = cache / "world.sqlite"
        with connect(database, True) as connection:
            counts = {
                "searchDocuments": connection.execute("SELECT COUNT(*) FROM search_document").fetchone()[0],
                "ftsRows": connection.execute("SELECT COUNT(*) FROM search_fts").fetchone()[0],
                "uniqueVectors": connection.execute("SELECT COUNT(*) FROM vector_embedding").fetchone()[0],
                "vectorLinks": connection.execute("SELECT COUNT(*) FROM document_vector").fetchone()[0],
            }
        output["profiles"][profile] = {
            "coldWallMs": round(cold_wall, 3),
            "warmForcedWallMs": round(warm_wall, 3),
            "exactWallMs": round(exact_wall, 3),
            "cold": cold,
            "warm": warm,
            "exact": exact,
            "databaseBytes": database.stat().st_size,
            "counts": counts,
            "search": searches,
        }
    rendered = json.dumps(output, ensure_ascii=False, indent=2) + "\n"
    if args.output:
        args.output.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
