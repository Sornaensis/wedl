//! Typed authored lore and explicit links. Presentation filters are author reads,
//! and never supply character knowledge, traits, membership or authority.
use crate::{
    Error, Result,
    lore_types::{Profile, Registry, TypeSpec, bounded, token},
    model::{Record, StoryTime, World},
    validation::{Diagnostic, valid_id_for_kind},
};
use serde_json::{Value as Json, json};
use serde_yaml::{Mapping, Value as Yaml};
use std::collections::BTreeSet;

const CAPABILITY: &str = "lore-core-v1";
const MAX_LIMIT: usize = 1000;

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}
fn mapping<'a>(value: &'a Yaml, label: &str) -> Result<&'a Mapping> {
    match value {
        Yaml::Mapping(value) => Ok(value),
        _ => Err(invalid(format!("{label} must be an untagged mapping"))),
    }
}
fn closed<'a>(
    value: &'a Yaml,
    required: &[&str],
    optional: &[&str],
    label: &str,
) -> Result<&'a Mapping> {
    let fields = mapping(value, label)?;
    if required.iter().any(|key| !fields.contains_key(*key)) || fields.keys().any(|key| !matches!(key,Yaml::String(key) if required.contains(&key.as_str()) || optional.contains(&key.as_str()))) {
        return Err(invalid(format!("{label} has a closed field contract")));
    }
    Ok(fields)
}
fn text<'a>(value: &'a Yaml, label: &str) -> Result<&'a str> {
    match value {
        Yaml::String(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(invalid(format!(
            "{label} must be a nonblank untagged string"
        ))),
    }
}
fn required<'a>(record: &'a Record, field: &str) -> Result<&'a Yaml> {
    record
        .get(field)
        .ok_or_else(|| invalid(format!("{field} is required")))
}
fn enabled(world: &World) -> bool {
    world
        .world_record()
        .ok()
        .and_then(|r| r.list("capabilities"))
        .is_some_and(|v| {
            v.iter()
                .any(|v| matches!(v,Yaml::String(v) if v==CAPABILITY))
        })
}
fn require_enabled(world: &World) -> Result<()> {
    if enabled(world) {
        Ok(())
    } else {
        Err(invalid("structured lore requires declared lore-core-v1"))
    }
}
fn registry(world: &World) -> Result<Registry> {
    match world.world_record()?.get("profiles") {
        Some(value) => Registry::parse(value),
        None => Ok(Registry::default()),
    }
}
pub(crate) fn ordered_labels<'a>(value: &'a Yaml, label: &str) -> Result<Vec<&'a str>> {
    bounded(value, label)?;
    let Yaml::Sequence(values) = value else {
        return Err(invalid(format!(
            "{label} must be a sorted nonempty string array"
        )));
    };
    if values.is_empty() {
        return Err(invalid(format!("{label} must not be empty")));
    }
    let mut result = Vec::new();
    for value in values {
        let value = text(value, label)?;
        if value.chars().count() > 256
            || value.contains(['\n', '\r'])
            || result.last().is_some_and(|prior| *prior >= value)
        {
            return Err(invalid(format!(
                "{label} requires sorted unique single-line labels of at most 256 characters"
            )));
        }
        result.push(value);
    }
    Ok(result)
}
pub(crate) fn domain(value: &str) -> bool {
    value.contains('.')
        && value.chars().count() <= 256
        && value.split('.').all(|part| {
            !part.is_empty()
                && part.chars().any(char::is_alphanumeric)
                && part.chars().all(|c| c.is_alphanumeric() || c == '-')
        })
}

#[derive(Clone, Copy)]
pub(crate) enum Valid<'a> {
    Timeless,
    Interval {
        timeline: &'a str,
        start: Option<(i64, i32)>,
        end: Option<(i64, i32)>,
    },
}
impl Valid<'_> {
    pub(crate) fn applies(&self, at: Option<&StoryTime>) -> bool {
        match (self, at) {
            (Self::Timeless, _) | (_, None) => true,
            (
                Self::Interval {
                    timeline,
                    start,
                    end,
                },
                Some(at),
            ) => {
                *timeline == at.timeline
                    && start.is_none_or(|start| start <= (at.tick, at.order))
                    && end.is_none_or(|end| (at.tick, at.order) <= end)
            }
        }
    }
}
fn point(value: &Yaml, timeline: &str, label: &str) -> Result<(i64, i32)> {
    let fields = closed(value, &["timeline", "tick", "order"], &[], label)?;
    if text(&fields["timeline"], label)? != timeline {
        return Err(invalid(format!("{label} must use its interval timeline")));
    }
    let tick = match &fields["tick"] {
        Yaml::Number(value) => value.as_i64(),
        _ => None,
    }
    .ok_or_else(|| invalid(format!("{label}.tick must be an exact signed i64")))?;
    let order = match &fields["order"] {
        Yaml::Number(value) => value.as_i64().and_then(|v| i32::try_from(v).ok()),
        _ => None,
    }
    .ok_or_else(|| invalid(format!("{label}.order must be an exact signed i32")))?;
    Ok((tick, order))
}
pub(crate) fn validity<'a>(
    value: &'a Yaml,
    world: Option<&World>,
    label: &str,
) -> Result<Valid<'a>> {
    bounded(value, label)?;
    let fields = mapping(value, label)?;
    match fields.get("kind").and_then(Yaml::as_str) {
        Some("timeless") => {
            closed(value, &["kind"], &[], label)?;
            Ok(Valid::Timeless)
        }
        Some("interval") => {
            let fields = closed(value, &["kind", "timeline"], &["start", "end"], label)?;
            let timeline = text(&fields["timeline"], label)?;
            if let Some(world) = world {
                world
                    .story_time(0, Some(timeline), 0)
                    .map_err(|e| invalid(e.to_string()))?;
            }
            let start = fields
                .get("start")
                .map(|v| point(v, timeline, &format!("{label}.start")))
                .transpose()?;
            let end = fields
                .get("end")
                .map(|v| point(v, timeline, &format!("{label}.end")))
                .transpose()?;
            if start.zip(end).is_some_and(|(start, end)| start > end) {
                return Err(invalid(format!("{label} start must not follow end")));
            }
            Ok(Valid::Interval {
                timeline,
                start,
                end,
            })
        }
        _ => Err(invalid(format!(
            "{label}.kind must be timeless or interval"
        ))),
    }
}
/// Closed source validity grammar; timeline existence remains candidate validation.
pub fn validate_valid_request(value: &Yaml) -> Result<()> {
    validity(value, None, "valid")
        .map(|_| ())
        .map_err(|e| Error::InvalidInput(e.to_string()))
}
struct Link<'a> {
    index: usize,
    relation: &'a str,
    target: &'a str,
    valid: Valid<'a>,
    source: &'a Yaml,
}
fn link_members<'a>(value: &'a Yaml, world: Option<&World>) -> Result<Vec<Link<'a>>> {
    bounded(value, "concept_links")?;
    let Yaml::Sequence(values) = value else {
        return Err(invalid("concept_links must be an ordered array"));
    };
    let mut result = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let label = format!("concept_links[{index}]");
        let fields = closed(value, &["relation", "concept_id", "valid"], &[], &label)?;
        let relation = text(&fields["relation"], &label)?;
        if !token(relation) {
            return Err(invalid(format!(
                "{label}.relation must be a lowercase relation token"
            )));
        }
        let target = text(&fields["concept_id"], &label)?;
        if !valid_id_for_kind(target, "lore-concept") {
            return Err(invalid(format!(
                "{label}.concept_id must be an exact concept ID"
            )));
        }
        if let Some(world) = world {
            if world
                .records
                .get(target)
                .is_none_or(|record| record.kind != "lore-concept")
            {
                return Err(invalid(format!(
                    "{label}.concept_id must name an existing lore-concept"
                )));
            }
        }
        let valid = validity(&fields["valid"], world, &format!("{label}.valid"))?;
        result.push(Link {
            index,
            relation,
            target,
            valid,
            source: &fields["valid"],
        });
    }
    Ok(result)
}
/// Closed link request grammar; endpoint existence remains candidate validation.
pub fn validate_links_request(value: &Yaml) -> Result<()> {
    link_members(value, None)
        .map(|_| ())
        .map_err(|e| Error::InvalidInput(e.to_string()))
}
struct Concept<'a> {
    record: &'a Record,
    domain: &'a str,
    profile: &'a str,
    values: &'a Yaml,
    valid: Valid<'a>,
    valid_source: &'a Yaml,
    audience: Vec<&'a str>,
    views: Vec<&'a str>,
    links: Vec<Link<'a>>,
}
fn read_concept<'a>(world: &World, record: &'a Record, profiles: &Registry) -> Result<Concept<'a>> {
    if !valid_id_for_kind(&record.id, "lore-concept") {
        return Err(invalid("lore-concept requires an exact concept ID"));
    }
    if !matches!(record.get("status"), None | Some(Yaml::String(_)))
        || !["canonical", "draft", "retired"].contains(&record.status())
    {
        return Err(invalid(
            "lore-concept status must be canonical, draft or retired",
        ));
    }
    if record.title.trim().is_empty() || record.title.chars().count() > 256 {
        return Err(invalid(
            "lore-concept title must be nonblank and at most 256 characters",
        ));
    }
    let authored_domain = text(required(record, "domain")?, "domain")?;
    if !domain(authored_domain) {
        return Err(invalid(
            "domain must be a nonblank dotted identifier of at most 256 characters",
        ));
    }
    let profile = text(required(record, "profile")?, "profile")?;
    if !token(profile) {
        return Err(invalid("profile must be an exact profile ID"));
    }
    let definition = profiles
        .get(profile)
        .ok_or_else(|| invalid("concept profile is not declared in this world"))?;
    let values = required(record, "values")?;
    definition.validate_value(values, world)?;
    let valid_source = required(record, "valid")?;
    let valid = validity(valid_source, Some(world), "valid")?;
    let audience = ordered_labels(required(record, "audience")?, "audience")?;
    let views = ordered_labels(required(record, "perspectives")?, "perspectives")?;
    if let Some(provenance) = record.get("provenance") {
        if !matches!(provenance, Yaml::String(_)) {
            return Err(invalid("provenance must be an untagged string"));
        }
    }
    if record.body.len() > 1024 * 1024 {
        return Err(invalid("lore-concept body exceeds 1 MiB"));
    }
    let links = record
        .get("concept_links")
        .map(|v| link_members(v, Some(world)))
        .transpose()?
        .unwrap_or_default();
    Ok(Concept {
        record,
        domain: authored_domain,
        profile,
        values,
        valid,
        valid_source,
        audience,
        views,
        links,
    })
}
fn diagnostic(record: &Record, field: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        code: "WDL-LORE-001".into(),
        message: message.into(),
        severity: "error".into(),
        entity_id: Some(record.id.clone()),
        path: Some(record.source_path.clone()),
        field: Some(field.into()),
    }
}
/// Validate explicit profiles, typed concepts and concept links for every status.
pub fn validate(world: &World) -> Vec<Diagnostic> {
    let Ok(owner) = world.world_record() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let profiles = match registry(world) {
        Ok(profiles) => profiles,
        Err(error) => {
            out.push(diagnostic(owner, "profiles", error.to_string()));
            Registry::default()
        }
    };
    for record in world.records.values() {
        let has_lore = record.kind == "lore-concept"
            || record.get("profiles").is_some()
            || record.get("concept_links").is_some();
        if has_lore && !enabled(world) {
            out.push(diagnostic(
                record,
                "lore",
                "structured lore requires declared lore-core-v1",
            ));
        }
        if record.get("profiles").is_some() && record.id != owner.id {
            out.push(diagnostic(
                record,
                "profiles",
                "profiles belong only on the world record",
            ));
        }
        if record.kind == "lore-concept" {
            if let Err(error) = read_concept(world, record, &profiles) {
                out.push(diagnostic(record, "lore", error.to_string()));
            }
        } else if let Some(links) = record.get("concept_links") {
            if record.kind == "hypothesis" {
                out.push(diagnostic(
                    record,
                    "concept_links",
                    "hypotheses cannot own canonical concept links",
                ));
            } else if let Err(error) = link_members(links, Some(world)) {
                out.push(diagnostic(record, "concept_links", error.to_string()));
            }
        }
    }
    out
}
/// Reference paths retain source array positions; arbitrary strings never become edges.
pub fn references(world: &World, record: &Record) -> Result<Vec<(String, String)>> {
    let mut refs = Vec::new();
    if record.kind == "lore-concept" {
        let profiles = registry(world)?;
        let concept = read_concept(world, record, &profiles)?;
        refs.extend(
            profiles
                .get(concept.profile)
                .unwrap()
                .references(concept.values, world)?,
        );
    }
    if let Some(value) = record.get("concept_links") {
        for link in link_members(value, Some(world))? {
            refs.push((
                format!("concept_links[{}].concept_id", link.index),
                link.target.into(),
            ));
        }
    }
    Ok(refs)
}
fn read(world: &World) -> Result<Registry> {
    require_enabled(world)?;
    if let Some(error) = validate(world).first() {
        return Err(invalid(error.message.clone()));
    }
    registry(world)
}
fn limit_argument(limit: usize) -> Result<()> {
    if !(1..=MAX_LIMIT).contains(&limit) {
        Err(Error::InvalidInput(
            "lore limit must be from 1 to 1000".into(),
        ))
    } else {
        Ok(())
    }
}
fn bounded_result(count: usize, limit: usize) -> Result<()> {
    if count > limit {
        Err(Error::InvalidInput(format!(
            "lore has more than {limit} matching items; refusing an incomplete result"
        )))
    } else {
        Ok(())
    }
}
fn scope(at: Option<&StoryTime>) -> &'static str {
    if at.is_some() {
        "author-as-of-lore"
    } else {
        "author-all-time-lore"
    }
}
fn envelope(at: Option<&StoryTime>, basis: &str) -> Json {
    json!({"protocol":crate::CONTRACT,"perspective":"author","scope":scope(at),"at":at,"basis":basis})
}
pub(crate) fn citation(world: &World, record: &Record, field: &str) -> Json {
    json!({"entityId":record.id,"sourcePath":record.source_path,"blobOid":record.blob_oid,"revision":world.revision,"fingerprint":world.fingerprint,"section":field})
}
fn scalar_type(value: &Yaml) -> Result<&'static str> {
    match value {
        Yaml::Null => Ok("null"),
        Yaml::Bool(_) => Ok("boolean"),
        Yaml::Number(_) => Ok("number"),
        Yaml::String(_) => Ok("string"),
        _ => Err(invalid("enum values must be untagged JSON scalars")),
    }
}
// Integer JSON values use the shared exact decimal-string codec. Annotate only
// enum alternatives, where the declared type alone cannot disambiguate them.
pub(crate) fn annotate_schema(spec: &TypeSpec, value: &mut Json) -> Result<()> {
    match spec {
        TypeSpec::Enum { values } => {
            value["enumValueTypes"] =
                json!(values.iter().map(scalar_type).collect::<Result<Vec<_>>>()?);
        }
        TypeSpec::Array { items, .. } => annotate_schema(items, &mut value["items"])?,
        TypeSpec::Object { required, optional } => {
            for (key, child) in required {
                annotate_schema(child, &mut value["required"][key])?;
            }
            for (key, child) in optional {
                annotate_schema(child, &mut value["optional"][key])?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn profile_value(world: &World, profile: &Profile) -> Result<Json> {
    let owner = world.world_record()?;
    let index = owner
        .list("profiles")
        .and_then(|v| {
            v.iter()
                .position(|v| v.get("id").and_then(Yaml::as_str) == Some(profile.id.as_str()))
        })
        .ok_or_else(|| invalid("profile declaration is unavailable"))?;
    let mut value = crate::service::yaml_json(&profile.declaration)?;
    annotate_schema(&profile.fields, &mut value["fields"])?;
    value["citation"] = citation(world, owner, &format!("profiles[{index}]"));
    Ok(value)
}
/// Explicit author profile declarations, including their complete schemas.
pub fn profiles(world: &World, limit: usize) -> Result<Json> {
    limit_argument(limit)?;
    let registry = read(world)?;
    bounded_result(registry.profiles.len(), limit)?;
    let items: Vec<_> = registry
        .profiles
        .values()
        .map(|profile| profile_value(world, profile))
        .collect::<Result<_>>()?;
    let mut result = envelope(None, "authored-lore-profiles");
    result["count"] = json!(items.len());
    result["items"] = json!(items);
    result["limit"] = json!(limit);
    Ok(result)
}
pub fn profile(world: &World, id: &str) -> Result<Json> {
    if !token(id) {
        return Err(Error::InvalidInput(
            "profile requires an exact lowercase profile ID".into(),
        ));
    }
    let registry = read(world)?;
    let selected = registry
        .get(id)
        .ok_or_else(|| Error::NotFound("lore profile is unavailable".into()))?;
    let mut result = envelope(None, "authored-lore-profile");
    result
        .as_object_mut()
        .unwrap()
        .extend(profile_value(world, selected)?.as_object().unwrap().clone());
    Ok(result)
}
pub(crate) fn query_arguments(
    world: &World,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
) -> Result<()> {
    for value in [audience, view] {
        if value.trim().is_empty() || value.chars().count() > 256 || value.contains(['\n', '\r']) {
            return Err(Error::InvalidInput(
                "audience and view must be nonblank single-line labels of at most 256 characters"
                    .into(),
            ));
        }
    }
    if let Some(at) = at {
        world.story_time(at.tick, Some(&at.timeline), at.order)?;
    }
    Ok(())
}
fn visible(concept: &Concept<'_>, at: Option<&StoryTime>, audience: &str, view: &str) -> bool {
    concept.record.status() == "canonical"
        && concept.audience.contains(&audience)
        && concept.views.contains(&view)
        && concept.valid.applies(at)
}
fn concepts<'a>(world: &'a World, registry: &Registry) -> Result<Vec<Concept<'a>>> {
    world
        .records
        .values()
        .filter(|r| r.kind == "lore-concept")
        .map(|r| read_concept(world, r, registry))
        .collect()
}
fn allowed_ids(
    world: &World,
    concepts: &[Concept<'_>],
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
) -> Result<BTreeSet<String>> {
    let mut result = BTreeSet::new();
    for record in world
        .records
        .values()
        .filter(|r| r.kind != "lore-concept" && r.kind != "hypothesis")
    {
        // Episodes and briefs carry their own explicit narrative applicability.
        // Other ordinary endpoints retain the existing author source boundary.
        let visible = match record.kind.as_str() {
            "existence-episode" => {
                crate::existence::record_visible(world, record, at, audience, view)?
            }
            "context-brief" => {
                crate::description::record_visible(world, record, at, audience, view)?
            }
            _ => true,
        };
        if visible {
            result.insert(record.id.clone());
        }
    }
    result.extend(
        concepts
            .iter()
            .filter(|c| visible(c, at, audience, view))
            .map(|c| c.record.id.clone()),
    );
    Ok(result)
}
/// Shared author admission for explicitly typed source consumers.
pub(crate) fn allowed_record_ids(
    world: &World,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
) -> Result<BTreeSet<String>> {
    query_arguments(world, at, audience, view)?;
    let registry = read(world)?;
    let concepts = concepts(world, &registry)?;
    allowed_ids(world, &concepts, at, audience, view)
}
/// Project an already validated profile value using the same concept visibility.
pub(crate) fn project_value(
    profile: &Profile,
    value: &Yaml,
    allowed: &BTreeSet<String>,
) -> Result<(Json, Json)> {
    projected(&profile.fields, value, allowed)?
        .ok_or_else(|| invalid("a profile root must be an object"))
}
pub(crate) fn projected(
    spec: &TypeSpec,
    value: &Yaml,
    allowed: &BTreeSet<String>,
) -> Result<Option<(Json, Json)>> {
    let name = match spec {
        TypeSpec::String => "string",
        TypeSpec::Boolean => "boolean",
        TypeSpec::Number => "number",
        TypeSpec::Null => "null",
        TypeSpec::Entity { .. } => "entity",
        TypeSpec::Enum { .. } => "enum",
        TypeSpec::Array { .. } => "array",
        TypeSpec::Object { .. } => "object",
    };
    match spec {
        TypeSpec::Entity { .. } => {
            let id = value
                .get("id")
                .and_then(Yaml::as_str)
                .ok_or_else(|| invalid("typed entity is malformed"))?;
            if !allowed.contains(id) {
                return Ok(None);
            }
            Ok(Some((
                crate::service::yaml_json(value)?,
                json!({"type":name}),
            )))
        }
        TypeSpec::Object { required, optional } => {
            let mut result = serde_json::Map::new();
            let mut fields = serde_json::Map::new();
            for (key, value) in mapping(value, "typed object")? {
                let key = text(key, "typed field")?;
                let child = required
                    .get(key)
                    .or_else(|| optional.get(key))
                    .ok_or_else(|| invalid("undeclared typed field"))?;
                if let Some((value, shape)) = projected(child, value, allowed)? {
                    result.insert(key.into(), value);
                    fields.insert(key.into(), shape);
                }
            }
            Ok(Some((
                Json::Object(result),
                json!({"type":name,"fields":fields}),
            )))
        }
        TypeSpec::Array { items, .. } => {
            let Yaml::Sequence(values) = value else {
                return Err(invalid("typed array is malformed"));
            };
            let mut result = Vec::new();
            let mut shapes = Vec::new();
            for value in values {
                if let Some((value, shape)) = projected(items, value, allowed)? {
                    result.push(value);
                    shapes.push(shape);
                }
            }
            Ok(Some((json!(result), json!({"type":name,"items":shapes}))))
        }
        TypeSpec::Enum { .. } => Ok(Some((
            crate::service::yaml_json(value)?,
            json!({"type":name,"scalarType":scalar_type(value)?}),
        ))),
        _ => Ok(Some((
            crate::service::yaml_json(value)?,
            json!({"type":name}),
        ))),
    }
}
fn link_value(world: &World, owner: &Record, link: &Link<'_>) -> Result<Json> {
    let target = &world.records[link.target];
    Ok(
        json!({"ownerId":owner.id,"conceptId":target.id,"title":target.title,"relation":link.relation,
        "valid":crate::service::yaml_json(link.source)?,"citation":citation(world,owner,&format!("concept_links[{}]",link.index)),"targetCitation":citation(world,target,"record")}),
    )
}
fn concept_value(
    world: &World,
    concept: &Concept<'_>,
    registry: &Registry,
    allowed: &BTreeSet<String>,
    at: Option<&StoryTime>,
) -> Result<Json> {
    let profile = registry.get(concept.profile).unwrap();
    let (values, types) = projected(&profile.fields, concept.values, allowed)?
        .ok_or_else(|| invalid("a profile root must be an object"))?;
    let mut references = Vec::new();
    for (field, id) in profile.references(concept.values, world)? {
        if allowed.contains(&id) {
            let target = &world.records[&id];
            references.push(json!({"sourceField":field,"entityId":id,"kind":target.kind,"title":target.title,
                "citation":citation(world,concept.record,&field),"targetCitation":citation(world,target,"record")}));
        }
    }
    let links: Vec<_> = concept
        .links
        .iter()
        .filter(|l| allowed.contains(l.target) && l.valid.applies(at))
        .map(|l| link_value(world, concept.record, l))
        .collect::<Result<_>>()?;
    let mut result = json!({"conceptId":concept.record.id,"title":concept.record.title,"domain":concept.domain,"profile":concept.profile,
        "valid":crate::service::yaml_json(concept.valid_source)?,"values":values,"valueTypes":types,"references":references,"conceptLinks":links,
        "body":concept.record.body,"citation":citation(world,concept.record,"record")});
    if let Some(value) = concept.record.get("provenance") {
        result["provenance"] = crate::service::yaml_json(value)?;
    }
    Ok(result)
}

/// Explicit character profile projection without current author titles, bodies or citations.
pub(crate) fn character_values(
    world: &World,
    allowed: &BTreeSet<String>,
    at: &StoryTime,
) -> Result<Vec<Json>> {
    let registry = read(world)?;
    let concepts = concepts(world, &registry)?;
    concepts.iter().filter(|concept| allowed.contains(&concept.record.id)).map(|concept| {
        let profile = registry.get(concept.profile).unwrap();
        let (values, types) = projected(&profile.fields, concept.values, allowed)?
            .ok_or_else(|| invalid("a profile root must be an object"))?;
        let references = profile.references(concept.values, world)?.into_iter()
            .filter(|(_, id)| allowed.contains(id))
            .map(|(_,id)| json!({"entityId":id,
                "kind":world.records[&id].kind,"target":{"kind":"record","id":id}}))
            .collect::<Vec<_>>();
        Ok(json!({"conceptId":concept.record.id,"domain":concept.domain,"profile":concept.profile,
            "valid":crate::service::yaml_json(concept.valid_source)?,"values":values,"valueTypes":types,
            "references":references,"conceptLinks":character_links(world,concept.record,allowed,at)?,
            "target":{"kind":"record","id":concept.record.id}}))
    }).collect()
}

pub(crate) fn character_links(
    world: &World,
    owner: &Record,
    allowed: &BTreeSet<String>,
    at: &StoryTime,
) -> Result<Vec<Json>> {
    let authored = owner
        .get("concept_links")
        .map(|value| link_members(value, Some(world)))
        .transpose()?
        .unwrap_or_default();
    authored
        .iter()
        .filter(|link| allowed.contains(link.target) && link.valid.applies(Some(at)))
        .map(|link| {
            Ok(
                json!({"ownerId":owner.id,"conceptId":link.target,"relation":link.relation,
            "valid":crate::service::yaml_json(link.source)?,
            "target":{"kind":"record","id":link.target}}),
            )
        })
        .collect()
}
fn selected<'a, 'b>(
    world: &World,
    concepts: &'b [Concept<'a>],
    reference: &str,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
) -> Result<&'b Concept<'a>> {
    let visible: Vec<_> = concepts
        .iter()
        .filter(|c| visible(c, at, audience, view))
        .collect();
    if world.records.contains_key(reference) {
        return visible
            .into_iter()
            .find(|c| c.record.id == reference)
            .ok_or_else(|| Error::NotFound("lore concept is unavailable".into()));
    }
    let folded = reference.to_lowercase();
    let matches: Vec<_> = visible
        .into_iter()
        .filter(|c| {
            c.record.title.to_lowercase() == folded
                || c.record.list("aliases").is_some_and(|v| {
                    v.iter()
                        .filter_map(Yaml::as_str)
                        .any(|alias| alias.to_lowercase() == folded)
                })
        })
        .collect();
    match matches.as_slice() {
        [one] => Ok(one),
        [] => Err(Error::NotFound("lore concept is unavailable".into())),
        _ => Err(Error::Ambiguous(
            "ambiguous visible lore concept; use an exact ID".into(),
        )),
    }
}
/// One complete admitted snapshot for bounded family consumers. Their own
/// relevant-profile and traversal bounds apply after the shared admission gate.
pub(crate) fn scoped_concept_values(
    world: &World,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
) -> Result<Vec<Json>> {
    query_arguments(world, at, audience, view)?;
    let registry = read(world)?;
    let concepts = concepts(world, &registry)?;
    let allowed = allowed_ids(world, &concepts, at, audience, view)?;
    concepts
        .iter()
        .filter(|concept| allowed.contains(&concept.record.id))
        .map(|concept| concept_value(world, concept, &registry, &allowed, at))
        .collect()
}
/// Full-author concept composition projects typed endpoints against the one
/// union scope, preserving native indexes and literal lookalike strings.
pub(crate) fn full_author_concept_values(
    world: &World,
    at: Option<&StoryTime>,
) -> Result<Vec<Json>> {
    query_arguments(world, at, "author", "author")?;
    let registry = read(world)?;
    let concepts = concepts(world, &registry)?;
    let mut allowed = BTreeSet::new();
    for record in world.records.values() {
        if crate::description::full_author_record_visible(world, record, at)? {
            allowed.insert(record.id.clone());
        }
    }
    concepts
        .iter()
        .filter(|concept| allowed.contains(&concept.record.id))
        .map(|concept| concept_value(world, concept, &registry, &allowed, at))
        .collect()
}

/// Complete author catalogue. Admission precedes filters, projection and bounds.
pub fn catalog(
    world: &World,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
    domain_filter: Option<&str>,
    limit: usize,
) -> Result<Json> {
    limit_argument(limit)?;
    query_arguments(world, at, audience, view)?;
    if domain_filter.is_some_and(|v| !domain(v)) {
        return Err(Error::InvalidInput(
            "domain must be an exact dotted identifier".into(),
        ));
    }
    let registry = read(world)?;
    let concepts = concepts(world, &registry)?;
    let allowed = allowed_ids(world, &concepts, at, audience, view)?;
    let selected: Vec<_> = concepts
        .iter()
        .filter(|c| {
            allowed.contains(&c.record.id) && domain_filter.is_none_or(|domain| domain == c.domain)
        })
        .collect();
    bounded_result(selected.len(), limit)?;
    let items: Vec<_> = selected
        .into_iter()
        .map(|c| concept_value(world, c, &registry, &allowed, at))
        .collect::<Result<_>>()?;
    let mut result = envelope(at, "authored-lore-concepts");
    result["audience"] = json!(audience);
    result["view"] = json!(view);
    result["domain"] = json!(domain_filter);
    result["count"] = json!(items.len());
    result["items"] = json!(items);
    result["limit"] = json!(limit);
    Ok(result)
}
pub fn concept(
    world: &World,
    reference: &str,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
) -> Result<Json> {
    query_arguments(world, at, audience, view)?;
    let registry = read(world)?;
    let concepts = concepts(world, &registry)?;
    let selected = selected(world, &concepts, reference, at, audience, view)?;
    let allowed = allowed_ids(world, &concepts, at, audience, view)?;
    let mut result = envelope(at, "authored-lore-concept");
    result["audience"] = json!(audience);
    result["view"] = json!(view);
    result.as_object_mut().unwrap().extend(
        concept_value(world, selected, &registry, &allowed, at)?
            .as_object()
            .unwrap()
            .clone(),
    );
    Ok(result)
}
/// Explicit outgoing links only; neither link presence nor a target grants traits.
pub fn links(
    world: &World,
    reference: &str,
    at: Option<&StoryTime>,
    audience: &str,
    view: &str,
    limit: usize,
) -> Result<Json> {
    limit_argument(limit)?;
    query_arguments(world, at, audience, view)?;
    let registry = read(world)?;
    let concepts = concepts(world, &registry)?;
    let allowed = allowed_ids(world, &concepts, at, audience, view)?;
    let owner = if let Some(record) = world.records.get(reference) {
        if !allowed.contains(&record.id) {
            return Err(Error::NotFound("lore link owner is unavailable".into()));
        }
        record
    } else {
        // Filter hidden concept aliases before ordinary author resolution, too.
        let folded = reference.to_lowercase();
        let matches: Vec<_> = world
            .records
            .values()
            .filter(|r| allowed.contains(&r.id))
            .filter(|r| {
                r.title.to_lowercase() == folded
                    || r.list("aliases").is_some_and(|v| {
                        v.iter()
                            .filter_map(Yaml::as_str)
                            .any(|alias| alias.to_lowercase() == folded)
                    })
            })
            .collect();
        match matches.as_slice() {
            [one] => *one,
            [] => return Err(Error::NotFound("lore link owner is unavailable".into())),
            _ => {
                return Err(Error::Ambiguous(
                    "ambiguous visible lore link owner; use an exact ID".into(),
                ));
            }
        }
    };
    let authored = owner
        .get("concept_links")
        .map(|v| link_members(v, Some(world)))
        .transpose()?
        .unwrap_or_default();
    let selected: Vec<_> = authored
        .iter()
        .filter(|l| allowed.contains(l.target) && l.valid.applies(at))
        .collect();
    bounded_result(selected.len(), limit)?;
    let items: Vec<_> = selected
        .into_iter()
        .map(|link| link_value(world, owner, link))
        .collect::<Result<_>>()?;
    let mut result = envelope(at, "authored-concept-links");
    result["audience"] = json!(audience);
    result["view"] = json!(view);
    result["ownerId"] = json!(owner.id);
    result["ownerKind"] = json!(owner.kind);
    result["title"] = json!(owner.title);
    result["count"] = json!(items.len());
    result["items"] = json!(items);
    result["limit"] = json!(limit);
    Ok(result)
}
