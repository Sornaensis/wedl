//! Chronology annotation edits share the existing candidate validation and writer.
use std::{fs, path::Path, process::Command};

use serde_json::{Value, json};
use wedl::{Error, authoring, chronology, model::World, record_edit, source, validation};

const CALENDARS: &str = include_str!("../examples/authoring/19-world-chronology.yaml");
const ANNOTATIONS: &str = include_str!("../examples/authoring/20-character-chronology.yaml");
const HERO: &str = "char_00000000000000000000000001";
const CALENDAR: &str = "calendar_00000000000000000000000001";
const ANNOTATION: &str = "chronology_00000000000000000000000001";

fn save(world: &World, target: Option<&str>, request: &str) -> World {
    let base = &world.fingerprint;
    let preview = match target {
        Some(id) => authoring::preview_record_update(world, id, request, base),
        None => authoring::preview_record_create(world, request, base),
    }
    .unwrap();
    assert_eq!(preview["canApply"], true, "{preview}");
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        *base
    );
    let hash = preview["afterSha256"].as_str().unwrap();
    match target {
        Some(id) => authoring::apply_record_update(world, id, request, base, hash),
        None => authoring::apply_record_create(world, request, base, hash),
    }
    .unwrap();
    let result = source::load_world(&world.root, None).unwrap();
    assert_eq!(result.fingerprint, preview["candidateFingerprint"]);
    assert_eq!(
        result.records[preview["entityId"].as_str().unwrap()].raw_bytes,
        preview["afterSource"].as_str().unwrap().as_bytes()
    );
    assert!(validation::validate_world(&result).valid);
    result
}

fn fixture() -> (tempfile::TempDir, World) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("world");
    source::init_world(&root, "Annotated harbor").unwrap();
    let mut world = source::load_world(&root, None).unwrap();
    world = save(&world, Some(&world.world_record().unwrap().id), CALENDARS);
    for request in [
        include_str!("../examples/authoring/01-location.yaml"),
        include_str!("../examples/authoring/02-character.yaml"),
        include_str!("../examples/authoring/04-event.yaml"),
    ] {
        world = save(&world, None, request);
    }
    (temp, world)
}

fn list(world: &World, record: &str) -> Value {
    chronology::annotations(
        world,
        &chronology::AnnotationFilter {
            record: Some(record.into()),
            calendar: None,
            era: None,
            limit: 100,
        },
    )
    .unwrap()
}

fn refuse(world: &World, target: &str, patch: &str) {
    let preview =
        authoring::preview_record_update(world, target, patch, &world.fingerprint).unwrap();
    assert_eq!(preview["canApply"], false, "{preview}");
    assert!(matches!(
        authoring::apply_record_update(
            world,
            target,
            patch,
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

fn patch(value: &str) -> String {
    format!(
        "kind: character\nchronology:\n- id: {ANNOTATION}\n  value: {value}\n  provenance: [ledger]\n"
    )
}

#[test]
fn annotation_example_preserves_other_source_and_has_explicit_relations() {
    let (_temp, world) = fixture();
    let saved = save(&world, Some(HERO), ANNOTATIONS);
    assert_eq!(list(&saved, HERO)["count"], 8);
    assert_eq!(saved.current_time().unwrap(), world.current_time().unwrap());
    assert_eq!(saved.records[HERO].body, world.records[HERO].body);
    for (id, record) in &world.records {
        if id != HERO {
            assert_eq!(saved.records[id].raw_bytes, record.raw_bytes);
        }
    }
    for (right, relation) in [
        (2, "equal"),
        (3, "overlaps"),
        (4, "overlaps"),
        (5, "unknown"),
        (6, "unknown"),
        (7, "unknown"),
        (8, "after"),
    ] {
        let id = format!("chronology_{right:026}");
        let compared =
            chronology::compare_annotations(&saved, HERO, ANNOTATION, HERO, &id).unwrap();
        assert_eq!(compared["relation"], relation, "{compared}");
    }
    let before = saved.records[HERO].body.clone();
    let cleared = save(&saved, Some(HERO), "kind: character\nchronology: []\n");
    assert_eq!(list(&cleared, HERO)["count"], 0);
    assert_eq!(cleared.records[HERO].body, before);
}

#[test]
fn annotation_field_supports_create_and_each_existing_editable_record_kind() {
    let (_temp, world) = fixture();
    let create = format!(
        "kind: character\nid: char_00000000000000000000000002\ntitle: Visitor\nchronology:\n- id: {ANNOTATION}\n  value: {{civil: {{calendar_id: {CALENDAR}, year: -1}}}}\n  provenance: [ledger]\n"
    );
    let created = save(&world, None, &create);
    assert_eq!(list(&created, "Visitor")["count"], 1);

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/tideglass");
    let source = source::load_world(root, None).unwrap();
    for kind in [
        "character",
        "location",
        "object",
        "event",
        "knowledge",
        "scene",
        "conversation",
        "route",
        "map",
    ] {
        let record = source
            .records
            .values()
            .find(|record| record.kind == kind)
            .unwrap();
        let request = format!(
            "kind: {kind}\nchronology:\n- id: {ANNOTATION}\n  value: {{duration: {{value: 2, unit: day}}}}\n  provenance: [ledger]\n"
        );
        let candidate = record_edit::update(&source, &record.id, &request).unwrap();
        assert_eq!(candidate.body, record.body);
        let mut records = source.records.clone();
        records.insert(candidate.id.clone(), candidate);
        let changed =
            source::world_from_records(source.root.clone(), records.into_values().collect(), None)
                .unwrap();
        let report = validation::validate_world(&changed);
        assert!(report.valid, "{kind}: {:?}", report.diagnostics);
        assert_eq!(list(&changed, &record.id)["count"], 1);
    }
}

#[test]
fn annotation_semantic_errors_are_reviewable_and_do_not_publish() {
    let (_temp, world) = fixture();
    for value in [
        format!("{{civil: {{calendar_id: {CALENDAR}, year: 0, month: 2, day: 32}}}}"),
        "{civil: {calendar_id: calendar_00000000000000000000000999, year: 0}}".into(),
        format!("{{civil: {{calendar_id: {CALENDAR}, year: 0, day: 1}}}}"),
        "{era: {era_id: era_00000000000000000000000001, year: 0}}".into(),
        format!(
            "{{range: {{calendar_id: {CALENDAR}, lower: {{calendar_id: {CALENDAR}, year: 2}}, upper: {{calendar_id: {CALENDAR}, year: 1}}}}}}"
        ),
        "{relative: {relation: before, before_id: char_00000000000000000000000999}}".into(),
        format!("{{conflict: {{claims: [{{civil: {{calendar_id: {CALENDAR}, year: 0}}}}]}}}}"),
        "{duration: {value: 2, unit: hour}}".into(),
        "{duration: {value: 2.0, unit: day}}".into(),
        "null".into(),
    ] {
        refuse(&world, HERO, &patch(&value));
    }
    let duplicate = format!(
        "{}- id: {ANNOTATION}\n  value: {{duration: {{value: 2, unit: day}}}}\n  provenance: [ledger]\n",
        patch("{duration: {value: 1, unit: day}}")
    );
    refuse(&world, HERO, &duplicate);
}

#[test]
fn declaration_edits_cannot_orphan_or_invalidate_saved_annotations() {
    let (_temp, world) = fixture();
    let world = save(&world, Some(HERO), ANNOTATIONS);
    let owner = &world.world_record().unwrap().id;
    refuse(
        &world,
        owner,
        "kind: world\nchronology: {calendars: [], eras: [], anchors: []}\n",
    );
    // Era label changes are harmless, but removing its referenced identity is not.
    refuse(
        &world,
        owner,
        &CALENDARS.replace(
            "era_00000000000000000000000001",
            "era_00000000000000000000000002",
        ),
    );
    refuse(
        &world,
        owner,
        &CALENDARS.replace("axis_day: 0}", "axis_day: 9223372036854775807}"),
    );
}

#[test]
fn unchanged_annotations_retain_bytes_and_outer_shapes_remain_typed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/chronology_conformance");
    let world = source::load_world(root, None).unwrap();
    for record in world
        .records
        .values()
        .filter(|record| record.kind != "world" && record.get("chronology").is_some())
    {
        let mut request = serde_yaml::Mapping::new();
        request.insert("kind".into(), record.kind.clone().into());
        request.insert(
            "chronology".into(),
            record.get("chronology").unwrap().clone(),
        );
        let candidate = record_edit::update(
            &world,
            &record.id,
            &serde_yaml::to_string(&request).unwrap(),
        )
        .unwrap();
        assert_eq!(candidate.raw_bytes, record.raw_bytes);
    }
    let (_temp, world) = fixture();
    for value in ["null", "{}", "'a date'", "!tag []"] {
        assert!(matches!(
            record_edit::update(
                &world,
                HERO,
                &format!("kind: character\nchronology: {value}\n")
            ),
            Err(Error::InvalidInput(_))
        ));
    }
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
fn request_twenty_runs_through_cli_and_matches_the_saved_preview() {
    let (temp, world) = fixture();
    let request = temp.path().join("annotations.yaml");
    fs::write(&request, ANNOTATIONS).unwrap();
    let args = [
        "record",
        "update",
        HERO,
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
        saved.records[HERO].raw_bytes,
        preview["afterSource"].as_str().unwrap().as_bytes()
    );
    assert_eq!(
        cli(&world.root, &["chronology", "annotations", HERO])["result"]["count"],
        8
    );
    assert_eq!(
        cli(
            &world.root,
            &[
                "chronology",
                "compare",
                HERO,
                ANNOTATION,
                HERO,
                "chronology_00000000000000000000000002"
            ]
        )["result"]["relation"],
        json!("equal")
    );
}
