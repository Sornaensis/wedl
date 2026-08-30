from __future__ import annotations

from pathlib import Path
import sqlite3
import importlib.util

import pytest
import yaml

from wedl import SQLITE_SCHEMA
from wedl.compiler import DDL, INDEX_DDL, cache_readiness, compile_world, require_database
from wedl.errors import CompileRequired, ValidationFailed
from wedl.model import Record, World
from wedl.repository import Repository
from wedl.spatial_index import build_spatial_projection, insert_spatial_index, install_optional_spatial_index
from wedl.source import serialize_record


ROOT = Path(__file__).parents[1]
_BENCHMARK_SPEC = importlib.util.spec_from_file_location("spatial_index_benchmark", ROOT / "tools" / "benchmark_spatial_index.py")
assert _BENCHMARK_SPEC and _BENCHMARK_SPEC.loader
_BENCHMARK = importlib.util.module_from_spec(_BENCHMARK_SPEC)
_BENCHMARK_SPEC.loader.exec_module(_BENCHMARK)


def _world() -> World:
    value = yaml.safe_load((ROOT / "tests/fixtures/spatial_v07/valid-multimap.yaml").read_text())
    records = [value["world"], *value["maps"], *value["locations"], value["anchor"], value["portal"], value["route"], value["overlay"]]
    typed = [Record(item, "", f"story/{index}.md", b"") for index, item in enumerate(records)]
    return World("component", "tree", {record.id: record for record in typed}, ROOT)


def _connection(world: World) -> sqlite3.Connection:
    connection = sqlite3.connect(":memory:")
    connection.executescript(DDL)
    connection.executescript(INDEX_DDL)
    connection.executemany(
        "INSERT INTO entity VALUES (?,?,?,?,?,?,?,?,?)",
        ((record.id, record.kind, record.title, "world", "canonical", record.source_path, None, record.body, "{}") for record in world.records.values()),
    )
    return connection


_PROJECTION_TABLES = (
    ("capabilities", "spatial_capability"),
    ("maps", "spatial_map"),
    ("locations", "spatial_location"),
    ("vertices", "spatial_location_vertex"),
    ("hierarchy", "spatial_hierarchy"),
    ("location_links", "spatial_location_link"),
    ("routes", "spatial_route"),
    ("route_edges", "spatial_route_edge"),
    ("route_modes", "spatial_route_mode"),
    ("anchors", "spatial_anchor"),
    ("portals", "spatial_portal"),
    ("portal_modes", "spatial_portal_mode"),
    ("overlays", "spatial_overlay"),
    ("overlay_locations", "spatial_overlay_location"),
    ("overlay_audiences", "spatial_overlay_audience"),
    ("overlay_perspectives", "spatial_overlay_perspective"),
)


def _compiled_rows(connection: sqlite3.Connection) -> dict[str, tuple[tuple[object, ...], ...]]:
    return {
        attribute: tuple(connection.execute(f'SELECT * FROM "{table}" ORDER BY rowid'))
        for attribute, table in _PROJECTION_TABLES
    }


def test_projection_is_deterministic_and_preserves_authored_direction_without_closure() -> None:
    world = _world()
    first = build_spatial_projection(world)
    assert first == build_spatial_projection(world)
    assert [(row[2], row[3], row[4]) for row in first.routes] == [("loc_00000000000000000000000000", "location:gate", "one-way")]
    assert first.route_edges == (("route:place-gate", "loc_00000000000000000000000000", "location:gate", 0),)
    assert first.hierarchy == (("location:gate", "loc_00000000000000000000000000", 4),)
    assert all("closure" not in row[-1] for row in first.routes)


def test_projection_and_ddl_have_source_parity_and_indexed_candidate_shapes() -> None:
    world = _world()
    projection = build_spatial_projection(world)
    connection = _connection(world)
    try:
        stats = insert_spatial_index(connection, projection, batch_size=2)
        assert stats["spatialLocationCount"] == 2
        assert connection.execute("PRAGMA foreign_key_check").fetchall() == []
        assert connection.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        assert connection.execute("SELECT id,map_id,geometry_kind FROM spatial_location ORDER BY source_ordinal").fetchall() == [
            ("loc_00000000000000000000000000", None, None), ("location:gate", "map:town", "point")
        ]
        assert connection.execute("SELECT has_spatial FROM spatial_location WHERE id=?", ("loc_00000000000000000000000000",)).fetchone() == (0,)
        assert connection.execute("SELECT x,y,z FROM spatial_location_vertex WHERE location_id=?", ("location:gate",)).fetchall() == [(0.0, 0.0, None)]
        assert connection.execute("SELECT mode FROM spatial_route_mode ORDER BY source_ordinal", ()).fetchall() == [("foot",), ("horse",)]
        plan = connection.execute("EXPLAIN QUERY PLAN SELECT id FROM spatial_location WHERE map_id=? AND min_x<=? AND max_x>=?", ("map:town", 0, 0)).fetchall()
        assert any("spatial_location_map_bounds_idx" in item[-1] for item in plan)
        assert stats["spatialBoundsIndex"] in {"rtree", "btree"}
        assert _compiled_rows(connection) == {
            attribute: getattr(projection, attribute) for attribute, _table in _PROJECTION_TABLES
        }
    finally:
        connection.close()


def test_optional_rtree_failure_has_the_portable_btree_fallback() -> None:
    class NoRTree:
        def execute(self, _sql: str) -> None:
            raise sqlite3.OperationalError("no such module: rtree")
    assert install_optional_spatial_index(NoRTree()) == "btree"  # type: ignore[arg-type]


def test_benchmark_smoke_has_json_safe_bounded_batch_evidence() -> None:
    result = _BENCHMARK.measure(_BENCHMARK.synthetic_projection(locations=16, maps=2, routes=32, portals=2, overlays=4), repeats=2)
    assert result["spatialLocationCount"] == 16
    assert result["spatialRouteCount"] == 32
    assert result["spatialInsertBatches"] > 0
    assert result["spatialBoundsIndex"] in {"rtree", "btree"}
    assert result["databaseBytes"] > 0
    assert len(result["databaseDigest"]) == len(result["rowDigest"]) == 64
    expected_indexes = {
        "mapBounds": "spatial_location_map_bounds_idx",
        "hierarchyParent": "spatial_hierarchy_parent_idx",
        "locationLinkTarget": "spatial_location_link_target_idx",
        "routeAdjacency": "spatial_route_edge_from_idx",
        "overlayCandidate": "spatial_overlay_candidate_idx",
    }
    for case, index in expected_indexes.items():
        assert any(index in detail for detail in result["queryPlans"][case])


def test_two_way_route_has_only_its_authored_reverse_edge_and_overlay_interval_is_literal() -> None:
    world = _world()
    route = world.records["route:place-gate"].frontmatter
    route["direction"] = "two-way"
    projection = build_spatial_projection(world)
    assert [(item[1], item[2], item[3]) for item in projection.route_edges] == [
        ("loc_00000000000000000000000000", "location:gate", 0),
        ("location:gate", "loc_00000000000000000000000000", 1),
    ]
    overlay = projection.overlays[0]
    assert overlay[5:10] == ("main", -1, 0, 1, 0)


def test_parent_fk_is_deferred_without_changing_source_ordering() -> None:
    original = _world()
    records = {
        record.id: Record(record.frontmatter, record.body, "story/00-child.md" if record.id == "location:gate" else f"story/99-{record.id}.md", record.raw_bytes)
        for record in original.records.values()
    }
    world = World("component", "tree", records, ROOT)
    projection = build_spatial_projection(world)
    connection = _connection(world)
    try:
        insert_spatial_index(connection, projection)
        connection.commit()
        assert connection.execute("PRAGMA foreign_key_check").fetchall() == []
    finally:
        connection.close()


def test_legacy_parent_and_plain_and_detailed_links_have_exact_normalized_parity() -> None:
    world = _world()
    root = world.records["loc_00000000000000000000000000"].frontmatter
    child = world.records["location:gate"].frontmatter
    child["parent"] = child.pop("parent_id")
    root["links"] = ["location:gate"]
    child["links"] = [{"target": "loc_00000000000000000000000000", "label": "Return road", "summary": "Authored only"}]
    projection = build_spatial_projection(world)
    assert projection.hierarchy == (("location:gate", "loc_00000000000000000000000000", 4),)
    assert projection.location_links == (
        ("loc_00000000000000000000000000", "location:gate", 0, '"location:gate"'),
        ("location:gate", "loc_00000000000000000000000000", 0, '{"label":"Return road","summary":"Authored only","target":"loc_00000000000000000000000000"}'),
    )
    connection = _connection(world)
    try:
        insert_spatial_index(connection, projection)
        assert tuple(connection.execute("SELECT * FROM spatial_location_link ORDER BY rowid")) == projection.location_links
        assert tuple(connection.execute("SELECT location_id,parent_id FROM spatial_hierarchy")) == (
            ("location:gate", "loc_00000000000000000000000000"),
        )
    finally:
        connection.close()


@pytest.mark.parametrize("outside", [-(2**63) - 1, 2**63])
def test_projection_refuses_integer_coordinates_outside_sqlite_signed_i64(outside: int) -> None:
    world = _world()
    world.records["map:town"].frontmatter["bounds"]["max"][0] = outside
    if outside < 0:
        world.records["map:town"].frontmatter["bounds"]["min"][0] = outside
        world.records["map:town"].frontmatter["bounds"]["max"][0] = 10
    with pytest.raises(ValueError, match="outside signed i64"):
        build_spatial_projection(world)


@pytest.mark.parametrize("coordinate", [2**53 - 1, 2**53, 2**53 + 1, -(2**53 + 1), -(2**63), 2**63 - 1])
def test_integer_bounds_remain_exact_and_index_candidates_never_false_negative(coordinate: int) -> None:
    world = _world()
    town = world.records["map:town"].frontmatter
    lower = coordinate if coordinate == -(2**63) else coordinate - 1
    upper = coordinate if coordinate == 2**63 - 1 else coordinate + 1
    town["bounds"] = {"min": [lower, -1], "max": [upper, 1]}
    world.records["location:gate"].frontmatter["spatial"]["geometry"]["coordinates"] = [coordinate, 0]
    world.records["anchor:town-earth"].frontmatter["from"]["coordinates"] = [coordinate, 0]
    projection = build_spatial_projection(world)
    location_row = next(row for row in projection.locations if row[0] == "location:gate")
    assert location_row[6] == location_row[9] == coordinate
    connection = _connection(world)
    try:
        stats = insert_spatial_index(connection, projection)
        assert connection.execute(
            "SELECT min_x,max_x,typeof(min_x),typeof(max_x),geometry_json FROM spatial_location WHERE id='location:gate'"
        ).fetchone() == (coordinate, coordinate, "integer", "integer", f"[{coordinate},0]")
        assert connection.execute(
            "SELECT min_x,max_x,typeof(min_x),typeof(max_x) FROM spatial_map WHERE id='map:town'"
        ).fetchone() == (lower, upper, "integer", "integer")
        statement = (
            "SELECT id FROM spatial_location INDEXED BY spatial_location_map_bounds_idx "
            "WHERE map_id=? AND min_x<=? AND max_x>=? ORDER BY id"
        )
        assert connection.execute(statement, ("map:town", coordinate, coordinate)).fetchall() == [("location:gate",)]
        plan = connection.execute("EXPLAIN QUERY PLAN " + statement, ("map:town", coordinate, coordinate)).fetchall()
        assert any("spatial_location_map_bounds_idx" in row[-1] for row in plan)
        if stats["spatialBoundsIndex"] == "rtree":
            # RTree is candidate-only. SQLite rounds lower bounds down and
            # upper bounds up; the NUMERIC base table supplies the exact
            # post-filter so false positives cannot become exact matches.
            assert connection.execute(
                """
                SELECT location.id
                FROM spatial_location_rtree AS candidate
                JOIN spatial_location AS location ON location.source_ordinal=candidate.source_ordinal
                WHERE candidate.min_x<=? AND candidate.max_x>=?
                  AND location.min_x<=? AND location.max_x>=?
                ORDER BY location.id
                """,
                (coordinate, coordinate, coordinate, coordinate),
            ).fetchall() == [("location:gate",)]
    finally:
        connection.close()


def test_real_bounds_remain_real_and_use_the_same_exact_btree_candidate_path() -> None:
    world = _world()
    world.records["map:town"].frontmatter["bounds"] = {"min": [-0.25, -1.0], "max": [0.25, 1.0]}
    world.records["location:gate"].frontmatter["spatial"]["geometry"]["coordinates"] = [0.125, 0.5]
    world.records["anchor:town-earth"].frontmatter["from"]["coordinates"] = [0.125, 0.5]
    projection = build_spatial_projection(world)
    connection = _connection(world)
    try:
        insert_spatial_index(connection, projection)
        assert connection.execute(
            "SELECT min_x,max_x,typeof(min_x),typeof(max_x) FROM spatial_location WHERE id='location:gate'"
        ).fetchone() == (0.125, 0.125, "real", "real")
        assert connection.execute(
            "SELECT id FROM spatial_location INDEXED BY spatial_location_map_bounds_idx "
            "WHERE map_id='map:town' AND min_x<=0.125 AND max_x>=0.125"
        ).fetchall() == [("location:gate",)]
    finally:
        connection.close()


def test_portal_target_union_is_enforced_by_check_and_foreign_keys() -> None:
    world = _world()
    projection = build_spatial_projection(world)
    connection = _connection(world)
    try:
        insert_spatial_index(connection, projection)
        with pytest.raises(sqlite3.IntegrityError):
            connection.execute(
                "UPDATE spatial_portal SET target_kind='position',target_map_id='map:town',target_coordinates_json=NULL WHERE id='portal:gate-place'"
            )
        with pytest.raises(sqlite3.IntegrityError):
            connection.execute(
                "UPDATE spatial_portal SET target_kind='position',target_location_id=NULL,target_map_id='map:missing',target_coordinates_json='[0,0]' WHERE id='portal:gate-place'"
            )
    finally:
        connection.close()


def test_invalid_component_is_refused_and_non_v07_world_is_empty() -> None:
    world = _world()
    world.records["location:gate"].frontmatter["spatial"]["geometry"]["coordinates"] = [float("inf"), 0]
    with pytest.raises(ValueError, match="valid v0.7 component"):
        build_spatial_projection(world)
    legacy = _world()
    legacy.records[legacy.world_record.id].frontmatter["schema"] = "wedl/v0.6"
    assert build_spatial_projection(legacy) == build_spatial_projection(legacy).__class__((), (), (), (), (), (), (), (), (), (), (), (), (), (), (), ())


def test_cache_schema_changes_and_generic_compiler_remains_latent() -> None:
    assert SQLITE_SCHEMA == "wedl-sqlite/v7"
    world = _world()
    # The projection is available to opt-in component callers only.  Generic
    # world validation remains the migration boundary.
    from wedl.validation import validate_world
    assert any(item["code"] == "WDL-SRC-001" for item in validate_world(world))


def test_generic_v07_compile_rejects_without_creating_a_disposable_cache(tmp_path: Path) -> None:
    world = _world()
    story = tmp_path / "story"
    story.mkdir()
    for ordinal, record in enumerate(world.records.values()):
        (story / f"{ordinal}.md").write_bytes(serialize_record(record.frontmatter, ""))
    with pytest.raises(ValidationFailed):
        compile_world(Repository(tmp_path), "WORKTREE", profile_name="state")
    assert not (tmp_path / ".wedl").exists()


def test_supported_cache_miss_loads_and_validates_source_once(ash_repo, monkeypatch: pytest.MonkeyPatch) -> None:
    actual = ash_repo.load_world
    calls: list[dict[str, object]] = []

    def instrumented(*args: object, **kwargs: object) -> World:
        calls.append(dict(kwargs))
        return actual(*args, **kwargs)

    monkeypatch.setattr(ash_repo, "load_world", instrumented)
    result = compile_world(ash_repo, force=True, profile_name="state")
    assert result["status"] == "compiled"
    assert calls == [{"cache_write": False}]


def test_readable_cache_missing_spatial_structure_is_not_ready_and_rebuilds_atomically(ash_repo) -> None:
    result = compile_world(ash_repo, force=True, profile_name="state")
    database = Path(result["database"])
    connection = sqlite3.connect(database)
    try:
        connection.execute("DROP TABLE spatial_location_link")
        connection.commit()
    finally:
        connection.close()
    readiness = cache_readiness(ash_repo)
    assert readiness["state"] == "incompatible"
    assert "missingTable:spatial_location_link" in readiness["incompatibleFields"]
    with pytest.raises(CompileRequired):
        require_database(ash_repo, require_compiled=True)
    _world_value, rebuilt = require_database(ash_repo)
    assert rebuilt == database
    assert cache_readiness(ash_repo)["state"] == "ready"
    connection = sqlite3.connect(database)
    try:
        assert connection.execute(
            "SELECT name FROM sqlite_schema WHERE type='table' AND name='spatial_location_link'"
        ).fetchone() == ("spatial_location_link",)
        assert connection.execute("PRAGMA integrity_check").fetchone() == ("ok",)
    finally:
        connection.close()
    assert list(database.parent.glob("world-*.sqlite")) == []
