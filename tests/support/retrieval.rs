#![allow(dead_code)]
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{fs, path::PathBuf};
use wedl::{
    model::{Record, StoryTime, World},
    source, validation,
};

pub const WORLD: &str = "world_00000000000000000000000000";
pub const VIEWER: &str = "char_00000000000000000000000001";
pub const OTHER: &str = "char_00000000000000000000000002";
pub const PLACE: &str = "loc_00000000000000000000000001";
pub const TIMED_PLACE: &str = "loc_00000000000000000000000002";
pub const OBJECT: &str = "obj_00000000000000000000000001";
pub const PRIVATE_OBJECT: &str = "obj_00000000000000000000000002";
pub const SCENE: &str = "scene_00000000000000000000000001";
pub const CONVERSATION: &str = "conv_00000000000000000000000001";
pub const KNOWLEDGE: &str = "know_00000000000000000000000001";
pub const PRIVATE_KNOWLEDGE: &str = "know_00000000000000000000000002";
pub const FUTURE_KNOWLEDGE: &str = "know_00000000000000000000000003";
pub const LINKED_KNOWLEDGE: &str = "know_00000000000000000000000004";
pub const LINKED_ASSERTION: &str = "assertion_00000000000000000000000001";
pub const TICK: i64 = 9_007_199_254_740_993;

pub fn id(kind: &str, n: usize) -> String {
    format!("{kind}_{n:026}")
}
pub fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
pub fn horizon() -> StoryTime {
    at(TICK, 0)
}
pub fn point(tick: i64, order: i32) -> Json {
    json!({"timeline":"main","tick":tick,"order":order})
}
pub fn interval(start: i64, end: i64) -> Json {
    json!({"kind":"interval","timeline":"main","start":point(start,0),"end":point(end,0)})
}
pub fn section(n: usize, heading: &str, markdown: &str) -> Json {
    json!({"id":id("description",n),"heading":heading,"markdown":markdown,"valid":{"kind":"timeless"},
        "audience":["author","public"],"perspectives":["author","ordinary"]})
}
pub fn record(kind: &str, identifier: &str, fields: Json) -> Record {
    let mut data = json!({"schema":"wedl/v1","kind":kind,"id":identifier,"title":format!("AUTHOR_TITLE_{identifier}")});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let raw = format!(
        "---\n{}---\nAUTHOR_BODY_PRIVATE ferry vessel messenger\n",
        serde_yaml::to_string(&data).unwrap()
    );
    source::parse_record(raw.as_bytes(), &format!("story/{identifier}.md")).unwrap()
}
pub fn from_records(records: Vec<Record>) -> World {
    source::world_from_records(PathBuf::new(), records, None).unwrap()
}
pub fn valid(world: &World) {
    let report = validation::validate_world(world);
    assert!(
        report.valid,
        "{:?}",
        report
            .diagnostics
            .iter()
            .filter(|d| d.severity == "error")
            .collect::<Vec<_>>()
    );
}
pub fn edit(world: &World, identifier: &str, change: impl FnOnce(&mut Yaml)) -> World {
    let old = &world.records[identifier];
    let mut fields = old.frontmatter.clone();
    change(&mut fields);
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&fields).unwrap(),
        old.body
    );
    let replacement = source::parse_record(raw.as_bytes(), &old.source_path).unwrap();
    from_records(
        world
            .records
            .values()
            .map(|r| {
                if r.id == identifier {
                    replacement.clone()
                } else {
                    r.clone()
                }
            })
            .collect(),
    )
}
pub fn set(world: &World, identifier: &str, field: &str, value: Json) -> World {
    edit(world, identifier, |data| {
        data[field] = serde_yaml::to_value(value).unwrap()
    })
}
pub fn add(world: &World, record: Record) -> World {
    let mut records = world.records.values().cloned().collect::<Vec<_>>();
    records.push(record);
    from_records(records)
}
pub fn save(world: &World) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    for record in world.records.values() {
        let path = directory.path().join(&record.source_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, &record.raw_bytes).unwrap();
    }
    directory
}
pub fn without_prior(world: &World) -> World {
    let records = world
        .records
        .values()
        .map(|record| {
            if !matches!(record.kind.as_str(), "world" | "location" | "object") {
                return record.clone();
            }
            let mut fields = record.frontmatter.clone();
            fields.as_mapping_mut().unwrap().remove("descriptions");
            let raw = format!(
                "---\n{}---\n{}",
                serde_yaml::to_string(&fields).unwrap(),
                record.body
            );
            source::parse_record(raw.as_bytes(), &record.source_path).unwrap()
        })
        .collect();
    from_records(records)
}
pub fn private_edit(world: &World) -> World {
    let mut records = Vec::new();
    for owner in world.records.values() {
        let mut data = owner.frontmatter.clone();
        data["title"] = serde_yaml::to_value("CHANGED_PRIVATE_TITLE vessel ferry").unwrap();
        data["aliases"] = serde_yaml::to_value(vec!["PRIVATE_ALIAS"]).unwrap();
        if [PRIVATE_KNOWLEDGE, FUTURE_KNOWLEDGE].contains(&owner.id.as_str()) {
            data["claim"]["statement"] =
                serde_yaml::to_value("PRIVATE_CHANGED ferry vessel repeated").unwrap();
        }
        if owner.id == LINKED_ASSERTION {
            data["value"] = serde_yaml::to_value("PRIVATE_CHANGED_CANON ferry vessel").unwrap();
            data["classification"] = serde_yaml::to_value("unresolved").unwrap();
        }
        if [WORLD, VIEWER, PRIVATE_OBJECT, TIMED_PLACE].contains(&owner.id.as_str()) {
            if let Some(sections) = data.get_mut("descriptions").and_then(Yaml::as_sequence_mut) {
                for section in sections {
                    let eligible = section
                        .get("valid")
                        .and_then(|v| v.get("kind"))
                        .and_then(Yaml::as_str)
                        == Some("timeless");
                    let public = section
                        .get("audience")
                        .and_then(Yaml::as_sequence)
                        .is_some_and(|a| a.iter().any(|v| v.as_str() == Some("public")));
                    if owner.id == PRIVATE_OBJECT || owner.id == TIMED_PLACE || !public || !eligible
                    {
                        // The viewer's currently admitted dated section is real
                        // evidence; only its future section changes here.
                        if owner.id == VIEWER
                            && section.get("id").and_then(Yaml::as_str)
                                == Some(id("description", 20).as_str())
                        {
                            continue;
                        }
                        section["markdown"] =
                            serde_yaml::to_value("PRIVATE_SECTION_CHANGED ferry vessel").unwrap();
                    }
                }
            }
        }
        if owner.id == SCENE {
            data["observations"][1]["text"] =
                serde_yaml::to_value("PRIVATE_OBSERVATION_CHANGED vessel ferry").unwrap();
            data["observations"][2]["text"] =
                serde_yaml::to_value("FUTURE_OBSERVATION_CHANGED vessel ferry").unwrap();
        }
        if owner.id == CONVERSATION {
            data["turns"][2]["text"] =
                serde_yaml::to_value("PRIVATE_BEAT_CHANGED vessel ferry").unwrap();
            data["turns"][3]["text"] =
                serde_yaml::to_value("FUTURE_BEAT_CHANGED vessel ferry").unwrap();
            data["recollections"][1]["summary"] =
                serde_yaml::to_value("PRIVATE_MEMORY_CHANGED vessel ferry").unwrap();
        }
        let raw = format!(
            "---\n{}---\nCHANGED_PRIVATE_BODY vessel ferry\n",
            serde_yaml::to_string(&data).unwrap()
        );
        records.push(
            source::parse_record(
                raw.as_bytes(),
                &format!("story/private-path-{}.md", owner.id),
            )
            .unwrap(),
        );
    }
    let world = from_records(records);
    valid(&world);
    world
}
pub fn world() -> World {
    let mut private = section(10, "Private words", "PRIVATE_PRIOR ferry vessel");
    private["audience"] = json!(["author"]);
    let mut future = section(11, "Later words", "FUTURE_PRIOR ferry vessel");
    future["valid"] = interval(TICK + 1, TICK + 10);
    let mut dated = section(12, "Dated words", "DATED_PRIOR ferry vessel");
    dated["valid"] = interval(TICK - 10, TICK + 10);
    let mut character = section(20, "Ferry habit", "I wait for the ferry beside the quay.");
    character["valid"] = interval(TICK - 10, TICK + 10);
    let mut character_future = section(
        21,
        "Future character",
        "FUTURE_CHARACTER_SECTION ferry vessel",
    );
    character_future["valid"] = interval(TICK + 1, TICK + 10);
    let participants = json!([{"character":VIEWER},{"character":OTHER}]);
    let transition = |id: &str, tick: i64, state: &str| json!({"id":id,"time":point(tick,0),"state":state,"confidence":0.75,"acquisition":"heard","note":"AUTHOR_ACQUISITION_NOTE"});
    let records = vec![
        record(
            "world",
            WORLD,
            json!({"default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"branch","label":"Branch"}],
            "state_keys":{},"capabilities":["lore-core-v1","descriptions-v1","assertions-v1"],"profiles":[],
            "predicates":[{"id":"report","subject_kinds":["character"],"value_type":{"type":"string"}}],
            "descriptions":[section(1,"River routes","A vessel is a ferry boat carrying travellers across the river."),
                section(2,"Boat service","A ferry vessel carries passengers across water near the quay."),
                section(3,"Messages","A courier messenger delivers a letter and carries a parcel."),private,future,dated]}),
        ),
        record(
            "character",
            VIEWER,
            json!({"initial_state":{},"descriptions":[character,character_future]}),
        ),
        record("character", OTHER, json!({"initial_state":{}})),
        record(
            "location",
            PLACE,
            json!({"descriptions":[
            section(4,"Crossings","A boat vessel crosses a river while a passenger waits at the quay."),
            section(5,"Watch","A guard sentry watches the gate at night."),
            section(6,"Signals","A bell rings and chimes to signal an alarm.")]}),
        ),
        record(
            "object",
            OBJECT,
            json!({"descriptions":[section(7,"Recollection","A memory recollection remembers a spoken warning."),
            section(8,"Delivery","A messenger courier carries a message by ferry boat.")]}),
        ),
        record(
            "object",
            PRIVATE_OBJECT,
            json!({"audience":["author"],"descriptions":[section(9,"Private prior","PRIVATE_OWNER_PRIOR vessel ferry")]}),
        ),
        record(
            "location",
            TIMED_PLACE,
            json!({"time":{"start":point(TICK+1,0),"end":point(TICK+10,0)},
            "descriptions":[section(13,"Qualified owner","FUTURE_OWNER_PRIOR vessel ferry")]}),
        ),
        record(
            "knowledge",
            KNOWLEDGE,
            json!({"knower":VIEWER,"claim":{"key":"ordinary-report","statement":"A ferry carries the courier across the river.","truth":"false"},
            "transitions":[transition("heard",TICK-4,"accepted")]}),
        ),
        record(
            "knowledge",
            PRIVATE_KNOWLEDGE,
            json!({"knower":OTHER,"claim":{"key":"private","statement":"PRIVATE_KNOWLEDGE vessel ferry"},"transitions":[transition("private",TICK-3,"accepted")]}),
        ),
        record(
            "knowledge",
            FUTURE_KNOWLEDGE,
            json!({"knower":VIEWER,"claim":{"key":"future","statement":"FUTURE_KNOWLEDGE vessel ferry"},"transitions":[transition("future",TICK+1,"accepted")]}),
        ),
        record(
            "assertion",
            LINKED_ASSERTION,
            json!({"domain":"retrieval.test","subject":{"kind":"record","id":VIEWER},"predicate":"report", "value":"AUTHOR_CANON_VALUE","classification":"author-fact",
            "valid":{"kind":"timeless"},"evidence":[],"audience":["author"],"perspectives":["author"]}),
        ),
        record(
            "knowledge",
            LINKED_KNOWLEDGE,
            json!({"knower":VIEWER,"claim":{"key":"linked-report","statement":"A messenger promised the ferry sails at night.","assertion_id":LINKED_ASSERTION},
            "transitions":[transition("learned",TICK-3,"accepted"),transition("doubted",TICK-1,"rejected"),transition("forgotten",TICK+2,"forgotten")]}),
        ),
        record(
            "scene",
            SCENE,
            json!({"status":"closed","location":PLACE,"time":{"start":point(TICK-5,0),"current":point(TICK,0),"end":point(TICK+5,0)},"participants":participants,
            "observations":[{"id":"bell","at":point(TICK-1,0),"until":point(TICK,0),"audience":["participants"],"text":"The ferry bell rings near the gate."},
                {"id":"private","at":point(TICK-1,0),"audience":[OTHER],"text":"PRIVATE_OBSERVATION vessel ferry"},
                {"id":"future","at":point(TICK+1,0),"audience":["participants"],"text":"FUTURE_OBSERVATION vessel ferry"}]}),
        ),
        record(
            "conversation",
            CONVERSATION,
            json!({"status":"closed","location":PLACE,"scene":SCENE,"time":{"start":point(TICK-5,0),"end":point(TICK+5,0)},"participants":participants,
            "turns":[{"id":"heard","kind":"speech","at":point(TICK-2,0),"speaker":OTHER,"audience":["participants"],"text":"The ferry crosses the river at night."},
                {"id":"wave","kind":"action","at":point(TICK-1,0),"actors":[OTHER],"audience":["participants"],"text":"The courier waves a letter."},
                {"id":"private","kind":"speech","at":point(TICK-1,1),"speaker":OTHER,"audience":[OTHER],"text":"PRIVATE_BEAT vessel ferry"},
                {"id":"future","kind":"speech","at":point(TICK+1,0),"speaker":OTHER,"audience":["participants"],"text":"FUTURE_BEAT vessel ferry"}],
            "recollections":[{"id":"remembered","character":VIEWER,"at":point(TICK-1,2),"state":"remembered","confidence":0.5,
                "summary":"I remember the ferry crossing.","interpretation":"The messenger may return.","emotional_impression":"Cautious about the river.",
                "remembered_quotes":[{"source_turn":"private","speaker":OTHER,"text":"I recall a quiet promise.","fidelity":"approximate"}]},
                {"id":"private-memory","character":OTHER,"at":point(TICK-1,2),"state":"remembered","confidence":1.0,"summary":"PRIVATE_MEMORY vessel ferry"}]}),
        ),
    ];
    let world = from_records(records);
    valid(&world);
    world
}
