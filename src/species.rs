//! Explicit common and individual descriptions composed from admitted lore.
//! Profile names and selected fields have no built-in biological meaning.
use crate::{
    Error, Result, family_context,
    lore_types::{TypeSpec, token},
    model::{StoryTime, World},
    narrative::{self, CharacterScope},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use std::collections::{BTreeMap, BTreeSet};

const MAX_DEPTH: usize = 16;
const MAX_RECORDS: usize = 64;
const MAX_LINK_VISITS: usize = 65_536;
const ROLES: [&str; 7] = [
    "taxonomy",
    "life_stage",
    "habitat",
    "traits",
    "culture",
    "language",
    "religion",
];

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}
fn field_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Every role and reference field is selected by the caller, without defaults.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub common_profile: String,
    pub individual_profile: String,
    pub individual_link: String,
    pub common_link: Option<String>,
    #[serde(default)]
    pub common_fields: BTreeMap<String, String>,
    #[serde(default)]
    pub individual_fields: BTreeMap<String, String>,
}
impl Selection {
    pub fn validate(&self) -> Result<()> {
        if !token(&self.common_profile) || !token(&self.individual_profile) {
            return Err(invalid("species selections require exact profile IDs"));
        }
        if !field_token(&self.individual_link)
            || self.common_link.as_deref().is_some_and(|v| !field_token(v))
        {
            return Err(invalid(
                "species link selectors must be exact root field names",
            ));
        }
        for fields in [&self.common_fields, &self.individual_fields] {
            if fields
                .iter()
                .any(|(role, field)| !ROLES.contains(&role.as_str()) || !field_token(field))
            {
                return Err(invalid(
                    "species fields map taxonomy, life_stage, habitat, traits, culture, language or religion to exact root field names",
                ));
            }
        }
        Ok(())
    }
}

fn snapshot(
    mut snapshot: family_context::Snapshot,
    selected: &Selection,
) -> Result<family_context::Snapshot> {
    if !snapshot.profile_exists(&selected.common_profile)
        || !snapshot.profile_exists(&selected.individual_profile)
    {
        return Err(invalid(
            "species selections require existing exact profile IDs",
        ));
    }
    snapshot.entries.retain(|_, entry| {
        entry["profile"] == selected.common_profile
            || entry["profile"] == selected.individual_profile
    });
    Ok(snapshot)
}
fn selected_entry<'a>(
    snapshot: &'a family_context::Snapshot,
    id: &str,
    profile: &str,
) -> Result<&'a Json> {
    snapshot
        .entries
        .get(id)
        .filter(|entry| entry["profile"] == profile)
        .ok_or_else(|| Error::NotFound("species description is unavailable".into()))
}

fn unknown(owner: &Json, field: &str, reason: &str) -> Json {
    json!({"ownerId":owner["conceptId"],"sourceField":field,"reason":reason})
}
fn is_reference(value: &Json, shape: &Json) -> bool {
    shape["type"] == "entity"
        && value.as_object().is_some_and(|v| v.len() == 2)
        && value["kind"] == "record"
        && value["id"].is_string()
}
struct Link<'a> {
    id: &'a str,
    source_field: &'a str,
}
/// Inspect only the projected values and shapes. A withheld array member never
/// creates an extra unknown marker or exposes its former source position.
fn links<'a>(
    entry: &'a Json,
    field: &str,
    declared: Option<&TypeSpec>,
) -> std::result::Result<Vec<Link<'a>>, &'static str> {
    match declared {
        Some(TypeSpec::Entity { .. }) => {}
        Some(TypeSpec::Array { items, .. })
            if matches!(items.as_ref(), TypeSpec::Entity { .. }) => {}
        Some(_) => return Err("not-typed-reference"),
        None => return Err("absent-or-unavailable"),
    }
    let Some(value) = entry["values"].get(field) else {
        return Err("absent-or-unavailable");
    };
    let Some(shape) = entry["valueTypes"]["fields"].get(field) else {
        return Err("absent-or-unavailable");
    };
    let ids: Vec<_> = if is_reference(value, shape) {
        vec![value["id"].as_str().unwrap()]
    } else if let (Some(values), Some(shapes)) = (value.as_array(), shape["items"].as_array()) {
        if shape["type"] != "array"
            || values.len() != shapes.len()
            || !values.iter().zip(shapes).all(|(v, s)| is_reference(v, s))
        {
            return Err("not-typed-reference");
        }
        values.iter().map(|v| v["id"].as_str().unwrap()).collect()
    } else {
        return Err("not-typed-reference");
    };
    let scalar = format!("values.{field}.id");
    let array = format!("values.{field}[");
    let mut references = entry["references"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|reference| {
            let path = reference["sourceField"].as_str().unwrap();
            path == scalar || path.starts_with(&array)
        });
    ids.into_iter()
        .map(|id| {
            let reference = references
                .find(|reference| reference["entityId"] == id)
                .ok_or("absent-or-unavailable")?;
            Ok(Link {
                id,
                source_field: reference["sourceField"].as_str().unwrap(),
            })
        })
        .collect()
}

struct Composition<'a> {
    snapshot: &'a family_context::Snapshot,
    selected: &'a Selection,
    common: Vec<&'a Json>,
    unknowns: Vec<Json>,
    active: BTreeSet<String>,
    done: BTreeSet<String>,
    visited: BTreeSet<String>,
    link_visits: usize,
}
impl<'a> Composition<'a> {
    fn build(
        snapshot: &'a family_context::Snapshot,
        selected: &'a Selection,
        individual: &'a Json,
    ) -> Result<Self> {
        let id = individual["conceptId"].as_str().unwrap().to_owned();
        let mut result = Self {
            snapshot,
            selected,
            common: Vec::new(),
            unknowns: Vec::new(),
            active: BTreeSet::from([id.clone()]),
            done: BTreeSet::new(),
            visited: BTreeSet::from([id]),
            link_visits: 0,
        };
        result.follow(individual, &selected.individual_link, 1)?;
        Ok(result)
    }
    fn follow(&mut self, owner: &'a Json, field: &str, depth: usize) -> Result<()> {
        match links(owner, field, self.snapshot.field_type(owner, field)) {
            Ok(links) => {
                for link in links {
                    self.visit(owner, link, depth)?;
                }
            }
            Err(reason) => self
                .unknowns
                .push(unknown(owner, &format!("values.{field}"), reason)),
        }
        Ok(())
    }
    fn visit(&mut self, owner: &Json, link: Link<'a>, depth: usize) -> Result<()> {
        self.link_visits += 1;
        if self.link_visits > MAX_LINK_VISITS {
            return Err(invalid(
                "species links exceed 65536 visits; refusing an incomplete composition",
            ));
        }
        if self.active.contains(link.id) {
            return Err(invalid(
                "species common links contain a cycle; refusing an incomplete composition",
            ));
        }
        if depth > MAX_DEPTH {
            return Err(invalid(
                "species composition exceeds depth 16 or 64 records; refusing an incomplete composition",
            ));
        }
        if self.done.contains(link.id) {
            return Ok(());
        }
        if self.visited.len() >= MAX_RECORDS {
            return Err(invalid(
                "species composition exceeds depth 16 or 64 records; refusing an incomplete composition",
            ));
        }
        self.visited.insert(link.id.into());
        let Some(entry) = self
            .snapshot
            .entries
            .get(link.id)
            .filter(|entry| entry["profile"] == self.selected.common_profile)
        else {
            self.unknowns.push(unknown(
                owner,
                link.source_field,
                "target-profile-unavailable",
            ));
            self.done.insert(link.id.into());
            return Ok(());
        };
        self.active.insert(link.id.into());
        self.common.push(entry);
        if let Some(field) = &self.selected.common_link {
            self.follow(entry, field, depth + 1)?;
        }
        self.active.remove(link.id);
        self.done.insert(link.id.into());
        Ok(())
    }
}

fn statement(entry: &Json, fields: &BTreeMap<String, String>, character: bool) -> Json {
    let mut selected_fields = serde_json::Map::new();
    for role in ROLES {
        let field = fields.get(role);
        let mut result = json!({"known":false,"field":field});
        match field {
            None => result["reason"] = json!("unselected"),
            Some(field) => match (
                entry["values"].get(field),
                entry["valueTypes"]["fields"].get(field),
            ) {
                (Some(value), Some(shape)) => {
                    result["known"] = json!(true);
                    result["value"] = value.clone();
                    result["valueType"] = shape.clone();
                    if !character {
                        let mut citation = entry["citation"].clone();
                        citation["section"] = json!(format!("values.{field}"));
                        result["citation"] = citation;
                    }
                    let prefix = format!("values.{field}");
                    result["references"] = json!(
                        entry["references"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .filter(|reference| {
                                let path = reference["sourceField"].as_str().unwrap();
                                path.strip_prefix(&prefix).is_some_and(|suffix| {
                                    suffix.starts_with('.') || suffix.starts_with('[')
                                })
                            })
                            .collect::<Vec<_>>()
                    );
                    if character {
                        result = family_context::character_entry(&result);
                    }
                }
                _ => result["reason"] = json!("not-supplied-or-visible"),
            },
        }
        selected_fields.insert(role.into(), result);
    }
    if character {
        json!({"conceptId":entry["conceptId"],"profile":entry["profile"],
            "fields":selected_fields,"target":entry["target"]})
    } else {
        json!({"conceptId":entry["conceptId"],"title":entry["title"],"profile":entry["profile"],
            "fields":selected_fields,"body":entry["body"],"bodyKind":"undated-authored-prose","citation":entry["citation"]})
    }
}

/// Character fields describe the selected root without exposing source positions.
fn character_unknowns(unknowns: Vec<Json>) -> Json {
    json!(
        unknowns
            .into_iter()
            .map(|unknown| {
                let field = unknown["sourceField"]
                    .as_str()
                    .unwrap()
                    .strip_prefix("values.")
                    .unwrap()
                    .split(['[', '.'])
                    .next()
                    .unwrap();
                json!({"ownerId":unknown["ownerId"],"field":field,"reason":unknown["reason"]})
            })
            .collect::<Vec<_>>()
    )
}
fn envelope(
    selected: &Selection,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
    basis: &str,
) -> Json {
    json!({"protocol":crate::CONTRACT,"perspective":"author",
        "scope":if at.is_some(){"author-as-of-lore"}else{"author-all-time-lore"},
        "at":at,"audience":audience,"view":view,"selection":selected,"basis":basis})
}

/// Common statements and one authored individual variation, without precedence.
pub fn compose(
    world: &World,
    individual_id: &str,
    selected: &Selection,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
) -> Result<Json> {
    selected.validate()?;
    let snapshot = snapshot(
        family_context::Snapshot::author(world, at, audience, view)?,
        selected,
    )?;
    let individual = selected_entry(&snapshot, individual_id, &selected.individual_profile)?;
    let composition = Composition::build(&snapshot, selected, individual)?;
    let owners = std::iter::once(individual)
        .chain(composition.common.iter().copied())
        .map(|entry| entry["conceptId"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let evidence = family_context::author_optional(world, &owners, at, audience, view)?;
    let mut result = envelope(
        selected,
        at,
        audience,
        view,
        "authored-common-individual-descriptions",
    );
    result["individual"] = statement(individual, &selected.individual_fields, false);
    result["common"] = json!(
        composition
            .common
            .iter()
            .map(|entry| statement(entry, &selected.common_fields, false))
            .collect::<Vec<_>>()
    );
    result["linkageComplete"] = json!(composition.unknowns.is_empty());
    result["unknowns"] = json!(composition.unknowns);
    result["canon"] = json!(evidence.canon);
    result["sections"] = json!(evidence.sections);
    Ok(result)
}

/// Only admitted variation records with a confirmed explicit path are returned.
pub fn affected(
    world: &World,
    common_id: &str,
    selected: &Selection,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
    limit: usize,
) -> Result<Json> {
    if !(1..=1000).contains(&limit) {
        return Err(invalid("species limit must be from 1 to 1000"));
    }
    selected.validate()?;
    let snapshot = snapshot(
        family_context::Snapshot::author(world, at, audience, view)?,
        selected,
    )?;
    let common = selected_entry(&snapshot, common_id, &selected.common_profile)?;
    let mut items = Vec::new();
    let mut owners = vec![common_id.to_owned()];
    for individual in snapshot
        .entries
        .values()
        .filter(|entry| entry["profile"] == selected.individual_profile)
    {
        let composition = Composition::build(&snapshot, selected, individual)?;
        if composition
            .common
            .iter()
            .any(|entry| entry["conceptId"] == common_id)
        {
            if items.len() == limit {
                return Err(invalid(format!(
                    "species has more than {limit} matching individuals; refusing an incomplete result"
                )));
            }
            let mut item = statement(individual, &selected.individual_fields, false);
            item["linkageComplete"] = json!(composition.unknowns.is_empty());
            item["unknowns"] = json!(composition.unknowns);
            items.push(item);
            let id = individual["conceptId"].as_str().unwrap();
            if id != common_id {
                owners.push(id.to_owned());
            }
        }
    }
    let mut result = envelope(
        selected,
        at,
        audience,
        view,
        "explicitly-linked-individual-descriptions",
    );
    let evidence = family_context::author_optional(world, &owners, at, audience, view)?;
    result["common"] = statement(common, &selected.common_fields, false);
    result["count"] = json!(items.len());
    result["items"] = json!(items);
    result["limit"] = json!(limit);
    result["canon"] = json!(evidence.canon);
    result["sections"] = json!(evidence.sections);
    Ok(result)
}

/// Common and individual statements require independent acquired-record admission.
pub fn character_compose(
    world: &World,
    viewer: &str,
    individual_id: &str,
    selected: &Selection,
    belief_targets: &[String],
    at: &StoryTime,
) -> Result<Json> {
    let scope = CharacterScope::new(world, viewer, at)?;
    character_compose_in_scope(
        world,
        viewer,
        individual_id,
        selected,
        belief_targets,
        at,
        &scope,
    )
}

pub(crate) fn character_compose_in_scope(
    world: &World,
    viewer: &str,
    individual_id: &str,
    selected: &Selection,
    belief_targets: &[String],
    at: &StoryTime,
    scope: &CharacterScope<'_>,
) -> Result<Json> {
    selected.validate()?;
    family_context::validate_belief_targets(belief_targets)?;
    let snapshot = snapshot(
        family_context::Snapshot::character(world, viewer, at, scope)?,
        selected,
    )?;
    let individual = selected_entry(&snapshot, individual_id, &selected.individual_profile)?;
    let composition = Composition::build(&snapshot, selected, individual)?;
    let owners = std::iter::once(individual)
        .chain(composition.common.iter().copied())
        .map(|entry| entry["conceptId"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let evidence =
        family_context::character_evidence(world, &owners, viewer, at, belief_targets, scope)?;
    let mut result = narrative::envelope(viewer, at, "admitted-common-individual-descriptions");
    result["selection"] = json!(selected);
    result["individual"] = statement(individual, &selected.individual_fields, true);
    result["common"] = json!(
        composition
            .common
            .iter()
            .map(|entry| statement(entry, &selected.common_fields, true))
            .collect::<Vec<_>>()
    );
    result["linkageComplete"] = json!(composition.unknowns.is_empty());
    result["unknowns"] = character_unknowns(composition.unknowns);
    result["beliefs"] = json!(evidence.beliefs);
    result["sections"] = json!(evidence.sections);
    result["complete"] = json!(true);
    family_context::finish(result)
}

/// An admitted common description has only independently known affected records.
pub fn character_affected(
    world: &World,
    viewer: &str,
    common_id: &str,
    selected: &Selection,
    at: &StoryTime,
    limit: usize,
) -> Result<Json> {
    if !(1..=1000).contains(&limit) {
        return Err(invalid("species limit must be from 1 to 1000"));
    }
    selected.validate()?;
    let scope = CharacterScope::new(world, viewer, at)?;
    let snapshot = snapshot(
        family_context::Snapshot::character(world, viewer, at, &scope)?,
        selected,
    )?;
    let common = selected_entry(&snapshot, common_id, &selected.common_profile)?;
    let mut items = Vec::new();
    let mut owners = vec![common_id.to_owned()];
    for individual in snapshot
        .entries
        .values()
        .filter(|entry| entry["profile"] == selected.individual_profile)
    {
        let composition = Composition::build(&snapshot, selected, individual)?;
        if composition
            .common
            .iter()
            .any(|entry| entry["conceptId"] == common_id)
        {
            if items.len() == limit {
                return Err(invalid(format!(
                    "species has more than {limit} matching individuals; refusing an incomplete result"
                )));
            }
            let mut item = statement(individual, &selected.individual_fields, true);
            item["linkageComplete"] = json!(composition.unknowns.is_empty());
            item["unknowns"] = character_unknowns(composition.unknowns);
            items.push(item);
            let id = individual["conceptId"].as_str().unwrap();
            if id != common_id {
                owners.push(id.to_owned());
            }
        }
    }
    let evidence = family_context::character_evidence(world, &owners, viewer, at, &[], &scope)?;
    let mut result = narrative::envelope(viewer, at, "admitted-linked-individual-descriptions");
    result["selection"] = json!(selected);
    result["common"] = statement(common, &selected.common_fields, true);
    result["count"] = json!(items.len());
    result["items"] = json!(items);
    result["limit"] = json!(limit);
    result["sections"] = json!(evidence.sections);
    result["complete"] = json!(true);
    family_context::finish(result)
}
