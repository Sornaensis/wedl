use super::*;
use crate::{
    action_drafts, action_outcomes, edit_drafts,
    model::{Record, World},
    service::Operation,
};
use serde_yaml::Value as Yaml;

const ACTION_SAVE: &str = "/api/action-drafts/save";
const ACTION_APPLY: &str = "/api/action-drafts/apply-outcome";
const EDIT_SAVE: &str = "/api/edit-drafts/save";
const EDIT_APPLY: &str = "/api/edit-drafts/apply";
const REVIEW_BUDGET: usize = 1_000_000;

#[derive(Serialize, PartialEq)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
enum Display {
    Missing,
    Empty,
    Text(String),
    Reference(String),
    Integer(String),
    Number(String),
    Boolean(bool),
    List(Vec<Display>),
    Fields(Vec<NamedValue>),
    Unavailable,
}
impl Display {
    fn complete(&self) -> bool {
        match self {
            Self::Unavailable => false,
            Self::List(values) => values.iter().all(Self::complete),
            Self::Fields(values) => values.iter().all(|field| field.value.complete()),
            _ => true,
        }
    }
}
#[derive(Serialize, PartialEq)]
struct NamedValue {
    label: String,
    value: Display,
}
#[derive(Serialize)]
struct FieldChange {
    label: String,
    operation: &'static str,
    before: Display,
    after: Display,
}
#[derive(Serialize)]
pub(super) struct Change {
    record: String,
    operation: &'static str,
    fields: Vec<FieldChange>,
    complete: bool,
}

fn words(value: &str) -> String {
    let label = value.replace(['_', '-'], " ");
    let mut chars = label.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}
fn machine_key(key: &str) -> bool {
    matches!(
        key,
        "id" | "schema"
            | "tick"
            | "order"
            | "path"
            | "source_path"
            | "sourcePath"
            | "sha256"
            | "revision"
            | "fingerprint"
            | "blob_oid"
            | "blobOid"
    ) || key.ends_with("_id")
        || key.ends_with("Id")
        || key.ends_with("Sha256")
}
fn display(world: &World, value: Option<&Yaml>, depth: usize) -> Display {
    if depth > 8 {
        return Display::Unavailable;
    }
    match value {
        None => Display::Missing,
        Some(Yaml::Null) => Display::Empty,
        Some(Yaml::Bool(value)) => Display::Boolean(*value),
        Some(Yaml::Number(value)) if value.is_i64() || value.is_u64() => {
            Display::Integer(value.to_string())
        }
        Some(Yaml::Number(value)) => Display::Number(value.to_string()),
        Some(Yaml::String(value)) => {
            if let Some(record) = world.records.get(value) {
                return Display::Reference(display_text(world, &record.title));
            }
            let clean = display_text(world, value);
            if clean == *value {
                Display::Text(clean)
            } else {
                Display::Unavailable
            }
        }
        Some(Yaml::Sequence(values)) if values.len() <= 1024 => Display::List(
            values
                .iter()
                .map(|value| display(world, Some(value), depth + 1))
                .collect(),
        ),
        Some(value @ Yaml::Mapping(values)) if values.len() <= 256 => {
            if value.get("tick").is_some()
                && values
                    .keys()
                    .all(|key| matches!(key.as_str(), Some("timeline" | "tick" | "order")))
            {
                return crate::model::StoryTime::from_yaml(value, &world.default_timeline())
                    .ok()
                    .and_then(|at| super::frame::named_moment(world, &at))
                    .map_or(Display::Unavailable, Display::Reference);
            }
            let reference = match values.len() {
                1 => value["entity"].as_str(),
                2 if value["kind"].as_str() == Some("record") => value["id"].as_str(),
                _ => None,
            };
            if let Some(id) = reference {
                return super::frame::record_label(world, id)
                    .map(|name| Display::Reference(display_text(world, name)))
                    .unwrap_or(Display::Unavailable);
            }
            let mut result = Vec::new();
            for (key, value) in values {
                let Some(key) = key.as_str() else {
                    return Display::Unavailable;
                };
                if key.is_empty() || key.len() > 120 || display_text(world, key) != key {
                    return Display::Unavailable;
                }
                let value = if machine_key(key) {
                    value
                        .as_str()
                        .and_then(|id| super::frame::record_label(world, id))
                        .map(|name| Display::Reference(display_text(world, name)))
                        .unwrap_or(Display::Unavailable)
                } else {
                    display(world, Some(value), depth + 1)
                };
                result.push(NamedValue {
                    label: words(key),
                    value,
                });
            }
            Display::Fields(result)
        }
        _ => Display::Unavailable,
    }
}

fn field_changes(
    world: &World,
    before: Option<&Record>,
    after: &Record,
) -> Result<Vec<FieldChange>> {
    let after_fields = after.frontmatter.as_mapping().ok_or_else(unavailable)?;
    let before_fields = before.and_then(|record| record.frontmatter.as_mapping());
    let keys = after_fields
        .keys()
        .chain(before_fields.into_iter().flat_map(|fields| fields.keys()))
        .filter_map(Yaml::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    let mut changes = Vec::new();
    for key in keys {
        if matches!(key, "id" | "schema" | "kind") {
            continue;
        }
        let old = before_fields.and_then(|fields| fields.get(key));
        let new = after_fields.get(key);
        if old == new {
            continue;
        }
        let operation = match (old, new) {
            (_, None | Some(Yaml::Null)) => "Clear",
            (None, _) => "Set",
            (Some(Yaml::Sequence(old)), Some(Yaml::Sequence(new)))
                if old.len() == new.len()
                    && old.iter().all(|item| {
                        old.iter().filter(|value| *value == item).count()
                            == new.iter().filter(|value| *value == item).count()
                    }) =>
            {
                "Reorder"
            }
            _ => "Replace",
        };
        let label = if machine_key(key) || display_text(world, key) != key {
            "Private field".into()
        } else {
            words(key)
        };
        let convert = |value: Option<&Yaml>| {
            if machine_key(key) && value.is_some() {
                Display::Unavailable
            } else {
                display(world, value, 0)
            }
        };
        changes.push(FieldChange {
            label,
            operation,
            before: convert(old),
            after: convert(new),
        });
    }
    if before.map(|record| record.body.as_str()) != Some(after.body.as_str()) {
        let body = |text: &str| {
            if display_text(world, text) == text {
                Display::Text(text.into())
            } else {
                Display::Unavailable
            }
        };
        changes.push(FieldChange {
            label: "Prose".into(),
            operation: if after.body.is_empty() {
                "Clear"
            } else {
                "Replace"
            },
            before: before
                .map(|record| body(&record.body))
                .unwrap_or(Display::Missing),
            after: body(&after.body),
        });
    }
    Ok(changes)
}

pub(super) fn diffs(world: &World, report: &Value) -> Result<Vec<Change>> {
    let Some(files) = report["files"].as_array() else {
        return Ok(Vec::new());
    };
    files
        .iter()
        .map(|file| {
            let path = exact(file, "sourcePath")?;
            let before = file["beforeSource"]
                .as_str()
                .map(|raw| crate::source::parse_record(raw.as_bytes(), path))
                .transpose()?;
            let after = crate::source::parse_record(exact(file, "afterSource")?.as_bytes(), path)?;
            let fields = field_changes(world, before.as_ref(), &after)?;
            let complete = fields.iter().all(|field| {
                field.before.complete() && field.after.complete() && field.before != field.after
            }) && (!fields.is_empty() || file["changed"] == false);
            let operation = if before.is_none() {
                "Create record"
            } else if file["changed"] == false {
                "No source change"
            } else {
                "Update record"
            };
            Ok(Change {
                record: display_text(world, &after.title),
                operation,
                fields,
                complete,
            })
        })
        .collect()
}

#[derive(Serialize)]
struct SemanticCheck {
    label: String,
    policy: &'static str,
    status: &'static str,
    operands: Vec<NamedValue>,
}

fn predicate_view(
    world: &World,
    changed: &BTreeMap<String, Record>,
    predicate: Value,
) -> Option<(String, Vec<NamedValue>)> {
    use crate::consequence_semantics::Predicate;
    let record = |id: &str| changed.get(id).or_else(|| world.records.get(id));
    let reference = |id: &str| {
        record(id)
            .map(|record| Display::Reference(display_text(world, &record.title)))
            .unwrap_or(Display::Unavailable)
    };
    let name = |label: &str, value| NamedValue {
        label: label.into(),
        value,
    };
    let authored = |value: &str| {
        let clean = display_text(world, value);
        if clean == value {
            Display::Text(clean)
        } else {
            Display::Unavailable
        }
    };
    let state_field = |key: &str| {
        if machine_key(key) {
            Display::Unavailable
        } else {
            authored(key)
        }
    };
    let (label, operands) = match serde_json::from_value(predicate).ok()? {
        Predicate::StateEquals {
            target_id,
            key,
            expected_value_raw,
        } => {
            let expected =
                Yaml::deserialize(&mut serde_json::Deserializer::from_str(&expected_value_raw))
                    .ok()?;
            let expected = match display(world, Some(&expected), 0) {
                Display::Integer(_) => Display::Integer(expected_value_raw.trim().into()),
                Display::Number(_) => Display::Number(expected_value_raw.trim().into()),
                other => other,
            };
            (
                "State equals",
                vec![
                    name("Record", reference(&target_id)),
                    name("Field", state_field(&key)),
                    name("Expected value", expected),
                ],
            )
        }
        Predicate::StateAbsent { target_id, key } => (
            "State is absent",
            vec![
                name("Record", reference(&target_id)),
                name("Field", state_field(&key)),
            ],
        ),
        Predicate::KnowledgeState {
            knowledge_id,
            state,
        } => (
            "Knowledge state matches",
            vec![
                name("Knowledge", reference(&knowledge_id)),
                name("Expected state", authored(&state)),
            ],
        ),
        Predicate::RelationshipMatches {
            relationship_id,
            values,
        } => {
            let mut operands = vec![name("Relationship", reference(&relationship_id))];
            if let Some(status) = values.status {
                operands.push(name("Expected status", authored(&status)));
            }
            if let Some(metrics) = values.metrics {
                operands.push(name(
                    "Expected metrics",
                    Display::Fields(
                        metrics
                            .into_iter()
                            .map(|(key, value)| match state_field(&key) {
                                Display::Text(key) => name(&key, Display::Number(value)),
                                _ => name("Metric", Display::Unavailable),
                            })
                            .collect(),
                    ),
                ));
            }
            if let Some(facets) = values.facets {
                operands.push(name(
                    "Expected facets",
                    Display::List(facets.iter().map(|facet| authored(facet)).collect()),
                ));
            }
            ("Relationship matches", operands)
        }
        Predicate::StoryPointState {
            story_point_id,
            state,
        } => (
            "Story point state matches",
            vec![
                name("Story point", reference(&story_point_id)),
                name("Expected state", authored(&state)),
            ],
        ),
        Predicate::OutcomeLinked {
            event_id,
            target_id,
        } => (
            "Event links to outcome",
            vec![
                name("Event", reference(&event_id)),
                name("Outcome", reference(&target_id)),
            ],
        ),
        Predicate::TransitionCaused {
            record_id,
            transition_id,
            event_id,
        } => {
            let transition = record(&record_id)
                .and_then(|record| {
                    let rows = if record.kind == "story-point" {
                        record.get("lifecycle")?.get("transitions")?.as_sequence()?
                    } else {
                        record.list("transitions")?
                    };
                    let mut positions = rows
                        .iter()
                        .enumerate()
                        .filter(|(_, row)| row["id"].as_str() == Some(&transition_id));
                    let (ordinal, _) = positions.next()?;
                    positions
                        .next()
                        .is_none()
                        .then(|| Display::Text(format!("Transition {}", ordinal + 1)))
                })
                .unwrap_or(Display::Unavailable);
            (
                "Event caused transition",
                vec![
                    name("Record", reference(&record_id)),
                    name("Transition", transition),
                    name("Event", reference(&event_id)),
                ],
            )
        }
    };
    Some((label.into(), operands))
}

fn semantic_checks(
    world: &World,
    changed: &BTreeMap<String, Record>,
    analysis: &Value,
) -> (Vec<SemanticCheck>, bool) {
    let Some(items) = analysis["expectations"].as_array() else {
        return (
            vec![SemanticCheck {
                label: "Semantic expectations".into(),
                policy: "Check",
                status: "Unavailable",
                operands: vec![],
            }],
            false,
        );
    };
    let mut complete = analysis["outcome"] == "ok";
    let rows = items
        .iter()
        .map(|item| {
            let predicate = predicate_view(world, changed, item["predicate"].clone());
            complete &= predicate
                .as_ref()
                .is_some_and(|(_, operands)| operands.iter().all(|item| item.value.complete()));
            let (label, operands) =
                predicate.unwrap_or_else(|| ("Unsupported semantic predicate".into(), vec![]));
            let status = match item["outcome"].as_str() {
                Some("pass") => "Passed",
                Some("fail") => "Failed",
                Some("unsupported") => "Unsupported",
                _ => "Unavailable",
            };
            let policy = match item["policy"].as_str() {
                Some("required") => "Required",
                Some("advisory") => "Advisory",
                _ => "Unavailable",
            };
            complete &= matches!(status, "Passed" | "Failed") && policy != "Unavailable";
            SemanticCheck {
                label,
                policy,
                status,
                operands,
            }
        })
        .collect();
    (rows, complete)
}

pub(super) fn checks(world: &World, report: &Value) -> Result<(Vec<Value>, bool)> {
    let validation = match report["validation"]["valid"].as_bool() {
        Some(true) => "Passed",
        Some(false) => "Failed",
        None => "Unavailable",
    };
    let mut rows =
        vec![json!({"label":"Native source validation","policy":"Required","status":validation})];
    let analysis = &report["semanticAnalysis"];
    if analysis.is_null() {
        return Ok((rows, true));
    }
    let mut changed = BTreeMap::new();
    if let Some(files) = report["files"].as_array() {
        for file in files {
            let record = crate::source::parse_record(
                exact(file, "afterSource")?.as_bytes(),
                exact(file, "sourcePath")?,
            )?;
            changed.insert(record.id.clone(), record);
        }
    }
    let (semantic, complete) = semantic_checks(world, &changed, analysis);
    rows.extend(semantic.into_iter().map(|check| json!(check)));
    Ok((rows, complete))
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    id: String,
    hash: String,
}

fn saved_token(
    world: &World,
    handles: &mut Handles,
    kind: &'static str,
    id: &str,
    hash: &str,
) -> Result<String> {
    handles.issue(
        kind,
        Pinned::bind(
            world,
            serde_json::to_string(&Saved {
                id: id.into(),
                hash: hash.into(),
            })?,
        )?,
    )
}
fn saved(world: &World, handles: &Handles, kind: &'static str, token: &str) -> Result<Saved> {
    let raw = Pinned::read(world, &handles.resolve(kind, token)?)?;
    serde_json::from_str(&raw).map_err(|_| unavailable())
}
fn id(prefix: &str) -> Result<String> {
    let mut bytes = [0; 26];
    getrandom::fill(&mut bytes).map_err(|_| unavailable())?;
    let suffix: String = bytes
        .iter()
        .map(|byte| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ"[usize::from(byte & 31)] as char)
        .collect();
    Ok(format!("{prefix}_{suffix}"))
}
fn phase(value: &str) -> &'static str {
    match value {
        "draft" => "Draft",
        "context-needed" => "Context needed",
        "resolution-needed" => "Resolution needed",
        "proposed" => "Proposed",
        "stale" => "Source changed",
        "cancelled" => "Cancelled",
        "applied" => "Applied",
        _ => "Unknown",
    }
}
fn records(
    world: &World,
    handles: &mut Handles,
    kind: &'static str,
    filter: impl Fn(&Record) -> bool,
) -> Result<Vec<Choice>> {
    world
        .records
        .values()
        .filter(|record| filter(record))
        .map(|record| {
            choice(
                handles,
                kind,
                &display_text(world, &record.title),
                Pinned::bind(world, record.id.clone())?,
            )
        })
        .collect()
}
fn lists(service: &Service, action: bool, handles: &mut Handles) -> Result<Value> {
    let world = service.world();
    let result = service.execute(if action {
        Operation::ActionDraftList
    } else {
        Operation::EditDraftList
    })?;
    let kind = if action { "action" } else { "edit" };
    let mut drafts = Vec::new();
    for row in result["result"]["items"]
        .as_array()
        .ok_or_else(unavailable)?
    {
        let label = format!(
            "{} · {}",
            display_text(world, exact(row, "title")?),
            phase(
                row["effectivePhase"]
                    .as_str()
                    .or_else(|| row["phase"].as_str())
                    .unwrap_or("")
            )
        );
        drafts.push(Choice {
            label,
            token: saved_token(
                world,
                handles,
                kind,
                exact(row, "id")?,
                exact(row, "sourceSha256")?,
            )?,
        });
    }
    let targets = records(world, handles, "edit-target", |record| {
        record.status() == "canonical"
    })?;
    let actors = if action {
        records(world, handles, "action-actor", |record| {
            record.kind == "character"
        })?
    } else {
        Vec::new()
    };
    Ok(json!({"drafts":drafts,"targets":targets,"actors":actors}))
}

fn native_detail(world: &World, kind: &str, selected: &Saved) -> Result<Value> {
    if kind == "action" {
        crate::action_store::detail(world, &selected.id, Some(&selected.hash))
    } else {
        edit_drafts::detail(world, &selected.id, Some(&selected.hash))
    }
}
fn action_open(world: &World, detail: &Value, handles: &mut Handles) -> Result<Value> {
    let draft: action_drafts::Draft = serde_json::from_value(detail["draft"].clone())?;
    let active = action_drafts::active_proposal_ids(&draft);
    let mut proposals = Vec::new();
    for proposal in &draft.proposals {
        let value = serde_json::to_string(
            &json!({"id":draft.id,"hash":detail["sourceSha256"],"proposal":proposal.id}),
        )?;
        proposals.push(json!({"label":display_text(world,&proposal.summary),"active":active.contains(&proposal.id),"token":handles.issue("proposal",Pinned::bind(world,value)?)?}));
    }
    let references = |refs: &[action_drafts::Reference]| {
        refs.iter()
            .filter_map(|reference| match reference {
                action_drafts::Reference::Record { id } => super::frame::record_label(world, id),
                action_drafts::Reference::Section { owner_id, .. } => {
                    super::frame::record_label(world, owner_id)
                }
            })
            .map(|label| display_text(world, label))
            .collect::<Vec<_>>()
    };
    let mut fields = Vec::new();
    for target in &draft.target_refs {
        let action_drafts::Reference::Record { id } = target else {
            continue;
        };
        let Some(record) = world.records.get(id) else {
            continue;
        };
        let definitions = world
            .world_record()?
            .get("state_keys")
            .and_then(|keys| keys.get(&record.kind))
            .and_then(Yaml::as_mapping);
        for key in definitions
            .into_iter()
            .flat_map(|values| values.keys())
            .filter_map(Yaml::as_str)
        {
            let Ok(definition) = crate::state_schema::field_definition(world, &record.kind, key)
            else {
                continue;
            };
            use crate::state_schema::StateType::*;
            let field_kind = match definition.value_type {
                String => "text",
                Boolean => "boolean",
                Integer => "integer",
                Number => "number",
                _ => continue,
            };
            if definition.profile.is_some() || display_text(world, key) != key {
                continue;
            }
            let label = format!("{} · {}", display_text(world, &record.title), words(key));
            let token = handles.issue(
                "expectation",
                Pinned::bind(
                    world,
                    serde_json::to_string(&json!({"id":record.id,"key":key,"kind":field_kind}))?,
                )?,
            )?;
            fields.push(json!({"label":label,"token":token,"valueType":field_kind}));
        }
    }
    let editable = draft.phase == "draft"
        && detail["sourceChanged"] == false
        && world.revision.is_none()
        && display_text(world, &draft.title) == draft.title
        && display_text(world, &draft.intent) == draft.intent;
    let questions: Vec<_> = draft.questions.iter().map(|question|json!({"text":display_text(world,&question.text),"required":question.required,"answered":question.answer.is_some()})).collect();
    let native = handles.issue("native-file", exact(detail, "source")?.into())?;
    Ok(
        json!({"title":display_text(world,&draft.title),"intent":display_text(world,&draft.intent),"phase":phase(exact(detail,"effectivePhase")?),
        "editable":editable,"current":detail["sourceChanged"] == false,"participants":references(&draft.actor_refs),"targets":references(&draft.target_refs),
        "questions":questions,"proposals":proposals,"fields":fields,"native":native,
        "canPrepareReadiness":matches!(draft.phase.as_str(),"draft"|"context-needed"|"resolution-needed"),
        "canPrepareProposal":matches!(draft.phase.as_str(),"resolution-needed"|"stale")}),
    )
}
fn edit_open(world: &World, detail: &Value, handles: &mut Handles) -> Result<Value> {
    let draft: edit_drafts::Draft = serde_json::from_value(detail["draft"].clone())?;
    let record = world
        .records
        .get(&draft.target_id)
        .ok_or_else(unavailable)?;
    let title = display_text(world, &record.title);
    let body = if draft.body_mode == edit_drafts::BodyMode::Replace {
        &draft.body_markdown
    } else {
        &record.body
    };
    let editable = draft.phase == "draft"
        && world.revision.is_none()
        && draft.base_fingerprint == world.fingerprint
        && display_text(world, body) == *body;
    let native = handles.issue("native-file", exact(detail, "source")?.into())?;
    Ok(
        json!({"title":display_text(world,&draft.title),"record":title,"phase":phase(&draft.phase),"editable":editable,
        "current":draft.base_fingerprint==world.fingerprint,"historical":world.revision.is_some(),"body":display_text(world,body),"native":native}),
    )
}

#[derive(Deserialize, Serialize)]
struct ProposalPin {
    id: String,
    hash: String,
    proposal: String,
}

fn review(
    world: &World,
    query: &BTreeMap<String, String>,
    handles: &mut Handles,
    action: bool,
    live: bool,
) -> Result<Value> {
    closed(query, &["draft", "proposal"])?;
    let selected = saved(
        world,
        handles,
        if action { "action" } else { "edit" },
        field(query, "draft")?,
    )?;
    let (report, route, raw) = if action {
        let pin: ProposalPin = serde_json::from_str(&Pinned::read(
            world,
            &handles.resolve("proposal", field(query, "proposal")?)?,
        )?)?;
        if pin.id != selected.id || pin.hash != selected.hash {
            return Err(unavailable());
        }
        let request = action_outcomes::ReviewRequest {
            expected_fingerprint: world.fingerprint.clone(),
            action_id: selected.id,
            expected_sha256: selected.hash,
            proposal_id: pin.proposal,
            max_characters: REVIEW_BUDGET,
        };
        let report = action_outcomes::review(world, &request)?;
        let apply = action_outcomes::ApplyRequest {
            expected_fingerprint: request.expected_fingerprint,
            action_id: request.action_id,
            expected_sha256: request.expected_sha256,
            proposal_id: request.proposal_id,
            max_characters: request.max_characters,
            review_sha256: exact(&report, "reviewSha256")?.into(),
            approved: false,
        };
        (report, ACTION_APPLY, serde_json::to_string(&apply)?)
    } else {
        if query.contains_key("proposal") {
            return Err(invalid());
        }
        let request = edit_drafts::ReviewRequest {
            expected_fingerprint: world.fingerprint.clone(),
            edit_id: selected.id,
            expected_sha256: selected.hash,
            max_characters: REVIEW_BUDGET,
        };
        let report = edit_drafts::review(world, &request)?;
        let apply = edit_drafts::ApplyRequest {
            expected_fingerprint: request.expected_fingerprint,
            edit_id: request.edit_id,
            expected_sha256: request.expected_sha256,
            max_characters: request.max_characters,
            review_sha256: exact(&report, "reviewSha256")?.into(),
            approved: false,
        };
        (report, EDIT_APPLY, serde_json::to_string(&apply)?)
    };
    let changes = diffs(world, &report["consequences"])?;
    let (validation, semantic_complete) = checks(
        world,
        if report["consequences"].is_null() {
            &report
        } else {
            &report["consequences"]
        },
    )?;
    let complete = !report["consequences"].is_null()
        && changes.iter().all(|change| change.complete)
        && semantic_complete;
    let can_apply = report["canApply"] == true && complete && live;
    let approval = if can_apply {
        Some(handles.issue(
            "candidate-approval",
            Pinned::bind(
                world,
                serde_json::to_string(&json!({"route":route,"raw":raw}))?,
            )?,
        )?)
    } else {
        None
    };
    let evidence = report["evidence"].as_array().map(|rows| {
        rows.iter()
            .map(|row| {
                let title = row["title"]
                    .as_str()
                    .or_else(|| row["ownerTitle"].as_str())
                    .map(|name| display_text(world, name));
                json!({"label":title.unwrap_or_else(||"Verified original source".into())})
            })
            .collect::<Vec<_>>()
    });
    let file = handles.issue("native-file", serde_json::to_string(&report)?)?;
    Ok(
        json!({"changes":changes,"validation":validation,"evidence":evidence,"complete":complete,"approval":approval,"canApply":can_apply,
        "canonicalNoOp":report["consequences"]["canonicalNoOp"].as_bool(),"file":file}),
    )
}

fn approve(
    world: &World,
    query: &BTreeMap<String, String>,
    handles: &mut Handles,
    live: bool,
) -> Result<Value> {
    closed(query, &["candidate", "approved"])?;
    if !live || field(query, "approved")? != "1" {
        return Err(invalid());
    }
    let material: Value = serde_json::from_str(&Pinned::read(
        world,
        &handles.resolve("candidate-approval", field(query, "candidate")?)?,
    )?)?;
    let route = match exact(&material, "route")? {
        ACTION_APPLY => ACTION_APPLY,
        EDIT_APPLY => EDIT_APPLY,
        _ => return Err(unavailable()),
    };
    let mut request: Value = serde_json::from_str(exact(&material, "raw")?)?;
    request["approved"] = true.into();
    let raw = serde_json::to_string(&request)?;
    if route == ACTION_APPLY {
        let request = action_outcomes::decode_apply(&raw)?;
        crate::action_store::detail(world, &request.action_id, Some(&request.expected_sha256))?;
    } else {
        let request = edit_drafts::decode_apply(&raw)?;
        edit_drafts::detail(world, &request.edit_id, Some(&request.expected_sha256))?;
    }
    Ok(json!({"route":route,"request":handles.request(route,raw)?}))
}

fn workbench_record<'a>(
    world: &'a World,
    handles: &Handles,
    kind: &'static str,
    token: &str,
) -> Result<&'a Record> {
    let id = Pinned::read(world, &handles.resolve(kind, token)?)?;
    world.records.get(&id).ok_or_else(unavailable)
}

fn action_save(
    service: &Service,
    query: &BTreeMap<String, String>,
    handles: &mut Handles,
) -> Result<Value> {
    closed(
        query,
        &[
            "draft", "title", "intent", "actor", "target", "frame", "raw",
        ],
    )?;
    let world = service.world();
    let (request, raw) = if let Some(raw) = query.get("raw") {
        (action_drafts::decode_save(raw)?, raw.clone())
    } else {
        let intent = field(query, "intent")?.to_owned();
        let selected = query
            .get("draft")
            .filter(|value| !value.is_empty())
            .map(|token| saved(world, handles, "action", token))
            .transpose()?;
        let draft = if let Some(selected) = &selected {
            let detail = native_detail(world, "action", selected)?;
            let mut draft: action_drafts::Draft = serde_json::from_value(detail["draft"].clone())?;
            if draft.phase != "draft" || display_text(world, &draft.intent) != draft.intent {
                return Err(Error::Unsupported(
                    "Use the exact file workflow for this draft".into(),
                ));
            }
            draft.intent = intent;
            draft.title = field(query, "title")?.into();
            draft
        } else {
            let frame = crate::writing_frames::decode(
                &handles.resolve("writing-frame", field(query, "frame")?)?,
            )?;
            service.execute(Operation::WritingResolve {
                frame: frame.clone(),
            })?;
            let actor = workbench_record(world, handles, "action-actor", field(query, "actor")?)?;
            let target = query
                .get("target")
                .filter(|token| !token.is_empty())
                .map(|token| workbench_record(world, handles, "edit-target", token))
                .transpose()?;
            action_drafts::Draft {
                id: id("action")?,
                title: field(query, "title")?.into(),
                world_id: world.world_record()?.id.clone(),
                base_fingerprint: world.fingerprint.clone(),
                horizon: frame.at,
                intent,
                actor_refs: vec![action_drafts::Reference::Record {
                    id: actor.id.clone(),
                }],
                target_refs: target
                    .map(|record| action_drafts::Reference::Record {
                        id: record.id.clone(),
                    })
                    .into_iter()
                    .collect(),
                hypothesis_refs: Vec::new(),
                evidence: Vec::new(),
                questions: Vec::new(),
                proposals: Vec::new(),
                phase: "draft".into(),
                body_markdown: String::new(),
                applied_receipt: None,
            }
        };
        let request = action_drafts::SaveRequest {
            expected_fingerprint: world.fingerprint.clone(),
            expected_sha256: selected.map(|value| value.hash),
            draft,
            review: None,
        };
        let raw = serde_json::to_string(&request)?;
        (request, raw)
    };
    crate::action_store::preview(world, &request)?;
    Ok(
        json!({"request":handles.request(ACTION_SAVE,raw)?,"label":display_text(world,&request.draft.title)}),
    )
}

fn edit_save(
    world: &World,
    query: &BTreeMap<String, String>,
    handles: &mut Handles,
) -> Result<Value> {
    closed(query, &["draft", "target", "operation", "text", "raw"])?;
    let (request, raw) = if let Some(raw) = query.get("raw") {
        (edit_drafts::decode_save(raw)?, raw.clone())
    } else {
        let selected = query
            .get("draft")
            .filter(|value| !value.is_empty())
            .map(|token| saved(world, handles, "edit", token))
            .transpose()?;
        let mut draft = if let Some(selected) = &selected {
            serde_json::from_value::<edit_drafts::Draft>(
                native_detail(world, "edit", selected)?["draft"].clone(),
            )?
        } else {
            let record = workbench_record(world, handles, "edit-target", field(query, "target")?)?;
            edit_drafts::Draft {
                id: id("edit")?,
                title: format!("Edit {}", record.title),
                world_id: world.world_record()?.id.clone(),
                target_id: record.id.clone(),
                base_fingerprint: world.fingerprint.clone(),
                base_source_sha256: action_drafts::sha256(&record.raw_bytes),
                base_source_path: record.source_path.clone(),
                patch: serde_yaml::to_string(&serde_json::json!({"kind":record.kind}))?,
                body_mode: edit_drafts::BodyMode::Preserve,
                body_markdown: String::new(),
                phase: "draft".into(),
                applied_receipt: None,
            }
        };
        if draft.phase != "draft" || draft.base_fingerprint != world.fingerprint {
            return Err(Error::Conflict("Retained edit source changed".into()));
        }
        let text = query.get("text").map(String::as_str).unwrap_or("");
        match field(query, "operation")? {
            "omit" => {}
            "replace-prose" | "clear-prose" => {
                let previous = if draft.body_mode == edit_drafts::BodyMode::Replace {
                    &draft.body_markdown
                } else {
                    &world
                        .records
                        .get(&draft.target_id)
                        .ok_or_else(unavailable)?
                        .body
                };
                let line_endings = |raw: &str| {
                    let crlf = raw.matches("\r\n").count();
                    let lf = raw.matches('\n').count();
                    (crlf, lf)
                };
                let (crlf, lf) = line_endings(previous);
                let clear = field(query, "operation")? == "clear-prose";
                if !clear
                    && ((crlf != 0 && crlf != lf) || previous.replace("\r\n", "").contains('\r'))
                {
                    return Err(Error::Unsupported(
                        "Use exact source import for mixed line endings".into(),
                    ));
                }
                draft.body_markdown = if clear {
                    String::new()
                } else if crlf > 0 {
                    text.replace("\r\n", "\n").replace('\n', "\r\n")
                } else {
                    text.into()
                };
                draft.body_mode = edit_drafts::BodyMode::Replace;
            }
            "rename" | "reorder-aliases" => {
                let mut patch: Yaml = serde_yaml::from_str(&draft.patch)?;
                let mapping = patch.as_mapping_mut().ok_or_else(invalid)?;
                if mapping
                    .keys()
                    .any(|key| !matches!(key.as_str(), Some("kind" | "title" | "aliases")))
                {
                    return Err(Error::Unsupported(
                        "The retained patch requires exact native editing".into(),
                    ));
                }
                if field(query, "operation")? == "rename" {
                    mapping.insert("title".into(), field(query, "text")?.into());
                } else {
                    mapping.insert(
                        "aliases".into(),
                        Yaml::Sequence(
                            text.lines().map(|line| Yaml::String(line.into())).collect(),
                        ),
                    );
                }
                draft.patch = serde_yaml::to_string(&patch)?;
            }
            _ => return Err(invalid()),
        }
        let request = edit_drafts::SaveRequest {
            expected_fingerprint: world.fingerprint.clone(),
            expected_sha256: selected.map(|value| value.hash),
            draft,
        };
        let raw = serde_json::to_string(&request)?;
        (request, raw)
    };
    edit_drafts::preview(world, &request)?;
    Ok(
        json!({"request":handles.request(EDIT_SAVE,raw)?,"label":display_text(world,&request.draft.title)}),
    )
}

fn expectation(
    world: &World,
    query: &BTreeMap<String, String>,
    handles: &mut Handles,
) -> Result<Value> {
    closed(query, &["draft", "field", "operation", "value", "policy"])?;
    let selected = saved(world, handles, "action", field(query, "draft")?)?;
    let detail = native_detail(world, "action", &selected)?;
    let draft: action_drafts::Draft = serde_json::from_value(detail["draft"].clone())?;
    let field_value: Value = serde_json::from_str(&Pinned::read(
        world,
        &handles.resolve("expectation", field(query, "field")?)?,
    )?)?;
    let target = exact(&field_value, "id")?;
    if !draft
        .target_refs
        .iter()
        .any(|reference| matches!(reference,action_drafts::Reference::Record{id} if id==target))
    {
        return Err(unavailable());
    }
    let key = exact(&field_value, "key")?.to_owned();
    use crate::consequence_semantics::{Analysis, CheckRequest, Expectation, Policy, Predicate};
    let predicate = match field(query, "operation")? {
        "absent" => Predicate::StateAbsent {
            target_id: target.into(),
            key,
        },
        "equals" => {
            let raw = query.get("value").map(String::as_str).unwrap_or("");
            let expected_value_raw = match exact(&field_value, "kind")? {
                "text" => serde_json::to_string(raw)?,
                "boolean" if matches!(raw, "true" | "false") => raw.into(),
                "integer" | "number" if serde_json::from_str::<serde_json::Number>(raw).is_ok() => {
                    raw.into()
                }
                _ => return Err(invalid()),
            };
            Predicate::StateEquals {
                target_id: target.into(),
                key,
                expected_value_raw,
            }
        }
        _ => return Err(invalid()),
    };
    let policy = match field(query, "policy")? {
        "required" => Policy::Required,
        "advisory" => Policy::Advisory,
        _ => return Err(invalid()),
    };
    let native = CheckRequest {
        expected_fingerprint: world.fingerprint.clone(),
        analysis: Analysis {
            at: serde_json::from_value(serde_json::to_value(draft.horizon)?)?,
            policy,
            expectations: vec![Expectation {
                id: "selected-field".into(),
                predicate,
            }],
        },
        max_characters: 100_000,
    };
    let result = crate::consequence_semantics::check_request(world, &native)?;
    let file = handles.issue("native-file", serde_json::to_string_pretty(&native)?)?;
    let bound = Pinned::bind(
        world,
        serde_json::to_string(
            &json!({"draft":selected.id,"hash":selected.hash,"analysis":native.analysis}),
        )?,
    )?;
    let analysis = handles.issue("action-analysis", bound)?;
    Ok(
        json!({"validation":checks(world,&json!({"validation":{"valid":true},"semanticAnalysis":result}))?.0,"file":file,"analysis":analysis}),
    )
}

fn prepare_context(
    service: &Service,
    query: &BTreeMap<String, String>,
    handles: &mut Handles,
    live: bool,
) -> Result<Value> {
    closed(
        query,
        &[
            "draft",
            "frame",
            "stage",
            "plan",
            "analysis",
            "summary",
            "supersedes",
        ],
    )?;
    let world = service.world();
    let saved = saved(world, handles, "action", field(query, "draft")?)?;
    let detail = native_detail(world, "action", &saved)?;
    let mut draft: action_drafts::Draft = serde_json::from_value(detail["draft"].clone())?;
    let frame =
        crate::writing_frames::decode(&handles.resolve("writing-frame", field(query, "frame")?)?)?;
    let stage = field(query, "stage")?;
    let mut changes = Vec::new();
    if stage == "proposal" {
        if !matches!(draft.phase.as_str(), "resolution-needed" | "stale") {
            return Err(invalid());
        }
        let raw = handles.resolve("native-file", field(query, "plan")?)?;
        let mut plan = crate::consequences::decode(&raw)?;
        if let Some(token) = query.get("analysis").filter(|value| !value.is_empty()) {
            if plan.analysis.is_some() {
                return Err(Error::Unsupported(
                    "Preserve the Plan's existing exact expectations".into(),
                ));
            }
            let prepared: Value = serde_json::from_str(&Pinned::read(
                world,
                &handles.resolve("action-analysis", token)?,
            )?)?;
            if prepared["draft"] != saved.id || prepared["hash"] != saved.hash {
                return Err(unavailable());
            }
            plan.analysis = Some(serde_json::from_value(prepared["analysis"].clone())?);
        }
        let proposed = crate::consequences::preview(world, &plan, REVIEW_BUDGET)?;
        changes = diffs(world, &proposed)?;
        if changes.iter().any(|item| !item.complete) {
            return Err(Error::Unsupported(
                "The exact Plan needs native review".into(),
            ));
        }
        let supersedes = query
            .get("supersedes")
            .filter(|value| !value.is_empty())
            .map(|token| -> Result<_> {
                let pin: ProposalPin = serde_json::from_str(&Pinned::read(
                    world,
                    &handles.resolve("proposal", token)?,
                )?)?;
                if pin.id != saved.id
                    || pin.hash != saved.hash
                    || !action_drafts::active_proposal_ids(&draft).contains(&pin.proposal)
                {
                    return Err(unavailable());
                }
                Ok(pin.proposal)
            })
            .transpose()?;
        let question_answers = draft
            .questions
            .iter()
            .filter_map(|question| {
                question
                    .answer
                    .clone()
                    .map(|answer| action_drafts::QuestionAnswer {
                        question_id: question.id.clone(),
                        answer,
                    })
            })
            .collect();
        draft.proposals.push(action_drafts::Proposal {
            id: id("proposal")?,
            summary: field(query, "summary")?.into(),
            context_fingerprint: world.fingerprint.clone(),
            evidence: Vec::new(),
            question_answers,
            resolver: "human".into(),
            consequences: plan,
            supersedes,
        });
        draft.phase = "proposed".into();
    } else if stage == "readiness"
        && matches!(
            draft.phase.as_str(),
            "draft" | "context-needed" | "resolution-needed"
        )
    {
        draft.phase = "resolution-needed".into();
    } else {
        return Err(invalid());
    }
    let native = action_drafts::ReviewRequest {
        draft: draft.clone(),
        frame: frame.clone(),
        briefs: Vec::new(),
        families: Default::default(),
        max_characters: 100_000,
    };
    let review = action_drafts::review(world, &native)?;
    let request = action_drafts::SaveRequest {
        expected_fingerprint: world.fingerprint.clone(),
        expected_sha256: Some(saved.hash),
        draft,
        review: Some(action_drafts::ReviewConfirmation {
            frame,
            briefs: native.briefs,
            families: native.families,
            max_characters: native.max_characters,
            review_sha256: exact(&review, "reviewSha256")?.into(),
            sufficient: false,
        }),
    };
    let file = handles.issue("native-file", serde_json::to_string(&review)?)?;
    let candidate = if live {
        Some(handles.issue(
            "action-context",
            Pinned::bind(world, serde_json::to_string(&request)?)?,
        )?)
    } else {
        None
    };
    let context = &review["context"];
    let character = context
        .get("characterPrompt")
        .map(|packet| super::frame::packet(world, packet, true));
    let author = context
        .get("authorMargin")
        .map(|packet| super::frame::packet(world, packet, false));
    Ok(
        json!({"label":if stage=="proposal" {"Proposed alternative"} else {"Readiness review"},"requiredAnswered":review["requiredAnswered"],"changes":changes,
        "characterPrompt":character,"authorMargin":author,"candidate":candidate,"file":file}),
    )
}

fn confirm_context(
    world: &World,
    query: &BTreeMap<String, String>,
    handles: &mut Handles,
    live: bool,
) -> Result<Value> {
    closed(query, &["candidate", "sufficient"])?;
    if !live || field(query, "sufficient")? != "1" {
        return Err(invalid());
    }
    let raw = Pinned::read(
        world,
        &handles.resolve("action-context", field(query, "candidate")?)?,
    )?;
    let mut value: Value = serde_json::from_str(&raw)?;
    value["review"]["sufficient"] = true.into();
    let raw = serde_json::to_string(&value)?;
    let native = action_drafts::decode_save(&raw)?;
    crate::action_store::preview(world, &native)?;
    Ok(
        json!({"request":handles.request(ACTION_SAVE,raw)?,"label":display_text(world,&native.draft.title)}),
    )
}

fn inspect(world: &World, query: &BTreeMap<String, String>, handles: &Handles) -> Result<Value> {
    closed(query, &["route", "token"])?;
    let route = match field(query, "route")? {
        ACTION_APPLY => ACTION_APPLY,
        EDIT_APPLY => EDIT_APPLY,
        ACTION_SAVE => ACTION_SAVE,
        EDIT_SAVE => EDIT_SAVE,
        "/api/manuscripts/save" => "/api/manuscripts/save",
        "/api/retcon-reviews/save" => "/api/retcon-reviews/save",
        _ => return Err(invalid()),
    };
    let raw = handles.resolve(route, field(query, "token")?)?;
    let mut semantic_verification = None;
    let verified = match route {
        ACTION_APPLY => {
            let request = action_outcomes::decode_apply(&raw)?;
            action_outcomes::inspect_applied(world, &request)
                .and_then(|result| {
                    action_outcomes::verify(
                        world,
                        &action_outcomes::VerifyRequest {
                            expected_fingerprint: world.fingerprint.clone(),
                            action_id: request.action_id,
                            expected_sha256: exact(&result, "sourceSha256")?.into(),
                            max_characters: request.max_characters,
                        },
                    )
                })
                .ok()
                .is_some_and(|result| {
                    if result["verified"] != true {
                        return false;
                    }
                    semantic_verification = result
                        .get("semanticVerification")
                        .map(|report| semantic_checks(world, &BTreeMap::new(), report).0);
                    true
                })
        }
        EDIT_APPLY => {
            let request = edit_drafts::decode_apply(&raw)?;
            edit_drafts::inspect_applied(world, &request).is_ok()
        }
        ACTION_SAVE => {
            let request = action_drafts::decode_save(&raw)?;
            let expected = action_drafts::sha256(&action_drafts::serialize(&request.draft)?);
            request.expected_fingerprint == world.fingerprint
                && crate::action_store::detail(world, &request.draft.id, Some(&expected)).is_ok()
        }
        EDIT_SAVE => {
            let request = edit_drafts::decode_save(&raw)?;
            let expected = action_drafts::sha256(&edit_drafts::serialize(&request.draft)?);
            request.expected_fingerprint == world.fingerprint
                && edit_drafts::detail(world, &request.draft.id, Some(&expected)).is_ok()
        }
        _ => false,
    };
    Ok(json!({"verified":verified,"semanticVerification":semantic_verification}))
}

pub(super) fn read(
    service: &Service,
    endpoint: &str,
    query: &BTreeMap<String, String>,
    handles: &mut Handles,
    live: bool,
) -> Result<Value> {
    let world = service.world();
    match endpoint {
        "action-list" | "edit-list" => {
            closed(query, &[])?;
            lists(service, endpoint == "action-list", handles)
        }
        "action-open" | "edit-open" => {
            closed(query, &["draft"])?;
            let kind = if endpoint == "action-open" {
                "action"
            } else {
                "edit"
            };
            let selected = saved(world, handles, kind, field(query, "draft")?)?;
            let detail = native_detail(world, kind, &selected)?;
            if kind == "action" {
                action_open(world, &detail, handles)
            } else {
                edit_open(world, &detail, handles)
            }
        }
        "action-review" | "edit-review" => {
            review(world, query, handles, endpoint == "action-review", live)
        }
        "action-save" if live => action_save(service, query, handles),
        "edit-save" if live => edit_save(world, query, handles),
        "action-expectation" => expectation(world, query, handles),
        "action-prepare-context" => prepare_context(service, query, handles, live),
        "action-confirm-context" => confirm_context(world, query, handles, live),
        "approve" => approve(world, query, handles, live),
        "inspect" => inspect(world, query, handles),
        _ => Err(unavailable()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_shorthand_never_discards_other_members() {
        let directory = tempfile::tempdir().unwrap();
        let world =
            crate::source::init_world(&directory.path().join("world"), "Timeline test").unwrap();
        let time: Yaml = serde_yaml::from_str("{tick: 0, order: 0}").unwrap();
        assert!(matches!(
            display(&world, Some(&time), 0),
            Display::Reference(_)
        ));

        let compound: Yaml = serde_yaml::from_str("{tick: 0, order: 0, note: Other fact}").unwrap();
        let result = display(&world, Some(&compound), 0);
        assert!(!result.complete());
        let Display::Fields(fields) = result else {
            panic!("compound mapping lost its fields");
        };
        assert!(
            fields.iter().any(
                |item| item.label == "Note" && item.value == Display::Text("Other fact".into())
            )
        );
    }
}
