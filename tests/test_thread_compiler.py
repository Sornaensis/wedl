from __future__ import annotations

from contextlib import closing
from pathlib import Path
import sqlite3

import pytest

from wedl import THREAD_SOURCE_SCHEMA
from wedl.compiler import DDL, INDEX_DDL, _insert_entities, _insert_threads, cache_readiness, compile_world, connect
from wedl.errors import ValidationFailed
from wedl.model import Record, World


THREAD_A = "thread_0123456789ABCDEFGHJKMNPQRS"
THREAD_B = "thread_0123456789ABCDEFGHJKMNPQRT"
WORLD_ID = "world_0123456789ABCDEFGHJKMNPQRS"
CHARACTER_ID = "char_0123456789ABCDEFGHJKMNPQRS"
HYPOTHESIS_ID = "hyp_0123456789ABCDEFGHJKMNPQRS"


def _record(frontmatter: dict[str, object], path: str) -> Record:
    return Record(frontmatter, "", path, b"")


def test_thread_join_ddl_is_deterministic_and_never_treats_threads_as_entities() -> None:
    world_record = _record({
        "schema": THREAD_SOURCE_SCHEMA, "kind": "world", "id": WORLD_ID, "title": "World",
        "domain": "world", "status": "canonical", "threads": [
            {"id": THREAD_B, "label": "Road"}, {"id": THREAD_A, "label": "Archive"},
        ],
    }, "story/world.md")
    character = _record({
        "schema": THREAD_SOURCE_SCHEMA, "kind": "character", "id": CHARACTER_ID, "title": "Mara",
        "domain": "cast", "status": "canonical", "threads": [THREAD_B, THREAD_A],
    }, "story/characters/mara.md")
    hypothesis = _record({
        "schema": THREAD_SOURCE_SCHEMA, "kind": "hypothesis", "id": HYPOTHESIS_ID, "title": "Draft",
        "domain": "notes", "status": "draft", "threads": [THREAD_A],
    }, "story/hypotheses/draft.md")
    world = World("WORKTREE", "tree", {
        world_record.id: world_record, character.id: character, hypothesis.id: hypothesis,
    }, Path("."))
    connection = sqlite3.connect(":memory:")
    try:
        connection.executescript(DDL)
        _insert_entities(connection, world)
        _insert_threads(connection, world)
        connection.executescript(INDEX_DDL)

        assert connection.execute("SELECT id,label FROM narrative_thread ORDER BY id").fetchall() == [
            (THREAD_A, "Archive"), (THREAD_B, "Road"),
        ]
        assert connection.execute("SELECT record_id,thread_id FROM record_thread ORDER BY record_id,thread_id").fetchall() == [
            (CHARACTER_ID, THREAD_A), (CHARACTER_ID, THREAD_B),
        ]
        assert connection.execute("SELECT id FROM entity WHERE id LIKE 'thread_%'").fetchall() == []
        assert connection.execute("SELECT target_id FROM entity_ref WHERE target_id LIKE 'thread_%'").fetchall() == []
        indexes = {row[1] for row in connection.execute("PRAGMA index_list(record_thread)")}
        assert "record_thread_thread_idx" in indexes
        foreign_keys = {
            (row[3], row[2], row[4])
            for row in connection.execute("PRAGMA foreign_key_list(record_thread)")
        }
        assert foreign_keys == {
            ("record_id", "entity", "id"),
            ("thread_id", "narrative_thread", "id"),
        }
        with pytest.raises(sqlite3.IntegrityError):
            connection.execute("INSERT INTO record_thread VALUES (?,?)", ("char_missing", THREAD_A))
        with pytest.raises(sqlite3.IntegrityError):
            connection.execute("INSERT INTO record_thread VALUES (?,?)", (CHARACTER_ID, "thread_missing"))
    finally:
        connection.close()


def test_invalid_v05_compilation_stops_before_publishing_a_database(ash_repo) -> None:
    for path in (ash_repo.root / "story").rglob("*.md"):
        path.write_text(
            path.read_text(encoding="utf-8").replace("schema: wedl/v0.3", "schema: wedl/v0.5", 1),
            encoding="utf-8",
        )
    database = ash_repo.root / ".wedl" / "world.sqlite"

    with pytest.raises(ValidationFailed) as failure:
        compile_world(ash_repo, revision="WORKTREE")

    assert [item["code"] for item in failure.value.diagnostics] == ["WDL-THREAD-001"]
    assert not database.exists()


def test_v4_sqlite_cache_is_incompatible_after_thread_join_schema_bump(ash_repo) -> None:
    compile_world(ash_repo, profile_name="fts")
    database = ash_repo.root / ".wedl" / "world.sqlite"
    with closing(connect(database)) as connection:
        connection.execute("UPDATE revision SET sqlite_schema='wedl-sqlite/v4'")
        connection.commit()

    readiness = cache_readiness(ash_repo)

    assert readiness["state"] == "incompatible"
    assert "sqliteSchema" in readiness["incompatibleFields"]
