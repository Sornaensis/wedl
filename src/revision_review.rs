//! Bounded author comparison of three original, immutable Git revisions.
//!
//! Every world is independently validated and observed at the same exact time.
//! Source cells retain their original paths, blob identities and ordinals. This
//! module never constructs a merged world, consults HEAD, or proposes an apply.
use crate::{
    Error, Result, briefs,
    consequence_semantics::{Time, projection},
    description, historical, lore,
    model::{Record, StoryTime, World},
    scene, state_schema, validation,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use serde_yaml::{Mapping, Value as Yaml};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::Path,
};

const MAX_REQUEST_BYTES: usize = 65_536;
const MAX_RECORDS: usize = 8192;
const MAX_SOURCE_BYTES: usize = 64 * 1024 * 1024;
const MAX_NODES: usize = 200_000;
const MAX_SOURCE_PATH_BYTES: usize = 8 * 1024 * 1024;
const MAX_WORK: usize = 8_000_000;
const MAX_POSITIONS: usize = 4096;
const MAX_CITATIONS: usize = 16_384;
const MAX_REPORT_BYTES: usize = 8 * 1024 * 1024;
const GENERATIONAL: &[&str] = &[
    "organization",
    "parentage",
    "union",
    "affiliation",
    "legacy",
    "tenure",
    "claim",
    "vital-history",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub base_revision: String,
    pub left_revision: String,
    pub right_revision: String,
    pub at: Time,
    pub max_items: usize,
    pub max_characters: usize,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}
fn unavailable(message: impl Into<String>) -> Error {
    Error::NotFound(message.into())
}

fn request_time(request: &Request) -> Result<StoryTime> {
    if [
        &request.base_revision,
        &request.left_revision,
        &request.right_revision,
    ]
    .into_iter()
    .any(|pin| !historical::valid_oid(pin))
    {
        return Err(invalid(
            "revision review requires three literal full lowercase commit object IDs",
        ));
    }
    if !(1..=1000).contains(&request.max_items)
        || !(256..=1_000_000).contains(&request.max_characters)
    {
        return Err(invalid(
            "revision review requires maxItems 1..1000 and maxCharacters 256..1000000",
        ));
    }
    request.at.point()
}

/// Decode one closed request. Time coordinates are canonical decimal strings;
/// serde rejects duplicate fields and numeric tokens rather than coercing them.
pub fn decode(raw: &str) -> Result<Request> {
    if raw.len() > MAX_REQUEST_BYTES {
        return Err(invalid("revision review request exceeds 64 KiB"));
    }
    let request: Request = serde_json::from_str(raw)
        .map_err(|e| invalid(format!("invalid revision review request: {e}")))?;
    request_time(&request)?;
    Ok(request)
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Work {
    source_records: usize,
    source_bytes: usize,
    source_nodes: usize,
    source_path_bytes: usize,
    source_positions: usize,
    estimated_work: usize,
    projection_work: usize,
}
impl Work {
    fn step(&mut self, amount: usize) -> Result<()> {
        self.projection_work = self.projection_work.saturating_add(amount);
        if self.estimated_work.saturating_add(self.projection_work) > MAX_WORK {
            return Err(invalid(
                "complete revision review exceeds 8000000 aggregate work units",
            ));
        }
        Ok(())
    }
}

fn source_bounds<'a>(worlds: impl IntoIterator<Item = &'a World>) -> Result<Work> {
    let mut work = Work::default();
    for world in worlds {
        projection::preflight(world)?;
        work.source_records = work.source_records.saturating_add(world.records.len());
        let mut world_nodes = 0usize;
        let mut folds = 0usize;
        let mut members = 0usize;
        for record in world.records.values() {
            work.source_bytes = work.source_bytes.saturating_add(record.raw_bytes.len());
            folds += usize::from(matches!(
                record.kind.as_str(),
                "character" | "object" | "location" | "environment" | "story-point"
            ));
            members = members
                .saturating_add(record.list("effects").map_or(0, Vec::len))
                .saturating_add(record.list("transitions").map_or(0, Vec::len))
                .saturating_add(
                    record
                        .get("lifecycle")
                        .and_then(|v| v.get("transitions"))
                        .and_then(Yaml::as_sequence)
                        .map_or(0, Vec::len),
                );
            let mut pending = vec![(&record.frontmatter, 0usize, 0usize)];
            while let Some((value, depth, path_bytes)) = pending.pop() {
                work.source_nodes = work.source_nodes.saturating_add(1);
                work.source_path_bytes = work.source_path_bytes.saturating_add(path_bytes);
                if record.kind == "world" {
                    world_nodes += 1;
                }
                if work.source_nodes > MAX_NODES || depth > 64 {
                    return Err(invalid(
                        "revision review source exceeds 200000 aggregate nodes or 64 levels",
                    ));
                }
                if path_bytes > 4096 || work.source_path_bytes > MAX_SOURCE_PATH_BYTES {
                    return Err(invalid(
                        "revision review source paths exceed 4096 bytes per path or 8 MiB aggregate expansion",
                    ));
                }
                match value {
                    Yaml::Mapping(values) => {
                        if values.keys().any(|key| !key.is_string()) {
                            return Err(Error::Unsupported(
                                "revision review requires literal string mapping keys".into(),
                            ));
                        }
                        pending.extend(values.iter().flat_map(|(k, v)| {
                            let expanded = path_bytes
                                .saturating_add(k.as_str().unwrap().len())
                                .saturating_add(1);
                            [(k, depth + 1, expanded), (v, depth + 1, expanded)]
                        }));
                    }
                    Yaml::Sequence(values) => pending.extend(
                        values
                            .iter()
                            .map(|v| (v, depth + 1, path_bytes.saturating_add(22))),
                    ),
                    Yaml::Tagged(_) => {
                        return Err(Error::Unsupported(
                            "revision review does not interpret YAML tags".into(),
                        ));
                    }
                    _ => {}
                }
            }
        }
        // Includes repeated declaration parsing, graph validation and native
        // semantic replay, before any validator or semantic fold is invoked.
        let count = world.records.len();
        work.estimated_work = work
            .estimated_work
            .saturating_add(count.saturating_mul(count.saturating_add(world_nodes)))
            .saturating_add(folds.saturating_mul(count.saturating_add(members)));
        if work.source_records > MAX_RECORDS || work.source_bytes > MAX_SOURCE_BYTES {
            return Err(invalid(
                "revision review source exceeds 8192 records or 64 MiB in aggregate",
            ));
        }
        work.step(0)?;
    }
    Ok(work)
}

fn point(world: &World, raw: &Yaml) -> Result<StoryTime> {
    let time = StoryTime::from_yaml(raw, &world.default_timeline())?;
    world.story_time(time.tick, Some(&time.timeline), time.order)
}
fn before(world: &World, raw: &Yaml, at: &StoryTime) -> Result<bool> {
    Ok(point(world, raw)?.not_after(at))
}
fn required<'a>(value: &'a Yaml, key: &str) -> Result<&'a Yaml> {
    value
        .get(key)
        .ok_or_else(|| unavailable(format!("validated revision source lacks {key}")))
}
fn applicable(world: &World, raw: &Yaml, at: &StoryTime) -> Result<bool> {
    Ok(lore::validity(raw, Some(world), "revision source validity")?.applies(Some(at)))
}
fn labels(raw: &Yaml, key: &str, options: &[&str]) -> Result<bool> {
    match raw.get(key) {
        None => Ok(true),
        Some(value) => Ok(lore::ordered_labels(value, key)?
            .into_iter()
            .any(|label| options.contains(&label))),
    }
}
fn author_labels(raw: &Yaml) -> Result<bool> {
    Ok(labels(raw, "audience", &["public", "author"])?
        && labels(raw, "perspectives", &["ordinary", "author"])?)
}
fn initialization(world: &World, raw: &Yaml, at: &StoryTime) -> Result<bool> {
    let applicability = required(raw, "applicability")?;
    before(
        world,
        applicability
            .get("point")
            .or_else(|| applicability.get("first"))
            .ok_or_else(|| unavailable("generational applicability lacks an exact first point"))?,
        at,
    )
}

pub(crate) fn owner_admitted(world: &World, record: &Record, at: &StoryTime) -> Result<bool> {
    if record.kind == "existence-episode" {
        if record.status() != "canonical"
            || !author_labels(&record.frontmatter)?
            || !applicable(world, required(&record.frontmatter, "valid")?, at)?
        {
            return Ok(false);
        }
        let character = record
            .string("character_id")
            .and_then(|id| world.records.get(id))
            .ok_or_else(|| {
                unavailable("existence episode lacks its validated character endpoint")
            })?;
        let mut visible = false;
        for audience in ["public", "author"] {
            for view in ["ordinary", "author"] {
                visible |= description::record_visible(world, character, Some(at), audience, view)?;
            }
        }
        return Ok(visible);
    }
    let mut visible = false;
    for audience in ["public", "author"] {
        for view in ["ordinary", "author"] {
            visible |= description::record_visible(world, record, Some(at), audience, view)?;
        }
    }
    if !visible || !projection::admitted(world, record, at)? {
        return Ok(false);
    }
    if GENERATIONAL.contains(&record.kind.as_str()) {
        return Ok(record.status() == "canonical"
            && initialization(world, required(&record.frontmatter, "initialization")?, at)?);
    }
    if record.kind == "assertion" {
        return Ok(record.status() == "canonical"
            && applicable(world, required(&record.frontmatter, "valid")?, at)?);
    }
    if record.kind == "overlay"
        && let Some(valid) = record.get("valid")
    {
        return Ok(before(world, required(valid, "start")?, at)?
            && at.not_after(&point(world, required(valid, "end")?)?));
    }
    if record.kind == "knowledge"
        && record.get("claim").is_some_and(|claim| {
            claim.get("assertion_id").is_some() || claim.get("genealogy").is_some()
        })
    {
        for member in record.list("transitions").into_iter().flatten() {
            if before(world, required(member, "time")?, at)?
                && member
                    .get("state")
                    .and_then(Yaml::as_str)
                    .is_some_and(|state| {
                        ["accepted", "suspected", "uncertain", "remembered"].contains(&state)
                    })
            {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    Ok(true)
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Token {
    Key(String),
    Index(usize),
}
type Tokens = Vec<Token>;
fn field_path(tokens: &[Token]) -> String {
    let mut result = String::new();
    for token in tokens {
        match token {
            Token::Key(key) => {
                if !result.is_empty() {
                    result.push('.');
                }
                result.push_str(key);
            }
            Token::Index(index) => result.push_str(&format!("[{index}]")),
        }
    }
    result
}
fn index_paths(
    raw: &Yaml,
    path: &mut Tokens,
    result: &mut BTreeMap<String, Vec<Tokens>>,
    work: &mut Work,
) -> Result<()> {
    work.step(1)?;
    result
        .entry(field_path(path))
        .or_default()
        .push(path.clone());
    match raw {
        Yaml::Mapping(values) => {
            for (key, value) in values {
                path.push(Token::Key(
                    key.as_str()
                        .ok_or_else(|| unavailable("nonliteral mapping key"))?
                        .into(),
                ));
                index_paths(value, path, result, work)?;
                path.pop();
            }
        }
        Yaml::Sequence(values) => {
            for (ordinal, value) in values.iter().enumerate() {
                path.push(Token::Index(ordinal));
                index_paths(value, path, result, work)?;
                path.pop();
            }
        }
        _ => {}
    }
    Ok(())
}

fn condition_refs(raw: &Yaml, path: &str, out: &mut Vec<(String, String)>) {
    let Some(fields) = raw.as_mapping() else {
        return;
    };
    for (operator, value) in fields {
        let Some(operator) = operator.as_str() else {
            continue;
        };
        let prefix = format!("{path}.{operator}");
        match operator {
            "all" | "any" => {
                for (ordinal, child) in value.as_sequence().into_iter().flatten().enumerate() {
                    condition_refs(child, &format!("{prefix}[{ordinal}]"), out);
                }
            }
            "not" => condition_refs(value, &prefix, out),
            "event" => {
                if let Some(id) = value.as_str() {
                    out.push((prefix, id.into()));
                }
            }
            "state" | "knowledge" | "relationship" => {
                let key = match operator {
                    "state" => "entity",
                    "knowledge" => "character",
                    _ => "id",
                };
                if let Some(id) = value.get(key).and_then(Yaml::as_str) {
                    out.push((format!("{prefix}.{key}"), id.into()));
                }
            }
            "scene" => {
                for key in ["location", "participant", "object"] {
                    if let Some(id) = value.get(key).and_then(Yaml::as_str) {
                        out.push((format!("{prefix}.{key}"), id.into()));
                    }
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn references(world: &World, record: &Record) -> Result<Vec<(String, String)>> {
    let mut refs = validation::collect_refs(world, record);
    refs.extend(lore::references(world, record)?);
    if record.kind == "story-point" {
        if let Some(when) = record.get("when") {
            condition_refs(when, "when", &mut refs);
        }
        for (ordinal, value) in record.list("requires").into_iter().flatten().enumerate() {
            if let Some(id) = value.get("id").and_then(Yaml::as_str) {
                refs.push((format!("requires[{ordinal}].id"), id.into()));
            }
        }
    }
    // The core extractor has scalar entity operands. Set operands are declared
    // separately here, never discovered by scanning arbitrary string values.
    let mut state_value = |owner: &Record, key: &str, raw: &Yaml, prefix: &str| -> Result<()> {
        let definition = state_schema::field_definition(world, &owner.kind, key)?;
        if definition.profile.is_none()
            && matches!(definition.value_type, state_schema::StateType::EntitySet)
        {
            if let Some(values) = raw.as_sequence() {
                for (ordinal, member) in values.iter().enumerate() {
                    if let Some(id) = member.get("entity").and_then(Yaml::as_str) {
                        refs.push((format!("{prefix}[{ordinal}].entity"), id.into()));
                    }
                }
            } else if let Some(id) = raw.get("entity").and_then(Yaml::as_str) {
                refs.push((format!("{prefix}.entity"), id.into()));
            }
        }
        Ok(())
    };
    if let Some(initial) = record.get("initial_state").and_then(Yaml::as_mapping) {
        for (key, value) in initial {
            if let Some(key) = key.as_str() {
                state_value(record, key, value, &format!("initial_state.{key}"))?;
            }
        }
    }
    if record.kind == "event" {
        for (ordinal, effect) in record.list("effects").into_iter().flatten().enumerate() {
            if let (Some(owner), Some(key), Some(value)) = (
                effect
                    .get("target")
                    .and_then(Yaml::as_str)
                    .and_then(|id| world.records.get(id)),
                effect.get("key").and_then(Yaml::as_str),
                effect.get("value"),
            ) {
                state_value(owner, key, value, &format!("effects[{ordinal}].value"))?;
            }
        }
    }
    refs.sort();
    refs.dedup();
    Ok(refs)
}

#[derive(Clone)]
pub(crate) enum Node {
    Scalar(Yaml),
    Map(BTreeMap<String, Node>),
    List(Vec<(usize, Node)>),
}
impl Node {
    pub(crate) fn value(&self) -> Yaml {
        match self {
            Self::Scalar(value) => value.clone(),
            Self::Map(values) => Yaml::Mapping(
                values
                    .iter()
                    .map(|(k, v)| (Yaml::String(k.clone()), v.value()))
                    .collect::<Mapping>(),
            ),
            Self::List(values) => Yaml::Sequence(values.iter().map(|(_, v)| v.value()).collect()),
        }
    }
}
fn identity<'a>(record: &Record, field: &str, raw: &'a Yaml) -> Option<&'a str> {
    let key = if matches!(field, "transitions" | "initialization")
        && GENERATIONAL.contains(&record.kind.as_str())
    {
        "transition_id"
    } else {
        "id"
    };
    let declared = matches!(
        field,
        "transitions"
            | "lifecycle.transitions"
            | "initialization"
            | "effects"
            | "descriptions"
            | "spatial_history"
            | "turns"
            | "recollections"
            | "observations"
    ) || record.kind == "world"
        && matches!(
            field,
            "profiles"
                | "predicates"
                | "timelines"
                | "chronology.calendars"
                | "chronology.eras"
                | "chronology.anchors"
        );
    declared
        .then(|| raw.get(key).and_then(Yaml::as_str))
        .flatten()
}

pub(crate) fn member_admitted(
    world: &World,
    record: &Record,
    field: &str,
    member: &Yaml,
    at: &StoryTime,
) -> Result<bool> {
    match field {
        "transitions" if GENERATIONAL.contains(&record.kind.as_str()) => {
            initialization(world, member, at)
        }
        "transitions" | "lifecycle.transitions" => before(world, required(member, "time")?, at),
        "observations" | "turns" | "recollections" => before(world, required(member, "at")?, at),
        "participants" if matches!(record.kind.as_str(), "scene" | "conversation") => before(
            world,
            member
                .get("from")
                .filter(|v| !v.is_null())
                .or_else(|| record.get("time").and_then(|v| v.get("start")))
                .ok_or_else(|| unavailable("participant lacks an authored start"))?,
            at,
        ),
        "descriptions" | "concept_links" => {
            Ok(applicable(world, required(member, "valid")?, at)? && author_labels(member)?)
        }
        "spatial_history" => applicable(world, required(member, "valid")?, at),
        _ => Ok(true),
    }
}

fn project_node(
    world: &World,
    record: &Record,
    raw: &Yaml,
    path: &mut Tokens,
    denied: &BTreeSet<Tokens>,
    at: &StoryTime,
    work: &mut Work,
) -> Result<Option<Node>> {
    work.step(1)?;
    if denied.contains(path) {
        return Ok(None);
    }
    let field = field_path(path);
    Ok(Some(match raw {
        Yaml::Mapping(values) => {
            let mut map = BTreeMap::new();
            for (key, value) in values {
                let key = key
                    .as_str()
                    .ok_or_else(|| unavailable("nonliteral mapping key"))?;
                path.push(Token::Key(key.into()));
                if let Some(value) = project_node(world, record, value, path, denied, at, work)? {
                    map.insert(key.into(), value);
                }
                path.pop();
            }
            Node::Map(map)
        }
        Yaml::Sequence(values) => {
            let mut selected = Vec::new();
            let mut ids = BTreeSet::new();
            for (ordinal, value) in values.iter().enumerate() {
                if !member_admitted(world, record, &field, value, at)? {
                    continue;
                }
                path.push(Token::Index(ordinal));
                if let Some(value) = project_node(world, record, value, path, denied, at, work)? {
                    if let Some(id) = identity(record, &field, &value.value())
                        && !ids.insert(id.to_owned())
                    {
                        return Err(unavailable(
                            "revision review requires unique owner-local member identities",
                        ));
                    }
                    selected.push((ordinal, value));
                }
                path.pop();
            }
            // Applicable correction source members remain visible. A withheld
            // predecessor is not an operative source pointer at this horizon.
            if matches!(field.as_str(), "descriptions" | "spatial_history") {
                for (_, node) in &mut selected {
                    if let Node::Map(fields) = node
                        && fields
                            .get("replaces")
                            .is_some_and(|v| v.value().as_str().is_some_and(|id| !ids.contains(id)))
                    {
                        fields.remove("replaces");
                    }
                }
            }
            Node::List(selected)
        }
        _ => Node::Scalar(raw.clone()),
    }))
}

fn source_node(
    world: &World,
    record: &Record,
    allowed: &BTreeSet<String>,
    at: &StoryTime,
    work: &mut Work,
) -> Result<Node> {
    let mut paths = BTreeMap::new();
    index_paths(&record.frontmatter, &mut Vec::new(), &mut paths, work)?;
    let mut denied = BTreeSet::new();
    for (field, target) in references(world, record)? {
        work.step(1)?;
        if allowed.contains(&target) {
            continue;
        }
        let positions = paths.get(&field).ok_or_else(|| {
            unavailable(format!("reference source position {field} is unavailable"))
        })?;
        if positions.len() != 1 {
            return Err(Error::Unsupported(
                "reference positions are ambiguous under dotted source keys".into(),
            ));
        }
        let mut path = positions[0].clone();
        if matches!(path.first(), Some(Token::Key(key)) if key == "effects" || key == "concept_links")
        {
            path.truncate(2);
        } else if matches!(path.first(), Some(Token::Key(key)) if key == "participants")
            && matches!(path.last(), Some(Token::Key(key)) if key == "character")
        {
            path.pop();
        } else if matches!(path.last(), Some(Token::Key(key)) if key == "id" || key == "entity") {
            path.pop();
        }
        denied.insert(path);
    }
    if let Some(genealogy) = record.get("claim").and_then(|value| value.get("genealogy")) {
        if let Some(valid) = genealogy.get("valid")
            && !before(world, required(valid, "from")?, at)?
        {
            denied.insert(vec![
                Token::Key("claim".into()),
                Token::Key("genealogy".into()),
            ]);
        }
    }
    project_node(
        world,
        record,
        &record.frontmatter,
        &mut Vec::new(),
        &denied,
        at,
        work,
    )?
    .ok_or_else(|| unavailable("whole revision source is unavailable after reference admission"))
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CellKey {
    section: String,
    member: Option<String>,
    ordinal: Option<usize>,
}
struct Cell {
    value: Yaml,
    position: Json,
    citation: Json,
    markdown: bool,
    transition: bool,
}
struct RecordView {
    fields: Yaml,
    body: String,
    cells: BTreeMap<CellKey, Cell>,
    sequences: BTreeMap<String, Cell>,
}
struct View<'a> {
    world: &'a World,
    records: BTreeMap<String, RecordView>,
    semantic: projection::Projection,
}

fn citation(
    world: &World,
    record: &Record,
    section: &str,
    ordinal: Option<usize>,
    member: Option<&str>,
) -> Json {
    json!({"entityId":record.id,"sourcePath":record.source_path,"blobOid":record.blob_oid,
        "revision":world.revision,"fingerprint":world.fingerprint,"section":section,
        "sourceOrdinal":ordinal,"memberId":member})
}
fn section(field: &str) -> String {
    if [
        "initial_state",
        "initialization",
        "effects",
        "transitions",
        "lifecycle",
        "descriptions",
        "spatial_history",
        "concept_links",
    ]
    .contains(&field.split('.').next().unwrap_or(""))
    {
        field.into()
    } else {
        format!("frontmatter.{field}")
    }
}
fn cell(
    world: &World,
    record: &Record,
    section: &str,
    value: Yaml,
    ordinal: Option<usize>,
    member: Option<&str>,
    markdown: bool,
    transition: bool,
) -> Cell {
    Cell {
        value,
        position: json!({"section":section,"sourceOrdinal":ordinal,"memberId":member}),
        citation: citation(world, record, section, ordinal, member),
        markdown,
        transition,
    }
}
fn reserve_position(count: &mut usize, work: &mut Work) -> Result<()> {
    *count = count.saturating_add(1);
    work.source_positions = work.source_positions.saturating_add(1);
    if *count > MAX_POSITIONS || work.source_positions > MAX_POSITIONS * 3 {
        return Err(invalid(
            "complete revision source view exceeds 4096 positions per original or 12288 in aggregate",
        ));
    }
    Ok(())
}
fn cells(
    world: &World,
    record: &Record,
    node: &Node,
    field: &str,
    view: &mut RecordView,
    count: &mut usize,
    work: &mut Work,
) -> Result<()> {
    work.step(1)?;
    let section = section(field);
    match node {
        Node::Map(values) if !values.is_empty() && field != "initialization" => {
            for (key, value) in values {
                cells(
                    world,
                    record,
                    value,
                    &if field.is_empty() {
                        key.clone()
                    } else {
                        format!("{field}.{key}")
                    },
                    view,
                    count,
                    work,
                )?;
            }
        }
        Node::List(values) if !values.is_empty() => {
            let mut stable = true;
            for (admitted_ordinal, (ordinal, value)) in values.iter().enumerate() {
                reserve_position(count, work)?;
                let value = value.value();
                let member = identity(record, field, &value).map(str::to_owned);
                stable &= member.is_some();
                let key = CellKey {
                    section: section.clone(),
                    ordinal: member.is_none().then_some(admitted_ordinal),
                    member: member.clone(),
                };
                let item = cell(
                    world,
                    record,
                    &section,
                    value,
                    Some(*ordinal),
                    member.as_deref(),
                    false,
                    matches!(field, "transitions" | "lifecycle.transitions"),
                );
                if view.cells.insert(key, item).is_some() {
                    return Err(unavailable("source cell identities are ambiguous"));
                }
            }
            if stable {
                reserve_position(count, work)?;
                view.sequences.insert(
                    section.clone(),
                    cell(
                        world,
                        record,
                        &section,
                        node.value(),
                        None,
                        None,
                        false,
                        false,
                    ),
                );
            }
        }
        _ => {
            reserve_position(count, work)?;
            let value = node.value();
            let member = identity(record, field, &value).map(str::to_owned);
            let key = CellKey {
                section: section.clone(),
                ordinal: None,
                member: member.clone(),
            };
            if view
                .cells
                .insert(
                    key,
                    cell(
                        world,
                        record,
                        &section,
                        value,
                        None,
                        member.as_deref(),
                        false,
                        field == "initialization",
                    ),
                )
                .is_some()
            {
                return Err(Error::Unsupported(
                    "dotted source keys produce ambiguous review cells".into(),
                ));
            }
        }
    }
    Ok(())
}

fn allowed_ids(
    world: &World,
    at: &StoryTime,
    excluded_kinds: &[&str],
    empty_histories: bool,
    work: &mut Work,
) -> Result<BTreeSet<String>> {
    let mut allowed = BTreeSet::new();
    for record in world.records.values() {
        work.step(1)?;
        let empty_history = empty_histories
            && matches!(record.kind.as_str(), "knowledge" | "relationship")
            && (record.kind != "knowledge" || record.status() == "canonical")
            && record.list("transitions").is_none_or(Vec::is_empty);
        let mut empty_visible = false;
        if empty_history {
            for audience in ["public", "author"] {
                for view in ["ordinary", "author"] {
                    empty_visible |=
                        description::record_visible(world, record, Some(at), audience, view)?;
                }
            }
        }
        if !excluded_kinds.contains(&record.kind.as_str())
            && (owner_admitted(world, record, at)? || empty_visible)
        {
            allowed.insert(record.id.clone());
        }
    }
    // Required exact pins are a conjunction. Withhold the entire brief when a
    // required owner/member is outside this review horizon, including through
    // another brief. Never turn a required reference into a partial operand.
    loop {
        let mut removed = Vec::new();
        for record in world
            .records
            .values()
            .filter(|r| r.kind == "context-brief" && allowed.contains(&r.id))
        {
            for reference in record
                .list("required_refs")
                .into_iter()
                .flatten()
                .chain(record.get("scope_ref"))
            {
                work.step(1)?;
                if !brief_reference_admitted(world, reference, &allowed, at)? {
                    removed.push(record.id.clone());
                    break;
                }
            }
        }
        for record in world
            .records
            .values()
            .filter(|r| r.kind == "assertion" && allowed.contains(&r.id))
        {
            work.step(1)?;
            if crate::assertion::references(world, record)?
                .iter()
                .any(|(field, id)| {
                    matches!(
                        field.as_str(),
                        "subject.id" | "attributed_to.id" | "value.id"
                    ) && !allowed.contains(id)
                })
            {
                removed.push(record.id.clone());
            }
        }
        // Check one union closure before keeping a required brief. Separate
        // single-label closures cannot admit independently scoped targets.
        for record in world
            .records
            .values()
            .filter(|record| record.kind == "context-brief" && allowed.contains(&record.id))
        {
            if removed.contains(&record.id) {
                continue;
            }
            work.step(world.records.len().saturating_mul(1000))?;
            if briefs::collect_full_author(world, std::slice::from_ref(&record.id), Some(at), 1000)?
                .is_none()
            {
                removed.push(record.id.clone());
            }
        }
        if removed.is_empty() {
            break;
        }
        for id in removed {
            allowed.remove(&id);
        }
    }
    Ok(allowed)
}

/// Source-only native author projection, shared by read-only attribution and
/// impact analysis. Callers validate the original World before entering this
/// function. Original list ordinals survive every admission/pruning decision;
/// no semantic fold is performed and no synthetic source is published.
pub(crate) fn source_view(
    world: &World,
    at: &StoryTime,
    excluded_kinds: &[&str],
) -> Result<BTreeMap<String, Node>> {
    admitted_source(world, at, excluded_kinds, false)
}

/// Empty untimed native histories retain literal authored source for grouped
/// record edits. Future-only histories remain excluded, and this source view
/// never replaces the original world used for semantic folds.
pub(crate) fn consequence_source_view(
    world: &World,
    at: &StoryTime,
) -> Result<BTreeMap<String, Node>> {
    admitted_source(world, at, &[], true)
}

fn admitted_source(
    world: &World,
    at: &StoryTime,
    excluded_kinds: &[&str],
    empty_histories: bool,
) -> Result<BTreeMap<String, Node>> {
    let mut work = source_bounds([world])?;
    projection::horizon(world, at)?;
    let allowed = allowed_ids(world, at, excluded_kinds, empty_histories, &mut work)?;
    let mut records = BTreeMap::new();
    for id in &allowed {
        records.insert(
            id.clone(),
            source_node(world, &world.records[id], &allowed, at, &mut work)?,
        );
    }
    Ok(records)
}

fn build_view<'a>(world: &'a World, at: &StoryTime, work: &mut Work) -> Result<View<'a>> {
    let allowed = allowed_ids(world, at, &[], false, work)?;
    let mut records = BTreeMap::new();
    let mut count = 0usize;
    for id in &allowed {
        let record = &world.records[id];
        let node = source_node(world, record, &allowed, at, work)?;
        let mut view = RecordView {
            fields: node.value(),
            body: record.body.clone(),
            cells: BTreeMap::new(),
            sequences: BTreeMap::new(),
        };
        cells(world, record, &node, "", &mut view, &mut count, work)?;
        reserve_position(&mut count, work)?;
        view.cells.insert(
            CellKey {
                section: "bodyMarkdown".into(),
                member: None,
                ordinal: None,
            },
            cell(
                world,
                record,
                "bodyMarkdown",
                Yaml::String(record.body.clone()),
                None,
                None,
                true,
                false,
            ),
        );
        records.insert(id.clone(), view);
    }
    Ok(View {
        world,
        records,
        semantic: projection::project(world, at)?,
    })
}

pub(crate) fn brief_reference_admitted(
    world: &World,
    reference: &Yaml,
    allowed: &BTreeSet<String>,
    at: &StoryTime,
) -> Result<bool> {
    let owner = briefs::resolve_reference_request(world, reference)?;
    if !allowed.contains(&owner.id) {
        return Ok(false);
    }
    let kind = required(reference, "kind")?
        .as_str()
        .ok_or_else(|| unavailable("invalid exact brief reference"))?;
    if kind == "record" {
        return Ok(true);
    }
    let id = required(reference, "id")?
        .as_str()
        .ok_or_else(|| unavailable("invalid exact brief member"))?;
    if kind == "section" {
        if let Some(member) = owner
            .list("descriptions")
            .into_iter()
            .flatten()
            .find(|member| member.get("id").and_then(Yaml::as_str) == Some(id))
        {
            if !member_admitted(world, owner, "descriptions", member, at)? {
                return Ok(false);
            }
            for replacement in owner.list("descriptions").into_iter().flatten() {
                if replacement.get("replaces").and_then(Yaml::as_str) == Some(id)
                    && member_admitted(world, owner, "descriptions", replacement, at)?
                {
                    return Ok(false);
                }
            }
            return Ok(true);
        }
        // The exact native resolver and full-author collector handle world chronology
        // section pins and their domain-specific applicability.
        return Ok(owner.kind == "world");
    }
    if GENERATIONAL.contains(&owner.kind.as_str()) {
        for member in owner
            .get("initialization")
            .into_iter()
            .chain(owner.list("transitions").into_iter().flatten())
        {
            if member.get("transition_id").and_then(Yaml::as_str) == Some(id) {
                return initialization(world, member, at);
            }
        }
    } else {
        let field = if owner.kind == "story-point" {
            owner
                .get("lifecycle")
                .and_then(|value| value.get("transitions"))
        } else {
            owner.get("transitions")
        };
        for member in field.and_then(Yaml::as_sequence).into_iter().flatten() {
            if member.get("id").and_then(Yaml::as_str) == Some(id) {
                return before(world, required(member, "time")?, at);
            }
        }
    }
    Ok(false)
}

fn equal(left: Option<&Cell>, right: Option<&Cell>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(l), Some(r)) => state_schema::values_equal(&l.value, &r.value),
        _ => false,
    }
}
fn divergent<T>(base: &T, left: &T, right: &T) -> bool
where
    T: PartialEq,
{
    base != left && base != right && left != right
}
fn record_equal(left: &RecordView, right: &RecordView) -> bool {
    state_schema::values_equal(&left.fields, &right.fields) && left.body == right.body
}
fn change(base: &View<'_>, side: &View<'_>, id: &str) -> &'static str {
    match (base.records.get(id), side.records.get(id)) {
        (None, None) => "none",
        (None, Some(_)) => "added",
        (Some(_), None) => "removed",
        (Some(left), Some(right)) if !record_equal(left, right) => "modified",
        (Some(_), Some(_))
            if base.world.records[id].source_path != side.world.records[id].source_path =>
        {
            "moved"
        }
        _ => "none",
    }
}
fn version(view: &View<'_>, id: &str) -> Json {
    if !view.records.contains_key(id) {
        return Json::Null;
    }
    let record = &view.world.records[id];
    json!({"id":id,"kind":record.kind,"title":record.title,"citation":citation(view.world, record, "frontmatter.title", None, None)})
}
fn text(cell: Option<&Cell>, markdown: bool) -> Result<Json> {
    Ok(match cell {
        Some(cell) => {
            json!({"present":true,"encoding":if markdown {"markdown"} else {"canonical-json"},
            "text":if markdown {cell.value.as_str().unwrap_or("").to_owned()} else {serde_json::to_string(&cell.value)?},"citation":cell.citation})
        }
        None => {
            json!({"present":false,"encoding":if markdown {"markdown"} else {"canonical-json"},"text":Json::Null,"citation":Json::Null})
        }
    })
}

struct Size {
    bytes: usize,
    characters: usize,
    limit: usize,
}
impl Write for Size {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes = self.bytes.saturating_add(bytes.len());
        // JSON's writer emits complete UTF-8 substrings; continuation bytes
        // never start a character even if a writer implementation splits one.
        self.characters = self
            .characters
            .saturating_add(bytes.iter().filter(|byte| **byte & 0xc0 != 0x80).count());
        if self.bytes > MAX_REPORT_BYTES || self.characters > self.limit {
            return Err(std::io::Error::other(
                "complete review report bound exceeded",
            ));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn measured(value: &impl Serialize, limit: usize) -> Result<Size> {
    let mut size = Size {
        bytes: 0,
        characters: 0,
        limit,
    };
    serde_json::to_writer(&mut size, value).map_err(|_| {
        invalid("complete revision review exceeds maxCharacters or 8 MiB; no result was truncated")
    })?;
    Ok(size)
}
struct ReportRows<'a> {
    request: &'a Request,
    changes: Vec<Json>,
    conflicts: Vec<Json>,
    bytes: usize,
    characters: usize,
    positions: usize,
    citations: usize,
}
impl<'a> ReportRows<'a> {
    fn new(request: &'a Request) -> Self {
        Self {
            request,
            changes: Vec::new(),
            conflicts: Vec::new(),
            bytes: 0,
            characters: 0,
            positions: 0,
            citations: 0,
        }
    }
    fn add(&mut self, value: Json, conflict: bool) -> Result<()> {
        if self.changes.len().saturating_add(self.conflicts.len()) >= self.request.max_items {
            return Err(invalid(
                "complete revision review exceeds maxItems; no result was truncated",
            ));
        }
        let size = measured(&value, self.request.max_characters)?;
        self.bytes = self.bytes.saturating_add(size.bytes);
        self.characters = self.characters.saturating_add(size.characters);
        let mut pending = vec![&value];
        while let Some(value) = pending.pop() {
            match value {
                Json::Object(fields) => {
                    self.citations += usize::from(
                        fields.contains_key("sourcePath") && fields.contains_key("blobOid"),
                    );
                    pending.extend(fields.values());
                }
                Json::Array(values) => pending.extend(values),
                _ => {}
            }
        }
        if self.bytes > MAX_REPORT_BYTES
            || self.characters > self.request.max_characters
            || self.citations > MAX_CITATIONS
        {
            return Err(invalid(
                "complete revision review exceeds its aggregate evidence or report bound",
            ));
        }
        if conflict {
            self.conflicts.push(value);
        } else {
            self.changes.push(value);
        }
        Ok(())
    }
    fn position(
        &mut self,
        position: &Json,
        values: &mut Vec<Json>,
        seen: &mut BTreeSet<String>,
    ) -> Result<()> {
        if seen.insert(serde_json::to_string(position)?) {
            self.positions += 1;
            if self.positions > MAX_POSITIONS {
                return Err(invalid(
                    "complete revision review exceeds 4096 changed positions",
                ));
            }
            values.push(position.clone());
        }
        Ok(())
    }
}

fn compare_sources(
    views: &[View<'_>; 3],
    rows: &mut ReportRows<'_>,
    work: &mut Work,
) -> Result<()> {
    let ids: BTreeSet<_> = views.iter().flat_map(|view| view.records.keys()).collect();
    for id in ids {
        work.step(1)?;
        if views
            .iter()
            .any(|view| view.world.records.contains_key(id) && !view.records.contains_key(id))
        {
            return Err(unavailable(
                "a record exists outside admission in one original revision; absence cannot be established",
            ));
        }
        let left_change = change(&views[0], &views[1], id);
        let right_change = change(&views[0], &views[2], id);
        if left_change == "none" && right_change == "none" {
            continue;
        }
        let versions = [
            version(&views[0], id),
            version(&views[1], id),
            version(&views[2], id),
        ];
        let duplicate = !views[0].records.contains_key(id)
            && views[1]
                .records
                .get(id)
                .zip(views[2].records.get(id))
                .is_some_and(|(left, right)| {
                    !record_equal(left, right)
                        || views[1].world.records[id].source_path
                            != views[2].world.records[id].source_path
                });
        if duplicate {
            rows.add(
                json!({"kind":"duplicate-id","id":id,"left":versions[1],"right":versions[2]}),
                true,
            )?;
        }
        let keys: BTreeSet<_> = views
            .iter()
            .filter_map(|view| view.records.get(id))
            .flat_map(|record| record.cells.keys())
            .collect();
        let mut positions = Vec::new();
        let mut seen = BTreeSet::new();
        for key in keys {
            work.step(1)?;
            let cells = views.each_ref().map(|view| {
                view.records
                    .get(id)
                    .and_then(|record| record.cells.get(key))
            });
            if equal(cells[0], cells[1]) && equal(cells[0], cells[2]) {
                continue;
            }
            let selected = cells[1].or(cells[2]).or(cells[0]).unwrap();
            rows.position(&selected.position, &mut positions, &mut seen)?;
            if !duplicate
                && !equal(cells[0], cells[1])
                && !equal(cells[0], cells[2])
                && !equal(cells[1], cells[2])
            {
                if selected.transition && key.member.is_some() {
                    rows.add(json!({"kind":"divergent-transition","ownerId":id,
                        "member":{"kind":"transition","ownerId":id,"id":key.member},
                        "base":cells[0].map(|c| &c.citation),"left":cells[1].map(|c| &c.citation),"right":cells[2].map(|c| &c.citation)}), true)?;
                } else {
                    rows.add(json!({"kind":"text","id":id,"position":selected.position,
                        "base":text(cells[0], selected.markdown)?,"left":text(cells[1], selected.markdown)?,"right":text(cells[2], selected.markdown)?}), true)?;
                }
            }
        }
        let fields: BTreeSet<_> = views
            .iter()
            .filter_map(|view| view.records.get(id))
            .flat_map(|record| record.sequences.keys())
            .collect();
        for field in fields {
            work.step(1)?;
            let sequences = views.each_ref().map(|view| {
                view.records
                    .get(id)
                    .and_then(|record| record.sequences.get(field))
            });
            let orders = sequences.map(|cell| {
                cell.map(|cell| {
                    cell.value
                        .as_sequence()
                        .unwrap()
                        .iter()
                        .filter_map(|member| {
                            member
                                .get("id")
                                .or_else(|| member.get("transition_id"))
                                .and_then(Yaml::as_str)
                                .map(str::to_owned)
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
            });
            let selected = sequences.iter().flatten().next().unwrap();
            if orders[0] != orders[1] || orders[0] != orders[2] {
                rows.position(&selected.position, &mut positions, &mut seen)?;
            }
            let right: BTreeSet<_> = orders[2].iter().collect();
            let shared: BTreeSet<_> = orders[1].iter().filter(|id| right.contains(id)).collect();
            let orders = orders.each_ref().map(|order| {
                order
                    .iter()
                    .filter(|id| shared.contains(id))
                    .collect::<Vec<_>>()
            });
            if !duplicate && divergent(&orders[0], &orders[1], &orders[2]) {
                rows.add(json!({"kind":"text","id":id,"position":selected.position,
                    "base":text(sequences[0], false)?,"left":text(sequences[1], false)?,"right":text(sequences[2], false)?}), true)?;
            }
        }
        let present = versions.iter().find(|v| !v.is_null()).unwrap();
        rows.add(json!({"id":id,"kind":present["kind"],"leftChange":left_change,"rightChange":right_change,
            "base":versions[0],"left":versions[1],"right":versions[2],"positions":positions}), false)?;
    }
    Ok(())
}

fn state_equal(left: Option<&projection::Snapshot>, right: Option<&projection::Snapshot>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left.equal(right),
        (None, None) => true,
        (Some(value), None) | (None, Some(value)) => value.payload.is_null(),
    }
}
fn state_wire(snapshot: Option<&projection::Snapshot>) -> Json {
    snapshot
        .map(projection::Snapshot::wire)
        .unwrap_or_else(|| json!({"presence":"absent","payload":Json::Null,"citations":[]}))
}
fn compare_state(views: &[View<'_>; 3], rows: &mut ReportRows<'_>, work: &mut Work) -> Result<()> {
    let subjects: BTreeSet<_> = views
        .iter()
        .flat_map(|view| view.semantic.snapshots.keys())
        .filter(|subject| matches!(subject, projection::Subject::State { .. }))
        .collect();
    if subjects.len() > projection::MAX_SUBJECTS {
        return Err(invalid(
            "complete revision review exceeds 1024 semantic subjects",
        ));
    }
    for subject in subjects {
        work.step(1)?;
        let projection::Subject::State { entity_id, .. } = subject else {
            unreachable!()
        };
        // Native local-author semantic scope contains every original record.
        // Narrative source-label admission does not remove a state subject or
        // its canonical effect history from this independent original fold.
        let snapshots = views
            .each_ref()
            .map(|view| view.semantic.snapshots.get(subject));
        for (view, snapshot) in views.iter().zip(snapshots) {
            if snapshot.is_none() && view.world.records.contains_key(entity_id) {
                return Err(Error::Unsupported(
                    "state comparison crosses an unavailable owner or removed state declaration"
                        .into(),
                ));
            }
        }
        if !state_equal(snapshots[0], snapshots[1])
            && !state_equal(snapshots[0], snapshots[2])
            && !state_equal(snapshots[1], snapshots[2])
        {
            rows.add(json!({"kind":"state","subject":subject,"base":state_wire(snapshots[0]),"left":state_wire(snapshots[1]),"right":state_wire(snapshots[2])}), true)?;
        }
    }
    Ok(())
}

type SceneSets = BTreeMap<(String, String), Vec<String>>;
fn scene_sets(view: &View<'_>, at: &StoryTime, work: &mut Work) -> Result<SceneSets> {
    let mut result = SceneSets::new();
    let mut active = Vec::new();
    for id in view.records.keys() {
        work.step(1)?;
        let record = &view.world.records[id];
        if record.kind != "scene" || record.status() != "active" {
            continue;
        }
        let time = required(&record.frontmatter, "time")?;
        let end = time
            .get("end")
            .filter(|v| !v.is_null())
            .or_else(|| time.get("current"))
            .ok_or_else(|| unavailable("active scene lacks a cursor"))?;
        if !before(view.world, required(time, "start")?, at)?
            || !at.not_after(&point(view.world, end)?)
        {
            continue;
        }
        active.push(id.clone());
        for member in record.list("participants").into_iter().flatten() {
            work.step(1)?;
            if let Some(character) = member.get("character").and_then(Yaml::as_str)
                && view.records.contains_key(character)
                && scene::present(view.world, record, character, at)?
            {
                result
                    .entry(("participation".into(), character.into()))
                    .or_default()
                    .push(id.clone());
            }
        }
    }
    if active.len() <= 1 {
        result.insert(
            ("selection".into(), view.world.world_record()?.id.clone()),
            active,
        );
    }
    Ok(result)
}
fn scene_wire(view: &View<'_>, ids: &[String], scope: &str) -> Json {
    json!({"scenes":ids.iter().map(|id| version(view,id)).collect::<Vec<_>>(),
        "citations":ids.iter().map(|id| citation(view.world,&view.world.records[id],if scope=="selection" {"frontmatter.status"} else {"frontmatter.participants"},None,None)).collect::<Vec<_>>()})
}
fn compare_scenes(
    views: &[View<'_>; 3],
    at: &StoryTime,
    rows: &mut ReportRows<'_>,
    work: &mut Work,
) -> Result<()> {
    let scenes = [
        scene_sets(&views[0], at, work)?,
        scene_sets(&views[1], at, work)?,
        scene_sets(&views[2], at, work)?,
    ];
    let keys: BTreeSet<_> = scenes.iter().flat_map(|values| values.keys()).collect();
    for key in keys {
        work.step(1)?;
        if key.0 == "selection" && scenes.iter().any(|values| !values.contains_key(key)) {
            continue;
        }
        let values = scenes
            .each_ref()
            .map(|values| values.get(key).map(Vec::as_slice).unwrap_or(&[]));
        if divergent(&values[0], &values[1], &values[2]) {
            rows.add(json!({"kind":"active-scene","scope":key.0,"ownerId":key.1,
                "base":scene_wire(&views[0],values[0],&key.0),"left":scene_wire(&views[1],values[1],&key.0),"right":scene_wire(&views[2],values[2],&key.0)}), true)?;
        }
    }
    Ok(())
}

fn digest(domain: &[u8], value: &impl Serialize) -> Result<String> {
    let mut digest = Sha256::new();
    digest.update(domain);
    digest.update(serde_json::to_vec(value)?);
    Ok(hex::encode(digest.finalize()))
}

/// Compare original canonical story sources, with literal commit identity and
/// ancestry checked before reading any domain data. No write-capable adapter,
/// mutable worktree loader, cache, candidate or merge engine is involved.
pub fn review(root: &Path, request: &Request) -> Result<Json> {
    let at = request_time(request)?;
    if !historical::is_ancestor(root, &request.base_revision, &request.left_revision)?
        || !historical::is_ancestor(root, &request.base_revision, &request.right_revision)?
    {
        return Err(invalid(
            "baseRevision must be an original ancestor of both branch revisions",
        ));
    }
    let worlds = [
        historical::load_world(root, &request.base_revision)?,
        historical::load_world(root, &request.left_revision)?,
        historical::load_world(root, &request.right_revision)?,
    ];
    let mut work = source_bounds(worlds.each_ref())?;
    let world_id = &worlds[0].world_record()?.id;
    for world in &worlds {
        if &world.world_record()?.id != world_id {
            return Err(invalid(
                "revision review requires the same exact world ID in every original",
            ));
        }
        projection::horizon(world, &at)?;
        let report = validation::validate_world(world);
        if !report.valid {
            return Err(Error::Validation(format!(
                "revision review original source is invalid: {}",
                report
                    .diagnostics
                    .iter()
                    .find(|d| d.severity == "error")
                    .map(|d| d.message.as_str())
                    .unwrap_or("source validation failed")
            )));
        }
    }
    let views = [
        build_view(&worlds[0], &at, &mut work)?,
        build_view(&worlds[1], &at, &mut work)?,
        build_view(&worlds[2], &at, &mut work)?,
    ];
    let mut rows = ReportRows::new(request);
    compare_sources(&views, &mut rows, &mut work)?;
    compare_state(&views, &mut rows, &mut work)?;
    compare_scenes(&views, &at, &mut rows, &mut work)?;
    rows.conflicts
        .sort_by_cached_key(|value| serde_json::to_string(value).unwrap());
    let mut result = json!({"protocol":crate::CONTRACT,"operation":"revision-review","outcome":"ok",
        "scope":"author-as-of","worldId":world_id,"at":request.at,
        "baseRevision":request.base_revision,"leftRevision":request.left_revision,"rightRevision":request.right_revision,
        "sourceFingerprints":{"base":worlds[0].fingerprint,"left":worlds[1].fingerprint,"right":worlds[2].fingerprint},
        "requestSha256":digest(b"wedl-rs/revision-review-request-v1\0",request)?,
        "changes":rows.changes,"conflicts":rows.conflicts,
        "writesPerformed":false,"canApply":false,"complete":true,"validationComplete":false,
        "bounds":{"maxItems":request.max_items,"maxCharacters":request.max_characters,
            "maxSourceRecords":MAX_RECORDS,"maxSourceBytes":MAX_SOURCE_BYTES,"maxSourceNodes":MAX_NODES,
            "maxSourcePathBytes":MAX_SOURCE_PATH_BYTES,"maxSourcePathLength":4096,
            "maxSourcePositionsPerRevision":MAX_POSITIONS,"maxSourcePositions":MAX_POSITIONS*3,
            "maxWork":MAX_WORK,"maxPositions":MAX_POSITIONS,"maxCitations":MAX_CITATIONS,"maxReportBytes":MAX_REPORT_BYTES,
            "changedPositions":rows.positions,"citations":rows.citations,"work":work},
        "semantics":{"source":"independently-admitted-originals","state":"native-typed-values","body":"literal-markdown",
            "stateScope":"complete-original-author-worlds","stateReferences":"native-values-with-scalar-entity-effect-pruning",
            "futureEntityTargetPruning":true,
            "scopeAudiences":["public","author"],"scopePerspectives":["ordinary","author"],
            "retconImpact":false,"mergeCandidate":false}});
    measured(&result, request.max_characters)?;
    result["reportSha256"] = json!(digest(b"wedl-rs/revision-review-report-v1\0", &result)?);
    measured(&result, request.max_characters)?;
    Ok(result)
}
