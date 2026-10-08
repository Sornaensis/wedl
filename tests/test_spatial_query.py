from __future__ import annotations

import sqlite3
from pathlib import Path

import pytest
import yaml

from wedl.compiler import DDL, INDEX_DDL
from wedl.model import Record, StoryTime, World
from wedl.spatial_index import (_story_time_digits, _time_rectangles, build_spatial_projection,
                                insert_spatial_index, install_optional_spatial_index)
from wedl.spatial_query import (BoundingBox, MapPosition, SpatialOutcomeKind,
    SpatialReason, SpatialStore, SpatialSubreason)


ROOT = Path(__file__).parents[1]


def test_explorer_page_queries_use_existing_indexes_without_unbounded_sorts() -> None:
    connection = sqlite3.connect(":memory:")
    connection.executescript(DDL)
    connection.executescript(INDEX_DDL)
    connection.execute("CREATE VIRTUAL TABLE spatial_location_rtree USING rtree(source_ordinal,min_x,max_x,min_y,max_y,min_map,max_map)")
    plans = {
        "viewport": ("SELECT loc.id,loc.source_ordinal FROM spatial_location_rtree AS box "
                     "CROSS JOIN spatial_location AS loc ON loc.source_ordinal=box.source_ordinal "
                     "WHERE box.min_map<=? AND box.max_map>=? AND box.min_x<=? AND box.max_x>=? "
                     "AND box.min_y<=? AND box.max_y>=? AND loc.map_id=? AND loc.min_x<=? "
                     "AND loc.max_x>=? AND loc.min_y<=? AND loc.max_y>=? LIMIT ?",
                     (1, 1, 1, 0, 1, 0, "map", 1, 0, 1, 0, 10001), "VIRTUAL TABLE INDEX"),
    }
    for query, params, expected_index in plans.values():
        detail = " ".join(row[3] for row in connection.execute("EXPLAIN QUERY PLAN " + query, params))
        assert expected_index in detail
        assert "USE TEMP B-TREE FOR ORDER BY" not in detail
    store = _store()
    _location(store, "location:query-parent", 80)
    for index, ident in enumerate(("location:z-last", "location:a-first", "location:m-middle")):
        _location(store, ident, 81 + index, parent="location:query-parent")
    statements = []
    store.connection.set_trace_callback(statements.append)
    first = store.children("location:query-parent", limit=1)
    second = store.children("location:query-parent", limit=1, cursor=first.value.cursor)
    final = store.children("location:query-parent", limit=1, cursor=second.value.cursor)
    store.connection.set_trace_callback(None)
    assert first.value.ids + second.value.ids + final.value.ids == (
        "location:z-last", "location:a-first", "location:m-middle")
    assert final.value.cursor is None
    page_sql = [sql for sql in statements if "SELECT id,source_ordinal" in sql]
    assert len(page_sql) == 3
    for sql in page_sql:
        detail = " ".join(row[3] for row in store.connection.execute("EXPLAIN QUERY PLAN " + sql))
        assert "COVERING INDEX spatial_location_parent_idx" in detail
        assert "TEMP B-TREE" not in detail
    seek_plan = " ".join(row[3] for row in
        store.connection.execute("EXPLAIN QUERY PLAN " + page_sql[-1]))
    assert "source_ordinal>" in seek_plan or "(source_ordinal,id)>" in seek_plan
    store.connection.close()
    connection.close()


def test_explorer_bbox_requires_map_scoped_rtree_but_legacy_bbox_stays_available() -> None:
    store = _store()
    assert {row[1] for row in store.connection.execute("PRAGMA table_info(spatial_location_rtree)")} >= {
        "min_map", "max_map"}
    box = BoundingBox((-1, -1), (1, 1))
    result = store.bbox("map:town", box, candidate_budget=10)
    assert result.kind is SpatialOutcomeKind.OK and result.value.ids == ("location:gate",)
    store.connection.execute("DROP TABLE spatial_location_rtree")
    assert store.bbox("map:town", box, candidate_budget=10).kind is SpatialOutcomeKind.UNAVAILABLE
    assert store.bbox("map:town", box).kind is SpatialOutcomeKind.OK
    store.connection.close()


def test_failed_rtree_population_leaves_no_partial_explorer_index() -> None:
    store = _store()
    connection = store.connection
    connection.execute("DROP TABLE spatial_location_rtree")

    class FailAfterFirstRow:
        def execute(self, *args):
            return connection.execute(*args)

        def executemany(self, statement, rows):
            first = next(iter(rows))
            connection.execute(statement, first)
            raise sqlite3.OperationalError("simulated population failure")

    assert install_optional_spatial_index(FailAfterFirstRow()) == "btree"
    assert connection.execute("SELECT 1 FROM sqlite_master WHERE name='spatial_location_rtree'").fetchone() is None
    assert store.bbox("map:town", BoundingBox((-1, -1), (1, 1)), candidate_budget=10).kind is SpatialOutcomeKind.UNAVAILABLE
    connection.close()


def test_failed_temporal_index_population_removes_both_explorer_rtrees() -> None:
    store = _store()
    connection = store.connection
    connection.execute("DROP TABLE spatial_overlay_time_rtree")
    connection.execute("DROP TABLE spatial_location_rtree")

    class FailTemporalInsert:
        def execute(self, *args):
            return connection.execute(*args)

        def executemany(self, statement, rows):
            if statement.startswith("INSERT INTO spatial_overlay_time_rtree"):
                raise sqlite3.OperationalError("simulated temporal population failure")
            return connection.executemany(statement, rows)

    assert install_optional_spatial_index(FailTemporalInsert()) == "btree"
    assert connection.execute("SELECT name FROM sqlite_master WHERE name IN "
                              "('spatial_location_rtree','spatial_overlay_time_rtree')").fetchall() == []
    connection.close()


def test_story_time_rtree_boxes_cover_exact_signed_tick_order_ranges() -> None:
    cases = (
        ((-(2**63), -(2**31)), (-(2**63), -(2**31) + 1),
         ((-(2**63), -(2**31)), (-(2**63), -(2**31) + 1), (-(2**63), -(2**31) + 2))),
        ((-1, 0), (1, 0), ((-1, -1), (-1, 0), (0, 0), (1, 0), (1, 1))),
        (((2**63) - 2, 0), ((2**63) - 1, 0),
         (((2**63) - 3, 0), ((2**63) - 2, 0), ((2**63) - 1, 0), ((2**63) - 1, 1))),
        ((5, -1), (5, 1), ((5, -2), (5, -1), (5, 0), (5, 1), (5, 2))),
    )
    for start, end, probes in cases:
        rectangles = list(_time_rectangles(_story_time_digits(*start), _story_time_digits(*end)))
        assert 1 <= len(rectangles) <= 15
        for rectangle in rectangles:
            assert all(0 <= low <= high < 2**24 for low, high in rectangle)
        for point in probes:
            digits = _story_time_digits(*point)
            covered = sum(all(low <= digit <= high for digit, (low, high) in zip(digits, rectangle))
                          for rectangle in rectangles)
            assert covered == int(start <= point <= end), (start, end, point, covered)


def test_explorer_sparse_viewport_work_does_not_grow_with_map_size() -> None:
    store = _store()
    connection = store.connection
    map_key = connection.execute("SELECT rowid FROM spatial_map WHERE id='map:town'").fetchone()[0]

    def insert_noise(start: int, stop: int) -> None:
        connection.executemany("INSERT INTO entity VALUES (?,?,?,?,?,?,?,?,?)", (
            (f"noise:{index:05d}", "location", f"Noise {index}", "world", "canonical",
             f"story/noise-{index:05d}.md", None, "", "{}")
            for index in range(start, stop)))
        connection.executemany("INSERT INTO spatial_location VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)", (
            (f"noise:{index:05d}", 1000 + index, 1, None, "map:town", "point",
             -40000 + index, 0, None, -40000 + index, 0, None, None, "{}")
            for index in range(start, stop)))
        connection.executemany("INSERT INTO spatial_location_rtree VALUES (?,?,?,?,?,?,?)", (
            (1000 + index, -40000 + index, -40000 + index, 0, 0, map_key, map_key)
            for index in range(start, stop)))

    connection.executemany("INSERT INTO entity VALUES (?,?,?,?,?,?,?,?,?)", (
        (f"hit:{index:02d}", "location", f"Hit {index}", "world", "canonical",
         f"story/hit-{index:02d}.md", None, "", "{}")
        for index in range(10)))
    connection.executemany("INSERT INTO spatial_location VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)", (
        (f"hit:{index:02d}", 200000 + index, 1, None, "map:town", "point",
         49990 + index, 0, None, 49990 + index, 0, None, None, "{}")
        for index in range(10)))
    connection.executemany("INSERT INTO spatial_location_rtree VALUES (?,?,?,?,?,?,?)", (
        (200000 + index, 49990 + index, 49990 + index, 0, 0, map_key, map_key)
        for index in range(10)))

    def vm_steps() -> int:
        steps = [0]

        def count_steps() -> int:
            steps[0] += 10
            return 0

        connection.set_progress_handler(count_steps, 10)
        try:
            result = store.bbox("map:town", BoundingBox((49990, -1), (50000, 1)),
                                limit=100, candidate_budget=10000)
        finally:
            connection.set_progress_handler(None, 0)
        assert result.kind is SpatialOutcomeKind.OK
        assert result.value.ids == tuple(f"hit:{index:02d}" for index in range(10))
        return steps[0]

    insert_noise(0, 1000)
    small = vm_steps()
    insert_noise(1000, 10000)
    large = vm_steps()
    assert large <= 2 * small + 100, (small, large)
    connection.close()


def _store(*, overlay_valid: dict | None = None) -> SpatialStore:
    value = yaml.safe_load((ROOT / "tests/fixtures/spatial_v07/valid-multimap.yaml").read_text())
    if overlay_valid is not None:
        value["overlay"]["valid"] = overlay_valid
    records = [value["world"], *value["maps"], *value["locations"], value["anchor"], value["portal"], value["route"], value["overlay"]]
    typed = [Record(item, "", f"story/{index}.md", b"") for index, item in enumerate(records)]
    world = World("component", "tree", {record.id: record for record in typed}, ROOT)
    connection = sqlite3.connect(":memory:")
    connection.executescript(DDL); connection.executescript(INDEX_DDL)
    connection.executemany("INSERT INTO entity VALUES (?,?,?,?,?,?,?,?,?)", ((record.id, record.kind, record.title, "world", "canonical", record.source_path, None, record.body, "{}") for record in world.records.values()))
    insert_spatial_index(connection, build_spatial_projection(world))
    return SpatialStore(connection, "revision-a")


ROOT_LOCATION = "loc_00000000000000000000000000"
GATE_LOCATION = "location:gate"


def _entity(store: SpatialStore, ident: str, kind: str, ordinal: int) -> None:
    store.connection.execute(
        "INSERT INTO entity VALUES (?,?,?,?,?,?,?,?,?)",
        (ident, kind, ident, "world", "canonical", f"story/generated-{kind}-{ordinal}-{ident}.md", None, "", "{}"),
    )


def _location(store: SpatialStore, ident: str, ordinal: int, *, parent: str | None = None,
              map_id: str | None = None, point: tuple[int | float, int | float] | None = None) -> None:
    _entity(store, ident, "location", ordinal)
    spatial = point is not None
    x, y = point if point is not None else (None, None)
    store.connection.execute(
        "INSERT INTO spatial_location VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        (ident, ordinal, int(spatial), parent, map_id, "point" if spatial else None,
         x, y, None, x, y, None, f"[{x},{y}]" if spatial else None, "{}"),
    )


def _route(store: SpatialStore, ident: str, ordinal: int, start: str, target: str,
           value: int | float, *, unit: str = "pace", mode: str = "foot") -> None:
    _entity(store, ident, "route", ordinal)
    store.connection.execute(
        "INSERT INTO spatial_route VALUES (?,?,?,?,?,'[\"foot\"]','open','exact',?,?,NULL,NULL,NULL,NULL,'{}')",
        (ident, ordinal, start, target, "one-way", value, unit),
    )
    store.connection.execute("INSERT INTO spatial_route_edge VALUES (?,?,?,0)", (ident, start, target))
    store.connection.execute("INSERT INTO spatial_route_mode VALUES (?,?,0)", (ident, mode))


def test_hierarchy_is_authored_parent_path_and_coordinate_free_locations_are_readable() -> None:
    store = _store()
    result = store.containment("location:gate")
    assert result.kind is SpatialOutcomeKind.OK
    assert result.value.ids == ("loc_00000000000000000000000000", "location:gate")
    assert store.containment("loc_00000000000000000000000000").value.ids == ("loc_00000000000000000000000000",)


def test_bbox_same_map_pagination_and_cursor_revision_binding() -> None:
    store = _store()
    first = store.bbox("map:town", BoundingBox((-1, -1), (1, 1)), limit=1)
    assert first.kind is SpatialOutcomeKind.OK
    assert first.value.ids == ("location:gate",)
    assert first.value.units == "pace"
    assert first.value.cursor is None
    changed = SpatialStore(store.connection, "revision-b").bbox("map:town", BoundingBox((-1, -1), (1, 1)), limit=1, cursor="eyJiaW5kaW5nIjoiZmFrZSIsImxhc3QiOlswLCJ4Il19")
    assert changed.kind is SpatialOutcomeKind.INVALID and changed.reason is SpatialReason.CURSOR


def test_bbox_requires_geometry_capability_and_map_dimensionality() -> None:
    store = _store()
    store.connection.execute("DELETE FROM spatial_capability WHERE name='geometry-v1'")
    assert SpatialStore(store.connection, "revision-a").bbox("map:town", BoundingBox((0, 0), (0, 0))).kind is SpatialOutcomeKind.UNAVAILABLE
    store.connection.execute("INSERT INTO spatial_capability VALUES ('geometry-v1', 1)")
    result = store.bbox("map:town", BoundingBox((0, 0, 0), (0, 0, 0)))
    assert result.kind is SpatialOutcomeKind.UNAVAILABLE and result.reason is SpatialReason.GEOMETRY


def test_nearby_is_same_map_candidate_read_not_authored_topology() -> None:
    store = _store()
    result = store.nearby(MapPosition("map:town", (0, 0)), maximum=1)
    assert result.kind is SpatialOutcomeKind.OK
    assert result.value.ids == ("location:gate",)
    assert result.value.basis == "same-map-authored-geometry"


def test_directed_path_does_not_infer_reverse_and_unknown_metric_is_closed() -> None:
    store = _store()
    forward = store.path("loc_00000000000000000000000000", "location:gate", metric="route_distance")
    assert forward.kind is SpatialOutcomeKind.UNAVAILABLE and forward.reason is SpatialReason.METRIC
    assert forward.subreason is SpatialSubreason.UNKNOWN_METRIC
    reverse = store.path("location:gate", "loc_00000000000000000000000000", metric="route_distance")
    assert reverse.kind is SpatialOutcomeKind.UNAVAILABLE and reverse.reason is SpatialReason.PATH
    assert reverse.subreason is SpatialSubreason.UNREACHABLE


def test_typed_path_has_stable_ties_and_respects_mode_and_availability() -> None:
    store = _store()
    store.connection.execute("UPDATE spatial_route SET route_distance=5,route_distance_unit='pace',availability='open'")
    result = store.path("loc_00000000000000000000000000", "location:gate", metric="route_distance", modes=["horse"])
    assert result.kind is SpatialOutcomeKind.OK
    assert result.value.route_ids == ("route:place-gate",)
    assert result.value.metric.value == 5 and result.value.metric.unit == "pace"
    store.connection.execute("UPDATE spatial_route SET availability='closed'")
    closed = store.path("loc_00000000000000000000000000", "location:gate", metric="route_distance")
    assert closed.kind is SpatialOutcomeKind.UNAVAILABLE and closed.reason is SpatialReason.PATH
    assert closed.subreason is SpatialSubreason.CLOSED_EDGE
    store.connection.execute("UPDATE spatial_route SET availability='restricted'")
    restricted = store.path("loc_00000000000000000000000000", "location:gate", metric="route_distance")
    assert restricted.kind is SpatialOutcomeKind.UNAVAILABLE and restricted.subreason is SpatialSubreason.UNAVAILABLE_EDGE


def test_path_keeps_incompatible_units_in_separate_typed_graphs() -> None:
    store = _store()
    store.connection.execute("UPDATE spatial_route SET route_distance=5,route_distance_unit='pace'")
    store.connection.execute("INSERT INTO entity VALUES ('route:other','route','other','world','canonical','story/extra.md',NULL,'','{}')")
    store.connection.execute("INSERT INTO spatial_route VALUES ('route:other',99,'location:gate','loc_00000000000000000000000000','one-way','[\"foot\"]','open','exact',2,'mile',NULL,NULL,NULL,NULL,'{}')")
    store.connection.execute("INSERT INTO spatial_route_edge VALUES ('route:other','location:gate','loc_00000000000000000000000000',0)")
    store.connection.execute("INSERT INTO spatial_route_mode VALUES ('route:other','foot',0)")
    result = store.path("loc_00000000000000000000000000", "location:gate", metric="route_distance")
    assert result.kind is SpatialOutcomeKind.OK and result.value.metric.unit == "pace"


def test_path_refuses_to_rank_mixed_units_and_preserves_requested_identity_unit() -> None:
    store = _store()
    store.connection.execute("UPDATE spatial_route SET availability='closed'")
    _route(store, "route:pace", 20, ROOT_LOCATION, GATE_LOCATION, 10, unit="pace")
    _route(store, "route:mile", 21, ROOT_LOCATION, GATE_LOCATION, 1, unit="mile")
    ambiguous = store.path(ROOT_LOCATION, GATE_LOCATION, metric="route_distance")
    assert ambiguous.kind is SpatialOutcomeKind.UNAVAILABLE and ambiguous.reason is SpatialReason.METRIC
    assert ambiguous.subreason is SpatialSubreason.AMBIGUOUS_UNIT

    absent = store.path(ROOT_LOCATION, GATE_LOCATION, metric="route_distance", unit="league")
    assert absent.kind is SpatialOutcomeKind.UNAVAILABLE and absent.reason is SpatialReason.METRIC
    assert absent.subreason is SpatialSubreason.INCOMPATIBLE_UNIT

    identity = store.path(ROOT_LOCATION, ROOT_LOCATION, metric="route_distance", unit="pace")
    assert identity.kind is SpatialOutcomeKind.OK
    assert identity.value.metric.value == 0 and identity.value.metric.unit == "pace"


def test_path_reports_downstream_closed_and_restricted_edges() -> None:
    store = _store()
    store.connection.execute("DELETE FROM spatial_route_edge WHERE route_id='route:place-gate'")
    store.connection.execute("DELETE FROM spatial_route_mode WHERE route_id='route:place-gate'")
    store.connection.execute("DELETE FROM spatial_route WHERE id='route:place-gate'")
    _location(store, "location:downstream", 20)
    _route(store, "route:downstream-open", 20, ROOT_LOCATION, "location:downstream", 1)
    _route(store, "route:downstream-blocked", 21, "location:downstream", GATE_LOCATION, 1)
    store.connection.execute("UPDATE spatial_route SET availability='closed' WHERE id='route:downstream-blocked'")
    closed = store.path(ROOT_LOCATION, GATE_LOCATION, metric="route_distance")
    assert closed.kind is SpatialOutcomeKind.UNAVAILABLE and closed.reason is SpatialReason.PATH
    assert closed.subreason is SpatialSubreason.CLOSED_EDGE

    store.connection.execute("UPDATE spatial_route SET availability='restricted' WHERE id='route:downstream-blocked'")
    restricted = store.path(ROOT_LOCATION, GATE_LOCATION, metric="route_distance")
    assert restricted.kind is SpatialOutcomeKind.UNAVAILABLE and restricted.reason is SpatialReason.PATH
    assert restricted.subreason is SpatialSubreason.UNAVAILABLE_EDGE


def test_overlay_filters_time_audience_perspective_before_visibility_and_closes_explicit_ids() -> None:
    store = _store()
    at = StoryTime("main", 0, 0)
    hidden = store.overlay_as_of("location:gate", at, audience="public", perspective="ordinary")
    assert hidden.kind is SpatialOutcomeKind.OK and hidden.value.ids == ()
    unauthorized = store.overlay_as_of("location:gate", at, audience="public", perspective="ordinary", overlay_id="overlay:ward")
    missing = store.overlay_as_of("location:gate", at, audience="public", perspective="ordinary", overlay_id="overlay:missing")
    # The complete outcomes, including every transport-consumed stable field,
    # must be identical: explicit IDs cannot be enumerated by authorization.
    assert unauthorized == missing
    assert unauthorized.kind is SpatialOutcomeKind.FORBIDDEN
    assert unauthorized.reason is SpatialReason.OVERLAY
    assert unauthorized.subreason is None
    assert unauthorized.detail == "explicit overlay is not authorized in this scope"

    # Authorization is separate from applicability.  A caller allowed to name
    # the overlay gets a normal empty result when the location or horizon does
    # not apply, rather than a forbidden or unavailable answer.
    out_of_location = store.overlay_as_of(ROOT_LOCATION, at, audience="author", perspective="author", overlay_id="overlay:ward")
    out_of_horizon = store.overlay_as_of(GATE_LOCATION, StoryTime("main", 2, 0), audience="author", perspective="author", overlay_id="overlay:ward")
    assert out_of_location.kind is SpatialOutcomeKind.OK and out_of_location.value.ids == ()
    assert out_of_horizon.kind is SpatialOutcomeKind.OK and out_of_horizon.value.ids == ()

    # Catalogue reads do not perform a separate record lookup that could turn
    # hidden membership into an identifier oracle.
    statements: list[str] = []
    store.connection.set_trace_callback(statements.append)
    catalogue_empty = store.overlay_as_of(ROOT_LOCATION, at, audience="public", perspective="ordinary")
    store.connection.set_trace_callback(None)
    assert catalogue_empty.kind is SpatialOutcomeKind.OK and catalogue_empty.value.ids == ()
    assert not any("FROM spatial_overlay WHERE id=" in statement for statement in statements)

    allowed = store.overlay_as_of("location:gate", at, audience="author", perspective="author")
    assert allowed.kind is SpatialOutcomeKind.OK and allowed.value.ids == ("overlay:ward",)
    explicit_allowed = store.overlay_as_of("location:gate", at, audience="author", perspective="author", overlay_id="overlay:ward")
    assert explicit_allowed.kind is SpatialOutcomeKind.OK and explicit_allowed.value.ids == ("overlay:ward",)


def test_overlay_story_time_never_crosses_timelines_or_converts_duration() -> None:
    store = _store()
    result = store.overlay_as_of("location:gate", StoryTime("other", 0, 0), audience="author", perspective="author")
    assert result.kind is SpatialOutcomeKind.OK and result.value.ids == ()
    invalid = store.overlay_as_of("location:gate", {"timeline": "main", "tick": "0"}, audience="author", perspective="author")
    assert invalid.kind is SpatialOutcomeKind.INVALID


def test_limits_are_closed_without_partial_route_or_overlay_answers() -> None:
    store = _store()
    assert store.bbox("map:town", BoundingBox((0, 0), (0, 0)), limit=101).kind is SpatialOutcomeKind.LIMIT
    for ordinal in range(2_001):
        ident = f"overlay:extra-{ordinal}"
        store.connection.execute("INSERT INTO entity VALUES (?,?,?,?,?,?,?,?,?)", (ident, "overlay", ident, "world", "canonical", f"story/{ident}.md", None, "", "{}"))
        store.connection.execute("INSERT INTO spatial_overlay VALUES (?,?, 'static','[\"author\"]','[\"author\"]',NULL,NULL,NULL,NULL,NULL,'{}')", (ident, ordinal + 100))
        store.connection.execute("INSERT INTO spatial_overlay_location VALUES (?,?,0)", (ident, "location:gate"))
        store.connection.execute("INSERT INTO spatial_overlay_audience VALUES (?, 'author', 0)", (ident,))
        store.connection.execute("INSERT INTO spatial_overlay_perspective VALUES (?, 'author', 0)", (ident,))
    overlays = store.overlay_as_of("location:gate", StoryTime("main", 0, 0), audience="author", perspective="author")
    assert overlays.kind is SpatialOutcomeKind.LIMIT and overlays.reason is SpatialReason.LIMIT
    # Authorization is evaluated before the budget: a caller who cannot see
    # those records gets the same successful empty catalogue as no overlays.
    hidden = store.overlay_as_of("location:gate", StoryTime("main", 0, 0), audience="public", perspective="ordinary")
    assert hidden.kind is SpatialOutcomeKind.OK and hidden.value.ids == ()


def test_children_bbox_within_adjacency_and_portal_reachability_are_authored_only() -> None:
    store = _store()
    assert store.children("loc_00000000000000000000000000").value.ids == ("location:gate",)
    assert store.bbox("map:town", BoundingBox((0, 0), (0, 0)), relation="within").value.ids == ("location:gate",)
    adjacency = store.adjacency("loc_00000000000000000000000000")
    assert adjacency.kind is SpatialOutcomeKind.OK
    assert adjacency.value.route_ids == ("route:place-gate",)
    # The return portal is directed and explicitly authored, so reachability
    # sees it. It does not make a new reverse route.
    reached = store.reachability("location:gate")
    assert reached.kind is SpatialOutcomeKind.OK
    assert reached.value.ids == ("loc_00000000000000000000000000", "location:gate")


def test_nearby_target_radius_aliases_and_exact_radius_postfilter() -> None:
    store = _store()
    assert store.nearby(target=MapPosition("map:town", (2, 0)), radius=1).value.ids == ()
    assert store.nearby(position=MapPosition("map:town", (1, 0)), maximum=1).value.ids == ("location:gate",)

    # Btree bounds only nominate candidates: this line's bounds intersect the
    # unit circle but no point of its actual authored geometry does.
    _entity(store, "location:line-corner", "location", 40)
    store.connection.execute(
        "INSERT INTO spatial_location VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        ("location:line-corner", 40, 1, None, "map:town", "line", .9, .9, None, 1, 1, None, "[[0.9,0.9],[1,1]]", "{}"),
    )
    exact = store.nearby(MapPosition("map:town", (0, 0)), radius=1)
    assert exact.kind is SpatialOutcomeKind.OK and "location:line-corner" not in exact.value.ids

    store.connection.execute("UPDATE spatial_location SET geometry_json=NULL WHERE id=?", (GATE_LOCATION,))
    unknown = store.nearby(MapPosition("map:town", (0, 0)), radius=1)
    assert unknown.kind is SpatialOutcomeKind.UNAVAILABLE
    assert unknown.reason is SpatialReason.GEOMETRY
    assert unknown.subreason is SpatialSubreason.UNKNOWN_COORDINATE


def test_success_summaries_expose_closed_partial_unknown_filter_and_horizon_semantics() -> None:
    store = _store()
    catalogue = store.nearby(MapPosition("map:town", (0, 0)), radius=1)
    assert catalogue.kind is SpatialOutcomeKind.OK
    assert catalogue.value.partial is False and catalogue.value.unknown is False

    path = store.path(ROOT_LOCATION, GATE_LOCATION, metric="route_distance")
    assert path.kind is SpatialOutcomeKind.UNAVAILABLE
    assert path.subreason is SpatialSubreason.UNKNOWN_METRIC
    store.connection.execute("UPDATE spatial_route SET route_distance=1,route_distance_unit='pace'")
    path = store.path(ROOT_LOCATION, GATE_LOCATION, metric="route_distance", modes=["horse"])
    assert path.kind is SpatialOutcomeKind.OK
    assert path.value.filters == {"modes": ["horse"], "availability": ["open"]}
    assert path.value.partial is False and path.value.unknown is False
    assert path.value.metric.complete is True
    assert path.value.metric.partial is False and path.value.metric.unknown is False

    adjacency = store.adjacency(ROOT_LOCATION, modes=["foot"])
    reachable = store.reachability(ROOT_LOCATION, modes=["foot"])
    assert adjacency.value.filters == reachable.value.filters == {"modes": ["foot"], "availability": ["open"]}
    assert adjacency.value.partial is False and adjacency.value.unknown is False
    assert reachable.value.partial is False and reachable.value.unknown is False
    at = StoryTime("main", 0, 0)
    overlay = store.overlay_as_of(GATE_LOCATION, at, audience="author", perspective="author")
    assert overlay.kind is SpatialOutcomeKind.OK
    assert overlay.value.filters["horizon"] == at.to_dict()
    assert overlay.value.partial is False and overlay.value.unknown is False


def test_query_limits_are_closed_at_zero_maximum_plus_one_and_path_overlength() -> None:
    store = _store()
    for limit, expected in ((0, SpatialOutcomeKind.LIMIT), (100, SpatialOutcomeKind.OK), (101, SpatialOutcomeKind.LIMIT)):
        assert store.nearby(MapPosition("map:town", (0, 0)), radius=1, limit=limit).kind is expected
        assert store.path(ROOT_LOCATION, GATE_LOCATION, metric="route_distance", limit=limit).kind in {
            expected, SpatialOutcomeKind.UNAVAILABLE,
        }
    store.connection.execute("UPDATE spatial_route SET route_distance=1,route_distance_unit='pace'")
    previous = ROOT_LOCATION
    for index in range(100):
        ident = f"location:chain-{index:03d}"
        _location(store, ident, 1_000 + index)
        _route(store, f"route:chain-{index:03d}", 2_000 + index, previous, ident, 1)
        previous = ident
    exact_max = store.path(ROOT_LOCATION, "location:chain-098", metric="route_distance", limit=100)
    assert exact_max.kind is SpatialOutcomeKind.OK and len(exact_max.value.ids) == 100
    overlength = store.path(ROOT_LOCATION, "location:chain-099", metric="route_distance", limit=100)
    assert overlength.kind is SpatialOutcomeKind.LIMIT and overlength.reason is SpatialReason.LIMIT and overlength.value is None


def test_path_preserves_exact_large_integers_overflow_and_near_equal_ordering() -> None:
    store = _store()
    for value in (2**53 - 1, 2**53 + 1, 2**63 - 1):
        store.connection.execute(
            "UPDATE spatial_route SET route_distance=?,route_distance_unit='pace',availability='open' WHERE id='route:place-gate'",
            (value,),
        )
        result = store.path(ROOT_LOCATION, GATE_LOCATION, metric="route_distance")
        assert result.kind is SpatialOutcomeKind.OK and result.value.metric.value == value

    store.connection.execute("UPDATE spatial_route SET availability='closed'")
    _location(store, "location:overflow-middle", 20)
    _route(store, "route:overflow-a", 20, ROOT_LOCATION, "location:overflow-middle", 2**63 - 1)
    _route(store, "route:overflow-b", 21, "location:overflow-middle", GATE_LOCATION, 2**63 - 1)
    overflow = store.path(ROOT_LOCATION, GATE_LOCATION, metric="route_distance")
    assert overflow.kind is SpatialOutcomeKind.OK
    assert overflow.value.metric.value == 2 * (2**63 - 1)

    competition = _store()
    competition.connection.execute("UPDATE spatial_route SET availability='closed'")
    _location(competition, "location:integer-a", 30)
    _location(competition, "location:integer-b", 31)
    # The cheaper path deliberately has the later authored tie key. Eager float
    # coercion collapses these totals around 2**53 and chooses the wrong path.
    _route(competition, "route:integer-b1", 20, ROOT_LOCATION, "location:integer-b", 2**53)
    _route(competition, "route:integer-b2", 21, "location:integer-b", GATE_LOCATION, 2)
    _route(competition, "route:integer-a1", 30, ROOT_LOCATION, "location:integer-a", 2**53 + 1)
    _route(competition, "route:integer-a2", 31, "location:integer-a", GATE_LOCATION, 0)
    exact = competition.path(ROOT_LOCATION, GATE_LOCATION, metric="route_distance")
    assert exact.kind is SpatialOutcomeKind.OK
    assert exact.value.ids == (ROOT_LOCATION, "location:integer-a", GATE_LOCATION)
    assert exact.value.metric.value == 2**53 + 1


def test_path_uses_exact_mixed_float_comparison_and_authored_tie_keys() -> None:
    store = _store()
    store.connection.execute("UPDATE spatial_route SET availability='closed'")
    _location(store, "location:mixed", 30)
    _route(store, "route:mixed-direct", 10, ROOT_LOCATION, GATE_LOCATION, 0.30000000000000004)
    _route(store, "route:mixed-a", 20, ROOT_LOCATION, "location:mixed", 0.1)
    _route(store, "route:mixed-b", 21, "location:mixed", GATE_LOCATION, 0.2)
    result = store.path(ROOT_LOCATION, GATE_LOCATION, metric="route_distance")
    assert result.kind is SpatialOutcomeKind.OK
    assert result.value.route_ids == ("route:mixed-a", "route:mixed-b")

    tied = _store()
    tied.connection.execute("UPDATE spatial_route SET availability='closed'")
    _location(tied, "location:tie-first", 40)
    _location(tied, "location:tie-second", 41)
    for ident, ordinal, middle in (("first", 20, "location:tie-first"), ("second", 30, "location:tie-second")):
        _route(tied, f"route:{ident}-a", ordinal, ROOT_LOCATION, middle, 2)
        _route(tied, f"route:{ident}-b", ordinal + 1, middle, GATE_LOCATION, 3)
    tie = tied.path(ROOT_LOCATION, GATE_LOCATION, metric="route_distance")
    assert tie.kind is SpatialOutcomeKind.OK
    assert tie.value.route_ids == ("route:first-a", "route:first-b")


def test_absent_endpoints_are_unavailable_but_valid_empty_controls_stay_ok() -> None:
    store = _store()
    at = StoryTime("main", 0, 0)
    assert store.adjacency("location:missing").kind is SpatialOutcomeKind.UNAVAILABLE
    assert store.reachability("location:missing").kind is SpatialOutcomeKind.UNAVAILABLE
    missing_overlay = store.overlay_as_of("location:missing", at, audience="author", perspective="author")
    assert missing_overlay.kind is SpatialOutcomeKind.UNAVAILABLE and missing_overlay.reason is SpatialReason.OVERLAY

    _location(store, "location:disconnected", 30)
    adjacency = store.adjacency("location:disconnected")
    reachable = store.reachability("location:disconnected")
    overlay = store.overlay_as_of("location:disconnected", at, audience="author", perspective="author")
    assert adjacency.kind is SpatialOutcomeKind.OK and adjacency.value.target_location_ids == ()
    assert reachable.kind is SpatialOutcomeKind.OK and reachable.value.ids == ("location:disconnected",)
    assert overlay.kind is SpatialOutcomeKind.OK and overlay.value.ids == ()


def test_containment_uses_bounded_primary_key_walk_and_closes_corrupt_caches() -> None:
    store = _store()
    statements: list[str] = []
    store.connection.set_trace_callback(statements.append)
    result = store.containment(GATE_LOCATION)
    store.connection.set_trace_callback(None)
    assert result.kind is SpatialOutcomeKind.OK
    location_reads = [statement for statement in statements if "FROM spatial_location" in statement]
    assert len(location_reads) == 2
    assert all("WHERE id=" in statement for statement in location_reads)

    store.connection.execute("UPDATE spatial_location SET parent_id=? WHERE id=?", (GATE_LOCATION, ROOT_LOCATION))
    cyclic = store.containment(GATE_LOCATION)
    assert cyclic.kind is SpatialOutcomeKind.UNAVAILABLE and "cyclic" in cyclic.detail

    stale = _store()
    stale.connection.execute("UPDATE spatial_location SET parent_id='location:missing' WHERE id=?", (GATE_LOCATION,))
    corrupt = stale.containment(GATE_LOCATION)
    assert corrupt.kind is SpatialOutcomeKind.UNAVAILABLE and "reference" in corrupt.detail


def test_context_preserves_validated_registry_order_for_combined_capabilities() -> None:
    store = _store()
    store.connection.execute("UPDATE spatial_capability SET source_ordinal=source_ordinal+10")
    store.connection.execute("INSERT INTO spatial_capability VALUES ('generational-core-v1',0)")
    store.connection.execute("UPDATE spatial_capability SET source_ordinal=source_ordinal-9 WHERE name!='generational-core-v1'")
    combined = SpatialStore(store.connection, "revision-combined")
    assert combined.context.capabilities == (
        "generational-core-v1", "spatial-core-v1", "geometry-v1", "route-v1", "overlay-v1",
    )

    corrupt = _store()
    corrupt.connection.execute("UPDATE spatial_capability SET source_ordinal=100-source_ordinal")
    with pytest.raises(ValueError, match="registry order"):
        SpatialStore(corrupt.connection, "revision-corrupt")


def test_positive_keyset_cursor_paging_is_complete_and_request_bound() -> None:
    store = _store()
    _location(store, "location:page-a", 30, map_id="map:town", point=(0, 0))
    _location(store, "location:page-b", 31, map_id="map:town", point=(0, 0))
    bounds = BoundingBox((-1, -1), (1, 1))
    first = store.bbox("map:town", bounds, limit=1)
    second = store.bbox("map:town", bounds, limit=1, cursor=first.value.cursor)
    third = store.bbox("map:town", bounds, limit=1, cursor=second.value.cursor)
    assert first.value.ids + second.value.ids + third.value.ids == (
        GATE_LOCATION, "location:page-a", "location:page-b",
    )
    assert third.value.cursor is None
    rebound = store.bbox("map:town", BoundingBox((-2, -2), (2, 2)), limit=1, cursor=first.value.cursor)
    assert rebound.kind is SpatialOutcomeKind.INVALID and rebound.reason is SpatialReason.CURSOR
    _location(store, "location:cursor-parent", 90)
    expected = ("location:z-child", "location:a-child", "location:m-child")
    for index, ident in enumerate(expected):
        _location(store, ident, 91 + index, parent="location:cursor-parent")
    first = store.children("location:cursor-parent", limit=1)
    second = store.children("location:cursor-parent", limit=1, cursor=first.value.cursor)
    final = store.children("location:cursor-parent", limit=1, cursor=second.value.cursor)
    assert first.value.ids + second.value.ids + final.value.ids == expected
    assert final.value.cursor is None
    for reader, parent, limit, cursor in (
        (store, "location:cursor-parent", 1, "malformed"),
        (store, "location:cursor-parent", 2, first.value.cursor),
        (store, expected[0], 1, first.value.cursor),
        (SpatialStore(store.connection, "new-revision"), "location:cursor-parent", 1, first.value.cursor),
    ):
        invalid = reader.children(parent, limit=limit, cursor=cursor)
        assert invalid.kind is SpatialOutcomeKind.INVALID and invalid.reason is SpatialReason.CURSOR
        assert invalid.value is None
    assert store.children(expected[0]).value.ids == ()
    assert store.children("location:missing-parent").kind is SpatialOutcomeKind.UNAVAILABLE
    for limit in (0, 101):
        closed = store.children("location:cursor-parent", limit=limit)
        assert closed.kind is SpatialOutcomeKind.LIMIT and closed.value == ()


def test_signed_i64_story_time_boundaries_and_cross_map_discontinuity_are_closed() -> None:
    store = _store()
    for tick in (-(2**63), 2**63 - 1):
        result = store.overlay_as_of(GATE_LOCATION, {"timeline": "main", "tick": tick, "order": 0}, audience="author", perspective="author")
        assert result.kind is SpatialOutcomeKind.OK and result.value.story_time.tick == tick
    for tick in (-(2**63) - 1, 2**63):
        assert store.overlay_as_of(GATE_LOCATION, {"timeline": "main", "tick": tick, "order": 0}, audience="author", perspective="author").kind is SpatialOutcomeKind.INVALID

    _location(store, "location:earth", 30, map_id="map:earth", point=(12, 55))
    nearby = store.nearby(MapPosition("map:town", (12, 55)), radius=0)
    assert "location:earth" not in nearby.value.ids
    path = store.path(GATE_LOCATION, "location:earth", metric="route_distance")
    assert path.kind is SpatialOutcomeKind.UNAVAILABLE and path.reason is SpatialReason.PATH
    assert path.subreason is SpatialSubreason.CROSS_MAP_DISCONTINUITY


def test_canonical_query_vector_and_source_compiled_fixture_stay_in_parity() -> None:
    vector = yaml.safe_load((ROOT / "tests/fixtures/architecture/spatial-query-v1.yaml").read_text())
    fixture = yaml.safe_load((ROOT / "tests/fixtures/spatial_v07/valid-multimap.yaml").read_text())
    store = _store()
    assert vector["protocol"] == store.context.protocol
    assert tuple(vector["common"]["capabilities"]) == store.context.capabilities
    assert vector["common"]["limit"] == {"minimum": 1, "maximum": 100}

    locations = {item["id"]: item for item in fixture["locations"]}
    expected_path = (locations[GATE_LOCATION]["parent_id"], GATE_LOCATION)
    assert store.containment(GATE_LOCATION).value.ids == expected_path
    assert fixture["route"]["direction"] == "one-way"
    assert store.path(GATE_LOCATION, ROOT_LOCATION, metric="route_distance").reason is SpatialReason.PATH
    assert fixture["overlay"]["audience"] == ["author"]
    hidden = store.overlay_as_of(GATE_LOCATION, StoryTime("main", 0, 0), audience="public", perspective="ordinary")
    assert hidden.kind is SpatialOutcomeKind.OK and hidden.value.ids == ()


def test_repeated_path_order_is_stable_across_cycles_and_insertion_order() -> None:
    store = _store()
    store.connection.execute("UPDATE spatial_route SET availability='closed'")
    for index in range(4):
        _location(store, f"location:property-{index}", 40 + index)
    authored = [
        ("route:property-3", 33, ROOT_LOCATION, "location:property-1", 1),
        ("route:property-2", 32, "location:property-0", GATE_LOCATION, 1),
        ("route:property-1", 31, ROOT_LOCATION, "location:property-0", 1),
        ("route:property-cycle", 34, "location:property-0", ROOT_LOCATION, 0),
        ("route:property-4", 35, "location:property-1", GATE_LOCATION, 1),
    ]
    for args in authored:
        _route(store, *args)
    results = [store.path(ROOT_LOCATION, GATE_LOCATION, metric="route_distance") for _ in range(8)]
    assert all(result.kind is SpatialOutcomeKind.OK for result in results)
    assert {result.value.route_ids for result in results} == {("route:property-1", "route:property-2")}
