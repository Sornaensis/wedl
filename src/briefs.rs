//! Explicit authored briefs and all-or-nothing required source pins.
//! Selecting a pin never grants character knowledge or imports untimed body prose.
use crate::{
    Error, Result, assertion, description, generational, learned_assertions, lore,
    model::{Record, StoryTime, World},
    narrative,
    validation::{self, Diagnostic},
};
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::collections::BTreeSet;

const CAPABILITY: &str = "briefs-v1";
const MAX_REFS: usize = 1024;
const MAX_WORK: usize = 16384;
const GENERATIONAL: &[&str] = &[
    "parentage",
    "union",
    "organization",
    "affiliation",
    "legacy",
    "tenure",
    "claim",
    "vital-history",
];

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}
fn malformed() -> Error {
    invalid("brief source is invalid; use author validation for diagnostics")
}
fn enabled(world: &World) -> bool {
    world
        .world_record()
        .ok()
        .and_then(|r| r.list("capabilities"))
        .is_some_and(|values| values.iter().any(|v| v.as_str() == Some(CAPABILITY)))
}
fn text<'a>(value: &'a Yaml, field: &str) -> Result<&'a str> {
    match value {
        Yaml::String(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(invalid(format!("{field} must be nonblank text"))),
    }
}
fn required<'a>(record: &'a Record, field: &str) -> Result<&'a Yaml> {
    record
        .get(field)
        .ok_or_else(|| invalid(format!("{field} is required")))
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Reference {
    kind: String,
    owner: Option<String>,
    id: String,
}
impl Reference {
    fn record(id: &str) -> Self {
        Self {
            kind: "record".into(),
            owner: None,
            id: id.into(),
        }
    }
    fn parse(value: &Yaml) -> Result<Self> {
        let Yaml::Mapping(fields) = value else {
            return Err(invalid("reference must be a closed mapping"));
        };
        let kind = fields
            .get("kind")
            .ok_or_else(|| invalid("reference kind is required"))?;
        let kind = text(kind, "reference kind")?;
        let expected: &[&str] = match kind {
            "record" => &["kind", "id"],
            "section" | "transition" => &["kind", "owner_id", "id"],
            _ => {
                return Err(invalid(
                    "reference kind must be record, section or transition",
                ));
            }
        };
        if fields.len() != expected.len()
            || fields
                .keys()
                .any(|key| !matches!(key, Yaml::String(key) if expected.contains(&key.as_str())))
        {
            return Err(invalid(
                "reference requires exact record or owner/member fields",
            ));
        }
        let id = text(
            fields
                .get("id")
                .ok_or_else(|| invalid("reference id is required"))?,
            "reference id",
        )?
        .into();
        let owner = if kind == "record" {
            None
        } else {
            Some(
                text(
                    fields
                        .get("owner_id")
                        .ok_or_else(|| invalid("reference owner_id is required"))?,
                    "reference owner_id",
                )?
                .into(),
            )
        };
        Ok(Self {
            kind: kind.into(),
            owner,
            id,
        })
    }
    fn owner_id(&self) -> &str {
        self.owner.as_deref().unwrap_or(&self.id)
    }
    fn wire(&self) -> Json {
        match &self.owner {
            Some(owner) => json!({"kind":self.kind,"ownerId":owner,"id":self.id}),
            None => json!({"kind":"record","id":self.id}),
        }
    }
    fn resolve<'a>(&self, world: &'a World) -> Result<&'a Record> {
        let owner = world
            .records
            .get(self.owner_id())
            .filter(|r| r.kind != "hypothesis" && validation::is_record_kind(&r.kind))
            .ok_or_else(|| invalid("reference requires an exact current canonical-kind owner"))?;
        if self.kind != "record"
            && members(owner, &self.kind)
                .iter()
                .filter(|(id, _, _)| *id == self.id)
                .count()
                != 1
        {
            return Err(invalid(
                "reference member must exist uniquely in its exact owner",
            ));
        }
        Ok(owner)
    }
}

pub(crate) fn members<'a>(owner: &'a Record, kind: &str) -> Vec<(&'a str, &'a Yaml, String)> {
    let mut values = Vec::new();
    let mut add = |source: Option<&'a Yaml>, field: &str, id_key: &str| {
        if let Some(items) = source.and_then(Yaml::as_sequence) {
            for (index, value) in items.iter().enumerate() {
                if let Some(id) = value.get(id_key).and_then(Yaml::as_str) {
                    values.push((id, value, format!("{field}[{index}]")));
                }
            }
        }
    };
    if kind == "section" {
        add(owner.get("descriptions"), "descriptions", "id");
        if owner.kind == "world" {
            for group in ["calendars", "eras", "anchors"] {
                if let Some(items) = owner
                    .get("chronology")
                    .and_then(|v| v.get(group))
                    .and_then(Yaml::as_sequence)
                {
                    for (index, value) in items.iter().enumerate() {
                        if let Some(id) = value.get("id").and_then(Yaml::as_str) {
                            values.push((id, value, format!("chronology.{group}[{index}]")));
                        }
                    }
                }
            }
        }
    } else if kind == "transition" {
        if GENERATIONAL.contains(&owner.kind.as_str()) {
            add(owner.get("transitions"), "transitions", "transition_id");
            if let Some(value) = owner.get("initialization")
                && let Some(id) = value.get("transition_id").and_then(Yaml::as_str)
            {
                values.insert(0, (id, value, "initialization".into()));
            }
        } else if owner.kind == "story-point" {
            add(
                owner
                    .get("lifecycle")
                    .and_then(|value| value.get("transitions")),
                "lifecycle.transitions",
                "id",
            );
        } else if ["knowledge", "relationship"].contains(&owner.kind.as_str()) {
            add(owner.get("transitions"), "transitions", "id");
        }
    }
    values
}

pub fn validate_reference_request(value: &Yaml) -> Result<()> {
    Reference::parse(value).map(|_| ())
}
/// Private author workflows reuse the exact canonical owner/member resolver.
/// Resolving a pointer grants no character knowledge or source admission.
pub(crate) fn resolve_reference_request<'a>(world: &'a World, value: &Yaml) -> Result<&'a Record> {
    Reference::parse(value)?.resolve(world)
}
fn references_value(value: &Yaml) -> Result<Vec<Reference>> {
    let Yaml::Sequence(values) = value else {
        return Err(invalid("required_refs must be an explicit ordered array"));
    };
    if values.len() > MAX_REFS {
        return Err(invalid("required_refs exceeds 1024 members"));
    }
    let mut seen = BTreeSet::new();
    values
        .iter()
        .map(|value| {
            let reference = Reference::parse(value)?;
            if !seen.insert(reference.clone()) {
                return Err(invalid(
                    "required_refs must contain unique exact references",
                ));
            }
            Ok(reference)
        })
        .collect()
}
pub fn validate_refs_request(value: &Yaml) -> Result<()> {
    references_value(value).map(|_| ())
}

fn validate_record(world: &World, record: &Record) -> Result<()> {
    if !enabled(world) {
        return Err(invalid("context-brief requires declared briefs-v1"));
    }
    if !validation::valid_id_for_kind(&record.id, "context-brief") {
        return Err(invalid("context-brief requires a brief_ identity"));
    }
    if record.title.trim().is_empty()
        || record.title.chars().count() > 256
        || record.title.contains(['\n', '\r'])
    {
        return Err(invalid(
            "brief title must be a nonblank single-line label of at most 256 characters",
        ));
    }
    if !lore::domain(text(required(record, "domain")?, "domain")?) {
        return Err(invalid("brief domain must be a dotted identifier"));
    }
    if let Some(status) = record.get("status")
        && !["canonical", "draft", "retired"].contains(&text(status, "status")?)
    {
        return Err(invalid("invalid brief status"));
    }
    lore::ordered_labels(required(record, "audience")?, "audience")?;
    lore::ordered_labels(required(record, "perspectives")?, "perspectives")?;
    lore::validity(required(record, "valid")?, Some(world), "valid")?;
    if record.body.len() > 1024 * 1024 {
        return Err(invalid("brief body exceeds 1 MiB"));
    }
    let scope = text(required(record, "scope")?, "scope")?;
    match scope {
        "world" if record.get("scope_ref").is_none() => {}
        "region" | "era" => {
            let reference = Reference::parse(required(record, "scope_ref")?)?;
            let owner = reference.resolve(world)?;
            if scope == "region"
                && (reference.kind != "record"
                    || !["location", "overlay"].contains(&owner.kind.as_str()))
            {
                return Err(invalid(
                    "region scope requires an exact location or overlay record",
                ));
            }
            if scope == "era"
                && (reference.kind != "section"
                    || owner.id != world.world_record()?.id
                    || !owner
                        .get("chronology")
                        .and_then(|v| v.get("eras"))
                        .and_then(Yaml::as_sequence)
                        .is_some_and(|values| {
                            values.iter().any(|value| {
                                value.get("id").and_then(Yaml::as_str)
                                    == Some(reference.id.as_str())
                            })
                        }))
            {
                return Err(invalid(
                    "era scope requires an exact world chronology era section",
                ));
            }
        }
        "world" => return Err(invalid("world scope forbids scope_ref")),
        _ => return Err(invalid("scope must be world, region or era")),
    }
    for reference in references_value(required(record, "required_refs")?)? {
        reference.resolve(world)?;
    }
    Ok(())
}

/// Domain validation is pure and never calls the whole-world validator recursively.
pub fn validate(world: &World) -> Vec<Diagnostic> {
    world
        .records
        .values()
        .filter(|r| r.kind == "context-brief")
        .filter_map(|record| {
            validate_record(world, record)
                .err()
                .map(|error| Diagnostic {
                    code: "BRIEF-SOURCE".into(),
                    message: error.to_string(),
                    severity: "error".into(),
                    entity_id: Some(record.id.clone()),
                    path: Some(record.source_path.clone()),
                    field: Some("brief".into()),
                })
        })
        .collect()
}
pub fn references(world: &World, record: &Record) -> Result<Vec<(String, String)>> {
    if record.kind != "context-brief" {
        return Ok(Vec::new());
    }
    let mut refs = Vec::new();
    for (index, reference) in references_value(required(record, "required_refs")?)?
        .into_iter()
        .enumerate()
    {
        reference.resolve(world)?;
        refs.push((
            format!("required_refs[{index}]"),
            reference.owner_id().into(),
        ));
    }
    if let Some(value) = record.get("scope_ref") {
        let reference = Reference::parse(value)?;
        reference.resolve(world)?;
        refs.push(("scope_ref".into(), reference.owner_id().into()));
    }
    Ok(refs)
}

fn selected(world: &World, ids: &[String], limit: usize) -> Result<()> {
    narrative::limit_argument(limit)?;
    if ids.len() > 100
        || ids
            .iter()
            .any(|id| !validation::valid_id_for_kind(id, "context-brief"))
    {
        return Err(Error::InvalidInput(
            "brief selections require at most 100 exact brief IDs".into(),
        ));
    }
    if !validation::validate_world(world).valid || !enabled(world) {
        return Err(malformed());
    }
    Ok(())
}
struct Lens<'a> {
    at: Option<&'a StoryTime>,
    audience: &'a str,
    view: &'a str,
    viewer: Option<&'a str>,
    full_author: bool,
    known: Option<BTreeSet<String>>,
}
impl Lens<'_> {
    fn visible(&self, world: &World, owner: &Record) -> Result<bool> {
        if let Some(known) = &self.known {
            Ok(known.contains(&owner.id))
        } else if self.full_author {
            description::full_author_record_visible(world, owner, self.at)
        } else {
            description::record_visible(world, owner, self.at, self.audience, self.view)
        }
    }
}
fn item(
    world: &World,
    owner: &Record,
    target: &Reference,
    label: &str,
    value: Json,
    field: &str,
    character: bool,
) -> Json {
    json!({"target":target.wire(),"label":label,"value":value,
        "citations":if character {vec![]} else {vec![lore::citation(world,owner,field)]}})
}
fn chronological_member(owner: &Record, reference: &Reference) -> Option<(Json, String)> {
    if owner.kind != "world" || reference.kind != "section" {
        return None;
    }
    members(owner, "section")
        .into_iter()
        .find(|(id, _, field)| *id == reference.id && field.starts_with("chronology."))
        .and_then(|(_, value, field)| {
            crate::service::yaml_json(value)
                .ok()
                .map(|value| (value, field))
        })
}
fn domain_horizon(world: &World, owner: &Record) -> Result<StoryTime> {
    let timeline = owner
        .get("initialization")
        .and_then(|v| v.get("applicability"))
        .and_then(|v| v.get("point").or_else(|| v.get("first")))
        .and_then(|v| v.get("timeline"))
        .and_then(Yaml::as_str)
        .unwrap_or(&world.default_timeline())
        .to_owned();
    StoryTime::new(timeline, i64::MAX, i32::MAX)
}
fn domain_projection(
    world: &World,
    owner: &Record,
    reference: &Reference,
    lens: &Lens<'_>,
) -> Result<Option<Json>> {
    if lens.viewer.is_some() {
        return Ok(None);
    }
    let last;
    let at = match lens.at {
        Some(at) => at,
        None => {
            last = domain_horizon(world, owner)?;
            &last
        }
    };
    let history = generational::history(world, &owner.id, at, 1000)?;
    if history["available"] != true {
        return Ok(None);
    }
    if reference.kind == "section" {
        return Ok(Some(history));
    }
    let value = if reference.kind == "transition" {
        let Some(value) = history["history"].as_array().and_then(|values| {
            values
                .iter()
                .find(|value| value["transitionId"] == reference.id)
        }) else {
            return Ok(None);
        };
        value.clone()
    } else {
        let mut value = serde_json::Map::new();
        for key in [
            "recordId",
            "kind",
            "lifecycle",
            "active",
            "value",
            "citations",
            "appliedTransitionIds",
        ] {
            if let Some(member) = history.get(key) {
                value.insert(key.into(), member.clone());
            }
        }
        Json::Object(value)
    };
    let label = value
        .get("value")
        .or_else(|| value.get("payload"))
        .and_then(|value| value.get("title"))
        .and_then(Json::as_str)
        .unwrap_or(&owner.kind)
        .to_owned();
    let field = if reference.kind == "transition" {
        members(owner, "transition")
            .into_iter()
            .find(|(id, _, _)| *id == reference.id)
            .map(|(_, _, field)| field)
            .unwrap_or_else(|| "record".into())
    } else {
        "record".into()
    };
    Ok(Some(item(
        world, owner, reference, &label, value, &field, false,
    )))
}

fn character_knowledge(
    world: &World,
    owner: &Record,
    reference: &Reference,
    viewer: &str,
    at: &StoryTime,
) -> Result<Option<Json>> {
    if owner.status() != "canonical"
        || owner.string("knower") != Some(viewer)
        || !narrative::labels_match(owner, viewer)?
    {
        return Ok(None);
    }
    let claim = required(owner, "claim")?;
    let transitions = owner.list("transitions").ok_or_else(malformed)?;
    let latest = transitions
        .iter()
        .filter_map(|value| {
            StoryTime::from_yaml(value.get("time")?, &world.default_timeline())
                .ok()
                .map(|time| (value, time))
        })
        .filter(|(_, time)| time.not_after(at))
        .max_by(|a, b| a.1.cmp(&b.1));
    let Some((held, _)) =
        latest.filter(|(value, _)| value.get("state").and_then(Yaml::as_str) != Some("forgotten"))
    else {
        return Ok(None);
    };
    let selected = if reference.kind == "transition" {
        transitions
            .iter()
            .find(|value| value.get("id").and_then(Yaml::as_str) == Some(reference.id.as_str()))
    } else {
        Some(held)
    };
    let Some(selected) = selected else {
        return Ok(None);
    };
    let time = StoryTime::from_yaml(
        selected.get("time").ok_or_else(malformed)?,
        &world.default_timeline(),
    )?;
    if !time.not_after(at)
        || !transitions.iter().any(|value| {
            matches!(
                value.get("state").and_then(Yaml::as_str),
                Some("accepted" | "suspected" | "uncertain" | "remembered")
            ) && value
                .get("time")
                .and_then(|value| StoryTime::from_yaml(value, &world.default_timeline()).ok())
                .is_some_and(|learned| learned.not_after(&time))
        })
    {
        return Ok(None);
    }
    let (label, mut value) = if claim.get("assertion_id").is_some() {
        let Some(value) = learned_assertions::admitted(world, viewer, at)?
            .into_iter()
            .find(|value| value["knowledgeId"] == owner.id)
        else {
            return Ok(None);
        };
        (
            value["statement"]
                .as_str()
                .ok_or_else(malformed)?
                .to_owned(),
            value,
        )
    } else if claim.get("genealogy").is_some() {
        let Some(value) = generational::knowledge::admitted(world, viewer, at, false)?
            .into_iter()
            .find(|value| value["knowledgeId"] == owner.id)
        else {
            return Ok(None);
        };
        (
            value["kind"].as_str().ok_or_else(malformed)?.to_owned(),
            value,
        )
    } else {
        let statement = claim
            .get("statement")
            .and_then(Yaml::as_str)
            .ok_or_else(malformed)?;
        (
            statement.into(),
            json!({"knowledgeId":owner.id,"statement":statement}),
        )
    };
    let transition_id = selected
        .get("id")
        .and_then(Yaml::as_str)
        .ok_or_else(malformed)?;
    value["transitionId"] = json!(transition_id);
    value["time"] = json!(time);
    value["state"] = crate::service::yaml_json(selected.get("state").ok_or_else(malformed)?)?;
    if value.get("beliefState").is_some() {
        value["beliefState"] = value["state"].clone();
    }
    value["confidence"] = json!(
        selected
            .get("confidence")
            .and_then(Yaml::as_f64)
            .ok_or_else(malformed)?
    );
    if value.get("uncertain").is_some() {
        value["uncertain"] = json!(
            selected.get("state").and_then(Yaml::as_str) != Some("accepted")
                || selected
                    .get("confidence")
                    .and_then(Yaml::as_f64)
                    .is_none_or(|confidence| confidence < 1.0)
        );
    }
    // A selected historic transition uses its own safe identity. No current
    // transition target/citation may masquerade as evidence of that member.
    value.as_object_mut().unwrap().remove("target");
    value.as_object_mut().unwrap().remove("citations");
    Ok(Some(item(
        world, owner, reference, &label, value, "claim", true,
    )))
}

fn project(world: &World, reference: &Reference, lens: &Lens<'_>) -> Result<Option<Vec<Json>>> {
    let Ok(owner) = reference.resolve(world) else {
        return Ok(None);
    };
    if let Some(viewer) = lens.viewer
        && owner.kind == "assertion"
    {
        if reference.kind != "record" {
            return Ok(None);
        }
        let values = learned_assertions::admitted(world, viewer, lens.at.unwrap())?
            .into_iter()
            .filter(|value| value["assertionId"] == owner.id)
            .map(|value| {
                let target = Reference {
                    kind: "transition".into(),
                    owner: Some(value["knowledgeId"].as_str().unwrap().into()),
                    id: value["transitionId"].as_str().unwrap().into(),
                };
                item(
                    world,
                    owner,
                    &target,
                    value["statement"].as_str().unwrap(),
                    value.clone(),
                    "record",
                    true,
                )
            })
            .collect::<Vec<_>>();
        return Ok((!values.is_empty()).then_some(values));
    }
    if !lens.visible(world, owner)? {
        return Ok(None);
    }
    let character = lens.viewer.is_some();
    if owner.kind == "existence-episode" {
        if character || reference.kind != "record" {
            return Ok(None);
        }
        let value = if lens.full_author {
            crate::existence::full_author_value(world, owner, lens.at)?
        } else {
            crate::existence::admitted_value(world, owner, lens.at, lens.audience, lens.view)?
        };
        return Ok(value.map(|value| {
            vec![item(
                world,
                owner,
                reference,
                &owner.title,
                value,
                "record",
                false,
            )]
        }));
    }
    if GENERATIONAL.contains(&owner.kind.as_str()) {
        if character && reference.kind != "section" {
            return Ok(None);
        }
        if !character {
            let Some(value) = domain_projection(world, owner, reference, lens)? else {
                return Ok(None);
            };
            if reference.kind != "section" {
                return Ok(Some(vec![value]));
            }
        }
    }
    if reference.kind == "section" {
        if !character && let Some((value, field)) = chronological_member(owner, reference) {
            let label = value
                .get("label")
                .and_then(Json::as_str)
                .unwrap_or(&reference.id)
                .to_owned();
            return Ok(Some(vec![item(
                world, owner, reference, &label, value, &field, false,
            )]));
        }
        let values = if let Some(viewer) = lens.viewer {
            description::character_values(world, owner, viewer, lens.at.unwrap())?
        } else if lens.full_author {
            description::full_author_values(world, owner, lens.at)?
        } else {
            match description::section(
                world,
                &owner.id,
                &reference.id,
                lens.at,
                lens.audience,
                lens.view,
            ) {
                Ok(value) => vec![value],
                Err(Error::NotFound(_)) => Vec::new(),
                Err(error) => return Err(error),
            }
        };
        let value = values
            .into_iter()
            .find(|value| value["sectionId"] == reference.id);
        return Ok(value.map(|value| {
            let label = value["heading"]
                .as_str()
                .unwrap_or(&reference.id)
                .to_owned();
            let field = members(owner, "section")
                .into_iter()
                .find(|(id, _, _)| *id == reference.id)
                .map(|(_, _, field)| field)
                .unwrap_or_else(|| "descriptions".into());
            // The section owns its heading and Markdown. Its parent's current
            // summary title is not a historical label for this member.
            let mut member = serde_json::Map::new();
            for key in [
                "ownerId",
                "sectionId",
                "heading",
                "markdown",
                "valid",
                "replaces",
                "citation",
                "target",
            ] {
                if let Some(value) = value.get(key) {
                    member.insert(key.into(), value.clone());
                }
            }
            vec![item(
                world,
                owner,
                reference,
                &label,
                Json::Object(member),
                &field,
                character,
            )]
        }));
    }
    if let Some(viewer) = lens.viewer
        && owner.kind == "knowledge"
    {
        return Ok(
            character_knowledge(world, owner, reference, viewer, lens.at.unwrap())?
                .map(|value| vec![value]),
        );
    }
    if reference.kind == "transition" {
        if character {
            return Ok(None);
        }
        let Some((_, value, field)) = members(owner, "transition")
            .into_iter()
            .find(|(id, _, _)| *id == reference.id)
        else {
            return Ok(None);
        };
        if let Some(at) = lens.at
            && value
                .get("time")
                .and_then(|value| StoryTime::from_yaml(value, &world.default_timeline()).ok())
                .is_none_or(|time| !time.not_after(at))
        {
            return Ok(None);
        }
        return Ok(Some(vec![item(
            world,
            owner,
            reference,
            &owner.title,
            crate::service::yaml_json(value)?,
            &field,
            false,
        )]));
    }
    if owner.kind == "context-brief" {
        if owner.status() != "canonical"
            || !lore::validity(required(owner, "valid")?, Some(world), "valid")?.applies(lens.at)
        {
            return Ok(None);
        }
        let mut value = json!({"scope":owner.string("scope"),"valid":crate::service::yaml_json(required(owner,"valid")?)?,"requiredRefs":references_value(required(owner,"required_refs")?)?.iter().map(Reference::wire).collect::<Vec<_>>(),"bodyMarkdown":owner.body});
        if let Some(scope) = owner.get("scope_ref") {
            value["scopeRef"] = Reference::parse(scope)?.wire();
        }
        return Ok(Some(vec![item(
            world,
            owner,
            reference,
            &owner.title,
            value,
            "record",
            character,
        )]));
    }
    if owner.kind == "assertion" {
        let values = if lens.full_author {
            assertion::full_author_values(world, lens.at)?
        } else {
            assertion::admitted_values(world, lens.at, lens.audience, lens.view)?
        };
        return Ok(values
            .into_iter()
            .find(|value| value["assertionId"] == owner.id)
            .map(|value| {
                vec![item(
                    world,
                    owner,
                    reference,
                    &owner.title,
                    value,
                    "record",
                    false,
                )]
            }));
    }
    if owner.kind == "lore-concept" {
        let value = if let Some(known) = &lens.known {
            lore::character_values(world, known, lens.at.unwrap())?
                .into_iter()
                .find(|value| value["conceptId"] == owner.id)
        } else if lens.full_author {
            lore::full_author_concept_values(world, lens.at)?
                .into_iter()
                .find(|value| value["conceptId"] == owner.id)
        } else {
            lore::scoped_concept_values(world, lens.at, lens.audience, lens.view)?
                .into_iter()
                .find(|value| value["conceptId"] == owner.id)
        };
        return Ok(value.map(|value| {
            vec![item(
                world,
                owner,
                reference,
                if character { &owner.kind } else { &owner.title },
                value,
                "record",
                character,
            )]
        }));
    }
    let value = if character {
        json!({"recordId":owner.id,"kind":owner.kind})
    } else {
        let body_included = lens
            .at
            .map(|at| narrative::author_body_visible(world, owner, at))
            .transpose()?
            .unwrap_or(true);
        let mut value = json!({"recordId":owner.id,"kind":owner.kind,"title":owner.title,
            "bodyMarkdown":if body_included {owner.body.as_str()} else {""}});
        if lens.at.is_some() {
            value["bodyIncluded"] = json!(body_included);
        }
        value
    };
    Ok(Some(vec![item(
        world,
        owner,
        reference,
        if character { &owner.kind } else { &owner.title },
        value,
        "record",
        character,
    )]))
}

fn collect(
    world: &World,
    ids: &[String],
    lens: &Lens<'_>,
    limit: usize,
) -> Result<Option<Vec<Json>>> {
    let mut pending = ids
        .iter()
        .rev()
        .map(|id| Reference::record(id))
        .collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    let mut emitted = BTreeSet::new();
    let mut items = Vec::new();
    let mut overflow = false;
    let mut deferred = None;
    while let Some(reference) = pending.pop() {
        if !seen.insert(reference.clone()) {
            continue;
        }
        if seen.len() > MAX_WORK {
            return Err(Error::InvalidInput(
                "brief closure exceeds 16384 distinct references; refusing incomplete material"
                    .into(),
            ));
        }
        let projected = match project(world, &reference, lens) {
            Ok(Some(values)) => values,
            Ok(None) | Err(Error::NotFound(_)) => return Ok(None),
            Err(error @ Error::InvalidInput(_)) => {
                deferred = Some(error);
                continue;
            }
            Err(error) => return Err(error),
        };
        for value in projected {
            let identity = serde_json::to_string(&value["target"])?;
            if emitted.insert(identity) {
                if emitted.len() <= limit && !overflow {
                    items.push(value);
                } else {
                    overflow = true;
                    items.clear();
                }
            }
        }
        if reference.kind == "record"
            && let Some(owner) = world
                .records
                .get(&reference.id)
                .filter(|record| record.kind == "context-brief")
        {
            if let Some(scope) = owner.get("scope_ref")
                && project(world, &Reference::parse(scope)?, lens)?.is_none()
            {
                return Ok(None);
            }
            pending.extend(
                references_value(required(owner, "required_refs")?)?
                    .into_iter()
                    .rev(),
            );
        }
    }
    if let Some(error) = deferred {
        return Err(error);
    }
    if overflow {
        return Err(Error::InvalidInput(format!(
            "brief result exceeds {limit} items; refusing an incomplete result"
        )));
    }
    Ok(Some(items))
}

pub(crate) fn collect_author(
    world: &World,
    ids: &[String],
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
    limit: usize,
) -> Result<Option<Vec<Json>>> {
    selected(world, ids, limit)?;
    lore::query_arguments(world, at, audience, view)?;
    collect(
        world,
        ids,
        &Lens {
            at,
            audience,
            view,
            viewer: None,
            full_author: false,
            known: None,
        },
        limit,
    )
}
/// One complete closure under the full local-author label union. Projection,
/// replacement suppression and required references all share this scope.
pub(crate) fn collect_full_author(
    world: &World,
    ids: &[String],
    at: Option<&StoryTime>,
    limit: usize,
) -> Result<Option<Vec<Json>>> {
    selected(world, ids, limit)?;
    lore::query_arguments(world, at, "author", "author")?;
    collect(
        world,
        ids,
        &Lens {
            at,
            audience: "author",
            view: "author",
            viewer: None,
            full_author: true,
            known: None,
        },
        limit,
    )
}

pub(crate) fn collect_character(
    world: &World,
    ids: &[String],
    viewer: &str,
    at: &StoryTime,
    scope: &narrative::CharacterScope<'_>,
    limit: usize,
) -> Result<Option<Vec<Json>>> {
    selected(world, ids, limit)?;
    if !scope.matches(world, viewer, at) {
        return Err(malformed());
    }
    collect(
        world,
        ids,
        &Lens {
            at: Some(at),
            audience: "public",
            view: "ordinary",
            viewer: Some(viewer),
            full_author: false,
            known: Some(scope.allowed_ids()?),
        },
        limit,
    )
}
fn result(mut envelope: Json, id: &str, items: Option<Vec<Json>>, limit: usize) -> Json {
    envelope["briefId"] = json!(id);
    envelope["outcome"] = json!(if items.is_some() {
        "available"
    } else {
        "unknown"
    });
    envelope["items"] = json!(items.unwrap_or_default());
    envelope["count"] = json!(envelope["items"].as_array().unwrap().len());
    envelope["limit"] = json!(limit);
    envelope["complete"] = json!(true);
    envelope
}
pub fn compose(
    world: &World,
    id: &str,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
    limit: usize,
) -> Result<Json> {
    let items = collect_author(world, &[id.into()], at, audience, view, limit)?;
    Ok(result(
        json!({"protocol":crate::CONTRACT,"perspective":"author","scope":if at.is_some(){"author-as-of-brief"}else{"author-all-time-brief"},"at":at,"audience":audience,"view":view,"basis":"authored-required-pins"}),
        id,
        items,
        limit,
    ))
}
/// Full local-author composition. Explicit single-label composition remains
/// available through `compose` for callers choosing a narrower scope.
pub fn compose_full_author(
    world: &World,
    id: &str,
    at: Option<&StoryTime>,
    limit: usize,
) -> Result<Json> {
    let items = collect_full_author(world, &[id.into()], at, limit)?;
    Ok(result(
        json!({"protocol":crate::CONTRACT,"perspective":"author",
            "scope":if at.is_some(){"author-as-of-brief"}else{"author-all-time-brief"},
            "at":at,"audiences":["author","public"],"views":["author","ordinary"],
            "basis":"authored-required-pins"}),
        id,
        items,
        limit,
    ))
}

pub fn character(
    world: &World,
    viewer: &str,
    id: &str,
    at: &StoryTime,
    limit: usize,
) -> Result<Json> {
    let scope = narrative::CharacterScope::new(world, viewer, at)?;
    let items = collect_character(world, &[id.into()], viewer, at, &scope, limit)?;
    Ok(result(
        narrative::envelope(viewer, at, "admitted-required-pins"),
        id,
        items,
        limit,
    ))
}
