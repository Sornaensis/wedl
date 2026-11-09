#![allow(dead_code)]
//! Small shared source for coordinated scene, presence and movement tests.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeMap, fs, path::Path, time::SystemTime};
use wedl::{
    model::{StoryTime, World},
    source, validation,
};

pub const WORLD: &str = "world_00000000000000000000000000";
pub const ORIGIN: &str = "loc_00000000000000000000000001";
pub const DESTINATION: &str = "loc_00000000000000000000000002";
pub const ACTOR: &str = "char_00000000000000000000000001";
pub const OTHER_ACTOR: &str = "char_00000000000000000000000002";
pub const SCENE: &str = "scene_00000000000000000000000001";
pub const OTHER_SCENE: &str = "scene_00000000000000000000000002";
pub const BAG: &str = "obj_00000000000000000000000001";
pub const CONTAINED: &str = "obj_00000000000000000000000002";
pub const PROP: &str = "obj_00000000000000000000000003";
pub const TICK: i64 = 9_007_199_254_740_993;
pub const ORDER: i32 = 7;

/// Authored YAML has numeric coordinates; StoryTime serializes wire strings.
pub fn point(tick: i64, order: i32) -> Json {
    json!({"timeline":"main","tick":tick,"order":order})
}

pub fn at(order: i32) -> StoryTime {
    StoryTime::new("main", TICK, order).unwrap()
}

pub fn horizon() -> StoryTime {
    at(ORDER)
}

fn write_record(root: &Path, kind: &str, id: &str, fields: Json, body: &str) {
    let mut data = json!({"schema":"wedl/v1","kind":kind,"id":id,
        "title":format!("Scene control {kind} {id}"),"status":"canonical",
        "x-author":{"literal":null,"numbers":[1,1.0,"1"]}});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let header = format!(
        "---\n# Preserve this comment on unchanged records.\n{}---\n",
        serde_yaml::to_string(&data).unwrap()
    )
    .replace('\n', "\r\n");
    fs::write(
        root.join("story").join(format!("{id}.md")),
        format!("{header}{body}"),
    )
    .unwrap();
}

/// Two open active scenes share the exact current point and have distinct cast.
/// The first scene lists a held bag, its contained object and an unrelated prop.
/// Placement links are typed, independently declared fields, not exclusive keys.
/// No compiled or private cache is initialized.
pub fn fixture() -> (tempfile::TempDir, World) {
    let directory = tempfile::tempdir().unwrap();
    let initial = source::init_world(
        &directory.path().join("scene operations world"),
        "Coordinated scene controls",
    )
    .unwrap();
    let owner = initial.world_record().unwrap();
    let mut fields = owner.frontmatter.clone();
    fields["id"] = WORLD.into();
    fields["current_time"] = serde_yaml::to_value(point(TICK, ORDER)).unwrap();
    fields["timelines"] = serde_yaml::to_value(json!([
        {"id":"main","label":"Main"},
        {"id":"other","label":"Other"}
    ]))
    .unwrap();
    fields["state_keys"] = serde_yaml::to_value(json!({
        "character":{"location":{"type":"entity","kind":"location"}},
        "object":{
            "holder":{"type":"entity","kind":"character"},
            "container":{"type":"entity","kind":"object"},
            "location":{"type":"entity","kind":"location"}
        }
    }))
    .unwrap();
    fs::write(
        initial.root.join(&owner.source_path),
        format!(
            "---\n{}---\n\n# Coordinated scene controls\nExact world prose 🌲  \r\n",
            serde_yaml::to_string(&fields).unwrap()
        ),
    )
    .unwrap();
    for (id, body) in [
        (ORIGIN, "\r\n# Dock\r\nOrigin prose  \r\n"),
        (
            DESTINATION,
            "\n# Terrace\nDestination prose without final newline",
        ),
    ] {
        write_record(&initial.root, "location", id, json!({}), body);
    }
    for (id, location, body) in [
        (ACTOR, ORIGIN, "Actor source 🌲  \r\n"),
        (OTHER_ACTOR, DESTINATION, "Other actor source.\n"),
    ] {
        write_record(
            &initial.root,
            "character",
            id,
            json!({"initial_state":{"location":{"entity":location}}}),
            body,
        );
    }
    for (id, state, body) in [
        (
            BAG,
            json!({"holder":{"entity":ACTOR}}),
            "Held bag source.\r\n",
        ),
        (
            CONTAINED,
            json!({"container":{"entity":BAG}}),
            "Contained object source.\n",
        ),
        (
            PROP,
            json!({"location":{"entity":ORIGIN}}),
            "Stationary prop source.\n",
        ),
    ] {
        write_record(
            &initial.root,
            "object",
            id,
            json!({"initial_state":state,"object_affordances":["can-be-seen"]}),
            body,
        );
    }
    for (id, location, actor, objects, body) in [
        (
            SCENE,
            ORIGIN,
            ACTOR,
            vec![BAG, CONTAINED, PROP],
            "\r\n# Dock scene\nRetain exact scene prose 🌲  \r\nNo final newline",
        ),
        (
            OTHER_SCENE,
            DESTINATION,
            OTHER_ACTOR,
            vec![],
            "\n# Terrace scene\nRetain unrelated active scene prose.\r\n",
        ),
    ] {
        write_record(
            &initial.root,
            "scene",
            id,
            json!({"status":"active","location":location,
                "time":{"start":point(TICK-1,0),"current":point(TICK,ORDER)},
                "participants":[{"character":actor,"role":"witness","point_of_view":true}],
                "objects":objects,"conversations":[],"environments":[],
                "outcome_events":[],"observations":[]}),
            body,
        );
    }
    let world = source::load_world(&initial.root, None).unwrap();
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    (directory, world)
}

pub type SourceImage = BTreeMap<String, (Vec<u8>, SystemTime)>;

/// Captures every source/private file, including added paths, with exact mtimes.
pub fn source_image(world: &World) -> SourceImage {
    let mut result = BTreeMap::new();
    for folder in ["story", "author"] {
        let path = world.root.join(folder);
        if !path.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(path) {
            let entry = entry.unwrap();
            if entry.file_type().is_file() {
                result.insert(
                    entry
                        .path()
                        .strip_prefix(&world.root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    (
                        fs::read(entry.path()).unwrap(),
                        fs::metadata(entry.path()).unwrap().modified().unwrap(),
                    ),
                );
            }
        }
    }
    result
}

/// Reconstruct the complete proposed world without publishing any source file.
pub fn candidate(world: &World, report: &Json) -> World {
    let mut records = world.records.clone();
    for file in report["preview"]["files"].as_array().unwrap() {
        let record = source::parse_record(
            file["afterSource"].as_str().unwrap().as_bytes(),
            file["sourcePath"].as_str().unwrap(),
        )
        .unwrap();
        records.insert(record.id.clone(), record);
    }
    source::world_from_records(
        world.root.clone(),
        records.into_values().collect(),
        world.revision.clone(),
    )
    .unwrap()
}

/// Mutate a native snapshot for focused precondition tests, without disk writes.
pub fn changed(world: &World, id: &str, edit: impl FnOnce(&mut Yaml)) -> World {
    let old = &world.records[id];
    let mut fields = old.frontmatter.clone();
    edit(&mut fields);
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&fields).unwrap(),
        old.body
    );
    let record = source::parse_record(raw.as_bytes(), &old.source_path).unwrap();
    let mut records = world.records.clone();
    records.insert(record.id.clone(), record);
    source::world_from_records(
        world.root.clone(),
        records.into_values().collect(),
        world.revision.clone(),
    )
    .unwrap()
}
