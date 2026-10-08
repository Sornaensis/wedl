"""Pure, test-local conformance checks for accepted ADR 0003.

These helpers deliberately model only the decision vector.  They are not WEDL
runtime code and must not be imported by production modules.
"""

from __future__ import annotations

from adrai_fixtures import current_decision, current_status

from math import lcm
from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[1]
ADR = current_decision("A01M48NNH43QCFRPC68NQQTFT63")
VECTORS = ROOT / "tests/fixtures/architecture/calendar-chronology-semantics-v1.yaml"


def _vector() -> dict:
    return yaml.safe_load(VECTORS.read_text(encoding="utf-8"))


def _checked_i64(value: int, bounds: dict) -> int:
    if not bounds["min"] <= value <= bounds["max"]:
        raise ValueError("outside signed i64")
    return value


def _checked_subtract(left: int, right: int, bounds: dict) -> int:
    return _checked_i64(_checked_i64(left, bounds) - _checked_i64(right, bounds), bounds)


def _checked_add(left: int, right: int, bounds: dict) -> int:
    return _checked_i64(_checked_i64(left, bounds) + _checked_i64(right, bounds), bounds)


def _month_length(calendar: dict, year: int, month: int) -> int:
    months = calendar["months"]
    if 1 <= month <= len(months):
        length = months[month - 1]
        leap = calendar.get("leap")
        if leap and month == leap["month"] and year % leap["cycle"] in leap["residues"]:
            length += leap["extra_days"]
        return length
    intercalary = calendar.get("intercalary")
    if intercalary and month == intercalary["month"] and year % intercalary["cycle"] in intercalary["residues"]:
        return intercalary["length"]
    raise ValueError("invalid month")


def _validate_civil(calendar: dict, civil: dict) -> tuple[int, int, int]:
    year, month, day = civil["year"], civil["month"], civil["day"]
    if not 1 <= day <= _month_length(calendar, year, month):
        raise ValueError("invalid civil day")
    return year, month, day


def _year_length(calendar: dict, year: int) -> int:
    months = sum(calendar["months"])
    leap = calendar.get("leap")
    if leap and year % leap["cycle"] in leap["residues"]:
        months += leap["extra_days"]
    intercalary = calendar.get("intercalary")
    if intercalary and year % intercalary["cycle"] in intercalary["residues"]:
        months += intercalary["length"]
    return months


def _prepare_calendar(calendar: dict) -> dict:
    """Build finite-cycle prefixes once; this is test-local setup work."""
    cycles = [rule["cycle"] for rule in (calendar.get("leap"), calendar.get("intercalary")) if rule]
    cycle = lcm(*cycles) if cycles else 1
    year_prefix = [0]
    month_prefixes = []
    definition_cells = 0
    for residue in range(cycle):
        starts = [0]
        month_count = len(calendar["months"]) + (1 if calendar.get("intercalary") and residue % calendar["intercalary"]["cycle"] in calendar["intercalary"]["residues"] else 0)
        for month in range(1, month_count + 1):
            starts.append(starts[-1] + _month_length(calendar, residue, month))
        month_prefixes.append(starts)
        year_prefix.append(year_prefix[-1] + starts[-1])
        definition_cells += 1 + month_count
    return {"cycle": cycle, "cycle_days": year_prefix[-1], "year_prefix": year_prefix, "month_prefixes": month_prefixes, "definition_cells": definition_cells}


def _civil_ordinal(calendar: dict, civil: dict, plan: dict, work: dict) -> tuple[int, int]:
    year, month, day = _validate_civil(calendar, civil)
    quotient, residue = divmod(year, plan["cycle"])
    work["quotient_remainder"] += 1
    work["year_prefix_lookup"] += 1
    work["month_prefix_lookup"] += 1
    work["month_length_validation"] += 1
    month_work = month - 1
    return quotient * plan["cycle_days"] + plan["year_prefix"][residue] + plan["month_prefixes"][residue][month_work] + day - 1, month_work


def _civil_to_axis(calendar: dict, civil: dict, bounds: dict, plan: dict, work: dict) -> tuple[int, int]:
    if calendar["epoch"] is None:
        raise LookupError("unavailable shared axis")
    ordinal, month_work = _civil_ordinal(calendar, civil, plan, work)
    epoch = calendar["epoch"]
    epoch_ordinal, _ = _civil_ordinal(calendar, epoch["civil"], plan, work)
    relative = _checked_subtract(ordinal, epoch_ordinal, bounds)
    return _checked_add(epoch["axis_day"], relative, bounds), month_work


def _interval_for_precision(calendar: dict, case: dict) -> tuple[tuple[int, int, int], tuple[int, int, int]]:
    source = case["input"]
    year, month, day = source["year"], source["month"], source["day"]
    if day is not None:
        _validate_civil(calendar, source)
        return (year, month, day), (year, month, day)
    if month is not None:
        return (year, month, 1), (year, month, _month_length(calendar, year, month))
    last_month = len(calendar["months"]) + (1 if calendar.get("intercalary") and year % calendar["intercalary"]["cycle"] in calendar["intercalary"]["residues"] else 0)
    return (year, 1, 1), (year, last_month, _month_length(calendar, year, last_month))


def _overlaps(left: list[int | None], right: list[int | None]) -> bool:
    left_start, left_end = left
    right_start, right_end = right
    return not ((left_end is not None and right_start is not None and left_end < right_start) or (right_end is not None and left_start is not None and right_end < left_start))


def _validate_anchors(raw: list[list], bounds: dict) -> list[tuple[int, tuple[str, int, int]]]:
    unique: list[tuple[int, tuple[str, int, int]]] = []
    seen = set()
    axes, times = set(), set()
    for item in raw:
        axis, story = _checked_i64(item[0], bounds), tuple(item[1])
        pair = (axis, story)
        if pair in seen:  # Exact provenance aliases are deliberately coalesced.
            continue
        if axis in axes or story in times:
            raise ValueError("anchor tie")
        seen.add(pair)
        axes.add(axis)
        times.add(story)
        unique.append(pair)
    ordered = sorted(unique)
    if any(ordered[index][1] >= ordered[index + 1][1] for index in range(len(ordered) - 1)):
        raise ValueError("crossed or decreasing anchors")
    return ordered


def _mapping_candidates(candidates: list[list]) -> list[list]:
    return sorted((list(value) for value in candidates), key=tuple)


def _assert_outcome(case: dict, actual: dict) -> None:
    assert actual == case["outcome"], case


def test_vector_is_parseable_concrete_and_adr_defines_every_semantic_id() -> None:
    vector = _vector()
    text = " ".join(ADR.read_text(encoding="utf-8").split())
    index = ADR.metadata
    assert current_status(index["adr"])["state"] == "active"
    assert vector["status"] == "accepted-semantic-only"
    assert "**Status:** Accepted" in text
    assert "**Approved:** 2026-08-27 by Project owner (user)" in text
    assert "The conservative decisions listed in this ADR only." in text
    assert index["adr"] == "A01M48NNH43QCFRPC68NQQTFT63" and ADR.path.is_file()
    assert 'schema = "adrai/decision/v1"' in text
    assert "proposed semantic contract" not in text.lower()
    assert "## Proposed decision" not in text
    assert "no source schema, parser, API, model, compiler, query, UI" in text
    assert vector["semantic_fixture"]["years"] == {"first": -249, "last": 250, "count": 500}
    for identifier, statement in vector["semantics"].items():
        assert " ".join(f"`{identifier}`: {statement}".split()) in text


def test_generator_scale_memberships_and_finite_calendar_work_are_reproducible() -> None:
    fixture = _vector()["semantic_fixture"]
    generated = [
        (-249 + number // fixture["generator"]["yearly_claims"], (number % 20) // 2 + 1, number % 2 + 1)
        for number in range(fixture["generator"]["expected_events"])
    ]
    memberships = [fixture["generator"]["membership_sequence"][number % 4] for number in range(len(generated))]
    assert len(generated) == 10_000 and generated[0] == (-249, 1, 1) and generated[-1] == (250, 10, 2)
    assert memberships[:8] == ["thread-a", "thread-b", "thread-a", "thread-b"] * 2
    assert {item[0] for item in generated} == set(range(-249, 251))
    calendars = fixture["calendars"]
    assert sum(len(value["months"]) for value in calendars.values()) == 25
    assert max(len(value["months"]) for value in calendars.values()) <= 12
    assert all(len(value.get("leap", {}).get("residues", [])) <= 1 for value in calendars.values())
    assert fixture["anchors"]["expected_count"] == 100
    assert len([(-20_000 + number * 400, ["main", number, 0]) for number in range(100)]) == 100
    plans = {name: _prepare_calendar(calendar) for name, calendar in calendars.items()}
    expected = fixture["work_bounds"]["definition_prep"]
    assert {name: plans[name]["definition_cells"] for name in plans} == {"solar": expected["solar_cells"], "regnal": expected["regnal_cells"], "isolated": expected["isolated_cells"]}
    assert sum(plan["definition_cells"] for plan in plans.values()) == expected["total_cells"]


def test_civil_axis_precision_approximation_and_conflict_cases() -> None:
    vector = _vector()
    fixture, cases = vector["semantic_fixture"], vector["cases"]
    solar = fixture["calendars"]["solar"]
    bounds = fixture["i64"]
    plans = {name: _prepare_calendar(calendar) for name, calendar in fixture["calendars"].items()}
    fixed = cases["fixed_difference"]
    _assert_outcome(fixed, {"kind": "exact", "days": _checked_subtract(fixed["input"]["left_axis_day"], fixed["input"]["right_axis_day"], bounds)})
    _assert_outcome(cases["fixed_difference_refused"], {"kind": "unavailable"})
    lower_boundary = cases["fixed_difference_lower_boundary"]
    _assert_outcome(lower_boundary, {"kind": "exact", "days": _checked_subtract(lower_boundary["input"]["left_axis_day"], lower_boundary["input"]["right_axis_day"], bounds)})
    try:
        overflow_difference = cases["fixed_difference_overflow"]["input"]
        _checked_subtract(overflow_difference["left_axis_day"], overflow_difference["right_axis_day"], bounds)
    except ValueError:
        _assert_outcome(cases["fixed_difference_overflow"], {"kind": "invalid", "reason": "i64-overflow"})
    else:
        raise AssertionError("fixed-day difference overflow must be invalid")
    assert _interval_for_precision(solar, cases["precision_year"]) == ((0, 1, 1), (0, 12, 31))
    assert _interval_for_precision(solar, cases["precision_month"]) == ((0, 2, 1), (0, 2, 29))
    regnal = fixture["calendars"]["regnal"]
    assert _interval_for_precision(regnal, cases["precision_regnal_year_zero"]) == ((0, 1, 1), (0, 13, 5))
    assert _interval_for_precision(regnal, cases["precision_regnal_negative_cycle"]) == ((-5, 1, 1), (-5, 13, 5))
    explicit = cases["explicit_range"]
    _assert_outcome(explicit, {"kind": "interval", "start": 0, "end": 30, "inclusive": True})
    _assert_outcome(cases["omitted_range"], {"kind": "interval", "start": None, "end": 30, "unbounded_start": True})
    qualitative = cases["qualitative_approximate"]
    _assert_outcome(qualitative, {"kind": "display-valid", "exact_order": "unavailable"})
    bounded = cases["bounded_approximate_overlap"]
    _assert_outcome(bounded, {"kind": "overlap-only", "overlaps": _overlaps(**{"left": bounded["input"]["left"], "right": bounded["input"]["right"]})})
    conflict = cases["conflict"]
    _assert_outcome(conflict, {"kind": "ordered-conflict", "values": [claim["value"] for claim in conflict["input"]["claims"]]})
    for name in (
        "calendar_conversion",
        "calendar_conversion_solar_negative",
        "calendar_conversion_regnal_epoch",
        "calendar_conversion_regnal_intercalary",
        "calendar_conversion_regnal_cycle",
    ):
        calendar = fixture["calendars"][cases[name]["input"]["calendar"]]
        work = {key: 0 for key in fixture["work_bounds"]["per_date_normalization"]}
        _assert_outcome(cases[name], {"kind": "unique", "axis_day": _civil_to_axis(calendar, cases[name]["input"]["civil"], bounds, plans[cases[name]["input"]["calendar"]], work)[0]})
        # Subject and epoch are each normalized by bounded lookups, never by a
        # year- or month-length scan.
        assert work == {key: value * 2 for key, value in fixture["work_bounds"]["per_date_normalization"].items()}
    for name in ("solar_leap_negative", "solar_year_zero", "regnal_intercalary"):
        calendar = fixture["calendars"][cases[name]["input"]["calendar"]]
        _validate_civil(calendar, cases[name]["input"]["civil"])
        _assert_outcome(cases[name], {"kind": "valid"})
    for name in ("calendar_conversion_invalid", "overflow"):
        try:
            if name == "overflow":
                _checked_i64(cases[name]["input"]["axis_day"], bounds)
            else:
                _validate_civil(solar, cases[name]["input"]["civil"])
        except ValueError:
            _assert_outcome(cases[name], {"kind": "invalid"})
        else:
            raise AssertionError(f"{name} must be invalid")
    try:
        _civil_to_axis(fixture["calendars"]["isolated"], cases["isolated_calendar"]["input"]["civil"], bounds, plans["isolated"], {key: 0 for key in fixture["work_bounds"]["per_date_normalization"]})
    except LookupError:
        _assert_outcome(cases["isolated_calendar"], {"kind": "unavailable"})
    else:
        raise AssertionError("isolated calendar unexpectedly mapped")


def test_anchor_aliases_rejections_mapping_order_and_era_overlap() -> None:
    vector = _vector()
    fixture, cases = vector["semantic_fixture"], vector["cases"]
    bounds = fixture["i64"]
    generated = [[-20_000 + number * 400, ["main", number, 0]] for number in range(100)]
    assert len(_validate_anchors(generated, bounds)) == 100
    _assert_outcome(cases["anchor_unique"], {"kind": "unique", "story_times": [["main", 0, 0]]})
    ambiguous = cases["mapping_ambiguous"]
    _assert_outcome(ambiguous, {"kind": "ambiguous", "story_times": _mapping_candidates(ambiguous["input"]["candidates"])})
    aliases = cases["anchor_alias"]["input"]["anchors"]
    assert len(_validate_anchors(aliases, bounds)) == cases["anchor_alias"]["outcome"]["count"]
    _assert_outcome(cases["anchor_alias"], {"kind": "alias-coalesced", "count": 1})
    for name in ("anchor_duplicate_axis", "anchor_duplicate_story_time", "anchor_decreasing", "anchor_crossed"):
        try:
            _validate_anchors(cases[name]["input"]["anchors"], bounds)
        except ValueError:
            _assert_outcome(cases[name], {"kind": "invalid"})
        else:
            raise AssertionError(f"{name} must be rejected")
    overlapping = [era["id"] for era in fixture["eras"] if era["machine_year"] == cases["era_overlap"]["input"]["machine_year"]]
    _assert_outcome(cases["era_overlap"], {"kind": "display-ambiguous", "eras": overlapping})


def test_thread_membership_does_not_change_shared_axis_or_story_time() -> None:
    invariant = _vector()["thread_invariance"]
    records = invariant["records"]
    assert [record["axis_day"] for record in records] == invariant["outcome"]["shared_axis_days"]
    assert [record["story_time"] for record in records] == invariant["outcome"]["shared_story_times"]
    assert {membership for record in records for membership in record["memberships"]} == {"thread-a", "thread-b"}
