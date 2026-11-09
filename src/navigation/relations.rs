//! Exact pair participation and requested narrative-thread intersections.
//! Neither a scene link nor a grouping membership grants a record.
use crate::{
    Error, Result, description, hypotheses,
    model::{Record, StoryTime, World},
    validation,
};
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeMap, io::Write};

const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct ByteBudget(usize);

impl Write for ByteBudget {
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

impl ByteBudget {
    pub(crate) fn charge(&mut self, value: &impl serde::Serialize) -> Result<()> {
        serde_json::to_writer(self, value)
            .map_err(|_| invalid("complete navigation result exceeds 8 MiB"))
    }
}

pub(crate) fn complete(value: Json) -> Result<Json> {
    ByteBudget::default().charge(&value)?;
    Ok(value)
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

pub(crate) fn limit_argument(limit: usize) -> Result<()> {
    if (1..=1000).contains(&limit) {
        Ok(())
    } else {
        Err(invalid("navigation limit must be from 1 to 1000"))
    }
}

pub(crate) fn bounded(count: usize, limit: usize) -> Result<()> {
    if count <= limit {
        Ok(())
    } else {
        Err(invalid(format!(
            "navigation result exceeds {limit} items; refusing an incomplete result"
        )))
    }
}

pub(crate) fn point(world: &World, value: Option<&Yaml>) -> Result<StoryTime> {
    let value =
        value.ok_or_else(|| Error::Validation("navigation source requires a time".into()))?;
    let at = StoryTime::from_yaml(value, &world.default_timeline())?;
    world.story_time(at.tick, Some(&at.timeline), at.order)
}

pub(crate) fn citation(world: &World, record: &Record, section: &str) -> Json {
    json!({"entityId":record.id,"sourcePath":record.source_path,"blobOid":record.blob_oid,
        "revision":world.revision,"fingerprint":world.fingerprint,"section":section})
}

pub(crate) fn envelope(world: &World, at: Option<&StoryTime>, limit: usize) -> Json {
    json!({"protocol":crate::CONTRACT,"perspective":"author","revision":world.revision,
        "fingerprint":world.fingerprint,"effectiveTime":at,
        "timeScope":if at.is_some() {"as-of"} else {"all-time"},"complete":true,"limit":limit})
}

/// Settled status and independent owner scope precede membership or link reads.
/// Dated identity is admitted from its authored start, not inferred from prose.
pub(crate) fn settled_visible(
    world: &World,
    record: &Record,
    at: Option<&StoryTime>,
) -> Result<bool> {
    if !hypotheses::is_adoptable_canonical_record(record) {
        return Ok(false);
    }
    visible(world, record, at)
}

pub(crate) fn visible(world: &World, record: &Record, at: Option<&StoryTime>) -> Result<bool> {
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
        "knowledge" | "relationship" => {
            for transition in record.list("transitions").into_iter().flatten() {
                if point(world, transition.get("time"))?.not_after(at) {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        "organization" | "parentage" | "union" | "affiliation" | "legacy" | "tenure" | "claim"
        | "vital-history" => {
            let applicability = record
                .get("initialization")
                .and_then(|value| value.get("applicability"));
            Ok(point(
                world,
                applicability.and_then(|value| value.get("point").or_else(|| value.get("first"))),
            )?
            .not_after(at))
        }
        "assertion" => Ok(record.status() == "canonical"
            && crate::lore::validity(
                record.get("valid").ok_or_else(|| {
                    Error::Validation("assertion navigation requires validity".into())
                })?,
                Some(world),
                "valid",
            )?
            .applies(Some(at))),
        "overlay" => match record.get("valid").filter(|value| !value.is_null()) {
            Some(valid) => Ok(point(world, valid.get("start"))?.not_after(at)
                && at.not_after(&point(world, valid.get("end"))?)),
            None => Ok(true),
        },
        _ => Ok(true),
    }
}

pub(crate) fn named(record: &Record) -> Json {
    json!({"id":record.id,"kind":record.kind,"title":record.title})
}

fn character<'a>(world: &'a World, id: &str, at: &StoryTime) -> Result<&'a Record> {
    if !validation::valid_id_for_kind(id, "character") {
        return Err(invalid("interactions require exact character IDs"));
    }
    let unavailable = || Error::NotFound("interaction character is unavailable".into());
    let record = world.records.get(id).ok_or_else(unavailable)?;
    if record.kind != "character" || !settled_visible(world, record, Some(at))? {
        return Err(unavailable());
    }
    Ok(record)
}

/// Canonical events must explicitly list both characters. Multiple authored
/// roles, including an omitted role, remain in source order for each character.
pub fn interactions(
    world: &World,
    first: &str,
    second: &str,
    at: &StoryTime,
    limit: usize,
) -> Result<Json> {
    limit_argument(limit)?;
    world.story_time(at.tick, Some(&at.timeline), at.order)?;
    if first == second {
        return Err(invalid("interactions require two distinct characters"));
    }
    let first = character(world, first, at)?;
    let second = character(world, second, at)?;
    let mut items = Vec::new();
    let mut bytes = ByteBudget::default();
    for event in world
        .records
        .values()
        .filter(|record| record.kind == "event" && record.status() == "canonical")
    {
        if !settled_visible(world, event, Some(at))? {
            continue;
        }
        let mut roles: BTreeMap<&str, Vec<Option<&str>>> = BTreeMap::new();
        for participant in event.list("participants").into_iter().flatten() {
            let id = participant.get("character").and_then(Yaml::as_str);
            if let Some(id) = id.filter(|id| *id == first.id || *id == second.id) {
                let role = participant
                    .get("role")
                    .filter(|value| !value.is_null())
                    .map(|value| {
                        value.as_str().ok_or_else(|| {
                            Error::Validation("an interaction role must be text".into())
                        })
                    })
                    .transpose()?;
                roles.entry(id).or_default().push(role);
            }
        }
        if !roles.contains_key(first.id.as_str()) || !roles.contains_key(second.id.as_str()) {
            continue;
        }
        bounded(items.len() + 1, limit)?;
        let time = point(world, event.get("time"))?;
        let location = event
            .string("location")
            .and_then(|id| world.records.get(id))
            .filter(|record| record.kind == "location")
            .map(|record| visible(world, record, Some(at)).map(|visible| visible.then_some(record)))
            .transpose()?
            .flatten();
        let item = json!({"eventId":event.id,"title":event.title,"time":time,
            "locationId":location.map(|record| &record.id),"location":location.map(named),
            "roles":roles,"citation":citation(world,event,"participants")});
        bytes.charge(&item)?;
        items.push((time, event.id.as_str(), item));
    }
    items.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
    let mut result = envelope(world, Some(at), limit);
    result["first"] = named(first);
    result["second"] = named(second);
    result["count"] = json!(items.len());
    result["items"] = json!(
        items
            .into_iter()
            .map(|(_, _, item)| item)
            .collect::<Vec<_>>()
    );
    complete(result)
}

fn selections(world: &World, records: &[String], threads: &[String]) -> Result<()> {
    if records.is_empty()
        || records.len() > 256
        || records.windows(2).any(|pair| pair[0] >= pair[1])
        || records.iter().any(|id| !validation::valid_record_id(id))
    {
        return Err(invalid(
            "memberships require 1 to 256 sorted unique exact record IDs",
        ));
    }
    if threads.is_empty()
        || threads.len() > 32
        || threads.windows(2).any(|pair| pair[0] >= pair[1])
        || threads
            .iter()
            .any(|id| !crate::core_validation::thread_id(id))
    {
        return Err(invalid(
            "memberships require 1 to 32 sorted unique exact thread IDs",
        ));
    }
    let declared = world.world_record()?.list("threads");
    if threads.iter().any(|id| {
        !declared.is_some_and(|values| {
            values
                .iter()
                .any(|value| value.get("id").and_then(Yaml::as_str) == Some(id.as_str()))
        })
    }) {
        return Err(invalid(
            "membership threads must be declared exact thread IDs",
        ));
    }
    Ok(())
}

/// A requested record is returned only when independently available and a
/// member of at least one selected thread. All unselected memberships stay out.
pub fn thread_memberships(
    world: &World,
    record_ids: &[String],
    thread_ids: &[String],
    at: Option<&StoryTime>,
    limit: usize,
) -> Result<Json> {
    limit_argument(limit)?;
    selections(world, record_ids, thread_ids)?;
    if let Some(at) = at {
        world.story_time(at.tick, Some(&at.timeline), at.order)?;
    }
    let mut records = Vec::new();
    for id in record_ids {
        let Some(record) = world.records.get(id) else {
            continue;
        };
        if !settled_visible(world, record, at)? {
            continue;
        }
        // Iterate the validated selector, so malformed ordering or incidental
        // duplicate source membership cannot disclose an unselected thread.
        let selected: Vec<_> = thread_ids
            .iter()
            .filter(|id| {
                record.list("threads").is_some_and(|values| {
                    values
                        .iter()
                        .any(|value| value.as_str() == Some(id.as_str()))
                })
            })
            .collect();
        if selected.is_empty() {
            continue;
        }
        bounded(records.len() + 1, limit)?;
        records.push(json!({"recordId":record.id,"threadIds":selected}));
    }
    let mut result = envelope(world, at, limit);
    result["selectedThreadIds"] = json!(thread_ids);
    result["count"] = json!(records.len());
    result["records"] = json!(records);
    complete(result)
}
