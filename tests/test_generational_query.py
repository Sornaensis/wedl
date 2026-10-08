"""Privacy, exact horizon, and indexed generational read regressions."""
from __future__ import annotations

from dataclasses import replace
from copy import deepcopy
from contextlib import closing
from pathlib import Path
import sqlite3
import subprocess

import pytest

from test_generational_compiler import _add_parentage, _connection as _unindexed_connection, _world
from wedl.compiler import INDEX_DDL, compile_world, _bootstrap_compiled_connection, _insert_entities, _compiled_database_issues
from wedl.generational_query import TrustedViewerScope, discovery_connection, query_connection, query_generational
from wedl.ids import id_from_seed
from wedl.generational_index import cited_ancestors, cited_containment, cited_descendants, cited_relative_path, insert_generational_index
from wedl.model import Record, StoryTime
from wedl.util import canonical_json


def _connection(world, at):
    connection = _unindexed_connection(world, at)
    connection.executescript(INDEX_DDL)
    return connection


def _scope(*, mode: str = "author-as-of", at: StoryTime | None = None,
           audiences=frozenset({"public", "council"}),
           perspectives=frozenset({"ordinary", "council-ledger"})) -> TrustedViewerScope:
    return TrustedViewerScope("vector-revision", mode, "main", at or StoryTime("main", 0, 0),
                              audiences, perspectives, frozenset({"generational-core-v1"}),
                              id_from_seed("character", "character_child") if mode == "character" else None)


def _run(connection: sqlite3.Connection, scope: TrustedViewerScope, operation: str,
         subject_id: str, **extra):
    return query_connection(connection, scope,
                            {"operation": operation, "subject_id": subject_id, **extra})


def test_discovery_five_thousand_characters_ten_thousand_edges_indexed_work() -> None:
    world, _mapping = _world()
    character = next(record for record in world.records.values() if record.kind == "character")
    parentage = next(record for record in world.records.values() if record.kind == "parentage")
    generated = []
    for index in range(5000):
        data = deepcopy(character.frontmatter)
        data["id"] = id_from_seed("character", f"scale-{index}")
        data["title"] = f"Scale Person {index:05d}"
        data["aliases"] = [f"Scale Alias {index:05d}"]
        record = Record(data, "", f"story/characters/{data['id']}.md", b"")
        world.records[record.id] = record
        generated.append(record.id)
    for index in range(10000):
        _add_parentage(world, parentage, seed=f"scale-edge-{index}",
                       child_id=generated[index % 5000],
                       parent_id=generated[(index + 1) % 5000], tick=0)
    connection = sqlite3.connect(":memory:")
    connection.row_factory = sqlite3.Row
    try:
        _bootstrap_compiled_connection(connection)
        _insert_entities(connection, world)
        connection.executescript(INDEX_DDL)
        insert_generational_index(connection, world, StoryTime("main", 0, 0))
        assert connection.execute("SELECT COUNT(*) FROM entity WHERE kind='character'").fetchone()[0] >= 5000
        assert connection.execute("SELECT COUNT(*) FROM generational_parentage").fetchone()[0] >= 10000
        scope = TrustedViewerScope("vector-revision", "author-as-of", "main", StoryTime("main", 0, 0),
                                   frozenset({"public"}), frozenset({"ordinary"}),
                                   frozenset({"generational-core-v1"}))
        request = {"operation": "discover", "kind": "character", "text": "scale",
                   "items": 20, "cursor": None}
        work = [0]
        def count_work() -> int:
            work[0] += 1000
            return 0
        connection.set_progress_handler(count_work, 1000)
        first = discovery_connection(connection, scope, request)
        connection.set_progress_handler(None, 0)
        assert first["state"] == "available" and len(first["results"]) == 20
        assert first["cursor"] and len(canonical_json(first).encode()) < 16000
        assert work[0] < 100000, work[0]
        plan = [row[3] for row in connection.execute(
            "EXPLAIN QUERY PLAN SELECT name_key,entity_id FROM generational_discovery_segment "
            "INDEXED BY generational_discovery_segment_idx WHERE audience='public' "
            "AND perspective='ordinary' AND timeline='main' AND kind='character' "
            "AND node=1 AND name_key>='scale' AND name_key<'scalf' "
            "ORDER BY name_key,entity_id LIMIT 21")]
        assert any("generational_discovery_segment_idx" in step for step in plan)
    finally:
        connection.close()


def test_discovery_future_dense_name_range_stays_bounded() -> None:
    world, mapping = _world()
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        rank = connection.execute(
            "SELECT MAX(time_rank) FROM generational_discovery_time "
            "WHERE audience='public' AND perspective='ordinary' AND timeline='main'").fetchone()[0] + 1
        connection.execute("INSERT INTO generational_discovery_time VALUES (?,?,?,?,?,?)",
                           ("public", "ordinary", "main", 1000000, 0, rank))
        connection.executemany(
            "INSERT INTO generational_discovery_segment VALUES (?,?,?,?,?,?,?,?,?)",
            [("public", "ordinary", "main", "character", rank,
              f"future {index:05d}", mapping["character_child"],
              f"Future {index:05d}", "Character Child") for index in range(20000)],
        )
        connection.execute("INSERT INTO generational_discovery_time VALUES (?,?,?,?,?,?)",
                           ("withheld", "ordinary", "main", -500, 0, 1))
        connection.executemany(
            "INSERT INTO generational_discovery_segment VALUES (?,?,?,?,?,?,?,?,?)",
            [("withheld", "ordinary", "main", "character", 1,
              f"hidden {index:05d}", mapping["character_child"],
              f"Hidden {index:05d}", "Character Child") for index in range(20000)],
        )
        scope = _scope(audiences=frozenset({"public"}),
                       perspectives=frozenset({"ordinary"}))
        def measured(prefix: str) -> tuple[dict, int]:
            work = [0]
            def count_work() -> int:
                work[0] += 1000
                return 0
            connection.set_progress_handler(count_work, 1000)
            try:
                result = discovery_connection(connection, scope, {
                    "operation": "discover", "kind": "character", "text": prefix,
                    "items": 20, "cursor": None})
            finally:
                connection.set_progress_handler(None, 0)
            return result, work[0]
        future, future_work = measured("future")
        hidden, hidden_work = measured("hidden")
        absent, absent_work = measured("absent")
        assert future == hidden == absent == {"state": "available", "results": [], "cursor": None}
        assert future_work < 10000 and hidden_work < 10000 and absent_work < 10000
        assert abs(future_work - absent_work) < 3000
        assert abs(hidden_work - absent_work) < 3000
    finally:
        connection.close()


def test_discovery_name_is_admitted_at_exact_same_tick_order() -> None:
    world, mapping = _world()
    character = next(record for record in world.records.values() if record.kind == "character")
    data = deepcopy(character.frontmatter)
    data["id"] = id_from_seed("character", "same-tick-discovery")
    data["title"] = "Same Tick Name"
    data["aliases"] = []
    subject = Record(data, "", f"story/characters/{data['id']}.md", b"")
    world.records[subject.id] = subject
    parentage = next(record for record in world.records.values() if record.kind == "parentage")
    fact = _add_parentage(world, parentage, seed="same-tick-discovery",
                          child_id=subject.id, parent_id=mapping["character_biological"], tick=0)
    fact.frontmatter["initialization"]["applicability"]["point"]["order"] = 1
    connection = _connection(world, StoryTime("main", 0, 1))
    try:
        request = {"operation": "discover", "kind": "character", "text": "same tick",
                   "items": 20, "cursor": None}
        earlier = discovery_connection(connection, _scope(at=StoryTime("main", 0, 0)), request)
        later = discovery_connection(connection, _scope(at=StoryTime("main", 0, 1)), request)
        assert earlier == {"state": "available", "results": [], "cursor": None}
        assert later["state"] == "available" and later["cursor"] is None
        assert [(row["id"], row["matchedName"]) for row in later["results"]] == [
            (subject.id, "Same Tick Name")]
    finally:
        connection.close()


def test_discovery_closes_overlong_titles_and_aliases() -> None:
    world, mapping = _world()
    oversized = world.records[mapping["character_child"]]
    oversized.frontmatter["title"] = "T" * 300
    oversized.frontmatter["aliases"] = ["Short Alias"]
    safe = world.records[mapping["character_biological"]]
    safe.frontmatter["title"] = "Safe Title"
    safe.frontmatter["aliases"] = ["Z" * 300]
    organization = next(record for record in world.records.values()
                        if record.kind == "organization" and record.title == "House Aster")
    organization.frontmatter["transitions"][0]["payload"]["title"] = "O" * 300
    restored = deepcopy(organization.frontmatter["transitions"][0])
    restored["transition_id"] = id_from_seed("generational-transition", "restored-title")
    restored["applicability"]["point"]["tick"] = -4
    restored["payload"] = {"title": "House Renewed", "aliases": ["Renewed"]}
    organization.frontmatter["transitions"].append(restored)
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        scope = _scope()
        def find(prefix: str) -> dict:
            return discovery_connection(connection, scope, {
                "operation": "discover", "kind": "character", "text": prefix,
                "items": 20, "cursor": None})
        assert find("short alias") == {"state": "available", "results": [], "cursor": None}
        assert find("T" * 300) == {"state": "available", "results": [], "cursor": None}
        assert find("Z" * 300) == {"state": "available", "results": [], "cursor": None}
        early_organization = discovery_connection(connection, _scope(at=StoryTime("main", -20, 0)), {
            "operation": "discover", "kind": "organization", "text": "Aster",
            "items": 20, "cursor": None})
        assert early_organization["state"] == "available"
        assert any(row["id"] == organization.id and row["title"] == "House Aster"
                   for row in early_organization["results"])
        assert discovery_connection(connection, _scope(at=StoryTime("main", -5, 0)), {
            "operation": "discover", "kind": "organization", "text": "Aster",
            "items": 20, "cursor": None}) == {"state": "available", "results": [], "cursor": None}
        restored_organization = discovery_connection(connection, scope, {
            "operation": "discover", "kind": "organization", "text": "Aster",
            "items": 20, "cursor": None})
        assert any(row["id"] == organization.id and row["title"] == "House Renewed"
                   for row in restored_organization["results"])
        assert discovery_connection(connection, _scope(at=StoryTime("main", -20, 0)), {
            "operation": "labels", "ids": [organization.id]}) == {"state": "available", "labels": [
                {"id": organization.id, "kind": "organization", "title": "House Aster"}]}
        labels = discovery_connection(connection, _scope(at=StoryTime("main", -5, 0)), {"operation": "labels", "ids": [
            oversized.id, safe.id, organization.id]})
        assert labels == {"state": "available", "labels": [
            {"id": safe.id, "kind": "character", "title": "Safe Title"}]}
    finally:
        connection.close()


def test_labels_seek_title_amid_many_aliases() -> None:
    world, mapping = _world()
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        subject = mapping["character_child"]
        connection.executemany(
            "INSERT INTO generational_discovery_name VALUES (?,?,?,?,?,?,?,?,?,?,?)",
            [("public", "ordinary", "main", f"many alias {index:05d}", subject,
              "character", f"Many Alias {index:05d}", "character_child", 0, 0, 0)
             for index in range(20000)],
        )
        work = [0]
        def count_work() -> int:
            work[0] += 1000
            return 0
        connection.set_progress_handler(count_work, 1000)
        try:
            result = discovery_connection(connection, _scope(
                audiences=frozenset({"public"}), perspectives=frozenset({"ordinary"})),
                {"operation": "labels", "ids": [subject]})
        finally:
            connection.set_progress_handler(None, 0)
        assert result == {"state": "available", "labels": [
            {"id": subject, "kind": "character", "title": "character_child"}]}
        assert work[0] < 10000
    finally:
        connection.close()


def test_overlong_current_title_closes_dense_alias_range_before_limit() -> None:
    world, _mapping = _world()
    organization = next(record for record in world.records.values()
                        if record.kind == "organization" and record.title == "House Aster")
    organization.frontmatter["initialization"]["payload"]["aliases"] = [
        f"Dense Name {index:05d}" for index in range(20000)]
    organization.frontmatter["transitions"][0]["payload"] = {
        "title": "O" * 300, "aliases": []}
    connection = _connection(world, StoryTime("main", -5, 0))
    try:
        request = {"operation": "discover", "kind": "organization", "text": "Dense",
                   "items": 20, "cursor": None}
        visible = discovery_connection(connection, _scope(at=StoryTime("main", -20, 0)), request)
        assert visible["state"] == "available" and len(visible["results"]) == 20
        assert visible["cursor"] and len(canonical_json(visible).encode()) < 16000
        work = [0]
        def count_work() -> int:
            work[0] += 1000
            return 0
        connection.set_progress_handler(count_work, 1000)
        try:
            closed = discovery_connection(connection, _scope(at=StoryTime("main", -5, 0)), request)
        finally:
            connection.set_progress_handler(None, 0)
        assert closed == {"state": "available", "results": [], "cursor": None}
        assert work[0] < 10000
    finally:
        connection.close()


def test_parentage_descendants_and_boundary_order() -> None:
    world, mapping = _world()
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        early = _run(connection, _scope(at=StoryTime("main", -10, 0)), "parents",
                     mapping["character_child"])
        assert early["state"] == "available"
        assert [value["label"] for value in early["relations"]] == ["biological-parent"]
        assert early["relations"][0]["citations"][0]["applicability"]["point"]["tick"] == "-10"
        later = _run(connection, _scope(), "parents", mapping["character_child"])
        assert {value["label"] for value in later["relations"]} == {
            "biological-parent", "adoptive-parent"}
        descendants = _run(connection, _scope(), "descendants", mapping["character_biological"],
                           depth=3, items=10)
        assert [item["targetId"] for item in descendants["relations"]] == [mapping["character_child"]]
        assert descendants["relations"][0]["generationDistance"] == 1
        assert _run(connection, _scope(), "ancestors", mapping["character_child"],
                    depth=1, items=1) == {"state": "limit", "code": "GEN-LIMIT-001"}
    finally:
        connection.close()


def test_union_organization_legacy_vital_and_private_search() -> None:
    world, mapping = _world()
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        union = next(record.id for record in world.records.values() if record.kind == "union")
        assert len(_run(connection, _scope(), "union", union)["participants"]) == 3
        organization = next(record.id for record in world.records.values() if record.title == "Cadet House")
        result = _run(connection, _scope(), "organization", organization)
        assert result["state"] == "available" and len(result["parentPath"]) == 1
        assert result["roles"]
        legacy = next(record.id for record in world.records.values() if record.kind == "legacy")
        result = _run(connection, _scope(), "legacy", legacy)
        assert result["state"] == "available"
        assert len(result["holders"]) == 2 and len(result["claims"]) == 2
        vital = _run(connection, _scope(at=StoryTime("main", 3, 0)), "vital", mapping["character_child"])
        assert vital["vital"] == "living"
        vital = _run(connection, _scope(at=StoryTime("main", 3, 1)), "vital", mapping["character_child"])
        assert vital["vital"] == "dead"
        future = mapping["character_future_lineage"]
        assert _run(connection, _scope(), "parents", future)["state"] == "unknown"
        all_time = replace(_scope(), mode="author-all-time", at=None)
        assert _run(connection, all_time, "parents", future)["state"] == "available"
        sealed = _run(connection, _scope(), "search", "ignored", text="secret_parent")
        assert sealed == {"state": "available", "results": [], "cursor": None}
    finally:
        connection.close()


def test_reverse_union_membership_and_organization_legacies_fold_before_budget() -> None:
    world, mapping = _world()
    union = next(record for record in world.records.values() if record.kind == "union")
    legacy = next(record for record in world.records.values() if record.kind == "legacy")
    organization = legacy.frontmatter["organization_id"]
    original_members = union.frontmatter["participant_ids"]
    union.frontmatter["transitions"].extend((
        {"transition_id": id_from_seed("generational-transition", "reverse-reconcile"),
         "transition_kind": "union-reconcile", "applicability": {
             "applicability_kind": "instant", "point": {"timeline": "main", "tick": 0, "order": 0}},
         "payload": {"participant_ids": original_members[1:]}},
        {"transition_id": id_from_seed("generational-transition", "reverse-end"),
         "transition_kind": "union-end", "applicability": {
             "applicability_kind": "instant", "point": {"timeline": "main", "tick": 0, "order": 2}},
         "payload": {}},
    ))
    legacy.frontmatter["transitions"].extend((
        {"transition_id": id_from_seed("generational-transition", "linked-dormant"),
         "transition_kind": "legacy-dormant", "applicability": {
             "applicability_kind": "instant", "point": {"timeline": "main", "tick": 0, "order": 1}},
         "payload": {}},
        {"transition_id": id_from_seed("generational-transition", "linked-dissolved"),
         "transition_kind": "legacy-dissolve", "applicability": {
             "applicability_kind": "instant", "point": {"timeline": "main", "tick": 1, "order": 0}},
         "payload": {}},
    ))
    hidden_data = deepcopy(union.frontmatter)
    hidden_data["id"] = id_from_seed("union", "reverse-hidden")
    hidden_data["audience"] = ["sealed"]
    hidden_data["initialization"]["transition_id"] = id_from_seed("generational-transition", "reverse-hidden-init")
    hidden_data["transitions"] = []
    hidden = Record(hidden_data, "", f"story/unions/{hidden_data['id']}.md", b"")
    world.records[hidden.id] = hidden
    future_data = deepcopy(hidden_data)
    future_data["id"] = id_from_seed("union", "reverse-future")
    future_data["audience"] = ["public"]
    future_data["initialization"]["transition_id"] = id_from_seed("generational-transition", "reverse-future-init")
    future_data["initialization"]["applicability"]["point"] = {
        "timeline": "main", "tick": 5, "order": 0}
    future = Record(future_data, "", f"story/unions/{future_data['id']}.md", b"")
    world.records[future.id] = future
    sealed_legacy_data = deepcopy(legacy.frontmatter)
    sealed_legacy_data["id"] = id_from_seed("legacy", "reverse-hidden")
    sealed_legacy_data["audience"] = ["sealed"]
    sealed_legacy_data["initialization"]["transition_id"] = id_from_seed(
        "generational-transition", "reverse-hidden-legacy-init")
    sealed_legacy_data["transitions"] = []
    sealed_legacy = Record(sealed_legacy_data, "", f"story/legacies/{sealed_legacy_data['id']}.md", b"")
    world.records[sealed_legacy.id] = sealed_legacy
    future_legacy_data = deepcopy(sealed_legacy_data)
    future_legacy_data["id"] = id_from_seed("legacy", "reverse-future")
    future_legacy_data["audience"] = ["public"]
    future_legacy_data["initialization"]["transition_id"] = id_from_seed(
        "generational-transition", "reverse-future-legacy-init")
    future_legacy_data["initialization"]["applicability"]["point"] = {
        "timeline": "main", "tick": 5, "order": 0}
    future_legacy = Record(future_legacy_data, "", f"story/legacies/{future_legacy_data['id']}.md", b"")
    world.records[future_legacy.id] = future_legacy
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        before = _scope(at=StoryTime("main", -3, 1))
        assert len(_run(connection, before, "character-unions", original_members[0])["unions"]) == 1
        current = _scope(at=StoryTime("main", 0, 0))
        assert _run(connection, current, "character-unions", original_members[0]) == {"state": "unknown"}
        current_member = _run(connection, current, "character-unions", original_members[1])
        assert [row["recordId"] for row in current_member["unions"]] == [union.id]
        assert _run(connection, current, "character-unions", original_members[1], items=0) == {
            "state": "invalid", "code": "GEN-REQUEST-001"}
        assert _run(connection, _scope(at=StoryTime("main", 0, 2)),
                    "character-unions", original_members[1]) == {"state": "unknown"}
        assert _run(connection, current, "character-unions", mapping["character_unrecorded"]) == {
            "state": "unknown"}
        for at, state in ((StoryTime("main", 0, 0), "active"),
                          (StoryTime("main", 0, 1), "dormant"),
                          (StoryTime("main", 1, 0), "dissolved")):
            rows = _run(connection, _scope(at=at), "organization-legacies", organization)["legacies"]
            assert [(row["recordId"], row["state"]) for row in rows] == [(legacy.id, state)]
        all_time = replace(current, mode="author-all-time", at=None)
        assert _run(connection, all_time, "organization-legacies", organization) == {
            "state": "invalid", "code": "GEN-REQUEST-001"}
    finally:
        connection.close()


def test_reverse_reads_close_overflow_and_seek_large_unrelated_postings() -> None:
    world, mapping = _world()
    legacy = next(record for record in world.records.values() if record.kind == "legacy")
    extra_data = deepcopy(legacy.frontmatter)
    extra_data["id"] = id_from_seed("legacy", "reverse-second")
    extra_data["title"] = "Second linked legacy"
    extra_data["initialization"]["transition_id"] = id_from_seed(
        "generational-transition", "reverse-second-init")
    extra = Record(extra_data, "", f"story/legacies/{extra_data['id']}.md", b"")
    world.records[extra.id] = extra
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        organization = legacy.frontmatter["organization_id"]
        assert _run(connection, _scope(), "organization-legacies", organization, items=1) == {
            "state": "limit", "code": "GEN-LIMIT-001"}
        assert len(_run(connection, _scope(), "organization-legacies", organization,
                        items=2)["legacies"]) == 2
        union = next(record for record in world.records.values() if record.kind == "union")
        transition = union.frontmatter["initialization"]["transition_id"]
        connection.executemany(
            "INSERT INTO generational_union_participant VALUES (?,?,?,?)",
            ((union.id, transition, f"char_noise_{index:06d}", index)
             for index in range(10000)),
        )
        # Valid but unrelated private records make a table scan measurable.
        connection.executemany("INSERT INTO entity VALUES (?,?,?,?,?,?,?,?,?)", (
            (f"legacy_noise_{index:06d}", "legacy", "noise", "test", "canonical",
             f"story/legacies/legacy_noise_{index:06d}.md", None, "", "{}")
            for index in range(5000)))
        connection.executemany("INSERT INTO generational_record VALUES (?,?,?,?,?,?,?,?,?,?)", (
            (f"legacy_noise_{index:06d}", "legacy", 100000 + index,
             f"story/legacies/legacy_noise_{index:06d}.md", None, "canonical",
             "generational-core-v1", "main", '["public"]', '["ordinary"]')
            for index in range(5000)))
        connection.executemany("INSERT INTO generational_legacy VALUES (?,?,?)", (
            (f"legacy_noise_{index:06d}", "office", "organization_noise")
            for index in range(5000)))
        union_plan = " ".join(str(row[3]) for row in connection.execute(
            "EXPLAIN QUERY PLAN SELECT DISTINCT union_id FROM generational_union_participant "
            "INDEXED BY generational_union_participant_idx WHERE participant_id=? ORDER BY union_id",
            (mapping["character_alpha"],)))
        legacy_plan = " ".join(str(row[3]) for row in connection.execute(
            "EXPLAIN QUERY PLAN SELECT id FROM generational_legacy "
            "INDEXED BY generational_legacy_organization_idx WHERE organization_id=? ORDER BY id",
            (organization,)))
        assert "generational_union_participant_idx" in union_plan
        assert "generational_legacy_organization_idx" in legacy_plan
        steps = [0]

        def progress() -> int:
            steps[0] += 100
            return 0

        connection.set_progress_handler(progress, 100)
        try:
            assert len(_run(connection, _scope(), "character-unions",
                            mapping["character_alpha"])["unions"]) == 1
            union_steps = steps[0]
            steps[0] = 0
            assert len(_run(connection, _scope(), "organization-legacies",
                            organization)["legacies"]) == 2
            legacy_steps = steps[0]
        finally:
            connection.set_progress_handler(None, 0)
        assert union_steps < 5000, union_steps
        assert legacy_steps < 5000, legacy_steps
        base_transition = legacy.frontmatter["initialization"]["transition_id"]
        for index in range(499):
            ident = id_from_seed("legacy", f"reverse-cap-{index}")
            transition_id = id_from_seed("generational-transition", f"reverse-cap-{index}")
            path = f"story/legacies/{ident}.md"
            connection.execute(
                "INSERT INTO entity SELECT ?,kind,title,domain,status,?,blob_oid,body_markdown,frontmatter_json "
                "FROM entity WHERE id=?", (ident, path, legacy.id))
            connection.execute(
                "INSERT INTO generational_record SELECT ?,kind,?, ?,blob_oid,status,capability,timeline,"
                "audience_json,perspectives_json FROM generational_record WHERE id=?",
                (ident, 200000 + index, path, legacy.id))
            connection.execute(
                "INSERT INTO generational_legacy SELECT ?,legacy_kind,organization_id "
                "FROM generational_legacy WHERE id=?", (ident, legacy.id))
            connection.execute(
                "INSERT INTO generational_transition SELECT ?,?,source_ordinal,transition_kind,"
                "applicability_kind,timeline,start_tick,start_order,end_tick,end_order,payload_json,"
                "cause_event_id,cause_citation_json,replaces_transition_id,citation_json "
                "FROM generational_transition WHERE id=?",
                (transition_id, ident, base_transition))
            connection.execute(
                "INSERT INTO generational_candidate SELECT ?,?,source_ordinal,capability,timeline,"
                "start_tick,start_order,end_tick,end_order,audience_json,perspectives_json,"
                "citation_json,structural_json FROM generational_candidate WHERE transition_id=?",
                (ident, transition_id, base_transition))
        # Two genuine fixture records plus 499 distinct compiled candidates.
        assert _run(connection, _scope(), "organization-legacies", organization,
                    items=500) == {"state": "limit", "code": "GEN-LIMIT-001"}
        assert _run(connection, _scope(), "organization-legacies", organization,
                    items=501) == {"state": "invalid", "code": "GEN-REQUEST-001"}
    finally:
        connection.close()


def test_reverse_single_large_union_obeys_nested_participant_budget() -> None:
    world, mapping = _world()
    union = next(record for record in world.records.values() if record.kind == "union")
    template = world.records[mapping["character_alpha"]]
    participants = set(union.frontmatter["participant_ids"])
    for index in range(97):
        data = deepcopy(template.frontmatter)
        data["id"] = id_from_seed("character", f"reverse-member-{index}")
        data["title"] = f"Reverse member {index}"
        record = Record(data, "", f"story/characters/{data['id']}.md", b"")
        world.records[record.id] = record
        participants.add(record.id)
    members = sorted(participants)
    assert len(members) == 100
    union.frontmatter["participant_ids"] = members
    union.frontmatter["initialization"]["payload"]["participant_ids"] = members
    union.frontmatter["transitions"][0]["payload"]["participant_ids"] = members
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        assert _run(connection, _scope(), "character-unions", mapping["character_alpha"],
                    items=99) == {"state": "limit", "code": "GEN-LIMIT-001"}
        rows = _run(connection, _scope(), "character-unions", mapping["character_alpha"],
                    items=100)["unions"]
        assert len(rows) == 1 and rows[0]["value"]["participant_ids"] == members
    finally:
        connection.close()


def _former_role_world():
    world, mapping = _world()
    template = next(record for record in world.records.values() if record.kind == "affiliation")

    def add(seed, character, *, init_tick=-5, role=None, end_tick=None, end_order=0,
            audience=None):
        data = deepcopy(template.frontmatter)
        data["id"] = id_from_seed("affiliation", seed)
        data["title"] = seed
        data["character_id"] = mapping[character]
        data["initialization"]["transition_id"] = id_from_seed("generational-transition", f"{seed}-init")
        data["initialization"]["applicability"]["point"]["tick"] = init_tick
        data["initialization"]["payload"]["role"] = None
        data["transitions"] = []
        if audience is not None:
            data["audience"] = [audience]
        if role is not None:
            data["transitions"].append({
                "transition_id": id_from_seed("generational-transition", f"{seed}-earlier-role"),
                "transition_kind": "affiliation-role", "applicability": {
                    "applicability_kind": "instant", "point": {"timeline": "main", "tick": -1, "order": 2}},
                "payload": {"role": "apprentice"}})
            data["transitions"].append({
                "transition_id": id_from_seed("generational-transition", f"{seed}-role"),
                "transition_kind": "affiliation-role", "applicability": {
                    "applicability_kind": "instant", "point": {"timeline": "main", "tick": 0, "order": 0}},
                "payload": {"role": role}, "cause_event_id": mapping["event_appointment"]})
        if end_tick is not None:
            data["transitions"].append({
                "transition_id": id_from_seed("generational-transition", f"{seed}-end"),
                "transition_kind": "affiliation-end", "applicability": {
                    "applicability_kind": "instant", "point": {"timeline": "main", "tick": end_tick,
                                                            "order": end_order}}, "payload": {}})
        record = Record(data, "", f"story/affiliations/{data['id']}.md", b"")
        world.records[record.id] = record
        return record.id

    ended = add("former-role", "character_alpha", role="steward", end_tick=0, end_order=1)
    null_role = add("former-null", "character_beta", end_tick=-2)
    hidden = add("former-hidden", "character_gamma", role="secret steward",
                 end_tick=0, end_order=1, audience="sealed")
    future = add("former-future", "character_no_lineage", init_tick=5, end_tick=6)
    template.frontmatter["transitions"].append({
        "transition_id": id_from_seed("generational-transition", "active-role-churn"),
        "transition_kind": "affiliation-role", "applicability": {
            "applicability_kind": "instant", "point": {"timeline": "main", "tick": 0, "order": 1}},
        "payload": {"role": "warden"}})
    organization = next(record.id for record in world.records.values() if record.title == "Cadet House")
    return world, organization, ended, null_role, hidden, future


def _seed_former_role_cause(connection: sqlite3.Connection, world) -> None:
    # The lightweight index fixture omits narrative events from the full cache.
    event = next(record for record in world.records.values()
                 if record.kind == "event" and record.frontmatter["time"]["tick"] == 0)
    connection.execute("INSERT INTO event VALUES (?,?,?,?,?,?)",
                       (event.id, "main", 0, -1, None, "canonical"))


def test_former_roles_exact_horizon_visibility_role_and_combined_cap() -> None:
    world, organization, ended, null_role, hidden, future = _former_role_world()
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        _seed_former_role_cause(connection, world)
        before = _run(connection, _scope(at=StoryTime("main", 0, -1)), "organization",
                      organization, includeFormerRoles=True)
        initial = _run(connection, _scope(at=StoryTime("main", -1, 1)), "organization",
                       organization, includeFormerRoles=True)
        at_role = _run(connection, _scope(at=StoryTime("main", 0, 0)), "organization",
                       organization, includeFormerRoles=True)
        at_end = _run(connection, _scope(at=StoryTime("main", 0, 1)), "organization", organization,
                      includeFormerRoles=True)
        assert ended in [row["recordId"] for row in before["roles"]]
        assert ended not in [row["recordId"] for row in before["formerRoles"]]
        role_before = next(row for row in before["roles"] if row["recordId"] == ended)
        role_initial = next(row for row in initial["roles"] if row["recordId"] == ended)
        role_at = next(row for row in at_role["roles"] if row["recordId"] == ended)
        former = next(row for row in at_end["formerRoles"] if row["recordId"] == ended)
        assert role_initial["value"]["role"] is None
        assert role_before["value"]["role"] == "apprentice" and role_before["causes"] == []
        assert role_at["value"]["role"] == former["value"]["role"] == "steward"
        assert len(role_at["citations"]) == 3 and len(former["citations"]) == 4
        assert len(role_at["causes"]) == len(former["causes"]) == 1
        assert former["causes"][0]["citation"]["applicability"]["point"] == {
            "timeline": "main", "tick": "0", "order": "-1"}
        assert former["state"] == "ended"
        assert at_role["roles"][0]["value"]["role"] == "heir-apparent"
        assert at_end["roles"][0]["value"]["role"] == "warden"
        assert [row["recordId"] for row in at_end["formerRoles"]] == sorted((ended, null_role))
        assert next(row for row in at_end["formerRoles"] if row["recordId"] == null_role)["value"]["role"] is None
        assert hidden not in canonical_json(at_end) and future not in canonical_json(at_end)
        assert "secret steward" not in canonical_json(at_end)
        default = _run(connection, _scope(at=StoryTime("main", 0, 1)), "organization", organization)
        assert canonical_json(default) == canonical_json({key: value for key, value in at_end.items()
                                                         if key != "formerRoles"})
        assert _run(connection, _scope(at=StoryTime("main", 0, 1)), "organization", organization, items=2,
                    includeFormerRoles=True) == {"state": "limit", "code": "GEN-LIMIT-001"}
        assert len(_run(connection, _scope(at=StoryTime("main", 0, 1)), "organization", organization, items=3,
                        includeFormerRoles=True)["roles"]) == 1
        assert _run(connection, _scope(at=StoryTime("main", 0, 1)), "organization", organization, items=3,
                    includeFormerRoles=True)["state"] == "available"
        world.records.pop(hidden)
        world.records.pop(future)
        comparison = _connection(world, StoryTime("main", 0, 0))
        try:
            _seed_former_role_cause(comparison, world)
            assert canonical_json(_run(comparison, _scope(at=StoryTime("main", 0, 1)),
                                       "organization", organization, includeFormerRoles=True)) == canonical_json(at_end)
        finally:
            comparison.close()
    finally:
        connection.close()


def test_former_roles_request_is_strictly_opt_in() -> None:
    world, organization, *_ = _former_role_world()
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        for value in (False, None, 1, "true"):
            assert _run(connection, _scope(), "organization", organization,
                        includeFormerRoles=value) == {"state": "invalid", "code": "GEN-REQUEST-001"}
        assert _run(connection, _scope(), "organization", organization,
                    includeFormerRoles=True, cursor=None) == {
                        "state": "invalid", "code": "GEN-REQUEST-001"}
        assert _run(connection, replace(_scope(), mode="author-all-time", at=None),
                    "organization", organization, includeFormerRoles=True) == {
                        "state": "invalid", "code": "GEN-REQUEST-001"}
        assert _run(connection, replace(_scope(), at=None), "organization", organization,
                    includeFormerRoles=True) == {"state": "invalid", "code": "GEN-REQUEST-001"}
        assert _run(connection, _scope(), "union", next(record.id for record in world.records.values()
                                                        if record.kind == "union"),
                    includeFormerRoles=True) == {"state": "invalid", "code": "GEN-REQUEST-001"}
        assert _run(connection, _scope(mode="character"), "organization", organization,
                    includeFormerRoles=True) == {"state": "invalid", "code": "GEN-REQUEST-001"}
        for items in (0, 501):
            assert _run(connection, _scope(), "organization", organization,
                        includeFormerRoles=True, items=items) == {
                            "state": "invalid", "code": "GEN-REQUEST-001"}
    finally:
        connection.close()


def test_character_absent_future_secret_are_identical_and_scope_is_closed() -> None:
    world, mapping = _world()
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        scope = _scope(mode="character")
        absent = _run(connection, scope, "parents", mapping["character_no_lineage"])
        future = _run(connection, scope, "parents", mapping["character_future_lineage"])
        secret = _run(connection, scope, "parents", mapping["character_secret"])
        assert absent == future == secret == {"state": "unknown"}
        assert _run(connection, scope, "search", "ignored", text="lineage") == absent
        assert query_connection(connection, _scope(), {"operation": "parents",
            "subject_id": mapping["character_child"], "audiences": ["archivist"]}) == {
                "state": "invalid", "code": "GEN-REQUEST-001"}
        assert _run(connection, replace(_scope(), at=StoryTime("alternate", 0, 0)),
                    "parents", mapping["character_child"])["code"] == "GEN-TIME-001"
    finally:
        connection.close()


def test_cursor_is_bound_to_scope_and_revision() -> None:
    world, _mapping = _world()
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        first = _run(connection, _scope(), "search", "ignored", text="basis", items=1)
        assert first["state"] == "available" and first["cursor"]
        second = _run(connection, _scope(), "search", "ignored", text="basis", items=1,
                      cursor=first["cursor"])
        assert second["state"] == "available"
        changed = replace(_scope(), revision="other")
        assert _run(connection, changed, "search", "ignored", text="basis", items=1,
                    cursor=first["cursor"])["state"] == "invalid"
    finally:
        connection.close()


def test_relatives_are_one_continuous_cited_path_in_both_directions() -> None:
    world, mapping = _world()
    template = next(record for record in world.records.values() if record.kind == "parentage")
    sibling = id_from_seed("character", "query-sibling")
    _add_parentage(world, template, seed="query-sibling-edge", child_id=sibling,
                   parent_id=mapping["character_biological"], tick=-2)
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        child, parent = mapping["character_child"], mapping["character_biological"]
        for first, second in ((child, parent), (parent, child), (child, sibling)):
            result = _run(connection, _scope(), "relatives", first,
                          target_id=second, depth=3, items=10)
            assert result["state"] == "available"
            edges = result["relations"][0]["edges"]
            assert edges[0]["from"] == first and edges[-1]["to"] == second
            assert all(left["to"] == right["from"] for left, right in zip(edges, edges[1:]))
            assert all(edge["citations"] for edge in edges)
        assert _run(connection, _scope(), "relatives", child,
                    target_id=sibling, depth=1, items=10) == {
                        "state": "limit", "code": "GEN-LIMIT-001"}
    finally:
        connection.close()


def test_relative_path_merges_equal_depth_edges_by_global_applicability() -> None:
    world, mapping = _world()
    template = next(record for record in world.records.values() if record.kind == "parentage")
    source = mapping["character_no_lineage"]
    early_frontier = id_from_seed("character", "relative-early-frontier")
    late_frontier = id_from_seed("character", "relative-late-frontier")
    destination = id_from_seed("character", "relative-common-destination")
    _add_parentage(world, template, seed="relative-first-early", child_id=source,
                   parent_id=early_frontier, tick=-20)
    late_first = _add_parentage(world, template, seed="relative-first-late", child_id=source,
                                parent_id=late_frontier, tick=-19)
    _add_parentage(world, template, seed="relative-second-late", child_id=early_frontier,
                   parent_id=destination, tick=-1)
    late_second = _add_parentage(world, template, seed="relative-second-early", child_id=late_frontier,
                                 parent_id=destination, tick=-2)
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        result = _run(connection, _scope(), "relatives", source,
                      target_id=destination, depth=2, items=8)
        assert result["state"] == "available"
        assert [edge["recordId"] for edge in result["relations"][0]["edges"]] == [
            late_first.id, late_second.id]
    finally:
        connection.close()


def test_relative_path_work_stays_bounded_at_high_degree() -> None:
    measurements = []
    for extra in (2, 200):
        world, mapping = _world()
        template = next(record for record in world.records.values() if record.kind == "parentage")
        source = mapping["character_no_lineage"]
        destination = id_from_seed("character", "relative-degree-target")
        first = _add_parentage(world, template, seed="relative-degree-first",
                                child_id=source, parent_id=destination, tick=-20)
        for number in range(extra):
            _add_parentage(world, template, seed=f"relative-degree-extra-{number}",
                           child_id=source,
                           parent_id=id_from_seed("character", f"relative-degree-neighbor-{number}"),
                           tick=-10)
        connection = _connection(world, StoryTime("main", 0, 0))
        try:
            candidates = frozenset(row[0] for row in connection.execute(
                "SELECT id FROM generational_record"))
            steps = 0

            def progress():
                nonlocal steps
                steps += 1
                return 0

            connection.set_progress_handler(progress, 1)
            result = cited_relative_path(connection, source, destination,
                                         at=StoryTime("main", 0, 0),
                                         candidate_ids=candidates, max_depth=1,
                                         max_items=1)
            connection.set_progress_handler(None, 0)
            assert result["status"] == "ok"
            assert [edge["edgeId"] for edge in result["paths"][0]["edges"]] == [first.id]
            first_steps = steps
            steps = 0
            connection.set_progress_handler(progress, 1)
            absent = cited_relative_path(connection, source,
                                         id_from_seed("character", "relative-degree-absent"),
                                         at=StoryTime("main", 0, 0),
                                         candidate_ids=candidates, max_depth=1,
                                         max_items=1)
            connection.set_progress_handler(None, 0)
            assert absent == {"status": "limit", "paths": []}
            measurements.append((first_steps, steps))
        finally:
            connection.close()
    assert all(abs(later - earlier) < 100 for earlier, later in
               zip(measurements[0], measurements[1])), measurements


def test_ancestor_and_descendant_work_stays_bounded_at_high_degree() -> None:
    measurements = []
    for degree in (2, 200):
        world, mapping = _world()
        template = next(record for record in world.records.values() if record.kind == "parentage")
        child = mapping["character_no_lineage"]
        parent = id_from_seed("character", "degree-descendant-root")
        for number in range(degree):
            _add_parentage(world, template, seed=f"degree-ancestor-{number}",
                           child_id=child,
                           parent_id=id_from_seed("character", f"degree-ancestor-parent-{number}"),
                           tick=-10)
            _add_parentage(world, template, seed=f"degree-descendant-{number}",
                           child_id=id_from_seed("character", f"degree-descendant-child-{number}"),
                           parent_id=parent, tick=-10)
        connection = _connection(world, StoryTime("main", 0, 0))
        try:
            candidates = frozenset(row[0] for row in connection.execute(
                "SELECT id FROM generational_record"))
            pair = []
            for fn, source in ((cited_ancestors, child), (cited_descendants, parent)):
                steps = 0

                def progress():
                    nonlocal steps
                    steps += 1
                    return 0

                connection.set_progress_handler(progress, 1)
                result = fn(connection, source, at=StoryTime("main", 0, 0),
                            candidate_ids=candidates, max_depth=1, max_items=1)
                connection.set_progress_handler(None, 0)
                assert result == {"status": "limit", "paths": []}
                pair.append(steps)
            measurements.append(pair)
        finally:
            connection.close()
    assert all(abs(later - earlier) < 100 for earlier, later in
               zip(measurements[0], measurements[1])), measurements


def test_containment_one_parent_fits_one_item() -> None:
    world, _mapping = _world()
    child = next(record.id for record in world.records.values() if record.title == "Cadet House")
    parent = next(record.id for record in world.records.values() if record.title == "House Aster")
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        candidates = frozenset(row[0] for row in connection.execute(
            "SELECT id FROM generational_record"))
        result = cited_containment(connection, child, at=StoryTime("main", 0, 0),
                                   candidate_ids=candidates, max_depth=2, max_items=1)
        assert result["status"] == "ok"
        assert [path["targetId"] for path in result["paths"]] == [parent]
        public = _scope(audiences=frozenset({"public"}), perspectives=frozenset({"ordinary"}))
        assert _run(connection, public, "organization", child, items=1)["state"] == "available"
    finally:
        connection.close()


def test_parent_item_limit_counts_only_active_authorized_edges() -> None:
    world, mapping = _world()
    template = next(record for record in world.records.values() if record.kind == "parentage")
    child = mapping["character_child"]
    for number in range(4):
        record = _add_parentage(world, template, seed=f"ended-query-{number}",
                                child_id=child, parent_id=id_from_seed("character", f"ended-parent-{number}"),
                                tick=-9)
        record.frontmatter["transitions"].append({
            "transition_id": id_from_seed("generational-transition", f"ended-query-{number}-end"),
            "transition_kind": "parentage-end",
            "applicability": {"applicability_kind": "instant", "point": {
                "timeline": "main", "tick": -1, "order": 0}}, "payload": {}})
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        result = _run(connection, _scope(), "parents", child, items=2)
        assert result["state"] == "available" and len(result["relations"]) == 2
    finally:
        connection.close()


def test_private_search_pages_distinct_records_after_many_matching_transitions() -> None:
    world, _mapping = _world()
    first = next(record for record in world.records.values() if record.title == "House Aster")
    second = next(record for record in world.records.values() if record.title == "Cadet House")
    for number in range(505):
        first.frontmatter["transitions"].append({
            "transition_id": id_from_seed("generational-transition", f"search-rename-{number}"),
            "transition_kind": "organization-rename",
            "applicability": {"applicability_kind": "instant", "point": {
                "timeline": "main", "tick": number + 1, "order": 0}},
            "payload": {"title": f"queryneedle-{number}", "aliases": []}})
    second.frontmatter["initialization"]["payload"]["title"] = "queryneedle-second"
    connection = _connection(world, StoryTime("main", 600, 0))
    try:
        scope = _scope(at=StoryTime("main", 600, 0))
        statements = []
        connection.set_trace_callback(statements.append)
        first_page = _run(connection, scope, "search", "ignored", text="queryneedle", items=1)
        assert first_page["state"] == "available" and len(first_page["results"]) == 1
        assert first_page["cursor"]
        second_page = _run(connection, scope, "search", "ignored", text="queryneedle",
                           items=1, cursor=first_page["cursor"])
        assert second_page["state"] == "available" and len(second_page["results"]) == 1
        assert first_page["results"][0]["recordId"] != second_page["results"][0]["recordId"]
        assert second_page["cursor"] is None
        assert any("INDEXED BY generational_search_lookup_idx" in sql
                   and "ORDER BY start_tick,start_order,source_ordinal,record_id" in sql
                   for sql in statements)
        assert not any("GROUP BY" in sql for sql in statements)
    finally:
        connection.close()


def test_character_malformed_search_and_relative_target_are_invalid() -> None:
    world, mapping = _world()
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        scope = _scope(mode="character")
        assert _run(connection, scope, "search", "ignored", text="")["state"] == "invalid"
        assert _run(connection, scope, "relatives", mapping["character_child"],
                    target_id="bad")["state"] == "invalid"
        assert _run(connection, scope, "relatives", mapping["character_child"],
                    target_id=mapping["character_biological"])["state"] == "unknown"
    finally:
        connection.close()


def test_character_and_public_leak_matrix_for_claim_role_vital_and_search() -> None:
    world, mapping = _world()
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        legacy = next(record.id for record in world.records.values() if record.kind == "legacy")
        organization = next(record.id for record in world.records.values()
                            if record.title == "Cadet House")
        public = _scope(audiences=frozenset({"public"}),
                        perspectives=frozenset({"ordinary"}))
        council = _scope()
        public_legacy = _run(connection, public, "legacy", legacy)
        assert public_legacy["claims"] == []
        assert len(_run(connection, council, "legacy", legacy)["claims"]) == 2
        assert _run(connection, public, "organization", organization)["roles"] == []
        assert len(_run(connection, council, "organization", organization)["roles"]) == 1
        secret_parentage = next(record.id for record in world.records.values()
                                if record.title == "Sealed lineage")
        assert secret_parentage not in canonical_json(public_legacy)
        assert _run(connection, public, "search", "ignored", text="character_secret") == {
            "state": "available", "results": [], "cursor": None}
        character = _scope(mode="character")
        for operation, subject in (("parents", mapping["character_secret"]),
                                   ("vital", mapping["character_child"]),
                                   ("legacy", legacy), ("organization", organization)):
            assert _run(connection, character, operation, subject) == {"state": "unknown"}
    finally:
        connection.close()


def test_source_compiled_and_rebuilt_query_parity(tmp_path: Path, monkeypatch) -> None:
    world, organization, *_ = _former_role_world()
    world.config["current_time"] = {"timeline": "main", "tick": 0, "order": 1}
    _, mapping = _world()
    at = StoryTime("main", 0, 0)
    scope = _scope(at=StoryTime("main", 0, 1))
    requests = [
        {"operation": "parents", "subject_id": mapping["character_child"]},
        {"operation": "vital", "subject_id": mapping["character_child"]},
        {"operation": "search", "text": "basis", "items": 2},
        {"operation": "organization", "subject_id": organization,
         "includeFormerRoles": True},
    ]
    source = _connection(world, at)
    try:
        _seed_former_role_cause(source, world)
        expected = [query_connection(source, scope, request) for request in requests]
    finally:
        source.close()
    class PinnedRepository:
        root = tmp_path
        source_root = "story"
        _compiled_world_cache = {}
        last_load_stats = {"mode": "fixture", "parsed": len(world.records),
                           "cacheHits": 0, "blobReads": 0}
        revision = "vector-revision"

        def resolve(self, revision):
            return self.revision if revision == "HEAD" else revision

        def tree_oid(self, revision):
            return "vector-tree" if revision == "vector-revision" else "vector-tree-next"

        def load_world(self, revision, *, cache_write=False):
            return replace(world, revision=revision, tree_oid=self.tree_oid(revision))

        def is_ancestor(self, older, newer):
            return older == "vector-revision" and newer == "vector-revision-next"

        def changed_paths(self, older, newer):
            return ["story/kinships/fixture.md"]

    repository = PinnedRepository()
    first = compile_world(repository, profile_name="fts")
    assert first["buildMode"] == "full"
    database = Path(first["database"])
    assert [query_generational(repository, scope, request) for request in requests] == expected
    implicit_scope = replace(scope, at=None)
    assert query_generational(repository, implicit_scope, requests[-1]) == {
        "state": "invalid", "code": "GEN-REQUEST-001"}
    assert query_generational(repository, implicit_scope,
                              {"operation": "organization", "subject_id": organization})["state"] == "available"

    repository.revision = "vector-revision-next"
    later_scope = replace(scope, revision=repository.revision)
    second = compile_world(repository)
    assert second["buildMode"] == "fast-forward-rebuild"
    def comparable(result):
        return {key: (bool(value) if key == "cursor" else value)
                for key, value in result.items()}

    assert [comparable(query_generational(repository, later_scope, request))
            for request in requests] == [comparable(result) for result in expected]

    connection = sqlite3.connect(database)
    try:
        connection.execute("DROP INDEX generational_search_lookup_idx")
        connection.commit()
    finally:
        connection.close()
    assert "missingIndex:generational_search_lookup_idx" in _compiled_database_issues(database)
    from wedl import compiler
    original_replace = compiler.os.replace
    replacements = []

    def observed_replace(source, target):
        assert Path(source).exists() and Path(target).exists()
        replacements.append((Path(source), Path(target)))
        return original_replace(source, target)

    monkeypatch.setattr(compiler.os, "replace", observed_replace)
    repaired = compile_world(repository)
    assert repaired["status"] != "cache-hit" and replacements
    assert replacements[-1][1] == database and replacements[-1][0] != database
    assert _compiled_database_issues(database) == ()
    assert [comparable(query_generational(repository, later_scope, request))
            for request in requests] == [comparable(result) for result in expected]


class _EntryRepository:
    """Source-free repository with mutable refs and independently mutable trees."""

    source_root = "story"

    def __init__(self, root, world):
        self.root = root
        self.world = world
        self.revision = world.revision
        self.trees = {world.revision: world.tree_oid,
                      "vector-revision-next": "vector-tree-next",
                      "WORKTREE": "worktree-tree"}
        self.refs = {"branch": self.revision, "short": self.revision}
        self._compiled_world_cache = {}
        self.calls = []
        self.last_load_stats = {"mode": "fixture", "parsed": len(world.records),
                                "cacheHits": 0, "blobReads": 0}

    def resolve(self, revision):
        self.calls.append(("resolve", revision))
        resolved = self.revision if revision == "HEAD" else self.refs.get(revision, revision)
        if resolved not in self.trees:
            raise ValueError("revision is unavailable")
        return resolved

    def tree_oid(self, revision):
        self.calls.append(("tree", revision))
        return self.trees[revision]

    def load_world(self, revision, *, cache_write=False):
        return replace(self.world, revision=revision, tree_oid=self.tree_oid(revision))

    def is_ancestor(self, older, newer):
        return older == "vector-revision" and newer == "vector-revision-next"

    def changed_paths(self, older, newer):
        return ["story/kinships/fixture.md"]


def _entry_fixture(tmp_path):
    world, mapping = _world()
    repository = _EntryRepository(tmp_path, world)
    compiled = compile_world(repository, profile_name="fts")
    repository.calls.clear()
    return repository, Path(compiled["database"]), mapping


def _pinned_git_entry_fixture(tmp_path, monkeypatch):
    from wedl import compiler
    from wedl.repository import Repository

    # Git is read-only here; the compiled vector and every cache write belong
    # to tmp_path. This also works when the test source is in a Git worktree.
    repository = Repository(Path(__file__).resolve().parents[1])
    assert repository.is_git, "the real-Git lookup regression requires a Git checkout"
    revision = repository.head()
    tree = repository.tree_oid(revision)
    fake, database, mapping = _entry_fixture(tmp_path)
    with closing(sqlite3.connect(database)) as connection:
        connection.execute("UPDATE revision SET head_commit=?,tree_oid=?", (revision, tree))
        connection.commit()
    paths = compiler.cache_paths(fake)
    monkeypatch.setattr(compiler, "cache_paths", lambda _repository: paths)
    return repository, database, mapping, replace(_scope(), revision=revision), tree


def test_pinned_git_entry_pairs_lookup_and_checks_integrity_each_read(tmp_path, monkeypatch):
    from wedl import compiler

    repository, database, mapping, scope, tree = _pinned_git_entry_fixture(tmp_path, monkeypatch)
    calls = []
    original_git = repository._git
    original_check = compiler._compiled_database_issues
    checked = []

    def observed_git(args, **kwargs):
        calls.append(list(args))
        return original_git(args, **kwargs)

    def observed_check(path):
        checked.append(path)
        return original_check(path)

    monkeypatch.setattr(repository, "_git", observed_git)
    monkeypatch.setattr(compiler, "_compiled_database_issues", observed_check)
    requests = [{"operation": "parents", "subject_id": mapping["character_child"]},
                {"operation": "ancestors", "subject_id": mapping["character_child"]},
                {"operation": "vital", "subject_id": mapping["character_child"]},
                {"operation": "search", "text": "character", "items": 2}]
    before = database.read_bytes()
    with closing(sqlite3.connect(database)) as connection:
        connection.row_factory = sqlite3.Row
        for viewer in (scope, replace(_scope(mode="character"), revision=scope.revision)):
            for request in requests:
                expected = query_connection(connection, viewer, request)
                calls.clear()
                assert query_generational(repository, viewer, request, require_compiled=True) == expected
                assert calls == [["rev-parse", scope.revision,
                                  f"{scope.revision}^{{tree}}"]]
    assert checked == [database] * 8
    assert database.read_bytes() == before
    # Source reads use the same validated pair, including tree objects, without
    # writing a source cache into this real, read-only checkout.
    for target in (scope.revision, tree):
        calls.clear()
        source_world = repository.load_world(target, cache_write=False)
        assert (source_world.revision, source_world.tree_oid) == (target, tree)
        assert source_world.root == repository.root
        assert calls[0] == ["rev-parse", target, f"{target}^{{tree}}"]
        assert sum(args[0] == "rev-parse" for args in calls) == 1
    # A tree object was historically accepted: do not narrow to ^{commit}.
    assert repository._resolve_pinned_target(tree) == (tree, tree)
    with closing(sqlite3.connect(database)) as connection:
        connection.execute("DROP INDEX generational_search_lookup_idx")
        connection.commit()
    damaged = database.read_bytes()
    assert query_generational(repository, scope, requests[0], require_compiled=True) == {"state": "unavailable"}
    assert checked == [database] * 9 and database.read_bytes() == damaged


def test_pinned_git_entry_closes_failed_and_malformed_target_replies(tmp_path, monkeypatch):
    from wedl import compiler
    from wedl.errors import RepositoryError

    repository, database, mapping, scope, tree = _pinned_git_entry_fixture(tmp_path, monkeypatch)
    request = {"operation": "parents", "subject_id": mapping["character_child"]}
    before = database.read_bytes()
    original_git = repository._git
    assert query_generational(repository, replace(scope, revision="0" * 40), request) == {"state": "unavailable"}

    def forbidden(*args, **kwargs):
        pytest.fail("invalid Git target reached readiness or rebuild")

    monkeypatch.setattr(compiler, "_cache_readiness_for_resolved_revision", forbidden)
    monkeypatch.setattr(compiler, "compile_world", forbidden)
    monkeypatch.setattr(repository, "_tree_entries", forbidden)
    monkeypatch.setattr(repository, "_source_cache", forbidden)
    replies = [(1, f"{scope.revision}\n{tree}\n".encode()), (0, b""),
               (0, f"{scope.revision}\n".encode()),
               (0, f"{scope.revision}\n{tree}\n{tree}\n".encode()),
               (0, f"{'f' * 40}\n{tree}\n".encode()),
               (0, f"{scope.revision}\nnot-an-oid\n".encode()),
               (0, f"{scope.revision}\n{tree.upper()}\n".encode()),
               (0, b"\xff\n\xff\n")]
    for returncode, stdout in replies:
        calls = []

        def reply(args, **kwargs):
            calls.append(list(args))
            return subprocess.CompletedProcess(args, returncode, stdout, b"failure")

        monkeypatch.setattr(repository, "_git", reply)
        assert query_generational(repository, scope, request) == {"state": "unavailable"}
        assert len(calls) == 1 and database.read_bytes() == before
        calls.clear()
        # Paired ASCII/two-line validation is deliberately stricter than the
        # historical independent decode/strip calls; no old error text promised.
        with pytest.raises(RepositoryError):
            repository.load_world(scope.revision)
        assert calls == [["rev-parse", scope.revision, f"{scope.revision}^{{tree}}"]]
        assert database.read_bytes() == before

    for error in (RepositoryError("Git failed"), OSError("Git missing")):
        def failed(*args, **kwargs):
            raise error

        monkeypatch.setattr(repository, "_git", failed)
        assert query_generational(repository, scope, request) == {"state": "unavailable"}
        with pytest.raises(type(error)) as failure:
            repository.load_world(scope.revision)
        assert failure.value is error
    monkeypatch.setattr(repository, "_git", original_git)
    assert database.read_bytes() == before


def test_repository_entry_resolves_once_and_preserves_ready_answers(tmp_path: Path, monkeypatch) -> None:
    from wedl import compiler

    repository, database, mapping = _entry_fixture(tmp_path)
    requests = [
        {"operation": "parents", "subject_id": mapping["character_child"]},
        {"operation": "ancestors", "subject_id": mapping["character_child"]},
        {"operation": "vital", "subject_id": mapping["character_child"]},
        {"operation": "search", "text": "character", "items": 2},
    ]
    checked = []
    original_check = compiler._compiled_database_issues

    def observed_check(path):
        checked.append(path)
        return original_check(path)

    monkeypatch.setattr(compiler, "_compiled_database_issues", observed_check)
    with closing(sqlite3.connect(database)) as connection:
        connection.row_factory = sqlite3.Row
        for scope in (_scope(), _scope(mode="character")):
            for request in requests:
                expected = query_connection(connection, scope, request)
                repository.calls.clear()
                assert query_generational(repository, scope, request, require_compiled=True) == expected
                assert repository.calls == [("resolve", scope.revision), ("tree", scope.revision)]
    assert checked == [database] * (2 * len(requests))


def test_repository_entry_rejects_alias_and_missing_revision(tmp_path: Path, monkeypatch) -> None:
    from wedl import generational_query

    repository, database, mapping = _entry_fixture(tmp_path)
    original = database.read_bytes()
    request = {"operation": "parents", "subject_id": mapping["character_child"]}

    def unexpected_readiness(*args, **kwargs):
        pytest.fail("rejected revision reached cache readiness")

    monkeypatch.setattr(generational_query, "_require_database_for_resolved_revision", unexpected_readiness)
    for revision in ("HEAD", "branch", "short"):
        repository.calls.clear()
        assert query_generational(repository, replace(_scope(), revision=revision), request) == {
            "state": "invalid", "code": "GEN-REQUEST-001"}
        assert repository.calls == [("resolve", revision)]
    assert query_generational(repository, replace(_scope(), revision="missing"), request) == {"state": "unavailable"}

    def unavailable(revision):
        raise OSError("unavailable repository")

    monkeypatch.setattr(repository, "resolve", unavailable)
    assert query_generational(repository, _scope(), request) == {"state": "unavailable"}
    assert database.read_bytes() == original


def test_repository_entry_target_changes_remain_fresh(tmp_path: Path, monkeypatch) -> None:
    from wedl import compiler
    from wedl.errors import CompileRequired

    repository, database, mapping = _entry_fixture(tmp_path)
    request = {"operation": "parents", "subject_id": mapping["character_child"]}
    assert query_generational(repository, _scope(), request, require_compiled=True)["state"] == "available"
    repository.trees[repository.revision] = "changed-tree"
    assert compiler.cache_readiness(repository)["state"] == "stale"
    assert query_generational(repository, _scope(), request, require_compiled=True) == {"state": "unavailable"}
    with pytest.raises(CompileRequired):
        compiler.require_database(repository, "branch", require_compiled=True)
    compiler.require_database(repository, "branch")
    assert compiler.cache_readiness(repository, "branch")["state"] == "ready"

    repository.revision = "vector-revision-next"
    repository.refs["branch"] = repository.revision
    for ref in ("HEAD", "branch"):
        assert compiler.cache_readiness(repository, ref)["target"]["revision"] == repository.revision
        with pytest.raises(CompileRequired):
            compiler.require_database(repository, ref, require_compiled=True)
    compiler.require_database(repository, "HEAD")
    assert compiler.cache_readiness(repository, "branch")["state"] == "ready"
    assert query_generational(repository, replace(_scope(), revision=repository.revision), request,
                              require_compiled=True)["state"] == "available"

    for tree in ("worktree-tree", "worktree-changed"):
        repository.trees["WORKTREE"] = tree
        assert compiler.cache_readiness(repository, "WORKTREE")["state"] == "stale"
        compiler.require_database(repository, "WORKTREE")
        assert query_generational(repository, replace(_scope(), revision="WORKTREE"), request,
                                  require_compiled=True)["state"] == "available"

    original_compile = compiler.compile_world

    def moving_compile(repo, revision):
        result = original_compile(repo, revision)
        repo.revision = "vector-revision"
        repo.refs["branch"] = repo.revision
        return result

    monkeypatch.setattr(compiler, "compile_world", moving_compile)
    for ref in ("HEAD", "branch"):
        repository.revision = "vector-revision-next"
        repository.refs["branch"] = repository.revision
        # The database remains WORKTREE, so each public call must rebuild then
        # check its original mutable request again after the ref moves.
        with pytest.raises(RuntimeError, match="did not produce a ready database: stale"):
            compiler.require_database(repository, ref)
        original_compile(repository, "WORKTREE")


def test_repository_entry_missing_stale_damaged_cache_stays_fail_closed(tmp_path: Path, monkeypatch) -> None:
    from wedl import compiler

    repository, database, mapping = _entry_fixture(tmp_path)
    request = {"operation": "parents", "subject_id": mapping["character_child"]}
    expected = query_generational(repository, _scope(), request, require_compiled=True)
    original_compile = compiler.compile_world
    rebuilds = []

    def observed_compile(repo, revision):
        rebuilds.append(revision)
        return original_compile(repo, revision)

    monkeypatch.setattr(compiler, "compile_world", observed_compile)
    original_tree_oid = repository.tree_oid
    original_database = database.read_bytes()
    for error_type in (ValueError, OSError):
        def unavailable_tree(revision):
            repository.calls.append(("tree", revision))
            raise error_type("resolved target tree is unavailable")

        monkeypatch.setattr(repository, "tree_oid", unavailable_tree)
        repository.calls.clear()
        assert query_generational(repository, _scope(), request, require_compiled=True) == {"state": "unavailable"}
        assert repository.calls == [("resolve", repository.revision), ("tree", repository.revision)]
        assert rebuilds == [] and database.read_bytes() == original_database
    monkeypatch.setattr(repository, "tree_oid", original_tree_oid)
    for state in ("missing", "stale", "incompatible"):
        if state == "missing":
            database.unlink()
        elif state == "stale":
            repository.trees[repository.revision] = "another-tree"
        else:
            with closing(sqlite3.connect(database)) as connection:
                connection.execute("DROP INDEX generational_search_lookup_idx")
        assert compiler.cache_readiness(repository)["state"] == state
        before = database.read_bytes() if database.exists() else None
        previous_rebuilds = list(rebuilds)
        assert query_generational(repository, _scope(), request, require_compiled=True) == {"state": "unavailable"}
        assert rebuilds == previous_rebuilds
        assert (database.read_bytes() if database.exists() else None) == before
        assert query_generational(repository, _scope(), request) == expected
        assert rebuilds == previous_rebuilds + [repository.revision]
        assert compiler.cache_readiness(repository)["state"] == "ready"


def test_vital_direct_and_search_agree_before_birth_and_when_withheld() -> None:
    world, mapping = _world()
    vital = next(record for record in world.records.values()
                 if record.kind == "vital-history" and record.frontmatter["character_id"] == mapping["character_child"])
    before = _scope(at=StoryTime("main", -10, -1))
    born = _scope(at=StoryTime("main", -10, 0))
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        assert _run(connection, before, "vital", mapping["character_child"]) == {"state": "unknown"}
        assert vital.id not in canonical_json(_run(connection, before, "search", "ignored", text="vital"))
        assert _run(connection, born, "vital", mapping["character_child"])["vital"] == "living"
        assert vital.id in [item["recordId"] for item in
                            _run(connection, born, "search", "ignored", text="vital")["results"]]
    finally:
        connection.close()
    vital.frontmatter["disclosure"] = "withheld"
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        assert _run(connection, born, "vital", mapping["character_child"]) == {"state": "unknown"}
        assert vital.id not in canonical_json(_run(connection, born, "search", "ignored", text="vital"))
    finally:
        connection.close()


def test_union_history_and_active_roster_obey_items_limit() -> None:
    world, mapping = _world()
    union = next(record.id for record in world.records.values() if record.kind == "union")
    organization = next(record.id for record in world.records.values() if record.title == "Cadet House")
    template = next(record for record in world.records.values() if record.kind == "affiliation")
    for number in range(3):
        data = deepcopy(template.frontmatter)
        data["id"] = id_from_seed("affiliation", f"ended-roster-{number}")
        data["initialization"]["transition_id"] = id_from_seed("generational-transition", f"ended-roster-{number}-init")
        data["initialization"]["applicability"]["point"]["tick"] = -5
        data["transitions"] = [{"transition_id": id_from_seed("generational-transition", f"ended-roster-{number}-end"),
                                 "transition_kind": "affiliation-end", "applicability": {
                                     "applicability_kind": "instant", "point": {"timeline": "main", "tick": -2, "order": 0}},
                                 "payload": {}}]
        record = Record(data, "", f"story/affiliations/{data['id']}.md", b"")
        world.records[record.id] = record
    connection = _connection(world, StoryTime("main", 0, 0))
    try:
        assert _run(connection, _scope(), "union", union, items=2) == {
            "state": "limit", "code": "GEN-LIMIT-001"}
        assert len(_run(connection, _scope(), "union", union, items=3)["participants"]) == 3
        assert len(_run(connection, _scope(), "organization", organization, items=2)["roles"]) == 1
        alltime = replace(_scope(), mode="author-all-time", at=None)
        assert _run(connection, alltime, "union", union, items=1) == {
            "state": "limit", "code": "GEN-LIMIT-001"}
        assert _run(connection, alltime, "parents", mapping["character_child"], items=1) == {
            "state": "limit", "code": "GEN-LIMIT-001"}
    finally:
        connection.close()


def test_private_search_index_has_bounded_work_and_distinct_cap() -> None:
    world, mapping = _world()
    first = next(record for record in world.records.values() if record.title == "House Aster")
    scope = _scope(at=StoryTime("main", 3000, 0), audiences=frozenset({"public"}),
                   perspectives=frozenset({"ordinary"}))
    measurements = []
    for count in (0, 505, 2020):
        first.frontmatter["transitions"] = [
            {"transition_id": id_from_seed("generational-transition", f"unmatched-{number}"),
             "transition_kind": "organization-rename", "applicability": {
                 "applicability_kind": "instant", "point": {"timeline": "main", "tick": number + 1, "order": 0}},
             "payload": {"title": f"unrelated-{number}", "aliases": []}}
            for number in range(count)]
        connection = _connection(world, StoryTime("main", 3000, 0))
        try:
            steps = 0

            def progress():
                nonlocal steps
                steps += 1
                return 0

            connection.set_progress_handler(progress, 1)
            result = _run(connection, scope, "search", "ignored", text="absentterm", items=1)
            connection.set_progress_handler(None, 0)
            assert result == {"state": "available", "results": [], "cursor": None}
            measurements.append(steps)
        finally:
            connection.close()
    assert max(measurements) - min(measurements) < 20, measurements
    template = next(record for record in world.records.values() if record.kind == "parentage")
    connection = _connection(world, StoryTime("main", 3000, 0))
    try:
        baseline_steps = 0

        def baseline_progress():
            nonlocal baseline_steps
            baseline_steps += 1
            return 0

        connection.set_progress_handler(baseline_progress, 1)
        baseline_page = _run(connection, scope, "search", "ignored", text="basis", items=1)
        connection.set_progress_handler(None, 0)
    finally:
        connection.close()
    for number in range(505):
        hidden = _add_parentage(world, template, seed=f"hidden-search-{number}",
                                child_id=mapping["character_child"],
                                parent_id=mapping["character_biological"], tick=-9)
        hidden.frontmatter["audience"] = ["archivist"]
    connection = _connection(world, StoryTime("main", 3000, 0))
    try:
        steps = 0

        def progress():
            nonlocal steps
            steps += 1
            return 0

        connection.set_progress_handler(progress, 1)
        assert _run(connection, scope, "search", "ignored", text="basis", items=1) == baseline_page
        connection.set_progress_handler(None, 0)
        assert abs(steps - baseline_steps) < 20, (steps, baseline_steps)
    finally:
        connection.close()
    for number in range(501):
        _add_parentage(world, template, seed=f"authorized-cap-{number}",
                       child_id=mapping["character_child"],
                       parent_id=mapping["character_biological"], tick=-9)
    connection = _connection(world, StoryTime("main", 3000, 0))
    try:
        large = _run(connection, scope, "search", "ignored", text="basis", items=1)
        assert large["state"] == "available" and len(large["results"]) == 1
        assert large["cursor"]
    finally:
        connection.close()
