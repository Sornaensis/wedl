//! Reviewed family evidence feeds explicit character and writing selections.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::fs;
use wedl::{
    Error, authoring, character, environment, magic,
    model::{StoryTime, World},
    social, source, species, validation, writing_frames,
};
const ANA: &str = "char_00000000000000000000000001";
const SCENE: &str = "scene_00000000000000000000000002";
const REQUESTS: [&str; 28] = [
    include_str!("../examples/authoring/176-public-tide-description.yaml"),
    include_str!("../examples/authoring/177-public-river-description.yaml"),
    include_str!("../examples/authoring/178-public-individual-description.yaml"),
    include_str!("../examples/authoring/179-learn-individual-description.yaml"),
    include_str!("../examples/authoring/180-learn-river-description.yaml"),
    include_str!("../examples/authoring/181-learn-common-description-later.yaml"),
    include_str!("../examples/authoring/182-public-river-language.yaml"),
    include_str!("../examples/authoring/183-public-river-sense.yaml"),
    include_str!("../examples/authoring/184-public-later-river-spelling.yaml"),
    include_str!("../examples/authoring/185-public-harbor-script.yaml"),
    include_str!("../examples/authoring/186-public-river-religion.yaml"),
    include_str!("../examples/authoring/187-learn-river-language.yaml"),
    include_str!("../examples/authoring/188-learn-river-sense.yaml"),
    include_str!("../examples/authoring/189-learn-later-river-spelling.yaml"),
    include_str!("../examples/authoring/190-learn-harbor-script-later.yaml"),
    include_str!("../examples/authoring/191-learn-river-religion.yaml"),
    include_str!("../examples/authoring/192-learned-social-account.yaml"),
    include_str!("../examples/authoring/193-public-reed-trade.yaml"),
    include_str!("../examples/authoring/194-public-reed-resource.yaml"),
    include_str!("../examples/authoring/195-public-reed-livelihood.yaml"),
    include_str!("../examples/authoring/196-learn-reed-trade.yaml"),
    include_str!("../examples/authoring/197-learn-reed-resource.yaml"),
    include_str!("../examples/authoring/198-learn-reed-livelihood-later.yaml"),
    include_str!("../examples/authoring/199-learned-trade-account.yaml"),
    include_str!("../examples/authoring/200-middle-practice-dated-explanation.yaml"),
    include_str!("../examples/authoring/201-middle-practice-account.yaml"),
    include_str!("../examples/authoring/202-learned-magic-account.yaml"),
    include_str!("../examples/authoring/203-correct-middle-practice-account.yaml"),
];

fn id(prefix: &str, n: usize) -> String {
    format!("{prefix}_{n:026}")
}
fn at(tick: i64) -> StoryTime {
    StoryTime::new("main", tick, 0).unwrap()
}
fn yaml(value: &impl serde::Serialize) -> String {
    serde_yaml::to_string(value).unwrap()
}
fn unchanged(world: &World) {
    let loaded = source::load_world(&world.root, None).unwrap();
    assert_eq!(loaded.fingerprint, world.fingerprint);
    for (id, record) in &world.records {
        assert_eq!(loaded.records[id].raw_bytes, record.raw_bytes);
    }
}
fn save(world: &World, target: Option<&str>, input: &str) -> World {
    let preview = match target {
        Some(id) => authoring::preview_record_update(world, id, input, &world.fingerprint),
        None => authoring::preview_record_create(world, input, &world.fingerprint),
    }
    .unwrap();
    assert_eq!(preview["canApply"], true, "{preview}");
    assert_eq!(preview["writesPerformed"], false);
    unchanged(world);
    let hash = preview["afterSha256"].as_str().unwrap();
    match target {
        Some(id) => authoring::apply_record_update(world, id, input, &world.fingerprint, hash),
        None => authoring::apply_record_create(world, input, &world.fingerprint, hash),
    }
    .unwrap();
    let loaded = source::load_world(&world.root, None).unwrap();
    assert_eq!(loaded.fingerprint, preview["candidateFingerprint"]);
    let edited = preview["entityId"].as_str().unwrap();
    assert_eq!(
        fs::read(world.root.join(preview["sourcePath"].as_str().unwrap())).unwrap(),
        preview["afterSource"].as_str().unwrap().as_bytes()
    );
    for (id, record) in &world.records {
        if id != edited {
            assert_eq!(loaded.records[id].raw_bytes, record.raw_bytes);
        }
    }
    let report = validation::validate_world(&loaded);
    assert!(report.valid, "{:?}", report.diagnostics);
    loaded
}
fn rejected(world: &World, target: &str, input: &str) {
    match authoring::preview_record_update(world, target, input, &world.fingerprint) {
        Ok(preview) => {
            assert_eq!(preview["canApply"], false, "{preview}");
            assert!(
                authoring::apply_record_update(
                    world,
                    target,
                    input,
                    &world.fingerprint,
                    preview["afterSha256"].as_str().unwrap()
                )
                .is_err()
            );
        }
        Err(error) => assert!(
            matches!(error, Error::InvalidInput(_) | Error::Validation(_)),
            "{error}"
        ),
    }
    unchanged(world);
}
fn fixture() -> (tempfile::TempDir, World) {
    let dir = tempfile::tempdir().unwrap();
    let world = source::init_world(&dir.path().join("world"), "Reviewed family capture").unwrap();
    let owner = world.world_record().unwrap();
    let mut fields = owner.frontmatter.clone();
    fields["capabilities"] =
        serde_yaml::from_str("[lore-core-v1, descriptions-v1, assertions-v1]").unwrap();
    fs::write(
        world.root.join(&owner.source_path),
        format!("---\n{}---\n{}", yaml(&fields), owner.body),
    )
    .unwrap();
    let mut world = source::load_world(&world.root, None).unwrap();
    let owner_id = world.world_record().unwrap().id.clone();
    world = save(
        &world,
        Some(&owner_id),
        include_str!("../examples/authoring/126-world-governance-profile.yaml"),
    );
    for input in [
        include_str!("../examples/authoring/01-location.yaml"),
        include_str!("../examples/authoring/02-character.yaml"),
        include_str!("../examples/authoring/03-object.yaml"),
        include_str!("../examples/authoring/04-event.yaml"),
        include_str!("../examples/authoring/05-knowledge.yaml"),
        include_str!("../examples/authoring/21-rowan.yaml"),
        include_str!("../examples/authoring/73-tide-folk-common.yaml"),
        include_str!("../examples/authoring/74-river-folk-common.yaml"),
        include_str!("../examples/authoring/75-ana-individual-variation.yaml"),
        include_str!("../examples/authoring/81-lantern-magic-tradition.yaml"),
        include_str!("../examples/authoring/82-tidal-magic-realm.yaml"),
        include_str!("../examples/authoring/83-lantern-magic-artifact.yaml"),
        include_str!("../examples/authoring/85-middle-lantern-calling.yaml"),
        include_str!("../examples/authoring/99-harbor-script.yaml"),
        include_str!("../examples/authoring/100-river-language.yaml"),
        include_str!("../examples/authoring/102-ceremonial-register.yaml"),
        include_str!("../examples/authoring/103-river-sense.yaml"),
        include_str!("../examples/authoring/107-later-river-spelling.yaml"),
        include_str!("../examples/authoring/110-harbor-culture.yaml"),
        include_str!("../examples/authoring/111-river-religion.yaml"),
        include_str!("../examples/authoring/113-communal-doctrine.yaml"),
        include_str!("../examples/authoring/114-individual-doctrine-account.yaml"),
        include_str!("../examples/authoring/115-correct-communal-doctrine.yaml"),
        include_str!("../examples/authoring/116-harbor-authored-era.yaml"),
        include_str!("../examples/authoring/117-reed-resource.yaml"),
        include_str!("../examples/authoring/118-reed-habitat.yaml"),
        include_str!("../examples/authoring/119-reed-weaving.yaml"),
        include_str!("../examples/authoring/120-harbor-workshop.yaml"),
        include_str!("../examples/authoring/121-reed-livelihood.yaml"),
        include_str!("../examples/authoring/122-reed-trade.yaml"),
        include_str!("../examples/authoring/123-trade-availability.yaml"),
        include_str!("../examples/authoring/124-contrary-trade-account.yaml"),
        include_str!("../examples/authoring/125-unresolved-trade-dependency.yaml"),
        include_str!("../examples/authoring/160-ferry-discussion-scene.yaml"),
    ] {
        world = save(&world, None, input);
    }
    (dir, world)
}
fn extend(mut world: World) -> World {
    for (offset, input) in REQUESTS.iter().enumerate() {
        let target = match offset + 176 {
            176 => Some(3),
            177 => Some(4),
            178 => Some(5),
            182 => Some(13),
            183 => Some(16),
            184 => Some(20),
            185 => Some(12),
            186 => Some(24),
            193 => Some(32),
            194 => Some(27),
            195 => Some(31),
            200 => Some(10),
            _ => None,
        }
        .map(|n| id("concept", n));
        world = save(&world, target.as_deref(), input);
    }
    world
}
fn families() -> writing_frames::FamilySelection {
    writing_frames::decode_families(include_str!("../examples/writing/family-context.json"))
        .unwrap()
}
fn species_selection() -> species::Selection {
    families().species.unwrap().selection
}
fn social_selection() -> social::Selection {
    let s = families().social.unwrap();
    social::Selection {
        language: s.language,
        sense: s.sense,
        form: s.form,
        contexts: s.contexts,
    }
}
fn frame(world: &World, lens: &str, tick: i64) -> writing_frames::Frame {
    writing_frames::decode(
        &json!({"fingerprint":world.fingerprint,"work":null,"sceneId":SCENE,"povId":ANA,
        "at":{"timeline":"main","tick":tick.to_string(),"order":"0"},"lens":lens})
        .to_string(),
    )
    .unwrap()
}
fn species_at(world: &World, tick: i64) -> Json {
    species::character_compose(
        world,
        ANA,
        &id("concept", 5),
        &species_selection(),
        &[],
        &at(tick),
    )
    .unwrap()
}
fn social_at(world: &World, tick: i64) -> Json {
    social::character_context(
        world,
        ANA,
        &social_selection(),
        &[id("assertion", 8)],
        &at(tick),
    )
    .unwrap()
}
fn environment_at(world: &World, tick: i64) -> Json {
    environment::character_context(
        world,
        ANA,
        &[id("concept", 32)],
        &[id("assertion", 9)],
        &at(tick),
    )
    .unwrap()
}
fn magic_at(world: &World, tick: i64) -> Json {
    magic::character(world, ANA, &[id("assertion", 19)], &at(tick)).unwrap()
}
fn section_count(value: &Json) -> u64 {
    value["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|stream| stream["result"]["count"].as_u64().unwrap())
        .sum()
}
#[test]
fn twenty_eight_reviewed_requests_capture_independent_families_at_exact_horizons() {
    let (_dir, before) = fixture();
    let baseline = character::context(&before, ANA, &at(4), 100000).unwrap();
    let count = before.records.len();
    let world = extend(before);
    assert_eq!(world.records.len(), count + 16);
    assert_eq!(
        world
            .world_record()
            .unwrap()
            .list("profiles")
            .unwrap()
            .len(),
        25
    );
    assert_eq!(
        world
            .world_record()
            .unwrap()
            .list("predicates")
            .unwrap()
            .len(),
        9
    );
    assert_eq!(
        character::context(&world, ANA, &at(4), 100000).unwrap(),
        baseline
    );
    let s12 = species_at(&world, 12);
    let s13 = species_at(&world, 13);
    let s17 = species_at(&world, 17);
    assert_eq!(s12["common"].as_array().unwrap().len(), 1);
    assert_eq!(s13["common"].as_array().unwrap().len(), 2);
    assert_eq!(s17["common"].as_array().unwrap().len(), 1);
    assert_eq!(section_count(&s12), 1);
    assert_eq!(section_count(&s13), 2);
    assert_eq!(social_at(&world, 12)["count"], 4);
    assert_eq!(social_at(&world, 13)["count"], 5);
    let e12 = environment_at(&world, 12);
    let e13 = environment_at(&world, 13);
    assert_eq!(e12["count"], 2);
    assert_eq!(e12["links"].as_array().unwrap().len(), 1);
    assert_eq!(e13["count"], 3);
    assert_eq!(e13["links"].as_array().unwrap().len(), 2);
    assert_eq!(environment_at(&world, 17)["count"], 2);
    let learned = magic_at(&world, 12);
    assert_eq!(learned["count"], 1);
    assert_eq!(
        learned["beliefs"][0]["result"]["items"][0]["state"],
        "rejected"
    );
    assert_eq!(magic_at(&world, 16)["outcome"], "unknown");
    let author_magic = families().magic.unwrap().author.unwrap();
    let author = magic::compose(
        &world,
        &author_magic.root_id,
        &author_magic.selection,
        Some(&at(12)),
        "author",
        "author",
    )
    .unwrap();
    assert_eq!(
        author["canon"][0]["result"]["items"][0]["assertionId"],
        id("assertion", 20)
    );
    assert_eq!(section_count(&author), 1);
    let author_species = species::compose(
        &world,
        &id("concept", 5),
        &species_selection(),
        Some(&at(12)),
        "author",
        "author",
    )
    .unwrap();
    assert_eq!(section_count(&author_species), 3); // individual, corrected common and independent author-only section
    let selected = families();
    let character = writing_frames::context_with_families(
        &world,
        None,
        &frame(&world, "character", 12),
        &[],
        &selected,
        100000,
    )
    .unwrap();
    let dramatic = writing_frames::context_with_families(
        &world,
        None,
        &frame(&world, "dramatic-irony", 12),
        &[],
        &selected,
        100000,
    )
    .unwrap();
    assert_eq!(character["characterPrompt"], dramatic["characterPrompt"]);
    assert_eq!(
        character["characterPrompt"]["familyContexts"]
            .as_object()
            .unwrap()
            .len(),
        4
    );
    assert!(dramatic["authorMargin"]["familyContexts"]["magic"]["canon"].is_array());
    let text = character["characterPrompt"].to_string();
    for forbidden in [
        "PRIVATE_FAMILY_SECTION",
        "AUTHOR_FAMILY_ACQUISITION",
        "AUTHOR_FAMILY_REPORT_NOTE",
        "UNREAD_FAMILY_KNOWLEDGE_BODY",
        "UNREAD_FAMILY_REPORT_BODY",
        "AUTHOR_MAGIC_RULE_BODY",
        "AUTHOR_MAGIC_CORRECTION_BODY",
        "The witnessed vow must be spoken",
        "sourcePath",
        "sourceField",
        "fingerprint",
    ] {
        assert!(!text.contains(forbidden), "leaked {forbidden}");
    }
    assert!(text.contains("I learned that middle calling needs no witness."));
    assert!(text.contains("Someone told me river gatherings forbid boats."));
    assert!(text.contains("I was told no reed carts may reach the dock."));
    unchanged(&world);
}
#[test]
fn reviewed_edits_preserve_family_admission_and_refuse_invalid_shared_source() {
    let (_dir, before) = fixture();
    let world = extend(before);
    let selected = families();
    let old_frame = frame(&world, "character", 13);
    let old_packet =
        writing_frames::context_with_families(&world, None, &old_frame, &[], &selected, 100000)
            .unwrap();
    let expected = [
        species_at(&world, 13),
        social_at(&world, 13),
        environment_at(&world, 13),
        magic_at(&world, 13),
    ];
    let altered = save(
        &world,
        Some(&id("assertion", 20)),
        &yaml(
            &json!({"kind":"assertion","value":"A changed canonical rule still grants no learned wording.","body":"PRIVATE_CANON_ONLY_EDIT\n"}),
        ),
    );
    assert!(
        writing_frames::context_with_families(&altered, None, &old_frame, &[], &selected, 100000)
            .is_err()
    );
    assert_eq!(
        [
            species_at(&altered, 13),
            social_at(&altered, 13),
            environment_at(&altered, 13),
            magic_at(&altered, 13)
        ],
        expected
    );
    let recaptured = writing_frames::context_with_families(
        &altered,
        None,
        &frame(&altered, "character", 13),
        &[],
        &selected,
        100000,
    )
    .unwrap();
    assert_eq!(recaptured["characterPrompt"], old_packet["characterPrompt"]);
    let mut values = altered.records[&id("concept", 4)]
        .get("values")
        .unwrap()
        .clone();
    values["parents"].as_sequence_mut().unwrap().insert(
        0,
        serde_yaml::to_value(json!({"kind":"record","id":id("concept",6)})).unwrap(),
    );
    let shifted = save(
        &altered,
        Some(&id("concept", 4)),
        &yaml(&json!({"kind":"lore-concept","values":values})),
    );
    assert_eq!(species_at(&shifted, 13), expected[0]);
    let known = &shifted.records[&id("know", 37)];
    let mut claim = known.get("claim").unwrap().clone();
    claim["subject"] = Yaml::String(ANA.into());
    rejected(
        &shifted,
        &known.id,
        &yaml(&json!({"kind":"knowledge","claim":claim})),
    );
    let mut bad = shifted.records[&id("concept", 4)]
        .get("values")
        .unwrap()
        .clone();
    bad["parents"] = Yaml::String("literal words cannot become a typed link".into());
    rejected(
        &shifted,
        &id("concept", 4),
        &yaml(&json!({"kind":"lore-concept","values":bad})),
    );
    unchanged(&shifted);
}
