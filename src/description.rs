//! Explicit dated Markdown sections. Opaque body prose is never extracted or dated.
//! Owner/member scope precedes correction suppression and complete-result bounds.
use crate::{
    Error, Result, lore,
    lore_types::bounded,
    model::{Record, StoryTime, World},
    validation::{Diagnostic, is_record_kind},
};
use serde_json::{Value as Json, json};
use serde_yaml::{Mapping, Value as Yaml};
use std::collections::{BTreeMap, BTreeSet};

const CAPABILITY: &str = "descriptions-v1";
const MAX_MARKDOWN_BYTES: usize = 1024 * 1024;
const MAX_LIMIT: usize = 1000;

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}
fn unavailable() -> Error {
    Error::NotFound("description section or owner is unavailable".into())
}
fn enabled(world: &World) -> bool {
    world
        .world_record()
        .ok()
        .and_then(|r| r.list("capabilities"))
        .is_some_and(|v| v.iter().any(|v| v.as_str() == Some(CAPABILITY)))
}
fn member_id(value: &str) -> bool {
    value.strip_prefix("description_").is_some_and(|tail| {
        tail.len() == 26
            && tail
                .bytes()
                .all(|b| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&b))
    })
}
fn member_fields<'a>(value: &'a Yaml, field: &str) -> Result<&'a Mapping> {
    let Yaml::Mapping(fields) = value else {
        return Err(invalid(format!("{field} must be an untagged mapping")));
    };
    let required = [
        "id",
        "heading",
        "markdown",
        "valid",
        "audience",
        "perspectives",
    ];
    if required.iter().any(|key| !fields.contains_key(*key))
        || fields.keys().any(|key| !matches!(key,Yaml::String(key) if required.contains(&key.as_str()) || key == "replaces")) {
        return Err(invalid(format!("{field} has a closed description member contract")));
    }
    Ok(fields)
}

struct Member<'a> {
    index: usize,
    id: &'a str,
    heading: &'a str,
    markdown: &'a str,
    valid: lore::Valid<'a>,
    valid_source: &'a Yaml,
    audience: Vec<&'a str>,
    views: Vec<&'a str>,
    replaces: Option<&'a str>,
}
fn members<'a>(value: &'a Yaml, world: Option<&World>) -> Result<Vec<Member<'a>>> {
    bounded(value, "descriptions")?;
    let Yaml::Sequence(values) = value else {
        return Err(invalid("descriptions must be an ordered array"));
    };
    let mut result = Vec::with_capacity(values.len());
    let mut earlier = BTreeMap::<&str, &Mapping>::new();
    for (index, value) in values.iter().enumerate() {
        let field = format!("descriptions[{index}]");
        let fields = member_fields(value, &field)?;
        let id = fields["id"]
            .as_str()
            .filter(|id| member_id(id))
            .ok_or_else(|| invalid(format!("{field}.id must be an exact description ID")))?;
        if earlier.contains_key(id) {
            return Err(invalid(format!("{field}.id must be locally unique")));
        }
        let heading = fields["heading"]
            .as_str()
            .filter(|value| !value.trim().is_empty() && value.chars().count() <= 256)
            .ok_or_else(|| {
                invalid(format!(
                    "{field}.heading must be nonblank text of at most 256 characters"
                ))
            })?;
        let markdown = fields["markdown"]
            .as_str()
            .ok_or_else(|| invalid(format!("{field}.markdown must be untagged text")))?;
        if markdown.len() > MAX_MARKDOWN_BYTES {
            return Err(invalid(format!("{field}.markdown exceeds 1 MiB")));
        }
        let valid_source = &fields["valid"];
        let valid = lore::validity(valid_source, world, &format!("{field}.valid"))?;
        let audience = lore::ordered_labels(&fields["audience"], &format!("{field}.audience"))?;
        let views =
            lore::ordered_labels(&fields["perspectives"], &format!("{field}.perspectives"))?;
        let replaces = match fields.get("replaces") {
            None => None,
            Some(Yaml::String(target)) => {
                let prior = earlier.get(target.as_str()).ok_or_else(|| {
                    invalid(format!(
                        "{field}.replaces must name an earlier same-owner section"
                    ))
                })?;
                for key in ["valid", "audience", "perspectives"] {
                    if fields[key] != prior[key] {
                        return Err(invalid(format!(
                            "{field}.replaces must retain exact validity and scope labels"
                        )));
                    }
                }
                Some(target.as_str())
            }
            Some(_) => {
                return Err(invalid(format!(
                    "{field}.replaces must be an exact earlier section ID"
                )));
            }
        };
        earlier.insert(id, fields);
        result.push(Member {
            index,
            id,
            heading,
            markdown,
            valid,
            valid_source,
            audience,
            views,
            replaces,
        });
    }
    Ok(result)
}

/// Request grammar is pure; candidate validation also checks world timelines/capability.
pub fn validate_request(value: &Yaml) -> Result<()> {
    members(value, None)
        .map(|_| ())
        .map_err(|e| Error::InvalidInput(e.to_string()))
}
/// Validate every authored section, including currently invisible source statuses.
pub fn validate(world: &World) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for record in world.records.values() {
        let Some(value) = record.get("descriptions") else {
            continue;
        };
        let error = if !enabled(world) {
            Some(invalid(
                "description sections require declared descriptions-v1",
            ))
        } else if record.kind == "hypothesis" || !is_record_kind(&record.kind) {
            Some(invalid(
                "description sections require a canonical owner kind",
            ))
        } else {
            members(value, Some(world)).err()
        };
        if let Some(error) = error {
            out.push(Diagnostic {
                code: "WDL-DESCRIPTION-001".into(),
                message: error.to_string(),
                severity: "error".into(),
                entity_id: Some(record.id.clone()),
                path: Some(record.source_path.clone()),
                field: Some("descriptions".into()),
            });
        }
    }
    out
}

fn owner_label_matches(record: &Record, field: &str, selected: &str) -> Result<bool> {
    match record.get(field) {
        None => Ok(true),
        Some(Yaml::Sequence(labels)) if labels.iter().all(|v| matches!(v, Yaml::String(_))) => {
            Ok(labels.iter().any(|label| label.as_str() == Some(selected)))
        }
        Some(_) => Err(invalid(
            "narrative owner scope labels must be string arrays",
        )),
    }
}
/// Shared narrative endpoint visibility, independent of description installation.
/// Ordinary owners retain author availability; concepts, briefs and episodes
/// additionally keep their explicit canonical/applicability boundary.
pub(crate) fn record_visible(
    world: &World,
    record: &Record,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
) -> Result<bool> {
    if record.kind == "existence-episode" {
        return crate::existence::record_visible(world, record, at, audience, view);
    }
    if record.kind == "hypothesis"
        || !is_record_kind(&record.kind)
        || !owner_label_matches(record, "audience", audience)?
        || !owner_label_matches(record, "perspectives", view)?
    {
        return Ok(false);
    }
    if matches!(record.kind.as_str(), "lore-concept" | "context-brief") {
        if record.status() != "canonical" {
            return Ok(false);
        }
        let valid = record
            .get("valid")
            .ok_or_else(|| invalid("narrative owner validity is required"))?;
        return Ok(lore::validity(valid, Some(world), "valid")?.applies(at));
    }
    Ok(true)
}
fn read<'a>(
    world: &'a World,
    owner_id: &str,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
) -> Result<&'a Record> {
    if !enabled(world) {
        return Err(invalid(
            "description reads require declared descriptions-v1",
        ));
    }
    lore::query_arguments(world, at, audience, view)?;
    if let Some(error) = validate(world).first() {
        return Err(invalid(error.message.clone()));
    }
    let owner = world.records.get(owner_id).ok_or_else(unavailable)?;
    if !record_visible(world, owner, at, audience, view)? {
        return Err(unavailable());
    }
    Ok(owner)
}
fn selected<'a>(
    world: &World,
    owner: &'a Record,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
) -> Result<Vec<Member<'a>>> {
    let Some(value) = owner.get("descriptions") else {
        return Ok(Vec::new());
    };
    let eligible: Vec<_> = members(value, Some(world))?
        .into_iter()
        .filter(|m| {
            m.audience.contains(&audience) && m.views.contains(&view) && m.valid.applies(at)
        })
        .collect();
    let replaced: BTreeSet<_> = if at.is_some() {
        eligible.iter().filter_map(|m| m.replaces).collect()
    } else {
        BTreeSet::new()
    };
    Ok(eligible
        .into_iter()
        .filter(|m| !replaced.contains(m.id))
        .collect())
}

/// Borrowed prose for a separately bounded author retrieval projection. The
/// caller validates the complete source before selecting or copying any text.
pub(crate) struct AuthorSection<'a> {
    pub index: usize,
    pub id: &'a str,
    pub heading: &'a str,
    pub markdown: &'a str,
}

/// Complete local-author scope. Owners and referenced endpoints may carry
/// different admitted labels; no one audience/view pair must admit them all.
pub(crate) fn full_author_record_visible(
    world: &World,
    owner: &Record,
    at: Option<&StoryTime>,
) -> Result<bool> {
    if owner.kind == "existence-episode" {
        return crate::existence::full_author_record_visible(world, owner, at);
    }
    for audience in ["author", "public"] {
        for view in ["author", "ordinary"] {
            if record_visible(world, owner, at, audience, view)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub(crate) fn author_record_visible(world: &World, owner: &Record, at: &StoryTime) -> Result<bool> {
    full_author_record_visible(world, owner, Some(at))
}

fn full_author_members<'a>(
    world: &World,
    owner: &'a Record,
    at: Option<&StoryTime>,
) -> Result<Vec<Member<'a>>> {
    let Some(value) = owner.get("descriptions") else {
        return Ok(Vec::new());
    };
    let eligible: Vec<_> = members(value, Some(world))?
        .into_iter()
        .filter(|member| {
            member
                .audience
                .iter()
                .any(|label| matches!(*label, "author" | "public"))
                && member
                    .views
                    .iter()
                    .any(|label| matches!(*label, "author" | "ordinary"))
                && member.valid.applies(at)
        })
        .collect();
    let suppressed: BTreeSet<_> = if at.is_some() {
        eligible
            .iter()
            .filter_map(|member| member.replaces)
            .collect()
    } else {
        BTreeSet::new()
    };
    Ok(eligible
        .into_iter()
        .filter(|member| !suppressed.contains(member.id))
        .collect())
}

pub(crate) fn full_author_values(
    world: &World,
    owner: &Record,
    at: Option<&StoryTime>,
) -> Result<Vec<Json>> {
    if !full_author_record_visible(world, owner, at)? {
        return Ok(Vec::new());
    }
    full_author_members(world, owner, at)?
        .iter()
        .map(|member| {
            let mut result = value(world, owner, member)?;
            result["ownerId"] = json!(owner.id);
            Ok(result)
        })
        .collect()
}

pub(crate) fn author_sections<'a>(
    world: &World,
    owner: &'a Record,
    at: &StoryTime,
) -> Result<Vec<AuthorSection<'a>>> {
    Ok(full_author_members(world, owner, Some(at))?
        .into_iter()
        .map(|member| AuthorSection {
            index: member.index,
            id: member.id,
            heading: member.heading,
            markdown: member.markdown,
        })
        .collect())
}
fn envelope(owner: &Record, at: Option<&StoryTime>, audience: &str, view: &str) -> Json {
    json!({"protocol":crate::CONTRACT,"perspective":"author",
        "scope":if at.is_some(){"author-as-of-descriptions"}else{"author-all-time-descriptions"},
        "at":at,"audience":audience,"view":view,"basis":"authored-description-sections",
        "ownerId":owner.id,"ownerKind":owner.kind,"title":owner.title})
}
fn value(world: &World, owner: &Record, member: &Member<'_>) -> Result<Json> {
    let mut result = json!({"sectionId":member.id,"heading":member.heading,"markdown":member.markdown,
        "valid":crate::service::yaml_json(member.valid_source)?,
        "citation":lore::citation(world,owner,&format!("descriptions[{}]",member.index)),
        "target":{"kind":"section","ownerId":owner.id,"sectionId":member.id}});
    if let Some(replaces) = member.replaces {
        result["replaces"] = json!(replaces);
    }
    Ok(result)
}

/// Complete admitted descriptions; no horizon retains the full admitted correction history.
pub fn list(
    world: &World,
    owner_id: &str,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
    limit: usize,
) -> Result<Json> {
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(Error::InvalidInput(
            "description limit must be from 1 to 1000".into(),
        ));
    }
    let owner = read(world, owner_id, at, audience, view)?;
    let members = selected(world, owner, at, audience, view)?;
    if members.len() > limit {
        return Err(Error::InvalidInput(format!(
            "descriptions have more than {limit} matching sections; refusing an incomplete result"
        )));
    }
    let mut result = envelope(owner, at, audience, view);
    result["count"] = json!(members.len());
    result["items"] = Json::Array(
        members
            .iter()
            .map(|m| value(world, owner, m))
            .collect::<Result<_>>()?,
    );
    Ok(result)
}
/// Resolve one stable local section in the same admitted/history context as list.
pub fn section(
    world: &World,
    owner_id: &str,
    section_id: &str,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
) -> Result<Json> {
    let owner = read(world, owner_id, at, audience, view)?;
    let members = selected(world, owner, at, audience, view)?;
    let member = members
        .iter()
        .find(|m| m.id == section_id)
        .ok_or_else(unavailable)?;
    let mut result = envelope(owner, at, audience, view);
    result
        .as_object_mut()
        .unwrap()
        .extend(value(world, owner, member)?.as_object().unwrap().clone());
    Ok(result)
}

/// Known-owner character projection. Admission precedes corrections and serialization.
pub(crate) fn character_values(
    world: &World,
    owner: &Record,
    viewer: &str,
    at: &StoryTime,
) -> Result<Vec<Json>> {
    let Some(value) = owner.get("descriptions") else {
        return Ok(Vec::new());
    };
    let eligible = members(value, Some(world))?
        .into_iter()
        .filter(|member| {
            member
                .audience
                .iter()
                .any(|label| *label == "public" || *label == viewer)
                && member
                    .views
                    .iter()
                    .any(|label| *label == "ordinary" || *label == viewer)
                && member.valid.applies(Some(at))
        })
        .collect::<Vec<_>>();
    let ids = eligible
        .iter()
        .map(|member| member.id)
        .collect::<BTreeSet<_>>();
    let suppressed = eligible
        .iter()
        .filter_map(|member| member.replaces)
        .collect::<BTreeSet<_>>();
    eligible
        .iter()
        .filter(|member| !suppressed.contains(member.id))
        .map(|member| {
            let mut item = json!({"ownerId":owner.id,"sectionId":member.id,
            "heading":member.heading,"markdown":member.markdown,
            "valid":crate::service::yaml_json(member.valid_source)?,
            "target":{"kind":"section","ownerId":owner.id,"sectionId":member.id}});
            if let Some(previous) = member.replaces.filter(|id| ids.contains(id)) {
                item["replaces"] = json!(previous);
            }
            Ok(item)
        })
        .collect()
}
