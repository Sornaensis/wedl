"""Small authored-source scale smoke; the envelope run is an explicit command."""

from __future__ import annotations

from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = spec_from_file_location("benchmark_spatial_release", ROOT / "tools/benchmark_spatial_release.py")
assert SPEC and SPEC.loader
benchmark = module_from_spec(SPEC)
SPEC.loader.exec_module(benchmark)


def test_generated_authored_source_compile_rebuild_and_http_are_repeat_stable(tmp_path: Path) -> None:
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
