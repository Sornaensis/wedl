//! Reviewed edits keep profile-bound state and authored species descriptions explicit.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeMap, fs, process::Command};
use wedl::{
    Error, authoring, character, lore_state,
    model::{Record, StoryTime, World},
    record_edit, runtime, source, species, validation,
};

const ANA: &str = "char_00000000000000000000000001";
const ROOT: &str = "concept_00000000000000000000000003";
const INDIVIDUAL: &str = "concept_00000000000000000000000005";
const SET_EVENT: &str = "event_00000000000000000000000002";
const REQUESTS: [&str; 8] = [
    include_str!("../examples/authoring/72-world-state-profile-bindings.yaml"),
    include_str!("../examples/authoring/73-tide-folk-common.yaml"),
    include_str!("../examples/authoring/74-river-folk-common.yaml"),
    include_str!("../examples/authoring/75-ana-individual-variation.yaml"),
    include_str!("../examples/authoring/76-character-bound-state.yaml"),
    include_str!("../examples/authoring/77-bound-state-set-event.yaml"),
    include_str!("../examples/authoring/78-common-description-update.yaml"),
    include_str!("../examples/authoring/79-bound-state-clear-event.yaml"),
];
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn yaml(value: &str) -> Yaml {
    serde_yaml::from_str(value).unwrap()
}
fn unchanged(world: &World) {
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
}
fn retained(before: &Record, after: &Record, changed: &[&str]) {
    assert_eq!(before.body, after.body);
    for (key, value) in before.frontmatter.as_mapping().unwrap() {
        if !changed.contains(&key.as_str().unwrap()) {
            assert_eq!(after.frontmatter.get(key), Some(value), "{key:?}");
        }
    }
}
fn save(world: &World, target: Option<&str>, request: &str) -> World {
    let preview = match target {
        Some(id) => authoring::preview_record_update(world, id, request, &world.fingerprint),
        None => authoring::preview_record_create(world, request, &world.fingerprint),
    }
    .unwrap();
    assert_eq!(preview["canApply"], true, "{preview}");
    unchanged(world);
    let hash = preview["afterSha256"].as_str().unwrap();
    match target {
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
    let report = validation::validate_world(&saved);
    assert!(report.valid, "{:?}", report.diagnostics);
    saved
}
fn refused(world: &World, id: &str, request: &str) {
    let preview = authoring::preview_record_update(world, id, request, &world.fingerprint).unwrap();
    assert_eq!(preview["canApply"], false, "{request}: {preview}");
    assert!(matches!(
        authoring::apply_record_update(
            world,
            id,
            request,
            &world.fingerprint,
            preview["afterSha256"].as_str().unwrap()
        ),
        Err(Error::Validation(_))
    ));
    unchanged(world);
}
fn fixture() -> (tempfile::TempDir, World) {
    let dir = tempfile::tempdir().unwrap();
    let world = source::init_world(&dir.path().join("world"), "Reviewed profile state").unwrap();
    let path = world.root.join("story/world.md");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("capabilities: []", "capabilities: [lore-core-v1]"),
    )
    .unwrap();
    let mut world = source::load_world(&world.root, None).unwrap();
    for request in [
        include_str!("../examples/authoring/01-location.yaml"),
        include_str!("../examples/authoring/02-character.yaml"),
        include_str!("../examples/authoring/04-event.yaml"),
        include_str!("../examples/authoring/05-knowledge.yaml"),
    ] {
        world = save(&world, None, request);
    }
    (dir, world)
}
fn through(world: &World, count: usize) -> World {
    let mut world = world.clone();
    let id = world.world_record().unwrap().id.clone();
    for (index, request) in REQUESTS.iter().take(count).enumerate() {
        let target = match index {
            0 => Some(id.as_str()),
            4 => Some(ANA),
            6 => Some(ROOT),
            _ => None,
        };
        world = save(&world, target, request);
    }
    world
}
fn selection() -> species::Selection {
    species::Selection {
        common_profile: "species-common".into(),
        individual_profile: "species-individual".into(),
        individual_link: "common".into(),
        common_link: Some("parents".into()),
        common_fields: BTreeMap::from([
            ("traits".into(), "traits".into()),
            ("habitat".into(), "habitats".into()),
            ("life_stage".into(), "life_stage".into()),
        ]),
        individual_fields: BTreeMap::from([
            ("traits".into(), "traits".into()),
            ("language".into(), "language".into()),
        ]),
    }
}

#[test]
fn world_bindings_and_initial_values_preserve_omitted_source_and_noop_bytes() {
    let (_dir, mut base) = fixture();
    let world_id = base.world_record().unwrap().id.clone();
    base = save(
        &base,
        Some(&world_id),
        include_str!("../examples/authoring/19-world-chronology.yaml"),
    );
    let original_world = base.world_record().unwrap().clone();
    let original_character = base.records[ANA].clone();
    let world = through(&base, 5);
    retained(
        &original_world,
        world.world_record().unwrap(),
        &["profiles", "state_keys", "state_profile_bindings"],
    );
    retained(&original_character, &world.records[ANA], &["initial_state"]);
    let read = lore_state::state(&world, ANA, &at(11, 0), "author", "author").unwrap();
    assert_eq!(
        read["state"]["species_notes"]["measure"],
        "9007199254740993"
    );
    assert!(read["state"]["species_notes"]["absence"].is_null());
    assert_eq!(
        read["references"][0]["sourceField"],
        "initial_state.species_notes.variation.id"
    );
    assert_eq!(
        record_edit::update(&world, &world_id, REQUESTS[0])
            .unwrap()
            .raw_bytes,
        world.world_record().unwrap().raw_bytes
    );
    assert_eq!(
        record_edit::update(&world, ANA, REQUESTS[4])
            .unwrap()
            .raw_bytes,
        world.records[ANA].raw_bytes
    );
}

#[test]
fn profile_and_binding_edits_validate_initial_and_historical_values_after_clear() {
    let (_dir, base) = fixture();
    let world = through(&base, 8);
    let id = world.world_record().unwrap().id.clone();
    assert_eq!(
        lore_state::state(&world, ANA, &at(18, 0), "author", "author").unwrap()["state"],
        json!({})
    );
    refused(&world, &id, "kind: world\nstate_profile_bindings: {}\n");
    refused(&world, &id, "kind: world\nstate_keys: {}\n");
    refused(
        &world,
        &id,
        "kind: world\nstate_profile_bindings: {character: {species_notes: missing-profile}}\n",
    );
    refused(
        &world,
        &id,
        "kind: world\nstate_profile_bindings: {character: {undeclared: character-description-state}}\n",
    );
    let mut request = yaml(REQUESTS[0]);
    request["profiles"][4]["fields"]["required"]["measure"] = yaml("{type: string}");
    refused(&world, &id, &serde_yaml::to_string(&request).unwrap());
    // A cancelled event remains a typed source dependency, too.
    let world = save(
        &world,
        Some("event_00000000000000000000000003"),
        "kind: event\ncauses: []\n",
    );
    let path = world.root.join(&world.records[SET_EVENT].source_path);
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("status: canonical", "status: cancelled"),
    )
    .unwrap();
    let cancelled = source::load_world(&world.root, None).unwrap();
    assert_eq!(cancelled.records[SET_EVENT].status(), "cancelled");
    let report = validation::validate_world(&cancelled);
    assert!(report.valid, "{:?}", report.diagnostics);
    let mut only_historical = yaml(REQUESTS[0]);
    only_historical["profiles"][4]["fields"]["required"]["markers"]["max_items"] = yaml("0");
    refused(
        &cancelled,
        &id,
        &serde_yaml::to_string(&only_historical).unwrap(),
    );
}

#[test]
fn compatible_profile_and_placeholder_changes_do_not_rewrite_state_sources() {
    let (_dir, base) = fixture();
    let world = through(&base, 8);
    let mut request = yaml(REQUESTS[0]);
    request["profiles"][4]["fields"]["optional"]["review_note"] = yaml("{type: string}");
    request["state_keys"]["character"]["species_notes"] = yaml("{type: boolean}");
    let after = save(
        &world,
        Some(&world.world_record().unwrap().id),
        &serde_yaml::to_string(&request).unwrap(),
    );
    for id in [ANA, SET_EVENT, INDIVIDUAL] {
        assert_eq!(after.records[id].raw_bytes, world.records[id].raw_bytes);
    }
    let state = lore_state::state(&after, ANA, &at(15, 0), "author", "author").unwrap();
    assert!(state["state"]["species_notes"].get("review_note").is_none());
    assert_eq!(
        state["state"]["species_notes"]["measure"],
        "9007199254740995"
    );
    assert_eq!(
        runtime::state(&after, ANA, &at(15, 0)).unwrap()["state"],
        json!({"mood":"observant"})
    );
}

#[test]
fn literal_entity_and_chronology_fields_do_not_become_legacy_references() {
    let (_dir, base) = fixture();
    let world_id = base.world_record().unwrap().id.clone();
    let world = save(
        &base,
        Some(&world_id),
        "kind: world\nstate_keys: {character: {notes: {type: entity, kind: location}}}\nprofiles:\n  - id: literal-profile\n    label: Literal profile\n    fields:\n      type: object\n      required:\n        entity: {type: string}\n        chronology: {type: string}\n        subject: {type: entity, kinds: [character]}\n      optional: {}\nstate_profile_bindings: {character: {notes: literal-profile}}\n",
    );
    let request = format!(
        "kind: character\ninitial_state:\n  notes:\n    entity: location:missing\n    chronology: calendar_00000000000000000000000999\n    subject: {{kind: record, id: {ANA}}}\n"
    );
    let world = save(&world, Some(ANA), &request);
    let read = lore_state::state(&world, ANA, &at(1, 0), "author", "author").unwrap();
    assert_eq!(read["state"]["notes"]["entity"], "location:missing");
    assert_eq!(read["references"].as_array().unwrap().len(), 1);
    assert_eq!(
        read["references"][0]["sourceField"],
        "initial_state.notes.subject.id"
    );
    let event = format!(
        "kind: event\ntime: {{timeline: main, tick: 1, order: 0}}\neffects:\n- id: effect_00000000000000000000000009\n  target: {ANA}\n  key: notes\n  operation: set\n  value:\n    entity: location:also-missing\n    chronology: another literal\n    subject: {{kind: record, id: {ANA}}}\n"
    );
    let world = save(&world, Some("event_00000000000000000000000001"), &event);
    assert_eq!(
        lore_state::state(&world, ANA, &at(1, 0), "author", "author").unwrap()["references"][0]["sourceField"],
        "effects[0].value.subject.id"
    );
}

#[test]
fn malformed_requests_and_wrong_typed_values_never_publish() {
    let (_dir, base) = fixture();
    let world = through(&base, 6);
    let world_id = world.world_record().unwrap().id.clone();
    for (id, request) in [
        (
            ANA,
            "kind: character\ninitial_state: {species_notes: null}\n",
        ),
        (ANA, "kind: character\ninitial_state: {mood: null}\n"),
        (
            ANA,
            "kind: character\ninitial_state: {species_notes: !opaque {}}\n",
        ),
        (world_id.as_str(), "kind: world\nstate_keys: null\n"),
        (
            world_id.as_str(),
            "kind: world\nstate_profile_bindings: null\n",
        ),
        (
            world_id.as_str(),
            "kind: world\nstate_profile_bindings: {character: {species_notes: {id: character-description-state}}}\n",
        ),
    ] {
        assert!(
            matches!(
                record_edit::update(&world, id, request),
                Err(Error::InvalidInput(_))
            ),
            "{request}"
        );
        unchanged(&world);
    }
    for operation in ["add-to-set", "remove-from-set"] {
        let request = format!(
            "kind: event\neffects: [{{id: effect_00000000000000000000000001, target: {ANA}, key: species_notes, operation: {operation}, value: item}}]\n"
        );
        assert!(matches!(
            record_edit::update(&world, SET_EVENT, &request),
            Err(Error::InvalidInput(_))
        ));
        unchanged(&world);
    }
    let mut wrong = yaml(REQUESTS[4]);
    wrong["initial_state"]["species_notes"]["variation"]["id"] = yaml(ANA);
    refused(&world, ANA, &serde_yaml::to_string(&wrong).unwrap());
    let mut wrong_effect = yaml(REQUESTS[5]);
    wrong_effect.as_mapping_mut().unwrap().remove(&yaml("id"));
    wrong_effect["effects"][0]["value"]["measure"] = yaml("'9007199254740995'");
    refused(
        &world,
        SET_EVENT,
        &serde_yaml::to_string(&wrong_effect).unwrap(),
    );
}

#[test]
fn common_edits_change_composition_without_rewriting_individual_or_bound_state() {
    let (_dir, base) = fixture();
    let before = through(&base, 6);
    let selected = selection();
    let original = species::compose(
        &before,
        INDIVIDUAL,
        &selected,
        Some(&at(15, 0)),
        "author",
        "author",
    )
    .unwrap();
    let after = save(&before, Some(ROOT), REQUESTS[6]);
    let changed = species::compose(
        &after,
        INDIVIDUAL,
        &selected,
        Some(&at(15, 0)),
        "author",
        "author",
    )
    .unwrap();
    assert_ne!(
        original["common"][1]["fields"]["traits"]["value"],
        changed["common"][1]["fields"]["traits"]["value"]
    );
    // Content stays separate; whole-world fingerprints in citations can change.
    for role in ["traits", "language"] {
        for property in ["known", "field", "value", "valueType"] {
            assert_eq!(
                original["individual"]["fields"][role][property],
                changed["individual"]["fields"][role][property]
            );
        }
    }
    for id in [ANA, INDIVIDUAL, SET_EVENT] {
        assert_eq!(before.records[id].raw_bytes, after.records[id].raw_bytes);
    }
    assert_eq!(
        lore_state::state(&before, ANA, &at(15, 0), "author", "author").unwrap()["state"],
        lore_state::state(&after, ANA, &at(15, 0), "author", "author").unwrap()["state"]
    );
    assert_eq!(
        species::affected(
            &after,
            ROOT,
            &selected,
            Some(&at(20, 0)),
            "author",
            "author",
            1
        )
        .unwrap()["count"],
        1
    );
    assert_eq!(
        species::affected(
            &after,
            ROOT,
            &selected,
            Some(&at(20, 1)),
            "author",
            "author",
            1
        )
        .unwrap()["count"],
        0
    );
}

#[test]
fn bound_profiles_and_canonical_events_preserve_complete_character_responses() {
    let (_dir, before) = fixture();
    let after = through(&before, 8);
    for point in [at(4, 0), at(15, 0), at(18, 0), at(21, 0)] {
        assert_eq!(
            character::context(&before, ANA, &point, 100_000).unwrap(),
            character::context(&after, ANA, &point, 100_000).unwrap()
        );
        for query in ["Ana", "species", "scar", "bestiary", "ledger"] {
            assert_eq!(
                character::search(&before, ANA, query, &point, 20).unwrap(),
                character::search(&after, ANA, query, &point, 20).unwrap()
            );
        }
        assert_eq!(
            character::context(&before, ANA, &point, 1)
                .unwrap_err()
                .json(),
            character::context(&after, ANA, &point, 1)
                .unwrap_err()
                .json()
        );
    }
    let cleared = lore_state::state(&after, ANA, &at(18, 0), "author", "author").unwrap();
    assert_eq!(cleared["state"], json!({}));
    assert_eq!(
        cleared["citations"]["species_notes"]["section"],
        "effects[0]"
    );
    let ordinary = runtime::state(&after, ANA, &at(18, 0)).unwrap();
    assert_eq!(ordinary["state"], json!({"mood":"observant"}));
    assert!(ordinary["citations"].get("species_notes").is_none());
}

#[test]
fn all_eight_requests_preview_and_save_through_cli_with_exact_bytes() {
    let (_dir, mut world) = fixture();
    let world_id = world.world_record().unwrap().id.clone();
    for (index, request) in REQUESTS.iter().enumerate() {
        let target = match index {
            0 => Some(world_id.as_str()),
            4 => Some(ANA),
            6 => Some(ROOT),
            _ => None,
        };
        let path = world
            .root
            .parent()
            .unwrap()
            .join(format!("request-{}.yaml", index + 72));
        fs::write(&path, request).unwrap();
        let mut args = vec![
            "--compact",
            "--repo",
            world.root.to_str().unwrap(),
            "record",
        ];
        match target {
            Some(id) => args.extend(["update", id]),
            None => args.push("create"),
        }
        args.extend([
            "--file",
            path.to_str().unwrap(),
            "--expected-fingerprint",
            &world.fingerprint,
        ]);
        let preview = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
            .args(&args)
            .output()
            .unwrap();
        assert!(
            preview.status.success(),
            "{}",
            String::from_utf8_lossy(&preview.stderr)
        );
        let preview: Json = serde_json::from_slice(&preview.stdout).unwrap();
        let preview = &preview["result"];
        assert_eq!(preview["canApply"], true, "{preview}");
        unchanged(&world);
        let applied = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
            .args(args)
            .args([
                "--apply",
                "--expected-after-sha256",
                preview["afterSha256"].as_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            applied.status.success(),
            "{}",
            String::from_utf8_lossy(&applied.stderr)
        );
        assert_eq!(
            fs::read(world.root.join(preview["sourcePath"].as_str().unwrap())).unwrap(),
            preview["afterSource"].as_str().unwrap().as_bytes()
        );
        world = source::load_world(&world.root, None).unwrap();
        assert_eq!(world.fingerprint, preview["candidateFingerprint"]);
    }
    assert!(validation::validate_world(&world).valid);
    assert_eq!(
        lore_state::state(&world, ANA, &at(18, 0), "author", "author").unwrap()["state"],
        json!({})
    );
}
