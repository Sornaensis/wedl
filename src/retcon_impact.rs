//! Bounded reverse dependencies between exact, admitted native source occurrences.
//!
//! The source bodies remain opaque. Only reference positions declared by native
//! codecs produce edges; historical citations, prose and ID-looking strings do
//! not. Each original is admitted independently before graph construction. This
//! is an informational author operation, never an approval or publication gate.
use crate::{
    Error, Result,
    consequence_semantics::{Policy, Time, projection},
    consequences::{self, Plan},
    manuscript,
    model::{Record, StoryTime, World},
    revision_review::{self, Node},
    validation,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io::Write,
};

pub const MAX_REPORT_BYTES: usize = 262_144;
pub(crate) const WITNESS_VERSION: &str = "wedl-retcon-witness/v1";
pub(crate) const MAX_WITNESS_BYTES: usize = 1_048_576;
pub(crate) const MAX_WITNESS_POSITIONS: usize = 4096;
const MAX_GRAPH_NODES: usize = 4096;
const MAX_GRAPH_EDGES: usize = 16_384;
const MAX_GRAPH_WORK: usize = 1_000_000;
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
const WORLD_DECLARATIONS: &[&str] = &[
    "profiles",
    "predicates",
    "state_profile_bindings",
    "state_keys",
    "relationship_metrics",
    "chronology",
    "timelines",
];

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}
fn unavailable(message: impl Into<String>) -> Error {
    Error::NotFound(message.into())
}

/// Exact source-style typed references. Member IDs are scoped by their owner.
/// Manuscript/work identities remain separate from canonical records.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Reference {
    #[serde(rename = "record")]
    Record { id: String },
    #[serde(rename = "section")]
    Section { owner_id: String, id: String },
    #[serde(rename = "transition")]
    Transition { owner_id: String, id: String },
    #[serde(rename = "knowledge")]
    Knowledge {
        entity_id: String,
        transition_id: String,
    },
    #[serde(rename = "observation")]
    Observation {
        entity_id: String,
        observation_id: String,
    },
    #[serde(rename = "turn")]
    Turn { entity_id: String, turn_id: String },
    #[serde(rename = "recollection")]
    Recollection {
        entity_id: String,
        recollection_id: String,
    },
    #[serde(rename = "author-record")]
    AuthorRecord { id: String },
}
impl Reference {
    pub(crate) fn owner(&self) -> &str {
        match self {
            Self::Record { id } | Self::AuthorRecord { id } => id,
            Self::Section { owner_id, .. } | Self::Transition { owner_id, .. } => owner_id,
            Self::Knowledge { entity_id, .. }
            | Self::Observation { entity_id, .. }
            | Self::Turn { entity_id, .. }
            | Self::Recollection { entity_id, .. } => entity_id,
        }
    }
    fn member(&self) -> Option<&str> {
        match self {
            Self::Record { .. } | Self::AuthorRecord { .. } => None,
            Self::Section { id, .. } | Self::Transition { id, .. } => Some(id),
            Self::Knowledge { transition_id, .. } => Some(transition_id),
            Self::Observation { observation_id, .. } => Some(observation_id),
            Self::Turn { turn_id, .. } => Some(turn_id),
            Self::Recollection {
                recollection_id, ..
            } => Some(recollection_id),
        }
    }
    fn validate(&self) -> Result<()> {
        for id in std::iter::once(self.owner()).chain(self.member()) {
            if id.trim().is_empty() || id.len() > 256 {
                return Err(invalid(
                    "retcon references require nonblank exact IDs of at most 256 bytes",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub target: Reference,
    pub at: Time,
    pub depth: usize,
    pub max_items: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewRequest {
    pub plan: Plan,
    pub impact_request: Request,
}

pub fn validate_request(request: &Request) -> Result<StoryTime> {
    request.target.validate()?;
    if !(1..=32).contains(&request.depth) || !(1..=1000).contains(&request.max_items) {
        return Err(invalid(
            "retcon impact requires depth 1..32 and maxItems 1..1000",
        ));
    }
    request.at.point()
}
pub fn decode(raw: &str) -> Result<Request> {
    if raw.len() > 65_536 {
        return Err(invalid("retcon impact request exceeds 64 KiB"));
    }
    duplicate_free(raw)?;
    let value = serde_json::from_str(raw)
        .map_err(|e| invalid(format!("invalid retcon impact request: {e}")))?;
    validate_request(&value)?;
    Ok(value)
}
pub fn decode_preview(raw: &str) -> Result<PreviewRequest> {
    if raw.len() > 524_288 {
        return Err(invalid("retcon impact preview request exceeds 512 KiB"));
    }
    duplicate_free(raw)?;
    let value: PreviewRequest = serde_json::from_str(raw)
        .map_err(|e| invalid(format!("invalid retcon impact preview request: {e}")))?;
    validate_request(&value.impact_request)?;
    consequences::validate_plan(&value.plan)?;
    Ok(value)
}

fn duplicate_free(raw: &str) -> Result<()> {
    // The native YAML visitor rejects duplicate JSON map keys recursively,
    // including escaped spellings and internally tagged enum operands.
    let mut decoder = serde_json::Deserializer::from_str(raw);
    let _: Yaml = Deserialize::deserialize(&mut decoder)
        .map_err(|_| invalid("invalid or duplicate retcon impact JSON"))?;
    decoder
        .end()
        .map_err(|_| invalid("invalid retcon impact JSON"))
}

struct Size {
    size: usize,
    max: usize,
}
impl Size {
    fn charge(&mut self, value: &impl Serialize) -> Result<()> {
        let maximum = self.max;
        serde_json::to_writer(self, value).map_err(|_| {
            invalid(format!(
                "complete retcon result exceeds {maximum} UTF-8 bytes"
            ))
        })
    }
}
impl Write for Size {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.size = self.size.saturating_add(bytes.len());
        if self.size > self.max {
            return Err(std::io::Error::other(
                "complete retcon result exceeds byte bound",
            ));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn bounded(value: &impl Serialize, max: usize) -> Result<()> {
    serde_json::to_writer(&mut Size { size: 0, max }, value)
        .map_err(|_| invalid(format!("complete retcon result exceeds {max} UTF-8 bytes")))
}
fn value_raw(value: &Yaml) -> Result<String> {
    // JSON object keys canonicalize independently of source mapping layout;
    // integral and floating point numbers retain distinct serialized tokens.
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            Yaml::Number(number) if !number.as_f64().is_some_and(f64::is_finite) => {
                return Err(Error::Unsupported(
                    "retcon witness does not canonicalize non-finite YAML numbers as JSON null"
                        .into(),
                ));
            }
            Yaml::Mapping(values) => pending.extend(values.values()),
            Yaml::Sequence(values) => pending.extend(values),
            _ => {}
        }
    }
    let value = serde_json::to_value(value)?;
    bounded(&value, MAX_WITNESS_BYTES)?;
    Ok(serde_json::to_string(&value)?)
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Token {
    Key(String),
    Index(usize),
}
type Tokens = Vec<Token>;
fn path(tokens: &[Token]) -> String {
    let mut result = String::new();
    for token in tokens {
        match token {
            Token::Key(key) => {
                if !result.is_empty() {
                    result.push('.');
                }
                result.push_str(key);
            }
            Token::Index(ordinal) => result.push_str(&format!("[{ordinal}]")),
        }
    }
    result
}
fn node_at<'a>(node: &'a Node, fields: &[&str]) -> Option<&'a Node> {
    let mut value = node;
    for field in fields {
        let Node::Map(map) = value else {
            return None;
        };
        value = map.get(*field)?;
    }
    Some(value)
}
fn text(node: &Node, field: &str) -> Option<String> {
    match node_at(node, &[field])? {
        Node::Scalar(Yaml::String(value)) => Some(value.clone()),
        _ => None,
    }
}
fn node_time(world: &World, node: &Node) -> Option<StoryTime> {
    for fields in [
        &["at"][..],
        &["time"][..],
        &["time", "start"][..],
        &["applicability", "point"][..],
        &["applicability", "first"][..],
    ] {
        if let Some(raw) = node_at(node, fields)
            && let Ok(point) = StoryTime::from_yaml(&raw.value(), &world.default_timeline())
        {
            return Some(point);
        }
    }
    None
}

#[derive(Clone)]
struct Position {
    tokens: Tokens,
    normalized: String,
    ordinal: Option<usize>,
    normalized_ordinal: Option<usize>,
    member_id: Option<String>,
    time: Option<StoryTime>,
}
fn index_node(
    world: &World,
    node: &Node,
    raw: &mut Tokens,
    normalized: &mut Tokens,
    context: &Position,
    positions: &mut BTreeMap<String, Position>,
    work: &mut usize,
) -> Result<()> {
    *work = work.saturating_add(1);
    if *work > MAX_GRAPH_WORK {
        return Err(invalid("retcon impact exceeds 1000000 graph work units"));
    }
    let field = path(raw);
    if field.len() > 4096 {
        return Err(invalid("retcon source position exceeds 4096 bytes"));
    }
    let mut position = context.clone();
    position.tokens = raw.clone();
    position.normalized = path(normalized);
    if positions.insert(field, position.clone()).is_some() {
        return Err(Error::Unsupported(
            "retcon occurrence positions are ambiguous under dotted source keys".into(),
        ));
    }
    match node {
        Node::Map(values) => {
            for (key, value) in values {
                let mut context = position.clone();
                if raw.is_empty() && key == "initialization" {
                    context.member_id = text(value, "transition_id");
                    context.time = node_time(world, value).or(context.time);
                }
                raw.push(Token::Key(key.clone()));
                normalized.push(Token::Key(key.clone()));
                index_node(world, value, raw, normalized, &context, positions, work)?;
                raw.pop();
                normalized.pop();
            }
        }
        Node::List(values) => {
            let collection = path(raw);
            let authored_member = matches!(
                collection.as_str(),
                "effects"
                    | "transitions"
                    | "lifecycle.transitions"
                    | "descriptions"
                    | "spatial_history"
                    | "turns"
                    | "observations"
                    | "recollections"
                    | "chronology.calendars"
                    | "chronology.eras"
                    | "chronology.anchors"
            );
            for (selected, (original, value)) in values.iter().enumerate() {
                let mut context = position.clone();
                if authored_member || context.ordinal.is_none() {
                    context.ordinal = Some(*original);
                    context.normalized_ordinal = Some(selected);
                }
                if authored_member {
                    context.member_id = text(value, "transition_id").or_else(|| text(value, "id"));
                    context.time = node_time(world, value).or(context.time);
                } else if collection == "participants"
                    && let Some(start) = node_at(value, &["from"])
                    && let Ok(point) =
                        StoryTime::from_yaml(&start.value(), &world.default_timeline())
                {
                    context.time = Some(point);
                }
                raw.push(Token::Index(*original));
                normalized.push(Token::Index(selected));
                index_node(world, value, raw, normalized, &context, positions, work)?;
                raw.pop();
                normalized.pop();
            }
        }
        Node::Scalar(_) => {}
    }
    Ok(())
}
fn remove_node(node: &mut Node, tokens: &[Token]) -> Result<()> {
    let Some((first, rest)) = tokens.split_first() else {
        return Err(invalid("cannot remove whole retcon owner"));
    };
    match (node, first) {
        (Node::Map(values), Token::Key(key)) => {
            if rest.is_empty() {
                values.remove(key);
            } else if let Some(value) = values.get_mut(key) {
                remove_node(value, rest)?;
            }
        }
        (Node::List(values), Token::Index(index)) => {
            if rest.is_empty() {
                values.retain(|(ordinal, _)| ordinal != index);
            } else if let Some((_, value)) = values.iter_mut().find(|(ordinal, _)| ordinal == index)
            {
                remove_node(value, rest)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[derive(Clone)]
struct Edge {
    owner: Reference,
    target: Reference,
    relation: String,
    field: String,
    position: Position,
}
struct Graph<'a> {
    world: &'a World,
    authors: Option<&'a manuscript::Snapshot>,
    records: BTreeMap<String, Node>,
    nodes: BTreeSet<Reference>,
    members: BTreeMap<String, BTreeSet<Reference>>,
    edges: Vec<Edge>,
    reverse: BTreeMap<Reference, Vec<usize>>,
}

fn record_ref(id: &str) -> Reference {
    Reference::Record { id: id.into() }
}
fn member_refs(record: &Record, node: &Node) -> Vec<Reference> {
    let mut result = Vec::new();
    for (fields, class) in [
        (&["descriptions"][..], "section"),
        (&["spatial_history"][..], "section"),
        (&["transitions"][..], "transition"),
        (&["lifecycle", "transitions"][..], "transition"),
        (&["observations"][..], "observation"),
        (&["turns"][..], "turn"),
        (&["recollections"][..], "recollection"),
        (&["chronology", "calendars"][..], "section"),
        (&["chronology", "eras"][..], "section"),
        (&["chronology", "anchors"][..], "section"),
    ] {
        if fields[0] == "chronology" && record.kind != "world" {
            continue;
        }
        if class == "transition"
            && !GENERATIONAL.contains(&record.kind.as_str())
            && !["knowledge", "relationship", "story-point"].contains(&record.kind.as_str())
        {
            continue;
        }
        if class == "observation" && record.kind != "scene"
            || ["turn", "recollection"].contains(&class) && record.kind != "conversation"
        {
            continue;
        }
        let Some(Node::List(values)) = node_at(node, fields) else {
            continue;
        };
        for (_, value) in values {
            if class == "turn" && text(value, "kind").is_some_and(|kind| kind != "speech") {
                continue;
            }
            let key = if class == "transition" && GENERATIONAL.contains(&record.kind.as_str()) {
                "transition_id"
            } else {
                "id"
            };
            let Some(id) = text(value, key) else {
                continue;
            };
            let reference = match class {
                "section" => Reference::Section {
                    owner_id: record.id.clone(),
                    id,
                },
                "transition" => Reference::Transition {
                    owner_id: record.id.clone(),
                    id: id.clone(),
                },
                "observation" => Reference::Observation {
                    entity_id: record.id.clone(),
                    observation_id: id,
                },
                "turn" => Reference::Turn {
                    entity_id: record.id.clone(),
                    turn_id: id,
                },
                "recollection" => Reference::Recollection {
                    entity_id: record.id.clone(),
                    recollection_id: id,
                },
                _ => unreachable!(),
            };
            if record.kind == "knowledge" && class == "transition" {
                result.push(Reference::Knowledge {
                    entity_id: record.id.clone(),
                    transition_id: reference.member().unwrap().into(),
                });
            }
            result.push(reference);
        }
    }
    if GENERATIONAL.contains(&record.kind.as_str())
        && let Some(initial) = node_at(node, &["initialization"])
        && let Some(id) = text(initial, "transition_id")
    {
        result.push(Reference::Transition {
            owner_id: record.id.clone(),
            id,
        });
    }
    result
}

#[derive(Clone)]
struct Occurrence {
    field: String,
    target: Reference,
    relation: String,
    prune: String,
}
fn relation(record: &Record, field: &str) -> &'static str {
    if field == "causes"
        || field.starts_with("causes[")
        || field.ends_with(".causing_event")
        || field.ends_with(".cause_event_id")
    {
        "cause"
    } else if field.ends_with(".source_entity") {
        "acquisition"
    } else if field.starts_with("initial_state.")
        || field.starts_with("effects[") && field.contains(".value")
        || record.kind == "lore-concept" && field.starts_with("values.")
        || record.kind == "assertion" && field.starts_with("value.")
    {
        "typed-entity"
    } else if record.kind == "event" && field.starts_with("effects[") {
        "effect"
    } else if field.starts_with("participants[") {
        "participant"
    } else if field.starts_with("outcome_events[") || field.starts_with("related_story_points[") {
        "outcome"
    } else if record.kind == "location" && field.starts_with("links[") {
        "location-link"
    } else if field.starts_with("spatial_history[") {
        "spatial-history"
    } else if field.starts_with("spatial.")
        || matches!(record.kind.as_str(), "anchor" | "portal" | "overlay")
    {
        "spatial"
    } else if field == "claim.assertion_id" || record.kind == "assertion" && field == "replaces" {
        "assertion"
    } else if field.starts_with("claim.genealogy.payload.") {
        "genealogy-reference"
    } else if GENERATIONAL.contains(&record.kind.as_str()) && field.contains(".payload.") {
        "tenure"
    } else if record.kind == "assertion" {
        "predicate"
    } else if field.starts_with("concept_links[") {
        "concept"
    } else if field == "when" || field.starts_with("when.") || field.starts_with("requires[") {
        "predicate"
    } else if record.kind == "knowledge" && field.starts_with("claim.")
        || record.kind == "conversation"
            && (field.starts_with("turns[") || field.starts_with("recollections["))
    {
        "authored-reference"
    } else {
        "reference"
    }
}
fn exact_reference(node: &Node) -> Result<Reference> {
    serde_json::from_value(serde_json::to_value(node.value())?)
        .map_err(|e| invalid(format!("invalid declared retcon reference: {e}")))
}
fn occurrences(world: &World, record: &Record, node: &Node) -> Result<Vec<Occurrence>> {
    let mut result = Vec::new();
    for (field, id) in revision_review::references(world, record)? {
        if record.kind == "context-brief"
            && (field.starts_with("required_refs[") || field == "scope_ref")
            || record.kind == "knowledge" && field.starts_with("claim.genealogy.evidence[")
        {
            continue;
        }
        result.push(Occurrence {
            prune: field.clone(),
            relation: relation(record, &field).into(),
            field,
            target: record_ref(&id),
        });
    }
    if record.kind == "context-brief" {
        if let Some(Node::List(values)) = node_at(node, &["required_refs"]) {
            for (ordinal, value) in values {
                let field = format!("required_refs[{ordinal}]");
                result.push(Occurrence {
                    target: exact_reference(value)?,
                    relation: "pin".into(),
                    prune: field.clone(),
                    field,
                });
            }
        }
        if let Some(value) = node_at(node, &["scope_ref"]) {
            result.push(Occurrence {
                target: exact_reference(value)?,
                relation: "brief-scope".into(),
                field: "scope_ref".into(),
                prune: "scope_ref".into(),
            });
        }
    }
    if record.kind == "knowledge"
        && let Some(Node::List(values)) = node_at(node, &["claim", "genealogy", "evidence"])
    {
        for (ordinal, value) in values {
            let field = format!("claim.genealogy.evidence[{ordinal}]");
            result.push(Occurrence {
                target: exact_reference(value)?,
                relation: "genealogy-evidence".into(),
                prune: field.clone(),
                field,
            });
        }
    }
    if record.kind == "conversation" {
        if let Some(Node::List(values)) = node_at(node, &["turns"]) {
            for (ordinal, value) in values {
                if let Some(Node::Scalar(Yaml::String(id))) = node_at(value, &["interrupts"]) {
                    let field = format!("turns[{ordinal}].interrupts");
                    result.push(Occurrence {
                        target: Reference::Turn {
                            entity_id: record.id.clone(),
                            turn_id: id.clone(),
                        },
                        relation: "source-member".into(),
                        prune: field.clone(),
                        field,
                    });
                }
                if let Some(addressee) = node_at(value, &["addressee"]) {
                    let field = format!("turns[{ordinal}].addressee");
                    match addressee {
                        Node::Scalar(Yaml::String(id)) if id != "participants" => {
                            result.push(Occurrence {
                                target: record_ref(id),
                                relation: "authored-reference".into(),
                                prune: field.clone(),
                                field,
                            })
                        }
                        Node::List(values) => {
                            for (i, item) in values {
                                if let Node::Scalar(Yaml::String(id)) = item {
                                    let field = format!("{field}[{i}]");
                                    result.push(Occurrence {
                                        target: record_ref(id),
                                        relation: "authored-reference".into(),
                                        prune: field.clone(),
                                        field,
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        if let Some(Node::List(values)) = node_at(node, &["recollections"]) {
            for (ordinal, value) in values {
                if let Some(Node::List(turns)) = node_at(value, &["exact_turns"]) {
                    for (i, item) in turns {
                        if let Node::Scalar(Yaml::String(id)) = item {
                            let field = format!("recollections[{ordinal}].exact_turns[{i}]");
                            result.push(Occurrence {
                                target: Reference::Turn {
                                    entity_id: record.id.clone(),
                                    turn_id: id.clone(),
                                },
                                relation: "source-member".into(),
                                prune: field.clone(),
                                field,
                            });
                        }
                    }
                }
            }
        }
    }
    for fields in [
        &["descriptions"][..],
        &["spatial_history"][..],
        &["transitions"][..],
    ] {
        let generational =
            fields[0] == "transitions" && GENERATIONAL.contains(&record.kind.as_str());
        if fields[0] == "transitions" && !generational {
            continue;
        }
        if let Some(Node::List(values)) = node_at(node, fields) {
            for (ordinal, value) in values {
                let key = if generational {
                    "replaces_transition_id"
                } else {
                    "replaces"
                };
                if let Some(id) = text(value, key) {
                    let target = if generational {
                        Reference::Transition {
                            owner_id: record.id.clone(),
                            id,
                        }
                    } else {
                        Reference::Section {
                            owner_id: record.id.clone(),
                            id,
                        }
                    };
                    let field = format!("{}[{ordinal}].{key}", fields[0]);
                    result.push(Occurrence {
                        target,
                        relation: "correction".into(),
                        prune: field.clone(),
                        field,
                    });
                }
            }
        }
    }
    Ok(result)
}

impl<'a> Graph<'a> {
    fn build(
        world: &'a World,
        authors: Option<&'a manuscript::Snapshot>,
        at: &StoryTime,
    ) -> Result<Self> {
        preflight(world)?;
        if let Some(authors) = authors {
            manuscript::verify_snapshot(authors)?;
        }
        if !validation::validate_world(world).valid {
            return Err(Error::Validation(
                "retcon impact requires a valid original source snapshot".into(),
            ));
        }
        let records = revision_review::source_view(world, at, &["hypothesis"])?;
        let mut graph = Self {
            world,
            authors,
            records,
            nodes: BTreeSet::new(),
            members: BTreeMap::new(),
            edges: Vec::new(),
            reverse: BTreeMap::new(),
        };
        graph.register()?;
        let mut work = 0usize;
        for id in graph.records.keys().cloned().collect::<Vec<_>>() {
            let record = &world.records[&id];
            let node = &graph.records[&id];
            let occurrences = occurrences(world, record, node)?;
            let mut positions = BTreeMap::new();
            let context = Position {
                tokens: Vec::new(),
                normalized: String::new(),
                ordinal: None,
                normalized_ordinal: None,
                member_id: None,
                time: node_time(world, node),
            };
            index_node(
                world,
                node,
                &mut Vec::new(),
                &mut Vec::new(),
                &context,
                &mut positions,
                &mut work,
            )?;
            // Exact live-member links have a stronger admission condition than
            // the core owner's record reference. Remove unavailable operands.
            let denied: Vec<_> = occurrences
                .iter()
                .filter(|item| !graph.nodes.contains(&item.target))
                .filter_map(|item| {
                    positions
                        .get(&item.prune)
                        .map(|position| position.tokens.clone())
                })
                .collect();
            if !denied.is_empty() {
                let node = graph.records.get_mut(&id).unwrap();
                for tokens in denied {
                    remove_node(node, &tokens)?;
                }
                positions.clear();
                index_node(
                    world,
                    node,
                    &mut Vec::new(),
                    &mut Vec::new(),
                    &context,
                    &mut positions,
                    &mut work,
                )?;
            }
            let mut seen = BTreeSet::new();
            for item in occurrences {
                work = work.saturating_add(1);
                if work > MAX_GRAPH_WORK {
                    return Err(invalid("retcon impact exceeds 1000000 graph work units"));
                }
                let Some(position) = positions.get(&item.field) else {
                    continue;
                };
                if !graph.nodes.contains(&item.target)
                    || !seen.insert((item.field.clone(), item.target.clone()))
                {
                    continue;
                }
                if graph.edges.len() >= MAX_GRAPH_EDGES {
                    return Err(invalid(
                        "retcon graph exceeds 16384 complete typed occurrences",
                    ));
                }
                let edge = Edge {
                    owner: record_ref(&id),
                    target: item.target,
                    relation: item.relation,
                    field: item.field,
                    position: position.clone(),
                };
                graph
                    .reverse
                    .entry(edge.target.clone())
                    .or_default()
                    .push(graph.edges.len());
                graph.edges.push(edge);
            }
        }
        graph.private(at, &mut work)?;
        Ok(graph)
    }
    fn record(&self, reference: &Reference) -> Option<&Record> {
        if matches!(reference, Reference::AuthorRecord { .. }) {
            self.authors?.documents.get(reference.owner())
        } else {
            self.world.records.get(reference.owner())
        }
    }
    fn private(&mut self, at: &StoryTime, work: &mut usize) -> Result<()> {
        let Some(authors) = self.authors else {
            return Ok(());
        };
        // The source image is structurally verified before iteration. Original
        // link validation belongs to the base capture, never its candidate.
        for document in authors.documents.values() {
            if !matches!(document.kind.as_str(), "manuscript-segment" | "work") {
                continue;
            }
            if let Some(placement) = document.frontmatter.get("placement")
                && !StoryTime::from_yaml(placement, &self.world.default_timeline())?.not_after(at)
            {
                continue;
            }
            self.nodes.insert(Reference::AuthorRecord {
                id: document.id.clone(),
            });
            if self.nodes.len() > MAX_GRAPH_NODES {
                return Err(invalid(
                    "retcon graph exceeds 4096 admitted record/member nodes",
                ));
            }
        }
        for document in authors.documents.values() {
            let owner = Reference::AuthorRecord {
                id: document.id.clone(),
            };
            if !self.nodes.contains(&owner) {
                continue;
            }
            let mut add = |target: Reference,
                           relation: &str,
                           field: String,
                           ordinal: Option<usize>,
                           member_id: Option<String>|
             -> Result<()> {
                *work = work.saturating_add(1);
                if *work > MAX_GRAPH_WORK {
                    return Err(invalid("retcon impact exceeds 1000000 graph work units"));
                }
                if !self.nodes.contains(&target) {
                    return Ok(());
                }
                if self.edges.len() >= MAX_GRAPH_EDGES {
                    return Err(invalid(
                        "retcon graph exceeds 16384 complete typed occurrences",
                    ));
                }
                let edge = Edge {
                    owner: owner.clone(),
                    target,
                    relation: relation.into(),
                    position: Position {
                        tokens: Vec::new(),
                        normalized: field.clone(),
                        ordinal,
                        normalized_ordinal: ordinal,
                        member_id,
                        time: None,
                    },
                    field,
                };
                self.reverse
                    .entry(edge.target.clone())
                    .or_default()
                    .push(self.edges.len());
                self.edges.push(edge);
                Ok(())
            };
            if document.kind == "manuscript-segment" {
                for (ordinal, value) in document
                    .list("source_links")
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    add(
                        serde_yaml::from_value(value.clone())
                            .map_err(|_| invalid("invalid manuscript source reference"))?,
                        "manuscript-source",
                        format!("source_links[{ordinal}]"),
                        Some(ordinal),
                        None,
                    )?;
                }
                if let Some(id) = document.string("pov_character_id") {
                    add(
                        record_ref(id),
                        "manuscript-pov",
                        "pov_character_id".into(),
                        None,
                        None,
                    )?;
                }
            } else if document.kind == "work" {
                for (chapter_index, chapter) in
                    document.list("chapters").into_iter().flatten().enumerate()
                {
                    let member = chapter
                        .get("id")
                        .and_then(Yaml::as_str)
                        .ok_or_else(|| invalid("work chapter requires its exact ID"))?;
                    for (ordinal, value) in chapter
                        .get("segments")
                        .and_then(Yaml::as_sequence)
                        .into_iter()
                        .flatten()
                        .enumerate()
                    {
                        add(
                            serde_yaml::from_value(value.clone())
                                .map_err(|_| invalid("invalid work segment reference"))?,
                            "work-segment",
                            format!("chapters[{chapter_index}].segments[{ordinal}]"),
                            Some(ordinal),
                            Some(member.into()),
                        )?;
                    }
                }
            }
        }
        Ok(())
    }
    fn register(&mut self) -> Result<()> {
        for (id, node) in &self.records {
            self.nodes.insert(record_ref(id));
            let members = member_refs(&self.world.records[id], node);
            let target = self.members.entry(id.clone()).or_default();
            for reference in members {
                if !self.nodes.insert(reference.clone()) {
                    return Err(unavailable(
                        "retcon source has ambiguous owner-local member identity",
                    ));
                }
                target.insert(reference);
            }
            if self.nodes.len() > MAX_GRAPH_NODES {
                return Err(invalid(
                    "retcon graph exceeds 4096 admitted record/member nodes",
                ));
            }
        }
        Ok(())
    }
    fn keys(&self, reference: &Reference) -> BTreeSet<Reference> {
        let mut result = BTreeSet::from([reference.clone()]);
        if let Reference::Record { id } = reference
            && let Some(members) = self.members.get(id)
        {
            result.extend(members.iter().cloned());
        }
        result
    }
    fn closure(
        &self,
        starts: &BTreeSet<Reference>,
        depth: usize,
        max_edges: usize,
    ) -> Result<(BTreeSet<Reference>, Vec<(usize, usize)>)> {
        let mut selected = BTreeSet::new();
        let mut pending = VecDeque::new();
        for start in starts {
            for key in self.keys(start) {
                if selected.insert(key.clone()) {
                    pending.push_back((key, 0usize));
                }
            }
        }
        let mut seen = BTreeSet::new();
        let mut found = Vec::new();
        while let Some((key, distance)) = pending.pop_front() {
            for index in self.reverse.get(&key).into_iter().flatten() {
                if seen.contains(index) {
                    continue;
                }
                if distance >= depth {
                    return Err(invalid(
                        "complete retcon dependency closure exceeds the requested depth",
                    ));
                }
                seen.insert(*index);
                if seen.len() > max_edges {
                    return Err(invalid(
                        "complete retcon dependency closure exceeds its item bound",
                    ));
                }
                found.push((*index, distance + 1));
                for next in self.keys(&self.edges[*index].owner) {
                    if selected.insert(next.clone()) {
                        pending.push_back((next, distance + 1));
                    }
                }
                if selected.len() > MAX_GRAPH_NODES {
                    return Err(invalid("retcon closure exceeds 4096 input nodes"));
                }
            }
        }
        found.sort_by(|(a, da), (b, db)| {
            let a = &self.edges[*a];
            let b = &self.edges[*b];
            (da, &a.owner, &a.target, &a.field, &a.relation).cmp(&(
                db,
                &b.owner,
                &b.target,
                &b.field,
                &b.relation,
            ))
        });
        Ok((selected, found))
    }
}

pub(crate) fn preflight(world: &World) -> Result<()> {
    projection::preflight(world)?;
    let mut paths = 0usize;
    for record in world.records.values() {
        paths = paths.saturating_add(record.source_path.len());
        if record.source_path.len() > 4096 || paths > 8 * 1024 * 1024 {
            return Err(invalid(
                "retcon source paths exceed 4096 bytes each or 8 MiB in aggregate",
            ));
        }
    }
    let mut declarations = 0usize;
    let mut pending = vec![&world.world_record()?.frontmatter];
    while let Some(value) = pending.pop() {
        declarations = declarations.saturating_add(1);
        match value {
            Yaml::Mapping(values) => pending.extend(values.values()),
            Yaml::Sequence(values) => pending.extend(values),
            _ => {}
        }
    }
    let records = world.records.len();
    if records.saturating_mul(records.saturating_add(declarations)) > 8_000_000 {
        return Err(invalid(
            "retcon source exceeds 8000000 estimated validation work units",
        ));
    }
    Ok(())
}

fn graphs<'a>(
    base: &'a World,
    candidate: &'a World,
    authors: Option<&'a manuscript::Snapshot>,
    request: &Request,
    plan_sha256: &str,
) -> Result<(Graph<'a>, Graph<'a>)> {
    let at = validate_request(request)?;
    if plan_sha256.len() != 64
        || !plan_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid(
            "retcon candidate identity requires a lowercase SHA-256 plan digest",
        ));
    }
    if base.world_record()?.id != candidate.world_record()?.id {
        return Err(invalid(
            "retcon originals must belong to the same native world",
        ));
    }
    if let Some(authors) = authors {
        preflight(base)?;
        manuscript::validate_manuscript_snapshot(base, authors)?;
    }
    let base = Graph::build(base, authors, &at)?;
    let candidate = Graph::build(candidate, authors, &at)?;
    if !base.nodes.contains(&request.target) && !candidate.nodes.contains(&request.target) {
        return Err(unavailable(
            "retcon target is not exactly available at the selected horizon in either original",
        ));
    }
    Ok((base, candidate))
}
fn report(
    base: &Graph<'_>,
    candidate: &Graph<'_>,
    request: &Request,
    plan_sha256: &str,
) -> Result<Json> {
    let starts = BTreeSet::from([request.target.clone()]);
    let mut items = Vec::new();
    for (name, graph) in [("base", base), ("candidate", candidate)] {
        let (_, edges) = graph.closure(
            &starts,
            request.depth,
            request.max_items.saturating_sub(items.len()),
        )?;
        for (index, distance) in edges {
            let edge = &graph.edges[index];
            let record = graph
                .record(&edge.owner)
                .ok_or_else(|| unavailable("retcon source owner is unavailable"))?;
            let provenance = if name == "base"
                || matches!(edge.owner, Reference::AuthorRecord { .. })
            {
                json!({"kind":"source","revision":base.world.revision,"blobOid":record.blob_oid})
            } else {
                json!({"kind":"candidate","baseRevision":base.world.revision,"planSha256":plan_sha256})
            };
            items.push(json!({"snapshot":name,"owner":edge.owner,"target":edge.target,"relation":edge.relation,"distance":distance,
                "citation":{"entityId":record.id,"sourcePath":record.source_path,"section":edge.field,
                    "sourceOrdinal":edge.position.ordinal,"memberId":edge.position.member_id,"time":edge.position.time,"provenance":provenance}}));
        }
    }
    let mut unsupported = vec!["prose mentions", "historical citation dependencies"];
    if base.authors.is_none() {
        unsupported.insert(0, "manuscript/work author records");
    }
    let result = json!({"protocol":crate::CONTRACT,"outcome":"ok","target":request.target,"at":request.at,
        "depth":request.depth,"maxItems":request.max_items,"base":{"revision":base.world.revision},
        "candidate":{"baseRevision":base.world.revision,"planSha256":plan_sha256},"items":items,
        "mentions":[],"semanticSuggestions":[],"canApply":false,"writesPerformed":false,
        "scope":{"audiences":["public","author"],"perspectives":["ordinary","author"],
            "unsupported":unsupported},
        "notice":"Typed source dependency impact is informational. It neither infers consequences nor approves canonical changes."});
    bounded(&result, MAX_REPORT_BYTES)?;
    Ok(result)
}

/// Pure bounded impact over two independently validated originals.
pub fn evaluate(
    base: &World,
    candidate: &World,
    request: &Request,
    plan_sha256: &str,
) -> Result<Json> {
    let (before, after) = graphs(base, candidate, None, request, plan_sha256)?;
    report(&before, &after, request, plan_sha256)
}

/// Pure bounded impact including one exact private source image. The supplied
/// author source is validated against the base and is unchanged in the
/// candidate. No live files, HEAD, or author files are read by this operation.
pub fn evaluate_with_authors(
    base: &World,
    candidate: &World,
    authors: &manuscript::Snapshot,
    request: &Request,
    plan_sha256: &str,
) -> Result<Json> {
    let (before, after) = graphs(base, candidate, Some(authors), request, plan_sha256)?;
    report(&before, &after, request, plan_sha256)
}

#[derive(Debug)]
pub(crate) struct Captured {
    pub report: Json,
    pub seeds: Vec<Reference>,
    pub path_hints: BTreeMap<Reference, String>,
    pub witness: Json,
}

// Retain the pure canonical-only primitive independently of private adapters.
#[allow(dead_code)]
pub(crate) fn capture(
    base: &World,
    candidate: &World,
    request: &Request,
    touched_ids: &[String],
    plan_sha256: &str,
) -> Result<Captured> {
    capture_inputs(base, candidate, None, request, touched_ids, plan_sha256)
}

pub(crate) fn capture_with_authors(
    base: &World,
    candidate: &World,
    authors: &manuscript::Snapshot,
    request: &Request,
    touched_ids: &[String],
    plan_sha256: &str,
) -> Result<Captured> {
    capture_inputs(
        base,
        candidate,
        Some(authors),
        request,
        touched_ids,
        plan_sha256,
    )
}

fn capture_inputs(
    base: &World,
    candidate: &World,
    authors: Option<&manuscript::Snapshot>,
    request: &Request,
    touched_ids: &[String],
    plan_sha256: &str,
) -> Result<Captured> {
    let (before, after) = graphs(base, candidate, authors, request, plan_sha256)?;
    let report = report(&before, &after, request, plan_sha256)?;
    let mut seeds = BTreeSet::from([request.target.clone()]);
    for id in touched_ids {
        let reference = record_ref(id);
        reference.validate()?;
        seeds.insert(reference);
    }
    let starts = seeds.clone();
    for graph in [&before, &after] {
        let (closure, _) = graph.closure(&starts, request.depth, MAX_WITNESS_POSITIONS)?;
        seeds.extend(closure);
    }
    let mut path_hints = BTreeMap::new();
    for reference in &seeds {
        if let Some(record) = before.record(reference).or_else(|| after.record(reference)) {
            path_hints.insert(reference.clone(), record.source_path.clone());
        }
    }
    let seeds: Vec<_> = seeds.into_iter().collect();
    let witness = witness_graph(&before, request, &seeds, &path_hints)?;
    Ok(Captured {
        report,
        seeds,
        path_hints,
        witness,
    })
}

fn sections(graph: &Graph<'_>, reference: &Reference, budget: &mut Size) -> Result<Vec<Json>> {
    if !graph.nodes.contains(reference) {
        return Ok(Vec::new());
    }
    let record = graph
        .record(reference)
        .ok_or_else(|| unavailable("retcon source input is unavailable"))?;
    if matches!(reference, Reference::AuthorRecord { .. }) {
        let mut result = Vec::new();
        for (field, value) in record
            .frontmatter
            .as_mapping()
            .ok_or_else(|| invalid("retcon manuscript source must be a mapping"))?
        {
            let field = field
                .as_str()
                .ok_or_else(|| invalid("retcon manuscript fields must be literal strings"))?;
            let cell = json!({"field":field,"ordinal":result.len(),"valueRaw":value_raw(value)?});
            budget.charge(&cell)?;
            result.push(cell);
        }
        bounded(&record.body, MAX_WITNESS_BYTES)?;
        let cell = json!({"field":"bodyMarkdown","ordinal":result.len(),"valueRaw":serde_json::to_string(&record.body)?});
        budget.charge(&cell)?;
        result.push(cell);
        return Ok(result);
    }
    let node = &graph.records[reference.owner()];
    let Node::Map(projected) = node else {
        return Err(invalid("retcon source must be a mapping"));
    };
    let mut result = Vec::new();
    for (field, _) in record.frontmatter.as_mapping().unwrap() {
        let field = field
            .as_str()
            .ok_or_else(|| invalid("retcon source fields must be literal strings"))?;
        if record.kind == "world" && WORLD_DECLARATIONS.contains(&field) {
            continue;
        }
        let Some(value) = projected.get(field) else {
            continue;
        };
        let cell =
            json!({"field":field,"ordinal":result.len(),"valueRaw":value_raw(&value.value())?});
        budget.charge(&cell)?;
        result.push(cell);
    }
    // A world chronology section is an explicitly requested native member,
    // never a date-heading interpretation of the Markdown body.
    if record.kind == "world"
        && let Reference::Section { id, .. } = reference
    {
        for field in ["calendars", "eras", "anchors"] {
            if let Some(Node::List(values)) = node_at(node, &["chronology", field]) {
                for (_, value) in values {
                    if text(value, "id").as_deref() == Some(id) {
                        let cell = json!({"field":format!("chronology.{field}"),"ordinal":result.len(),"valueRaw":value_raw(&value.value())?});
                        budget.charge(&cell)?;
                        result.push(cell);
                    }
                }
            }
        }
    }
    bounded(&record.body, MAX_WITNESS_BYTES)?;
    let cell = json!({"field":"bodyMarkdown","ordinal":result.len(),"valueRaw":serde_json::to_string(&record.body)?});
    budget.charge(&cell)?;
    result.push(cell);
    Ok(result)
}

fn consulted(graph: &Graph<'_>, selected: &BTreeSet<Reference>) -> Result<Vec<Json>> {
    let world = graph.world.world_record()?;
    let config = &world.frontmatter;
    let mut result = BTreeMap::<String, Yaml>::new();
    for field in ["schema", "capabilities", "default_timeline"] {
        if let Some(value) = config.get(field) {
            result.insert(field.into(), value.clone());
        }
    }
    let mut keys = BTreeSet::<(String, String)>::new();
    let mut profiles = BTreeSet::new();
    let mut predicates = BTreeSet::new();
    let owners: BTreeSet<_> = selected
        .iter()
        .filter(|reference| graph.nodes.contains(*reference))
        .filter(|reference| !matches!(reference, Reference::AuthorRecord { .. }))
        .map(|reference| reference.owner())
        .collect();
    for id in owners {
        let record = &graph.world.records[id];
        let node = &graph.records[id];
        if record.kind == "lore-concept"
            && let Some(profile) = text(node, "profile")
        {
            profiles.insert(profile);
        }
        if record.kind == "assertion"
            && let Some(predicate) = text(node, "predicate")
        {
            predicates.insert(predicate);
        }
        if let Some(Node::Map(initial)) = node_at(node, &["initial_state"]) {
            keys.extend(initial.keys().map(|key| (record.kind.clone(), key.clone())));
        }
        if record.kind == "event"
            && let Some(Node::List(effects)) = node_at(node, &["effects"])
        {
            for (_, effect) in effects {
                if let (Some(target), Some(key)) = (
                    text(effect, "target").and_then(|id| graph.world.records.get(&id)),
                    text(effect, "key"),
                ) {
                    keys.insert((target.kind.clone(), key));
                }
            }
        }
        if record.kind == "relationship"
            && let Some(metrics) = config.get("relationship_metrics")
        {
            let mut names = BTreeSet::new();
            if let Some(Node::List(transitions)) = node_at(node, &["transitions"]) {
                for (_, transition) in transitions {
                    if let Some(Node::Map(values)) = node_at(transition, &["metrics"]) {
                        names.extend(values.keys().cloned());
                    }
                }
            }
            for name in names {
                if let Some(value) = metrics.get(&name) {
                    result.insert(format!("relationship_metrics.{name}"), value.clone());
                }
            }
        }
    }
    for (kind, key) in keys {
        if let Some(value) = config
            .get("state_keys")
            .and_then(|v| v.get(&kind))
            .and_then(|v| v.get(&key))
        {
            result.insert(format!("state_keys.{kind}.{key}"), value.clone());
        }
        if let Some(value) = config
            .get("state_profile_bindings")
            .and_then(|v| v.get(&kind))
            .and_then(|v| v.get(&key))
        {
            result.insert(
                format!("state_profile_bindings.{kind}.{key}"),
                value.clone(),
            );
            if let Some(id) = value.as_str() {
                profiles.insert(id.into());
            }
        }
    }
    for (field, wanted) in [("profiles", profiles), ("predicates", predicates)] {
        for value in world.list(field).into_iter().flatten() {
            if let Some(id) = value.get("id").and_then(Yaml::as_str)
                && wanted.contains(id)
            {
                result.insert(format!("{field}.{id}"), value.clone());
            }
        }
    }
    result
        .into_iter()
        .map(|(field, value)| {
            Ok(json!({"path":world.source_path,"field":field,"valueRaw":value_raw(&value)?}))
        })
        .collect()
}

fn witness_graph(
    graph: &Graph<'_>,
    request: &Request,
    seeds: &[Reference],
    path_hints: &BTreeMap<Reference, String>,
) -> Result<Json> {
    if seeds.len() > MAX_GRAPH_NODES || path_hints.len() > MAX_GRAPH_NODES {
        return Err(invalid("retcon witness exceeds 4096 original inputs"));
    }
    for seed in seeds {
        seed.validate()?;
    }
    let starts = seeds.iter().cloned().collect();
    let (selected, found) = graph.closure(&starts, request.depth, MAX_WITNESS_POSITIONS)?;
    let mut result = json!({"version":WITNESS_VERSION,"worldId":graph.world.world_record()?.id,"target":request.target,"at":request.at,
        "depth":request.depth,"maxItems":request.max_items,"seeds":seeds,"inputs":[],"edges":[],"consulted":[]});
    let mut budget = Size {
        size: 0,
        max: MAX_WITNESS_BYTES,
    };
    budget.charge(&result)?;
    let mut count = selected.len().saturating_add(found.len());
    let mut inputs = Vec::new();
    for reference in &selected {
        let present = graph.nodes.contains(reference);
        let path = if present {
            graph
                .record(reference)
                .map(|record| record.source_path.clone())
        } else {
            path_hints.get(reference).cloned()
        };
        let mut input = json!({"ref":reference,"path":path,"present":present,"sections":[]});
        budget.charge(&input)?;
        let sections = sections(graph, reference, &mut budget)?;
        count = count.saturating_add(sections.len());
        if count > MAX_WITNESS_POSITIONS {
            return Err(invalid(
                "complete retcon witness exceeds 4096 source positions",
            ));
        }
        input["sections"] = sections.into();
        inputs.push(input);
    }
    let mut declarations = consulted(graph, &selected)?;
    for timeline in graph
        .world
        .world_record()?
        .list("timelines")
        .into_iter()
        .flatten()
    {
        if timeline.get("id").and_then(Yaml::as_str) == Some(&request.at.timeline) {
            declarations.push(json!({"path":graph.world.world_record()?.source_path,"field":format!("timelines.{}",request.at.timeline),"valueRaw":value_raw(timeline)?}));
        }
    }
    declarations.sort_by(|a, b| {
        (a["path"].as_str(), a["field"].as_str()).cmp(&(b["path"].as_str(), b["field"].as_str()))
    });
    if count.saturating_add(declarations.len()) > MAX_WITNESS_POSITIONS {
        return Err(invalid(
            "complete retcon witness exceeds 4096 source positions",
        ));
    }
    for declaration in &declarations {
        budget.charge(declaration)?;
    }
    let mut edges = Vec::new();
    for (index, _) in found {
        let edge = &graph.edges[index];
        let value = json!({"owner":edge.owner,"target":edge.target,"relation":edge.relation,"field":edge.position.normalized,
            "ordinal":edge.position.normalized_ordinal,"memberId":edge.position.member_id,"time":edge.position.time});
        budget.charge(&value)?;
        edges.push(value);
    }
    edges.sort_by_key(Json::to_string);
    result["inputs"] = inputs.into();
    result["edges"] = edges.into();
    result["consulted"] = declarations.into();
    bounded(&result, MAX_WITNESS_BYTES)?;
    Ok(result)
}

/// Rebuild from retained original seeds, following newly incoming dependencies.
/// Only admitted source values and declarations consulted by them are returned.
#[allow(dead_code)]
pub(crate) fn witness(
    current: &World,
    request: &Request,
    seeds: &[Reference],
    path_hints: &BTreeMap<Reference, String>,
) -> Result<Json> {
    let at = validate_request(request)?;
    witness_graph(
        &Graph::build(current, None, &at)?,
        request,
        seeds,
        path_hints,
    )
}

/// Rebuild a pure comparison witness from exact source images. The caller
/// validates newly authored links; unchanged links from a verified original
/// remain evidence when their former target is removed by a retcon.
pub(crate) fn witness_with_authors(
    current: &World,
    authors: &manuscript::Snapshot,
    request: &Request,
    seeds: &[Reference],
    path_hints: &BTreeMap<Reference, String>,
) -> Result<Json> {
    if authors.revision != current.revision {
        return Err(invalid(
            "retcon manuscript source must share the exact comparison revision",
        ));
    }
    let at = validate_request(request)?;
    witness_graph(
        &Graph::build(current, Some(authors), &at)?,
        request,
        seeds,
        path_hints,
    )
}

fn exact_current(world: &World) -> Result<()> {
    let fresh = crate::historical::load_current_world(&world.root)?;
    if fresh.fingerprint != world.fingerprint
        || fresh.records.len() != world.records.len()
        || world.records.iter().any(|(id, record)| {
            fresh.records.get(id).is_none_or(|original| {
                record.raw_bytes != original.raw_bytes
                    || record.source_path != original.source_path
                    || record.id != original.id
                    || record.kind != original.kind
                    || record.title != original.title
                    || record.body != original.body
                    || record.frontmatter != original.frontmatter
                    || record.blob_oid != original.blob_oid
            })
        })
    {
        return Err(Error::Conflict(
            "retcon source changed or differs from its exact capture; reload the source snapshot"
                .into(),
        ));
    }
    Ok(())
}

/// Construct one native candidate and return only its bounded impact report.
/// Current reads are fenced; an explicitly selected historical World stays at
/// its immutable revision. No source or private review bytes are written.
pub fn preview(world: &World, request: &PreviewRequest) -> Result<Json> {
    validate_request(&request.impact_request)?;
    preflight(world)?;
    let stamp = if world.revision.is_none() {
        Some(crate::publication::read_stamp(&world.root)?)
    } else {
        None
    };
    if world.revision.is_some() {
        crate::historical::verify_world(world)?;
    } else {
        exact_current(world)?;
    }
    let authors = manuscript::snapshot(&world.root, world.revision.as_deref())?;
    let prepared = consequences::prepare(world, &request.plan, 1_000_000)?;
    if prepared.report["validation"]["valid"] != true {
        return Err(Error::Validation(
            "retcon impact requires a valid complete candidate".into(),
        ));
    }
    if request
        .plan
        .analysis
        .as_ref()
        .is_some_and(|analysis| analysis.policy == Policy::Required)
        && prepared.report["semanticAnalysis"]["applyAllowed"] != true
    {
        return Err(Error::Conflict(
            "required consequence semantic checks do not permit retcon impact analysis".into(),
        ));
    }
    let digest = prepared.report["planSha256"]
        .as_str()
        .ok_or_else(|| invalid("consequence candidate lacks its plan digest"))?;
    let result = evaluate_with_authors(
        world,
        &prepared.candidate,
        &authors,
        &request.impact_request,
        digest,
    )?;
    if let Some(stamp) = stamp {
        exact_current(world)?;
        if manuscript::snapshot(&world.root, None)?.fingerprint != authors.fingerprint {
            return Err(Error::Conflict(
                "manuscript source changed during retcon impact preview; reload source".into(),
            ));
        }
        if crate::publication::read_stamp(&world.root)? != stamp {
            return Err(Error::Conflict(
                "publication changed during retcon impact preview; reload source".into(),
            ));
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
