//! Admission and finite metadata selection precede filters, search and pages.
use super::{
    Budget, Filters, Lens, MAX_SELECTION, Operation, Request, fold, invalid, source_error,
};
use crate::{
    Result,
    model::{Record, StoryTime, World},
    narrative::CharacterScope,
};
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Row {
    pub item: Json,
    pub learned: Option<Json>,
}
pub(super) struct Selection {
    pub rows: BTreeMap<String, Row>,
    pub allowed: BTreeSet<String>,
    pub known: Vec<String>,
    pub assertions: BTreeMap<String, Json>,
}

pub(super) fn citation(world: &World, record: &Record, section: Option<&str>) -> Json {
    let target = if let Some(section) = section {
        json!({"kind":"section","ownerId":record.id,"id":section})
    } else {
        json!({"kind":"record","id":record.id})
    };
    json!({"kind":"source","fingerprint":world.fingerprint,"revision":world.revision,
        "path":record.source_path,"blobOid":record.blob_oid,"target":target})
}
pub(super) fn learned_citation(world: &World, value: &Json) -> Json {
    json!({"kind":"learned","fingerprint":world.fingerprint,"revision":world.revision,
        "knowledgeId":value["knowledgeId"],"transitionId":value["transitionId"],
        "at":value["time"],"state":value["state"]})
}
fn point(world: &World, value: &Yaml) -> Result<StoryTime> {
    let point =
        StoryTime::from_yaml(value, &world.default_timeline()).map_err(|_| source_error())?;
    world
        .story_time(point.tick, Some(&point.timeline), point.order)
        .map_err(|_| source_error())
}
/// Exact native source metadata boundary from the pinned catalogue selector.
/// Closed scene/conversation metadata begins at the authored end. Open metadata
/// is undated; that does not grant the open aggregate's prose to search.
fn temporal(world: &World, record: &Record, request: &Request) -> Result<bool> {
    // All-time means every position on the explicitly selected timeline,
    // including wholly unbounded intervals; it does not merge timelines.
    if record
        .get("valid")
        .and_then(|valid| valid.get("kind"))
        .and_then(Yaml::as_str)
        == Some("interval")
        && record
            .get("valid")
            .and_then(|valid| valid.get("timeline"))
            .and_then(Yaml::as_str)
            != Some(request.timeline())
    {
        return Ok(false);
    }
    let time = record.get("time");
    let optional = |value: Option<&Yaml>| {
        value
            .filter(|value| !value.is_null())
            .map(|value| point(world, value))
            .transpose()
    };
    let (start, end) = match record.kind.as_str() {
        "event" => (optional(time)?, None),
        "scene" | "conversation" => (optional(time.and_then(|time| time.get("end")))?, None),
        "environment" => (
            optional(time.and_then(|time| time.get("start")))?,
            optional(time.and_then(|time| time.get("end")))?,
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
    if start
        .as_ref()
        .is_some_and(|start| start.timeline != request.timeline())
        || end
            .as_ref()
            .is_some_and(|end| end.timeline != request.timeline())
    {
        return Ok(false);
    }
    Ok(request.at.as_ref().is_none_or(|at| {
        start.as_ref().is_none_or(|start| start.not_after(at))
            && end.as_ref().is_none_or(|end| at.not_after(end))
    }))
}

fn era_value(value: &Yaml, output: &mut BTreeSet<String>) {
    // Native range/approx endpoints are civil dates, not arbitrary tagged
    // values. Relative references and prose never import another record's eras.
    if let Some(era) = value
        .get("era")
        .and_then(|value| value.get("era_id"))
        .and_then(Yaml::as_str)
    {
        output.insert(era.into());
    }
    if let Some(claims) = value
        .get("conflict")
        .and_then(|value| value.get("claims"))
        .and_then(Yaml::as_sequence)
    {
        for claim in claims {
            era_value(claim, output);
        }
    }
}
fn eras(record: &Record) -> Vec<String> {
    let mut eras = BTreeSet::new();
    if record.kind != "world" {
        for annotation in record.list("chronology").into_iter().flatten() {
            if let Some(value) = annotation.get("value") {
                era_value(value, &mut eras);
            }
        }
    }
    eras.into_iter().collect()
}
fn slug(value: &str) -> String {
    let folded = crate::reader::casefold::fold(value);
    let mut out = String::new();
    let mut separated = false;
    for c in folded.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            if separated && !out.is_empty() {
                out.push('-');
            }
            out.push(c);
            separated = false;
        } else {
            separated = true;
        }
    }
    if out.is_empty() { "record".into() } else { out }
}
fn matches(
    record: &Record,
    title: &str,
    profile: Option<&str>,
    eras: &[String],
    aliases: bool,
    filters: &Filters,
) -> bool {
    if !filters.kinds.is_empty() && !filters.kinds.iter().any(|kind| kind == &record.kind) {
        return false;
    }
    if !filters.profiles.is_empty()
        && !profile.is_some_and(|profile| filters.profiles.iter().any(|id| id == profile))
    {
        return false;
    }
    if !filters.eras.is_empty() && !eras.iter().any(|era| filters.eras.contains(era)) {
        return false;
    }
    let name = fold(&filters.name);
    if name.is_empty() {
        return true;
    }
    let matching = |value: &str| fold(value).contains(&name) || slug(value).contains(&name);
    matching(&record.id)
        || matching(title)
        || aliases
            && record
                .list("aliases")
                .into_iter()
                .flatten()
                .filter_map(Yaml::as_str)
                .any(matching)
}
fn validate_threads(world: &World, ids: &[String]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let declarations = world.world_record()?.list("threads");
    if ids.iter().any(|id| {
        !declarations.is_some_and(|values| {
            values
                .iter()
                .any(|value| value.get("id").and_then(Yaml::as_str) == Some(id.as_str()))
        })
    }) {
        return Err(invalid("CAT_REQUEST"));
    }
    Ok(())
}
fn memberships(record: &Record, ids: &[String]) -> Vec<String> {
    if !crate::hypotheses::is_adoptable_canonical_record(record) {
        return Vec::new();
    }
    record
        .list("threads")
        .into_iter()
        .flatten()
        .filter_map(Yaml::as_str)
        .filter(|id| {
            ids.binary_search_by(|candidate| candidate.as_str().cmp(id))
                .is_ok()
        })
        .map(str::to_owned)
        .collect()
}
fn assertion_values(
    world: &World,
    at: Option<&StoryTime>,
    allowed: &mut BTreeSet<String>,
) -> Result<BTreeMap<String, Json>> {
    let values = crate::assertion::full_author_values(world, at).map_err(|_| source_error())?;
    let mut selected = values
        .into_iter()
        .filter_map(|value| Some((value["assertionId"].as_str()?.to_owned(), value)))
        .collect::<BTreeMap<_, _>>();
    allowed.retain(|id| {
        world
            .records
            .get(id)
            .is_none_or(|record| record.kind != "assertion")
            || selected.contains_key(id)
    });
    let mut types = BTreeMap::new();
    for predicate in world
        .world_record()?
        .list("predicates")
        .into_iter()
        .flatten()
    {
        let id = predicate
            .get("id")
            .and_then(Yaml::as_str)
            .ok_or_else(source_error)?;
        types.insert(
            id,
            crate::lore_types::TypeSpec::parse(
                predicate.get("value_type").ok_or_else(source_error)?,
                "catalogue predicate",
            )
            .map_err(|_| source_error())?,
        );
    }
    // Independently unavailable subject/attribution/root entity values must not
    // give an assertion a card, even through another assertion's typed pointer.
    loop {
        let mut remove = Vec::new();
        for (id, value) in &mut selected {
            let record = &world.records[id];
            if !allowed.contains(id)
                || value["subject"]["id"]
                    .as_str()
                    .is_none_or(|id| !allowed.contains(id))
                || value
                    .get("attributedTo")
                    .and_then(|value| value.get("id"))
                    .and_then(Json::as_str)
                    .is_some_and(|id| !allowed.contains(id))
            {
                remove.push(id.clone());
                continue;
            }
            let spec = types
                .get(record.string("predicate").ok_or_else(source_error)?)
                .ok_or_else(source_error)?;
            let projected = crate::lore::projected(
                spec,
                record.get("value").ok_or_else(source_error)?,
                allowed,
            )
            .map_err(|_| source_error())?;
            if let Some((projected, shape)) = projected {
                value["value"] = projected;
                value["valueType"] = shape;
            } else {
                remove.push(id.clone());
            }
        }
        if remove.is_empty() {
            break;
        }
        for id in remove {
            allowed.remove(&id);
            selected.remove(&id);
        }
    }
    Ok(selected)
}

pub(super) fn select(
    world: &World,
    request: &Request,
    character_scope: Option<&CharacterScope<'_>>,
) -> Result<Selection> {
    let page = match &request.operation {
        Operation::Page(page) => Some(page),
        _ => None,
    };
    if let Some(page) = page {
        validate_threads(world, &page.threads)?;
    }
    let learned_enabled = page.is_none_or(|page| page.query.is_some());
    let mut learned = BTreeMap::new();
    if let Lens::Character { character, .. } = &request.lens {
        let at = request.at.as_ref().unwrap();
        for value in
            crate::learned_assertions::admitted(world, character, at).map_err(|_| source_error())?
        {
            learned.insert(
                value["knowledgeId"]
                    .as_str()
                    .ok_or_else(source_error)?
                    .to_owned(),
                value,
            );
        }
        // Native genealogical learning also keeps its literal remembered labels
        // separate from the record's author heading and canonical family graph.
        for mut value in crate::generational::knowledge::admitted(world, character, at, false)
            .map_err(|_| source_error())?
        {
            let mut labels = Vec::new();
            if let Some(kind) = value["kind"].as_str() {
                labels.push(kind);
            }
            if let Some(values) = value["labels"].as_object() {
                labels.extend(values.values().filter_map(Json::as_str));
            }
            value["statement"] = json!(labels.join(" · "));
            value["catalogueDocumentKind"] = json!("generational-belief");
            learned.insert(
                value["knowledgeId"]
                    .as_str()
                    .ok_or_else(source_error)?
                    .to_owned(),
                value,
            );
        }
    }
    let mut allowed = BTreeSet::new();
    for record in world.records.values() {
        if matches!(record.kind.as_str(), "hypothesis" | "context-brief") {
            continue;
        }
        let typed = record.kind == "knowledge"
            && record.get("claim").is_some_and(|claim| {
                claim.get("assertion_id").is_some() || claim.get("genealogy").is_some()
            });
        if typed && character_scope.is_some() {
            if learned_enabled && learned.contains_key(&record.id) {
                allowed.insert(record.id.clone());
            }
            continue;
        }
        // Assertion-linked belief cards are a character learned projection only.
        if record.kind == "knowledge"
            && record
                .get("claim")
                .is_some_and(|claim| claim.get("assertion_id").is_some())
        {
            continue;
        }
        let visible = match character_scope {
            Some(scope) => scope.visible(record).map_err(|_| source_error())?,
            None => {
                crate::description::full_author_record_visible(world, record, request.at.as_ref())
                    .map_err(|_| source_error())?
            }
        };
        if visible && temporal(world, record, request)? {
            allowed.insert(record.id.clone());
        }
    }
    let assertions = if character_scope.is_none() {
        assertion_values(world, request.at.as_ref(), &mut allowed)?
    } else {
        BTreeMap::new()
    };
    let mut rows = BTreeMap::new();
    let mut budget = Budget::new(8 * 1024 * 1024, "CAT_TEXT_LIMIT");
    for id in &allowed {
        if let Operation::Labels(targets) = &request.operation
            && !targets.contains(id)
        {
            continue;
        }
        let record = &world.records[id];
        let learned = learned.get(id);
        let title = match learned {
            Some(value) => value["statement"].as_str().ok_or_else(source_error)?,
            None => record.title.as_str(),
        };
        let profile = (record.kind == "lore-concept")
            .then(|| record.string("profile"))
            .flatten();
        let era_ids = if learned.is_some() {
            Vec::new()
        } else {
            eras(record)
        };
        if page.is_some_and(|page| {
            !matches(
                record,
                title,
                profile,
                &era_ids,
                learned.is_none(),
                &page.filters,
            )
        }) {
            continue;
        }
        if rows.len() == MAX_SELECTION {
            return Err(invalid("CAT_SELECTION_LIMIT"));
        }
        let thread_ids = page.map_or_else(Vec::new, |page| memberships(record, &page.threads));
        let citation = match learned {
            Some(value) => learned_citation(world, value),
            None => citation(world, record, None),
        };
        let item = json!({"id":record.id,"kind":record.kind,"title":title,"profile":profile,
            "eraIds":era_ids,"threadIds":thread_ids,"citations":[citation],"match":null});
        budget.charge(&item)?;
        rows.insert(
            id.clone(),
            Row {
                item,
                learned: learned.cloned(),
            },
        );
    }
    let known = if character_scope.is_some() {
        rows.keys().cloned().collect()
    } else {
        Vec::new()
    };
    Ok(Selection {
        rows,
        allowed,
        known,
        assertions,
    })
}
