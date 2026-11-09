//! Explicit conversation creation and appended beats as one native consequence
//! Plan. Conversation prose does not infer events, state changes or knowledge.
use crate::{
    Error, Result, consequences,
    model::{Record, StoryTime, World},
    scene, validation,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use serde_yaml::{Mapping, Value as Yaml};
use std::{collections::BTreeSet, io::Write};

pub const MAX_REQUEST_BYTES: usize = 16_384;
pub const MAX_TITLE_BYTES: usize = 1_024;
pub const MIN_CHARACTERS: usize = consequences::MIN_CHARACTERS;
pub const MAX_CHARACTERS: usize = consequences::MAX_CHARACTERS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub expected_fingerprint: String,
    pub at: StoryTime,
    pub action: Action,
}

// Omission is the only absent optional request field. An explicit null must
// not silently discard an author-supplied addressee or interruption request.
fn optional<'de, D, T>(decoder: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(decoder).map(Some)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BeatKind {
    Speech,
    Action,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all_fields = "camelCase", deny_unknown_fields)]
pub enum Action {
    #[serde(rename = "conversation.create")]
    ConversationCreate {
        conversation_id: String,
        title: String,
        scene_id: String,
        character_ids: Vec<String>,
    },
    #[serde(rename = "conversation.append")]
    ConversationAppend {
        conversation_id: String,
        turn_id: String,
        kind: BeatKind,
        text: String,
        #[serde(
            default,
            deserialize_with = "optional",
            skip_serializing_if = "Option::is_none"
        )]
        speaker_id: Option<String>,
        #[serde(
            default,
            deserialize_with = "optional",
            skip_serializing_if = "Option::is_none"
        )]
        addressee_id: Option<String>,
        #[serde(
            default,
            deserialize_with = "optional",
            skip_serializing_if = "Option::is_none"
        )]
        interrupt_last: Option<bool>,
        #[serde(
            default,
            deserialize_with = "optional",
            skip_serializing_if = "Option::is_none"
        )]
        actor_ids: Option<Vec<String>>,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireRequest {
    expected_fingerprint: String,
    at: WireTime,
    action: Action,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireTime {
    timeline: String,
    tick: String,
    order: String,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}

fn identifier(value: &str, kind: &str) -> Result<()> {
    if !validation::valid_id_for_kind(value, kind) {
        return Err(invalid(format!(
            "conversation-control requires an exact {kind} ID"
        )));
    }
    Ok(())
}

fn characters(values: &[String], label: &str) -> Result<()> {
    if values.is_empty() || values.len() > MAX_REQUEST_BYTES / 31 {
        return Err(invalid(format!(
            "conversation-control requires a bounded nonempty {label} list"
        )));
    }
    let mut seen = BTreeSet::new();
    for id in values {
        identifier(id, "character")?;
        if !seen.insert(id) {
            return Err(invalid(format!(
                "conversation-control {label} must be distinct"
            )));
        }
    }
    Ok(())
}

fn validate_request(request: &Request) -> Result<()> {
    if request.expected_fingerprint.len() != 64
        || !request
            .expected_fingerprint
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid(
            "conversation-control requires an exact lowercase source fingerprint",
        ));
    }
    if request.at.timeline.trim().is_empty() || request.at.timeline.len() > 128 {
        return Err(invalid(
            "conversation-control timeline must be nonempty and within 128 UTF-8 bytes",
        ));
    }
    match &request.action {
        Action::ConversationCreate {
            conversation_id,
            title,
            scene_id,
            character_ids,
        } => {
            identifier(conversation_id, "conversation")?;
            if title.trim().is_empty() || title.len() > MAX_TITLE_BYTES {
                return Err(invalid(
                    "conversation-control title must be nonempty and within 1024 UTF-8 bytes",
                ));
            }
            identifier(scene_id, "scene")?;
            characters(character_ids, "characterIds")?;
        }
        Action::ConversationAppend {
            conversation_id,
            turn_id,
            kind,
            text,
            speaker_id,
            addressee_id,
            interrupt_last,
            actor_ids,
        } => {
            identifier(conversation_id, "conversation")?;
            if turn_id.trim().is_empty() {
                return Err(invalid(
                    "conversation.append requires a nonempty local turnId",
                ));
            }
            if text.trim().is_empty() {
                return Err(invalid("conversation.append requires nonempty text"));
            }
            match kind {
                BeatKind::Speech => {
                    let speaker = speaker_id
                        .as_deref()
                        .ok_or_else(|| invalid("speech beats require an exact speakerId"))?;
                    identifier(speaker, "character")?;
                    if let Some(addressee) = addressee_id {
                        identifier(addressee, "character")?;
                    }
                    if actor_ids.is_some() {
                        return Err(invalid("speech beats cannot carry actorIds"));
                    }
                }
                BeatKind::Action => {
                    if speaker_id.is_some() || addressee_id.is_some() || interrupt_last.is_some() {
                        return Err(invalid(
                            "action beats cannot carry speakerId, addresseeId or interruptLast",
                        ));
                    }
                    let actors = actor_ids
                        .as_ref()
                        .ok_or_else(|| invalid("action beats require nonempty actorIds"))?;
                    characters(actors, "actorIds")?;
                }
            }
        }
    }
    if serde_json::to_vec(request)?.len() > MAX_REQUEST_BYTES {
        return Err(invalid("conversation-control request exceeds 16 KiB"));
    }
    Ok(())
}

/// Decode a closed request with explicit exact decimal string coordinates.
/// This boundary never allocates an ID, an order or a shared authoring cursor.
pub fn decode(raw: &str) -> Result<Request> {
    if raw.len() > MAX_REQUEST_BYTES {
        return Err(invalid("conversation-control request exceeds 16 KiB"));
    }
    let mut decoder = serde_json::Deserializer::from_str(raw);
    let _: Yaml = Deserialize::deserialize(&mut decoder)
        .map_err(|_| invalid("invalid or duplicate conversation-control JSON"))?;
    decoder
        .end()
        .map_err(|_| invalid("invalid conversation-control JSON"))?;
    let wire: WireRequest = serde_json::from_str(raw)
        .map_err(|_| invalid("invalid conversation-control request fields"))?;
    let tick = wire.at.tick.parse::<i64>().map_err(|_| {
        invalid("conversation-control tick must be a canonical decimal string within i64")
    })?;
    let order = wire.at.order.parse::<i32>().map_err(|_| {
        invalid("conversation-control order must be a canonical decimal string within i32")
    })?;
    if tick.to_string() != wire.at.tick || order.to_string() != wire.at.order {
        return Err(invalid(
            "conversation-control coordinates must be canonical signed decimal strings",
        ));
    }
    let request = Request {
        expected_fingerprint: wire.expected_fingerprint,
        at: StoryTime::new(wire.at.timeline, tick, order)?,
        action: wire.action,
    };
    validate_request(&request)?;
    Ok(request)
}

fn exact<'a>(world: &'a World, id: &str, kind: &str) -> Result<&'a Record> {
    let record = world
        .records
        .get(id)
        .ok_or_else(|| Error::NotFound(format!("unknown exact {kind} ID {id:?}")))?;
    if record.kind != kind {
        return Err(invalid(format!("{id:?} is not a {kind}")));
    }
    Ok(record)
}

fn source_point(at: &StoryTime) -> Yaml {
    Mapping::from_iter([
        ("timeline".into(), at.timeline.clone().into()),
        ("tick".into(), at.tick.into()),
        ("order".into(), i64::from(at.order).into()),
    ])
    .into()
}

fn point(world: &World, value: &Yaml) -> Result<StoryTime> {
    StoryTime::from_yaml(value, &world.default_timeline())
}

fn optional_point(world: &World, value: Option<&Yaml>) -> Result<Option<StoryTime>> {
    value
        .filter(|value| !value.is_null())
        .map(|value| point(world, value))
        .transpose()
}

fn list<'a>(record: &'a Record, field: &str) -> &'a [Yaml] {
    record.list(field).map(Vec::as_slice).unwrap_or(&[])
}

// Creating a conversation may use a future explicit time. The authored scene
// and participant intervals apply, without the read API's old current cap.
fn prospective_present(world: &World, scene: &Record, id: &str, at: &StoryTime) -> Result<bool> {
    let time = scene.get("time").expect("validated scene time");
    let start = point(world, &time["start"])?;
    let end = optional_point(world, time.get("end"))?;
    if !start.not_after(at) || end.as_ref().is_some_and(|end| !at.not_after(end)) {
        return Ok(false);
    }
    let Some(row) = list(scene, "participants")
        .iter()
        .find(|row| row["character"].as_str() == Some(id))
    else {
        return Ok(false);
    };
    let from = optional_point(world, row.get("from"))?.unwrap_or(start);
    let until = optional_point(world, row.get("until"))?.or(end);
    Ok(from.not_after(at) && until.as_ref().is_none_or(|until| at.not_after(until)))
}

fn source_bound(record: &Record) -> Result<()> {
    if record.raw_bytes.len() > consequences::MAX_PREVIEW_BYTES {
        return Err(invalid(
            "complete conversation-control source exceeds 8 MiB",
        ));
    }
    Ok(())
}

fn construct(world: &World, request: &Request) -> Result<consequences::Plan> {
    validate_request(request)?;
    if request.expected_fingerprint != world.fingerprint {
        return Err(Error::Conflict(
            "source fingerprint differs from the conversation-control request snapshot".into(),
        ));
    }
    if !validation::validate_world(world).valid {
        return Err(Error::Validation(
            "base snapshot failed validation; run validate for diagnostics".into(),
        ));
    }
    world.story_time(
        request.at.tick,
        Some(&request.at.timeline),
        request.at.order,
    )?;
    let at = &request.at;
    let operations = match &request.action {
        Action::ConversationCreate {
            conversation_id,
            title,
            scene_id,
            character_ids,
        } => {
            if world.records.contains_key(conversation_id) {
                return Err(Error::Conflict(format!(
                    "conversation ID {conversation_id:?} already exists"
                )));
            }
            let scene = exact(world, scene_id, "scene")?;
            source_bound(scene)?;
            if scene.status() != "active" {
                return Err(invalid("conversation.create requires an active scene"));
            }
            let cursor = optional_point(world, world.world_record()?.get("current_time"))?
                .unwrap_or(point(world, &scene.frontmatter["time"]["current"])?);
            if !cursor.not_after(at) {
                return Err(invalid(
                    "conversation.create cannot precede the active cursor or change its timeline",
                ));
            }
            for id in character_ids {
                exact(world, id, "character")?;
                if !prospective_present(world, scene, id, at)? {
                    return Err(invalid(
                        "conversation.create characters must be present in the selected scene at the conversation time",
                    ));
                }
            }
            let participants = character_ids
                .iter()
                .map(|id| {
                    Yaml::Mapping(Mapping::from_iter([
                        ("character".into(), id.clone().into()),
                        ("from".into(), source_point(at)),
                    ]))
                })
                .collect::<Vec<_>>();
            let draft = Mapping::from_iter([
                ("kind".into(), "conversation".into()),
                ("id".into(), conversation_id.clone().into()),
                ("title".into(), title.clone().into()),
                ("status".into(), "active".into()),
                ("scene".into(), scene_id.clone().into()),
                (
                    "location".into(),
                    scene
                        .get("location")
                        .expect("validated scene location")
                        .clone(),
                ),
                (
                    "time".into(),
                    Mapping::from_iter([("start".into(), source_point(at))]).into(),
                ),
                ("participants".into(), participants.into()),
                ("turns".into(), Vec::<Yaml>::new().into()),
                ("recollections".into(), Vec::<Yaml>::new().into()),
                ("body".into(), format!("# {title}\n").into()),
            ]);
            let mut conversations = list(scene, "conversations").to_vec();
            conversations.push(conversation_id.clone().into());
            let patch = Mapping::from_iter([
                ("kind".into(), "scene".into()),
                ("conversations".into(), conversations.into()),
            ]);
            vec![
                consequences::Operation::Create {
                    draft: serde_yaml::to_string(&draft)?,
                },
                consequences::Operation::Update {
                    entity_id: scene_id.clone(),
                    patch: serde_yaml::to_string(&patch)?,
                },
            ]
        }
        Action::ConversationAppend {
            conversation_id,
            turn_id,
            kind,
            text,
            speaker_id,
            addressee_id,
            interrupt_last,
            actor_ids,
        } => {
            let conversation = exact(world, conversation_id, "conversation")?;
            source_bound(conversation)?;
            if conversation.status() != "active" {
                return Err(invalid(
                    "conversation.append requires an active conversation",
                ));
            }
            // The append contract preserves source order and permits no time
            // collision. It does not change existing native raw-edit behavior.
            let mut previous: Option<StoryTime> = None;
            for turn in list(conversation, "turns") {
                let current = scene::turn_time(world, conversation, turn)?;
                if previous
                    .as_ref()
                    .is_some_and(|previous| !previous.not_after(&current) || previous == &current)
                {
                    return Err(invalid(
                        "conversation.append requires strictly ordered existing turns",
                    ));
                }
                previous = Some(current);
            }
            if previous
                .as_ref()
                .is_some_and(|previous| !previous.not_after(at) || previous == at)
            {
                return Err(invalid(
                    "conversation.append time must be strictly after every existing turn on the same timeline",
                ));
            }
            let require_participant = |id: &str| -> Result<()> {
                exact(world, id, "character")?;
                if !scene::present(world, conversation, id, at)? {
                    return Err(invalid(
                        "conversation.append characters must be conversation participants present at the beat time",
                    ));
                }
                Ok(())
            };
            let mut turn = Mapping::from_iter([
                ("id".into(), turn_id.clone().into()),
                (
                    "kind".into(),
                    match kind {
                        BeatKind::Speech => "speech",
                        BeatKind::Action => "action",
                    }
                    .into(),
                ),
                ("at".into(), source_point(at)),
                ("text".into(), text.clone().into()),
                ("audience".into(), vec![Yaml::from("participants")].into()),
            ]);
            match kind {
                BeatKind::Speech => {
                    let speaker = speaker_id.as_ref().expect("validated speech speaker");
                    require_participant(speaker)?;
                    turn.insert("speaker".into(), speaker.clone().into());
                    if let Some(addressee) = addressee_id {
                        require_participant(addressee)?;
                        turn.insert("addressee".into(), addressee.clone().into());
                    }
                    if interrupt_last == &Some(true) {
                        let earlier = list(conversation, "turns")
                            .iter()
                            .rev()
                            .find(|turn| {
                                turn.get("kind").and_then(Yaml::as_str).unwrap_or("speech")
                                    == "speech"
                            })
                            .ok_or_else(|| {
                                invalid("there is no earlier spoken line to interrupt")
                            })?;
                        turn.insert(
                            "interrupts".into(),
                            earlier
                                .get("id")
                                .expect("validated earlier speech ID")
                                .clone(),
                        );
                    }
                }
                BeatKind::Action => {
                    let actors = actor_ids.as_ref().expect("validated action actors");
                    for id in actors {
                        require_participant(id)?;
                    }
                    turn.insert(
                        "actors".into(),
                        actors
                            .iter()
                            .cloned()
                            .map(Yaml::from)
                            .collect::<Vec<_>>()
                            .into(),
                    );
                }
            }
            let mut turns = list(conversation, "turns").to_vec();
            turns.push(turn.into());
            let patch = Mapping::from_iter([
                ("kind".into(), "conversation".into()),
                ("turns".into(), turns.into()),
            ]);
            vec![consequences::Operation::Update {
                entity_id: conversation_id.clone(),
                patch: serde_yaml::to_string(&patch)?,
            }]
        }
    };
    let plan = consequences::Plan {
        expected_fingerprint: request.expected_fingerprint.clone(),
        operations,
        analysis: None,
    };
    consequences::validate_plan(&plan)?;
    Ok(plan)
}

fn prepare(
    world: &World,
    request: &Request,
    maximum: usize,
) -> Result<(consequences::Plan, consequences::Prepared)> {
    let plan = construct(world, request)?;
    let prepared = consequences::prepare(world, &plan, maximum)?;
    if prepared.report["validation"]["valid"] != true {
        return Err(Error::Validation(format!(
            "conversation-control candidate failed validation: {}",
            prepared.report["validation"]["diagnostics"]
        )));
    }
    Ok((plan, prepared))
}

/// Return the exact complete candidate-validated Plan. The ordinary consequence
/// review and guarded writer own any subsequent publication.
pub fn plan(world: &World, request: &Request) -> Result<consequences::Plan> {
    prepare(world, request, MAX_CHARACTERS).map(|(plan, _)| plan)
}

fn bounded_result(value: &Json, maximum: usize) -> Result<()> {
    struct Bounded {
        bytes: Vec<u8>,
    }
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > consequences::MAX_PREVIEW_BYTES.saturating_sub(self.bytes.len()) {
                return Err(std::io::Error::other(
                    "complete conversation-control JSON bound",
                ));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Bounded { bytes: Vec::new() };
    serde_json::to_writer(&mut writer, value)
        .map_err(|_| invalid("conversation-control preview exceeds 8 MiB of complete JSON"))?;
    let characters = std::str::from_utf8(&writer.bytes)
        .expect("JSON is UTF-8")
        .chars()
        .count();
    if characters > maximum {
        return Err(invalid(format!(
            "complete conversation-control preview requires {characters} characters; maxCharacters is {maximum}"
        )));
    }
    Ok(())
}

/// Export the exact Plan and complete ordinary consequence preview. Explicit
/// coordinates leave world/scene clocks unchanged for both supported actions.
pub fn preview(world: &World, request: &Request, max_characters: usize) -> Result<Json> {
    if !(MIN_CHARACTERS..=MAX_CHARACTERS).contains(&max_characters) {
        return Err(invalid(
            "conversation-control preview maxCharacters must be 256..1000000",
        ));
    }
    let (plan, prepared) = prepare(world, request, max_characters)?;
    let result = json!({
        "protocol": crate::CONTRACT, "operation":"conversation-control-preview",
        "scope":"local-author-conversation-control", "request":request,
        "plan":plan, "preview":prepared.report,
        "maxCharacters":max_characters, "complete":true, "validationComplete":false,
        "writesPerformed":false, "canApply":false,
    });
    bounded_result(&result, max_characters)?;
    Ok(result)
}
