//! Pure local-author checks and same-horizon consequence comparisons.
//!
//! Expectations are explicit authored operands, never inferred effects or an
//! approval decision. Required checks gate a caller only when that caller binds
//! this complete analysis into its own reviewed request. No filesystem, cache,
//! character-admission or publication operation belongs to this module.
use crate::{
    Error, Result,
    model::{Record, StoryTime, World},
    state_schema, validation,
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use sha2::{Digest, Sha256};
use std::{cmp::Ordering, collections::BTreeMap, collections::BTreeSet, io::Write};

pub(crate) mod projection;

pub const MAX_EXPECTATIONS: usize = 100;
pub const MAX_ANALYSIS_BYTES: usize = 262_144;
pub const MAX_REQUEST_BYTES: usize = 524_288;
pub const MAX_REPORT_BYTES: usize = 8 * 1024 * 1024;
const MAX_OPERAND_BYTES: usize = 65_536;
const MAX_NUMERIC_BYTES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Time {
    pub timeline: String,
    pub tick: String,
    pub order: String,
}

impl Time {
    pub fn point(&self) -> Result<StoryTime> {
        if self.timeline.trim().is_empty() || self.timeline.len() > 128 {
            return Err(invalid(
                "semantic horizon requires a nonblank timeline of at most 128 bytes",
            ));
        }
        let tick = self
            .tick
            .parse::<i64>()
            .map_err(|_| invalid("semantic tick requires an exact signed 64-bit decimal string"))?;
        let order = self.order.parse::<i32>().map_err(|_| {
            invalid("semantic order requires an exact signed 32-bit decimal string")
        })?;
        if tick.to_string() != self.tick || order.to_string() != self.order {
            return Err(invalid("semantic time requires canonical decimal strings"));
        }
        StoryTime::new(self.timeline.clone(), tick, order)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Policy {
    Required,
    Advisory,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Analysis {
    pub at: Time,
    pub policy: Policy,
    pub expectations: Vec<Expectation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expectation {
    pub id: String,
    pub predicate: Predicate,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Predicate {
    #[serde(rename = "state.equals")]
    StateEquals {
        #[serde(rename = "targetId")]
        target_id: String,
        key: String,
        #[serde(rename = "expectedValueRaw")]
        expected_value_raw: String,
    },
    #[serde(rename = "state.absent")]
    StateAbsent {
        #[serde(rename = "targetId")]
        target_id: String,
        key: String,
    },
    #[serde(rename = "knowledge.state")]
    KnowledgeState {
        #[serde(rename = "knowledgeId")]
        knowledge_id: String,
        state: String,
    },
    #[serde(rename = "relationship.matches")]
    RelationshipMatches {
        #[serde(rename = "relationshipId")]
        relationship_id: String,
        values: RelationshipValues,
    },
    #[serde(rename = "story-point.state")]
    StoryPointState {
        #[serde(rename = "storyPointId")]
        story_point_id: String,
        state: String,
    },
    #[serde(rename = "outcome.linked")]
    OutcomeLinked {
        #[serde(rename = "eventId")]
        event_id: String,
        #[serde(rename = "targetId")]
        target_id: String,
    },
    #[serde(rename = "transition.caused")]
    TransitionCaused {
        #[serde(rename = "recordId")]
        record_id: String,
        #[serde(rename = "transitionId")]
        transition_id: String,
        #[serde(rename = "eventId")]
        event_id: String,
    },
}

fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    decoder: D,
) -> std::result::Result<Option<T>, D::Error> {
    T::deserialize(decoder).map(Some)
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipValues {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub status: Option<String>,
    /// Decimal JSON-number tokens, retained as strings to avoid browser rounding.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub metrics: Option<BTreeMap<String, String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub facets: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckRequest {
    pub expected_fingerprint: String,
    pub analysis: Analysis,
    pub max_characters: usize,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn text(value: &str, maximum: usize, label: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > maximum {
        return Err(invalid(format!(
            "{label} must be nonblank and at most {maximum} bytes"
        )));
    }
    Ok(())
}

fn identifier(value: &str) -> Result<()> {
    if !validation::valid_record_id(value) {
        return Err(invalid("semantic predicates require exact record IDs"));
    }
    Ok(())
}

fn bounded_value(value: &Yaml, depth: usize, nodes: &mut usize) -> Result<()> {
    *nodes = nodes.saturating_add(1);
    if depth > 32 || *nodes > 16_384 {
        return Err(invalid("semantic JSON exceeds complete structural bounds"));
    }
    match value {
        Yaml::Sequence(items) => {
            for item in items {
                bounded_value(item, depth + 1, nodes)?;
            }
        }
        Yaml::Mapping(items) => {
            for (key, item) in items {
                if !matches!(key, Yaml::String(_)) {
                    return Err(invalid("semantic JSON object keys must be strings"));
                }
                bounded_value(item, depth + 1, nodes)?;
            }
        }
        Yaml::Number(value) if !value.as_f64().is_some_and(f64::is_finite) => {
            return Err(invalid("semantic operands require finite numbers"));
        }
        Yaml::Tagged(_) => return Err(invalid("tagged semantic operands are unsupported")),
        _ => {}
    }
    Ok(())
}

fn json_value(payload: &str, maximum: usize) -> Result<Yaml> {
    if payload.len() > maximum {
        return Err(invalid("semantic JSON exceeds its complete byte bound"));
    }
    // YAML's value visitor catches repeated JSON keys before an ordinary JSON
    // map would erase them, including equivalent escaped key spellings.
    let mut decoder = serde_json::Deserializer::from_str(payload);
    let value = Yaml::deserialize(&mut decoder)
        .map_err(|_| invalid("invalid or duplicate semantic JSON"))?;
    decoder
        .end()
        .map_err(|_| invalid("invalid semantic JSON"))?;
    bounded_value(&value, 0, &mut 0)?;
    Ok(value)
}

fn operand(raw: &str) -> Result<Yaml> {
    let value = json_value(raw, MAX_OPERAND_BYTES)?;
    // The native schema distinguishes integer and floating values. Preserve
    // that distinction without accepting a decimal token that silently rounds
    // to a different authored number on its way into the typed comparator.
    let bytes = raw.as_bytes();
    let mut i = 0;
    let mut quoted = false;
    while i < bytes.len() {
        if quoted {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i] == b'"' {
                quoted = false;
            }
        } else if bytes[i] == b'"' {
            quoted = true;
        } else if bytes[i] == b'-' || bytes[i].is_ascii_digit() {
            let start = i;
            while bytes.get(i).is_some_and(|b| {
                b.is_ascii_digit() || matches!(b, b'-' | b'+' | b'.' | b'e' | b'E')
            }) {
                i += 1;
            }
            let token = &raw[start..i];
            let mut parser = serde_json::Deserializer::from_str(token);
            let native = Yaml::deserialize(&mut parser)
                .map_err(|_| invalid("state numeric operand is outside native bounds"))?;
            if Decimal::parse(token)? != Decimal::source(&native)? {
                return Err(invalid(
                    "state numeric operand loses precision in its native representation",
                ));
            }
            continue;
        }
        i += 1;
    }
    Ok(value)
}

struct LimitedWriter {
    bytes: Vec<u8>,
    maximum: usize,
}
impl Write for LimitedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.maximum.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other(
                "complete semantic JSON exceeds byte bound",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn encoded(value: &impl Serialize, maximum: usize) -> Result<Vec<u8>> {
    // A small fixed upper bound avoids an unbounded serializer allocation for
    // native callers constructing a request directly.
    let mut writer = LimitedWriter {
        bytes: Vec::new(),
        maximum,
    };
    serde_json::to_writer(&mut writer, value)
        .map_err(|_| invalid("complete semantic JSON exceeds byte bound"))?;
    Ok(writer.bytes)
}

fn digest(domain: &[u8], bytes: &[u8]) -> String {
    let mut value = Sha256::new();
    value.update(domain);
    value.update(bytes);
    hex::encode(value.finalize())
}

/// Validate shape and literal operands without resolving a historical analysis
/// against some later source snapshot.
pub fn validate_analysis(analysis: &Analysis) -> Result<()> {
    analysis.at.point()?;
    if analysis.expectations.len() > MAX_EXPECTATIONS {
        return Err(invalid(
            "semantic analysis permits at most 100 expectations",
        ));
    }
    encoded(analysis, MAX_ANALYSIS_BYTES)?;
    let mut ids = BTreeSet::new();
    for item in &analysis.expectations {
        text(&item.id, 128, "expectation ID")?;
        if !ids.insert(&item.id) {
            return Err(invalid("semantic expectation IDs must be unique"));
        }
        match &item.predicate {
            Predicate::StateEquals {
                target_id,
                key,
                expected_value_raw,
            } => {
                identifier(target_id)?;
                text(key, 256, "state key")?;
                operand(expected_value_raw)?;
            }
            Predicate::StateAbsent { target_id, key } => {
                identifier(target_id)?;
                text(key, 256, "state key")?;
            }
            Predicate::KnowledgeState {
                knowledge_id,
                state,
            } => {
                identifier(knowledge_id)?;
                if ![
                    "accepted",
                    "suspected",
                    "rejected",
                    "uncertain",
                    "remembered",
                    "forgotten",
                ]
                .contains(&state.as_str())
                {
                    return Err(invalid("unsupported expected knowledge state"));
                }
            }
            Predicate::StoryPointState {
                story_point_id,
                state,
            } => {
                identifier(story_point_id)?;
                if !["dormant", "active", "resolved", "failed", "cancelled"]
                    .contains(&state.as_str())
                {
                    return Err(invalid("unsupported expected story-point state"));
                }
            }
            Predicate::RelationshipMatches {
                relationship_id,
                values,
            } => {
                identifier(relationship_id)?;
                if values.status.is_none() && values.metrics.is_none() && values.facets.is_none() {
                    return Err(invalid(
                        "relationship expectation requires a selected value",
                    ));
                }
                if let Some(status) = &values.status {
                    text(status, 256, "relationship status")?;
                }
                if let Some(metrics) = &values.metrics {
                    if metrics.len() > 256 {
                        return Err(invalid(
                            "relationship expectation exceeds 256 metric operands",
                        ));
                    }
                    for (name, raw) in metrics {
                        text(name, 256, "relationship metric")?;
                        Decimal::parse(raw)?;
                        if !raw.parse::<f64>().is_ok_and(f64::is_finite) {
                            return Err(invalid("relationship metric operand must be finite"));
                        }
                    }
                }
                if let Some(facets) = &values.facets {
                    if facets.len() > 1024 || facets.iter().any(|v| v.len() > 4096) {
                        return Err(invalid(
                            "relationship facets exceed complete operand bounds",
                        ));
                    }
                }
            }
            Predicate::OutcomeLinked {
                event_id,
                target_id,
            } => {
                identifier(event_id)?;
                identifier(target_id)?;
            }
            Predicate::TransitionCaused {
                record_id,
                transition_id,
                event_id,
            } => {
                identifier(record_id)?;
                identifier(event_id)?;
                text(transition_id, 256, "transition ID")?;
            }
        }
    }
    Ok(())
}

pub fn decode_analysis(payload: &str) -> Result<Analysis> {
    json_value(payload, MAX_ANALYSIS_BYTES)?;
    let analysis = serde_json::from_str(payload)
        .map_err(|_| invalid("invalid, missing, duplicate or unknown semantic analysis fields"))?;
    validate_analysis(&analysis)?;
    Ok(analysis)
}

pub fn decode_check(payload: &str) -> Result<CheckRequest> {
    json_value(payload, MAX_REQUEST_BYTES)?;
    let request: CheckRequest = serde_json::from_str(payload).map_err(|_| {
        invalid("invalid, missing, duplicate or unknown semantic check request fields")
    })?;
    validate_request(&request)?;
    Ok(request)
}

fn validate_request(request: &CheckRequest) -> Result<()> {
    if !crate::action_drafts::valid_hash(&request.expected_fingerprint)
        || !(256..=1_000_000).contains(&request.max_characters)
    {
        return Err(invalid(
            "semantic check requires an exact source fingerprint and maxCharacters 256..1000000",
        ));
    }
    validate_analysis(&request.analysis)
}

fn exact<'a>(
    world: &'a World,
    id: &str,
    kinds: &[&str],
    at: &StoryTime,
) -> Result<Option<&'a Record>> {
    let Some(record) = world.records.get(id) else {
        return Ok(None);
    };
    if !kinds.contains(&record.kind.as_str()) {
        return Err(invalid(format!(
            "semantic reference {id} is {}, expected {}",
            record.kind,
            kinds.join("/")
        )));
    }
    Ok(projection::admitted(world, record, at)?.then_some(record))
}

enum Evaluated {
    Known {
        passed: bool,
        actual: Json,
        citations: Vec<Json>,
    },
    Unknown,
    Unsupported(&'static str),
}

fn known(passed: bool, actual: Json, citations: Vec<Json>) -> Evaluated {
    Evaluated::Known {
        passed,
        actual,
        citations,
    }
}

fn citation(world: &World, record: &Record, section: &str) -> Json {
    json!({"entityId":record.id,"sourcePath":record.source_path,"blobOid":record.blob_oid,
        "revision":world.revision,"fingerprint":world.fingerprint,"section":section})
}

fn state_check(
    world: &World,
    view: &projection::Projection,
    at: &StoryTime,
    target_id: &str,
    key: &str,
    expected: Option<&str>,
) -> Result<Evaluated> {
    let Some(record) = exact(
        world,
        target_id,
        &["character", "object", "location", "environment"],
        at,
    )?
    else {
        return Ok(Evaluated::Unknown);
    };
    let declared = world
        .world_record()?
        .get("state_keys")
        .and_then(|kinds| kinds.get(&record.kind))
        .and_then(|keys| keys.get(key));
    if declared.is_none() {
        return Ok(Evaluated::Unsupported("state key is not declared"));
    }
    let definition = state_schema::field_definition(world, &record.kind, key)?;
    let expected = expected.map(operand).transpose()?;
    if let Some(value) = &expected {
        state_schema::validate_value(world, &definition, value)
            .map_err(|error| invalid(format!("invalid expected state value: {error}")))?;
    }
    let subject = projection::Subject::State {
        entity_id: target_id.into(),
        key: key.into(),
    };
    let Some(snapshot) = view.snapshots.get(&subject) else {
        return Ok(Evaluated::Unknown);
    };
    let state = snapshot
        .state
        .as_ref()
        .ok_or_else(|| Error::Data("semantic state projection is missing typed data".into()))?;
    let observed = if let Some(value) = &state.value {
        json!({"presence":"present","valueRaw":serde_json::to_string(value)?,
            "value":crate::service::yaml_json(value)?,"valueType":snapshot.payload.get("valueType").cloned().unwrap_or(Json::Null)})
    } else {
        json!({"presence":"absent"})
    };
    let passed = match (&expected, &state.value) {
        (None, None) => true,
        (Some(expected), Some(actual)) => {
            state_schema::values_equal_for_definition(&state.definition, expected, actual)
        }
        _ => false,
    };
    Ok(known(
        passed,
        json!({"kind":"state","observed":observed}),
        snapshot.citations.clone(),
    ))
}

fn latest_transition<'a>(
    world: &World,
    record: &'a Record,
    at: &StoryTime,
) -> Result<Option<&'a Yaml>> {
    let mut latest: Option<(usize, StoryTime, &Yaml)> = None;
    for (ordinal, transition) in record.list("transitions").into_iter().flatten().enumerate() {
        let point = StoryTime::from_yaml(
            transition
                .get("time")
                .ok_or_else(|| Error::Validation("transition requires time".into()))?,
            &world.default_timeline(),
        )?;
        if point.not_after(at)
            && latest.as_ref().is_none_or(|(old_ordinal, old, _)| {
                (point.tick, point.order, ordinal) > (old.tick, old.order, *old_ordinal)
            })
        {
            latest = Some((ordinal, point, transition));
        }
    }
    Ok(latest.map(|(_, _, value)| value))
}

fn relationship_check(
    world: &World,
    view: &projection::Projection,
    at: &StoryTime,
    id: &str,
    expected: &RelationshipValues,
) -> Result<Evaluated> {
    let Some(record) = exact(world, id, &["relationship"], at)? else {
        return Ok(Evaluated::Unknown);
    };
    let Some(snapshot) = view.snapshots.get(&projection::Subject::Relationship {
        relationship_id: id.into(),
    }) else {
        return Ok(Evaluated::Unknown);
    };
    let Some(transition) = latest_transition(world, record, at)? else {
        return Ok(Evaluated::Unknown);
    };
    let mut actual = serde_json::Map::new();
    let mut passed = true;
    if let Some(status) = &expected.status {
        actual.insert("status".into(), snapshot.payload["status"].clone());
        passed &= snapshot.payload["status"].as_str() == Some(status);
    }
    if let Some(facets) = &expected.facets {
        actual.insert("facets".into(), snapshot.payload["facets"].clone());
        passed &= snapshot.payload["facets"] == json!(facets);
    }
    if let Some(metrics) = &expected.metrics {
        let mut values = serde_json::Map::new();
        for (name, raw) in metrics {
            let Some(definition) = world
                .world_record()?
                .get("relationship_metrics")
                .and_then(|definitions| definitions.get(name))
                .and_then(Yaml::as_mapping)
            else {
                return Ok(Evaluated::Unsupported(
                    "relationship metric is not declared with a supported definition",
                ));
            };
            let expected = Decimal::parse(raw)?;
            if definition.keys().any(|key| {
                !key.as_str()
                    .is_some_and(|key| ["min", "max", "default"].contains(&key))
            }) {
                return Ok(Evaluated::Unsupported(
                    "relationship metric definition contains unsupported fields",
                ));
            }
            let min = definition.get("min").map(Decimal::source).transpose();
            let max = definition.get("max").map(Decimal::source).transpose();
            let (Ok(min), Ok(max)) = (min, max) else {
                return Ok(Evaluated::Unsupported(
                    "relationship metric has unsupported bounds",
                ));
            };
            if min
                .as_ref()
                .zip(max.as_ref())
                .is_some_and(|(a, b)| a.compare(b) == Ordering::Greater)
            {
                return Ok(Evaluated::Unsupported(
                    "relationship metric bounds are reversed",
                ));
            }
            if let Some(default) = definition.get("default") {
                let Ok(default) = Decimal::source(default) else {
                    return Ok(Evaluated::Unsupported(
                        "relationship metric default is not a finite number",
                    ));
                };
                if min
                    .as_ref()
                    .is_some_and(|min| default.compare(min) == Ordering::Less)
                    || max
                        .as_ref()
                        .is_some_and(|max| default.compare(max) == Ordering::Greater)
                {
                    return Ok(Evaluated::Unsupported(
                        "relationship metric default is outside its declared bounds",
                    ));
                }
            }
            if min
                .as_ref()
                .is_some_and(|min| expected.compare(min) == Ordering::Less)
                || max
                    .as_ref()
                    .is_some_and(|max| expected.compare(max) == Ordering::Greater)
            {
                return Err(invalid(format!(
                    "expected relationship metric {name} is outside its declared bounds"
                )));
            }
            let observed = transition
                .get("metrics")
                .and_then(|values| values.get(name));
            if let Some(value) = observed {
                let Ok(number) = Decimal::source(value) else {
                    return Ok(Evaluated::Unsupported(
                        "observed relationship metric is not a finite number",
                    ));
                };
                if min
                    .as_ref()
                    .is_some_and(|min| number.compare(min) == Ordering::Less)
                    || max
                        .as_ref()
                        .is_some_and(|max| number.compare(max) == Ordering::Greater)
                {
                    return Ok(Evaluated::Unsupported(
                        "observed relationship metric is outside its declared bounds",
                    ));
                }
                passed &= expected == number;
                values.insert(
                    name.clone(),
                    json!({"presence":"present","valueRaw":serde_json::to_string(value)?}),
                );
            } else {
                passed = false;
                values.insert(name.clone(), json!({"presence":"absent"}));
            }
        }
        actual.insert("metrics".into(), Json::Object(values));
    }
    Ok(known(
        passed,
        json!({"kind":"relationship","values":actual}),
        snapshot.citations.clone(),
    ))
}

fn evaluate(
    world: &World,
    view: &projection::Projection,
    at: &StoryTime,
    predicate: &Predicate,
) -> Result<Evaluated> {
    use projection::Subject;
    match predicate {
        Predicate::StateEquals {
            target_id,
            key,
            expected_value_raw,
        } => state_check(world, view, at, target_id, key, Some(expected_value_raw)),
        Predicate::StateAbsent { target_id, key } => {
            state_check(world, view, at, target_id, key, None)
        }
        Predicate::KnowledgeState {
            knowledge_id,
            state,
        } => {
            if exact(world, knowledge_id, &["knowledge"], at)?.is_none() {
                return Ok(Evaluated::Unknown);
            }
            let Some(snapshot) = view.snapshots.get(&Subject::Knowledge {
                knowledge_id: knowledge_id.clone(),
            }) else {
                return Ok(Evaluated::Unknown);
            };
            Ok(known(
                snapshot.payload["state"].as_str() == Some(state),
                json!({"kind":"knowledge","state":snapshot.payload["state"],"transitionId":snapshot.payload["transitionId"],"time":snapshot.payload["time"]}),
                snapshot.citations.clone(),
            ))
        }
        Predicate::RelationshipMatches {
            relationship_id,
            values,
        } => relationship_check(world, view, at, relationship_id, values),
        Predicate::StoryPointState {
            story_point_id,
            state,
        } => {
            if exact(world, story_point_id, &["story-point"], at)?.is_none() {
                return Ok(Evaluated::Unknown);
            }
            let Some(snapshot) = view.snapshots.get(&Subject::StoryPoint {
                story_point_id: story_point_id.clone(),
            }) else {
                return Ok(Evaluated::Unknown);
            };
            Ok(known(
                snapshot.payload["storedState"].as_str() == Some(state),
                json!({"kind":"story-point","state":snapshot.payload["storedState"]}),
                snapshot.citations.clone(),
            ))
        }
        Predicate::OutcomeLinked {
            event_id,
            target_id,
        } => {
            let event = exact(world, event_id, &["event"], at)?;
            let target = exact(world, target_id, &["story-point", "scene"], at)?;
            if event.is_none() || target.is_none() {
                return Ok(Evaluated::Unknown);
            }
            let Some(snapshot) = projection::outcome(world, event_id, target_id, at)? else {
                return Ok(Evaluated::Unknown);
            };
            let passed = snapshot.payload["targetLinked"] == true
                && (snapshot.payload["targetKind"] == "scene"
                    || snapshot.payload["eventLinked"] == true);
            Ok(known(
                passed,
                json!({"kind":"outcome","targetKind":snapshot.payload["targetKind"],"targetLinked":snapshot.payload["targetLinked"],"eventLinked":snapshot.payload["eventLinked"]}),
                snapshot.citations,
            ))
        }
        Predicate::TransitionCaused {
            record_id,
            transition_id,
            event_id,
        } => {
            let record = exact(
                world,
                record_id,
                &["knowledge", "relationship", "story-point"],
                at,
            )?;
            let event = exact(world, event_id, &["event"], at)?;
            let (Some(record), Some(_)) = (record, event) else {
                return Ok(Evaluated::Unknown);
            };
            let (section, transitions) = if record.kind == "story-point" {
                (
                    "lifecycle.transitions",
                    record.get("lifecycle").and_then(|v| v.get("transitions")),
                )
            } else {
                ("transitions", record.get("transitions"))
            };
            let mut matches = transitions
                .and_then(Yaml::as_sequence)
                .into_iter()
                .flatten()
                .enumerate()
                .filter(|(_, value)| value.get("id").and_then(Yaml::as_str) == Some(transition_id));
            let Some((ordinal, selected)) = matches.next() else {
                return Ok(Evaluated::Unknown);
            };
            if matches.next().is_some() {
                return Err(invalid(
                    "selected transition ID is ambiguous within its owner",
                ));
            }
            let point = StoryTime::from_yaml(
                selected
                    .get("time")
                    .ok_or_else(|| Error::Validation("transition requires time".into()))?,
                &world.default_timeline(),
            )?;
            if !point.not_after(at) {
                return Ok(Evaluated::Unknown);
            }
            let cause = selected
                .get("causing_event")
                .filter(|value| !value.is_null());
            let actual = match cause {
                None => json!({"presence":"absent"}),
                Some(value) => {
                    let id = value.as_str().ok_or_else(|| {
                        Error::Validation("transition causing_event must be an ID".into())
                    })?;
                    if exact(world, id, &["event"], at)?.is_none() {
                        return Ok(Evaluated::Unknown);
                    }
                    json!({"presence":"present","value":id})
                }
            };
            let mut evidence = citation(world, record, &format!("{section}[{ordinal}]"));
            evidence["transitionId"] = json!(transition_id);
            evidence["time"] = json!(point);
            Ok(known(
                cause.and_then(Yaml::as_str) == Some(event_id),
                json!({"kind":"transition","transitionId":transition_id,"time":point,"causingEvent":actual}),
                vec![evidence],
            ))
        }
    }
}

fn evaluate_all(
    world: &World,
    analysis: &Analysis,
    view: &projection::Projection,
    at: &StoryTime,
) -> Result<(Vec<Json>, bool)> {
    let mut results = Vec::with_capacity(analysis.expectations.len());
    let mut all_pass = true;
    let mut bytes = 0usize;
    for expectation in &analysis.expectations {
        let (outcome, actual, evidence, reason) =
            match evaluate(world, view, at, &expectation.predicate)? {
                Evaluated::Known {
                    passed,
                    actual,
                    citations,
                } => (
                    if passed { "pass" } else { "fail" },
                    actual,
                    citations,
                    None,
                ),
                Evaluated::Unknown => (
                    "unknown",
                    json!({"kind":"unavailable"}),
                    vec![],
                    Some("selected source is unavailable at this horizon"),
                ),
                Evaluated::Unsupported(reason) => (
                    "unsupported",
                    json!({"kind":"unsupported"}),
                    vec![],
                    Some(reason),
                ),
            };
        all_pass &= outcome == "pass";
        let result = json!({"id":expectation.id,"policy":analysis.policy,"outcome":outcome,
            "predicate":expectation.predicate,"actual":actual,"citations":evidence,"reason":reason});
        bytes = bytes.saturating_add(encoded(&result, MAX_REPORT_BYTES)?.len());
        if bytes > MAX_REPORT_BYTES {
            return Err(invalid("complete expectation results exceed 8 MiB"));
        }
        results.push(result);
    }
    Ok((results, all_pass))
}

fn valid_source(world: &World) -> Result<()> {
    projection::preflight(world)?;
    if !validation::validate_world(world).valid {
        return Err(Error::Validation(
            "semantic analysis requires a complete valid source snapshot".into(),
        ));
    }
    Ok(())
}

fn stamp(report: &mut Json) -> Result<()> {
    let bytes = encoded(report, MAX_REPORT_BYTES)?;
    report["reportSha256"] = json!(digest(b"wedl-consequence-semantics\0", &bytes));
    encoded(report, MAX_REPORT_BYTES)?;
    Ok(())
}

/// Evaluate explicit checks on one complete immutable author source snapshot.
pub fn check(world: &World, analysis: &Analysis) -> Result<Json> {
    validate_analysis(analysis)?;
    valid_source(world)?;
    let at = analysis.at.point()?;
    let view = projection::project(world, &at)?;
    let (expectations, all_pass) = evaluate_all(world, analysis, &view, &at)?;
    let mut result = json!({"protocol":crate::CONTRACT,"operation":"consequence-check",
        "outcome":"ok","perspective":"author","scope":"explicit-semantic-expectations",
        "fingerprint":world.fingerprint,"revision":world.revision,"at":at,"policy":analysis.policy,
        "analysisSha256":digest(b"wedl-consequence-analysis\0", &encoded(analysis,MAX_ANALYSIS_BYTES)?),
        "expectations":expectations,"expectationsSatisfied":all_pass,
        "applyAllowed":analysis.policy == Policy::Advisory || all_pass,"writesPerformed":false,
        "notice":"Explicit expectations inspect authored semantics. Passing checks do not supply approval, infer effects or establish a belief's truth."});
    stamp(&mut result)?;
    Ok(result)
}

/// Compare both complete worlds at the identical horizon, then check the
/// candidate. Source edits are independently visible in the ordinary preview.
pub fn analyze(base: &World, candidate: &World, analysis: &Analysis) -> Result<Json> {
    validate_analysis(analysis)?;
    valid_source(base)?;
    valid_source(candidate)?;
    if base.world_record()?.id != candidate.world_record()?.id {
        return Err(invalid("semantic delta requires the same world identity"));
    }
    let at = analysis.at.point()?;
    let delta = projection::delta(base, candidate, &at)?;
    let view = projection::project(candidate, &at)?;
    let (expectations, all_pass) = evaluate_all(candidate, analysis, &view, &at)?;
    let mut result = json!({"protocol":crate::CONTRACT,"operation":"consequence-analysis",
        "outcome":"ok","perspective":"author","scope":"same-horizon-semantics",
        "baseFingerprint":base.fingerprint,"candidateFingerprint":candidate.fingerprint,
        "at":at,"policy":analysis.policy,"delta":delta,
        "analysisSha256":digest(b"wedl-consequence-analysis\0", &encoded(analysis,MAX_ANALYSIS_BYTES)?),
        "expectations":expectations,"expectationsSatisfied":all_pass,
        "applyAllowed":analysis.policy == Policy::Advisory || all_pass,"writesPerformed":false,
        "notice":"Both worlds are evaluated at the same horizon. Differences do not establish causal attribution; passing checks do not supply approval."});
    stamp(&mut result)?;
    Ok(result)
}

/// Closed, source-pinned native/service/CLI check with a complete output budget.
pub fn check_request(world: &World, request: &CheckRequest) -> Result<Json> {
    validate_request(request)?;
    if request.expected_fingerprint != world.fingerprint {
        return Err(Error::Conflict(
            "source fingerprint differs from the semantic check request".into(),
        ));
    }
    let mut result = check(world, &request.analysis)?;
    result
        .as_object_mut()
        .expect("native object")
        .remove("reportSha256");
    result["maxCharacters"] = json!(request.max_characters);
    stamp(&mut result)?;
    let bytes = encoded(&result, MAX_REPORT_BYTES)?;
    if std::str::from_utf8(&bytes)
        .expect("JSON is UTF-8")
        .chars()
        .count()
        > request.max_characters
    {
        return Err(invalid(
            "complete semantic check report exceeds maxCharacters",
        ));
    }
    Ok(result)
}

/// Exact bounded decimal comparison for numeric metrics. No operand is first
/// rounded through binary64, and integers never become Boolean/string values.
#[derive(Debug, PartialEq, Eq)]
struct Decimal {
    negative: bool,
    coefficient: String,
    exponent: i32,
}

impl Decimal {
    fn parse(raw: &str) -> Result<Self> {
        if raw.is_empty() || raw.len() > MAX_NUMERIC_BYTES {
            return Err(invalid(
                "metric operands require bounded decimal JSON-number strings",
            ));
        }
        let bytes = raw.as_bytes();
        let mut i = usize::from(bytes[0] == b'-');
        let negative = i == 1;
        let start = i;
        match bytes.get(i) {
            Some(b'0') => i += 1,
            Some(b'1'..=b'9') => {
                i += 1;
                while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                }
            }
            _ => return Err(invalid("metric operand is not a decimal JSON number")),
        }
        let mut coefficient = raw[start..i].to_owned();
        let mut fraction = 0i32;
        if bytes.get(i) == Some(&b'.') {
            i += 1;
            let start = i;
            while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            if i == start {
                return Err(invalid("metric operand has no fractional digits"));
            }
            fraction = i32::try_from(i - start).expect("bounded numeric bytes");
            coefficient.push_str(&raw[start..i]);
        }
        let mut exponent = 0i32;
        if matches!(bytes.get(i), Some(b'e' | b'E')) {
            i += 1;
            let start = i;
            if matches!(bytes.get(i), Some(b'+' | b'-')) {
                i += 1;
            }
            let digits = i;
            while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            if i == digits {
                return Err(invalid("metric operand has no exponent digits"));
            }
            exponent = raw[start..i]
                .parse()
                .map_err(|_| invalid("metric exponent exceeds complete bounds"))?;
            if !(-4096..=4096).contains(&exponent) {
                return Err(invalid("metric exponent exceeds complete bounds"));
            }
        }
        if i != bytes.len() {
            return Err(invalid("metric operand is not a decimal JSON number"));
        }
        exponent -= fraction;
        coefficient = coefficient.trim_start_matches('0').to_owned();
        if coefficient.is_empty() {
            return Ok(Self {
                negative: false,
                coefficient: "0".into(),
                exponent: 0,
            });
        }
        while coefficient.ends_with('0') {
            coefficient.pop();
            exponent += 1;
        }
        Ok(Self {
            negative,
            coefficient,
            exponent,
        })
    }

    fn source(value: &Yaml) -> Result<Self> {
        let Yaml::Number(number) = value else {
            return Err(invalid("metric source is not a number"));
        };
        if !number.as_f64().is_some_and(f64::is_finite) {
            return Err(invalid("metric source is not finite"));
        }
        Self::parse(&number.to_string())
    }

    fn compare(&self, other: &Self) -> Ordering {
        if self == other {
            return Ordering::Equal;
        }
        if self.negative != other.negative {
            return if self.negative {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        }
        let magnitude = if self.coefficient == "0" {
            Ordering::Less
        } else if other.coefficient == "0" {
            Ordering::Greater
        } else {
            let left = self.coefficient.len() as i64 + i64::from(self.exponent);
            let right = other.coefficient.len() as i64 + i64::from(other.exponent);
            left.cmp(&right).then_with(|| {
                let len = self.coefficient.len().max(other.coefficient.len());
                self.coefficient
                    .bytes()
                    .chain(std::iter::repeat(b'0'))
                    .take(len)
                    .cmp(
                        other
                            .coefficient
                            .bytes()
                            .chain(std::iter::repeat(b'0'))
                            .take(len),
                    )
            })
        };
        if self.negative {
            magnitude.reverse()
        } else {
            magnitude
        }
    }
}
