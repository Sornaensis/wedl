//! Author environment, material and technology context from explicit source.
//! Direct endpoints never become additional traversal or evidence roots.
use crate::{
    Error, Result, family_context,
    lore_types::TypeSpec,
    model::{StoryTime, World},
    narrative::{self, CharacterScope},
    validation::valid_id_for_kind,
};
use serde_json::{Value as Json, json};
use std::collections::BTreeSet;

const LINK_FIELDS: [&str; 6] = [
    "eras",
    "dependencies",
    "resources",
    "habitats",
    "technologies",
    "infrastructures",
];
const PREDICATES: [&str; 2] = ["context-availability", "context-dependency-evidence"];

fn role(entry: &Json) -> Option<&'static str> {
    match entry["profile"].as_str()? {
        "context-habitat" => Some("habitat"),
        "context-resource" => Some("resource"),
        "context-livelihood" => Some("livelihood"),
        "context-trade-practice" => Some("trade-practice"),
        "context-infrastructure" => Some("infrastructure"),
        "context-technology" => Some("technology"),
        "context-era" => Some("era"),
        _ => None,
    }
}
fn unavailable() -> Error {
    Error::NotFound("environment context is unavailable".into())
}
fn record_target(id: &str) -> Json {
    json!({"kind":"record","id":id})
}
fn hook(target: Json, citation: &Json) -> Json {
    json!({"target":target,"citation":citation})
}
fn ordinal(path: &str, prefix: &str, suffix: &str) -> Option<usize> {
    let digits = path.strip_prefix(prefix)?.strip_suffix(suffix)?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}
fn array_reference(path: &str) -> Option<(&str, usize)> {
    let (field, index) = path.strip_prefix("values.")?.split_once('[')?;
    if !LINK_FIELDS.contains(&field) {
        return None;
    }
    Some((field, ordinal(index, "", "].id")?))
}

pub(crate) fn validate_roots(roots: &[String]) -> Result<()> {
    let root_ids = roots.iter().cloned().collect::<BTreeSet<_>>();
    if roots.is_empty()
        || roots.len() > 8
        || root_ids.len() != roots.len()
        || roots
            .iter()
            .any(|id| !valid_id_for_kind(id, "lore-concept"))
    {
        return Err(Error::InvalidInput(
            "environment context requires 1 to 8 distinct exact concept IDs".into(),
        ));
    }
    Ok(())
}

fn choose(
    snapshot: &family_context::Snapshot,
    root_ids: &BTreeSet<String>,
    character: bool,
) -> Result<(BTreeSet<String>, Vec<Json>)> {
    if root_ids
        .iter()
        .any(|id| snapshot.entries.get(id).and_then(role).is_none())
    {
        return Err(unavailable());
    }

    let mut selected = root_ids.clone();
    let mut links = Vec::new();
    for owner in root_ids {
        let entry = &snapshot.entries[owner];
        for reference in entry["references"].as_array().unwrap() {
            let source_field = reference["sourceField"].as_str().unwrap();
            let Some((field, index)) = array_reference(source_field) else {
                continue;
            };
            // Familiar names and empty literal arrays confer no entity semantics.
            if !matches!(snapshot.field_type(entry, field), Some(TypeSpec::Array { items, .. }) if matches!(items.as_ref(), TypeSpec::Entity { .. }))
            {
                continue;
            }
            let target = reference["entityId"].as_str().unwrap();
            if snapshot.entries.get(target).and_then(role).is_none() {
                continue;
            }
            selected.insert(target.to_owned());
            links.push(if character {
                json!({"owner":record_target(owner),"field":format!("values.{field}"),
                    "target":record_target(target)})
            } else {
                json!({"owner":record_target(owner),"field":format!("values.{field}"),
                "sourceField":source_field,"ordinal":index,"target":record_target(target),
                "citation":reference["citation"],"targetCitation":reference["targetCitation"]})
            });
        }
        for link in entry["conceptLinks"].as_array().unwrap() {
            let target = link["conceptId"].as_str().unwrap();
            if snapshot.entries.get(target).and_then(role).is_none() {
                continue;
            }
            selected.insert(target.to_owned());
            links.push(if character {
                json!({"owner":record_target(owner),"field":"concept_links","target":record_target(target),
                    "relation":link["relation"],"valid":link["valid"]})
            } else {
                let source_field = link["citation"]["section"].as_str().unwrap();
                let index = ordinal(source_field, "concept_links[", "]")
                    .ok_or_else(|| Error::Data("invalid projected concept-link source index".into()))?;
                json!({"owner":record_target(owner),"field":"concept_links",
                    "sourceField":source_field,"ordinal":index,"target":record_target(target),
                    "relation":link["relation"],"valid":link["valid"],
                    "citation":link["citation"],"targetCitation":link["targetCitation"]})
            });
        }
        if selected.len() > family_context::MAX_RECORDS {
            return Err(Error::InvalidInput(
                "environment context exceeds 32 selected records; refusing an incomplete result"
                    .into(),
            ));
        }
    }
    Ok((selected, links))
}

/// Compose one complete author context at an exact horizon over 1–8 roots.
/// Source values describe accounts; they do not execute quantities or dependencies.
pub fn context(
    world: &World,
    roots: &[String],
    at: &StoryTime,
    audience: &str,
    view: &str,
) -> Result<Json> {
    validate_roots(roots)?;
    let root_ids = roots.iter().cloned().collect::<BTreeSet<_>>();
    let snapshot = family_context::Snapshot::read(world, at, audience, view)?;
    let (selected, links) = choose(&snapshot, &root_ids, false)?;
    let owners = root_ids.into_iter().collect::<Vec<_>>();
    let evidence = family_context::evidence(world, &owners, at, audience, view)?;
    let records = selected
        .iter()
        .map(|id| json!({"role":role(&snapshot.entries[id]).unwrap(),"item":snapshot.entries[id]}))
        .collect::<Vec<_>>();
    let mut hooks = selected
        .iter()
        .map(|id| hook(record_target(id), &snapshot.entries[id]["citation"]))
        .collect::<Vec<_>>();
    let mut unknowns = Vec::new();
    let mut competing = Vec::new();
    for stream in &evidence.canon {
        let owner = stream["ownerId"].as_str().unwrap();
        let items = stream["result"]["items"].as_array().unwrap();
        for predicate in PREDICATES {
            let accounts = items
                .iter()
                .filter(|item| item["predicate"] == predicate)
                .collect::<Vec<_>>();
            if accounts.is_empty() {
                unknowns.push(json!({"owner":record_target(owner),"stream":predicate,
                    "basis":"no-admitted-account"}));
            }
            for account in &accounts {
                let id = account["assertionId"].as_str().unwrap();
                if account["classification"] == "unresolved" {
                    unknowns.push(json!({"owner":record_target(owner),"stream":predicate,
                        "basis":"explicit-unresolved","target":record_target(id)}));
                }
                hooks.push(hook(record_target(id), &account["citation"]));
            }
            // Used types preserve distinctions erased by integer-string wire values.
            if accounts.first().is_some_and(|first| {
                accounts.iter().skip(1).any(|account| {
                    account["value"] != first["value"] || account["valueType"] != first["valueType"]
                })
            }) {
                let targets = accounts
                    .iter()
                    .map(|account| record_target(account["assertionId"].as_str().unwrap()))
                    .collect::<Vec<_>>();
                competing.push(json!({"subject":record_target(owner),"predicate":predicate,
                    "basis":"different-literal-accounts","targets":targets}));
            }
        }
    }
    for stream in &evidence.sections {
        for section in stream["result"]["items"].as_array().unwrap() {
            hooks.push(hook(section["target"].clone(), &section["citation"]));
        }
    }

    family_context::finish(json!({"protocol":crate::CONTRACT,"perspective":"author",
        "scope":"author-as-of-environment-context","basis":"authored-environment-context",
        "at":at,"audience":audience,"view":view,"selection":{"contexts":roots},
        "count":records.len(),"records":records,"links":links,
        "canon":evidence.canon,"sections":evidence.sections,"unknowns":unknowns,
        "competingAccounts":competing,"hooks":hooks}))
}

/// Character roots must already be independently known. Direct links never
/// teach endpoints, and learned targets are not joined to canonical accounts.
pub fn character_context(
    world: &World,
    viewer: &str,
    roots: &[String],
    belief_targets: &[String],
    at: &StoryTime,
) -> Result<Json> {
    validate_roots(roots)?;
    family_context::validate_belief_targets(belief_targets)?;
    let scope = CharacterScope::new(world, viewer, at)?;
    character_context_in_scope(world, viewer, roots, belief_targets, at, &scope)
}

pub(crate) fn character_context_in_scope(
    world: &World,
    viewer: &str,
    roots: &[String],
    belief_targets: &[String],
    at: &StoryTime,
    scope: &CharacterScope<'_>,
) -> Result<Json> {
    validate_roots(roots)?;
    family_context::validate_belief_targets(belief_targets)?;
    family_context::require_context_capabilities(world)?;
    let root_ids = roots.iter().cloned().collect::<BTreeSet<_>>();
    let snapshot = family_context::Snapshot::character(world, viewer, at, scope)?;
    let (selected, links) = choose(&snapshot, &root_ids, true)?;
    let owners = root_ids.into_iter().collect::<Vec<_>>();
    let evidence =
        family_context::character_evidence(world, &owners, viewer, at, belief_targets, scope)?;
    let records = selected
        .iter()
        .map(|id| {
            let item = &snapshot.entries[id];
            json!({"role":role(item).unwrap(),"item":family_context::character_entry(item)})
        })
        .collect::<Vec<_>>();
    let mut hooks = selected
        .iter()
        .map(|id| json!({"target":record_target(id)}))
        .collect::<Vec<_>>();
    for owner in &evidence.sections {
        for section in owner["result"]["items"].as_array().unwrap() {
            hooks.push(json!({"target":section["target"]}));
        }
    }
    let mut result = narrative::envelope(viewer, at, "admitted-environment-context");
    result["scope"] = json!("character-as-of-environment-context");
    result["selection"] = json!({"contexts":roots,"beliefTargets":belief_targets});
    result["count"] = json!(records.len());
    result["records"] = json!(records);
    result["links"] = json!(links);
    result["sections"] = json!(evidence.sections);
    result["beliefs"] = json!(evidence.beliefs);
    result["hooks"] = json!(hooks);
    result["complete"] = json!(true);
    family_context::finish(result)
}
