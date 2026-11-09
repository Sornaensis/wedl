//! Explicitly learned proposition words, separate from canonical assertion truth.
//! Acquisition time controls admission; author applicability and corrections do not.
use crate::{
    Error, Result,
    lore_types::bounded,
    model::{Record, StoryTime, World},
    narrative,
    validation::{self, Diagnostic, valid_id_for_kind},
};
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;

const AFFIRMATIVE: &[&str] = &["accepted", "suspected", "uncertain", "remembered"];
const STATES: &[&str] = &[
    "accepted",
    "suspected",
    "rejected",
    "uncertain",
    "remembered",
    "forgotten",
];

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}
fn malformed() -> Error {
    invalid("character evidence is malformed; use author validation for diagnostics")
}
fn linked(record: &Record) -> bool {
    record.kind == "knowledge"
        && record
            .get("claim")
            .and_then(|claim| claim.get("assertion_id"))
            .is_some()
}
fn enabled(world: &World) -> bool {
    let capabilities = world
        .world_record()
        .ok()
        .and_then(|record| record.list("capabilities"));
    ["lore-core-v1", "assertions-v1"].iter().all(|required| {
        capabilities.is_some_and(|values| values.iter().any(|v| v.as_str() == Some(required)))
    })
}
fn require_enabled(world: &World) -> Result<()> {
    if enabled(world) {
        Ok(())
    } else {
        Err(invalid(
            "learned assertion reads require declared lore-core-v1 and assertions-v1",
        ))
    }
}
fn text<'a>(value: Option<&'a Yaml>, field: &str) -> Result<&'a str> {
    match value {
        Some(Yaml::String(value)) if !value.trim().is_empty() => Ok(value),
        _ => Err(invalid(format!("{field}: requires nonblank untagged text"))),
    }
}

struct Claim<'a> {
    knower: &'a str,
    key: &'a str,
    statement: &'a str,
    assertion: &'a str,
}
fn claim<'a>(world: &World, record: &'a Record, references: bool) -> Result<Claim<'a>> {
    let raw = record
        .get("claim")
        .ok_or_else(|| invalid("claim: required field is missing"))?;
    bounded(raw, "claim")?;
    let Yaml::Mapping(fields) = raw else {
        return Err(invalid("claim: requires a closed mapping"));
    };
    if fields.len() != 3
        || fields.keys().any(|key| {
            !matches!(key,Yaml::String(key) if ["key","statement","assertion_id"].contains(&key.as_str()))
        })
    {
        return Err(invalid(
            "claim: linked assertions require exactly key, statement and assertion_id",
        ));
    }
    let key = text(fields.get("key"), "claim.key")?;
    let statement = text(fields.get("statement"), "claim.statement")?;
    let assertion = text(fields.get("assertion_id"), "claim.assertion_id")?;
    let knower = text(record.get("knower"), "knower")?;
    for (id, kind, field) in [
        (assertion, "assertion", "claim.assertion_id"),
        (knower, "character", "knower"),
    ] {
        if !valid_id_for_kind(id, kind)
            || (references && world.records.get(id).is_none_or(|r| r.kind != kind))
        {
            return Err(invalid(format!(
                "{field}: requires an existing exact {kind} ID"
            )));
        }
    }
    Ok(Claim {
        knower,
        key,
        statement,
        assertion,
    })
}

/// Linked-claim additions to ordinary knowledge validation. Core knowledge rules
/// continue to validate transitions, their ordering and causal source references.
pub fn validate(world: &World) -> Vec<Diagnostic> {
    world
        .records
        .values()
        .filter(|record| linked(record))
        .filter_map(|record| {
            let result = (|| {
                require_enabled(world)?;
                let claim = claim(world, record, true)?;
                narrative::labels_match(record, claim.knower)?;
                Ok::<_, Error>(())
            })();
            result.err().map(|error| Diagnostic {
                code: "WDL-LEARNED-ASSERTION-001".into(),
                message: error.to_string(),
                severity: "error".into(),
                entity_id: Some(record.id.clone()),
                path: Some(record.source_path.clone()),
                field: Some("claim".into()),
            })
        })
        .collect()
}

fn select(world: &World, viewer: &str, at: &StoryTime) -> Result<()> {
    if !valid_id_for_kind(viewer, "character")
        || !world
            .records
            .get(viewer)
            .is_some_and(|record| record.kind == "character")
    {
        return Err(Error::NotFound("character view is unavailable".into()));
    }
    world.story_time(at.tick, Some(&at.timeline), at.order)?;
    Ok(())
}

struct Held<'a> {
    id: &'a str,
    state: &'a str,
    confidence: f64,
    time: StoryTime,
}
fn held<'a>(world: &World, record: &'a Record, at: &StoryTime) -> Result<Option<Held<'a>>> {
    let transitions: &[Yaml] = match record.get("transitions") {
        None => &[],
        Some(Yaml::Sequence(values)) => values,
        _ => return Err(malformed()),
    };
    let mut learned = false;
    let mut latest: Option<Held<'a>> = None;
    for transition in transitions {
        let time = StoryTime::from_yaml(
            transition.get("time").ok_or_else(malformed)?,
            &world.default_timeline(),
        )
        .map_err(|_| malformed())?;
        world
            .story_time(time.tick, Some(&time.timeline), time.order)
            .map_err(|_| malformed())?;
        if !time.not_after(at) {
            continue;
        }
        let state = text(transition.get("state"), "state").map_err(|_| malformed())?;
        if !STATES.contains(&state) {
            return Err(malformed());
        }
        learned |= AFFIRMATIVE.contains(&state);
        let id = text(transition.get("id"), "id").map_err(|_| malformed())?;
        let confidence = transition
            .get("confidence")
            .and_then(Yaml::as_f64)
            .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
            .ok_or_else(malformed)?;
        if latest.as_ref().is_none_or(|previous| {
            (time.tick, time.order) > (previous.time.tick, previous.time.order)
        }) {
            latest = Some(Held {
                id,
                state,
                confidence,
                time,
            });
        }
    }
    Ok(latest.filter(|held| learned && held.state != "forgotten"))
}

/// Shared learned-word projection with no pre-filter result budget. Callers bound
/// their own admitted target, search corpus or complete character packet.
pub(crate) fn admitted(world: &World, viewer: &str, at: &StoryTime) -> Result<Vec<Json>> {
    select(world, viewer, at)?;
    if !world.records.values().any(linked) {
        return Ok(Vec::new());
    }
    require_enabled(world).map_err(|_| malformed())?;
    let mut result = Vec::new();
    for record in world.records.values().filter(|record| {
        linked(record) && record.status() == "canonical" && record.string("knower") == Some(viewer)
    }) {
        if !narrative::labels_match(record, viewer).map_err(|_| malformed())? {
            continue;
        }
        let Some(held) = held(world, record, at)? else {
            continue;
        };
        // The source validator checks the live link. Character projection only
        // reads the character's own literal claim, never the canonical target.
        let claim = claim(world, record, false).map_err(|_| malformed())?;
        let value = json!({"knowledgeId":record.id,"transitionId":held.id,
            "assertionId":claim.assertion,"claim":{"key":claim.key,"statement":claim.statement,"assertion_id":claim.assertion},
            "statement":claim.statement,"state":held.state,"confidence":held.confidence,"time":held.time,
            "target":{"kind":"transition","ownerId":record.id,"id":held.id},"citations":[]});
        result.push((held.time.tick, held.time.order, &record.id, held.id, value));
    }
    result.sort_by(|a, b| (&a.0, &a.1, a.2, a.3).cmp(&(&b.0, &b.1, b.2, b.3)));
    Ok(result
        .into_iter()
        .map(|(_, _, _, _, value)| value)
        .collect())
}

/// Complete explicitly learned claims for an exact assertion or knowledge ID.
pub fn list(
    world: &World,
    viewer: &str,
    target: &str,
    at: &StoryTime,
    limit: usize,
) -> Result<Json> {
    if !(1..=1000).contains(&limit) {
        return Err(Error::InvalidInput(
            "learned assertion limit must be from 1 to 1000".into(),
        ));
    }
    if target.trim().is_empty() || target.len() > 256 || target.contains(['\r', '\n']) {
        return Err(Error::InvalidInput(
            "learned assertion target must be an exact nonblank record ID".into(),
        ));
    }
    select(world, viewer, at)?;
    require_enabled(world)?;
    if !validation::validate_world(world).valid {
        return Err(malformed());
    }
    let items = admitted(world, viewer, at)?
        .into_iter()
        .filter(|value| value["assertionId"] == target || value["knowledgeId"] == target)
        .collect::<Vec<_>>();
    if items.len() > limit {
        return Err(Error::InvalidInput(format!(
            "learned assertions have more than {limit} matching claims; refusing an incomplete result"
        )));
    }
    Ok(json!({"protocol":crate::CONTRACT,"perspective":"character",
        "scope":"explicit-authored-evidence","basis":"explicitly-learned-assertions",
        "characterId":viewer,"effectiveTime":at,"targetId":target,"count":items.len(),
        "items":items,"complete":true,"limit":limit}))
}
