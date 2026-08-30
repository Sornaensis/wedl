"""Pure, bounded chronology arithmetic.

This module deliberately knows nothing about source files, databases, or the
application clock.  A calendar either has an explicit mapping to its signed
day axis, or it remains an isolated display calendar.
"""
from __future__ import annotations

from bisect import bisect_right
from dataclasses import dataclass
from enum import Enum
import re
from typing import Generic, Iterable, TypeAlias, TypeVar

from .model import StoryTime


I64_MIN = -(2**63)
I64_MAX = 2**63 - 1


class ChronologyInvariantError(ValueError):
    """Raised only for impossible direct construction of kernel values."""


def _i64(value: int, field: str = "value") -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise ChronologyInvariantError(f"{field} must be a signed integer")
    if not I64_MIN <= value <= I64_MAX:
        raise ChronologyInvariantError(f"{field} is outside signed i64")
    return value


def _id(value: str, field: str = "id") -> str:
    if not isinstance(value, str) or not value.strip():
        raise ChronologyInvariantError(f"{field} must be non-empty")
    return value


_KERNEL_ID = re.compile(r"^(calendar|era|chronology)_[0-9ABCDEFGHJKMNPQRSTVWXYZ]{26}$")


def _definition_id(value: str, prefix: str) -> str:
    _id(value)
    if not value.startswith(prefix) or not _KERNEL_ID.fullmatch(value):
        raise ChronologyInvariantError(f"id must be an uppercase Crockford {prefix} id")
    return value


def _checked_add(left: int, right: int) -> int:
    return _i64(left + right)


def _checked_sub(left: int, right: int) -> int:
    return _i64(left - right)


@dataclass(frozen=True, slots=True)
class AxisDay:
    value: int
    def __post_init__(self) -> None: _i64(self.value, "axis day")


@dataclass(frozen=True, slots=True)
class LocalDay:
    year: int
    month: int
    day: int
    def __post_init__(self) -> None:
        _i64(self.year, "year")
        if isinstance(self.month, bool) or not isinstance(self.month, int): raise ChronologyInvariantError("month must be an integer")
        if isinstance(self.day, bool) or not isinstance(self.day, int): raise ChronologyInvariantError("day must be an integer")


@dataclass(frozen=True, slots=True)
class CivilDate:
    calendar_id: str
    year: int
    month: int | None = None
    day: int | None = None
    def __post_init__(self) -> None:
        _id(self.calendar_id, "calendar id"); _i64(self.year, "year")
        if self.day is not None and self.month is None: raise ChronologyInvariantError("day requires month")
        for name, value in (("month", self.month), ("day", self.day)):
            if value is not None and (isinstance(value, bool) or not isinstance(value, int)):
                raise ChronologyInvariantError(f"{name} must be an integer")


@dataclass(frozen=True, slots=True)
class EraDate:
    era_id: str
    year: int
    month: int | None = None
    day: int | None = None
    def __post_init__(self) -> None:
        _id(self.era_id, "era id"); _i64(self.year, "display year")
        if self.day is not None and self.month is None: raise ChronologyInvariantError("day requires month")
        for name, value in (("month", self.month), ("day", self.day)):
            if value is not None and (isinstance(value, bool) or not isinstance(value, int)):
                raise ChronologyInvariantError(f"{name} must be an integer")


@dataclass(frozen=True, slots=True)
class CivilRange:
    calendar_id: str
    lower: CivilDate | None
    upper: CivilDate | None
    def __post_init__(self) -> None:
        _id(self.calendar_id, "calendar id")
        for value in (self.lower, self.upper):
            if value is not None and value.calendar_id != self.calendar_id:
                raise ChronologyInvariantError("range endpoint calendar differs")


@dataclass(frozen=True, slots=True)
class ApproximateDate:
    display_value: str
    bounds: CivilRange
    def __post_init__(self) -> None:
        if not isinstance(self.display_value, str):
            raise ChronologyInvariantError("approximation display value must be a string")
        if not isinstance(self.bounds, CivilRange):
            raise ChronologyInvariantError("approximation requires CivilRange bounds")


@dataclass(frozen=True, slots=True)
class ConflictingDates:
    claims: tuple["DateValue", ...]
    def __post_init__(self) -> None:
        if not isinstance(self.claims, tuple) or not 2 <= len(self.claims) <= 64:
            raise ChronologyInvariantError("conflict requires 2..64 claims")


DateValue: TypeAlias = CivilDate | EraDate | CivilRange | ApproximateDate | ConflictingDates


@dataclass(frozen=True, slots=True)
class Month:
    number: int
    days: int
    label: str | None = None


@dataclass(frozen=True, slots=True)
class MonthDelta:
    target_month: int
    delta_days: int


@dataclass(frozen=True, slots=True)
class IntercalaryMonth:
    number: int
    days: int
    label: str | None = None


Change: TypeAlias = MonthDelta | IntercalaryMonth


@dataclass(frozen=True, slots=True)
class CycleOverride:
    residue: int
    change: Change


@dataclass(frozen=True, slots=True)
class CycleRule:
    period: int
    overrides: tuple[CycleOverride, ...] = ()


@dataclass(frozen=True, slots=True)
class TableYear:
    year: int
    overrides: tuple[Change, ...] = ()


@dataclass(frozen=True, slots=True)
class TableRule:
    years: tuple[TableYear, ...]


Rule: TypeAlias = CycleRule | TableRule


@dataclass(frozen=True, slots=True)
class CalendarEpoch:
    civil: LocalDay
    axis_day: AxisDay


@dataclass(frozen=True, slots=True)
class CalendarDefinition:
    id: str
    label: str
    months: tuple[Month, ...]
    rule: Rule
    epoch: CalendarEpoch | None = None


@dataclass(frozen=True, slots=True)
class EraBounds:
    lower: LocalDay
    upper: LocalDay
    def __post_init__(self) -> None:
        if not isinstance(self.lower, LocalDay) or not isinstance(self.upper, LocalDay):
            raise ChronologyInvariantError("era bounds require exact LocalDay endpoints")


@dataclass(frozen=True, slots=True)
class EraDefinition:
    id: str
    calendar_id: str
    label: str
    aliases: tuple[str, ...]
    display_year_zero: bool
    display_epoch_year: int
    machine_epoch_year: int
    bounds: EraBounds | None = None


@dataclass(frozen=True, slots=True)
class Anchor:
    id: str
    axis_day: AxisDay
    story_time: StoryTime
    provenance: tuple[str, ...] = ()


class InvalidReason(Enum):
    DEFINITION = "definition"
    DATE = "date"
    RANGE = "range"
    ERA = "era"
    ANCHOR = "anchor"
    OVERFLOW = "overflow"


class UnavailableReason(Enum):
    NO_EPOCH = "no_epoch"
    TABLE_GAP = "table_gap"
    DISCONNECTED_TABLE = "disconnected_table"
    NO_SHARED_AXIS = "no_shared_axis"
    APPROXIMATE_ONLY = "approximate_only"
    CONFLICTING_CLAIMS = "conflicting_claims"


T = TypeVar("T")


@dataclass(frozen=True, slots=True)
class Ok(Generic[T]):
    value: T


@dataclass(frozen=True, slots=True)
class Invalid:
    reason: InvalidReason
    detail: str


@dataclass(frozen=True, slots=True)
class Unavailable:
    reason: UnavailableReason
    detail: str


Result: TypeAlias = Ok[T] | Invalid | Unavailable


@dataclass(frozen=True, slots=True)
class PreparationStats:
    year_rows: int
    month_prefix_cells: int
    total_cells: int


@dataclass(frozen=True, slots=True)
class _YearLayout:
    year: int
    months: tuple[Month, ...]
    prefixes: tuple[int, ...]
    days: int


@dataclass(frozen=True, slots=True)
class PreparedCalendar:
    definition: CalendarDefinition
    stats: PreparationStats
    cycle_layouts: tuple[_YearLayout, ...] = ()
    table_layouts: tuple[_YearLayout, ...] = ()
    table_years: tuple[int, ...] = ()
    table_prefixes: tuple[int, ...] = ()
    cycle_prefixes: tuple[int, ...] = ()
    cycle_days: int = 0
    epoch_offset: int | None = None


@dataclass(frozen=True, slots=True)
class PreparedEra:
    definition: EraDefinition
    calendar: PreparedCalendar
    bounds: EraBounds | None


@dataclass(frozen=True, slots=True)
class AnchorPoint:
    axis_day: AxisDay
    story_time: StoryTime
    anchor_ids: tuple[str, ...]
    provenance: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class AnchorIndex:
    points: tuple[AnchorPoint, ...]


@dataclass(frozen=True, slots=True)
class ChronologyCatalog:
    calendars: tuple[PreparedCalendar, ...]
    eras: tuple[PreparedEra, ...]
    anchors: AnchorIndex
    def calendar(self, calendar_id: str) -> PreparedCalendar | None:
        return next((item for item in self.calendars if item.definition.id == calendar_id), None)
    def era(self, era_id: str) -> PreparedEra | None:
        return next((item for item in self.eras if item.definition.id == era_id), None)


@dataclass(frozen=True, slots=True)
class ExactAxis:
    day: AxisDay


@dataclass(frozen=True, slots=True)
class AxisInterval:
    lower: AxisDay | None
    upper: AxisDay | None
    def __post_init__(self) -> None:
        if self.lower is not None and self.upper is not None and self.lower.value > self.upper.value:
            raise ChronologyInvariantError("interval lower is after upper")


@dataclass(frozen=True, slots=True)
class ApproximateAxisInterval:
    interval: AxisInterval


class TemporalRelation(Enum):
    BEFORE = "before"
    EQUAL = "equal"
    AFTER = "after"
    OVERLAP = "overlap"


@dataclass(frozen=True, slots=True)
class NoMatch: pass


@dataclass(frozen=True, slots=True)
class Unique:
    value: StoryTime


@dataclass(frozen=True, slots=True)
class Ambiguous:
    values: tuple[StoryTime, ...]


def _invalid(reason: InvalidReason, detail: str) -> Invalid: return Invalid(reason, detail)


def _valid_months(months: tuple[Month, ...]) -> tuple[Month, ...] | Invalid:
    if not isinstance(months, tuple) or not months or len(months) > 64: return _invalid(InvalidReason.DEFINITION, "calendar requires 1..64 months")
    if any(not isinstance(m, Month) for m in months): return _invalid(InvalidReason.DEFINITION, "invalid month")
    if any(isinstance(m.number, bool) or not isinstance(m.number, int) or not 1 <= m.number <= 64 or isinstance(m.days, bool) or not isinstance(m.days, int) or not 1 <= m.days <= 4096 for m in months):
        return _invalid(InvalidReason.DEFINITION, "invalid month")
    if tuple(m.number for m in months) != tuple(sorted(m.number for m in months)) or len({m.number for m in months}) != len(months):
        return _invalid(InvalidReason.DEFINITION, "months must be unique and ordered")
    return months


def _layout(base: tuple[Month, ...], changes: Iterable[Change], year: int) -> _YearLayout | Invalid:
    by_number = {month.number: month for month in base}
    seen: set[int] = set()
    for change in changes:
        if isinstance(change, MonthDelta):
            if isinstance(change.target_month, bool) or not isinstance(change.target_month, int) or change.target_month not in by_number or change.target_month in seen or isinstance(change.delta_days, bool) or not isinstance(change.delta_days, int):
                return _invalid(InvalidReason.DEFINITION, "invalid month delta")
            days = by_number[change.target_month].days + change.delta_days
            if not 1 <= days <= 4096: return _invalid(InvalidReason.DEFINITION, "month delta result is invalid")
            old = by_number[change.target_month]
            by_number[change.target_month] = Month(old.number, days, old.label); seen.add(change.target_month)
        elif isinstance(change, IntercalaryMonth):
            if isinstance(change.number, bool) or not isinstance(change.number, int) or isinstance(change.days, bool) or not isinstance(change.days, int) or change.number in by_number or change.number in seen or not 1 <= change.number <= 64 or not 1 <= change.days <= 4096:
                return _invalid(InvalidReason.DEFINITION, "invalid intercalary month")
            by_number[change.number] = Month(change.number, change.days, change.label); seen.add(change.number)
        else: return _invalid(InvalidReason.DEFINITION, "unknown month change")
    months = tuple(sorted(by_number.values(), key=lambda item: item.number))
    if len(months) > 64: return _invalid(InvalidReason.DEFINITION, "too many effective months")
    prefixes: list[int] = []; total = 0
    for month in months:
        prefixes.append(total); total += month.days
    return _YearLayout(year, months, tuple(prefixes), total)


def prepare_calendar(definition: CalendarDefinition) -> Result[PreparedCalendar]:
    if not isinstance(definition, CalendarDefinition): return _invalid(InvalidReason.DEFINITION, "invalid calendar definition")
    try: _definition_id(definition.id, "calendar_")
    except ChronologyInvariantError as exc: return _invalid(InvalidReason.DEFINITION, str(exc))
    base = _valid_months(definition.months)
    if isinstance(base, Invalid): return base
    if isinstance(definition.rule, CycleRule):
        rule = definition.rule
        if not isinstance(rule.overrides, tuple) or isinstance(rule.period, bool) or not isinstance(rule.period, int) or not 1 <= rule.period <= 500: return _invalid(InvalidReason.DEFINITION, "invalid cycle period")
        if len(base) + len(rule.overrides) > 500:
            return _invalid(InvalidReason.DEFINITION, "calendar mechanics exceed aggregate cap")
        grouped: list[list[Change]] = [[] for _ in range(rule.period)]
        for override in rule.overrides:
            if not isinstance(override, CycleOverride): return _invalid(InvalidReason.DEFINITION, "invalid cycle override")
            if isinstance(override.residue, bool) or not isinstance(override.residue, int) or not 0 <= override.residue < rule.period: return _invalid(InvalidReason.DEFINITION, "invalid cycle residue")
            grouped[override.residue].append(override.change)
        layouts: list[_YearLayout] = []
        for residue, changes in enumerate(grouped):
            item = _layout(base, changes, residue)
            if isinstance(item, Invalid): return item
            layouts.append(item)
        prefixes: list[int] = []; total = 0
        for item in layouts: prefixes.append(total); total += item.days
        prepared = PreparedCalendar(definition, PreparationStats(rule.period, sum(len(x.months) for x in layouts), rule.period + sum(len(x.months) for x in layouts)), tuple(layouts), (), (), (), tuple(prefixes), total)
    elif isinstance(definition.rule, TableRule):
        rows = definition.rule.years
        if not isinstance(rows, tuple) or not rows or len(rows) > 500 or any(not isinstance(row, TableYear) or not isinstance(row.overrides, tuple) for row in rows): return _invalid(InvalidReason.DEFINITION, "table requires typed 1..500 rows")
        if len(base) + len(rows) + sum(len(row.overrides) for row in rows) > 500:
            return _invalid(InvalidReason.DEFINITION, "calendar mechanics exceed aggregate cap")
        years = tuple(row.year for row in rows)
        if any(isinstance(year, bool) or not isinstance(year, int) or not I64_MIN <= year <= I64_MAX for year in years) or years != tuple(sorted(years)) or len(set(years)) != len(years): return _invalid(InvalidReason.DEFINITION, "table years must be unique and ordered i64")
        layouts = []
        for row in rows:
            item = _layout(base, row.overrides, row.year)
            if isinstance(item, Invalid): return item
            layouts.append(item)
        prefixes: list[int] = []; total = 0
        for item in layouts: prefixes.append(total); total += item.days
        prepared = PreparedCalendar(definition, PreparationStats(len(rows), sum(len(x.months) for x in layouts), len(rows) + sum(len(x.months) for x in layouts)), (), tuple(layouts), years, tuple(prefixes), (), 0)
    else: return _invalid(InvalidReason.DEFINITION, "unknown calendar rule")
    if definition.epoch is None: return Ok(prepared)
    if not isinstance(definition.epoch, CalendarEpoch) or not isinstance(definition.epoch.civil, LocalDay) or not isinstance(definition.epoch.axis_day, AxisDay): return _invalid(InvalidReason.DEFINITION, "invalid calendar epoch")
    raw = _ordinal(prepared, definition.epoch.civil)
    if not isinstance(raw, Ok): return _invalid(InvalidReason.DEFINITION, "epoch civil date is not available")
    try: offset = _checked_sub(definition.epoch.axis_day.value, raw.value)
    except ChronologyInvariantError: return _invalid(InvalidReason.OVERFLOW, "epoch offset overflows")
    return Ok(PreparedCalendar(prepared.definition, prepared.stats, prepared.cycle_layouts, prepared.table_layouts, prepared.table_years, prepared.table_prefixes, prepared.cycle_prefixes, prepared.cycle_days, offset))


def _year_layout(calendar: PreparedCalendar, year: int) -> _YearLayout | Unavailable:
    if calendar.cycle_layouts:
        return calendar.cycle_layouts[year % len(calendar.cycle_layouts)]
    index = bisect_right(calendar.table_years, year) - 1
    if index < 0 or calendar.table_years[index] != year: return Unavailable(UnavailableReason.TABLE_GAP, "year is not an explicit table row")
    return calendar.table_layouts[index]


def _ordinal(calendar: PreparedCalendar, day: LocalDay) -> Result[int]:
    layout = _year_layout(calendar, day.year)
    if isinstance(layout, Unavailable): return layout
    index = next((i for i, month in enumerate(layout.months) if month.number == day.month), None)
    if index is None or not 1 <= day.day <= layout.months[index].days: return _invalid(InvalidReason.DATE, "invalid civil day")
    try:
        if calendar.cycle_layouts:
            period = len(calendar.cycle_layouts); quotient, residue = divmod(day.year, period)
            start = _checked_add(quotient * calendar.cycle_days, calendar.cycle_prefixes[residue])
        else:
            row = bisect_right(calendar.table_years, day.year) - 1; start = calendar.table_prefixes[row]
        return Ok(_checked_add(start, layout.prefixes[index] + day.day - 1))
    except ChronologyInvariantError: return _invalid(InvalidReason.OVERFLOW, "civil ordinal overflows")


def civil_to_axis(calendar: PreparedCalendar, day: LocalDay) -> Result[AxisDay]:
    if calendar.epoch_offset is None: return Unavailable(UnavailableReason.NO_EPOCH, "calendar has no shared-axis epoch")
    ordinal = _ordinal(calendar, day)
    if not isinstance(ordinal, Ok): return ordinal
    try: return Ok(AxisDay(_checked_add(ordinal.value, calendar.epoch_offset)))
    except ChronologyInvariantError: return _invalid(InvalidReason.OVERFLOW, "axis day overflows")


def axis_to_civil(calendar: PreparedCalendar, day: AxisDay) -> Result[LocalDay]:
    if calendar.epoch_offset is None: return Unavailable(UnavailableReason.NO_EPOCH, "calendar has no shared-axis epoch")
    try: ordinal = _checked_sub(day.value, calendar.epoch_offset)
    except ChronologyInvariantError: return _invalid(InvalidReason.OVERFLOW, "civil ordinal overflows")
    if calendar.cycle_layouts:
        quotient, residue_day = divmod(ordinal, calendar.cycle_days)
        residue = bisect_right(calendar.cycle_prefixes, residue_day) - 1
        layout = calendar.cycle_layouts[residue]; in_year = residue_day - calendar.cycle_prefixes[residue]
        year = _checked_add(quotient * len(calendar.cycle_layouts), residue)
    else:
        row = bisect_right(calendar.table_prefixes, ordinal) - 1
        if row < 0 or ordinal >= calendar.table_prefixes[row] + calendar.table_layouts[row].days: return Unavailable(UnavailableReason.DISCONNECTED_TABLE, "axis day does not land in a table row")
        layout = calendar.table_layouts[row]; in_year = ordinal - calendar.table_prefixes[row]; year = layout.year
    month_index = bisect_right(layout.prefixes, in_year) - 1
    month = layout.months[month_index]
    return Ok(LocalDay(year, month.number, in_year - layout.prefixes[month_index] + 1))


def normalize_civil(calendar: PreparedCalendar, value: CivilDate) -> Result[ExactAxis | AxisInterval]:
    if value.calendar_id != calendar.definition.id: return _invalid(InvalidReason.DATE, "calendar does not match civil date")
    layout = _year_layout(calendar, value.year)
    if isinstance(layout, Unavailable): return layout
    if value.month is not None and value.day is not None:
        exact = civil_to_axis(calendar, LocalDay(value.year, value.month, value.day))
        return Ok(ExactAxis(exact.value)) if isinstance(exact, Ok) else exact
    if calendar.epoch_offset is None: return Unavailable(UnavailableReason.NO_EPOCH, "calendar has no shared-axis epoch")
    if value.month is None:
        first, last = LocalDay(value.year, layout.months[0].number, 1), LocalDay(value.year, layout.months[-1].number, layout.months[-1].days)
    else:
        item = next((m for m in layout.months if m.number == value.month), None)
        if item is None: return _invalid(InvalidReason.DATE, "invalid month")
        first, last = LocalDay(value.year, item.number, 1), LocalDay(value.year, item.number, item.days)
    low, high = civil_to_axis(calendar, first), civil_to_axis(calendar, last)
    if not isinstance(low, Ok): return low
    if not isinstance(high, Ok): return high
    return Ok(AxisInterval(low.value, high.value))


def _local_interval(calendar: PreparedCalendar, value: CivilDate) -> Result[tuple[LocalDay, LocalDay]]:
    if value.calendar_id != calendar.definition.id:
        return _invalid(InvalidReason.DATE, "calendar does not match civil date")
    layout = _year_layout(calendar, value.year)
    if isinstance(layout, Unavailable):
        return layout
    if value.month is None:
        first = LocalDay(value.year, layout.months[0].number, 1)
        last = LocalDay(value.year, layout.months[-1].number, layout.months[-1].days)
    else:
        month = next((item for item in layout.months if item.number == value.month), None)
        if month is None:
            return _invalid(InvalidReason.DATE, "invalid month")
        if value.day is None:
            first, last = LocalDay(value.year, month.number, 1), LocalDay(value.year, month.number, month.days)
        else:
            if not 1 <= value.day <= month.days:
                return _invalid(InvalidReason.DATE, "invalid civil day")
            first = last = LocalDay(value.year, month.number, value.day)
    low, high = _ordinal(calendar, first), _ordinal(calendar, last)
    if not isinstance(low, Ok): return low
    if not isinstance(high, Ok): return high
    return Ok((first, last))


def _within_era_bounds(era: PreparedEra, value: CivilDate) -> Result[None]:
    interval = _local_interval(era.calendar, value)
    if not isinstance(interval, Ok): return interval
    if era.bounds is None:
        return Ok(None)
    low, high = _ordinal(era.calendar, interval.value[0]), _ordinal(era.calendar, interval.value[1])
    bound_low, bound_high = _ordinal(era.calendar, era.bounds.lower), _ordinal(era.calendar, era.bounds.upper)
    for result in (low, high, bound_low, bound_high):
        if not isinstance(result, Ok): return result
    if low.value < bound_low.value or high.value > bound_high.value:
        return _invalid(InvalidReason.ERA, "era date is outside bounds")
    return Ok(None)


def prepare_era(definition: EraDefinition, calendar: PreparedCalendar) -> Result[PreparedEra]:
    if not isinstance(definition, EraDefinition) or not isinstance(calendar, PreparedCalendar): return _invalid(InvalidReason.ERA, "invalid era definition")
    if definition.calendar_id != calendar.definition.id: return _invalid(InvalidReason.ERA, "era calendar does not match")
    if not isinstance(definition.display_year_zero, bool): return _invalid(InvalidReason.ERA, "display_year_zero must be boolean")
    if not definition.display_year_zero and definition.display_epoch_year == 0: return _invalid(InvalidReason.ERA, "zero-less era cannot have zero epoch")
    try: _definition_id(definition.id, "era_"); _i64(definition.display_epoch_year); _i64(definition.machine_epoch_year)
    except ChronologyInvariantError as exc: return _invalid(InvalidReason.ERA, str(exc))
    bounds: EraBounds | None = None
    if definition.bounds is not None:
        if not isinstance(definition.bounds, EraBounds):
            return _invalid(InvalidReason.ERA, "invalid era bounds")
        low = _ordinal(calendar, definition.bounds.lower)
        high = _ordinal(calendar, definition.bounds.upper)
        if not isinstance(low, Ok) or not isinstance(high, Ok): return _invalid(InvalidReason.ERA, "era bound is not an available local day")
        if low.value > high.value: return _invalid(InvalidReason.ERA, "invalid era bounds")
        bounds = definition.bounds
    return Ok(PreparedEra(definition, calendar, bounds))


def _era_ordinal(era: PreparedEra, display: int) -> Result[int]:
    try:
        _i64(display, "display year")
        if era.definition.display_year_zero: return Ok(display)
        if display == 0: return _invalid(InvalidReason.ERA, "display year zero is disabled")
        return Ok(display if display < 0 else display - 1)
    except ChronologyInvariantError as exc: return _invalid(InvalidReason.OVERFLOW, str(exc))


def era_to_civil(era: PreparedEra, value: EraDate) -> Result[CivilDate]:
    if value.era_id != era.definition.id: return _invalid(InvalidReason.ERA, "era does not match date")
    display, epoch = _era_ordinal(era, value.year), _era_ordinal(era, era.definition.display_epoch_year)
    if not isinstance(display, Ok): return display
    if not isinstance(epoch, Ok): return epoch
    try: machine = _checked_add(era.definition.machine_epoch_year, _checked_sub(display.value, epoch.value))
    except ChronologyInvariantError: return _invalid(InvalidReason.OVERFLOW, "era conversion overflows")
    civil = CivilDate(era.definition.calendar_id, machine, value.month, value.day)
    local = _local_interval(era.calendar, civil)
    if not isinstance(local, Ok): return local
    bounded = _within_era_bounds(era, civil)
    if not isinstance(bounded, Ok): return bounded
    return Ok(civil)


def civil_to_era(era: PreparedEra, value: CivilDate) -> Result[EraDate]:
    if value.calendar_id != era.definition.calendar_id: return _invalid(InvalidReason.ERA, "civil calendar does not match era")
    try:
        if era.definition.display_year_zero:
            display = _checked_add(era.definition.display_epoch_year, _checked_sub(value.year, era.definition.machine_epoch_year))
        else:
            epoch = _era_ordinal(era, era.definition.display_epoch_year)
            if not isinstance(epoch, Ok): return epoch
            ordinal = _checked_add(epoch.value, _checked_sub(value.year, era.definition.machine_epoch_year))
            display = ordinal if ordinal < 0 else _checked_add(ordinal, 1)
        result = EraDate(era.definition.id, display, value.month, value.day)
    except ChronologyInvariantError: return _invalid(InvalidReason.OVERFLOW, "era conversion overflows")
    bounded = _within_era_bounds(era, value)
    if not isinstance(bounded, Ok): return bounded
    return Ok(result)


def normalize_era(catalog: ChronologyCatalog, value: EraDate) -> Result[ExactAxis | AxisInterval]:
    era = catalog.era(value.era_id)
    if era is None: return _invalid(InvalidReason.ERA, "unknown era")
    civil = era_to_civil(era, value)
    if not isinstance(civil, Ok): return civil
    result = normalize_civil(era.calendar, civil.value)
    if not isinstance(result, Ok): return result
    bounded = _within_era_bounds(era, civil.value)
    if not isinstance(bounded, Ok): return bounded
    return result


def normalize_date(catalog: ChronologyCatalog, value: DateValue) -> Result[ExactAxis | AxisInterval | ApproximateAxisInterval]:
    if isinstance(value, CivilDate):
        calendar = catalog.calendar(value.calendar_id)
        return _invalid(InvalidReason.DATE, "unknown calendar") if calendar is None else normalize_civil(calendar, value)
    if isinstance(value, EraDate): return normalize_era(catalog, value)
    if isinstance(value, CivilRange):
        calendar = catalog.calendar(value.calendar_id)
        if calendar is None: return _invalid(InvalidReason.RANGE, "unknown calendar")
        lower = normalize_civil(calendar, value.lower) if value.lower else Ok(None); upper = normalize_civil(calendar, value.upper) if value.upper else Ok(None)
        if not isinstance(lower, Ok): return lower
        if not isinstance(upper, Ok): return upper
        low = lower.value.day if isinstance(lower.value, ExactAxis) else (lower.value.lower if lower.value else None)
        high = upper.value.day if isinstance(upper.value, ExactAxis) else (upper.value.upper if upper.value else None)
        try: return Ok(AxisInterval(low, high))
        except ChronologyInvariantError: return _invalid(InvalidReason.RANGE, "range is reversed")
    if isinstance(value, ApproximateDate):
        bounds = normalize_date(catalog, value.bounds)
        if not isinstance(bounds, Ok): return bounds
        if not isinstance(bounds.value, AxisInterval): return _invalid(InvalidReason.RANGE, "approximation bounds must be interval")
        return Ok(ApproximateAxisInterval(bounds.value))
    if isinstance(value, ConflictingDates): return Unavailable(UnavailableReason.CONFLICTING_CLAIMS, "conflicting claims are not canonical")
    return _invalid(InvalidReason.DATE, "unknown date value")


def fixed_day_difference(left: AxisDay, right: AxisDay) -> Result[int]:
    try: return Ok(_checked_sub(left.value, right.value))
    except ChronologyInvariantError: return _invalid(InvalidReason.OVERFLOW, "fixed-day difference overflows")


def _interval(value: ExactAxis | AxisInterval | ApproximateAxisInterval) -> AxisInterval:
    if isinstance(value, ExactAxis): return AxisInterval(value.day, value.day)
    return value.interval if isinstance(value, ApproximateAxisInterval) else value


def dates_overlap(catalog: ChronologyCatalog, left: DateValue, right: DateValue) -> Result[bool]:
    one, two = normalize_date(catalog, left), normalize_date(catalog, right)
    if not isinstance(one, Ok): return one
    if not isinstance(two, Ok): return two
    a, b = _interval(one.value), _interval(two.value)
    if a.upper is not None and b.lower is not None and a.upper.value < b.lower.value: return Ok(False)
    if b.upper is not None and a.lower is not None and b.upper.value < a.lower.value: return Ok(False)
    return Ok(True)


def compare_dates(catalog: ChronologyCatalog, left: DateValue, right: DateValue) -> Result[TemporalRelation]:
    one, two = normalize_date(catalog, left), normalize_date(catalog, right)
    if not isinstance(one, Ok): return one
    if not isinstance(two, Ok): return two
    if isinstance(one.value, ApproximateAxisInterval) or isinstance(two.value, ApproximateAxisInterval):
        a, b = _interval(one.value), _interval(two.value)
        if (isinstance(one.value, ApproximateAxisInterval) and (a.lower is None or a.upper is None)) or (isinstance(two.value, ApproximateAxisInterval) and (b.lower is None or b.upper is None)):
            return Unavailable(UnavailableReason.APPROXIMATE_ONLY, "qualitative approximation does not establish overlap")
        overlap = dates_overlap(catalog, left, right)
        if isinstance(overlap, Ok) and overlap.value: return Ok(TemporalRelation.OVERLAP)
        return Unavailable(UnavailableReason.APPROXIMATE_ONLY, "approximate dates do not establish order")
    a, b = _interval(one.value), _interval(two.value)
    if a.upper is not None and b.lower is not None and a.upper.value < b.lower.value: return Ok(TemporalRelation.BEFORE)
    if b.upper is not None and a.lower is not None and b.upper.value < a.lower.value: return Ok(TemporalRelation.AFTER)
    if a.lower == a.upper == b.lower == b.upper: return Ok(TemporalRelation.EQUAL)
    return Ok(TemporalRelation.OVERLAP)


def prepare_anchors(anchors: Iterable[Anchor]) -> Result[AnchorIndex]:
    grouped: dict[tuple[int, StoryTime], list[Anchor]] = {}
    seen_ids: set[str] = set()
    try:
        anchor_values: list[object] = []
        for anchor in anchors:
            if len(anchor_values) >= 500:
                return _invalid(InvalidReason.ANCHOR, "too many anchors")
            anchor_values.append(anchor)
    except TypeError:
        return _invalid(InvalidReason.ANCHOR, "invalid anchor iterable")
    for anchor in anchor_values:
        if not isinstance(anchor, Anchor):
            return _invalid(InvalidReason.ANCHOR, "invalid anchor")
        if not isinstance(anchor.axis_day, AxisDay) or not isinstance(anchor.story_time, StoryTime):
            return _invalid(InvalidReason.ANCHOR, "invalid anchor coordinate")
        if not isinstance(anchor.provenance, tuple) or any(not isinstance(item, str) for item in anchor.provenance):
            return _invalid(InvalidReason.ANCHOR, "invalid anchor provenance")
        try: _definition_id(anchor.id, "chronology_")
        except ChronologyInvariantError as exc: return _invalid(InvalidReason.ANCHOR, str(exc))
        if anchor.id in seen_ids:
            return _invalid(InvalidReason.ANCHOR, "duplicate anchor id")
        seen_ids.add(anchor.id)
        grouped.setdefault((anchor.axis_day.value, anchor.story_time), []).append(anchor)
    points: list[AnchorPoint] = []
    coordinate_to_story: dict[int, StoryTime] = {}; story_to_coordinate: dict[StoryTime, int] = {}
    for (axis, story), aliases in grouped.items():
        if axis in coordinate_to_story and coordinate_to_story[axis] != story: return _invalid(InvalidReason.ANCHOR, "same axis day has different StoryTime")
        if story in story_to_coordinate and story_to_coordinate[story] != axis: return _invalid(InvalidReason.ANCHOR, "same StoryTime has different axis day")
        coordinate_to_story[axis] = story; story_to_coordinate[story] = axis
        points.append(AnchorPoint(AxisDay(axis), story, tuple(sorted(a.id for a in aliases)), tuple(item for a in aliases for item in a.provenance)))
    points.sort(key=lambda item: item.axis_day.value)
    for previous, current in zip(points, points[1:]):
        if _story_key(previous.story_time) >= _story_key(current.story_time): return _invalid(InvalidReason.ANCHOR, "anchors cross global StoryTime order")
    return Ok(AnchorIndex(tuple(points)))


def _story_key(story: StoryTime) -> tuple[str, int, int]: return (story.timeline, story.tick, story.order)


def prepare_catalog(calendars: Iterable[CalendarDefinition], eras: Iterable[EraDefinition] = (), anchors: Iterable[Anchor] = ()) -> Result[ChronologyCatalog]:
    def materialize(values: Iterable[object], kind: str) -> tuple[object, ...] | Invalid:
        try:
            result: list[object] = []
            for value in values:
                if len(result) >= 500: return _invalid(InvalidReason.DEFINITION, f"too many {kind}")
                result.append(value)
            return tuple(result)
        except TypeError:
            return _invalid(InvalidReason.DEFINITION, f"invalid {kind} iterable")
    calendar_values, era_values, anchor_values = materialize(calendars, "calendars"), materialize(eras, "eras"), materialize(anchors, "anchors")
    for values in (calendar_values, era_values, anchor_values):
        if isinstance(values, Invalid): return values
    prepared: list[PreparedCalendar] = []
    for definition in calendar_values:
        item = prepare_calendar(definition)
        if not isinstance(item, Ok): return item
        if any(old.definition.id == item.value.definition.id for old in prepared): return _invalid(InvalidReason.DEFINITION, "duplicate calendar id")
        prepared.append(item.value)
    prepared_eras: list[PreparedEra] = []
    for definition in era_values:
        if not isinstance(definition, EraDefinition):
            return _invalid(InvalidReason.ERA, "invalid era definition")
        calendar = next((item for item in prepared if item.definition.id == definition.calendar_id), None)
        if calendar is None: return _invalid(InvalidReason.ERA, "era refers to unknown calendar")
        item = prepare_era(definition, calendar)
        if not isinstance(item, Ok): return item
        if any(old.definition.id == item.value.definition.id for old in prepared_eras): return _invalid(InvalidReason.ERA, "duplicate era id")
        prepared_eras.append(item.value)
    index = prepare_anchors(anchor_values)
    if not isinstance(index, Ok): return index
    return Ok(ChronologyCatalog(tuple(prepared), tuple(prepared_eras), index.value))


def resolve_story_time(index: AnchorIndex, target: AxisDay | AxisInterval) -> NoMatch | Unique | Ambiguous:
    interval = AxisInterval(target, target) if isinstance(target, AxisDay) else target
    values = tuple(sorted((point.story_time for point in index.points if (interval.lower is None or point.axis_day.value >= interval.lower.value) and (interval.upper is None or point.axis_day.value <= interval.upper.value)), key=_story_key))
    if not values: return NoMatch()
    return Unique(values[0]) if len(values) == 1 else Ambiguous(values)


def map_date_to_story_time(catalog: ChronologyCatalog, value: DateValue) -> Result[NoMatch | Unique | Ambiguous]:
    normalized = normalize_date(catalog, value)
    if not isinstance(normalized, Ok): return normalized
    if isinstance(normalized.value, ApproximateAxisInterval): return Unavailable(UnavailableReason.APPROXIMATE_ONLY, "approximate date cannot map to StoryTime")
    return Ok(resolve_story_time(catalog.anchors, normalized.value.day if isinstance(normalized.value, ExactAxis) else normalized.value))


def format_date(catalog: ChronologyCatalog, value: CivilDate | EraDate) -> Result[str]:
    if isinstance(value, CivilDate):
        calendar = catalog.calendar(value.calendar_id)
        if calendar is None: return _invalid(InvalidReason.DATE, "unknown calendar")
        valid = _local_interval(calendar, value)
        if not isinstance(valid, Ok): return valid
        head = f"{value.calendar_id}:{value.year}"
    else:
        era = catalog.era(value.era_id)
        if era is None: return _invalid(InvalidReason.ERA, "unknown era")
        civil = era_to_civil(era, value)
        if not isinstance(civil, Ok): return civil
        valid = _within_era_bounds(era, civil.value)
        if not isinstance(valid, Ok): return valid
        head = f"{value.era_id}:{value.year}"
    if value.month is not None: head += f"-{value.month}"
    if value.day is not None: head += f"-{value.day}"
    return Ok(head)
