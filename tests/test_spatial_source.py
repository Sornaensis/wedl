from __future__ import annotations

import pytest

from wedl.ids import valid_id, valid_spatial_id
from wedl.spatial import SpatialReference
from wedl.source import generated_path
from wedl.v07 import canonical_capabilities


def test_spatial_ids_are_narrow_and_do_not_change_legacy_id_acceptance() -> None:
    assert valid_spatial_id("map:riverward", "map")
    assert not valid_spatial_id("map:Riverward")
    assert not valid_spatial_id("map:riverward", "route")
    assert valid_id("loc_00000000000000000000000000", "location")
    assert SpatialReference.from_value("route:archive-gate", "route") is not None


def test_v07_capabilities_are_closed_and_protocol_ordered() -> None:
    assert canonical_capabilities(["spatial-core-v1", "geometry-v1"]) == ("spatial-core-v1", "geometry-v1")
    assert canonical_capabilities(["geometry-v1"]) is None
    assert canonical_capabilities(["geometry-v1", "spatial-core-v1"]) is None
    assert canonical_capabilities(["spatial-core-v1", "spatial-core-v1"]) is None


@pytest.mark.parametrize(
    ("value", "expected"),
    [
        ([], None),
        (["generational-core-v1"], ("generational-core-v1",)),
        (["spatial-core-v1"], ("spatial-core-v1",)),
        (["geometry-v1"], None),
        (["route-v1"], None),
        (["overlay-v1"], None),
        (["generational-core-v1", "spatial-core-v1"], ("generational-core-v1", "spatial-core-v1")),
        (["spatial-core-v1", "geometry-v1"], ("spatial-core-v1", "geometry-v1")),
        (["spatial-core-v1", "route-v1"], ("spatial-core-v1", "route-v1")),
        (["spatial-core-v1", "overlay-v1"], ("spatial-core-v1", "overlay-v1")),
        (["generational-core-v1", "spatial-core-v1", "geometry-v1", "route-v1", "overlay-v1"], ("generational-core-v1", "spatial-core-v1", "geometry-v1", "route-v1", "overlay-v1")),
    ],
)
def test_v07_capability_declaration_requires_a_core_and_spatial_option_dependencies(value, expected) -> None:
    assert canonical_capabilities(value) == expected


def test_spatial_path_codec_is_portable_for_every_colon_kind_and_legacy_locations() -> None:
    for kind in ("map", "location", "anchor", "portal", "route", "overlay"):
        value = f"{kind}:north/gate"
        assert valid_spatial_id(value, kind)
        assert generated_path("story", kind, "Ignored", value, {"schema": "wedl/v0.7"}).endswith("north/gate.md")
    assert not valid_spatial_id("map:con", "map")
    assert not valid_spatial_id("map:north/con", "map")
    with pytest.raises(Exception):
        generated_path("story", "map", "Ignored", "map:con", {"schema": "wedl/v0.7"})
    legacy = "loc_00000000000000000000000000"
    assert generated_path("story", "location", "Legacy", legacy, {"schema": "wedl/v0.7"}) == f"story/locations/legacy--{legacy}.md"
