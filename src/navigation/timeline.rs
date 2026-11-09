//! Authored chronology, including noncanonical points and inclusive spans.
//! Coordinates are ordinal; no duration or interpolated cursor is inferred.
use crate::{
    Error, Result, description,
    model::{Record, StoryTime, World},
    narrative,
    reader::casefold::fold,
};
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::io::Write;

const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_LINKS: usize = 16_384;

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}
fn malformed(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}
fn point(world: &World, value: Option<&Yaml>) -> Result<StoryTime> {
    let point = StoryTime::from_yaml(
        value.ok_or_else(|| malformed("navigation requires an exact story coordinate"))?,
        &world.default_timeline(),
    )?;
    world.story_time(point.tick, Some(&point.timeline), point.order)
}
fn optional_point(world: &World, value: Option<&Yaml>) -> Result<Option<StoryTime>> {
    value
        .filter(|value| !value.is_null())
        .map(|value| point(world, Some(value)))
        .transpose()
}
fn list<'a>(value: Option<&'a Yaml>, field: &str) -> Result<&'a [Yaml]> {
    match value {
        None | Some(Yaml::Null) => Ok(&[]),
        Some(Yaml::Sequence(values)) => Ok(values),
        _ => Err(malformed(format!("navigation {field} must be an array"))),
    }
}
fn text<'a>(value: Option<&'a Yaml>, field: &str) -> Result<&'a str> {
    value
        .and_then(Yaml::as_str)
        .ok_or_else(|| malformed(format!("navigation {field} must be a string")))
}
fn citation(world: &World, record: &Record, section: &str) -> Json {
    json!({"entityId":record.id,"sourcePath":record.source_path,
        "blobOid":record.blob_oid,"revision":world.revision,
        "fingerprint":world.fingerprint,"section":section})
}

struct Size(usize);
impl Write for Size {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self.0.saturating_add(bytes.len());
        if self.0 > MAX_BYTES {
            return Err(std::io::Error::other("navigation byte limit exceeded"));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn size(value: &Json) -> Result<usize> {
    let mut count = Size(0);
    serde_json::to_writer(&mut count, value)
        .map_err(|_| invalid("complete timeline result exceeds 8 MiB"))?;
    Ok(count.0)
}
#[derive(Default)]
struct Budget {
    items: usize,
    links: usize,
    bytes: usize,
}
impl Budget {
    fn link(&mut self) -> Result<()> {
        self.links += 1;
        if self.links > MAX_LINKS {
            return Err(invalid(
                "complete timeline result exceeds 16384 named references",
            ));
        }
        Ok(())
    }
    fn row(&mut self, value: &Json, limit: usize) -> Result<()> {
        self.items += 1;
        if self.items > limit {
            return Err(invalid(format!(
                "timeline exceeds {limit} points and spans; refusing an incomplete result"
            )));
        }
        self.bytes = self.bytes.saturating_add(size(value)?);
        if self.bytes > MAX_BYTES {
            return Err(invalid("complete timeline result exceeds 8 MiB"));
        }
        Ok(())
    }
}

// An author chronology can show any authored status, but a link never bypasses
// its target's own labels or time. Story-point identity is timeless; its dated
// transitions are selected separately below.
fn visible(world: &World, record: &Record, at: Option<&StoryTime>) -> Result<bool> {
    if !description::full_author_record_visible(world, record, at)? {
        return Ok(false);
    }
    let Some(at) = at else {
        return Ok(true);
    };
    match record.kind.as_str() {
        "event" => Ok(point(world, record.get("time"))?.not_after(at)),
        "scene" | "conversation" | "environment" => {
            Ok(point(world, record.get("time").and_then(|time| time.get("start")))?.not_after(at))
        }
        _ => Ok(true),
    }
}
fn reference(
    world: &World,
    value: Option<&Yaml>,
    kind: &str,
    at: Option<&StoryTime>,
    budget: &mut Budget,
) -> Result<Json> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(Json::Null);
    };
    let id = text(Some(value), "reference")?;
    let Some(record) = world.records.get(id) else {
        return Ok(Json::Null);
    };
    if record.kind != kind || !visible(world, record, at)? {
        return Ok(Json::Null);
    }
    budget.link()?;
    Ok(json!({"id":record.id,"kind":record.kind,"title":record.title}))
}
fn references(
    world: &World,
    values: Option<&Yaml>,
    kind: &str,
    at: Option<&StoryTime>,
    budget: &mut Budget,
) -> Result<Vec<Json>> {
    let mut result = Vec::new();
    for value in list(values, "references")? {
        let value = reference(world, Some(value), kind, at, budget)?;
        if !value.is_null() {
            result.push(value);
        }
    }
    Ok(result)
}
fn causing_event(
    world: &World,
    value: Option<&Yaml>,
    transition: &StoryTime,
    at: Option<&StoryTime>,
    budget: &mut Budget,
) -> Result<Json> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(Json::Null);
    };
    let Some(event) = world.records.get(text(Some(value), "causing event")?) else {
        return Ok(Json::Null);
    };
    if event.kind != "event"
        || !visible(world, event, at)?
        || !point(world, event.get("time"))?.not_after(transition)
    {
        return Ok(Json::Null);
    }
    reference(world, Some(value), "event", at, budget)
}
fn summary(world: &World, record: &Record, at: Option<&StoryTime>) -> Result<String> {
    if let Some(at) = at {
        if !narrative::author_body_visible(world, record, at)? {
            return Ok(String::new());
        }
    }
    if record.body.len() > MAX_BYTES {
        return Err(invalid("complete timeline result exceeds 8 MiB"));
    }
    Ok(record
        .body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join(" "))
}
fn event_participants(
    world: &World,
    event: &Record,
    at: Option<&StoryTime>,
    budget: &mut Budget,
) -> Result<(Vec<Json>, Vec<Json>)> {
    let mut named = Vec::new();
    let mut native = Vec::new();
    for participant in list(event.get("participants"), "participants")? {
        let mut character =
            reference(world, participant.get("character"), "character", at, budget)?;
        if character.is_null() {
            continue;
        }
        let mut original = json!({"character":character["id"]});
        if let Some(role) = participant.get("role").filter(|value| !value.is_null()) {
            let role = text(Some(role), "participant role")?;
            character["role"] = json!(role);
            original["role"] = json!(role);
        }
        named.push(character);
        native.push(original);
    }
    Ok((named, native))
}
fn span_participants(
    world: &World,
    record: &Record,
    start: &StoryTime,
    presence_end: Option<&StoryTime>,
    at: Option<&StoryTime>,
    budget: &mut Budget,
) -> Result<Vec<Json>> {
    let mut result = Vec::new();
    for (index, participant) in list(record.get("participants"), "participants")?
        .iter()
        .enumerate()
    {
        let from = optional_point(world, participant.get("from"))?.unwrap_or_else(|| start.clone());
        let until =
            optional_point(world, participant.get("until"))?.or_else(|| presence_end.cloned());
        if !start.not_after(&from)
            || presence_end.is_some_and(|end| !from.not_after(end))
            || until.as_ref().is_some_and(|until| {
                !from.not_after(until) || presence_end.is_some_and(|end| !until.not_after(end))
            })
        {
            return Err(malformed(
                "timeline participant interval must lie inside its parent span",
            ));
        }
        if at.is_some_and(|at| !from.not_after(at)) {
            continue;
        }
        let mut character =
            reference(world, participant.get("character"), "character", at, budget)?;
        if character.is_null() {
            continue;
        }
        character["from"] = json!(from);
        character["until"] = json!(until.filter(|end| at.is_none_or(|at| end.not_after(at))));
        if let Some(role) = participant.get("role").filter(|value| !value.is_null()) {
            character["role"] = json!(text(Some(role), "participant role")?);
        }
        if let Some(pov) = participant
            .get("point_of_view")
            .filter(|value| !value.is_null())
        {
            character["pointOfView"] = json!(
                pov.as_bool()
                    .ok_or_else(|| malformed("participant point_of_view must be boolean"))?
            );
        }
        character["citation"] = citation(world, record, &format!("participants[{index}]"));
        result.push(character);
    }
    Ok(result)
}

/// A complete authored chronology on one timeline. `limit` bounds the combined
/// points and spans. Noncanonical events are chronology only, never state.
pub fn timeline(
    world: &World,
    timeline_id: &str,
    at: Option<&StoryTime>,
    limit: usize,
) -> Result<Json> {
    if !(1..=1000).contains(&limit) {
        return Err(invalid("timeline limit must be from 1 to 1000"));
    }
    world.story_time(0, Some(timeline_id), 0)?;
    if let Some(at) = at {
        world.story_time(at.tick, Some(&at.timeline), at.order)?;
        if at.timeline != timeline_id {
            return Err(invalid("timeline and horizon must agree"));
        }
    }
    let declaration = world
        .world_record()?
        .list("timelines")
        .and_then(|values| {
            values
                .iter()
                .find(|value| value.get("id").and_then(Yaml::as_str) == Some(timeline_id))
        })
        .ok_or_else(|| invalid("unknown timeline"))?;
    let mut definition = json!({
        "id":timeline_id,
        "label":declaration.get("label").and_then(Yaml::as_str).unwrap_or(timeline_id)
    });
    if let Some(origin) = declaration.get("origin") {
        let tick = origin
            .get("tick")
            .and_then(Yaml::as_i64)
            .ok_or_else(|| malformed("timeline origin requires an exact signed 64-bit tick"))?;
        definition["origin"] = json!({"tick":tick.to_string(),
            "label":text(origin.get("label"), "origin label")?});
    }

    let mut budget = Budget::default();
    let mut points = Vec::new();
    let mut spans = Vec::new();
    let mut events = Vec::new();
    for record in world.records.values() {
        if !matches!(
            record.kind.as_str(),
            "event" | "story-point" | "scene" | "conversation"
        ) || !visible(world, record, at)?
        {
            continue;
        }
        let name_key = fold(&record.title);
        let own = || json!({"id":record.id,"kind":record.kind,"title":record.title});
        match record.kind.as_str() {
            "event" => {
                let time = point(world, record.get("time"))?;
                if time.timeline != timeline_id {
                    continue;
                }
                let location =
                    reference(world, record.get("location"), "location", at, &mut budget)?;
                let (participants, native_participants) =
                    event_participants(world, record, at, &mut budget)?;
                let row = json!({"kind":"event","at":time,"status":record.status(),
                    "entity":own(),"summary":summary(world,record,at)?,"location":location,
                    "participants":participants,
                    "causes":references(world,record.get("causes"),"event",at,&mut budget)?,
                    "plotThreads":references(world,record.get("related_story_points"),"story-point",at,&mut budget)?,
                    "citation":citation(world,record,"time")});
                budget.row(&row, limit)?;
                if record.status() == "canonical" {
                    events.push((
                        time.clone(),
                        record.id.clone(),
                        json!({
                            "entityId":record.id,"title":record.title,"time":time,
                            "locationId":location.get("id"),"participants":native_participants,
                            "citation":citation(world,record,"time")
                        }),
                    ));
                }
                points.push((
                    time,
                    "event",
                    name_key,
                    record.id.clone(),
                    String::new(),
                    row,
                ));
            }
            "story-point" => {
                let lifecycle = record
                    .get("lifecycle")
                    .ok_or_else(|| malformed("story-point lifecycle is required"))?;
                for (index, transition) in
                    list(lifecycle.get("transitions"), "lifecycle transitions")?
                        .iter()
                        .enumerate()
                {
                    let time = point(world, transition.get("time"))?;
                    if time.timeline != timeline_id || at.is_some_and(|at| !time.not_after(at)) {
                        continue;
                    }
                    let id = text(transition.get("id"), "transition ID")?;
                    let mut requirements = Vec::new();
                    for required in list(record.get("requires"), "requirements")? {
                        let mut target =
                            reference(world, required.get("id"), "story-point", at, &mut budget)?;
                        if !target.is_null() {
                            target["states"] = json!(
                                list(required.get("states"), "required states")?
                                    .iter()
                                    .map(|state| text(Some(state), "required state"))
                                    .collect::<Result<Vec<_>>>()?
                            );
                            requirements.push(target);
                        }
                    }
                    let note = match transition.get("note").filter(|value| !value.is_null()) {
                        Some(value) => text(Some(value), "transition note")?.to_owned(),
                        None => summary(world, record, at)?,
                    };
                    let row = json!({"kind":"plot-transition","at":time,"status":record.status(),
                        "entity":own(),"transitionId":id,
                        "lifecycleState":text(transition.get("state"), "lifecycle state")?,
                        "summary":note,
                        "causingEvent":causing_event(world,transition.get("causing_event"),&time,at,&mut budget)?,
                        "outcomeEvents":references(world,record.get("outcome_events"),"event",at,&mut budget)?,
                        "requirements":requirements,
                        "citation":citation(world,record,&format!("lifecycle.transitions[{index}]"))});
                    budget.row(&row, limit)?;
                    points.push((
                        time,
                        "plot-transition",
                        name_key.clone(),
                        record.id.clone(),
                        id.into(),
                        row,
                    ));
                }
            }
            "scene" | "conversation" => {
                let value = record
                    .get("time")
                    .ok_or_else(|| malformed("timeline span requires time"))?;
                let start = point(world, value.get("start"))?;
                if start.timeline != timeline_id {
                    continue;
                }
                let current = if record.kind == "scene" {
                    Some(point(world, value.get("current"))?)
                } else {
                    None
                };
                let end = optional_point(world, value.get("end"))?;
                if current.as_ref().is_some_and(|time| !start.not_after(time))
                    || end.as_ref().is_some_and(|time| !start.not_after(time))
                    || current
                        .as_ref()
                        .zip(end.as_ref())
                        .is_some_and(|(current, end)| !current.not_after(end))
                {
                    return Err(malformed(
                        "timeline span must satisfy start <= current <= end on one timeline",
                    ));
                }
                let presence_end = end.as_ref().or_else(|| {
                    (record.kind == "scene" && record.status() == "active")
                        .then_some(current.as_ref())
                        .flatten()
                });
                let mut row = json!({"kind":record.kind,"start":start,
                    "end":end.as_ref().filter(|end| at.is_none_or(|at| end.not_after(at))),
                    "status":record.status(),"entity":own(),"summary":summary(world,record,at)?,
                    "location":reference(world,record.get("location"),"location",at,&mut budget)?,
                    "participants":span_participants(world,record,&start,presence_end,at,&mut budget)?,
                    "citation":citation(world,record,"time")});
                if record.kind == "scene" {
                    row["current"] = json!(
                        current
                            .as_ref()
                            .filter(|current| at.is_none_or(|at| current.not_after(at)))
                    );
                    row["conversations"] = json!(references(
                        world,
                        record.get("conversations"),
                        "conversation",
                        at,
                        &mut budget
                    )?);
                    row["plotThreads"] = json!(references(
                        world,
                        record.get("story_points"),
                        "story-point",
                        at,
                        &mut budget
                    )?);
                } else {
                    row["scene"] = reference(world, record.get("scene"), "scene", at, &mut budget)?;
                }
                budget.row(&row, limit)?;
                spans.push((start, record.kind.clone(), name_key, record.id.clone(), row));
            }
            _ => unreachable!(),
        }
    }
    points.sort_by(|a, b| (&a.0, a.1, &a.2, &a.3, &a.4).cmp(&(&b.0, b.1, &b.2, &b.3, &b.4)));
    spans.sort_by(|a, b| (&a.0, &a.1, &a.2, &a.3).cmp(&(&b.0, &b.1, &b.2, &b.3)));
    events.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    let result = json!({"protocol":crate::CONTRACT,"perspective":"author", "revision":world.revision,
        "fingerprint":world.fingerprint,"timeline":timeline_id,"timelineDefinition":definition,
        "effectiveTime":at,"timeScope":if at.is_some(){"as-of"}else{"all-time"},
        "temporalSemantics":{"spacing":"ordinal","durationSemantics":"none","intervalEndpoints":"inclusive"},
        "events":events.into_iter().map(|(_,_,row)|row).collect::<Vec<_>>(),
        "points":points.into_iter().map(|(_,_,_,_,_,row)|row).collect::<Vec<_>>(),
        "spans":spans.into_iter().map(|(_,_,_,_,row)|row).collect::<Vec<_>>()});
    size(&result)?;
    Ok(result)
}
