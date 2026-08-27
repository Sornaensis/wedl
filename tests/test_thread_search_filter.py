from __future__ import annotations

from contextlib import closing
from dataclasses import FrozenInstanceError
from pathlib import Path
import sqlite3

import pytest

from wedl import SOURCE_SCHEMA, THREAD_SOURCE_SCHEMA
from wedl.compiler import compile_world, connect
from wedl.errors import UsageError
from wedl.profiles import CompilationProfile
from wedl.search import _finish_ranked_search, search
from wedl.thread_filter import ThreadFilter, filter_ranked_candidates, resolve_thread_filter


THREAD_A = "thread_0123456789ABCDEFGHJKMNPQRS"
THREAD_B = "thread_0123456789ABCDEFGHJKMNPQRT"
THREAD_UNKNOWN = "thread_0123456789ABCDEFGHJKMNPQRV"


def _selector_connection(schema: str = THREAD_SOURCE_SCHEMA) -> sqlite3.Connection:
    connection = sqlite3.connect(":memory:")
    connection.execute("CREATE TABLE revision(source_schema TEXT NOT NULL)")
    connection.execute("INSERT INTO revision VALUES (?)", (schema,))
    connection.execute("CREATE TABLE narrative_thread(id TEXT PRIMARY KEY,label TEXT NOT NULL)")
    connection.execute("CREATE TABLE record_thread(record_id TEXT NOT NULL,thread_id TEXT NOT NULL)")
    connection.executemany(
        "INSERT INTO narrative_thread VALUES (?,?)",
        [(THREAD_A, "Archive"), (THREAD_B, "Road")],
    )
    return connection


def _append_frontmatter(path: Path, value: str) -> None:
    frontmatter, body = path.read_text(encoding="utf-8").split("\n---\n", 1)
    path.write_text(f"{frontmatter}\n{value}\n---\n{body}", encoding="utf-8")


def _upgrade_to_v05(repository) -> tuple[str, str]:
    for path in (repository.root / "story").rglob("*.md"):
        path.write_text(
            path.read_text(encoding="utf-8").replace("schema: wedl/v0.3", "schema: wedl/v0.5", 1),
            encoding="utf-8",
        )
    _append_frontmatter(
        repository.root / "story" / "world.md",
        f"threads:\n  - id: {THREAD_A}\n    label: Archive\n  - id: {THREAD_B}\n    label: Road",
    )
    member_path = sorted((repository.root / "story" / "characters").glob("*.md"))[0]
    _append_frontmatter(member_path, f"threads:\n  - {THREAD_A}")
    world = repository.load_world()
    member = next(
        record
        for record in world.records.values()
        if record.source_path == member_path.relative_to(repository.root).as_posix()
    )
    return member.id, member.title


def test_resolve_thread_filter_requires_normalized_declared_v05_ids() -> None:
    connection = _selector_connection()

    assert resolve_thread_filter(connection, None) is None
    thread_filter = resolve_thread_filter(connection, (THREAD_A, THREAD_B))
    assert thread_filter is not None
    assert thread_filter.thread_ids == (THREAD_A, THREAD_B)
    with pytest.raises(FrozenInstanceError):
        thread_filter.thread_ids = (THREAD_B,)  # type: ignore[misc]
    with pytest.raises(TypeError):
        ThreadFilter((THREAD_A,))

    cases = (
        ((), "thread filter must contain at least one thread id"),
        ((THREAD_A, THREAD_A), "thread filter thread ids must be unique"),
        ((THREAD_B, THREAD_A), "thread filter thread ids must be sorted"),
        (("not-a-thread",), "thread filter id must use the thread_<26 Crockford> format"),
        ((3,), "thread filter id must use the thread_<26 Crockford> format"),
        ((THREAD_UNKNOWN,), "thread filter id is not declared by the world"),
    )
    for thread_ids, message in cases:
        with pytest.raises(UsageError, match=f"^{message}$"):
            resolve_thread_filter(connection, thread_ids)

    v03 = _selector_connection(SOURCE_SCHEMA)
    with pytest.raises(UsageError, match="^thread filtering requires a validated wedl/v0.5 world$"):
        resolve_thread_filter(v03, (THREAD_A,))


def test_thread_filter_membership_lookup_is_single_bounded_stable_any_subsequence() -> None:
    connection = _selector_connection()
    connection.executemany(
        "INSERT INTO record_thread VALUES (?,?)",
        [("char_a", THREAD_A), ("char_c", THREAD_B)],
    )
    candidates = [
        {"documentId": "a", "entityId": "char_a"},
        {"documentId": "b", "entityId": "char_b"},
        {"documentId": "c", "entityId": "char_c"},
    ]
    statements: list[str] = []
    connection.set_trace_callback(statements.append)

    filtered = filter_ranked_candidates(
        connection, candidates, resolve_thread_filter(connection, (THREAD_A, THREAD_B))
    )

    assert filtered == [candidates[0], candidates[2]]
    membership = [statement for statement in statements if "record_thread" in statement]
    assert len(membership) == 1
    assert "JOIN" not in membership[0].upper()
    assert "char_a" in membership[0] and "char_b" in membership[0] and "char_c" in membership[0]

    profile = CompilationProfile(
        name="fts", fts_enabled=True, vector_enabled=False, vector_provider=None,
        vector_model=None, vector_dimensions=0, vector_max_features=0,
        fts_candidate_limit=3, vector_candidate_limit=2,
        hybrid_fts_weight=1.0, hybrid_vector_weight=1.0, hybrid_rrf_k=60.0,
    )
    assert _finish_ranked_search(
        connection, candidates, 1, profile, resolve_thread_filter(connection, (THREAD_B,))
    ) == [candidates[2]]


def test_search_private_thread_filter_is_post_rank_no_backfill_and_does_not_mutate_corpora(ash_repo) -> None:
    member_id, member_title = _upgrade_to_v05(ash_repo)
    compile_world(ash_repo, revision="WORKTREE", profile_name="fts")
    database = ash_repo.root / ".wedl" / "world.sqlite"
    with closing(connect(database, True)) as connection:
        thread_filter = resolve_thread_filter(connection, (THREAD_A,))
        empty_filter = resolve_thread_filter(connection, (THREAD_B,))
        window = 120
        unfiltered = search(
            connection, member_title, perspective="author", character_id=None, scene_id=None,
            at=None, mode="fts", limit=window,
        )
        corpus_before = tuple(
            connection.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
            for table in ("search_document", "search_fts", "vector_embedding", "document_vector")
        )
        statements: list[str] = []
        connection.set_trace_callback(statements.append)
        filtered = search(
            connection, member_title, perspective="author", character_id=None, scene_id=None,
            at=None, mode="fts", limit=1, _thread_filter=thread_filter,
        )
        no_backfill = search(
            connection, member_title, perspective="author", character_id=None, scene_id=None,
            at=None, mode="fts", limit=1, _thread_filter=empty_filter,
        )
        corpus_after = tuple(
            connection.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
            for table in ("search_document", "search_fts", "vector_embedding", "document_vector")
        )

    assert unfiltered
    assert filtered == [item for item in unfiltered if item["entityId"] == member_id][:1]
    assert no_backfill == []
    assert corpus_after == corpus_before
    membership = [statement for statement in statements if "record_thread" in statement]
    assert len(membership) == 2
    assert all("JOIN" not in statement.upper() for statement in membership)
    assert all("record_thread" not in statement for statement in statements[:statements.index(membership[0])])
