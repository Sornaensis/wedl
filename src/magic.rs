//! Explicit narrative roles composed from admitted finite lore profiles.
//! Restrictions and exceptions remain independent authored statements.
use crate::{
    Error, Result, family_context,
    lore_types::{TypeSpec, token},
    model::{StoryTime, World},
    narrative::{self, CharacterScope},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use std::collections::{BTreeMap, BTreeSet};

const ROLES: [&str; 4] = ["tradition", "practice", "realm", "artifact"];
const MAX_DEPTH: usize = 16;
const MAX_RECORDS: usize = 64;
const MAX_LINK_VISITS: usize = 1024;
const MAX_LINK_FIELDS: usize = 16;

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

/// The caller explicitly assigns current profile IDs and authored root fields.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub profiles: BTreeMap<String, String>,
    #[serde(default)]
    pub links: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub restrictions: BTreeMap<String, String>,
    #[serde(default)]
    pub exceptions: BTreeMap<String, String>,
}
impl Selection {
    pub fn validate(&self) -> Result<()> {
        if self.profiles.is_empty()
            || self
                .profiles
                .iter()
                .any(|(role, profile)| !ROLES.contains(&role.as_str()) || !token(profile))
        {
            return Err(invalid(
                "magic profiles must assign existing exact profile IDs to tradition, practice, realm or artifact",
            ));
        }
        if self.profiles.values().collect::<BTreeSet<_>>().len() != self.profiles.len() {
            return Err(invalid("assign each exact magic profile to only one role"));
        }
        for (role, fields) in &self.links {
            if !self.profiles.contains_key(role)
                || fields.len() > MAX_LINK_FIELDS
                || fields.iter().any(|field| !field_token(field))
                || fields.iter().collect::<BTreeSet<_>>().len() != fields.len()
            {
                return Err(invalid(
                    "magic links require a selected role and at most sixteen distinct exact root fields per role",
                ));
            }
        }
        for fields in [&self.restrictions, &self.exceptions] {
            if fields
                .iter()
                .any(|(role, field)| !self.profiles.contains_key(role) || !field_token(field))
            {
                return Err(invalid(
                    "magic restrictions and exceptions map selected roles to exact root fields",
                ));
            }
        }
        Ok(())
    }
    fn role(&self, entry: &Json) -> Option<&str> {
        self.profiles
            .iter()
            .find(|(_, profile)| entry["profile"] == profile.as_str())
            .map(|(role, _)| role.as_str())
    }
}

fn unavailable() -> Error {
    Error::NotFound("magic description is unavailable".into())
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
/// A declared entity descriptor is necessary even for an empty projected array.
/// Only admitted values and their original source citations supply actual edges.
fn links<'a>(
    entry: &'a Json,
    field: &str,
    declared: Option<&TypeSpec>,
) -> std::result::Result<Vec<&'a Json>, &'static str> {
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
            references
                .find(|reference| reference["entityId"] == id)
                .ok_or("absent-or-unavailable")
        })
        .collect()
}

struct Composition<'a> {
    snapshot: &'a family_context::Snapshot,
    selected: &'a Selection,
    records: Vec<&'a Json>,
    links: Vec<Json>,
    unknowns: Vec<Json>,
    active: BTreeSet<String>,
    done: BTreeSet<String>,
    visits: usize,
}
impl<'a> Composition<'a> {
    fn visit(&mut self, id: &str, depth: usize, incoming: Option<(&Json, &Json)>) -> Result<()> {
        if self.active.contains(id) {
            return Err(invalid(
                "magic typed links contain a cycle; refusing an incomplete composition",
            ));
        }
        self.visits += 1;
        if self.visits > MAX_LINK_VISITS {
            return Err(invalid(
                "magic links exceed 1024 visits; refusing an incomplete composition",
            ));
        }
        if depth > MAX_DEPTH {
            return Err(invalid(
                "magic composition exceeds depth 16; refusing an incomplete composition",
            ));
        }
        if self.done.contains(id) {
            return Ok(());
        }
        if self.active.len() + self.done.len() >= MAX_RECORDS {
            return Err(invalid(
                "magic composition exceeds 64 records; refusing an incomplete composition",
            ));
        }
        let Some(entry) = self.snapshot.entries.get(id) else {
            let Some((owner, reference)) = incoming else {
                return Err(unavailable());
            };
            self.unknowns.push(unknown(
                owner,
                reference["sourceField"].as_str().unwrap(),
                "target-profile-unavailable",
            ));
            self.done.insert(id.into());
            return Ok(());
        };
        self.active.insert(id.into());
        self.records.push(entry);
        let role = self.selected.role(entry).unwrap();
        if let Some(fields) = self.selected.links.get(role) {
            for field in fields {
                match links(entry, field, self.snapshot.field_type(entry, field)) {
                    Ok(references) => {
                        for reference in references {
                            let mut edge = reference.clone();
                            edge["ownerId"] = entry["conceptId"].clone();
                            self.links.push(edge);
                            self.visit(
                                reference["entityId"].as_str().unwrap(),
                                depth + 1,
                                Some((entry, reference)),
                            )?;
                        }
                    }
                    Err(reason) => {
                        self.unknowns
                            .push(unknown(entry, &format!("values.{field}"), reason))
                    }
                }
            }
        }
        self.active.remove(id);
        self.done.insert(id.into());
        Ok(())
    }
}

fn field_statement(entry: &Json, field: Option<&String>) -> Json {
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
                let mut citation = entry["citation"].clone();
                citation["section"] = json!(format!("values.{field}"));
                result["citation"] = citation;
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
            }
            _ => result["reason"] = json!("not-supplied-or-visible"),
        },
    }
    result
}
fn statement(entry: &Json, selected: &Selection) -> Json {
    let role = selected.role(entry).unwrap();
    let mut result = json!({"role":role,"conceptId":entry["conceptId"],"title":entry["title"],"profile":entry["profile"],
        "fields":{"restrictions":field_statement(entry,selected.restrictions.get(role)),"exceptions":field_statement(entry,selected.exceptions.get(role))},
        "body":entry["body"],"bodyKind":"undated-authored-prose","citation":entry["citation"]});
    if let Some(provenance) = entry.get("provenance") {
        result["provenance"] = provenance.clone();
    }
    result
}

/// Explicitly linked authored descriptions, without inferred costs or precedence.
pub fn compose(
    world: &World,
    root_id: &str,
    selected: &Selection,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
) -> Result<Json> {
    selected.validate()?;
    let mut snapshot = family_context::Snapshot::author(world, at, audience, view)?;
    if selected
        .profiles
        .values()
        .any(|id| !snapshot.profile_exists(id))
    {
        return Err(invalid(
            "magic selections require existing exact profile IDs",
        ));
    }
    snapshot
        .entries
        .retain(|_, entry| selected.role(entry).is_some());
    if !snapshot.entries.contains_key(root_id) {
        return Err(unavailable());
    }
    let mut composition = Composition {
        snapshot: &snapshot,
        selected,
        records: Vec::new(),
        links: Vec::new(),
        unknowns: Vec::new(),
        active: BTreeSet::new(),
        done: BTreeSet::new(),
        visits: 0,
    };
    composition.visit(root_id, 0, None)?;
    let owners = composition
        .records
        .iter()
        .map(|entry| entry["conceptId"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let evidence = family_context::author_optional(world, &owners, at, audience, view)?;
    Ok(json!({"protocol":crate::CONTRACT,"perspective":"author",
        "scope":if at.is_some(){"author-as-of-lore"}else{"author-all-time-lore"},
        "at":at,"audience":audience,"view":view,"selection":selected,
        "basis":"explicitly-linked-magic-descriptions","rootId":root_id,
        "records":composition.records.iter().map(|entry|statement(entry,selected)).collect::<Vec<_>>(),
        "links":composition.links,"unknowns":composition.unknowns,
        "canon":evidence.canon,"sections":evidence.sections,
        "linkageComplete":composition.unknowns.is_empty()}))
}

/// Explicit learned explanations, independent of current canonical profiles.
pub fn character(
    world: &World,
    viewer: &str,
    belief_targets: &[String],
    at: &StoryTime,
) -> Result<Json> {
    let scope = CharacterScope::new(world, viewer, at)?;
    character_in_scope(world, viewer, belief_targets, at, &scope)
}

pub(crate) fn character_in_scope(
    world: &World,
    viewer: &str,
    belief_targets: &[String],
    at: &StoryTime,
    scope: &CharacterScope<'_>,
) -> Result<Json> {
    if belief_targets.is_empty() {
        return Err(invalid(
            "magic character reading requires one to eight explicit belief targets",
        ));
    }
    let evidence =
        family_context::character_evidence(world, &[], viewer, at, belief_targets, scope)?;
    let count = evidence
        .beliefs
        .iter()
        .map(|stream| stream["result"]["items"].as_array().unwrap().len())
        .sum::<usize>();
    let mut result = narrative::envelope(viewer, at, "explicitly-learned-magic-explanations");
    result["beliefTargets"] = json!(belief_targets);
    result["beliefs"] = json!(evidence.beliefs);
    result["count"] = json!(count);
    result["outcome"] = json!(if count == 0 { "unknown" } else { "available" });
    result["complete"] = json!(true);
    family_context::finish(result)
}
