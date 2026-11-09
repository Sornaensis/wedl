//! Explicit causal edges and advisory gaps, admitted at one author horizon.
//! Shared location, cast or plot membership never creates a causal edge.
use crate::{
    Error, Result, description,
    model::{Record, StoryTime, World},
    reader::casefold::fold,
};
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_ADMITTED_EVENTS: usize = 16_384;
const MAX_ADMITTED_EDGES: usize = 65_536;

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}
fn limit_argument(limit: usize) -> Result<()> {
    if (1..=1000).contains(&limit) {
        Ok(())
    } else {
        Err(invalid("causal navigation limit must be from 1 to 1000"))
    }
}
fn bounded(count: usize, limit: usize, kind: &str) -> Result<()> {
    if count > limit {
        Err(invalid(format!(
            "causal navigation exceeds {limit} {kind}; refusing an incomplete result"
        )))
    } else {
        Ok(())
    }
}
fn point(record: &Record, default_timeline: &str, declared: &BTreeSet<&str>) -> Result<StoryTime> {
    let point = StoryTime::from_yaml(
        record
            .get("time")
            .ok_or_else(|| Error::Validation("causal event requires time".into()))?,
        default_timeline,
    )?;
    if !declared.contains(point.timeline.as_str()) {
        return Err(invalid(format!("unknown timeline {:?}", point.timeline)));
    }
    Ok(point)
}
fn references<'a>(record: &'a Record, field: &str) -> Result<&'a [Yaml]> {
    match record.get(field) {
        None | Some(Yaml::Null) => Ok(&[]),
        Some(Yaml::Sequence(values)) => Ok(values),
        _ => Err(Error::Validation(format!("{field} must be an array"))),
    }
}
fn reference_id(value: &Yaml) -> Result<&str> {
    value
        .as_str()
        .ok_or_else(|| Error::Validation("causal navigation requires exact record IDs".into()))
}
fn contains_reference(record: &Record, field: &str, target: &str) -> Result<bool> {
    let mut found = false;
    for value in references(record, field)? {
        found |= reference_id(value)? == target;
    }
    Ok(found)
}
fn named(record: &Record) -> Json {
    json!({"id":record.id,"kind":record.kind,"title":record.title})
}
fn citation(world: &World, record: &Record, section: &str) -> Json {
    json!({"entityId":record.id,"sourcePath":record.source_path,
        "blobOid":record.blob_oid,"revision":world.revision,
        "fingerprint":world.fingerprint,"section":section})
}
fn admitted_events<'a>(
    world: &'a World,
    at: &StoryTime,
) -> Result<BTreeMap<&'a str, (&'a Record, StoryTime)>> {
    // Read immutable timeline declarations once. Looking up the world record
    // for every event would turn the bounded inventory into quadratic work.
    let config = world.world_record()?;
    let default_timeline = config.string("default_timeline").unwrap_or("main");
    let declared: BTreeSet<_> = config
        .list("timelines")
        .into_iter()
        .flatten()
        .filter_map(|value| value.get("id").and_then(Yaml::as_str))
        .collect();
    let mut events = BTreeMap::new();
    for record in world
        .records
        .values()
        .filter(|record| record.kind == "event" && record.status() == "canonical")
    {
        if !description::author_record_visible(world, record, at)? {
            continue;
        }
        let time = point(record, default_timeline, &declared)?;
        if time.not_after(at) {
            if events.len() == MAX_ADMITTED_EVENTS {
                return Err(invalid(
                    "causal navigation source exceeds 16384 admitted events",
                ));
            }
            events.insert(record.id.as_str(), (record, time));
        }
    }
    Ok(events)
}
fn location<'a>(world: &'a World, event: &Record, at: &StoryTime) -> Result<Option<&'a Record>> {
    let Some(id) = event.string("location") else {
        return Ok(None);
    };
    let Some(record) = world.records.get(id) else {
        return Ok(None);
    };
    Ok(
        (record.kind == "location" && description::author_record_visible(world, record, at)?)
            .then_some(record),
    )
}

struct Size(usize);
impl Write for Size {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self.0.saturating_add(bytes.len());
        if self.0 > MAX_BYTES {
            return Err(std::io::Error::other("navigation byte bound exceeded"));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn complete_size(value: &impl serde::Serialize) -> Result<()> {
    serde_json::to_writer(Size(0), value)
        .map_err(|_| invalid("complete causal navigation result exceeds 8 MiB"))
}

/// Authored-link gaps across the admitted horizon. These are reading aids,
/// not validation errors, inferred links, or authorization to change source.
pub fn continuity_advisories(world: &World, at: &StoryTime, limit: usize) -> Result<Vec<Json>> {
    limit_argument(limit)?;
    world.story_time(at.tick, Some(&at.timeline), at.order)?;
    let events = admitted_events(world, at)?;
    advisories_from_events(world, at, limit, &events)
}
fn advisories_from_events(
    world: &World,
    at: &StoryTime,
    limit: usize,
    events: &BTreeMap<&str, (&Record, StoryTime)>,
) -> Result<Vec<Json>> {
    let mut advisories = Vec::new();
    for record in world
        .records
        .values()
        .filter(|record| record.kind == "story-point" && record.status() == "canonical")
    {
        if !description::author_record_visible(world, record, at)? {
            continue;
        }
        for (index, outcome) in references(record, "outcome_events")?.iter().enumerate() {
            let Some((event, _)) = events.get(reference_id(outcome)?) else {
                continue;
            };
            if contains_reference(event, "related_story_points", &record.id)? {
                continue;
            }
            bounded(advisories.len() + 1, limit, "continuity advisories")?;
            advisories.push((
                "WDL-ADVISORY-001",
                fold(&record.title),
                record.id.clone(),
                fold(&event.title),
                event.id.clone(),
                json!({"code":"WDL-ADVISORY-001","kind":"plot-link-asymmetry",
                    "storyPoint":named(record),"event":named(event),
                    "message":"The story point declares this outcome event, but the event does not name the story point as related.",
                    "citation":citation(world,record,&format!("outcome_events[{index}]"))}),
            ));
        }
    }
    for (event, event_at) in events.values() {
        let Some(event_location) = location(world, event, at)? else {
            continue;
        };
        for (index, cause) in references(event, "causes")?.iter().enumerate() {
            let Some((cause, cause_at)) = events.get(reference_id(cause)?) else {
                continue;
            };
            // Invalid source is refused by Service. Direct readers still never
            // present a non-earlier edge as an authored continuity advisory.
            if !cause_at.not_after(event_at) || cause_at == event_at {
                continue;
            }
            let Some(cause_location) = location(world, cause, at)? else {
                continue;
            };
            if cause_location.id == event_location.id {
                continue;
            }
            bounded(advisories.len() + 1, limit, "continuity advisories")?;
            advisories.push((
                "WDL-ADVISORY-002",
                fold(&cause.title),
                cause.id.clone(),
                fold(&event.title),
                event.id.clone(),
                json!({"code":"WDL-ADVISORY-002","kind":"cross-location-causality",
                    "cause":named(cause),"effect":named(event),
                    "causeLocation":named(cause_location),"effectLocation":named(event_location),
                    "message":"This authored causal edge crosses locations; record any needed handoff explicitly if it matters to the story.",
                    "citation":citation(world,event,&format!("causes[{index}]"))}),
            ));
        }
    }
    advisories.sort_by(|a, b| (a.0, &a.1, &a.2, &a.3, &a.4).cmp(&(b.0, &b.1, &b.2, &b.3, &b.4)));
    let result: Vec<_> = advisories
        .into_iter()
        .map(|(_, _, _, _, _, row)| row)
        .collect();
    complete_size(&result)?;
    Ok(result)
}

/// Trace explicit canonical causes in either direction. `both` follows the
/// whole connected component, including siblings reached through a shared cause.
pub fn causal(
    world: &World,
    event: &str,
    at: &StoryTime,
    direction: &str,
    limit: usize,
) -> Result<Json> {
    limit_argument(limit)?;
    if !["upstream", "downstream", "both"].contains(&direction) {
        return Err(invalid(
            "causal direction must be upstream, downstream or both",
        ));
    }
    world.story_time(at.tick, Some(&at.timeline), at.order)?;
    // Exact IDs remain authoritative. Name/alias resolution happens only after
    // admission, so an unrelated hidden alias cannot make a visible name
    // ambiguous or put a withheld ID into a resolution error.
    let exact = world.records.get(event);
    if let Some(root) = exact {
        if root.kind != "event" {
            return Err(invalid("causal expects an event"));
        }
        if !description::author_record_visible(world, root, at)? {
            return Err(Error::NotFound("causal event is unavailable".into()));
        }
        if root.status() != "canonical" {
            return Err(invalid("causal requires a canonical focus event"));
        }
    }
    let events = admitted_events(world, at)?;
    let unavailable = || Error::NotFound("causal event is unavailable at this horizon".into());
    let root = if let Some(root) = exact {
        events
            .get(root.id.as_str())
            .map(|(record, _)| *record)
            .ok_or_else(unavailable)?
    } else {
        let folded = event.to_lowercase();
        let mut matches = events.values().filter(|(record, _)| {
            record.title.to_lowercase() == folded
                || record.list("aliases").is_some_and(|aliases| {
                    aliases
                        .iter()
                        .filter_map(Yaml::as_str)
                        .any(|alias| alias.to_lowercase() == folded)
                })
        });
        match (matches.next(), matches.next()) {
            (Some((record, _)), None) => *record,
            (None, _) => return Err(unavailable()),
            _ => {
                return Err(Error::Ambiguous(
                    "ambiguous causal event reference; use an exact ID".into(),
                ));
            }
        }
    };
    let mut upstream: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut downstream: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut source_indexes = BTreeMap::new();
    for (id, (record, event_at)) in &events {
        for (index, cause) in references(record, "causes")?.iter().enumerate() {
            let cause = reference_id(cause)?;
            let Some((_, cause_at)) = events.get(cause) else {
                continue;
            };
            if !cause_at.not_after(event_at) || cause_at == event_at {
                return Err(Error::Validation(
                    "causal edges require a strictly earlier event on the same timeline".into(),
                ));
            }
            if source_indexes.insert((cause, *id), index).is_some() {
                return Err(Error::Validation("causal edges must be unique".into()));
            }
            if source_indexes.len() > MAX_ADMITTED_EDGES {
                return Err(invalid(
                    "causal navigation source exceeds 65536 admitted edges",
                ));
            }
            upstream.entry(id).or_default().insert(cause);
            downstream.entry(cause).or_default().insert(id);
        }
    }
    let mut selected = BTreeSet::from([root.id.as_str()]);
    let mut pending = vec![root.id.as_str()];
    while let Some(current) = pending.pop() {
        for neighbors in [
            matches!(direction, "upstream" | "both")
                .then(|| upstream.get(current))
                .flatten(),
            matches!(direction, "downstream" | "both")
                .then(|| downstream.get(current))
                .flatten(),
        ]
        .into_iter()
        .flatten()
        {
            for neighbor in neighbors {
                if selected.insert(*neighbor) {
                    bounded(selected.len(), limit, "nodes")?;
                    pending.push(*neighbor);
                }
            }
        }
    }
    let mut ordered: Vec<_> = selected.iter().map(|id| &events[id]).collect();
    ordered
        .sort_by(|(a, at), (b, bt)| (at, fold(&a.title), &a.id).cmp(&(bt, fold(&b.title), &b.id)));
    let nodes: Vec<_> = ordered
        .iter()
        .map(|(event, time)| {
            json!({
                "eventId":event.id,"title":event.title,"time":time,"event":named(event),
                "status":event.status(),"citation":citation(world,event,"time/causes")
            })
        })
        .collect();
    let mut edges = Vec::new();
    for ((cause_id, event_id), index) in source_indexes {
        if !selected.contains(cause_id) || !selected.contains(event_id) {
            continue;
        }
        bounded(edges.len() + 1, limit, "edges")?;
        let cause = events[cause_id].0;
        let event = events[event_id].0;
        edges.push((
            fold(&cause.title),
            cause.id.as_str(),
            fold(&event.title),
            event.id.as_str(),
            json!({"causeId":cause.id,"eventId":event.id,
                "cause":named(cause),"effect":named(event),
                "citation":citation(world,event,&format!("causes[{index}]"))}),
        ));
    }
    edges.sort_by(|a, b| (&a.0, a.1, &a.2, a.3).cmp(&(&b.0, b.1, &b.2, b.3)));
    let result = json!({"protocol":crate::CONTRACT,"perspective":"author",
        "revision":world.revision,"fingerprint":world.fingerprint,
        "effectiveTime":at,"timeScope":"as-of","direction":direction,
        "targetEventId":root.id,"focusEvent":named(root),"nodes":nodes,
        "edges":edges.into_iter().map(|(_,_,_,_,row)|row).collect::<Vec<_>>(),
        "continuityAdvisories":advisories_from_events(world,at,limit,&events)?});
    complete_size(&result)?;
    Ok(result)
}
