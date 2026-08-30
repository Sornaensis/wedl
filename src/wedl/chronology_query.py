"""Typed chronology reads.  This is deliberately separate from public HTTP/CLI wire APIs."""
from __future__ import annotations
from dataclasses import dataclass
from enum import StrEnum
import json
from typing import Any
from .chronology import (ApproximateDate, CivilDate, CivilRange, EraDate,
    ConflictingDates, Invalid, InvalidReason, Ok, Unavailable, UnavailableReason)
from .chronology_index import (AnnotationIndexRow, ChronologyProjection,
    ComparisonKind, ComparableOperand, describe_date_value, externalize_date_value,
    kernelize_date_value, load_chronology_projection)

class ChronologyPredicate(StrEnum): ON_DATE="on_date"; OVERLAPS="overlaps"; BEFORE="before"; AFTER="after"; BETWEEN="between"
class EraMatchMode(StrEnum): AUTHORED="authored"; OVERLAPS_BOUNDS="overlaps_bounds"
class OutcomeKind(StrEnum): OK="ok"; INVALID="invalid"; UNAVAILABLE="unavailable"
class ChronologyReason(StrEnum):
    """Closed reason vocabulary at the internal chronology read boundary."""
    DEFINITION="definition"; DATE="date"; RANGE="range"; ERA="era"; ANCHOR="anchor"; OVERFLOW="overflow"
    NO_EPOCH="no_epoch"; TABLE_GAP="table_gap"; DISCONNECTED_TABLE="disconnected_table"; NO_SHARED_AXIS="no_shared_axis"
    APPROXIMATE_ONLY="approximate_only"; CONFLICTING_CLAIMS="conflicting_claims"
    INVALID_REQUEST="invalid_request"; NO_CHRONOLOGY="no_chronology"; CONVERSION_EXACTNESS="conversion_exactness"
@dataclass(frozen=True, slots=True)
class ChronologyAdvisory: kind: str; message: str; count: int = 1
@dataclass(frozen=True, slots=True)
class AnnotationQuery:
    predicate: ChronologyPredicate; value: Any; upper: Any = None; era_id: str | None = None; era_mode: EraMatchMode = EraMatchMode.AUTHORED; limit: int = 100
@dataclass(frozen=True, slots=True)
class AnnotationHit:
    record_id: str; annotation_id: str; source_ordinal: int; role: str | None; display: str | None; provenance: tuple[str, ...]
    value_kind: str; precision: str; basis_id: str | None; lower_day: int | None; upper_day: int | None; relation: str; value: Any
@dataclass(frozen=True, slots=True)
class AnnotationQueryResult: revision: str; request: AnnotationQuery; matches: tuple[AnnotationHit, ...]; advisories: tuple[ChronologyAdvisory, ...]
@dataclass(frozen=True, slots=True)
class ConversionRequest: value: Any; target_calendar_id: str | None = None; target_era_id: str | None = None
@dataclass(frozen=True, slots=True)
class ConvertedDate: source: Any; target: Any; formatted: str; axis_day: int | None
@dataclass(frozen=True, slots=True)
class ChronologyOutcome:
    kind: OutcomeKind; revision: str; value: Any = None; reason: ChronologyReason | None = None
    detail: str | None = None; advisories: tuple[ChronologyAdvisory, ...] = ()

@dataclass(frozen=True, slots=True)
class EraSelector:
    """Resolved author-facing era filter.  Kernel ids never leave this type."""
    source_id: str
    kernel_id: str
    basis_id: str
    lower_day: int | None
    upper_day: int | None
    lower_unbounded: bool
    upper_unbounded: bool

@dataclass(frozen=True, slots=True)
class CandidatePlan:
    """Immutable, shared source/SQLite candidate boundary for one request."""
    predicate: ChronologyPredicate
    basis_id: str
    lower_day: int | None
    upper_day: int | None
    era_mode: EraMatchMode
    era: EraSelector | None

class SourceChronologyStore:
    def __init__(self, projection: ChronologyProjection, revision: str): self.projection, self.revision = projection, revision
    @property
    def annotations(self): return self.projection.annotations
    def iter_candidate_annotations(self, plan: CandidatePlan):
        """Yield the same conservative boundary SQLite reads, in author order."""
        yield from (row for row in self.annotations if _candidate_row(row, plan))
    def noncomparable_count(self, plan: CandidatePlan) -> int:
        """Count the explicitly scoped, persisted non-comparable exclusions.

        A row without a calendar has no determinable relation to a calendar
        query and is deliberately not reported.  An authored-era filter is a
        non-temporal scope and therefore applies before this count as well.
        """
        return sum(_noncomparable_in_scope(row, plan) for row in self.annotations)
class SQLiteChronologyStore(SourceChronologyStore):
    _BATCH = 256
    def __init__(self, connection: Any, revision: str):
        self.connection = connection
        # Introspection for the internal compiler benchmark.  This records the
        # actual production candidate stream, rather than duplicating SQL in a
        # benchmark-only surrogate.
        self.last_candidate_stats: dict[str, Any] = {}
        super().__init__(load_chronology_projection(connection, load_annotations=False), revision)

    def iter_candidate_annotations(self, plan: CandidatePlan):
        """Stream bounded, index-backed candidates in authored keyset order."""
        # No unconditional approximate OR: finite approximate rows use the
        # same endpoint prefilter as other comparable rows.  The evaluator is
        # still authoritative for inclusion and advisories.
        where = ["basis_id = ?", "comparison_kind != 'noncomparable'"]; params: list[Any] = [plan.basis_id]
        if plan.era is not None and plan.era_mode is EraMatchMode.AUTHORED:
            where.append("era_id = ?"); params.append(plan.era.source_id)
        if plan.predicate in {ChronologyPredicate.ON_DATE, ChronologyPredicate.OVERLAPS, ChronologyPredicate.BETWEEN} and plan.lower_day is not None and plan.upper_day is not None:
            where.append("(lower_unbounded = 1 OR lower_day <= ?)"); where.append("(upper_unbounded = 1 OR upper_day >= ?)"); params.extend((plan.upper_day, plan.lower_day))
        elif plan.predicate is ChronologyPredicate.BEFORE and plan.lower_day is not None:
            where.append("(upper_unbounded = 1 OR upper_day < ?)"); params.append(plan.lower_day)
        elif plan.predicate is ChronologyPredicate.AFTER and plan.upper_day is not None:
            where.append("(lower_unbounded = 1 OR lower_day > ?)"); params.append(plan.upper_day)
        # An overlaps-bounds era selector is an independent predicate.  Do not
        # collapse it into the request interval: a wide annotation can overlap
        # both disjoint windows.  The selector prefilter is safe only on the
        # same basis; cross-basis rows remain a conservative stream and are
        # rejected by the shared evaluator below.
        if plan.era is not None and plan.era_mode is EraMatchMode.OVERLAPS_BOUNDS and plan.era.basis_id == plan.basis_id:
            if not plan.era.upper_unbounded:
                where.append("(lower_unbounded = 1 OR lower_day <= ?)"); params.append(plan.era.upper_day)
            if not plan.era.lower_unbounded:
                where.append("(upper_unbounded = 1 OR upper_day >= ?)"); params.append(plan.era.lower_day)
        cursor: tuple[int, int, str, str] | None = None
        candidates = batches = pages = maximum_batch = 0
        query_plan: tuple[str, ...] = ()
        continuation_plan: tuple[str, ...] = ()
        while True:
            page_where, page_params = list(where), list(params)
            if cursor is not None:
                record_ordinal, source_ordinal, record_id, annotation_id = cursor
                page_where.append("(record_ordinal > ? OR (record_ordinal = ? AND source_ordinal > ?) OR (record_ordinal = ? AND source_ordinal = ? AND record_id > ?) OR (record_ordinal = ? AND source_ordinal = ? AND record_id = ? AND annotation_id > ?))")
                page_params.extend((record_ordinal, record_ordinal, source_ordinal, record_ordinal, source_ordinal, record_id, record_ordinal, source_ordinal, record_id, annotation_id))
            indexed_table = "chronology_annotation INDEXED BY chronology_annotation_era_basis_order_idx" if plan.era is not None and plan.era_mode is EraMatchMode.AUTHORED else "chronology_annotation"
            statement = ("SELECT * FROM " + indexed_table + " WHERE " + " AND ".join(page_where)
                + " ORDER BY record_ordinal,source_ordinal,record_id,annotation_id LIMIT ?")
            if not query_plan:
                query_plan = tuple(str(item[-1]) for item in self.connection.execute(
                    "EXPLAIN QUERY PLAN " + statement, [*page_params, self._BATCH]
                ).fetchall())
            elif cursor is not None and not continuation_plan:
                continuation_plan = tuple(str(item[-1]) for item in self.connection.execute(
                    "EXPLAIN QUERY PLAN " + statement, [*page_params, self._BATCH]
                ).fetchall())
            batch = self.connection.execute(statement, [*page_params, self._BATCH]).fetchall()
            pages += 1
            candidates += len(batch)
            if batch:
                batches += 1
                maximum_batch = max(maximum_batch, len(batch))
            for row in batch:
                yield _annotation_from_sql(row)
            if len(batch) < self._BATCH:
                self.last_candidate_stats = {
                    "basis": plan.basis_id, "predicate": plan.predicate.value,
                    "candidates": candidates, "batches": batches, "pages": pages,
                    "batchSize": self._BATCH, "maxReturnedBatch": maximum_batch,
                    "queryPlan": query_plan, "continuationQueryPlan": continuation_plan,
                }
                break
            last = batch[-1]
            cursor = (last["record_ordinal"], last["source_ordinal"], last["record_id"], last["annotation_id"])

    def noncomparable_count(self, plan: CandidatePlan) -> int:
        # Scope is persisted in normalized tables.  It is not a heuristic over
        # nullable calendar_id/value JSON: conflicts can carry multiple bases.
        where = ["a.comparison_kind = 'noncomparable'", "(a.unknown_basis = 1 OR EXISTS (SELECT 1 FROM chronology_annotation_basis_scope bs WHERE bs.record_id = a.record_id AND bs.annotation_id = a.annotation_id AND bs.basis_id = ?))"]
        params: list[Any] = [plan.basis_id]
        if plan.era is not None and plan.era_mode is EraMatchMode.AUTHORED:
            where.append("EXISTS (SELECT 1 FROM chronology_annotation_era_scope es WHERE es.record_id = a.record_id AND es.annotation_id = a.annotation_id AND es.era_id = ?)")
            params.append(plan.era.source_id)
        statement = "SELECT count(*) FROM chronology_annotation AS a INDEXED BY chronology_annotation_exclusion_idx WHERE " + " AND ".join(where)
        explain = tuple(str(item[-1]) for item in self.connection.execute("EXPLAIN QUERY PLAN " + statement, params).fetchall())
        count = int(self.connection.execute(statement, params).fetchone()[0])
        self.last_candidate_stats = {**self.last_candidate_stats, "advisoryQueryPlan": explain}
        return count


def _candidate_row(row: AnnotationIndexRow, plan: CandidatePlan) -> bool:
    """Pure conservative equivalent of the indexed SQL prefilter."""
    if row.basis_id != plan.basis_id or row.comparison_kind is ComparisonKind.NONCOMPARABLE:
        return False
    if plan.era is not None and plan.era_mode is EraMatchMode.AUTHORED and row.era_id != plan.era.source_id:
        return False
    lo, hi = row.lower_day, row.upper_day
    if plan.predicate in {ChronologyPredicate.ON_DATE, ChronologyPredicate.OVERLAPS, ChronologyPredicate.BETWEEN}:
        return _intervals_overlap(lo, hi, plan.lower_day, plan.upper_day)
    if plan.predicate is ChronologyPredicate.BEFORE:
        return hi is not None and plan.lower_day is not None and hi < plan.lower_day
    return lo is not None and plan.upper_day is not None and lo > plan.upper_day


def _row_overlaps_era_selector(row: AnnotationIndexRow, plan: CandidatePlan) -> bool:
    """Return whether a row independently satisfies an overlap-bounds filter."""
    selector = plan.era
    return not (selector is not None and plan.era_mode is EraMatchMode.OVERLAPS_BOUNDS and (
        row.basis_id != selector.basis_id
        or not _intervals_overlap(row.lower_day, row.upper_day, selector.lower_day, selector.upper_day)
    ))


def _relation(row: AnnotationIndexRow, plan: CandidatePlan) -> str | None:
    """Evaluate the original request interval, never an era-intersected one."""
    lo, hi = row.lower_day, row.upper_day
    if plan.predicate is ChronologyPredicate.ON_DATE and _intervals_overlap(lo, hi, plan.lower_day, plan.upper_day):
        return "exact" if lo == plan.lower_day and hi == plan.upper_day else "overlaps"
    if plan.predicate in {ChronologyPredicate.OVERLAPS, ChronologyPredicate.BETWEEN} and _intervals_overlap(lo, hi, plan.lower_day, plan.upper_day):
        return "overlaps"
    if plan.predicate is ChronologyPredicate.BEFORE and hi is not None and plan.lower_day is not None and hi < plan.lower_day:
        return "before"
    if plan.predicate is ChronologyPredicate.AFTER and lo is not None and plan.upper_day is not None and lo > plan.upper_day:
        return "after"
    return None


def _noncomparable_in_scope(row: AnnotationIndexRow, plan: CandidatePlan) -> bool:
    if row.comparison_kind is not ComparisonKind.NONCOMPARABLE:
        return False
    if not row.unknown_basis and plan.basis_id not in row.scope_bases:
        return False
    return not (plan.era is not None and plan.era_mode is EraMatchMode.AUTHORED and plan.era.source_id not in row.scope_eras)


def _annotation_from_sql(row: Any) -> AnnotationIndexRow:
    return AnnotationIndexRow(row["record_id"], row["record_ordinal"], row["annotation_id"], row["source_ordinal"], row["role"], row["display"], row["provenance_json"], row["value_kind"], row["calendar_id"], row["era_id"], row["precision"], row["basis_id"], row["lower_day"], row["upper_day"], bool(row["lower_unbounded"]), bool(row["upper_unbounded"]), ComparisonKind(row["comparison_kind"]), row["exclusion_reason"], row["value_json"], (), (), bool(row["unknown_basis"]))


def _authored_format(store: SourceChronologyStore, private: Any, rendered: str) -> str:
    value = externalize_date_value(store.projection, private)
    if hasattr(value, "calendar_id"):
        return rendered.replace(private.calendar_id, value.calendar_id, 1)
    if hasattr(value, "era_id"):
        return rendered.replace(private.era_id, value.era_id, 1)
    return rendered

def _reason(value: InvalidReason | UnavailableReason) -> ChronologyReason:
    return ChronologyReason(value.value)


def _kernel_failure(store: SourceChronologyStore, result: Invalid | Unavailable) -> ChronologyOutcome:
    return ChronologyOutcome(OutcomeKind.INVALID if isinstance(result, Invalid) else OutcomeKind.UNAVAILABLE,
        store.revision, reason=_reason(result.reason), detail=result.detail)


def _invalid(store: SourceChronologyStore, detail: str, reason: ChronologyReason = ChronologyReason.INVALID_REQUEST) -> ChronologyOutcome:
    return ChronologyOutcome(OutcomeKind.INVALID, store.revision, reason=reason, detail=detail)


def _date_operand(store: SourceChronologyStore, value: Any) -> Ok[ComparableOperand] | Invalid | Unavailable:
    """Validate exactly the closed internal operand algebra before dereference."""
    if not isinstance(value, (CivilDate, EraDate, CivilRange, ApproximateDate, ConflictingDates)):
        return Invalid(InvalidReason.DATE, "date operand is not a supported chronology value")
    try:
        return describe_date_value(store.projection, kernelize_date_value(store.projection, value))
    except (AttributeError, KeyError, TypeError, ValueError) as exc:
        # Constructors are deliberately lightweight; malformed hand-built
        # values must still be total at this boundary.
        return Invalid(InvalidReason.DATE, f"invalid date operand: {exc}")


def _intervals_overlap(left_lower: int | None, left_upper: int | None, right_lower: int | None, right_upper: int | None) -> bool:
    return not ((left_upper is not None and right_lower is not None and left_upper < right_lower) or (right_upper is not None and left_lower is not None and right_upper < left_lower))


def _resolve_era_selector(store: SourceChronologyStore, identifier: str) -> Ok[EraSelector] | Invalid | Unavailable:
    """Resolve once before iteration/SQL, preserving kernel era semantics."""
    kernel_id = store.projection.era_kernel_id(identifier)
    catalog = store.projection.catalog
    if kernel_id is None or catalog is None:
        return Invalid(InvalidReason.ERA, "unknown era")
    era = catalog.era(kernel_id)
    if era is None:
        return Invalid(InvalidReason.ERA, "unknown era")
    if era.bounds is None:
        source_calendar = store.projection.source_calendar_id(era.definition.calendar_id)
        basis = "axis" if era.calendar.epoch_offset is not None else f"calendar:{source_calendar or era.definition.calendar_id}"
        return Ok(EraSelector(identifier, kernel_id, basis, None, None, True, True))
    lower = _date_operand(store, CivilDate(era.definition.calendar_id, era.bounds.lower.year, era.bounds.lower.month, era.bounds.lower.day))
    upper = _date_operand(store, CivilDate(era.definition.calendar_id, era.bounds.upper.year, era.bounds.upper.month, era.bounds.upper.day))
    if not isinstance(lower, Ok): return lower
    if not isinstance(upper, Ok): return upper
    if lower.value.basis_id != upper.value.basis_id:
        return Invalid(InvalidReason.ERA, "era bounds have no comparable basis")
    return Ok(EraSelector(identifier, kernel_id, lower.value.basis_id, lower.value.lower_day,
        upper.value.upper_day, False, False))


def _valid_request(store: SourceChronologyStore, request: Any) -> str | None:
    if not isinstance(request, AnnotationQuery): return "request must be AnnotationQuery"
    if not isinstance(request.predicate, ChronologyPredicate): return "predicate is invalid"
    if type(request.limit) is not int or not 1 <= request.limit <= 10_000: return "limit must be an integer in 1..10000"
    if request.era_id is not None and (not isinstance(request.era_id, str) or not request.era_id): return "era_id must be a nonempty string"
    if not isinstance(request.era_mode, EraMatchMode): return "era mode is invalid"
    if request.predicate is ChronologyPredicate.BETWEEN and request.upper is None: return "between requires an upper operand"
    if request.predicate is not ChronologyPredicate.BETWEEN and request.upper is not None: return "upper operand is only valid for between"
    if not isinstance(request.value, (CivilDate, EraDate, CivilRange, ApproximateDate, ConflictingDates)): return "value is not a supported chronology operand"
    if request.upper is not None and not isinstance(request.upper, (CivilDate, EraDate, CivilRange, ApproximateDate, ConflictingDates)): return "upper is not a supported chronology operand"
    if isinstance(request.value, ApproximateDate) and request.predicate is not ChronologyPredicate.OVERLAPS: return "approximate query operands are overlap-only"
    if isinstance(request.upper, ApproximateDate) and request.predicate is not ChronologyPredicate.OVERLAPS: return "approximate upper operands are overlap-only"
    return None


def query_annotations(store: SourceChronologyStore, request: AnnotationQuery) -> ChronologyOutcome:
    if not isinstance(store, SourceChronologyStore):
        return ChronologyOutcome(OutcomeKind.INVALID, "", reason=ChronologyReason.INVALID_REQUEST, detail="chronology store is invalid")
    if store.projection.catalog is None:
        return ChronologyOutcome(OutcomeKind.UNAVAILABLE, store.revision, reason=ChronologyReason.NO_CHRONOLOGY, detail="world has no chronology capability")
    invalid = _valid_request(store, request)
    if invalid: return _invalid(store, invalid)
    # Resolve both filter modes before reading a source row or issuing a
    # chronology_annotation statement.  AUTHORED is not a free-text filter.
    selector: EraSelector | None = None
    if request.era_id is not None:
        resolved = _resolve_era_selector(store, request.era_id)
        if not isinstance(resolved, Ok): return _kernel_failure(store, resolved)
        selector = resolved.value
    described = _date_operand(store, request.value)
    if not isinstance(described, Ok): return _kernel_failure(store, described)
    operand = described.value
    if operand.approximate and (operand.lower_day is None or operand.upper_day is None):
        return ChronologyOutcome(OutcomeKind.UNAVAILABLE, store.revision, reason=ChronologyReason.APPROXIMATE_ONLY, detail="qualitative approximation does not establish overlap")
    basis, lower, upper = operand.basis_id, operand.lower_day, operand.upper_day
    if request.predicate is ChronologyPredicate.BETWEEN:
        second = _date_operand(store, request.upper)
        if not isinstance(second, Ok): return _kernel_failure(store, second)
        if second.value.approximate or second.value.lower_day is None or second.value.upper_day is None:
            return ChronologyOutcome(OutcomeKind.UNAVAILABLE, store.revision, reason=ChronologyReason.APPROXIMATE_ONLY, detail="between requires finite non-approximate bounds")
        if second.value.basis_id != basis or lower is None or upper is None:
            return _invalid(store, "between bounds require one finite comparable basis", ChronologyReason.RANGE)
        upper = second.value.upper_day
        if lower > upper:
            return _invalid(store, "between bounds are reversed", ChronologyReason.RANGE)
    elif request.predicate is ChronologyPredicate.BEFORE and lower is None:
        return _invalid(store, "before requires a finite query lower bound", ChronologyReason.RANGE)
    elif request.predicate is ChronologyPredicate.AFTER and upper is None:
        return _invalid(store, "after requires a finite query upper bound", ChronologyReason.RANGE)
    plan = CandidatePlan(request.predicate, basis, lower, upper, request.era_mode, selector)
    hits: list[AnnotationHit] = []
    total_relation_hits = 0
    approximate_included = approximate_excluded = 0
    for row in store.iter_candidate_annotations(plan):
        if not _row_overlaps_era_selector(row, plan):
            continue
        lo, hi = row.lower_day, row.upper_day
        relation = _relation(row, plan)
        if row.comparison_kind is ComparisonKind.APPROXIMATE:
            if relation is None:
                continue
            if request.predicate is not ChronologyPredicate.OVERLAPS:
                approximate_excluded += 1
                continue
            approximate_included += 1
        if relation:
            total_relation_hits += 1
            # Exhaustion continues for exact advisory/count semantics, but the
            # retained public result is bounded at all times.
            if len(hits) < request.limit:
                hits.append(AnnotationHit(row.record_id,row.annotation_id,row.source_ordinal,row.role,row.display,tuple(json.loads(row.provenance_json)),row.value_kind,row.precision,row.basis_id,lo,hi,relation,json.loads(row.value_json)))
    # Both stores yield candidates in deterministic authored order; preserve it
    # so equal interval semantics cannot perturb externally visible ordering.
    advisories: list[ChronologyAdvisory] = []
    if approximate_included:
        advisories.append(ChronologyAdvisory("approximate-overlap-included", "approximate claims included by overlap", approximate_included))
    if approximate_excluded:
        advisories.append(ChronologyAdvisory("approximate-relation-excluded", "approximate claims are overlap-only", approximate_excluded))
    noncomparable = store.noncomparable_count(plan)
    if noncomparable:
        advisories.append(ChronologyAdvisory("noncomparable-excluded", "non-comparable claims excluded", noncomparable))
    limited = max(total_relation_hits - request.limit, 0)
    if limited: advisories.append(ChronologyAdvisory("result-limit","result limit applied",limited))
    if isinstance(store, SQLiteChronologyStore):
        store.last_candidate_stats = {**store.last_candidate_stats, "totalRelationHits": total_relation_hits, "retainedMatches": len(hits)}
    return ChronologyOutcome(OutcomeKind.OK, store.revision, AnnotationQueryResult(store.revision,request,tuple(hits),tuple(advisories)), advisories=tuple(advisories))

def format_chronology_date(store: SourceChronologyStore, value: Any) -> ChronologyOutcome:
    if not isinstance(store, SourceChronologyStore):
        return ChronologyOutcome(OutcomeKind.INVALID, "", reason=ChronologyReason.INVALID_REQUEST, detail="chronology store is invalid")
    if store.projection.catalog is None:
        return ChronologyOutcome(OutcomeKind.UNAVAILABLE,store.revision,reason=ChronologyReason.NO_CHRONOLOGY,detail="compiled formatting requires source catalog")
    if not isinstance(value, (CivilDate, EraDate)):
        return _invalid(store, "formatting requires a civil or era date")
    from .chronology import format_date
    try:
        private = kernelize_date_value(store.projection, value)
        result = format_date(store.projection.catalog, private)
    except (AttributeError, KeyError, TypeError, ValueError) as exc:
        return _invalid(store, f"invalid date operand: {exc}")
    return ChronologyOutcome(OutcomeKind.OK,store.revision,_authored_format(store, private, result.value)) if isinstance(result,Ok) else _kernel_failure(store, result)
def convert_chronology_date(store: SourceChronologyStore, request: ConversionRequest) -> ChronologyOutcome:
    if not isinstance(store, SourceChronologyStore):
        return ChronologyOutcome(OutcomeKind.INVALID, "", reason=ChronologyReason.INVALID_REQUEST, detail="chronology store is invalid")
    if not isinstance(request, ConversionRequest): return _invalid(store, "request must be ConversionRequest")
    if type(request.target_calendar_id) not in {str, type(None)} or type(request.target_era_id) not in {str, type(None)}:
        return _invalid(store, "target identifiers must be strings")
    if bool(request.target_calendar_id)==bool(request.target_era_id): return _invalid(store,"exactly one target is required")
    if not isinstance(request.value, (CivilDate, EraDate)):
        return _invalid(store, "conversion requires an exact civil or era date")
    catalog = store.projection.catalog
    if catalog is None: return ChronologyOutcome(OutcomeKind.UNAVAILABLE,store.revision,reason=ChronologyReason.NO_CHRONOLOGY,detail="conversion requires chronology catalog")
    from .chronology import AxisDay, ExactAxis, axis_to_civil, civil_to_era, format_date, normalize_date
    try: normalized = normalize_date(catalog, kernelize_date_value(store.projection, request.value))
    except (AttributeError, KeyError, TypeError, ValueError) as exc: return _invalid(store, f"invalid date operand: {exc}")
    if not isinstance(normalized, Ok): return _kernel_failure(store, normalized)
    if not isinstance(normalized.value, ExactAxis):
        return ChronologyOutcome(OutcomeKind.UNAVAILABLE,store.revision,reason=ChronologyReason.CONVERSION_EXACTNESS,detail="conversion requires an exact shared-axis date")
    target: Any
    if request.target_calendar_id:
        target_id = store.projection.calendar_kernel_id(request.target_calendar_id)
        calendar = catalog.calendar(target_id) if target_id else None
        if calendar is None: return ChronologyOutcome(OutcomeKind.INVALID,store.revision,reason=ChronologyReason.DATE,detail="unknown target calendar")
        civil = axis_to_civil(calendar, AxisDay(normalized.value.day.value))
        if not isinstance(civil, Ok): return _kernel_failure(store, civil)
        target = CivilDate(calendar.definition.id,civil.value.year,civil.value.month,civil.value.day)
    else:
        target_id = store.projection.era_kernel_id(request.target_era_id)
        era = catalog.era(target_id) if target_id else None
        if era is None: return ChronologyOutcome(OutcomeKind.INVALID,store.revision,reason=ChronologyReason.ERA,detail="unknown target era")
        civil = axis_to_civil(era.calendar, AxisDay(normalized.value.day.value))
        if not isinstance(civil, Ok): return _kernel_failure(store, civil)
        converted = civil_to_era(era,CivilDate(era.definition.calendar_id,civil.value.year,civil.value.month,civil.value.day))
        if not isinstance(converted, Ok): return _kernel_failure(store, converted)
        target = converted.value
    rendered = format_date(catalog,target)
    if not isinstance(rendered, Ok): return _kernel_failure(store, rendered)
    external = externalize_date_value(store.projection, target)
    # Formatting is an authored display surface, never a leaked private kernel ID.
    formatted = _authored_format(store, target, rendered.value)
    return ChronologyOutcome(OutcomeKind.OK,store.revision,ConvertedDate(request.value,external,formatted,normalized.value.day.value))
def map_chronology_date_to_story_time(store: SourceChronologyStore, value: Any) -> ChronologyOutcome:
    if not isinstance(store, SourceChronologyStore):
        return ChronologyOutcome(OutcomeKind.INVALID, "", reason=ChronologyReason.INVALID_REQUEST, detail="chronology store is invalid")
    if store.projection.catalog is None: return ChronologyOutcome(OutcomeKind.UNAVAILABLE,store.revision,reason=ChronologyReason.NO_CHRONOLOGY,detail="compiled mapping requires source catalog")
    if not isinstance(value, (CivilDate, EraDate, CivilRange, ApproximateDate, ConflictingDates)):
        return _invalid(store, "mapping requires a supported chronology value")
    from .chronology import map_date_to_story_time
    try: result=map_date_to_story_time(store.projection.catalog,kernelize_date_value(store.projection, value))
    except (AttributeError, KeyError, TypeError, ValueError) as exc: return _invalid(store, f"invalid date operand: {exc}")
    return ChronologyOutcome(OutcomeKind.OK,store.revision,result.value) if isinstance(result,Ok) else _kernel_failure(store, result)
