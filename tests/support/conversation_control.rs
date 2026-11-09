#![allow(dead_code)]
//! Small real-source fixture shared by conversation planners and transports.
#[path = "scene_control.rs"]
mod scenes;
pub use scenes::{ACTOR, ORDER, ORIGIN, SCENE, TICK, at, candidate, changed, point, source_image};
use serde_json::{Value as Json, json};
use std::fs;
use wedl::{model::World, source, validation};

pub const THIRD: &str = "char_00000000000000000000000003";
pub const CONVERSATION: &str = "conv_00000000000000000000000001";
pub const NEW_CONVERSATION: &str = "conv_00000000000000000000000002";
pub const OTHER_ACTOR: &str = scenes::OTHER_ACTOR;
pub const OTHER_SCENE: &str = scenes::OTHER_SCENE;
pub const WORLD: &str = scenes::WORLD;

pub fn fixture() -> (tempfile::TempDir, World) {
    let (directory, world) = scenes::fixture();
    for (id, fields, body) in [
        (
            THIRD,
            json!({"kind":"character","initial_state":{"location":{"entity":ORIGIN}}}),
            "Third character exact source.\r\n",
        ),
        (
            CONVERSATION,
            json!({"kind":"conversation","status":"active","scene":SCENE,"location":ORIGIN,
                "time":{"start":point(TICK,ORDER-2)},
                "participants":[{"character":ACTOR},{"character":THIRD}],
                "turns":[
                    {"id":"original-speech","at":point(TICK,ORDER-2),"speaker":ACTOR,
                        "text":"Original speech 🌲  \r\nRetain the exact words.","audience":["participants"]},
                    {"id":"intervening-action","kind":"action","at":point(TICK,ORDER-1),
                        "actors":[ACTOR],"text":"An earlier gesture.","audience":["participants"]}
                ],
                "recollections":[{"id":"original-memory","character":ACTOR,
                    "at":point(TICK,ORDER-1),"state":"remembered","confidence":0.75,
                    "summary":"My independently authored memory.","exact_turns":["original-speech"],
                    "remembered_quotes":[{"source_turn":"original-speech","speaker":ACTOR,
                        "text":"An imperfect memory","fidelity":"approximate"}]}]}),
            "\r\n# Conversation source\r\nExact body 🌲  \r\nNo final newline",
        ),
    ] {
        let mut data = json!({"schema":"wedl/v1","id":id,"title":format!("Conversation fixture {id}"),
            "status":"canonical","x-author":{"literal":null,"numbers":[1,1.0,"1"]}});
        data.as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        let header = format!(
            "---\n# Preserve unchanged source.\n{}---\n",
            serde_yaml::to_string(&data).unwrap()
        )
        .replace('\n', "\r\n");
        fs::write(
            world.root.join("story").join(format!("{id}.md")),
            format!("{header}{body}"),
        )
        .unwrap();
    }
    let scene = &world.records[SCENE];
    let mut fields = scene.frontmatter.clone();
    fields["participants"]
        .as_sequence_mut()
        .unwrap()
        .push(serde_yaml::to_value(json!({"character":THIRD,"role":"listener"})).unwrap());
    fields["conversations"] = serde_yaml::to_value(json!([CONVERSATION])).unwrap();
    fs::write(
        world.root.join(&scene.source_path),
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&fields).unwrap(),
            scene.body
        ),
    )
    .unwrap();
    let world = source::load_world(&world.root, None).unwrap();
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    (directory, world)
}

pub fn input(world: &World, order: i32, action: Json) -> Json {
    json!({"expectedFingerprint":world.fingerprint,"at":at(order),"action":action})
}

pub fn create(world: &World) -> Json {
    input(
        world,
        ORDER + 1,
        json!({"type":"conversation.create","conversationId":NEW_CONVERSATION,
        "title":"  A new conversation 🌲  ","sceneId":SCENE,"characterIds":[THIRD,ACTOR]}),
    )
}

pub fn speech(world: &World) -> Json {
    input(
        world,
        ORDER + 1,
        json!({"type":"conversation.append","conversationId":CONVERSATION,
        "turnId":"new-speech","kind":"speech","text":"  Exact speech 🌲\r\nTrailing bytes  ",
        "speakerId":ACTOR,"addresseeId":THIRD}),
    )
}

pub fn action(world: &World) -> Json {
    input(
        world,
        ORDER + 1,
        json!({"type":"conversation.append","conversationId":CONVERSATION,
        "turnId":"new-action","kind":"action","text":"  An exact action 🌲\r\n  ",
        "actorIds":[THIRD,ACTOR]}),
    )
}
