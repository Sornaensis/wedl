"""Measure the full authored generational fixture through strict compiled reads."""
from __future__ import annotations

import argparse
from contextlib import closing
import ctypes
import cProfile
import hashlib
import io
import json
from pathlib import Path
import platform
import pstats
import sqlite3
import statistics
import subprocess
import sys
import tempfile
import threading
import time

from generate_generational_fixture import build_fixture, source_digest
from wedl.compiler import (INDEX_DDL, _bootstrap_compiled_connection,
                           _insert_entities, cache_readiness, compile_world)
from wedl.generational_context import build_generational_context
from wedl.generational_index import insert_generational_index
from wedl.generational_query import TrustedViewerScope, query_connection, query_generational
from wedl.model import StoryTime
from wedl.repository import Repository
from wedl.source import serialize_record
from wedl.util import canonical_json
from wedl.validation import validate_world


ROOT = Path(__file__).resolve().parents[1]


def _git(root: Path, *args: str) -> str:
    return subprocess.run(["git", "-C", str(root), *args], check=True,
                          capture_output=True, text=True).stdout.strip()


def _scope(revision: str, tick: int, order: int = 0, *,
           mode: str = "author-as-of", private: bool = True,
           character: str | None = None) -> TrustedViewerScope:
    return TrustedViewerScope(revision, mode, "main",
        None if mode == "author-all-time" else StoryTime("main", tick, order),
        frozenset({"public", "council", "archivist"} if private else {"public"}),
        frozenset({"ordinary", "council-ledger", "archive"} if private else {"ordinary"}),
        frozenset({"generational-core-v1"}), character)


def _prepare_repository(root: Path, world) -> Repository:
    # Clone the verified project instead of creating a new Git repository.
    subprocess.run(["git", "clone", "--shared", "--quiet", str(ROOT), str(root)],
                   check=True, capture_output=True)
    _git(root, "config", "user.name", "wedl benchmark")
    _git(root, "config", "user.email", "wedl-benchmark@test.invalid")
    _git(root, "rm", "-r", "-q", "--ignore-unmatch", "story")
    for record in sorted(world.records.values(), key=lambda value: value.source_path):
        path = root / record.source_path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(serialize_record(record.frontmatter, f"# {record.title}\n"))
    (root / ".gitignore").write_text(".wedl/\n", encoding="utf-8")
    _git(root, "add", "story", ".gitignore")
    _git(root, "commit", "-qm", "seed authored generational benchmark")
    return Repository(root)


def _sample_memory(stop: threading.Event, readings: list[int]) -> None:
    if sys.platform == "win32":
        class MemoryCounters(ctypes.Structure):
            _fields_ = [("cb", ctypes.c_ulong), ("pageFaultCount", ctypes.c_ulong),
                        ("peakWorkingSetSize", ctypes.c_size_t),
                        ("workingSetSize", ctypes.c_size_t),
                        ("quotaPeakPagedPoolUsage", ctypes.c_size_t),
                        ("quotaPagedPoolUsage", ctypes.c_size_t),
                        ("quotaPeakNonPagedPoolUsage", ctypes.c_size_t),
                        ("quotaNonPagedPoolUsage", ctypes.c_size_t),
                        ("pagefileUsage", ctypes.c_size_t),
                        ("peakPagefileUsage", ctypes.c_size_t)]
        counters = MemoryCounters()
        counters.cb = ctypes.sizeof(counters)
        current_process = ctypes.windll.kernel32.GetCurrentProcess
        current_process.restype = ctypes.c_void_p
        handle = current_process()
        read = ctypes.windll.psapi.GetProcessMemoryInfo
        read.argtypes = [ctypes.c_void_p, ctypes.POINTER(MemoryCounters), ctypes.c_ulong]
        read.restype = ctypes.c_int
        while True:
            if read(handle, ctypes.byref(counters), counters.cb):
                readings.append(int(counters.workingSetSize))
            if stop.wait(0.05):
                return
    try:
        import psutil
        process = psutil.Process()
        while True:
            readings.append(process.memory_info().rss)
            if stop.wait(0.05):
                return
    except ImportError:
        pass


def _stats(samples: list[float]) -> dict[str, float]:
    ordered = sorted(samples)
    rank = (len(ordered) - 1) * 0.95
    lower = int(rank)
    upper = min(lower + 1, len(ordered) - 1)
    p95 = ordered[lower] + (ordered[upper] - ordered[lower]) * (rank - lower)
    return {"median": round(statistics.median(samples), 3),
            "p95": round(p95, 3), "worst": round(ordered[-1], 3),
            "samples": [round(sample, 3) for sample in samples]}


def _measured(repository: Repository, scope: TrustedViewerScope,
              request: dict, repeats: int) -> tuple[dict, dict]:
    query_generational(repository, scope, request, require_compiled=True)
    samples = []
    result = {}
    for _ in range(repeats):
        start = time.perf_counter()
        result = query_generational(repository, scope, request, require_compiled=True)
        samples.append((time.perf_counter() - start) * 1000)
    return result, _stats(samples)


def benchmark(*, work_dir: Path, repeats: int = 5) -> dict:
    started = time.perf_counter()
    stop = threading.Event()
    readings: list[int] = []
    watcher = threading.Thread(target=_sample_memory, args=(stop, readings), daemon=True)
    watcher.start()
    try:
        build_started = time.perf_counter()
        world, manifest = build_fixture()
        first_digest = source_digest(world)
        independent, independent_manifest = build_fixture()
        second_digest = source_digest(independent)
        if first_digest != second_digest or manifest != independent_manifest:
            raise AssertionError("independent fixture generations differ")
        del independent
        build_ms = (time.perf_counter() - build_started) * 1000
        diagnostics = validate_world(world)
        if diagnostics:
            raise AssertionError(f"authored source invalid: {diagnostics[:5]}")
        if not (manifest["counts"]["characters"] >= 5000
                and manifest["counts"]["generations"] >= 100
                and manifest["counts"]["kinshipEdges"] >= 10000
                and manifest["counts"]["transitions"] >= 500):
            raise AssertionError("fixture scale dimensions missed")

        repository = _prepare_repository(work_dir / "repository", world)
        revision = repository.head()
        before_source = source_digest(world)
        compile_started = time.perf_counter()
        compiled = compile_world(repository, revision)
        cold_ms = (time.perf_counter() - compile_started) * 1000
        if compiled["status"] != "compiled" or compiled["recordCount"] != len(world.records):
            raise AssertionError("cold compilation did not include the full fixture")
        if source_digest(world) != before_source:
            raise AssertionError("compilation changed canonical source")
        database = Path(compiled["database"])
        ready = cache_readiness(repository, revision)
        if ready["state"] != "ready":
            raise AssertionError(f"strict cache not ready: {ready}")

        # Project the same authored source independently of the repository
        # compiler and compare exact folded answers and citation paths.
        source = sqlite3.connect(":memory:")
        source.row_factory = sqlite3.Row
        try:
            _bootstrap_compiled_connection(source)
            _insert_entities(source, world)
            source.executescript(INDEX_DDL)
            insert_generational_index(source, world, StoryTime("main", 0, 0))
            cases = {
                "lineageEarly": (_scope(revision, -99), {"operation": "ancestors",
                    "subject_id": manifest["answers"]["boundedLineage"], "depth": 2, "items": 12}),
                "lineageLate": (_scope(revision, 2), {"operation": "ancestors",
                    "subject_id": manifest["answers"]["boundedLineage"], "depth": 2, "items": 12}),
                "rosterEarly": (_scope(revision, -100), {"operation": "organization",
                    "subject_id": manifest["answers"]["dynasty"], "depth": 2, "items": 24}),
                "rosterLate": (_scope(revision, 0), {"operation": "organization",
                    "subject_id": manifest["answers"]["dynasty"], "depth": 2, "items": 24}),
                "rosterAllTime": (_scope(revision, 0, mode="author-all-time"),
                    {"operation": "organization", "subject_id": manifest["answers"]["dynasty"],
                     "depth": 2, "items": 24}),
                "holderEarly": (_scope(revision, -91), {"operation": "legacy",
                    "subject_id": manifest["answers"]["legacy"], "items": 100}),
                "holderVacancy": (_scope(revision, -89), {"operation": "legacy",
                    "subject_id": manifest["answers"]["legacy"], "items": 100}),
                "holderLate": (_scope(revision, 0), {"operation": "legacy",
                    "subject_id": manifest["answers"]["legacy"], "items": 100}),
                "holderAllTime": (_scope(revision, 0, mode="author-all-time"),
                    {"operation": "legacy", "subject_id": manifest["answers"]["legacy"],
                     "items": 100}),
                "futureBefore": (_scope(revision, 0), {"operation": "parents",
                    "subject_id": manifest["answers"]["futureChild"], "items": 4}),
                "futureAfter": (_scope(revision, 6), {"operation": "parents",
                    "subject_id": manifest["answers"]["futureChild"], "items": 4}),
                "secretAuthor": (_scope(revision, 0), {"operation": "parents",
                    "subject_id": manifest["answers"]["secretChild"], "items": 4}),
                "secretPublic": (_scope(revision, 0, private=False), {"operation": "parents",
                    "subject_id": manifest["answers"]["secretChild"], "items": 4}),
                "secretCharacter": (_scope(revision, 0, mode="character",
                    character=manifest["answers"]["secretChild"]),
                    {"operation": "parents", "subject_id": manifest["answers"]["secretChild"]}),
                "mixedParentage": (_scope(revision, 0), {"operation": "parents",
                    "subject_id": manifest["answers"]["biologicalChild"], "items": 4}),
                "biologicalDescendants": (_scope(revision, 0), {"operation": "descendants",
                    "subject_id": manifest["answers"]["biologicalParent"],
                    "depth": 1, "items": 4}),
                "adoptiveDescendants": (_scope(revision, 0), {"operation": "descendants",
                    "subject_id": manifest["answers"]["adoptiveParent"],
                    "depth": 1, "items": 4}),
                "sameTickBefore": (_scope(revision, -10, 0), {"operation": "parents",
                    "subject_id": manifest["answers"]["biologicalChild"], "items": 4}),
                "sameTickAfter": (_scope(revision, -10, 1), {"operation": "parents",
                    "subject_id": manifest["answers"]["biologicalChild"], "items": 4}),
            }
            answers = {}
            for name, (scope, request) in cases.items():
                expected = query_connection(source, scope, request)
                actual = query_generational(repository, scope, request,
                                            require_compiled=True)
                if actual != expected:
                    raise AssertionError(f"source/strict compiled result differs: {name}")
                answers[name] = actual
        finally:
            source.close()

        privacy_passed = (answers["secretAuthor"]["state"] == "available"
                          and answers["secretPublic"] == answers["secretCharacter"]
                          == {"state": "unknown"})
        horizon_passed = (answers["futureBefore"]["state"] == "unknown"
                          and answers["futureAfter"]["state"] == "available")
        if (answers["lineageLate"]["state"] != "available"
                or {row["targetId"] for row in answers["mixedParentage"]["relations"]}
                != {manifest["answers"]["biologicalParent"],
                    manifest["answers"]["adoptiveParent"]}
                or len(answers["holderLate"]["holders"]) != 2
                or {row["value"]["basis"] for row in answers["holderLate"]["holders"]}
                != {"legal", "de-facto"}
                or len(answers["holderLate"]["succession"]) < 3):
            raise AssertionError("published known-answer lineage or tenure case failed")
        for term in ("sealed", "future"):
            public = query_generational(repository, _scope(revision, 0, private=False),
                {"operation": "search", "text": term, "items": 2}, require_compiled=True)
            privacy_passed &= public == {"state": "available", "results": [], "cursor": None}
        context = build_generational_context(repository, _scope(revision, 0, private=False),
            manifest["answers"]["secretChild"], max_characters=4096, max_items=10,
            max_depth=2, require_compiled=True)
        privacy_passed &= "Sealed lineage" not in canonical_json(context)
        answer_sha = hashlib.sha256(canonical_json(answers).encode()).hexdigest()

        timings = {}
        measured_answers = {}
        for name in ("lineageEarly", "lineageLate", "rosterEarly", "rosterLate",
                     "holderEarly", "holderLate"):
            scope, request = cases[name]
            result, stats = _measured(repository, scope, request, repeats)
            if result != answers[name]:
                raise AssertionError(f"warm read changed answer: {name}")
            timings[name] = stats
            measured_answers[name] = result["state"]
        warm_ms = max(value["p95"] for value in timings.values())
        profile = None
        if warm_ms > 250:
            slowest = max(timings, key=lambda name: timings[name]["p95"])
            profiler = cProfile.Profile()
            profiler.enable()
            query_generational(repository, *cases[slowest], require_compiled=True)
            profiler.disable()
            stream = io.StringIO()
            pstats.Stats(profiler, stream=stream).sort_stats("cumtime").print_stats(12)
            profile = {"operation": slowest, "topCumulative": stream.getvalue()}

        with closing(sqlite3.connect(database)) as connection:
            compiled_counts = dict(connection.execute(
                "SELECT kind,COUNT(*) FROM entity GROUP BY kind").fetchall())
            private_fts = connection.execute("SELECT COUNT(*) FROM search_document WHERE "
                "entity_id IN (SELECT id FROM generational_record)").fetchone()[0]
        if private_fts:
            raise AssertionError("generational evidence entered generic search documents")
        with closing(sqlite3.connect(database)) as damaged:
            damaged.execute("DROP INDEX generational_search_lookup_idx")
            damaged.commit()
        incompatible = cache_readiness(repository, revision)
        if incompatible["state"] != "incompatible":
            raise AssertionError("damaged cache was not rejected")
        repaired_answer = query_generational(repository, *cases["lineageLate"],
                                              require_compiled=False)
        rebuilt = cache_readiness(repository, revision)
        rebuild_passed = (repaired_answer == answers["lineageLate"]
                          and rebuilt["state"] == "ready"
                          and repository.head() == revision
                          and not _git(repository.root, "status", "--porcelain"))
        if not rebuild_passed:
            raise AssertionError("disposable cache did not rebuild without source writes")
        return {"kind": "authored-generational", "counts": manifest["counts"],
                "recordKinds": manifest["recordKinds"], "compiledRecordKinds": compiled_counts,
                "sourceSha256": first_digest, "independentSourceSha256": second_digest,
                "answerSha256": answer_sha, "knownAnswers": manifest["answers"],
                "privacyPassed": bool(privacy_passed), "horizonPassed": bool(horizon_passed),
                "sourceCompiledParity": True, "cacheState": ready["state"],
                "cacheRebuildPassed": rebuild_passed,
                "cacheDamageReason": incompatible.get("reason"),
                "warmQueryMs": warm_ms, "warmStatistic": "maximum operation p95",
                "warmTargetMs": 250, "warmTargetPassed": warm_ms <= 250,
                "warmOperations": timings, "warmStates": measured_answers,
                "coldCompileMs": round(cold_ms, 3), "coldStagesMs": compiled["timingsMs"],
                "generationAndDigestMs": round(build_ms, 3),
                "sqliteBytes": database.stat().st_size,
                "peakProcessRssBytes": max(readings) if readings else None,
                "contextCharacters": len(canonical_json(context)),
                "contextTruncated": context.get("truncated", False),
                "profile": {"platform": platform.platform(), "python": sys.version.split()[0],
                            "sqlite": sqlite3.sqlite_version,
                            "searchProfile": compiled["searchProfile"],
                            "repeats": repeats, "warmupPerOperation": 1,
                            "bounds": {"lineageDepth": 2, "lineageItems": 12,
                                       "rosterItems": 24, "holderItems": 100}},
                "bottleneckProfile": profile,
                "wallMs": round((time.perf_counter() - started) * 1000, 3)}
    finally:
        stop.set()
        watcher.join(timeout=1)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--work-dir", type=Path)
    parser.add_argument("--repeats", type=int, default=5)
    args = parser.parse_args()
    if args.repeats < 3:
        parser.error("at least three repetitions are required")
    if args.work_dir is not None:
        args.work_dir.mkdir(parents=True, exist_ok=True)
        if any(args.work_dir.iterdir()):
            parser.error("work directory must be empty")
        result = benchmark(work_dir=args.work_dir, repeats=args.repeats)
    else:
        with tempfile.TemporaryDirectory(prefix="wedl-generational-benchmark-") as temp:
            result = benchmark(work_dir=Path(temp), repeats=args.repeats)
    output = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(output, encoding="utf-8")
    print(output, end="")


if __name__ == "__main__":
    main()
