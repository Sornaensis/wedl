//! Family readers consume ordinary reviewed source without teaching characters.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::fs;
use wedl::{
    authoring, character, environment,
    model::{StoryTime, World},
    social, source, validation,
};
const ANA: &str = "char_00000000000000000000000001";
const REQUESTS: [&str; 28] = [
    include_str!("../examples/authoring/98-world-family-context-profiles.yaml"),
    include_str!("../examples/authoring/99-harbor-script.yaml"),
    include_str!("../examples/authoring/100-river-language.yaml"),
    include_str!("../examples/authoring/101-hill-language.yaml"),
    include_str!("../examples/authoring/102-ceremonial-register.yaml"),
    include_str!("../examples/authoring/103-river-sense.yaml"),
    include_str!("../examples/authoring/104-hill-sense.yaml"),
    include_str!("../examples/authoring/105-older-river-spelling.yaml"),
    include_str!("../examples/authoring/106-current-river-spelling.yaml"),
    include_str!("../examples/authoring/107-later-river-spelling.yaml"),
    include_str!("../examples/authoring/108-overlapping-river-spelling.yaml"),
    include_str!("../examples/authoring/109-hill-homonym.yaml"),
    include_str!("../examples/authoring/110-harbor-culture.yaml"),
    include_str!("../examples/authoring/111-river-religion.yaml"),
    include_str!("../examples/authoring/112-speaker-religion-membership.yaml"),
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
];
fn id(n: u32) -> String {
    format!("concept_{n:026}")
}
fn aid(n: u32) -> String {
    format!("assertion_{n:026}")
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn save(world: &World, target: Option<&str>, input: &str) -> World {
    let preview = match target {
        Some(id) => authoring::preview_record_update(world, id, input, &world.fingerprint),
        None => authoring::preview_record_create(world, input, &world.fingerprint),
    }
    .unwrap();
    assert_eq!(preview["canApply"], true, "{preview}");
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
    let hash = preview["afterSha256"].as_str().unwrap();
    match target {
        Some(id) => authoring::apply_record_update(world, id, input, &world.fingerprint, hash),
        None => authoring::apply_record_create(world, input, &world.fingerprint, hash),
    }
    .unwrap();
    let loaded = source::load_world(&world.root, None).unwrap();
    assert_eq!(loaded.fingerprint, preview["candidateFingerprint"]);
    assert_eq!(
        fs::read(world.root.join(preview["sourcePath"].as_str().unwrap())).unwrap(),
        preview["afterSource"].as_str().unwrap().as_bytes()
    );
    let edited = preview["entityId"].as_str().unwrap();
    for (id, record) in &world.records {
        if id != edited {
            assert_eq!(record.raw_bytes, loaded.records[id].raw_bytes);
        }
    }
    let report = validation::validate_world(&loaded);
    assert!(report.valid, "{:?}", report.diagnostics);
    loaded
}
fn fixture() -> (tempfile::TempDir, World) {
    let dir = tempfile::tempdir().unwrap();
    let world = source::init_world(&dir.path().join("world"), "Reviewed family contexts").unwrap();
    let owner = world.world_record().unwrap();
    let mut fields = owner.frontmatter.clone();
    fields["capabilities"] =
        serde_yaml::from_str("[lore-core-v1, descriptions-v1, assertions-v1]").unwrap();
    fields["author_note"] = Yaml::String("Preserve this unrelated metadata".into());
    fs::write(
        world.root.join(&owner.source_path),
        format!(
            "---\n{}---\n{}\r\nUntimed retained body  \r\n",
            serde_yaml::to_string(&fields).unwrap(),
            owner.body
        ),
    )
    .unwrap();
    let mut world = source::load_world(&world.root, None).unwrap();
    for input in [
        include_str!("../examples/authoring/01-location.yaml"),
        include_str!("../examples/authoring/02-character.yaml"),
        include_str!("../examples/authoring/04-event.yaml"),
        include_str!("../examples/authoring/05-knowledge.yaml"),
    ] {
        world = save(&world, None, input);
    }
    (dir, world)
}
fn selection() -> social::Selection {
    social::Selection {
        language: id(13),
        sense: Some(id(16)),
        form: None,
        contexts: vec![id(23), id(24), id(25)],
    }
}
fn record_ids(value: &Json) -> Vec<&str> {
    value["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["item"]["conceptId"].as_str().unwrap())
        .collect()
}
#[test]
fn all_twenty_eight_requests_publish_exact_bytes_and_preserve_independent_accounts_and_learning() {
    let (_dir, before) = fixture();
    let before_context = character::context(&before, ANA, &at(5, 0), 100_000).unwrap();
    let before_search = character::search(&before, ANA, "bell", &at(5, 0), 100).unwrap();
    let before_budget = character::context(&before, ANA, &at(5, 0), 1).map_err(|e| e.to_string());
    let mut world = before.clone();
    let world_id = world.world_record().unwrap().id.clone();
    for (i, input) in REQUESTS.iter().enumerate() {
        world = save(&world, if i == 0 { Some(&world_id) } else { None }, input);
        if i == 17 {
            let result =
                social::context(&world, &selection(), &at(3, 0), "author", "author").unwrap();
            let religion = result["canon"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["ownerId"] == id(24))
                .unwrap();
            let accounts = religion["result"]["items"].as_array().unwrap();
            assert_eq!(
                accounts
                    .iter()
                    .map(|v| v["assertionId"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                [aid(7), aid(8)]
            );
            assert!(world.records.contains_key(&aid(6)));
        }
    }
    assert_eq!(world.records.len(), before.records.len() + 27);
    let old = before.world_record().unwrap();
    let new = world.world_record().unwrap();
    assert_eq!(old.body, new.body);
    for (key, value) in old.frontmatter.as_mapping().unwrap() {
        if !["profiles", "predicates"].contains(&key.as_str().unwrap()) {
            assert_eq!(new.frontmatter.get(key), Some(value));
        }
    }
    assert_eq!(
        character::context(&world, ANA, &at(5, 0), 100_000).unwrap(),
        before_context
    );
    assert_eq!(
        character::search(&world, ANA, "bell", &at(5, 0), 100).unwrap(),
        before_search
    );
    assert_eq!(
        character::context(&world, ANA, &at(5, 0), 1).map_err(|e| e.to_string()),
        before_budget
    );
    let language = social::context(&world, &selection(), &at(3, 0), "author", "author").unwrap();
    assert_eq!(language["count"], 9);
    assert_eq!(
        language["sections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["result"]["count"].as_u64().unwrap())
            .sum::<u64>(),
        2
    );
    let trade = environment::context(&world, &[id(32)], &at(3, 0), "author", "author").unwrap();
    assert_eq!(trade["count"], 5);
    assert!(!record_ids(&trade).contains(&id(29).as_str()));
    assert_eq!(trade["canon"][0]["result"]["count"], 3);
    assert_eq!(trade["sections"][0]["result"]["count"], 0);
    assert_eq!(trade["competingAccounts"].as_array().unwrap().len(), 1);
    let resource = trade["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["item"]["conceptId"] == id(27))
        .unwrap();
    assert_eq!(
        resource["item"]["values"]["quantity"]["amount_text"],
        "9007199254740993"
    );
    assert_eq!(
        resource["item"]["valueTypes"]["fields"]["quantity"]["fields"]["amount_text"]["type"],
        "string"
    );
}
#[test]
fn reviewed_context_source_changes_recompute_reads_without_rewriting_neighbors_or_inventing_values()
{
    let (_dir, mut world) = fixture();
    let world_id = world.world_record().unwrap().id.clone();
    for (i, input) in REQUESTS.iter().enumerate() {
        world = save(&world, if i == 0 { Some(&world_id) } else { None }, input);
    }
    let old = environment::context(&world, &[id(32)], &at(3, 0), "author", "author").unwrap();
    let character_before = character::context(&world, ANA, &at(5, 0), 100_000).unwrap();
    let mut definitions: Yaml = serde_yaml::from_str(REQUESTS[0]).unwrap();
    let profile = definitions["profiles"]
        .as_sequence_mut()
        .unwrap()
        .iter_mut()
        .find(|v| v["id"].as_str() == Some("context-trade-practice"))
        .unwrap();
    profile["fields"]["optional"]["account_note"] = serde_yaml::from_str("{type: string}").unwrap();
    let extended = save(
        &world,
        Some(&world_id),
        &serde_yaml::to_string(&definitions).unwrap(),
    );
    assert_eq!(
        extended.records[&id(32)].raw_bytes,
        world.records[&id(32)].raw_bytes
    );
    let projection =
        environment::context(&extended, &[id(32)], &at(3, 0), "author", "author").unwrap();
    let trade = projection["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["item"]["conceptId"] == id(32))
        .unwrap();
    assert!(trade["item"]["values"].get("account_note").is_none());
    assert_eq!(
        projection["links"].as_array().unwrap().len(),
        old["links"].as_array().unwrap().len()
    );
    let changed = save(
        &extended,
        Some(&id(30)),
        "kind: lore-concept\nvalues:\n  technologies: []\n  service_text: This independent workshop account has changed.\n",
    );
    assert_eq!(
        changed.records[&id(32)].raw_bytes,
        world.records[&id(32)].raw_bytes
    );
    let current = environment::context(&changed, &[id(32)], &at(3, 0), "author", "author").unwrap();
    assert_eq!(current["count"], 5);
    let workshop = current["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["item"]["conceptId"] == id(30))
        .unwrap();
    assert_eq!(workshop["item"]["values"]["technologies"], json!([]));
    assert_eq!(
        character::context(&changed, ANA, &at(5, 0), 100_000).unwrap(),
        character_before
    );
    let incompatible = definitions["profiles"]
        .as_sequence_mut()
        .unwrap()
        .iter_mut()
        .find(|v| v["id"].as_str() == Some("context-resource"))
        .unwrap();
    incompatible["fields"]["optional"]["quantity"]["required"]["amount_text"] =
        serde_yaml::from_str("{type: number}").unwrap();
    let preview = authoring::preview_record_update(
        &changed,
        &world_id,
        &serde_yaml::to_string(&definitions).unwrap(),
        &changed.fingerprint,
    )
    .unwrap();
    assert_eq!(preview["canApply"], false);
    assert_eq!(
        source::load_world(&changed.root, None).unwrap().fingerprint,
        changed.fingerprint
    );
}
