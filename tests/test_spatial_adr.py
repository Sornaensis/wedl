"""Static conformance checks for accepted ADR 0004; no runtime is implied."""

from __future__ import annotations

from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[1]
ADR = ROOT / "docs/decisions/0004-spatial-domain-query-and-version-contract.md"
SCHEMA = ROOT / "docs/decisions/examples/spatial-schema-v07.yaml"
QUERIES = ROOT / "docs/decisions/examples/spatial-query-v1.yaml"


def _yaml(path: Path) -> dict:
    return yaml.safe_load(path.read_text(encoding="utf-8"))


def test_adr_is_accepted_indexed_and_explicitly_non_runtime() -> None:
    text = ADR.read_text(encoding="utf-8")
    index = (ROOT / "docs/decisions/README.md").read_text(encoding="utf-8")
    assert "**Status:** Accepted" in text
    assert "**Approved:** 2026-08-30 by Project owner (user)" in text
    assert "not a runtime, source-parser, transport, renderer, migration command" in text
    assert "0004 — Spatial domain" in index and "**Accepted**" in index
    for phrase in ("hybrid", "ID path", "local planar", "axis order", "anchors", "portals", "one-way", "two-way"):
        assert phrase.lower() in text.lower()


def test_spatial_shape_preserves_coordinate_free_worlds_and_non_inference() -> None:
    vector = _yaml(SCHEMA)
    locations = {item["id"]: item for item in vector["locations"]}
    assert vector["schema"] == "wedl/v0.7"
    assert vector["capabilities"]["required"] == ["spatial-core-v1"]
    assert "spatial" not in locations["location:city"]
    assert locations["location:archive"]["parent_id"] == "location:city"
    source_story_time = {"start": vector["overlays"][0]["valid"]["start"], "end": vector["overlays"][0]["valid"]["end"]}
    assert all(isinstance(point[field], int) for point in source_story_time.values() for field in ("tick", "order"))
    assert vector["maps"][0]["crs"].startswith("local-planar:")
    assert vector["maps"][1]["axis_order"] == ["longitude", "latitude"]
    assert vector["routes"][0]["direction"] == "one-way"
    overlays = {item["id"]: item for item in vector["overlays"]}
    assert overlays["overlay:ward"]["lifecycle"] == "time-bounded"
    assert overlays["overlay:wayfinding"]["lifecycle"] == "static"
    assert overlays["overlay:ward"]["perspectives"] == ["council-archive"]
    boundary = vector["visibility_boundary"]
    assert boundary["filter_order"] == ["timeline", "horizon", "audience", "perspective", "membership", "ordering", "pagination", "serialization"]
    assert boundary["catalogue_hidden_result"] == "observationally-identical-to-no-overlay"
    assert {value["owner"] for value in boundary["deferred_boundary_kinds"].values()} == {"scope-governance-maintainer"}
    rules = " ".join(vector["non_inference"])
    for phrase in ("proximity", "reverse routes", "shared metric", "duration", "missing geometry"):
        assert phrase in rules


def test_version_migration_and_cross_project_boundary_are_closed() -> None:
    vector = _yaml(SCHEMA)
    text = ADR.read_text(encoding="utf-8")
    assert set(vector["version_matrix"]) == {"wedl/v0.3", "wedl/v0.5", "wedl/v0.6", "wedl/v0.7"}
    assert all(vector["version_matrix"][version]["mixed_with_v07"] == "rejected" for version in ("wedl/v0.3", "wedl/v0.5", "wedl/v0.6"))
    assert vector["migration"]["state"] == "future-work"
    assert vector["migration"]["blocked_on"] == "generational-history-adr"
    for guarantee in ("lossless", "previewable", "atomic", "idempotent", "expected-head-protected", "git-reversible"):
        assert guarantee in vector["migration"]["guarantees"]
    assert "waits for the future generational-history ADR" in text
    assert "does not claim completion" in text


def test_query_vectors_close_states_budgets_time_and_audience() -> None:
    vector = _yaml(QUERIES)
    assert vector["protocol"] == "wedl-spatial/v1"
    assert vector["common"]["states"] == ["ok", "invalid", "unavailable", "forbidden", "limit"]
    assert vector["common"]["limit"] == {"minimum": 1, "maximum": 100}
    assert vector["common"]["cursor"] == "opaque-keyset-bound-to-normalized-request-and-revision"
    for name, query in vector["queries"].items():
        request = query["request"]
        assert request["protocol"] == vector["protocol"]
        assert request["revision"] == vector["common"]["revision"]
        assert request["capabilities"] == vector["common"]["capabilities"]
        assert request["cursor"] is None
        assert isinstance(request["limit"], int)
        if name == "limit":
            assert request["limit"] > vector["common"]["limit"]["maximum"]
        else:
            assert vector["common"]["limit"]["minimum"] <= request["limit"] <= vector["common"]["limit"]["maximum"]
    assert vector["queries"]["nearby_incompatible"]["response"] == {"state": "unavailable", "code": "SPATIAL-METRIC-001"}
    assert vector["queries"]["path_one_way_reverse"]["response"]["code"] == "SPATIAL-PATH-001"
    public_hidden = vector["queries"]["overlay_as_of_public_hidden"]
    public_control = vector["queries"]["overlay_as_of_no_overlay_control"]
    assert public_hidden["request"]["query_scope"] == public_control["request"]["query_scope"] == "location-catalogue"
    assert public_hidden["response"] == public_control["response"] == {"state": "ok", "overlays": [], "story_time": {"timeline": "main", "tick": "12", "order": "0"}}
    forbidden = vector["queries"]["overlay_by_id_forbidden"]
    assert forbidden["request"]["query_scope"] == "explicit-overlay-id"
    assert forbidden["response"] == {"state": "forbidden", "code": "SPATIAL-OVERLAY-001"}
    authorized = vector["queries"]["overlay_as_of_authorized"]["response"]
    assert authorized["story_time"] == {"timeline": "main", "tick": "12", "order": "0"}
    for query in vector["queries"].values():
        for wire_time in (query["request"].get("at"), query["response"].get("story_time")):
            if wire_time is None:
                continue
            for field in ("tick", "order"):
                value = wire_time[field]
                assert isinstance(value, str), "public transport must reject numeric StoryTime fields"
                assert value == str(int(value)), "public transport requires canonical signed decimal strings"
    assert vector["queries"]["limit"]["response"] == {"state": "limit", "code": "SPATIAL-LIMIT-001", "records": []}
    assert vector["budgets"] == {"returned_records": 100, "route_expansions": 1000, "overlay_candidates_after_authorization": 2000, "geometry_vertices": 10000}
