from __future__ import annotations

from contextlib import closing
import json
from pathlib import Path

import pytest

from wedl import (
    SOURCE_SCHEMA,
    SUPPORTED_SOURCE_SCHEMAS,
    THREAD_SOURCE_SCHEMA,
    V04_RECOVERY_CONTRACT,
)
from wedl.errors import SupersededSchemaError
from wedl.ids import id_from_seed, valid_id
from wedl.model import Record, Thread, World
from wedl.repository import PARSER_FINGERPRINT, Repository
from wedl.source import extract_entity_refs, parse_record, serialize_record


THREAD_A = "thread_0123456789ABCDEFGHJKMNPQRS"
THREAD_B = "thread_0123456789ABCDEFGHJKMNPQRT"
WORLD_ID = "world_0123456789ABCDEFGHJKMNPQRS"
CHARACTER_ID = "char_0123456789ABCDEFGHJKMNPQRS"


def _record(frontmatter: dict[str, object], path: str) -> Record:
    return Record(frontmatter, "", path, b"")


def test_thread_ids_are_non_entity_identifiers_and_v05_invalidates_parser_cache() -> None:
    generated = id_from_seed("thread", "main narrative work")

    assert valid_id(generated, "thread")
    assert generated.startswith("thread_")
    assert SOURCE_SCHEMA == "wedl/v0.3"
    assert THREAD_SOURCE_SCHEMA == "wedl/v0.5"
    assert SUPPORTED_SOURCE_SCHEMAS == frozenset((SOURCE_SCHEMA, THREAD_SOURCE_SCHEMA))
    assert "wedl/v0.3,wedl/v0.5" in PARSER_FINGERPRINT


def test_v05_world_and_record_expose_grouping_membership_without_entity_refs() -> None:
    world_record = _record({
        "schema": THREAD_SOURCE_SCHEMA,
        "kind": "world",
        "id": WORLD_ID,
        "title": "Threaded world",
        "threads": [{"id": THREAD_A, "label": "Archive"}, {"id": THREAD_B, "label": "Road"}],
    }, "story/world.md")
    character = _record({
        "schema": THREAD_SOURCE_SCHEMA,
        "kind": "character",
        "id": CHARACTER_ID,
        "title": "Mara",
        "threads": [THREAD_A, THREAD_B],
    }, "story/characters/mara.md")
    world = World("WORKTREE", "tree", {world_record.id: world_record, character.id: character}, Path("."))

    assert world.schema == THREAD_SOURCE_SCHEMA
    assert world.threads == (Thread(THREAD_A, "Archive"), Thread(THREAD_B, "Road"))
    assert world.thread_ids == frozenset((THREAD_A, THREAD_B))
    assert character.thread_ids == (THREAD_A, THREAD_B)
    assert extract_entity_refs({"threads": [{"id": THREAD_A, "label": CHARACTER_ID}], "owner": CHARACTER_ID}) == {CHARACTER_ID}


def test_v05_source_round_trip_canonicalizes_thread_lists_without_synthesizing_memberships() -> None:
    world = {
        "schema": THREAD_SOURCE_SCHEMA,
        "kind": "world",
        "id": WORLD_ID,
        "title": "Threaded world",
        "domain": "world",
        "status": "canonical",
        "tags": [],
        "aliases": [],
        "threads": [{"label": "Road", "id": THREAD_B}, {"label": "Archive", "id": THREAD_A}],
    }
    character = {
        "schema": THREAD_SOURCE_SCHEMA,
        "kind": "character",
        "id": CHARACTER_ID,
        "title": "Mara",
        "domain": "cast",
        "status": "canonical",
        "tags": [],
        "aliases": [],
        "threads": [THREAD_B, THREAD_A],
    }
    no_membership = {key: value for key, value in character.items() if key != "threads"}

    parsed_world = parse_record(serialize_record(world, "# World"), "story/world.md")
    parsed_character = parse_record(serialize_record(character, "# Mara"), "story/characters/mara.md")
    rendered_without_membership = serialize_record(no_membership, "# Mara")

    assert parsed_world.frontmatter["threads"] == [{"id": THREAD_A, "label": "Archive"}, {"id": THREAD_B, "label": "Road"}]
    assert parsed_character.frontmatter["threads"] == [THREAD_A, THREAD_B]
    assert b"threads:" not in rendered_without_membership


def test_exact_v04_is_quarantined_before_record_construction_with_recovery_link() -> None:
    data = b"---\nschema: wedl/v0.4\nkind: world\nid: world_0123456789ABCDEFGHJKMNPQRS\n---\n"

    with pytest.raises(SupersededSchemaError) as raised:
        parse_record(data, "story/world.md")

    assert raised.value.code == "v04_superseded"
    assert str(raised.value) == "story/world.md: wedl/v0.4 is superseded; see docs/THREAD_SCHEMA_CONTRACT.md#4-quarantined-v04-recovery"
    assert raised.value.details == {
        "schema": "wedl/v0.4",
        "recoveryContract": V04_RECOVERY_CONTRACT,
    }


def test_current_fingerprint_v04_cache_entry_is_evicted_before_record_rehydration(ash_repo, monkeypatch) -> None:
    repository = Repository(ash_repo.root)
    path, oid = repository._tree_entries(repository.resolve("HEAD"))[0]
    cached_frontmatter = {
        "schema": "wedl/v0.4",
        "kind": "world",
        "id": WORLD_ID,
    }
    with closing(repository._source_cache()) as cache:
        cache.execute(
            "INSERT INTO parsed_record(parser_fingerprint,blob_oid,frontmatter_json,body,raw_bytes) VALUES (?,?,?,?,?)",
            (PARSER_FINGERPRINT, oid, json.dumps(cached_frontmatter), "", b"cached-v04"),
        )
        cache.commit()

    def forbid_rehydration(*_args, **_kwargs):
        raise AssertionError("v0.4 cache entry reached Record construction")

    monkeypatch.setattr("wedl.repository.Record", forbid_rehydration)
    with pytest.raises(SupersededSchemaError) as raised:
        repository.load_world()

    assert raised.value.code == "v04_superseded"
    with closing(repository._source_cache()) as cache:
        remaining = cache.execute(
            "SELECT COUNT(*) FROM parsed_record WHERE parser_fingerprint=? AND blob_oid=?",
            (PARSER_FINGERPRINT, oid),
        ).fetchone()
    assert remaining == (0,)
