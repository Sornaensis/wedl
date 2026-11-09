//! Informational local-author consequences of explicit native event source.
//!
//! Event-local differences compare two folds at the event's exact coordinate.
//! The exclusion view retains the event and every source owner, suppressing only
//! its effects and explicitly caused native transitions at that same coordinate.
//! It is never validated, serialized as source, cached, or used for an apply.
use crate::{
    Error, Result,
    consequence_semantics::{
        Time,
        projection::{self, Projection, Subject},
    },
    consequences::Plan,
    model::{Record, StoryTime, World},
    service, validation,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
};

mod grouped;

pub const MAX_ITEMS: usize = 1000;
pub const MAX_CHARACTERS: usize = 1_000_000;
pub const MAX_REQUEST_BYTES: usize = 524_288;
pub const MAX_REPORT_BYTES: usize = 8 * 1024 * 1024;
const MAX_GROUPS: usize = 100;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventRequest {
    pub expected_fingerprint: String,
    pub event_id: String,
    pub at: Time,
    pub max_items: usize,
    pub max_characters: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupedRequest {
    pub plan: Plan,
    pub at: Time,
    pub focus_event_ids: Vec<String>,
    pub max_items: usize,
    pub max_characters: usize,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn limits(items: usize, characters: usize) -> Result<()> {
    if !(1..=MAX_ITEMS).contains(&items) || !(256..=MAX_CHARACTERS).contains(&characters) {
        return Err(invalid(
            "consequences require maxItems 1..1000 and maxCharacters 256..1000000",
        ));
    }
    Ok(())
}

fn exact_id(id: &str) -> Result<()> {
    if id.is_empty() || id.len() > 256 || id.trim() != id {
        return Err(invalid(
            "consequence identities require exact nonblank record IDs of at most 256 bytes",
        ));
    }
    Ok(())
}

fn decode<T: DeserializeOwned>(raw: &str) -> Result<T> {
    if raw.len() > MAX_REQUEST_BYTES {
        return Err(invalid("consequence request exceeds 524288 bytes"));
    }
    // Reject duplicate keys, including escaped spellings inside nested plans.
    let mut decoder = serde_json::Deserializer::from_str(raw);
    let _: Yaml = Deserialize::deserialize(&mut decoder)
        .map_err(|_| invalid("invalid or duplicate consequence request JSON"))?;
    decoder
        .end()
        .map_err(|_| invalid("invalid consequence request JSON"))?;
    serde_json::from_str(raw)
        .map_err(|error| invalid(format!("invalid consequence request: {error}")))
}

fn event_request(request: &EventRequest) -> Result<StoryTime> {
    limits(request.max_items, request.max_characters)?;
    exact_id(&request.event_id)?;
    if request.expected_fingerprint.len() != 64
        || !request
            .expected_fingerprint
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid(
            "expectedFingerprint requires an exact lowercase SHA-256 fingerprint",
        ));
    }
    request.at.point()
}

pub fn decode_event(raw: &str) -> Result<EventRequest> {
    let request = decode(raw)?;
    event_request(&request)?;
    Ok(request)
}

fn grouped_request(request: &GroupedRequest) -> Result<StoryTime> {
    limits(request.max_items, request.max_characters)?;
    crate::consequences::validate_plan(&request.plan)?;
    if request.focus_event_ids.len() > MAX_GROUPS {
        return Err(invalid(
            "grouped consequences permit at most 100 explicit focus events",
        ));
    }
    let mut ids = BTreeSet::new();
    for id in &request.focus_event_ids {
        exact_id(id)?;
        if !ids.insert(id) {
            return Err(invalid("focusEventIds contains a duplicate exact ID"));
        }
    }
    request.at.point()
}

pub fn decode_grouped(raw: &str) -> Result<GroupedRequest> {
    let request = decode(raw)?;
    grouped_request(&request)?;
    Ok(request)
}

struct Encoded(Vec<u8>);
impl Write for Encoded {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > MAX_REPORT_BYTES {
            return Err(std::io::Error::other(
                "complete consequence report exceeds 8 MiB",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn encoded(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes = Encoded(Vec::new());
    serde_json::to_writer(&mut bytes, value)
        .map_err(|_| invalid("complete consequence report exceeds 8 MiB"))?;
    Ok(bytes.0)
}
fn digest(domain: &[u8], value: &impl Serialize) -> Result<String> {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update(encoded(value)?);
    Ok(hex::encode(hash.finalize()))
}
fn finish(
    mut report: Json,
    request: &impl Serialize,
    count: usize,
    max_items: usize,
    max_characters: usize,
) -> Result<Json> {
    if count > max_items {
        return Err(invalid(format!(
            "complete consequence report requires {count} items; maxItems is {max_items}"
        )));
    }
    report["bounds"] = json!({"maxItems":max_items,"maxCharacters":max_characters,
        "maxReportBytes":MAX_REPORT_BYTES,"maxGroups":MAX_GROUPS,"items":count});
    report["requestSha256"] = digest(b"wedl-rs/consequences-request-v1\0", request)?.into();
    report["reportSha256"] = digest(b"wedl-rs/consequences-report-v1\0", &report)?.into();
    let bytes = encoded(&report)?;
    let characters = std::str::from_utf8(&bytes)
        .expect("JSON is UTF-8")
        .chars()
        .count();
    if characters > max_characters {
        return Err(invalid(format!(
            "complete consequence report requires {characters} characters; maxCharacters is {max_characters}"
        )));
    }
    Ok(report)
}

fn point(world: &World, raw: &Yaml) -> Result<StoryTime> {
    let point = StoryTime::from_yaml(raw, &world.default_timeline())?;
    world.story_time(point.tick, Some(&point.timeline), point.order)
}
fn event_time(world: &World, record: &Record) -> Result<StoryTime> {
    point(
        world,
        record
            .get("time")
            .ok_or_else(|| invalid("event has no exact authored time"))?,
    )
}
fn reference(record: &Record) -> Json {
    json!({"id":record.id,"kind":record.kind,"title":record.title})
}
fn citation(
    world: &World,
    record: &Record,
    section: &str,
    ordinal: Option<usize>,
    member: Option<&str>,
    at: Option<&StoryTime>,
) -> Json {
    json!({"entityId":record.id,"sourcePath":record.source_path,"blobOid":record.blob_oid,
        "revision":world.revision,"fingerprint":world.fingerprint,"section":section,
        "sourceOrdinal":ordinal,"memberId":member,"time":at})
}
fn members(record: &Record) -> Option<(&str, &[Yaml])> {
    let (section, raw) = match record.kind.as_str() {
        "knowledge" | "relationship" => ("transitions", record.get("transitions")),
        "story-point" => (
            "lifecycle.transitions",
            record
                .get("lifecycle")
                .and_then(|value| value.get("transitions")),
        ),
        _ => return None,
    };
    Some((
        section,
        raw.and_then(Yaml::as_sequence).map_or(&[], Vec::as_slice),
    ))
}
fn latest<'a>(
    world: &World,
    record: &'a Record,
    at: &StoryTime,
) -> Result<Option<(usize, &'a Yaml, StoryTime)>> {
    let mut latest = None;
    if let Some((_, members)) = members(record) {
        for (ordinal, member) in members.iter().enumerate() {
            let time = point(world, &member["time"])?;
            if time.not_after(at)
                && latest
                    .as_ref()
                    .is_none_or(|(index, _, old): &(usize, &Yaml, StoryTime)| {
                        (time.tick, time.order, ordinal) > (old.tick, old.order, *index)
                    })
            {
                latest = Some((ordinal, member, time));
            }
        }
    }
    Ok(latest)
}
fn history_subject(record: &Record) -> Option<Subject> {
    Some(match record.kind.as_str() {
        "knowledge" => Subject::Knowledge {
            knowledge_id: record.id.clone(),
        },
        "relationship" => Subject::Relationship {
            relationship_id: record.id.clone(),
        },
        "story-point" => Subject::StoryPoint {
            story_point_id: record.id.clone(),
        },
        _ => return None,
    })
}

type Ordinals = BTreeMap<(String, String), Vec<usize>>;

fn excluded(world: &World, focus: &Record, at: &StoryTime) -> Result<(World, Ordinals)> {
    let mut excluded = world.clone();
    let mut ordinals = BTreeMap::new();
    excluded
        .records
        .get_mut(&focus.id)
        .expect("focus exists")
        .frontmatter["effects"] = Yaml::Sequence(Vec::new());
    for record in world.records.values() {
        let Some((section, history)) = members(record) else {
            continue;
        };
        let mut kept = Vec::new();
        let mut indexes = Vec::new();
        for (index, member) in history.iter().enumerate() {
            let is_focus = member.get("causing_event").and_then(Yaml::as_str) == Some(&focus.id);
            if is_focus && point(world, &member["time"])? == *at {
                continue;
            }
            kept.push(member.clone());
            indexes.push(index);
        }
        if kept.len() != history.len() {
            let original = excluded
                .records
                .get_mut(&record.id)
                .expect("owner retained");
            if section == "lifecycle.transitions" {
                original.frontmatter["lifecycle"]["transitions"] = Yaml::Sequence(kept);
            } else {
                original.frontmatter["transitions"] = Yaml::Sequence(kept);
            }
            ordinals.insert((record.id.clone(), section.into()), indexes);
        }
    }
    Ok((excluded, ordinals))
}

/// Replace compacted runtime indexes with original source positions. For
/// lifecycle snapshots the native runtime supplies a broad citation; retain
/// the exact selected lifecycle member plus the actual condition declarations.
fn enrich(
    world: &World,
    projection: &mut Projection,
    at: &StoryTime,
    ordinals: &Ordinals,
) -> Result<()> {
    for (subject, snapshot) in &mut projection.snapshots {
        if let Subject::StoryPoint { story_point_id } = subject {
            let record = &world.records[story_point_id];
            let mut evidence = Vec::new();
            if let Some((index, member, time)) = latest(world, record, at)? {
                let ordinal = ordinals
                    .get(&(record.id.clone(), "lifecycle.transitions".into()))
                    .map_or(index, |indexes| indexes[index]);
                evidence.push(citation(
                    world,
                    record,
                    "lifecycle.transitions",
                    Some(ordinal),
                    member["id"].as_str(),
                    Some(&time),
                ));
            } else {
                evidence.push(citation(
                    world,
                    record,
                    "lifecycle.initial_state",
                    None,
                    None,
                    None,
                ));
            }
            for section in ["requires", "when"] {
                if record.get(section).is_some() {
                    evidence.push(citation(world, record, section, None, None, None));
                }
            }
            snapshot.citations = evidence;
            continue;
        }
        for evidence in &mut snapshot.citations {
            let Some(id) = evidence["entityId"].as_str().map(str::to_owned) else {
                continue;
            };
            let Some(record) = world.records.get(&id) else {
                continue;
            };
            let Some(section) = evidence["section"].as_str().map(str::to_owned) else {
                continue;
            };
            let Some((name, index)) = section.strip_suffix(']').and_then(|s| s.rsplit_once('['))
            else {
                continue;
            };
            let Ok(index) = index.parse::<usize>() else {
                continue;
            };
            let original = ordinals
                .get(&(id, name.into()))
                .map_or(index, |indexes| indexes[index]);
            let raw = if name == "effects" {
                record.list("effects").and_then(|values| values.get(index))
            } else if name == "transitions" {
                record
                    .list("transitions")
                    .and_then(|values| values.get(index))
            } else {
                None
            };
            evidence["section"] = name.into();
            evidence["sourceOrdinal"] = original.into();
            if let Some(raw) = raw {
                evidence["memberId"] = json!(raw.get("id").and_then(Yaml::as_str));
                evidence["time"] = json!(if name == "effects" {
                    event_time(world, record)?
                } else {
                    point(world, &raw["time"])?
                });
            }
        }
    }
    Ok(())
}

fn effects(world: &World, focus: &Record, at: &StoryTime) -> Result<Vec<Json>> {
    focus.list("effects").into_iter().flatten().enumerate().map(|(index, effect)| {
        Ok(json!({"effect":service::yaml_json(effect)?,"effectRaw":serde_json::to_string(effect)?,
            "citation":citation(world,focus,"effects",Some(index),effect.get("id").and_then(Yaml::as_str),Some(at))}))
    }).collect()
}

fn caused(
    world: &World,
    focus: &Record,
    event_at: &StoryTime,
    at: &StoryTime,
) -> Result<Vec<Json>> {
    let mut rows = Vec::new();
    for record in world.records.values() {
        let Some((section, history)) = members(record) else {
            continue;
        };
        if !projection::admitted(world, record, at)? {
            continue;
        }
        for (index, member) in history.iter().enumerate() {
            if member.get("causing_event").and_then(Yaml::as_str) != Some(&focus.id) {
                continue;
            }
            let time = point(world, &member["time"])?;
            if !time.not_after(at) {
                continue;
            }
            let allowed: &[&str] = match record.kind.as_str() {
                "knowledge" => &[
                    "id",
                    "time",
                    "causing_event",
                    "note",
                    "state",
                    "confidence",
                    "acquisition",
                    "source_entity",
                ],
                "relationship" => &[
                    "id",
                    "time",
                    "causing_event",
                    "note",
                    "relationship_status",
                    "metrics",
                    "facets",
                ],
                _ => &["id", "time", "causing_event", "note", "state"],
            };
            let selected = Yaml::Mapping(
                allowed
                    .iter()
                    .filter_map(|key| {
                        member
                            .get(*key)
                            .map(|value| (Yaml::String((*key).into()), value.clone()))
                    })
                    .collect(),
            );
            let mut transition = service::yaml_json(&selected)?;
            transition["time"] = json!(time);
            rows.push(((time.tick,time.order,record.kind.clone(),record.id.clone(),index),
                json!({"recordId":record.id,"kind":record.kind,"section":section,"sourceOrdinal":index,
                "transitionId":member["id"],"transition":transition,"transitionRaw":serde_json::to_string(&selected)?,
                "time":time,"atEventTime":time==*event_at,
                "citation":citation(world,record,section,Some(index),member["id"].as_str(),Some(&time))})));
        }
    }
    rows.sort_by(|(left, _), (right, _)| left.cmp(right));
    Ok(rows.into_iter().map(|(_, value)| value).collect())
}

fn successors(
    world: &World,
    focus: &Record,
    event_at: &StoryTime,
    at: &StoryTime,
) -> Result<Vec<Json>> {
    let mut rows = Vec::new();
    for record in world
        .records
        .values()
        .filter(|record| record.kind == "event")
    {
        if !projection::admitted(world, record, at)? {
            continue;
        }
        let time = event_time(world, record)?;
        if (time.tick, time.order) <= (event_at.tick, event_at.order) {
            continue;
        }
        if let Some(index) = record
            .list("causes")
            .into_iter()
            .flatten()
            .position(|id| id.as_str() == Some(&focus.id))
        {
            rows.push((
                (time.tick, time.order, record.id.clone()),
                json!({"event":reference(record),"time":time,
                "citation":citation(world,record,"causes",Some(index),None,Some(&time))}),
            ));
        }
    }
    rows.sort_by(|(left, _), (right, _)| left.cmp(right));
    Ok(rows.into_iter().map(|(_, row)| row).collect())
}

fn outcomes(world: &World, focus: &Record, at: &StoryTime) -> Result<Vec<Json>> {
    let mut rows = Vec::new();
    for target in world
        .records
        .values()
        .filter(|record| matches!(record.kind.as_str(), "story-point" | "scene"))
    {
        if !projection::admitted(world, target, at)? {
            continue;
        }
        let from_target = target
            .list("outcome_events")
            .into_iter()
            .flatten()
            .position(|id| id.as_str() == Some(&focus.id));
        let from_event = (target.kind == "story-point")
            .then(|| {
                focus
                    .list("related_story_points")
                    .into_iter()
                    .flatten()
                    .position(|id| id.as_str() == Some(&target.id))
            })
            .flatten();
        if from_target.is_none() && from_event.is_none() {
            continue;
        }
        let mut citations = Vec::new();
        if let Some(index) = from_target {
            citations.push(citation(
                world,
                target,
                "outcome_events",
                Some(index),
                None,
                None,
            ));
        }
        if let Some(index) = from_event {
            citations.push(citation(
                world,
                focus,
                "related_story_points",
                Some(index),
                None,
                None,
            ));
        }
        rows.push(json!({"target":reference(target),"targetLinked":from_target.is_some(),"eventLinked":from_event.is_some(),
            "reciprocal":from_target.is_some()&&from_event.is_some(),"citations":citations}));
    }
    rows.sort_by_cached_key(|row| {
        (
            row["target"]["kind"].as_str().unwrap().to_owned(),
            row["target"]["id"].as_str().unwrap().to_owned(),
        )
    });
    Ok(rows)
}

fn current(
    world: &World,
    focus: &Record,
    subject: &Subject,
    projection: &Projection,
    event_at: &StoryTime,
    at: &StoryTime,
) -> Result<Json> {
    let snapshot = projection::boundary(world, projection, subject, at)?;
    let mut supersession = "unknown";
    let mut superseding = Vec::new();
    match subject {
        Subject::State { .. } => {
            for writer in snapshot["citations"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|citation| citation["section"] == "effects")
            {
                let Some(id) = writer["entityId"].as_str() else {
                    continue;
                };
                if id == focus.id {
                    supersession = "unchanged";
                } else if let Some(record) = world.records.get(id) {
                    let time = event_time(world, record)?;
                    if (time.tick, time.order) > (event_at.tick, event_at.order) {
                        supersession = "superseded";
                        superseding.push(writer.clone());
                    }
                }
            }
        }
        Subject::Knowledge { .. } | Subject::Relationship { .. } | Subject::StoryPoint { .. } => {
            if snapshot["presence"] == "present" {
                let record = &world.records[subject.owner()];
                if let Some((index, member, time)) = latest(world, record, at)? {
                    let (section, history) = members(record).expect("history subject");
                    let mut most_recent = None;
                    for (index, member) in history.iter().enumerate() {
                        if member.get("causing_event").and_then(Yaml::as_str) != Some(&focus.id) {
                            continue;
                        }
                        let point = point(world, &member["time"])?;
                        if point.not_after(at) {
                            most_recent = most_recent.max(Some((point.tick, point.order, index)));
                        }
                    }
                    if let Some(previous) = most_recent {
                        if member.get("causing_event").and_then(Yaml::as_str) == Some(&focus.id) {
                            supersession = "unchanged";
                        } else if (time.tick, time.order, index) > previous {
                            supersession = "superseded";
                            superseding.push(citation(
                                world,
                                record,
                                section,
                                Some(index),
                                member["id"].as_str(),
                                Some(&time),
                            ));
                        }
                    }
                }
            }
        }
        Subject::Outcome { .. } => {
            if snapshot["presence"] == "present" {
                supersession = "unchanged";
            }
        }
    }
    Ok(
        json!({"subject":subject,"at":at,"snapshot":snapshot,"supersession":supersession,"supersedingCitations":superseding}),
    )
}

/// Read from one validated captured snapshot. The expected fingerprint binds
/// the capture; current-disk fences, when needed, belong to the service caller.
pub fn read(world: &World, request: &EventRequest) -> Result<Json> {
    let at = event_request(request)?;
    if request.expected_fingerprint != world.fingerprint {
        return Err(Error::Conflict(
            "source fingerprint differs from event consequence request".into(),
        ));
    }
    projection::preflight(world)?;
    if !validation::validate_world(world).valid {
        return Err(Error::Validation(
            "event consequences require valid original source".into(),
        ));
    }
    projection::horizon(world, &at)?;
    let focus = world
        .records
        .get(&request.event_id)
        .filter(|record| record.kind == "event")
        .ok_or_else(|| Error::NotFound("exact event is unavailable".into()))?;
    if !projection::admitted(world, focus, &at)? {
        return Err(Error::NotFound(
            "exact event is unavailable at this horizon".into(),
        ));
    }
    let event_at = event_time(world, focus)?;
    let (excluded, ordinals) = excluded(world, focus, &event_at)?;
    let mut before = projection::project(&excluded, &event_at)?;
    let mut after = projection::project(world, &event_at)?;
    enrich(&excluded, &mut before, &event_at, &ordinals)?;
    enrich(world, &mut after, &event_at, &BTreeMap::new())?;
    let changes =
        projection::compare(&excluded, &before, world, &after, &event_at)?["changes"].clone();
    let mut horizon = projection::project(world, &at)?;
    enrich(world, &mut horizon, &at, &BTreeMap::new())?;
    let effects = effects(world, focus, &event_at)?;
    let caused = caused(world, focus, &event_at, &at)?;
    let successors = successors(world, focus, &event_at, &at)?;
    let outcomes = outcomes(world, focus, &at)?;
    let mut affected = BTreeSet::new();
    for effect in focus.list("effects").into_iter().flatten() {
        affected.insert(Subject::State {
            entity_id: effect["target"].as_str().expect("validated target").into(),
            key: effect["key"].as_str().expect("validated key").into(),
        });
    }
    for row in &caused {
        affected
            .insert(history_subject(&world.records[row["recordId"].as_str().unwrap()]).unwrap());
    }
    let mut advisories = Vec::new();
    for outcome in &outcomes {
        affected.insert(Subject::Outcome {
            event_id: focus.id.clone(),
            target_id: outcome["target"]["id"].as_str().unwrap().into(),
        });
        if outcome["target"]["kind"] == "story-point" && outcome["reciprocal"] == false {
            advisories.push(json!({"code":"CONSEQUENCE-LINK-001","target":outcome["target"],
                "message":"An explicit story-point outcome link has no reciprocal authored link.","citations":outcome["citations"]}));
        }
    }
    let current: Vec<_> = affected
        .iter()
        .map(|subject| current(world, focus, subject, &horizon, &event_at, &at))
        .collect::<Result<_>>()?;
    let count = effects.len()
        + caused.len()
        + successors.len()
        + outcomes.len()
        + changes.as_array().unwrap().len()
        + current.len()
        + advisories.len();
    let report = json!({"protocol":crate::CONTRACT,"operation":"event-consequences","outcome":"ok",
        "scope":"local-author","worldId":world.world_record()?.id,"fingerprint":world.fingerprint,"revision":world.revision,
        "event":reference(focus),"eventTime":event_at,"at":at,
        "comparison":{"mode":"event-local","at":event_at,"eventRetained":true,"sameTimeOtherWritesRetained":true},
        "effects":effects,"changes":changes,"causedTransitions":caused,"outcomes":outcomes,
        "causalSuccessors":successors,"currentAtHorizon":current,"advisories":advisories,
        "complete":true,"validationComplete":false,"writesPerformed":false,"canApply":false,
        "semantics":{"stateScope":"complete-original-author-world","stateReferences":"native-values-with-scalar-entity-effect-pruning",
            "futureEntityTargetPruning":true,"delayedTransitions":"explicit-history-only","causalSuccessors":"direct-explicit-links"},
        "notice":"Exact-time native contribution exclusion and explicit authored history. This read does not infer causation or approve source changes."});
    finish(
        report,
        request,
        count,
        request.max_items,
        request.max_characters,
    )
}

/// Preview explicit source attribution alongside same-horizon semantic values.
pub fn preview(world: &World, request: &GroupedRequest) -> Result<Json> {
    grouped::preview(world, request)
}
