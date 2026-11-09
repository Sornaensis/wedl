//! Reviewed context briefs preserve source and feed exact independent writing frames.
use serde_json::json;
use serde_yaml::Value as Yaml;
use std::fs;
use wedl::{
    Error, authoring, briefs, character,
    model::{StoryTime, World},
    source, validation, writing_frames,
};
const ANA: &str = "char_00000000000000000000000001";
const HARBOR: &str = "loc_00000000000000000000000001";
const BELL: &str = "obj_00000000000000000000000001";
const NOTICE: &str = "concept_00000000000000000000000037";
const SCENE: &str = "scene_00000000000000000000000002";
const REQUESTS: [&str; 17] = [
    include_str!("../examples/authoring/159-bell-dated-description.yaml"),
    include_str!("../examples/authoring/160-ferry-discussion-scene.yaml"),
    include_str!("../examples/authoring/161-ferry-detail-brief.yaml"),
    include_str!("../examples/authoring/162-learn-ferry-detail-brief.yaml"),
    include_str!("../examples/authoring/163-harbor-writing-brief.yaml"),
    include_str!("../examples/authoring/164-learn-harbor-writing-brief.yaml"),
    include_str!("../examples/authoring/165-withheld-required-brief.yaml"),
    include_str!("../examples/authoring/166-learn-withheld-required-brief.yaml"),
    include_str!("../examples/authoring/167-learned-report-brief.yaml"),
    include_str!("../examples/authoring/168-learn-report-brief.yaml"),
    include_str!("../examples/authoring/169-canonical-rule-brief.yaml"),
    include_str!("../examples/authoring/170-learn-canonical-rule-brief.yaml"),
    include_str!("../examples/authoring/171-repeat-nested-required-pin.yaml"),
    include_str!("../examples/authoring/172-revise-authored-brief-prose.yaml"),
    include_str!("../examples/authoring/173-reciprocal-setting-brief.yaml"),
    include_str!("../examples/authoring/174-learn-reciprocal-setting-brief.yaml"),
    include_str!("../examples/authoring/175-close-required-brief-cycle.yaml"),
];
fn id(prefix: &str, n: usize) -> String {
    format!("{prefix}_{n:026}")
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn request(value: &impl serde::Serialize) -> String {
    serde_yaml::to_string(value).unwrap()
}
fn unchanged(world: &World) {
    let current = source::load_world(&world.root, None).unwrap();
    assert_eq!(current.fingerprint, world.fingerprint);
    for (id, record) in &world.records {
        assert_eq!(current.records[id].raw_bytes, record.raw_bytes);
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
    let current = source::load_world(&world.root, None).unwrap();
    assert_eq!(current.fingerprint, preview["candidateFingerprint"]);
    let edited = preview["entityId"].as_str().unwrap();
    assert_eq!(
        fs::read(world.root.join(preview["sourcePath"].as_str().unwrap())).unwrap(),
        preview["afterSource"].as_str().unwrap().as_bytes()
    );
    for (id, record) in &world.records {
        if id != edited {
            assert_eq!(current.records[id].raw_bytes, record.raw_bytes);
        }
    }
    let validation = validation::validate_world(&current);
    assert!(validation.valid, "{:?}", validation.diagnostics);
    current
}
fn rejected(world: &World, target: Option<&str>, input: &str) {
    let preview = match target {
        Some(id) => authoring::preview_record_update(world, id, input, &world.fingerprint),
        None => authoring::preview_record_create(world, input, &world.fingerprint),
    };
    match preview {
        Ok(preview) => {
            assert_eq!(preview["canApply"], false, "{preview}");
            let hash = preview["afterSha256"].as_str().unwrap();
            let applied = match target {
                Some(id) => {
                    authoring::apply_record_update(world, id, input, &world.fingerprint, hash)
                }
                None => authoring::apply_record_create(world, input, &world.fingerprint, hash),
            };
            assert!(matches!(applied, Err(Error::Validation(_))), "{applied:?}");
        }
        Err(error) => assert!(
            matches!(error, Error::InvalidInput(_) | Error::Validation(_)),
            "{error}"
        ),
    }
    unchanged(world);
}
fn direct_source(world: &World, target: &str, change: impl FnOnce(&mut Yaml)) -> World {
    let record = &world.records[target];
    let mut fields = record.frontmatter.clone();
    change(&mut fields);
    fs::write(
        world.root.join(&record.source_path),
        format!("---\n{}---\n{}", request(&fields), record.body),
    )
    .unwrap();
    source::load_world(&world.root, None).unwrap()
}

fn fixture() -> (tempfile::TempDir, World) {
    let dir = tempfile::tempdir().unwrap();
    let mut world =
        source::init_world(&dir.path().join("world"), "Reviewed writing frames").unwrap();
    let world_id = world.world_record().unwrap().id.clone();
    world = direct_source(&world, &world_id, |fields| {
        fields["capabilities"] =
            serde_yaml::from_str("[lore-core-v1, descriptions-v1, assertions-v1, briefs-v1]")
                .unwrap();
    });
    for input in [
        include_str!("../examples/authoring/01-location.yaml"),
        include_str!("../examples/authoring/02-character.yaml"),
        include_str!("../examples/authoring/03-object.yaml"),
        include_str!("../examples/authoring/04-event.yaml"),
        include_str!("../examples/authoring/05-knowledge.yaml"),
        include_str!("../examples/authoring/21-rowan.yaml"),
    ] {
        world = save(&world, None, input);
    }
    world = save(
        &world,
        Some(&world_id),
        include_str!("../examples/authoring/126-world-governance-profile.yaml"),
    );
    world = save(
        &world,
        Some(&world_id),
        include_str!("../examples/authoring/19-world-chronology.yaml"),
    );
    world = save(
        &world,
        None,
        &request(
            &json!({"kind":"lore-concept","id":id("concept",23),"title":"Private reference", "domain":"private.reference","profile":"context-resource","values":{},"valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]}),
        ),
    );
    for (input, target) in [
        (
            include_str!("../examples/authoring/143-public-reference-context.yaml"),
            None,
        ),
        (
            include_str!("../examples/authoring/144-public-reading-context.yaml"),
            None,
        ),
        (
            include_str!("../examples/authoring/145-canonical-ferry-rule.yaml"),
            None,
        ),
        (
            include_str!("../examples/authoring/146-learned-ferry-report.yaml"),
            None,
        ),
        (
            include_str!("../examples/authoring/148-learn-reading-context.yaml"),
            None,
        ),
        (
            include_str!("../examples/authoring/150-learn-harbor-record.yaml"),
            None,
        ),
        (
            include_str!("../examples/authoring/153-correct-public-reading-section.yaml"),
            Some(NOTICE),
        ),
        (
            include_str!("../examples/authoring/154-correct-canonical-ferry-rule.yaml"),
            None,
        ),
        (
            include_str!("../examples/authoring/158-dated-self-description.yaml"),
            Some(ANA),
        ),
    ] {
        world = save(&world, target, input);
    }
    (dir, world)
}
fn extend(mut world: World) -> World {
    for (offset, input) in REQUESTS.iter().enumerate() {
        let target = match offset + 159 {
            159 => Some(BELL.to_owned()),
            171 | 175 => Some(id("brief", 2)),
            172 => Some(id("brief", 1)),
            _ => None,
        };
        world = save(&world, target.as_deref(), input);
    }
    world
}
fn frame(world: &World, lens: &str, tick: i64) -> writing_frames::Frame {
    writing_frames::decode(
        &json!({"fingerprint":world.fingerprint,"work":null,"sceneId":SCENE,"povId":ANA,
        "at":{"timeline":"main","tick":tick.to_string(),"order":"0"},"lens":lens})
        .to_string(),
    )
    .unwrap()
}
#[test]
fn seventeen_reviewed_requests_preserve_exact_bytes_and_compose_deduplicated_cycles() {
    let (_dir, before) = fixture();
    let count = before.records.len();
    let baseline = character::context(&before, ANA, &at(4, 0), 100_000).unwrap();
    let world = extend(before);
    assert_eq!(world.records.len(), count + 13);
    assert_eq!(
        character::context(&world, ANA, &at(4, 0), 100_000).unwrap(),
        baseline
    );
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
    let author = briefs::compose(
        &world,
        &id("brief", 2),
        Some(&at(12, 0)),
        "author",
        "author",
        100,
    )
    .unwrap();
    assert_eq!(author["outcome"], "available");
    assert_eq!(author["count"], 7);
    assert_eq!(
        author["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["target"]["ownerId"] == NOTICE
                && item["target"]["id"] == id("description", 2))
            .count(),
        1
    );
    let standalone = briefs::character(&world, ANA, &id("brief", 2), &at(12, 0), 100).unwrap();
    assert_eq!(standalone["outcome"], "unknown");
    assert_eq!(standalone["count"], 0);
    let selected = frame(&world, "character", 12);
    let character_packet =
        writing_frames::context(&world, None, &selected, &[id("brief", 2)], 100_000).unwrap();
    let char_text = character_packet["characterPrompt"].to_string();
    assert!(char_text.contains("blue cord"));
    assert!(char_text.contains("morning bell"));
    for secret in [
        "UNTIMED_SCENE_PROSE",
        "UNREAD_BRIEF_KNOWLEDGE_BODY",
        "AUTHOR_BRIEF_ACQUISITION",
        "AUTHOR_ACQUISITION_NOTE",
        "Private reference",
        "CANONICAL_RULE_BODY",
        "sourcePath",
        "fingerprint",
    ] {
        assert!(!char_text.contains(secret), "{secret}: {char_text}");
    }
    let dramatic = writing_frames::context(
        &world,
        None,
        &frame(&world, "dramatic-irony", 12),
        &[id("brief", 2)],
        100_000,
    )
    .unwrap();
    assert_eq!(
        dramatic["characterPrompt"],
        character_packet["characterPrompt"]
    );
    assert!(dramatic.get("authorMargin").is_some());
    assert_eq!(
        writing_frames::context(
            &world,
            None,
            &selected,
            &[id("brief", 2), id("brief", 2)],
            100_000
        )
        .unwrap()["characterPrompt"],
        character_packet["characterPrompt"]
    );
    assert!(writing_frames::context(&world, None, &selected, &[id("brief", 2)], 256).is_err());
    unchanged(&world);
}
#[test]
fn selected_briefs_never_teach_pins_or_replace_learned_assertions_with_canon() {
    let (_dir, world) = fixture();
    let world = extend(world);
    let unknown = briefs::character(&world, ANA, &id("brief", 3), &at(12, 0), 1).unwrap();
    assert_eq!(unknown["outcome"], "unknown");
    assert_eq!(unknown["items"], json!([]));
    let learned = briefs::character(&world, ANA, &id("brief", 4), &at(12, 0), 100).unwrap();
    assert_eq!(learned["outcome"], "available");
    assert_eq!(learned["count"], 2);
    assert!(
        learned
            .to_string()
            .contains("Residents told me the ferry runs all night.")
    );
    assert!(!learned.to_string().contains("serves only"));
    assert_eq!(
        briefs::compose(
            &world,
            &id("brief", 4),
            Some(&at(12, 0)),
            "author",
            "author",
            100
        )
        .unwrap()["outcome"],
        "unknown"
    );
    assert_eq!(
        briefs::compose(&world, &id("brief", 4), None, "author", "author", 100).unwrap()["count"],
        2
    );
    assert_eq!(
        briefs::compose(
            &world,
            &id("brief", 5),
            Some(&at(12, 0)),
            "author",
            "author",
            100
        )
        .unwrap()["count"],
        2
    );
    assert_eq!(
        briefs::character(&world, ANA, &id("brief", 5), &at(12, 0), 100).unwrap()["outcome"],
        "unknown"
    );
    assert_eq!(
        briefs::character(&world, ANA, &id("brief", 4), &at(14, 0), 100).unwrap()["outcome"],
        "unknown"
    );
    assert!(
        writing_frames::context(
            &world,
            None,
            &frame(&world, "character", 12),
            &[id("brief", 3)],
            100_000
        )
        .is_err()
    );
    unchanged(&world);
}
#[test]
fn invalid_pins_scopes_and_referenced_member_removal_never_publish() {
    let (_dir, world) = fixture();
    let world = extend(world);
    for refs in [
        json!(null),
        json!([{"kind":"record","id":HARBOR,"extra":true}]),
        json!([{"kind":"record","id":HARBOR},{"kind":"record","id":HARBOR}]),
        json!([{"kind":"section","owner_id":HARBOR,"id":id("description",2)}]),
        json!([{"kind":"record","id":"Harbor"}]),
    ] {
        rejected(
            &world,
            Some(&id("brief", 1)),
            &request(&json!({"kind":"context-brief","required_refs":refs})),
        );
    }
    for fields in [
        json!({"scope":"world","scope_ref":{"kind":"record","id":HARBOR}}),
        json!({"scope":"era","scope_ref":{"kind":"record","id":HARBOR}}),
        json!({"scope":"era","scope_ref":{"kind":"section","owner_id":world.world_record().unwrap().id,"id":id("calendar",1)}}),
        json!({"scope":"region","scope_ref":{"kind":"record","id":ANA}}),
        json!({"audience":[]}),
        json!({"perspectives":["public","author"]}),
    ] {
        let mut input = fields;
        input["kind"] = json!("context-brief");
        rejected(&world, Some(&id("brief", 1)), &request(&input));
    }
    rejected(
        &world,
        Some(BELL),
        "kind: object
descriptions: []
",
    );
    let mut missing: Yaml = serde_yaml::from_str(REQUESTS[2]).unwrap();
    missing["id"] = Yaml::String(id("brief", 90));
    missing
        .as_mapping_mut()
        .unwrap()
        .remove(Yaml::String("required_refs".into()));
    rejected(&world, None, &request(&missing));
    let era = world.world_record().unwrap().id.clone();
    let valid = json!({"kind":"context-brief","id":id("brief",90),"title":"Era scope","domain":"harbor.writing","scope":"era","scope_ref":{"kind":"section","owner_id":era,"id":id("era",1)},"valid":{"kind":"timeless"},"required_refs":[],"audience":["author"],"perspectives":["author"]});
    let updated = save(&world, None, &request(&valid));
    assert_eq!(
        briefs::compose(&updated, &id("brief", 90), None, "author", "author", 100).unwrap()["count"],
        1
    );
}
#[test]
fn brief_updates_invalidate_current_frames_and_preserve_the_captured_world() {
    let (_dir, world) = fixture();
    let world = extend(world);
    let selected = frame(&world, "character", 12);
    let before =
        writing_frames::context(&world, None, &selected, &[id("brief", 2)], 100_000).unwrap();
    let original = world.records[&id("brief", 1)].clone();
    let updated = save(
        &world,
        Some(&id("brief", 1)),
        "kind: context-brief
body: |
  Revised explicit setting words.
",
    );
    for (key, value) in original.frontmatter.as_mapping().unwrap() {
        assert_eq!(
            updated.records[&id("brief", 1)].frontmatter.get(key),
            Some(value)
        );
    }
    assert!(writing_frames::resolve(&updated, None, &selected).is_err());
    assert_eq!(
        writing_frames::context(&world, None, &selected, &[id("brief", 2)], 100_000).unwrap(),
        before
    );
    let after = writing_frames::context(
        &updated,
        None,
        &frame(&updated, "character", 12),
        &[id("brief", 2)],
        100_000,
    )
    .unwrap();
    assert!(
        after["characterPrompt"]
            .to_string()
            .contains("Revised explicit setting words.")
    );
    assert_eq!(
        world.world_record().unwrap().get("current_time"),
        updated.world_record().unwrap().get("current_time")
    );
    assert_eq!(
        world.records[SCENE].get("time"),
        updated.records[SCENE].get("time")
    );
    unchanged(&updated);
}
