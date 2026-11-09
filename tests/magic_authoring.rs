//! Reviewed magic edits preserve existing state, individual practices and character evidence.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeMap, fs};
use wedl::{
    Error, authoring, character, lore_state,
    magic::{self, Selection},
    model::{StoryTime, World},
    record_edit, source, validation,
};

const ANA: &str = "char_00000000000000000000000001";
const TRADITION: &str = "concept_00000000000000000000000006";
const EARLY: &str = "concept_00000000000000000000000009";
const MIDDLE: &str = "concept_00000000000000000000000010";
const LATE: &str = "concept_00000000000000000000000011";
const REQUESTS: [&str; 8] = [
    include_str!("../examples/authoring/80-world-magic-profiles.yaml"),
    include_str!("../examples/authoring/81-lantern-magic-tradition.yaml"),
    include_str!("../examples/authoring/82-tidal-magic-realm.yaml"),
    include_str!("../examples/authoring/83-lantern-magic-artifact.yaml"),
    include_str!("../examples/authoring/84-early-lantern-calling.yaml"),
    include_str!("../examples/authoring/85-middle-lantern-calling.yaml"),
    include_str!("../examples/authoring/86-late-lantern-calling.yaml"),
    include_str!("../examples/authoring/87-magic-tradition-update.yaml"),
];
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn save(world: &World, target: Option<&str>, request: &str) -> World {
    let preview = match target {
        Some(id) => authoring::preview_record_update(world, id, request, &world.fingerprint),
        None => authoring::preview_record_create(world, request, &world.fingerprint),
    }
    .unwrap();
    assert_eq!(preview["canApply"], true, "{preview}");
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
    let hash = preview["afterSha256"].as_str().unwrap();
    match target {
        Some(id) => authoring::apply_record_update(world, id, request, &world.fingerprint, hash),
        None => authoring::apply_record_create(world, request, &world.fingerprint, hash),
    }
    .unwrap();
    assert_eq!(
        fs::read(world.root.join(preview["sourcePath"].as_str().unwrap())).unwrap(),
        preview["afterSource"].as_str().unwrap().as_bytes()
    );
    let saved = source::load_world(&world.root, None).unwrap();
    assert_eq!(saved.fingerprint, preview["candidateFingerprint"]);
    let report = validation::validate_world(&saved);
    assert!(report.valid, "{:?}", report.diagnostics);
    saved
}
fn fixture() -> (tempfile::TempDir, World) {
    let dir = tempfile::tempdir().unwrap();
    let base = source::init_world(&dir.path().join("world"), "Reviewed magic accounts").unwrap();
    let path = base.root.join("story/world.md");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("capabilities: []", "capabilities: [lore-core-v1]"),
    )
    .unwrap();
    let mut world = source::load_world(&base.root, None).unwrap();
    for request in [
        include_str!("../examples/authoring/01-location.yaml"),
        include_str!("../examples/authoring/02-character.yaml"),
        include_str!("../examples/authoring/04-event.yaml"),
        include_str!("../examples/authoring/05-knowledge.yaml"),
    ] {
        world = save(&world, None, request);
    }
    let world_id = world.world_record().unwrap().id.clone();
    for (target, request) in [
        (
            Some(world_id.as_str()),
            include_str!("../examples/authoring/19-world-chronology.yaml"),
        ),
        (
            Some(world_id.as_str()),
            include_str!("../examples/authoring/72-world-state-profile-bindings.yaml"),
        ),
        (
            None,
            include_str!("../examples/authoring/73-tide-folk-common.yaml"),
        ),
        (
            None,
            include_str!("../examples/authoring/74-river-folk-common.yaml"),
        ),
        (
            None,
            include_str!("../examples/authoring/75-ana-individual-variation.yaml"),
        ),
        (
            Some(ANA),
            include_str!("../examples/authoring/76-character-bound-state.yaml"),
        ),
        (
            None,
            include_str!("../examples/authoring/77-bound-state-set-event.yaml"),
        ),
        (
            Some("concept_00000000000000000000000003"),
            include_str!("../examples/authoring/78-common-description-update.yaml"),
        ),
        (
            None,
            include_str!("../examples/authoring/79-bound-state-clear-event.yaml"),
        ),
    ] {
        world = save(&world, target, request);
    }
    (dir, world)
}
fn selection() -> Selection {
    let roles = ["tradition", "practice", "realm", "artifact"];
    Selection {
        profiles: roles
            .iter()
            .map(|r| (r.to_string(), format!("magic-{r}")))
            .collect(),
        links: roles
            .iter()
            .map(|r| (r.to_string(), vec!["links".into()]))
            .collect(),
        restrictions: roles
            .iter()
            .map(|r| (r.to_string(), "restrictions".into()))
            .collect(),
        exceptions: roles
            .iter()
            .map(|r| (r.to_string(), "exceptions".into()))
            .collect(),
    }
}
fn compose(world: &World, id: &str, tick: i64, order: i32) -> Json {
    magic::compose(
        world,
        id,
        &selection(),
        Some(&at(tick, order)),
        "author",
        "author",
    )
    .unwrap()
}
fn through(world: &World, count: usize) -> World {
    let mut world = world.clone();
    let world_id = world.world_record().unwrap().id.clone();
    for (i, request) in REQUESTS.iter().take(count).enumerate() {
        let target = match i {
            0 => Some(world_id.as_str()),
            7 => Some(TRADITION),
            _ => None,
        };
        world = save(&world, target, request);
    }
    world
}

#[test]
fn reviewed_magic_sequence_preserves_bound_state_practices_and_character_evidence() {
    let (_dir, before) = fixture();
    let character_before = character::context(&before, ANA, &at(4, 0), 100_000).unwrap();
    let search_before = character::search(&before, ANA, "lantern", &at(4, 0), 100).unwrap();
    let state_before = lore_state::state(&before, ANA, &at(12, 0), "author", "author").unwrap();
    let world = through(&before, 7);
    let practices: BTreeMap<_, _> = [EARLY, MIDDLE, LATE]
        .into_iter()
        .map(|id| (id, world.records[id].raw_bytes.clone()))
        .collect();
    let old = compose(&world, EARLY, 5, 0);
    let world = save(&world, Some(TRADITION), REQUESTS[7]);
    let current = compose(&world, EARLY, 5, 0);
    assert_eq!(current["linkageComplete"], true);
    assert_eq!(current["unknowns"], json!([]));
    assert_eq!(
        current["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["role"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["practice", "tradition", "realm", "artifact"]
    );
    assert_eq!(current["links"].as_array().unwrap().len(), 4);
    let restrictions = &current["records"][0]["fields"]["restrictions"];
    assert_eq!(restrictions["value"]["count"], "9007199254740993");
    assert_eq!(restrictions["value"]["ratio"].as_f64(), Some(1.0));
    assert_eq!(
        restrictions["value"]["literal"]["id"],
        "concept_00000000000000000000000099"
    );
    assert_eq!(restrictions["references"], json!([]));
    assert_eq!(
        current["records"][2]["fields"]["restrictions"]["known"],
        false
    );
    assert_eq!(current["records"][2]["fields"]["exceptions"]["known"], true);
    assert!(current["records"][2]["fields"]["exceptions"]["value"].is_null());
    assert_eq!(
        current["records"][3]["fields"]["exceptions"]["value"],
        json!([])
    );
    assert_ne!(
        old["records"][1]["fields"]["restrictions"]["value"],
        current["records"][1]["fields"]["restrictions"]["value"]
    );
    for (id, bytes) in practices {
        assert_eq!(world.records[id].raw_bytes, bytes);
    }
    for (id, record) in &before.records {
        if record.kind != "world" {
            assert_eq!(world.records[id].raw_bytes, record.raw_bytes);
        }
    }
    let before_world = before.world_record().unwrap();
    let after_world = world.world_record().unwrap();
    for field in ["state_keys", "state_profile_bindings", "chronology"] {
        assert_eq!(after_world.get(field), before_world.get(field));
    }
    assert_eq!(after_world.body, before_world.body);
    assert_eq!(
        record_edit::update(&world, &after_world.id, REQUESTS[0])
            .unwrap()
            .raw_bytes,
        after_world.raw_bytes
    );
    assert_eq!(
        lore_state::state(&world, ANA, &at(12, 0), "author", "author").unwrap()["state"],
        state_before["state"]
    );
    assert_eq!(
        character::context(&world, ANA, &at(4, 0), 100_000).unwrap(),
        character_before
    );
    assert_eq!(
        character::search(&world, ANA, "lantern", &at(4, 0), 100).unwrap(),
        search_before
    );
    for (id, start, end) in [(EARLY, 0, 9), (MIDDLE, 10, 19), (LATE, 20, 29)] {
        for (tick, order) in [(start, 0), (end, 0)] {
            assert_eq!(
                compose(&world, id, tick, order)["records"]
                    .as_array()
                    .unwrap()
                    .len(),
                4
            );
        }
        for (tick, order) in [(start, -1), (end, 1)] {
            assert!(matches!(
                magic::compose(
                    &world,
                    id,
                    &selection(),
                    Some(&at(tick, order)),
                    "author",
                    "author"
                ),
                Err(Error::NotFound(_))
            ));
        }
    }
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
}

#[test]
fn reviewed_profile_changes_validate_all_magic_records_and_keep_literal_objects_literal() {
    let (_dir, base) = fixture();
    let world = through(&base, 8);
    let world_id = &world.world_record().unwrap().id;
    let original: Yaml = serde_yaml::from_str(REQUESTS[0]).unwrap();
    let mut removed = original.clone();
    removed["profiles"]
        .as_sequence_mut()
        .unwrap()
        .retain(|p| p["id"] != "magic-artifact");
    let mut changed_count = original.clone();
    changed_count["profiles"][6]["fields"]["required"]["restrictions"]["required"]["count"] =
        serde_yaml::from_str("{type: string}").unwrap();
    let mut changed_literal = original;
    changed_literal["profiles"][6]["fields"]["required"]["restrictions"]["required"]["literal"] =
        serde_yaml::from_str("{type: entity, kinds: [lore-concept]}").unwrap();
    for invalid in [removed, changed_count, changed_literal] {
        let request = serde_yaml::to_string(&invalid).unwrap();
        let preview =
            authoring::preview_record_update(&world, world_id, &request, &world.fingerprint)
                .unwrap();
        assert_eq!(preview["canApply"], false, "{preview}");
        assert!(matches!(
            authoring::apply_record_update(
                &world,
                world_id,
                &request,
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
