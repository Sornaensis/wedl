//! Narrative source uses the existing reviewed writer without changing canonical or learned neighbors.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::fs;
use wedl::{
    Error, assertion, authoring, character, chronology, description,
    model::{Record, StoryTime, World},
    record_edit, source, validation,
};

const ANA: &str = "char_00000000000000000000000001";
const KNOWLEDGE: &str = "know_00000000000000000000000001";
const PRACTICE: &str = "concept_00000000000000000000000009";
const FACT: &str = "assertion_00000000000000000000000001";
const ACCOUNT: &str = "assertion_00000000000000000000000002";
const UNRESOLVED: &str = "assertion_00000000000000000000000003";
const CORRECTION: &str = "assertion_00000000000000000000000004";
const ALTERNATIVE: &str = "assertion_00000000000000000000000005";
const REQUESTS: [&str; 10] = [
    include_str!("../examples/authoring/88-world-narrative-predicates.yaml"),
    include_str!("../examples/authoring/89-early-practice-descriptions.yaml"),
    include_str!("../examples/authoring/90-correct-early-description.yaml"),
    include_str!("../examples/authoring/91-early-practice-fact.yaml"),
    include_str!("../examples/authoring/92-attributed-practice-account.yaml"),
    include_str!("../examples/authoring/93-unresolved-practice-origin.yaml"),
    include_str!("../examples/authoring/94-correct-practice-fact.yaml"),
    include_str!("../examples/authoring/95-account-description-and-date.yaml"),
    include_str!("../examples/authoring/96-extend-narrative-predicate.yaml"),
    include_str!("../examples/authoring/97-alternative-practice-correction.yaml"),
];
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn yaml(value: &str) -> Yaml {
    serde_yaml::from_str(value).unwrap()
}
fn request(value: &Yaml) -> String {
    serde_yaml::to_string(value).unwrap()
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
fn save(world: &World, target: Option<&str>, input: &str) -> World {
    let preview = match target {
        Some(id) => authoring::preview_record_update(world, id, input, &world.fingerprint),
        None => authoring::preview_record_create(world, input, &world.fingerprint),
    }
    .unwrap();
    assert_eq!(preview["canApply"], true, "{preview}");
    unchanged(world);
    let hash = preview["afterSha256"].as_str().unwrap();
    match target {
        Some(id) => authoring::apply_record_update(world, id, input, &world.fingerprint, hash),
        None => authoring::apply_record_create(world, input, &world.fingerprint, hash),
    }
    .unwrap();
    let saved = source::load_world(&world.root, None).unwrap();
    assert_eq!(saved.fingerprint, preview["candidateFingerprint"]);
    assert_eq!(
        fs::read(world.root.join(preview["sourcePath"].as_str().unwrap())).unwrap(),
        preview["afterSource"].as_str().unwrap().as_bytes()
    );
    let edited = preview["entityId"].as_str().unwrap();
    for (id, record) in &world.records {
        if id != edited {
            assert_eq!(saved.records[id].raw_bytes, record.raw_bytes);
        }
    }
    let report = validation::validate_world(&saved);
    assert!(report.valid, "{:?}", report.diagnostics);
    saved
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
fn initialized(capabilities: &str) -> (tempfile::TempDir, World) {
    let dir = tempfile::tempdir().unwrap();
    let world = source::init_world(&dir.path().join("world"), "Reviewed narrative source").unwrap();
    let owner = world.world_record().unwrap();
    let mut fields = owner.frontmatter.clone();
    fields["capabilities"] = yaml(capabilities);
    fields["author_metadata"] = yaml(
        "{large: 9007199254740993, decimal: 1.0, chronology: literal-note, entity: nonexistent-record}",
    );
    fs::write(
        world.root.join(&owner.source_path),
        format!(
            "---\n{}---\n{}\r\nOpaque retained world tail  \r\n",
            request(&fields),
            owner.body
        ),
    )
    .unwrap();
    let loaded = source::load_world(&world.root, None).unwrap();
    (dir, loaded)
}
fn fixture() -> (tempfile::TempDir, World) {
    let (dir, mut world) = initialized("[lore-core-v1, descriptions-v1, assertions-v1]");
    for input in [
        include_str!("../examples/authoring/01-location.yaml"),
        include_str!("../examples/authoring/02-character.yaml"),
        include_str!("../examples/authoring/04-event.yaml"),
        include_str!("../examples/authoring/05-knowledge.yaml"),
    ] {
        world = save(&world, None, input);
    }
    let world_id = world.world_record().unwrap().id.clone();
    for input in [
        include_str!("../examples/authoring/19-world-chronology.yaml"),
        include_str!("../examples/authoring/72-world-state-profile-bindings.yaml"),
        include_str!("../examples/authoring/80-world-magic-profiles.yaml"),
    ] {
        world = save(&world, Some(&world_id), input);
    }
    for input in [
        include_str!("../examples/authoring/81-lantern-magic-tradition.yaml"),
        include_str!("../examples/authoring/82-tidal-magic-realm.yaml"),
        include_str!("../examples/authoring/83-lantern-magic-artifact.yaml"),
        include_str!("../examples/authoring/84-early-lantern-calling.yaml"),
        include_str!("../examples/authoring/85-middle-lantern-calling.yaml"),
        include_str!("../examples/authoring/86-late-lantern-calling.yaml"),
    ] {
        world = save(&world, None, input);
    }
    (dir, world)
}
fn through(world: &World, count: usize) -> World {
    let mut world = world.clone();
    let world_id = world.world_record().unwrap().id.clone();
    for (index, input) in REQUESTS.iter().take(count).enumerate() {
        let target = match index {
            0 | 8 => Some(world_id.as_str()),
            1 | 2 => Some(PRACTICE),
            7 => Some(ACCOUNT),
            _ => None,
        };
        world = save(&world, target, input);
    }
    world
}
fn assertions(world: &World, target: &str, time: Option<&StoryTime>) -> Json {
    assertion::as_of(world, target, time, "author", "author", 100).unwrap()
}
fn assertion_ids(result: &Json) -> Vec<&str> {
    result["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["assertionId"].as_str().unwrap())
        .collect()
}

#[test]
fn all_ten_narrative_requests_publish_exact_reviewed_bytes_without_changing_neighbors_or_learning()
{
    let (_dir, before) = fixture();
    let context = character::context(&before, ANA, &at(5, 0), 100_000).unwrap();
    let search = character::search(&before, ANA, "bell", &at(5, 0), 100).unwrap();
    let budget = character::context(&before, ANA, &at(5, 0), 1).map_err(|e| e.to_string());
    let saved = through(&before, 10);
    assert_eq!(saved.records.len(), before.records.len() + 5);
    retained(
        before.world_record().unwrap(),
        saved.world_record().unwrap(),
        &["predicates"],
    );
    retained(
        &before.records[PRACTICE],
        &saved.records[PRACTICE],
        &["descriptions"],
    );
    assert_eq!(
        saved.current_time().unwrap(),
        before.current_time().unwrap()
    );
    let current =
        description::list(&saved, PRACTICE, Some(&at(5, 0)), "author", "author", 10).unwrap();
    assert_eq!(current["count"], 2);
    assert_eq!(
        current["items"][0]["sectionId"],
        "description_00000000000000000000000002"
    );
    assert_eq!(
        current["items"][1]["citation"]["section"],
        "descriptions[2]"
    );
    assert_eq!(
        description::list(&saved, PRACTICE, None, "author", "author", 10).unwrap()["count"],
        3
    );
    let propositions = assertions(&saved, PRACTICE, Some(&at(5, 0)));
    assert_eq!(
        assertion_ids(&propositions),
        [ACCOUNT, UNRESOLVED, CORRECTION, ALTERNATIVE]
    );
    assert!(propositions["items"][1]["value"].is_null());
    assert_eq!(
        propositions["items"][2]["value"]["count"],
        "9007199254740995"
    );
    assert_eq!(
        propositions["items"][3]["value"]["count"],
        "9007199254740993"
    );
    assert_eq!(assertions(&saved, PRACTICE, None)["count"], 5);
    assert_eq!(
        character::context(&saved, ANA, &at(5, 0), 100_000).unwrap(),
        context
    );
    assert_eq!(
        character::search(&saved, ANA, "bell", &at(5, 0), 100).unwrap(),
        search
    );
    assert_eq!(
        character::context(&saved, ANA, &at(5, 0), 1).map_err(|e| e.to_string()),
        budget
    );
    let declaration = assertion::predicates(&saved, 10).unwrap();
    let rule = declaration["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "practice-rule")
        .unwrap();
    assert_eq!(
        rule["value_type"]["optional"]["qualification"]["type"],
        "string"
    );
    assert!(
        assertions(&saved, FACT, None)["items"][0]["value"]
            .get("qualification")
            .is_none()
    );
    assert_eq!(
        record_edit::update(&saved, &saved.world_record().unwrap().id, REQUESTS[8])
            .unwrap()
            .raw_bytes,
        saved.world_record().unwrap().raw_bytes
    );
}

#[test]
fn description_only_creation_corrections_clear_and_noop_preserve_body_and_omitted_fields() {
    let (_dir, base) = initialized("[descriptions-v1]");
    let owner = "loc_00000000000000000000000009";
    let mut create = yaml(REQUESTS[1]);
    create["kind"] = yaml("location");
    create["id"] = yaml(owner);
    create["title"] = yaml("Recorded harbor");
    create["body"] = Yaml::String("\r\n# Undated 2040 heading\r\nUnchanged body  \rTail\n".into());
    let world = save(&base, None, &request(&create));
    let original = world.records[owner].clone();
    let mut correction = yaml(REQUESTS[2]);
    correction["kind"] = yaml("location");
    let world = save(&world, Some(owner), &request(&correction));
    retained(&original, &world.records[owner], &["descriptions"]);
    assert_eq!(
        description::list(&world, owner, Some(&at(0, 0)), "author", "author", 10).unwrap()["count"],
        2
    );
    assert_eq!(
        description::list(&world, owner, None, "author", "author", 10).unwrap()["count"],
        3
    );
    let world_id = world.world_record().unwrap().id.clone();
    let mut world_sections = correction.clone();
    world_sections["kind"] = yaml("world");
    let world = save(&world, Some(&world_id), &request(&world_sections));
    assert_eq!(
        description::list(&world, &world_id, Some(&at(5, 0)), "author", "author", 10).unwrap()["count"],
        2
    );
    let renamed = save(
        &world,
        Some(owner),
        "kind: location\ntitle: Recorded harbor renamed\n",
    );
    retained(&world.records[owner], &renamed.records[owner], &["title"]);
    let cleared = save(&renamed, Some(owner), "kind: location\ndescriptions: []\n");
    assert_eq!(cleared.records[owner].body, original.body);
    assert_eq!(
        cleared.records[owner].get("descriptions"),
        Some(&yaml("[]"))
    );
    assert_eq!(
        description::list(&cleared, owner, None, "author", "author", 10).unwrap()["count"],
        0
    );
    assert_eq!(
        record_edit::update(&cleared, owner, "kind: location\ndescriptions: []\n")
            .unwrap()
            .raw_bytes,
        cleared.records[owner].raw_bytes
    );
    assert!(cleared.world_record().unwrap().get("profiles").is_none());
    for input in [
        "kind: location\ndescriptions: null\n",
        "kind: location\ndescriptions: {}\n",
    ] {
        rejected(&cleared, Some(owner), input);
    }
    let mut bad = correction;
    bad["descriptions"][2]["valid"]["end"]["order"] = yaml("1");
    rejected(&cleared, Some(owner), &request(&bad));
    let (_other, missing_cap) = initialized("[]");
    rejected(&missing_cap, None, &request(&create));
}

#[test]
fn assertion_creation_and_reviewed_updates_preserve_nulls_enforce_classification_and_retirement() {
    let (_dir, base) = fixture();
    let world = through(&base, 7);
    assert_eq!(world.records[UNRESOLVED].get("value"), Some(&Yaml::Null));
    assert_eq!(
        record_edit::update(&world, UNRESOLVED, "kind: assertion\nvalue: null\n")
            .unwrap()
            .raw_bytes,
        world.records[UNRESOLVED].raw_bytes
    );
    rejected(
        &world,
        Some(FACT),
        "kind: assertion\nclassification: attributed-claim\n",
    );
    rejected(&world, Some(FACT), "kind: assertion\nvalue: null\n");
    rejected(&world, Some(ACCOUNT), "kind: assertion\nvalue: 42\n");
    rejected(
        &world,
        Some(UNRESOLVED),
        "kind: assertion\nvalue: unknown\n",
    );
    rejected(
        &world,
        Some(FACT),
        &format!("kind: assertion\nreplaces: {CORRECTION}\n"),
    );
    rejected(
        &world,
        Some(CORRECTION),
        "kind: assertion\nvalid: {kind: timeless}\n",
    );
    let mut missing = yaml(REQUESTS[5]);
    missing["id"] = yaml("assertion_00000000000000000000000009");
    missing.as_mapping_mut().unwrap().remove("value");
    rejected(&world, None, &request(&missing));
    let retired = save(
        &world,
        Some(CORRECTION),
        "kind: assertion\nstatus: retired\n",
    );
    retained(
        &world.records[CORRECTION],
        &retired.records[CORRECTION],
        &["status"],
    );
    assert_eq!(
        assertion_ids(&assertions(&retired, PRACTICE, Some(&at(5, 0)))),
        [FACT, ACCOUNT, UNRESOLVED]
    );
    let attributed = save(
        &retired,
        Some(FACT),
        &format!(
            "kind: assertion\nclassification: attributed-claim\nattributed_to: {{kind: record, id: {ANA}}}\n"
        ),
    );
    retained(
        &retired.records[FACT],
        &attributed.records[FACT],
        &["classification", "attributed_to"],
    );
    assert_eq!(
        assertions(&attributed, FACT, None)["items"][0]["classification"],
        "attributed-claim"
    );
    assert_eq!(
        assertions(&attributed, FACT, None)["items"][0]["attributedTo"]["id"],
        ANA
    );
    rejected(
        &attributed,
        Some(FACT),
        "kind: assertion\nclassification: author-fact\n",
    );
}

#[test]
fn predicate_replacements_validate_retired_values_and_preserve_all_dependent_source() {
    let (_dir, base) = fixture();
    let mut world = through(&base, 10);
    for id in [FACT, CORRECTION, ALTERNATIVE] {
        world = save(&world, Some(id), "kind: assertion\nstatus: retired\n");
    }
    assert_eq!(
        assertion_ids(&assertions(&world, PRACTICE, Some(&at(5, 0)))),
        [ACCOUNT, UNRESOLVED]
    );
    let world_id = world.world_record().unwrap().id.clone();
    let original = yaml(REQUESTS[8]);
    let mut wrong_type = original.clone();
    wrong_type["predicates"][0]["value_type"]["required"]["count"] = yaml("{type: string}");
    let mut new_required = original.clone();
    new_required["predicates"][0]["value_type"]["required"]["new_fact"] = yaml("{type: string}");
    let mut missing = original.clone();
    missing["predicates"].as_sequence_mut().unwrap().remove(0);
    let mut literal_to_reference = original;
    literal_to_reference["predicates"][0]["value_type"]["required"]["literal"] =
        yaml("{type: entity, kinds: [lore-concept]}");
    for invalid in [wrong_type, new_required, missing, literal_to_reference] {
        rejected(&world, Some(&world_id), &request(&invalid));
    }
    rejected(&world, Some(&world_id), "kind: world\npredicates: []\n");
    assert!(validation::validate_world(&world).valid);
    assert_eq!(
        world.records[FACT].get("value").unwrap()["count"],
        yaml("9007199254740993")
    );
    assert_eq!(
        world.records[ALTERNATIVE].get("value").unwrap()["qualification"],
        yaml("This is a separate authored correction; it does not override the other correction.")
    );
}

#[test]
fn historical_evidence_and_assertion_chronology_remain_explicit_source_without_learning() {
    let (_dir, base) = fixture();
    let before_date = through(&base, 7);
    let world = save(&before_date, Some(ACCOUNT), REQUESTS[7]);
    retained(
        &before_date.records[ACCOUNT],
        &world.records[ACCOUNT],
        &["descriptions", "chronology"],
    );
    let annotations = chronology::annotations(
        &world,
        &chronology::AnnotationFilter {
            record: Some(ACCOUNT.into()),
            calendar: None,
            era: None,
            limit: 100,
        },
    )
    .unwrap();
    assert_eq!(annotations["count"], 1);
    assert_eq!(
        description::list(&world, ACCOUNT, Some(&at(4, 0)), "author", "author", 10).unwrap()["count"],
        0
    );
    assert_eq!(
        description::list(&world, ACCOUNT, Some(&at(5, 0)), "author", "author", 10).unwrap()["count"],
        1
    );
    let body = world.records[ACCOUNT].body.clone();
    let original_evidence = world.records[ACCOUNT].get("evidence").unwrap();
    assert_eq!(
        original_evidence[0]["target"]["owner_id"],
        yaml("char_00000000000000000000000099")
    );
    assert!(
        assertion::references(&world, &world.records[ACCOUNT])
            .unwrap()
            .iter()
            .all(|(path, _)| !path.starts_with("evidence"))
    );
    assert_eq!(
        assertions(&world, ACCOUNT, None)["items"][0]["evidence"],
        json!([])
    );
    let evidence = json!([{"revision":"a".repeat(40),"path":"old-source/removed.md","blob_oid":"b".repeat(64),"target":{"kind":"transition","owner_id":"removed-owner","id":"removed-transition"}}]);
    let patch = serde_yaml::to_string(&json!({"kind":"assertion","evidence":evidence})).unwrap();
    let amended = save(&world, Some(ACCOUNT), &patch);
    retained(
        &world.records[ACCOUNT],
        &amended.records[ACCOUNT],
        &["evidence"],
    );
    let read = assertions(&amended, ACCOUNT, None);
    assert_eq!(read["items"][0]["evidence"], json!([]));
    assert!(!read.to_string().contains("removed-owner"));
    let mut duplicate = yaml(&patch);
    let repeated = duplicate["evidence"][0].clone();
    duplicate["evidence"]
        .as_sequence_mut()
        .unwrap()
        .push(repeated);
    rejected(&amended, Some(ACCOUNT), &request(&duplicate));
    for (field, value) in [
        ("path", yaml("../private.md")),
        ("revision", yaml("abc")),
        ("target", yaml("{kind: section, id: missing-owner}")),
    ] {
        let mut invalid = yaml(&patch);
        invalid["evidence"][0][field] = value;
        rejected(&amended, Some(ACCOUNT), &request(&invalid));
    }
    let cleared = save(
        &amended,
        Some(ACCOUNT),
        "kind: assertion\nchronology: []\ndescriptions: []\n",
    );
    assert_eq!(cleared.records[ACCOUNT].body, body);
    assert_eq!(
        cleared.records[ACCOUNT].get("evidence"),
        amended.records[ACCOUNT].get("evidence")
    );
    let mut invalid_date = yaml(REQUESTS[7]);
    invalid_date["chronology"][0]["value"]["civil"]["calendar_id"] =
        yaml("calendar_00000000000000000000000099");
    rejected(&cleared, Some(ACCOUNT), &request(&invalid_date));
    let learned = save(
        &cleared,
        Some(KNOWLEDGE),
        &format!(
            "kind: knowledge\nclaim: {{key: learned, statement: An explicitly linked claim, assertion_id: {FACT}}}\n"
        ),
    );
    assert_eq!(
        wedl::learned_assertions::list(&learned, ANA, FACT, &at(5, 0), 100).unwrap()["count"],
        1,
    );
    rejected(
        &learned,
        Some(KNOWLEDGE),
        &format!(
            "kind: knowledge\nclaim: {{key: mixed, statement: Invalid mixed claim, assertion_id: {FACT}, truth: 'true'}}\n"
        ),
    );
}
