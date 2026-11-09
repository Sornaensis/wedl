//! Shared admission and complete-result bounds for native family context readers.
use crate::{
    Error, Result, assertion, description, learned_assertions, lore,
    lore_types::{Registry, TypeSpec},
    model::{StoryTime, World},
    narrative,
    narrative::CharacterScope,
    validation,
};
use serde_json::{Value as Json, json};
use std::{collections::BTreeMap, io};

pub(crate) const MAX_RECORDS: usize = 32;
pub(crate) const MAX_EVIDENCE_ITEMS: usize = 512;
pub(crate) const MAX_REPORT_BYTES: usize = 262_144;

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

pub(crate) fn require_context_capabilities(world: &World) -> Result<()> {
    if ["lore-core-v1", "assertions-v1", "descriptions-v1"]
        .iter()
        .any(|name| !enabled(world, name))
    {
        return Err(Error::Validation(
            "family contexts require declared lore-core-v1, assertions-v1 and descriptions-v1"
                .into(),
        ));
    }
    Ok(())
}

pub(crate) struct Snapshot {
    pub entries: BTreeMap<String, Json>,
    profiles: Registry,
}

impl Snapshot {
    pub fn read(world: &World, at: &StoryTime, audience: &str, view: &str) -> Result<Self> {
        require_context_capabilities(world)?;
        Self::author(world, Some(at), audience, view)
    }

    pub fn author(
        world: &World,
        at: Option<&StoryTime>,
        audience: &str,
        view: &str,
    ) -> Result<Self> {
        let world_record = world.world_record()?;
        let entries = lore::scoped_concept_values(world, at, audience, view)?
            .into_iter()
            .map(|entry| (entry["conceptId"].as_str().unwrap().to_owned(), entry))
            .collect();
        let profiles = match world_record.get("profiles") {
            Some(value) => Registry::parse(value)?,
            None => Registry::default(),
        };
        Ok(Self { entries, profiles })
    }

    /// Derive knowledge inside one captured source, never from request grants.
    /// Reference paths exist only for internal family traversal and address the
    /// already projected value. A withheld source array member has no position.
    pub fn character(
        world: &World,
        viewer: &str,
        at: &StoryTime,
        scope: &CharacterScope<'_>,
    ) -> Result<Self> {
        if !scope.matches(world, viewer, at) {
            return Err(Error::Validation(
                "family context scope does not match its captured read".into(),
            ));
        }
        let mut entries = BTreeMap::new();
        for mut entry in lore::character_values(world, &scope.allowed_ids()?, at)? {
            let mut paths = Vec::new();
            projected_references(&entry["values"], &entry["valueTypes"], "values", &mut paths);
            let references = paths
                .into_iter()
                .map(|(path, id)| {
                    json!({"sourceField":path,"entityId":id,"kind":world.records[&id].kind,
                    "target":{"kind":"record","id":id}})
                })
                .collect::<Vec<_>>();
            entry["references"] = json!(references);
            entries.insert(entry["conceptId"].as_str().unwrap().to_owned(), entry);
        }
        let profiles = match world.world_record()?.get("profiles") {
            Some(value) => Registry::parse(value)?,
            None => Registry::default(),
        };
        Ok(Self { entries, profiles })
    }

    pub fn profile_exists(&self, id: &str) -> bool {
        self.profiles.get(id).is_some()
    }

    pub fn field_type(&self, entry: &Json, field: &str) -> Option<&TypeSpec> {
        let profile = self.profiles.get(entry["profile"].as_str()?)?;
        match &profile.fields {
            TypeSpec::Object { required, optional } => {
                required.get(field).or_else(|| optional.get(field))
            }
            _ => None,
        }
    }
}

fn projected_references(value: &Json, shape: &Json, path: &str, out: &mut Vec<(String, String)>) {
    match shape["type"].as_str() {
        Some("entity") => {
            if let Some(id) = value["id"].as_str() {
                out.push((format!("{path}.id"), id.into()));
            }
        }
        Some("object") => {
            if let Some(fields) = shape["fields"].as_object() {
                for (field, child) in fields {
                    projected_references(&value[field], child, &format!("{path}.{field}"), out);
                }
            }
        }
        Some("array") => {
            if let (Some(values), Some(shapes)) = (value.as_array(), shape["items"].as_array()) {
                for (index, (value, shape)) in values.iter().zip(shapes).enumerate() {
                    projected_references(value, shape, &format!("{path}[{index}]"), out);
                }
            }
        }
        _ => {}
    }
}

/// Public character items retain exact typed identities without source paths.
pub(crate) fn character_entry(entry: &Json) -> Json {
    let mut item = entry.clone();
    if let Some(references) = item["references"].as_array_mut() {
        for reference in references {
            reference.as_object_mut().unwrap().remove("sourceField");
        }
    }
    item
}

pub(crate) struct Evidence {
    pub canon: Vec<Json>,
    pub sections: Vec<Json>,
}

pub(crate) fn evidence(
    world: &World,
    owners: &[String],
    at: &StoryTime,
    audience: &str,
    view: &str,
) -> Result<Evidence> {
    author_evidence(world, owners, Some(at), audience, view, true, true)
}

/// Existing lore-only species/magic reads do not require newly enabled streams.
pub(crate) fn author_optional(
    world: &World,
    owners: &[String],
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
) -> Result<Evidence> {
    author_evidence(
        world,
        owners,
        at,
        audience,
        view,
        enabled(world, "assertions-v1"),
        enabled(world, "descriptions-v1"),
    )
}

fn author_evidence(
    world: &World,
    owners: &[String],
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
    assertions_enabled: bool,
    descriptions_enabled: bool,
) -> Result<Evidence> {
    let mut canon = Vec::new();
    let mut sections = Vec::new();
    let mut count = 0usize;
    for owner in owners {
        if assertions_enabled {
            let assertions =
                assertion::as_of(world, owner, at, audience, view, MAX_EVIDENCE_ITEMS)?;
            count += assertions["items"].as_array().unwrap().len();
            canon.push(json!({"ownerId":owner,"result":assertions}));
        }
        if descriptions_enabled {
            let descriptions =
                description::list(world, owner, at, audience, view, MAX_EVIDENCE_ITEMS)?;
            count += descriptions["items"].as_array().unwrap().len();
            sections.push(json!({"ownerId":owner,"result":descriptions}));
        }
        if count > MAX_EVIDENCE_ITEMS {
            return Err(Error::InvalidInput(
                "family context exceeds the 512-item evidence limit".into(),
            ));
        }
    }
    Ok(Evidence { canon, sections })
}

pub(crate) struct CharacterEvidence {
    pub beliefs: Vec<Json>,
    pub sections: Vec<Json>,
}

pub(crate) fn validate_belief_targets(targets: &[String]) -> Result<()> {
    if targets.len() > 8
        || targets.iter().any(|id| {
            !validation::valid_id_for_kind(id, "assertion")
                && !validation::valid_id_for_kind(id, "knowledge")
        })
        || targets
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != targets.len()
    {
        return Err(Error::InvalidInput(
            "belief targets require at most eight distinct exact assertion or knowledge IDs".into(),
        ));
    }
    Ok(())
}

/// Literal learned targets remain an independent stream. Canonical assertion
/// subjects, classifications, values and correction links are never consulted.
pub(crate) fn character_evidence(
    world: &World,
    owners: &[String],
    viewer: &str,
    at: &StoryTime,
    belief_targets: &[String],
    scope: &CharacterScope<'_>,
) -> Result<CharacterEvidence> {
    validate_belief_targets(belief_targets)?;
    if !scope.matches(world, viewer, at) {
        return Err(Error::Validation(
            "family context scope does not match its captured read".into(),
        ));
    }
    let mut count = 0usize;
    let mut sections = Vec::new();
    for owner in owners {
        let record = world
            .records
            .get(owner)
            .ok_or_else(narrative::unavailable)?;
        if !scope.visible(record)? {
            return Err(narrative::unavailable());
        }
        if enabled(world, "descriptions-v1") {
            let items = description::character_values(world, record, viewer, at)?;
            count += items.len();
            let mut result = narrative::envelope(viewer, at, "admitted-description-sections");
            result["ownerId"] = json!(owner);
            result["count"] = json!(items.len());
            result["items"] = json!(items);
            result["limit"] = json!(MAX_EVIDENCE_ITEMS);
            sections.push(json!({"ownerId":owner,"result":result}));
        }
    }
    let held = if belief_targets.is_empty() {
        Vec::new()
    } else {
        narrative::require_capability(world, "lore-core-v1")?;
        narrative::require_capability(world, "assertions-v1")?;
        learned_assertions::admitted(world, viewer, at)?
    };
    let mut beliefs = Vec::new();
    for target in belief_targets {
        let items = held
            .iter()
            .filter(|item| item["assertionId"] == *target || item["knowledgeId"] == *target)
            .cloned()
            .collect::<Vec<_>>();
        count += items.len();
        let result = json!({"protocol":crate::CONTRACT,"perspective":"character",
            "scope":"explicit-authored-evidence","basis":"explicitly-learned-assertions",
            "characterId":viewer,"effectiveTime":at,"targetId":target,"count":items.len(),
            "items":items,"complete":true,"limit":MAX_EVIDENCE_ITEMS});
        beliefs.push(json!({"targetId":target,"result":result}));
    }
    if count > MAX_EVIDENCE_ITEMS {
        return Err(Error::InvalidInput(
            "family context exceeds the 512-item evidence limit".into(),
        ));
    }
    Ok(CharacterEvidence { beliefs, sections })
}

/// Count actual compact JSON bytes without allocating another serialized report.
/// The final stdout newline belongs to the same complete-report budget.
pub(crate) fn finish(result: Json) -> Result<Json> {
    struct Counter(usize);
    impl io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.0 {
                return Err(io::Error::other("context report limit exceeded"));
            }
            self.0 -= bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(&mut Counter(MAX_REPORT_BYTES - 1), &result).map_err(|_| {
        Error::InvalidInput("family context exceeds the 262144-byte report limit".into())
    })?;
    Ok(result)
}
