//! Shared literal history mechanics and authored organizations/affiliations.
//! This module never treats an organizational fact as a character belief.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Applicability {
    pub first: StoryTime,
    pub last: Option<StoryTime>,
}
impl Applicability {
    pub fn json(&self) -> Json {
        match &self.last {
            Some(last) => {
                json!({"applicabilityKind":"inclusive-interval","first":self.first,"last":last})
            }
            None => json!({"applicabilityKind":"instant","point":self.first}),
        }
    }
}
pub(super) struct Entry<'a> {
    pub id: &'a str,
    pub kind: &'a str,
    pub applicability: Applicability,
    pub payload: &'a Yaml,
    pub cause: Option<&'a Record>,
    pub replaces: Option<usize>,
    pub field: String,
}
pub(super) struct LiteralHistory<'a> {
    pub record: &'a Record,
    pub entries: Vec<Entry<'a>>,
}
pub(super) struct LiteralFold {
    pub visible: usize,
    pub superseded: BTreeSet<usize>,
    pub lifecycle: String,
    pub value: Json,
    pub active: bool,
}

pub(super) fn canonical_ref<'a>(
    world: &'a World,
    value: &Yaml,
    kind: &str,
    label: &str,
) -> Result<&'a Record> {
    let id = text(value, label)?;
    world
        .records
        .get(id)
        .filter(|record| record.kind == kind && canonical(record) && valid_id_for_kind(id, kind))
        .ok_or_else(|| invalid(label, format!("expected an exact canonical {kind} ID")))
}
pub(super) fn optional_ref<'a>(
    world: &'a World,
    record: &Record,
    key: &str,
    kind: &str,
) -> Result<Option<&'a Record>> {
    match record.get(key) {
        None | Some(Yaml::Null) => Ok(None),
        Some(value) => canonical_ref(world, value, kind, key).map(Some),
    }
}
pub(super) fn ordered_text<'a>(value: &'a Yaml, label: &str) -> Result<Vec<&'a str>> {
    let Yaml::Sequence(values) = value else {
        return Err(invalid(label, "expected a sorted unique string array"));
    };
    if values.len() > MAX_MEMBERS {
        return Err(invalid(label, "array exceeds 500 entries"));
    }
    let mut result = Vec::new();
    for value in values {
        let item = text(value, label)?;
        if result.last().is_some_and(|last| *last >= item) {
            return Err(invalid(label, "strings must be sorted and unique"));
        }
        result.push(item);
    }
    Ok(result)
}

/// Grammar, exact time ordering and corrections are common to the six domains.
/// The callback checks each domain's literal payload and closed lifecycle table.
pub(super) fn parse_literal<'a>(
    world: &'a World,
    record: &'a Record,
    check: impl Fn(&World, &Record, &Entry<'_>, Option<&str>) -> Result<String>,
) -> Result<LiteralHistory<'a>> {
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
    let Yaml::Sequence(values) = field(record, "transitions")? else {
        return Err(invalid("transitions", "expected an untagged array"));
    };
    if values.len() >= MAX_HISTORY {
        return Err(invalid(
            "transitions",
            "history including initialization exceeds 1000 entries",
        ));
    }
    let mut history = LiteralHistory {
        record,
        entries: Vec::new(),
    };
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    let mut before_states: Vec<Option<String>> = Vec::new();
    let mut state: Option<String> = None;
    for (index, value) in std::iter::once(field(record, "initialization")?)
        .chain(values)
        .enumerate()
    {
        let label = if index == 0 {
            "initialization".into()
        } else {
            format!("transitions[{}]", index - 1)
        };
        let fields = closed(
            value,
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
        if !super::transition_kinds(&record.kind).contains(&kind)
            || kind.ends_with("-initialize") != (index == 0)
        {
            return Err(invalid(
                &format!("{label}.transition_kind"),
                "transition kind does not apply here",
            ));
        }
        let app_label = format!("{label}.applicability");
        let interval = kind == "tenure-vacate";
        let fields_app = closed(
            &fields["applicability"],
            if interval {
                &["applicability_kind", "first", "last"]
            } else {
                &["applicability_kind", "point"]
            },
            &[],
            &app_label,
        )?;
        let expected = if interval {
            "inclusive-interval"
        } else {
            "instant"
        };
        if text(&fields_app["applicability_kind"], &app_label)? != expected {
            return Err(invalid(
                &app_label,
                format!("this transition requires {expected} applicability"),
            ));
        }
        let first_key = if interval { "first" } else { "point" };
        let first = point(
            world,
            &fields_app[first_key],
            &format!("{app_label}.{first_key}"),
        )?;
        let last = if interval {
            let last = point(world, &fields_app["last"], &format!("{app_label}.last"))?;
            if !first.not_after(&last) {
                return Err(invalid(
                    &app_label,
                    "interval must end at or after its start on the same timeline",
                ));
            }
            Some(last)
        } else {
            None
        };
        let applicability = Applicability { first, last };
        let replaces = fields
            .get("replaces_transition_id")
            .map(|value| {
                let id = text(value, &format!("{label}.replaces_transition_id"))?;
                let target = *seen.get(id).ok_or_else(|| {
                    invalid(
                        &format!("{label}.replaces_transition_id"),
                        "replacement must target an earlier entry in this record",
                    )
                })?;
                if history.entries[target].applicability != applicability {
                    return Err(invalid(
                        &format!("{label}.replaces_transition_id"),
                        "replacement requires exact identical applicability",
                    ));
                }
                Ok(target)
            })
            .transpose()?;
        if let Some(previous) = history.entries.last() {
            if !previous.applicability.first.not_after(&applicability.first)
                || previous.applicability.first == applicability.first && replaces.is_none()
            {
                return Err(invalid(
                    &app_label,
                    "history must increase on one timeline; equal starts require exact replacement",
                ));
            }
        }
        let cause = fields
            .get("cause_event_id")
            .map(|value| {
                let event =
                    canonical_ref(world, value, "event", &format!("{label}.cause_event_id"))?;
                let cause_at = point(
                    world,
                    field(event, "time")?,
                    &format!("{label}.cause_event_id.time"),
                )?;
                if !cause_at.not_after(&applicability.first) || cause_at == applicability.first {
                    return Err(invalid(
                        &format!("{label}.cause_event_id"),
                        "cause must be strictly earlier on the transition timeline",
                    ));
                }
                Ok(event)
            })
            .transpose()?;
        if kind == "tenure-transfer" && cause.is_none() {
            return Err(invalid(
                &format!("{label}.cause_event_id"),
                "tenure transfer requires an earlier event cause",
            ));
        }
        let entry = Entry {
            id,
            kind,
            applicability,
            payload: &fields["payload"],
            cause,
            replaces,
            field: label,
        };
        if let Some(target) = replaces {
            state = before_states[target].clone();
        }
        before_states.push(state.clone());
        state = Some(check(world, record, &entry, state.as_deref())?);
        seen.insert(id, index);
        history.entries.push(entry);
    }
    Ok(history)
}

pub(super) fn effective_indices(history: &LiteralHistory<'_>, visible: usize) -> Vec<usize> {
    let superseded: BTreeSet<_> = history.entries[..visible]
        .iter()
        .filter_map(|entry| entry.replaces)
        .collect();
    (0..visible)
        .filter(|index| !superseded.contains(index))
        .collect()
}
pub(super) fn fold_literal(
    history: &LiteralHistory<'_>,
    at: &StoryTime,
    initial: Json,
    mut apply: impl FnMut(&Entry<'_>, &StoryTime, &mut LiteralFold),
) -> Option<LiteralFold> {
    if !canonical(history.record) || !history.entries[0].applicability.first.not_after(at) {
        return None;
    }
    let visible = history
        .entries
        .iter()
        .take_while(|entry| entry.applicability.first.not_after(at))
        .count();
    let superseded = history.entries[..visible]
        .iter()
        .filter_map(|entry| entry.replaces)
        .collect();
    let mut folded = LiteralFold {
        visible,
        superseded,
        lifecycle: String::new(),
        value: initial,
        active: false,
    };
    for index in effective_indices(history, visible) {
        apply(&history.entries[index], at, &mut folded);
    }
    Some(folded)
}
pub(super) fn entry_citation(world: &World, history: &LiteralHistory<'_>, index: usize) -> Json {
    let entry = &history.entries[index];
    let mut result = citation(world, history.record, &entry.field);
    result["transitionId"] = json!(entry.id);
    result["time"] = json!(entry.applicability.first);
    result["applicability"] = entry.applicability.json();
    result["replacesTransitionId"] = json!(entry.replaces.map(|target| history.entries[target].id));
    if let Some(cause) = entry.cause {
        result["causeCitation"] = citation(world, cause, "time");
    }
    result
}
fn bound_arrays(value: &Json, limit: usize) -> Result<()> {
    match value {
        Json::Array(values) => {
            bounded(values.len(), limit)?;
            for value in values {
                bound_arrays(value, limit)?;
            }
        }
        Json::Object(values) => {
            for value in values.values() {
                bound_arrays(value, limit)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn reference_visible(world: &World, id: &str, at: &StoryTime) -> bool {
    let Some(record) = world.records.get(id) else {
        return false;
    };
    canonical(record)
        && record
            .get("initialization")
            .and_then(|value| value.get("applicability"))
            .and_then(|value| value.get("point"))
            .is_some_and(|value| {
                point(world, value, "initialization.applicability.point")
                    .is_ok_and(|first| first.not_after(at))
            })
}
/// Preserve authored nulls while omitting references outside this horizon.
/// Source bytes and citations remain literal; this is a query projection.
fn visible_references(world: &World, value: &mut Json, at: &StoryTime) {
    match value {
        Json::Object(fields) => {
            fields.retain(|key, value| {
                if (key.ends_with("_id") || key.ends_with("Id"))
                    && let Some(id) = value.as_str()
                    && world.records.get(id).is_some_and(|record| {
                        ["organization", "legacy", "tenure", "claim"]
                            .contains(&record.kind.as_str())
                    })
                    && !reference_visible(world, id, at)
                {
                    return false;
                }
                if ["competes_with", "competesWith"].contains(&key.as_str())
                    && let Some(values) = value.as_array_mut()
                {
                    values.retain(|value| {
                        value
                            .as_str()
                            .is_some_and(|id| reference_visible(world, id, at))
                    });
                } else {
                    visible_references(world, value, at);
                }
                true
            });
        }
        Json::Array(values) => {
            for value in values {
                visible_references(world, value, at);
            }
        }
        _ => {}
    }
}
pub(super) fn state_json(
    world: &World,
    history: &LiteralHistory<'_>,
    at: &StoryTime,
    folded: &LiteralFold,
    limit: usize,
) -> Result<Json> {
    bounded(folded.visible, limit)?;
    let mut value = folded.value.clone();
    visible_references(world, &mut value, at);
    bound_arrays(&value, limit)?;
    Ok(
        json!({"recordId":history.record.id,"recordTitle":history.record.title,"kind":history.record.kind,
        "recordStatus":history.record.status(),"lifecycle":folded.lifecycle,"active":folded.active,"value":value,
        "citations":(0..folded.visible).map(|index| entry_citation(world,history,index)).collect::<Vec<_>>(),
        "appliedTransitionIds":effective_indices(history,folded.visible).into_iter().map(|index|history.entries[index].id).collect::<Vec<_>>() }),
    )
}
pub(super) fn history_json(
    world: &World,
    history: &LiteralHistory<'_>,
    at: &StoryTime,
    folded: Option<&LiteralFold>,
    limit: usize,
) -> Result<Json> {
    limits(limit, 0)?;
    let mut result = envelope(world, at, limit)?;
    result["recordId"] = json!(history.record.id);
    result["recordTitle"] = json!(history.record.title);
    result["kind"] = json!(history.record.kind);
    result["recordStatus"] = json!(history.record.status());
    result["history"] = json!([]);
    result["count"] = json!(0);
    let Some(folded) = folded else {
        result["available"] = json!(false);
        result["reason"] = json!(if !canonical(history.record) {
            "noncanonical"
        } else if history.entries[0].applicability.first.timeline != at.timeline {
            "different_timeline"
        } else {
            "before_initialization"
        });
        return Ok(result);
    };
    let state = state_json(world, history, at, folded, limit)?;
    result
        .as_object_mut()
        .unwrap()
        .extend(state.as_object().unwrap().clone());
    let mut entries = Vec::new();
    for (index, entry) in history.entries[..folded.visible].iter().enumerate() {
        let mut payload = serde_json::to_value(entry.payload)?;
        visible_references(world, &mut payload, at);
        bound_arrays(&payload, limit)?;
        entries.push(json!({"transitionId":entry.id,"transitionKind":entry.kind,"time":entry.applicability.first,
            "applicability":entry.applicability.json(),"payload":payload,"causeEventId":entry.cause.map(|record|&record.id),
            "replacesTransitionId":entry.replaces.map(|target|history.entries[target].id),"superseded":folded.superseded.contains(&index),
            "citation":entry_citation(world,history,index)}));
    }
    result["available"] = json!(true);
    result["reason"] = Json::Null;
    result["count"] = json!(entries.len());
    result["history"] = json!(entries);
    Ok(result)
}
pub(super) fn require_valid(world: &World) -> Result<()> {
    if let Some(diagnostic) = super::validate(world).first() {
        return Err(Error::Validation(diagnostic.message.clone()));
    }
    if world
        .records
        .values()
        .any(|record| GENERATIONAL_KINDS.contains(&record.kind.as_str()))
        && !world
            .world_record()?
            .list("capabilities")
            .is_some_and(|values| {
                values.iter().any(
                    |value| matches!(value,Yaml::String(value) if value == "generational-core-v1"),
                )
            })
    {
        return Err(invalid(
            "capabilities",
            "generational records require generational-core-v1",
        ));
    }
    Ok(())
}

fn check_organization(
    world: &World,
    record: &Record,
    entry: &Entry<'_>,
    before: Option<&str>,
) -> Result<String> {
    let label = format!("{}.payload", entry.field);
    match entry.kind {
        "organization-initialize" | "organization-rename" => {
            let fields = closed(entry.payload, &["title", "aliases"], &[], &label)?;
            text(&fields["title"], &format!("{label}.title"))?;
            ordered_text(&fields["aliases"], &format!("{label}.aliases"))?;
        }
        "organization-reparent" => {
            let fields = closed(entry.payload, &["parent_id"], &[], &label)?;
            if !matches!(fields["parent_id"], Yaml::Null) {
                canonical_ref(
                    world,
                    &fields["parent_id"],
                    "organization",
                    &format!("{label}.parent_id"),
                )?;
            }
        }
        "affiliation-initialize" | "affiliation-role" => {
            let fields = closed(entry.payload, &["role"], &[], &label)?;
            if entry.kind == "affiliation-initialize" {
                if !matches!(fields["role"], Yaml::Null | Yaml::String(_)) {
                    return Err(invalid(
                        &format!("{label}.role"),
                        "initial role must be a literal string or null",
                    ));
                }
            } else {
                text(&fields["role"], &format!("{label}.role"))?;
            }
        }
        "organization-dormant" | "organization-dissolve" | "affiliation-end" => {
            closed(entry.payload, &[], &[], &label)?;
        }
        _ => {
            return Err(invalid(
                &format!("{}.transition_kind", entry.field),
                "unsupported organization or affiliation transition",
            ));
        }
    }
    let next = match (record.kind.as_str(), entry.kind, before) {
        ("organization", "organization-initialize", None) => Some("active"),
        (
            "organization",
            "organization-rename" | "organization-reparent",
            Some(state @ ("active" | "dormant")),
        ) => Some(state),
        ("organization", "organization-dormant", Some("active")) => Some("dormant"),
        ("organization", "organization-dissolve", Some("active" | "dormant")) => Some("dissolved"),
        ("affiliation", "affiliation-initialize", None) => Some("active"),
        ("affiliation", "affiliation-role", Some("active")) => Some("active"),
        ("affiliation", "affiliation-end", Some("active")) => Some("ended"),
        _ => None,
    };
    next.map(str::to_owned).ok_or_else(|| {
        invalid(
            &format!("{}.transition_kind", entry.field),
            "transition is not allowed from the current lifecycle state",
        )
    })
}

fn parse_organization<'a>(world: &'a World, record: &'a Record) -> Result<LiteralHistory<'a>> {
    match record.kind.as_str() {
        "organization" => {
            let kind = text(field(record, "organization_kind")?, "organization_kind")?;
            if !["house", "dynasty", "clan", "institution", "other"].contains(&kind) {
                return Err(invalid(
                    "organization_kind",
                    "expected house, dynasty, clan, institution or other",
                ));
            }
            optional_ref(world, record, "parent_id", "organization")?;
            optional_ref(world, record, "location_id", "location")?;
        }
        "affiliation" => {
            canonical_ref(
                world,
                field(record, "character_id")?,
                "character",
                "character_id",
            )?;
            canonical_ref(
                world,
                field(record, "organization_id")?,
                "organization",
                "organization_id",
            )?;
        }
        _ => return Err(invalid("kind", "expected organization or affiliation")),
    }
    parse_literal(world, record, check_organization)
}
fn fold_organization(history: &LiteralHistory<'_>, at: &StoryTime) -> Option<LiteralFold> {
    let record = history.record;
    let initial = if record.kind == "organization" {
        json!({"organizationKind":record.string("organization_kind"),"parentId":record.string("parent_id"),"locationId":record.string("location_id")})
    } else {
        json!({"characterId":record.string("character_id"),"organizationId":record.string("organization_id")})
    };
    fold_literal(history, at, initial, |entry, _, folded| {
        match entry.kind {
            "organization-initialize" => folded.lifecycle = "active".into(),
            "organization-dormant" => folded.lifecycle = "dormant".into(),
            "organization-dissolve" => folded.lifecycle = "dissolved".into(),
            "affiliation-initialize" => folded.lifecycle = "active".into(),
            "affiliation-end" => folded.lifecycle = "ended".into(),
            _ => {}
        }
        match entry.kind {
            "organization-initialize" | "organization-rename" => {
                folded.value["title"] = json!(entry.payload["title"].as_str());
                folded.value["aliases"] = serde_json::to_value(&entry.payload["aliases"])
                    .expect("validated literal aliases");
            }
            "organization-reparent" => {
                folded.value["parentId"] = json!(entry.payload["parent_id"].as_str())
            }
            "affiliation-initialize" | "affiliation-role" => {
                folded.value["role"] = json!(entry.payload["role"].as_str())
            }
            _ => {}
        }
        folded.active = ["active", "dormant"].contains(&folded.lifecycle.as_str());
    })
}

fn inspect_organizations(world: &World) -> (Vec<LiteralHistory<'_>>, Vec<Diagnostic>) {
    let mut histories = Vec::new();
    let mut diagnostics = Vec::new();
    for record in world
        .records
        .values()
        .filter(|record| ["organization", "affiliation"].contains(&record.kind.as_str()))
    {
        match parse_organization(world, record) {
            Ok(history) => histories.push(history),
            Err(error) => diagnostics.push(diagnostic(record, "GEN-HISTORY", error)),
        }
    }
    // Upstream validates the canonical static header graph. Reparenting is
    // folded at the query instant; combining all historical links would reject
    // legitimate detach/reparent sequences that never coexist.
    let graph: BTreeMap<_, _> = histories
        .iter()
        .filter(|history| history.record.kind == "organization" && canonical(history.record))
        .filter_map(|history| {
            history
                .record
                .string("parent_id")
                .map(|parent| (history.record.id.as_str(), (parent, history.record)))
        })
        .collect();
    let mut visited = BTreeSet::new();
    for start in graph.keys().copied() {
        let mut path = BTreeSet::new();
        let mut node = start;
        while !visited.contains(node) {
            if !path.insert(node) {
                let record = graph[node].1;
                diagnostics.push(diagnostic(
                    record,
                    "GEN-ORGANIZATION-002",
                    invalid(
                        "parent_id",
                        "canonical organization header graph contains a cycle",
                    ),
                ));
                break;
            }
            let Some((parent, _)) = graph.get(node) else {
                break;
            };
            node = parent;
        }
        visited.extend(path);
    }
    let mut affiliations: Vec<(&LiteralHistory<'_>, Option<&StoryTime>)> = Vec::new();
    for history in histories
        .iter()
        .filter(|history| history.record.kind == "affiliation" && canonical(history.record))
    {
        let start = &history.entries[0].applicability.first;
        let end = effective_indices(history, history.entries.len())
            .into_iter()
            .filter_map(|index| {
                (history.entries[index].kind == "affiliation-end")
                    .then_some(&history.entries[index].applicability.first)
            })
            .last();
        for (other, other_end) in &affiliations {
            if history.record.string("character_id") != other.record.string("character_id")
                || history.record.string("organization_id")
                    != other.record.string("organization_id")
            {
                continue;
            }
            let other_start = &other.entries[0].applicability.first;
            if start.timeline == other_start.timeline
                && other_end.is_none_or(|end| start.not_after(end))
                && end.is_none_or(|end| other_start.not_after(end))
            {
                diagnostics.push(diagnostic(history.record, "GEN-AFFILIATION-001", invalid("character_id", "canonical affiliations for the same character and organization cannot overlap, including shared endpoints")));
            }
        }
        affiliations.push((history, end));
    }
    (histories, diagnostics)
}
pub(super) fn validate(world: &World) -> Vec<Diagnostic> {
    inspect_organizations(world).1
}
pub(super) fn organization_at(
    world: &World,
    record: &Record,
    at: &StoryTime,
    limit: usize,
) -> Result<Option<Json>> {
    if record.kind != "organization" {
        return Err(invalid("kind", "expected an organization"));
    }
    let history = parse_organization(world, record)?;
    fold_organization(&history, at)
        .map(|folded| state_json(world, &history, at, &folded, limit))
        .transpose()
}
pub(super) fn history(world: &World, selector: &str, at: &StoryTime, limit: usize) -> Result<Json> {
    limits(limit, 0)?;
    require_valid(world)?;
    let record = world.resolve(selector)?;
    let history = parse_organization(world, record)?;
    let folded = fold_organization(&history, at);
    history_json(world, &history, at, folded.as_ref(), limit)
}

/// Exact current organizational identity, containment and authored members.
pub fn organization(
    world: &World,
    selector: &str,
    at: &StoryTime,
    max_depth: usize,
    include_former: bool,
    limit: usize,
) -> Result<Json> {
    limits(limit, max_depth)?;
    let mut result = envelope(world, at, limit)?;
    require_valid(world)?;
    let selected = world.resolve(selector)?;
    if selected.kind != "organization" {
        return Err(Error::InvalidInput(
            "organization query requires an organization".into(),
        ));
    }
    let (histories, _) = inspect_organizations(world);
    let by_id: BTreeMap<_, _> = histories
        .iter()
        .map(|history| (history.record.id.as_str(), history))
        .collect();
    let selected_history = by_id[selected.id.as_str()];
    result["recordId"] = json!(selected.id);
    result["recordTitle"] = json!(selected.title);
    result["maxDepth"] = json!(max_depth);
    result["includeFormer"] = json!(include_former);
    result["organization"] = Json::Null;
    result["parentPath"] = json!([]);
    result["roles"] = json!([]);
    if include_former {
        result["formerRoles"] = json!([]);
    }
    let Some(folded) = fold_organization(selected_history, at) else {
        result["available"] = json!(false);
        result["reason"] = json!(if !canonical(selected) {
            "noncanonical"
        } else if selected_history.entries[0].applicability.first.timeline != at.timeline {
            "different_timeline"
        } else {
            "before_initialization"
        });
        return Ok(result);
    };
    result["available"] = json!(true);
    result["reason"] = Json::Null;
    result["organization"] = state_json(world, selected_history, at, &folded, limit)?;
    let mut current = selected_history;
    let mut current_fold = folded;
    let mut seen = BTreeSet::from([selected.id.as_str()]);
    let mut steps = Vec::new();
    let mut paths = Vec::new();
    while current_fold.active {
        let Some(parent_id) = current_fold.value["parentId"].as_str() else {
            break;
        };
        let Some(parent) = by_id.get(parent_id).copied() else {
            break;
        };
        let Some(parent_fold) = fold_organization(parent, at).filter(|folded| folded.active) else {
            break;
        };
        if !seen.insert(parent.record.id.as_str()) {
            return Err(Error::InvalidInput(
                "organization containment contains a cycle at this horizon".into(),
            ));
        }
        if paths.len() == max_depth {
            return Err(Error::InvalidInput("organization containment exceeds maximum depth; increase depth for complete results".into()));
        }
        bounded(paths.len() + 1, limit)?;
        let evidence = state_json(world, current, at, &current_fold, limit)?;
        steps.push(json!({"from":current.record.id,"to":parent.record.id,"recordId":current.record.id,"citations":evidence["citations"]}));
        paths.push(json!({"targetId":parent.record.id,"edges":steps}));
        current = parent;
        current_fold = parent_fold;
    }
    let (mut roles, mut former) = (Vec::new(), Vec::new());
    for history in histories.iter().filter(|history| {
        history.record.kind == "affiliation"
            && history.record.string("organization_id") == Some(selected.id.as_str())
    }) {
        let Some(folded) = fold_organization(history, at) else {
            continue;
        };
        if folded.lifecycle == "active" {
            roles.push(state_json(world, history, at, &folded, limit)?);
        } else if include_former && folded.lifecycle == "ended" {
            former.push(state_json(world, history, at, &folded, limit)?);
        }
        bounded(roles.len() + former.len(), limit)?;
    }
    result["parentPath"] = json!(paths);
    result["roles"] = json!(roles);
    if include_former {
        result["formerRoles"] = json!(former);
    }
    Ok(result)
}
