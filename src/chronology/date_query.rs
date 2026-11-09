//! Author-facing civil/era inputs and explicit anchor interval reads.
use super::*;

#[derive(Clone, Debug)]
pub enum DateTarget {
    Calendar(String),
    Era(String),
}

struct ResolvedDate<'c, 'w> {
    calendar: &'c Calendar<'w>,
    lower: CivilDate,
    upper: CivilDate,
    month: Option<u32>,
    day: Option<u32>,
}

fn unknown(kind: &str, id: &str) -> Failure {
    Failure::Invalid(format!("unknown exact {kind} ID {id:?}"))
}

fn resolve_date<'c, 'w>(
    catalog: &'c Catalog<'w>,
    value: &DateInput,
) -> Calculation<ResolvedDate<'c, 'w>> {
    let (calendar, year, month, day, era) = match value {
        DateInput::Civil {
            calendar,
            year,
            month,
            day,
        } => (
            catalog
                .calendars
                .get(calendar.as_str())
                .ok_or_else(|| unknown("calendar", calendar))?,
            *year,
            *month,
            *day,
            None,
        ),
        DateInput::Era {
            era,
            year,
            month,
            day,
        } => {
            let era = catalog
                .eras
                .get(era.as_str())
                .ok_or_else(|| unknown("era", era))?;
            (
                &catalog.calendars[era.calendar],
                era.civil_year(*year)?,
                *month,
                *day,
                Some(era),
            )
        }
        _ => {
            return Err(Failure::Invalid(
                "formatting and conversion require a civil or era date".into(),
            ));
        }
    };
    let (lower, upper) = partial_dates(calendar, year, month, day)?;
    if let Some(era) = era {
        era.contains_interval(lower, upper)?;
    }
    Ok(ResolvedDate {
        calendar,
        lower,
        upper,
        month,
        day,
    })
}

fn format_input(value: &DateInput) -> String {
    let (id, year, month, day) = match value {
        DateInput::Civil {
            calendar,
            year,
            month,
            day,
        } => (calendar, year, month, day),
        DateInput::Era {
            era,
            year,
            month,
            day,
        } => (era, year, month, day),
        _ => unreachable!("only resolved civil and era dates are formatted"),
    };
    let mut result = format!("{id}:{year}");
    if let Some(month) = month {
        result.push_str(&format!("-{month}"));
    }
    if let Some(day) = day {
        result.push_str(&format!("-{day}"));
    }
    result
}

/// Validate a local civil or era date without requiring a shared-axis epoch.
/// Authored precision and exact display-year strings are retained.
pub fn format_date(world: &World, value: &DateInput) -> Result<Json> {
    let catalog = Catalog::parse(world)?;
    let mut result = envelope(world, &catalog);
    result["value"] = value.json()?;
    let date = match resolve_date(&catalog, value) {
        Ok(date) => date,
        Err(error) => return unavailable(result, error),
    };
    result["found"] = json!(true);
    result["calendarId"] = json!(date.calendar.id);
    result["formatted"] = json!(format_input(value));
    result["precision"] = json!(if date.day.is_some() {
        "day"
    } else if date.month.is_some() {
        "month"
    } else {
        "year"
    });
    result["civil"] = json!({
        "year": date.lower.year.to_string(),
        "month": date.month.map(|value| value.to_string()),
        "day": date.day.map(|value| value.to_string()),
    });
    result["bounds"] = json!({"lower":date_value(date.lower),"upper":date_value(date.upper)});
    result["axisDay"] = Json::Null;
    result["axisInterval"] = Json::Null;
    match date
        .calendar
        .axis(date.lower)
        .and_then(|lower| date.calendar.axis(date.upper).map(|upper| (lower, upper)))
    {
        Ok((lower, upper)) => {
            result["axisInterval"] =
                json!({"lowerDay":lower.to_string(),"upperDay":upper.to_string()});
            if date.day.is_some() {
                result["axisDay"] = json!(lower.to_string());
            }
            result["axisReason"] = Json::Null;
        }
        Err(Failure::Unavailable(reason)) => result["axisReason"] = json!(reason),
        // Local validation has already succeeded, so only signed-axis overflow
        // can make an otherwise valid interval unrepresentable here.
        Err(Failure::Invalid(_)) => result["axisReason"] = json!("overflow"),
    }
    Ok(result)
}

/// Convert an explicitly complete civil or era day to a calendar or era.
/// A partial date never becomes exact merely because its bounds coincide.
pub fn convert_date(world: &World, value: &DateInput, target: &DateTarget) -> Result<Json> {
    let catalog = Catalog::parse(world)?;
    let (target_calendar, target_era) = match target {
        DateTarget::Calendar(id) => (catalog.calendar(id)?, None),
        DateTarget::Era(id) => {
            let era = catalog
                .eras
                .get(id.as_str())
                .ok_or_else(|| Error::NotFound(format!("unknown exact era ID {id:?}")))?;
            (&catalog.calendars[era.calendar], Some(era))
        }
    };
    let mut result = envelope(world, &catalog);
    result["source"] = value.json()?;
    result["toCalendarId"] = json!(target_calendar.id);
    result["toEraId"] = json!(target_era.map(|era| era.id));
    let source = match resolve_date(&catalog, value) {
        Ok(source) => source,
        Err(error) => return unavailable(result, error),
    };
    result["fromCalendarId"] = json!(source.calendar.id);
    if source.day.is_none() {
        return unavailable(result, Failure::Unavailable("conversion_exactness"));
    }
    let axis = match source.calendar.axis(source.lower) {
        Ok(axis) => axis,
        Err(error) => return unavailable(result, error),
    };
    result["axisDay"] = json!(axis.to_string());
    let civil = match target_calendar.civil(axis) {
        Ok(civil) => civil,
        Err(error) => return unavailable(result, error),
    };
    let converted = match target_era {
        Some(era) => DateInput::Era {
            era: era.id.to_owned(),
            year: era.display(civil)?,
            month: Some(civil.month),
            day: Some(civil.day),
        },
        None => DateInput::Civil {
            calendar: target_calendar.id.to_owned(),
            year: civil.year,
            month: Some(civil.month),
            day: Some(civil.day),
        },
    };
    result["found"] = json!(true);
    result["target"] = converted.json()?;
    result["targetCivil"] = date_value(civil);
    result["formatted"] = json!(format_input(&converted));
    Ok(result)
}

// Preserve unavailable table-year results before the shared annotation
// normalizer converts source-semantic failures into validation diagnostics.
// source_value/json already enforce the recursion and node budgets.
pub(super) fn available_dates(catalog: &Catalog<'_>, value: &DateInput) -> Calculation<()> {
    let mut unavailable = None;
    let mut check = |value: &DateInput| match available_dates(catalog, value) {
        Ok(()) => Ok(()),
        Err(Failure::Unavailable(reason)) => {
            unavailable.get_or_insert(reason);
            Ok(())
        }
        Err(error) => Err(error),
    };
    match value {
        DateInput::Civil { .. } | DateInput::Era { .. } => {
            resolve_date(catalog, value)?;
        }
        DateInput::Range {
            calendar,
            lower,
            upper,
        } => {
            if !catalog.calendars.contains_key(calendar.as_str()) {
                return Err(unknown("calendar", calendar));
            }
            for endpoint in [lower, upper].into_iter().flatten() {
                check(endpoint)?;
            }
            if let (Some(lower), Some(upper)) = (lower, upper)
                && let (Ok(lower), Ok(upper)) =
                    (resolve_date(catalog, lower), resolve_date(catalog, upper))
                && lower.lower > upper.upper
            {
                return Err(Failure::Invalid("date range bounds are reversed".into()));
            }
        }
        DateInput::Approx { bounds, .. } => check(bounds)?,
        DateInput::Conflict { claims } => {
            for claim in claims {
                check(claim)?;
            }
        }
    }
    unavailable.map_or(Ok(()), |reason| Err(Failure::Unavailable(reason)))
}

fn lacks_epoch(catalog: &Catalog<'_>, value: &DateInput) -> bool {
    match value {
        DateInput::Civil { calendar, .. } | DateInput::Range { calendar, .. } => catalog
            .calendars
            .get(calendar.as_str())
            .is_some_and(|calendar| calendar.offset.is_none()),
        DateInput::Era { era, .. } => catalog
            .eras
            .get(era.as_str())
            .is_some_and(|era| catalog.calendars[era.calendar].offset.is_none()),
        _ => false,
    }
}

/// Return only authored anchors inside the inclusive date interval. Omitting
/// `timeline` includes independent timelines; display sorting implies no order
/// or elapsed time between them. `upper` combines two finite certain operands.
pub fn story_times(
    world: &World,
    value: &DateInput,
    upper: Option<&DateInput>,
    timeline: Option<&str>,
    limit: usize,
) -> Result<Json> {
    if !(1..=1000).contains(&limit) {
        return Err(Error::InvalidInput(
            "chronology limit must be in 1..=1000".into(),
        ));
    }
    let selected = timeline
        .map(|timeline| {
            world
                .story_time(0, Some(timeline), 0)
                .map(|time| time.timeline)
        })
        .transpose()?;
    let catalog = Catalog::parse(world)?;
    let mut result = envelope(world, &catalog);
    result["value"] = value.json()?;
    result["upper"] = upper
        .map(DateInput::json)
        .transpose()?
        .unwrap_or(Json::Null);
    result["timeline"] = json!(selected);
    result["available"] = json!(false);
    result["complete"] = json!(false);
    result["mapping"] = Json::Null;
    result["items"] = json!([]);
    result["count"] = json!(0);
    result["limit"] = json!(limit);
    let mut unavailable_reason = None;
    let mut normalized = Vec::new();
    // Validate every operand before reporting unavailability: a missing table
    // year or isolated calendar must not hide a malformed second operand.
    for input in std::iter::once(value).chain(upper) {
        match available_dates(&catalog, input) {
            Ok(()) => {}
            Err(Failure::Unavailable(reason)) => {
                unavailable_reason.get_or_insert(reason);
                continue;
            }
            Err(error) => return Err(error.error()),
        }
        if lacks_epoch(&catalog, input) {
            unavailable_reason.get_or_insert("no_epoch");
            continue;
        }
        let info = annotation::normalize_input(world, &catalog, input)?;
        if let Some(reason) = info.reason {
            unavailable_reason.get_or_insert(reason);
        } else if info.approximate {
            unavailable_reason.get_or_insert("approximate_only");
        }
        normalized.push(info);
    }
    if let Some(reason) = unavailable_reason {
        return unavailable(result, Failure::Unavailable(reason));
    }
    let mut normalized = normalized.into_iter();
    let left = normalized.next().expect("the first operand was normalized");
    let mut interval = left
        .interval
        .ok_or_else(|| Error::InvalidInput("date operand does not establish an interval".into()))?;
    if interval.basis != "axis" {
        return unavailable(result, Failure::Unavailable("no_epoch"));
    }
    if let Some(right) = normalized.next() {
        let right = right.interval.ok_or_else(|| {
            Error::InvalidInput("upper date operand does not establish an interval".into())
        })?;
        if right.basis != "axis" {
            return unavailable(result, Failure::Unavailable("no_epoch"));
        }
        if interval.lower.is_none()
            || interval.upper.is_none()
            || right.lower.is_none()
            || right.upper.is_none()
        {
            return Err(Error::InvalidInput(
                "two-operand story intervals require finite bounds".into(),
            ));
        }
        interval.upper = right.upper;
        if interval.lower > interval.upper {
            return Err(Error::InvalidInput(
                "story interval bounds are reversed".into(),
            ));
        }
    }
    let mut anchors: Vec<_> = catalog
        .anchors
        .iter()
        .filter(|anchor| {
            selected
                .as_ref()
                .is_none_or(|timeline| anchor.time.timeline == *timeline)
                && interval.lower.is_none_or(|lower| anchor.axis >= lower)
                && interval.upper.is_none_or(|upper| anchor.axis <= upper)
        })
        .collect();
    if anchors.len() > limit {
        return Err(Error::InvalidInput(
            "story anchor matches exceed limit; increase limit for complete results".into(),
        ));
    }
    anchors.sort_by(|left, right| left.time.cmp(&right.time));
    let mut source = citation(world, catalog.owner);
    source["field"] = json!("chronology.anchors");
    let items: Vec<_> = anchors
        .iter()
        .map(|anchor| {
            json!({
                "axisDay":anchor.axis.to_string(),"storyTime":anchor.time,
                "anchorIds":anchor.ids,"provenance":anchor.provenance,
                "citation":source,
            })
        })
        .collect();
    result["available"] = json!(true);
    result["complete"] = json!(true);
    result["found"] = json!(!items.is_empty());
    result["mapping"] = json!(match items.len() {
        0 => "none",
        1 => "unique",
        _ => "ambiguous",
    });
    result["reason"] = if items.is_empty() {
        json!("no_match")
    } else {
        Json::Null
    };
    result["normalized"] = json!({
        "basis":"axis","lowerDay":interval.lower.map(|value|value.to_string()),
        "upperDay":interval.upper.map(|value|value.to_string()),
    });
    result["count"] = json!(items.len());
    result["items"] = json!(items);
    Ok(result)
}
