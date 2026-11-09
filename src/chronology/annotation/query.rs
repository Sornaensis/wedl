//! Typed date operands and author-source interval predicates.
use super::*;
use std::str::FromStr;

/// A typed request value. Range endpoints must be civil dates on its calendar;
/// approximation bounds must be a range. Conflicts never select one claim.
#[derive(Clone, Debug)]
pub enum DateInput {
    Civil {
        calendar: String,
        year: i64,
        month: Option<u32>,
        day: Option<u32>,
    },
    Era {
        era: String,
        year: i64,
        month: Option<u32>,
        day: Option<u32>,
    },
    Range {
        calendar: String,
        lower: Option<Box<DateInput>>,
        upper: Option<Box<DateInput>>,
    },
    Approx {
        display: String,
        bounds: Box<DateInput>,
    },
    Conflict {
        claims: Vec<DateInput>,
    },
}

fn usage(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn object<'a>(
    value: &'a Json,
    required: &[&str],
    optional: &[&str],
    where_: &str,
) -> Result<&'a serde_json::Map<String, Json>> {
    let fields = value
        .as_object()
        .ok_or_else(|| usage(format!("{where_} must be an object")))?;
    if required.iter().any(|key| !fields.contains_key(*key))
        || fields
            .keys()
            .any(|key| !required.contains(&key.as_str()) && !optional.contains(&key.as_str()))
    {
        return Err(usage(format!("{where_} has missing or unsupported fields")));
    }
    Ok(fields)
}

fn request_text(value: &Json, field: &str) -> Result<String> {
    value
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| usage(format!("{field} must be a nonblank string")))
}

fn decimal<T: FromStr + ToString>(value: &Json, field: &str) -> Result<T> {
    let text = value
        .as_str()
        .ok_or_else(|| usage(format!("{field} must be an exact decimal string")))?;
    let value: T = text
        .parse()
        .map_err(|_| usage(format!("{field} is outside its integer range")))?;
    if value.to_string() != text {
        return Err(usage(format!("{field} must be a canonical decimal string")));
    }
    Ok(value)
}

fn input_components(
    fields: &serde_json::Map<String, Json>,
) -> Result<(i64, Option<u32>, Option<u32>)> {
    let year = decimal(&fields["year"], "year")?;
    let month = fields
        .get("month")
        .map(|v| decimal(v, "month"))
        .transpose()?;
    let day = fields.get("day").map(|v| decimal(v, "day")).transpose()?;
    if day.is_some() && month.is_none() {
        return Err(usage("day requires month"));
    }
    Ok((year, month, day))
}

fn parse_civil(value: &Json) -> Result<DateInput> {
    let fields = object(
        value,
        &["calendar_id", "year"],
        &["month", "day"],
        "civil date",
    )?;
    let (year, month, day) = input_components(fields)?;
    Ok(DateInput::Civil {
        calendar: request_text(&fields["calendar_id"], "calendar_id")?,
        year,
        month,
        day,
    })
}

fn parse_range(value: &Json) -> Result<DateInput> {
    let fields = object(value, &["calendar_id", "lower", "upper"], &[], "range")?;
    let endpoint = |key: &str| {
        (!fields[key].is_null())
            .then(|| parse_civil(&fields[key]).map(Box::new))
            .transpose()
    };
    Ok(DateInput::Range {
        calendar: request_text(&fields["calendar_id"], "calendar_id")?,
        lower: endpoint("lower")?,
        upper: endpoint("upper")?,
    })
}

fn parse_value(value: &Json, depth: usize, nodes: &mut usize) -> Result<DateInput> {
    *nodes += 1;
    if depth > MAX_VALUE_DEPTH || *nodes > MAX_VALUE_NODES {
        return Err(usage("date inputs exceed depth 64 or 4096 value nodes"));
    }
    let fields = object(
        value,
        &[],
        &["civil", "era", "range", "approx", "conflict"],
        "date value",
    )?;
    if fields.len() != 1 {
        return Err(usage("date value requires exactly one tag"));
    }
    let (tag, payload) = fields.iter().next().expect("one date tag");
    match tag.as_str() {
        "civil" => parse_civil(payload),
        "era" => {
            let fields = object(payload, &["era_id", "year"], &["month", "day"], "era date")?;
            let (year, month, day) = input_components(fields)?;
            Ok(DateInput::Era {
                era: request_text(&fields["era_id"], "era_id")?,
                year,
                month,
                day,
            })
        }
        "range" => parse_range(payload),
        "approx" => {
            let fields = object(payload, &["display_value", "bounds"], &[], "approximation")?;
            Ok(DateInput::Approx {
                display: request_text(&fields["display_value"], "display_value")?,
                bounds: Box::new(parse_range(&fields["bounds"])?),
            })
        }
        "conflict" => {
            let fields = object(payload, &["claims"], &[], "conflict")?;
            let claims = fields["claims"]
                .as_array()
                .filter(|claims| (2..=64).contains(&claims.len()))
                .ok_or_else(|| usage("conflict claims must contain 2..64 date values"))?;
            Ok(DateInput::Conflict {
                claims: claims
                    .iter()
                    .map(|claim| parse_value(claim, depth + 1, nodes))
                    .collect::<Result<_>>()?,
            })
        }
        _ => unreachable!("closed date tags"),
    }
}

impl DateInput {
    /// Decode the closed public tagged union. Every coordinate is a canonical
    /// decimal string, including coordinates larger than JavaScript integers.
    pub fn parse_json(value: &str) -> Result<Self> {
        if value.len() > 1_048_576 {
            return Err(usage("date input exceeds 1 MiB"));
        }
        // Use JSON's string/Unicode grammar with YAML Value's duplicate-aware
        // mapping visitor. Parsing YAML text would reject valid JSON surrogate
        // pairs; serde_json::Value alone silently keeps duplicate fields.
        // Retain serde_json's standard 128-container recursion budget.
        let mut decoder = serde_json::Deserializer::from_str(value);
        let _: Yaml = serde::Deserialize::deserialize(&mut decoder)
            .map_err(|error| usage(format!("invalid or duplicate date fields: {error}")))?;
        decoder
            .end()
            .map_err(|error| usage(format!("invalid date JSON: {error}")))?;
        let value = serde_json::from_str(value)
            .map_err(|error| usage(format!("invalid date JSON: {error}")))?;
        let result = parse_value(&value, 0, &mut 0)?;
        // Validate typed range/approximation structure even before a catalog is
        // available. The same guard also covers hand-built library inputs.
        result.source_value()?;
        Ok(result)
    }

    fn encoded(&self, public: bool, depth: usize, nodes: &mut usize) -> Result<Json> {
        *nodes += 1;
        if depth > MAX_VALUE_DEPTH || *nodes > MAX_VALUE_NODES {
            return Err(usage("date inputs exceed depth 64 or 4096 value nodes"));
        }
        let components = |id_key: &str,
                          id: &str,
                          year: i64,
                          month: Option<u32>,
                          day: Option<u32>| {
            if id.trim().is_empty() || day.is_some() && month.is_none() {
                return Err(usage("dates require an identifier and day requires month"));
            }
            let mut value =
                json!({id_key:id,"year":if public {json!(year.to_string())} else {json!(year)}});
            if let Some(month) = month {
                value["month"] = if public {
                    json!(month.to_string())
                } else {
                    json!(month)
                };
            }
            if let Some(day) = day {
                value["day"] = if public {
                    json!(day.to_string())
                } else {
                    json!(day)
                };
            }
            Ok(value)
        };
        match self {
            Self::Civil {
                calendar,
                year,
                month,
                day,
            } => Ok(json!({"civil":components("calendar_id",calendar,*year,*month,*day)?})),
            Self::Era {
                era,
                year,
                month,
                day,
            } => Ok(json!({"era":components("era_id",era,*year,*month,*day)?})),
            Self::Range {
                calendar,
                lower,
                upper,
            } => {
                if calendar.trim().is_empty() {
                    return Err(usage("range requires a calendar identifier"));
                }
                let mut endpoint = |value: &Option<Box<DateInput>>| -> Result<Json> {
                    match value.as_deref() {
                        None => Ok(Json::Null),
                        Some(Self::Civil { calendar: id, .. }) if id == calendar => Ok(value
                            .as_ref()
                            .expect("present endpoint")
                            .encoded(public, depth + 1, nodes)?["civil"]
                            .clone()),
                        Some(_) => Err(usage(
                            "range endpoints must be civil dates on the range calendar",
                        )),
                    }
                };
                let lower = endpoint(lower)?;
                let upper = endpoint(upper)?;
                Ok(json!({"range":{"calendar_id":calendar,"lower":lower,"upper":upper}}))
            }
            Self::Approx { display, bounds } => {
                if display.trim().is_empty() || !matches!(bounds.as_ref(), Self::Range { .. }) {
                    return Err(usage(
                        "approximation requires nonblank display and range bounds",
                    ));
                }
                let mut bounds = bounds.encoded(public, depth + 1, nodes)?["range"].clone();
                if !public {
                    bounds
                        .as_object_mut()
                        .expect("range payload")
                        .remove("calendar_id");
                }
                Ok(json!({"approx":{"display_value":display,"bounds":bounds}}))
            }
            Self::Conflict { claims } => {
                if !(2..=64).contains(&claims.len()) {
                    return Err(usage("conflict claims must contain 2..64 date values"));
                }
                Ok(
                    json!({"conflict":{"claims":claims.iter().map(|claim| claim.encoded(public,depth+1,nodes)).collect::<Result<Vec<_>>>()?}}),
                )
            }
        }
    }

    pub fn json(&self) -> Result<Json> {
        self.encoded(true, 0, &mut 0)
    }

    pub(in crate::chronology) fn source_value(&self) -> Result<Yaml> {
        Ok(serde_yaml::to_value(self.encoded(false, 0, &mut 0)?)?)
    }
}

/// Share source normalization with stored annotations, retaining request range
/// calendars even when both approximation bounds are open.
pub(in crate::chronology) fn normalize_input(
    world: &World,
    catalog: &Catalog<'_>,
    input: &DateInput,
) -> Result<ValueInfo> {
    input.source_value()?;
    fn normalize(normalizer: &mut Normalizer<'_, '_>, input: &DateInput) -> Result<ValueInfo> {
        match input {
            DateInput::Approx { bounds, .. } => {
                let mut value = normalize(normalizer, bounds)?;
                value.kind = "approx";
                value.approximate = true;
                if value
                    .interval
                    .as_ref()
                    .is_none_or(|value| value.lower.is_none() || value.upper.is_none())
                {
                    value.reason = Some("approximate_only");
                }
                Ok(value)
            }
            DateInput::Conflict { claims } => {
                let mut result = ValueInfo::unavailable("conflict", "conflicting_claims");
                result.unknown_basis = false;
                for claim in claims {
                    let child = normalize(normalizer, claim)?;
                    result.calendars.extend(child.calendars);
                    result.eras.extend(child.eras);
                    result.unknown_basis |= child.unknown_basis;
                }
                Ok(result)
            }
            _ => normalizer.value(&input.source_value()?, 0),
        }
    }
    normalize(
        &mut Normalizer {
            world,
            catalog,
            nodes: 0,
        },
        input,
    )
    .map_err(|error| usage(error.to_string()))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DatePredicate {
    OnDate,
    Before,
    After,
    Overlaps,
    Between,
}

impl DatePredicate {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OnDate => "on_date",
            Self::Before => "before",
            Self::After => "after",
            Self::Overlaps => "overlaps",
            Self::Between => "between",
        }
    }
}

impl FromStr for DatePredicate {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self> {
        match value {
            "on_date" => Ok(Self::OnDate),
            "before" => Ok(Self::Before),
            "after" => Ok(Self::After),
            "overlaps" => Ok(Self::Overlaps),
            "between" => Ok(Self::Between),
            _ => Err(usage(
                "predicate must be on_date, before, after, overlaps or between",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EraMatchMode {
    Authored,
    OverlapsBounds,
}

impl EraMatchMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Authored => "authored",
            Self::OverlapsBounds => "overlaps_bounds",
        }
    }
}

impl FromStr for EraMatchMode {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self> {
        match value {
            "authored" => Ok(Self::Authored),
            "overlaps_bounds" => Ok(Self::OverlapsBounds),
            _ => Err(usage("era mode must be authored or overlaps_bounds")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AnnotationQuery {
    pub predicate: DatePredicate,
    pub value: DateInput,
    pub upper: Option<DateInput>,
    pub record: Option<String>,
    pub calendar: Option<String>,
    pub era: Option<String>,
    pub era_mode: EraMatchMode,
    pub limit: usize,
}

fn overlap(left: &Interval, right: &Interval) -> bool {
    !left.upper.zip(right.lower).is_some_and(|(hi, lo)| hi < lo)
        && !right.upper.zip(left.lower).is_some_and(|(hi, lo)| hi < lo)
}

fn predicate_relation(
    predicate: DatePredicate,
    row: &Interval,
    query: &Interval,
) -> Option<&'static str> {
    match predicate {
        DatePredicate::OnDate if overlap(row, query) => {
            Some(if row.lower == query.lower && row.upper == query.upper {
                "exact"
            } else {
                "overlaps"
            })
        }
        DatePredicate::Overlaps | DatePredicate::Between if overlap(row, query) => Some("overlaps"),
        DatePredicate::Before if row.upper.zip(query.lower).is_some_and(|(hi, lo)| hi < lo) => {
            Some("before")
        }
        DatePredicate::After if row.lower.zip(query.upper).is_some_and(|(lo, hi)| lo > hi) => {
            Some("after")
        }
        _ => None,
    }
}

fn era_interval(catalog: &Catalog<'_>, id: &str) -> Result<Interval> {
    let era = catalog
        .eras
        .get(id)
        .ok_or_else(|| Error::NotFound(format!("unknown exact era ID {id:?}")))?;
    let calendar = &catalog.calendars[era.calendar];
    Ok(Interval {
        basis: basis(calendar),
        lower: era
            .bounds
            .map(|bounds| coordinate(calendar, bounds.0))
            .transpose()?,
        upper: era
            .bounds
            .map(|bounds| coordinate(calendar, bounds.1))
            .transpose()?,
    })
}

/// Search authored claims using inclusive intervals, without turning missing or
/// conflicting evidence into a date. Results are complete or return an error.
pub fn search_annotations(world: &World, query: &AnnotationQuery) -> Result<Json> {
    if !(1..=1000).contains(&query.limit) {
        return Err(usage("chronology search limit must be in 1..=1000"));
    }
    if (query.predicate == DatePredicate::Between) != query.upper.is_some() {
        return Err(usage(
            "between requires upper; other predicates forbid upper",
        ));
    }
    if query.era.is_none() && query.era_mode != EraMatchMode::Authored {
        return Err(usage("overlaps_bounds mode requires an era filter"));
    }
    if matches!(&query.value, DateInput::Approx { .. })
        && query.predicate != DatePredicate::Overlaps
    {
        return Err(usage("approximate query operands support only overlaps"));
    }
    if query
        .upper
        .as_ref()
        .is_some_and(|upper| matches!(upper, DateInput::Approx { .. }))
    {
        return Err(usage("between upper operands cannot be approximate"));
    }
    let catalog = Catalog::parse(world)?;
    let selected = query
        .record
        .as_deref()
        .map(|record| world.resolve(record))
        .transpose()?;
    if selected.is_some_and(|record| record.kind == "world") {
        return Err(usage(
            "world chronology contains declarations; select a record with annotations",
        ));
    }
    if let Some(calendar) = query.calendar.as_deref() {
        catalog.calendar(calendar)?;
    }
    let era = query
        .era
        .as_deref()
        .map(|id| era_interval(&catalog, id))
        .transpose()?;
    let mut result = envelope(world, &catalog);
    result["request"] = json!({
        "predicate":query.predicate.as_str(),"value":query.value.json()?,
        "upper":query.upper.as_ref().map(DateInput::json).transpose()?,
        "record":selected.map(|record| &record.id),"calendar":query.calendar,
        "era":query.era,"eraMode":query.era_mode.as_str(),"limit":query.limit,
    });
    result["items"] = json!([]);
    result["count"] = json!(0);
    result["limit"] = json!(query.limit);
    result["complete"] = json!(false);
    result["advisories"] = json!([]);
    // Keep source validation strict while query-only table gaps remain an
    // explicit unavailable result. Validate every supplied available operand
    // before returning so a gap cannot conceal an invalid second operand.
    let mut described = Vec::new();
    let mut missing = None;
    for input in std::iter::once(&query.value).chain(query.upper.as_ref()) {
        match date_query::available_dates(&catalog, input) {
            Ok(()) => described.push(Some(normalize_input(world, &catalog, input)?)),
            Err(Failure::Unavailable(reason)) => {
                missing.get_or_insert(reason);
                described.push(None);
            }
            Err(error) => return Err(error.error()),
        }
    }
    if let Some(reason) = missing {
        result["available"] = json!(false);
        result["reason"] = json!(reason);
        return Ok(result);
    }
    let value = described.remove(0).expect("available first operand");
    let upper = described.pop().flatten();
    if let Some(reason) = value
        .reason
        .or(upper.as_ref().and_then(|upper| upper.reason))
    {
        result["available"] = json!(false);
        result["reason"] = json!(reason);
        return Ok(result);
    }
    let mut interval = value.interval.expect("available date has an interval");
    if let Some(upper) = upper {
        let bounds = upper
            .interval
            .expect("available upper date has an interval");
        if upper.approximate
            || interval.lower.is_none()
            || interval.upper.is_none()
            || bounds.lower.is_none()
            || bounds.upper.is_none()
            || bounds.basis != interval.basis
        {
            return Err(usage(
                "between requires finite non-approximate operands on one comparable basis",
            ));
        }
        interval.upper = bounds.upper;
        if interval.lower > interval.upper {
            return Err(usage("between bounds are reversed"));
        }
    }
    if query.predicate == DatePredicate::Before && interval.lower.is_none()
        || query.predicate == DatePredicate::After && interval.upper.is_none()
    {
        return Err(usage(
            "before requires a finite lower bound; after requires a finite upper bound",
        ));
    }
    let mut items = Vec::new();
    let mut approximate_included = 0usize;
    let mut approximate_excluded = 0usize;
    let mut noncomparable = 0usize;
    for annotation in all_annotations(world, &catalog)? {
        let info = &annotation.info;
        if selected.is_some_and(|record| record.id != annotation.record.id)
            || query
                .calendar
                .as_ref()
                .is_some_and(|id| !info.calendars.contains(id))
            || (query.era_mode == EraMatchMode::Authored
                && query.era.as_ref().is_some_and(|id| !info.eras.contains(id)))
        {
            continue;
        }
        if info.reason.is_some() {
            // Count unknown-basis claims and claims that explicitly touch this
            // basis, but do not infer a date from their references.
            let in_scope = info.unknown_basis
                || info
                    .calendars
                    .iter()
                    .any(|id| basis(&catalog.calendars[id.as_str()]) == interval.basis);
            if in_scope {
                noncomparable += 1;
            }
            continue;
        }
        let Some(bounds) = info.interval.as_ref() else {
            continue;
        };
        if bounds.basis != interval.basis {
            continue;
        }
        if query.era_mode == EraMatchMode::OverlapsBounds
            && era
                .as_ref()
                .is_some_and(|era| era.basis != bounds.basis || !overlap(bounds, era))
        {
            continue;
        }
        let Some(relation) = predicate_relation(query.predicate, bounds, &interval) else {
            continue;
        };
        if info.approximate {
            if query.predicate != DatePredicate::Overlaps {
                approximate_excluded += 1;
                continue;
            }
            approximate_included += 1;
        }
        if items.len() == query.limit {
            return Err(usage(
                "chronology search result exceeds limit; narrow filters or increase limit for complete results",
            ));
        }
        let mut item = annotation.json(world)?;
        item["relation"] = json!(relation);
        items.push(item);
    }
    let mut advisories = Vec::new();
    for (kind, count) in [
        ("approximate-overlap-included", approximate_included),
        ("approximate-relation-excluded", approximate_excluded),
        ("noncomparable-excluded", noncomparable),
    ] {
        if count != 0 {
            advisories.push(json!({"kind":kind,"count":count}));
        }
    }
    result["available"] = json!(true);
    result["reason"] = Json::Null;
    result["complete"] = json!(true);
    result["normalized"] = json!({"basis":interval.basis,"lowerDay":interval.lower.map(|day| day.to_string()),"upperDay":interval.upper.map(|day| day.to_string()),"approximate":value.approximate});
    result["count"] = json!(items.len());
    result["items"] = json!(items);
    result["advisories"] = json!(advisories);
    Ok(result)
}
