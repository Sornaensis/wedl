use super::*;
use crate::{
    model::{StoryTime, World},
    service::Operation,
    writing_frames::{self, Frame, Lens, WorkSelection},
};

const WORK: &str = "writing-work";
const SCENE: &str = "writing-scene";
const POV: &str = "writing-pov";
const MOMENT: &str = "writing-moment";
const CONVERSATION: &str = "writing-conversation";
const BRIEF: &str = "writing-brief";
const FRAME: &str = "writing-frame";
const SELECTION: &str = "writing-selection";
const BUDGET: usize = 100_000;
const PLAN_BUDGET: usize = 1_000_000;

pub(super) fn record_label<'a>(world: &'a World, id: &str) -> Option<&'a str> {
    world.records.get(id).map(|record| record.title.as_str())
}

fn timeline_label<'a>(world: &'a World, timeline: &str) -> Option<&'a str> {
    world
        .world_record()
        .ok()?
        .list("timelines")?
        .iter()
        .find(|row| row["id"].as_str() == Some(timeline))?["label"]
        .as_str()
}

fn moment_names(world: &World) -> Result<Vec<(StoryTime, String)>> {
    let mut result = vec![(world.current_time()?, "Current story time".into())];
    for record in world.records.values() {
        let Some(time) = record.get("time") else {
            continue;
        };
        if matches!(record.kind.as_str(), "event" | "story-point") {
            if let Ok(at) = StoryTime::from_yaml(time, &world.default_timeline()) {
                result.push((at, record.title.clone()));
            }
        } else if record.kind == "scene" {
            for (key, label) in [
                ("start", "Scene opening"),
                ("current", "Scene position"),
                ("end", "Scene closing"),
            ] {
                if let Some(value) = time.get(key).filter(|value| !value.is_null()) {
                    result.push((
                        StoryTime::from_yaml(value, &world.default_timeline())?,
                        format!("{} · {label}", record.title),
                    ));
                }
            }
        }
    }
    if result.len() > 4096 {
        return Err(invalid());
    }
    for (at, title) in &mut result {
        world.story_time(at.tick, Some(&at.timeline), at.order)?;
        if let Some(timeline) = timeline_label(world, &at.timeline) {
            *title = format!("{title} · {timeline}");
        }
    }
    Ok(result)
}

pub(super) fn named_moment(world: &World, target: &StoryTime) -> Option<String> {
    moment_names(world)
        .ok()?
        .into_iter()
        .find(|(time, _)| time == target)
        .map(|(_, label)| label)
}

fn choice_set(
    world: &World,
    handles: &mut Handles,
    record_kind: &str,
    token_kind: &'static str,
) -> Result<Vec<Choice>> {
    world
        .records
        .values()
        .filter(|record| record.kind == record_kind)
        .map(|record| {
            choice(
                handles,
                token_kind,
                &display_text(world, &record.title),
                Pinned::bind(world, record.id.clone())?,
            )
        })
        .collect()
}

fn choices(service: &Service, handles: &mut Handles) -> Result<Value> {
    let world = service.world();
    let frame = Frame {
        fingerprint: world.fingerprint.clone(),
        work: None,
        scene_id: None,
        pov_id: None,
        at: world.current_time()?,
        lens: Lens::Author,
    };
    let options = service.execute(Operation::WritingChoices { frame, limit: 1000 })?;
    let options = &options["result"];
    let work_source = options["authorFingerprint"].as_str();
    let mut works = Vec::new();
    for work in options["works"].as_array().into_iter().flatten() {
        let selected = WorkSelection {
            work_id: exact(work, "id")?.into(),
            author_fingerprint: work_source.ok_or_else(unavailable)?.into(),
        };
        let value = serde_json::to_string(&selected)?;
        works.push(choice(
            handles,
            WORK,
            &display_text(world, exact(work, "label")?),
            Pinned::bind(world, value)?,
        )?);
    }
    let moments = moment_names(world)?
        .into_iter()
        .map(|(at, label)| {
            let value = serde_json::to_string(&json!({"at":at,"label":label}))?;
            choice(
                handles,
                MOMENT,
                &display_text(world, &label),
                Pinned::bind(world, value)?,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(
        json!({"works":works,"moments":moments,"scenes":choice_set(world, handles, "scene", SCENE)?,
        "viewpoints":choice_set(world, handles, "character", POV)?, "conversations":choice_set(world, handles, "conversation", CONVERSATION)?,
        "briefs":choice_set(world, handles, "context-brief", BRIEF)?}),
    )
}

fn selected(
    world: &World,
    query: &BTreeMap<String, String>,
    key: &str,
    kind: &'static str,
    handles: &Handles,
) -> Result<Option<String>> {
    query
        .get(key)
        .filter(|token| !token.is_empty())
        .map(|token| Pinned::read(world, &handles.resolve(kind, token)?))
        .transpose()
}

fn fields(
    service: &Service,
    query: &BTreeMap<String, String>,
    handles: &Handles,
) -> Result<(Frame, String)> {
    let world = service.world();
    let work = selected(world, query, "work", WORK, handles)?
        .map(|raw| -> Result<_> {
            let value: Value = serde_json::from_str(&raw)?;
            Ok(WorkSelection {
                work_id: exact(&value, "workId")?.into(),
                author_fingerprint: exact(&value, "authorFingerprint")?.into(),
            })
        })
        .transpose()?;
    let moment: Value = serde_json::from_str(
        &selected(world, query, "moment", MOMENT, handles)?.ok_or_else(invalid)?,
    )?;
    let lens = match field(query, "lens")? {
        "author" => Lens::Author,
        "character" => Lens::Character,
        "dramatic-irony" => Lens::DramaticIrony,
        _ => return Err(invalid()),
    };
    let frame = Frame {
        fingerprint: world.fingerprint.clone(),
        work,
        scene_id: selected(world, query, "scene", SCENE, handles)?,
        pov_id: selected(world, query, "viewpoint", POV, handles)?,
        at: at(&moment["at"])?,
        lens,
    };
    Ok((frame, exact(&moment, "label")?.into()))
}

#[derive(Serialize)]
pub(super) struct Line {
    label: &'static str,
    text: String,
}

pub(super) fn packet(world: &World, value: &Value, character: bool) -> Vec<Line> {
    let mut lines = Vec::new();
    let names: Vec<_> = value["scene"]["presentParticipants"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| {
            row.as_str()
                .or_else(|| row["characterId"].as_str())
                .and_then(|id| record_label(world, id))
        })
        .collect();
    if !names.is_empty() {
        lines.push(Line {
            label: "Present participants",
            text: names.join(", "),
        });
    }
    for (key, prose, title) in [
        ("beliefs", "statement", "Learned belief"),
        ("observations", "text", "Observation"),
        ("descriptions", "markdown", "Description"),
    ] {
        for row in value[key].as_array().into_iter().flatten() {
            if let Some(text) = row[prose].as_str() {
                lines.push(Line {
                    label: title,
                    text: text.into(),
                });
            }
        }
    }
    if character {
        for conversation in value["conversations"].as_array().into_iter().flatten() {
            for beat in conversation["beats"].as_array().into_iter().flatten() {
                if let Some(text) = beat["text"].as_str() {
                    lines.push(Line {
                        label: "Conversation beat",
                        text: text.into(),
                    });
                }
            }
        }
    }
    for row in value["scene"]["outcomeEvents"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if let Some(title) = row["title"].as_str() {
            lines.push(Line {
                label: "Authored outcome",
                text: title.into(),
            });
        }
    }
    for (key, title) in [
        ("objects", "Present object"),
        ("environments", "Present environment"),
    ] {
        for row in value["present"][key].as_array().into_iter().flatten() {
            if let Some(label) = row["objectId"]
                .as_str()
                .or_else(|| row["environmentId"].as_str())
                .and_then(|id| record_label(world, id))
            {
                lines.push(Line {
                    label: title,
                    text: label.into(),
                });
            }
        }
    }
    for line in &mut lines {
        line.text = display_text(world, &line.text);
    }
    lines
}

#[derive(Serialize)]
struct FamilyCard {
    label: &'static str,
    entries: Vec<String>,
}

fn family_cards(world: &World, packet: &Value) -> Vec<FamilyCard> {
    fn append(world: &World, value: &Value, entries: &mut Vec<String>, depth: usize) {
        if depth > 6 || entries.len() >= 1000 {
            return;
        }
        match value {
            Value::Array(rows) => {
                for row in rows {
                    append(world, row, entries, depth + 1);
                }
            }
            Value::Object(fields) => {
                for (key, item) in fields {
                    if matches!(
                        key.as_str(),
                        "label" | "title" | "statement" | "summary" | "text" | "markdown"
                    ) {
                        if let Some(text) = item.as_str() {
                            entries.push(display_text(world, text));
                        }
                    } else if item.is_object() || item.is_array() {
                        append(world, item, entries, depth + 1);
                    }
                }
            }
            _ => {}
        }
    }
    [
        ("species", "Species"),
        ("magic", "Magic"),
        ("social", "Social context"),
        ("environment", "Environment"),
    ]
    .into_iter()
    .filter_map(|(key, label)| {
        let selected = packet["familyContexts"].get(key)?;
        let mut entries = Vec::new();
        append(world, selected, &mut entries, 0);
        Some(FamilyCard { label, entries })
    })
    .collect()
}

fn resolve_frame(
    service: &Service,
    frame: Frame,
    moment: String,
    exact_frame: String,
    query: &BTreeMap<String, String>,
    handles: &mut Handles,
) -> Result<Value> {
    let world = service.world();
    let work = writing_frames::with_author_snapshot(world, &frame, false, |authors| {
        writing_frames::resolve(world, authors, &frame)?;
        Ok(json!(
            frame
                .work
                .as_ref()
                .and_then(|work| authors?.documents.get(&work.work_id))
                .map(|record| display_text(world, &record.title))
        ))
    })?;
    let name = |id: &str| record_label(world, id).map(|label| display_text(world, label));
    let mut view = json!({"moment":display_text(world,&moment),"scene":frame.scene_id.as_deref().and_then(name),
        "viewpoint":frame.pov_id.as_deref().and_then(name),"lens":frame.lens,
        "work":work,"hasContext":frame.scene_id.is_some() && frame.pov_id.is_some()});
    if view["hasContext"] == true {
        let briefs = selected(world, query, "brief", BRIEF, handles)?
            .into_iter()
            .collect();
        let families = query
            .get("family")
            .filter(|value| !value.is_empty())
            .map(|raw| writing_frames::decode_families(raw))
            .transpose()?
            .unwrap_or_default();
        let context = service.execute(Operation::WritingFamilyContext {
            frame: frame.clone(),
            briefs,
            families,
            max_characters: BUDGET,
        })?;
        for (key, families, character) in [
            ("characterPrompt", "characterFamilies", true),
            ("authorMargin", "authorFamilies", false),
        ] {
            if let Some(packet_value) = context["result"].get(key) {
                view[key] = serde_json::to_value(packet(world, packet_value, character))?;
                view[families] = serde_json::to_value(family_cards(world, packet_value))?;
            }
        }
    }
    let selection = serde_json::to_string(
        &json!({"frame":exact_frame,"moment":moment,"brief":query.get("brief"),"family":query.get("family")}),
    )?;
    view["restore"] = handles.issue(SELECTION, selection)?.into();
    view["frame"] = handles.issue(FRAME, exact_frame.clone())?.into();
    view["file"] = handles.issue("native-file", exact_frame)?.into();
    Ok(view)
}

fn plan_result(
    world: &World,
    native: &Value,
    clock: Option<String>,
    handles: &mut Handles,
) -> Result<Value> {
    let native_plan = native.get("plan").ok_or_else(unavailable)?;
    let plan_file = handles.issue("native-file", serde_json::to_string_pretty(native_plan)?)?;
    let preview = native.get("preview").ok_or_else(unavailable)?;
    Ok(
        json!({"changes":super::candidates::diffs(world, preview)?, "validation":super::candidates::checks(world, preview)?.0, "clock":clock, "file":plan_file}),
    )
}

fn plan(
    service: &Service,
    query: &BTreeMap<String, String>,
    handles: &mut Handles,
) -> Result<Value> {
    let world = service.world();
    let frame = writing_frames::decode(&handles.resolve(FRAME, field(query, "frame")?)?)?;
    service.execute(Operation::WritingResolve {
        frame: frame.clone(),
    })?;
    let kind = field(query, "kind")?;
    let (native, clock) = match kind {
        "clock" => (
            crate::current_time::preview(
                world,
                &crate::current_time::Request {
                    expected_fingerprint: frame.fingerprint,
                    at: frame.at.clone(),
                },
                PLAN_BUDGET,
            )?,
            named_moment(world, &frame.at),
        ),
        "advance" | "close" => {
            let scene_id = frame.scene_id.ok_or_else(invalid)?;
            let action = if kind == "close" {
                crate::scene_control::Action::SceneClose { scene_id }
            } else {
                crate::scene_control::Action::SceneAdvance {
                    scene_id,
                    location_id: None,
                    event: None,
                }
            };
            (
                crate::scene_control::preview(
                    world,
                    &crate::scene_control::Request {
                        expected_fingerprint: frame.fingerprint,
                        at: frame.at,
                        action,
                    },
                    PLAN_BUDGET,
                )?,
                None,
            )
        }
        "speech" => {
            let conversation_id = selected(world, query, "conversation", CONVERSATION, handles)?
                .ok_or_else(invalid)?;
            let speaker_id = frame.pov_id.ok_or_else(invalid)?;
            let mut random = [0; 16];
            getrandom::fill(&mut random).map_err(|_| unavailable())?;
            let action = crate::conversation_control::Action::ConversationAppend {
                conversation_id,
                turn_id: format!("turn-{}", hex::encode(random)),
                kind: crate::conversation_control::BeatKind::Speech,
                text: field(query, "prose")?.into(),
                speaker_id: Some(speaker_id),
                addressee_id: None,
                interrupt_last: None,
                actor_ids: None,
            };
            (
                crate::conversation_control::preview(
                    world,
                    &crate::conversation_control::Request {
                        expected_fingerprint: frame.fingerprint,
                        at: frame.at,
                        action,
                    },
                    PLAN_BUDGET,
                )?,
                None,
            )
        }
        _ => return Err(invalid()),
    };
    plan_result(world, &native, clock, handles)
}

pub(super) fn read(
    service: &Service,
    endpoint: &str,
    query: &BTreeMap<String, String>,
    handles: &mut Handles,
) -> Result<Value> {
    match endpoint {
        "writing-choices" => {
            closed(query, &[])?;
            choices(service, handles)
        }
        "writing-frame" => {
            closed(
                query,
                &[
                    "work",
                    "moment",
                    "scene",
                    "viewpoint",
                    "lens",
                    "brief",
                    "family",
                    "frame",
                ],
            )?;
            let (frame, moment, raw) = if let Some(token) = query.get("frame") {
                if query.len() != 1 {
                    return Err(invalid());
                }
                let selection: Value = serde_json::from_str(&handles.resolve(SELECTION, token)?)?;
                let raw = exact(&selection, "frame")?.to_owned();
                let frame = writing_frames::decode(&raw)?;
                let moment = exact(&selection, "moment")?.to_owned();
                let fields = ["brief", "family"]
                    .into_iter()
                    .filter_map(|key| {
                        selection[key]
                            .as_str()
                            .map(|value| (key.into(), value.into()))
                    })
                    .collect();
                return resolve_frame(service, frame, moment, raw, &fields, handles);
            } else {
                let (frame, moment) = fields(service, query, handles)?;
                let raw = serde_json::to_string_pretty(&frame)?;
                (frame, moment, raw)
            };
            resolve_frame(service, frame, moment, raw, query, handles)
        }
        "writing-import" => {
            closed(query, &["raw"])?;
            let raw = field(query, "raw")?;
            let frame = writing_frames::decode(raw)?;
            let moment = named_moment(service.world(), &frame.at)
                .unwrap_or_else(|| "Selected story position".into());
            resolve_frame(service, frame, moment, raw.into(), query, handles)
        }
        "writing-plan" => {
            closed(query, &["frame", "kind", "conversation", "prose"])?;
            plan(service, query, handles)
        }
        "writing-exchange" => {
            closed(query, &["raw", "kind"])?;
            let world = service.world();
            let raw = field(query, "raw")?;
            let native = match field(query, "kind")? {
                "clock" => crate::current_time::preview(
                    world,
                    &crate::current_time::decode(raw)?,
                    PLAN_BUDGET,
                )?,
                "scene" => crate::scene_control::preview(
                    world,
                    &crate::scene_control::decode(raw)?,
                    PLAN_BUDGET,
                )?,
                "conversation" => crate::conversation_control::preview(
                    world,
                    &crate::conversation_control::decode(raw)?,
                    PLAN_BUDGET,
                )?,
                "plan" => {
                    let preview = crate::consequences::preview(
                        world,
                        &crate::consequences::decode(raw)?,
                        PLAN_BUDGET,
                    )?;
                    return Ok(
                        json!({"changes":super::candidates::diffs(world,&preview)?,"validation":super::candidates::checks(world,&preview)?.0,"clock":null,"file":handles.issue("native-file",raw.into())?}),
                    );
                }
                _ => return Err(invalid()),
            };
            plan_result(world, &native, None, handles)
        }
        _ => Err(unavailable()),
    }
}
