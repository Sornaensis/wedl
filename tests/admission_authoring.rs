//! Reviewed source edits drive explicit narrative admission without granting canon.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::fs;
use wedl::{
    Error, assertion, authoring, character, generational, learned_assertions,
    model::{Record, StoryTime, World},
    narrative, runtime, source, validation,
};

const ANA: &str = "char_00000000000000000000000001";
const HARBOR: &str = "loc_00000000000000000000000001";
const NOTICE: &str = "concept_00000000000000000000000037";
const MECHANISM: &str = "concept_00000000000000000000000038";
const SECRET: &str = "concept_00000000000000000000000023";
const RULE: &str = "assertion_00000000000000000000000017";
const CORRECTION: &str = "assertion_00000000000000000000000018";
const REPORT: &str = "know_00000000000000000000000010";
const READ_NOTICE: &str = "know_00000000000000000000000012";
const BELL: &str = "know_00000000000000000000000001";
const REQUESTS: [&str; 16] = [
    include_str!("../examples/authoring/143-public-reference-context.yaml"),
    include_str!("../examples/authoring/144-public-reading-context.yaml"),
    include_str!("../examples/authoring/145-canonical-ferry-rule.yaml"),
    include_str!("../examples/authoring/146-learned-ferry-report.yaml"),
    include_str!("../examples/authoring/147-private-ferry-report.yaml"),
    include_str!("../examples/authoring/148-learn-reading-context.yaml"),
    include_str!("../examples/authoring/149-learn-reference-context.yaml"),
    include_str!("../examples/authoring/150-learn-harbor-record.yaml"),
    include_str!("../examples/authoring/151-harbor-learnable-placement.yaml"),
    include_str!("../examples/authoring/152-learn-harbor-map.yaml"),
    include_str!("../examples/authoring/153-correct-public-reading-section.yaml"),
    include_str!("../examples/authoring/154-correct-canonical-ferry-rule.yaml"),
    include_str!("../examples/authoring/155-learn-existence-pointer.yaml"),
    include_str!("../examples/authoring/156-forget-reading-context.yaml"),
    include_str!("../examples/authoring/157-unlearned-rejection.yaml"),
    include_str!("../examples/authoring/158-dated-self-description.yaml"),
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
    let world =
        source::init_world(&dir.path().join("world"), "Reviewed narrative admission").unwrap();
    let world_id = world.world_record().unwrap().id.clone();
    let mut world = direct_source(&world, &world_id, |fields| {
        fields["capabilities"] = serde_yaml::from_str(
            "[generational-core-v1, spatial-core-v1, geometry-v1, lore-core-v1, descriptions-v1, assertions-v1, existence-v1, collective-parties-v1, spatial-history-v1]"
        ).unwrap();
        fields["author_metadata"] =
            serde_yaml::from_str("{chronology: literal-note, optional: null}").unwrap();
    });
    for input in [
        include_str!("../examples/authoring/01-location.yaml"),
        include_str!("../examples/authoring/02-character.yaml"),
        include_str!("../examples/authoring/04-event.yaml"),
        include_str!("../examples/authoring/05-knowledge.yaml"),
        include_str!("../examples/authoring/15-map.yaml"),
    ] {
        world = save(&world, None, input);
    }
    world = save(
        &world,
        Some(HARBOR),
        include_str!("../examples/authoring/16-harbor-placement.yaml"),
    );
    world = save(
        &world,
        Some(&world_id),
        include_str!("../examples/authoring/126-world-governance-profile.yaml"),
    );
    world = save(
        &world,
        None,
        include_str!("../examples/authoring/127-embodied-episode.yaml"),
    );
    world = save(
        &world,
        None,
        &request(
            &json!({"kind":"lore-concept","id":SECRET,"title":"Hidden authored context",
        "domain":"private.reference","profile":"context-resource","values":{},"valid":{"kind":"timeless"},
        "audience":["author"],"perspectives":["author"]}),
        ),
    );
    (dir, world)
}
fn packet(world: &World, tick: i64) -> Json {
    character::context(world, ANA, &at(tick, 0), 100_000).unwrap()
}
fn search(world: &World, tick: i64) -> Json {
    character::search(world, ANA, "ferry", &at(tick, 0), 100).unwrap()
}
fn budget(world: &World, tick: i64) -> String {
    character::context(world, ANA, &at(tick, 0), 1)
        .unwrap_err()
        .to_string()
}
fn ids(value: &Json, field: &str) -> Vec<String> {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item[field].as_str().unwrap().to_owned())
        .collect()
}
fn retained(before: &Record, after: &Record, changed: &[&str]) {
    assert_eq!(before.body, after.body);
    assert_eq!(before.source_path, after.source_path);
    for (key, value) in before.frontmatter.as_mapping().unwrap() {
        if !changed.contains(&key.as_str().unwrap()) {
            assert_eq!(after.frontmatter.get(key), Some(value));
        }
    }
}

#[test]
fn all_sixteen_requests_publish_exact_bytes_and_drive_all_nine_admitted_reads() {
    let (_dir, mut world) = fixture();
    let baseline = world.records.len();
    let before = packet(&world, 12);
    let before_search = search(&world, 12);
    let before_budget = budget(&world, 12);
    let mut prior_packet = before.clone();
    let mut prior_search = before_search.clone();
    let mut prior_budget = before_budget.clone();
    let mut learned_before_correction = Json::Null;
    for (offset, input) in REQUESTS.iter().enumerate() {
        let number = 143 + offset;
        let target = match number {
            151 => Some(HARBOR),
            153 => Some(NOTICE),
            156 => Some(READ_NOTICE),
            158 => Some(ANA),
            _ => None,
        };
        world = save(&world, target, input);
        if [143, 144, 145].contains(&number) {
            assert_eq!(packet(&world, 12), before);
            assert_eq!(search(&world, 12), before_search);
            assert_eq!(budget(&world, 12), before_budget);
        }
        if [147, 149, 151, 152, 154, 156, 157].contains(&number) {
            assert_eq!(packet(&world, 12), prior_packet, "request{number}");
            assert_eq!(search(&world, 12), prior_search, "request{number}");
            assert_eq!(budget(&world, 12), prior_budget, "request{number}");
        }
        if number == 146 {
            assert_eq!(
                learned_assertions::list(&world, ANA, RULE, &at(10, 0), 100).unwrap()["count"],
                0
            );
            let held = learned_assertions::list(&world, ANA, RULE, &at(12, 0), 100).unwrap();
            assert_eq!(held["items"][0]["state"], "rejected");
            assert_eq!(
                held["items"][0]["statement"],
                "Residents told me the ferry runs all night."
            );
            learned_before_correction = held;
        }
        if number == 148 {
            assert_eq!(
                ids(
                    &narrative::descriptions(&world, ANA, NOTICE, &at(12, 0), 100).unwrap(),
                    "sectionId"
                ),
                [id("description", 1), id("description", 5)]
            );
            assert!(narrative::lore_concept(&world,ANA,NOTICE,&at(12,0)).unwrap()["values"]["dependencies"].as_array().unwrap().is_empty());
        }
        if number == 151 {
            assert_eq!(
                narrative::spatial_history(&world, ANA, HARBOR, &at(12, 0), 100).unwrap()["count"],
                0
            );
        }
        if number == 153 {
            let sections = narrative::descriptions(&world, ANA, NOTICE, &at(12, 0), 100).unwrap();
            assert_eq!(
                ids(&sections, "sectionId"),
                [id("description", 5), id("description", 2)]
            );
            assert!(
                narrative::section(&world, ANA, NOTICE, &id("description", 1), &at(12, 0)).is_err()
            );
            assert!(
                world.records[NOTICE]
                    .get("descriptions")
                    .unwrap()
                    .as_sequence()
                    .unwrap()
                    .iter()
                    .any(|v| v["id"].as_str() == Some(&id("description", 1)))
            );
        }
        if number == 154 {
            assert_eq!(
                learned_assertions::list(&world, ANA, RULE, &at(12, 0), 100).unwrap(),
                learned_before_correction
            );
            assert_eq!(
                ids(
                    &assertion::as_of(&world, NOTICE, Some(&at(12, 0)), "author", "author", 100)
                        .unwrap(),
                    "assertionId"
                ),
                [CORRECTION]
            );
        }
        prior_packet = packet(&world, 12);
        prior_search = search(&world, 12);
        prior_budget = budget(&world, 12);
    }
    assert_eq!(world.records.len(), baseline + 12);
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
    let learned = learned_assertions::list(&world, ANA, RULE, &at(12, 0), 100).unwrap();
    assert_eq!(learned["count"], 1);
    assert_eq!(learned["items"][0]["knowledgeId"], REPORT);
    let descriptions = narrative::descriptions(&world, ANA, NOTICE, &at(12, 0), 100).unwrap();
    assert_eq!(descriptions["count"], 2);
    let section =
        narrative::section(&world, ANA, NOTICE, &id("description", 2), &at(12, 0)).unwrap();
    assert_eq!(
        section["markdown"],
        "The ferry opens after the morning bell."
    );
    let catalog =
        narrative::lore_catalog(&world, ANA, Some("narrative.reading"), &at(15, 0), 100).unwrap();
    assert_eq!(ids(&catalog, "conceptId"), [NOTICE, MECHANISM]);
    let concept = narrative::lore_concept(&world, ANA, NOTICE, &at(15, 0)).unwrap();
    assert_eq!(
        concept["values"]["dependencies"],
        json!([{"kind":"record","id":MECHANISM}])
    );
    assert_eq!(
        concept["values"]["places"],
        json!([{"kind":"record","id":HARBOR}])
    );
    assert_eq!(
        concept["values"]["quantity"]["amount_text"],
        "9007199254740993"
    );
    let links = narrative::lore_links(&world, ANA, NOTICE, &at(15, 0), 100).unwrap();
    assert_eq!(ids(&links, "conceptId"), [MECHANISM]);
    let spatial = narrative::spatial_history(&world, ANA, HARBOR, &at(15, 0), 100).unwrap();
    assert_eq!(ids(&spatial, "memberId"), [id("spatialhistory", 2)]);
    assert_eq!(
        spatial["items"][0]["value"]["geometry"]["coordinates"],
        json!(["3", "4"])
    );
    assert_eq!(spatial["items"][0]["replaces"], id("spatialhistory", 1));
    assert_eq!(
        narrative::spatial_history(&world, ANA, HARBOR, &at(18, 0), 100).unwrap()["count"],
        0
    );
    let existence = narrative::existence(&world, ANA, ANA, &at(12, 0), 100).unwrap();
    assert_eq!(existence["items"], json!([]));
    assert_eq!(existence["reason"], "typed-episode-knowledge-unavailable");
    let governance = narrative::governance(&world, ANA, HARBOR, &at(12, 0), 100).unwrap();
    assert_eq!(governance["count"], 1);
    assert_eq!(governance["items"][0]["jurisdiction"], "unknown");
    assert_eq!(governance["items"][0]["basis"], Json::Null);
    assert_eq!(governance["items"][0]["parties"], json!([]));
    assert_eq!(
        learned_assertions::list(&world, ANA, CORRECTION, &at(12, 0), 100).unwrap()["count"],
        0
    );
    let missing = id("loc", 999);
    let unknown_governance = narrative::governance(&world, ANA, &missing, &at(12, 0), 100).unwrap();
    assert_eq!(unknown_governance["items"], json!([]));
    assert_eq!(unknown_governance["count"], 0);
    assert_eq!(
        narrative::governance(&world, ANA, HARBOR, &at(9, 0), 100).unwrap()["items"],
        json!([])
    );
    let mut unknown_existence =
        narrative::existence(&world, ANA, &id("char", 999), &at(12, 0), 100).unwrap();
    unknown_existence["targetId"] = json!(ANA);
    assert_eq!(unknown_existence, existence);
    assert_eq!(
        narrative::descriptions(&world, ANA, SECRET, &at(12, 0), 100)
            .unwrap_err()
            .to_string(),
        narrative::descriptions(&world, ANA, &missing, &at(12, 0), 100)
            .unwrap_err()
            .to_string()
    );
    assert_eq!(
        narrative::descriptions(&world, ANA, ANA, &at(19, 0), 100).unwrap()["count"],
        1
    );
    assert_eq!(
        narrative::descriptions(&world, ANA, ANA, &at(19, 1), 100).unwrap()["count"],
        0
    );
    assert_eq!(
        narrative::descriptions(&world, ANA, NOTICE, &at(21, 0), 100).unwrap()["items"][0]["markdown"],
        "The ferry uses its winter schedule."
    );
    assert!(narrative::descriptions(&world, ANA, NOTICE, &at(22, 0), 100).is_err());
    assert_eq!(
        narrative::lore_catalog(&world, ANA, None, &at(22, 0), 100).unwrap()["count"],
        0
    );
    let text = serde_json::to_string(&packet(&world, 12)).unwrap();
    for hidden in [
        "UNTIMED_",
        "UNREAD_",
        "PRIVATE_NARRATIVE_",
        "CANONICAL_RULE_BODY",
        "AUTHOR_ACQUISITION_NOTE",
        "UNLEARNED_REJECTION_WORDS",
        "sourcePath",
        "fingerprint",
        "revision",
    ] {
        assert!(!text.contains(hidden), "{hidden}: {text}");
    }
    let author = runtime::knowledge(&world, ANA, &at(12, 0), false).unwrap();
    assert!(
        author["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["knowledgeId"] == id("know", 11))
    );
    assert!(author["items"].as_array().unwrap().iter().any(
        |v| v["note"] == "AUTHOR_ACQUISITION_NOTE remains outside the linked character result."
    ));
}

#[test]
fn invalid_linked_claims_scopes_and_capability_dependencies_never_publish() {
    let (_dir, mut world) = fixture();
    for input in &REQUESTS[..4] {
        world = save(&world, None, input);
    }
    let before = world.records[REPORT].clone();
    for claim in [
        json!({"key":"report","statement":"words","assertion_id":RULE,"subject":ANA}),
        json!({"key":"report","statement":"words","assertion_id":ANA}),
        json!({"key":"report","statement":"words","assertion_id":id("assertion",999)}),
        json!({"key":"report","statement":" ","assertion_id":RULE}),
        json!({"key":"report","statement":"words","assertion_id":RULE,"genealogy":{}}),
    ] {
        rejected(
            &world,
            Some(REPORT),
            &request(&json!({"kind":"knowledge","claim":claim})),
        );
    }
    for field in ["audience", "perspectives"] {
        for value in [
            Json::Null,
            json!(["public", "public"]),
            json!([""]),
            json!([3]),
            json!("public"),
        ] {
            let mut input = json!({"kind":"knowledge"});
            input[field] = value;
            rejected(&world, Some(REPORT), &request(&input));
        }
    }
    let body = world.records[REPORT].body.clone();
    world = save(&world, Some(REPORT), "kind: knowledge\naudience: []\n");
    retained(&before, &world.records[REPORT], &["audience"]);
    assert_eq!(world.records[REPORT].body, body);
    assert_eq!(
        learned_assertions::list(&world, ANA, RULE, &at(12, 0), 100).unwrap()["count"],
        0
    );
    world = save(
        &world,
        Some(REPORT),
        &request(
            &json!({"kind":"knowledge","audience":["public",ANA],"perspectives":["ordinary",ANA]}),
        ),
    );
    assert_eq!(
        learned_assertions::list(&world, ANA, RULE, &at(12, 0), 100).unwrap()["count"],
        1
    );
    let fingerprint = world.fingerprint.clone();
    world = save(&world, Some(REPORT), "kind: knowledge\n");
    assert_eq!(world.fingerprint, fingerprint);
    let world_id = world.world_record().unwrap().id.clone();
    let invalid = direct_source(&world, &world_id, |data| {
        data["capabilities"]
            .as_sequence_mut()
            .unwrap()
            .retain(|v| v.as_str() != Some("assertions-v1"));
    });
    rejected(
        &invalid,
        Some(REPORT),
        "kind: knowledge\ntitle: Invalid candidate cannot publish\n",
    );
    let labels = invalid.records[REPORT].get("audience").unwrap().clone();
    assert_eq!(
        labels,
        serde_yaml::to_value(json!(["public", ANA])).unwrap()
    );
}

#[test]
fn reviewed_ordinary_and_generational_labels_control_admission_and_historical_proof_dependencies() {
    let (_dir, world) = fixture();
    let world_id = world.world_record().unwrap().id.clone();
    let mut world = direct_source(&world, &world_id, |data| {
        let caps = data["capabilities"].as_sequence_mut().unwrap();
        let index = caps
            .iter()
            .position(|v| v.as_str() == Some("lore-core-v1"))
            .unwrap();
        caps.insert(index, Yaml::String("generational-knowledge-v1".into()));
    });
    let belief = id("know", 99);
    let claim = json!({"key":"learned-vital","statement":"Author summary stays separate from literal labels.",
        "genealogy":{"kind":"vital","payload":{"character_id":ANA,"state":"living"},
        "valid":{"from":{"timeline":"main","tick":0,"order":0}},"labels":{(ANA):"Learned Ana"},
        "evidence":[{"kind":"knowledge","entity_id":BELL,"transition_id":"first-observation"}]}});
    world = save(
        &world,
        None,
        &request(
            &json!({"kind":"knowledge","id":belief,"title":"Scoped typed learning",
        "knower":ANA,"claim":claim,"audience":["public"],"perspectives":["ordinary"],
        "transitions":[{"id":"learned-vital","time":{"timeline":"main","tick":2,"order":0},
        "state":"accepted","confidence":1.0,"acquisition":"told"}]}),
        ),
    );
    assert_eq!(
        generational::knowledge::assertions(
            &world,
            ANA,
            &at(3, 0),
            Some("vital"),
            None,
            false,
            100
        )
        .unwrap()["count"],
        1
    );
    // A previously used proof cannot be made private while retaining the dependent learning.
    rejected(
        &world,
        Some(BELL),
        "kind: knowledge\naudience: [author]\nperspectives: [author]\n",
    );
    let mut replacement = claim.clone();
    replacement["genealogy"]["evidence"] = json!([]);
    world = save(
        &world,
        Some(&belief),
        &request(&json!({"kind":"knowledge","claim":replacement})),
    );
    let bell_before = world.records[BELL].clone();
    world = save(
        &world,
        Some(BELL),
        "kind: knowledge\naudience: [author]\nperspectives: [author]\n",
    );
    retained(
        &bell_before,
        &world.records[BELL],
        &["audience", "perspectives"],
    );
    assert!(
        !serde_json::to_string(&packet(&world, 3))
            .unwrap()
            .contains("The harbor bell has rung.")
    );
    assert!(
        runtime::knowledge(&world, ANA, &at(3, 0), false).unwrap()["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["knowledgeId"] == BELL)
    );
    world = save(&world, Some(&belief), "kind: knowledge\naudience: []\n");
    assert_eq!(
        generational::knowledge::assertions(
            &world,
            ANA,
            &at(3, 0),
            Some("vital"),
            None,
            false,
            100
        )
        .unwrap()["count"],
        0
    );
    assert!(
        !serde_json::to_string(&packet(&world, 3))
            .unwrap()
            .contains("Learned Ana")
    );
    world = save(
        &world,
        Some(&belief),
        &request(
            &json!({"kind":"knowledge","audience":["public",ANA],"perspectives":["ordinary",ANA]}),
        ),
    );
    assert_eq!(
        generational::knowledge::assertions(
            &world,
            ANA,
            &at(3, 0),
            Some("vital"),
            None,
            false,
            100
        )
        .unwrap()["count"],
        1
    );
}
