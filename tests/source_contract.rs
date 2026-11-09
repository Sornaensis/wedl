use serde_yaml::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use wedl::{
    model::StoryTime,
    source::{load_world, parse_record, world_from_records},
    validation::validate_world,
};

const WORLD: &str = "---\nschema: wedl/v1\nkind: world\nid: world_00000000000000000000000000\ntitle: Test World\ncapabilities: []\ntimelines: [{id: main, label: Main}]\ndefault_timeline: main\nthreads: []\nstate_keys:\n  character:\n    condition: {type: string}\n    location: {type: entity, kind: location, exclusive_group: placement}\n    holder: {type: entity, kind: character, exclusive_group: placement}\n---\n";
const CHARACTER: &str = "---\nschema: wedl/v1\nkind: character\nid: char_00000000000000000000000001\ntitle: Ana\naliases: [Annie]\ninitial_state: {condition: well}\n---\n";

fn world() -> wedl::model::World {
    world_from_records(
        PathBuf::from("."),
        vec![
            parse_record(WORLD.as_bytes(), "story/world.md").unwrap(),
            parse_record(CHARACTER.as_bytes(), "story/characters/ana.md").unwrap(),
        ],
        None,
    )
    .unwrap()
}

fn sample(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
}

fn rebuilt(world: wedl::model::World) -> wedl::model::World {
    let records = world
        .records
        .values()
        .map(|record| {
            let text = format!(
                "---\n{}---\n{}",
                serde_yaml::to_string(&record.frontmatter).unwrap(),
                record.body
            );
            parse_record(text.as_bytes(), &record.source_path).unwrap()
        })
        .collect();
    world_from_records(world.root, records, world.revision).unwrap()
}

#[test]
fn envelope_preserves_exact_body_and_scalar_types() {
    for newline in ["\n", "\r\n"] {
        let envelope = CHARACTER.replace('\n', newline);
        let payload = "\r\n\n  Prose é —  \r\n---\n\t";
        let raw = format!("{envelope}{payload}");
        let record = parse_record(raw.as_bytes(), "story/ana.md").unwrap();
        assert_eq!(record.body, payload);
        assert_eq!(record.raw_bytes, raw.as_bytes());
    }
    let raw = CHARACTER.replacen(
        "initial_state:",
        "x-opaque: {1: [true, 1, 1.0, '1', null]}\ninitial_state:",
        1,
    );
    let record = parse_record(raw.as_bytes(), "story/ana.md").unwrap();
    let values = record
        .get("x-opaque")
        .unwrap()
        .as_mapping()
        .unwrap()
        .get(&Value::from(1))
        .unwrap()
        .as_sequence()
        .unwrap();
    assert!(values[0].is_bool());
    assert!(values[1].as_i64().is_some());
    assert!(values[2].as_i64().is_none());
    assert!(values[3].is_string());
    assert!(values[4].is_null());
}

#[test]
fn malformed_envelopes_and_duplicate_keys_are_rejected() {
    for bad in [
        CHARACTER.replace("schema: wedl/v1", "schema: wedl/v1\nschema: wedl/v1"),
        CHARACTER.replace("{condition: well}", "{condition: well, condition: ill}"),
        CHARACTER.replace("schema: wedl/v1", "schema: wedl/unsupported"),
        CHARACTER.replace("schema: wedl/v1", "schema: other/v1"),
        CHARACTER.replace("schema: wedl/v1", "schema: true"),
        CHARACTER.trim_start_matches("---\n").to_owned(),
        CHARACTER.trim_end_matches("---\n").to_owned(),
    ] {
        assert!(
            parse_record(bad.as_bytes(), "story/bad.md").is_err(),
            "accepted {bad:?}"
        );
    }
    let mut invalid = CHARACTER.as_bytes().to_vec();
    invalid.push(255);
    assert!(parse_record(&invalid, "story/bad.md").is_err());
}

#[test]
fn exact_numeric_time_and_public_strings() {
    for field in [
        "tick: true",
        "tick: 1.0",
        "tick: '1'",
        "tick: 9223372036854775808",
        "tick: 0, order: 2147483648",
        "tick: 0, order: false",
        "tick: 0, timeline: ''",
    ] {
        let value: Value = serde_yaml::from_str(&format!("{{{field}}}")).unwrap();
        assert!(
            StoryTime::from_yaml(&value, "main").is_err(),
            "accepted {field}"
        );
    }
    let value: Value =
        serde_yaml::from_str("{tick: -9223372036854775808, order: 2147483647}").unwrap();
    let point = StoryTime::from_yaml(&value, "main").unwrap();
    assert_eq!(point.tick, i64::MIN);
    assert_eq!(
        serde_json::to_value(&point).unwrap(),
        serde_json::json!({"timeline":"main","tick":"-9223372036854775808","order":"2147483647"})
    );
    assert!(!point.not_after(&StoryTime::new("other", i64::MAX, i32::MAX).unwrap()));
}

#[test]
fn duplicate_ids_and_case_collisions_are_load_errors() {
    let owner = parse_record(WORLD.as_bytes(), "story/world.md").unwrap();
    let person = parse_record(CHARACTER.as_bytes(), "story/ana.md").unwrap();
    assert!(
        world_from_records(
            PathBuf::from("."),
            vec![owner.clone(), person.clone(), person.clone()],
            None
        )
        .is_err()
    );
    let mut other = parse_record(
        CHARACTER
            .replace("00000000000000000000000001", "00000000000000000000000002")
            .as_bytes(),
        "story/ANA.md",
    )
    .unwrap();
    other.title = "Other".into();
    assert!(world_from_records(PathBuf::from("."), vec![owner, person, other], None).is_err());
}

#[test]
fn identity_resolution_is_ambiguous_instead_of_arbitrary() {
    let mut world = self::world();
    assert_eq!(world.resolve("ANNIE").unwrap().title, "Ana");
    let other = parse_record(
        CHARACTER
            .replace("00000000000000000000000001", "00000000000000000000000002")
            .as_bytes(),
        "story/other.md",
    )
    .unwrap();
    world.records.insert(other.id.clone(), other);
    assert!(matches!(
        world.resolve("Ana"),
        Err(wedl::Error::Ambiguous(_))
    ));
    assert!(world.resolve("char_00000000000000000000000001").is_ok());
    assert!(world.story_time(0, Some("undeclared"), 0).is_err());
}

#[test]
fn source_fingerprint_tracks_raw_bytes_and_is_order_independent() {
    let world = world();
    let mut records: Vec<_> = world.records.values().cloned().collect();
    records.reverse();
    let same = world_from_records(
        PathBuf::from("elsewhere"),
        records.clone(),
        Some("unrelated-revision".into()),
    )
    .unwrap();
    assert_eq!(same.fingerprint, world.fingerprint);
    records[0].raw_bytes.push(b' ');
    records[0] = parse_record(&records[0].raw_bytes, &records[0].source_path).unwrap();
    let changed = world_from_records(PathBuf::from("."), records, None).unwrap();
    assert_ne!(changed.fingerprint, world.fingerprint);
}

#[test]
fn all_packaged_corpora_load_and_pass_implemented_validation() {
    for (name, count) in [
        ("tideglass", 64),
        ("ash_archive", 262),
        ("frontiersmen", 309),
        ("chronology_conformance", 7),
    ] {
        let world = load_world(sample(name), None).unwrap();
        assert_eq!(world.records.len(), count, "{name}");
        let report = validate_world(&world);
        assert!(
            report.valid,
            "{name}: {:?}",
            report
                .diagnostics
                .iter()
                .filter(|d| d.severity == "error")
                .collect::<Vec<_>>()
        );
        assert!(!report.complete);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "RUST-PARTIAL-VALIDATION")
        );
    }
}

#[test]
fn capability_order_prerequisites_and_source_references_are_checked() {
    let mut world = self::world();
    world
        .records
        .get_mut("world_00000000000000000000000000")
        .unwrap()
        .frontmatter["capabilities"] = serde_yaml::from_str("[route-v1]").unwrap();
    let world = rebuilt(world);
    let report = validate_world(&world);
    assert!(!report.valid);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "NAR-CAPABILITY-001" && d.severity == "error")
    );
    let mut world = self::world();
    world
        .records
        .get_mut("char_00000000000000000000000001")
        .unwrap()
        .frontmatter["initial_state"]["location"] =
        serde_yaml::from_str("{entity: loc_00000000000000000000000002}").unwrap();
    let world = rebuilt(world);
    let report = validate_world(&world);
    assert!(report.diagnostics.iter().any(|d| d.code == "WDL-REF-001"));
}

#[test]
fn causes_and_conflicting_writes_are_rejected() {
    let mut world = world();
    let event = "---\nschema: wedl/v1\nkind: event\nid: event_00000000000000000000000003\ntitle: Change\ntime: {tick: 5}\ncauses: [event_00000000000000000000000004]\neffects:\n- {id: effect_00000000000000000000000003, operation: set, target: char_00000000000000000000000001, key: condition, value: tired}\n---\n";
    let first = parse_record(event.as_bytes(), "story/event1.md").unwrap();
    let second = parse_record(
        event
            .replace(
                "event_00000000000000000000000003",
                "event_00000000000000000000000004",
            )
            .replace("causes: [event_00000000000000000000000004]", "causes: []")
            .as_bytes(),
        "story/event2.md",
    )
    .unwrap();
    world.records.insert(first.id.clone(), first);
    world.records.insert(second.id.clone(), second);
    let world = rebuilt(world);
    let report = validate_world(&world);
    assert!(report.diagnostics.iter().any(|d| d.code == "WDL-CAUSE-001"));
    assert!(report.diagnostics.iter().any(|d| d.code == "WDL-STATE-020"));
}

#[test]
fn state_types_and_exclusive_placement_are_checked() {
    let mut world = world();
    let raw = "---\nschema: wedl/v1\nkind: location\nid: loc_00000000000000000000000002\ntitle: Dock\n---\n";
    let location = parse_record(raw.as_bytes(), "story/dock.md").unwrap();
    world.records.insert(location.id.clone(), location);
    let person = world
        .records
        .get_mut("char_00000000000000000000000001")
        .unwrap();
    person.frontmatter["initial_state"] = serde_yaml::from_str("{condition: false, location: {entity: loc_00000000000000000000000002}, holder: {entity: char_00000000000000000000000001}}").unwrap();
    let world = rebuilt(world);
    let report = validate_world(&world);
    assert!(report.diagnostics.iter().any(|d| d.code == "WDL-STATE-011"));
    assert!(report.diagnostics.iter().any(|d| d.code == "WDL-STATE-015"));
}

#[test]
fn prose_that_looks_like_an_id_is_not_a_reference() {
    let mut world = world();
    let literal = Value::from("loc_00000000000000000000000002");
    let person = world
        .records
        .get_mut("char_00000000000000000000000001")
        .unwrap();
    person.frontmatter["title"] = literal.clone();
    person.frontmatter["aliases"] = Value::Sequence(vec![literal.clone()]);
    person.frontmatter["tags"] = Value::Sequence(vec![literal.clone()]);
    person.frontmatter["initial_state"]["condition"] = literal.clone();
    person.frontmatter["x-opaque"] =
        serde_yaml::from_str("{entity: loc_00000000000000000000000002}").unwrap();
    let knowledge = "---\nschema: wedl/v1\nkind: knowledge\nid: know_00000000000000000000000003\ntitle: Claim\nknower: char_00000000000000000000000001\nclaim: {key: literal.statement, statement: loc_00000000000000000000000002}\ntransitions: []\n---\nloc_00000000000000000000000002\n";
    let record = parse_record(knowledge.as_bytes(), "story/claim.md").unwrap();
    world.records.insert(record.id.clone(), record);
    let world = rebuilt(world);
    let report = validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
}

#[test]
fn stale_model_and_fingerprint_are_rejected_before_cache_publication() {
    let temp = tempfile::tempdir().unwrap();
    let mut world = world();
    world.root = temp.path().to_owned();
    world
        .records
        .get_mut("char_00000000000000000000000001")
        .unwrap()
        .body = "forged source".into();
    assert!(
        validate_world(&world)
            .diagnostics
            .iter()
            .any(|d| d.code == "RUST-SOURCE-MISMATCH")
    );
    assert!(wedl::compiler::compile(&world).is_err());
    assert!(!temp.path().join(".wedl-rust").exists());
    let mut world = rebuilt(world);
    world.fingerprint = "forged".into();
    assert!(
        validate_world(&world)
            .diagnostics
            .iter()
            .any(|d| d.code == "RUST-FINGERPRINT-MISMATCH")
    );
    assert!(wedl::compiler::compile(&world).is_err());
    assert!(!temp.path().join(".wedl-rust").exists());
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

#[test]
fn historical_source_is_pinned_and_does_not_read_dirty_worktree() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::create_dir(root.join("story")).unwrap();
    fs::write(root.join("story/world.md"), WORLD).unwrap();
    let original = format!(
        "{}\r\n  Exact original  \n",
        CHARACTER.replace('\n', "\r\n")
    );
    fs::write(root.join("story/ana.md"), &original).unwrap();
    git(root, &["init", "--quiet"]);
    git(root, &["add", "story"]);
    git(root, &["commit", "--quiet", "-m", "seed"]);
    let commit = git(root, &["rev-parse", "HEAD"]);
    let blob = git(root, &["rev-parse", "HEAD:story/ana.md"]);
    fs::write(
        root.join("story/ana.md"),
        CHARACTER.replace("Ana", "Changed"),
    )
    .unwrap();
    let before = git(root, &["status", "--porcelain"]);
    let snapshot = load_world(root, Some(&commit)).unwrap();
    assert_eq!(snapshot.revision.as_deref(), Some(commit.as_str()));
    assert_eq!(
        snapshot.resolve("Ana").unwrap().raw_bytes,
        original.as_bytes()
    );
    assert_eq!(
        snapshot.resolve("Ana").unwrap().blob_oid.as_deref(),
        Some(blob.as_str())
    );
    assert_eq!(git(root, &["status", "--porcelain"]), before);
    assert_eq!(
        load_world(root, None)
            .unwrap()
            .resolve("Changed")
            .unwrap()
            .title,
        "Changed"
    );
    assert!(!root.join(".wedl").exists());
}

#[cfg(unix)]
#[test]
fn source_symlinks_are_refused() {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::create_dir(root.join("story")).unwrap();
    fs::write(root.join("story/world.md"), WORLD).unwrap();
    fs::write(root.join("outside.md"), CHARACTER).unwrap();
    symlink(root.join("outside.md"), root.join("story/ana.md")).unwrap();
    assert!(load_world(root, None).is_err());
}
