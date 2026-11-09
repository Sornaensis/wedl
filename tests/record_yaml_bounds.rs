//! Draft and Plan boundaries apply before YAML aliases allocate expanded values.
use serde_json::json;
use std::path::PathBuf;
use wedl::{
    Error, consequences,
    model::{Record, World},
    record_edit, source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const ACTOR: &str = "char_00000000000000000000000001";
const ENVIRONMENT: &str = "env_00000000000000000000000001";
const NEW_ENVIRONMENT: &str = "env_00000000000000000000000002";

fn record(kind: &str, id: &str, fields: &str) -> Record {
    source::parse_record(
        format!(
            "---\n# Original source must remain untouched.\nschema: wedl/v1\nkind: {kind}\nid: {id}\ntitle: Fixture {kind}\n{fields}---\nAuthored body.\n"
        )
        .as_bytes(),
        &format!("story/{id}.md"),
    )
    .unwrap()
}

fn fixture() -> World {
    let world = source::world_from_records(
        PathBuf::from("/yaml-bounds-must-not-read-or-write"),
        vec![
            record(
                "world",
                WORLD,
                "capabilities: []\nthreads: []\ndefault_timeline: main\ntimelines: [{id: main, label: Main}]\nstate_keys: {}\n",
            ),
            record("character", ACTOR, "initial_state: {}\n"),
            record(
                "environment",
                ENVIRONMENT,
                &format!(
                    "time: {{start: {{tick: 0}}, end: null}}\ntargets: [{ACTOR}]\nconditions: {{fog: &fog mist, echo: *fog}}\n"
                ),
            ),
        ],
        None,
    )
    .unwrap();
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    world
}

fn expanded_conditions() -> String {
    // Only 16,384 Unicode characters per value, but 65,536 UTF-8 bytes.
    // The raw request remains small and every condition is otherwise valid.
    let mut patch = format!(
        "kind: environment\nconditions:\n  first: &large {}\n",
        "🌲".repeat(16_384)
    );
    for index in 0..16 {
        patch.push_str(&format!("  copy{index}: *large\n"));
    }
    patch
}

fn create_environment(patch: &str) -> String {
    format!(
        "{patch}id: {NEW_ENVIRONMENT}\ntitle: Proposed weather\ntime: {{start: {{tick: 0}}, end: null}}\ntargets: [{ACTOR}]\n"
    )
}

fn assert_expansion_error(error: Error) {
    assert!(matches!(&error, Error::InvalidInput(_)), "{error}");
    assert!(error.to_string().contains("expanded YAML"), "{error}");
}

#[test]
fn native_create_and_update_bound_expanded_utf8_before_constructing_records() {
    let world = fixture();
    let before = world.records[ENVIRONMENT].raw_bytes.clone();
    let patch = expanded_conditions();
    assert!(patch.len() < 70_000);
    assert_expansion_error(record_edit::update(&world, ENVIRONMENT, &patch).unwrap_err());
    assert_expansion_error(record_edit::create(&world, &create_environment(&patch)).unwrap_err());
    assert_eq!(world.records[ENVIRONMENT].raw_bytes, before);
    assert!(!world.records.contains_key(NEW_ENVIRONMENT));
}

#[test]
fn retained_plan_yaml_stays_opaque_until_preview_but_cannot_bypass_expansion_bounds() {
    let world = fixture();
    let patch = expanded_conditions();
    for operation in [
        json!({"type":"record.update","entityId":ENVIRONMENT,"patch":patch}),
        json!({"type":"record.create","draft":create_environment(&patch)}),
    ] {
        let payload =
            json!({"expectedFingerprint":world.fingerprint,"operations":[operation]}).to_string();
        assert!(payload.len() < consequences::MAX_PLAN_BYTES);
        let plan = consequences::decode(&payload).unwrap();
        let error = consequences::preview(&world, &plan, consequences::MAX_CHARACTERS).unwrap_err();
        assert_expansion_error(error);
    }
}

#[test]
fn repeated_null_collections_hit_the_expanded_node_bound_without_large_strings() {
    let world = fixture();
    // The scalar-byte bound alone cannot cover repeated nulls. Even a field
    // that typed construction would reject must not allocate an oversized tree.
    let patch = format!(
        "kind: character\nunknown: &nodes [{}]\nalso_unknown: [{}]\n",
        vec!["null"; 4096].join(", "),
        vec!["*nodes"; 32].join(", "),
    );
    assert!(patch.len() < 26_000);
    let error = record_edit::update(&world, ACTOR, &patch).unwrap_err();
    assert!(error.to_string().contains("work bound exceeded"), "{error}");
    assert_expansion_error(error);
}

#[test]
fn safe_aliases_keep_native_types_plan_results_and_unchanged_source_bytes() {
    let world = fixture();
    let alias = "kind: environment\nconditions: {fog: &fog '🌲 mist', echo: *fog, count: 9007199254740993, calm: true, absent: null}\nsensory: [*fog]\n";
    let expanded = "kind: environment\nconditions: {fog: '🌲 mist', echo: '🌲 mist', count: 9007199254740993, calm: true, absent: null}\nsensory: ['🌲 mist']\n";
    let update = record_edit::update(&world, ENVIRONMENT, alias).unwrap();
    assert_eq!(
        update.raw_bytes,
        record_edit::update(&world, ENVIRONMENT, expanded)
            .unwrap()
            .raw_bytes
    );
    assert_eq!(
        update.get("conditions").unwrap()["count"].as_i64(),
        Some(9_007_199_254_740_993)
    );
    assert!(update.get("conditions").unwrap()["absent"].is_null());
    assert_eq!(
        record_edit::create(&world, &create_environment(alias))
            .unwrap()
            .raw_bytes,
        record_edit::create(&world, &create_environment(expanded))
            .unwrap()
            .raw_bytes
    );
    let plan = consequences::decode(
        &json!({"expectedFingerprint":world.fingerprint,"operations":[
            {"type":"record.update","entityId":ENVIRONMENT,"patch":alias}
        ]})
        .to_string(),
    )
    .unwrap();
    let report = consequences::preview(&world, &plan, consequences::MAX_CHARACTERS).unwrap();
    assert_eq!(report["validation"]["valid"], true);
    assert_eq!(
        report["files"][0]["afterSource"]
            .as_str()
            .unwrap()
            .as_bytes(),
        update.raw_bytes
    );
    assert_eq!(
        record_edit::update(&world, ENVIRONMENT, "kind: environment\n")
            .unwrap()
            .raw_bytes,
        world.records[ENVIRONMENT].raw_bytes
    );
}

#[test]
fn expanded_string_limit_is_inclusive_and_counts_keys_as_well_as_values() {
    let world = fixture();
    // kind/character/title/body contribute22 bytes, with two copies of the
    // anchored value. The encoded input stays below the separate raw-byte cap.
    let value = "x".repeat((1024 * 1024 - 22) / 2);
    let at_limit = format!("kind: character\ntitle: &text {value}\nbody: *text\n");
    assert!(at_limit.len() < 1024 * 1024);
    let record = record_edit::update(&world, ACTOR, &at_limit).unwrap();
    assert_eq!(record.body, value);

    let over_limit = format!("kind: character\ntitle: &text {value}x\nbody: *text\n");
    assert!(over_limit.len() < 1024 * 1024);
    assert_expansion_error(record_edit::update(&world, ACTOR, &over_limit).unwrap_err());
}
