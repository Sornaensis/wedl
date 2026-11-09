//! Typed lore uses the existing reviewed single-record writer and candidate validation.
use serde_json::Value as Json;
use serde_yaml::Value as Yaml;
use std::{fs, process::Command};
use wedl::{
    Error, authoring, character, lore,
    model::{Record, StoryTime, World},
    record_edit, source, validation,
};

const ANA: &str = "char_00000000000000000000000001";
const LANDING: &str = "location:harbor/landing";
const SYMBOL: &str = "concept_00000000000000000000000001";
const TRADITION: &str = "concept_00000000000000000000000002";
const REQUESTS: [&str; 8] = [
    include_str!("../examples/authoring/64-world-lore-profiles.yaml"),
    include_str!("../examples/authoring/65-lantern-symbol.yaml"),
    include_str!("../examples/authoring/66-lantern-tradition.yaml"),
    include_str!("../examples/authoring/67-character-concept-links.yaml"),
    include_str!("../examples/authoring/68-tradition-values.yaml"),
    include_str!("../examples/authoring/69-world-profile-extension.yaml"),
    include_str!("../examples/authoring/70-retire-symbol.yaml"),
    include_str!("../examples/authoring/71-clear-concept-links.yaml"),
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
fn refused(world: &World, target: &str, request: &str) {
    let preview =
        authoring::preview_record_update(world, target, request, &world.fingerprint).unwrap();
    assert_eq!(preview["canApply"], false, "{request}: {preview}");
    let result = authoring::apply_record_update(
        world,
        target,
        request,
        &world.fingerprint,
        preview["afterSha256"].as_str().unwrap(),
    );
    assert!(matches!(result, Err(Error::Validation(_))), "{result:?}");
    unchanged(world);
}
fn fixture() -> (tempfile::TempDir, World) {
    let dir = tempfile::tempdir().unwrap();
    let mut world = source::init_world(&dir.path().join("world"), "Reviewed typed lore").unwrap();
    let path = world.root.join("story/world.md");
    fs::write(
        &path,
        fs::read_to_string(&path).unwrap().replace(
            "capabilities: []",
            "capabilities: [spatial-core-v1, lore-core-v1]",
        ),
    )
    .unwrap();
    world = source::load_world(&world.root, None).unwrap();
    for request in [
        include_str!("../examples/authoring/01-location.yaml"),
        include_str!("../examples/authoring/02-character.yaml"),
        include_str!("../examples/authoring/04-event.yaml"),
        include_str!("../examples/authoring/05-knowledge.yaml"),
        include_str!("../examples/authoring/11-landing.yaml"),
    ] {
        world = save(&world, None, request);
    }
    (dir, world)
}
fn with_concepts(world: &World) -> World {
    let world = save(world, Some(&world.world_record().unwrap().id), REQUESTS[0]);
    let world = save(&world, None, REQUESTS[1]);
    save(&world, None, REQUESTS[2])
}
fn values_request(mut edit: impl FnMut(&mut Yaml)) -> String {
    let mut request = yaml(REQUESTS[4]);
    edit(&mut request["values"]);
    serde_yaml::to_string(&request).unwrap()
}

#[test]
fn profile_and_value_updates_preserve_source_and_do_not_supply_optional_defaults() {
    let (_dir, mut world) = fixture();
    let world_id = world.world_record().unwrap().id.clone();
    world = save(
        &world,
        Some(&world_id),
        include_str!("../examples/authoring/19-world-chronology.yaml"),
    );
    let before = world.world_record().unwrap().clone();
    world = with_concepts(&world);
    retained(&before, world.world_record().unwrap(), &["profiles"]);
    let concept_before = world.records[TRADITION].clone();
    world = save(&world, Some(TRADITION), REQUESTS[4]);
    retained(&concept_before, &world.records[TRADITION], &["values"]);
    world = save(&world, Some(&world_id), REQUESTS[5]);
    retained(&before, world.world_record().unwrap(), &["profiles"]);
    assert_eq!(
        world.records[TRADITION].get("values").unwrap()["offering"]["amount"],
        yaml("5")
    );
    assert!(
        world.records[TRADITION]
            .get("values")
            .unwrap()
            .get("season_label")
            .is_none()
    );
    assert_eq!(
        lore::profile(&world, "harbor-tradition").unwrap()["label"],
        "Harbor tradition details"
    );
    for (id, request) in [
        (world_id.as_str(), REQUESTS[5]),
        (TRADITION, "kind: lore-concept\n"),
    ] {
        let noop = record_edit::update(&world, id, request).unwrap();
        assert_eq!(noop.raw_bytes, world.records[id].raw_bytes);
    }
    assert_eq!(
        world.records[TRADITION].source_path,
        format!("story/concepts/{TRADITION}.md")
    );
}

#[test]
fn incompatible_profile_edits_validate_all_current_dependents_before_publication() {
    let (_dir, base) = fixture();
    let world = with_concepts(&base);
    let id = world.world_record().unwrap().id.clone();
    refused(&world, &id, "kind: world\nprofiles: []\n");
    let mut missing = yaml(REQUESTS[0]);
    missing["profiles"].as_sequence_mut().unwrap().remove(1);
    refused(&world, &id, &serde_yaml::to_string(&missing).unwrap());
    for replacement in [
        yaml("{type: string}"),
        yaml("{type: entity, kinds: [character]}"),
    ] {
        let mut changed = yaml(REQUESTS[0]);
        changed["profiles"][1]["fields"]["required"]["gathering_place"] = replacement;
        refused(&world, &id, &serde_yaml::to_string(&changed).unwrap());
    }
    let mut required = yaml(REQUESTS[0]);
    required["profiles"][1]["fields"]["required"]["new_requirement"] = yaml("{type: boolean}");
    refused(&world, &id, &serde_yaml::to_string(&required).unwrap());
    // Retiring a concept changes query admission, not its source validation duty.
    let retired = save(
        &world,
        Some(TRADITION),
        "kind: lore-concept\nstatus: retired\n",
    );
    refused(&retired, &id, &serde_yaml::to_string(&required).unwrap());
}

#[test]
fn typed_values_and_reference_failures_are_reviewable_but_cannot_publish() {
    let (_dir, base) = fixture();
    let world = with_concepts(&base);
    for request in [
        "kind: lore-concept\nprofile: unknown-profile\n".to_owned(),
        values_request(|v| v["offering"]["amount"] = yaml("'5'")),
        values_request(|v| v["origin_story"] = yaml("false")),
        values_request(|v| v["gathering_place"]["id"] = yaml("location:missing")),
        values_request(|v| v["gathering_place"]["id"] = yaml(ANA)),
        values_request(|v| {
            v.as_mapping_mut().unwrap().remove(&yaml("customary"));
        }),
        values_request(|v| v["undeclared"] = yaml("true")),
    ] {
        refused(&world, TRADITION, &request);
    }
    let wrong_kind = format!(
        "kind: character\nconcept_links: [{{relation: associated-with, concept_id: {ANA}, valid: {{kind: timeless}}}}]\n"
    );
    assert!(matches!(
        record_edit::update(&world, ANA, &wrong_kind),
        Err(Error::InvalidInput(_))
    ));
    unchanged(&world);
    refused(
        &world,
        ANA,
        "kind: character\nconcept_links: [{relation: associated-with, concept_id: concept_00000000000000000000000999, valid: {kind: timeless}}]\n",
    );
    let (_cap_dir, cap_world) = fixture();
    let world_id = cap_world.world_record().unwrap().id.clone();
    let path = cap_world.root.join("story/world.md");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace(", lore-core-v1", ""),
    )
    .unwrap();
    let no_capability = source::load_world(&cap_world.root, None).unwrap();
    assert!(validation::validate_world(&no_capability).valid);
    // Even an explicit empty lore field needs capability opt-in.
    refused(&no_capability, &world_id, "kind: world\nprofiles: []\n");
    refused(&no_capability, ANA, "kind: character\nconcept_links: []\n");
}

#[test]
fn malformed_closed_requests_fail_before_writing_and_null_is_not_deletion() {
    let (_dir, base) = fixture();
    let world = with_concepts(&base);
    let world_id = world.world_record().unwrap().id.clone();
    for (id, request) in [
        (TRADITION, "kind: lore-concept\nvalues: null\n"),
        (TRADITION, "kind: lore-concept\nprofile: null\n"),
        (
            TRADITION,
            "kind: lore-concept\nvalid: {kind: timeless, extra: true}\n",
        ),
        (
            TRADITION,
            "kind: lore-concept\nvalid: {kind: interval, timeline: main, start: null}\n",
        ),
        (TRADITION, "kind: lore-concept\nstatus: active\n"),
        (TRADITION, "kind: lore-concept\nprofile_version: 1\n"),
        (ANA, "kind: character\nconcept_links: null\n"),
        (
            ANA,
            "kind: character\nconcept_links: [{relation: associated-with, concept_id: concept_00000000000000000000000001, valid: {kind: timeless}, inferred: true}]\n",
        ),
        (&world_id, "kind: world\nprofiles: null\n"),
        (
            &world_id,
            "kind: world\nprofiles: [{id: example, version: 1, label: Example, fields: {type: object, required: {}, optional: {}}}]\n",
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
    let mut missing = yaml(REQUESTS[1]);
    missing.as_mapping_mut().unwrap().remove(&yaml("values"));
    assert!(record_edit::create(&world, &serde_yaml::to_string(&missing).unwrap()).is_err());
    // These are declared values rather than update deletion instructions.
    assert!(record_edit::update(&world, TRADITION, REQUESTS[4]).is_ok());
}

#[test]
fn common_links_keep_source_order_and_retirement_does_not_delete_authored_values() {
    let (_dir, base) = fixture();
    let mut world = with_concepts(&base);
    let original = world.records[ANA].clone();
    world = save(&world, Some(ANA), REQUESTS[3]);
    retained(&original, &world.records[ANA], &["concept_links"]);
    for (point, count) in [
        (at(11, 0), 1),
        (at(12, 0), 2),
        (at(16, 0), 2),
        (at(16, 1), 1),
    ] {
        assert_eq!(
            lore::links(&world, ANA, Some(&point), "author", "author", 100).unwrap()["count"],
            count
        );
    }
    let world_id = world.world_record().unwrap().id.clone();
    let world_link = REQUESTS[3].replacen("kind: character", "kind: world", 1);
    world = save(&world, Some(&world_id), &world_link);
    assert_eq!(
        lore::links(&world, &world_id, Some(&at(15, 0)), "author", "author", 100).unwrap()["count"],
        2
    );
    world = save(&world, Some(SYMBOL), REQUESTS[6]);
    let concept = lore::concept(&world, TRADITION, Some(&at(15, 0)), "author", "author").unwrap();
    assert_eq!(concept["values"]["symbols"], serde_json::json!([]));
    assert_eq!(
        world.records[TRADITION].get("values").unwrap()["symbols"][0]["id"],
        yaml(SYMBOL)
    );
    assert_eq!(
        lore::links(&world, ANA, Some(&at(15, 0)), "author", "author", 100).unwrap()["count"],
        1
    );
    world = save(&world, Some(ANA), REQUESTS[7]);
    retained(&original, &world.records[ANA], &["concept_links"]);
    assert_eq!(
        lore::links(&world, ANA, None, "author", "author", 100).unwrap()["count"],
        0
    );
    assert_eq!(
        record_edit::update(&world, ANA, REQUESTS[7])
            .unwrap()
            .raw_bytes,
        world.records[ANA].raw_bytes
    );
}

#[test]
fn literal_fields_declared_nulls_large_numbers_and_common_annotations_round_trip() {
    let (_dir, base) = fixture();
    let mut world = with_concepts(&base);
    let request = values_request(|v| {
        v["description"] = yaml("concept_00000000000000000000000999");
        v["chronology"] = yaml("calendar_00000000000000000000000999");
        v["offering"]["amount"] = yaml("18446744073709551615");
    });
    world = save(&world, Some(TRADITION), &request);
    let read = lore::concept(&world, TRADITION, None, "author", "author").unwrap();
    assert_eq!(read["values"]["offering"]["amount"], "18446744073709551615");
    assert_eq!(
        read["valueTypes"]["fields"]["offering"]["fields"]["amount"]["type"],
        "number"
    );
    assert!(read["values"]["origin_story"].is_null());
    assert_eq!(read["references"].as_array().unwrap().len(), 3);
    assert_eq!(
        world.records[TRADITION].get("values").unwrap()["offering"]["amount"].as_u64(),
        Some(u64::MAX)
    );
    let id = world.world_record().unwrap().id.clone();
    world = save(
        &world,
        Some(&id),
        include_str!("../examples/authoring/19-world-chronology.yaml"),
    );
    let chronology = include_str!("../examples/authoring/20-character-chronology.yaml").replacen(
        "kind: character",
        "kind: lore-concept",
        1,
    );
    world = save(&world, Some(TRADITION), &chronology);
    assert!(world.records[TRADITION].list("chronology").is_some());
    assert_eq!(
        world.records[TRADITION].get("values").unwrap()["chronology"],
        yaml("calendar_00000000000000000000000999")
    );
}

#[test]
fn typed_lore_and_links_preserve_complete_generic_character_responses() {
    let (_dir, before) = fixture();
    let mut after = with_concepts(&before);
    for (id, request) in [
        (ANA, REQUESTS[3]),
        (TRADITION, REQUESTS[4]),
        (SYMBOL, REQUESTS[6]),
    ] {
        after = save(&after, Some(id), request);
    }
    assert_ne!(before.fingerprint, after.fingerprint);
    for point in [at(4, 0), at(15, 0), at(21, 0)] {
        assert_eq!(
            character::context(&before, ANA, &point, 100_000).unwrap(),
            character::context(&after, ANA, &point, 100_000).unwrap()
        );
        for query in ["lantern", "harbor", "coin", "concept"] {
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
}

#[test]
fn all_eight_numbered_requests_preview_and_save_through_cli_with_exact_bytes() {
    let (_dir, mut world) = fixture();
    let world_id = world.world_record().unwrap().id.clone();
    for (index, request) in REQUESTS.iter().enumerate() {
        let target = match index {
            0 | 5 => Some(world_id.as_str()),
            3 | 7 => Some(ANA),
            4 => Some(TRADITION),
            6 => Some(SYMBOL),
            _ => None,
        };
        let path = world
            .root
            .parent()
            .unwrap()
            .join(format!("request-{}.yaml", index + 64));
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
    assert_eq!(world.records[SYMBOL].status(), "retired");
    assert!(world.records[ANA].list("concept_links").unwrap().is_empty());
    assert_eq!(
        world.records[TRADITION].get("values").unwrap()["gathering_place"]["id"],
        yaml(LANDING)
    );
}
