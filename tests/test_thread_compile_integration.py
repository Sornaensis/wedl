from __future__ import annotations

from contextlib import closing
from pathlib import Path

import pytest

from wedl.errors import ValidationFailed
from wedl.compiler import cache_readiness, compile_world, connect, database_meta, require_database


THREAD_A = "thread_0123456789ABCDEFGHJKMNPQRS"


def _append_frontmatter(path: Path, value: str) -> None:
    frontmatter, body = path.read_text(encoding="utf-8").split("\n---\n", 1)
    path.write_text(f"{frontmatter}\n{value}\n---\n{body}", encoding="utf-8")


def _upgrade_to_v05(repository, *, declarations: str) -> None:
    for path in (repository.root / "story").rglob("*.md"):
        path.write_text(
            path.read_text(encoding="utf-8").replace("schema: wedl/v0.3", "schema: wedl/v0.5", 1),
            encoding="utf-8",
        )
    _append_frontmatter(repository.root / "story" / "world.md", declarations)


def _add_character_membership(repository) -> str:
    path = sorted((repository.root / "story" / "characters").glob("*.md"))[0]
    _append_frontmatter(path, f"threads:\n  - {THREAD_A}")
    return path.relative_to(repository.root).as_posix()


def _search_inputs(database: Path) -> tuple[list[str], list[str]]:
    with closing(connect(database, True)) as connection:
        chunks = [row[0] for row in connection.execute("SELECT chunk_hash FROM search_document ORDER BY document_id")]
        vectors = [row[0] for row in connection.execute("SELECT input_hash FROM vector_embedding ORDER BY model_id,input_hash")]
    return chunks, vectors


def test_valid_v05_compiles_publishes_threads_and_keeps_search_vector_inputs_stable(ash_repo) -> None:
    _upgrade_to_v05(
        ash_repo,
        declarations=f"threads:\n  - id: {THREAD_A}\n    label: Archive",
    )

    first = compile_world(ash_repo, revision="WORKTREE", profile_name="hybrid")
    database = ash_repo.root / ".wedl" / "world.sqlite"
    first_meta = database_meta(database)
    first_chunks, first_vectors = _search_inputs(database)
    member_path = _add_character_membership(ash_repo)

    second = compile_world(ash_repo, revision="WORKTREE")
    second_meta = database_meta(database)
    second_chunks, second_vectors = _search_inputs(database)
    rebuilt, _ = require_database(ash_repo, revision="WORKTREE", require_compiled=True)
    with closing(connect(database, True)) as connection:
        joins = [tuple(row) for row in connection.execute("SELECT record_id,thread_id FROM record_thread")]
        thread_rows = [tuple(row) for row in connection.execute("SELECT id,label FROM narrative_thread")]
        thread_entities = connection.execute("SELECT id FROM entity WHERE id LIKE 'thread_%'").fetchall()

    assert first["status"] == second["status"] == "compiled"
    assert first_meta["source_schema"] == second_meta["source_schema"] == "wedl/v0.5"
    assert first_meta["compiler_fingerprint"] != second_meta["compiler_fingerprint"]
    assert first_chunks == second_chunks
    assert first_vectors == second_vectors
    assert second["vectorCache"]["generated"] == 0
    assert joins == [(next(record.id for record in rebuilt if record.source_path == member_path), THREAD_A)]
    assert thread_rows == [(THREAD_A, "Archive")]
    assert thread_entities == []
    assert rebuilt.thread_ids == frozenset((THREAD_A,))


def test_invalid_v05_stops_before_database_revision_or_vector_publication(ash_repo) -> None:
    _upgrade_to_v05(ash_repo, declarations="threads: {}")
    cache = ash_repo.root / ".wedl"
    database = cache / "world.sqlite"

    with pytest.raises(ValidationFailed) as failure:
        compile_world(ash_repo, revision="WORKTREE", profile_name="hybrid")

    assert [item["code"] for item in failure.value.diagnostics] == ["WDL-THREAD-001"]
    assert not database.exists()
    assert not (cache / "vector-cache-v2.sqlite").exists()
    assert not list((cache / "revisions").glob("*.sqlite"))
    assert cache_readiness(ash_repo, revision="WORKTREE")["state"] == "missing"


def test_v04_compiled_cache_metadata_is_incompatible(ash_repo) -> None:
    compile_world(ash_repo, profile_name="fts")
    database = ash_repo.root / ".wedl" / "world.sqlite"
    with closing(connect(database)) as connection:
        connection.execute("UPDATE revision SET source_schema='wedl/v0.4'")
        connection.commit()

    readiness = cache_readiness(ash_repo)

    assert readiness["state"] == "incompatible"
    assert "sourceSchema" in readiness["incompatibleFields"]
