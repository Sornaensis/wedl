"""Focused compilation and replay checks against the ratified v0.7 vector."""
from __future__ import annotations

from copy import deepcopy
from pathlib import Path
import sqlite3

import yaml

from wedl.compiler import (
    INDEX_DDL, _bootstrap_compiled_connection, _compiled_database_issues,
    _insert_entities, _populate_full_compiled_connection, _verify_compiled_connection,
    fingerprint,
)
from wedl.generational import GENERATIONAL_KINDS
from wedl.generational_index import (
    cited_ancestors, cited_containment, current_holders, fold_record,
    insert_generational_index,
)
from wedl.ids import id_from_seed
from wedl.model import Record, StoryTime, World
from wedl.profiles import resolve_profile
from wedl.search import build_documents
from wedl.source import KIND_DIR


VECTOR = Path(__file__).resolve().parents[1] / "docs/decisions/examples/generational-schema-v07.yaml"


def _world() -> tuple[World, dict[str, str]]:
    vector = yaml.safe_load(VECTOR.read_text(encoding="utf-8"))
    mapping = {
        **{key: id_from_seed("character", key) for key in vector["reference_catalog"]["characters"]},
        **{key: id_from_seed("location", key) for key in vector["reference_catalog"]["locations"]},
        **{key: id_from_seed("event", key) for key in vector["endpoint_vectors"]["events"]},
    }

    def replace(value):
        if isinstance(value, str):
            return mapping.get(value, value)
        if isinstance(value, list):
            if value and all(isinstance(item, str) and item.startswith("character_") for item in value):
                return sorted(replace(item) for item in value)
            return [replace(item) for item in value]
        if isinstance(value, dict):
            return {key: replace(item) for key, item in value.items()}
        return value

    data = [replace(item) for item in vector["frontmatter_vectors"].values()]
    data.append(replace(vector["capability_registry"]["envelopes"]["generational_only"]["world"]))
    for original, identifier in mapping.items():
        kind = "character" if original.startswith("character_") else "location" if original.startswith("location_") else "event"
        item = {"schema": "wedl/v0.7", "kind": kind, "id": identifier,
                "title": original, "domain": "fixtures.core", "status": "canonical",
                "tags": [], "aliases": []}
        if kind == "event":
            item["time"] = vector["endpoint_vectors"]["events"][original]["story_time"]
        data.append(item)
    records = {}
    for item in data:
        path = f"story/{KIND_DIR[item['kind']]}/{item['id']}.md"
        body = "sealed lineage prose" if item["kind"] == "parentage" else ""
        record = Record(item, body, path, b"")
        records[record.id] = record
    return World("vector-revision", "vector-tree", records, VECTOR.parent), mapping


def _connection(world: World, at: StoryTime) -> sqlite3.Connection:
    connection = sqlite3.connect(":memory:")
    connection.row_factory = sqlite3.Row
    _bootstrap_compiled_connection(connection)
    _insert_entities(connection, world)
    insert_generational_index(connection, world, at)
    return connection


def _add_parentage(world: World, template: Record, *, seed: str,
                   child_id: str, parent_id: str, tick: int,
                   basis: str = "biological") -> Record:
    data = deepcopy(template.frontmatter)
    data["id"] = id_from_seed("parentage", seed)
    data["child_id"] = child_id
    data["parent_id"] = parent_id
    data["initialization"]["transition_id"] = id_from_seed("generational-transition", f"{seed}-init")
    data["initialization"]["applicability"]["point"]["tick"] = tick
    data["initialization"]["payload"]["basis"] = basis
    data["transitions"] = []
    record = Record(data, "", f"story/kinships/{data['id']}.md", b"")
    world.records[record.id] = record
    return record


def test_ratified_vector_compiles_all_kinds_and_replays_boundaries() -> None:
    world, mapping = _world()
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        counts = dict(connection.execute(
            "SELECT kind,COUNT(*) FROM generational_record GROUP BY kind"
        ).fetchall())
        assert set(counts) == GENERATIONAL_KINDS
        assert connection.execute("PRAGMA foreign_key_check").fetchone() is None
        assert connection.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        assert connection.execute("SELECT COUNT(*) FROM generational_union_participant").fetchone()[0] >= 3
        ids = {record.id for record in world.records.values() if record.kind in GENERATIONAL_KINDS}
        vacancy = next(record.id for record in world.records.values() if record.title.startswith("Vacancy after"))
        legacy = next(record.id for record in world.records.values() if record.kind == "legacy")
        before, inside, after = (StoryTime("main", -1, 0), StoryTime("main", -1, 1), StoryTime("main", 0, 0))
        assert fold_record(connection, vacancy, at=before, candidate_ids=ids)["state"] == "holding"
        assert fold_record(connection, vacancy, at=inside, candidate_ids=ids)["state"] == "vacant-interval"
        assert fold_record(connection, vacancy, at=after, candidate_ids=ids)["state"] == "expired"
        assert [item["value"]["holder_id"] for item in current_holders(connection, legacy, at=after, candidate_ids=ids)] == [mapping["character_alpha"], mapping["character_beta"]]
        target = next(record.id for record in world.records.values() if record.title == "Gamma receives the keys")
        assert fold_record(connection, target, at=StoryTime("main", 1, 1), candidate_ids=ids)["state"] == "vacant"
        assert fold_record(connection, target, at=StoryTime("main", 1, 2), candidate_ids=ids)["value"]["holder_id"] == mapping["character_gamma"]
        transferred = next(record.id for record in world.records.values() if record.title == "Alpha holds the keys")
        transfer = fold_record(connection, transferred, at=StoryTime("main", 1, 1), candidate_ids=ids)
        assert transfer["state"] == "vacant"
        assert transfer["value"]["holder_id"] is None
        assert transfer["citations"][-1]["causeCitation"]["entityId"] == mapping["event_transfer"]
        assert connection.execute("SELECT COUNT(*) FROM generational_claim").fetchone()[0] == 2
        future = next(record.id for record in world.records.values() if record.title == "Future lineage")
        assert fold_record(connection, future, at=after, candidate_ids=ids) is None
        assert connection.execute("SELECT COUNT(*) FROM generational_candidate WHERE record_id=?", (future,)).fetchone()[0] == 1
    finally:
        connection.close()


def test_cited_bounded_traversal_and_private_search_candidates() -> None:
    world, mapping = _world()
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        ids = {record.id for record in world.records.values() if record.kind in GENERATIONAL_KINDS}
        at = StoryTime("main", 0, 0)
        ancestry = cited_ancestors(connection, mapping["character_child"], at=at,
                                   candidate_ids=ids, max_depth=2, max_items=8)
        assert ancestry["status"] == "ok"
        assert {path["targetId"] for path in ancestry["paths"]} == {
            mapping["character_biological"], mapping["character_adoptive"]}
        assert all(path["edges"][0]["citations"][0]["sourcePath"] for path in ancestry["paths"])
        assert cited_ancestors(connection, mapping["character_child"], at=at,
                               candidate_ids=ids, max_depth=2, max_items=1) == {"status": "limit", "paths": []}
        child_org = next(record.id for record in world.records.values() if record.title == "Cadet House")
        containment = cited_containment(connection, child_org, at=at,
                                        candidate_ids=ids, max_depth=2, max_items=8)
        assert len(containment["paths"]) == 1 and containment["paths"][0]["edges"][0]["citations"]
        visible = ids - {record.id for record in world.records.values() if record.title == "Sealed lineage"}
        assert fold_record(connection, next(iter(ids - visible)), at=at, candidate_ids=visible) is None
        assert connection.execute("SELECT COUNT(*) FROM generational_candidate").fetchone()[0] > len(ids)
        assert all(document.entity_id not in ids for document in build_documents(world))
    finally:
        connection.close()


def test_shared_full_and_fast_forward_paths_have_identical_rows() -> None:
    world, _mapping = _world()
    profile = resolve_profile(world, profile_name="fts")
    snapshots = []
    for mode in ("full", "fast-forward-rebuild"):
        connection = sqlite3.connect(":memory:")
        connection.row_factory = sqlite3.Row
        try:
            _bootstrap_compiled_connection(connection)
            _populate_full_compiled_connection(connection, world, object(), profile,
                                               fingerprint(world, profile), mode, {})
            _verify_compiled_connection(connection)
            snapshots.append({
                table: [tuple(row) for row in connection.execute(f"SELECT * FROM {table} ORDER BY 1")]
                for table in ("generational_record", "generational_parentage", "generational_union_participant",
                              "generational_transition", "generational_current", "generational_candidate")
            })
            assert connection.execute("SELECT COUNT(*) FROM search_document WHERE entity_id IN (SELECT id FROM generational_record)").fetchone()[0] == 0
            assert connection.execute("SELECT COUNT(*) FROM search_fts WHERE search_fts MATCH 'sealed'").fetchone()[0] == 0
        finally:
            connection.close()
    assert snapshots[0] == snapshots[1]


def test_replacement_reparenting_and_timeline_are_exact() -> None:
    world, _mapping = _world()
    parent = next(record for record in world.records.values() if record.title == "House Aster")
    child = next(record for record in world.records.values() if record.title == "Cadet House")
    parent.frontmatter["title"] = "Future summary label"
    original = parent.frontmatter["transitions"][0]
    corrected = {
        **original,
        "transition_id": id_from_seed("generational-transition", "corrected-rename"),
        "payload": {"title": "Corrected House", "aliases": []},
        "replaces_transition_id": original["transition_id"],
    }
    parent.frontmatter["transitions"].append(corrected)
    child.frontmatter["transitions"].append({
        "transition_id": id_from_seed("generational-transition", "child-reparent"),
        "transition_kind": "organization-reparent",
        "applicability": {"applicability_kind": "instant", "point": {"timeline": "main", "tick": 1, "order": 0}},
        "payload": {"parent_id": None},
    })
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        ids = {record.id for record in world.records.values() if record.kind in GENERATIONAL_KINDS}
        current = fold_record(connection, parent.id, at=StoryTime("main", 0, 0), candidate_ids=ids)
        assert current["value"]["title"] == "Corrected House"
        assert any(item.get("supersededCitations") for item in current["citations"])
        assert all("Future summary label" not in row[0] for row in connection.execute(
            "SELECT structural_json FROM generational_candidate WHERE record_id=?", (parent.id,)
        ))
        assert fold_record(connection, parent.id, at=StoryTime("other", 0, 0), candidate_ids=ids) is None
        assert len(cited_containment(connection, child.id, at=StoryTime("main", 0, 0),
                                     candidate_ids=ids, max_depth=2, max_items=8)["paths"]) == 1
        assert cited_containment(connection, child.id, at=StoryTime("main", 1, 0),
                                 candidate_ids=ids, max_depth=2, max_items=8)["paths"] == []
    finally:
        connection.close()


def test_ancestry_item_bound_counts_only_visible_active_edges() -> None:
    world, mapping = _world()
    child = mapping["character_child"]
    edges = [record for record in world.records.values()
             if record.kind == "parentage" and record.frontmatter["child_id"] == child]
    assert len(edges) == 2
    inactive_data = deepcopy(edges[0].frontmatter)
    inactive_data["id"] = id_from_seed("parentage", "closed-compiler-edge")
    inactive_data["parent_id"] = id_from_seed("character", "closed-compiler-parent")
    inactive_data["initialization"]["transition_id"] = id_from_seed("generational-transition", "closed-compiler-init")
    inactive_data["transitions"] = [{
        "transition_id": id_from_seed("generational-transition", "closed-compiler-end"),
        "transition_kind": "parentage-end",
        "applicability": {"applicability_kind": "instant", "point": {"timeline": "main", "tick": -1, "order": 0}},
        "payload": {},
    }]
    inactive = Record(inactive_data, "", f"story/kinships/{inactive_data['id']}.md", b"")
    world.records[inactive.id] = inactive
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        result = cited_ancestors(
            connection, child, at=StoryTime("main", 0, 0),
            candidate_ids={edges[0].id, inactive.id}, max_depth=1, max_items=2,
        )
        assert result["status"] == "ok"
        assert [path["targetId"] for path in result["paths"]] == [edges[0].frontmatter["parent_id"]]
    finally:
        connection.close()


def test_ancestry_orders_each_depth_globally_by_authored_time() -> None:
    world, mapping = _world()
    template = next(record for record in world.records.values()
                    if record.kind == "parentage" and record.frontmatter["child_id"] == mapping["character_child"])
    _add_parentage(world, template, seed="bio-grandparent", child_id=mapping["character_biological"],
                   parent_id=mapping["character_alpha"], tick=-1)
    _add_parentage(world, template, seed="adoptive-grandparent", child_id=mapping["character_adoptive"],
                   parent_id=mapping["character_beta"], tick=-5)
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        ids = {record.id for record in world.records.values() if record.kind in GENERATIONAL_KINDS}
        result = cited_ancestors(connection, mapping["character_child"], at=StoryTime("main", 0, 0),
                                 candidate_ids=ids, max_depth=2, max_items=8)
        assert result["status"] == "ok"
        assert [path["targetId"] for path in result["paths"]] == [
            mapping["character_biological"], mapping["character_adoptive"],
            mapping["character_beta"], mapping["character_alpha"],
        ]
    finally:
        connection.close()


def test_distinct_authored_parentage_to_same_parent_keeps_both_citations() -> None:
    world, mapping = _world()
    template = next(record for record in world.records.values()
                    if record.kind == "parentage" and record.frontmatter["child_id"] == mapping["character_child"]
                    and record.frontmatter["parent_id"] == mapping["character_biological"])
    second = _add_parentage(world, template, seed="parallel-adoption",
                            child_id=mapping["character_child"],
                            parent_id=mapping["character_biological"], tick=-10,
                            basis="adoptive")
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        candidates = {template.id, second.id}
        result = cited_ancestors(connection, mapping["character_child"], at=StoryTime("main", 0, 0),
                                 candidate_ids=candidates, max_depth=1, max_items=2)
        assert result["status"] == "ok"
        assert [path["targetId"] for path in result["paths"]] == [
            mapping["character_biological"], mapping["character_biological"]]
        assert {path["edges"][0]["edgeId"] for path in result["paths"]} == candidates
        assert len({path["edges"][0]["citations"][0]["sourcePath"] for path in result["paths"]}) == 2
        assert cited_ancestors(connection, mapping["character_child"], at=StoryTime("main", 0, 0),
                               candidate_ids=candidates, max_depth=1, max_items=1) == {"status": "limit", "paths": []}
    finally:
        connection.close()


def test_reparent_cycle_fails_closed_without_partial_containment_path() -> None:
    world, _mapping = _world()
    root = next(record for record in world.records.values() if record.title == "House Aster")
    child = next(record for record in world.records.values() if record.title == "Cadet House")
    root.frontmatter["transitions"].append({
        "transition_id": id_from_seed("generational-transition", "cycle-reparent"),
        "transition_kind": "organization-reparent",
        "applicability": {"applicability_kind": "instant", "point": {"timeline": "main", "tick": 0, "order": 0}},
        "payload": {"parent_id": child.id},
    })
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        ids = {record.id for record in world.records.values() if record.kind in GENERATIONAL_KINDS}
        assert cited_containment(connection, child.id, at=StoryTime("main", 0, 0),
                                 candidate_ids=ids, max_depth=8, max_items=8) == {"status": "limit", "paths": []}
    finally:
        connection.close()


def test_missing_or_malformed_generational_shape_rejects_cache(tmp_path: Path) -> None:
    database = tmp_path / "world.sqlite"
    connection = sqlite3.connect(database)
    try:
        _bootstrap_compiled_connection(connection)
        connection.executescript(INDEX_DDL)
    finally:
        connection.close()
    assert _compiled_database_issues(database) == ()
    connection = sqlite3.connect(database)
    try:
        connection.execute("ALTER TABLE generational_candidate RENAME COLUMN capability TO wrong_capability")
        connection.commit()
    finally:
        connection.close()
    assert _compiled_database_issues(database) == ("tableShape:generational_candidate",)
    connection = sqlite3.connect(database)
    try:
        connection.execute("ALTER TABLE generational_candidate RENAME COLUMN wrong_capability TO capability")
        connection.execute("DROP INDEX generational_parentage_child_idx")
        connection.execute("CREATE INDEX generational_parentage_child_idx ON generational_parentage(parent_id,child_id,id)")
        connection.commit()
    finally:
        connection.close()
    assert _compiled_database_issues(database) == ("indexShape:generational_parentage_child_idx",)
    connection = sqlite3.connect(database)
    try:
        connection.execute("DROP INDEX generational_parentage_child_idx")
        connection.commit()
    finally:
        connection.close()
    assert _compiled_database_issues(database) == ("missingIndex:generational_parentage_child_idx",)


def test_reverse_parentage_index_is_required_and_has_parent_lead(tmp_path: Path) -> None:
    database = tmp_path / "reverse.sqlite"
    connection = sqlite3.connect(database)
    try:
        _bootstrap_compiled_connection(connection)
        connection.executescript(INDEX_DDL)
        columns = [row[2] for row in connection.execute(
            "PRAGMA index_info(generational_parentage_parent_idx)")]
        assert columns == ["parent_id", "child_id", "id"]
        ordered = [row[2] for row in connection.execute(
            "PRAGMA index_info(generational_parentage_parent_time_idx)")]
        assert ordered == ["parent_id", "timeline", "start_tick", "start_order",
                           "source_ordinal", "id", "child_id"]
        connection.execute("DROP INDEX generational_parentage_parent_idx")
        connection.commit()
    finally:
        connection.close()
    assert _compiled_database_issues(database) == (
        "missingIndex:generational_parentage_parent_idx",)


def test_private_structural_search_shape_is_required(tmp_path: Path) -> None:
    database = tmp_path / "search-shape.sqlite"
    connection = sqlite3.connect(database)
    try:
        _bootstrap_compiled_connection(connection)
        connection.executescript(INDEX_DDL)
        assert [row[2] for row in connection.execute(
            "PRAGMA index_info(generational_search_lookup_idx)")] == [
                "audience", "perspective", "timeline", "prefix", "start_tick",
                "start_order", "source_ordinal", "record_id"]
        connection.execute("ALTER TABLE generational_search_prefix RENAME COLUMN prefix TO wrong_prefix")
        connection.commit()
    finally:
        connection.close()
    assert "tableShape:generational_search_prefix" in _compiled_database_issues(database)


def test_private_discovery_shape_and_generation_reject_old_cache(tmp_path: Path) -> None:
    from wedl import SQLITE_SCHEMA
    from wedl.compiler import COMPILER_FINGERPRINT_PREFIX, GENERATIONAL_INDEX_GENERATION_TOKEN

    assert SQLITE_SCHEMA == "wedl-sqlite/v14"
    assert GENERATIONAL_INDEX_GENERATION_TOKEN == "wedl-generational-index/v7"
    assert GENERATIONAL_INDEX_GENERATION_TOKEN in COMPILER_FINGERPRINT_PREFIX
    database = tmp_path / "discovery-shape.sqlite"
    connection = sqlite3.connect(database)
    try:
        _bootstrap_compiled_connection(connection)
        connection.executescript(INDEX_DDL)
        assert [row[2] for row in connection.execute(
            "PRAGMA index_info(generational_discovery_key_idx)")] == [
                "audience", "perspective", "timeline", "kind", "name_key",
                "entity_id", "start_tick", "start_order"]
        assert [row[2] for row in connection.execute(
            "PRAGMA index_info(generational_discovery_segment_idx)")] == [
                "audience", "perspective", "timeline", "kind", "node",
                "name_key", "entity_id"]
        connection.execute("DROP INDEX generational_discovery_key_idx")
        connection.execute("DROP INDEX generational_discovery_segment_idx")
        connection.commit()
    finally:
        connection.close()
    assert set(_compiled_database_issues(database)) == {
        "missingIndex:generational_discovery_key_idx",
        "missingIndex:generational_discovery_segment_idx"}
    connection = sqlite3.connect(database)
    try:
        connection.execute("CREATE INDEX generational_discovery_key_idx ON "
                           "generational_discovery_name(audience,perspective,timeline,kind,"
                           "name_key,entity_id,start_tick,start_order)")
        connection.execute("CREATE INDEX generational_discovery_segment_idx ON "
                           "generational_discovery_segment(audience,perspective,timeline,kind,"
                           "node,name_key,entity_id)")
        connection.execute(
            "ALTER TABLE generational_discovery_time RENAME COLUMN time_rank TO wrong_rank")
        connection.execute("DROP INDEX generational_discovery_label_idx")
        connection.execute("CREATE INDEX generational_discovery_label_idx ON "
                           "generational_discovery_name(entity_id,audience,perspective,timeline,"
                           "start_tick,start_order,source_ordinal) WHERE name<>title")
        connection.commit()
    finally:
        connection.close()
    assert "tableShape:generational_discovery_time" in _compiled_database_issues(database)
    assert "indexShape:generational_discovery_label_idx" in _compiled_database_issues(database)
