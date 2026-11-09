//! Independent authored existence statements, without vital or identity inference.
use crate::{
    Error, Result, assertion, description, lore,
    model::{Record, StoryTime, World},
    validation::{self, Diagnostic, valid_id_for_kind},
};
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;

pub const EPISODE_KINDS: &[&str] = &["embodied", "disembodied", "constructed", "other", "unknown"];

/// Current learning records contain words, not a typed existence snapshot.
/// Every target therefore has the same empty character projection.
pub fn character(
    world: &World,
    viewer: &str,
    target_character: &str,
    at: &StoryTime,
    limit: usize,
) -> Result<Json> {
    crate::narrative::limit_argument(limit)?;
    crate::narrative::require_capability(world, "existence-v1")?;
    crate::narrative::require_capability(world, "generational-core-v1")?;
    let _scope = crate::narrative::CharacterScope::new(world, viewer, at)?;
    let mut result = crate::narrative::envelope(viewer, at, "explicitly-learned-existence");
    result["targetId"] = json!(target_character);
    result["count"] = json!(0);
    result["items"] = json!([]);
    result["limit"] = json!(limit);
    result["reason"] = json!("typed-episode-knowledge-unavailable");
    Ok(result)
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}
fn text<'a>(value: &'a Yaml, field: &str) -> Result<&'a str> {
    match value {
        Yaml::String(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(invalid(format!("{field} must be nonblank untagged text"))),
    }
}
fn required<'a>(record: &'a Record, field: &str) -> Result<&'a Yaml> {
    record
        .get(field)
        .ok_or_else(|| invalid(format!("existence episode requires {field}")))
}
fn require_enabled(world: &World) -> Result<()> {
    let capabilities = world.world_record()?.list("capabilities");
    if ["generational-core-v1", "existence-v1"]
        .iter()
        .all(|required| {
            capabilities
                .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(required)))
        })
    {
        Ok(())
    } else {
        Err(invalid(
            "existence episodes require declared generational-core-v1 and existence-v1",
        ))
    }
}

struct Episode<'a> {
    record: &'a Record,
    character_id: &'a str,
    valid: lore::Valid<'a>,
    audience: Vec<&'a str>,
    views: Vec<&'a str>,
}
fn episode<'a>(world: &World, record: &'a Record) -> Result<Episode<'a>> {
    if !valid_id_for_kind(&record.id, "existence-episode") {
        return Err(invalid("existence episode requires an exact existence ID"));
    }
    if record.title.trim().is_empty() || record.title.chars().count() > 256 {
        return Err(invalid(
            "existence episode title must be nonblank and at most 256 characters",
        ));
    }
    if !matches!(record.get("status"), None | Some(Yaml::String(_)))
        || !["canonical", "draft", "retired"].contains(&record.status())
    {
        return Err(invalid(
            "existence episode status must be canonical, draft or retired",
        ));
    }
    if !lore::domain(text(required(record, "domain")?, "domain")?) {
        return Err(invalid(
            "existence episode domain must be a dotted identifier of at most 256 characters",
        ));
    }
    let character_id = text(required(record, "character_id")?, "character_id")?;
    if !valid_id_for_kind(character_id, "character")
        || world
            .records
            .get(character_id)
            .is_none_or(|record| record.kind != "character")
    {
        return Err(invalid(
            "character_id must name an existing exact character",
        ));
    }
    let kind = text(required(record, "episode_kind")?, "episode_kind")?;
    if !EPISODE_KINDS.contains(&kind) {
        return Err(invalid("episode_kind is not a supported authored kind"));
    }
    let valid = lore::validity(required(record, "valid")?, Some(world), "valid")?;
    let lore::Valid::Interval { start, end, .. } = &valid else {
        return Err(invalid("existence episode validity must be an interval"));
    };
    let start_evidence = required(record, "start_evidence")?;
    assertion::validate_evidence_request(start_evidence)
        .map_err(|error| invalid(format!("start_evidence: {error}")))?;
    if let Some(value) = record.get("end_evidence") {
        assertion::validate_evidence_request(value)
            .map_err(|error| invalid(format!("end_evidence: {error}")))?;
    }
    if kind == "unknown" {
        if start.is_some() || end.is_some() {
            return Err(invalid("unknown episode validity must be wholly unbounded"));
        }
        if !start_evidence.as_sequence().unwrap().is_empty() || record.get("end_evidence").is_some()
        {
            return Err(invalid(
                "unknown episode requires empty start evidence and no end evidence",
            ));
        }
    } else {
        if start_evidence.as_sequence().unwrap().is_empty() {
            return Err(invalid(
                "affirmative episode requires nonempty start evidence, including an unbounded start",
            ));
        }
        if end.is_some() {
            if record
                .get("end_evidence")
                .and_then(Yaml::as_sequence)
                .is_none_or(Vec::is_empty)
            {
                return Err(invalid(
                    "bounded episode end requires nonempty end evidence",
                ));
            }
        } else if record.get("end_evidence").is_some() {
            return Err(invalid("unbounded episode end forbids end_evidence"));
        }
    }
    let audience = lore::ordered_labels(required(record, "audience")?, "audience")?;
    let views = lore::ordered_labels(required(record, "perspectives")?, "perspectives")?;
    if record
        .get("provenance")
        .is_some_and(|value| !matches!(value, Yaml::String(_)))
    {
        return Err(invalid("existence provenance must be untagged text"));
    }
    if record.body.len() > 1024 * 1024 {
        return Err(invalid("existence episode body exceeds 1 MiB"));
    }
    Ok(Episode {
        record,
        character_id,
        valid,
        audience,
        views,
    })
}

/// Shared author endpoint admission; callers compose full source validation.
/// A visible episode must also have an independently visible character endpoint.
pub(crate) fn record_visible(
    world: &World,
    record: &Record,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
) -> Result<bool> {
    if record.kind != "existence-episode" {
        return Ok(false);
    }
    require_enabled(world)?;
    let episode = episode(world, record)?;
    if episode.record.status() != "canonical"
        || !episode.audience.contains(&audience)
        || !episode.views.contains(&view)
        || !episode.valid.applies(at)
    {
        return Ok(false);
    }
    description::record_visible(
        world,
        &world.records[episode.character_id],
        at,
        audience,
        view,
    )
}

/// Full local-author admission keeps episode and character scopes independent.
pub(crate) fn full_author_record_visible(
    world: &World,
    record: &Record,
    at: Option<&StoryTime>,
) -> Result<bool> {
    if record.kind != "existence-episode" {
        return Ok(false);
    }
    require_enabled(world)?;
    let episode = episode(world, record)?;
    Ok(episode.record.status() == "canonical"
        && episode
            .audience
            .iter()
            .any(|label| matches!(*label, "author" | "public"))
        && episode
            .views
            .iter()
            .any(|label| matches!(*label, "author" | "ordinary"))
        && episode.valid.applies(at)
        && description::full_author_record_visible(
            world,
            &world.records[episode.character_id],
            at,
        )?)
}

/// All retained episode statuses validate; historical evidence is syntax only.
pub fn validate(world: &World) -> Vec<Diagnostic> {
    world
        .records
        .values()
        .filter(|record| record.kind == "existence-episode")
        .filter_map(|record| {
            require_enabled(world)
                .and_then(|_| episode(world, record).map(|_| ()))
                .err()
                .map(|error| Diagnostic {
                    code: "WDL-EXISTENCE-001".into(),
                    message: error.to_string(),
                    severity: "error".into(),
                    entity_id: Some(record.id.clone()),
                    path: Some(record.source_path.clone()),
                    field: Some("existence".into()),
                })
        })
        .collect()
}

/// Exact admitted episode projection. The caller validates the captured world;
/// unrelated episodes do not consume a consumer's required-item bound.
pub(crate) fn admitted_value(
    world: &World,
    record: &Record,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
) -> Result<Option<Json>> {
    if !record_visible(world, record, at, audience, view)? {
        return Ok(None);
    }
    Ok(Some(episode_value(world, record)?))
}

pub(crate) fn full_author_value(
    world: &World,
    record: &Record,
    at: Option<&StoryTime>,
) -> Result<Option<Json>> {
    if !full_author_record_visible(world, record, at)? {
        return Ok(None);
    }
    Ok(Some(episode_value(world, record)?))
}

fn episode_value(world: &World, record: &Record) -> Result<Json> {
    let mut item = json!({"episodeId":record.id,"title":record.title,
        "characterId":record.string("character_id").unwrap(),"episodeKind":record.string("episode_kind").unwrap(),
        "valid":crate::service::yaml_json(required(record,"valid")?)?,
        "startEvidence":crate::service::yaml_json(required(record,"start_evidence")?)?,
        "evidenceStatus":"unverified-historical","citation":lore::citation(world,record,"record")});
    for (source, output) in [
        ("end_evidence", "endEvidence"),
        ("provenance", "provenance"),
    ] {
        if let Some(value) = record.get(source) {
            item[output] = crate::service::yaml_json(value)?;
        }
    }
    Ok(item)
}

/// Complete author statements for one exact character, dated or all-time.
/// Empty results disclose no canonical endpoint title or existence conclusion.
pub fn list(
    world: &World,
    character_id: &str,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
    limit: usize,
) -> Result<Json> {
    if !(1..=1000).contains(&limit) {
        return Err(Error::InvalidInput(
            "existence result limit must be between 1 and 1000".into(),
        ));
    }
    lore::query_arguments(world, at, audience, view)?;
    require_enabled(world)?;
    if let Some(issue) = validation::validate_world(world)
        .diagnostics
        .iter()
        .find(|issue| issue.severity == "error")
    {
        return Err(invalid(format!(
            "existence source is invalid: {}",
            issue.message
        )));
    }
    let target_visible = match world.records.get(character_id) {
        Some(record) if record.kind == "character" => {
            description::record_visible(world, record, at, audience, view)?
        }
        _ => false,
    };
    let mut items = Vec::new();
    if target_visible {
        for record in world.records.values().filter(|record| {
            record.kind == "existence-episode"
                && record.string("character_id") == Some(character_id)
        }) {
            let Some(item) = admitted_value(world, record, at, audience, view)? else {
                continue;
            };
            if items.len() == limit {
                return Err(Error::InvalidInput(
                    "existence result exceeds the requested limit; refusing an incomplete result"
                        .into(),
                ));
            }
            items.push(item);
        }
    }
    Ok(json!({"protocol":crate::CONTRACT,"perspective":"author",
        "scope":if at.is_some(){"author-as-of-existence"}else{"author-all-time-existence"},
        "basis":"authored-existence-episodes","at":at,"audience":audience,"view":view,
        "characterId":character_id,"count":items.len(),"items":items,"limit":limit}))
}
