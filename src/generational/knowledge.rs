//! Literal, fallible generational beliefs. Learning time controls admission;
//! the independently authored inclusive validity interval controls applicability.
//! No query in this module joins canonical genealogy or canonical names.
use super::{LineageDirection, bounded, closed, field, invalid, limits, point, text};
use crate::{
    Error, Result,
    model::{Record, StoryTime, World},
    scene,
    validation::{Diagnostic, valid_id_for_kind},
};
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const CAPABILITY: &str = "generational-knowledge-v1";
const KINDS: &[&str] = &[
    "parentage",
    "union",
    "organization",
    "affiliation",
    "tenure",
    "claim",
    "vital",
];
const HELD: &[&str] = &["accepted", "suspected", "uncertain", "remembered"];
const STATES: &[&str] = &[
    "accepted",
    "suspected",
    "rejected",
    "uncertain",
    "remembered",
    "forgotten",
];
const ACQUISITIONS: &[&str] = &[
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

struct Assertion {
    kind: String,
    value: Json,
    labels: Json,
    from: StoryTime,
    until: Option<StoryTime>,
    evidence: Vec<Evidence>,
}
struct Evidence {
    kind: String,
    entity: String,
    member: String,
}
struct Transition<'a> {
    id: &'a str,
    time: StoryTime,
    state: &'a str,
    confidence: f64,
}

fn list<'a>(value: &'a Yaml, label: &str) -> Result<&'a [Yaml]> {
    match value {
        Yaml::Sequence(values) => Ok(values),
        _ => Err(invalid(label, "expected an untagged array")),
    }
}
fn literal<'a>(value: &'a Yaml, choices: &[&str], label: &str) -> Result<&'a str> {
    let value = text(value, label)?;
    if !choices.contains(&value) {
        return Err(invalid(label, "unsupported literal"));
    }
    Ok(value)
}
fn endpoint<'a>(
    world: &World,
    value: &'a Yaml,
    kind: &str,
    label: &str,
    check: bool,
) -> Result<&'a str> {
    let id = text(value, label)?;
    if !valid_id_for_kind(id, kind)
        || (check
            && !world
                .records
                .get(id)
                .is_some_and(|record| record.kind == kind))
    {
        return Err(invalid(
            label,
            format!("expected an existing exact {kind} reference"),
        ));
    }
    Ok(id)
}
fn reference_fields(kind: &str) -> &'static [(&'static str, &'static str)] {
    match kind {
        "parentage" => &[("child_id", "character"), ("parent_id", "character")],
        "union" => &[("participant_ids", "character")],
        "organization" => &[
            ("organization_id", "organization"),
            ("parent_id", "organization"),
        ],
        "affiliation" => &[
            ("character_id", "character"),
            ("organization_id", "organization"),
        ],
        "tenure" => &[("legacy_id", "legacy"), ("holder_id", "character")],
        "claim" => &[("legacy_id", "legacy"), ("claimant_id", "character")],
        "vital" => &[("character_id", "character")],
        _ => &[],
    }
}
fn nullable(kind: &str, name: &str) -> bool {
    matches!(
        (kind, name),
        ("organization", "parent_id") | ("tenure", "holder_id")
    )
}
fn bounded_text(value: &Yaml, label: &str) -> Result<()> {
    if text(value, label)?.chars().count() > 200 {
        return Err(invalid(label, "expected at most 200 characters"));
    }
    Ok(())
}

fn assertion(world: &World, value: &Yaml, check_references: bool) -> Result<Assertion> {
    let label = "claim.genealogy";
    let fields = closed(
        value,
        &["kind", "payload", "valid"],
        &["labels", "evidence"],
        label,
    )?;
    let kind = literal(&fields["kind"], KINDS, "claim.genealogy.kind")?;
    let required: &[&str] = match kind {
        "parentage" => &["child_id", "parent_id", "basis"],
        "union" => &["participant_ids", "state"],
        "organization" => &["organization_id", "parent_id"],
        "affiliation" => &["character_id", "organization_id", "role"],
        "tenure" => &["legacy_id", "holder_id", "basis"],
        "claim" => &["legacy_id", "claimant_id", "state"],
        "vital" => &["character_id", "state"],
        _ => unreachable!(),
    };
    let payload = closed(&fields["payload"], required, &[], "claim.genealogy.payload")?;
    let mut references = BTreeSet::new();
    for (name, expected) in reference_fields(kind) {
        let value = &payload[*name];
        let label = format!("claim.genealogy.payload.{name}");
        if *name == "participant_ids" {
            let values = list(value, &label)?;
            if !(2..=32).contains(&values.len()) {
                return Err(invalid(
                    &label,
                    "expected 2..32 sorted unique character IDs",
                ));
            }
            let mut previous = None;
            for value in values {
                let id = endpoint(world, value, expected, &label, check_references)?;
                if previous.is_some_and(|old| old >= id) {
                    return Err(invalid(&label, "participants must be sorted and unique"));
                }
                previous = Some(id);
                references.insert(id);
            }
        } else if !matches!(value, Yaml::Null) || !nullable(kind, name) {
            references.insert(endpoint(world, value, expected, &label, check_references)?);
        }
    }
    match kind {
        "parentage" => {
            literal(
                &payload["basis"],
                &["biological", "adoptive"],
                "claim.genealogy.payload.basis",
            )?;
        }
        "union" => {
            literal(
                &payload["state"],
                &["formed", "ended", "annulled"],
                "claim.genealogy.payload.state",
            )?;
        }
        "tenure" => {
            literal(
                &payload["basis"],
                &["legal", "de-facto"],
                "claim.genealogy.payload.basis",
            )?;
        }
        "claim" => {
            literal(
                &payload["state"],
                &[
                    "proposed",
                    "disputed",
                    "recognized",
                    "withdrawn",
                    "rejected",
                ],
                "claim.genealogy.payload.state",
            )?;
        }
        "vital" => {
            literal(
                &payload["state"],
                &["living", "dead", "existing", "ended"],
                "claim.genealogy.payload.state",
            )?;
        }
        "affiliation" if !matches!(&payload["role"], Yaml::Null) => {
            bounded_text(&payload["role"], "claim.genealogy.payload.role")?
        }
        _ => (),
    }
    let valid = closed(
        &fields["valid"],
        &["from"],
        &["until"],
        "claim.genealogy.valid",
    )?;
    let from = point(world, &valid["from"], "claim.genealogy.valid.from")?;
    let until = valid
        .get("until")
        .map(|value| point(world, value, "claim.genealogy.valid.until"))
        .transpose()?;
    if until.as_ref().is_some_and(|until| !from.not_after(until)) {
        return Err(invalid(
            "claim.genealogy.valid",
            "until must not precede from or change timeline",
        ));
    }
    let mut labels = serde_json::Map::new();
    if let Some(value) = fields.get("labels") {
        let Yaml::Mapping(values) = value else {
            return Err(invalid(
                "claim.genealogy.labels",
                "expected an untagged mapping",
            ));
        };
        if values.len() > 32 {
            return Err(invalid(
                "claim.genealogy.labels",
                "expected at most 32 labels",
            ));
        }
        for (id, name) in values {
            let id = text(id, "claim.genealogy.labels")?;
            if !references.contains(id) {
                return Err(invalid(
                    "claim.genealogy.labels",
                    "labels may name only asserted endpoints",
                ));
            }
            bounded_text(name, "claim.genealogy.labels")?;
            labels.insert(id.into(), json!(text(name, "claim.genealogy.labels")?));
        }
    }
    let mut evidence = Vec::new();
    if let Some(values) = fields.get("evidence") {
        let values = list(values, "claim.genealogy.evidence")?;
        if values.len() > 32 {
            return Err(invalid(
                "claim.genealogy.evidence",
                "expected at most 32 references",
            ));
        }
        let mut seen = BTreeSet::new();
        for (index, value) in values.iter().enumerate() {
            let label = format!("claim.genealogy.evidence[{index}]");
            let tag = value
                .get("kind")
                .ok_or_else(|| invalid(&label, "kind is required"))?;
            let tag = literal(
                tag,
                &["knowledge", "observation", "turn", "recollection"],
                &label,
            )?;
            let (entity_kind, member_field) = match tag {
                "knowledge" => ("knowledge", "transition_id"),
                "observation" => ("scene", "observation_id"),
                "turn" => ("conversation", "turn_id"),
                "recollection" => ("conversation", "recollection_id"),
                _ => unreachable!(),
            };
            let fields = closed(value, &["kind", "entity_id", member_field], &[], &label)?;
            let entity = endpoint(
                world,
                &fields["entity_id"],
                entity_kind,
                &label,
                check_references,
            )?;
            // Existing source members use stable, nonblank IDs; this capability
            // does not change the older knowledge/scene member-ID grammar.
            let member = text(&fields[member_field], &label)?;
            if !seen.insert((tag, entity, member)) {
                return Err(invalid(&label, "duplicate evidence reference"));
            }
            evidence.push(Evidence {
                kind: tag.into(),
                entity: entity.into(),
                member: member.into(),
            });
        }
    }
    Ok(Assertion {
        kind: kind.into(),
        value: serde_json::to_value(&fields["payload"])
            .map_err(|error| invalid("claim.genealogy.payload", error))?,
        labels: Json::Object(labels),
        from,
        until,
        evidence,
    })
}

fn learning_point(world: &World, value: &Yaml, label: &str) -> Result<StoryTime> {
    let fields = closed(value, &["tick"], &["timeline", "order"], label)?;
    if !matches!(&fields["tick"], Yaml::Number(n) if n.as_i64().is_some()) {
        return Err(invalid(
            label,
            "tick must be an exact signed 64-bit integer",
        ));
    }
    if let Some(value) = fields.get("order") {
        if !matches!(value, Yaml::Number(n) if n.as_i64().and_then(|n| i32::try_from(n).ok()).is_some())
        {
            return Err(invalid(
                label,
                "order must be an exact signed 32-bit integer",
            ));
        }
    }
    if let Some(value) = fields.get("timeline") {
        text(value, label)?;
    }
    let at =
        StoryTime::from_yaml(value, &world.default_timeline()).map_err(|e| invalid(label, e))?;
    world
        .story_time(at.tick, Some(&at.timeline), at.order)
        .map_err(|e| invalid(label, e))
}
fn transitions<'a>(
    world: &World,
    record: &'a Record,
    validate_source: bool,
) -> Result<Vec<Transition<'a>>> {
    let mut result: Vec<Transition<'a>> = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, value) in list(field(record, "transitions")?, "transitions")?
        .iter()
        .enumerate()
    {
        let label = format!("transitions[{index}]");
        let fields = closed(
            value,
            &["id", "time", "state", "confidence", "acquisition"],
            &["source_entity", "causing_event", "note"],
            &label,
        )?;
        let id = text(&fields["id"], &format!("{label}.id"))?;
        if !seen.insert(id) {
            return Err(invalid(&label, "duplicate transition ID"));
        }
        let time = learning_point(world, &fields["time"], &format!("{label}.time"))?;
        let state = literal(&fields["state"], STATES, &format!("{label}.state"))?;
        literal(
            &fields["acquisition"],
            ACQUISITIONS,
            &format!("{label}.acquisition"),
        )?;
        let confidence = match &fields["confidence"] {
            Yaml::Number(n) => n
                .as_f64()
                .filter(|n| n.is_finite() && (0.0..=1.0).contains(n)),
            _ => None,
        }
        .ok_or_else(|| {
            invalid(
                &label,
                "confidence must be a finite number between zero and one",
            )
        })?;
        if validate_source {
            if result
                .last()
                .is_some_and(|previous| !previous.time.not_after(&time) || previous.time == time)
            {
                return Err(invalid(
                    &label,
                    "transitions must be strictly ordered on one timeline",
                ));
            }
            for name in ["source_entity", "causing_event"] {
                if let Some(value) = fields
                    .get(name)
                    .filter(|value| !matches!(value, Yaml::Null))
                {
                    let id = text(value, &format!("{label}.{name}"))?;
                    let target = world
                        .records
                        .get(id)
                        .filter(|target| valid_id_for_kind(id, &target.kind))
                        .ok_or_else(|| {
                            invalid(&label, "expected an existing exact source reference")
                        })?;
                    if name == "causing_event"
                        && (target.kind != "event"
                            || target.status() != "canonical"
                            || !learning_point(world, field(target, "time")?, &label)?
                                .not_after(&time))
                    {
                        return Err(invalid(
                            &label,
                            "causing_event must be canonical and at or before this transition",
                        ));
                    }
                }
            }
            if !matches!(fields.get("note"), None | Some(Yaml::String(_))) {
                return Err(invalid(&label, "note must be untagged prose"));
            }
        }
        result.push(Transition {
            id,
            time,
            state,
            confidence,
        });
    }
    Ok(result)
}
fn latest<'a, 'b>(transitions: &'b [Transition<'a>], at: &StoryTime) -> Option<&'b Transition<'a>> {
    // Equal-time source order is the existing knowledge fold's tie breaker.
    transitions
        .iter()
        .enumerate()
        .filter(|(_, t)| t.time.not_after(at))
        .max_by_key(|(index, t)| (t.time.tick, t.time.order, *index))
        .map(|(_, t)| t)
}
fn learned(transitions: &[Transition<'_>]) -> Option<StoryTime> {
    transitions
        .iter()
        .filter(|t| HELD.contains(&t.state))
        .min_by_key(|t| (t.time.tick, t.time.order))
        .map(|t| t.time.clone())
}

/// Exact source evidence is checked at first learning, never by following its
/// current canonical text. Expired observations remain usable historical proof.
fn evidence_time(
    world: &World,
    evidence: &Evidence,
    knower: &str,
    at: &StoryTime,
) -> Result<Option<StoryTime>> {
    let Some(record) = world.records.get(&evidence.entity) else {
        return Ok(None);
    };
    match evidence.kind.as_str() {
        "knowledge"
            if record.kind == "knowledge"
                && record.status() == "canonical"
                && record.string("knower") == Some(knower) =>
        {
            if !crate::narrative::labels_match(record, knower)? {
                return Ok(None);
            }
            let transitions = transitions(world, record, false)?;
            if !latest(&transitions, at).is_some_and(|t| HELD.contains(&t.state)) {
                return Ok(None);
            }
            Ok(transitions
                .iter()
                .find(|t| {
                    t.id == evidence.member && HELD.contains(&t.state) && t.time.not_after(at)
                })
                .map(|t| t.time.clone()))
        }
        "observation"
            if record.kind == "scene" && ["active", "closed"].contains(&record.status()) =>
        {
            let Some(value) = record
                .list("observations")
                .into_iter()
                .flatten()
                .find(|value| {
                    value.get("id").and_then(Yaml::as_str) == Some(evidence.member.as_str())
                })
            else {
                return Ok(None);
            };
            let time = learning_point(
                world,
                value
                    .get("at")
                    .ok_or_else(|| invalid("evidence", "observation time is missing"))?,
                "evidence.at",
            )?;
            Ok((time.not_after(at)
                && scene::observation_visible(world, record, value, knower, &time)?)
            .then_some(time))
        }
        "turn"
            if record.kind == "conversation" && ["active", "closed"].contains(&record.status()) =>
        {
            let Some(value) = record.list("turns").into_iter().flatten().find(|value| {
                value.get("id").and_then(Yaml::as_str) == Some(evidence.member.as_str())
            }) else {
                return Ok(None);
            };
            if value.get("kind").and_then(Yaml::as_str).unwrap_or("speech") != "speech" {
                return Ok(None);
            }
            let time = scene::turn_time(world, record, value)?;
            Ok(
                (time.not_after(at) && scene::audible(world, record, value, knower)?)
                    .then_some(time),
            )
        }
        "recollection"
            if record.kind == "conversation" && ["active", "closed"].contains(&record.status()) =>
        {
            let mut current: Option<(&Yaml, StoryTime)> = None;
            for value in record
                .list("recollections")
                .into_iter()
                .flatten()
                .filter(|value| value.get("character").and_then(Yaml::as_str) == Some(knower))
            {
                let time = learning_point(
                    world,
                    value
                        .get("at")
                        .ok_or_else(|| invalid("evidence", "recollection time is missing"))?,
                    "evidence.at",
                )?;
                if time.not_after(at)
                    && current.as_ref().is_none_or(|(old, previous)| {
                        (
                            time.tick,
                            time.order,
                            value.get("id").and_then(Yaml::as_str),
                        ) > (
                            previous.tick,
                            previous.order,
                            old.get("id").and_then(Yaml::as_str),
                        )
                    })
                {
                    current = Some((value, time));
                }
            }
            Ok(current
                .filter(|(value, _)| {
                    value.get("id").and_then(Yaml::as_str) == Some(evidence.member.as_str())
                        && value.get("state").and_then(Yaml::as_str) == Some("remembered")
                })
                .map(|(_, time)| time))
        }
        _ => Ok(None),
    }
}
fn evidence(
    world: &World,
    assertion: &Assertion,
    knower: &str,
    learned_at: &StoryTime,
) -> Result<Vec<Json>> {
    assertion.evidence.iter().map(|item| {
        let time = evidence_time(world, item, knower, learned_at)?.ok_or_else(|| invalid("claim.genealogy.evidence", "evidence must be exact, admitted for this character and no later than learning"))?;
        Ok(json!({"kind":item.kind,"entityId":item.entity,"itemId":item.member,"time":time}))
    }).collect()
}
fn capability(world: &World, name: &str) -> bool {
    world
        .world_record()
        .ok()
        .and_then(|record| record.list("capabilities"))
        .is_some_and(|values| {
            values
                .iter()
                .any(|value| matches!(value, Yaml::String(value) if value == name))
        })
}
fn issue(record: &Record, code: &str, field: &str, error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic {
        code: code.into(),
        message: error.to_string(),
        severity: "error".into(),
        entity_id: Some(record.id.clone()),
        path: Some(record.source_path.clone()),
        field: Some(field.into()),
    }
}
/// Validate only the opted-in typed knowledge source, without comparing beliefs
/// with canonical state, checking graph consistency or deriving an assertion.
pub fn validate(world: &World) -> Vec<Diagnostic> {
    let mut result = Vec::new();
    for record in world.records.values().filter(|record| {
        record.kind == "knowledge"
            && record
                .get("claim")
                .and_then(|claim| claim.get("genealogy"))
                .is_some()
    }) {
        if !capability(world, CAPABILITY) || !capability(world, "generational-core-v1") {
            result.push(issue(
                record,
                "GEN-KNOWLEDGE-001",
                "claim.genealogy",
                "typed genealogy requires generational-knowledge-v1 and generational-core-v1",
            ));
        }
        let parsed = (|| {
            let claim = closed(
                field(record, "claim")?,
                &["key", "statement", "genealogy"],
                &[],
                "claim",
            )?;
            text(&claim["key"], "claim.key")?;
            text(&claim["statement"], "claim.statement")?;
            let knower = endpoint(world, field(record, "knower")?, "character", "knower", true)?;
            let assertion = assertion(world, &claim["genealogy"], true)?;
            let transitions = transitions(world, record, true)?;
            Ok::<_, Error>((knower, assertion, transitions))
        })();
        let (knower, assertion, transitions) = match parsed {
            Ok(value) => value,
            Err(error) => {
                result.push(issue(record, "GEN-KNOWLEDGE-002", "claim.genealogy", error));
                continue;
            }
        };
        let Some(learned_at) = learned(&transitions) else {
            result.push(issue(
                record,
                "GEN-KNOWLEDGE-005",
                "transitions",
                "typed genealogy requires an explicit affirmative learning transition",
            ));
            continue;
        };
        if transitions
            .iter()
            .any(|t| HELD.contains(&t.state) && t.time.timeline != assertion.from.timeline)
        {
            result.push(issue(
                record,
                "GEN-KNOWLEDGE-004",
                "transitions",
                "learning and asserted applicability must share a timeline",
            ));
            continue;
        }
        if let Err(error) = evidence(world, &assertion, knower, &learned_at) {
            result.push(issue(
                record,
                "GEN-KNOWLEDGE-006",
                "claim.genealogy.evidence",
                error,
            ));
        }
    }
    result
}

fn malformed() -> Error {
    Error::Validation(
        "character evidence is malformed; use author validation for diagnostics".into(),
    )
}
fn select(world: &World, viewer: &str, at: &StoryTime) -> Result<()> {
    if !valid_id_for_kind(viewer, "character")
        || !world
            .records
            .get(viewer)
            .is_some_and(|record| record.kind == "character")
    {
        return Err(Error::NotFound("character view is unavailable".into()));
    }
    world.story_time(at.tick, Some(&at.timeline), at.order)?;
    Ok(())
}
fn subject(id: &str, kind: &str) -> Result<()> {
    // Check selector syntax only. Looking up a subject's canonical record would
    // make the presence of otherwise unlearned facts observable.
    if !valid_id_for_kind(id, kind) {
        return Err(Error::InvalidInput(format!(
            "learned query requires an exact {kind} ID"
        )));
    }
    Ok(())
}
fn envelope(viewer: &str, at: &StoryTime, limit: usize) -> Json {
    json!({"protocol":crate::CONTRACT,"perspective":"character","scope":"explicit-authored-evidence",
        "characterId":viewer,"effectiveTime":at,"complete":true,"limit":limit})
}

/// Shared admission gate. There is deliberately no global candidate budget:
/// callers first filter admitted evidence for their requested subject or text.
/// `held_history` retains learned labels/history outside assertion validity.
pub(crate) fn admitted(
    world: &World,
    viewer: &str,
    at: &StoryTime,
    held_history: bool,
) -> Result<Vec<Json>> {
    select(world, viewer, at)?;
    if !capability(world, CAPABILITY) {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for record in world.records.values().filter(|record| {
        record.kind == "knowledge"
            && record.status() == "canonical"
            && record.string("knower") == Some(viewer)
    }) {
        if !crate::narrative::labels_match(record, viewer).map_err(|_| malformed())? {
            continue;
        }
        let Some(value) = record.get("claim").and_then(|claim| claim.get("genealogy")) else {
            continue;
        };
        let transitions = transitions(world, record, false).map_err(|_| malformed())?;
        let Some(current) = latest(&transitions, at).filter(|t| HELD.contains(&t.state)) else {
            continue;
        };
        let learned_at = learned(&transitions).ok_or_else(malformed)?;
        if !learned_at.not_after(at) {
            continue;
        }
        // Parsing literal endpoint IDs never resolves canonical records/names.
        let assertion = assertion(world, value, false).map_err(|_| malformed())?;
        if assertion.from.timeline != at.timeline {
            continue;
        }
        let applicable = assertion.from.not_after(at)
            && assertion
                .until
                .as_ref()
                .is_none_or(|until| at.not_after(until));
        if !held_history && !applicable {
            continue;
        }
        let evidence = evidence(world, &assertion, viewer, &learned_at).map_err(|_| malformed())?;
        let row = json!({"recordId":record.id,"knowledgeId":record.id,"kind":assertion.kind,
            "value":assertion.value,"labels":assertion.labels,"valid":{"from":assertion.from,"until":assertion.until},
            "learnedAt":learned_at,"transitionId":current.id,"time":current.time,
            "beliefState":current.state,"confidence":current.confidence,"applicable":applicable,
            "uncertain":current.state != "accepted" || current.confidence < 1.0,
            "citations":[{"knowledgeId":record.id,"transitionId":current.id,"time":current.time,"learnedAt":learned_at}],"evidence":evidence});
        result.push((learned_at.tick, learned_at.order, record.id.as_str(), row));
    }
    result.sort_by(|a, b| (&a.0, &a.1, a.2).cmp(&(&b.0, &b.1, b.2)));
    Ok(result.into_iter().map(|(_, _, _, row)| row).collect())
}
fn involves(row: &Json, endpoint: &str) -> bool {
    reference_fields(row["kind"].as_str().unwrap_or(""))
        .iter()
        .any(|(field, _)| {
            let value = &row["value"][*field];
            value.as_str() == Some(endpoint)
                || value.as_array().is_some_and(|values| {
                    values.iter().any(|value| value.as_str() == Some(endpoint))
                })
        })
}
/// Current applicable assertions, or currently held assertion history.
pub fn assertions(
    world: &World,
    viewer: &str,
    at: &StoryTime,
    kind: Option<&str>,
    endpoint: Option<&str>,
    held_history: bool,
    limit: usize,
) -> Result<Json> {
    limits(limit, 0)?;
    if kind.is_some_and(|kind| !KINDS.contains(&kind)) {
        return Err(Error::InvalidInput(
            "unsupported learned assertion kind".into(),
        ));
    }
    if let Some(id) = endpoint {
        if !["character", "organization", "legacy"]
            .iter()
            .any(|kind| valid_id_for_kind(id, kind))
        {
            return Err(Error::InvalidInput(
                "assertion endpoint must be an exact character, organization or legacy ID".into(),
            ));
        }
    }
    let items: Vec<_> = admitted(world, viewer, at, held_history)?
        .into_iter()
        .filter(|row| {
            kind.is_none_or(|kind| row["kind"] == kind)
                && endpoint.is_none_or(|id| involves(row, id))
        })
        .collect();
    bounded(items.len(), limit)?;
    let mut result = envelope(viewer, at, limit);
    result["kind"] = json!(kind);
    result["endpointId"] = json!(endpoint);
    result["heldHistory"] = json!(held_history);
    result["count"] = json!(items.len());
    result["items"] = json!(items);
    Ok(result)
}

pub fn family(
    world: &World,
    viewer: &str,
    character: &str,
    at: &StoryTime,
    limit: usize,
) -> Result<Json> {
    limits(limit, 0)?;
    subject(character, "character")?;
    let mut parents = Vec::new();
    let mut children = Vec::new();
    let mut unions = Vec::new();
    for row in admitted(world, viewer, at, false)? {
        if row["kind"] == "parentage" {
            if row["value"]["child_id"] == character {
                let mut edge = row.clone();
                edge["targetId"] = row["value"]["parent_id"].clone();
                parents.push(edge);
            }
            if row["value"]["parent_id"] == character {
                let mut edge = row.clone();
                edge["targetId"] = row["value"]["child_id"].clone();
                children.push(edge);
            }
        } else if row["kind"] == "union" && involves(&row, character) {
            unions.push(row);
        }
    }
    let count = parents.len() + children.len() + unions.len();
    bounded(count, limit)?;
    let mut result = envelope(viewer, at, limit);
    result["subjectId"] = json!(character);
    result["parents"] = json!(parents);
    result["children"] = json!(children);
    result["unions"] = json!(unions);
    result["count"] = json!(count);
    Ok(result)
}

#[derive(Clone)]
struct Edge {
    from: String,
    to: String,
    row: Json,
}
type Graph = BTreeMap<String, Vec<Edge>>;
fn graph(rows: &[Json], kind: &str, direction: Option<LineageDirection>) -> Graph {
    let mut result: Graph = BTreeMap::new();
    let from_field = if kind == "parentage" {
        "child_id"
    } else {
        "organization_id"
    };
    for row in rows.iter().filter(|row| row["kind"] == kind) {
        let (Some(from), Some(to)) = (
            row["value"][from_field].as_str(),
            row["value"]["parent_id"].as_str(),
        ) else {
            continue;
        };
        if direction != Some(LineageDirection::Descendants) {
            result.entry(from.into()).or_default().push(Edge {
                from: from.into(),
                to: to.into(),
                row: row.clone(),
            });
        }
        if kind == "parentage" && direction != Some(LineageDirection::Ancestors) {
            result.entry(to.into()).or_default().push(Edge {
                from: to.into(),
                to: from.into(),
                row: row.clone(),
            });
        }
    }
    for edges in result.values_mut() {
        edges.sort_by(|a, b| {
            a.row["knowledgeId"]
                .as_str()
                .cmp(&b.row["knowledgeId"].as_str())
                .then(a.to.cmp(&b.to))
        });
    }
    result
}
fn path_value(path: &[Edge]) -> Json {
    let edges: Vec<_> = path
        .iter()
        .map(|edge| {
            let mut row = edge.row.clone();
            row["from"] = json!(edge.from);
            row["to"] = json!(edge.to);
            row
        })
        .collect();
    json!({"edges":edges,"uncertain":path.iter().any(|edge| edge.row["uncertain"] == true)})
}
fn all_paths(graph: &Graph, from: &str, max_depth: usize, limit: usize) -> Result<Vec<Json>> {
    let mut seen = BTreeSet::from([from.to_owned()]);
    let mut queue = VecDeque::from([(from.to_owned(), Vec::<Edge>::new())]);
    let mut paths = BTreeMap::new();
    while let Some((node, path)) = queue.pop_front() {
        for edge in graph.get(&node).into_iter().flatten() {
            if seen.contains(&edge.to) {
                continue;
            }
            if path.len() == max_depth {
                return Err(Error::InvalidInput(
                    "learned lineage exceeds maximum depth; increase depth for complete results"
                        .into(),
                ));
            }
            bounded(paths.len() + 1, limit)?;
            let mut next = path.clone();
            next.push(edge.clone());
            let mut row = path_value(&next);
            row["targetId"] = json!(edge.to);
            row["generationDistance"] = json!(next.len().to_string());
            paths.insert(edge.to.clone(), row);
            seen.insert(edge.to.clone());
            queue.push_back((edge.to.clone(), next));
        }
    }
    Ok(paths.into_values().collect())
}
/// One shortest witnessed path per endpoint; stable knowledge IDs break ties.
pub fn lineage(
    world: &World,
    viewer: &str,
    character: &str,
    at: &StoryTime,
    direction: LineageDirection,
    max_depth: usize,
    limit: usize,
) -> Result<Json> {
    limits(limit, max_depth)?;
    subject(character, "character")?;
    let rows = admitted(world, viewer, at, false)?;
    let items = all_paths(
        &graph(&rows, "parentage", Some(direction)),
        character,
        max_depth,
        limit,
    )?;
    let mut result = envelope(viewer, at, limit);
    result["subjectId"] = json!(character);
    result["direction"] = json!(if direction == LineageDirection::Ancestors {
        "ancestors"
    } else {
        "descendants"
    });
    result["maxDepth"] = json!(max_depth);
    result["count"] = json!(items.len());
    result["items"] = json!(items);
    Ok(result)
}
/// One shortest undirected parentage witness, with uncertainty retained.
pub fn kinship_path(
    world: &World,
    viewer: &str,
    from: &str,
    to: &str,
    at: &StoryTime,
    max_hops: usize,
    limit: usize,
) -> Result<Json> {
    limits(limit, max_hops)?;
    subject(from, "character")?;
    subject(to, "character")?;
    let rows = admitted(world, viewer, at, false)?;
    let graph = graph(&rows, "parentage", None);
    let mut seen = BTreeSet::from([from.to_owned()]);
    let mut queue = VecDeque::from([(from.to_owned(), Vec::<Edge>::new())]);
    let mut matched = (from == to).then(Vec::new);
    'search: while matched.is_none() {
        let Some((node, path)) = queue.pop_front() else {
            break;
        };
        for edge in graph.get(&node).into_iter().flatten() {
            if seen.contains(&edge.to) {
                continue;
            }
            if path.len() == max_hops {
                return Err(Error::InvalidInput(
                    "learned kinship path exceeds maximum hops before search is complete".into(),
                ));
            }
            bounded(seen.len(), limit)?;
            let mut next = path.clone();
            next.push(edge.clone());
            seen.insert(edge.to.clone());
            if edge.to == to {
                matched = Some(next);
                break 'search;
            }
            queue.push_back((edge.to.clone(), next));
        }
    }
    let mut result = envelope(viewer, at, limit);
    result["fromId"] = json!(from);
    result["toId"] = json!(to);
    result["maxHops"] = json!(max_hops);
    result["found"] = json!(matched.is_some());
    result["reason"] = if matched.is_some() {
        Json::Null
    } else {
        json!("no_match")
    };
    result["distance"] = json!(matched.as_ref().map(|path| path.len().to_string()));
    let path = path_value(matched.as_deref().unwrap_or(&[]));
    result["edges"] = path["edges"].clone();
    result["uncertain"] = path["uncertain"].clone();
    result["visitedCount"] = json!(seen.len());
    Ok(result)
}

pub fn organization(
    world: &World,
    viewer: &str,
    organization: &str,
    at: &StoryTime,
    max_depth: usize,
    limit: usize,
) -> Result<Json> {
    limits(limit, max_depth)?;
    subject(organization, "organization")?;
    let rows = admitted(world, viewer, at, false)?;
    let parents: Vec<_> = rows
        .iter()
        .filter(|row| {
            row["kind"] == "organization" && row["value"]["organization_id"] == organization
        })
        .cloned()
        .collect();
    let affiliations: Vec<_> = rows
        .iter()
        .filter(|row| {
            row["kind"] == "affiliation" && row["value"]["organization_id"] == organization
        })
        .cloned()
        .collect();
    let paths = all_paths(
        &graph(&rows, "organization", Some(LineageDirection::Ancestors)),
        organization,
        max_depth,
        limit,
    )?;
    bounded(parents.len() + affiliations.len() + paths.len(), limit)?;
    let mut result = envelope(viewer, at, limit);
    result["subjectId"] = json!(organization);
    result["maxDepth"] = json!(max_depth);
    result["count"] = json!(parents.len() + affiliations.len() + paths.len());
    result["parents"] = json!(parents);
    result["affiliations"] = json!(affiliations);
    result["paths"] = json!(paths);
    Ok(result)
}
pub fn legacy(
    world: &World,
    viewer: &str,
    legacy: &str,
    at: &StoryTime,
    limit: usize,
) -> Result<Json> {
    limits(limit, 0)?;
    subject(legacy, "legacy")?;
    let rows = admitted(world, viewer, at, false)?;
    let tenures: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "tenure" && row["value"]["legacy_id"] == legacy)
        .cloned()
        .collect();
    let claims: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "claim" && row["value"]["legacy_id"] == legacy)
        .cloned()
        .collect();
    bounded(tenures.len() + claims.len(), limit)?;
    let mut result = envelope(viewer, at, limit);
    result["subjectId"] = json!(legacy);
    result["count"] = json!(tenures.len() + claims.len());
    result["holders"] = json!(
        tenures
            .iter()
            .filter(|row| !row["value"]["holder_id"].is_null())
            .collect::<Vec<_>>()
    );
    result["vacancies"] = json!(
        tenures
            .iter()
            .filter(|row| row["value"]["holder_id"].is_null())
            .collect::<Vec<_>>()
    );
    result["succession"] = json!([]);
    result["tenures"] = json!(tenures);
    result["claims"] = json!(claims);
    Ok(result)
}
pub fn vital(
    world: &World,
    viewer: &str,
    character: &str,
    at: &StoryTime,
    limit: usize,
) -> Result<Json> {
    limits(limit, 0)?;
    subject(character, "character")?;
    let assertions: Vec<_> = admitted(world, viewer, at, false)?
        .into_iter()
        .filter(|row| row["kind"] == "vital" && row["value"]["character_id"] == character)
        .collect();
    bounded(assertions.len(), limit)?;
    let states: BTreeSet<_> = assertions
        .iter()
        .filter_map(|row| row["value"]["state"].as_str())
        .collect();
    let mut result = envelope(viewer, at, limit);
    result["subjectId"] = json!(character);
    result["count"] = json!(assertions.len());
    result["available"] = json!(!assertions.is_empty());
    result["vital"] = json!(if states.len() == 1 {
        states.first().copied()
    } else {
        None
    });
    result["states"] = json!(states);
    result["conflicting"] = json!(states.len() > 1);
    result["uncertain"] = json!(assertions.iter().any(|row| row["uncertain"] == true));
    result["assertions"] = json!(assertions);
    Ok(result)
}
