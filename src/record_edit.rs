//! Typed draft construction only. Candidate validation and publication belong
//! to authoring; this module never reads or writes the filesystem.
use crate::{
    Error, Result,
    model::{Record, StoryTime, World},
    source::{self, SOURCE_SCHEMA},
    state_schema, validation,
};
use serde_yaml::{Mapping, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

mod yaml;
pub(crate) use yaml::preflight as preflight_yaml;

pub(crate) const MAX_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_YAML_NODES: usize = 131_072;
const COMMON: &[&str] = &[
    "kind",
    "title",
    "aliases",
    "tags",
    "body",
    "chronology",
    "concept_links",
    "descriptions",
    "status",
    "domain",
    "threads",
    "section_audiences",
    "audience",
    "perspectives",
];
pub(crate) const KNOWLEDGE_STATES: &[&str] = &[
    "accepted",
    "suspected",
    "rejected",
    "uncertain",
    "remembered",
    "forgotten",
];
pub(crate) const ACQUISITIONS: &[&str] = &[
    "authored",
    "experienced",
    "heard",
    "inferred",
    "observed",
    "read",
    "remembered",
    "tested",
    "told",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    World,
    Character,
    Location,
    Object,
    Event,
    Knowledge,
    Environment,
    Relationship,
    StoryPoint,
    Hypothesis,
    Scene,
    Conversation,
    Route,
    Map,
    Anchor,
    Portal,
    Overlay,
    LoreConcept,
    Assertion,
    ExistenceEpisode,
    ContextBrief,
    Parentage,
    Union,
    Organization,
    Affiliation,
    Legacy,
    Tenure,
    Claim,
    VitalHistory,
}

impl Kind {
    const ALL: &[Self] = &[
        Self::World,
        Self::Character,
        Self::Location,
        Self::Object,
        Self::Event,
        Self::Knowledge,
        Self::Environment,
        Self::Relationship,
        Self::StoryPoint,
        Self::Hypothesis,
        Self::Scene,
        Self::Conversation,
        Self::Route,
        Self::Map,
        Self::Anchor,
        Self::Portal,
        Self::Overlay,
        Self::LoreConcept,
        Self::Assertion,
        Self::ExistenceEpisode,
        Self::ContextBrief,
        Self::Parentage,
        Self::Union,
        Self::Organization,
        Self::Affiliation,
        Self::Legacy,
        Self::Tenure,
        Self::Claim,
        Self::VitalHistory,
    ];

    fn accepts(self, key: &str, creating: bool) -> bool {
        !(self == Self::Hypothesis
            && matches!(
                key,
                "chronology"
                    | "concept_links"
                    | "descriptions"
                    | "threads"
                    | "audience"
                    | "perspectives"
            ))
            && (COMMON.contains(&key) || self.fields().contains(&key) || (creating && key == "id"))
    }

    /// Required request fields beyond kind/id/title. These are construction
    /// requirements; whole-candidate domain validation may impose more rules.
    fn create_required(self) -> &'static [&'static str] {
        match self {
            Self::World | Self::Character | Self::Location | Self::Object => &[],
            Self::Event => &["time"],
            Self::Knowledge => &["knower", "claim"],
            Self::Environment => &["time", "targets"],
            Self::Relationship => &["from", "to"],
            Self::StoryPoint => &["activation_policy", "lifecycle"],
            Self::Hypothesis => &[
                "statement",
                "subjects",
                "alternatives",
                "context",
                "placement",
            ],
            Self::Scene | Self::Conversation => &["location", "time"],
            Self::Route => &["from_location_id", "to_location_id", "direction", "modes"],
            Self::Map => &["crs", "axis_order", "unit", "bounds"],
            Self::Anchor => &["from", "to"],
            Self::Portal => &["from_location_id", "to", "modes"],
            Self::Overlay => &["lifecycle", "membership", "audience", "perspectives"],
            Self::LoreConcept => &[
                "domain",
                "profile",
                "values",
                "valid",
                "audience",
                "perspectives",
            ],
            Self::Assertion => &[
                "domain",
                "subject",
                "predicate",
                "value",
                "classification",
                "valid",
                "evidence",
                "audience",
                "perspectives",
            ],
            Self::ExistenceEpisode => &[
                "domain",
                "character_id",
                "episode_kind",
                "valid",
                "start_evidence",
                "audience",
                "perspectives",
            ],
            Self::ContextBrief => &[
                "domain",
                "scope",
                "valid",
                "required_refs",
                "audience",
                "perspectives",
            ],
            Self::Parentage => &["initialization", "child_id", "parent_id"],
            Self::Union => &["initialization", "participant_ids"],
            Self::Organization => &["initialization", "organization_kind"],
            Self::Affiliation => &["initialization", "character_id", "organization_id"],
            Self::Legacy => &["initialization", "legacy_kind"],
            Self::Tenure => &["initialization", "legacy_id"],
            Self::Claim => &["initialization", "legacy_id", "claimant_id"],
            Self::VitalHistory => &["initialization", "character_id", "disclosure"],
        }
    }

    fn statuses(self) -> &'static [&'static str] {
        match self {
            Self::World => &["canonical"],
            Self::Event => &["canonical", "draft", "cancelled", "retconned"],
            Self::Hypothesis => &["open", "adopted", "rejected"],
            Self::Scene => &["planned", "active", "closed", "abandoned"],
            Self::Conversation => &["draft", "active", "closed", "retired"],
            Self::LoreConcept | Self::Assertion | Self::ExistenceEpisode | Self::ContextBrief => {
                &["canonical", "draft", "retired"]
            }
            kind if kind.generational() => &["canonical", "draft", "retired"],
            _ => &["canonical", "retired"],
        }
    }

    fn default_status(self) -> &'static str {
        match self {
            Self::Scene => "planned",
            Self::Conversation => "draft",
            Self::Hypothesis => "open",
            _ => "canonical",
        }
    }

    /// Literal defaults shared by creation and static authoring guidance.
    fn create_defaults(self) -> Mapping {
        let mut fields = Mapping::new();
        fields.insert("status".into(), self.default_status().into());
        for key in ["aliases", "tags"] {
            fields.insert(key.into(), Value::Sequence(Vec::new()));
        }
        let arrays: &[&str] = match self {
            Self::Event => &["participants", "causes", "effects"],
            Self::Knowledge
            | Self::Relationship
            | Self::Parentage
            | Self::Union
            | Self::Organization
            | Self::Affiliation
            | Self::Legacy
            | Self::Tenure
            | Self::Claim
            | Self::VitalHistory => &["transitions"],
            Self::Scene => &[
                "participants",
                "objects",
                "environments",
                "conversations",
                "story_points",
                "outcome_events",
                "observations",
            ],
            Self::Conversation => &["participants", "turns", "recollections"],
            _ => &[],
        };
        for key in arrays {
            fields.insert((*key).into(), Value::Sequence(Vec::new()));
        }
        if matches!(self, Self::Character | Self::Location | Self::Object) {
            fields.insert("initial_state".into(), Value::Mapping(Mapping::new()));
        }
        if self == Self::Route {
            fields.insert("availability".into(), "open".into());
            fields.insert("uncertainty".into(), "exact".into());
        }
        fields
    }

    fn parse(value: &Value) -> Result<Self> {
        match text(value, "kind")? {
            "world" => Ok(Self::World),
            "character" => Ok(Self::Character),
            "location" => Ok(Self::Location),
            "object" => Ok(Self::Object),
            "event" => Ok(Self::Event),
            "knowledge" => Ok(Self::Knowledge),
            "environment" => Ok(Self::Environment),
            "relationship" => Ok(Self::Relationship),
            "story-point" => Ok(Self::StoryPoint),
            "hypothesis" => Ok(Self::Hypothesis),
            "scene" => Ok(Self::Scene),
            "conversation" => Ok(Self::Conversation),
            "route" => Ok(Self::Route),
            "map" => Ok(Self::Map),
            "anchor" => Ok(Self::Anchor),
            "portal" => Ok(Self::Portal),
            "overlay" => Ok(Self::Overlay),
            "lore-concept" => Ok(Self::LoreConcept),
            "assertion" => Ok(Self::Assertion),
            "existence-episode" => Ok(Self::ExistenceEpisode),
            "context-brief" => Ok(Self::ContextBrief),
            "parentage" => Ok(Self::Parentage),
            "union" => Ok(Self::Union),
            "organization" => Ok(Self::Organization),
            "affiliation" => Ok(Self::Affiliation),
            "legacy" => Ok(Self::Legacy),
            "tenure" => Ok(Self::Tenure),
            "claim" => Ok(Self::Claim),
            "vital-history" => Ok(Self::VitalHistory),
            _ => Err(invalid(
                "editable kinds are world (update only), character, location, object, event, knowledge, environment, relationship, story-point, hypothesis, scene, conversation, route, map, anchor, portal, overlay, lore-concept, assertion, existence-episode, context-brief, organization, parentage, union, affiliation, legacy, tenure, claim, and vital-history",
            )),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::World => "world",
            Self::Character => "character",
            Self::Location => "location",
            Self::Object => "object",
            Self::Event => "event",
            Self::Knowledge => "knowledge",
            Self::Environment => "environment",
            Self::Relationship => "relationship",
            Self::StoryPoint => "story-point",
            Self::Hypothesis => "hypothesis",
            Self::Scene => "scene",
            Self::Conversation => "conversation",
            Self::Route => "route",
            Self::Map => "map",
            Self::Anchor => "anchor",
            Self::Portal => "portal",
            Self::Overlay => "overlay",
            Self::LoreConcept => "lore-concept",
            Self::Assertion => "assertion",
            Self::ExistenceEpisode => "existence-episode",
            Self::ContextBrief => "context-brief",
            Self::Parentage => "parentage",
            Self::Union => "union",
            Self::Organization => "organization",
            Self::Affiliation => "affiliation",
            Self::Legacy => "legacy",
            Self::Tenure => "tenure",
            Self::Claim => "claim",
            Self::VitalHistory => "vital-history",
        }
    }
    fn directory(self) -> &'static str {
        match self {
            Self::World => "world",
            Self::Character => "characters",
            Self::Location => "locations",
            Self::Object => "objects",
            Self::Event => "events",
            Self::Knowledge => "knowledge",
            Self::Environment => "environments",
            Self::Relationship => "relationships",
            Self::StoryPoint => "story-points",
            Self::Hypothesis => "hypotheses",
            Self::Scene => "scenes",
            Self::Conversation => "conversations",
            Self::Route => "routes",
            Self::Map => "maps",
            Self::Anchor => "anchors",
            Self::Portal => "portals",
            Self::Overlay => "overlays",
            Self::LoreConcept => "concepts",
            Self::Assertion => "assertions",
            Self::ExistenceEpisode => "existence",
            Self::ContextBrief => "briefs",
            Self::Parentage => "kinships",
            Self::Union => "unions",
            Self::Organization => "organizations",
            Self::Affiliation => "affiliations",
            Self::Legacy => "legacies",
            Self::Tenure => "tenures",
            Self::Claim => "claims",
            Self::VitalHistory => "vitals",
        }
    }
    fn generational(self) -> bool {
        matches!(
            self,
            Self::Organization
                | Self::Parentage
                | Self::Union
                | Self::Affiliation
                | Self::Legacy
                | Self::Tenure
                | Self::Claim
                | Self::VitalHistory
        )
    }
    fn fields(self) -> &'static [&'static str] {
        match self {
            Self::World => &[
                "current_time",
                "chronology",
                "profiles",
                "predicates",
                "state_keys",
                "state_profile_bindings",
                "timelines",
                "default_timeline",
                "capabilities",
                "relationship_metrics",
            ],
            Self::Character => &["initial_state", "role", "pronouns"],
            Self::Location => &["initial_state", "parent_id", "spatial", "spatial_history"],
            Self::Object => &["initial_state", "object_affordances"],
            Self::Event => &[
                "time",
                "location",
                "participants",
                "causes",
                "effects",
                "related_story_points",
            ],
            Self::Knowledge => &["knower", "claim", "transitions", "audience", "perspectives"],
            Self::Environment => &["time", "targets", "conditions", "sensory", "initial_state"],
            Self::Relationship => &["from", "to", "relationship_kind", "inverse", "transitions"],
            Self::StoryPoint => &[
                "activation_policy",
                "lifecycle",
                "requires",
                "when",
                "priority",
                "outcome_events",
            ],
            Self::Hypothesis => &[
                "statement",
                "subjects",
                "alternatives",
                "context",
                "placement",
                "resolution",
            ],
            Self::Scene => &[
                "status",
                "location",
                "time",
                "participants",
                "objects",
                "environments",
                "conversations",
                "story_points",
                "outcome_events",
                "observations",
            ],
            Self::Conversation => &[
                "status",
                "location",
                "scene",
                "time",
                "participants",
                "turns",
                "recollections",
            ],
            Self::Route => &[
                "from_location_id",
                "to_location_id",
                "direction",
                "modes",
                "availability",
                "uncertainty",
                "route_distance",
                "travel_cost",
                "duration",
                "spatial_history",
            ],
            Self::Map => &[
                "crs",
                "axis_order",
                "unit",
                "bounds",
                "z_policy",
                "origin",
                "scale",
            ],
            Self::Anchor => &["from", "to", "conversion", "status"],
            Self::Portal => &["from_location_id", "to", "modes", "status"],
            Self::Overlay => &[
                "lifecycle",
                "membership",
                "audience",
                "perspectives",
                "valid",
                "status",
            ],
            Self::LoreConcept => &[
                "domain",
                "profile",
                "values",
                "valid",
                "audience",
                "perspectives",
                "status",
                "provenance",
            ],
            Self::Assertion => &[
                "domain",
                "subject",
                "predicate",
                "value",
                "classification",
                "attributed_to",
                "valid",
                "evidence",
                "replaces",
                "provenance",
                "audience",
                "perspectives",
                "status",
            ],
            Self::ExistenceEpisode => &[
                "domain",
                "character_id",
                "episode_kind",
                "valid",
                "start_evidence",
                "end_evidence",
                "audience",
                "perspectives",
                "status",
                "provenance",
            ],
            Self::ContextBrief => &[
                "domain",
                "scope",
                "scope_ref",
                "valid",
                "required_refs",
                "audience",
                "perspectives",
                "status",
                "provenance",
            ],
            Self::Parentage => &[
                "child_id",
                "parent_id",
                "initialization",
                "transitions",
                "status",
            ],
            Self::Union => &["participant_ids", "initialization", "transitions", "status"],
            Self::Organization => &[
                "organization_kind",
                "parent_id",
                "location_id",
                "initialization",
                "transitions",
                "status",
            ],
            Self::Affiliation => &[
                "character_id",
                "organization_id",
                "initialization",
                "transitions",
                "status",
            ],
            Self::Legacy => &[
                "legacy_kind",
                "organization_id",
                "initialization",
                "transitions",
                "status",
            ],
            Self::Tenure => &[
                "legacy_id",
                "predecessor_tenure_id",
                "successor_tenure_id",
                "initialization",
                "transitions",
                "status",
            ],
            Self::Claim => &[
                "legacy_id",
                "claimant_id",
                "initialization",
                "transitions",
                "status",
            ],
            Self::VitalHistory => &[
                "character_id",
                "disclosure",
                "initialization",
                "transitions",
                "status",
            ],
        }
    }
}

/// Top-level request guidance, not a substitute for the native field parsers or
/// complete candidate validation. All lists use the same rules as construction.
pub(crate) fn kind_guidance() -> serde_json::Value {
    use serde_json::{Value as Json, json};
    let mut kinds = Kind::ALL.to_vec();
    kinds.sort_by_key(|kind| kind.name());
    Json::Array(kinds.into_iter().map(|kind| {
        let creating = kind != Kind::World;
        let mut required = if creating { vec!["kind", "id", "title"] } else { Vec::new() };
        if creating { required.extend(kind.create_required()); }
        let allowed = COMMON.iter().chain(kind.fields()).copied().collect::<BTreeSet<_>>();
        let create_optional = allowed.iter().copied()
            .filter(|key| creating && kind.accepts(key, true) && !required.contains(key)).collect::<Vec<_>>();
        let update_optional = allowed.iter().copied()
            .filter(|key| *key != "kind" && kind.accepts(key, false)).collect::<Vec<_>>();
        let defaults = if creating {
            let mut value = serde_json::to_value(kind.create_defaults()).expect("literal creation defaults are JSON");
            value["body"] = json!("");
            value
        } else { Json::Null };
        json!({
            "kind":kind.name(), "createSupported":creating,
            "createRequired":required, "createOptional":create_optional, "createDefaults":defaults,
            "updateRequired":["kind"], "updateOptional":update_optional,
            "statuses":kind.statuses(),
        })
    }).collect())
}

/// An editable no-op patch contains every authored field accepted by this same
/// codec, plus the exact Markdown body. Protected and direct-source-only fields
/// remain in the original record rather than being copied into a closed patch.
/// The caller must still construct/preview it and prove byte-identical output:
/// a valid source field can intentionally have a narrower typed-edit grammar.
pub(crate) fn scaffold_patch(record: &Record) -> Result<(String, Vec<String>)> {
    let kind = Kind::parse(&Value::String(record.kind.clone()))?;
    let fields = map(&record.frontmatter, "existing frontmatter")?;
    let mut patch = Mapping::new();
    patch.insert("kind".into(), kind.name().into());
    let mut retained = BTreeSet::new();
    for (key, value) in fields {
        let key = key.as_str().ok_or_else(|| {
            invalid("authoring scaffold requires string top-level source field names")
        })?;
        if key == "kind" {
            continue;
        }
        // A frontmatter field named body is retained metadata; patch.body
        // always names the separate Markdown body after the closing fence.
        if key != "body" && kind.accepts(key, false) {
            patch.insert(key.into(), value.clone());
        } else {
            retained.insert(key.to_owned());
        }
    }
    patch.insert("body".into(), record.body.clone().into());
    Ok((
        serde_yaml::to_string(&Value::Mapping(patch))?,
        retained.into_iter().collect(),
    ))
}

struct Draft {
    kind: Kind,
    fields: Mapping,
    body: Option<String>,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn string<'a>(value: &'a Value, label: &str) -> Result<&'a str> {
    match value {
        Value::String(value) => Ok(value),
        _ => Err(invalid(format!("{label} must be a string"))),
    }
}

fn text<'a>(value: &'a Value, label: &str) -> Result<&'a str> {
    let text = string(value, label)?;
    if text.trim().is_empty() {
        return Err(invalid(format!("{label} must not be blank")));
    }
    Ok(text)
}

fn map<'a>(value: &'a Value, label: &str) -> Result<&'a Mapping> {
    match value {
        Value::Mapping(value) => Ok(value),
        _ => Err(invalid(format!("{label} must be a mapping"))),
    }
}

fn list<'a>(value: &'a Value, label: &str) -> Result<&'a Vec<Value>> {
    match value {
        Value::Sequence(value) => Ok(value),
        _ => Err(invalid(format!("{label} must be an array"))),
    }
}

fn required<'a>(value: &'a Mapping, key: &str, label: &str) -> Result<&'a Value> {
    value
        .get(key)
        .ok_or_else(|| invalid(format!("{label}.{key} is required")))
}

fn closed<'a>(
    value: &'a Value,
    required_fields: &[&str],
    optional_fields: &[&str],
    label: &str,
) -> Result<&'a Mapping> {
    let fields = map(value, label)?;
    for key in fields.keys() {
        let key = text(key, &format!("{label} field name"))?;
        if !required_fields.contains(&key) && !optional_fields.contains(&key) {
            return Err(invalid(format!("unknown {label} field {key:?}")));
        }
    }
    for key in required_fields {
        required(fields, key, label)?;
    }
    Ok(fields)
}

/// This scan applies to submitted values only. Existing metadata is never
/// rejected or removed merely because an untouched field contains null. Within
/// chronology and supported history payloads null is data (for example an
/// isolated epoch or vacant holder), not a top-level deletion operation; the
/// shared domain validator checks its exact permitted positions.
fn supplied_literals(value: &Value, null_is_data: bool) -> Result<()> {
    match value {
        Value::Null if !null_is_data => Err(invalid(
            "null is not an edit operation; omit a field to keep it, or provide an empty array/map/string where that field allows it",
        )),
        Value::Tagged(_) => Err(invalid("tagged YAML is not accepted in record drafts")),
        Value::Sequence(values) => {
            for value in values {
                supplied_literals(value, null_is_data)?;
            }
            Ok(())
        }
        Value::Mapping(values) => {
            for (key, value) in values {
                supplied_literals(key, false)?;
                supplied_literals(value, null_is_data)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Replacing a turn array retains the source's three optional null spellings.
/// This exception applies only to the exact leaf field, not its descendants.
fn supplied_turn_literals(value: &Value) -> Result<()> {
    for turn in list(value, "turns")? {
        let fields = map(turn, "turn")?;
        let speech = fields
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("speech")
            == "speech";
        for (field, value) in fields {
            let field = text(field, "turn field")?;
            if value.is_null()
                && (field == "audience" || speech && matches!(field, "delivery" | "addressee"))
            {
                continue;
            }
            supplied_literals(value, false)?;
        }
    }
    Ok(())
}

fn record_reference(value: &Value, kind: Option<&str>, label: &str) -> Result<()> {
    let id = text(value, label)?;
    let valid = kind.map_or_else(
        || validation::valid_record_id(id),
        |kind| validation::valid_id_for_kind(id, kind),
    );
    if !valid {
        return Err(invalid(format!(
            "{label} must be an exact {}ID",
            kind.map(|kind| format!("{kind} "))
                .unwrap_or_else(|| "record ".into())
        )));
    }
    Ok(())
}

fn unique_strings(value: &Value, label: &str) -> Result<()> {
    let mut seen = BTreeSet::new();
    for value in list(value, label)? {
        let value = text(value, label)?;
        if !seen.insert(value) {
            return Err(invalid(format!("{label} contains duplicate {value:?}")));
        }
    }
    Ok(())
}

fn ordered_strings(value: &Value, label: &str) -> Result<()> {
    let mut previous = None;
    for value in list(value, label)? {
        let value = text(value, label)?;
        if previous.is_some_and(|old| old >= value) {
            return Err(invalid(format!("{label} must be sorted and unique")));
        }
        previous = Some(value);
    }
    Ok(())
}

fn point(world: &World, value: &Value, label: &str) -> Result<()> {
    closed(value, &["tick"], &["timeline", "order"], label)?;
    StoryTime::from_yaml(value, &world.default_timeline())
        .map_err(|error| invalid(format!("{label}: {error}")))?;
    Ok(())
}

fn entity_value(value: &Value, label: &str) -> Result<()> {
    let fields = closed(value, &["entity"], &[], label)?;
    record_reference(
        required(fields, "entity", label)?,
        None,
        &format!("{label}.entity"),
    )
}

fn set_item(value: &Value, label: &str) -> Result<()> {
    match value {
        Value::String(_) => Ok(()),
        Value::Mapping(_) => entity_value(value, label),
        _ => Err(invalid(format!(
            "{label} must be a string or a closed entity reference"
        ))),
    }
}

/// Only the seven current state value shapes are accepted. Declaration types,
/// target existence/kinds and exclusivity are checked on the complete candidate.
fn state_value(value: &Value, label: &str) -> Result<()> {
    match value {
        Value::String(_) | Value::Bool(_) => Ok(()),
        Value::Number(number) if number.as_f64().is_some_and(f64::is_finite) => Ok(()),
        Value::Mapping(_) => entity_value(value, label),
        Value::Sequence(values) => {
            for (index, value) in values.iter().enumerate() {
                set_item(value, &format!("{label}[{index}]"))?;
                if values
                    .first()
                    .is_some_and(|first| first.is_string() != value.is_string())
                {
                    return Err(invalid(format!(
                        "{label} must contain only strings or only entity references"
                    )));
                }
                if values[..index]
                    .iter()
                    .any(|previous| state_schema::values_equal(previous, value))
                {
                    return Err(invalid(format!("{label} contains a duplicate item")));
                }
            }
            Ok(())
        }
        _ => Err(invalid(format!("{label} is not a supported state value"))),
    }
}

fn profile_state_value(value: &Value, label: &str) -> Result<()> {
    map(value, label)?;
    crate::lore_types::bounded(value, label).map_err(|error| invalid(error.to_string()))
}

fn initial_state(world: &World, kind: Kind, value: &Value) -> Result<()> {
    let bindings =
        crate::lore_state::Bindings::parse(world).map_err(|error| invalid(error.to_string()))?;
    for (key, value) in map(value, "initial_state")? {
        let key = text(key, "initial_state key")?;
        let label = format!("initial_state.{key}");
        if bindings.get(kind.name(), key).is_some() {
            profile_state_value(value, &label)?;
        } else {
            state_value(value, &label)?;
        }
    }
    Ok(())
}

fn state_keys(value: &Value) -> Result<()> {
    for (kind, fields) in map(value, "state_keys")? {
        let kind = text(kind, "state owner kind")?;
        for (key, declaration) in map(fields, &format!("state_keys.{kind}"))? {
            text(key, "state key")?;
            state_schema::validate_definition(declaration)
                .map_err(|error| invalid(error.to_string()))?;
        }
    }
    Ok(())
}

fn participants(value: &Value) -> Result<()> {
    for (index, value) in list(value, "participants")?.iter().enumerate() {
        let label = format!("participants[{index}]");
        let fields = closed(value, &["character"], &["role"], &label)?;
        record_reference(
            required(fields, "character", &label)?,
            Some("character"),
            &format!("{label}.character"),
        )?;
        if let Some(value) = fields.get("role") {
            string(value, &format!("{label}.role"))?;
        }
    }
    Ok(())
}

fn causes(value: &Value) -> Result<()> {
    let mut seen = BTreeSet::new();
    for (index, value) in list(value, "causes")?.iter().enumerate() {
        record_reference(value, Some("event"), &format!("causes[{index}]"))?;
        if !seen.insert(string(value, "cause")?) {
            return Err(invalid("causes contains a duplicate event"));
        }
    }
    Ok(())
}

fn effects(world: &World, value: &Value) -> Result<()> {
    let bindings =
        crate::lore_state::Bindings::parse(world).map_err(|error| invalid(error.to_string()))?;
    let mut identifiers = BTreeSet::new();
    for (index, value) in list(value, "effects")?.iter().enumerate() {
        let label = format!("effects[{index}]");
        let fields = closed(
            value,
            &["id", "target", "key", "operation"],
            &["value"],
            &label,
        )?;
        let id = text(required(fields, "id", &label)?, &format!("{label}.id"))?;
        if !validation::valid_id_for_kind(id, "effect") || !identifiers.insert(id) {
            return Err(invalid(format!("{label}.id must be a unique effect ID")));
        }
        record_reference(
            required(fields, "target", &label)?,
            None,
            &format!("{label}.target"),
        )?;
        let key = text(required(fields, "key", &label)?, &format!("{label}.key"))?;
        let bound = fields
            .get("target")
            .and_then(Value::as_str)
            .and_then(|id| world.records.get(id))
            .is_some_and(|target| bindings.get(&target.kind, key).is_some());
        let operation = text(
            required(fields, "operation", &label)?,
            &format!("{label}.operation"),
        )?;
        match operation {
            "clear" => {
                if fields.contains_key("value") {
                    return Err(invalid("clear forbids a value field"));
                }
            }
            "set" if bound => profile_state_value(
                required(fields, "value", &label)?,
                &format!("{label}.value"),
            )?,
            "set" => state_value(
                required(fields, "value", &label)?,
                &format!("{label}.value"),
            )?,
            "add-to-set" | "remove-from-set" if bound => {
                return Err(invalid("profile-bound state supports only set and clear"));
            }
            "add-to-set" | "remove-from-set" => set_item(
                required(fields, "value", &label)?,
                &format!("{label}.value"),
            )?,
            _ => return Err(invalid(format!("unknown effect operation {operation:?}"))),
        }
    }
    Ok(())
}

fn claim(value: &Value) -> Result<()> {
    if value.get("assertion_id").is_some() {
        let fields = closed(value, &["key", "statement", "assertion_id"], &[], "claim")?;
        text(required(fields, "key", "claim")?, "claim.key")?;
        text(required(fields, "statement", "claim")?, "claim.statement")?;
        record_reference(
            required(fields, "assertion_id", "claim")?,
            Some("assertion"),
            "claim.assertion_id",
        )?;
        return Ok(());
    }
    if value.get("genealogy").is_some() {
        let fields = closed(value, &["key", "statement", "genealogy"], &[], "claim")?;
        text(required(fields, "key", "claim")?, "claim.key")?;
        text(required(fields, "statement", "claim")?, "claim.statement")?;
        map(required(fields, "genealogy", "claim")?, "claim.genealogy")?;
        // The candidate's shared generational validator checks the closed
        // assertion, endpoint references, validity, labels and evidence.
        return Ok(());
    }
    let fields = closed(
        value,
        &["key", "statement"],
        &["subject", "predicate", "object", "truth"],
        "claim",
    )?;
    text(required(fields, "key", "claim")?, "claim.key")?;
    text(required(fields, "statement", "claim")?, "claim.statement")?;
    if let Some(value) = fields.get("subject") {
        record_reference(value, None, "claim.subject")?;
    }
    if let Some(value) = fields.get("predicate") {
        text(value, "claim.predicate")?;
    }
    if let Some(value) = fields.get("object") {
        let object = closed(value, &[], &["text", "entity"], "claim.object")?;
        if object.len() != 1 {
            return Err(invalid(
                "claim.object requires exactly one of text or entity",
            ));
        }
        if let Some(value) = object.get("text") {
            text(value, "claim.object.text")?;
        }
        if let Some(value) = object.get("entity") {
            record_reference(value, None, "claim.object.entity")?;
        }
    }
    if let Some(value) = fields.get("truth") {
        if !["true", "false", "unknown", "contested"].contains(&text(value, "claim.truth")?) {
            return Err(invalid(
                "claim.truth must be the string true, false, unknown, or contested",
            ));
        }
    }
    Ok(())
}

fn transitions(world: &World, value: &Value) -> Result<()> {
    let mut identifiers = BTreeSet::new();
    for (index, value) in list(value, "transitions")?.iter().enumerate() {
        let label = format!("transitions[{index}]");
        let fields = closed(
            value,
            &["id", "time", "state", "acquisition", "confidence"],
            &["source_entity", "causing_event", "note"],
            &label,
        )?;
        let id = text(required(fields, "id", &label)?, &format!("{label}.id"))?;
        if !identifiers.insert(id) {
            return Err(invalid(format!("duplicate transition ID {id:?}")));
        }
        point(
            world,
            required(fields, "time", &label)?,
            &format!("{label}.time"),
        )?;
        if !KNOWLEDGE_STATES.contains(&text(
            required(fields, "state", &label)?,
            &format!("{label}.state"),
        )?) {
            return Err(invalid(format!("{label}.state is unknown")));
        }
        if !ACQUISITIONS.contains(&text(
            required(fields, "acquisition", &label)?,
            &format!("{label}.acquisition"),
        )?) {
            return Err(invalid(format!("{label}.acquisition is unknown")));
        }
        let confidence = required(fields, "confidence", &label)?;
        if !matches!(confidence, Value::Number(number) if number.as_f64().is_some_and(|value| value.is_finite() && (0.0..=1.0).contains(&value)))
        {
            return Err(invalid(format!(
                "{label}.confidence must be a finite number between 0 and 1"
            )));
        }
        if let Some(value) = fields.get("source_entity") {
            record_reference(value, None, &format!("{label}.source_entity"))?;
        }
        if let Some(value) = fields.get("causing_event") {
            record_reference(value, Some("event"), &format!("{label}.causing_event"))?;
        }
        if let Some(value) = fields.get("note") {
            string(value, &format!("{label}.note"))?;
        }
    }
    Ok(())
}

fn section_audiences(kind: Kind, value: &Value) -> Result<()> {
    let mut headings = BTreeSet::new();
    for (heading, values) in map(value, "section_audiences")? {
        let heading = text(heading, "section heading")?;
        if heading.chars().any(char::is_control) || !headings.insert(heading.trim().to_lowercase())
        {
            return Err(invalid(
                "section headings must be single-line and unique after trimming and case folding",
            ));
        }
        unique_strings(values, "section audience")?;
        for value in list(values, "section audience")? {
            let token = text(value, "section audience")?;
            if ["public", "author", "self"].contains(&token)
                || (token == "participants" && matches!(kind, Kind::Scene | Kind::Conversation))
            {
                continue;
            }
            record_reference(value, Some("character"), "section audience")?;
        }
    }
    Ok(())
}

fn declarations(world: &World, value: &Value, threads: bool) -> Result<()> {
    let label = if threads { "threads" } else { "timelines" };
    let mut identifiers = BTreeSet::new();
    let mut previous = "";
    let values = list(value, label)?;
    if !threads && values.is_empty() {
        return Err(invalid("timelines must not be empty"));
    }
    for value in values {
        let fields = closed(
            value,
            &["id", "label"],
            if threads { &[] } else { &["origin"] },
            label,
        )?;
        let id = text(&fields["id"], &format!("{label}.id"))?;
        text(&fields["label"], &format!("{label}.label"))?;
        if !identifiers.insert(id) {
            return Err(invalid(format!("{label}: duplicate ID {id}")));
        }
        if threads {
            if !crate::core_validation::thread_id(id) || id <= previous {
                return Err(invalid(
                    "thread declarations require sorted unique native thread IDs",
                ));
            }
            previous = id;
        }
        if let Some(origin) = fields.get("origin") {
            let origin = closed(origin, &["tick", "label"], &[], "timeline origin")?;
            text(&origin["label"], "timeline origin.label")?;
            let at = Value::Mapping(Mapping::from_iter([(
                "tick".into(),
                origin["tick"].clone(),
            )]));
            point(world, &at, "timeline origin")?;
        }
    }
    Ok(())
}

fn environment_conditions(value: &Value) -> Result<()> {
    for (key, value) in map(value, "conditions")? {
        text(key, "condition key")?;
        match value {
            Value::Null | Value::String(_) | Value::Bool(_) => {}
            Value::Number(number) if number.as_f64().is_some_and(f64::is_finite) => {}
            _ => {
                return Err(invalid(
                    "environment conditions must be flat finite scalar values",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) const STORY_STATES: &[&str] = &["dormant", "active", "resolved", "failed", "cancelled"];
fn story_state(value: &Value, label: &str) -> Result<()> {
    if !STORY_STATES.contains(&text(value, label)?) {
        return Err(invalid(format!("{label}: invalid lifecycle state")));
    }
    Ok(())
}
fn story_states(value: &Value, label: &str) -> Result<()> {
    unique_strings(value, label)?;
    if list(value, label)?.is_empty() {
        return Err(invalid(format!("{label} must not be empty")));
    }
    for value in list(value, label)? {
        story_state(value, label)?;
    }
    Ok(())
}
fn lifecycle(world: &World, value: &Value) -> Result<()> {
    let fields = closed(value, &["initial_state"], &["transitions"], "lifecycle")?;
    story_state(&fields["initial_state"], "lifecycle.initial_state")?;
    let Some(values) = fields.get("transitions") else {
        return Ok(());
    };
    let mut identifiers = BTreeSet::new();
    for value in list(values, "lifecycle.transitions")? {
        let fields = closed(
            value,
            &["id", "state", "time"],
            &["causing_event", "note"],
            "lifecycle transition",
        )?;
        if !identifiers.insert(text(&fields["id"], "lifecycle transition.id")?) {
            return Err(invalid("duplicate lifecycle transition ID"));
        }
        story_state(&fields["state"], "lifecycle transition.state")?;
        point(world, &fields["time"], "lifecycle transition.time")?;
        if let Some(value) = fields.get("causing_event") {
            record_reference(value, Some("event"), "lifecycle transition.causing_event")?;
        }
        if let Some(value) = fields.get("note") {
            text(value, "lifecycle transition.note")?;
        }
    }
    Ok(())
}
fn requirements(value: &Value) -> Result<()> {
    let mut identifiers = BTreeSet::new();
    for value in list(value, "requires")? {
        let fields = closed(value, &["id", "states"], &[], "requirement")?;
        record_reference(&fields["id"], Some("story-point"), "requirement.id")?;
        if !identifiers.insert(fields["id"].as_str().unwrap()) {
            return Err(invalid("duplicate story-point requirement"));
        }
        story_states(&fields["states"], "requirement.states")?;
    }
    Ok(())
}
fn condition(value: &Value, depth: usize, nodes: &mut usize) -> Result<()> {
    *nodes += 1;
    if depth > 32 || *nodes > 1000 {
        return Err(invalid("when expression exceeds depth 32 or 1000 nodes"));
    }
    let fields = map(value, "when")?;
    if fields.len() != 1 {
        return Err(invalid("when must contain exactly one operator"));
    }
    let (operator, value) = fields.iter().next().unwrap();
    match text(operator, "when operator")? {
        "all" | "any" => {
            for child in list(value, "logical condition")? {
                condition(child, depth + 1, nodes)?;
            }
        }
        "not" => condition(value, depth + 1, nodes)?,
        "event" => record_reference(value, Some("event"), "event condition")?,
        "state" => {
            let fields = closed(value, &["entity", "key", "equals"], &[], "state condition")?;
            record_reference(&fields["entity"], None, "state condition.entity")?;
            text(&fields["key"], "state condition.key")?;
            crate::lore_types::bounded(&fields["equals"], "state condition.equals")
                .map_err(|error| invalid(error.to_string()))?;
        }
        "knowledge" => {
            let fields = closed(
                value,
                &["character", "claim", "states"],
                &["minimum_confidence"],
                "knowledge condition",
            )?;
            record_reference(
                &fields["character"],
                Some("character"),
                "knowledge condition.character",
            )?;
            text(&fields["claim"], "knowledge condition.claim")?;
            unique_strings(&fields["states"], "knowledge condition.states")?;
            let states = list(&fields["states"], "knowledge condition.states")?;
            if states.is_empty()
                || states
                    .iter()
                    .any(|v| v.as_str().is_none_or(|v| !KNOWLEDGE_STATES.contains(&v)))
            {
                return Err(invalid(
                    "knowledge condition requires nonempty known states",
                ));
            }
            if let Some(value) = fields.get("minimum_confidence") {
                if nonnegative_number(value, "minimum_confidence")? > 1.0 {
                    return Err(invalid("minimum_confidence must be between 0 and 1"));
                }
            }
        }
        "relationship" => {
            let fields = closed(
                value,
                &["id", "metric"],
                &["min", "max"],
                "relationship condition",
            )?;
            record_reference(
                &fields["id"],
                Some("relationship"),
                "relationship condition.id",
            )?;
            text(&fields["metric"], "relationship condition.metric")?;
            if !fields.contains_key("min") && !fields.contains_key("max") {
                return Err(invalid("relationship condition requires min or max"));
            }
            for key in ["min", "max"] {
                if let Some(value) = fields.get(key) {
                    crate::core_validation::finite_number(value, key)
                        .map_err(|error| invalid(error.to_string()))?;
                }
            }
        }
        "scene" => {
            let fields = closed(
                value,
                &[],
                &["location", "participant", "object"],
                "scene condition",
            )?;
            for (field, kind) in [
                ("location", "location"),
                ("participant", "character"),
                ("object", "object"),
            ] {
                if let Some(value) = fields.get(field) {
                    record_reference(value, Some(kind), &format!("scene condition.{field}"))?;
                }
            }
        }
        _ => return Err(invalid("unknown when operator")),
    }
    Ok(())
}

// These helpers check submitted shapes only. The merged candidate's scene
// validator owns time ordering, reference existence, presence and overlap.
fn interval(world: &World, kind: Kind, value: &Value) -> Result<()> {
    let required_fields = if kind == Kind::Scene {
        &["start", "current"][..]
    } else {
        &["start"][..]
    };
    for (key, value) in closed(value, required_fields, &["end"], "time")? {
        if kind == Kind::Environment && key.as_str() == Some("end") && value.is_null() {
            continue;
        }
        point(world, value, &format!("time.{}", key.as_str().unwrap()))?;
    }
    Ok(())
}

fn timed_participants(world: &World, value: &Value) -> Result<()> {
    for (index, value) in list(value, "participants")?.iter().enumerate() {
        let label = format!("participants[{index}]");
        let fields = closed(
            value,
            &["character"],
            &["from", "until", "role", "point_of_view"],
            &label,
        )?;
        record_reference(
            required(fields, "character", &label)?,
            Some("character"),
            &format!("{label}.character"),
        )?;
        for key in ["from", "until"] {
            if let Some(value) = fields.get(key) {
                point(world, value, &format!("{label}.{key}"))?;
            }
        }
        if let Some(value) = fields.get("role") {
            string(value, &format!("{label}.role"))?;
        }
        if fields
            .get("point_of_view")
            .is_some_and(|value| !matches!(value, Value::Bool(_)))
        {
            return Err(invalid(format!("{label}.point_of_view must be boolean")));
        }
    }
    Ok(())
}

fn references(value: &Value, kind: &str, label: &str) -> Result<()> {
    unique_strings(value, label)?;
    for value in list(value, label)? {
        record_reference(value, Some(kind), label)?;
    }
    Ok(())
}

fn audience(value: &Value, label: &str) -> Result<()> {
    unique_strings(value, label)?;
    for value in list(value, label)? {
        if !["public", "participants", "author"].contains(&text(value, label)?) {
            record_reference(value, Some("character"), label)?;
        }
    }
    Ok(())
}

fn nonnegative_number(value: &Value, label: &str) -> Result<f64> {
    match value {
        Value::Number(number) => number
            .as_f64()
            .filter(|number| number.is_finite() && *number >= 0.0),
        _ => None,
    }
    .ok_or_else(|| invalid(format!("{label} must be a finite nonnegative number")))
}

fn route_metric(value: &Value, label: &str) -> Result<()> {
    let fields = closed(value, &["value", "unit"], &[], label)?;
    nonnegative_number(required(fields, "value", label)?, &format!("{label}.value"))?;
    text(required(fields, "unit", label)?, &format!("{label}.unit"))?;
    Ok(())
}

fn coordinates(value: &Value, label: &str) -> Result<Vec<f64>> {
    crate::geometry::coordinates(value, label).map_err(|error| invalid(error.to_string()))
}

fn map_bounds(value: &Value) -> Result<()> {
    let fields = closed(value, &["min", "max"], &[], "bounds")?;
    let min = coordinates(&fields["min"], "bounds.min")?;
    let max = coordinates(&fields["max"], "bounds.max")?;
    if min.len() != max.len() || min.iter().zip(&max).any(|(min, max)| min >= max) {
        return Err(invalid(
            "map bounds require matching dimensions and min < max on every axis",
        ));
    }
    Ok(())
}

fn placement(value: &Value) -> Result<()> {
    let fields = closed(value, &["map_id", "geometry"], &[], "spatial")?;
    record_reference(&fields["map_id"], Some("map"), "spatial.map_id")?;
    crate::geometry::validate_shape(&fields["geometry"]).map_err(|error| invalid(error.to_string()))
}

fn map_position(value: &Value, label: &str) -> Result<()> {
    let fields = closed(value, &["map_id", "coordinates"], &[], label)?;
    record_reference(&fields["map_id"], Some("map"), &format!("{label}.map_id"))?;
    coordinates(&fields["coordinates"], &format!("{label}.coordinates"))?;
    Ok(())
}

fn observations(world: &World, value: &Value) -> Result<()> {
    for (index, value) in list(value, "observations")?.iter().enumerate() {
        let label = format!("observations[{index}]");
        let fields = closed(
            value,
            &["id", "at", "text"],
            &["until", "audience", "salience"],
            &label,
        )?;
        for key in ["id", "text"] {
            text(required(fields, key, &label)?, &format!("{label}.{key}"))?;
        }
        point(
            world,
            required(fields, "at", &label)?,
            &format!("{label}.at"),
        )?;
        if let Some(value) = fields.get("until") {
            point(world, value, &format!("{label}.until"))?;
        }
        if let Some(value) = fields.get("audience") {
            audience(value, &format!("{label}.audience"))?;
        }
        if let Some(value) = fields.get("salience") {
            nonnegative_number(value, &format!("{label}.salience"))?;
        }
    }
    Ok(())
}

fn addressee(value: &Value, label: &str) -> Result<()> {
    match value {
        Value::String(value) if value == "participants" => Ok(()),
        Value::Sequence(_) => references(value, "character", label),
        _ => record_reference(value, Some("character"), label),
    }
}

fn turns(world: &World, value: &Value) -> Result<()> {
    for (index, value) in list(value, "turns")?.iter().enumerate() {
        let label = format!("turns[{index}]");
        let kind = map(value, &label)?
            .get("kind")
            .map(|value| text(value, &format!("{label}.kind")))
            .transpose()?
            .unwrap_or("speech");
        let fields = match kind {
            "speech" => closed(
                value,
                &["id", "at", "text", "speaker"],
                &["kind", "audience", "delivery", "addressee", "interrupts"],
                &label,
            )?,
            "action" => closed(
                value,
                &["id", "at", "text", "actors"],
                &["kind", "audience"],
                &label,
            )?,
            _ => return Err(invalid(format!("{label}.kind must be speech or action"))),
        };
        for key in ["id", "text"] {
            text(required(fields, key, &label)?, &format!("{label}.{key}"))?;
        }
        point(
            world,
            required(fields, "at", &label)?,
            &format!("{label}.at"),
        )?;
        if let Some(value) = fields.get("audience").filter(|value| !value.is_null()) {
            audience(value, &format!("{label}.audience"))?;
        }
        if kind == "speech" {
            record_reference(
                required(fields, "speaker", &label)?,
                Some("character"),
                &format!("{label}.speaker"),
            )?;
            if let Some(value) = fields.get("delivery").filter(|value| !value.is_null()) {
                string(value, &format!("{label}.delivery"))?;
            }
            if let Some(value) = fields.get("addressee").filter(|value| !value.is_null()) {
                addressee(value, &format!("{label}.addressee"))?;
            }
            if let Some(value) = fields.get("interrupts") {
                text(value, &format!("{label}.interrupts"))?;
            }
        } else {
            let actors = required(fields, "actors", &label)?;
            references(actors, "character", &format!("{label}.actors"))?;
            if list(actors, &format!("{label}.actors"))?.is_empty() {
                return Err(invalid(format!("{label}.actors must not be empty")));
            }
        }
    }
    Ok(())
}

fn recollections(world: &World, value: &Value) -> Result<()> {
    for (index, value) in list(value, "recollections")?.iter().enumerate() {
        let label = format!("recollections[{index}]");
        let fields = closed(
            value,
            &["id", "at", "character", "state", "confidence", "summary"],
            &[
                "exact_turns",
                "remembered_quotes",
                "interpretation",
                "emotional_impression",
            ],
            &label,
        )?;
        for key in ["id", "summary"] {
            text(required(fields, key, &label)?, &format!("{label}.{key}"))?;
        }
        point(
            world,
            required(fields, "at", &label)?,
            &format!("{label}.at"),
        )?;
        record_reference(
            required(fields, "character", &label)?,
            Some("character"),
            &format!("{label}.character"),
        )?;
        if !["remembered", "forgotten"].contains(&text(
            required(fields, "state", &label)?,
            &format!("{label}.state"),
        )?) {
            return Err(invalid(format!(
                "{label}.state must be remembered or forgotten"
            )));
        }
        if nonnegative_number(
            required(fields, "confidence", &label)?,
            &format!("{label}.confidence"),
        )? > 1.0
        {
            return Err(invalid(format!(
                "{label}.confidence must be between 0 and 1"
            )));
        }
        for key in ["interpretation", "emotional_impression"] {
            if let Some(value) = fields.get(key) {
                string(value, &format!("{label}.{key}"))?;
            }
        }
        if let Some(value) = fields.get("exact_turns") {
            unique_strings(value, &format!("{label}.exact_turns"))?;
        }
        if let Some(value) = fields.get("remembered_quotes") {
            let label = format!("{label}.remembered_quotes");
            for (index, value) in list(value, &label)?.iter().enumerate() {
                let label = format!("{label}[{index}]");
                let fields = closed(
                    value,
                    &["source_turn", "speaker", "text", "fidelity"],
                    &[],
                    &label,
                )?;
                for key in ["source_turn", "text"] {
                    text(required(fields, key, &label)?, &format!("{label}.{key}"))?;
                }
                record_reference(
                    required(fields, "speaker", &label)?,
                    Some("character"),
                    &format!("{label}.speaker"),
                )?;
                if !["exact", "approximate"].contains(&text(
                    required(fields, "fidelity", &label)?,
                    &format!("{label}.fidelity"),
                )?) {
                    return Err(invalid(format!(
                        "{label}.fidelity must be exact or approximate"
                    )));
                }
            }
        }
    }
    Ok(())
}

fn draft(world: &World, request: &str, creating: bool) -> Result<Draft> {
    if request.len() > MAX_BYTES {
        return Err(invalid("record draft exceeds 1 MiB"));
    }
    // Bound aliases before Value can allocate expanded copies. Raw input and
    // expanded string bytes each have a 1 MiB cap; keys and values count toward
    // 131,072 expanded nodes, with depth at most 128 in the shared visitor.
    preflight_yaml(request, MAX_YAML_NODES, MAX_BYTES).map_err(|error| {
        invalid(format!(
            "record draft is invalid or exceeds complete expanded YAML bounds: {error}"
        ))
    })?;
    let value: Value = serde_yaml::from_str(request)
        .map_err(|error| invalid(format!("invalid draft YAML: {error}")))?;
    let fields = map(&value, "record draft")?;
    let kind = Kind::parse(required(fields, "kind", "record draft")?)?;
    if creating && kind == Kind::World {
        return Err(invalid(
            "world records are update-only; use init to create a world",
        ));
    }
    for key in fields.keys() {
        let key = text(key, "record draft field name")?;
        if !kind.accepts(key, creating) {
            return Err(invalid(format!(
                "unknown or forbidden {} draft field {key:?}",
                kind.name()
            )));
        }
    }
    if creating {
        record_reference(
            required(fields, "id", "record draft")?,
            Some(kind.name()),
            "id",
        )?;
        text(required(fields, "title", "record draft")?, "title")?;
        let label = if kind.generational() {
            "generational draft".into()
        } else if matches!(kind, Kind::Scene | Kind::Conversation) {
            "record draft".into()
        } else {
            format!("{} draft", kind.name())
        };
        for key in kind.create_required() {
            required(fields, key, &label)?;
        }
    }
    for (key, value) in fields {
        let key = key.as_str().expect("field keys checked");
        let nullable_history = matches!(
            kind,
            Kind::Organization
                | Kind::Affiliation
                | Kind::Legacy
                | Kind::Tenure
                | Kind::Claim
                | Kind::VitalHistory
        ) && matches!(key, "initialization" | "transitions");
        let nullable_assertion =
            kind == Kind::Knowledge && key == "claim" && value.get("genealogy").is_some();
        if kind == Kind::Conversation && key == "turns" {
            supplied_turn_literals(value)?;
        } else {
            supplied_literals(
                value,
                key == "chronology"
                    || key == "spatial_history"
                    || key == "profiles"
                    || key == "predicates"
                    || key == "initial_state"
                    || key == "effects"
                    || (kind == Kind::Environment && matches!(key, "conditions" | "time"))
                    || (kind == Kind::Relationship && matches!(key, "inverse" | "transitions"))
                    || (kind == Kind::StoryPoint && key == "when")
                    || (kind == Kind::Hypothesis && matches!(key, "placement" | "resolution"))
                    || (kind == Kind::LoreConcept && key == "values")
                    || (kind == Kind::Assertion && key == "value")
                    || nullable_history
                    || nullable_assertion,
            )?;
        }
        match key {
            "statement" | "subjects" | "alternatives" | "context" | "placement" | "resolution"
                if kind == Kind::Hypothesis =>
            {
                crate::hypotheses::validate_request_field(key, value)
                    .map_err(|error| invalid(error.to_string()))?;
            }
            "timelines" => declarations(world, value, false)?,
            "default_timeline" => {
                text(value, key)?;
            }
            "capabilities" => {
                unique_strings(value, key)?;
                let mut previous = None;
                for value in list(value, key)? {
                    let name = text(value, key)?;
                    let index = validation::CAPABILITIES
                        .iter()
                        .position(|known| *known == name)
                        .ok_or_else(|| invalid(format!("unknown capability {name}")))?;
                    if previous.is_some_and(|previous| previous >= index) {
                        return Err(invalid("capabilities must be in registry order"));
                    }
                    previous = Some(index);
                }
            }
            "threads" if kind == Kind::World => declarations(world, value, true)?,
            "threads" => {
                ordered_strings(value, key)?;
                for value in list(value, key)? {
                    if !crate::core_validation::thread_id(value.as_str().unwrap()) {
                        return Err(invalid("threads require exact native thread IDs"));
                    }
                }
            }
            "relationship_metrics" => crate::core_validation::validate_metrics_request(value)
                .map_err(|error| invalid(error.to_string()))?,
            "section_audiences" => section_audiences(kind, value)?,
            "object_affordances" => {
                for value in list(value, key)? {
                    // These are authored labels, not a set or inferred actions.
                    string(value, key)?;
                }
            }
            "targets" => {
                unique_strings(value, key)?;
                if list(value, key)?.is_empty() {
                    return Err(invalid("environment targets must not be empty"));
                }
                for value in list(value, key)? {
                    record_reference(value, None, key)?;
                }
            }
            "conditions" => environment_conditions(value)?,
            "sensory" => unique_strings(value, key)?,
            "relationship_kind" | "inverse" => {
                crate::core_validation::validate_relationship_field(world, key, value)
                    .map_err(|error| invalid(error.to_string()))?;
            }
            "activation_policy" => {
                if !["manual", "suggest"].contains(&text(value, key)?) {
                    return Err(invalid("activation_policy must be manual or suggest"));
                }
            }
            "lifecycle" if kind == Kind::StoryPoint => lifecycle(world, value)?,
            "requires" => requirements(value)?,
            "when" => condition(value, 0, &mut 0)?,
            "priority" => {
                if !matches!(value, Value::Number(number) if !number.is_f64() && number.as_i64().is_some_and(|v| i32::try_from(v).is_ok()))
                {
                    return Err(invalid("priority must be a signed 32-bit integer"));
                }
            }
            "profiles" => {
                crate::lore_types::Registry::parse(value)
                    .map_err(|error| invalid(error.to_string()))?;
            }
            "predicates" => crate::assertion::validate_predicates_request(value)
                .map_err(|error| invalid(error.to_string()))?,
            "descriptions" => crate::description::validate_request(value)
                .map_err(|error| invalid(error.to_string()))?,
            "scope_ref" => crate::briefs::validate_reference_request(value)
                .map_err(|error| invalid(error.to_string()))?,
            "required_refs" => crate::briefs::validate_refs_request(value)
                .map_err(|error| invalid(error.to_string()))?,
            "scope" => {
                if !["world", "region", "era"].contains(&text(value, key)?) {
                    return Err(invalid("brief scope must be world, region or era"));
                }
            }
            "evidence" | "start_evidence" | "end_evidence" => {
                crate::assertion::validate_evidence_request(value)
                    .map_err(|error| invalid(error.to_string()))?
            }
            "episode_kind" => {
                if !crate::existence::EPISODE_KINDS.contains(&text(value, key)?) {
                    return Err(invalid("unknown existence episode kind"));
                }
            }
            "subject" | "attributed_to" => {
                let fields = closed(value, &["kind", "id"], &[], key)?;
                if text(&fields["kind"], key)? != "record" {
                    return Err(invalid(format!("{key} must be a typed record reference")));
                }
                record_reference(&fields["id"], None, &format!("{key}.id"))?;
            }
            "predicate" => {
                if !crate::lore_types::token(text(value, key)?) {
                    return Err(invalid("predicate must be an exact lowercase predicate ID"));
                }
            }
            "classification" => {
                if !["author-fact", "attributed-claim", "unresolved"].contains(&text(value, key)?) {
                    return Err(invalid("unknown assertion classification"));
                }
            }
            "replaces" => record_reference(value, Some("assertion"), key)?,
            "value" => crate::lore_types::bounded(value, key)
                .map_err(|error| invalid(error.to_string()))?,
            "current_time" => {
                closed(value, &["timeline", "tick", "order"], &[], key)?;
                point(world, value, key)?;
            }
            "state_keys" => state_keys(value)?,
            "state_profile_bindings" => crate::lore_state::validate_request(value)
                .map_err(|error| invalid(error.to_string()))?,
            "concept_links" => crate::lore::validate_links_request(value)
                .map_err(|error| invalid(error.to_string()))?,
            "domain" | "profile" => {
                text(value, key)?;
            }
            "values" => {
                map(value, key)?;
                crate::lore_types::bounded(value, key)
                    .map_err(|error| invalid(error.to_string()))?;
            }
            "provenance" => {
                string(value, key)?;
            }
            "chronology" => {
                if kind == Kind::World {
                    map(value, "chronology")?;
                } else {
                    list(value, "chronology")?;
                }
                // Whole-world candidate validation checks declarations and all
                // annotation references together. Invalid semantics remain
                // reviewable without publishing an invalid record.
            }
            "title" => {
                text(value, "title")?;
            }
            "aliases" | "tags" => unique_strings(value, key)?,
            "initial_state" => initial_state(world, kind, value)?,
            "role" => {
                string(value, "role")?;
            }
            "pronouns" => {
                let fields = closed(value, &["subject", "object"], &["possessive"], "pronouns")?;
                for (key, value) in fields {
                    text(value, &format!("pronouns.{}", key.as_str().unwrap()))?;
                }
            }
            "status" => {
                if !kind.statuses().contains(&text(value, "status")?) {
                    return Err(invalid(format!("invalid {} status", kind.name())));
                }
            }
            "time" if matches!(kind, Kind::Scene | Kind::Conversation | Kind::Environment) => {
                interval(world, kind, value)?
            }
            "time" => point(world, value, "time")?,
            "location" => record_reference(value, Some("location"), "location")?,
            "child_id" | "character_id" | "claimant_id" => {
                record_reference(value, Some("character"), key)?
            }
            "parent_id" if kind == Kind::Parentage => {
                record_reference(value, Some("character"), key)?;
            }
            "participant_ids" => references(value, "character", key)?,
            "parent_id" if kind == Kind::Organization => {
                record_reference(value, Some("organization"), key)?;
            }
            "organization_id" => record_reference(value, Some("organization"), key)?,
            "legacy_id" => record_reference(value, Some("legacy"), key)?,
            "predecessor_tenure_id" | "successor_tenure_id" => {
                record_reference(value, Some("tenure"), key)?
            }
            "parent_id" | "from_location_id" | "to_location_id" | "location_id" => {
                record_reference(value, Some("location"), key)?;
            }
            "organization_kind" | "legacy_kind" | "disclosure" => {
                // The domain validator owns the closed vocabulary so semantic
                // failures remain visible in a non-publishable preview.
                text(value, key)?;
            }
            "spatial" => placement(value)?,
            "spatial_history" => {
                crate::spatial_history::validate_request(value)
                    .map_err(|error| invalid(error.to_string()))?;
            }
            "crs" => {
                let crs = text(value, key)?;
                if crs != "EPSG:4326"
                    && !crs
                        .strip_prefix("local-planar:")
                        .is_some_and(|name| !name.trim().is_empty())
                {
                    return Err(invalid("crs must be local-planar:NAME or EPSG:4326"));
                }
            }
            "axis_order" => {
                let axes = list(value, key)?;
                if axes.len() != 2
                    || !matches!(
                        (axes[0].as_str(), axes[1].as_str()),
                        (Some("east"), Some("north")) | (Some("longitude"), Some("latitude"))
                    )
                {
                    return Err(invalid(
                        "axis_order must be [east, north] or [longitude, latitude]",
                    ));
                }
            }
            "unit" => {
                text(value, key)?;
            }
            "bounds" => map_bounds(value)?,
            "z_policy" => {
                if !["forbidden", "optional-level", "required"].contains(&text(value, key)?) {
                    return Err(invalid(
                        "z_policy must be forbidden, optional-level, or required",
                    ));
                }
            }
            "origin" => {
                let fields = closed(value, &["label", "coordinates"], &[], key)?;
                text(&fields["label"], "origin.label")?;
                coordinates(&fields["coordinates"], "origin.coordinates")?;
            }
            "scale" => route_metric(value, key)?,
            "from" | "to" if kind == Kind::Relationship => {
                record_reference(value, Some("character"), key)?
            }
            "from" => map_position(value, key)?,
            "to" if kind == Kind::Portal && matches!(value, Value::String(_)) => {
                record_reference(value, Some("location"), key)?;
            }
            "to" => map_position(value, key)?,
            "conversion" => {
                string(value, key)?;
            }
            "lifecycle" => {
                if !["static", "time-bounded"].contains(&text(value, key)?) {
                    return Err(invalid("overlay lifecycle must be static or time-bounded"));
                }
            }
            "membership" => {
                let fields = closed(value, &["location_ids"], &[], key)?;
                references(
                    &fields["location_ids"],
                    "location",
                    "membership.location_ids",
                )?;
                ordered_strings(&fields["location_ids"], "membership.location_ids")?;
            }
            "audience" | "perspectives"
                if matches!(
                    kind,
                    Kind::Overlay
                        | Kind::LoreConcept
                        | Kind::Assertion
                        | Kind::ExistenceEpisode
                        | Kind::ContextBrief
                ) =>
            {
                ordered_strings(value, key)?
            }
            "audience" | "perspectives" if kind == Kind::Knowledge => unique_strings(value, key)?,
            "audience" | "perspectives" => {
                for value in list(value, key)? {
                    string(value, key)?;
                }
            }
            "valid"
                if matches!(
                    kind,
                    Kind::LoreConcept
                        | Kind::Assertion
                        | Kind::ExistenceEpisode
                        | Kind::ContextBrief
                ) =>
            {
                crate::lore::validate_valid_request(value)
                    .map_err(|error| invalid(error.to_string()))?;
            }
            "valid" => {
                let fields = closed(value, &["start", "end"], &[], key)?;
                for name in ["start", "end"] {
                    let label = format!("valid.{name}");
                    closed(&fields[name], &["timeline", "tick", "order"], &[], &label)?;
                    point(world, &fields[name], &label)?;
                }
            }
            "direction" => {
                if !["one-way", "two-way"].contains(&text(value, key)?) {
                    return Err(invalid("direction must be one-way or two-way"));
                }
            }
            "modes" => {
                unique_strings(value, key)?;
                if kind == Kind::Portal {
                    ordered_strings(value, key)?;
                }
                if list(value, key)?.is_empty() {
                    return Err(invalid("modes must not be empty"));
                }
            }
            "availability" => {
                if !["open", "closed", "restricted", "unknown"].contains(&text(value, key)?) {
                    return Err(invalid(
                        "availability must be open, closed, restricted, or unknown",
                    ));
                }
            }
            "uncertainty" => {
                if !["exact", "estimated", "unknown"].contains(&text(value, key)?) {
                    return Err(invalid("uncertainty must be exact, estimated, or unknown"));
                }
            }
            "route_distance" | "travel_cost" | "duration" => route_metric(value, key)?,
            "scene" => record_reference(value, Some("scene"), "scene")?,
            "participants" if kind == Kind::Event => participants(value)?,
            "participants" => timed_participants(world, value)?,
            "objects" => references(value, "object", key)?,
            "environments" => references(value, "environment", key)?,
            "conversations" => references(value, "conversation", key)?,
            "story_points" | "related_story_points" => references(value, "story-point", key)?,
            "outcome_events" => references(value, "event", key)?,
            "observations" => observations(world, value)?,
            "turns" => turns(world, value)?,
            "recollections" => recollections(world, value)?,
            "causes" => causes(value)?,
            "effects" => effects(world, value)?,
            "knower" => record_reference(value, Some("character"), "knower")?,
            "claim" => claim(value)?,
            "initialization" => {
                map(value, key)?;
            }
            "transitions" if kind.generational() => {
                list(value, key)?;
                // The generational validator owns the closed transition grammar,
                // references and lifecycle. Semantic failures remain reviewable
                // in a canApply:false candidate and cannot publish.
            }
            "transitions" if kind == Kind::Relationship => {
                crate::core_validation::validate_relationship_field(world, key, value)
                    .map_err(|error| invalid(error.to_string()))?;
            }
            "transitions" => transitions(world, value)?,
            "kind" | "id" | "body" => {}
            _ => unreachable!("closed draft fields"),
        }
    }
    let body = fields
        .get("body")
        .map(|value| string(value, "body").map(str::to_owned))
        .transpose()?;
    if body.as_ref().is_some_and(|body| body.len() > MAX_BYTES) {
        return Err(invalid("body exceeds 1 MiB"));
    }
    Ok(Draft {
        kind,
        fields: fields.clone(),
        body,
    })
}

fn serialize(fields: Mapping, body: &str, newline: &str, path: &str) -> Result<Record> {
    let yaml = serde_yaml::to_string(&Value::Mapping(fields))?;
    let header = yaml.replace('\n', newline);
    source::parse_record(
        format!("---{newline}{header}---{newline}{body}").as_bytes(),
        path,
    )
}

/// Create a draft with a deterministic safe path. Namespaced spatial IDs use
/// `<kind>-<full SHA256 of ID>.md`; portable prefixed IDs are used verbatim.
pub fn create(world: &World, request: &str) -> Result<Record> {
    let Draft { kind, fields, body } = draft(world, request, true)?;
    let id = text(required(&fields, "id", "record draft")?, "id")?;
    if world.records.contains_key(id) {
        return Err(Error::Conflict(format!("record ID {id:?} already exists")));
    }
    let stem = if matches!(
        kind,
        Kind::Location | Kind::Route | Kind::Map | Kind::Anchor | Kind::Portal | Kind::Overlay
    ) && id.contains(':')
    {
        format!(
            "{}-{}",
            kind.name(),
            hex::encode(Sha256::digest(id.as_bytes()))
        )
    } else {
        id.into()
    };
    let path = format!("story/{}/{stem}.md", kind.directory());
    if world
        .records
        .values()
        .any(|record| record.source_path.to_lowercase() == path.to_lowercase())
    {
        return Err(Error::Conflict(format!(
            "source path {path:?} already belongs to a record"
        )));
    }
    let mut metadata = Mapping::new();
    metadata.insert("schema".into(), SOURCE_SCHEMA.into());
    metadata.insert("kind".into(), kind.name().into());
    metadata.insert("id".into(), id.into());
    metadata.extend(kind.create_defaults());
    for (key, value) in fields {
        if key.as_str() != Some("body") {
            metadata.insert(key, value);
        }
    }
    serialize(metadata, body.as_deref().unwrap_or(""), "\n", &path)
}

/// Partial means top-level omission: supplied arrays/maps replace whole values.
/// No-op updates retain exact bytes, including header comments and formatting.
pub fn update(world: &World, entity: &str, request: &str) -> Result<Record> {
    let Draft { kind, fields, body } = draft(world, request, false)?;
    let existing = world.resolve(entity)?;
    if existing.kind != kind.name() {
        return Err(invalid("patch kind must match the existing record kind"));
    }
    let parsed = source::parse_record(&existing.raw_bytes, &existing.source_path)?;
    if parsed.id != existing.id
        || parsed.kind != existing.kind
        || parsed.title != existing.title
        || parsed.body != existing.body
        || parsed.frontmatter != existing.frontmatter
    {
        return Err(Error::Validation(
            "existing record fields differ from their source bytes".into(),
        ));
    }
    let mut metadata = map(&existing.frontmatter, "existing frontmatter")?.clone();
    let mut structured_change = false;
    for (key, value) in fields {
        if ["kind", "body"].contains(&key.as_str().expect("field keys checked")) {
            continue;
        }
        if metadata
            .get(&key)
            .is_none_or(|previous| !state_schema::values_equal(previous, &value))
        {
            metadata.insert(key, value);
            structured_change = true;
        }
    }
    let body = body.as_deref().unwrap_or(&existing.body);
    if !structured_change && body == existing.body {
        return Ok(existing.clone());
    }
    if !structured_change {
        let header_end = existing
            .raw_bytes
            .len()
            .checked_sub(existing.body.len())
            .ok_or_else(|| Error::Data("invalid source body boundary".into()))?;
        let mut bytes = existing.raw_bytes[..header_end].to_vec();
        bytes.extend_from_slice(body.as_bytes());
        return source::parse_record(&bytes, &existing.source_path);
    }
    let newline = if existing.raw_bytes.starts_with(b"---\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    serialize(metadata, body, newline, &existing.source_path)
}
