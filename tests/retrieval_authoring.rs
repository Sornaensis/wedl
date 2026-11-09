//! Reviewed native sections train a public prior without teaching their source.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::fs;
use wedl::{authoring, character, compiler, model::World, retrieval::Mode, source, validation};

const ANA: &str = "char_00000000000000000000000001";
const CLAIM: &str = "assertion_00000000000000000000000018";
const REPORT: &str = "know_00000000000000000000000038";
const NOTICE: &str = "obj_00000000000000000000000002";
const INPUTS: [&str; 4] = [
    include_str!("../examples/authoring/204-public-retrieval-prior.yaml"),
    include_str!("../examples/authoring/205-learned-ferry-account.yaml"),
    include_str!("../examples/authoring/206-ferry-notice.yaml"),
    include_str!("../examples/authoring/207-learn-ferry-notice.yaml"),
];

fn save(world: &World, target: Option<&str>, input: &str) -> World {
    let preview = match target {
        Some(id) => authoring::preview_record_update(world, id, input, &world.fingerprint),
        None => authoring::preview_record_create(world, input, &world.fingerprint),
    }
    .unwrap();
    assert_eq!(preview["canApply"], true, "{preview}");
    assert_eq!(preview["writesPerformed"], false);
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
    let expected = preview["afterSha256"].as_str().unwrap();
    match target {
        Some(id) => authoring::apply_record_update(world, id, input, &world.fingerprint, expected),
        None => authoring::apply_record_create(world, input, &world.fingerprint, expected),
    }
    .unwrap();
    let next = source::load_world(&world.root, None).unwrap();
    assert_eq!(next.fingerprint, preview["candidateFingerprint"]);
    assert_eq!(
        fs::read(world.root.join(preview["sourcePath"].as_str().unwrap())).unwrap(),
        preview["afterSource"].as_str().unwrap().as_bytes()
    );
    for (id, record) in &world.records {
        if id != preview["entityId"].as_str().unwrap() {
            assert_eq!(next.records[id].raw_bytes, record.raw_bytes);
        }
    }
    assert!(validation::validate_world(&next).valid);
    next
}

fn fixture() -> (tempfile::TempDir, World) {
    let directory = tempfile::tempdir().unwrap();
    let world = source::init_world(&directory.path().join("world"), "Reviewed retrieval").unwrap();
    let owner = world.world_record().unwrap();
    let mut fields = owner.frontmatter.clone();
    fields["capabilities"] =
        serde_yaml::from_str("[lore-core-v1, descriptions-v1, assertions-v1]").unwrap();
    fs::write(
        world.root.join(&owner.source_path),
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&fields).unwrap(),
            owner.body
        ),
    )
    .unwrap();
    let world = source::load_world(&world.root, None).unwrap();
    let world_id = world.world_record().unwrap().id.clone();
    let world = save(&world, Some(&world_id), &serde_yaml::to_string(&json!({
        "kind":"world", "predicates":[{"id":"policy-text", "subject_kinds":["character"], "value_type":{"type":"string"}}]
    })).unwrap());
    let world = save(&world, None, &serde_yaml::to_string(&json!({"kind":"character", "id":ANA, "title":"Ana", "body":"PRIVATE_RETRIEVAL_CHARACTER_BODY"})).unwrap());
    let world = save(&world, None, &serde_yaml::to_string(&json!({
        "kind":"assertion", "id":CLAIM, "title":"Canonical ferry rule", "domain":"retrieval.ferry",
        "subject":{"kind":"record", "id":ANA}, "predicate":"policy-text", "value":"PRIVATE_RETRIEVAL_CANON requires a captain.",
        "classification":"author-fact", "valid":{"kind":"timeless"}, "audience":["author"], "perspectives":["author"], "evidence":[]
    })).unwrap());
    (directory, world)
}

fn query(world: &World, tick: i64, query: &str, mode: Mode) -> Json {
    character::search_with_mode(
        world,
        ANA,
        query,
        &wedl::model::StoryTime::new("main", tick, 0).unwrap(),
        100,
        mode,
    )
    .unwrap()
}
fn has(result: &Json, record: &str, kind: &str) -> bool {
    result["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["recordId"] == record && item["kind"] == kind)
}
fn complete(world: &World) -> World {
    let id = world.world_record().unwrap().id.clone();
    INPUTS
        .iter()
        .enumerate()
        .fold(world.clone(), |world, (index, input)| {
            save(&world, (index == 0).then_some(id.as_str()), input)
        })
}

#[test]
fn reviewed_retrieval_examples_preserve_bytes_and_independent_learning() {
    let (_directory, world) = fixture();
    let at = wedl::model::StoryTime::new("main", 12, 0).unwrap();
    let old_context = character::context(&world, ANA, &at, 100_000).unwrap();
    for mode in [Mode::Semantic, Mode::Hybrid] {
        assert!(character::search_with_mode(&world, ANA, "vessel", &at, 100, mode).is_err());
    }
    let next = complete(&world);
    assert_eq!(next.records.len(), world.records.len() + 3);
    assert_eq!(
        old_context,
        character::context(&next, ANA, &at, 100_000).unwrap()
    );
    assert_eq!(
        query(&next, 12, "vessel", Mode::Semantic)["items"],
        json!([])
    );
    assert_eq!(query(&next, 20, "vessel", Mode::Lexical)["total"], 0);
    for mode in [Mode::Semantic, Mode::Hybrid] {
        let recalled = query(&next, 20, "vessel", mode);
        assert!(has(&recalled, REPORT, "assertion-belief"), "{recalled}");
        assert!(recalled["items"].as_array().unwrap().iter().any(|item| {
            item["recordId"] == REPORT
                && item["semanticScore"]
                    .as_f64()
                    .is_some_and(|score| score > 0.0)
        }));
        assert!(has(
            &query(&next, 22, "vessel", mode),
            REPORT,
            "assertion-belief"
        ));
        assert!(!has(
            &query(&next, 26, "vessel", mode),
            REPORT,
            "assertion-belief"
        ));
        assert!(!has(
            &query(&next, 20, "vessel", mode),
            NOTICE,
            "description"
        ));
        assert!(has(
            &query(&next, 21, "vessel", mode),
            NOTICE,
            "description"
        ));
        assert!(!has(
            &query(&next, 24, "vessel", mode),
            NOTICE,
            "description"
        ));
        let text = serde_json::to_string(&query(&next, 22, "vessel", mode)).unwrap();
        assert!(text.contains("Residents told me the ferry needs no captain."));
        for withheld in [
            "PRIVATE_RETRIEVAL",
            "FUTURE_RETRIEVAL",
            "UNTIMED_RETRIEVAL",
            "AUTHOR_RETRIEVAL",
            "A ferry is a vessel.",
            "sourcePath",
            "fingerprint",
            "blobOid",
        ] {
            assert!(!text.contains(withheld), "{withheld}: {text}");
        }
    }
    let author = compiler::search_with_mode(&next, "vessel", 100, None, Mode::Semantic).unwrap();
    assert!(
        author["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == NOTICE)
    );
    assert!(!next.root.join(".wedl-rust/index.sqlite3").exists());
    assert_eq!(
        source::load_world(&next.root, None).unwrap().fingerprint,
        next.fingerprint
    );
}

#[test]
fn reviewed_private_and_canonical_edits_cannot_retrain_character_search() {
    let (_directory, world) = fixture();
    let world = complete(&world);
    let before: Vec<_> = [Mode::Lexical, Mode::Semantic, Mode::Hybrid]
        .iter()
        .map(|mode| query(&world, 22, "vessel", *mode))
        .collect();
    let id = world.world_record().unwrap().id.clone();
    let mut request: Yaml = serde_yaml::from_str(INPUTS[0]).unwrap();
    request["descriptions"][6]["markdown"] =
        Yaml::String("vessel ferry changed PRIVATE_RETRIEVAL_TRAINING".into());
    request["descriptions"][7]["markdown"] =
        Yaml::String("vessel ferry changed FUTURE_RETRIEVAL_TRAINING".into());
    request["body"] =
        Yaml::String("Different hidden source: ferry boat vessel AUTHOR_BODY_CHANGED".into());
    request["title"] = Yaml::String("AUTHOR_TITLE_CHANGED".into());
    let world = save(&world, Some(&id), &serde_yaml::to_string(&request).unwrap());
    let world = save(
        &world,
        Some(CLAIM),
        "kind: assertion\nvalue: A canonical revision mentions a vessel but teaches no report.\n",
    );
    for (index, mode) in [Mode::Lexical, Mode::Semantic, Mode::Hybrid]
        .into_iter()
        .enumerate()
    {
        assert_eq!(before[index], query(&world, 22, "vessel", mode));
    }
    let mut invalid: Yaml = serde_yaml::from_str(INPUTS[0]).unwrap();
    invalid["descriptions"][0]["valid"] = serde_yaml::from_str(
        "{kind: interval, timeline: main, start: {tick: 30}, end: {tick: 20}}",
    )
    .unwrap();
    let input = serde_yaml::to_string(&invalid).unwrap();
    match authoring::preview_record_update(&world, &id, &input, &world.fingerprint) {
        Ok(preview) => {
            assert_eq!(preview["canApply"], false);
            assert!(
                authoring::apply_record_update(
                    &world,
                    &id,
                    &input,
                    &world.fingerprint,
                    preview["afterSha256"].as_str().unwrap()
                )
                .is_err()
            );
        }
        Err(_) => {}
    }
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
}
