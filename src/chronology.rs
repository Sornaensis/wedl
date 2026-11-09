//! Exact authored calendar days, era display and explicit StoryTime anchors.
//! Calendar arithmetic follows the useful cycle/table semantics in the pinned
//! Python `src/wedl/chronology.py`; authored annotations retain their uncertainty.
//! Timelines are independent: no interpolation or ordering across timelines.
use crate::{
    Error, Result,
    model::{Record, StoryTime, World},
    validation::Diagnostic,
};
use serde_json::{Value as Json, json};
use serde_yaml::{Mapping, Value as Yaml};
use std::collections::{BTreeMap, BTreeSet};

const SCOPE: &str = "author-all-time-chronology-source";
const MAX_DECLARATIONS: usize = 500;

mod annotation;
mod date_query;
pub use annotation::{
    AnnotationFilter, AnnotationQuery, DateInput, DatePredicate, EraMatchMode, annotations,
    compare_annotations, search_annotations,
};
pub use date_query::{DateTarget, convert_date, format_date, story_times};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct CivilDate {
    pub year: i64,
    pub month: u32,
    pub day: u32,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}
fn closed<'a>(
    value: &'a Yaml,
    required: &[&str],
    optional: &[&str],
    name: &str,
) -> Result<&'a Mapping> {
    let Yaml::Mapping(map) = value else {
        return Err(invalid(format!("{name} must be a mapping")));
    };
    if required.iter().any(|key| !map.contains_key(*key))
        || map.keys().any(|key| !matches!(key, Yaml::String(key) if required.contains(&key.as_str()) || optional.contains(&key.as_str()) || key.starts_with("x-")))
    {
        return Err(invalid(format!("{name} has missing or unsupported fields")));
    }
    Ok(map)
}
fn text<'a>(value: &'a Yaml, name: &str) -> Result<&'a str> {
    match value {
        Yaml::String(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(invalid(format!("{name} must be a nonblank string"))),
    }
}
fn integer(value: &Yaml, name: &str) -> Result<i64> {
    match value {
        Yaml::Number(value) => value
            .as_i64()
            .ok_or_else(|| invalid(format!("{name} must be a signed i64 integer"))),
        _ => Err(invalid(format!("{name} must be a signed i64 integer"))),
    }
}
fn small(value: &Yaml, name: &str, maximum: i64) -> Result<u32> {
    let number = integer(value, name)?;
    if !(1..=maximum).contains(&number) {
        return Err(invalid(format!("{name} must be in 1..={maximum}")));
    }
    Ok(number as u32)
}
fn array<'a>(value: &'a Yaml, name: &str, maximum: usize) -> Result<&'a [Yaml]> {
    match value {
        Yaml::Sequence(values) if values.len() <= maximum => Ok(values),
        _ => Err(invalid(format!(
            "{name} must be an array of at most {maximum} entries"
        ))),
    }
}
fn strings(value: &Yaml, name: &str, nonempty: bool) -> Result<Vec<String>> {
    let values = array(value, name, MAX_DECLARATIONS)?;
    if nonempty && values.is_empty() {
        return Err(invalid(format!("{name} requires at least one string")));
    }
    values
        .iter()
        .map(|v| text(v, name).map(str::to_owned))
        .collect()
}
fn identifier<'a>(value: &'a Yaml, prefix: &str) -> Result<&'a str> {
    let value = text(value, "chronology identifier")?;
    let valid = value
        .strip_prefix(prefix)
        .and_then(|v| v.strip_prefix('_'))
        .is_some_and(|suffix| {
            suffix.len() == 26
                && suffix
                    .bytes()
                    .all(|b| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&b.to_ascii_uppercase()))
        });
    if !valid {
        return Err(invalid(format!(
            "expected a {prefix}_ identifier with 26 Crockford characters"
        )));
    }
    Ok(value)
}
fn local_date(value: &Yaml, name: &str) -> Result<CivilDate> {
    let map = closed(value, &["year", "month", "day"], &[], name)?;
    Ok(CivilDate {
        year: integer(&map["year"], "year")?,
        month: small(&map["month"], "month", 64)?,
        day: small(&map["day"], "day", 4096)?,
    })
}

#[derive(Clone)]
struct Month {
    number: u32,
    days: u32,
}
fn month(value: &Yaml) -> Result<Month> {
    let map = closed(value, &["number", "days"], &["label"], "month")?;
    if let Some(value) = map.get("label") {
        text(value, "month label")?;
    }
    Ok(Month {
        number: small(&map["number"], "month number", 64)?,
        days: small(&map["days"], "month days", 4096)?,
    })
}
struct Layout {
    months: Vec<Month>,
    prefixes: Vec<i128>,
    days: i128,
}
fn layout(base: &[Month], changes: &[&Yaml], cycle: bool) -> Result<Layout> {
    let mut months: BTreeMap<_, _> = base.iter().cloned().map(|v| (v.number, v)).collect();
    let mut seen = BTreeSet::new();
    for change in changes {
        let is_extra = change.get("intercalary_month").is_some();
        let required = match (cycle, is_extra) {
            (true, true) => vec!["residue", "intercalary_month"],
            (true, false) => vec!["residue", "target_month", "delta_days"],
            (false, true) => vec!["intercalary_month"],
            (false, false) => vec!["target_month", "delta_days"],
        };
        let fields = closed(change, &required, &[], "calendar override")?;
        if is_extra {
            let item = month(&fields["intercalary_month"])?;
            if months.contains_key(&item.number) || !seen.insert(item.number) {
                return Err(invalid(
                    "intercalary month must have a new unique month number",
                ));
            }
            months.insert(item.number, item);
        } else {
            let target = small(&fields["target_month"], "target_month", 64)?;
            if !seen.insert(target) || !base.iter().any(|item| item.number == target) {
                return Err(invalid("month delta requires a unique existing base month"));
            }
            let delta = integer(&fields["delta_days"], "delta_days")?;
            let item = months.get_mut(&target).expect("base month exists");
            let days = i128::from(item.days) + i128::from(delta);
            if !(1..=4096).contains(&days) {
                return Err(invalid("effective month length must be in 1..=4096"));
            }
            item.days = days as u32;
        }
    }
    let mut prefixes = Vec::new();
    let mut days = 0;
    for item in months.values() {
        prefixes.push(days);
        days += i128::from(item.days);
    }
    Ok(Layout {
        months: months.into_values().collect(),
        prefixes,
        days,
    })
}

#[derive(Debug)]
enum Failure {
    Invalid(String),
    Unavailable(&'static str),
}
type Calculation<T> = std::result::Result<T, Failure>;
impl Failure {
    fn error(self) -> Error {
        match self {
            Self::Invalid(message) => Error::InvalidInput(message),
            Self::Unavailable(reason) => {
                Error::InvalidInput(format!("calendar date unavailable: {reason}"))
            }
        }
    }
}
fn overflow() -> Failure {
    Failure::Invalid("chronology arithmetic overflows signed i64".into())
}
fn signed(value: i128) -> Calculation<i64> {
    i64::try_from(value).map_err(|_| overflow())
}

struct Calendar<'a> {
    id: &'a str,
    label: &'a str,
    source: &'a Yaml,
    layouts: Vec<Layout>,
    prefixes: Vec<i128>,
    total: i128,
    table_years: Option<Vec<i64>>,
    offset: Option<i128>,
}
impl<'a> Calendar<'a> {
    fn parse(value: &'a Yaml) -> Result<Self> {
        let map = closed(
            value,
            &["id", "label", "months", "rule"],
            &["epoch"],
            "calendar",
        )?;
        let id = identifier(&map["id"], "calendar")?;
        let label = text(&map["label"], "calendar label")?;
        let raw_months = array(&map["months"], "months", 64)?;
        if raw_months.is_empty() {
            return Err(invalid("calendar requires at least one month"));
        }
        let months: Vec<_> = raw_months.iter().map(month).collect::<Result<_>>()?;
        if months
            .windows(2)
            .any(|pair| pair[0].number >= pair[1].number)
        {
            return Err(invalid(
                "calendar month numbers must be unique and ascending",
            ));
        }
        let mut layouts = Vec::new();
        let mut table_years = None;
        let rule = &map["rule"];
        match rule.get("kind").and_then(|value| {
            if let Yaml::String(value) = value {
                Some(value.as_str())
            } else {
                None
            }
        }) {
            Some("cycle") => {
                let fields = closed(rule, &["kind", "period", "overrides"], &[], "cycle rule")?;
                let period = small(&fields["period"], "cycle period", 500)? as usize;
                let overrides = array(&fields["overrides"], "cycle overrides", MAX_DECLARATIONS)?;
                if months.len() + overrides.len() > MAX_DECLARATIONS {
                    return Err(invalid("calendar mechanics exceed 500 entries"));
                }
                let mut grouped = vec![Vec::new(); period];
                for item in overrides {
                    let residue = item
                        .get("residue")
                        .ok_or_else(|| invalid("cycle override requires residue"))?;
                    let residue = integer(residue, "cycle residue")?;
                    if residue < 0 || residue >= period as i64 {
                        return Err(invalid("cycle residue must be in 0..period"));
                    }
                    grouped[residue as usize].push(item);
                }
                for group in grouped {
                    layouts.push(layout(&months, &group, true)?);
                }
            }
            Some("table") => {
                let fields = closed(rule, &["kind", "years"], &[], "table rule")?;
                let rows = array(&fields["years"], "table years", MAX_DECLARATIONS)?;
                if rows.is_empty() {
                    return Err(invalid("table calendar requires at least one year"));
                }
                let mut years = Vec::new();
                let mut entries = months.len() + rows.len();
                for row in rows {
                    let fields = closed(row, &["year", "overrides"], &[], "table year")?;
                    let year = integer(&fields["year"], "table year")?;
                    if years.last().is_some_and(|last| *last >= year) {
                        return Err(invalid("table years must be unique and ascending"));
                    }
                    years.push(year);
                    let changes = array(&fields["overrides"], "table overrides", MAX_DECLARATIONS)?;
                    entries += changes.len();
                    if entries > MAX_DECLARATIONS {
                        return Err(invalid("calendar mechanics exceed 500 entries"));
                    }
                    layouts.push(layout(&months, &changes.iter().collect::<Vec<_>>(), false)?);
                }
                table_years = Some(years);
            }
            _ => return Err(invalid("calendar rule kind must be cycle or table")),
        }
        let mut total = 0;
        let prefixes = layouts
            .iter()
            .map(|layout| {
                let result = total;
                total += layout.days;
                result
            })
            .collect();
        let mut calendar = Self {
            id,
            label,
            source: value,
            layouts,
            prefixes,
            total,
            table_years,
            offset: None,
        };
        if let Some(epoch) = map
            .get("epoch")
            .filter(|value| !matches!(value, Yaml::Null))
        {
            let fields = closed(epoch, &["civil", "axis_day"], &[], "calendar epoch")?;
            let date = local_date(&fields["civil"], "epoch civil date")?;
            let day = integer(&fields["axis_day"], "epoch axis_day")?;
            let ordinal = calendar
                .ordinal(date)
                .map_err(|error| invalid(format!("invalid calendar epoch: {}", error.error())))?;
            calendar.offset = Some(
                i128::from(day)
                    .checked_sub(ordinal)
                    .ok_or_else(|| invalid("epoch offset overflow"))?,
            );
        }
        Ok(calendar)
    }
    fn row(&self, year: i64) -> Calculation<usize> {
        match &self.table_years {
            Some(years) => years
                .binary_search(&year)
                .map_err(|_| Failure::Unavailable("table_gap")),
            None => Ok(year.rem_euclid(self.layouts.len() as i64) as usize),
        }
    }
    fn ordinal(&self, date: CivilDate) -> Calculation<i128> {
        let row = self.row(date.year)?;
        let layout = &self.layouts[row];
        let index = layout
            .months
            .iter()
            .position(|item| item.number == date.month)
            .ok_or_else(|| {
                Failure::Invalid("unknown month in the selected calendar year".into())
            })?;
        if date.day == 0 || date.day > layout.months[index].days {
            return Err(Failure::Invalid(
                "day is outside the selected calendar month".into(),
            ));
        }
        let cycles = if self.table_years.is_none() {
            i128::from(date.year).div_euclid(self.layouts.len() as i128)
        } else {
            0
        };
        cycles
            .checked_mul(self.total)
            .and_then(|v| v.checked_add(self.prefixes[row]))
            .and_then(|v| v.checked_add(layout.prefixes[index]))
            .and_then(|v| v.checked_add(i128::from(date.day) - 1))
            .ok_or_else(overflow)
    }
    fn axis(&self, date: CivilDate) -> Calculation<i64> {
        let ordinal = self.ordinal(date)?;
        let offset = self.offset.ok_or(Failure::Unavailable("no_epoch"))?;
        signed(ordinal.checked_add(offset).ok_or_else(overflow)?)
    }
    fn civil(&self, axis: i64) -> Calculation<CivilDate> {
        let offset = self.offset.ok_or(Failure::Unavailable("no_epoch"))?;
        let ordinal = i128::from(axis).checked_sub(offset).ok_or_else(overflow)?;
        let (within, cycles) = if self.table_years.is_some() {
            if ordinal < 0 || ordinal >= self.total {
                return Err(Failure::Unavailable("disconnected_table"));
            }
            (ordinal, 0)
        } else {
            (
                ordinal.rem_euclid(self.total),
                ordinal.div_euclid(self.total),
            )
        };
        let row = self.prefixes.partition_point(|value| *value <= within) - 1;
        let layout = &self.layouts[row];
        let year = if let Some(years) = &self.table_years {
            years[row]
        } else {
            signed(
                cycles
                    .checked_mul(self.layouts.len() as i128)
                    .and_then(|v| v.checked_add(row as i128))
                    .ok_or_else(overflow)?,
            )?
        };
        let in_year = within - self.prefixes[row];
        let index = layout.prefixes.partition_point(|value| *value <= in_year) - 1;
        Ok(CivilDate {
            year,
            month: layout.months[index].number,
            day: (in_year - layout.prefixes[index] + 1) as u32,
        })
    }
}

struct Era<'a> {
    id: &'a str,
    calendar: &'a str,
    source: &'a Yaml,
    zero: bool,
    display_year: i64,
    machine_year: i64,
    bounds: Option<(CivilDate, CivilDate)>,
}
impl<'a> Era<'a> {
    fn parse(value: &'a Yaml, calendars: &BTreeMap<&str, Calendar<'_>>) -> Result<Self> {
        let map = closed(
            value,
            &[
                "id",
                "calendar_id",
                "label",
                "aliases",
                "display_year_zero",
                "display_epoch",
                "provenance",
            ],
            &["bounds"],
            "era",
        )?;
        let id = identifier(&map["id"], "era")?;
        let calendar_id = text(&map["calendar_id"], "era calendar_id")?;
        let calendar = calendars
            .get(calendar_id)
            .ok_or_else(|| invalid("era calendar_id must match an exact declared calendar ID"))?;
        text(&map["label"], "era label")?;
        strings(&map["aliases"], "era aliases", false)?;
        strings(&map["provenance"], "era provenance", true)?;
        let Yaml::Bool(zero) = &map["display_year_zero"] else {
            return Err(invalid("display_year_zero must be boolean"));
        };
        let epoch = closed(
            &map["display_epoch"],
            &["display_year", "machine_year"],
            &[],
            "era display_epoch",
        )?;
        let display_year = integer(&epoch["display_year"], "display_year")?;
        let machine_year = integer(&epoch["machine_year"], "machine_year")?;
        if !zero && display_year == 0 {
            return Err(invalid(
                "zero-less era cannot have display year zero at its epoch",
            ));
        }
        let bounds = if let Some(bounds) = map.get("bounds") {
            let fields = closed(bounds, &["lower", "upper"], &[], "era bounds")?;
            let lower = local_date(&fields["lower"], "era lower bound")?;
            let upper = local_date(&fields["upper"], "era upper bound")?;
            for endpoint in [lower, upper] {
                calendar
                    .ordinal(endpoint)
                    .map_err(|error| invalid(format!("invalid era bound: {}", error.error())))?;
            }
            if lower > upper {
                return Err(invalid("era lower bound must not be after its upper bound"));
            }
            Some((lower, upper))
        } else {
            None
        };
        Ok(Self {
            id,
            calendar: calendar_id,
            source: value,
            zero: *zero,
            display_year,
            machine_year,
            bounds,
        })
    }
    fn civil_year(&self, year: i64) -> Calculation<i64> {
        if !self.zero && year == 0 {
            return Err(Failure::Invalid("era cannot use display year zero".into()));
        }
        let ordinal = i128::from(year) - i128::from(!self.zero && year > 0);
        let epoch = i128::from(self.display_year) - i128::from(!self.zero && self.display_year > 0);
        signed(
            i128::from(self.machine_year)
                .checked_add(ordinal)
                .and_then(|value| value.checked_sub(epoch))
                .ok_or_else(overflow)?,
        )
    }

    fn contains_interval(&self, lower: CivilDate, upper: CivilDate) -> Calculation<()> {
        if self.bounds.is_some_and(|(lo, hi)| lower < lo || upper > hi) {
            Err(Failure::Invalid(
                "the complete era date interval must fit its inclusive bounds".into(),
            ))
        } else {
            Ok(())
        }
    }

    fn display(&self, date: CivilDate) -> Result<i64> {
        if self
            .bounds
            .is_some_and(|(lower, upper)| date < lower || date > upper)
        {
            return Err(Error::InvalidInput(
                "date is outside the selected era's inclusive bounds".into(),
            ));
        }
        let epoch = i128::from(self.display_year) - i128::from(!self.zero && self.display_year > 0);
        let ordinal = epoch
            .checked_add(i128::from(date.year))
            .and_then(|v| v.checked_sub(i128::from(self.machine_year)))
            .ok_or_else(|| overflow().error())?;
        let value = if !self.zero && ordinal >= 0 {
            ordinal.checked_add(1).ok_or_else(|| overflow().error())?
        } else {
            ordinal
        };
        signed(value).map_err(Failure::error)
    }
}

/// Resolve authored precision locally before projecting to any signed day axis.
/// Formatting can therefore retain a valid year even when its axis would overflow.
fn partial_dates(
    calendar: &Calendar<'_>,
    year: i64,
    month: Option<u32>,
    day: Option<u32>,
) -> Calculation<(CivilDate, CivilDate)> {
    if day.is_some() && month.is_none() {
        return Err(Failure::Invalid("date day requires a month".into()));
    }
    let row = calendar.row(year)?;
    let months = &calendar.layouts[row].months;
    let (first, last) = match month {
        Some(number) => {
            let item = months
                .iter()
                .find(|item| item.number == number)
                .ok_or_else(|| {
                    Failure::Invalid("date month is absent from this calendar year".into())
                })?;
            (item, item)
        }
        None => (
            months.first().expect("validated calendar has months"),
            months.last().expect("validated calendar has months"),
        ),
    };
    let lower = CivilDate {
        year,
        month: first.number,
        day: day.unwrap_or(1),
    };
    let upper = CivilDate {
        year,
        month: last.number,
        day: day.unwrap_or(last.days),
    };
    calendar.ordinal(lower)?;
    calendar.ordinal(upper)?;
    Ok((lower, upper))
}

struct Anchor {
    axis: i64,
    time: StoryTime,
    ids: Vec<String>,
    provenance: Vec<String>,
}
struct Catalog<'a> {
    owner: &'a Record,
    calendars: BTreeMap<&'a str, Calendar<'a>>,
    eras: BTreeMap<&'a str, Era<'a>>,
    anchors: Vec<Anchor>,
}
impl<'a> Catalog<'a> {
    fn parse(world: &'a World) -> Result<Self> {
        let owner = world.world_record()?;
        let mut result = Self {
            owner,
            calendars: BTreeMap::new(),
            eras: BTreeMap::new(),
            anchors: Vec::new(),
        };
        let Some(source) = owner.get("chronology") else {
            return Ok(result);
        };
        let fields = closed(
            source,
            &["calendars", "eras", "anchors"],
            &[],
            "world chronology",
        )?;
        let mut ids = BTreeSet::new();
        for value in array(&fields["calendars"], "calendars", MAX_DECLARATIONS)? {
            let calendar = Calendar::parse(value)?;
            if !ids.insert(calendar.id.to_ascii_uppercase()) {
                return Err(invalid("duplicate calendar identifier"));
            }
            result.calendars.insert(calendar.id, calendar);
        }
        for value in array(&fields["eras"], "eras", MAX_DECLARATIONS)? {
            let era = Era::parse(value, &result.calendars)?;
            if !ids.insert(era.id.to_ascii_uppercase()) {
                return Err(invalid("duplicate era identifier"));
            }
            result.eras.insert(era.id, era);
        }
        let mut groups: BTreeMap<(String, i64), Anchor> = BTreeMap::new();
        let mut reverse: BTreeMap<StoryTime, i64> = BTreeMap::new();
        for value in array(&fields["anchors"], "anchors", MAX_DECLARATIONS)? {
            let fields = closed(
                value,
                &["id", "axis_day", "story_time", "provenance"],
                &[],
                "chronology anchor",
            )?;
            let id = identifier(&fields["id"], "chronology")?;
            if !ids.insert(id.to_ascii_uppercase()) {
                return Err(invalid("duplicate chronology anchor identifier"));
            }
            let axis = integer(&fields["axis_day"], "anchor axis_day")?;
            let time_fields = closed(
                &fields["story_time"],
                &["timeline", "tick", "order"],
                &[],
                "anchor story_time",
            )?;
            let timeline = text(&time_fields["timeline"], "anchor timeline")?;
            let tick = integer(&time_fields["tick"], "anchor tick")?;
            let order = i32::try_from(integer(&time_fields["order"], "anchor order")?)
                .map_err(|_| invalid("anchor order must be signed i32"))?;
            let time = world
                .story_time(tick, Some(timeline), order)
                .map_err(|error| invalid(error.to_string()))?;
            let provenance = strings(&fields["provenance"], "anchor provenance", true)?;
            if reverse
                .insert(time.clone(), axis)
                .is_some_and(|old| old != axis)
            {
                return Err(invalid("same StoryTime cannot have different anchor days"));
            }
            let key = (time.timeline.clone(), axis);
            if let Some(anchor) = groups.get_mut(&key) {
                if anchor.time != time {
                    return Err(invalid(
                        "same axis day cannot have different StoryTimes within a timeline",
                    ));
                }
                anchor.ids.push(id.to_owned());
                anchor.provenance.extend(provenance);
            } else {
                groups.insert(
                    key,
                    Anchor {
                        axis,
                        time,
                        ids: vec![id.to_owned()],
                        provenance,
                    },
                );
            }
        }
        result.anchors = groups.into_values().collect();
        for pair in result.anchors.windows(2) {
            if pair[0].time.timeline == pair[1].time.timeline
                && (pair[0].time.tick, pair[0].time.order)
                    >= (pair[1].time.tick, pair[1].time.order)
            {
                return Err(invalid(
                    "chronology anchors cross StoryTime order within a timeline",
                ));
            }
        }
        for anchor in &mut result.anchors {
            anchor.ids.sort();
            anchor.provenance.sort();
            anchor.provenance.dedup();
        }
        Ok(result)
    }
    fn calendar(&self, id: &str) -> Result<&Calendar<'a>> {
        self.calendars
            .get(id)
            .ok_or_else(|| Error::NotFound(format!("unknown exact calendar ID {id:?}")))
    }
}

/// Validate calendar declarations, then each record's authored chronology claims.
pub fn validate(world: &World) -> Vec<Diagnostic> {
    match Catalog::parse(world) {
        Ok(catalog) => annotation::validate(world, &catalog),
        Err(error) => {
            let owner = world.world_record().ok();
            vec![Diagnostic {
                code: "WDL-CHRONOLOGY-001".into(),
                message: error.to_string(),
                severity: "error".into(),
                entity_id: owner.map(|v| v.id.clone()),
                path: owner.map(|v| v.source_path.clone()),
                field: Some("chronology".into()),
            }]
        }
    }
}
fn citation(world: &World, owner: &Record) -> Json {
    json!({"entityId":owner.id,"sourcePath":owner.source_path,"blobOid":owner.blob_oid,"revision":world.revision,"fingerprint":world.fingerprint,"section":"chronology"})
}
fn envelope(world: &World, catalog: &Catalog<'_>) -> Json {
    json!({"protocol":crate::CONTRACT,"perspective":"author","scope":SCOPE,"citation":citation(world,catalog.owner)})
}
fn date_value(date: CivilDate) -> Json {
    json!({"year":date.year.to_string(),"month":date.month.to_string(),"day":date.day.to_string()})
}
// Extensions are opaque source metadata, not part of the executable catalogue.
// Skip their values before traversal so unknown YAML types cannot affect reads.
fn declaration_value(value: &Yaml) -> Result<Json> {
    match value {
        Yaml::Mapping(map) => {
            let mut result = serde_json::Map::new();
            for (key, value) in map {
                let Yaml::String(key) = key else {
                    return Err(invalid("chronology declaration keys must be strings"));
                };
                if !key.starts_with("x-") {
                    result.insert(key.clone(), declaration_value(value)?);
                }
            }
            Ok(Json::Object(result))
        }
        Yaml::Sequence(values) => Ok(Json::Array(
            values
                .iter()
                .map(declaration_value)
                .collect::<Result<_>>()?,
        )),
        _ => crate::service::yaml_json(value),
    }
}
fn formatted(id: &str, date: CivilDate) -> String {
    format!("{id}:{}-{}-{}", date.year, date.month, date.day)
}
fn unavailable(mut value: Json, failure: Failure) -> Result<Json> {
    match failure {
        Failure::Invalid(message) => Err(Error::InvalidInput(message)),
        Failure::Unavailable(reason) => {
            value["found"] = json!(false);
            value["reason"] = json!(reason);
            Ok(value)
        }
    }
}

/// Complete catalogue in exact ID order. Declared integers are decimal strings.
pub fn calendars(world: &World, limit: usize) -> Result<Json> {
    if !(1..=1000).contains(&limit) {
        return Err(Error::InvalidInput(
            "chronology limit must be in 1..=1000".into(),
        ));
    }
    let catalog = Catalog::parse(world)?;
    if catalog.calendars.len() > limit {
        return Err(Error::InvalidInput(
            "calendar catalogue exceeds limit; increase limit for complete results".into(),
        ));
    }
    let mut items = Vec::new();
    for calendar in catalog.calendars.values() {
        let eras = catalog
            .eras
            .values()
            .filter(|era| era.calendar == calendar.id)
            .map(|era| declaration_value(era.source))
            .collect::<Result<Vec<_>>>()?;
        let raw = declaration_value(calendar.source)?;
        items.push(json!({"calendarId":calendar.id,"label":calendar.label,"months":raw["months"],"rule":raw["rule"],"epoch":raw["epoch"],"eras":eras,"citation":citation(world,catalog.owner)}));
    }
    let mut result = envelope(world, &catalog);
    result["basis"] = json!("authored-calendar-declarations");
    result["count"] = json!(items.len());
    result["complete"] = json!(true);
    result["limit"] = json!(limit);
    result["items"] = json!(items);
    Ok(result)
}

/// Validate and format a civil day, optionally displaying its year in an era.
pub fn date(world: &World, calendar: &str, value: CivilDate, era: Option<&str>) -> Result<Json> {
    let catalog = Catalog::parse(world)?;
    let calendar = catalog.calendar(calendar)?;
    let mut result = envelope(world, &catalog);
    result["calendarId"] = json!(calendar.id);
    result["date"] = date_value(value);
    if let Err(error) = calendar.ordinal(value) {
        return unavailable(result, error);
    }
    let display = match era {
        None => Json::Null,
        Some(id) => {
            let era = catalog
                .eras
                .get(id)
                .ok_or_else(|| Error::NotFound(format!("unknown exact era ID {id:?}")))?;
            if era.calendar != calendar.id {
                return Err(Error::InvalidInput(
                    "selected era belongs to a different calendar".into(),
                ));
            }
            let year = era.display(value)?;
            let display = CivilDate { year, ..value };
            let mut item = date_value(display);
            item["eraId"] = json!(era.id);
            item["formatted"] = json!(formatted(era.id, display));
            item
        }
    };
    result["found"] = json!(true);
    result["formatted"] = json!(formatted(calendar.id, value));
    result["era"] = display;
    match calendar.axis(value) {
        Ok(axis) => {
            result["axisDay"] = json!(axis.to_string());
            result["axisReason"] = Json::Null;
        }
        Err(Failure::Unavailable(reason)) => {
            result["axisDay"] = Json::Null;
            result["axisReason"] = json!(reason);
        }
        // A locally valid civil date is still displayable beyond the i64 axis.
        // `ordinal` above already rejected invalid months/days or table gaps.
        Err(Failure::Invalid(_)) => {
            result["axisDay"] = Json::Null;
            result["axisReason"] = json!("overflow");
        }
    }
    Ok(result)
}

/// Convert one exact civil day through explicitly authored shared-axis epochs.
pub fn convert(world: &World, from: &str, to: &str, value: CivilDate) -> Result<Json> {
    let catalog = Catalog::parse(world)?;
    let source = catalog.calendar(from)?;
    let target = catalog.calendar(to)?;
    let mut result = envelope(world, &catalog);
    result["fromCalendarId"] = json!(source.id);
    result["toCalendarId"] = json!(target.id);
    result["source"] = date_value(value);
    let axis = match source.axis(value) {
        Ok(axis) => axis,
        Err(error) => return unavailable(result, error),
    };
    result["axisDay"] = json!(axis.to_string());
    let date = match target.civil(axis) {
        Ok(date) => date,
        Err(error) => return unavailable(result, error),
    };
    result["found"] = json!(true);
    result["target"] = date_value(date);
    result["formatted"] = json!(formatted(target.id, date));
    Ok(result)
}

fn anchor_result(result: &mut Json, anchor: &Anchor) {
    result["found"] = json!(true);
    result["axisDay"] = json!(anchor.axis.to_string());
    result["storyTime"] = json!(anchor.time);
    result["anchorIds"] = json!(anchor.ids);
    result["provenance"] = json!(anchor.provenance);
}
fn anchor_envelope(world: &World, catalog: &Catalog<'_>, calendar: &str, timeline: &str) -> Json {
    let mut result = envelope(world, catalog);
    result["calendarId"] = json!(calendar);
    result["timeline"] = json!(timeline);
    result["anchorIds"] = json!([]);
    result["provenance"] = json!([]);
    result
}
/// Exact anchor match on the selected (default: world's default) timeline.
pub fn to_story(
    world: &World,
    calendar: &str,
    value: CivilDate,
    timeline: Option<&str>,
) -> Result<Json> {
    let selected = world.story_time(0, timeline, 0)?.timeline;
    let catalog = Catalog::parse(world)?;
    let calendar = catalog.calendar(calendar)?;
    let mut result = anchor_envelope(world, &catalog, calendar.id, &selected);
    result["date"] = date_value(value);
    let axis = match calendar.axis(value) {
        Ok(axis) => axis,
        Err(error) => return unavailable(result, error),
    };
    result["axisDay"] = json!(axis.to_string());
    if let Some(anchor) = catalog
        .anchors
        .iter()
        .find(|anchor| anchor.time.timeline == selected && anchor.axis == axis)
    {
        anchor_result(&mut result, anchor);
    } else {
        result["found"] = json!(false);
        result["reason"] = json!("no_match");
    }
    Ok(result)
}
/// Exact StoryTime lookup, including order; tick gaps never imply elapsed days.
pub fn from_story(world: &World, calendar: &str, at: &StoryTime) -> Result<Json> {
    world.story_time(at.tick, Some(&at.timeline), at.order)?;
    let catalog = Catalog::parse(world)?;
    let calendar = catalog.calendar(calendar)?;
    let mut result = anchor_envelope(world, &catalog, calendar.id, &at.timeline);
    result["storyTime"] = json!(at);
    if let Some(anchor) = catalog.anchors.iter().find(|anchor| &anchor.time == at) {
        let date = match calendar.civil(anchor.axis) {
            Ok(date) => date,
            Err(error) => return unavailable(result, error),
        };
        anchor_result(&mut result, anchor);
        result["date"] = date_value(date);
        result["formatted"] = json!(formatted(calendar.id, date));
    } else {
        result["found"] = json!(false);
        result["reason"] = json!("no_match");
    }
    Ok(result)
}
