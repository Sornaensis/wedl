//! Small author-local story queries. Authored lifecycle and causes remain facts;
//! eligibility is a read and never activates points or creates events.
use crate::{
    Error, Result,
    model::{Record, StoryTime, World},
    runtime, state_schema,
    validation::Diagnostic,
};
use serde_json::{Value as Json, json};
use serde_yaml::{Mapping, Value};
use std::collections::{BTreeMap, BTreeSet};

const LIMIT: usize = 1000;
const STATES: &[&str] = &["dormant", "active", "resolved", "failed", "cancelled"];
const BELIEFS: &[&str] = &[
    "accepted",
    "suspected",
    "rejected",
    "uncertain",
    "remembered",
    "forgotten",
];

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}
fn map<'a>(value: &'a Value, label: &str) -> Result<&'a Mapping> {
    value
        .as_mapping()
        .ok_or_else(|| invalid(format!("{label} must be a mapping")))
}
fn closed<'a>(
    value: &'a Value,
    required: &[&str],
    optional: &[&str],
    label: &str,
) -> Result<&'a Mapping> {
    let fields = map(value, label)?;
    if fields.keys().any(|key| {
        key.as_str()
            .is_none_or(|key| !required.contains(&key) && !optional.contains(&key))
    }) {
        return Err(invalid(format!("{label} contains an unknown field")));
    }
    if required.iter().any(|key| !fields.contains_key(*key)) {
        return Err(invalid(format!("{label} is missing a required field")));
    }
    Ok(fields)
}
fn text<'a>(value: &'a Value, label: &str) -> Result<&'a str> {
    value
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| invalid(format!("{label} must be a nonempty string")))
}
fn required<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value
        .get(key)
        .ok_or_else(|| invalid(format!("{key} is required")))
}
fn sequence<'a>(value: &'a Value, label: &str) -> Result<&'a Vec<Value>> {
    let values = value
        .as_sequence()
        .ok_or_else(|| invalid(format!("{label} must be a sequence")))?;
    if values.len() > LIMIT {
        return Err(invalid(format!("{label} exceeds {LIMIT} items")));
    }
    Ok(values)
}
fn optional_list<'a>(value: &'a Value, key: &str) -> Result<&'a [Value]> {
    match value.get(key) {
        None => Ok(&[]),
        Some(value) => Ok(sequence(value, key)?.as_slice()),
    }
}
fn number(value: &Value, label: &str) -> Result<f64> {
    value
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(|| invalid(format!("{label} must be a finite number")))
}
fn point(world: &World, value: &Value) -> Result<StoryTime> {
    closed(value, &["tick"], &["timeline", "order"], "story time")?;
    let at = StoryTime::from_yaml(value, &world.default_timeline())?;
    world.story_time(at.tick, Some(&at.timeline), at.order)
}
fn horizon(world: &World, at: &StoryTime) -> Result<()> {
    world
        .story_time(at.tick, Some(&at.timeline), at.order)
        .map(|_| ())
}
fn reference<'a>(world: &'a World, value: &Value, kind: Option<&str>) -> Result<&'a Record> {
    let id = text(value, "reference")?;
    let record = world
        .records
        .get(id)
        .ok_or_else(|| invalid(format!("unknown reference {id}")))?;
    if kind.is_some_and(|kind| record.kind != kind) {
        return Err(invalid(format!("{id} has the wrong record kind")));
    }
    Ok(record)
}
fn canonical(record: &Record) -> Result<()> {
    if record.status() != "canonical" {
        return Err(invalid(format!("{} must be canonical", record.id)));
    }
    Ok(())
}
fn strings(
    value: &Value,
    allowed: Option<&[&str]>,
    label: &str,
    nonempty: bool,
) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for value in sequence(value, label)? {
        let value = text(value, label)?;
        if allowed.is_some_and(|allowed| !allowed.contains(&value)) {
            return Err(invalid(format!("{label}: unsupported value {value}")));
        }
        if result.iter().any(|item| item == value) {
            return Err(invalid(format!("{label}: duplicate {value}")));
        }
        result.push(value.into());
    }
    if nonempty && result.is_empty() {
        return Err(invalid(format!("{label} must not be empty")));
    }
    Ok(result)
}
fn cite(world: &World, record: &Record, section: &str) -> Json {
    json!({"entityId":record.id,"sourcePath":record.source_path,"blobOid":record.blob_oid,
        "revision":world.revision,"fingerprint":world.fingerprint,"section":section})
}
fn envelope(world: &World, at: &StoryTime) -> Json {
    json!({"protocol":crate::CONTRACT,"perspective":"author","revision":world.revision,
        "fingerprint":world.fingerprint,"effectiveTime":at})
}

enum Condition {
    All(Vec<Condition>),
    Any(Vec<Condition>),
    Not(Box<Condition>),
    Event(String),
    State {
        entity: String,
        key: String,
        equals: Value,
    },
    Knowledge {
        character: String,
        claim: String,
        states: Vec<String>,
        minimum: f64,
    },
    Relationship {
        id: String,
        metric: String,
        min: Option<f64>,
        max: Option<f64>,
    },
    Scene {
        location: Option<String>,
        participant: Option<String>,
        object: Option<String>,
    },
}

fn condition(world: &World, value: &Value, depth: usize, nodes: &mut usize) -> Result<Condition> {
    *nodes += 1;
    if depth > 32 || *nodes > LIMIT {
        return Err(invalid("when expression exceeds depth 32 or 1000 nodes"));
    }
    let fields = map(value, "when")?;
    if fields.len() != 1 {
        return Err(invalid(
            "a when expression must contain exactly one operator",
        ));
    }
    let (operator, value) = fields.iter().next().unwrap();
    Ok(match text(operator, "when operator")? {
        "all" | "any" => {
            let children = sequence(value, "logical condition")?
                .iter()
                .map(|v| condition(world, v, depth + 1, nodes))
                .collect::<Result<Vec<_>>>()?;
            if operator.as_str() == Some("all") {
                Condition::All(children)
            } else {
                Condition::Any(children)
            }
        }
        "not" => Condition::Not(Box::new(condition(world, value, depth + 1, nodes)?)),
        "event" => {
            let record = reference(world, value, Some("event"))?;
            canonical(record)?;
            Condition::Event(record.id.clone())
        }
        "state" => {
            closed(value, &["entity", "key", "equals"], &[], "state condition")?;
            let record = reference(world, required(value, "entity")?, None)?;
            let key = text(required(value, "key")?, "state key")?;
            let definition = state_schema::field_definition(world, &record.kind, key)?;
            let equals = required(value, "equals")?;
            state_schema::validate_value(world, &definition, equals)?;
            Condition::State {
                entity: record.id.clone(),
                key: key.into(),
                equals: equals.clone(),
            }
        }
        "knowledge" => {
            closed(
                value,
                &["character", "claim", "states"],
                &["minimum_confidence"],
                "knowledge condition",
            )?;
            let character = reference(world, required(value, "character")?, Some("character"))?;
            let minimum = value
                .get("minimum_confidence")
                .map(|v| number(v, "minimum_confidence"))
                .transpose()?
                .unwrap_or(0.0);
            if !(0.0..=1.0).contains(&minimum) {
                return Err(invalid("minimum_confidence must be between 0 and 1"));
            }
            Condition::Knowledge {
                character: character.id.clone(),
                claim: text(required(value, "claim")?, "claim key")?.into(),
                states: strings(
                    required(value, "states")?,
                    Some(BELIEFS),
                    "knowledge states",
                    true,
                )?,
                minimum,
            }
        }
        "relationship" => {
            closed(
                value,
                &["id", "metric"],
                &["min", "max"],
                "relationship condition",
            )?;
            let record = reference(world, required(value, "id")?, Some("relationship"))?;
            let metric = text(required(value, "metric")?, "relationship metric")?;
            if world
                .world_record()?
                .get("relationship_metrics")
                .and_then(|v| v.get(metric))
                .is_none()
            {
                return Err(invalid(format!("undeclared relationship metric {metric}")));
            }
            let min = value.get("min").map(|v| number(v, "min")).transpose()?;
            let max = value.get("max").map(|v| number(v, "max")).transpose()?;
            if min.is_none() && max.is_none() || min.zip(max).is_some_and(|(a, b)| a > b) {
                return Err(invalid(
                    "relationship requires min/max bounds in increasing order",
                ));
            }
            Condition::Relationship {
                id: record.id.clone(),
                metric: metric.into(),
                min,
                max,
            }
        }
        "scene" => {
            closed(
                value,
                &[],
                &["location", "participant", "object"],
                "scene condition",
            )?;
            let selected = |field, kind| {
                value
                    .get(field)
                    .map(|value| reference(world, value, Some(kind)).map(|r| r.id.clone()))
                    .transpose()
            };
            Condition::Scene {
                location: selected("location", "location")?,
                participant: selected("participant", "character")?,
                object: selected("object", "object")?,
            }
        }
        operator => return Err(invalid(format!("unknown when operator {operator}"))),
    })
}

struct Transition<'a> {
    id: &'a str,
    state: &'a str,
    time: StoryTime,
}
struct StoryPoint<'a> {
    record: &'a Record,
    initial: &'a str,
    transitions: Vec<Transition<'a>>,
    requires: Vec<(String, Vec<String>)>,
    when: Condition,
    policy: &'a str,
    priority: i32,
}

fn story_point<'a>(world: &World, record: &'a Record) -> Result<StoryPoint<'a>> {
    for old in ["trigger", "dependencies", "on_activate", "repeat_policy"] {
        if record.get(old).is_some() {
            return Err(invalid(format!(
                "obsolete story-point field {old}; use requires/when and explicit lifecycle"
            )));
        }
    }
    let policy = text(
        required(&record.frontmatter, "activation_policy")?,
        "activation_policy",
    )?;
    if !["manual", "suggest"].contains(&policy) {
        return Err(invalid("activation_policy must be manual or suggest"));
    }
    let lifecycle = required(&record.frontmatter, "lifecycle")?;
    closed(lifecycle, &["initial_state"], &["transitions"], "lifecycle")?;
    let initial = text(required(lifecycle, "initial_state")?, "initial_state")?;
    if !STATES.contains(&initial) {
        return Err(invalid("invalid initial lifecycle state"));
    }
    let mut ids = BTreeSet::new();
    let mut transitions: Vec<Transition<'_>> = Vec::new();
    for transition in optional_list(lifecycle, "transitions")? {
        closed(
            transition,
            &["id", "state", "time"],
            &["causing_event", "note"],
            "lifecycle transition",
        )?;
        let id = text(required(transition, "id")?, "transition ID")?;
        if !ids.insert(id) {
            return Err(invalid(format!("duplicate lifecycle transition {id}")));
        }
        let state = text(required(transition, "state")?, "transition state")?;
        if !STATES.contains(&state) {
            return Err(invalid(format!("invalid lifecycle state {state}")));
        }
        let time = point(world, required(transition, "time")?)?;
        if transitions
            .last()
            .is_some_and(|last| !last.time.not_after(&time) || last.time == time)
        {
            return Err(invalid(
                "lifecycle transitions must be strictly ordered on one timeline",
            ));
        }
        if let Some(value) = transition.get("causing_event") {
            let cause = reference(world, value, Some("event"))?;
            canonical(cause)?;
            if !point(world, required(&cause.frontmatter, "time")?)?.not_after(&time) {
                return Err(invalid(
                    "lifecycle cause must be at or before the transition on the same timeline",
                ));
            }
        }
        if let Some(value) = transition.get("note") {
            text(value, "transition note")?;
        }
        transitions.push(Transition { id, state, time });
    }
    let mut requires = Vec::new();
    let mut required_ids = BTreeSet::new();
    for requirement in optional_list(&record.frontmatter, "requires")? {
        closed(requirement, &["id", "states"], &[], "requirement")?;
        let target = reference(world, required(requirement, "id")?, Some("story-point"))?;
        canonical(target)?;
        if !required_ids.insert(&target.id) {
            return Err(invalid("duplicate story-point requirement"));
        }
        requires.push((
            target.id.clone(),
            strings(
                required(requirement, "states")?,
                Some(STATES),
                "required states",
                true,
            )?,
        ));
    }
    for value in optional_list(&record.frontmatter, "outcome_events")? {
        canonical(reference(world, value, Some("event"))?)?;
    }
    let priority = match record.get("priority") {
        None => 0,
        Some(value) => value
            .as_i64()
            .and_then(|v| i32::try_from(v).ok())
            .ok_or_else(|| invalid("priority must be a signed 32-bit integer"))?,
    };
    let when = match record.get("when") {
        None => Condition::All(vec![]),
        Some(value) => condition(world, value, 0, &mut 0)?,
    };
    Ok(StoryPoint {
        record,
        initial,
        transitions,
        requires,
        when,
        policy,
        priority,
    })
}

fn acyclic(points: &[StoryPoint<'_>]) -> Result<()> {
    if points.len() > LIMIT {
        return Err(invalid("story-point count exceeds 1000"));
    }
    let mut remaining: BTreeMap<&str, usize> = points
        .iter()
        .map(|p| (p.record.id.as_str(), p.requires.len()))
        .collect();
    let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for point in points {
        for (id, _) in &point.requires {
            dependents.entry(id).or_default().push(&point.record.id);
        }
    }
    let mut ready: BTreeSet<&str> = remaining
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(*id))
        .collect();
    while let Some(id) = ready.pop_first() {
        remaining.remove(id);
        for dependent in dependents.get(id).into_iter().flatten() {
            if let Some(count) = remaining.get_mut(dependent) {
                *count -= 1;
                if *count == 0 {
                    ready.insert(dependent);
                }
            }
        }
    }
    if !remaining.is_empty() {
        return Err(invalid(format!(
            "cyclic story-point requirements: {}",
            remaining.keys().copied().collect::<Vec<_>>().join(", ")
        )));
    }
    Ok(())
}

fn stored<'a>(point: &'a StoryPoint<'_>, at: &StoryTime) -> (&'a str, Option<&'a Transition<'a>>) {
    let latest = point
        .transitions
        .iter()
        .enumerate()
        .filter(|(_, t)| t.time.not_after(at))
        .max_by_key(|(ordinal, t)| (t.time.tick, t.time.order, *ordinal))
        .map(|(_, t)| t);
    (latest.map(|t| t.state).unwrap_or(point.initial), latest)
}

fn evaluate(
    world: &World,
    condition: &Condition,
    at: &StoryTime,
    state: Option<&BTreeMap<String, BTreeMap<String, Value>>>,
    scene: Option<&Record>,
    allowed_records: Option<&BTreeSet<String>>,
) -> Result<(bool, Vec<String>)> {
    let mismatch = |why: String| Ok((false, vec![why]));
    match condition {
        Condition::All(values) | Condition::Any(values) => {
            let mut passed = Vec::new();
            let mut reasons = Vec::new();
            for value in values {
                let (ok, mut why) = evaluate(world, value, at, state, scene, allowed_records)?;
                passed.push(ok);
                reasons.append(&mut why);
            }
            let ok = if matches!(condition, Condition::All(_)) {
                passed.iter().all(|x| *x)
            } else {
                passed.iter().any(|x| *x)
            };
            if ok {
                Ok((true, vec![]))
            } else {
                if reasons.is_empty() {
                    reasons.push("no alternative condition matched".into());
                }
                Ok((false, reasons))
            }
        }
        Condition::Not(value) => {
            if evaluate(world, value, at, state, scene, allowed_records)?.0 {
                mismatch("negated condition matched".into())
            } else {
                Ok((true, vec![]))
            }
        }
        Condition::Event(id) => {
            let event = &world.records[id];
            let event_time = point(world, required(&event.frontmatter, "time")?)?;
            if event_time.not_after(at) {
                Ok((true, vec![]))
            } else {
                mismatch(format!("event {id} has not occurred at this horizon"))
            }
        }
        Condition::State {
            entity,
            key,
            equals,
        } => {
            let values = match state.and_then(|state| state.get(entity)) {
                Some(values) => values.clone(),
                None => runtime::resolved_state(world, entity, at)?,
            };
            let definition =
                state_schema::field_definition(world, &world.records[entity].kind, key)?;
            if values.get(key).is_some_and(|actual| {
                state_schema::values_equal_for_definition(&definition, actual, equals)
            }) {
                Ok((true, vec![]))
            } else {
                mismatch(format!(
                    "state {entity}.{key} does not equal the required value"
                ))
            }
        }
        Condition::Knowledge {
            character,
            claim,
            states,
            minimum,
        } => {
            for record in world.records.values().filter(|r| {
                r.kind == "knowledge"
                    && r.status() == "canonical"
                    && r.string("knower") == Some(character)
                    && allowed_records.is_none_or(|ids| ids.contains(&r.id))
            }) {
                if record
                    .get("claim")
                    .and_then(|v| v.get("key"))
                    .and_then(Value::as_str)
                    != Some(claim)
                {
                    continue;
                }
                let mut latest = None;
                for (ordinal, transition) in optional_list(&record.frontmatter, "transitions")?
                    .iter()
                    .enumerate()
                {
                    let when = point(world, required(transition, "time")?)?;
                    if when.not_after(at)
                        && latest.as_ref().is_none_or(
                            |(old, _, old_time): &(usize, &Value, StoryTime)| {
                                (when.tick, when.order, ordinal)
                                    > (old_time.tick, old_time.order, *old)
                            },
                        )
                    {
                        latest = Some((ordinal, transition, when));
                    }
                }
                if let Some((_, transition, _)) = latest {
                    let state = text(required(transition, "state")?, "knowledge state")?;
                    let confidence = transition
                        .get("confidence")
                        .map(|v| number(v, "knowledge confidence"))
                        .transpose()?
                        .unwrap_or(0.0);
                    if states.iter().any(|s| s == state) && confidence >= *minimum {
                        return Ok((true, vec![]));
                    }
                }
            }
            mismatch(format!(
                "{character} has no current {claim} belief meeting the required state/confidence"
            ))
        }
        Condition::Relationship {
            id,
            metric,
            min,
            max,
        } => {
            let record = &world.records[id];
            let from = text(required(&record.frontmatter, "from")?, "relationship.from")?;
            let values = runtime::relationships(world, from, at)?;
            let observed = values["items"]
                .as_array()
                .and_then(|items| items.iter().find(|v| v["relationshipId"] == *id))
                .and_then(|v| v["metrics"][metric].as_f64());
            if observed.is_some_and(|v| min.is_none_or(|m| v >= m) && max.is_none_or(|m| v <= m)) {
                Ok((true, vec![]))
            } else {
                mismatch(format!(
                    "relationship {id}.{metric} does not meet the required bounds"
                ))
            }
        }
        Condition::Scene {
            location,
            participant,
            object,
        } => {
            let Some(scene) = scene else {
                return mismatch("scene condition requires an admitted selected scene".into());
            };
            // A selected scene does not grant a hidden endpoint. Keep the
            // explanation independent of any withheld endpoint's identity.
            for id in [location, participant, object].into_iter().flatten() {
                let target = &world.records[id];
                if !crate::description::author_record_visible(world, target, at)? {
                    return mismatch(
                        "selected scene does not satisfy the authored scene condition".into(),
                    );
                }
            }
            let location_matches = location
                .as_ref()
                .is_none_or(|id| scene.string("location") == Some(id.as_str()));
            let participant_matches = participant
                .as_ref()
                .map(|id| crate::scene::present(world, scene, id, at))
                .transpose()?
                .unwrap_or(true);
            let object_matches = object.as_ref().is_none_or(|id| {
                scene.list("objects").is_some_and(|objects| {
                    objects
                        .iter()
                        .any(|value| value.as_str() == Some(id.as_str()))
                })
            });
            if location_matches && participant_matches && object_matches {
                Ok((true, vec![]))
            } else {
                mismatch("selected scene does not satisfy the authored scene condition".into())
            }
        }
    }
}

pub fn story_points(world: &World, at: &StoryTime) -> Result<Json> {
    story_points_with_state(world, at, None)
}

/// Explicit scene selection supplies only scene-predicate context. It never
/// restricts the story-point candidates, advances a clock, or grants learning.
pub fn story_points_with_scene(world: &World, at: &StoryTime, scene: Option<&str>) -> Result<Json> {
    horizon(world, at)?;
    let scene = scene.map(|id| selected_scene(world, id, at)).transpose()?;
    story_points_with_context(world, at, None, scene, None)
}

/// The navigation consumer supplies a request-local admitted replay without
/// modifying canonical source or the unrestricted canonical query contract.
pub(crate) fn story_points_with_admitted_context(
    world: &World,
    at: &StoryTime,
    scene: Option<&str>,
    projected_state: &BTreeMap<String, BTreeMap<String, Value>>,
    allowed_records: &BTreeSet<String>,
) -> Result<Json> {
    horizon(world, at)?;
    let scene = scene.map(|id| selected_scene(world, id, at)).transpose()?;
    story_points_with_context(
        world,
        at,
        Some(projected_state),
        scene,
        Some(allowed_records),
    )
}

fn selected_scene<'a>(world: &'a World, id: &str, at: &StoryTime) -> Result<&'a Record> {
    let unavailable =
        || Error::NotFound("selected story scene is unavailable at this horizon".into());
    let scene = world.records.get(id).ok_or_else(unavailable)?;
    if scene.kind != "scene"
        || !matches!(scene.status(), "active" | "closed")
        || !crate::description::author_record_visible(world, scene, at)?
    {
        return Err(unavailable());
    }
    let time = required(&scene.frontmatter, "time")?;
    let start = point(world, required(time, "start")?)?;
    let end = match time.get("end").filter(|value| !value.is_null()) {
        Some(end) => point(world, end)?,
        None if scene.status() == "active" => point(world, required(time, "current")?)?,
        None => return Err(invalid("a closed selected scene requires time.end")),
    };
    if !start.not_after(at) || !at.not_after(&end) {
        return Err(unavailable());
    }
    Ok(scene)
}

/// Semantic comparisons reuse the same condition/lifecycle evaluator with their
/// admitted replay. Ordinary runtime queries retain their existing state fold.
pub(crate) fn story_points_with_state(
    world: &World,
    at: &StoryTime,
    projected_state: Option<&BTreeMap<String, BTreeMap<String, Value>>>,
) -> Result<Json> {
    story_points_with_context(world, at, projected_state, None, None)
}

fn story_points_with_context(
    world: &World,
    at: &StoryTime,
    projected_state: Option<&BTreeMap<String, BTreeMap<String, Value>>>,
    scene: Option<&Record>,
    allowed_records: Option<&BTreeSet<String>>,
) -> Result<Json> {
    horizon(world, at)?;
    let points = world
        .records
        .values()
        .filter(|r| r.kind == "story-point")
        .map(|r| story_point(world, r))
        .collect::<Result<Vec<_>>>()?;
    acyclic(&points)?;
    let current: BTreeMap<&str, &str> = points
        .iter()
        .map(|p| (p.record.id.as_str(), stored(p, at).0))
        .collect();
    let mut ranked: Vec<_> = points
        .iter()
        .filter(|p| p.record.status() == "canonical")
        .collect();
    ranked.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| {
                a.record
                    .title
                    .to_lowercase()
                    .cmp(&b.record.title.to_lowercase())
            })
            .then_with(|| a.record.id.cmp(&b.record.id))
    });
    let mut items = Vec::new();
    for point in ranked {
        let (state, transition) = stored(point, at);
        let mut reasons = Vec::new();
        if state != "dormant" {
            reasons.push(format!(
                "stored lifecycle is {state}; eligibility requires dormant"
            ));
        }
        let mut requirements_satisfied = true;
        for (id, states) in &point.requires {
            if !states.iter().any(|s| s == current[id.as_str()]) {
                requirements_satisfied = false;
                reasons.push(format!(
                    "requires {id} in [{}], currently {}",
                    states.join(", "),
                    current[id.as_str()]
                ));
            }
        }
        let (condition_satisfied, mut condition_reasons) = evaluate(
            world,
            &point.when,
            at,
            projected_state,
            scene,
            allowed_records,
        )?;
        reasons.append(&mut condition_reasons);
        let mut outcomes = Vec::new();
        for id in optional_list(&point.record.frontmatter, "outcome_events")? {
            let event = reference(world, id, Some("event"))?;
            if point_time(world, event)?.not_after(at) {
                outcomes.push(event.id.clone());
            }
        }
        items.push(json!({"storyPointId":point.record.id,"title":point.record.title,"storedState":state,
            "eligible":reasons.is_empty(),"blockedReasons":reasons,"requirementsSatisfied":requirements_satisfied,
            "conditionSatisfied":condition_satisfied,"activationPolicy":point.policy,"priority":point.priority,
            "transitionId":transition.map(|t|t.id),"transitionTime":transition.map(|t|&t.time),
            "outcomeEvents":outcomes,"citation":cite(world,point.record,"lifecycle/requires/when")}));
    }
    let mut result = envelope(world, at);
    result["items"] = json!(items);
    Ok(result)
}

fn point_time(world: &World, record: &Record) -> Result<StoryTime> {
    point(world, required(&record.frontmatter, "time")?)
}
fn causes<'a>(world: &'a World, record: &Record) -> Result<Vec<&'a Record>> {
    let at = point_time(world, record)?;
    let mut ids = BTreeSet::new();
    let mut result = Vec::new();
    for value in optional_list(&record.frontmatter, "causes")? {
        let cause = reference(world, value, Some("event"))?;
        canonical(cause)?;
        let time = point_time(world, cause)?;
        if !time.not_after(&at) || (time.tick, time.order) == (at.tick, at.order) {
            return Err(invalid(format!(
                "{} cause {} must be strictly earlier on the same timeline",
                record.id, cause.id
            )));
        }
        if !ids.insert(&cause.id) {
            return Err(invalid(format!("duplicate cause {}", cause.id)));
        }
        result.push(cause);
    }
    result.sort_by_key(|r| &r.id);
    Ok(result)
}

pub fn causal(world: &World, event: &str, at: &StoryTime) -> Result<Json> {
    horizon(world, at)?;
    let root = world.resolve(event)?;
    if root.kind != "event" {
        return Err(Error::InvalidInput("causal expects an event".into()));
    }
    canonical(root)?;
    if !point_time(world, root)?.not_after(at) {
        return Err(Error::NotFound(
            "event has not occurred at this horizon".into(),
        ));
    }
    let mut pending = vec![root];
    let mut seen = BTreeSet::new();
    let mut edges = BTreeSet::new();
    let mut records = Vec::new();
    while let Some(record) = pending.pop() {
        if !seen.insert(&record.id) {
            continue;
        }
        if seen.len() > LIMIT {
            return Err(invalid("causal graph exceeds 1000 nodes"));
        }
        records.push((record, point_time(world, record)?));
        for cause in causes(world, record)? {
            edges.insert((&cause.id, &record.id));
            if edges.len() > LIMIT {
                return Err(invalid("causal graph exceeds 1000 edges"));
            }
            pending.push(cause);
        }
    }
    records.sort_by(|(a, ta), (b, tb)| (ta.tick, ta.order, &a.id).cmp(&(tb.tick, tb.order, &b.id)));
    let nodes:Vec<_>=records.iter().map(|(record,time)|json!({"eventId":record.id,"title":record.title,"time":time,"citation":cite(world,record,"time/causes")})).collect();
    let links: Vec<_> = edges
        .into_iter()
        .map(|(from, to)| json!({"causeId":from,"eventId":to}))
        .collect();
    let mut result = envelope(world, at);
    result["targetEventId"] = json!(root.id);
    result["nodes"] = json!(nodes);
    result["edges"] = json!(links);
    Ok(result)
}

struct Environment<'a> {
    record: &'a Record,
    start: StoryTime,
    end: Option<StoryTime>,
    targets: Vec<String>,
    conditions: Json,
    sensory: Vec<String>,
}
fn environment<'a>(world: &World, record: &'a Record) -> Result<Environment<'a>> {
    let time = required(&record.frontmatter, "time")?;
    closed(time, &["start"], &["end"], "environment time")?;
    let start = point(world, required(time, "start")?)?;
    let end = time
        .get("end")
        .filter(|v| !v.is_null())
        .map(|v| point(world, v))
        .transpose()?;
    if end.as_ref().is_some_and(|end| !start.not_after(end)) {
        return Err(invalid(
            "environment end must be at or after start on the same timeline",
        ));
    }
    let targets = strings(
        required(&record.frontmatter, "targets")?,
        None,
        "environment targets",
        true,
    )?;
    for id in &targets {
        if !world.records.contains_key(id) {
            return Err(invalid(format!("unknown environment target {id}")));
        }
    }
    let conditions = match record.get("conditions") {
        None => json!({}),
        Some(value) => {
            let mut conditions = serde_json::Map::new();
            for (key, value) in map(value, "environment conditions")? {
                let key = text(key, "environment condition key")?;
                let scalar = match value {
                    Value::Null | Value::Bool(_) | Value::String(_) => serde_json::to_value(value)?,
                    Value::Number(n) => {
                        if let Some(integer) = n.as_i64() {
                            Json::String(integer.to_string())
                        } else if let Some(integer) = n.as_u64() {
                            Json::String(integer.to_string())
                        } else {
                            serde_json::to_value(number(value, "environment condition number")?)?
                        }
                    }
                    _ => return Err(invalid("environment conditions must be flat scalar values")),
                };
                conditions.insert(key.into(), scalar);
            }
            Json::Object(conditions)
        }
    };
    let sensory = record
        .get("sensory")
        .map(|v| strings(v, None, "sensory descriptions", false))
        .transpose()?
        .unwrap_or_default();
    Ok(Environment {
        record,
        start,
        end,
        targets,
        conditions,
        sensory,
    })
}

pub fn environments(world: &World, entity: &str, at: &StoryTime) -> Result<Json> {
    horizon(world, at)?;
    let target = world.resolve(entity)?;
    let mut items = Vec::new();
    for record in world
        .records
        .values()
        .filter(|r| r.kind == "environment" && r.status() == "canonical")
    {
        let value = environment(world, record)?;
        if value.targets.contains(&target.id)
            && value.start.not_after(at)
            && value.end.as_ref().is_none_or(|end| at.not_after(end))
        {
            items.push(json!({"environmentId":value.record.id,"title":value.record.title,"targets":value.targets,
                "time":{"start":value.start,"end":value.end},"conditions":value.conditions,
                "sensory":value.sensory,"citation":cite(world,value.record,"targets/time/conditions")}));
            if items.len() > LIMIT {
                return Err(invalid("environment result exceeds 1000 records"));
            }
        }
    }
    let mut result = envelope(world, at);
    result["entityId"] = json!(target.id);
    result["scope"] = json!("explicit targets only");
    result["items"] = json!(items);
    Ok(result)
}

pub fn validate(world: &World) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut points = Vec::new();
    let mut point_failed = false;
    let diagnostic = |record: &Record, field: &str, error: Error| Diagnostic {
        code: "WRS-STORY".into(),
        message: error.to_string(),
        severity: "error".into(),
        entity_id: Some(record.id.clone()),
        path: Some(record.source_path.clone()),
        field: Some(field.into()),
    };
    for record in world.records.values() {
        let (field, result) = match record.kind.as_str() {
            "story-point" => match story_point(world, record) {
                Ok(point) => {
                    points.push(point);
                    continue;
                }
                Err(error) => {
                    point_failed = true;
                    ("lifecycle/requires/when", Err(error))
                }
            },
            "event" if record.status() == "canonical" => {
                ("causes", causes(world, record).map(|_| ()))
            }
            "environment" => ("targets/time", environment(world, record).map(|_| ())),
            _ => continue,
        };
        if let Err(error) = result {
            diagnostics.push(diagnostic(record, field, error));
        }
    }
    if !point_failed {
        if let Err(error) = acyclic(&points) {
            if let Some(first) = points.first() {
                diagnostics.push(diagnostic(first.record, "requires", error));
            }
        }
    }
    diagnostics
}
