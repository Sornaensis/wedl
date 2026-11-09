//! Literal canonical generational histories. Family paths use authored parentage
//! edges only; other histories never supply an inferred relationship or belief.
mod canonical;
pub mod discovery;
pub mod knowledge;
mod succession;

pub use canonical::organization;
pub use succession::{legacy, organization_legacies, vital};

use crate::{
    Error, Result,
    model::{Record, StoryTime, World},
    validation::{Diagnostic, valid_id_for_kind},
};
use serde_json::{Value as Json, json};
use serde_yaml::{Mapping, Value as Yaml};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const MAX_HISTORY: usize = 1000;
const MAX_MEMBERS: usize = 500;
const SCOPE: &str = "author-as-of-generational-source";
pub(crate) const GENERATIONAL_KINDS: &[&str] = &[
    "organization",
    "parentage",
    "union",
    "affiliation",
    "legacy",
    "tenure",
    "claim",
    "vital-history",
];

/// The initialization name is first. History validation and authoring guidance
/// share this closed vocabulary; payload and lifecycle checks remain separate.
pub(crate) fn transition_kinds(kind: &str) -> &'static [&'static str] {
    match kind {
        "parentage" => &["parentage-initialize", "parentage-confirm", "parentage-end"],
        "union" => &[
            "union-initialize",
            "union-form",
            "union-reconcile",
            "union-end",
            "union-annul",
        ],
        "organization" => &[
            "organization-initialize",
            "organization-rename",
            "organization-reparent",
            "organization-dormant",
            "organization-dissolve",
        ],
        "affiliation" => &[
            "affiliation-initialize",
            "affiliation-role",
            "affiliation-end",
        ],
        "legacy" => &[
            "legacy-initialize",
            "legacy-rename",
            "legacy-dormant",
            "legacy-dissolve",
        ],
        "tenure" => &[
            "tenure-initialize",
            "tenure-designate",
            "tenure-hold",
            "tenure-vacate",
            "tenure-transfer",
            "tenure-end",
        ],
        "claim" => &[
            "claim-initialize",
            "claim-dispute",
            "claim-recognize",
            "claim-withdraw",
            "claim-reject",
        ],
        "vital-history" => &[
            "vital-initialize",
            "vital-birth",
            "vital-death",
            "vital-existence-start",
            "vital-existence-end",
        ],
        _ => &[],
    }
}

fn invalid(field: &str, message: impl std::fmt::Display) -> Error {
    Error::Validation(format!("{field}: {message}"))
}
fn text<'a>(value: &'a Yaml, field: &str) -> Result<&'a str> {
    match value {
        Yaml::String(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(invalid(field, "expected a nonblank untagged string")),
    }
}
fn closed<'a>(
    value: &'a Yaml,
    required: &[&str],
    optional: &[&str],
    field: &str,
) -> Result<&'a Mapping> {
    let Yaml::Mapping(fields) = value else {
        return Err(invalid(field, "expected an untagged mapping"));
    };
    for key in fields.keys() {
        let key = text(key, field)?;
        if !required.contains(&key) && !optional.contains(&key) {
            return Err(invalid(&format!("{field}.{key}"), "unsupported field"));
        }
    }
    for key in required {
        if !fields.contains_key(*key) {
            return Err(invalid(
                &format!("{field}.{key}"),
                "required field is missing",
            ));
        }
    }
    Ok(fields)
}
fn field<'a>(record: &'a Record, key: &str) -> Result<&'a Yaml> {
    record
        .get(key)
        .ok_or_else(|| invalid(key, "required field is missing"))
}
fn transition_id(value: &str) -> bool {
    value.strip_prefix("transition_").is_some_and(|suffix| {
        suffix.len() == 26
            && suffix
                .bytes()
                .all(|c| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&c))
    })
}
fn point(world: &World, value: &Yaml, label: &str) -> Result<StoryTime> {
    let fields = closed(value, &["timeline", "tick", "order"], &[], label)?;
    text(&fields["timeline"], &format!("{label}.timeline"))?;
    if !matches!(&fields["tick"], Yaml::Number(value) if value.as_i64().is_some()) {
        return Err(invalid(
            &format!("{label}.tick"),
            "expected an untagged exact signed 64-bit integer",
        ));
    }
    if !matches!(&fields["order"], Yaml::Number(value) if value.as_i64().and_then(|value| i32::try_from(value).ok()).is_some())
    {
        return Err(invalid(
            &format!("{label}.order"),
            "expected an untagged exact signed 32-bit integer",
        ));
    }
    let at = StoryTime::from_yaml(value, &world.default_timeline())
        .map_err(|error| invalid(label, error))?;
    world
        .story_time(at.tick, Some(&at.timeline), at.order)
        .map_err(|error| invalid(label, error))
}
fn canonical(record: &Record) -> bool {
    record
        .get("status")
        .is_none_or(|value| matches!(value, Yaml::String(value) if value == "canonical"))
}
fn character_ref<'a>(world: &'a World, value: &Yaml, label: &str) -> Result<&'a str> {
    let id = text(value, label)?;
    world
        .records
        .get(id)
        .filter(|record| {
            record.kind == "character" && canonical(record) && valid_id_for_kind(id, "character")
        })
        .map(|record| record.id.as_str())
        .ok_or_else(|| invalid(label, "expected an existing exact canonical character ID"))
}
fn members<'a>(world: &'a World, value: &Yaml, label: &str) -> Result<Vec<&'a str>> {
    let Yaml::Sequence(values) = value else {
        return Err(invalid(label, "expected an array"));
    };
    if !(2..=MAX_MEMBERS).contains(&values.len()) {
        return Err(invalid(label, "expected 2..500 sorted unique characters"));
    }
    let mut result = Vec::new();
    for value in values {
        let id = character_ref(world, value, label)?;
        if result.last().is_some_and(|previous| *previous >= id) {
            return Err(invalid(label, "character IDs must be sorted and unique"));
        }
        result.push(id);
    }
    Ok(result)
}

#[derive(Clone)]
enum Payload<'a> {
    Basis(&'a str),
    Members(Vec<&'a str>),
    Empty,
}
impl Payload<'_> {
    fn json(&self) -> Json {
        match self {
            Self::Basis(basis) => json!({"basis":basis}),
            Self::Members(ids) => json!({"participantIds":ids}),
            Self::Empty => json!({}),
        }
    }
}
struct Transition<'a> {
    id: &'a str,
    kind: &'a str,
    at: StoryTime,
    payload: Payload<'a>,
    cause: Option<&'a Record>,
    replaces: Option<usize>,
    field: String,
}
struct History<'a> {
    record: &'a Record,
    child: Option<&'a str>,
    parent: Option<&'a str>,
    transitions: Vec<Transition<'a>>,
}

fn next_state(kind: &str, before: Option<&str>) -> Option<&'static str> {
    match (kind, before) {
        ("parentage-initialize", None) => Some("asserted"),
        ("parentage-confirm", Some("asserted")) => Some("confirmed"),
        ("parentage-end", Some("asserted" | "confirmed")) => Some("ended"),
        ("union-initialize", None) => Some("declared"),
        ("union-form", Some("declared")) => Some("formed"),
        ("union-reconcile", Some("formed")) => Some("formed"),
        ("union-end", Some("declared" | "formed")) => Some("ended"),
        ("union-annul", Some("declared" | "formed")) => Some("annulled"),
        _ => None,
    }
}
fn state_after(kind: &str) -> &'static str {
    match kind {
        "parentage-initialize" => "asserted",
        "parentage-confirm" => "confirmed",
        "union-initialize" => "declared",
        "union-form" | "union-reconcile" => "formed",
        "union-annul" => "annulled",
        _ => "ended",
    }
}
fn parse<'a>(world: &'a World, record: &'a Record) -> Result<History<'a>> {
    if !valid_id_for_kind(&record.id, &record.kind) {
        return Err(invalid("id", "invalid record identifier"));
    }
    let status = record
        .get("status")
        .map(|value| text(value, "status"))
        .transpose()?
        .unwrap_or("canonical");
    if !["canonical", "draft", "retired"].contains(&status) {
        return Err(invalid("status", "expected canonical, draft or retired"));
    }
    let (child, parent) = if record.kind == "parentage" {
        let child = character_ref(world, field(record, "child_id")?, "child_id")?;
        let parent = character_ref(world, field(record, "parent_id")?, "parent_id")?;
        if child == parent {
            return Err(invalid("parent_id", "parentage cannot reference itself"));
        }
        (Some(child), Some(parent))
    } else {
        members(world, field(record, "participant_ids")?, "participant_ids")?;
        (None, None)
    };
    let Yaml::Sequence(values) = field(record, "transitions")? else {
        return Err(invalid("transitions", "expected an array"));
    };
    if values.len() >= MAX_HISTORY {
        return Err(invalid(
            "transitions",
            "history including initialization exceeds 1000 entries",
        ));
    }
    let mut result = History {
        record,
        child,
        parent,
        transitions: Vec::new(),
    };
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    let mut before_states: Vec<Option<&'static str>> = Vec::new();
    let mut state = None;
    for (index, raw) in std::iter::once(field(record, "initialization")?)
        .chain(values)
        .enumerate()
    {
        let label = if index == 0 {
            "initialization".into()
        } else {
            format!("transitions[{}]", index - 1)
        };
        let fields = closed(
            raw,
            &[
                "transition_id",
                "transition_kind",
                "applicability",
                "payload",
            ],
            if index == 0 {
                &[]
            } else {
                &["cause_event_id", "replaces_transition_id"]
            },
            &label,
        )?;
        let id = text(&fields["transition_id"], &format!("{label}.transition_id"))?;
        if !transition_id(id) || seen.contains_key(id) {
            return Err(invalid(
                &format!("{label}.transition_id"),
                "expected a unique transition_ identifier",
            ));
        }
        let kind = text(
            &fields["transition_kind"],
            &format!("{label}.transition_kind"),
        )?;
        let allowed = transition_kinds(&record.kind);
        if !allowed.contains(&kind) || kind.ends_with("-initialize") != (index == 0) {
            return Err(invalid(
                &format!("{label}.transition_kind"),
                "transition kind does not apply here",
            ));
        }
        let app_label = format!("{label}.applicability");
        let app = closed(
            &fields["applicability"],
            &["applicability_kind", "point"],
            &[],
            &app_label,
        )?;
        if text(&app["applicability_kind"], &app_label)? != "instant" {
            return Err(invalid(
                &app_label,
                "parentage and union transitions require instant applicability",
            ));
        }
        let at = point(world, &app["point"], &format!("{app_label}.point"))?;
        let replaces = fields
            .get("replaces_transition_id")
            .map(|value| {
                let target = text(value, &format!("{label}.replaces_transition_id"))?;
                let Some(&target) = seen.get(target) else {
                    return Err(invalid(
                        &format!("{label}.replaces_transition_id"),
                        "replacement must target an earlier entry in this record",
                    ));
                };
                if result.transitions[target].at != at {
                    return Err(invalid(
                        &format!("{label}.replaces_transition_id"),
                        "replacement requires the exact same applicability",
                    ));
                }
                Ok(target)
            })
            .transpose()?;
        if let Some(previous) = result.transitions.last() {
            if !previous.at.not_after(&at) || previous.at == at && replaces.is_none() {
                return Err(invalid(
                    &app_label,
                    "history must increase on one timeline; equal instants require an explicit replacement",
                ));
            }
        }
        if let Some(target) = replaces {
            state = before_states[target];
        }
        before_states.push(state);
        state = Some(next_state(kind, state).ok_or_else(|| {
            invalid(
                &format!("{label}.transition_kind"),
                "transition is not allowed from the current lifecycle state",
            )
        })?);
        let payload_label = format!("{label}.payload");
        let payload = if ["parentage-initialize", "parentage-confirm"].contains(&kind) {
            let payload = closed(&fields["payload"], &["basis"], &[], &payload_label)?;
            let basis = text(&payload["basis"], &format!("{payload_label}.basis"))?;
            if !["biological", "adoptive"].contains(&basis) {
                return Err(invalid(
                    &payload_label,
                    "basis must be biological or adoptive",
                ));
            }
            Payload::Basis(basis)
        } else if ["union-initialize", "union-form", "union-reconcile"].contains(&kind) {
            let payload = closed(
                &fields["payload"],
                &["participant_ids"],
                &[],
                &payload_label,
            )?;
            Payload::Members(members(
                world,
                &payload["participant_ids"],
                &format!("{payload_label}.participant_ids"),
            )?)
        } else {
            closed(&fields["payload"], &[], &[], &payload_label)?;
            Payload::Empty
        };
        let cause = fields
            .get("cause_event_id")
            .map(|value| {
                let id = text(value, &format!("{label}.cause_event_id"))?;
                let event = world
                    .records
                    .get(id)
                    .filter(|event| {
                        event.kind == "event" && canonical(event) && valid_id_for_kind(id, "event")
                    })
                    .ok_or_else(|| {
                        invalid(
                            &format!("{label}.cause_event_id"),
                            "expected an exact canonical event ID",
                        )
                    })?;
                let event_time = point(
                    world,
                    field(event, "time")?,
                    &format!("{label}.cause_event_id.time"),
                )?;
                if !event_time.not_after(&at) || event_time == at {
                    return Err(invalid(
                        &format!("{label}.cause_event_id"),
                        "cause must be strictly earlier on the transition timeline",
                    ));
                }
                Ok(event)
            })
            .transpose()?;
        seen.insert(id, index);
        result.transitions.push(Transition {
            id,
            kind,
            at,
            payload,
            cause,
            replaces,
            field: label,
        });
    }
    Ok(result)
}

fn diagnostic(record: &Record, code: &str, error: Error) -> Diagnostic {
    let message = error.to_string();
    let field = message.split_once(": ").map(|(field, _)| field.to_owned());
    Diagnostic {
        code: code.into(),
        message,
        severity: "error".into(),
        entity_id: Some(record.id.clone()),
        path: Some(record.source_path.clone()),
        field,
    }
}
fn inspect(world: &World) -> (Vec<History<'_>>, Vec<Diagnostic>) {
    let mut histories = Vec::new();
    let mut diagnostics = Vec::new();
    for record in world
        .records
        .values()
        .filter(|record| ["parentage", "union"].contains(&record.kind.as_str()))
    {
        match parse(world, record) {
            Ok(history) => histories.push(history),
            Err(error) => diagnostics.push(diagnostic(record, "GEN-HISTORY", error)),
        }
    }
    // Transition identifiers are one authored namespace, including sibling
    // generational kinds whose full behavior is implemented in later slices.
    let mut ids = BTreeSet::new();
    for record in world
        .records
        .values()
        .filter(|record| GENERATIONAL_KINDS.contains(&record.kind.as_str()))
    {
        for (index, value) in record
            .get("initialization")
            .into_iter()
            .chain(record.list("transitions").into_iter().flatten())
            .enumerate()
        {
            if let Some(id) = value
                .get("transition_id")
                .and_then(Yaml::as_str)
                .filter(|id| transition_id(id))
            {
                if !ids.insert(id) {
                    let field = if index == 0 {
                        "initialization.transition_id".into()
                    } else {
                        format!("transitions[{}].transition_id", index - 1)
                    };
                    diagnostics.push(diagnostic(
                        record,
                        "GEN-TRANSITION-006",
                        invalid(&field, "transition ID must be globally unique"),
                    ));
                }
            }
        }
    }
    let mut duplicates = BTreeSet::new();
    let mut graph: BTreeMap<&str, Vec<(&str, &Record)>> = BTreeMap::new();
    for history in histories.iter().filter(|history| {
        history.record.kind == "parentage" && history.record.status() == "canonical"
    }) {
        let child = history.child.unwrap();
        let parent = history.parent.unwrap();
        let initial = &history.transitions[0];
        let Payload::Basis(basis) = initial.payload else {
            unreachable!("validated parentage initialization")
        };
        if !duplicates.insert((child, parent, basis, &initial.at)) {
            diagnostics.push(diagnostic(
                history.record,
                "GEN-PARENTAGE-003",
                invalid("initialization", "duplicate canonical parentage edge"),
            ));
        }
        graph
            .entry(child)
            .or_default()
            .push((parent, history.record));
    }
    let mut visited = BTreeSet::new();
    let mut active = BTreeSet::new();
    for start in graph.keys().copied() {
        if visited.contains(start) {
            continue;
        }
        active.insert(start);
        let mut stack = vec![(start, 0usize)];
        while let Some((node, index)) = stack.last_mut() {
            let edges = graph.get(node).map(Vec::as_slice).unwrap_or(&[]);
            if *index == edges.len() {
                let (node, _) = stack.pop().unwrap();
                active.remove(node);
                visited.insert(node);
                continue;
            }
            let (target, record) = edges[*index];
            *index += 1;
            if active.contains(target) {
                diagnostics.push(diagnostic(
                    record,
                    "GEN-PARENTAGE-002",
                    invalid("parent_id", "canonical parentage graph contains a cycle"),
                ));
            } else if !visited.contains(target) {
                active.insert(target);
                stack.push((target, 0));
            }
        }
    }
    (histories, diagnostics)
}
pub fn validate(world: &World) -> Vec<Diagnostic> {
    let mut diagnostics = inspect(world).1;
    diagnostics.extend(canonical::validate(world));
    diagnostics.extend(succession::validate(world));
    diagnostics.extend(knowledge::validate(world));
    diagnostics
}
fn read(world: &World) -> Result<Vec<History<'_>>> {
    canonical::require_valid(world)?;
    Ok(inspect(world).0)
}
fn limits(limit: usize, depth: usize) -> Result<()> {
    if !(1..=1000).contains(&limit) || depth > 32 {
        return Err(Error::InvalidInput(
            "generational limit must be 1..1000 and depth 0..32".into(),
        ));
    }
    Ok(())
}
fn bounded(count: usize, limit: usize) -> Result<()> {
    if count > limit {
        return Err(Error::InvalidInput(
            "generational results exceed limit; increase limit for complete results".into(),
        ));
    }
    Ok(())
}
fn query_character<'a>(world: &'a World, selector: &str) -> Result<&'a Record> {
    let record = world.resolve(selector)?;
    if record.kind != "character"
        || !canonical(record)
        || !valid_id_for_kind(&record.id, "character")
    {
        return Err(Error::InvalidInput(
            "family queries require a canonical character".into(),
        ));
    }
    Ok(record)
}
fn envelope(world: &World, at: &StoryTime, limit: usize) -> Result<Json> {
    world.story_time(at.tick, Some(&at.timeline), at.order)?;
    Ok(
        json!({"perspective":"author","scope":SCOPE,"effectiveTime":at,"revision":world.revision,"fingerprint":world.fingerprint,"complete":true,"limit":limit}),
    )
}
fn citation(world: &World, record: &Record, field: &str) -> Json {
    json!({"entityId":record.id,"sourcePath":record.source_path,"blobOid":record.blob_oid,"revision":world.revision,"fingerprint":world.fingerprint,"field":field})
}
fn transition_citation(world: &World, history: &History<'_>, index: usize) -> Json {
    let transition = &history.transitions[index];
    let mut result = citation(world, history.record, &transition.field);
    result["transitionId"] = json!(transition.id);
    result["time"] = json!(transition.at);
    result["replacesTransitionId"] = json!(
        transition
            .replaces
            .map(|target| history.transitions[target].id)
    );
    if let Some(cause) = transition.cause {
        result["causeCitation"] = citation(world, cause, "time");
    }
    result
}
struct Fold<'a> {
    visible: usize,
    state: &'static str,
    basis: Option<&'a str>,
    members: Vec<&'a str>,
    superseded: BTreeSet<usize>,
}
impl Fold<'_> {
    fn active(&self) -> bool {
        ["asserted", "confirmed", "declared", "formed"].contains(&self.state)
    }
}
fn fold<'a>(history: &History<'a>, at: &StoryTime) -> Option<Fold<'a>> {
    if history.record.status() != "canonical" || !history.transitions[0].at.not_after(at) {
        return None;
    }
    let visible = history
        .transitions
        .iter()
        .take_while(|transition| transition.at.not_after(at))
        .count();
    let superseded: BTreeSet<_> = history.transitions[..visible]
        .iter()
        .filter_map(|transition| transition.replaces)
        .collect();
    let mut folded = Fold {
        visible,
        state: "",
        basis: None,
        members: Vec::new(),
        superseded,
    };
    for (index, transition) in history.transitions[..visible].iter().enumerate() {
        if folded.superseded.contains(&index) {
            continue;
        }
        folded.state = state_after(transition.kind);
        match &transition.payload {
            Payload::Basis(value) => folded.basis = Some(value),
            Payload::Members(ids) => folded.members = ids.clone(),
            Payload::Empty => {}
        }
    }
    Some(folded)
}
fn folded_json(
    world: &World,
    history: &History<'_>,
    folded: &Fold<'_>,
    limit: usize,
) -> Result<Json> {
    bounded(folded.visible, limit)?;
    bounded(folded.members.len(), limit)?;
    let value = if history.record.kind == "parentage" {
        json!({"childId":history.child,"parentId":history.parent,"basis":folded.basis})
    } else {
        json!({"participantIds":folded.members})
    };
    Ok(
        json!({"recordId":history.record.id,"recordTitle":history.record.title,"kind":history.record.kind,"recordStatus":history.record.status(),"lifecycle":folded.state,"active":folded.active(),"value":value,
        "citations":(0..folded.visible).map(|index| transition_citation(world,history,index)).collect::<Vec<_>>(),
        "appliedTransitionIds":(0..folded.visible).filter(|index| !folded.superseded.contains(index)).map(|index| history.transitions[index].id).collect::<Vec<_>>() }),
    )
}

/// Current direct parents/children and union memberships. A union's latest
/// payload, not its static header or past participants, supplies its roster.
pub fn family(world: &World, character: &str, at: &StoryTime, limit: usize) -> Result<Json> {
    limits(limit, 0)?;
    let mut result = envelope(world, at, limit)?;
    let character = query_character(world, character)?;
    let histories = read(world)?;
    let (mut parents, mut children, mut unions) = (Vec::new(), Vec::new(), Vec::new());
    for history in &histories {
        let Some(folded) = fold(history, at).filter(Fold::active) else {
            continue;
        };
        let target = if history.child == Some(character.id.as_str()) {
            history.parent
        } else if history.parent == Some(character.id.as_str()) {
            history.child
        } else {
            None
        };
        if let Some(target) = target {
            let mut value = folded_json(world, history, &folded, limit)?;
            value["targetId"] = json!(target);
            value["targetTitle"] = json!(world.records[target].title);
            if history.child == Some(character.id.as_str()) {
                parents.push(value);
            } else {
                children.push(value);
            }
        } else if history.record.kind == "union" && folded.members.contains(&character.id.as_str())
        {
            unions.push(folded_json(world, history, &folded, limit)?);
        }
        bounded(parents.len() + children.len() + unions.len(), limit)?;
    }
    result["characterId"] = json!(character.id);
    result["count"] = json!(parents.len() + children.len() + unions.len());
    result["parents"] = json!(parents);
    result["children"] = json!(children);
    result["unions"] = json!(unions);
    Ok(result)
}
/// A canonical record's literal visible history and folded state. Corrections
/// retain their original citations, replacement links and superseded flags.
pub fn history(world: &World, record: &str, at: &StoryTime, limit: usize) -> Result<Json> {
    limits(limit, 0)?;
    let mut result = envelope(world, at, limit)?;
    let selected = world.resolve(record)?;
    match selected.kind.as_str() {
        "organization" | "affiliation" => {
            return canonical::history(world, &selected.id, at, limit);
        }
        "legacy" | "tenure" | "claim" | "vital-history" => {
            return succession::history(world, &selected.id, at, limit);
        }
        _ => {}
    }
    if !["parentage", "union"].contains(&selected.kind.as_str()) {
        return Err(Error::InvalidInput(
            "history requires a generational record".into(),
        ));
    }
    let histories = read(world)?;
    let history = histories
        .iter()
        .find(|history| history.record.id == selected.id)
        .expect("supported selected record was parsed");
    result["recordId"] = json!(selected.id);
    result["recordTitle"] = json!(selected.title);
    result["kind"] = json!(selected.kind);
    result["recordStatus"] = json!(selected.status());
    result["history"] = json!([]);
    result["count"] = json!(0);
    let Some(folded) = fold(history, at) else {
        result["available"] = json!(false);
        result["reason"] = json!(if selected.status() != "canonical" {
            "noncanonical"
        } else if history.transitions[0].at.timeline != at.timeline {
            "different_timeline"
        } else {
            "before_initialization"
        });
        return Ok(result);
    };
    let value = folded_json(world, history, &folded, limit)?;
    result
        .as_object_mut()
        .unwrap()
        .extend(value.as_object().unwrap().clone());
    result["available"] = json!(true);
    result["reason"] = Json::Null;
    result["count"] = json!(folded.visible);
    result["history"]=json!(history.transitions[..folded.visible].iter().enumerate().map(|(index,transition)|json!({
        "transitionId":transition.id,"transitionKind":transition.kind,"time":transition.at,"payload":transition.payload.json(),"causeEventId":transition.cause.map(|record| &record.id),
        "replacesTransitionId":transition.replaces.map(|target| history.transitions[target].id),"superseded":folded.superseded.contains(&index),"citation":transition_citation(world,history,index)
    })).collect::<Vec<_>>());
    Ok(result)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineageDirection {
    Ancestors,
    Descendants,
}
#[derive(Clone)]
struct Step<'a> {
    from: &'a str,
    to: &'a str,
    history: usize,
}
fn graph<'a>(
    histories: &[History<'a>],
    at: &StoryTime,
    direction: Option<LineageDirection>,
) -> BTreeMap<&'a str, Vec<Step<'a>>> {
    let mut graph: BTreeMap<&str, Vec<Step<'_>>> = BTreeMap::new();
    for (index, history) in histories.iter().enumerate() {
        if history.record.kind != "parentage"
            || !fold(history, at).is_some_and(|folded| folded.active())
        {
            continue;
        }
        let child = history.child.unwrap();
        let parent = history.parent.unwrap();
        if direction != Some(LineageDirection::Descendants) {
            graph.entry(child).or_default().push(Step {
                from: child,
                to: parent,
                history: index,
            });
        }
        if direction != Some(LineageDirection::Ancestors) {
            graph.entry(parent).or_default().push(Step {
                from: parent,
                to: child,
                history: index,
            });
        }
    }
    // Histories are record-ID ordered, giving deterministic BFS path ties.
    graph
}
fn path_json(
    world: &World,
    histories: &[History<'_>],
    at: &StoryTime,
    path: &[Step<'_>],
    limit: usize,
) -> Result<Json> {
    let mut edges = Vec::new();
    for step in path {
        let history = &histories[step.history];
        let folded = fold(history, at).expect("only active histories form edges");
        let source = folded_json(world, history, &folded, limit)?;
        edges.push(json!({"from":step.from,"to":step.to,"recordId":history.record.id,"basis":folded.basis,"lifecycle":folded.state,"citations":source["citations"]}));
    }
    Ok(json!(edges))
}
/// One shortest cited path per reachable ancestor/descendant. Discovering more
/// reachable nodes beyond either bound fails rather than returning a prefix.
pub fn lineage(
    world: &World,
    character: &str,
    at: &StoryTime,
    direction: LineageDirection,
    max_depth: usize,
    limit: usize,
) -> Result<Json> {
    limits(limit, max_depth)?;
    let mut result = envelope(world, at, limit)?;
    let character = query_character(world, character)?;
    let histories = read(world)?;
    let graph = graph(&histories, at, Some(direction));
    let mut seen = BTreeSet::from([character.id.as_str()]);
    let mut queue = VecDeque::from([(character.id.as_str(), Vec::new())]);
    let mut paths = BTreeMap::new();
    while let Some((node, path)) = queue.pop_front() {
        for edge in graph.get(node).into_iter().flatten() {
            if seen.contains(edge.to) {
                continue;
            }
            if path.len() == max_depth {
                return Err(Error::InvalidInput(
                    "lineage exceeds maximum depth; increase depth for complete results".into(),
                ));
            }
            bounded(paths.len() + 1, limit)?;
            let mut next = path.clone();
            next.push(edge.clone());
            seen.insert(edge.to);
            paths.insert(edge.to, next.clone());
            queue.push_back((edge.to, next));
        }
    }
    let items = paths.into_iter().map(|(target,path)|Ok(json!({"targetId":target,"generationDistance":path.len().to_string(),"edges":path_json(world,&histories,at,&path,limit)?}))).collect::<Result<Vec<_>>>()?;
    result["characterId"] = json!(character.id);
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
/// One shortest continuous parentage path in either direction. No kinship term
/// or biological certainty is inferred from that path.
pub fn kinship_path(
    world: &World,
    from: &str,
    to: &str,
    at: &StoryTime,
    max_hops: usize,
    limit: usize,
) -> Result<Json> {
    limits(limit, max_hops)?;
    let mut result = envelope(world, at, limit)?;
    let from = query_character(world, from)?;
    let to = query_character(world, to)?;
    let histories = read(world)?;
    result["fromId"] = json!(from.id);
    result["toId"] = json!(to.id);
    result["maxHops"] = json!(max_hops);
    result["found"] = json!(false);
    result["reason"] = json!("no_match");
    result["edges"] = json!([]);
    result["distance"] = Json::Null;
    let graph = graph(&histories, at, None);
    let mut seen = BTreeSet::from([from.id.as_str()]);
    let mut queue = VecDeque::from([(from.id.as_str(), Vec::new())]);
    let mut matched = (from.id == to.id).then(Vec::new);
    'search: while matched.is_none() {
        let Some((node, path)) = queue.pop_front() else {
            break;
        };
        for edge in graph.get(node).into_iter().flatten() {
            if seen.contains(edge.to) {
                continue;
            }
            if path.len() == max_hops {
                return Err(Error::InvalidInput(
                    "kinship path exceeds maximum hops before search is complete".into(),
                ));
            }
            bounded(seen.len(), limit)?;
            let mut next = path.clone();
            next.push(edge.clone());
            seen.insert(edge.to);
            if edge.to == to.id {
                matched = Some(next);
                break 'search;
            }
            queue.push_back((edge.to, next));
        }
    }
    if let Some(path) = matched {
        result["found"] = json!(true);
        result["reason"] = Json::Null;
        result["distance"] = json!(path.len().to_string());
        result["edges"] = path_json(world, &histories, at, &path, limit)?;
    }
    result["visitedCount"] = json!(seen.len());
    Ok(result)
}
