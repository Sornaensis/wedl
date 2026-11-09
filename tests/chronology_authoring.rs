//! Calendar declarations use the same reviewed single-record writer as other edits.
use std::{fs, path::Path, process::Command};

use serde_json::{Value, json};
use wedl::{Error, authoring, chronology, model::World, record_edit, source, validation};

const PATCH: &str = include_str!("../examples/authoring/19-world-chronology.yaml");
const HARBOR: &str = "calendar_00000000000000000000000001";
const TIDE: &str = "calendar_00000000000000000000000002";

fn blank() -> (tempfile::TempDir, World) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("world");
    source::init_world(&root, "Chronology authoring").unwrap();
    let world = source::load_world(&root, None).unwrap();
    (temp, world)
}

fn save(world: &World, patch: &str) -> World {
    let id = &world.world_record().unwrap().id;
    let preview = authoring::preview_record_update(world, id, patch, &world.fingerprint).unwrap();
    assert_eq!(preview["canApply"], true, "{preview}");
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint,
        "preview wrote source"
    );
    authoring::apply_record_update(
        world,
        id,
        patch,
        &world.fingerprint,
        preview["afterSha256"].as_str().unwrap(),
    )
    .unwrap();
    let saved = source::load_world(&world.root, None).unwrap();
    assert_eq!(saved.fingerprint, preview["candidateFingerprint"]);
    assert_eq!(
        saved.world_record().unwrap().raw_bytes,
        preview["afterSource"].as_str().unwrap().as_bytes()
    );
    assert!(validation::validate_world(&saved).valid);
    saved
}

#[test]
fn declaration_patch_preserves_other_world_fields_and_enables_queries() {
    let (_temp, world) = blank();
    let saved = save(&world, PATCH);
    let before = world.world_record().unwrap();
    let after = saved.world_record().unwrap();
    assert_eq!(after.id, before.id);
    assert_eq!(after.body, before.body);
    for (key, value) in before.frontmatter.as_mapping().unwrap() {
        assert_eq!(after.frontmatter.get(key), Some(value));
    }
    assert_eq!(saved.records.len(), 1);
    assert_eq!(saved.current_time().unwrap(), world.current_time().unwrap());
    let day = chronology::CivilDate {
        year: 0,
        month: 2,
        day: 1,
    };
    let converted = chronology::convert(&saved, HARBOR, TIDE, day).unwrap();
    assert_eq!(converted["found"], true);
    assert_eq!(converted["axisDay"], "30");
    let anchored = chronology::to_story(&saved, HARBOR, day, None).unwrap();
    assert_eq!(anchored["found"], true);
    assert_eq!(
        anchored["storyTime"],
        json!({"timeline":"main","tick":"1","order":"1"})
    );
    assert_eq!(
        chronology::to_story(
            &saved,
            HARBOR,
            chronology::CivilDate {
                year: 0,
                month: 2,
                day: 2
            },
            None
        )
        .unwrap()["found"],
        false
    );
    // Replacing the top-level declaration never performs a hidden nested merge.
    let cleared = save(
        &saved,
        "kind: world\nchronology: {calendars: [], eras: [], anchors: []}\n",
    );
    assert!(
        chronology::calendars(&cleared, 100).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(cleared.world_record().unwrap().body, before.body);
}

#[test]
fn unchanged_packaged_declarations_preserve_source_bytes_and_null_epochs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/chronology_conformance");
    let world = source::load_world(root, None).unwrap();
    let record = world.world_record().unwrap();
    let mut patch = serde_yaml::Mapping::new();
    patch.insert("kind".into(), "world".into());
    patch.insert(
        "chronology".into(),
        record.get("chronology").unwrap().clone(),
    );
    let patch = serde_yaml::to_string(&patch).unwrap();
    let candidate = record_edit::update(&world, &record.id, &patch).unwrap();
    assert_eq!(candidate.raw_bytes, record.raw_bytes);
    let preview =
        authoring::preview_record_update(&world, &record.id, &patch, &world.fingerprint).unwrap();
    assert_eq!(preview["canApply"], true);
    assert_eq!(preview["changed"], false);
    assert_eq!(preview["candidateFingerprint"], world.fingerprint);
    assert!(
        !preview["validation"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["code"] == "RUST-CHRONOLOGY-ANNOTATIONS-UNSUPPORTED")
    );
}

#[test]
fn invalid_calendar_edits_remain_reviewable_but_cannot_publish() {
    let (_temp, world) = blank();
    let world = save(&world, PATCH);
    let id = &world.world_record().unwrap().id;
    for patch in [
        PATCH.replace("period: 4", "period: 0"),
        PATCH.replace("axis_day: 30", "axis_day: 0"),
        PATCH.replace("timeline: main", "timeline: missing"),
        PATCH.replace("day: 1}, axis_day: 0", "day: 40}, axis_day: 0"),
        PATCH.replace("rule:\n", "unexpected: true\n      rule:\n"),
    ] {
        let preview =
            authoring::preview_record_update(&world, id, &patch, &world.fingerprint).unwrap();
        assert_eq!(preview["canApply"], false, "{preview}");
        assert!(!preview["validation"]["valid"].as_bool().unwrap());
        assert!(matches!(
            authoring::apply_record_update(
                &world,
                id,
                &patch,
                &world.fingerprint,
                preview["afterSha256"].as_str().unwrap()
            ),
            Err(Error::Validation(_))
        ));
        assert_eq!(
            source::load_world(&world.root, None).unwrap().fingerprint,
            world.fingerprint
        );
    }
}

#[test]
fn world_drafts_are_narrow_updates_and_null_does_not_delete_fields() {
    let (_temp, world) = blank();
    let id = &world.world_record().unwrap().id;
    for patch in [
        "kind: world\nchronology: null\n",
        "kind: world\nchronology: []\n",
        "kind: world\nchronology: !tag {}\n",
        "kind: world\nchronology: {x-note: !tag null}\n",
        "kind: world\ntitle: null\n",
        "kind: world\ncapabilities: null\n",
        "kind: world\ncurrent_time: {tick: 100}\n",
        "kind: world\nid: world_00000000000000000000000001\n",
    ] {
        assert!(
            matches!(
                record_edit::update(&world, id, patch),
                Err(Error::InvalidInput(_))
            ),
            "accepted {patch}"
        );
    }
    assert!(matches!(
        record_edit::create(
            &world,
            "kind: world\nid: world_00000000000000000000000001\ntitle: Other\n"
        ),
        Err(Error::InvalidInput(_))
    ));
}

fn cli(root: &Path, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .args(["--compact", "--repo", root.to_str().unwrap()])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn numbered_chronology_example_previews_and_saves_through_cli() {
    let (temp, world) = blank();
    let request = temp.path().join("chronology.yaml");
    fs::write(&request, PATCH).unwrap();
    let id = &world.world_record().unwrap().id;
    let args = [
        "record",
        "update",
        id,
        "--file",
        request.to_str().unwrap(),
        "--expected-fingerprint",
        &world.fingerprint,
    ];
    let response = cli(&world.root, &args);
    let preview = &response["result"];
    assert_eq!(preview["canApply"], true, "{response}");
    let mut apply = args.to_vec();
    apply.extend([
        "--apply",
        "--expected-after-sha256",
        preview["afterSha256"].as_str().unwrap(),
    ]);
    cli(&world.root, &apply);
    let saved = source::load_world(&world.root, None).unwrap();
    assert_eq!(saved.fingerprint, preview["candidateFingerprint"]);
    assert_eq!(
        fs::read(world.root.join("story/world.md")).unwrap(),
        preview["afterSource"].as_str().unwrap().as_bytes()
    );
    let response = cli(
        &world.root,
        &[
            "chronology",
            "convert",
            HARBOR,
            TIDE,
            "--year",
            "0",
            "--month",
            "2",
            "--day",
            "1",
        ],
    );
    assert_eq!(response["result"]["found"], true);
    assert_eq!(response["result"]["axisDay"], "30");
}
