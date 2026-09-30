"""Small authored-source scale smoke; the envelope run is an explicit command."""

from __future__ import annotations

from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
import sqlite3

from wedl.compiler import DDL, INDEX_DDL
from wedl.spatial_query import SpatialStore


ROOT = Path(__file__).resolve().parents[1]
SPEC = spec_from_file_location("benchmark_spatial_release", ROOT / "tools/benchmark_spatial_release.py")
assert SPEC and SPEC.loader
benchmark = module_from_spec(SPEC)
SPEC.loader.exec_module(benchmark)


def _assert_children_page_two_measures_production_cursor() -> None:
    with sqlite3.connect(":memory:") as connection:
        connection.executescript(DDL)
        connection.executescript(INDEX_DDL)
        connection.execute("INSERT INTO spatial_capability VALUES ('spatial-core-v1',0)")
        identifiers = ("location:parent", "location:z", "location:a", "location:m")
        for ordinal, ident in enumerate(identifiers):
            connection.execute("INSERT INTO entity VALUES (?, 'location', ?, 'world', 'canonical', ?, NULL, '', '{}')",
                               (ident, ident, f"story/{ordinal}.md"))
            connection.execute("INSERT INTO spatial_location(id,source_ordinal,has_spatial,parent_id,definition_json) VALUES (?,?,0,?,'{}')",
                               (ident, ordinal, None if ordinal == 0 else identifiers[0]))
        store = SpatialStore(connection, "benchmark-regression")
        statements = []
        connection.set_trace_callback(statements.append)
        evidence = benchmark._children_page_two_vm(store, identifiers[0], 1)
        connection.set_trace_callback(None)
        pages = [sql for sql in statements if "SELECT id,source_ordinal" in sql]
        assert len(pages) == 2 and all("ORDER BY source_ordinal,id" in sql for sql in pages)
        assert "(source_ordinal,id)>(1,'location:z')" in pages[1]
        assert connection.execute(pages[1]).fetchall() == [("location:a", 2), ("location:m", 3)]
        assert evidence["state"] == "ok" and evidence["vmStepsUpperBound"] <= 50_000


def test_generated_authored_source_compile_rebuild_and_http_are_repeat_stable(tmp_path: Path) -> None:
    _assert_children_page_two_measures_production_cursor()
    result = benchmark.run(base=ROOT, output=tmp_path / "evidence",
                           places=128, maps=2, routes=8, portals=1, overlays=1, repeats=2)
    assert result["counts"]["hierarchyDepth"] == 128
    assert result["counts"]["coordinateFreePlaces"] == 8
    assert result["compiledRows"] == {
        "spatial_map": 2, "spatial_location": 128, "spatial_route_edge": 8,
        "spatial_portal": 1, "spatial_overlay": 1,
    }
    assert len(result["compile"]["runs"]) == 2
    assert {sample["status"] for sample in result["compile"]["runs"]} == {"compiled"}
    assert all(case["outcome"]["state"] == "ok" for case in result["api"].values())
    assert all(len(case["samplesMs"]) == 2 and len(case["resultSha256"]) == 64
               for case in result["api"].values())
    assert (tmp_path / "evidence" / "manifest.json").is_file()
