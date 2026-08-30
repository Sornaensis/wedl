from __future__ import annotations

from wedl.model import Record
from wedl.spatial_validation import validate_spatial_component


def test_v07_component_is_not_generic_source_acceptance() -> None:
    # Component validation is opt-in and candidate-only; generic validation is
    # deliberately owned by the coordinated v0.7 rollout.
    records = [
        Record({"schema": "wedl/v0.7", "kind": "world", "id": "world_00000000000000000000000000", "title": "W", "capabilities": ["spatial-core-v1", "route-v1"], "default_timeline": "main", "timelines": [{"id": "main", "label": "Main"}], "chronology": {"calendars": [], "eras": [], "anchors": []}}, "", "story/world.md", b""),
        Record({"schema": "wedl/v0.7", "kind": "location", "id": "location:a", "title": "A"}, "", "story/locations/a.md", b""),
        Record({"schema": "wedl/v0.7", "kind": "location", "id": "location:b", "title": "B"}, "", "story/locations/b.md", b""),
        Record({"schema": "wedl/v0.7", "kind": "route", "id": "route:a-b", "title": "A B", "from_location_id": "location:a", "to_location_id": "location:b", "direction": "one-way", "modes": ["foot"]}, "", "story/routes/a-b.md", b""),
    ]
    assert validate_spatial_component(records) == []
    records[-1].frontmatter["direction"] = "sideways"
    assert validate_spatial_component(records)[0]["code"] == "SPATIAL-ROUTE-001"
