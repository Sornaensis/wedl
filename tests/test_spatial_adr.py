"""Static conformance checks for accepted ADR 0004; no runtime is implied."""

from __future__ import annotations

from adrai_fixtures import current_decision, current_status

from pathlib import Path

import yaml
import jsonschema

from wedl.api_schemas import components


ROOT = Path(__file__).resolve().parents[1]
ADR = current_decision("A01M48NPY19KENPF8EWB8A4W95W")


def test_explorer_adjunct_decision_records_bounded_selected_lens() -> None:
    text = ADR.read_text(encoding="utf-8")
    assert "wedl-spatial-explorer/v1" in text
    assert "catalog, places, viewport, layers, and routes" in text
    assert "reverse-edge index" in text
    assert "before counting or paging" in text
    assert "not a trusted character identity" in text or "does not confer trusted character identity" in text
SCHEMA = ROOT / "architecture/adrai/examples/spatial-schema-v07.yaml"
QUERIES = ROOT / "architecture/adrai/examples/spatial-query-v1.yaml"


def _yaml(path: Path) -> dict:
    return yaml.safe_load(path.read_text(encoding="utf-8"))


def test_adr_is_accepted_indexed_and_explicitly_non_runtime() -> None:
    text = ADR.read_text(encoding="utf-8")
    index = ADR.metadata
    assert current_status(index["adr"])["state"] == "active"
    assert "**Status:** Accepted" in text
    assert "**Approved:** 2026-08-30 by Project owner (user)" in text
    assert "not a runtime, source-parser, transport, renderer, migration command" in text
    assert index["adr"] == "A01M48NPY19KENPF8EWB8A4W95W" and ADR.path.is_file()
    assert index["title"] == "Spatial domain, queries, and schema-version contract"
    assert "# ADR 0004:" in text
    assert 'schema = "adrai/decision/v1"' in text
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
        assert vector["common"]["limit"]["minimum"] <= request["limit"] <= vector["common"]["limit"]["maximum"]
        response = query["response"]
        assert response["protocol"] == vector["protocol"]
        assert response["revision"] == vector["common"]["revision"]
        assert response["capabilities"] == vector["common"]["capabilities"]
        assert set(response["cache"]) == {"state", "revision", "treeOid", "sourceSchema", "fingerprint"}
        stem = "OverlayAsOf" if response["operation"] == "overlay-as-of" else "".join(part.capitalize() for part in response["operation"].split("-"))
        state = response["state"].capitalize()
        jsonschema.validate(response, {"components": components(), "$ref": f"#/components/schemas/Spatial{stem}{state}Outcome"})
    assert vector["queries"]["nearby"]["response"]["code"] == "SPATIAL-METRIC-001"
    assert vector["queries"]["path_one_way_reverse"]["response"]["code"] == "SPATIAL-PATH-001"
    public_hidden = vector["queries"]["overlay_as_of_public_hidden"]
    public_control = vector["queries"]["overlay_as_of_no_overlay_control"]
    assert public_hidden["request"]["queryScope"] == public_control["request"]["queryScope"] == "location"
    assert public_hidden["response"] == public_control["response"]
    hidden_id = vector["queries"]["overlay_by_id_public_hidden"]
    missing_id = vector["queries"]["overlay_by_id_missing_control"]
    assert hidden_id["request"]["queryScope"] == missing_id["request"]["queryScope"] == "overlay"
    assert hidden_id["response"] == missing_id["response"]
    assert hidden_id["response"] == missing_id["response"]
    assert hidden_id["response"]["code"] == "SPATIAL-OVERLAY-001"
    authorized = vector["queries"]["overlay_as_of_authorized"]["response"]
    assert authorized["result"]["storyTime"] == {"timeline": "main", "tick": "12", "order": "0"}
    for query in vector["queries"].values():
        response = query["response"]
        result = response.get("result", {})
        for wire_time in (query["request"].get("asOf"), result.get("storyTime"), result.get("filters", {}).get("horizon")):
            if wire_time is None:
                continue
            for field in ("tick", "order"):
                value = wire_time[field]
                assert isinstance(value, str), "public transport must reject numeric StoryTime fields"
                assert value == str(int(value)), "public transport requires canonical signed decimal strings"
    assert vector["queries"]["limit"]["response"]["code"] == "SPATIAL-LIMIT-001"
    assert vector["queries"]["limit"]["request"]["limit"] == 100
    assert vector["queries"]["limit"]["precondition"] == "map:budget contains 10001 authored geometry candidates intersecting the requested bounds"
    assert vector["budgets"] == {"returned_records": 100, "route_expansions": 1000, "overlay_candidates_after_authorization": 2000, "geometry_vertices": 10000}
