//! Family histories publish through the existing reviewed one-record writer.
use std::{fs, path::Path, process::Command};

use serde_json::Value as Json;
use serde_yaml::Value as Yaml;
use wedl::{
    Error, authoring,
    generational::{self, LineageDirection},
    model::{StoryTime, World},
    record_edit, source, validation,
};

const ANA: &str = "char_00000000000000000000000001";
const ROWAN: &str = "char_00000000000000000000000002";
const JULES: &str = "char_00000000000000000000000003";
const PARENTAGE: &str = "kinship_00000000000000000000000001";
const UNION: &str = "union_00000000000000000000000001";
const PARENT_REQUEST: &str = include_str!("../examples/authoring/23-parentage.yaml");
const UNION_REQUEST: &str = include_str!("../examples/authoring/24-union.yaml");
const PARENT_END: &str = include_str!("../examples/authoring/25-parentage-end.yaml");
const UNION_END: &str = include_str!("../examples/authoring/26-union-end.yaml");

fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime {
        timeline: "main".into(),
        tick,
        order,
    }
}

fn unchanged(world: &World) {
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
}

fn save(world: &World, id: Option<&str>, request: &str) -> World {
    let preview = match id {
        Some(id) => authoring::preview_record_update(world, id, request, &world.fingerprint),
        None => authoring::preview_record_create(world, request, &world.fingerprint),
    }
    .unwrap();
    assert_eq!(preview["canApply"], true, "{preview}");
    unchanged(world);
    let hash = preview["afterSha256"].as_str().unwrap();
    match id {
        Some(id) => authoring::apply_record_update(world, id, request, &world.fingerprint, hash),
        None => authoring::apply_record_create(world, request, &world.fingerprint, hash),
    }
    .unwrap();
    let saved = source::load_world(&world.root, None).unwrap();
    assert_eq!(saved.fingerprint, preview["candidateFingerprint"]);
    assert_eq!(
        fs::read(world.root.join(preview["sourcePath"].as_str().unwrap())).unwrap(),
        preview["afterSource"].as_str().unwrap().as_bytes()
    );
    assert!(validation::validate_world(&saved).valid);
    saved
}

fn refused(world: &World, id: Option<&str>, request: &str) {
    let preview = match id {
        Some(id) => authoring::preview_record_update(world, id, request, &world.fingerprint),
        None => authoring::preview_record_create(world, request, &world.fingerprint),
    }
    .unwrap();
    assert_eq!(preview["canApply"], false, "{preview}");
    let hash = preview["afterSha256"].as_str().unwrap();
    let applied = match id {
        Some(id) => authoring::apply_record_update(world, id, request, &world.fingerprint, hash),
        None => authoring::apply_record_create(world, request, &world.fingerprint, hash),
    };
    assert!(matches!(applied, Err(Error::Validation(_))), "{applied:?}");
    unchanged(world);
}

fn enable(world: &World) -> World {
    let path = world.root.join("story/world.md");
    let raw = fs::read_to_string(&path).unwrap();
    assert!(raw.contains("capabilities: []"));
    fs::write(
        path,
        raw.replace("capabilities: []", "capabilities: [generational-core-v1]"),
    )
    .unwrap();
    source::load_world(&world.root, None).unwrap()
}

fn fixture(capability: bool) -> (tempfile::TempDir, World) {
    let dir = tempfile::tempdir().unwrap();
    let mut world = source::init_world(&dir.path().join("world"), "Family authoring").unwrap();
    if capability {
        world = enable(&world);
    }
    for request in [
        include_str!("../examples/authoring/01-location.yaml"),
        include_str!("../examples/authoring/02-character.yaml"),
        include_str!("../examples/authoring/04-event.yaml"),
        include_str!("../examples/authoring/21-rowan.yaml"),
        include_str!("../examples/authoring/22-jules.yaml"),
    ] {
        world = save(&world, None, request);
    }
    (dir, world)
}

fn patch(kind: &str, key: &str, value: Yaml) -> String {
    let mut fields = serde_yaml::Mapping::new();
    fields.insert("kind".into(), kind.into());
    fields.insert(key.into(), value);
    serde_yaml::to_string(&fields).unwrap()
}

#[test]
fn family_examples_roundtrip_and_ending_preserves_the_earlier_history() {
    let (_dir, world) = fixture(true);
    let world = save(&world, None, PARENT_REQUEST);
    let world = save(&world, None, UNION_REQUEST);
    assert_eq!(
        generational::family(&world, ANA, &at(-40, -1), 100).unwrap()["count"],
        0
    );
    assert_eq!(
        generational::family(&world, ANA, &at(-40, 0), 100).unwrap()["parents"][0]["lifecycle"],
        "asserted"
    );
    assert_eq!(
        generational::family(&world, ANA, &at(2, 0), 100).unwrap()["parents"][0]["lifecycle"],
        "confirmed"
    );
    let family = generational::family(&world, ROWAN, &at(2, 0), 100).unwrap();
    assert_eq!(family["children"][0]["targetId"], ANA);
    assert_eq!(
        family["unions"][0]["value"]["participantIds"],
        serde_json::json!([ROWAN, JULES])
    );
    assert_eq!(
        generational::kinship_path(&world, ANA, JULES, &at(2, 0), 8, 100).unwrap()["found"],
        false
    );
    assert_eq!(
        generational::lineage(&world, ANA, &at(2, 0), LineageDirection::Ancestors, 8, 100).unwrap()
            ["items"][0]["targetId"],
        ROWAN
    );
    let world = save(&world, Some(PARENTAGE), PARENT_END);
    let world = save(&world, Some(UNION), UNION_END);
    assert_eq!(
        generational::family(&world, ANA, &at(10, -1), 100).unwrap()["count"],
        1
    );
    assert_eq!(
        generational::family(&world, ANA, &at(10, 0), 100).unwrap()["count"],
        0
    );
    assert_eq!(
        generational::family(&world, JULES, &at(11, -1), 100).unwrap()["count"],
        1
    );
    assert_eq!(
        generational::family(&world, JULES, &at(11, 0), 100).unwrap()["count"],
        0
    );
    for id in [PARENTAGE, UNION] {
        let history = generational::history(&world, id, &at(20, 0), 100).unwrap();
        assert_eq!(history["count"], 3);
        assert_eq!(history["lifecycle"], "ended");
        assert_eq!(history["active"], false);
    }
}

#[test]
fn capabilities_are_explicit_and_invalid_histories_never_publish() {
    let (_dir, world) = fixture(false);
    refused(&world, None, PARENT_REQUEST);
    refused(&world, None, UNION_REQUEST);
    let world = enable(&world);
    let world = save(&world, None, PARENT_REQUEST);
    let world = save(&world, None, UNION_REQUEST);
    for replacement in [
        PARENT_END.replace("parentage-end", "parentage-confirm"),
        PARENT_END.replace("tick: 2", "tick: 1"),
        PARENT_END.replace("payload: {}", "payload: {unexpected: true}"),
        PARENT_END.replace("tick: 10", "tick: -41"),
        PARENT_END.replace("parentage-end", "union-end"),
    ] {
        refused(&world, Some(PARENTAGE), &replacement);
    }
    refused(
        &world,
        Some(PARENTAGE),
        &format!("kind: parentage\nparent_id: {ANA}\n"),
    );
    refused(
        &world,
        Some(PARENTAGE),
        "kind: parentage\nparent_id: char_00000000000000000000000009\n",
    );
    // Reversing the existing canonical edge must fail even at another date.
    let mut reverse: Yaml = serde_yaml::from_str(PARENT_REQUEST).unwrap();
    reverse["id"] = "kinship_00000000000000000000000002".into();
    reverse["child_id"] = ROWAN.into();
    reverse["parent_id"] = ANA.into();
    reverse["initialization"]["applicability"]["point"]["tick"] = (-50).into();
    refused(&world, None, &serde_yaml::to_string(&reverse).unwrap());
    let roster: Yaml = serde_yaml::from_str(&format!("[{JULES}, {ROWAN}]")).unwrap();
    refused(
        &world,
        Some(UNION),
        &patch("union", "participant_ids", roster),
    );
}

#[test]
fn omissions_and_noops_preserve_prose_metadata_and_independent_chronology() {
    let (_dir, world) = fixture(true);
    let world = save(&world, None, PARENT_REQUEST);
    let path = world.root.join(&world.records[PARENTAGE].source_path);
    let raw = fs::read_to_string(&path).unwrap().replacen(
        "\n---\n",
        "\nx-author-note: {chronology: keep me}\n---\n",
        1,
    );
    fs::write(&path, raw).unwrap();
    let world = source::load_world(&world.root, None).unwrap();
    let original = world.records[PARENTAGE].clone();
    let world = save(
        &world,
        Some(PARENTAGE),
        "kind: parentage\nchronology:\n  - id: chronology_00000000000000000000000001\n    role: duration-note\n    value: {duration: {value: 2, unit: year}}\n    provenance: [family-ledger]\n",
    );
    let patch = "kind: parentage\ntitle: Rowan and Ana\n";
    let world = save(&world, Some(PARENTAGE), patch);
    let record = &world.records[PARENTAGE];
    assert_eq!(record.body, original.body);
    assert_eq!(record.source_path, original.source_path);
    for key in [
        "x-author-note",
        "initialization",
        "transitions",
        "child_id",
        "parent_id",
    ] {
        assert_eq!(record.get(key), original.get(key));
    }
    let preview =
        authoring::preview_record_update(&world, PARENTAGE, patch, &world.fingerprint).unwrap();
    assert_eq!(preview["changed"], false);
    assert_eq!(
        preview["afterSource"].as_str().unwrap().as_bytes(),
        record.raw_bytes
    );
    // Whole-array replacement removes confirmation; omission would retain it.
    let world = save(
        &world,
        Some(PARENTAGE),
        "kind: parentage\ntransitions: []\n",
    );
    assert_eq!(
        generational::history(&world, PARENTAGE, &at(20, 0), 100).unwrap()["lifecycle"],
        "asserted"
    );
    assert_eq!(
        world.records[PARENTAGE]
            .get("chronology")
            .unwrap()
            .as_sequence()
            .unwrap()
            .len(),
        1
    );
    let world = save(&world, Some(PARENTAGE), "kind: parentage\nchronology: []\n");
    assert!(
        world.records[PARENTAGE]
            .get("chronology")
            .unwrap()
            .as_sequence()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn same_instant_correction_is_an_explicit_full_history_replacement() {
    let (_dir, world) = fixture(true);
    let world = save(&world, None, PARENT_REQUEST);
    let mut transitions = world.records[PARENTAGE].get("transitions").unwrap().clone();
    let mut correction = transitions[0].clone();
    correction["transition_id"] = "transition_00000000000000000000000004".into();
    correction["replaces_transition_id"] = "transition_00000000000000000000000002".into();
    correction["payload"]["basis"] = "adoptive".into();
    transitions.as_sequence_mut().unwrap().push(correction);
    let world = save(
        &world,
        Some(PARENTAGE),
        &patch("parentage", "transitions", transitions),
    );
    let before = generational::history(&world, PARENTAGE, &at(2, -1), 100).unwrap();
    let after = generational::history(&world, PARENTAGE, &at(2, 0), 100).unwrap();
    assert_eq!(before["value"]["basis"], "biological");
    assert_eq!(before["count"], 1);
    assert_eq!(after["value"]["basis"], "adoptive");
    assert_eq!(after["history"][1]["superseded"], true);
    assert_eq!(after["history"][2]["citation"]["field"], "transitions[1]");
    assert_eq!(after["citations"].as_array().unwrap().len(), 3);
}

#[test]
fn request_shapes_and_status_changes_keep_source_and_query_contracts_separate() {
    let (_dir, world) = fixture(true);
    let mut request: Yaml = serde_yaml::from_str(PARENT_REQUEST).unwrap();
    request.as_mapping_mut().unwrap().remove("transitions");
    let mut world = save(&world, None, &serde_yaml::to_string(&request).unwrap());
    assert_eq!(world.records[PARENTAGE].status(), "canonical");
    assert!(
        world.records[PARENTAGE]
            .get("transitions")
            .unwrap()
            .as_sequence()
            .unwrap()
            .is_empty()
    );
    for invalid in [
        "kind: parentage\ninitialization: null\n",
        "kind: parentage\ninitialization: !tag {}\n",
        "kind: parentage\ntransitions: {}\n",
        "kind: parentage\ntransitions: [null]\n",
        "kind: parentage\nparent_id: loc_00000000000000000000000001\n",
        "kind: parentage\nid: kinship_00000000000000000000000002\n",
        "kind: parentage\nstatus: active\n",
        "kind: parentage\nunknown: true\n",
    ] {
        assert!(
            matches!(
                record_edit::update(&world, PARENTAGE, invalid),
                Err(Error::InvalidInput(_))
            ),
            "accepted {invalid}"
        );
    }
    request.as_mapping_mut().unwrap().remove("initialization");
    assert!(record_edit::create(&world, &serde_yaml::to_string(&request).unwrap()).is_err());
    for status in ["draft", "retired", "canonical"] {
        world = save(
            &world,
            Some(PARENTAGE),
            &format!("kind: parentage\nstatus: {status}\n"),
        );
        assert_eq!(
            generational::family(&world, ANA, &at(20, 0), 100).unwrap()["count"],
            if status == "canonical" { 1 } else { 0 }
        );
    }
}

fn cli(root: &Path, args: &[&str]) -> Json {
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
fn numbered_family_requests_preview_and_save_through_cli() {
    let (dir, mut world) = fixture(true);
    for (id, request) in [
        (None, PARENT_REQUEST),
        (None, UNION_REQUEST),
        (Some(PARENTAGE), PARENT_END),
        (Some(UNION), UNION_END),
    ] {
        let path = dir.path().join("request.yaml");
        fs::write(&path, request).unwrap();
        let mut args = vec!["record", if id.is_some() { "update" } else { "create" }];
        if let Some(id) = id {
            args.push(id);
        }
        args.extend([
            "--file",
            path.to_str().unwrap(),
            "--expected-fingerprint",
            &world.fingerprint,
        ]);
        let response = cli(&world.root, &args);
        let preview = &response["result"];
        assert_eq!(preview["canApply"], true, "{response}");
        unchanged(&world);
        args.extend([
            "--apply",
            "--expected-after-sha256",
            preview["afterSha256"].as_str().unwrap(),
        ]);
        cli(&world.root, &args);
        world = source::load_world(&world.root, None).unwrap();
        assert_eq!(world.fingerprint, preview["candidateFingerprint"]);
        assert_eq!(
            fs::read(world.root.join(preview["sourcePath"].as_str().unwrap())).unwrap(),
            preview["afterSource"].as_str().unwrap().as_bytes()
        );
    }
    let response = cli(
        &world.root,
        &["generational", "family", ANA, "--tick", "2", "--order", "0"],
    );
    assert_eq!(response["result"]["parents"][0]["targetId"], ROWAN);
    let response = cli(
        &world.root,
        &[
            "generational",
            "history",
            UNION,
            "--tick",
            "11",
            "--order",
            "0",
        ],
    );
    assert_eq!(response["result"]["lifecycle"], "ended");
}
