"""Production compiler/cache integration for the internal chronology index.

The fixture deliberately is a bare source root.  It verifies the supported
non-Git WORKTREE path rather than relying on an enclosing checkout or creating
a repository in the test.
"""
from __future__ import annotations

from contextlib import closing
import importlib.util
import json
from pathlib import Path

import pytest

from wedl.chronology import CivilDate
from wedl.chronology_index import build_chronology_projection, load_chronology_projection
from wedl.chronology_query import (
    AnnotationQuery, ChronologyPredicate, ConversionRequest, ChronologyReason,
    SQLiteChronologyStore, SourceChronologyStore, convert_chronology_date,
    query_annotations,
)
from wedl.compiler import cache_readiness, compile_world, connect, require_database
from wedl.errors import CompileRequired
from wedl.ids import id_from_seed
from wedl.repository import Repository
from wedl.source import serialize_record, split_envelope
from wedl.validation import validate_world


_BENCHMARK_SPEC = importlib.util.spec_from_file_location(
    "chronology_compile_benchmark", Path(__file__).parents[1] / "tools" / "benchmark_chronology_index.py"
)
assert _BENCHMARK_SPEC and _BENCHMARK_SPEC.loader
_BENCHMARK = importlib.util.module_from_spec(_BENCHMARK_SPEC)
_BENCHMARK_SPEC.loader.exec_module(_BENCHMARK)


def _write_legacy(root: Path, schema: str) -> None:
    story = root / "story"
    story.mkdir(parents=True)
    world_id = id_from_seed("world", f"legacy-{schema}")
    character_id = id_from_seed("character", f"legacy-{schema}")
    world = {
        "schema": schema, "id": world_id, "kind": "world", "status": "canonical",
        "title": "Legacy chronology sentinel", "domain": "world", "tags": [], "aliases": [],
        "default_timeline": "main", "timelines": [{"id": "main", "label": "Main"}],
        "state_keys": {}, "relationship_metrics": {},
        "embedding_policy": {"provider": "lsa", "model": "wedl-lsa-v1", "dimensions": 8, "max_features": 64},
        "compilation_policy": {"default_profile": "state"},
    }
    character = {
        "schema": schema, "id": character_id, "kind": "character", "status": "canonical",
        "title": "Legacy witness", "domain": "cast", "tags": [], "aliases": [],
    }
    if schema == "wedl/v0.5":
        world["threads"] = []
        character["threads"] = []
    (story / "world.md").write_bytes(serialize_record(world, "Legacy world."))
    (story / "character.md").write_bytes(serialize_record(character, "Legacy character."))


@pytest.mark.parametrize("schema", ("wedl/v0.3", "wedl/v0.5"))
def test_legacy_source_and_compiled_stores_share_no_chronology_sentinel(tmp_path: Path, schema: str) -> None:
    _write_legacy(tmp_path, schema)
    repository = Repository(tmp_path)
    assert repository.is_git is False and repository.resolve("HEAD") == "WORKTREE"
    world = repository.load_world("WORKTREE")
    assert validate_world(world) == []
    source = SourceChronologyStore(build_chronology_projection(world, validated=True), "WORKTREE")
    assert cache_readiness(repository, "WORKTREE")["state"] == "missing"
    report = compile_world(repository, "WORKTREE", profile_name="state")
    assert report["status"] == "compiled"
    with closing(connect(repository.root / ".wedl" / "world.sqlite", True)) as connection:
        sqlite = SQLiteChronologyStore(connection, "WORKTREE")
        request = AnnotationQuery(ChronologyPredicate.ON_DATE, CivilDate("calendar_unknown", 0, 1, 1))
        assert query_annotations(source, request) == query_annotations(sqlite, request)
        assert query_annotations(sqlite, request).reason is ChronologyReason.NO_CHRONOLOGY


def test_non_git_v06_compile_stale_strict_rebuild_cache_hit_and_store_lifetime(tmp_path: Path) -> None:
    _BENCHMARK.build_corpus(tmp_path, claims=100, years=(-4, 5))
    repository = Repository(tmp_path)
    assert repository.is_git is False and repository.resolve("HEAD") == "WORKTREE" and repository.status() == []
    world = repository.load_world("WORKTREE")
    assert validate_world(world) == []
    source = SourceChronologyStore(build_chronology_projection(world, validated=True), "WORKTREE")
    assert cache_readiness(repository, "WORKTREE")["state"] == "missing"
    with pytest.raises(CompileRequired):
        require_database(repository, "WORKTREE", require_compiled=True)
    first = compile_world(repository, "WORKTREE", profile_name="state")
    assert first["status"] == "compiled" and first["chronology"]["annotationCount"] == 100
    assert cache_readiness(repository, "WORKTREE")["state"] == "ready"
    with closing(connect(repository.root / ".wedl" / "world.sqlite", True)) as connection:
        sqlite = SQLiteChronologyStore(connection, "WORKTREE")
        request = AnnotationQuery(ChronologyPredicate.OVERLAPS, CivilDate(_BENCHMARK.SHARED_A, 0, 1, 3), limit=100)
        assert query_annotations(source, request) == query_annotations(sqlite, request)
        conversion = ConversionRequest(CivilDate(_BENCHMARK.SHARED_A, 0, 1, 1), target_calendar_id=_BENCHMARK.SHARED_B)
        assert convert_chronology_date(source, conversion) == convert_chronology_date(sqlite, conversion)
    changed = tmp_path / "story" / "characters" / "claim-00000.md"
    changed.write_bytes(changed.read_bytes() + b"\n")
    assert cache_readiness(repository, "WORKTREE")["state"] == "stale"
    with pytest.raises(CompileRequired):
        require_database(repository, "WORKTREE", require_compiled=True)
    rebuilt_world, database = require_database(repository, "WORKTREE")
    assert database.exists() and len(rebuilt_world.records) == 101
    assert cache_readiness(repository, "WORKTREE")["state"] == "ready"
    assert compile_world(repository, "WORKTREE")["status"] == "cache-hit"


def test_approximation_bound_extensions_are_opaque_to_projection_and_compilation(tmp_path: Path) -> None:
    """Extensions on approximate bounds are stored verbatim, never traversed."""
    _BENCHMARK.build_corpus(tmp_path, claims=1, years=(0, 0))
    source_path = tmp_path / "story" / "characters" / "claim-00000.md"
    frontmatter, body = split_envelope(source_path.read_bytes(), str(source_path))
    qualitative_id = id_from_seed("chronology", "opaque-qualitative-approximation")
    bounded_id = id_from_seed("chronology", "opaque-bounded-approximation")
    opaque = {
        "temporaryId": "keep", "calendar_id": "$calendar.keep", "snake_key": 7,
        "items": [{"era_id": "$era.keep"}, 8],
    }
    qualitative = {
        "id": qualitative_id, "provenance": ["test"],
        "value": {"approx": {
            "display_value": "unknown", "bounds": {
                "lower": None, "upper": None, "x-meta": opaque,
            }, "x-value": {"calendar_id": "$calendar.keep"},
        }},
    }
    bounded = {
        "id": bounded_id, "provenance": ["test"],
        "value": {"approx": {
            "display_value": "about", "bounds": {
                "lower": {"calendar_id": _BENCHMARK.SHARED_A, "year": 0, "month": 1, "day": 2},
                "upper": {"calendar_id": _BENCHMARK.SHARED_A, "year": 0, "month": 1, "day": 4},
                "x-meta": opaque,
            }, "x-value": {"nested": ["opaque", 9]},
        }},
    }
    frontmatter["chronology"].extend((qualitative, bounded))
    source_path.write_bytes(serialize_record(frontmatter, body))

    repository = Repository(tmp_path)
    world = repository.load_world("WORKTREE")
    assert validate_world(world) == []
    source = SourceChronologyStore(build_chronology_projection(world, validated=True), "WORKTREE")
    expected_values = {item["id"]: item["value"] for item in (qualitative, bounded)}
    source_rows = {item.annotation_id: item for item in source.annotations}
    assert source_rows[qualitative_id].calendar_id is None
    assert source_rows[bounded_id].calendar_id == _BENCHMARK.SHARED_A
    assert {identifier: json.loads(source_rows[identifier].value_json) for identifier in expected_values} == expected_values

    report = compile_world(repository, "WORKTREE", profile_name="state")
    assert report["status"] == "compiled" and report["chronology"]["annotationCount"] == 3
    with closing(connect(repository.root / ".wedl" / "world.sqlite", True)) as connection:
        compiled_rows = {item.annotation_id: item for item in load_chronology_projection(connection).annotations}
        assert {identifier: json.loads(compiled_rows[identifier].value_json) for identifier in expected_values} == expected_values
        assert compiled_rows[qualitative_id].calendar_id is None
        assert compiled_rows[bounded_id].calendar_id == _BENCHMARK.SHARED_A
