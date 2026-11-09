//! An intentionally partial validator. A clean report is not full upstream conformance.
use crate::{
    model::{Record, StoryTime, World},
    source::{self, SOURCE_SCHEMA},
    state_schema as state,
};
use serde::Serialize;
use serde_yaml::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const CAPABILITIES: &[&str] = &[
    "generational-core-v1",
    "spatial-core-v1",
    "geometry-v1",
    "route-v1",
    "overlay-v1",
    "generational-knowledge-v1",
    "lore-core-v1",
    "descriptions-v1",
    "assertions-v1",
    "existence-v1",
    "collective-parties-v1",
    "spatial-history-v1",
    "briefs-v1",
];

const KINDS: &[(&str, &str)] = &[
    ("world", "world"),
    ("character", "char"),
    ("knowledge", "know"),
    ("event", "event"),
    ("object", "obj"),
    ("environment", "env"),
    ("location", "loc"),
    ("relationship", "rel"),
    ("story-point", "sp"),
    ("scene", "scene"),
    ("conversation", "conv"),
    ("hypothesis", "hyp"),
    ("organization", "organization"),
    ("parentage", "kinship"),
    ("union", "union"),
    ("affiliation", "affiliation"),
    ("legacy", "legacy"),
    ("tenure", "tenure"),
    ("claim", "claim"),
    ("vital-history", "vital"),
    ("lore-concept", "concept"),
    ("assertion", "assertion"),
    ("context-brief", "brief"),
    ("existence-episode", "existence"),
    ("map", "map"),
    ("anchor", "anchor"),
    ("portal", "portal"),
    ("route", "route"),
    ("overlay", "overlay"),
];

pub(crate) fn is_record_kind(kind: &str) -> bool {
    KINDS.iter().any(|(known, _)| *known == kind)
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    pub severity: String,
    pub entity_id: Option<String>,
    pub path: Option<String>,
    pub field: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ValidationReport {
    /// No errors among checks implemented here; inspect `complete` separately.
    pub valid: bool,
    pub complete: bool,
    pub diagnostics: Vec<Diagnostic>,
}

fn emit(
    out: &mut Vec<Diagnostic>,
    code: &str,
    message: impl Into<String>,
    record: Option<&Record>,
    field: &str,
    warning: bool,
) {
    out.push(Diagnostic {
        code: code.into(),
        message: message.into(),
        severity: if warning { "warning" } else { "error" }.into(),
        entity_id: record.map(|r| r.id.clone()),
        path: record.map(|r| r.source_path.clone()),
        field: if field.is_empty() {
            None
        } else {
            Some(field.into())
        },
    });
}

fn prefixed_id(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .and_then(|s| s.strip_prefix('_'))
        .is_some_and(|body| {
            body.len() == 26
                && body
                    .bytes()
                    .all(|b| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&b))
        })
}

fn spatial_id(value: &str, kind: &str) -> bool {
    let Some((prefix, path)) = value.split_once(':') else {
        return false;
    };
    if prefix != kind
        || !["location", "map", "anchor", "portal", "route", "overlay"].contains(&kind)
    {
        return false;
    }
    path.split('/').all(|part| {
        !part.is_empty()
            && part.len() <= 128
            && part.as_bytes()[0].is_ascii_alphanumeric()
            && part
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && !["con", "prn", "aux", "nul"].contains(&part)
            && !(part.len() == 4
                && (part.starts_with("com") || part.starts_with("lpt"))
                && matches!(part.as_bytes()[3], b'1'..=b'9'))
    })
}

pub(crate) fn valid_id_for_kind(value: &str, kind: &str) -> bool {
    if kind == "effect" {
        return prefixed_id(value, "effect");
    }
    KINDS
        .iter()
        .find(|(known, _)| *known == kind)
        .is_some_and(|(_, prefix)| prefixed_id(value, prefix) || spatial_id(value, kind))
}

pub(crate) fn valid_record_id(value: &str) -> bool {
    KINDS.iter().any(|(kind, _)| valid_id_for_kind(value, kind))
}

// References are selected by declared structural paths. A title, statement,
// tag, note, string state value or extension that happens to look like an ID
// remains literal author data. `*` below visits sequence elements, not maps.
fn reference_path(value: &Value, parts: &[&str], field: &str, out: &mut Vec<(String, String)>) {
    let Some((part, rest)) = parts.split_first() else {
        if let Some(target) = value.as_str() {
            out.push((field.into(), target.into()));
        }
        return;
    };
    if *part == "*" {
        if let Some(values) = value.as_sequence() {
            for (index, value) in values.iter().enumerate() {
                reference_path(value, rest, &format!("{field}[{index}]"), out);
            }
        }
    } else if let Some(value) = value.get(*part) {
        reference_path(
            value,
            rest,
            &if field.is_empty() {
                (*part).into()
            } else {
                format!("{field}.{part}")
            },
            out,
        );
    }
}

pub(crate) fn collect_refs(world: &World, record: &Record) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if record.kind == "assertion" {
        if let Ok(references) = crate::assertion::references(world, record) {
            out.extend(references);
        }
    }
    if record.kind == "context-brief" {
        if let Ok(references) = crate::briefs::references(world, record) {
            out.extend(references);
        }
    }
    let paths: &[&str] = match record.kind.as_str() {
        "knowledge" => &[
            "knower",
            "claim.subject",
            "claim.object.entity",
            "claim.assertion_id",
            "claim.genealogy.payload.child_id",
            "claim.genealogy.payload.parent_id",
            "claim.genealogy.payload.participant_ids.*",
            "claim.genealogy.payload.organization_id",
            "claim.genealogy.payload.character_id",
            "claim.genealogy.payload.legacy_id",
            "claim.genealogy.payload.holder_id",
            "claim.genealogy.payload.claimant_id",
            "claim.genealogy.evidence.*.entity_id",
            "transitions.*.causing_event",
            "transitions.*.source_entity",
        ],
        "relationship" => &["from", "to", "inverse", "transitions.*.causing_event"],
        "event" => &[
            "causes.*",
            "effects.*.target",
            "location",
            "participants.*.character",
            "related_story_points.*",
        ],
        "environment" => &["targets.*"],
        "location" => &[
            "parent_id",
            "links.*.location",
            "spatial.map_id",
            "spatial_history.*.value.map_id",
            "spatial_history.*.value.parent_id",
        ],
        "scene" => &[
            "location",
            "participants.*.character",
            "conversations.*",
            "objects.*",
            "environments.*",
            "story_points.*",
            "outcome_events.*",
        ],
        "conversation" => &[
            "location",
            "scene",
            "participants.*.character",
            "turns.*.speaker",
            "turns.*.actors.*",
            "recollections.*.character",
            "recollections.*.remembered_quotes.*.speaker",
        ],
        "story-point" => &["outcome_events.*", "lifecycle.transitions.*.causing_event"],
        "hypothesis" => &[
            "subjects.*",
            "placement.event",
            "placement.scene",
            "placement.location",
            "resolution.canonical_entities.*",
        ],
        "organization" => &["parent_id", "location_id"],
        "parentage" => &["child_id", "parent_id"],
        "union" => &["participant_ids.*"],
        "affiliation" => &["character_id", "organization_id"],
        "legacy" => &["organization_id"],
        "tenure" => &["legacy_id", "predecessor_tenure_id", "successor_tenure_id"],
        "claim" => &["legacy_id", "claimant_id"],
        "vital-history" | "existence-episode" => &["character_id"],
        "anchor" => &["from.map_id", "to.map_id"],
        "portal" => &["from_location_id", "to", "to.map_id"],
        "route" => &["from_location_id", "to_location_id"],
        "overlay" => &["membership.location_ids.*"],
        _ => &[],
    };
    for path in paths {
        reference_path(
            &record.frontmatter,
            &path.split('.').collect::<Vec<_>>(),
            "",
            &mut out,
        );
    }
    reference_path(
        &record.frontmatter,
        &["concept_links", "*", "concept_id"],
        "",
        &mut out,
    );
    if let Some(initial) = record.get("initial_state").and_then(Value::as_mapping) {
        for (key, value) in initial {
            if let Some(key) = key.as_str() {
                if !has_state_profile_binding(world, &record.kind, key)
                    && raw_state_definition(world, &record.kind, key)
                        .and_then(|s| s.get("type"))
                        .and_then(Value::as_str)
                        == Some("entity")
                {
                    reference_path(
                        value,
                        &["entity"],
                        &format!("initial_state.{key}"),
                        &mut out,
                    );
                }
            }
        }
    }
    if record.kind == "event" {
        if let Some(effects) = record.list("effects") {
            for (index, effect) in effects.iter().enumerate() {
                let target = effect
                    .get("target")
                    .and_then(Value::as_str)
                    .and_then(|id| world.records.get(id));
                let key = effect.get("key").and_then(Value::as_str);
                if target.zip(key).is_some_and(|(target, key)| {
                    !has_state_profile_binding(world, &target.kind, key)
                        && raw_state_definition(world, &target.kind, key)
                            .and_then(|s| s.get("type"))
                            .and_then(Value::as_str)
                            == Some("entity")
                }) {
                    reference_path(
                        effect,
                        &["value", "entity"],
                        &format!("effects[{index}]"),
                        &mut out,
                    );
                }
            }
        }
    }

    if world
        .world_record()
        .ok()
        .is_some_and(|owner| owner.get("state_profile_bindings").is_some())
    {
        if let Ok(references) = crate::lore_state::references(world, record) {
            out.extend(references);
        }
    }

    if matches!(
        record.kind.as_str(),
        "organization"
            | "parentage"
            | "union"
            | "affiliation"
            | "legacy"
            | "tenure"
            | "claim"
            | "vital-history"
    ) {
        for payload in ["initialization.payload", "transitions.*.payload"] {
            for field in [
                "parent_id",
                "participant_ids.*",
                "holder_id",
                "from_tenure_id",
                "to_tenure_id",
                "competes_with.*",
            ] {
                reference_path(
                    &record.frontmatter,
                    &format!("{payload}.{field}").split('.').collect::<Vec<_>>(),
                    "",
                    &mut out,
                );
            }
        }
        reference_path(
            &record.frontmatter,
            &["transitions", "*", "cause_event_id"],
            "",
            &mut out,
        );
    }
    out
}

fn capability_requirements(capability: &str) -> &[&str] {
    match capability {
        "geometry-v1" | "route-v1" | "overlay-v1" | "spatial-history-v1" => &["spatial-core-v1"],
        "generational-knowledge-v1" | "existence-v1" => &["generational-core-v1"],
        "assertions-v1" => &["lore-core-v1"],
        "collective-parties-v1" => &["lore-core-v1", "generational-core-v1"],
        _ => &[],
    }
}

fn required_capability(record: &Record) -> Option<&'static str> {
    match record.kind.as_str() {
        "knowledge"
            if record
                .get("claim")
                .and_then(|claim| claim.get("assertion_id"))
                .is_some() =>
        {
            Some("assertions-v1")
        }
        "knowledge"
            if record
                .get("claim")
                .and_then(|claim| claim.get("genealogy"))
                .is_some() =>
        {
            Some("generational-knowledge-v1")
        }
        "organization" | "parentage" | "union" | "affiliation" | "legacy" | "tenure" | "claim"
        | "vital-history" => Some("generational-core-v1"),
        "map" | "anchor" => Some("geometry-v1"),
        "route" | "portal" => Some("route-v1"),
        "overlay" => Some("overlay-v1"),
        "lore-concept" => Some("lore-core-v1"),
        "assertion" => Some("assertions-v1"),
        "context-brief" => Some("briefs-v1"),
        "existence-episode" => Some("existence-v1"),
        _ => None,
    }
}

fn point(
    world: &World,
    record: &Record,
    value: Option<&Value>,
    field: &str,
    out: &mut Vec<Diagnostic>,
) -> Option<StoryTime> {
    let result = value
        .ok_or_else(|| "missing story time".to_owned())
        .and_then(|value| {
            StoryTime::from_yaml(value, &world.default_timeline())
                .map_err(|error| error.to_string())
        })
        .and_then(|time| {
            world
                .story_time(time.tick, Some(&time.timeline), time.order)
                .map_err(|error| error.to_string())
        });
    match result {
        Ok(time) => Some(time),
        Err(error) => {
            emit(out, "WDL-TIME-002", error, Some(record), field, false);
            None
        }
    }
}

fn times(world: &World, record: &Record, out: &mut Vec<Diagnostic>) {
    match record.kind.as_str() {
        "world" => {
            if let Some(value) = record.get("current_time") {
                point(world, record, Some(value), "current_time", out);
            }
        }
        "event" => {
            point(world, record, record.get("time"), "time", out);
        }
        "scene" | "conversation" | "environment" => {
            let time = record.get("time");
            let start = point(
                world,
                record,
                time.and_then(|t| t.get("start")),
                "time.start",
                out,
            );
            for field in ["current", "end"] {
                let value = time.and_then(|t| t.get(field));
                if (field == "current" && record.kind == "scene")
                    || value.is_some_and(|v| !v.is_null())
                {
                    let end = point(world, record, value, &format!("time.{field}"), out);
                    if let (Some(start), Some(end)) = (&start, end) {
                        if !start.not_after(&end) {
                            emit(
                                out,
                                "WDL-TIME-004",
                                "interval endpoint precedes start or uses another timeline",
                                Some(record),
                                &format!("time.{field}"),
                                false,
                            );
                        }
                    }
                }
            }
            if let Some(values) = record.list("participants") {
                for (index, item) in values.iter().enumerate() {
                    for field in ["from", "until"] {
                        if let Some(value) = item.get(field).filter(|value| !value.is_null()) {
                            point(
                                world,
                                record,
                                Some(value),
                                &format!("participants[{index}].{field}"),
                                out,
                            );
                        }
                    }
                }
            }
            for (collection, fields) in [
                ("turns", &["at"][..]),
                ("recollections", &["at"][..]),
                ("observations", &["at", "until"][..]),
            ] {
                if let Some(values) = record.list(collection) {
                    for (index, item) in values.iter().enumerate() {
                        for field in fields {
                            if let Some(value) = item
                                .get(*field)
                                .filter(|value| *field != "until" || !value.is_null())
                            {
                                point(
                                    world,
                                    record,
                                    Some(value),
                                    &format!("{collection}[{index}].{field}"),
                                    out,
                                );
                            }
                        }
                    }
                }
            }
        }
        "knowledge" | "relationship" | "story-point" => {
            let (field, raw) = if record.kind == "story-point" {
                (
                    "lifecycle.transitions",
                    record.get("lifecycle").and_then(|v| v.get("transitions")),
                )
            } else {
                ("transitions", record.get("transitions"))
            };
            if let Some(raw) = raw {
                let Some(values) = raw.as_sequence() else {
                    emit(
                        out,
                        "WDL-TIME-001",
                        "transitions must be an array",
                        Some(record),
                        field,
                        false,
                    );
                    return;
                };
                let mut previous: Option<StoryTime> = None;
                for (index, item) in values.iter().enumerate() {
                    let next = point(
                        world,
                        record,
                        item.get("time"),
                        &format!("{field}[{index}].time"),
                        out,
                    );
                    if let (Some(previous), Some(next)) = (&previous, &next) {
                        if !previous.not_after(next) || previous == next {
                            emit(
                                out,
                                "WDL-TIME-001",
                                "transitions must be strictly ordered on one timeline",
                                Some(record),
                                field,
                                false,
                            );
                        }
                    }
                    previous = next;
                    if record.kind == "knowledge"
                        && ![
                            "accepted",
                            "suspected",
                            "rejected",
                            "uncertain",
                            "remembered",
                            "forgotten",
                        ]
                        .contains(&item.get("state").and_then(Value::as_str).unwrap_or(""))
                    {
                        emit(
                            out,
                            "WDL-STATE-001",
                            "invalid knowledge transition state",
                            Some(record),
                            &format!("{field}[{index}].state"),
                            false,
                        );
                    }
                }
            }
        }
        _ => {}
    }
}

// Reserve chronology at declared operational objects, not inside author data.
// In particular, state keys, metric names, environment conditions and opaque
// extensions may themselves be named "chronology". Paths select only objects
// whose fields have a structural meaning; `*` visits sequence members.
fn chronology_namespace_path(
    value: &Value,
    parts: &[&str],
    field: &str,
    record: &Record,
    out: &mut Vec<Diagnostic>,
) {
    let Some((part, rest)) = parts.split_first() else {
        if value
            .as_mapping()
            .is_some_and(|fields| fields.contains_key("chronology"))
        {
            emit(
                out,
                "WDL-CHRON-003",
                "chronology belongs on its source record, not a nested operational object",
                Some(record),
                &format!("{field}.chronology"),
                false,
            );
        }
        return;
    };
    if *part == "*" {
        if let Some(values) = value.as_sequence() {
            for (index, value) in values.iter().enumerate() {
                chronology_namespace_path(value, rest, &format!("{field}[{index}]"), record, out);
            }
        }
    } else if let Some(value) = value.get(*part) {
        let field = if field.is_empty() {
            (*part).to_owned()
        } else {
            format!("{field}.{part}")
        };
        chronology_namespace_path(value, rest, &field, record, out);
    }
}

fn chronology_namespaces(record: &Record, out: &mut Vec<Diagnostic>) {
    let paths: &[&str] = match record.kind.as_str() {
        "world" => &[
            "current_time",
            "timelines.*",
            "threads.*",
            "profiles.*",
            "profiles.*.fields",
            "predicates.*",
            "predicates.*.value_type",
        ],
        "event" => &["time", "effects.*", "participants.*"],
        "knowledge" => &[
            "claim",
            "claim.object",
            "claim.genealogy",
            "claim.genealogy.payload",
            "claim.genealogy.valid",
            "claim.genealogy.valid.from",
            "claim.genealogy.valid.until",
            "claim.genealogy.labels",
            "claim.genealogy.evidence.*",
            "transitions.*",
            "transitions.*.time",
        ],
        "relationship" => &["transitions.*", "transitions.*.time"],
        "story-point" => &[
            "lifecycle",
            "lifecycle.transitions.*",
            "lifecycle.transitions.*.time",
            "requires.*",
            "when",
        ],
        "scene" | "conversation" => &[
            "time",
            "time.start",
            "time.current",
            "time.end",
            "participants.*",
            "participants.*.from",
            "participants.*.until",
            "observations.*",
            "observations.*.at",
            "observations.*.until",
            "turns.*",
            "turns.*.at",
            "recollections.*",
            "recollections.*.at",
            "recollections.*.remembered_quotes.*",
        ],
        "environment" => &["time", "time.start", "time.end"],
        "character" => &["pronouns"],
        "location" => &["links.*", "spatial", "spatial.geometry"],
        "map" => &["bounds", "origin", "scale"],
        "anchor" => &["from", "to"],
        "portal" => &["to"],
        "overlay" => &["membership", "valid", "valid.start", "valid.end"],
        "lore-concept" => &["valid", "valid.start", "valid.end"],
        "assertion" => &[
            "subject",
            "attributed_to",
            "valid",
            "valid.start",
            "valid.end",
            "evidence.*",
            "evidence.*.target",
        ],
        "existence-episode" => &[
            "valid",
            "valid.start",
            "valid.end",
            "start_evidence.*",
            "start_evidence.*.target",
            "end_evidence.*",
            "end_evidence.*.target",
        ],
        "hypothesis" => &["placement", "resolution"],
        "organization" | "parentage" | "union" | "affiliation" | "legacy" | "tenure" | "claim"
        | "vital-history" => &[
            "initialization",
            "initialization.applicability",
            "initialization.applicability.point",
            "initialization.payload",
            "transitions.*",
            "transitions.*.applicability",
            "transitions.*.applicability.point",
            "transitions.*.applicability.first",
            "transitions.*.applicability.last",
            "transitions.*.payload",
        ],
        _ => &[],
    };
    for path in paths {
        chronology_namespace_path(
            &record.frontmatter,
            &path.split('.').collect::<Vec<_>>(),
            "",
            record,
            out,
        );
    }
    for path in [
        "concept_links.*",
        "concept_links.*.valid",
        "concept_links.*.valid.start",
        "concept_links.*.valid.end",
        "descriptions.*",
        "descriptions.*.valid",
        "descriptions.*.valid.start",
        "descriptions.*.valid.end",
    ] {
        chronology_namespace_path(
            &record.frontmatter,
            &path.split('.').collect::<Vec<_>>(),
            "",
            record,
            out,
        );
    }
    if matches!(record.kind.as_str(), "location" | "route") {
        for path in [
            "spatial_history.*",
            "spatial_history.*.valid",
            "spatial_history.*.valid.start",
            "spatial_history.*.valid.end",
            "spatial_history.*.value",
            "spatial_history.*.value.geometry",
            "spatial_history.*.value.route_distance",
            "spatial_history.*.value.travel_cost",
            "spatial_history.*.value.duration",
        ] {
            chronology_namespace_path(
                &record.frontmatter,
                &path.split('.').collect::<Vec<_>>(),
                "",
                record,
                out,
            );
        }
    }
}

fn has_state_profile_binding(world: &World, kind: &str, key: &str) -> bool {
    world
        .world_record()
        .ok()
        .and_then(|owner| owner.get("state_profile_bindings"))
        .and_then(|bindings| bindings.get(kind))
        .is_some_and(|bindings| bindings.get(key).is_some())
}

fn raw_state_definition<'a>(world: &'a World, kind: &str, key: &str) -> Option<&'a Value> {
    world
        .world_record()
        .ok()?
        .get("state_keys")?
        .get(kind)?
        .get(key)
}

fn state_declarations(world: &World, out: &mut Vec<Diagnostic>) {
    let Ok(owner) = world.world_record() else {
        return;
    };
    let Some(value) = owner.get("state_keys") else {
        return;
    };
    let Value::Mapping(kinds) = value else {
        emit(
            out,
            "WDL-STATE-SCHEMA",
            "state_keys must be a mapping",
            Some(owner),
            "state_keys",
            false,
        );
        return;
    };
    for (kind, fields) in kinds {
        let Value::String(kind) = kind else {
            emit(
                out,
                "WDL-STATE-SCHEMA",
                "state kind must be a string",
                Some(owner),
                "state_keys",
                false,
            );
            continue;
        };
        if !is_record_kind(kind) {
            emit(
                out,
                "WDL-STATE-SCHEMA",
                format!("unknown state owner kind {kind:?}"),
                Some(owner),
                &format!("state_keys.{kind}"),
                false,
            );
        }
        let Value::Mapping(fields) = fields else {
            emit(
                out,
                "WDL-STATE-SCHEMA",
                "state fields must be a mapping",
                Some(owner),
                &format!("state_keys.{kind}"),
                false,
            );
            continue;
        };
        for (key, definition) in fields {
            let Value::String(key) = key else {
                emit(
                    out,
                    "WDL-STATE-SCHEMA",
                    "state key must be a nonempty string",
                    Some(owner),
                    &format!("state_keys.{kind}"),
                    false,
                );
                continue;
            };
            if key.trim().is_empty() {
                emit(
                    out,
                    "WDL-STATE-SCHEMA",
                    "state key must be a nonempty string",
                    Some(owner),
                    &format!("state_keys.{kind}"),
                    false,
                );
            }
            if let Err(error) = state::validate_definition(definition) {
                emit(
                    out,
                    "WDL-STATE-SCHEMA",
                    error.to_string(),
                    Some(owner),
                    &format!("state_keys.{kind}.{key}"),
                    false,
                );
            }
        }
    }
}

fn state_value(
    world: &World,
    record: &Record,
    key: &str,
    value: &Value,
    field: &str,
    out: &mut Vec<Diagnostic>,
) {
    match state::field_definition(world, &record.kind, key) {
        Ok(definition) => {
            if let Err(error) = state::validate_value(world, &definition, value) {
                emit(
                    out,
                    "WDL-STATE-011",
                    error.to_string(),
                    Some(record),
                    field,
                    false,
                );
            }
        }
        Err(error) => emit(
            out,
            "WDL-STATE-010",
            error.to_string(),
            Some(record),
            field,
            false,
        ),
    }
}

fn exclusive(
    world: &World,
    record: &Record,
    state: &BTreeMap<String, Value>,
    field: &str,
    out: &mut Vec<Diagnostic>,
) {
    let mut groups: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for key in state.keys() {
        if let Some(group) = raw_state_definition(world, &record.kind, key)
            .and_then(|value| value.get("exclusive_group"))
            .and_then(Value::as_str)
        {
            groups.entry(group).or_default().push(key);
        }
    }
    for (group, keys) in groups {
        if keys.len() > 1 {
            emit(
                out,
                "WDL-STATE-015",
                format!(
                    "exclusive state group {group:?} has multiple keys: {}",
                    keys.join(", ")
                ),
                Some(record),
                field,
                false,
            );
        }
    }
}

fn state_and_events(world: &World, out: &mut Vec<Diagnostic>) {
    let mut initial: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
    for record in world.records.values() {
        let mut state = BTreeMap::new();
        if let Some(value) = record.get("initial_state") {
            if let Some(mapping) = value.as_mapping() {
                for (key, value) in mapping {
                    if let Some(key) = key.as_str() {
                        state_value(
                            world,
                            record,
                            key,
                            value,
                            &format!("initial_state.{key}"),
                            out,
                        );
                        state.insert(key.into(), value.clone());
                    } else {
                        emit(
                            out,
                            "WDL-STATE-016",
                            "initial_state keys must be strings",
                            Some(record),
                            "initial_state",
                            false,
                        );
                    }
                }
            } else {
                emit(
                    out,
                    "WDL-STATE-016",
                    "initial_state must be a mapping",
                    Some(record),
                    "initial_state",
                    false,
                );
            }
        }
        exclusive(world, record, &state, "initial_state", out);
        initial.insert(record.id.clone(), state);
    }
    let default = world.default_timeline();
    let mut events: Vec<_> = world
        .records
        .values()
        .filter(|r| r.kind == "event")
        .filter_map(|r| {
            r.get("time")
                .and_then(|t| StoryTime::from_yaml(t, &default).ok())
                .map(|t| (t, r))
        })
        .collect();
    events.sort_by(|(ta, a), (tb, b)| (ta, &a.id).cmp(&(tb, &b.id)));
    let mut writes = BTreeMap::new();
    let mut states: BTreeMap<(String, String), BTreeMap<String, Value>> = BTreeMap::new();
    for (time, event) in events {
        if let Some(raw) = event.get("causes") {
            if let Some(causes) = raw.as_sequence() {
                for (index, cause) in causes.iter().enumerate() {
                    let candidate = cause.as_str().and_then(|id| world.records.get(id));
                    let valid = candidate.is_some_and(|cause| {
                        cause.kind == "event"
                            && (event.status() != "canonical" || cause.status() == "canonical")
                            && cause
                                .get("time")
                                .and_then(|v| StoryTime::from_yaml(v, &default).ok())
                                .is_some_and(|before| before.not_after(&time) && before != time)
                    });
                    if !valid {
                        emit(
                            out,
                            "WDL-CAUSE-001",
                            "cause must reference an earlier event on the same timeline; canonical events require canonical causes",
                            Some(event),
                            &format!("causes[{index}]"),
                            false,
                        );
                    }
                }
            } else {
                emit(
                    out,
                    "WDL-CAUSE-001",
                    "causes must be an array",
                    Some(event),
                    "causes",
                    false,
                );
            }
        }
        let Some(raw) = event.get("effects") else {
            continue;
        };
        let Some(effects) = raw.as_sequence() else {
            emit(
                out,
                "WDL-EVENT-002",
                "effects must be an array",
                Some(event),
                "effects",
                false,
            );
            continue;
        };
        let mut effect_ids = BTreeSet::new();
        for (index, effect) in effects.iter().enumerate() {
            let field = format!("effects[{index}]");
            if !effect.is_mapping() {
                emit(
                    out,
                    "WDL-EVENT-002",
                    "effect must be a mapping",
                    Some(event),
                    &field,
                    false,
                );
                continue;
            }
            let id = effect.get("id").and_then(Value::as_str).unwrap_or("");
            if !prefixed_id(id, "effect") || !effect_ids.insert(id) {
                emit(
                    out,
                    "WDL-EVENT-003",
                    "invalid or duplicate effect ID",
                    Some(event),
                    &format!("{field}.id"),
                    false,
                );
            }
            let operation = effect
                .get("operation")
                .and_then(Value::as_str)
                .unwrap_or("");
            if !["set", "clear", "add-to-set", "remove-from-set"].contains(&operation) {
                emit(
                    out,
                    "WDL-EVENT-005",
                    "unsupported effect operation",
                    Some(event),
                    &format!("{field}.operation"),
                    false,
                );
                continue;
            }
            let Some(owner) = effect
                .get("target")
                .and_then(Value::as_str)
                .and_then(|id| world.records.get(id))
            else {
                emit(
                    out,
                    "WDL-STATE-017",
                    "effect must target an existing record",
                    Some(event),
                    &format!("{field}.target"),
                    false,
                );
                continue;
            };
            let key = effect.get("key").and_then(Value::as_str).unwrap_or("");
            if let Err(error) = state::validate_effect(world, owner, effect) {
                let code = if operation == "clear" && effect.get("value").is_some() {
                    "WDL-STATE-018"
                } else if raw_state_definition(world, &owner.kind, key).is_none() {
                    "WDL-STATE-010"
                } else {
                    "WDL-STATE-011"
                };
                emit(out, code, error.to_string(), Some(event), &field, false);
                continue;
            }
            if event.status() != "canonical" {
                continue;
            }
            let write_key = (time.clone(), owner.id.as_str(), key);
            if writes
                .insert(write_key, event.id.as_str())
                .is_some_and(|previous| previous != event.id)
            {
                emit(
                    out,
                    "WDL-STATE-020",
                    "two canonical events write the same state cell at the same coordinate",
                    Some(event),
                    &field,
                    false,
                );
            }
            let state = states
                .entry((time.timeline.clone(), owner.id.clone()))
                .or_insert_with(|| initial.get(&owner.id).cloned().unwrap_or_default());
            if let Err(error) = state::apply_effect(world, owner, state, effect) {
                let message = error.to_string();
                let code = if message.contains("exclusive state group") {
                    "WDL-STATE-015"
                } else {
                    "WDL-STATE-021"
                };
                emit(out, code, message, Some(event), &field, false);
            }
        }
    }
}

// Strict shapes for the core records the typed editor can create. These checks
// intentionally visit authored fields, never strings found inside prose.
fn core_text<'a>(value: Option<&'a Value>, field: &str) -> Result<&'a str, String> {
    match value {
        Some(Value::String(value)) if !value.trim().is_empty() => Ok(value),
        _ => Err(format!("{field}: expected a nonblank string")),
    }
}

fn core_mapping<'a>(
    value: Option<&'a Value>,
    required: &[&str],
    optional: &[&str],
    field: &str,
) -> Result<&'a Value, String> {
    let Some(value @ Value::Mapping(fields)) = value else {
        return Err(format!("{field}: expected a mapping"));
    };
    for key in fields.keys() {
        let key = core_text(Some(key), field)?;
        if !required.contains(&key) && !optional.contains(&key) {
            return Err(format!("{field}.{key}: unknown field"));
        }
    }
    for key in required {
        if !fields.contains_key(*key) {
            return Err(format!("{field}.{key}: required field is missing"));
        }
    }
    Ok(value)
}

fn core_list<'a>(value: Option<&'a Value>, field: &str) -> Result<&'a [Value], String> {
    match value {
        None => Ok(&[]),
        Some(Value::Sequence(values)) => Ok(values),
        _ => Err(format!("{field}: expected an array")),
    }
}

fn core_reference<'a>(
    world: &'a World,
    value: Option<&Value>,
    kind: Option<&str>,
    field: &str,
) -> Result<&'a Record, String> {
    let id = core_text(value, field)?;
    let record = world
        .records
        .get(id)
        .filter(|record| valid_record_id(id) && kind.is_none_or(|kind| record.kind == kind));
    record.ok_or_else(|| {
        format!(
            "{field}: expected an exact existing {}ID",
            kind.map(|kind| format!("{kind} ")).unwrap_or_default()
        )
    })
}

fn core_optional_reference<'a>(
    world: &'a World,
    value: Option<&Value>,
    kind: Option<&str>,
    field: &str,
) -> Result<Option<&'a Record>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => core_reference(world, Some(value), kind, field).map(Some),
    }
}

fn core_prose(value: Option<&Value>, field: &str) -> Result<(), String> {
    if value.is_none_or(|value| matches!(value, Value::String(_))) {
        Ok(())
    } else {
        Err(format!("{field}: expected a string"))
    }
}

fn core_enum<'a>(
    value: Option<&'a Value>,
    allowed: &[&str],
    field: &str,
) -> Result<&'a str, String> {
    let value = core_text(value, field)?;
    if allowed.contains(&value) {
        Ok(value)
    } else {
        Err(format!("{field}: unsupported value {value:?}"))
    }
}

fn core_time(world: &World, value: Option<&Value>, field: &str) -> Result<StoryTime, String> {
    let value = core_mapping(value, &["tick"], &["timeline", "order"], field)?;
    let time = StoryTime::from_yaml(value, &world.default_timeline())
        .map_err(|e| format!("{field}: {e}"))?;
    world
        .story_time(time.tick, Some(&time.timeline), time.order)
        .map_err(|e| format!("{field}: {e}"))
}

fn core_event(world: &World, record: &Record) -> Result<(), String> {
    core_time(world, record.get("time"), "time")?;
    core_optional_reference(world, record.get("location"), Some("location"), "location")?;
    for (index, participant) in core_list(record.get("participants"), "participants")?
        .iter()
        .enumerate()
    {
        let field = format!("participants[{index}]");
        core_mapping(Some(participant), &["character"], &["role"], &field)?;
        core_reference(
            world,
            participant.get("character"),
            Some("character"),
            &format!("{field}.character"),
        )?;
        core_prose(participant.get("role"), &format!("{field}.role"))?;
    }
    for (field, kind) in [("causes", "event"), ("related_story_points", "story-point")] {
        let mut seen = BTreeSet::new();
        for (index, value) in core_list(record.get(field), field)?.iter().enumerate() {
            let reference =
                core_reference(world, Some(value), Some(kind), &format!("{field}[{index}]"))?;
            if !seen.insert(&reference.id) {
                return Err(format!("{field}[{index}]: duplicate reference"));
            }
        }
    }
    let mut ids = BTreeSet::new();
    for (index, effect) in core_list(record.get("effects"), "effects")?
        .iter()
        .enumerate()
    {
        let field = format!("effects[{index}]");
        core_mapping(
            Some(effect),
            &["id", "target", "key", "operation"],
            &["value"],
            &field,
        )?;
        let id = core_text(effect.get("id"), &format!("{field}.id"))?;
        if !valid_id_for_kind(id, "effect") || !ids.insert(id) {
            return Err(format!("{field}.id: invalid or duplicate effect ID"));
        }
        core_reference(
            world,
            effect.get("target"),
            None,
            &format!("{field}.target"),
        )?;
        core_text(effect.get("key"), &format!("{field}.key"))?;
        let operation = core_enum(
            effect.get("operation"),
            &["set", "clear", "add-to-set", "remove-from-set"],
            &format!("{field}.operation"),
        )?;
        if operation == "clear" && effect.get("value").is_some() {
            return Err(format!("{field}.value: clear forbids a value"));
        }
        if operation != "clear" && effect.get("value").is_none() {
            return Err(format!("{field}.value: required field is missing"));
        }
    }
    Ok(())
}

fn core_knowledge(world: &World, record: &Record) -> Result<(), String> {
    core_reference(world, record.get("knower"), Some("character"), "knower")?;
    let linked = record
        .get("claim")
        .is_some_and(|claim| claim.get("assertion_id").is_some());
    let claim = core_mapping(
        record.get("claim"),
        if linked {
            &["key", "statement", "assertion_id"]
        } else {
            &["key", "statement"]
        },
        if linked {
            &[]
        } else {
            &["subject", "predicate", "object", "truth"]
        },
        "claim",
    )?;
    core_text(claim.get("key"), "claim.key")?;
    core_text(claim.get("statement"), "claim.statement")?;
    if linked {
        core_reference(
            world,
            claim.get("assertion_id"),
            Some("assertion"),
            "claim.assertion_id",
        )?;
    }
    core_optional_reference(world, claim.get("subject"), None, "claim.subject")?;
    if claim.get("predicate").is_some() {
        core_text(claim.get("predicate"), "claim.predicate")?;
    }
    if let Some(object) = claim.get("object") {
        let object = core_mapping(Some(object), &[], &["text", "entity"], "claim.object")?;
        if object.as_mapping().is_none_or(|values| values.len() != 1) {
            return Err("claim.object: expected exactly one text or entity field".into());
        }
        if let Some(entity) = object.get("entity") {
            core_reference(world, Some(entity), None, "claim.object.entity")?;
        } else {
            core_text(object.get("text"), "claim.object.text")?;
        }
    }
    if claim.get("truth").is_some() {
        core_enum(
            claim.get("truth"),
            &["true", "false", "unknown", "contested"],
            "claim.truth",
        )?;
    }
    let mut ids = BTreeSet::new();
    for (index, transition) in core_list(record.get("transitions"), "transitions")?
        .iter()
        .enumerate()
    {
        let field = format!("transitions[{index}]");
        core_mapping(
            Some(transition),
            &["id", "time", "state", "confidence", "acquisition"],
            &["source_entity", "causing_event", "note"],
            &field,
        )?;
        let id = core_text(transition.get("id"), &format!("{field}.id"))?;
        if !ids.insert(id) {
            return Err(format!("{field}.id: duplicate transition ID"));
        }
        let at = core_time(world, transition.get("time"), &format!("{field}.time"))?;
        core_enum(
            transition.get("state"),
            &[
                "accepted",
                "suspected",
                "rejected",
                "uncertain",
                "remembered",
                "forgotten",
            ],
            &format!("{field}.state"),
        )?;
        core_enum(
            transition.get("acquisition"),
            &[
                "authored",
                "experienced",
                "heard",
                "inferred",
                "observed",
                "read",
                "remembered",
                "tested",
                "told",
            ],
            &format!("{field}.acquisition"),
        )?;
        if !matches!(transition.get("confidence"),Some(Value::Number(value)) if value.as_f64().is_some_and(|n|n.is_finite() && (0.0..=1.0).contains(&n)))
        {
            return Err(format!(
                "{field}.confidence: expected a finite number between 0 and 1"
            ));
        }
        core_optional_reference(
            world,
            transition.get("source_entity"),
            None,
            &format!("{field}.source_entity"),
        )?;
        if let Some(cause) = core_optional_reference(
            world,
            transition.get("causing_event"),
            Some("event"),
            &format!("{field}.causing_event"),
        )? {
            if cause.status() != "canonical"
                || !core_time(
                    world,
                    cause.get("time"),
                    &format!("{field}.causing_event.time"),
                )?
                .not_after(&at)
            {
                return Err(format!(
                    "{field}.causing_event: expected a canonical event at or before the transition on the same timeline"
                ));
            }
        }
        core_prose(transition.get("note"), &format!("{field}.note"))?;
    }
    Ok(())
}

fn core_record(world: &World, record: &Record) -> Result<(), String> {
    if record.kind == "knowledge" {
        for name in ["audience", "perspectives"] {
            if let Some(value) = record.get(name) {
                let values = value
                    .as_sequence()
                    .ok_or_else(|| format!("{name}: expected an untagged string array"))?;
                let mut seen = BTreeSet::new();
                for value in values {
                    let label = core_text(Some(value), name)?;
                    if !seen.insert(label) {
                        return Err(format!("{name}: duplicate scope label"));
                    }
                }
            }
        }
    }
    match record.kind.as_str() {
        "event" => core_event(world, record),
        "knowledge"
            if record
                .get("claim")
                .and_then(|claim| claim.get("genealogy"))
                .is_some() =>
        {
            Ok(())
        }
        "knowledge" => core_knowledge(world, record),
        "character" => {
            core_prose(record.get("role"), "role")?;
            if let Some(value) = record.get("pronouns") {
                core_mapping(
                    Some(value),
                    &["subject", "object"],
                    &["possessive"],
                    "pronouns",
                )?;
                for field in ["subject", "object", "possessive"] {
                    if value.get(field).is_some() {
                        core_text(value.get(field), &format!("pronouns.{field}"))?;
                    }
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

// These declarations describe prose audiences, not a date of acquisition.
// Character projections therefore still exclude ordinary untimed Markdown.
fn section_audiences(world: &World, record: &Record, out: &mut Vec<Diagnostic>) {
    let Some(value) = record.get("section_audiences") else {
        return;
    };
    let Value::Mapping(sections) = value else {
        emit(
            out,
            "WDL-AUDIENCE-001",
            "section_audiences must be a mapping",
            Some(record),
            "section_audiences",
            false,
        );
        return;
    };
    let mut names = BTreeSet::new();
    for (name, audience) in sections {
        let Value::String(name) = name else {
            emit(
                out,
                "WDL-AUDIENCE-001",
                "section names must be nonblank single-line strings",
                Some(record),
                "section_audiences",
                false,
            );
            continue;
        };
        if name.trim().is_empty() || name.chars().any(char::is_control) {
            emit(
                out,
                "WDL-AUDIENCE-001",
                "section names must be nonblank single-line strings",
                Some(record),
                "section_audiences",
                false,
            );
            continue;
        }
        if !names.insert(name.trim().to_lowercase()) {
            emit(
                out,
                "WDL-AUDIENCE-001",
                "section names must be unique after trimming and case folding",
                Some(record),
                "section_audiences",
                false,
            );
        }
        let field = format!("section_audiences.{name}");
        let Value::Sequence(values) = audience else {
            emit(
                out,
                "WDL-AUDIENCE-001",
                "section audience must be an array",
                Some(record),
                &field,
                false,
            );
            continue;
        };
        let mut seen = BTreeSet::new();
        for value in values {
            let token = match value {
                Value::String(value) => value.as_str(),
                _ => "",
            };
            let valid = ["public", "author", "self"].contains(&token)
                || (token == "participants"
                    && ["scene", "conversation"].contains(&record.kind.as_str()))
                || world
                    .records
                    .get(token)
                    .is_some_and(|target| target.kind == "character");
            if !valid || !seen.insert(token) {
                emit(
                    out,
                    "WDL-AUDIENCE-001",
                    "audience entries must be unique known tokens or exact character IDs",
                    Some(record),
                    &field,
                    false,
                );
            }
        }
    }
}

pub fn validate_world(world: &World) -> ValidationReport {
    #[cfg(feature = "snapshot-baseline")]
    crate::snapshot_baseline::record(|stats| stats.full_validations += 1);
    let mut diagnostics = Vec::new();
    let owner = match world.world_record() {
        Ok(owner) => owner,
        Err(error) => {
            emit(
                &mut diagnostics,
                "WDL-WORLD-001",
                error.to_string(),
                None,
                "",
                false,
            );
            return ValidationReport {
                valid: false,
                complete: false,
                diagnostics,
            };
        }
    };
    emit(
        &mut diagnostics,
        "RUST-PARTIAL-VALIDATION",
        "Implemented source, temporal/state, scene, spatial, chronology, canonical/learned history, typed lore, narrative evidence, brief and family-context checks; complete privacy, action/manuscript domains and transaction conformance are not certified.",
        Some(owner),
        "",
        true,
    );
    // Public library callers can construct/mutate models. Their parsed fields
    // must still agree with the exact bytes whose fingerprint they advertise.
    // Refuse an inconsistent projection before semantic evaluation or caches.
    for (map_id, record) in &world.records {
        let consistent =
            source::parse_record(&record.raw_bytes, &record.source_path).is_ok_and(|parsed| {
                parsed.id == *map_id
                    && parsed.id == record.id
                    && parsed.kind == record.kind
                    && parsed.title == record.title
                    && parsed.body == record.body
                    && parsed.frontmatter == record.frontmatter
            });
        if !consistent {
            emit(
                &mut diagnostics,
                "RUST-SOURCE-MISMATCH",
                "parsed record fields do not match its exact canonical source bytes",
                Some(record),
                "",
                false,
            );
        }
    }
    if source::fingerprint_for_records(&world.records) != world.fingerprint {
        emit(
            &mut diagnostics,
            "RUST-FINGERPRINT-MISMATCH",
            "snapshot fingerprint does not match the captured source records",
            Some(owner),
            "",
            false,
        );
    }
    if diagnostics.iter().any(|d| d.severity == "error") {
        return ValidationReport {
            valid: false,
            complete: false,
            diagnostics,
        };
    }
    let mut capabilities = BTreeSet::new();
    if let Some(values) = owner.list("capabilities") {
        let mut previous = None;
        for value in values {
            let capability = value.as_str().unwrap_or("");
            let index = CAPABILITIES.iter().position(|name| *name == capability);
            if index.is_none()
                || !capabilities.insert(capability)
                || previous.zip(index).is_some_and(|(a, b)| a >= b)
            {
                emit(
                    &mut diagnostics,
                    "NAR-CAPABILITY-001",
                    "capabilities must be known, unique, and in registry order",
                    Some(owner),
                    "capabilities",
                    false,
                );
            }
            previous = index;
        }
        for capability in &capabilities {
            if capability_requirements(capability)
                .iter()
                .any(|required| !capabilities.contains(required))
            {
                emit(
                    &mut diagnostics,
                    "NAR-CAPABILITY-001",
                    format!("missing prerequisite for {capability}"),
                    Some(owner),
                    "capabilities",
                    false,
                );
            }
            if CAPABILITIES.contains(capability) {
                let message = if *capability == "generational-core-v1" {
                    "Canonical generational histories and dated discovery/context are implemented; complete upstream conformance is not certified".into()
                } else if *capability == "generational-knowledge-v1" {
                    "Typed generational beliefs, learned labels and evidence admission are implemented; complete upstream conformance is not certified".into()
                } else if *capability == "lore-core-v1" {
                    "Typed profiles, bound state, scoped concepts/links, native family evidence, explicit character family reads, local semantic/hybrid retrieval, reviewed outcome publication, consequence analysis and revision review are implemented; complete upstream conformance is not certified".into()
                } else if *capability == "descriptions-v1" {
                    "Dated sections, corrections and explicit character admission are implemented, including generic character context/search; separate reveal-plan reader policies do not imply complete privacy conformance".into()
                } else if *capability == "assertions-v1" {
                    "Typed predicates, classified author assertions and separately learned character claims are implemented; complete upstream conformance is not certified".into()
                } else if *capability == "existence-v1" {
                    "Authored existence episodes, evidence syntax and scoped author reads are implemented; typed character episode admission remains unavailable".into()
                } else if *capability == "collective-parties-v1" {
                    "Explicit author governance roles and Party projections are implemented; character governance preserves learned words without assigning canonical jurisdiction or authority".into()
                } else if *capability == "briefs-v1" {
                    "Authored briefs, exact required-reference composition, captured writing frames, saved outcomes and manuscript workflows are implemented; complete upstream conformance is not certified".into()
                } else if [
                    "spatial-core-v1",
                    "geometry-v1",
                    "route-v1",
                    "overlay-v1",
                    "spatial-history-v1",
                ]
                .contains(capability)
                {
                    "Geometry, explicit spatial topology/history and author overlays are implemented; character history uses acquired endpoint admission, while structured reader spatial projections remain unavailable".into()
                } else {
                    format!(
                        "{capability} is retained and recognized; its full semantic validator is not implemented"
                    )
                };
                emit(
                    &mut diagnostics,
                    "RUST-CAPABILITY-PARTIAL",
                    message,
                    Some(owner),
                    "capabilities",
                    true,
                );
            }
        }
    } else {
        emit(
            &mut diagnostics,
            "NAR-CAPABILITY-001",
            "capabilities must be an array of strings",
            Some(owner),
            "capabilities",
            false,
        );
    }
    let mut declared = BTreeSet::new();
    if let Some(values) = owner.list("timelines").filter(|v| !v.is_empty()) {
        for (index, value) in values.iter().enumerate() {
            let id = value.get("id").and_then(Value::as_str).unwrap_or("");
            let label = value.get("label").and_then(Value::as_str).unwrap_or("");
            if id.trim().is_empty() || label.trim().is_empty() || !declared.insert(id) {
                emit(
                    &mut diagnostics,
                    "WDL-TIMELINE-002",
                    "timelines require unique nonempty IDs and nonempty labels",
                    Some(owner),
                    &format!("timelines[{index}]"),
                    false,
                );
            }
        }
    } else {
        emit(
            &mut diagnostics,
            "WDL-TIMELINE-001",
            "timelines must be a nonempty array",
            Some(owner),
            "timelines",
            false,
        );
    }
    if !declared.contains(world.default_timeline().as_str()) {
        emit(
            &mut diagnostics,
            "WDL-TIMELINE-011",
            "default timeline must be declared",
            Some(owner),
            "default_timeline",
            false,
        );
    }
    let mut declared_threads = BTreeSet::new();
    if let Some(threads) = owner.list("threads") {
        let mut previous = "";
        for item in threads {
            let id = item.get("id").and_then(Value::as_str).unwrap_or("");
            if !prefixed_id(id, "thread")
                || id <= previous
                || item
                    .get("label")
                    .and_then(Value::as_str)
                    .is_none_or(|s| s.trim().is_empty())
            {
                emit(
                    &mut diagnostics,
                    "WDL-THREAD-006",
                    "threads require valid sorted unique IDs and labels",
                    Some(owner),
                    "threads",
                    false,
                );
            }
            declared_threads.insert(id);
            previous = id;
        }
    } else if owner.get("threads").is_some() {
        emit(
            &mut diagnostics,
            "WDL-THREAD-001",
            "threads must be an array",
            Some(owner),
            "threads",
            false,
        );
    }
    let mut paths = BTreeSet::new();
    for record in world.records.values() {
        if record.string("schema") != Some(SOURCE_SCHEMA) {
            emit(
                &mut diagnostics,
                "WDL-SRC-001",
                format!("expected {SOURCE_SCHEMA}"),
                Some(record),
                "schema",
                false,
            );
        }
        if record.string("id") != Some(record.id.as_str())
            || record.string("kind") != Some(record.kind.as_str())
            || record.string("title") != Some(record.title.as_str())
        {
            emit(
                &mut diagnostics,
                "WDL-SRC-010",
                "cached record identity differs from frontmatter",
                Some(record),
                "",
                false,
            );
        }
        if let Some((_, prefix)) = KINDS.iter().find(|(kind, _)| *kind == record.kind) {
            if !prefixed_id(&record.id, prefix) && !spatial_id(&record.id, &record.kind) {
                emit(
                    &mut diagnostics,
                    "WDL-ID-001",
                    "invalid ID for record kind",
                    Some(record),
                    "id",
                    false,
                );
            }
        } else {
            emit(
                &mut diagnostics,
                "WDL-SRC-002",
                "unknown record kind",
                Some(record),
                "kind",
                false,
            );
        }
        if !paths.insert(record.source_path.to_lowercase()) {
            emit(
                &mut diagnostics,
                "WDL-SRC-005",
                "case-insensitive source path collision",
                Some(record),
                "",
                false,
            );
        }
        for field in ["tags", "aliases"] {
            if let Some(value) = record.get(field) {
                let mut seen = BTreeSet::new();
                if value.as_sequence().is_none_or(|values| {
                    values.iter().any(|v| {
                        core_text(Some(v), field).is_err()
                            || v.as_str().is_none_or(|s| !seen.insert(s))
                    })
                }) {
                    emit(
                        &mut diagnostics,
                        "WDL-SRC-004",
                        "expected an array of unique nonblank strings",
                        Some(record),
                        field,
                        false,
                    );
                }
            }
        }
        if record.title.trim().is_empty() {
            emit(
                &mut diagnostics,
                "WDL-SRC-003",
                "title must be nonempty",
                Some(record),
                "title",
                false,
            );
        }
        let statuses: &[&str] = match record.kind.as_str() {
            "world" => &["canonical"],
            "event" => &["canonical", "draft", "cancelled", "retconned"],
            "scene" => &["planned", "active", "closed", "abandoned"],
            "conversation" => &["draft", "active", "closed", "retired"],
            "hypothesis" => &["open", "adopted", "rejected"],
            "organization" | "parentage" | "union" | "affiliation" | "legacy" | "tenure"
            | "claim" | "vital-history" | "lore-concept" | "assertion" | "existence-episode"
            | "context-brief" => &["canonical", "draft", "retired"],
            _ => &["canonical", "retired"],
        };
        if record.get("status").is_some()
            && record
                .string("status")
                .is_none_or(|s| !statuses.contains(&s))
        {
            emit(
                &mut diagnostics,
                "WDL-STATUS-001",
                "invalid status for record kind",
                Some(record),
                "status",
                false,
            );
        }
        if let Some(required) = required_capability(record) {
            if !capabilities.contains(required) {
                emit(
                    &mut diagnostics,
                    "NAR-CAPABILITY-001",
                    format!("{} requires {required}", record.kind),
                    Some(record),
                    "kind",
                    false,
                );
            }
        }
        if record.kind != "world" {
            if let Some(value) = record.get("threads") {
                let mut previous = "";
                let valid = value.as_sequence().is_some_and(|values| {
                    values.iter().all(|value| {
                        let id = value.as_str().unwrap_or("");
                        let valid = id > previous && declared_threads.contains(id);
                        previous = id;
                        valid
                    })
                });
                if !valid {
                    emit(
                        &mut diagnostics,
                        "WDL-THREAD-008",
                        "record threads must be sorted unique declared IDs",
                        Some(record),
                        "threads",
                        false,
                    );
                }
            }
        }
        if let Some(value) = record.get("object_affordances") {
            if record.kind != "object"
                || value
                    .as_sequence()
                    .is_none_or(|v| v.iter().any(|v| !v.is_string()))
            {
                emit(
                    &mut diagnostics,
                    "GEN-AFFORDANCE-001",
                    "object_affordances must be an object-only string array",
                    Some(record),
                    "object_affordances",
                    false,
                );
            }
        }
        let refs = collect_refs(world, record);
        for (field, target) in refs {
            if !world.records.contains_key(&target) {
                emit(
                    &mut diagnostics,
                    "WDL-REF-001",
                    format!("unknown referenced record {target}"),
                    Some(record),
                    &field,
                    false,
                );
            }
        }
        for fragment in record.body.split("](story:").skip(1) {
            if let Some((target, _)) = fragment.split_once(')') {
                if valid_record_id(target) && !world.records.contains_key(target) {
                    emit(
                        &mut diagnostics,
                        "WDL-REF-001",
                        format!("unknown story link {target}"),
                        Some(record),
                        "body",
                        false,
                    );
                }
            }
        }
        times(world, record, &mut diagnostics);
        chronology_namespaces(record, &mut diagnostics);
        section_audiences(world, record, &mut diagnostics);
        if let Err(message) = core_record(world, record) {
            let field = message
                .split_once(':')
                .map(|(field, _)| field)
                .unwrap_or("")
                .to_owned();
            emit(
                &mut diagnostics,
                "WDL-CORE-SHAPE",
                message,
                Some(record),
                &field,
                false,
            );
        }
    }
    state_declarations(world, &mut diagnostics);
    diagnostics.extend(crate::lore_state::validate(world));
    state_and_events(world, &mut diagnostics);
    diagnostics.extend(crate::hypotheses::validate(world));
    diagnostics.extend(crate::story::validate(world));
    diagnostics.extend(crate::core_validation::validate(world));
    diagnostics.extend(crate::scene::validate(world));
    diagnostics.extend(crate::spatial::validate(world));
    diagnostics.extend(crate::geometry::validate(world));
    diagnostics.extend(crate::spatial_history::validate(world));
    diagnostics.extend(crate::lore::validate(world));
    diagnostics.extend(crate::description::validate(world));
    diagnostics.extend(crate::assertion::validate(world));
    diagnostics.extend(crate::learned_assertions::validate(world));
    diagnostics.extend(crate::existence::validate(world));
    diagnostics.extend(crate::briefs::validate(world));
    diagnostics.extend(crate::chronology::validate(world));
    diagnostics.extend(crate::generational::validate(world));
    let active: Vec<_> = world
        .records
        .values()
        .filter(|r| r.kind == "scene" && r.status() == "active")
        .collect();
    if active.len() > 1 && owner.get("current_time").is_none() {
        emit(
            &mut diagnostics,
            "WDL-CURSOR-002",
            "multiple active scenes require world.current_time",
            Some(owner),
            "current_time",
            false,
        );
    }
    if let Some(cursor) = owner
        .get("current_time")
        .and_then(|v| StoryTime::from_yaml(v, &world.default_timeline()).ok())
    {
        for scene in active {
            if scene
                .get("time")
                .and_then(|v| v.get("current"))
                .and_then(|v| StoryTime::from_yaml(v, &world.default_timeline()).ok())
                .as_ref()
                != Some(&cursor)
            {
                emit(
                    &mut diagnostics,
                    "WDL-CURSOR-005",
                    "active scene cursor must equal world.current_time",
                    Some(scene),
                    "time.current",
                    false,
                );
            }
        }
    }
    ValidationReport {
        valid: !diagnostics.iter().any(|d| d.severity == "error"),
        complete: false,
        diagnostics,
    }
}
