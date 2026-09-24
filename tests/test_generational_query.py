"""Privacy, exact horizon, and indexed generational read regressions."""
from __future__ import annotations

from dataclasses import replace
from copy import deepcopy
from pathlib import Path
import sqlite3

from test_generational_compiler import _add_parentage, _connection as _unindexed_connection, _world
from wedl.compiler import INDEX_DDL, compile_world, _compiled_database_issues
from wedl.generational_query import TrustedViewerScope, query_connection, query_generational
from wedl.ids import id_from_seed
from wedl.generational_index import cited_ancestors, cited_containment, cited_descendants, cited_relative_path
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
                   and "LIMIT 501" in sql
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
    world, mapping = _world()
    at = StoryTime("main", 0, 0)
    scope = _scope()
    requests = [
        {"operation": "parents", "subject_id": mapping["character_child"]},
        {"operation": "vital", "subject_id": mapping["character_child"]},
        {"operation": "search", "text": "basis", "items": 2},
    ]
    source = _connection(world, at)
    try:
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
        assert _run(connection, scope, "search", "ignored", text="basis", items=1) == {
            "state": "limit", "code": "GEN-LIMIT-001"}
    finally:
        connection.close()
