//! Integrated source-citation checks across the admitted catalogue and native
//! record/member vocabulary.
use serde_json::{Value as Json, json};
use std::path::PathBuf;
use wedl::{
    catalogue,
    model::{Record, World},
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const VIEWER: &str = "char_00000000000000000000000001";
const OTHER: &str = "char_00000000000000000000000002";
const PLACE: &str = "loc_00000000000000000000000001";
const SCENE: &str = "scene_00000000000000000000000001";
const CONVERSATION: &str = "conv_00000000000000000000000001";
const KNOWLEDGE: &str = "know_00000000000000000000000001";
const DESCRIPTION: &str = "description_00000000000000000000000001";

fn record(kind: &str, id: &str, fields: Json) -> Record {
    let mut value = json!({"schema":"wedl/v1","kind":kind,"id":id,"title":kind});
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    source::parse_record(
        format!("---\n{}---\n", serde_yaml::to_string(&value).unwrap()).as_bytes(),
        &format!("story/{id}.md"),
    )
    .unwrap()
}

fn fixture() -> World {
    let world = source::world_from_records(PathBuf::new(), vec![
        record("world", WORLD, json!({"capabilities":["lore-core-v1","descriptions-v1"],"default_timeline":"main","timelines":[{"id":"main","label":"Main"}],"state_keys":{}})),
        record("character", VIEWER, json!({})),
        record("character", OTHER, json!({})),
        record("location", PLACE, json!({"descriptions":[{"id":DESCRIPTION,"heading":"The place",
            "markdown":"A turquoise account.","valid":{"kind":"timeless"},"audience":["public"],"perspectives":["ordinary"]}]})),
        record("scene", SCENE, json!({"status":"active","location":PLACE,
            "time":{"start":{"tick":0},"current":{"tick":10}},
            "participants":[{"character":VIEWER},{"character":OTHER}],
            "conversations":[CONVERSATION],
            "observations":[
                {"id":"withheld_observation","at":{"tick":1},"audience":["author"],"text":"A withheld note."},
                {"id":"observed_signal","at":{"tick":5},"audience":["participants"],"text":"A saffron signal."}]})),
        record("conversation", CONVERSATION, json!({"status":"active","location":PLACE,"scene":SCENE,
            "time":{"start":{"tick":0}},"participants":[{"character":VIEWER},{"character":OTHER}],
            "turns":[
                {"id":"withheld_speech","kind":"speech","at":{"tick":1},"speaker":OTHER,"audience":["author"],"text":"An unheard remark."},
                {"id":"said_signal","kind":"speech","at":{"tick":5},"speaker":VIEWER,"audience":["participants"],"text":"A crimson signal."},
                {"id":"did_signal","kind":"action","at":{"tick":6},"actors":[VIEWER],"audience":["participants"],"text":"An indigo signal."}
            ],
            "recollections":[
                {"id":"withheld_memory","character":OTHER,"at":{"tick":7},"state":"remembered","confidence":0.8,"summary":"Another recollection."},
                {"id":"remembered_signal","character":VIEWER,"at":{"tick":8},"state":"remembered","confidence":0.8,"summary":"A viridian signal."}]})),
        record("knowledge", KNOWLEDGE, json!({"knower":VIEWER,
            "claim":{"key":"known_conversation","statement":"I know this conversation.","object":{"entity":CONVERSATION}},
            "transitions":[{"id":"acquired","time":{"tick":0},"state":"accepted","confidence":1.0,"acquisition":"heard"}]})),
    ], None).unwrap();
    let checked = validation::validate_world(&world);
    assert!(checked.valid, "{:?}", checked.diagnostics);
    world
}

fn search(world: &World, kind: &str, query: &str) -> Json {
    let request = json!({"protocol":"wedl-core-catalogue/v1","operation":"page",
        "fingerprint":world.fingerprint,"at":{"timeline":"main","tick":"10","order":"0"},
        "lens":{"kind":"character","characterId":VIEWER,"sceneId":SCENE},
        "filters":{"kinds":[kind],"profiles":[],"eras":[],"name":""},"sort":"relevance",
        "q":query,"searchMode":"lexical","threadIds":[],"limit":100,"cursor":null});
    catalogue::read(world, &catalogue::decode(&request.to_string()).unwrap()).unwrap()
}

#[test]
fn character_observation_search_cites_its_real_source_field_not_a_description_section() {
    let world = fixture();
    let result = search(&world, "scene", "saffron");
    assert_eq!(result["total"], 1);
    let matched = &result["items"][0]["match"];
    assert_eq!(matched["documentKind"], "observation");
    let citation = &matched["citation"];
    assert_eq!(citation["target"], json!({"kind":"record","id":SCENE}));
    assert_eq!(citation["field"], "observations[1]");
    assert_eq!(citation["path"], world.records[SCENE].source_path);
    assert_eq!(citation["fingerprint"], world.fingerprint);
    let description = search(&world, "location", "turquoise");
    assert_eq!(description["total"], 1);
    let matched = &description["items"][0]["match"];
    assert_eq!(matched["documentKind"], "description");
    assert_eq!(
        matched["citation"]["target"],
        json!({"kind":"section","ownerId":PLACE,"id":DESCRIPTION})
    );
    assert!(matched["citation"].get("field").is_none());
}

#[test]
fn character_conversation_search_cites_each_real_beat_and_recollection_field() {
    let world = fixture();
    for (query, kind, field) in [
        ("crimson", "speech", "turns[1]"),
        ("indigo", "action", "turns[2]"),
        ("viridian", "recollection", "recollections[1]"),
    ] {
        let result = search(&world, "conversation", query);
        assert_eq!(result["total"], 1, "{query}");
        let matched = &result["items"][0]["match"];
        assert_eq!(matched["documentKind"], kind);
        assert_eq!(
            matched["citation"]["target"],
            json!({"kind":"record","id":CONVERSATION})
        );
        assert_eq!(matched["citation"]["field"], field);
    }
}
