//! Explicit authored propositions. Classification is data, never an inferred truth winner.
use crate::{
    Error, Result, description, lore,
    lore_types::{TypeSpec, bounded, token},
    model::{Record, StoryTime, World},
    validation::{Diagnostic, is_record_kind, valid_id_for_kind},
};
use serde_json::{Value as Json, json};
use serde_yaml::{Mapping, Value as Yaml};
use std::collections::{BTreeMap, BTreeSet};

const CAPABILITY: &str = "assertions-v1";
const ATTRIBUTORS: [&str; 3] = ["character", "organization", "lore-concept"];

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}
fn text<'a>(value: &'a Yaml, label: &str) -> Result<&'a str> {
    match value {
        Yaml::String(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(invalid(format!(
            "{label} must be a nonblank untagged string"
        ))),
    }
}
fn required<'a>(record: &'a Record, key: &str) -> Result<&'a Yaml> {
    record
        .get(key)
        .ok_or_else(|| invalid(format!("{key} is required")))
}
fn closed<'a>(value: &'a Yaml, keys: &[&str], label: &str) -> Result<&'a Mapping> {
    let Yaml::Mapping(fields) = value else {
        return Err(invalid(format!("{label} must be an untagged mapping")));
    };
    if fields.len() != keys.len()
        || fields
            .keys()
            .any(|key| !matches!(key,Yaml::String(key) if keys.contains(&key.as_str())))
    {
        return Err(invalid(format!("{label} has a closed field contract")));
    }
    Ok(fields)
}
fn enabled(world: &World, capability: &str) -> bool {
    world
        .world_record()
        .ok()
        .and_then(|r| r.list("capabilities"))
        .is_some_and(|values| values.iter().any(|v| v.as_str() == Some(capability)))
}
fn require_enabled(world: &World, assertion_use: bool) -> Result<()> {
    if !enabled(world, "lore-core-v1") || (assertion_use && !enabled(world, CAPABILITY)) {
        return Err(invalid(if assertion_use {
            "assertions require declared lore-core-v1 and assertions-v1"
        } else {
            "predicate declarations require declared lore-core-v1"
        }));
    }
    Ok(())
}

struct Predicate {
    index: usize,
    subject_kinds: Vec<String>,
    value_type: TypeSpec,
    declaration: Yaml,
}
#[derive(Default)]
struct Registry(BTreeMap<String, Predicate>);
impl Registry {
    fn parse(value: &Yaml) -> Result<Self> {
        bounded(value, "predicates")?;
        let Yaml::Sequence(values) = value else {
            return Err(invalid("predicates must be an array"));
        };
        let mut predicates = BTreeMap::new();
        for (index, value) in values.iter().enumerate() {
            let label = format!("predicates[{index}]");
            let fields = closed(value, &["id", "subject_kinds", "value_type"], &label)?;
            let id = text(&fields["id"], &format!("{label}.id"))?;
            if !token(id) || predicates.contains_key(id) {
                return Err(invalid(format!(
                    "{label}.id must be a unique lowercase predicate token"
                )));
            }
            let Yaml::Sequence(kinds) = &fields["subject_kinds"] else {
                return Err(invalid(format!("{label}.subject_kinds must be an array")));
            };
            let mut subject_kinds = Vec::new();
            for kind in kinds {
                let kind = text(kind, &format!("{label}.subject_kinds"))?;
                if kind == "hypothesis"
                    || !is_record_kind(kind)
                    || subject_kinds.iter().any(|v| v == kind)
                {
                    return Err(invalid(format!(
                        "{label}.subject_kinds must contain unique canonical kinds"
                    )));
                }
                subject_kinds.push(kind.to_owned());
            }
            if subject_kinds.is_empty() {
                return Err(invalid(format!("{label}.subject_kinds must not be empty")));
            }
            let value_type =
                TypeSpec::parse(&fields["value_type"], &format!("{label}.value_type"))?;
            predicates.insert(
                id.to_owned(),
                Predicate {
                    index,
                    subject_kinds,
                    value_type,
                    declaration: value.clone(),
                },
            );
        }
        Ok(Self(predicates))
    }
    fn read(world: &World) -> Result<Self> {
        world
            .world_record()?
            .get("predicates")
            .map(Self::parse)
            .transpose()
            .map(Option::unwrap_or_default)
    }
}

/// Closed predicate declarations; existing dependent values validate with the candidate world.
pub fn validate_predicates_request(value: &Yaml) -> Result<()> {
    Registry::parse(value)
        .map(|_| ())
        .map_err(|e| Error::InvalidInput(e.to_string()))
}

fn oid(value: &Yaml) -> bool {
    value.as_str().is_some_and(|s| {
        matches!(s.len(), 40 | 64)
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn evidence(value: &Yaml) -> Result<()> {
    bounded(value, "evidence")?;
    let Yaml::Sequence(items) = value else {
        return Err(invalid("evidence must be an array"));
    };
    let mut seen = BTreeSet::new();
    for (index, item) in items.iter().enumerate() {
        let label = format!("evidence[{index}]");
        let fields = closed(item, &["revision", "path", "blob_oid", "target"], &label)?;
        if !oid(&fields["revision"]) || !oid(&fields["blob_oid"]) {
            return Err(invalid(format!(
                "{label} object IDs must be 40 or 64 lowercase hexadecimal characters"
            )));
        }
        let path = text(&fields["path"], &format!("{label}.path"))?;
        if path.contains(['\\', ':', '\0'])
            || path.starts_with('/')
            || path.split('/').any(|part| matches!(part, "" | "." | ".."))
        {
            return Err(invalid(format!(
                "{label}.path must be a safe repository-relative path"
            )));
        }
        let target = &fields["target"];
        let keys: &[&str] = match target.get("kind").and_then(Yaml::as_str) {
            Some("record") => &["kind", "id"],
            Some("section" | "transition") => &["kind", "owner_id", "id"],
            _ => {
                return Err(invalid(format!(
                    "{label}.target must be a record, section or transition reference"
                )));
            }
        };
        let target = closed(target, keys, &format!("{label}.target"))?;
        for key in keys {
            text(&target[*key], &format!("{label}.target.{key}"))?;
        }
        // Every permitted leaf is a string, so normalized JSON preserves exact identity.
        let identity = crate::service::yaml_json(item)?.to_string();
        if !seen.insert(identity) {
            return Err(invalid("evidence citations must be unique"));
        }
    }
    Ok(())
}
/// Historical citations are syntax checked, never resolved against current source or Git.
pub fn validate_evidence_request(value: &Yaml) -> Result<()> {
    evidence(value).map_err(|e| Error::InvalidInput(e.to_string()))
}

fn reference<'a>(world: &World, value: &'a Yaml, kinds: &[String], label: &str) -> Result<&'a str> {
    let fields = closed(value, &["kind", "id"], label)?;
    if fields["kind"].as_str() != Some("record") {
        return Err(invalid(format!("{label} must be a record reference")));
    }
    let id = text(&fields["id"], label)?;
    let record = world
        .records
        .get(id)
        .ok_or_else(|| invalid(format!("{label} target is absent")))?;
    if !kinds.contains(&record.kind) || !valid_id_for_kind(id, &record.kind) {
        return Err(invalid(format!(
            "{label} must resolve to an exact declared kind"
        )));
    }
    Ok(id)
}
struct Assertion<'a> {
    record: &'a Record,
    subject: &'a str,
    predicate: &'a Predicate,
    value: &'a Yaml,
    valid: lore::Valid<'a>,
    audience: Vec<&'a str>,
    views: Vec<&'a str>,
    attribution: Option<&'a str>,
    replaces: Option<&'a str>,
}
fn read_assertion<'a>(
    world: &World,
    record: &'a Record,
    registry: &'a Registry,
) -> Result<Assertion<'a>> {
    if !valid_id_for_kind(&record.id, "assertion") {
        return Err(invalid("assertion requires an exact assertion ID"));
    }
    if !matches!(record.get("status"), None | Some(Yaml::String(_)))
        || !["canonical", "draft", "retired"].contains(&record.status())
    {
        return Err(invalid(
            "assertion status must be canonical, draft or retired",
        ));
    }
    if record.title.trim().is_empty() || record.title.chars().count() > 256 {
        return Err(invalid(
            "assertion title must be nonblank and at most 256 characters",
        ));
    }
    if !lore::domain(text(required(record, "domain")?, "domain")?) {
        return Err(invalid(
            "assertion domain must be a dotted identifier of at most 256 characters",
        ));
    }
    let predicate = text(required(record, "predicate")?, "predicate")?;
    let predicate = registry
        .0
        .get(predicate)
        .ok_or_else(|| invalid("assertion predicate is undeclared"))?;
    let subject = reference(
        world,
        required(record, "subject")?,
        &predicate.subject_kinds,
        "subject",
    )?;
    let value = required(record, "value")?;
    predicate.value_type.validate_value(value, world)?;
    let classification = text(required(record, "classification")?, "classification")?;
    if !["author-fact", "attributed-claim", "unresolved"].contains(&classification) {
        return Err(invalid(
            "assertion classification must be author-fact, attributed-claim or unresolved",
        ));
    }
    if (classification == "attributed-claim") != record.get("attributed_to").is_some() {
        return Err(invalid(
            "attributed_to is required exactly for attributed-claim",
        ));
    }
    let attribution = record
        .get("attributed_to")
        .map(|v| {
            reference(
                world,
                v,
                &ATTRIBUTORS
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>(),
                "attributed_to",
            )
        })
        .transpose()?;
    let replaces = record
        .get("replaces")
        .map(|v| {
            let id = text(v, "replaces")?;
            if !valid_id_for_kind(id, "assertion")
                || world.records.get(id).is_none_or(|r| r.kind != "assertion")
            {
                return Err(invalid("replaces must name an existing exact assertion ID"));
            }
            Ok(id)
        })
        .transpose()?;
    let valid = lore::validity(required(record, "valid")?, Some(world), "valid")?;
    let audience = lore::ordered_labels(required(record, "audience")?, "audience")?;
    let views = lore::ordered_labels(required(record, "perspectives")?, "perspectives")?;
    evidence(required(record, "evidence")?)?;
    if record
        .get("provenance")
        .is_some_and(|v| !matches!(v, Yaml::String(_)))
    {
        return Err(invalid("provenance must be an untagged string"));
    }
    if record.body.len() > 1024 * 1024 {
        return Err(invalid("assertion body exceeds 1 MiB"));
    }
    Ok(Assertion {
        record,
        subject,
        predicate,
        value,
        valid,
        audience,
        views,
        attribution,
        replaces,
    })
}
fn diagnostic(record: &Record, field: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        code: "WDL-ASSERTION-001".into(),
        message: message.into(),
        severity: "error".into(),
        entity_id: Some(record.id.clone()),
        path: Some(record.source_path.clone()),
        field: Some(field.into()),
    }
}
/// Validate current predicates, all authored assertions and exact replacement compatibility.
pub fn validate(world: &World) -> Vec<Diagnostic> {
    let Ok(owner) = world.world_record() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let registry = match Registry::read(world) {
        Ok(registry) => registry,
        Err(error) => {
            out.push(diagnostic(owner, "predicates", error.to_string()));
            Registry::default()
        }
    };
    let mut assertions = BTreeMap::new();
    for record in world.records.values() {
        if record.get("predicates").is_some() {
            if record.id != owner.id {
                out.push(diagnostic(
                    record,
                    "predicates",
                    "predicates belong only on the world record",
                ));
            }
            if let Err(error) = require_enabled(world, false) {
                out.push(diagnostic(record, "predicates", error.to_string()));
            }
        }
        if record.kind != "assertion" {
            continue;
        }
        if let Err(error) = require_enabled(world, true) {
            out.push(diagnostic(record, "assertion", error.to_string()));
        }
        match read_assertion(world, record, &registry) {
            Ok(assertion) => {
                assertions.insert(record.id.as_str(), assertion);
            }
            Err(error) => out.push(diagnostic(record, "assertion", error.to_string())),
        }
    }
    for assertion in assertions.values() {
        if let Some(previous) = assertion.replaces {
            let target = &world.records[previous];
            if ["subject", "predicate", "valid"]
                .iter()
                .any(|key| assertion.record.get(key) != target.get(key))
            {
                out.push(diagnostic(
                    assertion.record,
                    "replaces",
                    "replacement must retain exact subject, predicate and validity",
                ));
            }
        }
    }
    // Iterative path coloring checks every edge once and cannot recurse through a long chain.
    let mut done = BTreeSet::new();
    for start in assertions.keys() {
        if done.contains(start) {
            continue;
        }
        let mut active = BTreeSet::new();
        let mut next = Some(*start);
        while let Some(id) = next {
            if done.contains(&id) {
                break;
            }
            if !active.insert(id) {
                out.push(diagnostic(
                    assertions[start].record,
                    "replaces",
                    "assertion replacements must be acyclic",
                ));
                break;
            }
            next = assertions.get(id).and_then(|a| a.replaces);
        }
        done.extend(active);
    }
    out
}
/// Live references retain original typed paths. Historical evidence is deliberately opaque.
pub fn references(world: &World, record: &Record) -> Result<Vec<(String, String)>> {
    if record.kind != "assertion" {
        return Ok(Vec::new());
    }
    let registry = Registry::read(world)?;
    let assertion = read_assertion(world, record, &registry)?;
    assertion_references(world, &assertion)
}
fn assertion_references(world: &World, assertion: &Assertion<'_>) -> Result<Vec<(String, String)>> {
    let mut refs = vec![("subject.id".into(), assertion.subject.into())];
    refs.extend(
        assertion
            .predicate
            .value_type
            .references(assertion.value, world)?
            .into_iter()
            .map(|(path, id)| (format!("value{}", path.strip_prefix("values").unwrap()), id)),
    );
    if let Some(id) = assertion.attribution {
        refs.push(("attributed_to.id".into(), id.into()));
    }
    if let Some(id) = assertion.replaces {
        refs.push(("replaces".into(), id.into()));
    }
    Ok(refs)
}
fn read(world: &World, assertion_use: bool) -> Result<Registry> {
    require_enabled(world, assertion_use)?;
    if let Some(error) = validate(world).first() {
        return Err(invalid(error.message.clone()));
    }
    if let Some(error) = lore::validate(world).first() {
        return Err(invalid(error.message.clone()));
    }
    Registry::read(world)
}
fn limit_argument(limit: usize) -> Result<()> {
    if !(1..=1000).contains(&limit) {
        return Err(Error::InvalidInput(
            "assertion limit must be from 1 to 1000".into(),
        ));
    }
    Ok(())
}
fn bounded_result(count: usize, limit: usize) -> Result<()> {
    if count > limit {
        return Err(Error::InvalidInput(format!(
            "assertions have more than {limit} matching items; refusing an incomplete result"
        )));
    }
    Ok(())
}
/// Full current predicate declarations. Enum alternatives retain their scalar identity.
pub fn predicates(world: &World, limit: usize) -> Result<Json> {
    limit_argument(limit)?;
    let registry = read(world, false)?;
    bounded_result(registry.0.len(), limit)?;
    let owner = world.world_record()?;
    let items = registry
        .0
        .values()
        .map(|predicate| {
            let mut value = crate::service::yaml_json(&predicate.declaration)?;
            lore::annotate_schema(&predicate.value_type, &mut value["value_type"])?;
            value["citation"] =
                lore::citation(world, owner, &format!("predicates[{}]", predicate.index));
            Ok(value)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(
        json!({"protocol":crate::CONTRACT,"perspective":"author","scope":"author-all-time",
        "basis":"authored-predicates","count":items.len(),"items":items}),
    )
}

fn admitted(assertion: &Assertion<'_>, at: Option<&StoryTime>, audience: &str, view: &str) -> bool {
    assertion.record.status() == "canonical"
        && assertion.audience.contains(&audience)
        && assertion.views.contains(&view)
        && assertion.valid.applies(at)
}
fn projected_items(
    world: &World,
    target: Option<&str>,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
    limit: Option<usize>,
    full_author: bool,
) -> Result<Vec<Json>> {
    let registry = read(world, true)?;
    let assertions = world
        .records
        .values()
        .filter(|r| r.kind == "assertion")
        .map(|r| read_assertion(world, r, &registry))
        .collect::<Result<Vec<_>>>()?;
    let eligible = assertions
        .iter()
        .filter(|a| {
            if full_author {
                a.record.status() == "canonical"
                    && a.audience
                        .iter()
                        .any(|label| matches!(*label, "author" | "public"))
                    && a.views
                        .iter()
                        .any(|label| matches!(*label, "author" | "ordinary"))
                    && a.valid.applies(at)
            } else {
                admitted(a, at, audience, view)
            }
        })
        .collect::<Vec<_>>();
    let mut allowed = BTreeSet::new();
    for record in world.records.values().filter(|r| r.kind != "assertion") {
        if if full_author {
            description::full_author_record_visible(world, record, at)?
        } else {
            description::record_visible(world, record, at, audience, view)?
        } {
            allowed.insert(record.id.clone());
        }
    }
    allowed.extend(eligible.iter().map(|a| a.record.id.clone()));
    let mut projected = Vec::new();
    for assertion in eligible {
        if !allowed.contains(assertion.subject)
            || assertion
                .attribution
                .is_some_and(|id| !allowed.contains(id))
        {
            continue;
        }
        let Some((value, shape)) =
            lore::projected(&assertion.predicate.value_type, assertion.value, &allowed)?
        else {
            continue;
        };
        projected.push((assertion, value, shape));
    }
    let ids = projected
        .iter()
        .map(|(a, _, _)| a.record.id.as_str())
        .collect::<BTreeSet<_>>();
    // Only admitted payloads can suppress. Applicability and classification never choose between forks.
    let suppressed = if at.is_some() {
        projected
            .iter()
            .filter_map(|(a, _, _)| a.replaces)
            .collect::<BTreeSet<_>>()
    } else {
        BTreeSet::new()
    };
    let selected = projected
        .iter()
        .filter(|(a, _, _)| {
            !suppressed.contains(a.record.id.as_str())
                && target.is_none_or(|target| a.record.id == target || a.subject == target)
        })
        .collect::<Vec<_>>();
    if let Some(limit) = limit {
        bounded_result(selected.len(), limit)?;
    }
    let items = selected.into_iter().map(|(assertion, value, shape)| {
        let record = assertion.record;
        let mut item = json!({"assertionId":record.id,"title":record.title,"domain":record.get("domain").unwrap().as_str(),
            "subject":crate::service::yaml_json(record.get("subject").unwrap())?,"predicate":record.string("predicate"),
            "value":value,"valueType":shape,"classification":record.string("classification"),
            "valid":crate::service::yaml_json(record.get("valid").unwrap())?,"evidence":[],
            "citation":lore::citation(world, record, "record")});
        if let Some(attribution) = record.get("attributed_to") {
            item["attributedTo"] = crate::service::yaml_json(attribution)?;
        }
        if let Some(previous) = assertion.replaces.filter(|id| ids.contains(id)) {
            item["replaces"] = json!(previous);
        }
        if let Some(provenance) = record.get("provenance") { item["provenance"] = crate::service::yaml_json(provenance)?; }
        let mut refs = Vec::new();
        for (field, id) in assertion_references(world, assertion)? {
            if !allowed.contains(&id) || (field == "replaces" && !ids.contains(id.as_str())) { continue; }
            let endpoint = &world.records[&id];
            refs.push(json!({"sourceField":field,"entityId":id,"kind":endpoint.kind,"title":endpoint.title,
                "citation":lore::citation(world, record, &field),"targetCitation":lore::citation(world, endpoint, "record")}));
        }
        item["references"] = json!(refs);
        Ok(item)
    }).collect::<Result<Vec<_>>>()?;
    Ok(items)
}

/// Complete admitted proposition payloads for a consumer that applies its own
/// relevant-result bound after joining. No capability or source means no stream.
pub(crate) fn admitted_values(
    world: &World,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
) -> Result<Vec<Json>> {
    lore::query_arguments(world, at, audience, view)?;
    if !enabled(world, CAPABILITY)
        && !world
            .records
            .values()
            .any(|record| record.kind == "assertion")
    {
        return Ok(Vec::new());
    }
    projected_items(world, None, at, audience, view, None, false)
}

pub(crate) fn full_author_values(world: &World, at: Option<&StoryTime>) -> Result<Vec<Json>> {
    lore::query_arguments(world, at, "author", "author")?;
    if !enabled(world, CAPABILITY)
        && !world
            .records
            .values()
            .any(|record| record.kind == "assertion")
    {
        return Ok(Vec::new());
    }
    projected_items(world, None, at, "author", "author", None, true)
}

/// Author propositions for an exact assertion or subject ID. No horizon retains correction history.
pub fn as_of(
    world: &World,
    target: &str,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
    limit: usize,
) -> Result<Json> {
    limit_argument(limit)?;
    lore::query_arguments(world, at, audience, view)?;
    if target.trim().is_empty() {
        return Err(Error::InvalidInput(
            "assertion target must be an exact nonblank record ID".into(),
        ));
    }
    let items = projected_items(world, Some(target), at, audience, view, Some(limit), false)?;
    Ok(json!({"protocol":crate::CONTRACT,"perspective":"author",
        "scope":if at.is_some(){"author-as-of-assertions"}else{"author-all-time-assertions"},
        "at":at,"audience":audience,"view":view,"basis":"authored-assertions",
        "targetId":target,"count":items.len(),"items":items}))
}
