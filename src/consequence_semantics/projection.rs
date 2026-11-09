//! Author-local semantic values at one exact horizon.
//!
//! The original, validated World is passed to established native folds. It is
//! not replaced with an artificial World missing future dependency records.
//! Admission follows each native family: canonical event effects, knowledge
//! and story-points; relationship history and state owners retain their native
//! status behavior. Timed owners are available from their authored start,
//! including after an interval ends. This is not an active-presence query.
use crate::{
    Error, Result, lore,
    model::{Record, StoryTime, World},
    runtime, service, state_schema,
    state_schema::{StateDefinition, StateType},
    story,
};
use serde::Serialize;
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
};

pub(crate) const MAX_SUBJECTS: usize = 1024;
const MAX_SOURCE_RECORDS: usize = 4096;
const MAX_SOURCE_BYTES: usize = 32 * 1024 * 1024;
const MAX_SOURCE_NODES: usize = 100_000;
const MAX_FOLD_WORK: usize = 2_000_000;
const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind")]
pub(crate) enum Subject {
    #[serde(rename = "state")]
    State {
        #[serde(rename = "entityId")]
        entity_id: String,
        key: String,
    },
    #[serde(rename = "knowledge")]
    Knowledge {
        #[serde(rename = "knowledgeId")]
        knowledge_id: String,
    },
    #[serde(rename = "relationship")]
    Relationship {
        #[serde(rename = "relationshipId")]
        relationship_id: String,
    },
    #[serde(rename = "story-point")]
    StoryPoint {
        #[serde(rename = "storyPointId")]
        story_point_id: String,
    },
    #[serde(rename = "outcome")]
    Outcome {
        #[serde(rename = "eventId")]
        event_id: String,
        #[serde(rename = "targetId")]
        target_id: String,
    },
}

impl Subject {
    pub(crate) fn owner(&self) -> &str {
        match self {
            Self::State { entity_id, .. } => entity_id,
            Self::Knowledge { knowledge_id } => knowledge_id,
            Self::Relationship { relationship_id } => relationship_id,
            Self::StoryPoint { story_point_id } => story_point_id,
            Self::Outcome { event_id, .. } => event_id,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct StateValue {
    pub definition: StateDefinition,
    pub value: Option<Yaml>,
}

#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    pub payload: Json,
    pub citations: Vec<Json>,
    pub state: Option<StateValue>,
    // Numeric metrics/confidence are compared before browser-safe conversion.
    comparison: Option<Yaml>,
}

impl Snapshot {
    pub fn wire(&self) -> Json {
        json!({
            "presence":if self.payload.is_null() {"absent"} else {"present"},
            "payload":self.payload,"citations":self.citations,
        })
    }

    pub(crate) fn equal(&self, other: &Self) -> bool {
        match (&self.state, &other.state) {
            (Some(left), Some(right)) => match (&left.value, &right.value) {
                (None, None) => true,
                (Some(left_value), Some(right_value)) => {
                    self.payload["valueType"] == other.payload["valueType"]
                        && self.payload["profileId"] == other.payload["profileId"]
                        && state_schema::values_equal_for_definition(
                            &left.definition,
                            left_value,
                            right_value,
                        )
                }
                _ => false,
            },
            (None, None) => match (&self.comparison, &other.comparison) {
                (Some(left), Some(right)) => numeric_values_equal(left, right),
                _ => self.payload == other.payload,
            },
            _ => false,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Projection {
    pub snapshots: BTreeMap<Subject, Snapshot>,
    bytes: usize,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

struct Size(usize);
impl Write for Size {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self.0.saturating_add(bytes.len());
        if self.0 > MAX_BYTES {
            return Err(std::io::Error::other("semantic byte bound exceeded"));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn size(value: &impl Serialize) -> Result<usize> {
    let mut writer = Size(0);
    serde_json::to_writer(&mut writer, value)
        .map_err(|_| invalid("complete semantic projection exceeds 8 MiB"))?;
    Ok(writer.0)
}

impl Projection {
    fn insert(&mut self, subject: Subject, snapshot: Snapshot) -> Result<()> {
        if self.snapshots.len() >= MAX_SUBJECTS {
            return Err(invalid(
                "complete semantic projection exceeds 1024 subjects",
            ));
        }
        self.bytes = self.bytes.saturating_add(size(
            &json!({"subject":subject,"snapshot":snapshot.wire()}),
        )?);
        if self.bytes > MAX_BYTES {
            return Err(invalid("complete semantic projection exceeds 8 MiB"));
        }
        if self.snapshots.insert(subject, snapshot).is_some() {
            return Err(invalid("duplicate semantic subject"));
        }
        Ok(())
    }
}

/// Bound input/estimated replay work before invoking the complete validator or
/// cloning semantic values. No source/cache/reader state is changed.
pub(crate) fn preflight(world: &World) -> Result<()> {
    if world.records.len() > MAX_SOURCE_RECORDS {
        return Err(invalid("semantic analysis source exceeds 4096 records"));
    }
    let mut bytes = 0usize;
    let mut nodes = 0usize;
    let mut members = 0usize;
    let mut folds = 1usize;
    for record in world.records.values() {
        bytes = bytes.saturating_add(record.raw_bytes.len());
        if bytes > MAX_SOURCE_BYTES {
            return Err(invalid("semantic analysis source exceeds 32 MiB"));
        }
        if matches!(
            record.kind.as_str(),
            "character" | "object" | "location" | "environment" | "story-point"
        ) {
            folds = folds.saturating_add(1);
        }
        for field in ["effects", "transitions"] {
            members = members.saturating_add(record.list(field).map_or(0, Vec::len));
        }
        members = members.saturating_add(
            record
                .get("lifecycle")
                .and_then(|v| v.get("transitions"))
                .and_then(Yaml::as_sequence)
                .map_or(0, Vec::len),
        );
        let mut pending = vec![(&record.frontmatter, 0usize)];
        while let Some((value, depth)) = pending.pop() {
            nodes = nodes.saturating_add(1);
            if nodes > MAX_SOURCE_NODES || depth > 64 {
                return Err(invalid(
                    "semantic analysis source exceeds 100000 nodes or 64 levels",
                ));
            }
            match value {
                Yaml::Mapping(values) => {
                    pending.extend(
                        values
                            .iter()
                            .flat_map(|(key, value)| [(key, depth + 1), (value, depth + 1)]),
                    );
                }
                Yaml::Sequence(values) => {
                    pending.extend(values.iter().map(|value| (value, depth + 1)));
                }
                Yaml::Tagged(value) => pending.push((&value.value, depth + 1)),
                _ => {}
            }
        }
    }
    if folds.saturating_mul(world.records.len().saturating_add(members)) > MAX_FOLD_WORK {
        return Err(invalid(
            "semantic analysis exceeds 2000000 estimated fold steps",
        ));
    }
    Ok(())
}

pub(crate) fn horizon(world: &World, at: &StoryTime) -> Result<()> {
    world
        .story_time(at.tick, Some(&at.timeline), at.order)
        .map(|_| ())
}

fn time(world: &World, value: &Yaml) -> Result<StoryTime> {
    let point = StoryTime::from_yaml(value, &world.default_timeline())?;
    horizon(world, &point)?;
    Ok(point)
}

fn latest<'a>(
    world: &World,
    record: &'a Record,
    at: &StoryTime,
) -> Result<Option<(usize, &'a Yaml, StoryTime)>> {
    let mut latest: Option<(usize, &Yaml, StoryTime)> = None;
    for (ordinal, member) in record.list("transitions").into_iter().flatten().enumerate() {
        let point = time(
            world,
            member
                .get("time")
                .ok_or_else(|| invalid("semantic transition has no time"))?,
        )?;
        if point.not_after(at)
            && latest.as_ref().is_none_or(|(old_index, _, old)| {
                (point.tick, point.order, ordinal) > (old.tick, old.order, *old_index)
            })
        {
            latest = Some((ordinal, member, point));
        }
    }
    Ok(latest)
}

/// Family admission is deliberately distinct from character visibility. State
/// owners and relationships follow the existing native status behavior. Static
/// scene outcome links remain inspectable for any valid authored scene status.
pub(crate) fn admitted(world: &World, record: &Record, at: &StoryTime) -> Result<bool> {
    match record.kind.as_str() {
        "event" => Ok(record.status() == "canonical"
            && time(
                world,
                record
                    .get("time")
                    .ok_or_else(|| invalid("semantic event has no time"))?,
            )?
            .not_after(at)),
        "scene" | "conversation" | "environment" => {
            let start = record
                .get("time")
                .and_then(|value| value.get("start"))
                .ok_or_else(|| invalid("semantic timed owner has no start"))?;
            Ok(time(world, start)?.not_after(at))
        }
        "knowledge" => Ok(record.status() == "canonical" && latest(world, record, at)?.is_some()),
        "relationship" => Ok(latest(world, record, at)?.is_some()),
        "story-point" => Ok(record.status() == "canonical"),
        _ => Ok(true),
    }
}

/// The pinned consequence projector withholds scalar entity effects before
/// replay when their endpoint is outside its temporal world. This narrow rule
/// does not reinterpret initial values, sets, bound profiles or literal strings.
fn scalar_effect_admitted(
    world: &World,
    owner: &Record,
    effect: &Yaml,
    at: &StoryTime,
) -> Result<bool> {
    let Some(value) = effect.get("value") else {
        return Ok(true);
    };
    let key = effect
        .get("key")
        .and_then(Yaml::as_str)
        .ok_or_else(|| invalid("semantic effect has no state key"))?;
    let definition = state_schema::field_definition(world, &owner.kind, key)?;
    if definition.profile.is_some() || definition.value_type != StateType::Entity {
        return Ok(true);
    }
    let id = value
        .get("entity")
        .and_then(Yaml::as_str)
        .ok_or_else(|| invalid("semantic scalar entity effect is malformed"))?;
    match world.records.get(id) {
        Some(record) => admitted(world, record, at),
        None => Ok(false),
    }
}

fn citation(world: &World, record: &Record, section: &str) -> Json {
    json!({"entityId":record.id,"sourcePath":record.source_path,
        "blobOid":record.blob_oid,"revision":world.revision,
        "fingerprint":world.fingerprint,"section":section})
}

fn state_payload(
    definition: &StateDefinition,
    value: &Yaml,
    all_ids: &BTreeSet<String>,
) -> Result<Json> {
    let value_raw = serde_json::to_string(value)?;
    if let Some(profile) = &definition.profile {
        // Bound profiles retain the complete native local-author value.
        // Scalar entity-effect pruning does not change their typed projection.
        let (value, shape) = lore::project_value(profile, value, all_ids)?;
        return Ok(
            json!({"value":value,"valueType":shape,"profileId":profile.id,
            "valueRaw":value_raw}),
        );
    }
    let name = match definition.value_type {
        StateType::String => "string",
        StateType::Boolean => "boolean",
        StateType::Integer => "integer",
        StateType::Number => "number",
        StateType::Entity => "entity",
        StateType::StringSet => "string-set",
        StateType::EntitySet => "entity-set",
    };
    let mut shape = json!({"type":name});
    if let Some(kind) = &definition.kind {
        shape["kind"] = json!(kind);
    }
    Ok(json!({"value":service::yaml_json(value)?,"valueType":shape,"valueRaw":value_raw}))
}

fn states(world: &World, at: &StoryTime, projection: &mut Projection) -> Result<()> {
    let declarations = world.world_record()?.get("state_keys");
    let all_ids = world.records.keys().cloned().collect();
    for record in world.records.values().filter(|record| {
        matches!(
            record.kind.as_str(),
            "character" | "object" | "location" | "environment"
        )
    }) {
        if !admitted(world, record, at)? {
            continue;
        }
        let Some(keys) = declarations
            .and_then(|value| value.get(&record.kind))
            .and_then(Yaml::as_mapping)
        else {
            continue;
        };
        if projection.snapshots.len().saturating_add(keys.len()) > MAX_SUBJECTS {
            return Err(invalid(
                "complete semantic projection exceeds 1024 subjects",
            ));
        }
        let (values, provenance) =
            runtime::replay_state_with_effects(world, &record.id, at, |effect| {
                scalar_effect_admitted(world, record, effect, at)
            })?;
        for key in keys.keys() {
            let key = key
                .as_str()
                .ok_or_else(|| invalid("semantic state key must be a string"))?;
            let definition = state_schema::field_definition(world, &record.kind, key)?;
            let value = values.get(key).cloned();
            let payload = value
                .as_ref()
                .map(|value| state_payload(&definition, value, &all_ids))
                .transpose()?
                .unwrap_or(Json::Null);
            let mut citations = Vec::new();
            if let Some(source) = provenance.get(key) {
                let mut source = source.clone();
                if source["section"] == "initial_state" {
                    source["section"] = json!(format!("initial_state.{key}"));
                }
                citations.push(source);
            }
            if value.is_none() && citations.is_empty() {
                citations.push(citation(world, record, "initial_state"));
            }
            projection.insert(
                Subject::State {
                    entity_id: record.id.clone(),
                    key: key.into(),
                },
                Snapshot {
                    payload,
                    citations,
                    state: Some(StateValue { definition, value }),
                    comparison: None,
                },
            )?;
        }
    }
    Ok(())
}

fn selected(item: &Json, keys: &[&str]) -> Json {
    Json::Object(
        keys.iter()
            .map(|key| ((*key).into(), item[*key].clone()))
            .collect(),
    )
}

fn histories(world: &World, at: &StoryTime, projection: &mut Projection) -> Result<()> {
    let mut knowers = BTreeSet::new();
    let mut relationship_owners = BTreeSet::new();
    for record in world.records.values() {
        match record.kind.as_str() {
            "knowledge" if record.status() == "canonical" => {
                if let Some(knower) = record.string("knower") {
                    knowers.insert(knower);
                }
            }
            "relationship" => {
                if let Some(owner) = record.string("from") {
                    relationship_owners.insert(owner);
                }
            }
            _ => {}
        }
    }
    for knower in knowers {
        let result = runtime::knowledge(world, knower, at, true)?;
        for item in result["items"].as_array().into_iter().flatten() {
            let id = item["knowledgeId"]
                .as_str()
                .ok_or_else(|| invalid("semantic knowledge identity is missing"))?;
            let record = &world.records[id];
            let Some((_, member, _)) = latest(world, record, at)? else {
                continue;
            };
            let mut payload = selected(item, &["state", "confidence", "transitionId", "time"]);
            let confidence = member.get("confidence").cloned().unwrap_or(Yaml::Null);
            let mut comparison = serde_yaml::to_value(&payload)?;
            comparison["confidence"] = confidence.clone();
            payload["confidence"] = service::yaml_json(&confidence)?;
            projection.insert(
                Subject::Knowledge {
                    knowledge_id: id.into(),
                },
                Snapshot {
                    payload,
                    citations: vec![item["citation"].clone()],
                    state: None,
                    comparison: Some(comparison),
                },
            )?;
        }
    }
    for owner in relationship_owners {
        let result = runtime::relationships(world, owner, at)?;
        for item in result["items"].as_array().into_iter().flatten() {
            let id = item["relationshipId"]
                .as_str()
                .ok_or_else(|| invalid("semantic relationship identity is missing"))?;
            let record = &world.records[id];
            let Some((_, member, _)) = latest(world, record, at)? else {
                continue;
            };
            let mut payload = selected(
                item,
                &["status", "metrics", "facets", "transitionId", "time"],
            );
            let metrics = member
                .get("metrics")
                .cloned()
                .unwrap_or_else(|| Yaml::Mapping(Default::default()));
            let mut comparison = serde_yaml::to_value(&payload)?;
            comparison["metrics"] = metrics.clone();
            payload["metrics"] = service::yaml_json(&metrics)?;
            projection.insert(
                Subject::Relationship {
                    relationship_id: id.into(),
                },
                Snapshot {
                    payload,
                    citations: vec![item["citation"].clone()],
                    state: None,
                    comparison: Some(comparison),
                },
            )?;
        }
    }
    Ok(())
}

fn story_points(world: &World, at: &StoryTime, projection: &mut Projection) -> Result<()> {
    let mut state: BTreeMap<String, BTreeMap<String, Yaml>> = BTreeMap::new();
    for (subject, snapshot) in &projection.snapshots {
        if let Subject::State { entity_id, key } = subject {
            // Keep a present empty owner entry when all its values are absent:
            // otherwise a fallback fold could reintroduce a withheld effect.
            let values = state.entry(entity_id.clone()).or_default();
            if let Some(value) = snapshot
                .state
                .as_ref()
                .and_then(|state| state.value.as_ref())
            {
                values.insert(key.clone(), value.clone());
            }
        }
    }
    let result = story::story_points_with_state(world, at, Some(&state))?;
    for item in result["items"].as_array().into_iter().flatten() {
        let id = item["storyPointId"]
            .as_str()
            .ok_or_else(|| invalid("semantic story-point identity is missing"))?;
        let mut source = item["citation"].clone();
        source["transitionId"] = item["transitionId"].clone();
        source["time"] = item["transitionTime"].clone();
        projection.insert(
            Subject::StoryPoint {
                story_point_id: id.into(),
            },
            Snapshot {
                payload: selected(
                    item,
                    &[
                        "storedState",
                        "eligible",
                        "requirementsSatisfied",
                        "conditionSatisfied",
                    ],
                ),
                citations: vec![source],
                state: None,
                comparison: None,
            },
        )?;
    }
    Ok(())
}

/// Return false link flags for an admitted but unlinked pair; absence from the
/// sparse projection map alone is not proof that either endpoint is unknown.
pub(crate) fn outcome(
    world: &World,
    event_id: &str,
    target_id: &str,
    at: &StoryTime,
) -> Result<Option<Snapshot>> {
    let (Some(event), Some(target)) = (world.records.get(event_id), world.records.get(target_id))
    else {
        return Ok(None);
    };
    if event.kind != "event"
        || !matches!(target.kind.as_str(), "story-point" | "scene")
        || !admitted(world, event, at)?
        || !admitted(world, target, at)?
    {
        return Ok(None);
    }
    let mut citations = Vec::new();
    let target_link = target
        .list("outcome_events")
        .into_iter()
        .flatten()
        .position(|value| value.as_str() == Some(event_id));
    let event_link = if target.kind == "story-point" {
        event
            .list("related_story_points")
            .into_iter()
            .flatten()
            .position(|value| value.as_str() == Some(target_id))
    } else {
        None
    };
    citations.push(citation(
        world,
        target,
        &target_link.map_or_else(
            || "outcome_events".into(),
            |index| format!("outcome_events[{index}]"),
        ),
    ));
    if target.kind == "story-point" {
        citations.push(citation(
            world,
            event,
            &event_link.map_or_else(
                || "related_story_points".into(),
                |index| format!("related_story_points[{index}]"),
            ),
        ));
    }
    Ok(Some(Snapshot {
        payload: json!({"targetKind":target.kind,"targetLinked":target_link.is_some(),
            "eventLinked":event_link.is_some()}),
        citations,
        state: None,
        comparison: None,
    }))
}

fn outcomes(world: &World, at: &StoryTime, projection: &mut Projection) -> Result<()> {
    let mut pairs = BTreeSet::new();
    for record in world.records.values() {
        let field = match record.kind.as_str() {
            "event" => "related_story_points",
            "scene" | "story-point" => "outcome_events",
            _ => continue,
        };
        if !admitted(world, record, at)? {
            continue;
        }
        for value in record.list(field).into_iter().flatten() {
            let id = value
                .as_str()
                .ok_or_else(|| invalid("semantic outcome link must be an exact ID"))?;
            let (event, target) = if field == "related_story_points" {
                (record.id.as_str(), id)
            } else {
                (id, record.id.as_str())
            };
            if let Some(snapshot) = outcome(world, event, target, at)? {
                if pairs.insert((event.to_owned(), target.to_owned())) {
                    projection.insert(
                        Subject::Outcome {
                            event_id: event.into(),
                            target_id: target.into(),
                        },
                        snapshot,
                    )?;
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn project(world: &World, at: &StoryTime) -> Result<Projection> {
    preflight(world)?;
    horizon(world, at)?;
    let mut projection = Projection::default();
    states(world, at, &mut projection)?;
    histories(world, at, &mut projection)?;
    story_points(world, at, &mut projection)?;
    outcomes(world, at, &mut projection)?;
    Ok(projection)
}

fn number_equal(left: &serde_yaml::Number, right: &serde_yaml::Number) -> bool {
    let integer = |number: &serde_yaml::Number| {
        number
            .as_i64()
            .map(i128::from)
            .or_else(|| number.as_u64().map(i128::from))
    };
    match (integer(left), integer(right)) {
        (Some(left), Some(right)) => left == right,
        (Some(integer), None) => right.as_f64().is_some_and(|value| {
            value.is_finite() && value.fract() == 0.0 && value as i128 == integer
        }),
        (None, Some(integer)) => left.as_f64().is_some_and(|value| {
            value.is_finite() && value.fract() == 0.0 && value as i128 == integer
        }),
        (None, None) => left.as_f64() == right.as_f64(),
    }
}

fn numeric_values_equal(left: &Yaml, right: &Yaml) -> bool {
    match (left, right) {
        (Yaml::Number(left), Yaml::Number(right)) => number_equal(left, right),
        (Yaml::Sequence(left), Yaml::Sequence(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| numeric_values_equal(left, right))
        }
        (Yaml::Mapping(left), Yaml::Mapping(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, value)| {
                    right
                        .get(key)
                        .is_some_and(|other| numeric_values_equal(value, other))
                })
        }
        _ => state_schema::values_equal(left, right),
    }
}

pub(crate) fn boundary(
    world: &World,
    projection: &Projection,
    subject: &Subject,
    at: &StoryTime,
) -> Result<Json> {
    if let Some(snapshot) = projection.snapshots.get(subject) {
        return Ok(snapshot.wire());
    }
    let owner = world.records.get(subject.owner());
    let mut absent = owner.is_none();
    if let Some(owner) = owner {
        let complete_history = matches!(subject, Subject::Knowledge { .. })
            && owner.kind == "knowledge"
            && owner.status() == "canonical"
            || matches!(subject, Subject::Relationship { .. }) && owner.kind == "relationship";
        if complete_history && latest(world, owner, at)?.is_none() {
            // The complete native history was examined. No current authored
            // belief/relationship is distinct from an inaccessible owner.
            return Ok(json!({"presence":"absent","payload":Json::Null,
                "citations":[citation(world,owner,"transitions")]}));
        }
    }
    if let Subject::Outcome {
        event_id,
        target_id,
    } = subject
    {
        absent |= !world.records.contains_key(target_id);
        if !absent {
            if let Some(snapshot) = outcome(world, event_id, target_id, at)? {
                return Ok(snapshot.wire());
            }
        }
    }
    Ok(json!({"presence":if absent {"absent"} else {"unavailable"},
        "payload":Json::Null,"citations":[]}))
}

/// Compare projected semantics at H against projected semantics at the same H.
/// No difference is labelled as caused by an event or operation. Existing
/// source excluded from one horizon/canonical family is explicitly unavailable,
/// not a fabricated deletion. Complete known histories without an effective
/// transition have absent current semantics, not a predicted future value.
pub(crate) fn delta(base: &World, candidate: &World, at: &StoryTime) -> Result<Json> {
    if base.world_record()?.id != candidate.world_record()?.id {
        return Err(invalid(
            "semantic comparison requires the same exact world ID",
        ));
    }
    let before = project(base, at)?;
    let after = project(candidate, at)?;
    compare(base, &before, candidate, &after, at)
}

/// Compare already projected values, including internally suppressed event
/// contributions whose citation ordinals were restored to the original source.
pub(crate) fn compare(
    base: &World,
    before: &Projection,
    candidate: &World,
    after: &Projection,
    at: &StoryTime,
) -> Result<Json> {
    if base.world_record()?.id != candidate.world_record()?.id {
        return Err(invalid(
            "semantic comparison requires the same exact world ID",
        ));
    }
    let subjects: BTreeSet<_> = before
        .snapshots
        .keys()
        .chain(after.snapshots.keys())
        .cloned()
        .collect();
    if subjects.len() > MAX_SUBJECTS {
        return Err(invalid("complete semantic delta exceeds 1024 subjects"));
    }
    let mut changes = Vec::new();
    let mut incomparable = 0usize;
    for subject in &subjects {
        if before
            .snapshots
            .get(subject)
            .zip(after.snapshots.get(subject))
            .is_some_and(|(left, right)| left.equal(right))
        {
            continue;
        }
        let left = boundary(base, before, subject, at)?;
        let right = boundary(candidate, after, subject, at)?;
        if left["presence"] == right["presence"] && left["payload"] == right["payload"] {
            continue;
        }
        let comparable = left["presence"] != "unavailable" && right["presence"] != "unavailable";
        incomparable += usize::from(!comparable);
        changes
            .push(json!({"subject":subject,"before":left,"after":right,"comparable":comparable}));
    }
    let result = json!({"protocol":crate::CONTRACT,"operation":"semantic-delta",
        "scope":"local-author","comparison":"same-horizon","at":at,
        "baseFingerprint":base.fingerprint,"candidateFingerprint":candidate.fingerprint,
        "baseRevision":base.revision,"candidateRevision":candidate.revision,
        "subjectCount":subjects.len(),"changedSubjects":changes.len(),
        "incomparableSubjects":incomparable,"changes":changes,"complete":true,
        "writesPerformed":false,"canApply":false,
        "notice":"Same-horizon authored semantic differences; no inferred causal attribution or character admission."});
    size(&result)?;
    Ok(result)
}
