"""Public chronology-v1 codec boundary tests."""
from pathlib import Path

import pytest
import yaml

from wedl.chronology import CivilDate, CivilRange
from wedl.chronology_api import PROTOCOL, convert_date, decode_date_value, encode_date_value, format_date, search_annotations
from wedl.errors import UsageError


CALENDAR = "calendar_0123456789ABCDEFGHJKMNPQRS"


def test_public_decimal_dates_are_strings_and_round_trip_without_integer_leakage():
    value = decode_date_value({"kind": "civil", "calendarId": CALENDAR, "year": "-9223372036854775808", "month": "1", "day": "2"})
    assert value == CivilDate(CALENDAR, -(2 ** 63), 1, 2)
    assert encode_date_value(value) == {"kind": "civil", "calendarId": CALENDAR, "year": "-9223372036854775808", "month": "1", "day": "2"}
    assert PROTOCOL == "wedl-chronology/v1"


@pytest.mark.parametrize("year", [0, True, 1.0, "+1", "-0", "01", " 1", "9223372036854775808"])
def test_public_decimal_dates_reject_noncanonical_numbers(year):
    with pytest.raises(UsageError):
        decode_date_value({"kind": "civil", "calendarId": CALENDAR, "year": year})


def test_open_ranges_and_bounded_conflicts_are_closed_public_shapes():
    value = decode_date_value({"kind": "range", "calendarId": CALENDAR, "lower": None, "upper": {"calendarId": CALENDAR, "year": "0"}})
    assert value == CivilRange(CALENDAR, None, CivilDate(CALENDAR, 0))
    conflict = decode_date_value({"kind": "conflict", "claims": [
        {"kind": "civil", "calendarId": CALENDAR, "year": "0"},
        {"kind": "civil", "calendarId": CALENDAR, "year": "1"},
    ]})
    assert len(conflict.claims) == 2
    with pytest.raises(UsageError):
        decode_date_value({"kind": "range", "calendarId": CALENDAR, "lower": None, "upper": None, "extra": True})

    vectors = yaml.safe_load((Path(__file__).parents[1] / "tests/fixtures/architecture/chronology-api-v1.yaml").read_text(encoding="utf-8"))["negative"]
    date_cases = {"numericCoordinate", "positiveOverflow", "negativeOverflow", "dayWithoutMonth", "extraDateField"}
    target_cases = {"malformedTarget", "ambiguousTarget"}
    search_cases = {"betweenWithoutUpper", "upperOutsideBetween"}
    assert set(vectors) == date_cases | target_cases | search_cases
    for name in sorted(date_cases):
        with pytest.raises(UsageError):
            decode_date_value(vectors[name])
    for name in sorted(target_cases):
        with pytest.raises(UsageError):
            convert_date(None, {"protocol": PROTOCOL, "value": {"kind": "civil", "calendarId": CALENDAR, "year": "0"}, "target": vectors[name]})
    for name in sorted(search_cases):
        with pytest.raises(UsageError):
            search_annotations(None, {"protocol": PROTOCOL, **vectors[name]})


@pytest.mark.parametrize("target", [{"calendarId": ""}, {"calendarId": " \t"}, {"calendarId": 1}, {"eraId": None}, {"calendarId": CALENDAR, "eraId": "era"}])
def test_conversion_target_is_closed_nonempty_before_repository_access(target):
    request = {"protocol": PROTOCOL, "value": {"kind": "civil", "calendarId": CALENDAR, "year": "0"}, "target": target}
    with pytest.raises(UsageError):
        convert_date(None, request)


@pytest.mark.parametrize("value", [
    {"kind": "civil", "calendarId": "", "year": "0"},
    {"kind": "civil", "calendarId": " \t", "year": "0"},
    {"kind": "era", "eraId": "\n", "year": "0"},
    {"kind": "range", "calendarId": " ", "lower": None, "upper": None},
    {"kind": "approximate", "displayValue": "about", "bounds": {"calendarId": "", "lower": None, "upper": None}},
])
def test_public_date_identifiers_reject_blank_values(value):
    with pytest.raises(UsageError):
        decode_date_value(value)


def _nested_conflict(depth: int) -> dict[str, object]:
    leaf: dict[str, object] = {"kind": "civil", "calendarId": CALENDAR, "year": "0"}
    for _ in range(depth):
        leaf = {"kind": "conflict", "claims": [leaf, {"kind": "civil", "calendarId": CALENDAR, "year": "1"}]}
    return leaf


def test_read_conflicts_are_not_advertised_for_format_or_convert_but_depth_is_bounded():
    conflict = _nested_conflict(1)
    with pytest.raises(UsageError):
        format_date(None, {"protocol": PROTOCOL, "value": conflict})
    assert len(decode_date_value(_nested_conflict(64)).claims) == 2
    with pytest.raises(UsageError, match="conflict nesting exceeds 64"):
        decode_date_value(_nested_conflict(65))
    with pytest.raises(UsageError, match="conflict nesting exceeds 64"):
        decode_date_value(_nested_conflict(300))
