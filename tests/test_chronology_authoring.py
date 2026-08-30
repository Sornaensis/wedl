"""Chronology replacement remains an entity.update authoring extension."""
import pytest

from wedl.authoring import _chronology_annotation, _chronology_catalog, _chronology_date
from wedl.errors import UsageError


CALENDAR = "calendar_0123456789ABCDEFGHJKMNPQRS"


def test_public_chronology_authoring_transcodes_only_exact_identifier_fields():
    annotation = _chronology_annotation({
        "temporaryId": "$chronology.note", "provenance": ["$calendar.free-text"],
        "value": {"kind": "civil", "calendarId": CALENDAR, "year": "0"},
    })
    assert annotation["temporaryId"] == "$chronology.note"
    assert annotation["provenance"] == ["$calendar.free-text"]
    assert annotation["value"] == {"civil": {"calendar_id": CALENDAR, "year": 0}}


def test_catalog_replacement_requires_complete_collections_and_scoped_temporary_ids():
    catalog = _chronology_catalog({"calendars": [], "eras": [], "anchors": []})
    assert catalog == {"calendars": [], "eras": [], "anchors": []}


def test_relative_and_duration_annotations_round_trip_to_source_grammar():
    assert _chronology_date({"kind": "relative", "relation": "after", "beforeId": "event_0123456789ABCDEFGHJKMNPQRS"}) == {"relative": {"relation": "after", "before_id": "event_0123456789ABCDEFGHJKMNPQRS"}}
    assert _chronology_date({"kind": "duration", "unit": "day", "value": "1"}) == {"duration": {"unit": "day", "value": 1}}
    nested = _chronology_date({"kind": "conflict", "claims": [
        {"kind": "relative", "relation": "after", "beforeId": "event_0123456789ABCDEFGHJKMNPQRS"},
        {"kind": "duration", "unit": "month", "value": "2"},
    ]})
    assert nested == {"conflict": {"claims": [{"relative": {"relation": "after", "before_id": "event_0123456789ABCDEFGHJKMNPQRS"}}, {"duration": {"unit": "month", "value": 2}}]}}


@pytest.mark.parametrize(
    ("lower", "upper"),
    [
        ({"year": "0", "month": "1", "day": "1", "x-endpoint": {"calendar_id": "opaque"}}, {"year": "1", "month": "1", "day": "1"}),
        ({"year": "0", "x-endpoint": {"calendar_id": "opaque"}}, None),
        (None, {"year": "1", "x-endpoint": {"calendar_id": "opaque"}}),
        (None, None),
    ],
)
def test_approximate_source_bounds_contain_only_civil_endpoints(lower, upper):
    bounds = {"lower": lower, "upper": upper}
    if lower is not None or upper is not None:
        bounds["calendarId"] = CALENDAR
    source = _chronology_date({
        "kind": "approximate", "displayValue": "about", "x-meta": {"snake_key": 7},
        "bounds": bounds,
    })
    approximate = source["approx"]
    assert approximate["x-meta"] == {"snake_key": 7}
    assert set(approximate).difference({"x-meta"}) == {"display_value", "bounds"}
    assert set(approximate["bounds"]) == {"lower", "upper"}
    assert "calendar_id" not in approximate["bounds"]
    for endpoint in approximate["bounds"].values():
        if endpoint is not None:
            assert endpoint["calendar_id"] == CALENDAR
    nested = _chronology_date({"kind": "conflict", "claims": [
        {"kind": "approximate", "displayValue": "about", "bounds": bounds},
        {"kind": "duration", "unit": "day", "value": "1"},
    ]})
    nested_bounds = nested["conflict"]["claims"][0]["approx"]["bounds"]
    assert set(nested_bounds) == {"lower", "upper"}
    assert "calendar_id" not in nested_bounds


def test_qualitative_approximation_omits_its_public_calendar_id() -> None:
    value = {"kind": "approximate", "displayValue": "unknown", "bounds": {"lower": None, "upper": None}}
    assert _chronology_date(value) == {"approx": {"display_value": "unknown", "bounds": {"lower": None, "upper": None}}}
    with pytest.raises(UsageError, match="omit calendarId"):
        _chronology_date({"kind": "approximate", "displayValue": "unknown", "bounds": {"calendarId": CALENDAR, "lower": None, "upper": None}})


def test_authoring_extensions_preserve_every_declaration_and_value_boundary():
    era = "era_0123456789ABCDEFGHJKMNPQRS"
    anchor = "chronology_0123456789ABCDEFGHJKMNPQRS"
    catalog = _chronology_catalog({
        "calendars": [{
            "id": CALENDAR, "label": "Calendar", "months": [{"number": "1", "days": "30", "x-month": {"year": "opaque"}}],
            "rule": {"kind": "cycle", "period": "1", "overrides": [], "x-rule": ["opaque"]},
            "epoch": {"civil": {"year": "0", "month": "1", "day": "1"}, "axisDay": "0", "x-epoch": {"day": "opaque"}},
            "x-calendar": {"year": "opaque"},
        }],
        "eras": [{
            "id": era, "calendarId": CALENDAR, "label": "Era", "aliases": [], "displayYearZero": True,
            "displayEpoch": {"displayYear": "0", "machineYear": "0"}, "provenance": ["test"], "x-era": {"month": "opaque"},
        }],
        "anchors": [{
            "id": anchor, "axisDay": "0", "storyTime": {"timeline": "main", "tick": "0", "order": "0"},
            "provenance": ["test"], "x-anchor": {"day": "opaque"},
        }],
    })
    assert catalog["calendars"][0]["x-calendar"] == {"year": "opaque"}
    assert catalog["calendars"][0]["months"][0]["x-month"] == {"year": "opaque"}
    assert catalog["calendars"][0]["rule"]["x-rule"] == ["opaque"]
    assert catalog["eras"][0]["x-era"] == {"month": "opaque"}
    assert catalog["anchors"][0]["x-anchor"] == {"day": "opaque"}
    annotation = _chronology_annotation({
        "temporaryId": "$chronology.ext", "provenance": ["test"], "x-annotation": {"year": "opaque"},
        "value": {"kind": "range", "calendarId": CALENDAR, "lower": {"year": "0", "x-endpoint": {"month": "opaque"}}, "upper": None, "x-value": {"day": "opaque"}},
    })
    assert annotation["x-annotation"] == {"year": "opaque"}
    assert annotation["value"]["range"]["x-value"] == {"day": "opaque"}
    assert annotation["value"]["range"]["lower"]["x-endpoint"] == {"month": "opaque"}
    with pytest.raises(UsageError):
        _chronology_annotation({"temporaryId": "$chronology.ext", "provenance": [], "value": {"kind": "civil", "calendarId": CALENDAR, "year": "0", "unexpected": True}})
    with pytest.raises(UsageError):
        _chronology_catalog({"calendars": [{"id": CALENDAR, "unexpected": True}], "eras": [], "anchors": []})


@pytest.mark.parametrize(
    ("value", "tag"),
    [
        ({"kind": "civil", "calendarId": CALENDAR, "year": "0", "x-value": {"year": "opaque"}}, "civil"),
        ({"kind": "era", "eraId": "era_0123456789ABCDEFGHJKMNPQRS", "year": "0", "x-value": {"year": "opaque"}}, "era"),
        ({"kind": "approximate", "displayValue": "about", "bounds": {"lower": None, "upper": None, "x-bounds": {"year": "opaque"}}, "x-value": {"year": "opaque"}}, "approx"),
        ({"kind": "relative", "relation": "after", "beforeId": "event_0123456789ABCDEFGHJKMNPQRS", "x-value": {"year": "opaque"}}, "relative"),
        ({"kind": "duration", "unit": "day", "value": "1", "x-value": {"year": "opaque"}}, "duration"),
        ({"kind": "conflict", "claims": [{"kind": "duration", "unit": "day", "value": "1"}, {"kind": "duration", "unit": "month", "value": "2"}], "x-value": {"year": "opaque"}}, "conflict"),
    ],
)
def test_authoring_value_extensions_are_opaque_for_every_value_kind(value, tag):
    source = _chronology_date(value)
    assert source[tag]["x-value"] == {"year": "opaque"}
    if tag == "approx":
        assert source[tag]["bounds"]["x-bounds"] == {"year": "opaque"}


def _nested_conflict(depth: int) -> dict[str, object]:
    leaf: dict[str, object] = {"kind": "duration", "unit": "day", "value": "1"}
    for _ in range(depth):
        leaf = {"kind": "conflict", "claims": [leaf, {"kind": "relative", "relation": "after", "beforeId": "event_0123456789ABCDEFGHJKMNPQRS"}]}
    return leaf


def test_authoring_conflict_depth_is_source_aligned_and_never_recurses_unbounded():
    assert "conflict" in _chronology_date(_nested_conflict(64))
    with pytest.raises(UsageError, match="conflict nesting exceeds 64"):
        _chronology_date(_nested_conflict(65))
    with pytest.raises(UsageError, match="conflict nesting exceeds 64"):
        _chronology_date(_nested_conflict(300))
