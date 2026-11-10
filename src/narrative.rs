//! Explicit character record admission for dated narrative source.
//! Known source is selected from held, scoped acquisitions; links never teach it.
use crate::{
    Error, Result, description, lore,
    model::{Record, StoryTime, World},
    spatial_history, validation,
};
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::collections::BTreeSet;

pub use crate::existence::character as existence;
pub use crate::governance::learned as governance;

fn malformed() -> Error {
    Error::Validation(
        "character narrative source is invalid; use author validation for diagnostics".into(),
    )
}

pub(crate) fn unavailable() -> Error {
    Error::NotFound("character narrative view is unavailable".into())
}

fn labels(value: Option<&Yaml>, public: &str, viewer: &str) -> Result<bool> {
    match value {
        None => Ok(true),
        Some(Yaml::Sequence(values))
            if values
                .iter()
                .all(|value| matches!(value, Yaml::String(text) if !text.trim().is_empty())) =>
        {
            Ok(values.iter().any(
                |value| matches!(value.as_str(), Some(value) if value == public || value == viewer),
            ))
        }
        _ => Err(malformed()),
    }
}

/// Optional source labels constrain learning and endpoints; caller labels grant nothing.
pub(crate) fn labels_match(record: &Record, viewer: &str) -> Result<bool> {
    Ok(labels(record.get("audience"), "public", viewer)?
        && labels(record.get("perspectives"), "ordinary", viewer)?)
}

/// The pinned source-prose boundary leaves identity timeless. Completed scene
/// and conversation summaries start at their explicit end; open aggregates
/// never enter a dated corpus. An environment's endpoints are inclusive.
pub(crate) fn author_body_visible(world: &World, record: &Record, at: &StoryTime) -> Result<bool> {
    fn point(world: &World, value: &Yaml) -> Result<StoryTime> {
        let value = StoryTime::from_yaml(value, &world.default_timeline())?;
        world.story_time(value.tick, Some(&value.timeline), value.order)
    }

    let time = record.get("time");
    let optional_point = |value: Option<&Yaml>| {
        value
            .filter(|value| !value.is_null())
            .map(|value| point(world, value))
            .transpose()
    };
    let (start, end) = match record.kind.as_str() {
        "event" => (optional_point(time)?, None),
        "scene" | "conversation" => {
            let Some(end) = optional_point(time.and_then(|value| value.get("end")))? else {
                return Ok(false);
            };
            (Some(end), None)
        }
        "environment" => (
            optional_point(time.and_then(|value| value.get("start")))?,
            optional_point(time.and_then(|value| value.get("end")))?,
        ),
        _ => {
            let mut earliest: Option<StoryTime> = None;
            for transitions in [
                record.get("transitions"),
                record
                    .get("lifecycle")
                    .and_then(|value| value.get("transitions")),
            ]
            .into_iter()
            .flatten()
            .filter_map(Yaml::as_sequence)
            {
                for transition in transitions {
                    if let Some(value) = transition.get("time").filter(|value| !value.is_null()) {
                        let next = point(world, value)?;
                        if earliest.as_ref().is_none_or(|previous| next < *previous) {
                            earliest = Some(next);
                        }
                    }
                }
            }
            (earliest, None)
        }
    };
    Ok(start.as_ref().is_none_or(|start| start.not_after(at))
        && end.as_ref().is_none_or(|end| at.not_after(end)))
}

fn enabled(world: &World, capability: &str) -> bool {
    world
        .world_record()
        .ok()
        .and_then(|record| record.list("capabilities"))
        .is_some_and(|values| {
            values
                .iter()
                .any(|value| value.as_str() == Some(capability))
        })
}

pub(crate) fn require_capability(world: &World, capability: &str) -> Result<()> {
    if enabled(world, capability) {
        Ok(())
    } else {
        Err(malformed())
    }
}

pub(crate) fn limit_argument(limit: usize) -> Result<()> {
    if (1..=1000).contains(&limit) {
        Ok(())
    } else {
        Err(Error::InvalidInput(
            "character narrative limit must be from 1 to 1000".into(),
        ))
    }
}

fn bounded(count: usize, limit: usize) -> Result<()> {
    if count <= limit {
        Ok(())
    } else {
        Err(Error::InvalidInput(format!(
            "character narrative result exceeds {limit} items; refusing an incomplete result"
        )))
    }
}

pub(crate) fn envelope(viewer: &str, at: &StoryTime, basis: &str) -> Json {
    json!({"protocol":crate::CONTRACT,"perspective":"character",
        "scope":"explicit-record-admission","characterId":viewer,
        "effectiveTime":at,"basis":basis})
}

/// Derived inside one captured source read. Its known set cannot be supplied by a request.
pub(crate) struct CharacterScope<'a> {
    world: &'a World,
    viewer: &'a str,
    at: &'a StoryTime,
    known: BTreeSet<String>,
}

impl<'a> CharacterScope<'a> {
    pub(crate) fn new(world: &'a World, viewer: &'a str, at: &'a StoryTime) -> Result<Self> {
        Self::with_scene(world, viewer, at, None)
    }

    /// A selected, scoped scene grants only its explicit currently present roots.
    /// The scene and its time are supplied by the caller; neither is inferred.
    pub(crate) fn with_scene(
        world: &'a World,
        viewer: &'a str,
        at: &'a StoryTime,
        selected_scene: Option<&str>,
    ) -> Result<Self> {
        if !validation::validate_world(world).valid {
            return Err(malformed());
        }
        Self::from_validated_world(world, viewer, at, selected_scene)
    }

    /// The generation provider validates the complete native source before
    /// exposing this immutable pin. Every lens still checks native viewer,
    /// timeline, presence and acquisition semantics below.
    pub(crate) fn from_compendium_pin(
        pin: &'a crate::service::CompendiumPin,
        viewer: &'a str,
        at: &'a StoryTime,
        selected_scene: Option<&str>,
    ) -> Result<Self> {
        Self::from_validated_world(pin.world(), viewer, at, selected_scene)
    }

    fn from_validated_world(
        world: &'a World,
        viewer: &'a str,
        at: &'a StoryTime,
        selected_scene: Option<&str>,
    ) -> Result<Self> {
        if !validation::valid_id_for_kind(viewer, "character")
            || world
                .records
                .get(viewer)
                .is_none_or(|record| record.kind != "character")
        {
            return Err(unavailable());
        }
        world.story_time(at.tick, Some(&at.timeline), at.order)?;
        let scene = selected_scene
            .map(|id| {
                let record = world.records.get(id).ok_or_else(unavailable)?;
                if record.kind != "scene"
                    || !labels_match(record, viewer)?
                    || !crate::scene::present(world, record, viewer, at)?
                {
                    return Err(unavailable());
                }
                Ok(record)
            })
            .transpose()?;
        let mut known = BTreeSet::from([viewer.to_owned()]);
        if let Some(scene) = scene {
            known.insert(scene.id.clone());
            if let Some(id) = scene.string("location") {
                known.insert(id.into());
            }
            for participant in scene.list("participants").into_iter().flatten() {
                let id = participant
                    .get("character")
                    .and_then(Yaml::as_str)
                    .ok_or_else(malformed)?;
                if crate::scene::present(world, scene, id, at)? {
                    known.insert(id.into());
                }
            }
            for key in ["objects", "environments"] {
                for value in scene.list(key).into_iter().flatten() {
                    known.insert(value.as_str().ok_or_else(malformed)?.into());
                }
            }
            for relationship in world.records.values().filter(|record| {
                record.kind == "relationship"
                    && record.status() == "canonical"
                    && record.string("from") == Some(viewer)
            }) {
                if !labels_match(relationship, viewer)? {
                    continue;
                }
                let mut acquired = false;
                for transition in relationship.list("transitions").into_iter().flatten() {
                    let time = StoryTime::from_yaml(
                        transition.get("time").ok_or_else(malformed)?,
                        &world.default_timeline(),
                    )
                    .map_err(|_| malformed())?;
                    acquired |= time.not_after(at);
                }
                if acquired {
                    known.insert(relationship.string("to").ok_or_else(malformed)?.into());
                }
            }
        }
        // Typed records retain their own learned/current eligibility. Their
        // payload endpoints never become canonical source knowledge.
        for item in crate::generational::knowledge::admitted(world, viewer, at, false)
            .map_err(|_| malformed())?
            .into_iter()
            .chain(crate::learned_assertions::admitted(world, viewer, at).map_err(|_| malformed())?)
        {
            known.insert(item["knowledgeId"].as_str().ok_or_else(malformed)?.into());
        }
        for record in world.records.values().filter(|record| {
            record.kind == "knowledge"
                && record.status() == "canonical"
                && record.string("knower") == Some(viewer)
                && record.get("claim").is_some_and(|claim| {
                    claim.get("assertion_id").is_none() && claim.get("genealogy").is_none()
                })
        }) {
            if !labels_match(record, viewer)? {
                continue;
            }
            let mut latest = None;
            for transition in record
                .list("transitions")
                .map(Vec::as_slice)
                .unwrap_or_default()
            {
                let time = StoryTime::from_yaml(
                    transition.get("time").ok_or_else(malformed)?,
                    &world.default_timeline(),
                )
                .map_err(|_| malformed())?;
                if !time.not_after(at) {
                    continue;
                }
                let state = transition
                    .get("state")
                    .and_then(Yaml::as_str)
                    .ok_or_else(malformed)?;
                if latest
                    .as_ref()
                    .is_none_or(|(previous, _): &(StoryTime, &str)| {
                        (time.tick, time.order) >= (previous.tick, previous.order)
                    })
                {
                    latest = Some((time, state));
                }
            }
            let Some((_, state)) = latest else {
                continue;
            };
            if state == "forgotten" {
                continue;
            }
            let claim = record.get("claim").ok_or_else(malformed)?;
            known.insert(record.id.clone());
            // This is the Rust source's ordinary entity acquisition. The selected
            // scene additionally permits its explicit subject below; typed
            // assertion/genealogy pointers and literal words grant no target.
            if let Some(id) = claim
                .get("object")
                .and_then(|object| object.get("entity"))
                .and_then(Yaml::as_str)
                && world.records.contains_key(id)
            {
                known.insert(id.into());
            }
            if scene.is_some()
                && let Some(id) = claim.get("subject").and_then(Yaml::as_str)
                && world.records.contains_key(id)
            {
                known.insert(id.into());
            }
        }
        Ok(Self {
            world,
            viewer,
            at,
            known,
        })
    }

    /// A scope cannot be reused for another snapshot, viewer or horizon.
    pub(crate) fn matches(&self, world: &World, viewer: &str, at: &StoryTime) -> bool {
        std::ptr::eq(self.world, world) && self.viewer == viewer && self.at == at
    }

    pub(crate) fn visible(&self, record: &Record) -> Result<bool> {
        if !self.known.contains(&record.id)
            || matches!(
                record.kind.as_str(),
                "assertion" | "existence-episode" | "hypothesis"
            )
            || !validation::is_record_kind(&record.kind)
            || !labels_match(record, self.viewer)?
        {
            return Ok(false);
        }
        if matches!(record.kind.as_str(), "lore-concept" | "context-brief") {
            if record.status() != "canonical" {
                return Ok(false);
            }
            return Ok(lore::validity(
                record.get("valid").ok_or_else(malformed)?,
                Some(self.world),
                "valid",
            )
            .map_err(|_| malformed())?
            .applies(Some(self.at)));
        }
        Ok(true)
    }

    pub(crate) fn allowed_ids(&self) -> Result<BTreeSet<String>> {
        self.known
            .iter()
            .filter_map(|id| self.world.records.get(id))
            .filter_map(|record| match self.visible(record) {
                Ok(true) => Some(Ok(record.id.clone())),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            })
            .collect()
    }

    fn owner(&self, id: &str) -> Result<&'a Record> {
        let record = self.world.records.get(id).ok_or_else(unavailable)?;
        if self.visible(record)? {
            Ok(record)
        } else {
            Err(unavailable())
        }
    }
}

/// Dated explicit sections on one known owner; owner body and source metadata are absent.
pub fn descriptions(
    world: &World,
    viewer: &str,
    owner: &str,
    at: &StoryTime,
    limit: usize,
) -> Result<Json> {
    limit_argument(limit)?;
    let scope = CharacterScope::new(world, viewer, at)?;
    require_capability(world, "descriptions-v1")?;
    let record = scope.owner(owner)?;
    let items = description::character_values(world, record, viewer, at)?;
    bounded(items.len(), limit)?;
    let mut result = envelope(viewer, at, "admitted-description-sections");
    result["ownerId"] = json!(owner);
    result["count"] = json!(items.len());
    result["items"] = json!(items);
    result["limit"] = json!(limit);
    Ok(result)
}

pub fn section(
    world: &World,
    viewer: &str,
    owner: &str,
    section_id: &str,
    at: &StoryTime,
) -> Result<Json> {
    let scope = CharacterScope::new(world, viewer, at)?;
    require_capability(world, "descriptions-v1")?;
    let record = scope.owner(owner)?;
    let item = description::character_values(world, record, viewer, at)?
        .into_iter()
        .find(|item| item["sectionId"] == section_id)
        .ok_or_else(unavailable)?;
    let mut result = envelope(viewer, at, "admitted-description-section");
    result
        .as_object_mut()
        .unwrap()
        .extend(item.as_object().unwrap().clone());
    Ok(result)
}

/// Generic context/search apply their own complete packet bounds after this admission.
pub(crate) fn admitted_descriptions(
    world: &World,
    viewer: &str,
    at: &StoryTime,
) -> Result<Vec<Json>> {
    if !enabled(world, "descriptions-v1")
        && !world
            .records
            .values()
            .any(|record| record.get("descriptions").is_some())
    {
        return Ok(Vec::new());
    }
    let scope = CharacterScope::new(world, viewer, at)?;
    admitted_descriptions_in_scope(world, viewer, at, &scope)
}

pub(crate) fn admitted_descriptions_in_scope(
    world: &World,
    viewer: &str,
    at: &StoryTime,
    scope: &CharacterScope<'_>,
) -> Result<Vec<Json>> {
    if !scope.matches(world, viewer, at) {
        return Err(malformed());
    }
    if !enabled(world, "descriptions-v1")
        && !world
            .records
            .values()
            .any(|record| record.get("descriptions").is_some())
    {
        return Ok(Vec::new());
    }
    require_capability(world, "descriptions-v1")?;
    let mut items = Vec::new();
    for record in world
        .records
        .values()
        .filter(|record| record.get("descriptions").is_some())
    {
        if scope.visible(record)? {
            items.extend(description::character_values(world, record, viewer, at)?);
        }
    }
    Ok(items)
}

pub fn lore_catalog(
    world: &World,
    viewer: &str,
    domain: Option<&str>,
    at: &StoryTime,
    limit: usize,
) -> Result<Json> {
    limit_argument(limit)?;
    if domain.is_some_and(|domain| !lore::domain(domain)) {
        return Err(Error::InvalidInput(
            "domain must be an exact dotted identifier".into(),
        ));
    }
    let scope = CharacterScope::new(world, viewer, at)?;
    require_capability(world, "lore-core-v1")?;
    let items = lore::character_values(world, &scope.allowed_ids()?, at)?
        .into_iter()
        .filter(|item| domain.is_none_or(|domain| item["domain"] == domain))
        .collect::<Vec<_>>();
    bounded(items.len(), limit)?;
    let mut result = envelope(viewer, at, "admitted-lore-concepts");
    result["domain"] = json!(domain);
    result["count"] = json!(items.len());
    result["items"] = json!(items);
    result["limit"] = json!(limit);
    Ok(result)
}

pub fn lore_concept(world: &World, viewer: &str, target: &str, at: &StoryTime) -> Result<Json> {
    let scope = CharacterScope::new(world, viewer, at)?;
    require_capability(world, "lore-core-v1")?;
    let owner = scope.owner(target)?;
    if owner.kind != "lore-concept" {
        return Err(unavailable());
    }
    let item = lore::character_values(world, &scope.allowed_ids()?, at)?
        .into_iter()
        .find(|item| item["conceptId"] == target)
        .ok_or_else(unavailable)?;
    let mut result = envelope(viewer, at, "admitted-lore-concept");
    result
        .as_object_mut()
        .unwrap()
        .extend(item.as_object().unwrap().clone());
    Ok(result)
}

pub fn lore_links(
    world: &World,
    viewer: &str,
    owner: &str,
    at: &StoryTime,
    limit: usize,
) -> Result<Json> {
    limit_argument(limit)?;
    let scope = CharacterScope::new(world, viewer, at)?;
    require_capability(world, "lore-core-v1")?;
    let record = scope.owner(owner)?;
    let items = lore::character_links(world, record, &scope.allowed_ids()?, at)?;
    bounded(items.len(), limit)?;
    let mut result = envelope(viewer, at, "admitted-concept-links");
    result["ownerId"] = json!(owner);
    result["count"] = json!(items.len());
    result["items"] = json!(items);
    result["limit"] = json!(limit);
    Ok(result)
}

pub fn spatial_history(
    world: &World,
    viewer: &str,
    owner: &str,
    at: &StoryTime,
    limit: usize,
) -> Result<Json> {
    limit_argument(limit)?;
    let scope = CharacterScope::new(world, viewer, at)?;
    require_capability(world, "spatial-history-v1")?;
    let record = scope.owner(owner)?;
    if !matches!(record.kind.as_str(), "location" | "route") || record.status() != "canonical" {
        return Err(unavailable());
    }
    let items = spatial_history::character_values(world, record, at, &scope.allowed_ids()?)?;
    bounded(items.len(), limit)?;
    let mut result = envelope(viewer, at, "admitted-spatial-history");
    result["ownerId"] = json!(owner);
    result["count"] = json!(items.len());
    result["items"] = json!(items);
    result["limit"] = json!(limit);
    Ok(result)
}
