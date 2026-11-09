//! Authored language, terminology and cultural context. Explicit links compose
//! separate accounts; they supply no translation, membership inference or belief.
use crate::{
    Error, Result, family_context,
    lore_types::TypeSpec,
    model::{StoryTime, World},
    narrative::{self, CharacterScope},
    validation::valid_id_for_kind,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use std::collections::BTreeSet;

const MAX_CONTEXTS: usize = 8;
const MAX_FORM_BYTES: usize = 4096;

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}
fn unavailable() -> Error {
    Error::NotFound("social context is unavailable".into())
}

/// Exact authored identities. The optional form is literal text, including an
/// explicitly empty string; it filters spellings without changing sense identity.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub language: String,
    pub sense: Option<String>,
    pub form: Option<String>,
    pub contexts: Vec<String>,
}
impl Selection {
    pub(crate) fn validate(&self) -> Result<()> {
        if !valid_id_for_kind(&self.language, "lore-concept")
            || self
                .sense
                .as_ref()
                .is_some_and(|id| !valid_id_for_kind(id, "lore-concept"))
        {
            return Err(invalid(
                "social language and sense must be exact concept IDs",
            ));
        }
        if self
            .form
            .as_ref()
            .is_some_and(|form| form.len() > MAX_FORM_BYTES)
        {
            return Err(invalid("social form must be at most 4096 UTF-8 bytes"));
        }
        if self.contexts.len() > MAX_CONTEXTS
            || self
                .contexts
                .iter()
                .any(|id| !valid_id_for_kind(id, "lore-concept"))
            || self.contexts.iter().collect::<BTreeSet<_>>().len() != self.contexts.len()
        {
            return Err(invalid(
                "social contexts must be at most eight distinct exact concept IDs",
            ));
        }
        Ok(())
    }
}

fn role(entry: &Json) -> Option<&'static str> {
    match entry["profile"].as_str()? {
        "social-language" => Some("language"),
        "social-sense" => Some("sense"),
        "social-era-spelling" => Some("spelling"),
        "social-script" => Some("script"),
        "social-register" => Some("register"),
        "social-culture" => Some("culture"),
        "social-religion" => Some("religion"),
        "social-membership" => Some("membership"),
        _ => None,
    }
}

fn required<'a>(
    snapshot: &'a family_context::Snapshot,
    id: &str,
    roles: &[&str],
) -> Result<&'a Json> {
    snapshot
        .entries
        .get(id)
        .filter(|entry| role(entry).is_some_and(|role| roles.contains(&role)))
        .ok_or_else(unavailable)
}

fn entity<'a>(value: &'a Json, shape: &Json) -> Option<&'a str> {
    if shape["type"] == "entity"
        && value.as_object().is_some_and(|value| value.len() == 2)
        && value["kind"] == "record"
    {
        value["id"].as_str()
    } else {
        None
    }
}

/// Consult both the declaration and admitted used shape. Familiar names and
/// Ref-shaped literals never become relationships. Missing projected leaves do
/// not expose an inaccessible endpoint or make an unrelated row relevant.
fn reference<'a>(
    snapshot: &family_context::Snapshot,
    entry: &'a Json,
    field: &str,
) -> Option<&'a str> {
    if !matches!(
        snapshot.field_type(entry, field),
        Some(TypeSpec::Entity { .. })
    ) {
        return None;
    }
    entity(
        entry["values"].get(field)?,
        entry["valueTypes"]["fields"].get(field)?,
    )
}

fn references<'a>(
    snapshot: &family_context::Snapshot,
    entry: &'a Json,
    field: &str,
) -> Vec<&'a str> {
    if !matches!(snapshot.field_type(entry, field), Some(TypeSpec::Array { items, .. })
        if matches!(items.as_ref(), TypeSpec::Entity { .. }))
    {
        return Vec::new();
    }
    let (Some(values), Some(shapes)) = (
        entry["values"][field].as_array(),
        entry["valueTypes"]["fields"][field]["items"].as_array(),
    ) else {
        return Vec::new();
    };
    values
        .iter()
        .zip(shapes)
        .filter_map(|(value, shape)| entity(value, shape))
        .collect()
}

fn select(selected: &mut BTreeSet<String>, id: &str) -> Result<()> {
    selected.insert(id.into());
    if selected.len() > family_context::MAX_RECORDS {
        return Err(invalid(
            "social context exceeds 32 records; refusing an incomplete result",
        ));
    }
    Ok(())
}

/// Compose one exact author horizon from separately admitted current source.
/// Only explicit owners contribute assertion/description evidence. Discovered
/// senses, spellings, registers and scripts do not become extra evidence roots.
pub fn context(
    world: &World,
    request: &Selection,
    at: &StoryTime,
    audience: &str,
    view: &str,
) -> Result<Json> {
    request.validate()?;
    let snapshot = family_context::Snapshot::read(world, at, audience, view)?;
    let (selected, owners) = choose(&snapshot, request)?;
    let evidence = family_context::evidence(world, &owners, at, audience, view)?;
    let records = selected
        .iter()
        .map(|id| {
            let item = &snapshot.entries[id];
            json!({"role":role(item).unwrap(),"item":item})
        })
        .collect::<Vec<_>>();
    family_context::finish(json!({
        "protocol":crate::CONTRACT,"perspective":"author","scope":"author-as-of-social-context",
        "at":at,"audience":audience,"view":view,"basis":"authored-social-language-context",
        "selection":request,"count":records.len(),"records":records,
        "canon":evidence.canon,"sections":evidence.sections
    }))
}

fn choose(
    snapshot: &family_context::Snapshot,
    request: &Selection,
) -> Result<(BTreeSet<String>, Vec<String>)> {
    required(&snapshot, &request.language, &["language"])?;
    let mut selected = BTreeSet::from([request.language.clone()]);
    let mut owners = vec![request.language.clone()];
    if let Some(id) = &request.sense {
        let sense = required(&snapshot, id, &["sense"])?;
        if reference(&snapshot, sense, "language") != Some(request.language.as_str()) {
            return Err(unavailable());
        }
        select(&mut selected, id)?;
        owners.push(id.clone());
    }
    for id in &request.contexts {
        required(&snapshot, id, &["culture", "religion", "membership"])?;
        select(&mut selected, id)?;
        owners.push(id.clone());
    }
    for (id, item) in &snapshot.entries {
        let Some(kind @ ("sense" | "spelling" | "register")) = role(item) else {
            continue;
        };
        if reference(&snapshot, item, "language") != Some(request.language.as_str()) {
            continue;
        }
        if kind == "spelling" {
            let Some(sense_id) = reference(&snapshot, item, "sense") else {
                continue;
            };
            if request.sense.as_ref().is_some_and(|id| id != sense_id) {
                continue;
            }
            let sense = required(&snapshot, sense_id, &["sense"])?;
            match reference(&snapshot, sense, "language") {
                None => continue,
                Some(language) if language != request.language => return Err(unavailable()),
                Some(_) => {}
            }
            // A current profile may change a field's type. Only an authored
            // string form supplies lexical matching; numeric wire strings do not.
            if !matches!(snapshot.field_type(item, "form"), Some(TypeSpec::String))
                || item["valueTypes"]["fields"]["form"]["type"] != "string"
                || !item["values"]["form"].is_string()
            {
                continue;
            }
            if request
                .form
                .as_ref()
                .is_some_and(|form| item["values"]["form"] != *form)
            {
                continue;
            }
            select(&mut selected, sense_id)?;
        } else if kind == "sense" && request.sense.as_ref().is_some_and(|sense| sense != id) {
            continue;
        }
        select(&mut selected, id)?;
    }
    // Association expansion is one explicit step from selected language and
    // spelling rows. Cultural links and membership tuples are retained in each
    // native item, not followed as an inferred membership or global graph.
    for id in selected.clone() {
        let item = &snapshot.entries[&id];
        if !matches!(role(item), Some("language" | "spelling")) {
            continue;
        }
        for script in references(&snapshot, item, "scripts") {
            required(&snapshot, script, &["script"])?;
            select(&mut selected, script)?;
        }
        if let Some(register) = reference(&snapshot, item, "register") {
            required(&snapshot, register, &["register"])?;
            select(&mut selected, register)?;
        }
    }
    Ok((selected, owners))
}

/// Exact character selection never grants knowledge of its requested concepts.
pub fn character_context(
    world: &World,
    viewer: &str,
    request: &Selection,
    belief_targets: &[String],
    at: &StoryTime,
) -> Result<Json> {
    request.validate()?;
    family_context::validate_belief_targets(belief_targets)?;
    let scope = CharacterScope::new(world, viewer, at)?;
    character_context_in_scope(world, viewer, request, belief_targets, at, &scope)
}

pub(crate) fn character_context_in_scope(
    world: &World,
    viewer: &str,
    request: &Selection,
    belief_targets: &[String],
    at: &StoryTime,
    scope: &CharacterScope<'_>,
) -> Result<Json> {
    request.validate()?;
    family_context::validate_belief_targets(belief_targets)?;
    family_context::require_context_capabilities(world)?;
    let snapshot = family_context::Snapshot::character(world, viewer, at, scope)?;
    let (selected, owners) = choose(&snapshot, request)?;
    let evidence =
        family_context::character_evidence(world, &owners, viewer, at, belief_targets, scope)?;
    let records = selected
        .iter()
        .map(|id| {
            let item = &snapshot.entries[id];
            json!({"role":role(item).unwrap(),"item":family_context::character_entry(item)})
        })
        .collect::<Vec<_>>();
    let mut result = narrative::envelope(viewer, at, "admitted-social-language-context");
    result["scope"] = json!("character-as-of-social-context");
    result["selection"] = serde_json::to_value(request)?;
    result["selection"]["beliefTargets"] = json!(belief_targets);
    result["count"] = json!(records.len());
    result["records"] = json!(records);
    result["beliefs"] = json!(evidence.beliefs);
    result["sections"] = json!(evidence.sections);
    result["complete"] = json!(true);
    family_context::finish(result)
}
