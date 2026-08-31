"""Validated, disposable chronology projection used by source and SQLite reads.

The projection keeps authored identifiers and JSON verbatim, but feeds the
kernel its canonical private IDs. This matters because v0.6 accepts existing
lower-case Crockford source IDs while the kernel requires canonical IDs.
"""
from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum
import json
import sqlite3
from typing import Any

from . import CHRONOLOGY_SOURCE_SCHEMA, V07_SOURCE_SCHEMA
from .chronology import (Anchor, ApproximateDate, AxisDay, CalendarDefinition,
    CalendarEpoch, CivilDate, CivilRange, ConflictingDates, CycleOverride,
    CycleRule, EraBounds, EraDate, EraDefinition, IntercalaryMonth, LocalDay,
    Month, MonthDelta, Ok, TableRule, TableYear, _local_interval, _ordinal,
    era_to_civil, normalize_civil, normalize_era, prepare_catalog, Invalid, Unavailable,
    UnavailableReason)
from .chronology_validation import _kernel_id, validate_v06_candidate
from .model import StoryTime, World
from .util import canonical_json


class ComparisonKind(StrEnum):
    EXACT = "exact"
    INTERVAL = "interval"
    APPROXIMATE = "approximate"
    NONCOMPARABLE = "noncomparable"


@dataclass(frozen=True, slots=True)
class ComparableOperand:
    """The one internal comparison representation used by both stores.

    Coordinates are inclusive.  ``axis`` is the shared kernel axis; an
    epoch-less calendar has its own prepared-local basis.  Keeping the source
    kind and openness here prevents the SQLite projection from silently
    turning a qualitative approximation into an ordered date.
    """
    source_kind: str
    basis_id: str
    lower_day: int | None
    upper_day: int | None
    lower_unbounded: bool
    upper_unbounded: bool
    precision: str
    calendar_id: str | None
    era_id: str | None
    approximate: bool = False


@dataclass(frozen=True, slots=True)
class CalendarIndexRow:
    id: str; source_ordinal: int; label: str; basis_id: str; has_epoch: bool; definition_json: str


@dataclass(frozen=True, slots=True)
class EraIndexRow:
    id: str; source_ordinal: int; calendar_id: str; label: str; basis_id: str; definition_json: str


@dataclass(frozen=True, slots=True)
class AnchorIndexRow:
    id: str; source_ordinal: int; axis_day: int; story_time: StoryTime; provenance_json: str


@dataclass(frozen=True, slots=True)
class AnnotationIndexRow:
    record_id: str; record_ordinal: int; annotation_id: str; source_ordinal: int
    role: str | None; display: str | None; provenance_json: str
    value_kind: str; calendar_id: str | None; era_id: str | None; precision: str
    basis_id: str | None; lower_day: int | None; upper_day: int | None
    lower_unbounded: bool; upper_unbounded: bool; comparison_kind: ComparisonKind
    exclusion_reason: str | None; value_json: str
    # Explicit, disposable advisory scope.  These memberships are deliberately
    # not inferred from a nullable display calendar: a conflict can mention
    # several bases and a qualitative form can be genuinely unknown.
    scope_bases: tuple[str, ...] = ()
    scope_eras: tuple[str, ...] = ()
    unknown_basis: bool = False


@dataclass(frozen=True, slots=True)
class ChronologyProjection:
    catalog: Any | None
    calendars: tuple[CalendarIndexRow, ...]
    eras: tuple[EraIndexRow, ...]
    anchors: tuple[AnchorIndexRow, ...]
    annotations: tuple[AnnotationIndexRow, ...]
    calendar_kernel_ids: tuple[tuple[str, str], ...] = ()
    era_kernel_ids: tuple[tuple[str, str], ...] = ()

    def calendar_kernel_id(self, identifier: str) -> str | None:
        return dict(self.calendar_kernel_ids).get(identifier)

    def era_kernel_id(self, identifier: str) -> str | None:
        return dict(self.era_kernel_ids).get(identifier)

    def source_calendar_id(self, identifier: str) -> str | None:
        return next((source for source, kernel in self.calendar_kernel_ids if kernel == identifier), None)

    def source_era_id(self, identifier: str) -> str | None:
        return next((source for source, kernel in self.era_kernel_ids if kernel == identifier), None)


def _change(raw: dict[str, Any]) -> MonthDelta | IntercalaryMonth:
    if "target_month" in raw:
        return MonthDelta(raw["target_month"], raw["delta_days"])
    value = raw["intercalary_month"]
    return IntercalaryMonth(value["number"], value["days"], value.get("label"))


def _calendar(raw: dict[str, Any]) -> CalendarDefinition:
    months = tuple(Month(item["number"], item["days"], item.get("label")) for item in raw["months"])
    rule = raw["rule"]
    if rule["kind"] == "cycle":
        parsed = CycleRule(rule["period"], tuple(CycleOverride(item["residue"], _change(item)) for item in rule.get("overrides", ())))
    else:
        parsed = TableRule(tuple(TableYear(item["year"], tuple(_change(change) for change in item.get("overrides", ()))) for item in rule["years"]))
    epoch = raw.get("epoch")
    prepared_epoch = None if epoch is None else CalendarEpoch(LocalDay(**epoch["civil"]), AxisDay(epoch["axis_day"]))
    return CalendarDefinition(_kernel_id(raw["id"], "calendar_"), raw["label"], months, parsed, prepared_epoch)


def _era(raw: dict[str, Any], calendar_ids: dict[str, str]) -> EraDefinition:
    epoch, bounds = raw["display_epoch"], raw.get("bounds")
    parsed_bounds = None if bounds is None else EraBounds(LocalDay(**bounds["lower"]), LocalDay(**bounds["upper"]))
    return EraDefinition(_kernel_id(raw["id"], "era_"), calendar_ids[raw["calendar_id"]], raw["label"], tuple(raw["aliases"]), raw["display_year_zero"], epoch["display_year"], epoch["machine_year"], parsed_bounds)


def kernelize_date_value(projection: ChronologyProjection, value: Any) -> Any:
    """Map an authored typed value to the catalog's private kernel IDs."""
    if isinstance(value, CivilDate):
        identifier = projection.calendar_kernel_id(value.calendar_id)
        return value if identifier is None else CivilDate(identifier, value.year, value.month, value.day)
    if isinstance(value, EraDate):
        identifier = projection.era_kernel_id(value.era_id)
        return value if identifier is None else EraDate(identifier, value.year, value.month, value.day)
    if isinstance(value, CivilRange):
        identifier = projection.calendar_kernel_id(value.calendar_id)
        return CivilRange(identifier or value.calendar_id, kernelize_date_value(projection, value.lower) if value.lower else None, kernelize_date_value(projection, value.upper) if value.upper else None)
    if isinstance(value, ApproximateDate):
        return ApproximateDate(value.display_value, kernelize_date_value(projection, value.bounds))
    return value


def externalize_date_value(projection: ChronologyProjection, value: Any) -> Any:
    if isinstance(value, CivilDate):
        identifier = projection.source_calendar_id(value.calendar_id)
        return value if identifier is None else CivilDate(identifier, value.year, value.month, value.day)
    if isinstance(value, EraDate):
        identifier = projection.source_era_id(value.era_id)
        return value if identifier is None else EraDate(identifier, value.year, value.month, value.day)
    return value


def _date_value(raw: dict[str, Any], calendar_ids: dict[str, str], era_ids: dict[str, str]) -> Any:
    tag = next(key for key in raw if not key.startswith("x-")); value = raw[tag]
    if tag == "civil":
        return CivilDate(calendar_ids[value["calendar_id"]], value["year"], value.get("month"), value.get("day"))
    if tag == "era":
        return EraDate(era_ids[value["era_id"]], value["year"], value.get("month"), value.get("day"))
    if tag in {"range", "approx"}:
        bounds = value if tag == "range" else value["bounds"]
        # Only the declared endpoints participate in chronology conversion.
        # Extension values are opaque authored JSON and may look like dates,
        # contain temporary-looking identifiers, or be arbitrary scalars.
        source_calendar = bounds.get("calendar_id") or next((item["calendar_id"] for item in (bounds.get("lower"), bounds.get("upper")) if item is not None), None)
        if source_calendar is None:
            return None
        def endpoint(item: dict[str, Any] | None) -> CivilDate | None:
            return None if item is None else CivilDate(calendar_ids[item["calendar_id"]], item["year"], item.get("month"), item.get("day"))
        result = CivilRange(calendar_ids[source_calendar], endpoint(bounds["lower"]), endpoint(bounds["upper"]))
        return ApproximateDate(value["display_value"], result) if tag == "approx" else result
    return None


def describe_date_value(projection: ChronologyProjection, value: Any) -> Ok[ComparableOperand] | Invalid | Unavailable:
    """Normalize a supported authored value without collapsing kernel failures.

    The kernel owns shared-axis arithmetic.  This adapter adds only the
    approved local basis for calendars without an epoch, and otherwise
    returns the kernel's exact ``Invalid``/``Unavailable`` object unchanged.
    """
    catalog = projection.catalog
    if catalog is None:
        return Unavailable(UnavailableReason.NO_EPOCH, "world has no chronology capability")
    if isinstance(value, ConflictingDates):
        return Unavailable(UnavailableReason.CONFLICTING_CLAIMS, "conflicting claims are not canonical")
    if isinstance(value, ApproximateDate):
        inner = describe_date_value(projection, value.bounds)
        if not isinstance(inner, Ok):
            return inner
        return Ok(ComparableOperand("approx", inner.value.basis_id, inner.value.lower_day, inner.value.upper_day,
            inner.value.lower_unbounded, inner.value.upper_unbounded, inner.value.precision, inner.value.calendar_id,
            inner.value.era_id, True))
    if isinstance(value, EraDate):
        era = catalog.era(value.era_id)
        if era is None:
            from .chronology import InvalidReason
            return Invalid(InvalidReason.ERA, "unknown era")
        # Epoch calendars delegate exactly to normalize_era.  For isolated
        # calendars the kernel's era_to_civil owns display-year conversion and
        # inclusive bounds, then the prepared local layout supplies its only
        # legal comparison coordinate.
        if era.calendar.epoch_offset is not None:
            normalized = normalize_era(catalog, value)
            if not isinstance(normalized, Ok):
                return normalized
            coordinate = normalized.value
            lower = coordinate.day.value if hasattr(coordinate, "day") else coordinate.lower.value
            upper = coordinate.day.value if hasattr(coordinate, "day") else coordinate.upper.value
            return Ok(ComparableOperand("era", "axis", lower, upper, False, False,
                "day" if value.day is not None else "month" if value.month is not None else "year",
                projection.source_calendar_id(era.definition.calendar_id) or era.definition.calendar_id,
                projection.source_era_id(value.era_id) or value.era_id))
        civil = era_to_civil(era, value)
        if not isinstance(civil, Ok): return civil
        described = describe_date_value(projection, civil.value)
        if not isinstance(described, Ok):
            return described
        item = described.value
        return Ok(ComparableOperand("era", item.basis_id, item.lower_day, item.upper_day, item.lower_unbounded,
            item.upper_unbounded, item.precision, item.calendar_id, projection.source_era_id(value.era_id) or value.era_id))
    if isinstance(value, CivilRange):
        calendar = catalog.calendar(value.calendar_id)
        if calendar is None:
            from .chronology import InvalidReason
            return Invalid(InvalidReason.RANGE, "unknown calendar")
        if (value.lower is not None and (not isinstance(value.lower, CivilDate) or value.lower.calendar_id != value.calendar_id)) or (value.upper is not None and (not isinstance(value.upper, CivilDate) or value.upper.calendar_id != value.calendar_id)):
            from .chronology import InvalidReason
            return Invalid(InvalidReason.RANGE, "range endpoints must use the declared calendar")
        basis = "axis" if calendar.epoch_offset is not None else f"calendar:{projection.source_calendar_id(value.calendar_id) or value.calendar_id}"
        lower = describe_date_value(projection, value.lower) if value.lower is not None else Ok(None)
        upper = describe_date_value(projection, value.upper) if value.upper is not None else Ok(None)
        if not isinstance(lower, Ok):
            return lower
        if not isinstance(upper, Ok):
            return upper
        if lower.value is not None and lower.value.basis_id != basis:
            from .chronology import InvalidReason
            return Invalid(InvalidReason.RANGE, "range lower calendar does not match")
        if upper.value is not None and upper.value.basis_id != basis:
            from .chronology import InvalidReason
            return Invalid(InvalidReason.RANGE, "range upper calendar does not match")
        low = lower.value.lower_day if lower.value is not None else None
        high = upper.value.upper_day if upper.value is not None else None
        if low is not None and high is not None and low > high:
            from .chronology import InvalidReason
            return Invalid(InvalidReason.RANGE, "range is reversed")
        return Ok(ComparableOperand("range", basis, low, high, value.lower is None, value.upper is None, "range",
            projection.source_calendar_id(value.calendar_id) or value.calendar_id, None))
    if not isinstance(value, CivilDate):
        from .chronology import InvalidReason
        return Invalid(InvalidReason.DATE, "unknown date value")
    calendar = catalog.calendar(value.calendar_id)
    if calendar is None:
        from .chronology import InvalidReason
        return Invalid(InvalidReason.DATE, "unknown calendar")
    source_id = projection.source_calendar_id(value.calendar_id) or value.calendar_id
    if calendar.epoch_offset is not None:
        normalized = normalize_civil(calendar, value)
        if not isinstance(normalized, Ok):
            return normalized
        coordinate = normalized.value
        lower = coordinate.day.value if hasattr(coordinate, "day") else coordinate.lower.value
        upper = coordinate.day.value if hasattr(coordinate, "day") else coordinate.upper.value
    else:
        interval = _local_interval(calendar, value)
        if not isinstance(interval, Ok):
            return interval
        low, high = _ordinal(calendar, interval.value[0]), _ordinal(calendar, interval.value[1])
        if not isinstance(low, Ok):
            return low
        if not isinstance(high, Ok):
            return high
        lower, upper = low.value, high.value
    precision = "day" if value.day is not None else "month" if value.month is not None else "year"
    return Ok(ComparableOperand("civil", "axis" if calendar.epoch_offset is not None else f"calendar:{source_id}", lower, upper,
        False, False, precision, source_id, None))


def bounds_for_value(projection: ChronologyProjection, value: Any) -> tuple[str, int | None, int | None] | None:
    """Compatibility accessor for persisted projection construction only.

    Querying uses :func:`describe_date_value` directly to retain failure
    categories and range openness.
    """
    result = describe_date_value(projection, value)
    return (result.value.basis_id, result.value.lower_day, result.value.upper_day) if isinstance(result, Ok) else None


def _scope_metadata(tagged: dict[str, Any], projection: ChronologyProjection) -> tuple[tuple[str, ...], tuple[str, ...], bool]:
    """Return explicit known-basis/era/unknown memberships for exclusions.

    This intentionally walks the authored tagged value, rather than guessing
    scope from a failed normalization or from serialized JSON at query time.
    """
    bases: set[str] = set(); eras: set[str] = set(); unknown = False
    calendar_ids = dict(projection.calendar_kernel_ids); era_ids = dict(projection.era_kernel_ids)
    def add_date(value: Any) -> None:
        nonlocal unknown
        described = describe_date_value(projection, value)
        if isinstance(described, Ok):
            bases.add(described.value.basis_id)
        else:
            unknown = True
    def walk(value: dict[str, Any]) -> None:
        nonlocal unknown
        tag = next((key for key in value if not key.startswith("x-")), None)
        if tag is None:
            unknown = True; return
        payload = value.get(tag)
        if tag in {"civil", "era", "range", "approx"}:
            if tag == "era" and isinstance(payload, dict) and isinstance(payload.get("era_id"), str):
                eras.add(payload["era_id"])
            parsed = _date_value(value, calendar_ids, era_ids)
            if parsed is None:
                # null/null approximation has no calendar/basis at all.
                unknown = True
            else:
                add_date(parsed)
            return
        if tag == "conflict" and isinstance(payload, dict):
            claims = payload.get("claims", ())
            if not isinstance(claims, list):
                unknown = True; return
            for claim in claims:
                if isinstance(claim, dict): walk(claim)
                else: unknown = True
            return
        # Relative references and display-only durations deliberately have no
        # determinable chronology basis.
        unknown = True
    walk(tagged)
    return tuple(sorted(bases)), tuple(sorted(eras)), unknown


def _annotation(record_id: str, record_ordinal: int, ordinal: int, raw: dict[str, Any], projection: ChronologyProjection) -> AnnotationIndexRow:
    tagged = raw["value"]; kind = next(key for key in tagged if not key.startswith("x-")); payload = tagged[kind]
    calendar_id = era_id = None
    if kind == "civil": calendar_id = payload["calendar_id"]
    elif kind == "era":
        era_id = payload["era_id"]; calendar_id = next(item.calendar_id for item in projection.eras if item.id == era_id)
    elif kind in {"range", "approx"}:
        raw_bounds = payload if kind == "range" else payload["bounds"]
        calendar_id = raw_bounds.get("calendar_id") or next((item["calendar_id"] for item in (raw_bounds.get("lower"), raw_bounds.get("upper")) if item is not None), None)
    value = _date_value(tagged, dict(projection.calendar_kernel_ids), dict(projection.era_kernel_ids))
    described = describe_date_value(projection, value) if value is not None else None
    bounds = (described.value.basis_id, described.value.lower_day, described.value.upper_day) if isinstance(described, Ok) else None
    raw_bounds = payload if kind == "range" else payload.get("bounds", {}) if isinstance(payload, dict) else {}
    lower_unbounded = kind in {"range", "approx"} and raw_bounds.get("lower") is None
    upper_unbounded = kind in {"range", "approx"} and raw_bounds.get("upper") is None
    scope_bases, scope_eras, unknown_basis = _scope_metadata(tagged, projection)
    exclusion_reason = None
    qualitative_approx = kind == "approx" and (raw_bounds.get("lower") is None or raw_bounds.get("upper") is None)
    if kind == "conflict":
        comparison, precision, exclusion_reason = ComparisonKind.NONCOMPARABLE, "none", "conflict"
    elif kind == "relative":
        comparison, precision, exclusion_reason = ComparisonKind.NONCOMPARABLE, "none", "relative"
    elif kind == "duration":
        comparison, precision, exclusion_reason = ComparisonKind.NONCOMPARABLE, "none", "duration"
    elif qualitative_approx:
        comparison, precision, exclusion_reason = ComparisonKind.NONCOMPARABLE, "none", "qualitative-approximation"
    elif bounds is None:
        comparison, precision = ComparisonKind.NONCOMPARABLE, "none"
        # This is persisted disposable-index metadata, rather than inferred
        # from a query's SQL absence.  It gives source and SQLite the same
        # closed advisory classification for qualitative approximations.
        exclusion_reason = "approximate-unbounded" if kind == "approx" else "noncomparable"
    elif kind == "approx": comparison, precision = ComparisonKind.APPROXIMATE, "range"
    elif kind == "range": comparison, precision = ComparisonKind.INTERVAL, "range"
    elif getattr(value, "day", None) is not None: comparison, precision = ComparisonKind.EXACT, "day"
    elif getattr(value, "month", None) is not None: comparison, precision = ComparisonKind.INTERVAL, "month"
    else: comparison, precision = ComparisonKind.INTERVAL, "year"
    return AnnotationIndexRow(record_id, record_ordinal, raw["id"], ordinal, raw.get("role"), raw.get("display"), canonical_json(raw["provenance"]), kind, calendar_id, era_id, precision, bounds[0] if bounds else None, bounds[1] if bounds else None, bounds[2] if bounds else None, lower_unbounded, upper_unbounded, comparison, exclusion_reason, canonical_json(tagged), scope_bases, scope_eras, unknown_basis)


def build_chronology_projection(world: World, *, validated: bool = False) -> ChronologyProjection:
    if world.schema not in {CHRONOLOGY_SOURCE_SCHEMA, V07_SOURCE_SCHEMA} or "chronology" not in world.world_record.frontmatter:
        return ChronologyProjection(None, (), (), (), ())
    if not validated:
        if world.schema == CHRONOLOGY_SOURCE_SCHEMA and validate_v06_candidate(world):
            raise ValueError("chronology projection requires a valid v0.6 world")
        if world.schema == V07_SOURCE_SCHEMA:
            from .validation import validate_world
            if validate_world(world):
                raise ValueError("chronology projection requires a valid v0.7 world")
    declaration = world.world_record.frontmatter["chronology"]
    definitions = tuple(_calendar(item) for item in declaration["calendars"])
    calendar_ids = tuple((item["id"], definition.id) for item, definition in zip(declaration["calendars"], definitions, strict=True))
    eras = tuple(_era(item, dict(calendar_ids)) for item in declaration["eras"])
    era_ids = tuple((item["id"], era.id) for item, era in zip(declaration["eras"], eras, strict=True))
    anchors = tuple(Anchor(_kernel_id(item["id"], "chronology_"), AxisDay(item["axis_day"]), StoryTime.from_value(item["story_time"], world.default_timeline), tuple(item["provenance"])) for item in declaration["anchors"])
    prepared = prepare_catalog(definitions, eras, anchors)
    if not isinstance(prepared, Ok): raise RuntimeError("validated chronology could not be prepared")
    calendar_rows = tuple(CalendarIndexRow(item["id"], index, item["label"], "axis" if definition.epoch is not None else f"calendar:{item['id']}", definition.epoch is not None, canonical_json(item)) for index, (item, definition) in enumerate(zip(declaration["calendars"], definitions, strict=True)))
    era_rows = tuple(EraIndexRow(item["id"], index, item["calendar_id"], item["label"], next(row.basis_id for row in calendar_rows if row.id == item["calendar_id"]), canonical_json(item)) for index, item in enumerate(declaration["eras"]))
    anchor_rows = tuple(AnchorIndexRow(item["id"], index, item["axis_day"], StoryTime.from_value(item["story_time"], world.default_timeline), canonical_json(item["provenance"])) for index, item in enumerate(declaration["anchors"]))
    bare = ChronologyProjection(prepared.value, calendar_rows, era_rows, anchor_rows, (), calendar_ids, era_ids)
    annotations: list[AnnotationIndexRow] = []
    records = sorted((item for item in world.records.values() if item.kind != "world"), key=lambda item: (item.source_path.casefold(), item.id))
    for record_ordinal, record in enumerate(records):
        annotations.extend(_annotation(record.id, record_ordinal, ordinal, raw, bare) for ordinal, raw in enumerate(record.frontmatter.get("chronology") or ()))
    return ChronologyProjection(prepared.value, calendar_rows, era_rows, anchor_rows, tuple(annotations), calendar_ids, era_ids)


def insert_chronology_index(connection: sqlite3.Connection, projection: ChronologyProjection, *, batch_size: int = 1_000) -> dict[str, int]:
    """Persist a projection without creating an unbounded executemany payload."""
    if type(batch_size) is not int or not 1 <= batch_size <= 1_000:
        raise ValueError("chronology batch_size must be an integer in 1..1000")
    batch_count = 0
    maximum_batch = 0
    def insert(sql: str, values: Any) -> None:
        nonlocal batch_count, maximum_batch
        batch: list[Any] = []
        for value in values:
            batch.append(value)
            if len(batch) == batch_size:
                connection.executemany(sql, batch); batch_count += 1; maximum_batch = max(maximum_batch, len(batch)); batch.clear()
        if batch:
            connection.executemany(sql, batch); batch_count += 1; maximum_batch = max(maximum_batch, len(batch))
    connection.execute("INSERT INTO chronology_capability VALUES (?)", (int(projection.catalog is not None),))
    insert("INSERT INTO chronology_calendar VALUES (?,?,?,?,?,?)", ((item.id, item.source_ordinal, item.label, item.basis_id, int(item.has_epoch), item.definition_json) for item in projection.calendars))
    insert("INSERT INTO chronology_era VALUES (?,?,?,?,?,?)", ((item.id, item.source_ordinal, item.calendar_id, item.label, item.basis_id, item.definition_json) for item in projection.eras))
    insert("INSERT INTO chronology_anchor VALUES (?,?,?,?,?,?,?)", ((item.id, item.source_ordinal, item.axis_day, item.story_time.timeline, item.story_time.tick, item.story_time.order, item.provenance_json) for item in projection.anchors))
    insert("INSERT INTO chronology_annotation VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)", ((item.record_id, item.record_ordinal, item.annotation_id, item.source_ordinal, item.role, item.display, item.provenance_json, item.value_kind, item.calendar_id, item.era_id, item.precision, item.basis_id, item.lower_day, item.upper_day, int(item.lower_unbounded), int(item.upper_unbounded), item.comparison_kind.value, item.exclusion_reason, int(item.unknown_basis), item.value_json) for item in projection.annotations))
    insert("INSERT INTO chronology_annotation_basis_scope VALUES (?,?,?)", ((item.record_id, item.annotation_id, basis) for item in projection.annotations for basis in item.scope_bases))
    insert("INSERT INTO chronology_annotation_era_scope VALUES (?,?,?)", ((item.record_id, item.annotation_id, era) for item in projection.annotations for era in item.scope_eras))
    return {"calendarCount": len(projection.calendars), "eraCount": len(projection.eras), "anchorCount": len(projection.anchors), "annotationCount": len(projection.annotations), "chronologyInsertBatches": batch_count, "chronologyMaxInsertBatch": maximum_batch}


def _annotation_row(row: Any) -> AnnotationIndexRow:
    return AnnotationIndexRow(row["record_id"], row["record_ordinal"], row["annotation_id"], row["source_ordinal"], row["role"], row["display"], row["provenance_json"], row["value_kind"], row["calendar_id"], row["era_id"], row["precision"], row["basis_id"], row["lower_day"], row["upper_day"], bool(row["lower_unbounded"]), bool(row["upper_unbounded"]), ComparisonKind(row["comparison_kind"]), row["exclusion_reason"], row["value_json"], (), (), bool(row["unknown_basis"]))


def load_chronology_projection(connection: sqlite3.Connection, *, load_annotations: bool = True) -> ChronologyProjection:
    capability = connection.execute("SELECT available FROM chronology_capability LIMIT 1").fetchone()
    if capability is not None and not bool(capability[0]):
        return ChronologyProjection(None, (), (), (), ())
    calendars = tuple(CalendarIndexRow(row["id"], row["source_ordinal"], row["label"], row["basis_id"], bool(row["has_epoch"]), row["definition_json"]) for row in connection.execute("SELECT * FROM chronology_calendar ORDER BY source_ordinal"))
    eras = tuple(EraIndexRow(row["id"], row["source_ordinal"], row["calendar_id"], row["label"], row["basis_id"], row["definition_json"]) for row in connection.execute("SELECT * FROM chronology_era ORDER BY source_ordinal"))
    anchors = tuple(AnchorIndexRow(row["id"], row["source_ordinal"], row["axis_day"], StoryTime(row["timeline"], row["tick"], row["ordering"]), row["provenance_json"]) for row in connection.execute("SELECT * FROM chronology_anchor ORDER BY source_ordinal"))
    calendar_ids = tuple((item.id, _kernel_id(item.id, "calendar_")) for item in calendars); era_ids = tuple((item.id, _kernel_id(item.id, "era_")) for item in eras)
    definitions = tuple(_calendar(json.loads(item.definition_json)) for item in calendars)
    prepared_eras = tuple(_era(json.loads(item.definition_json), dict(calendar_ids)) for item in eras)
    prepared_anchors = tuple(Anchor(_kernel_id(item.id, "chronology_"), AxisDay(item.axis_day), item.story_time, tuple(json.loads(item.provenance_json))) for item in anchors)
    result = prepare_catalog(definitions, prepared_eras, prepared_anchors)
    if not isinstance(result, Ok): raise RuntimeError("compiled chronology catalog is invalid")
    if load_annotations:
        base_scope: dict[tuple[str, str], list[str]] = {}
        era_scope: dict[tuple[str, str], list[str]] = {}
        for row in connection.execute("SELECT * FROM chronology_annotation_basis_scope"):
            base_scope.setdefault((row["record_id"], row["annotation_id"]), []).append(row["basis_id"])
        for row in connection.execute("SELECT * FROM chronology_annotation_era_scope"):
            era_scope.setdefault((row["record_id"], row["annotation_id"]), []).append(row["era_id"])
        annotations = tuple(
            AnnotationIndexRow(item.record_id, item.record_ordinal, item.annotation_id, item.source_ordinal, item.role, item.display, item.provenance_json, item.value_kind, item.calendar_id, item.era_id, item.precision, item.basis_id, item.lower_day, item.upper_day, item.lower_unbounded, item.upper_unbounded, item.comparison_kind, item.exclusion_reason, item.value_json, tuple(base_scope.get((item.record_id, item.annotation_id), ())), tuple(era_scope.get((item.record_id, item.annotation_id), ())), item.unknown_basis)
            for item in (_annotation_row(row) for row in connection.execute("SELECT * FROM chronology_annotation ORDER BY record_ordinal,source_ordinal,record_id,annotation_id"))
        )
    else:
        annotations = ()
    return ChronologyProjection(result.value, calendars, eras, anchors, annotations, calendar_ids, era_ids)
