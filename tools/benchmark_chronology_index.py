"""Production-path chronology-index benchmark.

Run uv run python tools/benchmark_chronology_index.py for the checked 500-year
corpus, or add --smoke for the small CI path.  The runner uses a temporary
non-Git WEDL source root and never touches this checkout's story tree, cache,
or Git metadata.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import platform
import sqlite3
import sys
from pathlib import Path
from tempfile import TemporaryDirectory
from time import perf_counter
from typing import Any

from wedl.chronology import CivilDate
from wedl.chronology_index import build_chronology_projection
from wedl.chronology_query import (
    AnnotationQuery, ChronologyPredicate, ConversionRequest,
    SQLiteChronologyStore, SourceChronologyStore, convert_chronology_date,
    query_annotations,
)
from wedl.compiler import cache_readiness, compile_world, connect, require_database
from wedl.ids import id_from_seed
from wedl.repository import Repository
from wedl.source import serialize_record
from wedl.util import canonical_json, sha256_bytes
from wedl.validation import validate_world


FULL_YEARS = (-249, 250)
FULL_CLAIMS = 10_000
SHARED_A = "calendar_0123456789abcdefghjkmnpqrs"
SHARED_B = "calendar_1123456789abcdefghjkmnpqrs"
ISOLATED = "calendar_2123456789abcdefghjkmnpqrs"
ERA = "era_0123456789abcdefghjkmnpqrs"


def _digest_files(paths: tuple[Path, ...]) -> str:
    digest = hashlib.sha256()
    for path in paths:
        digest.update(path.name.encode("utf-8"))
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\n")
    return digest.hexdigest()


def _calendar(identifier: str, label: str, epoch: int | None) -> dict[str, Any]:
    return {
        "id": identifier, "label": label,
        "months": [{"number": 1, "days": 30, "label": "Month"}],
        "rule": {"kind": "cycle", "period": 1, "overrides": []},
        "epoch": None if epoch is None else {
            "civil": {"year": 0, "month": 1, "day": 1}, "axis_day": epoch,
        },
    }


def _world_record() -> dict[str, Any]:
    return {
        "schema": "wedl/v0.6", "id": id_from_seed("world", "chronology-benchmark"),
        "kind": "world", "status": "canonical", "title": "Chronology benchmark",
        "domain": "benchmark", "tags": [], "aliases": [], "threads": [],
        "timelines": [{"id": "main", "label": "Main"}],
        "chronology": {
            "calendars": [
                _calendar(SHARED_A, "Shared A", 0),
                _calendar(SHARED_B, "Shared B", 0),
                _calendar(ISOLATED, "Isolated", None),
            ],
            "eras": [{
                "id": ERA, "calendar_id": SHARED_A, "label": "Common",
                "aliases": [], "display_year_zero": True,
                "display_epoch": {"display_year": 0, "machine_year": 0},
                "provenance": ["benchmark"],
            }],
            "anchors": [{
                "id": "chronology_0123456789abcdefghjkmnpqrs", "axis_day": 0,
                "story_time": {"timeline": "main", "tick": 0, "order": 0},
                "provenance": ["benchmark"],
            }],
        },
    }


def _claim_record(number: int, year: int) -> dict[str, Any]:
    # Most claims share one axis so the benchmark exercises selective indexed
    # queries; the deterministic remainder proves a second shared calendar and
    # an isolated local basis travel through the same production projection.
    calendar = SHARED_B if number % 20 == 0 else ISOLATED if number % 20 == 1 else SHARED_A
    return {
        "schema": "wedl/v0.6", "id": id_from_seed("character", f"chronology-benchmark-{number}"),
        "kind": "character", "status": "canonical", "title": f"Claim {number:05d}",
        "domain": "benchmark", "tags": [], "aliases": [],
        "chronology": [{
            "id": id_from_seed("chronology", f"chronology-benchmark-{number}"),
            "provenance": ["benchmark"],
            "value": {"civil": {"calendar_id": calendar, "year": year, "month": 1, "day": number % 30 + 1}},
        }],
    }


def build_corpus(root: Path, *, claims: int, years: tuple[int, int]) -> dict[str, Any]:
    """Write one world and the requested source records through production serialization."""
    source = root / "story"
    source.mkdir(parents=True)
    (source / "world.md").write_bytes(serialize_record(_world_record(), "Production benchmark world."))
    span = years[1] - years[0] + 1
    assert claims % span == 0, "claims must divide evenly across the signed-year span"
    for number in range(claims):
        year = years[0] + number // (claims // span)
        record = _claim_record(number, year)
        path = source / "characters" / f"claim-{number:05d}.md"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(serialize_record(record, "Production benchmark claim."))
    config = {"years": list(years), "claims": claims, "calendars": [SHARED_A, SHARED_B, ISOLATED], "recordsPerYear": claims // span}
    return {"generatorConfig": config, "generatorConfigSha256": sha256_bytes(canonical_json(config).encode("utf-8"))}


def _timed(callable_: Any) -> tuple[Any, float]:
    started = perf_counter()
    return callable_(), perf_counter() - started


def _query_pair(source: SourceChronologyStore, sqlite: SQLiteChronologyStore, request: AnnotationQuery) -> dict[str, Any]:
    left, source_seconds = _timed(lambda: query_annotations(source, request))
    right, sqlite_seconds = _timed(lambda: query_annotations(sqlite, request))
    if left != right:
        raise AssertionError("source and SQLite chronology query outcomes diverged")
    stats = dict(sqlite.last_candidate_stats)
    if not any("chronology_annotation_basis" in item for item in stats.get("queryPlan", ())):
        raise AssertionError(f"selective chronology query did not use a chronology index: {stats.get('queryPlan')}")
    return {
        "sourceSeconds": source_seconds, "sqliteSeconds": sqlite_seconds,
        "equal": True, "matches": len(left.value.matches), "candidateStats": stats,
    }


def run_benchmark(*, smoke: bool = False) -> dict[str, Any]:
    """Execute the production path and return machine-readable measured evidence."""
    claims, years = (100, (-4, 5)) if smoke else (FULL_CLAIMS, FULL_YEARS)
    repository_root = Path(__file__).resolve().parents[1]
    implementation = _digest_files((
        repository_root / "src" / "wedl" / "compiler.py",
        repository_root / "src" / "wedl" / "chronology_index.py",
        repository_root / "src" / "wedl" / "chronology_query.py",
        Path(__file__).resolve(),
    ))
    with TemporaryDirectory(prefix="wedl-chronology-benchmark-", ignore_cleanup_errors=True) as temporary:
        root = Path(temporary)
        corpus = build_corpus(root, claims=claims, years=years)
        repository = Repository(root)
        if repository.is_git or repository.resolve("HEAD") != "WORKTREE" or repository.status():
            raise AssertionError("benchmark temporary source root must remain a clean non-Git WORKTREE")
        missing = cache_readiness(repository, "WORKTREE")
        if missing["state"] != "missing":
            raise AssertionError(f"expected missing cache, got {missing}")
        loaded, source_load_seconds = _timed(lambda: repository.load_world("WORKTREE"))
        diagnostics, validation_seconds = _timed(lambda: validate_world(loaded))
        if diagnostics:
            raise AssertionError(f"generated benchmark corpus is invalid: {diagnostics[:3]}")
        projection, projection_seconds = _timed(lambda: build_chronology_projection(loaded, validated=True))
        first, compilation_seconds = _timed(lambda: compile_world(repository, "WORKTREE", profile_name="state"))
        if first["status"] != "compiled":
            raise AssertionError(f"expected initial compile, got {first}")
        ready = cache_readiness(repository, "WORKTREE")
        hit, cache_hit_seconds = _timed(lambda: compile_world(repository, "WORKTREE"))
        if ready["state"] != "ready" or hit["status"] != "cache-hit":
            raise AssertionError(f"unexpected cache lifecycle: {ready}, {hit}")
        (_compiled_world, database), strict_seconds = _timed(
            lambda: require_database(repository, "WORKTREE", require_compiled=True)
        )
        with connect(database, True) as connection:
            source = SourceChronologyStore(projection, "WORKTREE")
            sqlite = SQLiteChronologyStore(connection, "WORKTREE")
            per_year = claims // (years[1] - years[0] + 1)
            query_number = next(
                number for number in range(-years[0] * per_year, (-years[0] + 1) * per_year)
                if number % 20 not in {0, 1}
            )
            query_value = CivilDate(SHARED_A, 0, 1, query_number % 30 + 1)
            exact = _query_pair(source, sqlite, AnnotationQuery(
                ChronologyPredicate.ON_DATE, query_value, limit=claims,
            ))
            overlap = _query_pair(source, sqlite, AnnotationQuery(
                ChronologyPredicate.OVERLAPS, query_value, limit=claims,
            ))
            broad = _query_pair(source, sqlite, AnnotationQuery(
                ChronologyPredicate.BETWEEN,
                CivilDate(SHARED_A, years[0], 1, 1),
                CivilDate(SHARED_A, years[1], 1, 30), limit=claims,
            ))
            source_conversion, source_conversion_seconds = _timed(
                lambda: convert_chronology_date(source, ConversionRequest(CivilDate(SHARED_A, 0, 1, 1), target_calendar_id=SHARED_B))
            )
            sqlite_conversion, sqlite_conversion_seconds = _timed(
                lambda: convert_chronology_date(sqlite, ConversionRequest(CivilDate(SHARED_A, 0, 1, 1), target_calendar_id=SHARED_B))
            )
            if source_conversion != sqlite_conversion or source_conversion.kind.value != "ok":
                raise AssertionError("production calendar conversion did not preserve source/SQLite parity")
        chronology = first["chronology"]
        if chronology["annotationCount"] != claims or chronology["maxInsertBatch"] < 1:
            raise AssertionError(f"unexpected production chronology inserter stats: {chronology}")
        evidence = {
            "schemaVersion": 1,
            "command": "uv run python tools/benchmark_chronology_index.py",
            "mode": "smoke" if smoke else "full",
            "corpus": {"years": list(years), "claims": claims, "records": claims + 1,
                "treeOid": repository.tree_oid("WORKTREE"), **corpus},
            "implementation": {"fingerprintSha256": implementation,
                "files": ["src/wedl/compiler.py", "src/wedl/chronology_index.py", "src/wedl/chronology_query.py", "tools/benchmark_chronology_index.py"]},
            "environment": {"python": sys.version, "sqlite": sqlite3.sqlite_version, "platform": platform.platform()},
            "cache": {"initial": missing["state"], "first": first["status"], "ready": ready["state"],
                "cacheHit": hit["status"], "cacheHitSourceLoad": hit["sourceLoad"], "strict": "ready"},
            "compilation": {"recordCount": first["recordCount"], "databaseBytes": first["databaseBytes"],
                "chronology": chronology, "sourceLoad": first["sourceLoad"]},
            "timingsSeconds": {"sourceLoad": source_load_seconds, "validation": validation_seconds,
                "projection": projection_seconds, "compile": compilation_seconds, "cacheHit": cache_hit_seconds,
                "strictRead": strict_seconds, "sourceExact": exact["sourceSeconds"], "sqliteExact": exact["sqliteSeconds"],
                "sourceOverlap": overlap["sourceSeconds"], "sqliteOverlap": overlap["sqliteSeconds"],
                "sourceBroad": broad["sourceSeconds"], "sqliteBroad": broad["sqliteSeconds"],
                "sourceConversion": source_conversion_seconds, "sqliteConversion": sqlite_conversion_seconds},
            "queries": {"input": {"calendarId": SHARED_A, "year": 0, "month": 1, "day": query_value.day},
                "exact": exact, "overlap": overlap, "broad": broad},
            "conversion": {"equal": True, "kind": source_conversion.kind.value, "formatted": source_conversion.value.formatted},
            "budgets": {"sqliteQuerySeconds": 5.0, "compileSeconds": 120.0,
                "sqliteQueryWithinBudget": exact["sqliteSeconds"] <= 5.0 and overlap["sqliteSeconds"] <= 5.0 and broad["sqliteSeconds"] <= 5.0,
                "compileWithinBudget": compilation_seconds <= 120.0},
        }
        if not evidence["budgets"]["sqliteQueryWithinBudget"] or not evidence["budgets"]["compileWithinBudget"]:
            raise AssertionError("chronology benchmark exceeded its declared regression budget")
        return evidence


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--smoke", action="store_true", help="run the small production-path CI corpus")
    parser.add_argument("--write", action="store_true", help="replace the checked full evidence JSON (not valid with --smoke)")
    args = parser.parse_args()
    if args.write and args.smoke:
        parser.error("--write requires the full benchmark")
    evidence = json.dumps(run_benchmark(smoke=args.smoke), indent=2, sort_keys=True) + "\n"
    if args.write:
        (Path(__file__).resolve().parents[1] / "docs" / "chronology-index-benchmark.json").write_text(evidence, encoding="utf-8")
    print(evidence, end="")


if __name__ == "__main__":
    main()
