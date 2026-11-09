//! Exercise the supplied requests through the executable, then query their evidence.
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

use serde_json::Value;
use tempfile::TempDir;
use wedl::source;

const PERSON: &str = "char_00000000000000000000000001";
const SCENE: &str = "scene_00000000000000000000000001";
const CONVERSATION: &str = "conv_00000000000000000000000001";
const FLOW: [(&str, Option<&str>); 18] = [
    ("01-location.yaml", None),
    ("02-character.yaml", None),
    ("03-object.yaml", None),
    ("04-event.yaml", None),
    ("05-knowledge.yaml", None),
    ("06-character-update.yaml", Some(PERSON)),
    ("07-scene.yaml", None),
    ("08-conversation.yaml", None),
    ("09-scene-close.yaml", Some(SCENE)),
    ("10-conversation-close.yaml", Some(CONVERSATION)),
    ("11-landing.yaml", None),
    ("12-route.yaml", None),
    ("13-route-close.yaml", Some("route:harbor/steps")),
    ("14-route-reopen.yaml", Some("route:harbor/steps")),
    ("15-map.yaml", None),
    (
        "16-harbor-placement.yaml",
        Some("loc_00000000000000000000000001"),
    ),
    ("17-landing-placement.yaml", Some("location:harbor/landing")),
    ("18-map-bounds.yaml", Some("map:harbor")),
];

struct Fixture {
    _directory: TempDir,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("authoring world");
        source::init_world(&root, "Scene authoring").unwrap();
        Self {
            _directory: directory,
            root,
        }
    }

    fn call(&self, args: &[&str], input: &str) -> (i32, Value) {
        let mut child = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
            .args(["--compact", "--repo", self.root.to_str().unwrap()])
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        let bytes = if output.stdout.is_empty() {
            &output.stderr
        } else {
            &output.stdout
        };
        let value = serde_json::from_slice(bytes)
            .unwrap_or_else(|_| panic!("non-JSON output: {}", String::from_utf8_lossy(bytes)));
        (output.status.code().unwrap(), value)
    }

    fn fingerprint(&self) -> String {
        source::load_world(&self.root, None).unwrap().fingerprint
    }

    fn preview(&self, target: Option<&str>, request: &str) -> (Vec<String>, Value) {
        let fingerprint = self.fingerprint();
        let mut args = vec![
            "record".into(),
            if target.is_some() { "update" } else { "create" }.into(),
        ];
        if let Some(target) = target {
            args.push(target.into());
        }
        args.extend([
            "--file".into(),
            "-".into(),
            "--expected-fingerprint".into(),
            fingerprint.clone(),
        ]);
        let (code, value) = self.call(
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
            request,
        );
        assert_eq!(code, 0, "{value}");
        assert_eq!(self.fingerprint(), fingerprint, "preview wrote source");
        assert_eq!(value["protocol"], wedl::CONTRACT);
        assert_eq!(value["result"]["writesPerformed"], false);
        (args, value["result"].clone())
    }

    fn apply(&self, mut args: Vec<String>, preview: &Value, request: &str) -> (i32, Value) {
        args.extend([
            "--apply".into(),
            "--expected-after-sha256".into(),
            preview["afterSha256"].as_str().unwrap().into(),
        ]);
        self.call(
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
            request,
        )
    }

    fn example(&self, index: usize) {
        let (name, target) = FLOW[index];
        let request = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples/authoring")
                .join(name),
        )
        .unwrap();
        let (args, preview) = self.preview(target, &request);
        assert_eq!(preview["canApply"], true, "{name}: {preview}");
        let (code, saved) = self.apply(args, &preview, &request);
        assert_eq!(code, 0, "{name}: {saved}");
        let path = preview["sourcePath"].as_str().unwrap();
        assert_eq!(
            std::fs::read_to_string(self.root.join(path)).unwrap(),
            preview["afterSource"].as_str().unwrap()
        );
    }

    fn read(&self, args: &[&str]) -> Value {
        let (code, value) = self.call(args, "");
        assert_eq!(code, 0, "{value}");
        value["result"].clone()
    }
}

#[test]
fn ten_supplied_requests_create_records_and_expose_only_dated_evidence() {
    let fixture = Fixture::new();
    for index in 0..8 {
        fixture.example(index);
    }
    let world = source::load_world(&fixture.root, None).unwrap();
    assert_eq!(world.records[SCENE].status(), "planned");
    assert_eq!(world.records[CONVERSATION].status(), "draft");
    let scene_body = world.records[SCENE].body.clone();
    let conversation_body = world.records[CONVERSATION].body.clone();
    let before = fixture.read(&["character", "context", PERSON, "--tick", "3"]);
    assert!(before["observations"].as_array().unwrap().is_empty());
    assert!(before["conversations"].as_array().unwrap().is_empty());

    fixture.example(8);
    fixture.example(9);
    let world = source::load_world(&fixture.root, None).unwrap();
    assert_eq!(world.records.len(), 8);
    for id in [SCENE, CONVERSATION] {
        assert_eq!(world.records[id].status(), "closed");
        assert_eq!(
            world.records[id].string("schema"),
            Some(source::SOURCE_SCHEMA)
        );
    }
    assert_eq!(world.records[SCENE].body, scene_body);
    assert_eq!(world.records[CONVERSATION].body, conversation_body);
    assert_eq!(fixture.read(&["validate"])["valid"], true);

    let scene = fixture.read(&["scene", SCENE, "--tick", "2", "--order", "0"]);
    assert_eq!(scene["presentParticipants"].as_array().unwrap().len(), 1);
    assert_eq!(scene["observations"].as_array().unwrap().len(), 1);
    assert_eq!(scene["conversations"][0]["conversationId"], CONVERSATION);
    let before_speech = fixture.read(&[
        "character",
        "context",
        PERSON,
        "--tick",
        "1",
        "--order",
        "1",
    ]);
    assert_eq!(before_speech["beliefs"].as_array().unwrap().len(), 1);
    assert_eq!(before_speech["observations"].as_array().unwrap().len(), 1);
    assert!(
        before_speech["conversations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let transcript = fixture.read(&[
        "character",
        "conversation",
        PERSON,
        CONVERSATION,
        "--tick",
        "2",
        "--order",
        "0",
    ]);
    assert_eq!(transcript["beats"].as_array().unwrap().len(), 2);
    assert!(transcript["recollection"].is_null());
    let remembered = fixture.read(&[
        "character",
        "conversation",
        PERSON,
        CONVERSATION,
        "--tick",
        "3",
        "--order",
        "0",
    ]);
    assert_eq!(
        remembered["recollection"]["recollectionId"],
        "landing-memory"
    );
    assert_eq!(
        remembered["recollection"]["exactTurns"],
        serde_json::json!(["landing-report"])
    );
    assert_eq!(
        fixture.read(&["character", "search", PERSON, "lantern", "--tick", "1"])["total"],
        0
    );
    assert_eq!(
        fixture.read(&["character", "search", PERSON, "lantern", "--tick", "2"])["total"],
        1
    );
}

#[test]
fn invalid_status_time_and_reference_edits_never_publish_source() {
    let fixture = Fixture::new();
    for index in 0..10 {
        fixture.example(index);
    }
    let fingerprint = fixture.fingerprint();
    for (target, request) in [
        (SCENE, "kind: scene\nstatus: active\n"),
        (
            SCENE,
            "kind: scene\ntime: {start: {tick: 1}, current: {tick: 2}}\n",
        ),
        (CONVERSATION, "kind: conversation\nturns: []\n"),
        (
            CONVERSATION,
            "kind: conversation\nlocation: loc_00000000000000000000000099\n",
        ),
    ] {
        let (args, preview) = fixture.preview(Some(target), request);
        assert_eq!(preview["canApply"], false, "{request}: {preview}");
        assert_eq!(preview["validation"]["valid"], false);
        let (code, error) = fixture.apply(args, &preview, request);
        assert_eq!(code, 2, "{error}");
        assert_eq!(error["code"], "validation_error");
        assert_eq!(fixture.fingerprint(), fingerprint);
    }
    for (target, request) in [
        (SCENE, "kind: scene\nstatus: canonical\n".to_owned()),
        (
            CONVERSATION,
            format!(
                "kind: conversation\nparticipants: [{{character: {PERSON}, to: {{tick: 2}}}}]\n"
            ),
        ),
    ] {
        let (code, error) = fixture.call(
            &[
                "record",
                "update",
                target,
                "--file",
                "-",
                "--expected-fingerprint",
                &fingerprint,
            ],
            &request,
        );
        assert_eq!(code, 2, "{error}");
        assert_eq!(error["code"], "usage_error");
        assert_eq!(fixture.fingerprint(), fingerprint);
    }
}

#[test]
fn spatial_examples_require_declared_capabilities_and_update_directed_paths() {
    let fixture = Fixture::new();
    for index in 0..11 {
        fixture.example(index);
    }
    let route_request = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/authoring/12-route.yaml"),
    )
    .unwrap();
    let fingerprint = fixture.fingerprint();
    let (args, preview) = fixture.preview(None, &route_request);
    assert_eq!(preview["canApply"], false);
    assert!(
        preview["validation"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["code"] == "NAR-CAPABILITY-001")
    );
    let (code, error) = fixture.apply(args, &preview, &route_request);
    assert_eq!(code, 2, "{error}");
    assert_eq!(error["code"], "validation_error");
    assert_eq!(fixture.fingerprint(), fingerprint);

    // World declarations currently use a direct source edit, as documented.
    let world = source::load_world(&fixture.root, None).unwrap();
    let mut owner = world.world_record().unwrap().clone();
    owner.frontmatter["capabilities"] =
        serde_yaml::from_str("[spatial-core-v1, route-v1]").unwrap();
    std::fs::write(
        fixture.root.join(&owner.source_path),
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&owner.frontmatter).unwrap(),
            owner.body
        ),
    )
    .unwrap();
    fixture.example(11);
    let world = source::load_world(&fixture.root, None).unwrap();
    assert_eq!(world.records.len(), 10);
    let route_body = world.records["route:harbor/steps"].body.clone();

    let landing = fixture.read(&[
        "spatial",
        "location",
        "location:harbor/landing",
        "--mode",
        "foot",
    ]);
    assert_eq!(landing["parentId"], "loc_00000000000000000000000001");
    assert_eq!(landing["ancestors"].as_array().unwrap().len(), 1);
    let harbor = fixture.read(&["spatial", "location", "Harbor", "--mode", "foot"]);
    assert_eq!(
        harbor["children"][0]["locationId"],
        "location:harbor/landing"
    );
    for (from, to) in [("Harbor", "Lower Landing"), ("Lower Landing", "Harbor")] {
        let path = fixture.read(&["spatial", "path", from, to, "--mode", "foot"]);
        assert_eq!(path["found"], true);
        assert_eq!(path["hops"], 1);
        assert_eq!(path["edges"][0]["routeDistance"]["value"], "30");
        assert_eq!(path["edges"][0]["duration"]["value"], "45");
    }
    assert_eq!(
        fixture.read(&[
            "spatial",
            "path",
            "Harbor",
            "Lower Landing",
            "--mode",
            "boat"
        ])["found"],
        false
    );
    fixture.example(12);
    assert_eq!(
        fixture.read(&["spatial", "path", "Harbor", "Lower Landing"])["found"],
        false
    );
    fixture.example(13);
    assert_eq!(
        fixture.read(&["spatial", "path", "Harbor", "Lower Landing"])["found"],
        true
    );
    assert_eq!(
        fixture.read(&["spatial", "path", "Lower Landing", "Harbor"])["found"],
        false
    );
    let world = source::load_world(&fixture.root, None).unwrap();
    assert_eq!(world.records["route:harbor/steps"].body, route_body);
    assert_eq!(fixture.read(&["validate"])["valid"], true);
}

#[test]
fn geometry_examples_author_maps_and_points_without_inventing_route_metrics() {
    let fixture = Fixture::new();
    let world_path = fixture.root.join("story/world.md");
    let world_source = std::fs::read_to_string(&world_path).unwrap();
    std::fs::write(
        &world_path,
        world_source.replace(
            "capabilities: []",
            "capabilities: [spatial-core-v1, geometry-v1, route-v1]",
        ),
    )
    .unwrap();
    for index in 0..FLOW.len() {
        fixture.example(index);
    }
    let world = source::load_world(&fixture.root, None).unwrap();
    assert_eq!(world.records.len(), 11);
    assert_eq!(fixture.read(&["validate"])["valid"], true);
    let maps = fixture.read(&["spatial", "maps"]);
    assert_eq!(maps["items"].as_array().unwrap().len(), 1);
    assert_eq!(maps["items"][0]["mapId"], "map:harbor");
    assert_eq!(maps["items"][0]["unit"], "m");

    let rectangle = fixture.read(&[
        "spatial",
        "bbox",
        "Harbor Map",
        "--min-x",
        "-1",
        "--min-y",
        "-1",
        "--max-x",
        "3",
        "--max-y",
        "4",
    ]);
    assert_eq!(rectangle["items"].as_array().unwrap().len(), 2);
    let near = fixture.read(&[
        "spatial",
        "nearby",
        "map:harbor",
        "--x",
        "0",
        "--y",
        "0",
        "--radius",
        "5",
    ]);
    assert_eq!(near["items"].as_array().unwrap().len(), 2);
    let landing = near["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["locationId"] == "location:harbor/landing")
        .unwrap();
    assert_eq!(landing["distance"], 5.0);
    let small = fixture.read(&[
        "spatial",
        "nearby",
        "map:harbor",
        "--x",
        "0",
        "--y",
        "0",
        "--radius",
        "4.9",
    ]);
    assert_eq!(small["items"].as_array().unwrap().len(), 1);

    for (metric, unit, expected) in [("route_distance", "m", "30"), ("duration", "seconds", "45")] {
        let path = fixture.read(&[
            "spatial",
            "path",
            "Harbor",
            "Lower Landing",
            "--metric",
            metric,
            "--unit",
            unit,
        ]);
        assert_eq!(path["found"], true);
        assert_eq!(path["total"]["value"], expected);
        assert_eq!(path["total"]["unit"], unit);
    }
    let missing = fixture.read(&[
        "spatial",
        "path",
        "Harbor",
        "Lower Landing",
        "--metric",
        "travel_cost",
        "--unit",
        "coin",
    ]);
    assert_eq!(missing["found"], false);
    assert!(missing["total"].is_null());
    assert!(
        world.records["map:harbor"]
            .body
            .contains("Authored point coordinates")
    );
}
