#![allow(dead_code)]
use serde_json::{Value as Json, json};
use std::{fs, path::PathBuf};
use wedl::{
    action_drafts::{self, Draft, SaveRequest},
    model::{StoryTime, World},
    source, validation, writing_frames,
};

pub const ACTOR: &str = "char_00000000000000000000000001";
pub const PLACE: &str = "loc_00000000000000000000000001";
pub const SCENE: &str = "scene_00000000000000000000000001";
pub const SECTION: &str = "description_00000000000000000000000001";
pub const TICK: i64 = 9_007_199_254_740_993;

pub fn id(n: usize) -> String {
    format!("action_{n:026}")
}

pub fn point(tick: i64, order: i32) -> Json {
    json!({"timeline":"main","tick":tick,"order":order})
}

pub fn horizon() -> StoryTime {
    StoryTime::new("main", TICK, 1).unwrap()
}

fn record(root: &std::path::Path, kind: &str, id: &str, fields: Json, body: &str) {
    let mut data =
        json!({"schema":"wedl/v1","kind":kind,"id":id,"title":format!("Fixture {kind}")});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let source = format!("---\n{}---\n{body}", serde_yaml::to_string(&data).unwrap());
    fs::write(root.join("story").join(format!("{id}.md")), source).unwrap();
}

/// Small on-disk source with an explicitly present actor and an exact time above
/// binary64's integer range. Creating the fixture never initializes a cache.
pub fn fixture() -> (tempfile::TempDir, World) {
    let directory = tempfile::tempdir().unwrap();
    let world =
        source::init_world(&directory.path().join("world"), "Private actions fixture").unwrap();
    let owner = world.world_record().unwrap();
    let mut fields = owner.frontmatter.clone();
    fields["capabilities"] = serde_yaml::to_value(["descriptions-v1"]).unwrap();
    fs::write(
        world.root.join(&owner.source_path),
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&fields).unwrap(),
            owner.body
        ),
    )
    .unwrap();
    record(
        &world.root,
        "character",
        ACTOR,
        json!({"initial_state":{},"descriptions":[{"id":SECTION,"heading":"Known context",
            "markdown":"The actor has an explicit description.","valid":{"kind":"timeless"},
            "audience":["author","public"],"perspectives":["author","ordinary"]}]}),
        "Author-only actor source.\n",
    );
    record(&world.root, "location", PLACE, json!({}), "A landing.\n");
    record(
        &world.root,
        "scene",
        SCENE,
        json!({"status":"closed","location":PLACE,
            "time":{"start":point(TICK-1,0),"current":point(TICK,1),"end":point(TICK+1,0)},
            "participants":[{"character":ACTOR,"point_of_view":true}]}),
        "An explicit scene at an exact large clock.\n",
    );
    let world = source::load_world(&world.root, None).unwrap();
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    (directory, world)
}

pub fn draft(world: &World, id: &str) -> Draft {
    action_drafts::decode(
        &json!({"id":id,"title":"Retained private reasoning","worldId":world.world_record().unwrap().id,
            "baseFingerprint":world.fingerprint,"horizon":horizon(),"intent":"Ask the ferryman before acting.",
            "actorRefs":[{"kind":"record","id":ACTOR}],
            "evidence":[],"targetRefs":[{"kind":"record","id":PLACE}],"hypothesisRefs":[],
            "questions":[{"id":"question_00000000000000000000000001","text":"Is there enough context?","required":true}],
            "proposals":[],"phase":"draft","bodyMarkdown":"\r\nPrivate needle 🌲\r\n\nTrailing space  "})
        .to_string(),
    )
    .unwrap()
}

pub fn request(world: &World, draft: Draft, expected_sha256: Option<&str>) -> SaveRequest {
    action_drafts::decode_save(
        &json!({"expectedFingerprint":world.fingerprint,"expectedSha256":expected_sha256,
            "draft":draft,"review":null})
        .to_string(),
    )
    .unwrap()
}

pub fn frame(world: &World, lens: &str) -> writing_frames::Frame {
    writing_frames::decode(
        &json!({"fingerprint":world.fingerprint,"work":null,"sceneId":SCENE,"povId":ACTOR,"at":horizon(),"lens":lens})
            .to_string(),
    )
    .unwrap()
}

pub fn path(world: &World, id: &str) -> PathBuf {
    world.root.join("author/actions").join(format!("{id}.json"))
}

pub fn source_image(world: &World) -> Vec<(String, Vec<u8>)> {
    world
        .records
        .values()
        .map(|record| {
            (
                record.source_path.clone(),
                fs::read(world.root.join(&record.source_path)).unwrap(),
            )
        })
        .collect()
}

pub fn change_source(world: &World) -> World {
    let record = world.world_record().unwrap();
    let mut bytes = fs::read(world.root.join(&record.source_path)).unwrap();
    bytes.extend_from_slice(b"\nExplicit new source context.\n");
    fs::write(world.root.join(&record.source_path), bytes).unwrap();
    source::load_world(&world.root, None).unwrap()
}
