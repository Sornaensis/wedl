from __future__ import annotations

from wedl.chronology import Anchor, AxisDay, CalendarDefinition, CalendarEpoch, CycleRule, EraDefinition, Invalid, InvalidReason, LocalDay, Month, Ok, prepare_anchors, prepare_calendar, prepare_catalog
from wedl.model import StoryTime

CAL = "calendar_0123456789ABCDEFGHJKMNPQRS"
ERA = "era_0123456789ABCDEFGHJKMNPQRS"
A = "chronology_0123456789ABCDEFGHJKMNPQRS"
B = "chronology_1123456789ABCDEFGHJKMNPQRS"

def calendar(epoch: bool = True) -> CalendarDefinition:
    return CalendarDefinition(CAL, "Calendar", (Month(1, 30),), CycleRule(1), CalendarEpoch(LocalDay(0, 1, 1), AxisDay(0)) if epoch else None)

def test_exact_reserved_ids() -> None:
    assert isinstance(prepare_calendar(calendar()), Ok)
    assert isinstance(prepare_calendar(CalendarDefinition("calendar_SHORT", "bad", (Month(1, 1),), CycleRule(1))), Invalid)

def test_anchor_duplicate_tie_cross() -> None:
    one, two = StoryTime("main", 1), StoryTime("main", 2)
    duplicate = prepare_anchors((Anchor(A, AxisDay(0), one), Anchor(A, AxisDay(1), two)))
    tie = prepare_anchors((Anchor(A, AxisDay(0), one), Anchor(B, AxisDay(0), two)))
    cross = prepare_anchors((Anchor(A, AxisDay(0), two), Anchor(B, AxisDay(1), one)))
    assert duplicate == Invalid(InvalidReason.ANCHOR, "duplicate anchor id")
    assert isinstance(tie, Invalid) and "same axis" in tie.detail
    assert isinstance(cross, Invalid) and "cross" in cross.detail

def test_total_boundaries_and_caps() -> None:
    assert isinstance(prepare_calendar(object()), Invalid)
    malformed = CalendarDefinition(CAL, "bad", (Month(1, 1),), CycleRule(1), CalendarEpoch("bad", AxisDay(0)))  # type: ignore[arg-type]
    assert isinstance(prepare_calendar(malformed), Invalid)
    anchors = tuple(Anchor(f"chronology_{i:026X}", AxisDay(i), StoryTime("main", i)) for i in range(500))
    assert isinstance(prepare_anchors(anchors), Ok)
    more = anchors + (Anchor("chronology_99999999999999999999999999", AxisDay(501), StoryTime("main", 501)),)
    assert isinstance(prepare_anchors(more), Invalid)
    assert isinstance(prepare_catalog((calendar(),) * 501), Invalid)
    assert isinstance(prepare_catalog((calendar(),), (), more), Invalid)

def test_malformed_era_is_invalid_not_exception() -> None:
    assert isinstance(prepare_catalog((calendar(),), (object(),)), Invalid)
    assert isinstance(prepare_catalog((calendar(),), (EraDefinition(ERA, CAL, "era", (), 1, 0, 0),)), Invalid)  # type: ignore[arg-type]

def test_axis_round_trip_negative_year_and_monotonicity() -> None:
    from wedl.chronology import axis_to_civil, civil_to_axis
    prepared = prepare_calendar(calendar())
    assert isinstance(prepared, Ok)
    previous = None
    for year in range(-249, 251):
        result = civil_to_axis(prepared.value, LocalDay(year, 1, 1))
        assert isinstance(result, Ok)
        assert axis_to_civil(prepared.value, result.value) == Ok(LocalDay(year, 1, 1))
        if previous is not None: assert previous.value < result.value.value
        previous = result.value

def test_table_gap_is_unavailable_but_bad_epoch_is_definition_error() -> None:
    from wedl.chronology import TableRule, TableYear, Unavailable, civil_to_axis
    table = CalendarDefinition(CAL, "T", (Month(1, 1),), TableRule((TableYear(0, ()), TableYear(2, ()))), CalendarEpoch(LocalDay(0, 1, 1), AxisDay(0)))
    prepared = prepare_calendar(table)
    assert isinstance(prepared, Ok)
    assert isinstance(civil_to_axis(prepared.value, LocalDay(1, 1, 1)), Unavailable)
    bad = CalendarDefinition(CAL, "T", (Month(1, 1),), TableRule((TableYear(0, ()),)), CalendarEpoch(LocalDay(1, 1, 1), AxisDay(0)))
    assert isinstance(prepare_calendar(bad), Invalid)


def test_table_requires_at_least_one_year() -> None:
    from wedl.chronology import TableRule
    assert isinstance(prepare_calendar(CalendarDefinition(CAL, "T", (Month(1, 1),), TableRule(()))), Invalid)

def test_era_and_open_range_semantics_remain_noncanonical() -> None:
    from wedl.chronology import CivilDate, CivilRange, EraBounds, EraDate, normalize_date, era_to_civil
    era = EraDefinition(ERA, CAL, "E", (), False, 1, 0, EraBounds(LocalDay(-1, 1, 1), LocalDay(1, 1, 30)))
    catalog = prepare_catalog((calendar(),), (era,))
    assert isinstance(catalog, Ok)
    assert isinstance(era_to_civil(catalog.value.era(ERA), EraDate(ERA, 0, 1, 1)), Invalid)
    opened = normalize_date(catalog.value, CivilRange(CAL, None, CivilDate(CAL, 0, 1, 2)))
    assert isinstance(opened, Ok) and opened.value.lower is None

def test_cycle_layout_and_table_inversion() -> None:
    from wedl.chronology import TableRule, TableYear, axis_to_civil, civil_to_axis
    cycle = prepare_calendar(calendar())
    assert isinstance(cycle, Ok)
    for year in (-2, -1, 0, 1, 2):
        value = civil_to_axis(cycle.value, LocalDay(year, 1, 30))
        assert isinstance(value, Ok) and axis_to_civil(cycle.value, value.value) == Ok(LocalDay(year, 1, 30))
    table = CalendarDefinition(CAL, "T", (Month(1, 2),), TableRule((TableYear(-1, ()), TableYear(0, ()))), CalendarEpoch(LocalDay(-1, 1, 1), AxisDay(0)))
    prepared = prepare_calendar(table)
    assert isinstance(prepared, Ok)
    value = civil_to_axis(prepared.value, LocalDay(0, 1, 2))
    assert isinstance(value, Ok) and axis_to_civil(prepared.value, value.value) == Ok(LocalDay(0, 1, 2))

def test_zero_skip_i64_and_approx_conflict_refusal() -> None:
    from wedl.chronology import ApproximateDate, CivilDate, CivilRange, ConflictingDates, EraDate, compare_dates, era_to_civil, normalize_date, Unavailable
    era = EraDefinition(ERA, CAL, "E", (), False, 1, 0)
    catalog = prepare_catalog((calendar(),), (era,))
    assert isinstance(catalog, Ok)
    assert isinstance(era_to_civil(catalog.value.era(ERA), EraDate(ERA, 0)), Invalid)
    assert isinstance(era_to_civil(catalog.value.era(ERA), EraDate(ERA, 2**63 - 1)), Invalid)
    approx = ApproximateDate("maybe", CivilRange(CAL, None, None))
    assert isinstance(compare_dates(catalog.value, approx, CivilDate(CAL, 0, 1, 1)), Unavailable)
    assert isinstance(normalize_date(catalog.value, ConflictingDates((CivilDate(CAL, 0), CivilDate(CAL, 1)))), Unavailable)

def test_total_malformed_and_definition_endpoint_classifications() -> None:
    from wedl.chronology import TableRule, TableYear
    assert isinstance(prepare_anchors((object(),)), Invalid)
    assert isinstance(prepare_catalog((calendar(),), (object(),)), Invalid)
    assert isinstance(prepare_calendar(CalendarDefinition(CAL, "x", (Month(1, 1),), TableRule((TableYear(0, ()),)), CalendarEpoch(LocalDay(1, 1, 1), AxisDay(0)))), Invalid)

def test_nested_anchor_and_era_bounds_are_total_invalid_results() -> None:
    valid_time = StoryTime("main", 0)
    malformed = (
        Anchor(A, object(), valid_time),  # type: ignore[arg-type]
        Anchor(A, AxisDay(0), object()),  # type: ignore[arg-type]
        Anchor(A, AxisDay(0), valid_time, []),  # type: ignore[arg-type]
        Anchor(A, AxisDay(0), valid_time, ("ok", 1)),  # type: ignore[arg-type]
    )
    for anchor in malformed:
        result = prepare_anchors((anchor,))
        assert isinstance(result, Invalid) and result.reason is InvalidReason.ANCHOR
    bad_bounds = EraDefinition(ERA, CAL, "E", (), True, 0, 0, object())  # type: ignore[arg-type]
    assert isinstance(prepare_catalog((calendar(),), (bad_bounds,)), Invalid)
