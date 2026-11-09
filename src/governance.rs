//! Independent authored governance roles over one admitted source snapshot.
//! Profile names, Party tags and supersession pointers confer no authority.
use crate::{
    Error, Result, assertion, description, lore,
    lore_types::{Profile, Registry, TypeSpec, token},
    model::{Record, StoryTime, World},
    spatial, validation,
};
use serde::Serialize;
use serde_json::{Map, Value as Json, json};
use serde_yaml::Value as Yaml;
use std::collections::{BTreeMap, BTreeSet};

const ROLES: [&str; 6] = [
    "territory",
    "party",
    "basis",
    "subjects",
    "obligations",
    "supersedes",
];
const PARTY_KINDS: [&str; 3] = ["character", "organization", "lore-concept"];
const BASES: [&str; 4] = ["legal", "de-facto", "claimed", "contested"];
const MAX_LIMIT: usize = 1000;
const MAX_STATEMENTS: usize = 1000;

/// Learned words for an independently known place. The learning source has no
/// typed jurisdiction, authority basis or Party snapshot to join to canon.
pub fn learned(
    world: &World,
    viewer: &str,
    target: &str,
    at: &StoryTime,
    limit: usize,
) -> Result<Json> {
    crate::narrative::limit_argument(limit)?;
    for capability in [
        "collective-parties-v1",
        "generational-core-v1",
        "lore-core-v1",
    ] {
        crate::narrative::require_capability(world, capability)?;
    }
    let scope = crate::narrative::CharacterScope::new(world, viewer, at)?;
    let mut result = crate::narrative::envelope(viewer, at, "explicitly-learned-governance");
    result["targetId"] = json!(target);
    result["limit"] = json!(limit);
    result["count"] = json!(0);
    result["items"] = json!([]);
    let Some(endpoint) = world.records.get(target) else {
        return Ok(result);
    };
    if !matches!(endpoint.kind.as_str(), "location" | "overlay") || !scope.visible(endpoint)? {
        return Ok(result);
    }
    let mut items = crate::learned_assertions::admitted(world, viewer, at)?;
    if items.len() > limit {
        return Err(Error::InvalidInput(format!(
            "learned governance has more than {limit} admitted claims; refusing an incomplete result"
        )));
    }
    for item in &mut items {
        item["jurisdiction"] = json!("unknown");
        item["basis"] = Json::Null;
        item["parties"] = json!([]);
        item["presentation"] = json!("learned-claim; applicability unassigned");
    }
    result["count"] = json!(items.len());
    result["items"] = json!(items);
    Ok(result)
}

/// Exact current profile and six explicit root-field selections. A null role
/// selects no field; names and familiar profile labels are never inferred.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Binding {
    pub profile: String,
    pub roles: BTreeMap<String, Option<String>>,
}

impl Binding {
    pub fn validate(&self) -> Result<()> {
        if !token(&self.profile) {
            return Err(Error::InvalidInput(
                "governance binding requires an exact lowercase profile ID".into(),
            ));
        }
        if self.roles.len() != ROLES.len()
            || self
                .roles
                .keys()
                .any(|role| !ROLES.contains(&role.as_str()))
        {
            return Err(Error::InvalidInput(
                "governance binding requires exactly territory, party, basis, subjects, obligations and supersedes roles".into(),
            ));
        }
        let mut selected = BTreeSet::new();
        for field in self.roles.values().flatten() {
            if field.is_empty()
                || field.len() > 64
                || !field.as_bytes()[0].is_ascii_lowercase()
                || !field
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
                || !selected.insert(field)
            {
                return Err(Error::InvalidInput(
                    "governance role fields must be distinct lowercase root-field tokens or null"
                        .into(),
                ));
            }
        }
        Ok(())
    }
}

fn require_enabled(world: &World) -> Result<()> {
    let capabilities = world.world_record()?.list("capabilities");
    if [
        "generational-core-v1",
        "lore-core-v1",
        "collective-parties-v1",
    ]
    .iter()
    .all(|required| {
        capabilities
            .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(required)))
    }) {
        Ok(())
    } else {
        Err(Error::Validation(
            "collective parties require declared generational-core-v1, lore-core-v1 and collective-parties-v1".into(),
        ))
    }
}

fn checked_source(world: &World) -> Result<()> {
    require_enabled(world)?;
    if let Some(issue) = validation::validate_world(world)
        .diagnostics
        .iter()
        .find(|issue| issue.severity == "error")
    {
        return Err(Error::Validation(format!(
            "governance source is invalid: {}",
            issue.message
        )));
    }
    Ok(())
}

fn exact_shape<'a>(value: &'a Json, label: &str) -> Result<(&'a str, &'a str)> {
    let object = value.as_object().filter(|object| {
        object.len() == 2 && object.contains_key("kind") && object.contains_key("id")
    });
    object
        .and_then(|object| Some((object["kind"].as_str()?, object["id"].as_str()?)))
        .ok_or_else(|| {
            Error::InvalidInput(format!(
                "{label} requires exactly string kind and id fields"
            ))
        })
}

fn party_tag(record: &Record) -> Option<&'static str> {
    match record.kind.as_str() {
        "character" => Some("character"),
        "organization" => Some("organization"),
        "lore-concept" => Some("concept"),
        _ => None,
    }
}

/// Closed display grammar and actual endpoint identity only. This check grants
/// no audience, character knowledge, tenure, claim or interpersonal authority.
pub fn validate_party(world: &World, value: &Json) -> Result<()> {
    require_enabled(world)?;
    let (kind, id) = exact_shape(value, "Party")?;
    if world.records.get(id).is_some_and(|record| {
        party_tag(record) == Some(kind) && validation::valid_id_for_kind(id, &record.kind)
    }) {
        Ok(())
    } else {
        Err(Error::InvalidInput(
            "Party must match an existing exact character, organization or concept endpoint".into(),
        ))
    }
}

fn party_value(world: &World, reference: &Json) -> Result<Json> {
    let (kind, id) = exact_shape(reference, "record reference")?;
    let endpoint = world
        .records
        .get(id)
        .filter(|record| kind == "record" && validation::valid_id_for_kind(id, &record.kind));
    match endpoint.and_then(|record| Some((record, party_tag(record)?))) {
        Some((record, tag)) => Ok(json!({"kind":tag,"id":record.id})),
        None => Err(Error::InvalidInput(
            "Party reference must resolve to an exact character, organization or lore-concept"
                .into(),
        )),
    }
}

/// Pure source conversion after exact source validation. The caller must admit
/// the endpoint before presentation; this helper does not grant access.
pub fn party_from_reference(world: &World, reference: &Json) -> Result<Json> {
    checked_source(world)?;
    party_value(world, reference)
}

/// Return the unchanged typed source Ref shape. A Party is never a profile value.
pub fn party_reference(world: &World, party: &Json) -> Result<Json> {
    checked_source(world)?;
    validate_party(world, party)?;
    Ok(json!({"kind":"record","id":party["id"]}))
}

fn entity_subset(spec: &TypeSpec, allowed: &[&str]) -> bool {
    matches!(spec, TypeSpec::Entity { kinds }
        if !kinds.is_empty() && kinds.iter().all(|kind| allowed.contains(&kind.as_str())))
}

fn compatible(role: &str, spec: &TypeSpec) -> bool {
    match role {
        "territory" => entity_subset(spec, &["location", "overlay"]),
        "party" => entity_subset(spec, &PARTY_KINDS),
        "subjects" => {
            matches!(spec, TypeSpec::Array { items, .. } if entity_subset(items, &PARTY_KINDS))
        }
        "basis" => matches!(spec, TypeSpec::Enum { values }
            if !values.is_empty() && values.iter().all(|value| matches!(value,
                Yaml::String(value) if BASES.contains(&value.as_str())))),
        "supersedes" => entity_subset(spec, &["lore-concept"]),
        "obligations" => true,
        _ => false,
    }
}

fn role_types<'a>(
    profile: &'a Profile,
    binding: &Binding,
) -> Option<BTreeMap<&'static str, Option<&'a TypeSpec>>> {
    let TypeSpec::Object { required, optional } = &profile.fields else {
        return None;
    };
    let mut types = BTreeMap::new();
    for role in ROLES {
        let selected = match binding.roles[role].as_deref() {
            None => None,
            Some(field) => {
                let spec = required.get(field).or_else(|| optional.get(field))?;
                if !compatible(role, spec) {
                    return None;
                }
                Some(spec)
            }
        };
        types.insert(role, selected);
    }
    Some(types)
}

fn territory_matches(
    world: &World,
    id: &str,
    target: &str,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
) -> Result<bool> {
    let endpoint = &world.records[id];
    match endpoint.kind.as_str() {
        "location" => Ok(id == target),
        "overlay" => spatial::overlay_matches(world, endpoint, target, at, audience, view),
        _ => Ok(false),
    }
}

fn linked(statement: &Json, policy_id: &str) -> bool {
    statement["subject"]["id"].as_str() == Some(policy_id)
        || (statement["valueType"]["type"] == "entity"
            && statement["value"]["id"].as_str() == Some(policy_id))
}

/// Read one complete author result. Missing/incompatible bindings and absent or
/// hidden targets have the same empty shape. All source remains independently
/// validated, and selected typed roles must project in full before inclusion.
pub fn query(
    world: &World,
    target: &str,
    binding: &Binding,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
    limit: usize,
) -> Result<Json> {
    binding.validate()?;
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(Error::InvalidInput(
            "governance limit must be from 1 to 1000".into(),
        ));
    }
    if target.trim().is_empty() || target.len() > 256 || target.contains(['\n', '\r']) {
        return Err(Error::InvalidInput(
            "governance target must be an exact nonblank record ID".into(),
        ));
    }
    lore::query_arguments(world, at, audience, view)?;
    checked_source(world)?;
    let mut result = json!({"protocol":crate::CONTRACT,"perspective":"author",
        "scope":if at.is_some(){"author-as-of-governance"}else{"author-all-time-governance"},
        "at":at,"audience":audience,"view":view,"basis":"authored-governance",
        "targetId":target,"binding":binding,"count":0,"items":[]});
    let Some(endpoint) = world.records.get(target) else {
        return Ok(result);
    };
    if !matches!(endpoint.kind.as_str(), "location" | "overlay")
        || !description::record_visible(world, endpoint, at, audience, view)?
    {
        return Ok(result);
    }
    let registry = world
        .world_record()?
        .get("profiles")
        .map(Registry::parse)
        .transpose()?
        .unwrap_or_default();
    let Some(types) = registry
        .get(&binding.profile)
        .and_then(|profile| role_types(profile, binding))
    else {
        return Ok(result);
    };
    let mut allowed = BTreeSet::new();
    for record in world
        .records
        .values()
        .filter(|record| record.kind != "assertion")
    {
        if description::record_visible(world, record, at, audience, view)? {
            allowed.insert(record.id.clone());
        }
    }
    let statements = assertion::admitted_values(world, at, audience, view)?;
    let mut statement_count = 0usize;
    let mut items = Vec::new();
    'policies: for record in world.records.values().filter(|record| {
        record.kind == "lore-concept"
            && record.string("profile") == Some(binding.profile.as_str())
            && allowed.contains(&record.id)
    }) {
        let values = record.get("values").unwrap();
        let mut roles = Map::new();
        let mut known = Map::new();
        let mut shapes = Map::new();
        let mut citations = Map::new();
        let mut references = Vec::new();
        for role in ROLES {
            roles.insert(role.into(), Json::Null);
            known.insert(role.into(), json!(false));
            shapes.insert(role.into(), Json::Null);
            citations.insert(role.into(), Json::Null);
            let Some(field) = binding.roles[role].as_deref() else {
                continue;
            };
            let Some(raw) = values.get(field) else {
                continue;
            };
            let spec = types[role].unwrap();
            let refs = spec.references(raw, world)?;
            if refs.iter().any(|(_, id)| !allowed.contains(id)) {
                continue 'policies;
            }
            let Some((value, shape)) = lore::projected(spec, raw, &allowed)? else {
                continue 'policies;
            };
            roles.insert(role.into(), value);
            known.insert(role.into(), json!(true));
            shapes.insert(role.into(), shape);
            citations.insert(
                role.into(),
                lore::citation(world, record, &format!("values.{field}")),
            );
            for (path, id) in refs {
                let source_field =
                    format!("values.{field}{}", path.strip_prefix("values").unwrap());
                let endpoint = &world.records[&id];
                references.push(json!({"role":role,"sourceField":source_field,"entityId":id,
                    "kind":endpoint.kind,"title":endpoint.title,
                    "citation":lore::citation(world,record,&source_field),
                    "targetCitation":lore::citation(world,endpoint,"record")}));
            }
        }
        let jurisdiction = if let Some(id) = roles["territory"]["id"].as_str() {
            if !territory_matches(world, id, target, at, audience, view)? {
                continue;
            }
            "authored"
        } else {
            "unknown"
        };
        let mut parties = Vec::new();
        if !roles["party"].is_null() {
            parties.push(party_value(world, &roles["party"])?);
        }
        if let Some(subjects) = roles["subjects"].as_array() {
            for subject in subjects {
                parties.push(party_value(world, subject)?);
            }
        }
        let linked = statements
            .iter()
            .filter(|statement| linked(statement, &record.id))
            .cloned()
            .collect::<Vec<_>>();
        statement_count += linked.len();
        if statement_count > MAX_STATEMENTS {
            return Err(Error::InvalidInput(
                "governance exceeds 1000 linked statement occurrences; refusing an incomplete result"
                    .into(),
            ));
        }
        let supersedes = roles["supersedes"]["id"].as_str().map(str::to_owned);
        let mut item = json!({"conceptId":record.id,"title":record.title,"profile":binding.profile,
            "jurisdiction":jurisdiction,"roles":roles,"roleKnown":known,"roleTypes":shapes,
            "roleCitations":citations,"parties":parties,
            "valid":crate::service::yaml_json(record.get("valid").unwrap())?,
            "statements":linked,"references":references,"citation":lore::citation(world,record,"record")});
        if let Some(id) = supersedes {
            item["supersedesCitation"] = lore::citation(world, &world.records[&id], "record");
        }
        items.push(item);
        if items.len() > limit {
            return Err(Error::InvalidInput(format!(
                "governance has more than {limit} matching policies; refusing an incomplete result"
            )));
        }
    }
    result["count"] = json!(items.len());
    result["items"] = json!(items);
    Ok(result)
}
