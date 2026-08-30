from __future__ import annotations

from pathlib import Path
from copy import deepcopy

import yaml
import pytest

from wedl import COMPILED_SOURCE_SCHEMAS, SUPPORTED_SOURCE_SCHEMAS
from wedl.ids import valid_id
from wedl.model import Record
from wedl.source import generated_path, parse_record, serialize_record
from wedl.spatial import Anchor, Geometry, MapDefinition, Overlay, Portal, Route, spatial_frontmatter
from wedl.spatial_validation import PRODUCTION_DIAGNOSTICS, validate_spatial_component
from wedl.validation import validate_world
from wedl.model import World


ROOT = Path(__file__).parents[1]


def _record(value: dict, name: str) -> Record:
    return Record(value, "", f"story/{name}.md", b"")


def _candidate() -> list[Record]:
    fixture = yaml.safe_load((ROOT / "docs/examples/spatial-source-component-v07.yaml").read_text())
    values = [fixture["world"], fixture["map"], *fixture["locations"], fixture["route"], fixture["overlay"]]
    return [_record(value, str(index)) for index, value in enumerate(values)]


def test_component_fixture_covers_coordinate_free_maps_routes_and_timed_overlays() -> None:
    assert validate_spatial_component(_candidate()) == []


def test_component_rejects_nonfinite_geometry_cycles_and_unapproved_boundary() -> None:
    records = _candidate()
    location = records[3]
    location.frontmatter["spatial"]["geometry"]["coordinates"] = [float("inf"), 0]
    location.frontmatter["parent_id"] = "location:town"
    records[2].frontmatter["parent_id"] = "location:gate"
    records[-1].frontmatter["faction"] = "guard"
    errors = validate_spatial_component(records)
    assert {(item["code"], item["field"]) for item in errors} >= {
        ("SPATIAL-GEOMETRY-001", "spatial.geometry.coordinates"),
        ("SPATIAL-HIERARCHY-001", "parent_id"),
        ("SPATIAL-REQUEST-001", "faction"),
    }


def test_component_requires_world_ordered_capability_declaration_and_keeps_legacy_inert() -> None:
    records = _candidate()
    records[0].frontmatter["capabilities"] = ["route-v1", "spatial-core-v1"]
    assert any(item["code"] == "GEN-CAPABILITY-001" for item in validate_spatial_component(records))
    legacy = _record({"schema": "wedl/v0.6", "kind": "location", "id": "loc_00000000000000000000000000"}, "legacy")
    assert validate_spatial_component([legacy]) == []


def test_generational_only_envelope_accepts_coordinate_free_records_but_gates_spatial_features() -> None:
    records = _candidate()
    records[0].frontmatter["capabilities"] = ["generational-core-v1"]
    assert validate_spatial_component([records[0], records[2]]) == []

    records.append(
        _record(
            {
                "schema": "wedl/v0.7",
                "kind": "portal",
                "id": "portal:town-gate",
                "title": "Town gate portal",
                "from_location_id": "location:town",
                "to": "location:gate",
                "modes": ["foot"],
            },
            "portal",
        )
    )
    gated = {
        error["entityId"]
        for error in validate_spatial_component(records)
        if error["code"] == "GEN-CAPABILITY-001"
    }
    assert {"map:plain", "location:gate", "route:town-gate", "portal:town-gate", "overlay:ward"} <= gated


def test_frontmatter_round_trip_preserves_v07_body_but_keeps_legacy_canonical_bytes() -> None:
    body = "\n\n# Map notes\n\nTrailing space stays.  \n"
    data = serialize_record({"schema": "wedl/v0.7", "kind": "location", "id": "location:notes", "title": "Notes"}, body)
    record = parse_record(data, "story/locations/notes.md")
    assert record.body == body
    assert serialize_record(record.frontmatter, record.body).endswith(body.encode())
    legacy = serialize_record({"schema": "wedl/v0.6", "kind": "location", "id": "loc_00000000000000000000000000", "title": "Notes", "domain": "world", "status": "canonical", "tags": [], "aliases": []}, body)
    assert legacy.endswith(b"# Map notes\n\nTrailing space stays.\n")


def test_v07_parent_compatibility_input_serializes_to_adr_canonical_parent_id() -> None:
    frontmatter = {"schema": "wedl/v0.7", "kind": "location", "id": "location:gate", "title": "Gate", "parent": "location:town"}
    encoded = serialize_record(frontmatter, "")
    parsed = parse_record(encoded, "story/locations/gate.md")
    assert parsed.frontmatter["parent_id"] == "location:town"
    assert "parent" not in parsed.frontmatter
    records = _candidate()
    records[3].frontmatter.pop("parent_id")
    records[3].frontmatter["parent"] = "location:town"
    assert validate_spatial_component(records) == []
    with pytest.raises(Exception):
        serialize_record({**frontmatter, "parent_id": "location:elsewhere"}, "")


def test_diagnostic_catalog_is_closed() -> None:
    catalogue = {(item["code"], item["message"]) for item in validate_spatial_component(_candidate())}
    assert catalogue == set()
    assert {"GEN-CAPABILITY-001", "SPATIAL-ROUTE-001", "SPATIAL-TIME-001"} <= {code for code, _ in PRODUCTION_DIAGNOSTICS}


def test_feature_gates_legacy_locations_and_total_malformed_yaml_diagnostics() -> None:
    records = _candidate()
    for capability, index in (("spatial-core-v1", 1), ("geometry-v1", 3), ("route-v1", 4), ("overlay-v1", 5)):
        altered = deepcopy(records)
        altered[0].frontmatter["capabilities"].remove(capability)
        assert any(error["code"] == "GEN-CAPABILITY-001" for error in validate_spatial_component(altered))
    legacy = "loc_00000000000000000000000000"
    assert valid_id(legacy, "location")
    records[2].frontmatter["id"] = legacy
    records[3].frontmatter["parent_id"] = legacy
    records[4].frontmatter["from_location_id"] = legacy
    assert not any(error["code"] == "SPATIAL-REF-001" for error in validate_spatial_component(records))
    malformed = deepcopy(_candidate())
    malformed[0].frontmatter["timelines"] = [None, {"id": ["bad"]}]
    malformed[4].frontmatter["modes"] = ["foot", None]
    assert validate_spatial_component(malformed) == validate_spatial_component(malformed)


def test_component_paths_ordering_and_runtime_rejection_stay_latent() -> None:
    map_record = _candidate()[1].frontmatter
    map_record["provenance"] = [{"revision": "abc", "section": "Maps"}]
    encoded = serialize_record(map_record, "\n\nMap body.  \n")
    parsed = parse_record(encoded, "story/maps/plain.md")
    assert parsed.body == "\n\nMap body.  \n"
    assert list(parsed.frontmatter)[-1] == "provenance"
    assert generated_path("story", "map", "Ignored", "map:plain", map_record) == "story/maps/plain.md"
    assert "wedl/v0.7" not in SUPPORTED_SOURCE_SCHEMAS | COMPILED_SOURCE_SCHEMAS


def test_multimap_geodetic_anchor_portal_and_legacy_location_fixture() -> None:
    fixture = yaml.safe_load((ROOT / "tests/fixtures/spatial_v07/valid-multimap.yaml").read_text())
    records = [_record(fixture["world"], "world"), *[_record(item, f"map-{index}") for index, item in enumerate(fixture["maps"])], *[_record(item, f"location-{index}") for index, item in enumerate(fixture["locations"])], *[_record(fixture[name], name) for name in ("anchor", "portal", "route", "overlay")]]
    assert validate_spatial_component(records) == []


def test_arbitrary_record_frontmatter_never_raises() -> None:
    records = _candidate() + [Record({"schema": "wedl/v0.7"}, "", "story/bad.md", b"")]
    first = validate_spatial_component(records)
    assert first == validate_spatial_component(records)
    assert any(error["code"] == "SPATIAL-REQUEST-001" for error in first)
    unknown = deepcopy(_candidate())
    unknown[2].frontmatter["unapproved"] = "operative"
    unknown[2].frontmatter["x-note"] = "inert"
    assert any(error["code"] == "SPATIAL-REQUEST-001" for error in validate_spatial_component(unknown))


def test_envelope_and_map_diagnostics_identify_the_exact_leaf() -> None:
    records = _candidate()
    records[1].frontmatter["unapproved_map_field"] = True
    errors = {(item["code"], item["field"]) for item in validate_spatial_component(records)}
    assert ("SPATIAL-REQUEST-001", "unapproved_map_field") in errors
    records = _candidate()
    records[1].frontmatter["unit"] = ""
    errors = {(item["code"], item["field"]) for item in validate_spatial_component(records)}
    assert ("SPATIAL-MAP-001", "unit") in errors


def test_v07_envelope_uses_full_chronology_validation_and_listed_default_timeline() -> None:
    records = _candidate()
    records[0].frontmatter["default_timeline"] = "missing"
    records[0].frontmatter["chronology"] = {
        "calendars": [{"id": "not-a-calendar", "label": "Broken"}],
        "eras": [],
        "anchors": [],
    }
    errors = {(item["code"], item["field"]) for item in validate_spatial_component(records)}
    assert ("SPATIAL-REQUEST-001", "default_timeline") in errors
    assert ("SPATIAL-REQUEST-001", "chronology.calendars[0]") in errors


def test_invalid_fixture_bool_nan_and_duplicate_edges_are_leaf_diagnostics() -> None:
    invalid = yaml.safe_load((ROOT / "tests/fixtures/spatial_v07/invalid-nonfinite.yaml").read_text())["location"]
    records = _candidate()
    invalid["spatial"]["geometry"]["coordinates"] = [True, float("nan")]
    records.append(_record(invalid, "invalid"))
    duplicate = deepcopy(records[4].frontmatter)
    duplicate["id"] = "route:town-gate-copy"
    records.append(_record(duplicate, "duplicate"))
    errors = validate_spatial_component(records)
    assert ("SPATIAL-GEOMETRY-001", "spatial.geometry.coordinates") in {(item["code"], item["field"]) for item in errors}
    assert ("SPATIAL-ROUTE-001", "from_location_id") in {(item["code"], item["field"]) for item in errors}


def test_v07_envelope_legacy_links_and_geometry_variants_are_literal_not_routes() -> None:
    records = _candidate()
    records[0].frontmatter["chronology"] = {"calendars": [], "eras": [], "anchors": []}
    records[0].frontmatter["timelines"] = [{"id": "main", "label": "Main", "origin": {"tick": -7, "label": "Before"}}]
    records[1].frontmatter.update({"origin": {"label": "Datum", "coordinates": [0, 0]}, "scale": {"value": 1, "unit": "pace"}, "z_policy": "optional-level"})
    records[3].frontmatter["links"] = ["location:town", {"target": "location:town", "label": "Different detail"}]
    # duplicate legacy links are rejected as links, never converted into routes.
    errors = validate_spatial_component(records)
    assert ("SPATIAL-REF-001", "links[1]") in {(item["code"], item["field"]) for item in errors}
    records[3].frontmatter["links"] = [{"location": "location:town", "label": "Town gate"}]
    records[3].frontmatter["spatial"]["geometry"] = {"kind": "line", "coordinates": [[0, 0], [10, 5]]}
    assert validate_spatial_component(records) == []
    records[3].frontmatter["spatial"]["geometry"] = {"kind": "polygon", "coordinates": [[0, 0], [1, 0], [1, 1], [0, 0]]}
    assert validate_spatial_component(records) == []


def test_limit_story_time_and_static_overlay_have_exact_diagnostic_leaves() -> None:
    records = _candidate()
    records[3].frontmatter["spatial"]["geometry"] = {"kind": "line", "coordinates": [[0, 0]] * 10_001}
    records[-1].frontmatter["valid"]["end"] = {"timeline": "other", "tick": 2, "order": 0}
    errors = {(item["code"], item["field"]) for item in validate_spatial_component(records)}
    assert ("SPATIAL-LIMIT-001", "spatial.geometry.coordinates") in errors
    assert ("SPATIAL-TIME-001", "valid.end") in errors
    records = _candidate(); records[-1].frontmatter["lifecycle"] = "static"; records[-1].frontmatter.pop("valid")
    assert validate_spatial_component(records) == []


def test_typed_accessors_are_deeply_immutable_and_include_optional_values() -> None:
    records = _candidate(); map_value = records[1].frontmatter
    map_value.update({"origin": {"label": "Datum", "coordinates": [0, 0]}, "scale": {"value": 1, "unit": "pace"}, "z_policy": "optional-level"})
    definition = MapDefinition.from_value(map_value)
    assert definition is not None and definition.origin is not None and definition.origin.coordinates == (0, 0) and definition.scale is not None
    placement = spatial_frontmatter(records[3])
    assert placement is not None and isinstance(placement.geometry.coordinates, tuple)
    with pytest.raises((AttributeError, TypeError)):
        placement.geometry.coordinates[0] = 8
    route = Route.from_value(records[4].frontmatter)
    assert route is not None and route.route_distance is not None and route.availability == "open"
    portal = Portal.from_value({"id": "portal:point", "from_location_id": "location:town", "to": {"map_id": "map:plain", "coordinates": [1, 2]}, "modes": ["foot"]})
    assert portal is not None and portal.to.__class__.__name__ == "MapPosition"
    geometry = Geometry.from_value({"kind": "polygon", "coordinates": [[0, 0], [1, 0], [1, 1], [0, 0]]})
    assert geometry is not None and all(isinstance(point, tuple) for point in geometry.coordinates)


def test_z_policy_is_consistent_for_bounds_origin_geometry_and_accessors() -> None:
    records = _candidate()
    map_value, placement = records[1].frontmatter, records[3].frontmatter["spatial"]
    map_value.update({"z_policy": "optional-level", "bounds": {"min": [-100, -100, -5], "max": [100, 100, 5]}, "origin": {"label": "Datum", "coordinates": [0, 0, 0]}})
    placement["geometry"]["coordinates"] = [10, 5, 0]
    assert validate_spatial_component(records) == []
    assert MapDefinition.from_value(map_value) is not None
    records[1].frontmatter["z_policy"] = "required"
    assert validate_spatial_component(records) == []
    records[1].frontmatter["bounds"] = {"min": [-100, -100], "max": [100, 100]}
    records[1].frontmatter["origin"]["coordinates"] = [0, 0]
    assert ("SPATIAL-MAP-001", "bounds") in {(item["code"], item["field"]) for item in validate_spatial_component(records)}
    assert MapDefinition.from_value(records[1].frontmatter) is None
    records = _candidate()
    records[1].frontmatter["bounds"] = {"min": [-100, -100, -5], "max": [100, 100, 5]}
    assert ("SPATIAL-MAP-001", "bounds") in {(item["code"], item["field"]) for item in validate_spatial_component(records)}
    assert MapDefinition.from_value(records[1].frontmatter) is None


def test_accessor_and_validator_reject_the_same_spatial_leaf_shapes() -> None:
    assert Geometry.from_value({"kind": "line", "coordinates": []}) is None
    records = _candidate(); records[3].frontmatter["spatial"]["geometry"] = {"kind": "line", "coordinates": []}
    assert ("SPATIAL-GEOMETRY-001", "spatial.geometry.coordinates") in {(item["code"], item["field"]) for item in validate_spatial_component(records)}
    route = records[4].frontmatter
    route["modes"] = []
    assert Route.from_value(route) is None
    assert ("SPATIAL-ROUTE-001", "modes") in {(item["code"], item["field"]) for item in validate_spatial_component(records)}
    portal = {"schema": "wedl/v0.7", "kind": "portal", "id": "portal:point", "title": "Point", "from_location_id": "location:town", "to": "location:gate", "modes": ["foot", "foot"]}
    assert Portal.from_value(portal) is None
    records = _candidate(); records.append(_record(portal, "portal"))
    assert ("SPATIAL-PORTAL-001", "modes") in {(item["code"], item["field"]) for item in validate_spatial_component(records)}
    overlay = records[-2].frontmatter
    overlay["valid"] = None
    assert Overlay.from_value(overlay) is None
    assert ("SPATIAL-TIME-001", "valid.start") in {(item["code"], item["field"]) for item in validate_spatial_component(records)}
    overlay["lifecycle"] = "static"
    overlay["valid"] = {"start": {"timeline": "main", "tick": 0, "order": 0}, "end": {"timeline": "main", "tick": 0, "order": 0}}
    assert Overlay.from_value(overlay) is None
    assert ("SPATIAL-OVERLAY-001", "valid") in {(item["code"], item["field"]) for item in validate_spatial_component(records)}


def test_anchor_conversion_and_detailed_location_link_rules_are_shared_and_canonical() -> None:
    fixture = yaml.safe_load((ROOT / "tests/fixtures/spatial_v07/valid-multimap.yaml").read_text())
    records = [_record(fixture["world"], "world"), *[_record(item, f"map-{index}") for index, item in enumerate(fixture["maps"])], *[_record(item, f"location-{index}") for index, item in enumerate(fixture["locations"])], _record(fixture["anchor"], "anchor")]
    records[-1].frontmatter["conversion"] = 4
    assert Anchor.from_value(records[-1].frontmatter) is None
    assert ("SPATIAL-ANCHOR-001", "conversion") in {(item["code"], item["field"]) for item in validate_spatial_component(records)}
    records = _candidate()
    records[3].frontmatter["links"] = [{"location": "location:town", "description": "  ", "label": "Gateward", "summary": "A route"}]
    assert ("SPATIAL-REF-001", "links[0].description") in {(item["code"], item["field"]) for item in validate_spatial_component(records)}
    encoded = serialize_record({"schema": "wedl/v0.7", "kind": "location", "id": "location:gate", "title": "Gate", "links": [{"summary": "A route", "location": "location:town", "description": "Gateward"}]}, "")
    assert b"links:\n- description: Gateward\n  location: location:town\n  summary: A route\n" in encoded


def test_generic_validation_stays_rejected_and_component_validation_never_writes() -> None:
    records = _candidate()
    candidate = World("candidate", "tree", {record.id: record for record in records}, ROOT)
    generic = validate_world(candidate)
    assert any(item["code"] == "WDL-SRC-001" for item in generic)
    before = [(record.source_path, record.raw_bytes) for record in records]
    assert validate_spatial_component(records) == []
    assert [(record.source_path, record.raw_bytes) for record in records] == before
