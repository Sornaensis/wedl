//! Explicit object profiles govern authored state values and set effects.
//! Generic state reads omit these fields; scoped lore reads project their refs.
use crate::{
    Error, Result,
    lore_types::{Profile, Registry, bounded, token},
    model::{Record, StoryTime, World},
    validation::{Diagnostic, is_record_kind},
};
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::collections::BTreeMap;

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}

#[derive(Clone, Debug, Default)]
pub struct Bindings {
    entries: BTreeMap<String, BTreeMap<String, Profile>>,
}
impl Bindings {
    /// Resolve the closed world map without invoking candidate validation.
    pub fn parse(world: &World) -> Result<Self> {
        let owner = world.world_record()?;
        let Some(value) = owner.get("state_profile_bindings") else {
            return Ok(Self::default());
        };
        if !owner.list("capabilities").is_some_and(|values| {
            values
                .iter()
                .any(|value| value.as_str() == Some("lore-core-v1"))
        }) {
            return Err(invalid(
                "state_profile_bindings requires declared lore-core-v1",
            ));
        }
        bounded(value, "state_profile_bindings")?;
        let Yaml::Mapping(kinds) = value else {
            return Err(invalid("state_profile_bindings must be a mapping"));
        };
        if kinds.is_empty() {
            return Ok(Self::default());
        }
        let declarations = owner
            .get("state_keys")
            .and_then(Yaml::as_mapping)
            .ok_or_else(|| invalid("state_profile_bindings requires declared state_keys"))?;
        let registry = match owner.get("profiles") {
            Some(value) => Registry::parse(value)?,
            None => Registry::default(),
        };
        let mut result = Self::default();
        for (kind, keys) in kinds {
            let kind = kind
                .as_str()
                .ok_or_else(|| invalid("binding kinds must be strings"))?;
            if !is_record_kind(kind) || matches!(kind, "world" | "hypothesis") {
                return Err(invalid(
                    "binding kinds must be known non-world canonical kinds",
                ));
            }
            let declared = declarations
                .get(kind)
                .and_then(Yaml::as_mapping)
                .ok_or_else(|| {
                    invalid(format!("binding kind {kind} has no declared state keys"))
                })?;
            let Yaml::Mapping(keys) = keys else {
                return Err(invalid(format!(
                    "state_profile_bindings.{kind} must be a mapping"
                )));
            };
            let mut selected = BTreeMap::new();
            for (key, id) in keys {
                let key = key
                    .as_str()
                    .filter(|key| !key.trim().is_empty())
                    .ok_or_else(|| invalid("binding keys must be nonblank strings"))?;
                let declaration = declared.get(key).ok_or_else(|| {
                    invalid(format!("binding must name declared state key {kind}.{key}"))
                })?;
                crate::state_schema::validate_definition(declaration)?;
                let id = id
                    .as_str()
                    .filter(|id| token(id))
                    .ok_or_else(|| invalid("binding profile must be an exact profile ID"))?;
                let profile = registry
                    .get(id)
                    .ok_or_else(|| invalid(format!("unknown bound profile {id}")))?;
                selected.insert(key.to_owned(), profile.clone());
            }
            result.entries.insert(kind.to_owned(), selected);
        }
        Ok(result)
    }

    pub fn get(&self, kind: &str, key: &str) -> Option<&Profile> {
        self.entries.get(kind)?.get(key)
    }
}

/// Current resolved profile, or no binding. Ordinary worlds take a cheap path.
pub fn bound_profile(world: &World, kind: &str, key: &str) -> Result<Option<Profile>> {
    Ok(Bindings::parse(world)?.get(kind, key).cloned())
}

fn diagnostic(record: &Record, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        code: "WDL-LORE-STATE-001".into(),
        message: message.into(),
        severity: "error".into(),
        entity_id: Some(record.id.clone()),
        path: Some(record.source_path.clone()),
        field: Some("state_profile_bindings".into()),
    }
}

/// Binding declarations are world-only. Initial values and every event status
/// are checked by the shared state validator using these resolved definitions.
pub fn validate(world: &World) -> Vec<Diagnostic> {
    let Ok(owner) = world.world_record() else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for record in world.records.values() {
        if record.id != owner.id && record.get("state_profile_bindings").is_some() {
            result.push(diagnostic(
                record,
                "state_profile_bindings belongs only on the world",
            ));
        }
    }
    if let Err(error) = Bindings::parse(world) {
        result.push(diagnostic(owner, error.to_string()));
    }
    result
}

/// Closed request grammar. Existing keys, profile declarations and capability
/// are validated on the complete candidate, so an invalid edit stays reviewable.
pub fn validate_request(value: &Yaml) -> Result<()> {
    let check = || -> Result<()> {
        bounded(value, "state_profile_bindings")?;
        let Yaml::Mapping(kinds) = value else {
            return Err(invalid("state_profile_bindings must be a mapping"));
        };
        for (kind, keys) in kinds {
            let kind = kind
                .as_str()
                .ok_or_else(|| invalid("binding kinds must be strings"))?;
            if !is_record_kind(kind) || matches!(kind, "world" | "hypothesis") {
                return Err(invalid(
                    "binding kinds must be known non-world canonical kinds",
                ));
            }
            let Yaml::Mapping(keys) = keys else {
                return Err(invalid(format!(
                    "state_profile_bindings.{kind} must be a mapping"
                )));
            };
            for (key, id) in keys {
                if !key.as_str().is_some_and(|key| !key.trim().is_empty()) {
                    return Err(invalid("binding keys must be nonblank strings"));
                }
                if !id.as_str().is_some_and(token) {
                    return Err(invalid("binding profile must be an exact profile ID"));
                }
            }
        }
        Ok(())
    };
    check().map_err(|error| Error::InvalidInput(error.to_string()))
}

fn reference_paths(
    world: &World,
    profile: &Profile,
    value: &Yaml,
    prefix: &str,
) -> Result<Vec<(String, String)>> {
    Ok(profile
        .references(value, world)?
        .into_iter()
        .map(|(field, id)| (format!("{prefix}{}", &field["values".len()..]), id))
        .collect())
}

/// Exact typed leaves replace ordinary legacy entity walking on bound values.
pub fn references(world: &World, record: &Record) -> Result<Vec<(String, String)>> {
    let bindings = Bindings::parse(world)?;
    let mut result = Vec::new();
    if let Some(initial) = record.get("initial_state").and_then(Yaml::as_mapping) {
        for (key, value) in initial {
            if let Some(key) = key.as_str() {
                if let Some(profile) = bindings.get(&record.kind, key) {
                    result.extend(reference_paths(
                        world,
                        profile,
                        value,
                        &format!("initial_state.{key}"),
                    )?);
                }
            }
        }
    }
    if record.kind == "event" {
        if let Some(effects) = record.list("effects") {
            for (index, effect) in effects.iter().enumerate() {
                if effect.get("operation").and_then(Yaml::as_str) != Some("set") {
                    continue;
                }
                let owner = effect
                    .get("target")
                    .and_then(Yaml::as_str)
                    .and_then(|id| world.records.get(id));
                let key = effect.get("key").and_then(Yaml::as_str);
                if let (Some(owner), Some(key), Some(value)) = (owner, key, effect.get("value")) {
                    if let Some(profile) = bindings.get(&owner.kind, key) {
                        result.extend(reference_paths(
                            world,
                            profile,
                            value,
                            &format!("effects[{index}].value"),
                        )?);
                    }
                }
            }
        }
    }
    Ok(result)
}

/// Complete authored bound state at one exact horizon. Only concept endpoints
/// receive lore visibility filtering; ordinary endpoints retain author scope.
pub fn state(
    world: &World,
    reference: &str,
    at: &StoryTime,
    audience: &str,
    view: &str,
) -> Result<Json> {
    let allowed = crate::lore::allowed_record_ids(world, Some(at), audience, view)?;
    let owner = if let Some(record) = world.records.get(reference) {
        if !allowed.contains(&record.id) {
            return Err(Error::NotFound("lore state owner is unavailable".into()));
        }
        record
    } else {
        let folded = reference.to_lowercase();
        let matches: Vec<_> = world
            .records
            .values()
            .filter(|record| allowed.contains(&record.id))
            .filter(|record| {
                record.title.to_lowercase() == folded
                    || record.list("aliases").is_some_and(|aliases| {
                        aliases
                            .iter()
                            .filter_map(Yaml::as_str)
                            .any(|alias| alias.to_lowercase() == folded)
                    })
            })
            .collect();
        match matches.as_slice() {
            [record] => *record,
            [] => return Err(Error::NotFound("lore state owner is unavailable".into())),
            _ => {
                return Err(Error::Ambiguous(
                    "ambiguous visible lore state owner; use an exact ID".into(),
                ));
            }
        }
    };
    let bindings = Bindings::parse(world)?;
    let (resolved, provenance) = crate::runtime::replay_state(world, &owner.id, at)?;
    let mut values = serde_json::Map::new();
    let mut types = serde_json::Map::new();
    let mut profiles = serde_json::Map::new();
    let mut citations = serde_json::Map::new();
    let mut references = Vec::new();
    for (key, value) in &resolved {
        let Some(profile) = bindings.get(&owner.kind, key) else {
            continue;
        };
        let (projected, shape) = crate::lore::project_value(profile, value, &allowed)?;
        let source = provenance
            .get(key)
            .ok_or_else(|| invalid("bound state source citation is unavailable"))?;
        let source_id = source
            .get("entityId")
            .and_then(Json::as_str)
            .ok_or_else(|| invalid("bound state source record is unavailable"))?;
        let record = world
            .records
            .get(source_id)
            .ok_or_else(|| invalid("bound state source record is unavailable"))?;
        let section = source
            .get("section")
            .and_then(Json::as_str)
            .ok_or_else(|| invalid("bound state source section is unavailable"))?;
        let prefix = if section == "initial_state" {
            format!("initial_state.{key}")
        } else {
            format!("{section}.value")
        };
        for (field, id) in reference_paths(world, profile, value, &prefix)? {
            if allowed.contains(&id) {
                let target = &world.records[&id];
                references.push(json!({"stateKey":key,"sourceField":field,"entityId":id,"kind":target.kind,"title":target.title,
                    "citation":crate::lore::citation(world,record,&field),"targetCitation":crate::lore::citation(world,target,"record")}));
            }
        }
        let mut citation = source.clone();
        citation["section"] = json!(prefix);
        citations.insert(key.clone(), citation);
        values.insert(key.clone(), projected);
        types.insert(key.clone(), shape);
        profiles.insert(key.clone(), json!(profile.id));
    }
    // Clear records actual absence. Keep its authored effect citation without
    // manufacturing a value, value shape or a nonexistent `.value` source field.
    for (key, source) in &provenance {
        if !resolved.contains_key(key) && bindings.get(&owner.kind, key).is_some() {
            citations.insert(key.clone(), source.clone());
        }
    }
    Ok(
        json!({"protocol":crate::CONTRACT,"perspective":"author","scope":"author-as-of-lore","at":at,
        "basis":"authored-bound-state","audience":audience,"view":view,"entityId":owner.id,"kind":owner.kind,"title":owner.title,
        "state":values,"valueTypes":types,"profiles":profiles,"citations":citations,"references":references}),
    )
}
