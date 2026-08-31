"""Source-bound validation for the active ``wedl/v0.6`` chronology grammar.

The chronology kernel deliberately has no knowledge of source records. This
adapter enables v0.6 source loading, validation, compilation, internal typed
reads, and the public chronology boundary without changing replay semantics or
enabling upgrade migration or UI workflows.
"""
from __future__ import annotations

from copy import deepcopy
from bisect import bisect_right
from dataclasses import dataclass, replace
from hashlib import sha256
from typing import Any

from .chronology import (
    Anchor, ApproximateDate, AxisDay, CalendarDefinition, CalendarEpoch,
    CivilDate, CivilRange, ConflictingDates, CycleOverride, CycleRule,
    EraBounds, EraDate, EraDefinition, IntercalaryMonth, Invalid, LocalDay,
    Month, MonthDelta, Ok, TableRule, TableYear, Unavailable,
    UnavailableReason, PreparedCalendar, format_date, normalize_civil, normalize_date, prepare_catalog,
)
from .model import Record, StoryTime, World
from . import V04_SOURCE_SCHEMA

I64_MIN, I64_MAX = -(2**63), 2**63 - 1
I32_MIN, I32_MAX = -(2**31), 2**31 - 1
_ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"

# This is deliberately a finite wire contract, not merely a convenient list for
# documentation.  Clients key remediation UI on the exact `(code, severity,
# message)` triple, so no call site may leak exception/kernel prose into a
# diagnostic.  Keep the published YAML and Markdown tables in lockstep with it.
PRODUCTION_DIAGNOSTICS = frozenset({
    ("WDL-ANCHOR-001", "error", "anchor IDs must be unique and ascending"), ("WDL-ANCHOR-001", "error", "anchors must be an array of at most 500"), ("WDL-ANCHOR-001", "error", "invalid anchor definition"), ("WDL-ANCHOR-002", "error", "axis day must be signed i64"), ("WDL-ANCHOR-003", "error", "anchor StoryTime must be closed and use a declared timeline"), ("WDL-ANCHOR-004", "error", "anchor provenance must be an array of strings"), ("WDL-ANCHOR-007", "error", "anchors must map monotonically between StoryTime and axis day"),
    ("WDL-CAL-001", "error", "calendar IDs must be unique and ascending"), ("WDL-CAL-001", "error", "calendar must have a valid closed definition"), ("WDL-CAL-002", "error", "calendar months must contain 1..64 rows"), ("WDL-CAL-002", "error", "invalid calendar month"), ("WDL-CAL-002", "error", "month numbers must be unique and ascending"), ("WDL-CAL-003", "error", "calendar rule kind must be cycle or table"), ("WDL-CAL-004", "error", "invalid cycle rule"), ("WDL-CAL-005", "error", "invalid cycle override"), ("WDL-CAL-006", "error", "overrides must be unique and ordered"), ("WDL-CAL-007", "error", "invalid table year row"), ("WDL-CAL-007", "error", "table years must be unique and ascending"), ("WDL-CAL-007", "error", "table years must contain 1..500 rows"), ("WDL-CAL-008", "error", "invalid table override"), ("WDL-CAL-008", "error", "table overrides must be unique and ordered"), ("WDL-CAL-009", "error", "calendar mechanics exceed 500 rows"), ("WDL-CAL-010", "error", "invalid calendar epoch"), ("WDL-CAL-011", "error", "calendar definition is semantically invalid"),
    ("WDL-CHRON-001", "error", "chronology calendars must be an array of at most 500"), ("WDL-CHRON-001", "error", "world chronology must be a closed declaration"), ("WDL-CHRON-002", "error", "non-world chronology must be an array of at most 500"), ("WDL-CHRON-003", "error", "chronology is forbidden in nested operational objects"), ("WDL-CHRON-004", "error", "invalid chronology annotation envelope"), ("WDL-CHRON-005", "error", "annotation IDs must be unique per record"), ("WDL-CHRON-007", "error", "annotation value must contain exactly one tag"), ("WDL-CHRON-007", "error", "unknown annotation value tag"),
    ("WDL-DATE-001", "error", "civil date must have a known calendar and valid precision"), ("WDL-DATE-001", "error", "civil endpoint must be exact"), ("WDL-DATE-001", "error", "invalid civil precision"), ("WDL-DATE-002", "error", "calendar date is unavailable for the declared year"), ("WDL-DATE-003", "error", "civil date is semantically invalid"), ("WDL-DATE-003", "error", "invalid civil range"), ("WDL-DATE-003", "error", "range endpoint calendar differs"), ("WDL-DATE-003", "error", "range is reversed"), ("WDL-DATE-004", "error", "approximation bounds must share an ordered calendar"), ("WDL-DATE-004", "error", "invalid approximation"), ("WDL-DATE-005", "error", "conflict requires 2..64 claims within depth 64"), ("WDL-DATE-006", "error", "relative date must use declared source-record references"), ("WDL-DATE-007", "error", "invalid display-only duration"),
    ("WDL-ERA-001", "error", "era IDs must be unique and ascending"), ("WDL-ERA-001", "error", "era must have a valid closed definition"), ("WDL-ERA-001", "error", "eras must be an array of at most 500"), ("WDL-ERA-002", "error", "era calendar is unknown"), ("WDL-ERA-003", "error", "invalid era display metadata"), ("WDL-ERA-004", "error", "invalid era display epoch"), ("WDL-ERA-005", "error", "era bounds must be exact civil endpoints"), ("WDL-ERA-005", "error", "era definition is semantically invalid"), ("WDL-ERA-005", "error", "invalid era bounds"), ("WDL-ERA-006", "error", "invalid era annotation"), ("WDL-ERA-006", "error", "invalid era precision"), ("WDL-ERA-008", "error", "era date is semantically invalid"),
    ("WDL-SRC-001", "error", "v0.6 candidate must be homogeneous"), ("WDL-SRC-008", "error", "v0.6 candidate must be homogeneous"), ("WDL-WORLD-001", "error", "world must contain exactly one world record"),
})


def _error(code: str, message: str, record: Record | None, field: str | None) -> dict[str, Any]:
    if (code, "error", message) not in PRODUCTION_DIAGNOSTICS:
        raise AssertionError(f"unregistered chronology diagnostic: {(code, 'error', message)!r}")
    return {"code": code, "message": message, "severity": "error", "entityId": record.id if record else None,
            "path": record.source_path if record else None, "field": field}


def _i64(value: Any) -> bool:
    return type(value) is int and I64_MIN <= value <= I64_MAX


def _i32(value: Any) -> bool:
    return type(value) is int and I32_MIN <= value <= I32_MAX


def _keys(value: Any, required: set[str], optional: set[str] = set()) -> bool:
    return isinstance(value, dict) and all(isinstance(key, str) for key in value) and required <= set(value) and all(key in required | optional or key.startswith("x-") for key in value)


def _source_id(value: Any, prefix: str) -> bool:
    if not isinstance(value, str) or not value.startswith(prefix): return False
    suffix = value[len(prefix):]
    return len(suffix) == 26 and all(char.upper() in _ALPHABET for char in suffix)


def _kernel_id(value: str, prefix: str) -> str:
    """Give legacy authored IDs a private, deterministic kernel identity."""
    suffix = value[len(prefix):].upper()
    if len(suffix) == 26 and all(char in _ALPHABET for char in suffix):
        return prefix + suffix
    number = int.from_bytes(sha256(value.encode("utf-8")).digest(), "big")
    chars = []
    for _ in range(26):
        chars.append(_ALPHABET[number & 31]); number >>= 5
    return prefix + "".join(reversed(chars))


@dataclass
class _Prepared:
    calendar_ids: dict[str, str]
    era_ids: dict[str, str]
    catalog: Any | None
    calendar_definitions: dict[str, PreparedCalendar]
    era_definitions: dict[str, EraDefinition]


def _civil(value: Any, calendars: dict[str, str], record: Record, field: str, errors: list[dict[str, Any]], *, exact: bool = False) -> CivilDate | None:
    if not isinstance(value, dict) or not _keys(value, {"calendar_id", "year"}, {"month", "day"}):
        errors.append(_error("WDL-DATE-001", "civil date must have a known calendar and valid precision", record, field)); return None
    if not _source_id(value.get("calendar_id"), "calendar_") or value["calendar_id"] not in calendars:
        errors.append(_error("WDL-DATE-001", "civil date must have a known calendar and valid precision", record, f"{field}.calendar_id")); return None
    if not _i64(value.get("year")):
        errors.append(_error("WDL-DATE-001", "civil date must have a known calendar and valid precision", record, f"{field}.year")); return None
    if "month" in value and not _i64(value["month"]):
        errors.append(_error("WDL-DATE-001", "civil date must have a known calendar and valid precision", record, f"{field}.month")); return None
    if "day" in value and not _i64(value["day"]):
        errors.append(_error("WDL-DATE-001", "civil date must have a known calendar and valid precision", record, f"{field}.day")); return None
    if "day" in value and "month" not in value:
        errors.append(_error("WDL-DATE-001", "invalid civil precision", record, f"{field}.month")); return None
    if exact and set(k for k in value if not k.startswith("x-")) != {"calendar_id", "year", "month", "day"}:
        errors.append(_error("WDL-DATE-001", "civil endpoint must be exact", record, field)); return None
    try:
        return CivilDate(calendars[value["calendar_id"]], value["year"], value.get("month"), value.get("day"))
    except ValueError:
        errors.append(_error("WDL-DATE-001", "invalid civil precision", record, field)); return None


def _change(value: Any, *, cycle: bool) -> tuple[int | None, MonthDelta | IntercalaryMonth] | None:
    required = {"residue"} if cycle else set()
    if not isinstance(value, dict) or not required <= set(value): return None
    residue = value.get("residue") if cycle else None
    if cycle and not _i64(residue): return None
    tags = [key for key in ("target_month", "intercalary_month") if key in value]
    if len(tags) != 1: return None
    if tags[0] == "target_month":
        allowed = required | {"target_month", "delta_days"}
        if not _keys(value, allowed) or not _i64(value.get("target_month")) or not _i64(value.get("delta_days")): return None
        return residue, MonthDelta(value["target_month"], value["delta_days"])
    allowed = required | {"intercalary_month"}
    item = value["intercalary_month"]
    if not _keys(value, allowed) or not _keys(item, {"number", "days"}, {"label"}) or not _i64(item.get("number")) or not _i64(item.get("days")) or ("label" in item and not isinstance(item["label"], str)): return None
    return residue, IntercalaryMonth(item["number"], item["days"], item.get("label"))


def _change_leaf(value: Any, *, cycle: bool) -> str:
    """Return the malformed authored leaf, never a reconstructed kernel path."""
    if not isinstance(value, dict): return ""
    if cycle and not _i64(value.get("residue")): return ".residue"
    tags = [key for key in ("target_month", "intercalary_month") if key in value]
    if len(tags) != 1: return ""
    if tags[0] == "target_month":
        if not _i64(value.get("target_month")): return ".target_month"
        if not _i64(value.get("delta_days")): return ".delta_days"
        return ""
    item = value.get("intercalary_month")
    if not isinstance(item, dict): return ".intercalary_month"
    if not _i64(item.get("number")): return ".intercalary_month.number"
    if not _i64(item.get("days")): return ".intercalary_month.days"
    if "label" in item and not isinstance(item["label"], str): return ".intercalary_month.label"
    return ""


def _calendars(world: Record, chronology: dict[str, Any], errors: list[dict[str, Any]]) -> tuple[list[CalendarDefinition], dict[str, str], bool]:
    values = chronology.get("calendars")
    if not isinstance(values, list) or len(values) > 500:
        errors.append(_error("WDL-CHRON-001", "chronology calendars must be an array of at most 500", world, "chronology.calendars")); return [], {}, False
    definitions: list[CalendarDefinition] = []; ids: dict[str, str] = {}; canonical_ids: set[str] = set(); previous = ""
    for i, value in enumerate(values):
        field = f"chronology.calendars[{i}]"
        if not _keys(value, {"id", "label", "rule", "months", "epoch"}) or not _source_id(value.get("id"), "calendar_") or not isinstance(value.get("label"), str):
            errors.append(_error("WDL-CAL-001", "calendar must have a valid closed definition", world, field)); continue
        source_id = value["id"]
        if source_id in ids or source_id <= previous:
            errors.append(_error("WDL-CAL-001", "calendar IDs must be unique and ascending", world, f"{field}.id")); continue
        canonical_id = _kernel_id(source_id, "calendar_")
        if canonical_id in canonical_ids:
            errors.append(_error("WDL-CAL-001", "calendar IDs must be unique and ascending", world, f"{field}.id")); continue
        previous = source_id; ids[source_id] = canonical_id; canonical_ids.add(canonical_id)
        months_value = value["months"]
        if not isinstance(months_value, list) or not 1 <= len(months_value) <= 64:
            errors.append(_error("WDL-CAL-002", "calendar months must contain 1..64 rows", world, f"{field}.months")); continue
        months: list[Month] = []; numbers: list[int] = []
        for j, item in enumerate(months_value):
            if not _keys(item, {"number", "days"}, {"label"}) or not _i64(item.get("number")) or not 1 <= item["number"] <= 64 or not _i64(item.get("days")) or not 1 <= item["days"] <= 4096 or ("label" in item and not isinstance(item["label"], str)):
                leaf = ""
                if isinstance(item, dict):
                    if not _i64(item.get("number")) or not 1 <= item.get("number", 0) <= 64: leaf = ".number"
                    elif not _i64(item.get("days")) or not 1 <= item.get("days", 0) <= 4096: leaf = ".days"
                    elif "label" in item and not isinstance(item["label"], str): leaf = ".label"
                errors.append(_error("WDL-CAL-002", "invalid calendar month", world, f"{field}.months[{j}]{leaf}")); continue
            numbers.append(item["number"]); months.append(Month(item["number"], item["days"], item.get("label")))
        if numbers != sorted(set(numbers)):
            errors.append(_error("WDL-CAL-002", "month numbers must be unique and ascending", world, f"{field}.months")); continue
        rule = value["rule"]
        built: CycleRule | TableRule | None = None; mechanics = len(months)
        if not isinstance(rule, dict) or rule.get("kind") not in {"cycle", "table"}:
            errors.append(_error("WDL-CAL-003", "calendar rule kind must be cycle or table", world, f"{field}.rule")); continue
        if rule["kind"] == "cycle":
            if not _keys(rule, {"kind", "period", "overrides"}) or not _i64(rule.get("period")) or not 1 <= rule["period"] <= 500 or not isinstance(rule.get("overrides"), list):
                leaf = ".period" if isinstance(rule, dict) and (not _i64(rule.get("period")) or not 1 <= rule.get("period", 0) <= 500) else ".overrides" if isinstance(rule, dict) and not isinstance(rule.get("overrides"), list) else ""
                errors.append(_error("WDL-CAL-004", "invalid cycle rule", world, f"{field}.rule{leaf}")); continue
            changes: list[CycleOverride] = []; order: list[tuple[int, int]] = []
            for j, item in enumerate(rule["overrides"]):
                parsed = _change(item, cycle=True)
                if parsed is None or not 0 <= parsed[0] < rule["period"]:
                    leaf = _change_leaf(item, cycle=True) if parsed is None else ".residue"
                    errors.append(_error("WDL-CAL-005", "invalid cycle override", world, f"{field}.rule.overrides[{j}]{leaf}")); continue
                key = (parsed[0], parsed[1].target_month if isinstance(parsed[1], MonthDelta) else parsed[1].number)
                order.append(key); changes.append(CycleOverride(parsed[0], parsed[1]))
            if order != sorted(set(order)):
                errors.append(_error("WDL-CAL-006", "overrides must be unique and ordered", world, f"{field}.rule.overrides")); continue
            mechanics += len(changes); built = CycleRule(rule["period"], tuple(changes))
        else:
            years = rule.get("years")
            # Empty/oversized table definitions intentionally get this one error only.
            if not _keys(rule, {"kind", "years"}) or not isinstance(years, list) or not 1 <= len(years) <= 500:
                errors.append(_error("WDL-CAL-007", "table years must contain 1..500 rows", world, f"{field}.rule.years")); continue
            rows: list[TableYear] = []; keys: list[int] = []; malformed = False
            for j, item in enumerate(years):
                if not _keys(item, {"year", "overrides"}) or not _i64(item.get("year")) or not isinstance(item.get("overrides"), list):
                    leaf = ".year" if isinstance(item, dict) and not _i64(item.get("year")) else ".overrides" if isinstance(item, dict) and not isinstance(item.get("overrides"), list) else ""
                    errors.append(_error("WDL-CAL-007", "invalid table year row", world, f"{field}.rule.years[{j}]{leaf}")); malformed = True; continue
                changes: list[MonthDelta | IntercalaryMonth] = []; order: list[int] = []
                for k, change in enumerate(item["overrides"]):
                    parsed = _change(change, cycle=False)
                    if parsed is None:
                        leaf = _change_leaf(change, cycle=False)
                        errors.append(_error("WDL-CAL-008", "invalid table override", world, f"{field}.rule.years[{j}].overrides[{k}]{leaf}")); malformed = True; continue
                    target = parsed[1].target_month if isinstance(parsed[1], MonthDelta) else parsed[1].number
                    order.append(target); changes.append(parsed[1])
                if order != sorted(set(order)):
                    errors.append(_error("WDL-CAL-008", "table overrides must be unique and ordered", world, f"{field}.rule.years[{j}].overrides")); malformed = True
                keys.append(item["year"]); rows.append(TableYear(item["year"], tuple(changes))); mechanics += 1 + len(changes)
            if malformed or keys != sorted(set(keys)):
                if not malformed: errors.append(_error("WDL-CAL-007", "table years must be unique and ascending", world, f"{field}.rule.years[{next(i for i in range(1, len(keys)) if keys[i] <= keys[i-1])}].year"))
                continue
            built = TableRule(tuple(rows))
        if mechanics > 500:
            errors.append(_error("WDL-CAL-009", "calendar mechanics exceed 500 rows", world, f"{field}.rule")); continue
        epoch_value = value["epoch"]; epoch = None
        if epoch_value is not None:
            if not _keys(epoch_value, {"civil", "axis_day"}):
                errors.append(_error("WDL-CAL-010", "invalid calendar epoch", world, f"{field}.epoch")); continue
            civil = epoch_value.get("civil")
            if not _keys(civil, {"year", "month", "day"}):
                leaf = ".civil"
                if isinstance(civil, dict):
                    unexpected = next((key for key in civil if key not in {"year", "month", "day"} and not key.startswith("x-")), None)
                    leaf += f".{unexpected}" if unexpected is not None else ".year" if "year" not in civil else ".month" if "month" not in civil else ".day" if "day" not in civil else ""
                errors.append(_error("WDL-CAL-010", "invalid calendar epoch", world, f"{field}.epoch{leaf}")); continue
            if not _i64(civil.get("year")):
                errors.append(_error("WDL-CAL-010", "invalid calendar epoch", world, f"{field}.epoch.civil.year")); continue
            if not _i64(civil.get("month")):
                errors.append(_error("WDL-CAL-010", "invalid calendar epoch", world, f"{field}.epoch.civil.month")); continue
            if not _i64(civil.get("day")):
                errors.append(_error("WDL-CAL-010", "invalid calendar epoch", world, f"{field}.epoch.civil.day")); continue
            if not _i64(epoch_value.get("axis_day")):
                errors.append(_error("WDL-CAL-010", "invalid calendar epoch", world, f"{field}.epoch.axis_day")); continue
            try: epoch = CalendarEpoch(LocalDay(epoch_value["civil"]["year"], epoch_value["civil"]["month"], epoch_value["civil"]["day"]), AxisDay(epoch_value["axis_day"]))
            except (TypeError, ValueError): errors.append(_error("WDL-CAL-010", "invalid calendar epoch", world, f"{field}.epoch.civil")); continue
        definitions.append(CalendarDefinition(ids[source_id], value["label"], tuple(months), built, epoch))
    return definitions, ids, not errors


def _eras(owner: Record, chronology: dict[str, Any], calendars: dict[str, str], definitions: dict[str, PreparedCalendar], errors: list[dict[str, Any]]) -> tuple[list[EraDefinition], dict[str, str]]:
    values = chronology.get("eras")
    if not isinstance(values, list) or len(values) > 500:
        errors.append(_error("WDL-ERA-001", "eras must be an array of at most 500", owner, "chronology.eras")); return [], {}
    result: list[EraDefinition] = []; ids: dict[str, str] = {}; canonical_ids: set[str] = set(); previous = ""
    for i, value in enumerate(values):
        field = f"chronology.eras[{i}]"
        if not _keys(value, {"id", "calendar_id", "label", "aliases", "display_year_zero", "display_epoch", "provenance"}, {"bounds"}) or not _source_id(value.get("id"), "era_"):
            errors.append(_error("WDL-ERA-001", "era must have a valid closed definition", owner, field)); continue
        source_id = value["id"]
        if source_id in ids or source_id <= previous:
            errors.append(_error("WDL-ERA-001", "era IDs must be unique and ascending", owner, f"{field}.id")); continue
        canonical_id = _kernel_id(source_id, "era_")
        if canonical_id in canonical_ids:
            errors.append(_error("WDL-ERA-001", "era IDs must be unique and ascending", owner, f"{field}.id")); continue
        previous = source_id; ids[source_id] = canonical_id; canonical_ids.add(canonical_id)
        if value.get("calendar_id") not in calendars:
            errors.append(_error("WDL-ERA-002", "era calendar is unknown", owner, f"{field}.calendar_id")); continue
        if not isinstance(value.get("label"), str) or not isinstance(value.get("aliases"), list) or not all(isinstance(item, str) for item in value["aliases"]) or not isinstance(value.get("provenance"), list) or not value["provenance"] or not all(isinstance(item, str) and item for item in value["provenance"]) or type(value.get("display_year_zero")) is not bool:
            errors.append(_error("WDL-ERA-003", "invalid era display metadata", owner, field)); continue
        epoch = value.get("display_epoch")
        if not _keys(epoch, {"display_year", "machine_year"}) or not _i64(epoch.get("display_year")) or not _i64(epoch.get("machine_year")) or (not value["display_year_zero"] and epoch["display_year"] == 0):
            errors.append(_error("WDL-ERA-004", "invalid era display epoch", owner, f"{field}.{_era_display_leaf(value)}")); continue
        bounds = None
        if "bounds" in value:
            raw = value["bounds"]
            if not _keys(raw, {"lower", "upper"}) or not _keys(raw.get("lower"), {"year", "month", "day"}) or not _keys(raw.get("upper"), {"year", "month", "day"}):
                errors.append(_error("WDL-ERA-005", "era bounds must be exact civil endpoints", owner, f"{field}.{_era_bounds_leaf(value)}")); continue
            malformed_leaf = next((f"bounds.{side}.{key}" for side in ("lower", "upper") for key in ("year", "month", "day") if not _i64(raw[side].get(key))), None)
            if malformed_leaf is not None:
                errors.append(_error("WDL-ERA-005", "invalid era bounds", owner, f"{field}.{malformed_leaf}")); continue
            endpoints = {
                side: CivilDate(calendars[value["calendar_id"]], raw[side]["year"], raw[side]["month"], raw[side]["day"])
                for side in ("lower", "upper")
            }
            # Bounds are independently executable civil endpoints.  Validate
            # them before comparing their tuple order, so a malformed lower
            # coordinate can never be obscured by an otherwise reversed pair.
            calendar = definitions[calendars[value["calendar_id"]]]
            endpoint_error = next(((side, _local_ordinal_endpoint_leaf(calendar, endpoints[side])) for side in ("lower", "upper") if _local_ordinal_endpoint_leaf(calendar, endpoints[side]) is not None), None)
            if endpoint_error is not None:
                side, leaf = endpoint_error
                errors.append(_error("WDL-ERA-005", "invalid era bounds", owner, f"{field}.bounds.{side}.{leaf}")); continue
            if _civil_key(endpoints["lower"], upper=False) > _civil_key(endpoints["upper"], upper=True):
                errors.append(_error("WDL-ERA-005", "invalid era bounds", owner, f"{field}.bounds.lower.year")); continue
            try: bounds = EraBounds(LocalDay(raw["lower"]["year"], raw["lower"]["month"], raw["lower"]["day"]), LocalDay(raw["upper"]["year"], raw["upper"]["month"], raw["upper"]["day"]))
            except (TypeError, ValueError): errors.append(_error("WDL-ERA-005", "invalid era bounds", owner, f"{field}.{_era_bounds_leaf(value)}")); continue
        result.append(EraDefinition(ids[source_id], calendars[value["calendar_id"]], value["label"], tuple(value["aliases"]), value["display_year_zero"], epoch["display_year"], epoch["machine_year"], bounds))
    return result, ids


def _anchors(owner: Record, chronology: dict[str, Any], errors: list[dict[str, Any]]) -> list[Anchor]:
    values = chronology.get("anchors")
    timelines = {item.get("id") if isinstance(item, dict) else item for item in owner.frontmatter.get("timelines", [])}
    if not isinstance(values, list) or len(values) > 500:
        errors.append(_error("WDL-ANCHOR-001", "anchors must be an array of at most 500", owner, "chronology.anchors")); return []
    result: list[Anchor] = []; ids: set[str] = set(); canonical_ids: set[str] = set(); previous = ""
    for i, value in enumerate(values):
        field = f"chronology.anchors[{i}]"
        if not _keys(value, {"id", "axis_day", "story_time", "provenance"}) or not _source_id(value.get("id"), "chronology_"):
            errors.append(_error("WDL-ANCHOR-001", "invalid anchor definition", owner, field)); continue
        if value["id"] in ids or value["id"] <= previous:
            errors.append(_error("WDL-ANCHOR-001", "anchor IDs must be unique and ascending", owner, f"{field}.id")); continue
        canonical_id = _kernel_id(value["id"], "chronology_")
        if canonical_id in canonical_ids:
            errors.append(_error("WDL-ANCHOR-001", "anchor IDs must be unique and ascending", owner, f"{field}.id")); continue
        ids.add(value["id"]); canonical_ids.add(canonical_id); previous = value["id"]
        story = value.get("story_time")
        if not _i64(value.get("axis_day")):
            errors.append(_error("WDL-ANCHOR-002", "axis day must be signed i64", owner, f"{field}.axis_day")); continue
        if not _keys(story, {"timeline", "tick", "order"}) or story.get("timeline") not in timelines or not _i64(story.get("tick")) or not _i32(story.get("order")):
            errors.append(_error("WDL-ANCHOR-003", "anchor StoryTime must be closed and use a declared timeline", owner, f"{field}.story_time")); continue
        if not isinstance(value.get("provenance"), list) or not value["provenance"] or not all(isinstance(item, str) and item for item in value["provenance"]):
            errors.append(_error("WDL-ANCHOR-004", "anchor provenance must be an array of strings", owner, f"{field}.provenance")); continue
        result.append(Anchor(canonical_id, AxisDay(value["axis_day"]), StoryTime(story["timeline"], story["tick"], story["order"]), tuple(value["provenance"])))
    return result


def _layout_for_year(calendar: PreparedCalendar, year: int):
    """Select exactly the signed-year layout prepared by the kernel."""
    if calendar.cycle_layouts:
        return calendar.cycle_layouts[year % len(calendar.cycle_layouts)]
    index = bisect_right(calendar.table_years, year) - 1
    return None if index < 0 or calendar.table_years[index] != year else calendar.table_layouts[index]


def _checked(left: int, right: int) -> int | None:
    value = left + right
    return value if I64_MIN <= value <= I64_MAX else None


def _ordinal_leaf(calendar: PreparedCalendar, day: LocalDay) -> str | None:
    """Mirror the kernel ordinal stages and retain the authored operand leaf."""
    layout = _layout_for_year(calendar, day.year)
    if layout is None:
        return "year"
    index = next((i for i, month in enumerate(layout.months) if month.number == day.month), None)
    if index is None:
        return "month"
    if not 1 <= day.day <= layout.months[index].days:
        return "day"
    if calendar.cycle_layouts:
        quotient, residue = divmod(day.year, len(calendar.cycle_layouts))
        # This is deliberately one checked sum.  Python may temporarily hold a
        # product outside i64 which becomes representable once the residue
        # prefix is added; rejecting the product alone would diverge from the
        # kernel's `_checked_add(quotient * cycle_days, prefix)` behavior.
        start = _checked(quotient * calendar.cycle_days, calendar.cycle_prefixes[residue])
        if start is None:
            return "year"
    else:
        row = bisect_right(calendar.table_years, day.year) - 1
        start = calendar.table_prefixes[row]
    # Match the kernel's final checked addition exactly.  The selected month
    # and day have already been structurally validated, so an overflow belongs
    # to the final authored local-day coordinate.
    return None if _checked(start, layout.prefixes[index] + day.day - 1) is not None else "day"


def _civil_leaf(calendar: PreparedCalendar, value: CivilDate) -> str | None:
    layout = _layout_for_year(calendar, value.year)
    if layout is None:
        return "year"
    if value.month is None:
        return None
    month = next((item for item in layout.months if item.number == value.month), None)
    if month is None:
        return "month"
    if value.day is not None and not 1 <= value.day <= month.days:
        return "day"
    return None


def _civil_endpoint_leaf(calendar: PreparedCalendar, value: CivilDate) -> str | None:
    """Fully normalize one authored civil endpoint without requiring an epoch.

    Partial coordinates have two local endpoints.  Both must survive checked
    ordinal and shared-axis arithmetic before a containing range may be
    ordered.  An isolated calendar deliberately remains valid: ``NO_EPOCH``
    says it cannot map to the shared axis, not that its civil date is malformed.
    """
    initial = _civil_leaf(calendar, value)
    if initial is not None:
        return initial
    layout = _layout_for_year(calendar, value.year)
    assert layout is not None
    if value.month is None:
        endpoints = (LocalDay(value.year, layout.months[0].number, 1), LocalDay(value.year, layout.months[-1].number, layout.months[-1].days))
    elif value.day is None:
        month = next(item for item in layout.months if item.number == value.month)
        endpoints = (LocalDay(value.year, month.number, 1), LocalDay(value.year, month.number, month.days))
    else:
        endpoints = (LocalDay(value.year, value.month, value.day),)
    for endpoint in endpoints:
        leaf = _ordinal_leaf(calendar, endpoint)
        if leaf is not None:
            return leaf if value.day is not None else ("month" if value.month is not None else "year")
    normalized = normalize_civil(calendar, value)
    if isinstance(normalized, Invalid):
        # The complete selected coordinate is locally valid; the kernel can now
        # only have rejected the shared-axis addition.  Keep that failure on the
        # authored precision, and never reinterpret an isolated calendar as bad.
        return "day" if value.day is not None else ("month" if value.month is not None else "year")
    return None


def _local_ordinal_endpoint_leaf(calendar: PreparedCalendar, value: CivilDate) -> str | None:
    """Validate an exact definition endpoint without asking for a shared axis.

    Era bounds constrain a calendar's local civil ordinal. They intentionally do
    not require a calendar epoch and must not inherit an unrelated epoch-offset
    overflow from an otherwise usable calendar.
    """
    return _ordinal_leaf(calendar, LocalDay(value.year, value.month, value.day))


def _safe_civil_endpoint_leaf(calendar: PreparedCalendar, value: CivilDate) -> str | None:
    """Do not let a separately-invalid definition mask its own diagnostic."""
    try:
        return _civil_endpoint_leaf(calendar, value)
    except (KeyError, TypeError, ValueError):
        return None


def _calendar_kernel_leaf(raw: dict[str, Any], definition: CalendarDefinition, calendar: PreparedCalendar | None, *, epoch: bool) -> str:
    """Replay a kernel rejection onto one authored definition leaf.

    This runs only after the kernel has rejected the already-built definition.
    It does not replace semantic validation; it selects the member that supplies
    the rejected operand so diagnostics never collapse to a parent object.
    """
    base_days = {month.number: month.days for month in definition.months}
    rule = raw["rule"]
    changes: list[tuple[str, MonthDelta | IntercalaryMonth]] = []
    if rule["kind"] == "cycle":
        for index, change in enumerate(definition.rule.overrides):
            changes.append((f"rule.overrides[{index}]", change.change))
    else:
        for row_index, row in enumerate(definition.rule.years):
            for change_index, change in enumerate(row.overrides):
                changes.append((f"rule.years[{row_index}].overrides[{change_index}]", change))
    for field, change in changes:
        if isinstance(change, MonthDelta):
            if change.target_month not in base_days: return f"{field}.target_month"
            if not 1 <= base_days[change.target_month] + change.delta_days <= 4096: return f"{field}.delta_days"
        else:
            if change.number in base_days: return f"{field}.intercalary_month.number"
            if not 1 <= change.days <= 4096: return f"{field}.intercalary_month.days"
    if not epoch: return "rule"
    civil = raw["epoch"]["civil"]
    if calendar is None:
        return "rule"
    leaf = _ordinal_leaf(calendar, LocalDay(civil["year"], civil["month"], civil["day"]))
    if leaf is not None:
        return f"epoch.civil.{leaf}"
    # All remaining epoch failures are mapping/offset failures, owned by the
    # supplied shared-axis coordinate rather than an inferred civil member.
    return "epoch.axis_day"


def _era_display_leaf(raw: dict[str, Any]) -> str:
    epoch = raw.get("display_epoch")
    if not isinstance(epoch, dict): return "display_epoch"
    if not _i64(epoch.get("display_year")) or (not raw.get("display_year_zero") and epoch.get("display_year") == 0): return "display_epoch.display_year"
    return "display_epoch.machine_year"


def _era_bounds_leaf(raw: dict[str, Any], calendar: PreparedCalendar | None = None) -> str:
    bounds = raw.get("bounds")
    if not isinstance(bounds, dict): return "bounds"
    for side in ("lower", "upper"):
        endpoint = bounds.get(side)
        if not isinstance(endpoint, dict): return f"bounds.{side}"
        extra = next((key for key in endpoint if key not in {"year", "month", "day"} and not str(key).startswith("x-")), None)
        if extra is not None: return f"bounds.{side}.{extra}"
        for key in ("year", "month", "day"):
            if not _i64(endpoint.get(key)): return f"bounds.{side}.{key}"
        if calendar is not None:
            leaf = _ordinal_leaf(calendar, LocalDay(endpoint["year"], endpoint["month"], endpoint["day"]))
            if leaf is not None:
                return f"bounds.{side}.{leaf}"
    lower, upper = bounds["lower"], bounds["upper"]
    if (lower["year"], lower["month"], lower["day"]) > (upper["year"], upper["month"], upper["day"]):
        return "bounds.lower.year"
    return "bounds.lower.year"


def _era_machine_year(definition: EraDefinition, display_year: int) -> int | None:
    """Mirror the kernel's zero-skipping display conversion for leaf selection."""
    def ordinal(year: int) -> int | None:
        if definition.display_year_zero:
            return year
        if year == 0:
            return None
        return year if year < 0 else year - 1
    shown, epoch = ordinal(display_year), ordinal(definition.display_epoch_year)
    if shown is None or epoch is None:
        return None
    delta = shown - epoch
    if not I64_MIN <= delta <= I64_MAX:
        return None
    machine = definition.machine_epoch_year + delta
    return machine if I64_MIN <= machine <= I64_MAX else None


def _semantic(catalog: Any, value: Any, record: Record, field: str, errors: list[dict[str, Any]], calendar_definitions: dict[str, PreparedCalendar], era_definitions: dict[str, EraDefinition], *, era: bool = False) -> None:
    """Apply kernel-local validation without treating an isolated calendar as bad."""
    if isinstance(value, CivilDate):
        calendar = calendar_definitions[value.calendar_id]
        leaf = _civil_endpoint_leaf(calendar, value)
        if leaf is not None:
            code = "WDL-DATE-002" if leaf == "year" and _layout_for_year(calendar, value.year) is None else "WDL-DATE-003"
            errors.append(_error(code, "calendar date is unavailable for the declared year" if code == "WDL-DATE-002" else "civil date is semantically invalid", record, f"{field}.{leaf}")); return
    elif isinstance(value, EraDate):
        definition = era_definitions[value.era_id]
        machine_year = _era_machine_year(definition, value.year)
        if machine_year is None:
            errors.append(_error("WDL-ERA-008", "era date is semantically invalid", record, f"{field}.year")); return
        leaf = _civil_endpoint_leaf(calendar_definitions[definition.calendar_id], CivilDate(definition.calendar_id, machine_year, value.month, value.day))
        if leaf is not None:
            errors.append(_error("WDL-ERA-008", "era date is semantically invalid", record, f"{field}.{leaf}")); return
    elif isinstance(value, CivilRange):
        for side, endpoint in (("lower", value.lower), ("upper", value.upper)):
            if endpoint is None: continue
            leaf = _civil_endpoint_leaf(calendar_definitions[endpoint.calendar_id], endpoint)
            if leaf is not None:
                code = "WDL-DATE-002" if leaf == "year" else "WDL-DATE-003"
                errors.append(_error(code, "calendar date is unavailable for the declared year" if code == "WDL-DATE-002" else "civil date is semantically invalid", record, f"{field}.{side}.{leaf}")); return
    elif isinstance(value, ApproximateDate):
        for side, endpoint in (("lower", value.bounds.lower), ("upper", value.bounds.upper)):
            if endpoint is None: continue
            leaf = _civil_endpoint_leaf(calendar_definitions[endpoint.calendar_id], endpoint)
            if leaf is not None:
                code = "WDL-DATE-002" if leaf == "year" else "WDL-DATE-003"
                errors.append(_error(code, "calendar date is unavailable for the declared year" if code == "WDL-DATE-002" else "civil date is semantically invalid", record, f"{field}.bounds.{side}.{leaf}")); return
    if isinstance(value, (CivilDate, EraDate)):
        leaf = f"{field}.day" if value.day is not None else f"{field}.month" if value.month is not None else f"{field}.year"
        rendered = format_date(catalog, value)
        if isinstance(rendered, Invalid):
            errors.append(_error("WDL-ERA-008" if era else "WDL-DATE-002", "era date is semantically invalid" if era else "calendar date is unavailable for the declared year", record, leaf)); return
        if isinstance(rendered, Unavailable) and rendered.reason is not UnavailableReason.NO_EPOCH:
            errors.append(_error("WDL-DATE-002", "calendar date is unavailable for the declared year", record, f"{field}.year")); return
    normalized = normalize_date(catalog, value)
    if isinstance(normalized, Ok): return
    if isinstance(normalized, Unavailable):
        if normalized.reason is UnavailableReason.NO_EPOCH: return
        code = "WDL-DATE-002" if normalized.reason is UnavailableReason.TABLE_GAP else ("WDL-ERA-008" if era else "WDL-DATE-002")
        errors.append(_error(code, "calendar date is unavailable for the declared year" if code == "WDL-DATE-002" else "era date is semantically invalid", record, f"{field}.year")); return
    if isinstance(value, (CivilDate, EraDate)):
        field = f"{field}.day" if value.day is not None else f"{field}.month" if value.month is not None else f"{field}.year"
    errors.append(_error("WDL-ERA-008" if era else "WDL-DATE-003", "era date is semantically invalid" if era else "civil date is semantically invalid", record, field))


def _civil_key(value: CivilDate, *, upper: bool) -> tuple[int, int, int]:
    # This only orders isolated, same-calendar partial bounds; it never invents an axis.
    return (value.year, value.month if value.month is not None else (64 if upper else 0), value.day if value.day is not None else (4096 if upper else 0))


def _annotation(owner: Record, raw: Any, prepared: _Prepared, record_ids: set[str], errors: list[dict[str, Any]], field: str, depth: int = 0) -> None:
    calendars, eras, catalog = prepared.calendar_ids, prepared.era_ids, prepared.catalog
    if not _keys(raw, {"id", "provenance", "value"}, {"role", "display"}) or not _source_id(raw.get("id"), "chronology_") or not isinstance(raw.get("provenance"), list) or not raw["provenance"] or not all(isinstance(x, str) and x for x in raw["provenance"]) or ("role" in raw and not isinstance(raw["role"], str)) or ("display" in raw and not isinstance(raw["display"], str)):
        errors.append(_error("WDL-CHRON-004", "invalid chronology annotation envelope", owner, field)); return
    value = raw["value"]
    if not isinstance(value, dict) or len([key for key in value if not key.startswith("x-")]) != 1:
        errors.append(_error("WDL-CHRON-007", "annotation value must contain exactly one tag", owner, f"{field}.value")); return
    tag = next(key for key in value if not key.startswith("x-")); payload = value[tag]; value_field = f"{field}.value.{tag}"
    if tag == "civil":
        civil = _civil(payload, calendars, owner, value_field, errors)
        if civil: _semantic(catalog, civil, owner, value_field, errors, prepared.calendar_definitions, prepared.era_definitions)
    elif tag == "era":
        if not isinstance(payload, dict) or not _keys(payload, {"era_id", "year"}, {"month", "day"}): errors.append(_error("WDL-ERA-006", "invalid era annotation", owner, value_field))
        elif payload.get("era_id") not in eras: errors.append(_error("WDL-ERA-006", "invalid era annotation", owner, f"{value_field}.era_id"))
        elif not _i64(payload.get("year")): errors.append(_error("WDL-ERA-006", "invalid era annotation", owner, f"{value_field}.year"))
        elif "month" in payload and not _i64(payload["month"]): errors.append(_error("WDL-ERA-006", "invalid era annotation", owner, f"{value_field}.month"))
        elif "day" in payload and not _i64(payload["day"]): errors.append(_error("WDL-ERA-006", "invalid era annotation", owner, f"{value_field}.day"))
        elif "day" in payload and "month" not in payload: errors.append(_error("WDL-ERA-006", "invalid era precision", owner, f"{value_field}.month"))
        else:
            try: _semantic(catalog, EraDate(eras[payload["era_id"]], payload["year"], payload.get("month"), payload.get("day")), owner, value_field, errors, prepared.calendar_definitions, prepared.era_definitions, era=True)
            except ValueError: errors.append(_error("WDL-ERA-006", "invalid era precision", owner, f"{value_field}.month" if "month" not in payload else f"{value_field}.day"))
    elif tag == "range":
        if not isinstance(payload, dict) or not _keys(payload, {"calendar_id", "lower", "upper"}): errors.append(_error("WDL-DATE-003", "invalid civil range", owner, value_field))
        elif payload.get("calendar_id") not in calendars: errors.append(_error("WDL-DATE-003", "invalid civil range", owner, f"{value_field}.calendar_id"))
        else:
            before = len(errors)
            endpoints = {side: _civil(payload[side], calendars, owner, f"{value_field}.{side}", errors) if payload[side] is not None else None for side in ("lower", "upper")}
            if len(errors) != before:
                return
            if any(item and item.calendar_id != calendars[payload["calendar_id"]] for item in endpoints.values()):
                side = next(side for side, item in endpoints.items() if item and item.calendar_id != calendars[payload["calendar_id"]])
                errors.append(_error("WDL-DATE-003", "range endpoint calendar differs", owner, f"{value_field}.{side}.calendar_id"))
                return
            for side in ("lower", "upper"):
                endpoint = endpoints[side]
                if endpoint is None:
                    continue
                leaf = _civil_endpoint_leaf(prepared.calendar_definitions[endpoint.calendar_id], endpoint)
                if leaf is not None:
                    code = "WDL-DATE-002" if leaf == "year" and _layout_for_year(prepared.calendar_definitions[endpoint.calendar_id], endpoint.year) is None else "WDL-DATE-003"
                    message = "calendar date is unavailable for the declared year" if code == "WDL-DATE-002" else "civil date is semantically invalid"
                    errors.append(_error(code, message, owner, f"{value_field}.{side}.{leaf}"))
                    return
            if endpoints["lower"] and endpoints["upper"] and _civil_key(endpoints["lower"], upper=False) > _civil_key(endpoints["upper"], upper=True):
                errors.append(_error("WDL-DATE-003", "range is reversed", owner, f"{value_field}.lower.year")); return
            _semantic(catalog, CivilRange(calendars[payload["calendar_id"]], endpoints["lower"], endpoints["upper"]), owner, value_field, errors, prepared.calendar_definitions, prepared.era_definitions)
    elif tag == "approx":
        if not isinstance(payload, dict) or not _keys(payload, {"display_value", "bounds"}) or not isinstance(payload.get("display_value"), str) or not _keys(payload.get("bounds"), {"lower", "upper"}): errors.append(_error("WDL-DATE-004", "invalid approximation", owner, value_field))
        else:
            before = len(errors)
            bounds = {side: _civil(payload["bounds"][side], calendars, owner, f"{value_field}.bounds.{side}", errors) if payload["bounds"][side] is not None else None for side in ("lower", "upper")}
            if len(errors) != before:
                return
            if bounds["lower"] is None and bounds["upper"] is None: return
            present = bounds["lower"] or bounds["upper"]
            if bounds["lower"] and bounds["upper"] and bounds["lower"].calendar_id != bounds["upper"].calendar_id:
                errors.append(_error("WDL-DATE-004", "approximation bounds must share an ordered calendar", owner, f"{value_field}.bounds.upper.calendar_id")); return
            for side in ("lower", "upper"):
                endpoint = bounds[side]
                if endpoint is None:
                    continue
                leaf = _civil_endpoint_leaf(prepared.calendar_definitions[endpoint.calendar_id], endpoint)
                if leaf is not None:
                    code = "WDL-DATE-002" if leaf == "year" and _layout_for_year(prepared.calendar_definitions[endpoint.calendar_id], endpoint.year) is None else "WDL-DATE-003"
                    message = "calendar date is unavailable for the declared year" if code == "WDL-DATE-002" else "civil date is semantically invalid"
                    errors.append(_error(code, message, owner, f"{value_field}.bounds.{side}.{leaf}")); return
            if bounds["lower"] and bounds["upper"] and _civil_key(bounds["lower"], upper=False) > _civil_key(bounds["upper"], upper=True):
                errors.append(_error("WDL-DATE-004", "approximation bounds must share an ordered calendar", owner, f"{value_field}.bounds.lower.year")); return
            _semantic(catalog, ApproximateDate(payload["display_value"], CivilRange(present.calendar_id, bounds["lower"], bounds["upper"])), owner, value_field, errors, prepared.calendar_definitions, prepared.era_definitions)
    elif tag == "conflict":
        if depth >= 64 or not isinstance(payload, dict) or not isinstance(payload.get("claims"), list) or not 2 <= len(payload["claims"]) <= 64: errors.append(_error("WDL-DATE-005", "conflict requires 2..64 claims within depth 64", owner, value_field))
        else:
            for i, claim in enumerate(payload["claims"]): _annotation(owner, {"id": raw["id"], "provenance": raw["provenance"], "value": claim}, prepared, record_ids, errors, f"{value_field}.claims[{i}]", depth + 1)
    elif tag == "relative":
        invalid_ref = isinstance(payload, dict) and next((name for name in ("before_id", "after_id") if name in payload and (not isinstance(payload[name], str) or payload[name] not in record_ids)), None)
        if not isinstance(payload, dict) or not _keys(payload, {"relation"}, {"before_id", "after_id"}) or not isinstance(payload.get("relation"), str) or not any(name in payload for name in ("before_id", "after_id")) or invalid_ref:
            leaf = f".{invalid_ref}" if invalid_ref else ".relation" if isinstance(payload, dict) and ("relation" not in payload or not isinstance(payload.get("relation"), str)) else ""
            errors.append(_error("WDL-DATE-006", "relative date must use declared source-record references", owner, f"{value_field}{leaf}"))
    elif tag == "duration":
        if not isinstance(payload, dict) or not _keys(payload, {"unit", "value"}) or payload.get("unit") not in {"year", "month", "day"} or not _i64(payload.get("value")): errors.append(_error("WDL-DATE-007", "invalid display-only duration", owner, value_field))
    else: errors.append(_error("WDL-CHRON-007", "unknown annotation value tag", owner, value_field))


def _strip_chronology(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: _strip_chronology(item) for key, item in value.items() if key != "chronology"}
    if isinstance(value, list): return [_strip_chronology(item) for item in value]
    return value


def _nested_chronology(value: Any, field: str = "") -> list[str]:
    if isinstance(value, dict):
        paths: list[str] = []
        for key, item in value.items():
            if not isinstance(key, str): continue
            child = f"{field}.{key}" if field else key
            if key == "chronology": paths.append(child)
            paths.extend(_nested_chronology(item, child))
        return paths
    if isinstance(value, list):
        return [path for index, item in enumerate(value) for path in _nested_chronology(item, f"{field}[{index}]")]
    return []


def _inherited_v05(world: World, records: list[Record]) -> list[dict[str, Any]]:
    """Validate all non-chronology v0.5 rules without mutating candidate records."""
    clone: dict[str, Record] = {}
    for record in records:
        data = _strip_chronology(deepcopy(record.frontmatter))
        data["schema"] = "wedl/v0.5"
        clone[record.id] = Record(data, record.body, record.source_path, record.raw_bytes, record.blob_oid, record.revision)
    from .validation import validate_world
    inherited = World(world.revision, world.tree_oid, clone, world.root, world.source_root, world.is_worktree)
    return validate_world(inherited)


def _sort(errors: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return sorted(errors, key=lambda item: (item["path"] is None, item["path"] or "", item["entityId"] or "", item["field"] or "", item["code"], item["message"]))


def validate_v06_candidate(world: World, *, validate_inherited: bool = True) -> list[dict[str, Any]]:
    """Validate active v0.6 loading, compilation, and internal typed reads.

    ``validate_inherited`` is false only for a newer source envelope that
    deliberately reuses this module's chronology grammar.  That caller has
    already validated its own record envelope and must not be rejected for
    v0.6-only IDs or fields before chronology annotations are checked.
    """
    records = sorted(list(world), key=lambda record: (record.source_path.casefold(), record.id))
    schemas = [record.frontmatter.get("schema") if isinstance(record.frontmatter, dict) else None for record in records]
    world_records = [record for record in records if record.kind == "world"]
    if any(schema == V04_SOURCE_SCHEMA for schema in schemas):
        from .validation import validate_world
        return validate_world(world)
    if schemas and all(schema in {"wedl/v0.3", "wedl/v0.5"} for schema in schemas):
        # Keep existing runtime versions byte-for-byte on their established path.
        from .validation import validate_world
        return validate_world(world)
    if any(schema != "wedl/v0.6" for schema in schemas):
        owner = world_records[0] if world_records else (records[0] if records else None)
        same = all(schema == schemas[0] for schema in schemas) if schemas else True
        return [] if owner is None else [_error("WDL-SRC-008" if not same else "WDL-SRC-001", "v0.6 candidate must be homogeneous", owner, "schema")]
    if len(world_records) != 1:
        return [_error("WDL-WORLD-001", "world must contain exactly one world record", None, None)]
    owner = world_records[0]; errors: list[dict[str, Any]] = []
    if validate_inherited:
        inherited = _inherited_v05(world, records)
        if inherited:
            return _sort(inherited)
    chronology = owner.frontmatter.get("chronology")
    if not _keys(chronology, {"calendars", "eras", "anchors"}):
        return [_error("WDL-CHRON-001", "world chronology must be a closed declaration", owner, "chronology")]
    definitions, calendar_ids, _ = _calendars(owner, chronology, errors)
    if errors: return _sort(errors)
    # Fully prepare every calendar before constructing eras.  Calendar failure
    # is therefore always reported before an era endpoint is inspected.
    calendars_only = prepare_catalog(tuple(definitions))
    if isinstance(calendars_only, Invalid):
        for index, definition in enumerate(definitions):
            mechanics = prepare_catalog((replace(definition, epoch=None),))
            raw = chronology["calendars"][index]
            if isinstance(mechanics, Invalid):
                return [_error("WDL-CAL-011", "calendar definition is semantically invalid", owner, f"chronology.calendars[{index}].{_calendar_kernel_leaf(raw, definition, None, epoch=False)}")]
            complete = prepare_catalog((definition,))
            if isinstance(complete, Invalid):
                prepared = complete if isinstance(complete, Ok) else mechanics
                calendar = prepared.value.calendar(definition.id) if isinstance(prepared, Ok) else None
                return [_error("WDL-CAL-011", "calendar definition is semantically invalid", owner, f"chronology.calendars[{index}].{_calendar_kernel_leaf(raw, definition, calendar, epoch=True)}")]
        raise AssertionError(f"unattributed calendar preparation failure: {calendars_only.reason.value}")
    prepared_calendars = {item.definition.id: item for item in calendars_only.value.calendars}
    eras, era_ids = _eras(owner, chronology, calendar_ids, prepared_calendars, errors)
    anchors = _anchors(owner, chronology, errors)
    if errors: return _sort(errors)
    # The same split applies to eras: display arithmetic is independent of
    # inclusive bounds, so diagnostics never guess from a bounds key.
    for index, era in enumerate(eras):
        display = prepare_catalog(tuple(definitions), (replace(era, bounds=None),))
        if isinstance(display, Invalid):
            raw = chronology["eras"][index]
            return [_error("WDL-ERA-005", "era definition is semantically invalid", owner, f"chronology.eras[{index}].{_era_display_leaf(raw)}")]
        complete = prepare_catalog(tuple(definitions), (era,))
        if isinstance(complete, Invalid):
            raw = chronology["eras"][index]
            calendar = prepared_calendars[era.calendar_id]
            return [_error("WDL-ERA-005", "era definition is semantically invalid", owner, f"chronology.eras[{index}].{_era_bounds_leaf(raw, calendar)}")]
    for index in range(len(anchors)):
        item = prepare_catalog(tuple(definitions), tuple(eras), tuple(anchors[:index + 1]))
        if isinstance(item, Invalid):
            anchor = anchors[index]
            prior = anchors[:index]
            # These relations fully classify the kernel's monotonic-map
            # failures without coupling this boundary to its prose.
            if any(anchor.axis_day == other.axis_day for other in prior):
                leaf = "story_time"
            elif any(anchor.story_time == other.story_time for other in prior):
                leaf = "axis_day"
            else:
                leaf = "story_time"
            field = f"chronology.anchors[{index}].{leaf}"
            return [_error("WDL-ANCHOR-007", "anchors must map monotonically between StoryTime and axis day", owner, field)]
    result = prepare_catalog(tuple(definitions), tuple(eras), tuple(anchors))
    if isinstance(result, Invalid):
        reason = result.reason.value
        code = {"era": "WDL-ERA-005", "anchor": "WDL-ANCHOR-007", "definition": "WDL-CAL-011"}.get(reason, "WDL-CAL-011")
        message = {"WDL-CAL-011": "calendar definition is semantically invalid", "WDL-ERA-005": "era definition is semantically invalid", "WDL-ANCHOR-007": "anchors must map monotonically between StoryTime and axis day"}[code]
        # Every preceding phase owns a specific definition leaf, so this is
        # intentionally unreachable for well-formed source input.
        raise AssertionError(f"unattributed chronology preparation failure: {result.reason.value}")
    for record in records:
        for nested_field in _nested_chronology(record.frontmatter):
            if nested_field != "chronology":
                errors.append(_error("WDL-CHRON-003", "chronology is forbidden in nested operational objects", record, nested_field))
        if record.kind == "world": continue
        raw = record.frontmatter.get("chronology")
        if raw is None: continue
        if not isinstance(raw, list) or len(raw) > 500:
            errors.append(_error("WDL-CHRON-002", "non-world chronology must be an array of at most 500", record, "chronology")); continue
        annotation_ids: set[str] = set()
        for index, annotation in enumerate(raw):
            field = f"chronology[{index}]"
            if isinstance(annotation, dict) and isinstance(annotation.get("id"), str):
                if annotation["id"] in annotation_ids:
                    errors.append(_error("WDL-CHRON-005", "annotation IDs must be unique per record", record, f"{field}.id")); continue
                annotation_ids.add(annotation["id"])
            _annotation(record, annotation, _Prepared(calendar_ids, era_ids, result.value, prepared_calendars, {item.id: item for item in eras}), set(world.records), errors, field)
    return _sort(errors)
