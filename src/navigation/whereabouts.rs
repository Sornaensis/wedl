//! Bulk author whereabouts from explicit, admitted state and presence evidence.
//! Prominence is relative to the complete cohort, never the requested subset.
use crate::{
    Error, Result, description,
    lore_types::TypeSpec,
    model::{Record, StoryTime, World},
    scene,
    state_schema::{self, StateDefinition, StateType},
};
use serde::Serialize;
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
};

const MAX_COHORT: usize = 1000;
const MAX_EVIDENCE: usize = 10_000;
const MAX_BYTES: usize = 4 * 1024 * 1024;
const ALGORITHM: &str = "wedl-character-importance/v1";

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn malformed(message: &str) -> Error {
    Error::Validation(format!("whereabouts: {message}"))
}

fn point(world: &World, value: Option<&Yaml>) -> Result<StoryTime> {
    let at = StoryTime::from_yaml(
        value.ok_or_else(|| malformed("authored time is required"))?,
        &world.default_timeline(),
    )?;
    world.story_time(at.tick, Some(&at.timeline), at.order)
}

fn list(value: Option<&Yaml>) -> Result<&[Yaml]> {
    match value {
        None | Some(Yaml::Null) => Ok(&[]),
        Some(Yaml::Sequence(values)) => Ok(values),
        _ => Err(malformed("an authored list must be an array")),
    }
}

fn text(value: Option<&Yaml>) -> Result<&str> {
    value
        .and_then(Yaml::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| malformed("an authored reference must be a nonempty string"))
}

/// Endpoints are admitted independently; a dated owner does not grant its links.
fn visible(world: &World, record: &Record, at: &StoryTime) -> Result<bool> {
    if record.kind == "event" && record.status() != "canonical" {
        return Ok(false);
    }
    super::relations::visible(world, record, Some(at))
}

enum Location<'a> {
    Present(&'a Record),
    Absent,
    Withheld,
}

fn supports_location(definition: &StateDefinition) -> bool {
    let Some(profile) = &definition.profile else {
        return definition.value_type == StateType::Entity;
    };
    let TypeSpec::Object { required, optional } = &profile.fields else {
        return false;
    };
    matches!(required.get("entity").or_else(|| optional.get("entity")),
        Some(TypeSpec::Entity { kinds }) if kinds.iter().any(|kind| kind == "location"))
}

/// A bound placement has one operative leaf: the profile's root `entity`
/// field, declared as an entity type admitting locations. Siblings and nested
/// or Ref-shaped literal values never substitute for that exact leaf.
fn location<'a>(
    world: &'a World,
    definition: &StateDefinition,
    value: &Yaml,
    at: &StoryTime,
) -> Result<Location<'a>> {
    let id = if definition.profile.is_some() {
        if !supports_location(definition) {
            return Ok(Location::Absent);
        }
        let Some(reference) = value.get("entity").and_then(Yaml::as_mapping) else {
            return Ok(Location::Absent);
        };
        if reference.len() != 2 || reference.get("kind").and_then(Yaml::as_str) != Some("record") {
            return Ok(Location::Absent);
        }
        let Some(id) = reference.get("id").and_then(Yaml::as_str) else {
            return Ok(Location::Absent);
        };
        id
    } else {
        text(value.get("entity"))?
    };
    let record = world
        .records
        .get(id)
        .ok_or_else(|| malformed("unknown location state endpoint"))?;
    Ok(if record.kind != "location" {
        Location::Absent
    } else if visible(world, record, at)? {
        Location::Present(record)
    } else {
        Location::Withheld
    })
}

fn location_definition(world: &World) -> Result<Option<StateDefinition>> {
    if world
        .world_record()?
        .get("state_keys")
        .and_then(|value| value.get("character"))
        .and_then(|value| value.get("location"))
        .is_none()
    {
        return Ok(None);
    }
    let definition = state_schema::field_definition(world, "character", "location")?;
    Ok(supports_location(&definition).then_some(definition))
}

fn effect_visible(world: &World, owner: &Record, effect: &Yaml, at: &StoryTime) -> Result<bool> {
    let Some(value) = effect.get("value") else {
        return Ok(true);
    };
    let key = text(effect.get("key"))?;
    let definition = state_schema::field_definition(world, &owner.kind, key)?;
    if key == "location" && definition.profile.is_some() && supports_location(&definition) {
        return Ok(!matches!(
            location(world, &definition, value, at)?,
            Location::Withheld
        ));
    }
    if definition.profile.is_some() || definition.value_type != StateType::Entity {
        return Ok(true);
    }
    let id = text(value.get("entity"))?;
    match world.records.get(id) {
        Some(record) => visible(world, record, at),
        None => Err(malformed("unknown effect endpoint")),
    }
}

#[derive(Clone, Copy, Serialize)]
struct Reference<'a> {
    id: &'a str,
    kind: &'a str,
    title: &'a str,
}

impl<'a> From<&'a Record> for Reference<'a> {
    fn from(record: &'a Record) -> Self {
        Self {
            id: &record.id,
            kind: &record.kind,
            title: &record.title,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Citation<'a> {
    entity_id: &'a str,
    source_path: &'a str,
    blob_oid: Option<&'a str>,
    revision: Option<&'a str>,
    fingerprint: &'a str,
    section: String,
}

fn citation<'a>(world: &'a World, record: &'a Record, section: String) -> Citation<'a> {
    Citation {
        entity_id: &record.id,
        source_path: &record.source_path,
        blob_oid: record.blob_oid.as_deref(),
        revision: world.revision.as_deref(),
        fingerprint: &world.fingerprint,
        section,
    }
}

#[derive(Serialize)]
struct Journey<'a> {
    kind: &'static str,
    at: Option<StoryTime>,
    from: Option<Reference<'a>>,
    to: Option<Reference<'a>>,
    event: Option<Reference<'a>>,
    citation: Citation<'a>,
}

struct Character<'a> {
    record: &'a Record,
    current: Option<&'a Record>,
    last_known: Option<&'a Record>,
    journey: Vec<Journey<'a>>,
    raw: [usize; 4],
    neighbors: BTreeSet<&'a str>,
    scenes: Vec<&'a Record>,
}

#[derive(Default)]
struct Evidence(usize);

impl Evidence {
    fn add(&mut self, amount: usize) -> Result<()> {
        self.0 = self.0.saturating_add(amount);
        if self.0 > MAX_EVIDENCE {
            return Err(invalid(
                "whereabouts exceeds 10000 admitted evidence items; refusing an incomplete result",
            ));
        }
        Ok(())
    }
}

fn order(left: &Record, right: &Record) -> std::cmp::Ordering {
    left.title
        .to_lowercase()
        .cmp(&right.title.to_lowercase())
        .then_with(|| left.id.cmp(&right.id))
}

fn importance(raw: &[usize; 4], maxima: &[usize; 4]) -> Json {
    let names = [
        "scenes",
        "pointOfViewScenes",
        "events",
        "relationshipNeighbors",
    ];
    let weights = [40.0, 25.0, 20.0, 15.0];
    let mut counts = serde_json::Map::new();
    let mut normalized = serde_json::Map::new();
    let mut contributions = serde_json::Map::new();
    let mut score = 0.0_f64;
    for index in 0..4 {
        let ratio = if maxima[index] == 0 {
            0.0
        } else {
            (raw[index] as f64).ln_1p() / (maxima[index] as f64).ln_1p()
        };
        let contribution = (weights[index] * ratio * 100.0).round_ties_even() / 100.0;
        counts.insert(names[index].into(), json!(raw[index]));
        normalized.insert(
            names[index].into(),
            json!((ratio * 1_000_000.0).round_ties_even() / 1_000_000.0),
        );
        contributions.insert(names[index].into(), json!(contribution));
        score += contribution;
    }
    json!({"algorithm":ALGORITHM,"score":(score*100.0).round_ties_even()/100.0,
        "raw":counts,"normalized":normalized,"contributions":contributions})
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Entry<'w, 'j> {
    character: Reference<'w>,
    record_status: &'w str,
    role: Option<&'w str>,
    importance: Json,
    presence: &'static str,
    location: Option<Reference<'w>>,
    last_known_location: Option<Reference<'w>>,
    active_scene: Option<Reference<'w>>,
    journey: &'j [Journey<'w>],
}

#[derive(Serialize)]
struct LocationGroup<'a> {
    location: Reference<'a>,
    characters: Vec<Reference<'a>>,
}

#[derive(Serialize)]
struct SceneGroup<'a> {
    scene: Reference<'a>,
    location: Option<Reference<'a>>,
    characters: Vec<Reference<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Output<'w, 'j, 't> {
    protocol: &'static str,
    perspective: &'static str,
    revision: Option<&'w str>,
    fingerprint: &'w str,
    effective_time: &'t StoryTime,
    time_scope: &'static str,
    character_policy: Json,
    importance_policy: Json,
    character_filter: Option<Vec<Reference<'w>>>,
    characters: Vec<Entry<'w, 'j>>,
    locations: Vec<LocationGroup<'w>>,
    active_scenes: Vec<SceneGroup<'w>>,
    offstage_characters: Vec<Reference<'w>>,
    unlocated_characters: Vec<Reference<'w>>,
}

struct Size(usize);
impl Write for Size {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self.0.saturating_add(bytes.len());
        if self.0 > MAX_BYTES {
            return Err(std::io::Error::other("whereabouts output bound exceeded"));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn bounded(count: usize, limit: usize, label: &str) -> Result<()> {
    if count > limit {
        return Err(invalid(format!(
            "whereabouts exceeds {limit} {label}; refusing an incomplete result"
        )));
    }
    Ok(())
}

fn cohort<'a>(
    world: &'a World,
    at: &StoryTime,
    maximum: usize,
) -> Result<BTreeMap<&'a str, Character<'a>>> {
    let mut cohort = BTreeMap::new();
    for record in world.records.values().filter(|record| {
        record.kind == "character" && matches!(record.status(), "canonical" | "retired")
    }) {
        if visible(world, record, at)? {
            bounded(cohort.len() + 1, maximum, "admitted cohort characters")?;
            cohort.insert(
                record.id.as_str(),
                Character {
                    record,
                    current: None,
                    last_known: None,
                    journey: Vec::new(),
                    raw: [0; 4],
                    neighbors: BTreeSet::new(),
                    scenes: Vec::new(),
                },
            );
        }
    }
    Ok(cohort)
}

fn admitted_events<'a>(world: &'a World, at: &StoryTime) -> Result<Vec<(StoryTime, &'a Record)>> {
    let mut events = Vec::new();
    for event in world
        .records
        .values()
        .filter(|record| record.kind == "event" && record.status() == "canonical")
    {
        if !description::full_author_record_visible(world, event, Some(at))? {
            continue;
        }
        let time = point(world, event.get("time"))?;
        if time.not_after(at) {
            bounded(
                events.len() + 1,
                MAX_EVIDENCE,
                "admitted event evidence items",
            )?;
            events.push((time, event));
        }
    }
    events.sort_by(|(ta, a), (tb, b)| ta.cmp(tb).then_with(|| a.id.cmp(&b.id)));
    Ok(events)
}

fn count_prominence<'a>(
    world: &'a World,
    at: &StoryTime,
    events: &[(StoryTime, &'a Record)],
    cohort: &mut BTreeMap<&'a str, Character<'a>>,
    evidence: &mut Evidence,
) -> Result<[usize; 4]> {
    for (_, event) in events {
        let mut involved = BTreeSet::new();
        for participant in list(event.get("participants"))? {
            let id = text(participant.get("character"))?;
            if cohort.contains_key(id) {
                involved.insert(id);
            }
        }
        for effect in list(event.get("effects"))? {
            let id = text(effect.get("target"))?;
            if let Some(character) = cohort.get(id) {
                if effect_visible(world, character.record, effect, at)? {
                    involved.insert(id);
                }
            }
        }
        evidence.add(involved.len())?;
        for id in involved {
            cohort.get_mut(id).expect("admitted participant").raw[2] += 1;
        }
    }
    for record in world
        .records
        .values()
        .filter(|record| record.kind == "scene" && matches!(record.status(), "active" | "closed"))
    {
        if !visible(world, record, at)? {
            continue;
        }
        let start = point(world, record.get("time").and_then(|time| time.get("start")))?;
        let mut seen = BTreeSet::new();
        for participant in list(record.get("participants"))? {
            let id = text(participant.get("character"))?;
            let Some(character) = cohort.get_mut(id) else {
                continue;
            };
            if !seen.insert(id) {
                return Err(malformed("duplicate scene participant"));
            }
            let entered = participant
                .get("from")
                .filter(|value| !value.is_null())
                .map(|value| point(world, Some(value)))
                .transpose()?
                .unwrap_or_else(|| start.clone());
            if !entered.not_after(at) {
                continue;
            }
            evidence.add(1)?;
            character.raw[0] += 1;
            if participant.get("point_of_view").and_then(Yaml::as_bool) == Some(true) {
                character.raw[1] += 1;
            }
            if scene::present(world, record, id, at)? {
                character.scenes.push(record);
            }
        }
    }

    for relationship in world
        .records
        .values()
        .filter(|record| record.kind == "relationship" && record.status() == "canonical")
    {
        if !visible(world, relationship, at)? {
            continue;
        }
        let from = text(relationship.get("from"))?;
        let to = text(relationship.get("to"))?;
        if from == to || !cohort.contains_key(from) || !cohort.contains_key(to) {
            continue;
        }
        let mut established = false;
        for transition in list(relationship.get("transitions"))? {
            if point(world, transition.get("time"))?.not_after(at) {
                established = true;
                break;
            }
        }
        if established && !cohort[from].neighbors.contains(to) {
            evidence.add(1)?;
            cohort
                .get_mut(from)
                .expect("admitted endpoint")
                .neighbors
                .insert(to);
            cohort
                .get_mut(to)
                .expect("admitted endpoint")
                .neighbors
                .insert(from);
        }
    }
    let mut maxima = [0; 4];
    for character in cohort.values_mut() {
        character.raw[3] = character.neighbors.len();
        for (index, maximum) in maxima.iter_mut().enumerate() {
            *maximum = (*maximum).max(character.raw[index]);
        }
        character.scenes.sort_by(|left, right| order(left, right));
    }

    Ok(maxima)
}

/// Catalogue sorting shares the exact full-cohort scoring calculation, without
/// materializing journeys or applying a journey response limit. No filtered
/// catalogue selection can change the normalization denominator.
pub(crate) fn prominence(world: &World, at: &StoryTime) -> Result<BTreeMap<String, f64>> {
    world.story_time(at.tick, Some(&at.timeline), at.order)?;
    let mut cohort = cohort(world, at, 4096)?;
    let events = admitted_events(world, at)?;
    let maxima = count_prominence(world, at, &events, &mut cohort, &mut Evidence::default())?;
    Ok(cohort
        .into_iter()
        .map(|(id, character)| {
            (
                id.to_owned(),
                importance(&character.raw, &maxima)["score"]
                    .as_f64()
                    .expect("finite prominence score"),
            )
        })
        .collect())
}

/// Read a complete author projection at one exact horizon. A selection contains
/// one to thirty-two distinct exact character IDs, never titles or aliases.
/// `limit` bounds each returned group array and total returned journey entries.
pub fn whereabouts(
    world: &World,
    at: &StoryTime,
    characters: Option<&[String]>,
    limit: usize,
) -> Result<Json> {
    if !(1..=1000).contains(&limit) {
        return Err(invalid("whereabouts limit must be from 1 to 1000"));
    }
    world.story_time(at.tick, Some(&at.timeline), at.order)?;
    let selected = characters
        .map(|ids| {
            let unique: BTreeSet<_> = ids.iter().map(String::as_str).collect();
            if ids.is_empty()
                || ids.len() > 32
                || unique.len() != ids.len()
                || ids
                    .iter()
                    .any(|id| !crate::validation::valid_id_for_kind(id, "character"))
            {
                return Err(invalid(
                    "select between 1 and 32 distinct exact character IDs",
                ));
            }
            Ok(unique)
        })
        .transpose()?;

    let mut cohort = cohort(world, at, MAX_COHORT)?;
    if selected
        .as_ref()
        .is_some_and(|ids| ids.iter().any(|id| !cohort.contains_key(id)))
    {
        return Err(Error::NotFound(
            "whereabouts character selection is unavailable".into(),
        ));
    }
    let mut evidence = Evidence::default();
    let definition = location_definition(world)?;
    if let Some(definition) = &definition {
        for character in cohort.values_mut() {
            if let Some(value) = character
                .record
                .get("initial_state")
                .and_then(|value| value.get("location"))
            {
                state_schema::validate_value(world, definition, value)?;
                if let Location::Present(place) = location(world, definition, value, at)? {
                    evidence.add(1)?;
                    character.current = Some(place);
                    character.last_known = Some(place);
                    character.journey.push(Journey {
                        kind: "initial",
                        at: None,
                        from: None,
                        to: Some(place.into()),
                        event: None,
                        citation: citation(
                            world,
                            character.record,
                            "initial_state.location".into(),
                        ),
                    });
                }
            }
        }
    }

    let events = admitted_events(world, at)?;
    let maxima = count_prominence(world, at, &events, &mut cohort, &mut evidence)?;
    for (time, event) in events {
        for (ordinal, effect) in list(event.get("effects"))?.iter().enumerate() {
            let id = text(effect.get("target"))?;
            let Some(character) = cohort.get_mut(id) else {
                continue;
            };
            if !effect_visible(world, character.record, effect, at)? {
                continue;
            }
            let Some(definition) = definition
                .as_ref()
                .filter(|_| effect.get("key").and_then(Yaml::as_str) == Some("location"))
            else {
                continue;
            };
            state_schema::validate_effect(world, character.record, effect)?;
            let operation = text(effect.get("operation"))?;
            let next = match operation {
                "set" => match location(
                    world,
                    definition,
                    effect
                        .get("value")
                        .ok_or_else(|| malformed("set requires value"))?,
                    at,
                )? {
                    Location::Present(place) => Some(place),
                    Location::Absent => None,
                    Location::Withheld => continue,
                },
                "clear" => None,
                _ => {
                    return Err(malformed(
                        "operative location state only supports set and clear",
                    ));
                }
            };
            evidence.add(1)?;
            let previous = character.current;
            let kind = if operation == "clear" {
                "clear"
            } else if next.map(|record| &record.id) == previous.map(|record| &record.id) {
                "reaffirmation"
            } else {
                "move"
            };
            character.journey.push(Journey {
                kind,
                at: Some(time.clone()),
                from: previous.map(Into::into),
                to: next.map(Into::into),
                event: Some(event.into()),
                citation: citation(world, event, format!("effects[{ordinal}]")),
            });
            character.current = next;
            if next.is_some() {
                character.last_known = next;
            }
        }
    }

    let mut included: Vec<_> = cohort
        .values()
        .filter(|character| {
            selected
                .as_ref()
                .is_none_or(|ids| ids.contains(character.record.id.as_str()))
        })
        .collect();
    bounded(included.len(), limit, "characters")?;
    bounded(
        included
            .iter()
            .map(|character| character.journey.len())
            .sum(),
        limit,
        "journey entries",
    )?;
    included.sort_by(|left, right| order(left.record, right.record));
    let mut entries = Vec::new();
    let mut locations = BTreeMap::new();
    let mut active_scenes = BTreeMap::new();
    let mut offstage = Vec::new();
    let mut unlocated = Vec::new();
    for character in included {
        let reference: Reference<'_> = character.record.into();
        let active = character.scenes.first().copied();
        let presence = if character.current.is_none() {
            "unlocated"
        } else if active.is_some() {
            "active-scene"
        } else {
            "offstage"
        };
        if presence == "offstage" {
            offstage.push(reference);
        }
        if presence == "unlocated" {
            unlocated.push(reference);
        }
        if let Some(place) = character.current {
            locations
                .entry(place.id.as_str())
                .or_insert_with(|| LocationGroup {
                    location: place.into(),
                    characters: Vec::new(),
                })
                .characters
                .push(reference);
        }
        for scene in &character.scenes {
            let place = scene
                .string("location")
                .and_then(|id| world.records.get(id))
                .filter(|record| record.kind == "location");
            let place = match place {
                Some(record) if visible(world, record, at)? => Some(record),
                _ => None,
            };
            active_scenes
                .entry(scene.id.as_str())
                .or_insert_with(|| SceneGroup {
                    scene: (*scene).into(),
                    location: place.map(Into::into),
                    characters: Vec::new(),
                })
                .characters
                .push(reference);
        }
        entries.push(Entry {
            character: reference,
            record_status: character.record.status(),
            role: character
                .record
                .string("role")
                .map(str::trim)
                .filter(|role| !role.is_empty()),
            importance: importance(&character.raw, &maxima),
            presence,
            location: character.current.map(Into::into),
            last_known_location: character
                .current
                .is_none()
                .then_some(character.last_known)
                .flatten()
                .map(Into::into),
            active_scene: active.map(Into::into),
            journey: &character.journey,
        });
    }
    bounded(locations.len(), limit, "location groups")?;
    bounded(active_scenes.len(), limit, "active scenes")?;
    let mut locations: Vec<_> = locations.into_values().collect();
    locations.sort_by(|left, right| {
        left.location
            .title
            .to_lowercase()
            .cmp(&right.location.title.to_lowercase())
            .then_with(|| left.location.id.cmp(right.location.id))
    });
    let mut active_scenes: Vec<_> = active_scenes.into_values().collect();
    active_scenes.sort_by(|left, right| {
        left.scene
            .title
            .to_lowercase()
            .cmp(&right.scene.title.to_lowercase())
            .then_with(|| left.scene.id.cmp(right.scene.id))
    });
    let output = Output {
        protocol: crate::CONTRACT,
        perspective: "author",
        revision: world.revision.as_deref(),
        fingerprint: &world.fingerprint,
        effective_time: at,
        time_scope: "as-of",
        character_policy: json!({"includedStatuses":["canonical","retired"],
            "locationEvidence":"admitted initial location and canonical operative location effects","inference":"none"}),
        importance_policy: json!({"algorithm":ALGORITHM,"calculated":true,"nonCanonical":true,
            "cohort":"all admitted canonical and retired characters before selection",
            "weights":{"scenes":40,"pointOfViewScenes":25,"events":20,"relationshipNeighbors":15},
            "normalization":"log1p(raw) / log1p(cohort maximum); zero maximum contributes zero"}),
        character_filter: selected
            .as_ref()
            .map(|_| entries.iter().map(|entry| entry.character).collect()),
        characters: entries,
        locations,
        active_scenes,
        offstage_characters: offstage,
        unlocated_characters: unlocated,
    };
    serde_json::to_writer(&mut Size(0), &output)
        .map_err(|_| invalid("complete whereabouts output exceeds 4 MiB"))?;
    serde_json::to_value(output).map_err(|error| Error::InvalidInput(error.to_string()))
}
