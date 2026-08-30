"""Source/SQLite parity for the disposable v0.6 chronology read model."""
from pathlib import Path
import sqlite3
import json
import inspect
from copy import deepcopy
from dataclasses import replace
from tempfile import TemporaryDirectory
import importlib.util
import pytest

from wedl.chronology import ApproximateDate, CivilDate, CivilRange, EraDate
from wedl.chronology_index import ComparisonKind, build_chronology_projection, describe_date_value, insert_chronology_index, kernelize_date_value, load_chronology_projection
from wedl.chronology_query import (AnnotationQuery, ChronologyPredicate,
    EraMatchMode, SQLiteChronologyStore, SourceChronologyStore,
    ChronologyReason, ConversionRequest, convert_chronology_date,
    format_chronology_date, map_chronology_date_to_story_time, query_annotations)
from wedl.compiler import DDL, INDEX_DDL
from wedl.model import Record, World
from wedl.ids import id_from_seed
from wedl.validation import validate_world


_BENCHMARK_SPEC = importlib.util.spec_from_file_location(
    "chronology_index_benchmark", Path(__file__).parents[1] / "tools" / "benchmark_chronology_index.py"
)
assert _BENCHMARK_SPEC and _BENCHMARK_SPEC.loader
_BENCHMARK = importlib.util.module_from_spec(_BENCHMARK_SPEC)
_BENCHMARK_SPEC.loader.exec_module(_BENCHMARK)
run_benchmark = _BENCHMARK.run_benchmark


CAL = "calendar_0123456789abcdefghjkmnpqrs"
ISOLATED = "calendar_1123456789abcdefghjkmnpqrs"
ERA = "era_0123456789abcdefghjkmnpqrs"


def _record(data, path):
    return Record(data, "", path, b"")


def _world():
    world = _record({
        "schema": "wedl/v0.6", "id": "world_0123456789ABCDEFGHJKMNPQRS",
        "kind": "world", "status": "canonical", "title": "World", "domain": "main",
        "tags": [], "aliases": [], "threads": [], "timelines": [{"id": "main", "label": "Main"}],
        "chronology": {
            "calendars": [
                {"id": CAL, "label": "Shared", "months": [{"number": 1, "days": 30}],
                 "rule": {"kind": "cycle", "period": 1, "overrides": []},
                 "epoch": {"civil": {"year": 0, "month": 1, "day": 1}, "axis_day": 0}},
                {"id": ISOLATED, "label": "Local", "months": [{"number": 1, "days": 30}],
                 "rule": {"kind": "table", "years": [{"year": 0, "overrides": []}]}, "epoch": None},
            ],
            "eras": [{"id": ERA, "calendar_id": CAL, "label": "Common", "aliases": [],
                      "display_year_zero": True, "display_epoch": {"display_year": 0, "machine_year": 0},
                      "bounds": {"lower": {"year": 0, "month": 1, "day": 1}, "upper": {"year": 0, "month": 1, "day": 10}}, "provenance": ["ledger"]}],
            "anchors": [{"id": "chronology_0123456789abcdefghjkmnpqrs", "axis_day": 0,
                         "story_time": {"timeline": "main", "tick": 0, "order": 0}, "provenance": ["ledger"]}],
        },
    }, "story/world.md")
    character = _record({
        "schema": "wedl/v0.6", "id": "char_0123456789ABCDEFGHJKMNPQRS", "kind": "character",
        "status": "canonical", "title": "Character", "domain": "main", "tags": [], "aliases": [],
        "chronology": [
            {"id": "chronology_1123456789abcdefghjkmnpqrs", "provenance": ["ledger"], "value": {"civil": {"calendar_id": CAL, "year": 0, "month": 1, "day": 1}}},
            {"id": "chronology_2123456789abcdefghjkmnpqrs", "provenance": ["ledger"], "value": {"era": {"era_id": ERA, "year": 0, "month": 1, "day": 5}}},
            {"id": "chronology_3123456789abcdefghjkmnpqrs", "provenance": ["ledger"], "value": {"civil": {"calendar_id": ISOLATED, "year": 0, "month": 1, "day": 2}}},
            {"id": "chronology_4123456789abcdefghjkmnpqrs", "provenance": ["ledger"], "value": {"range": {"calendar_id": ISOLATED, "lower": {"calendar_id": ISOLATED, "year": 0, "month": 1, "day": 2}, "upper": {"calendar_id": ISOLATED, "year": 0, "month": 1, "day": 3}}}},
            {"id": "chronology_5123456789abcdefghjkmnpqrs", "provenance": ["ledger"], "value": {"approx": {"display_value": "about", "bounds": {"lower": {"calendar_id": ISOLATED, "year": 0, "month": 1, "day": 2}, "upper": {"calendar_id": ISOLATED, "year": 0, "month": 1, "day": 3}}}}},
        ],
    }, "story/character.md")
    return World("WORKTREE", "tree", {world.id: world, character.id: character}, Path("."), is_worktree=True)


def _sqlite_store(projection):
    connection = sqlite3.connect(":memory:")
    connection.row_factory = sqlite3.Row
    connection.executescript(DDL)
    connection.executescript(INDEX_DDL)
    for row in {row.record_id: row for row in projection.annotations}.values():
        connection.execute("INSERT INTO entity VALUES (?,?,?,?,?,?,?,?,?)", (row.record_id, "character", "Character", "main", "canonical", f"story/{row.record_id}.md", None, "", "{}"))
    insert_chronology_index(connection, projection)
    connection.commit()
    return connection, SQLiteChronologyStore(connection, "WORKTREE")


def test_isolated_calendar_intervals_and_sqlite_share_one_evaluator():
    world = _world()
    assert validate_world(world) == []
    projection = build_chronology_projection(world, validated=True)
    isolated = [row for row in projection.annotations if row.calendar_id == ISOLATED]
    assert [(row.basis_id, row.lower_day, row.upper_day) for row in isolated[:2]] == [(f"calendar:{ISOLATED}", 1, 1), (f"calendar:{ISOLATED}", 1, 2)]
    source = SourceChronologyStore(projection, "WORKTREE")
    connection, compiled = _sqlite_store(projection)
    try:
        request = AnnotationQuery(ChronologyPredicate.OVERLAPS, CivilDate(ISOLATED, 0, 1, 2))
        source_result, compiled_result = query_annotations(source, request), query_annotations(compiled, request)
        assert source_result == compiled_result
        assert [item.annotation_id for item in source_result.value.matches] == ["chronology_3123456789abcdefghjkmnpqrs", "chronology_4123456789abcdefghjkmnpqrs", "chronology_5123456789abcdefghjkmnpqrs"]
        assert {item.kind for item in source_result.value.advisories} == {"approximate-overlap-included"}
        assert format_chronology_date(source, CivilDate(CAL, 0, 1, 1)).value == f"{CAL}:0-1-1"
    finally:
        connection.close()


def test_era_bounds_filter_is_inclusive_and_never_crosses_bases():
    projection = build_chronology_projection(_world(), validated=True)
    source = SourceChronologyStore(projection, "WORKTREE")
    request = AnnotationQuery(ChronologyPredicate.OVERLAPS, CivilDate(CAL, 0, 1, 5), era_id=ERA, era_mode=EraMatchMode.OVERLAPS_BOUNDS)
    result = query_annotations(source, request)
    assert [item.annotation_id for item in result.value.matches] == ["chronology_2123456789abcdefghjkmnpqrs"]
    assert all(item.basis_id == "axis" for item in result.value.matches)


def test_overlaps_bounds_keeps_request_and_era_predicates_independent():
    """A wide range may overlap both disjoint windows without window collapse."""
    world = _world()
    world.records["char_0123456789ABCDEFGHJKMNPQRS"].frontmatter["chronology"].append({
        "id": "chronology_6123456789abcdefghjkmnpqrs", "provenance": ["ledger"],
        "value": {"range": {"calendar_id": CAL,
            "lower": {"calendar_id": CAL, "year": 0, "month": 1, "day": 1},
            "upper": {"calendar_id": CAL, "year": 0, "month": 1, "day": 20}}},
    })
    assert validate_world(world) == []
    projection = build_chronology_projection(world, validated=True)
    source = SourceChronologyStore(projection, "WORKTREE")
    connection, compiled = _sqlite_store(projection)
    try:
        bridge = AnnotationQuery(ChronologyPredicate.ON_DATE, CivilDate(CAL, 0, 1, 20),
            era_id=ERA, era_mode=EraMatchMode.OVERLAPS_BOUNDS)
        before = AnnotationQuery(ChronologyPredicate.BEFORE, CivilDate(CAL, 0, 1, 20),
            era_id=ERA, era_mode=EraMatchMode.OVERLAPS_BOUNDS)
        for request, expected in ((bridge, ["chronology_6123456789abcdefghjkmnpqrs"]),
                                  (before, ["chronology_1123456789abcdefghjkmnpqrs", "chronology_2123456789abcdefghjkmnpqrs"])):
            left, right = query_annotations(source, request), query_annotations(compiled, request)
            assert left == right
            assert [hit.annotation_id for hit in left.value.matches] == expected
    finally:
        connection.close()


def test_era_operands_and_filters_use_kernel_bounds_in_both_stores():
    projection = build_chronology_projection(_world(), validated=True)
    source = SourceChronologyStore(projection, "WORKTREE")
    connection, compiled = _sqlite_store(projection)
    try:
        for request in (
            AnnotationQuery(ChronologyPredicate.ON_DATE, EraDate(ERA, 0, 1, 10)),
            AnnotationQuery(ChronologyPredicate.ON_DATE, EraDate(ERA, 0, 1, 11)),
            AnnotationQuery(ChronologyPredicate.ON_DATE, EraDate(ERA, 0, 1)),
            AnnotationQuery(ChronologyPredicate.ON_DATE, CivilDate(CAL, 0, 1, 1), era_id="era_missing"),
            AnnotationQuery(ChronologyPredicate.ON_DATE, CivilDate(CAL, 0, 1, 1), era_id="era_missing", era_mode=EraMatchMode.OVERLAPS_BOUNDS),
        ):
            left, right = query_annotations(source, request), query_annotations(compiled, request)
            assert left == right
        assert query_annotations(source, AnnotationQuery(ChronologyPredicate.ON_DATE, EraDate(ERA, 0, 1, 11))).reason is ChronologyReason.ERA
        assert query_annotations(source, AnnotationQuery(ChronologyPredicate.ON_DATE, EraDate(ERA, 0, 1))).reason is ChronologyReason.ERA
        assert query_annotations(source, AnnotationQuery(ChronologyPredicate.ON_DATE, CivilDate(CAL, 0, 1, 1), era_id="era_missing")).reason is ChronologyReason.ERA
    finally:
        connection.close()


def test_cycle_calendar_has_no_hidden_century_limit():
    projection = build_chronology_projection(_world(), validated=True)
    source = SourceChronologyStore(projection, "WORKTREE")
    # The cycle calendar has no hidden 400-year datetime limit.
    distant = query_annotations(source, AnnotationQuery(ChronologyPredicate.ON_DATE, CivilDate(CAL, 499, 1, 1)))
    assert distant.value.matches == ()


def test_story_time_mapping_keeps_anchor_ambiguity_typed():
    world = _world()
    world.world_record.frontmatter["chronology"]["anchors"].append({
        "id": "chronology_8123456789abcdefghjkmnpqrs", "axis_day": 5,
        "story_time": {"timeline": "main", "tick": 5, "order": 0}, "provenance": ["ledger"],
    })
    assert validate_world(world) == []
    outcome = map_chronology_date_to_story_time(SourceChronologyStore(build_chronology_projection(world, validated=True), "WORKTREE"), CivilDate(CAL, 0))
    assert outcome.kind.value == "ok"
    assert tuple((item.timeline, item.tick, item.order) for item in outcome.value.values) == (("main", 0, 0), ("main", 5, 0))


def test_full_interval_on_date_and_approximate_predicate_rules_are_total():
    projection = build_chronology_projection(_world(), validated=True)
    source = SourceChronologyStore(projection, "WORKTREE")
    # A month operand is an inclusive interval, not its first day only.
    monthly = query_annotations(source, AnnotationQuery(ChronologyPredicate.ON_DATE, CivilDate(CAL, 0, 1)))
    assert {hit.annotation_id for hit in monthly.value.matches} >= {
        "chronology_1123456789abcdefghjkmnpqrs", "chronology_2123456789abcdefghjkmnpqrs"}
    before = query_annotations(source, AnnotationQuery(ChronologyPredicate.BEFORE, CivilDate(ISOLATED, 0, 1, 4)))
    assert "chronology_5123456789abcdefghjkmnpqrs" not in {hit.annotation_id for hit in before.value.matches}
    bad = query_annotations(source, AnnotationQuery("not-a-predicate", CivilDate(CAL, 0, 1, 1)))
    assert bad.kind.value == "invalid"


def test_sqlite_candidates_use_basis_index_and_legacy_sentinel_is_typed():
    projection = build_chronology_projection(_world(), validated=True)
    connection, compiled = _sqlite_store(projection)
    try:
        plan = connection.execute(
            "EXPLAIN QUERY PLAN SELECT * FROM chronology_annotation INDEXED BY chronology_annotation_basis_lower_idx "
            "WHERE basis_id = ? AND lower_day <= ? AND upper_day >= ? "
            "ORDER BY record_ordinal,source_ordinal,record_id,annotation_id LIMIT 256 OFFSET 0",
            ("axis", 10, 0),
        ).fetchall()
        assert any("chronology_annotation_basis_lower_idx" in str(row[-1]) for row in plan)
    finally:
        connection.close()
    legacy = replace(projection, catalog=None, calendars=(), eras=(), anchors=(), annotations=())
    connection = sqlite3.connect(":memory:"); connection.row_factory = sqlite3.Row
    try:
        connection.executescript(DDL); connection.executescript(INDEX_DDL); insert_chronology_index(connection, legacy); connection.commit()
        result = query_annotations(SQLiteChronologyStore(connection, "WORKTREE"), AnnotationQuery(ChronologyPredicate.ON_DATE, CivilDate(CAL, 0, 1, 1)))
        assert result.kind.value == "unavailable"
    finally:
        connection.close()


def test_index_insert_batch_size_is_bounded_and_total():
    connection = sqlite3.connect(":memory:")
    try:
        connection.executescript(DDL); connection.executescript(INDEX_DDL)
        projection = build_chronology_projection(_world(), validated=True)
        for row in {row.record_id: row for row in projection.annotations}.values():
            connection.execute("INSERT INTO entity VALUES (?,?,?,?,?,?,?,?,?)", (row.record_id, "character", "Character", "main", "canonical", f"story/{row.record_id}.md", None, "", "{}"))
        for value in (0, 1001, True, "10"):
            try:
                insert_chronology_index(connection, projection, batch_size=value)
            except ValueError:
                pass
            else:
                raise AssertionError(f"accepted invalid batch size {value!r}")
        stats = insert_chronology_index(connection, projection, batch_size=1)
        assert stats["chronologyMaxInsertBatch"] == 1
        assert stats["chronologyInsertBatches"] >= sum(stats[key] for key in ("calendarCount", "eraCount", "anchorCount", "annotationCount"))
    finally:
        connection.close()


def test_closed_outcomes_preserve_kernel_categories_and_are_total():
    projection = build_chronology_projection(_world(), validated=True)
    source = SourceChronologyStore(projection, "WORKTREE")
    connection, compiled = _sqlite_store(projection)
    try:
        requests = (
            AnnotationQuery(ChronologyPredicate.ON_DATE, object()),
            AnnotationQuery(ChronologyPredicate.ON_DATE, EraDate("era_missing", 0, 1, 1)),
            AnnotationQuery(ChronologyPredicate.BETWEEN, CivilDate(CAL, 0, 1, 3), CivilDate(CAL, 0, 1, 2)),
            AnnotationQuery(ChronologyPredicate.OVERLAPS, ApproximateDate("about", CivilRange(ISOLATED, None, CivilDate(ISOLATED, 0, 1, 2)))),
        )
        for request in requests:
            left, right = query_annotations(source, request), query_annotations(compiled, request)
            assert left == right
            assert left.kind.value in {"invalid", "unavailable"}
            assert isinstance(left.reason, ChronologyReason)
            assert left.detail
        assert query_annotations(source, requests[1]).reason is ChronologyReason.ERA
        assert query_annotations(source, requests[2]).reason is ChronologyReason.RANGE
        assert query_annotations(source, requests[3]).reason is ChronologyReason.APPROXIMATE_ONLY
        conversion = convert_chronology_date(source, ConversionRequest(CivilDate(CAL, 0), target_calendar_id=ISOLATED))
        assert conversion.reason is ChronologyReason.CONVERSION_EXACTNESS
    finally:
        connection.close()


def test_sqlite_query_consumes_bounded_stream_not_projection_rows():
    projection = build_chronology_projection(_world(), validated=True)
    connection, compiled = _sqlite_store(projection)
    try:
        assert compiled.annotations == ()
        result = query_annotations(compiled, AnnotationQuery(ChronologyPredicate.OVERLAPS, CivilDate(CAL, 0, 1, 1)))
        assert result.kind.value == "ok"
        assert [item.annotation_id for item in result.value.matches] == ["chronology_1123456789abcdefghjkmnpqrs"]
    finally:
        connection.close()


def test_candidate_access_is_stream_only_and_query_consumes_the_iterator():
    """The query boundary never offers a tuple-materializing candidate API."""
    legacy_name = "candidate_" + "annotations"
    assert not hasattr(SourceChronologyStore, legacy_name)
    assert not hasattr(SQLiteChronologyStore, legacy_name)
    implementation = inspect.getsource(query_annotations)
    assert implementation.count("iter_candidate_annotations") == 1


def test_sqlite_keeps_approximate_advisories_in_conservative_candidates():
    projection = build_chronology_projection(_world(), validated=True)
    source = SourceChronologyStore(projection, "WORKTREE")
    connection, compiled = _sqlite_store(projection)
    try:
        request = AnnotationQuery(ChronologyPredicate.BEFORE, CivilDate(ISOLATED, 0, 1, 4))
        left, right = query_annotations(source, request), query_annotations(compiled, request)
        assert left == right
        assert [item.kind for item in left.advisories] == ["approximate-relation-excluded"]
    finally:
        connection.close()


def test_real_noncomparable_scope_is_projected_recursively_and_matches_sqlite():
    """Scope evidence comes from validated authored values, never row surgery."""
    world = _world()
    annotations = world.records["char_0123456789ABCDEFGHJKMNPQRS"].frontmatter["chronology"]
    annotations.extend((
        {"id": "chronology_6123456789abcdefghjkmnpqrs", "provenance": ["ledger"],
         "value": {"approx": {"display_value": "early", "bounds": {"lower": {"calendar_id": CAL, "year": 0, "month": 1, "day": 1}, "upper": None}}}},
        {"id": "chronology_7123456789abcdefghjkmnpqrs", "provenance": ["ledger"],
         "value": {"approx": {"display_value": "some time", "bounds": {"lower": None, "upper": None}}}},
        {"id": "chronology_8123456789abcdefghjkmnpqrs", "provenance": ["ledger"],
         "value": {"conflict": {"claims": [
             {"civil": {"calendar_id": CAL, "year": 0, "month": 1, "day": 1}},
             {"era": {"era_id": ERA, "year": 0, "month": 1, "day": 2}},
             {"civil": {"calendar_id": ISOLATED, "year": 0, "month": 1, "day": 1}},
         ]}}},
        {"id": "chronology_9123456789abcdefghjkmnpqrs", "provenance": ["ledger"],
         "value": {"relative": {"relation": "before", "before_id": "char_0123456789ABCDEFGHJKMNPQRS"}}},
        {"id": "chronology_a123456789abcdefghjkmnpqrs", "provenance": ["ledger"],
         "value": {"duration": {"unit": "month", "value": 2}}},
    ))
    assert validate_world(world) == []
    projection = build_chronology_projection(world, validated=True)
    by_id = {row.annotation_id: row for row in projection.annotations}
    approx = ApproximateDate("about", CivilRange(CAL, CivilDate(CAL, 0, 1, 1), CivilDate(CAL, 0, 1, 2)))
    assert describe_date_value(projection, kernelize_date_value(projection, approx)).value.source_kind == "approx"
    one_sided, no_basis, conflict = (by_id[key] for key in (
        "chronology_6123456789abcdefghjkmnpqrs", "chronology_7123456789abcdefghjkmnpqrs", "chronology_8123456789abcdefghjkmnpqrs"))
    assert one_sided.exclusion_reason == "qualitative-approximation" and one_sided.scope_bases == ("axis",) and not one_sided.unknown_basis
    assert no_basis.unknown_basis and no_basis.scope_bases == ()
    assert conflict.exclusion_reason == "conflict" and conflict.scope_bases == ("axis", f"calendar:{ISOLATED}") and conflict.scope_eras == (ERA,)
    source = SourceChronologyStore(projection, "WORKTREE")
    connection, compiled = _sqlite_store(projection)
    try:
        request = AnnotationQuery(ChronologyPredicate.BEFORE, CivilDate(CAL, 0, 1, 3), era_id=ERA)
        left, right = query_annotations(source, request), query_annotations(compiled, request)
        assert left == right
        # AUTHORED scope only includes an explicitly authored/nested era.  The
        # conflict is non-comparable but still advisory-relevant exactly once.
        assert [(item.kind, item.count) for item in left.advisories] == [("noncomparable-excluded", 1)]
        assert any("chronology_annotation_exclusion_idx" in item for item in compiled.last_candidate_stats["advisoryQueryPlan"])
    finally:
        connection.close()


def test_advisories_are_aggregate_ordered_and_identical_for_source_and_sqlite():
    """Aggregate evidence is projected from validated records, never row surgery."""
    world = _world()
    annotations = world.records["char_0123456789ABCDEFGHJKMNPQRS"].frontmatter["chronology"]
    annotations.extend((
        {"id": "chronology_6123456789abcdefghjkmnpqrs", "provenance": ["ledger"],
         "value": {"civil": {"calendar_id": CAL, "year": 0, "month": 1, "day": 1}}},
        {"id": "chronology_7123456789abcdefghjkmnpqrs", "provenance": ["ledger"],
         "value": {"approx": {"display_value": "around", "bounds": {
             "lower": {"calendar_id": CAL, "year": 0, "month": 1, "day": 1},
             "upper": {"calendar_id": CAL, "year": 0, "month": 1, "day": 1},
         }}}},
        {"id": "chronology_8123456789abcdefghjkmnpqrs", "provenance": ["ledger"],
         "value": {"conflict": {"claims": [
             {"civil": {"calendar_id": CAL, "year": 0, "month": 1, "day": 1}},
             {"relative": {"relation": "before", "before_id": "char_0123456789ABCDEFGHJKMNPQRS"}},
         ]}}},
    ))
    assert validate_world(world) == []
    projection = build_chronology_projection(world, validated=True)
    source = SourceChronologyStore(projection, "WORKTREE")
    connection, compiled = _sqlite_store(projection)
    try:
        request = AnnotationQuery(ChronologyPredicate.BEFORE, CivilDate(CAL, 0, 1, 3), limit=1)
        left, right = query_annotations(source, request), query_annotations(compiled, request)
        assert left == right
        assert [(item.kind, item.count) for item in left.advisories] == [
            ("approximate-relation-excluded", 1), ("noncomparable-excluded", 1), ("result-limit", 1),
        ]
        assert left.value.advisories == left.advisories
        assert any("chronology_annotation_exclusion_idx" in item for item in compiled.last_candidate_stats["advisoryQueryPlan"])
    finally:
        connection.close()


@pytest.mark.parametrize("count,limit,batches", (
    (255, 254, 1), (255, 255, 1), (256, 255, 1), (256, 256, 1),
    (257, 256, 2), (257, 257, 2), (513, 255, 3), (513, 256, 3),
    (513, 257, 3), (513, 513, 3),
))
def test_sqlite_keyset_pages_are_complete_at_boundary_sizes(count, limit, batches):
    seed = _world(); base = seed.records["char_0123456789ABCDEFGHJKMNPQRS"].frontmatter
    records = {seed.world_record.id: seed.world_record}
    expected_ids: list[str] = []
    for index in range(count):
        record_id = id_from_seed("character", f"pagination-record:{index}")
        annotation_id = id_from_seed("chronology", f"pagination-annotation:{index}")
        data = deepcopy(base); data["id"] = record_id; data["title"] = f"Pagination {index}"
        value = ({"approx": {"display_value": "late but finite", "bounds": {
            "lower": {"calendar_id": CAL, "year": 0, "month": 1, "day": 1},
            "upper": {"calendar_id": CAL, "year": 0, "month": 1, "day": 1},
        }}} if index == count - 1 else {"civil": {"calendar_id": CAL, "year": 0, "month": 1, "day": 1}})
        data["chronology"] = [{"id": annotation_id, "provenance": ["ledger"], "value": value}]
        records[record_id] = _record(data, f"story/pagination/{index:04d}.md")
        expected_ids.append(record_id)
    scope_id = id_from_seed("character", f"pagination-scope:{count}")
    scope = deepcopy(base); scope["id"] = scope_id; scope["title"] = "Late non-comparable scope"
    scope["chronology"] = [{"id": id_from_seed("chronology", f"pagination-scope:{count}"), "provenance": ["ledger"],
        "value": {"conflict": {"claims": [
            {"civil": {"calendar_id": CAL, "year": 0, "month": 1, "day": 1}},
            {"relative": {"relation": "before", "before_id": expected_ids[0]}},
        ]}}}]
    records[scope_id] = _record(scope, "story/zz-pagination/noncomparable.md")
    world = World("WORKTREE", "tree", records, Path("."), is_worktree=True)
    assert validate_world(world) == []
    projection = build_chronology_projection(world, validated=True)
    class RecordingSource(SourceChronologyStore):
        def __init__(self, *args): super().__init__(*args); self.keys = []
        def iter_candidate_annotations(self, plan):
            for row in super().iter_candidate_annotations(plan):
                self.keys.append((row.record_ordinal, row.source_ordinal, row.record_id, row.annotation_id))
                yield row
    class RecordingSQLite(SQLiteChronologyStore):
        def __init__(self, *args): super().__init__(*args); self.keys = []
        def iter_candidate_annotations(self, plan):
            for row in super().iter_candidate_annotations(plan):
                self.keys.append((row.record_ordinal, row.source_ordinal, row.record_id, row.annotation_id))
                yield row
    source = RecordingSource(projection, "WORKTREE")
    connection, _compiled = _sqlite_store(projection)
    compiled = RecordingSQLite(connection, "WORKTREE")
    try:
        request = AnnotationQuery(ChronologyPredicate.BEFORE, CivilDate(CAL, 0, 1, 3), limit=limit)
        left, right = query_annotations(source, request), query_annotations(compiled, request)
        assert left == right
        expected_keys = [(row.record_ordinal, row.source_ordinal, row.record_id, row.annotation_id)
            for row in projection.annotations if row.comparison_kind is not ComparisonKind.NONCOMPARABLE]
        assert source.keys == compiled.keys == expected_keys
        assert len(set(compiled.keys)) == len(compiled.keys) == count
        # The final candidate is a relevant finite approximation: it is
        # exhausted and advisory-counted for BEFORE, but never retained.
        expected_relation_hits = count - 1
        assert [item.record_id for item in right.value.matches] == expected_ids[:min(limit, expected_relation_hits)]
        assert len({item.annotation_id for item in right.value.matches}) == min(limit, expected_relation_hits)
        assert compiled.last_candidate_stats["batches"] == batches
        assert compiled.last_candidate_stats["maxReturnedBatch"] <= 256
        assert compiled.last_candidate_stats["totalRelationHits"] == expected_relation_hits
        assert compiled.last_candidate_stats["retainedMatches"] == min(limit, expected_relation_hits)
        expected_advisories = [("approximate-relation-excluded", 1), ("noncomparable-excluded", 1)]
        if expected_relation_hits > limit:
            expected_advisories.append(("result-limit", expected_relation_hits - limit))
        assert [(item.kind, item.count) for item in right.advisories] == expected_advisories
        assert right.value.advisories == right.advisories
        assert compiled.last_candidate_stats["queryPlan"]
        if count > 256:
            assert compiled.last_candidate_stats["continuationQueryPlan"]
    finally:
        connection.close()


def test_authored_era_filter_uses_the_era_order_index_before_fetching_candidates():
    projection = build_chronology_projection(_world(), validated=True)
    connection, compiled = _sqlite_store(projection)
    try:
        result = query_annotations(compiled, AnnotationQuery(
            ChronologyPredicate.OVERLAPS, CivilDate(CAL, 0, 1, 5), era_id=ERA, era_mode=EraMatchMode.AUTHORED,
        ))
        assert result.kind.value == "ok"
        assert any("chronology_annotation_era_basis_order_idx" in item for item in compiled.last_candidate_stats["queryPlan"])
    finally:
        connection.close()


def test_real_record_matrix_covers_predicates_eras_bases_and_noncomparable_forms():
    """A real v0.6 corpus with independent projection and query oracles."""
    world = _world()
    open_era = "era_1123456789abcdefghjkmnpqrs"
    world.world_record.frontmatter["chronology"]["eras"].append({
        "id": open_era, "calendar_id": CAL, "label": "Unbounded", "aliases": [],
        "display_year_zero": True, "display_epoch": {"display_year": 0, "machine_year": 0},
        "provenance": ["ledger"],
    })
    owner = world.records["char_0123456789ABCDEFGHJKMNPQRS"]
    annotation = owner.frontmatter["chronology"]
    ids = {}
    def add(name, value):
        ids[name] = id_from_seed("chronology", f"matrix:{name}")
        annotation.append({"id": ids[name], "provenance": ["ledger"], "value": value})
    # Exact/partial civil and exact/partial era values include both bounded-era
    # endpoints.  All ranges below are authored CivilRange values, including
    # the two truly open forms (they are not approximate wrappers).
    add("partial-civil", {"civil": {"calendar_id": CAL, "year": 0}})
    add("era-lower", {"era": {"era_id": ERA, "year": 0, "month": 1, "day": 1}})
    add("era-upper", {"era": {"era_id": ERA, "year": 0, "month": 1, "day": 10}})
    add("partial-era", {"era": {"era_id": open_era, "year": 0}})
    add("open-lower-range", {"range": {"calendar_id": CAL, "lower": None,
        "upper": {"calendar_id": CAL, "year": 0, "month": 1, "day": 3}}})
    add("open-upper-range", {"range": {"calendar_id": CAL,
        "lower": {"calendar_id": CAL, "year": 0, "month": 1, "day": 3}, "upper": None}})
    add("closed-range", {"range": {"calendar_id": CAL,
        "lower": {"calendar_id": CAL, "year": 0, "month": 1, "day": 1},
        "upper": {"calendar_id": CAL, "year": 0, "month": 1, "day": 3}}})
    add("wide-range", {"range": {"calendar_id": CAL,
        "lower": {"calendar_id": CAL, "year": 0, "month": 1, "day": 1},
        "upper": {"calendar_id": CAL, "year": 0, "month": 1, "day": 20}}})
    add("finite-approx", {"approx": {"display_value": "around", "bounds": {
        "lower": {"calendar_id": CAL, "year": 0, "month": 1, "day": 1},
        "upper": {"calendar_id": CAL, "year": 0, "month": 1, "day": 2}}}})
    add("irrelevant-finite-approx", {"approx": {"display_value": "later", "bounds": {
        "lower": {"calendar_id": CAL, "year": 0, "month": 1, "day": 20},
        "upper": {"calendar_id": CAL, "year": 0, "month": 1, "day": 21}}}})
    add("one-sided-approx", {"approx": {"display_value": "early", "bounds": {
        "lower": {"calendar_id": CAL, "year": 0, "month": 1, "day": 1}, "upper": None}}})
    add("null-approx", {"approx": {"display_value": "some time", "bounds": {"lower": None, "upper": None}}})
    add("same-conflict", {"conflict": {"claims": [
        {"civil": {"calendar_id": CAL, "year": 0, "month": 1, "day": 1}},
        {"civil": {"calendar_id": CAL, "year": 0, "month": 1, "day": 2}},
    ]}})
    add("cross-conflict", {"conflict": {"claims": [
        {"civil": {"calendar_id": CAL, "year": 0, "month": 1, "day": 1}},
        {"civil": {"calendar_id": ISOLATED, "year": 0, "month": 1, "day": 1}},
    ]}})
    add("nested-era-conflict", {"conflict": {"claims": [
        {"conflict": {"claims": [
            {"era": {"era_id": ERA, "year": 0, "month": 1, "day": 2}},
            {"civil": {"calendar_id": CAL, "year": 0, "month": 1, "day": 2}},
        ]}},
        {"civil": {"calendar_id": ISOLATED, "year": 0, "month": 1, "day": 1}},
    ]}})
    for name, relative in (("relative-before", {"relation": "before", "before_id": owner.id}),
                           ("relative-after", {"relation": "after", "after_id": owner.id}),
                           ("relative-both", {"relation": "between", "before_id": owner.id, "after_id": owner.id})):
        add(name, {"relative": relative})
    for unit in ("year", "month", "day"):
        add(f"duration-{unit}", {"duration": {"unit": unit, "value": 1}})
    assert validate_world(world) == []
    projection = build_chronology_projection(world, validated=True)
    source = SourceChronologyStore(projection, "WORKTREE")
    connection, compiled = _sqlite_store(projection)
    try:
        rows = {row.annotation_id: row for row in projection.annotations}
        sql_rows = {row.annotation_id: row for row in load_chronology_projection(connection).annotations}
        raw = {item["id"]: item["value"] for item in annotation}
        iso_basis = f"calendar:{ISOLATED}"
        # (annotation id, value kind, authored source kind, comparison,
        # exclusion, basis, lower, upper, lower-open, upper-open, known bases,
        # known eras, unknown basis).  Values are deliberately literal: this is
        # an oracle for both the in-memory projection and a fresh SQLite reload.
        projection_oracle = (
            ("chronology_1123456789abcdefghjkmnpqrs", "civil", "civil", "exact", None, "axis", 0, 0, False, False, ("axis",), (), False),
            ("chronology_2123456789abcdefghjkmnpqrs", "era", "era", "exact", None, "axis", 4, 4, False, False, ("axis",), (ERA,), False),
            ("chronology_3123456789abcdefghjkmnpqrs", "civil", "civil", "exact", None, iso_basis, 1, 1, False, False, (iso_basis,), (), False),
            ("chronology_4123456789abcdefghjkmnpqrs", "range", "range", "interval", None, iso_basis, 1, 2, False, False, (iso_basis,), (), False),
            ("chronology_5123456789abcdefghjkmnpqrs", "approx", "approx", "approximate", None, iso_basis, 1, 2, False, False, (iso_basis,), (), False),
            ("chronology_3NC2W62RD82SZPWGPSYDKM6W05", "civil", "civil", "interval", None, "axis", 0, 29, False, False, ("axis",), (), False),
            ("chronology_16GPEA0RXYYP1QNNCYZ3HMYJJK", "era", "era", "exact", None, "axis", 0, 0, False, False, ("axis",), (ERA,), False),
            ("chronology_12XS1PGYWE808X14TA2FSGJSY3", "era", "era", "exact", None, "axis", 9, 9, False, False, ("axis",), (ERA,), False),
            ("chronology_1E47NEXG399M8DVWES2XQ8C5NX", "era", "era", "interval", None, "axis", 0, 29, False, False, ("axis",), (open_era,), False),
            ("chronology_7TKBZSW87XNQ43FJYS41K6YNHF", "range", "range", "interval", None, "axis", None, 2, True, False, ("axis",), (), False),
            ("chronology_5ER28926Q73JEJ2T678JMABXBK", "range", "range", "interval", None, "axis", 2, None, False, True, ("axis",), (), False),
            ("chronology_2ZZCX9MKRPBRB08RDZY8NSGMKS", "range", "range", "interval", None, "axis", 0, 2, False, False, ("axis",), (), False),
            ("chronology_2PN704DAJH8PRFZMN1BB75KZY5", "range", "range", "interval", None, "axis", 0, 19, False, False, ("axis",), (), False),
            ("chronology_73B7EZPDH2QP7EF38C9VCE1S97", "approx", "approx", "approximate", None, "axis", 0, 1, False, False, ("axis",), (), False),
            ("chronology_4EZ9ZK4KXT9JFRAZT8C8G1G262", "approx", "approx", "approximate", None, "axis", 19, 20, False, False, ("axis",), (), False),
            ("chronology_2X2498F8RFZH3Y83GNJRCJB5CC", "approx", "approx", "noncomparable", "qualitative-approximation", "axis", 0, None, False, True, ("axis",), (), False),
            ("chronology_4ZR65J1VAJ8276MCFG672P8S2N", "approx", "approx", "noncomparable", "qualitative-approximation", None, None, None, True, True, (), (), True),
            ("chronology_7P3E29GBVW6063N9D1K8HY8052", "conflict", "conflict", "noncomparable", "conflict", None, None, None, False, False, ("axis",), (), False),
            ("chronology_25V42APZ59BZSH77V9CQG7GWT7", "conflict", "conflict", "noncomparable", "conflict", None, None, None, False, False, ("axis", iso_basis), (), False),
            ("chronology_6SFP7RV23Z4N8S91HZNDPF8103", "conflict", "conflict", "noncomparable", "conflict", None, None, None, False, False, ("axis", iso_basis), (ERA,), False),
            ("chronology_6TKJDDFBF5EQTNAYZ3P341NJ8P", "relative", "relative", "noncomparable", "relative", None, None, None, False, False, (), (), True),
            ("chronology_332MTPVK1DJ81S45PP2HB8KSDM", "relative", "relative", "noncomparable", "relative", None, None, None, False, False, (), (), True),
            ("chronology_069S9DSTWES784M4THMA4XPEEP", "relative", "relative", "noncomparable", "relative", None, None, None, False, False, (), (), True),
            ("chronology_7M6BFYGS5PHQFQ76TPNH2BT2KT", "duration", "duration", "noncomparable", "duration", None, None, None, False, False, (), (), True),
            ("chronology_0YGADC7FJGD71WSJDNC87ABQED", "duration", "duration", "noncomparable", "duration", None, None, None, False, False, (), (), True),
            ("chronology_5QXEHCMZ9NC77A2ASPS20FBPBY", "duration", "duration", "noncomparable", "duration", None, None, None, False, False, (), (), True),
        )
        def actual_projection(row):
            authored = json.loads(row.value_json)
            source_kind = next(key for key in authored if not key.startswith("x-"))
            return (row.annotation_id, row.value_kind, source_kind, row.comparison_kind.value, row.exclusion_reason,
                row.basis_id, row.lower_day, row.upper_day, row.lower_unbounded, row.upper_unbounded,
                row.scope_bases, row.scope_eras, row.unknown_basis)
        for stored_rows in (rows, sql_rows):
            assert tuple(actual_projection(stored_rows[item[0]]) for item in projection_oracle) == projection_oracle
            assert tuple(json.loads(stored_rows[item[0]].value_json) for item in projection_oracle) == tuple(raw[item[0]] for item in projection_oracle)
            assert tuple((item[0], next(key for key in json.loads(stored_rows[item[0]].value_json) if not key.startswith("x-")) == "approx") for item in projection_oracle) == tuple((item[0], item[2] == "approx") for item in projection_oracle)

        owner_id = "char_0123456789ABCDEFGHJKMNPQRS"
        partial_hits = ((owner_id, "chronology_3NC2W62RD82SZPWGPSYDKM6W05", "overlaps"), (owner_id, "chronology_1E47NEXG399M8DVWES2XQ8C5NX", "overlaps"))
        before_hits = ((owner_id, "chronology_1123456789abcdefghjkmnpqrs", "before"), (owner_id, "chronology_16GPEA0RXYYP1QNNCYZ3HMYJJK", "before"))
        after_hits = ((owner_id, "chronology_2123456789abcdefghjkmnpqrs", "after"), (owner_id, "chronology_12XS1PGYWE808X14TA2FSGJSY3", "after"))
        isolated_hits = ((owner_id, "chronology_3123456789abcdefghjkmnpqrs", "exact"), (owner_id, "chronology_4123456789abcdefghjkmnpqrs", "overlaps"))
        open_lower_hits = ((owner_id, "chronology_1123456789abcdefghjkmnpqrs", "overlaps"), (owner_id, "chronology_3NC2W62RD82SZPWGPSYDKM6W05", "overlaps"))
        open_upper_hits = ((owner_id, "chronology_2123456789abcdefghjkmnpqrs", "overlaps"), (owner_id, "chronology_3NC2W62RD82SZPWGPSYDKM6W05", "overlaps"))
        limit_advisories = (("noncomparable-excluded", 11, "non-comparable claims excluded"), ("result-limit", 4, "result limit applied"))
        relation_advisories = (("approximate-relation-excluded", 1, "approximate claims are overlap-only"), ("noncomparable-excluded", 11, "non-comparable claims excluded"))
        authored_advisories = (("noncomparable-excluded", 1, "non-comparable claims excluded"),)
        # case id, predicate, lower/upper operands, era selection, limit,
        # expected kind/reason/hits/advisories, and executable coverage metadata.
        query_oracle = (
            ("on-date-none", ChronologyPredicate.ON_DATE, CivilDate(CAL, 0, 1, 3), None, None, EraMatchMode.AUTHORED, 2, "ok", None, partial_hits, limit_advisories, {"predicates": {"on_date"}, "era": {"none"}, "forms": {"exact-civil", "partial-civil", "same-conflict", "relative-before", "relative-after", "relative-both", "duration-year", "duration-month", "duration-day"}}),
            ("on-date-authored", ChronologyPredicate.ON_DATE, CivilDate(CAL, 0, 1, 3), None, ERA, EraMatchMode.AUTHORED, 2, "ok", None, (), authored_advisories, {"predicates": {"on_date"}, "era": {"authored", "bounded"}, "forms": {"exact-era-lower", "exact-era-upper", "nested-era-conflict"}}),
            ("on-date-bounds", ChronologyPredicate.ON_DATE, CivilDate(CAL, 0, 1, 3), None, ERA, EraMatchMode.OVERLAPS_BOUNDS, 2, "ok", None, partial_hits, limit_advisories, {"predicates": {"on_date"}, "era": {"overlaps_bounds", "bounded"}, "forms": {"closed-range", "wide-range"}}),
            ("overlaps-none", ChronologyPredicate.OVERLAPS, CivilDate(CAL, 0, 1, 3), None, None, EraMatchMode.AUTHORED, 2, "ok", None, partial_hits, limit_advisories, {"predicates": {"overlaps"}, "era": {"none"}, "forms": {"relevant-finite-approx", "irrelevant-finite-approx"}}),
            ("overlaps-authored", ChronologyPredicate.OVERLAPS, CivilDate(CAL, 0, 1, 3), None, ERA, EraMatchMode.AUTHORED, 2, "ok", None, (), authored_advisories, {"predicates": {"overlaps"}, "era": {"authored", "bounded"}, "forms": {"era"}}),
            ("overlaps-bounds", ChronologyPredicate.OVERLAPS, CivilDate(CAL, 0, 1, 3), None, ERA, EraMatchMode.OVERLAPS_BOUNDS, 2, "ok", None, partial_hits, limit_advisories, {"predicates": {"overlaps"}, "era": {"overlaps_bounds", "bounded"}, "forms": {"range"}}),
            ("before-none", ChronologyPredicate.BEFORE, CivilDate(CAL, 0, 1, 3), None, None, EraMatchMode.AUTHORED, 2, "ok", None, before_hits, relation_advisories, {"predicates": {"before"}, "era": {"none"}, "forms": {"one-sided-approx", "null-approx"}}),
            ("before-authored", ChronologyPredicate.BEFORE, CivilDate(CAL, 0, 1, 3), None, ERA, EraMatchMode.AUTHORED, 2, "ok", None, ((owner_id, "chronology_16GPEA0RXYYP1QNNCYZ3HMYJJK", "before"),), authored_advisories, {"predicates": {"before"}, "era": {"authored", "bounded"}, "forms": {"exact-era"}}),
            ("before-bounds", ChronologyPredicate.BEFORE, CivilDate(CAL, 0, 1, 3), None, ERA, EraMatchMode.OVERLAPS_BOUNDS, 2, "ok", None, before_hits, relation_advisories, {"predicates": {"before"}, "era": {"overlaps_bounds", "bounded"}, "forms": {"qualitative-approx"}}),
            ("after-none", ChronologyPredicate.AFTER, CivilDate(CAL, 0, 1, 3), None, None, EraMatchMode.AUTHORED, 2, "ok", None, after_hits, relation_advisories, {"predicates": {"after"}, "era": {"none"}, "forms": {"irrelevant-finite-approx"}}),
            ("after-authored", ChronologyPredicate.AFTER, CivilDate(CAL, 0, 1, 3), None, ERA, EraMatchMode.AUTHORED, 2, "ok", None, after_hits, authored_advisories, {"predicates": {"after"}, "era": {"authored", "bounded"}, "forms": {"era-bound"}}),
            ("after-bounds", ChronologyPredicate.AFTER, CivilDate(CAL, 0, 1, 3), None, ERA, EraMatchMode.OVERLAPS_BOUNDS, 2, "ok", None, after_hits, (("noncomparable-excluded", 11, "non-comparable claims excluded"),), {"predicates": {"after"}, "era": {"overlaps_bounds", "bounded"}, "forms": {"wide-range"}}),
            ("between-none", ChronologyPredicate.BETWEEN, CivilDate(CAL, 0, 1, 3), CivilDate(CAL, 0, 1, 4), None, EraMatchMode.AUTHORED, 2, "ok", None, partial_hits, limit_advisories, {"predicates": {"between"}, "era": {"none"}, "forms": {"partial-civil"}}),
            ("between-authored", ChronologyPredicate.BETWEEN, CivilDate(CAL, 0, 1, 3), CivilDate(CAL, 0, 1, 4), ERA, EraMatchMode.AUTHORED, 2, "ok", None, (), authored_advisories, {"predicates": {"between"}, "era": {"authored", "bounded"}, "forms": {"era"}}),
            ("between-bounds", ChronologyPredicate.BETWEEN, CivilDate(CAL, 0, 1, 3), CivilDate(CAL, 0, 1, 4), ERA, EraMatchMode.OVERLAPS_BOUNDS, 2, "ok", None, partial_hits, limit_advisories, {"predicates": {"between"}, "era": {"overlaps_bounds", "bounded"}, "forms": {"closed-range"}}),
            ("isolated", ChronologyPredicate.ON_DATE, CivilDate(ISOLATED, 0, 1, 2), None, None, EraMatchMode.AUTHORED, 2, "ok", None, isolated_hits, (("approximate-relation-excluded", 1, "approximate claims are overlap-only"), ("noncomparable-excluded", 9, "non-comparable claims excluded")), {"bases": {"isolated"}, "forms": {"isolated", "cross-conflict"}}),
            ("unbounded-era", ChronologyPredicate.ON_DATE, CivilDate(CAL, 0, 1, 3), None, open_era, EraMatchMode.OVERLAPS_BOUNDS, 2, "ok", None, partial_hits, limit_advisories, {"era": {"overlaps_bounds", "unbounded"}, "forms": {"partial-era"}}),
            ("unknown-era", ChronologyPredicate.ON_DATE, CivilDate(CAL, 0, 1, 1), None, "era_missing", EraMatchMode.AUTHORED, 100, "invalid", "era", (), (), {"era": {"unknown"}}),
            ("out-of-bounds-era-date", ChronologyPredicate.ON_DATE, EraDate(ERA, 0, 1, 11), None, None, EraMatchMode.AUTHORED, 100, "invalid", "era", (), (), {"era": {"out-of-bounds"}}),
            ("basis-mismatch-between", ChronologyPredicate.BETWEEN, CivilDate(CAL, 0, 1, 3), CivilDate(ISOLATED, 0, 1, 2), None, EraMatchMode.AUTHORED, 100, "invalid", "range", (), (), {"bases": {"mismatch"}}),
            ("open-lower-operand", ChronologyPredicate.OVERLAPS, CivilRange(CAL, None, CivilDate(CAL, 0, 1, 3)), None, None, EraMatchMode.AUTHORED, 2, "ok", None, open_lower_hits, (("approximate-overlap-included", 1, "approximate claims included by overlap"), ("noncomparable-excluded", 11, "non-comparable claims excluded"), ("result-limit", 7, "result limit applied")), {"forms": {"lower-open-range"}}),
            ("open-upper-operand", ChronologyPredicate.OVERLAPS, CivilRange(CAL, CivilDate(CAL, 0, 1, 3), None), None, None, EraMatchMode.AUTHORED, 2, "ok", None, open_upper_hits, (("approximate-overlap-included", 1, "approximate claims included by overlap"), ("noncomparable-excluded", 11, "non-comparable claims excluded"), ("result-limit", 7, "result limit applied")), {"forms": {"upper-open-range"}}),
            ("qualitative-operand", ChronologyPredicate.OVERLAPS, ApproximateDate("about", CivilRange(CAL, None, CivilDate(CAL, 0, 1, 3))), None, None, EraMatchMode.AUTHORED, 100, "unavailable", "approximate_only", (), (), {"forms": {"qualitative-operand", "null-approx"}}),
        )
        coverage = {}
        for case_id, predicate, value, upper, era_id, era_mode, limit, kind, reason, hits, advisories, metadata in query_oracle:
            request = AnnotationQuery(predicate, value, upper, era_id=era_id, era_mode=era_mode, limit=limit)
            for store in (source, compiled):
                outcome = query_annotations(store, request)
                assert outcome.kind.value == kind, case_id
                assert (outcome.reason.value if outcome.reason else None) == reason, case_id
                actual_hits = () if outcome.value is None else tuple((hit.record_id, hit.annotation_id, hit.relation) for hit in outcome.value.matches)
                actual_advisories = tuple((item.kind, item.count, item.message) for item in outcome.advisories)
                assert actual_hits == hits, case_id
                assert actual_advisories == advisories, case_id
                if outcome.value is not None:
                    assert tuple((item.kind, item.count, item.message) for item in outcome.value.advisories) == advisories
            for dimension, values in metadata.items():
                coverage.setdefault(dimension, set()).update(values)
        assert coverage == {
            "predicates": {"on_date", "overlaps", "before", "after", "between"},
            "era": {"none", "authored", "overlaps_bounds", "bounded", "unbounded", "unknown", "out-of-bounds"},
            "bases": {"isolated", "mismatch"},
            "forms": {"exact-civil", "partial-civil", "exact-era", "exact-era-lower", "exact-era-upper", "partial-era", "closed-range", "wide-range", "range", "relevant-finite-approx", "irrelevant-finite-approx", "one-sided-approx", "qualitative-approx", "qualitative-operand", "null-approx", "same-conflict", "cross-conflict", "nested-era-conflict", "relative-before", "relative-after", "relative-both", "duration-year", "duration-month", "duration-day", "era", "era-bound", "isolated", "lower-open-range", "upper-open-range"},
        }
    finally:
        connection.close()


def test_production_benchmark_smoke_uses_a_non_git_repository_and_measures_indexes():
    evidence = run_benchmark(smoke=True)
    assert evidence["cache"] == {
        "initial": "missing", "first": "compiled", "ready": "ready",
        "cacheHit": "cache-hit", "cacheHitSourceLoad": {
            "mode": "not-required", "parsed": 0, "cacheHits": 0, "blobReads": 0,
        }, "strict": "ready",
    }
    assert evidence["corpus"]["records"] == 101
    assert evidence["compilation"]["chronology"]["annotationCount"] == 100
    assert evidence["queries"]["exact"]["equal"] is True
    assert evidence["queries"]["overlap"]["candidateStats"]["batches"] >= 1
    assert evidence["conversion"] == {"equal": True, "kind": "ok", "formatted": "calendar_1123456789abcdefghjkmnpqrs:0-1-1"}


def test_checked_full_benchmark_evidence_has_deterministic_production_corpus_and_current_implementation():
    checked = json.loads((Path(__file__).parents[1] / "docs" / "chronology-index-benchmark.json").read_text(encoding="utf-8"))
    assert checked["mode"] == "full"
    assert checked["corpus"]["years"] == [-249, 250]
    assert checked["corpus"]["claims"] == 10_000
    assert checked["cache"]["initial"] == "missing"
    assert checked["cache"]["cacheHitSourceLoad"]["mode"] == "not-required"
    assert checked["compilation"]["chronology"]["annotationCount"] == 10_000
    assert checked["queries"]["exact"]["equal"] is True
    assert checked["queries"]["overlap"]["equal"] is True
    assert checked["queries"]["broad"]["equal"] is True
    assert checked["queries"]["broad"]["candidateStats"]["batches"] > 1
    assert checked["queries"]["broad"]["candidateStats"]["batchSize"] <= 256
    assert any("chronology_annotation_basis" in item for item in checked["queries"]["exact"]["candidateStats"]["queryPlan"])
    assert checked["conversion"]["equal"] is True
    assert checked["budgets"]["compileWithinBudget"] is True
    assert checked["budgets"]["sqliteQueryWithinBudget"] is True
    root = Path(__file__).parents[1]
    assert checked["implementation"]["fingerprintSha256"] == _BENCHMARK._digest_files((
        root / "src" / "wedl" / "compiler.py", root / "src" / "wedl" / "chronology_index.py",
        root / "src" / "wedl" / "chronology_query.py", root / "tools" / "benchmark_chronology_index.py",
    ))
    with TemporaryDirectory(prefix="wedl-chronology-corpus-") as temporary:
        corpus = _BENCHMARK.build_corpus(Path(temporary), claims=10_000, years=(-249, 250))
        repository = _BENCHMARK.Repository(temporary)
        assert repository.is_git is False
        assert repository.resolve("HEAD") == "WORKTREE"
        assert repository.tree_oid("WORKTREE") == checked["corpus"]["treeOid"]
        assert corpus["generatorConfigSha256"] == checked["corpus"]["generatorConfigSha256"]
