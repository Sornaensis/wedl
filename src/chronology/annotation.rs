//! Authored chronology claims. Partial dates describe inclusive intervals;
//! uncertainty, relative references and durations never supply invented dates.
use super::*;

mod query;
pub(super) use query::normalize_input;
pub use query::{AnnotationQuery, DateInput, DatePredicate, EraMatchMode, search_annotations};

const MAX_VALUE_DEPTH: usize = 64;
const MAX_VALUE_NODES: usize = 4096;

#[derive(Clone, Debug)]
pub struct AnnotationFilter {
    pub record: Option<String>,
    pub calendar: Option<String>,
    pub era: Option<String>,
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub(super) struct Interval {
    pub(super) basis: String,
    pub(super) lower: Option<i64>,
    pub(super) upper: Option<i64>,
}

pub(super) struct ValueInfo {
    pub(super) kind: &'static str,
    pub(super) precision: &'static str,
    pub(super) interval: Option<Interval>,
    pub(super) approximate: bool,
    pub(super) reason: Option<&'static str>,
    calendars: BTreeSet<String>,
    eras: BTreeSet<String>,
    unknown_basis: bool,
}

impl ValueInfo {
    fn unavailable(kind: &'static str, reason: &'static str) -> Self {
        Self {
            kind,
            precision: "none",
            interval: None,
            approximate: kind == "approx",
            reason: Some(reason),
            calendars: BTreeSet::new(),
            eras: BTreeSet::new(),
            unknown_basis: true,
        }
    }
    pub(super) fn json(&self) -> Json {
        json!({
            "valueKind": self.kind,
            "precision": self.precision,
            "basis": self.interval.as_ref().map(|value| &value.basis),
            "lowerDay": self.interval.as_ref().and_then(|value| value.lower).map(|value| value.to_string()),
            "upperDay": self.interval.as_ref().and_then(|value| value.upper).map(|value| value.to_string()),
            "approximate": self.approximate,
            "comparable": self.reason.is_none(),
            "reason": self.reason,
        })
    }
}

struct Annotation<'a> {
    record: &'a Record,
    id: &'a str,
    ordinal: usize,
    raw: &'a Yaml,
    info: ValueInfo,
}

impl Annotation<'_> {
    fn json(&self, world: &World) -> Result<Json> {
        let mut citation = super::citation(world, self.record);
        citation["field"] = json!(format!("chronology[{}]", self.ordinal));
        Ok(json!({
            "recordId": self.record.id,
            "recordTitle": self.record.title,
            "recordKind": self.record.kind,
            "recordStatus": self.record.status(),
            "annotationId": self.id,
            "sourceOrdinal": self.ordinal.to_string(),
            "role": self.raw.get("role").and_then(Yaml::as_str),
            "display": self.raw.get("display").and_then(Yaml::as_str),
            "provenance": declaration_value(&self.raw["provenance"])? ,
            "value": declaration_value(&self.raw["value"])? ,
            "normalized": self.info.json(),
            "citation": citation,
        }))
    }
}

fn calculation<T>(value: Calculation<T>) -> Result<T> {
    value.map_err(|error| invalid(error.error().to_string()))
}

fn coordinate(calendar: &Calendar<'_>, value: CivilDate) -> Result<i64> {
    if calendar.offset.is_some() {
        calculation(calendar.axis(value))
    } else {
        calculation(signed(calculation(calendar.ordinal(value))?))
    }
}

fn basis(calendar: &Calendar<'_>) -> String {
    if calendar.offset.is_some() {
        "axis".into()
    } else {
        format!("calendar:{}", calendar.id)
    }
}

fn precision(month: Option<u32>, day: Option<u32>) -> &'static str {
    if day.is_some() {
        "day"
    } else if month.is_some() {
        "month"
    } else {
        "year"
    }
}

fn components(fields: &Mapping) -> Result<(i64, Option<u32>, Option<u32>)> {
    let year = integer(&fields["year"], "annotation year")?;
    let month = fields
        .get("month")
        .map(|value| small(value, "annotation month", 64))
        .transpose()?;
    let day = fields
        .get("day")
        .map(|value| small(value, "annotation day", 4096))
        .transpose()?;
    if day.is_some() && month.is_none() {
        return Err(invalid("annotation day requires a month"));
    }
    Ok((year, month, day))
}

pub(super) fn partial(
    calendar: &Calendar<'_>,
    year: i64,
    month: Option<u32>,
    day: Option<u32>,
) -> Result<(Interval, CivilDate, CivilDate)> {
    let (lower, upper) = calculation(partial_dates(calendar, year, month, day))?;
    Ok((
        Interval {
            basis: basis(calendar),
            lower: Some(coordinate(calendar, lower)?),
            upper: Some(coordinate(calendar, upper)?),
        },
        lower,
        upper,
    ))
}

struct Normalizer<'w, 'c> {
    world: &'w World,
    catalog: &'c Catalog<'w>,
    nodes: usize,
}

impl Normalizer<'_, '_> {
    fn calendar(&self, value: &Yaml) -> Result<&Calendar<'_>> {
        let id = identifier(value, "calendar")?;
        self.catalog
            .calendars
            .get(id)
            .ok_or_else(|| invalid(format!("unknown exact annotation calendar ID {id:?}")))
    }

    fn civil(&self, value: &Yaml) -> Result<ValueInfo> {
        let fields = closed(
            value,
            &["calendar_id", "year"],
            &["month", "day"],
            "civil annotation",
        )?;
        let calendar = self.calendar(&fields["calendar_id"])?;
        let (year, month, day) = components(fields)?;
        let (interval, _, _) = partial(calendar, year, month, day)?;
        Ok(ValueInfo {
            kind: "civil",
            precision: precision(month, day),
            interval: Some(interval),
            approximate: false,
            reason: None,
            calendars: BTreeSet::from([calendar.id.to_owned()]),
            eras: BTreeSet::new(),
            unknown_basis: false,
        })
    }

    fn era(&self, value: &Yaml) -> Result<ValueInfo> {
        let fields = closed(
            value,
            &["era_id", "year"],
            &["month", "day"],
            "era annotation",
        )?;
        let id = identifier(&fields["era_id"], "era")?;
        let era = self
            .catalog
            .eras
            .get(id)
            .ok_or_else(|| invalid(format!("unknown exact annotation era ID {id:?}")))?;
        let calendar = &self.catalog.calendars[era.calendar];
        let (year, month, day) = components(fields)?;
        let machine = calculation(era.civil_year(year))?;
        let (interval, lower, upper) = partial(calendar, machine, month, day)?;
        calculation(era.contains_interval(lower, upper))?;
        Ok(ValueInfo {
            kind: "era",
            precision: precision(month, day),
            interval: Some(interval),
            approximate: false,
            reason: None,
            calendars: BTreeSet::from([calendar.id.to_owned()]),
            eras: BTreeSet::from([id.to_owned()]),
            unknown_basis: false,
        })
    }

    fn range(&self, value: &Yaml, approximate: bool) -> Result<ValueInfo> {
        let fields = if approximate {
            let fields = closed(
                value,
                &["display_value", "bounds"],
                &[],
                "approximate annotation",
            )?;
            text(&fields["display_value"], "approximation display_value")?;
            closed(
                &fields["bounds"],
                &["lower", "upper"],
                &[],
                "approximation bounds",
            )?
        } else {
            closed(
                value,
                &["calendar_id", "lower", "upper"],
                &[],
                "range annotation",
            )?
        };
        let lower = (!fields["lower"].is_null())
            .then(|| self.civil(&fields["lower"]))
            .transpose()?;
        let upper = (!fields["upper"].is_null())
            .then(|| self.civil(&fields["upper"]))
            .transpose()?;
        let calendar_id = if approximate {
            lower
                .as_ref()
                .or(upper.as_ref())
                .and_then(|value| value.calendars.first())
                .map(String::as_str)
        } else {
            Some(self.calendar(&fields["calendar_id"])?.id)
        };
        let Some(calendar_id) = calendar_id else {
            return Ok(ValueInfo::unavailable("approx", "approximate_only"));
        };
        for endpoint in [&lower, &upper].into_iter().flatten() {
            if !endpoint.calendars.contains(calendar_id) {
                return Err(invalid(
                    "range endpoints must use the same declared calendar",
                ));
            }
        }
        let calendar = &self.catalog.calendars[calendar_id];
        let lower_day = lower
            .as_ref()
            .and_then(|value| value.interval.as_ref()?.lower);
        let upper_day = upper
            .as_ref()
            .and_then(|value| value.interval.as_ref()?.upper);
        if lower_day.zip(upper_day).is_some_and(|(lo, hi)| lo > hi) {
            return Err(invalid("annotation range is reversed"));
        }
        let qualitative = approximate && (lower_day.is_none() || upper_day.is_none());
        Ok(ValueInfo {
            kind: if approximate { "approx" } else { "range" },
            precision: "range",
            interval: Some(Interval {
                basis: basis(calendar),
                lower: lower_day,
                upper: upper_day,
            }),
            approximate,
            reason: qualitative.then_some("approximate_only"),
            calendars: BTreeSet::from([calendar_id.to_owned()]),
            eras: BTreeSet::new(),
            unknown_basis: false,
        })
    }

    fn value(&mut self, value: &Yaml, depth: usize) -> Result<ValueInfo> {
        self.nodes += 1;
        if depth > MAX_VALUE_DEPTH || self.nodes > MAX_VALUE_NODES {
            return Err(invalid(
                "annotation values exceed depth 64 or 4096 total nodes per record",
            ));
        }
        let fields = closed(
            value,
            &[],
            &[
                "civil", "era", "range", "approx", "conflict", "relative", "duration",
            ],
            "annotation value",
        )?;
        let mut tags = fields.iter().filter_map(|(key, value)| {
            let key = key.as_str().expect("closed mapping keys are strings");
            (!key.starts_with("x-")).then_some((key, value))
        });
        let Some((tag, payload)) = tags.next() else {
            return Err(invalid("annotation value requires exactly one tag"));
        };
        if tags.next().is_some() {
            return Err(invalid("annotation value requires exactly one tag"));
        }
        match tag {
            "civil" => self.civil(payload),
            "era" => self.era(payload),
            "range" => self.range(payload, false),
            "approx" => self.range(payload, true),
            "conflict" => {
                let fields = closed(payload, &["claims"], &[], "conflicting annotation")?;
                let claims = array(&fields["claims"], "conflicting claims", 64)?;
                if claims.len() < 2 {
                    return Err(invalid("conflicting annotations require 2..64 claims"));
                }
                let mut info = ValueInfo::unavailable("conflict", "conflicting_claims");
                info.unknown_basis = false;
                for claim in claims {
                    let child = self.value(claim, depth + 1)?;
                    info.calendars.extend(child.calendars);
                    info.eras.extend(child.eras);
                    info.unknown_basis |= child.unknown_basis;
                }
                Ok(info)
            }
            "relative" => {
                let fields = closed(
                    payload,
                    &["relation"],
                    &["before_id", "after_id"],
                    "relative annotation",
                )?;
                text(&fields["relation"], "relative relation")?;
                let mut found = false;
                for field in ["before_id", "after_id"] {
                    if let Some(value) = fields.get(field) {
                        let id = text(value, "relative record ID")?;
                        if !self.world.records.contains_key(id) {
                            return Err(invalid(format!(
                                "unknown exact relative record ID {id:?}"
                            )));
                        }
                        found = true;
                    }
                }
                if !found {
                    return Err(invalid(
                        "relative annotation requires before_id or after_id",
                    ));
                }
                Ok(ValueInfo::unavailable("relative", "relative"))
            }
            "duration" => {
                let fields = closed(payload, &["unit", "value"], &[], "duration annotation")?;
                let unit = text(&fields["unit"], "duration unit")?;
                if !["year", "month", "day"].contains(&unit) {
                    return Err(invalid("duration unit must be year, month or day"));
                }
                integer(&fields["value"], "duration value")?;
                Ok(ValueInfo::unavailable("duration", "duration"))
            }
            _ => unreachable!("closed annotation value tags"),
        }
    }
}

fn record_annotations<'a>(
    world: &'a World,
    catalog: &Catalog<'a>,
    record: &'a Record,
) -> Result<Vec<Annotation<'a>>> {
    if record.kind == "world" {
        return Ok(Vec::new());
    }
    let Some(value) = record.get("chronology") else {
        return Ok(Vec::new());
    };
    let values = array(value, "record chronology", MAX_DECLARATIONS)?;
    let mut normalizer = Normalizer {
        world,
        catalog,
        nodes: 0,
    };
    let mut ids = BTreeSet::new();
    let mut annotations = Vec::new();
    for (ordinal, raw) in values.iter().enumerate() {
        let result = (|| {
            let fields = closed(
                raw,
                &["id", "provenance", "value"],
                &["role", "display"],
                "chronology annotation",
            )?;
            let id = identifier(&fields["id"], "chronology")?;
            if !ids.insert(id.to_ascii_uppercase()) {
                return Err(invalid(
                    "chronology annotation IDs must be unique per record",
                ));
            }
            strings(&fields["provenance"], "annotation provenance", true)?;
            for key in ["role", "display"] {
                if let Some(value) = fields.get(key) {
                    text(value, key)?;
                }
            }
            let info = normalizer.value(&fields["value"], 0)?;
            Ok(Annotation {
                record,
                id,
                ordinal,
                raw,
                info,
            })
        })();
        annotations.push(
            result.map_err(|error: Error| invalid(format!("chronology[{ordinal}]: {error}")))?,
        );
    }
    annotations.sort_by(|left, right| left.id.cmp(right.id));
    Ok(annotations)
}

pub(super) fn validate(world: &World, catalog: &Catalog<'_>) -> Vec<Diagnostic> {
    world
        .records
        .values()
        .filter_map(|record| {
            record_annotations(world, catalog, record)
                .err()
                .map(|error| Diagnostic {
                    code: "WDL-CHRONOLOGY-ANNOTATION-001".into(),
                    message: error.to_string(),
                    severity: "error".into(),
                    entity_id: Some(record.id.clone()),
                    path: Some(record.source_path.clone()),
                    field: Some("chronology".into()),
                })
        })
        .collect()
}

fn all_annotations<'a>(world: &'a World, catalog: &Catalog<'a>) -> Result<Vec<Annotation<'a>>> {
    let mut annotations = Vec::new();
    for record in world.records.values() {
        annotations.extend(record_annotations(world, catalog, record)?);
    }
    Ok(annotations)
}

/// Complete author-source claims, with filters on explicitly referenced IDs.
pub fn annotations(world: &World, filter: &AnnotationFilter) -> Result<Json> {
    if !(1..=1000).contains(&filter.limit) {
        return Err(Error::InvalidInput(
            "annotation limit must be in 1..=1000".into(),
        ));
    }
    let catalog = Catalog::parse(world)?;
    let selected = filter
        .record
        .as_deref()
        .map(|value| world.resolve(value))
        .transpose()?;
    if selected.is_some_and(|record| record.kind == "world") {
        return Err(Error::InvalidInput(
            "world chronology contains declarations; use chronology calendars".into(),
        ));
    }
    if let Some(id) = filter.calendar.as_deref() {
        catalog.calendar(id)?;
    }
    if let Some(id) = filter.era.as_deref() {
        if !catalog.eras.contains_key(id) {
            return Err(Error::NotFound(format!("unknown exact era ID {id:?}")));
        }
    }
    let mut items = Vec::new();
    for annotation in all_annotations(world, &catalog)? {
        if selected.is_some_and(|record| record.id != annotation.record.id)
            || filter
                .calendar
                .as_ref()
                .is_some_and(|id| !annotation.info.calendars.contains(id))
            || filter
                .era
                .as_ref()
                .is_some_and(|id| !annotation.info.eras.contains(id))
        {
            continue;
        }
        if items.len() == filter.limit {
            return Err(Error::InvalidInput(
                "annotation result exceeds limit; narrow filters or increase limit for complete results".into(),
            ));
        }
        items.push(annotation.json(world)?);
    }
    let mut result = envelope(world, &catalog);
    result["items"] = json!(items);
    result["count"] = json!(items.len());
    result["limit"] = json!(filter.limit);
    result["complete"] = json!(true);
    Ok(result)
}

fn relation(left: &ValueInfo, right: &ValueInfo) -> (&'static str, Option<&'static str>) {
    if let Some(reason) = left.reason.or(right.reason) {
        return ("unknown", Some(reason));
    }
    let (Some(left_bounds), Some(right_bounds)) = (&left.interval, &right.interval) else {
        return ("unknown", Some("no_shared_axis"));
    };
    if left_bounds.basis != right_bounds.basis {
        return ("unknown", Some("no_shared_axis"));
    }
    let before = left_bounds
        .upper
        .zip(right_bounds.lower)
        .is_some_and(|(hi, lo)| hi < lo);
    let after = left_bounds
        .lower
        .zip(right_bounds.upper)
        .is_some_and(|(lo, hi)| lo > hi);
    if left.approximate || right.approximate {
        return if before || after {
            ("unknown", Some("approximate_only"))
        } else {
            ("overlaps", None)
        };
    }
    if before {
        ("before", None)
    } else if after {
        ("after", None)
    } else if left_bounds.lower.is_some()
        && left_bounds.lower == left_bounds.upper
        && left_bounds.lower == right_bounds.lower
        && right_bounds.lower == right_bounds.upper
    {
        ("equal", None)
    } else {
        ("overlaps", None)
    }
}

/// Compare two authored claims without choosing a conflicting/uncertain date.
pub fn compare_annotations(
    world: &World,
    left_record: &str,
    left_id: &str,
    right_record: &str,
    right_id: &str,
) -> Result<Json> {
    let catalog = Catalog::parse(world)?;
    let left_owner = world.resolve(left_record)?;
    let right_owner = world.resolve(right_record)?;
    if left_owner.kind == "world" || right_owner.kind == "world" {
        return Err(Error::InvalidInput(
            "world chronology contains declarations, not annotations".into(),
        ));
    }
    let annotations = all_annotations(world, &catalog)?;
    let find = |record: &Record, id: &str| {
        annotations
            .iter()
            .find(|annotation| annotation.record.id == record.id && annotation.id == id)
            .ok_or_else(|| {
                Error::NotFound(format!(
                    "unknown exact annotation ID {id:?} on record {:?}",
                    record.id
                ))
            })
    };
    let left = find(left_owner, left_id)?;
    let right = find(right_owner, right_id)?;
    let (relation, reason) = relation(&left.info, &right.info);
    let mut result = envelope(world, &catalog);
    result["left"] = left.json(world)?;
    result["right"] = right.json(world)?;
    result["relation"] = json!(relation);
    result["reason"] = json!(reason);
    Ok(result)
}
