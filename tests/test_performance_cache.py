from __future__ import annotations

from wedl.compiler import compile_world
from wedl.repository import Repository


def test_persistent_source_cache_reuses_unchanged_blobs(ash_repo) -> None:
    first = ash_repo.load_world()
    assert len(first.records) == 262
    first_stats = ash_repo.last_load_stats
    assert first_stats["parsed"] == 262

    fresh_process_equivalent = Repository(ash_repo.root)
    second = fresh_process_equivalent.load_world()
    assert len(second.records) == 262
    assert fresh_process_equivalent.last_load_stats == {
        "mode": "parsed-source-cache",
        "records": 262,
        "parsed": 0,
        "cacheHits": 262,
        "blobReads": 0,
    }


def test_exact_compile_cache_skips_source_loading(ash_repo, monkeypatch) -> None:
    compile_world(ash_repo, force=True)

    def fail_load(*_args, **_kwargs):  # pragma: no cover - should not execute
        raise AssertionError("exact compile cache unexpectedly loaded source")

    monkeypatch.setattr(ash_repo, "load_world", fail_load)
    result = compile_world(ash_repo)
    assert result["status"] == "cache-hit"
    assert result["sourceLoad"]["mode"] == "not-required"


def test_compile_reports_search_substages_and_reuses_vectors(ash_repo) -> None:
    first = compile_world(ash_repo, force=True)
    stages = first["searchProjection"]["timingsMs"]
    assert {
        "buildDocuments",
        "buildVectors",
        "insertFts",
        "insertVectors",
    } <= set(stages)
    assert first["vectorCache"]["generated"] > 0
    assert first["uniqueVectorCount"] < first["vectorLinkCount"]

    second = compile_world(ash_repo, force=True)
    assert second["vectorCache"]["generated"] == 0
    assert second["vectorCache"]["reused"] == second["uniqueVectorCount"]
    assert second["vectorLinkCount"] == second["searchDocumentCount"]
