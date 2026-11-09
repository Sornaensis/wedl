//! Learned names and context preserve complete-response privacy; author names
//! become discoverable only when a visible canonical generational fact admits them.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeMap, path::PathBuf};
use wedl::{
    generational::{self, discovery as view},
    model::{Record, StoryTime, World},
    source,
};

fn id(kind: &str, number: u32) -> String {
    let prefix = match kind {
        "character" => "char",
        "knowledge" => "know",
        "parentage" => "kinship",
        "vital-history" => "vital",
        _ => kind,
    };
    format!("{prefix}_{number:026}")
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn time(tick: i64, order: i32) -> Json {
    json!({"timeline":"main","tick":tick,"order":order})
}
fn tr(number: u32, kind: &str, tick: i64, order: i32, payload: Json) -> Json {
    json!({"transition_id":id("transition",number),"transition_kind":kind,
        "applicability":{"applicability_kind":"instant","point":time(tick,order)},"payload":payload})
}
fn record(mut value: Json) -> Record {
    value["schema"] = json!(source::SOURCE_SCHEMA);
    source::parse_record(
        format!(
            "---\n{}---\nPrivate untimed prose.\n",
            serde_yaml::to_string(&value).unwrap()
        )
        .as_bytes(),
        &format!("story/{}.md", value["id"].as_str().unwrap()),
    )
    .unwrap()
}
fn fixture(extra: Vec<Json>) -> World {
    let mut values = vec![
        json!({"kind":"world","id":id("world",1),"title":"Discovery test",
        "capabilities":["generational-core-v1","generational-knowledge-v1"],"default_timeline":"main",
        "timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"state_keys":{}}),
    ];
    values.extend((1..=6).map(|number| json!({"kind":"character","id":id("character",number),
        "title":format!("Canonical person {number}"),"aliases":[format!("Canonical alias {number}")]})));
    values.extend(extra);
    let records: BTreeMap<_, _> = values
        .into_iter()
        .map(record)
        .map(|record| (record.id.clone(), record))
        .collect();
    source::world_from_records(
        PathBuf::new(),
        records.into_values().collect(),
        Some("source revision".into()),
    )
    .unwrap()
}
fn edit(base: &World, target: &str, change: impl FnOnce(&mut Yaml)) -> World {
    let original = &base.records[target];
    let mut value = original.frontmatter.clone();
    change(&mut value);
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&value).unwrap(),
        original.body
    );
    let replacement = source::parse_record(raw.as_bytes(), &original.source_path).unwrap();
    source::world_from_records(
        base.root.clone(),
        base.records
            .values()
            .map(|record| {
                if record.id == target {
                    replacement.clone()
                } else {
                    record.clone()
                }
            })
            .collect(),
        base.revision.clone(),
    )
    .unwrap()
}
fn knowledge(number: u32, knower: u32, parent: u32, learned: i64, label: &str) -> Json {
    json!({"kind":"knowledge","id":id("knowledge",number),"title":format!("Private knowledge header {number}"),
        "knower":id("character",knower),"claim":{"key":format!("belief{number}"),"statement":"Private statement",
        "genealogy":{"kind":"parentage","payload":{"child_id":id("character",1),"parent_id":id("character",parent),"basis":"adoptive"},
            "valid":{"from":time(-10,0)},"labels":{id("character",parent):label}}},
        "transitions":[{"id":format!("learning-{number}"),"time":time(learned,0),"state":"accepted","confidence":1.0,"acquisition":"told"}]})
}
fn organization() -> Json {
    let mut rename = tr(
        102,
        "organization-rename",
        5,
        1,
        json!({"title":"New House","aliases":["Next House"]}),
    );
    rename["cause_event_id"] = json!(id("event", 1));
    json!({"kind":"organization","id":id("organization",1),"title":"Future header summary","organization_kind":"institution",
        "initialization":tr(101,"organization-initialize",-20,0,json!({"title":"Old House","aliases":["Original House"]})),
        "transitions":[rename]})
}
fn parentage(number: u32, child: u32, parent: u32, tick: i64, order: i32) -> Json {
    json!({"kind":"parentage","id":id("parentage",number),"title":"Private genealogy summary",
        "child_id":id("character",child),"parent_id":id("character",parent),
        "initialization":tr(200+number,"parentage-initialize",tick,order,json!({"basis":"adoptive"})),"transitions":[]})
}
fn vital(number: u32, character: u32, disclosure: &str, tick: i64, order: i32) -> Json {
    json!({"kind":"vital-history","id":id("vital-history",number),"title":"Private vital summary",
        "character_id":id("character",character),"disclosure":disclosure,
        "initialization":tr(300+number*2,"vital-initialize",-10,0,json!({})),
        "transitions":[tr(301+number*2,"vital-birth",tick,order,json!({}))]})
}
fn canonical_fixture() -> World {
    fixture(vec![
        organization(),
        parentage(1, 1, 2, 0, 1),
        vital(1, 3, "known", 7, 1),
        vital(2, 4, "withheld", 1, 0),
        json!({"kind":"event","id":id("event",1),"title":"Cause Name","time":time(4,0)}),
    ])
}
fn assert_valid(world: &World) {
    let diagnostics = generational::validate(world);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn learned_conflicting_names_remain_literal_and_never_fall_back_to_canon() {
    let mut second = knowledge(2, 1, 2, 2, "Guardian");
    second["transitions"][0]["state"] = json!("suspected");
    second["transitions"][0]["confidence"] = json!(0.6);
    let world = fixture(vec![knowledge(1, 1, 2, 2, "Mother"), second]);
    assert_valid(&world);
    let viewer = id("character", 1);
    let selected = vec![id("character", 2), id("character", 3)];
    assert_eq!(
        view::learned_labels(&world, &viewer, &selected, &at(2, -1), 10).unwrap()["count"],
        0
    );
    let labels = view::learned_labels(&world, &viewer, &selected, &at(2, 0), 10).unwrap();
    assert_eq!(labels["count"], 2);
    assert_eq!(labels["labels"][0]["title"], "Guardian");
    assert_eq!(labels["labels"][1]["title"], "Mother");
    assert_eq!(labels["labels"][0]["uncertain"], true);
    assert_eq!(labels["labels"][1]["uncertain"], false);
    assert_eq!(
        view::learned_discover(&world, &viewer, "character", "gu", &at(2, 0), 1).unwrap()["count"],
        1
    );
    assert_eq!(
        view::learned_search(&world, &viewer, "gu pare", &at(2, 0), 1).unwrap()["results"][0]["knowledgeId"],
        id("knowledge", 2)
    );
    for text in ["Canonical", "Private", "adoptive"] {
        assert_eq!(
            view::learned_search(&world, &viewer, text, &at(2, 0), 10).unwrap()["count"],
            0
        );
    }
    let serialized = serde_json::to_string(&labels).unwrap();
    for excluded in [
        "fingerprint",
        "revision",
        "sourcePath",
        "blobOid",
        "Private",
        "Canonical",
    ] {
        assert!(!serialized.contains(excluded), "leaked {excluded}");
    }
}

#[test]
fn held_names_survive_expiry_but_rejection_and_forgetting_remove_them() {
    for state in ["rejected", "forgotten"] {
        let mut belief = knowledge(1, 1, 2, 2, "Remembered parent");
        belief["claim"]["genealogy"]["valid"]["until"] = time(3, 1);
        belief["transitions"].as_array_mut().unwrap().push(json!({"id":"later","time":time(5,0),"state":state,"confidence":1.0,"acquisition":"remembered"}));
        let world = fixture(vec![belief]);
        assert_valid(&world);
        let viewer = id("character", 1);
        let ids = vec![id("character", 2)];
        let labels = view::learned_labels(&world, &viewer, &ids, &at(4, 0), 1).unwrap();
        assert_eq!(labels["count"], 1);
        assert_eq!(labels["labels"][0]["applicable"], false);
        let search = view::learned_search(&world, &viewer, "remembered", &at(4, 0), 1).unwrap();
        assert_eq!(search["count"], 1);
        assert_eq!(search["results"][0]["applicable"], false);
        let context =
            view::learned_context(&world, &viewer, &viewer, &at(4, 0), 4, 10, 100_000).unwrap();
        assert_eq!(context["items"].as_array().unwrap().len(), 1);
        assert_eq!(context["items"][0]["kind"], "learned-history");
        assert_eq!(context["labels"].as_array().unwrap().len(), 1);
        assert_eq!(
            view::learned_labels(&world, &viewer, &ids, &at(5, 0), 1).unwrap()["count"],
            0
        );
        assert_eq!(
            view::learned_search(&world, &viewer, "remembered", &at(5, 0), 1).unwrap()["count"],
            0
        );
        assert!(view::learned_context(&world,&viewer,&viewer,&at(5,0),4,10,100_000).unwrap()["items"].as_array().unwrap().is_empty());
    }
}

#[test]
fn learned_filters_precede_counts_and_complete_limits() {
    let world = fixture(vec![
        knowledge(1, 1, 2, 0, "Target"),
        knowledge(2, 1, 3, 0, "Unrelated"),
        knowledge(3, 2, 4, 0, "Target hidden"),
        knowledge(4, 1, 5, 9, "Target future"),
    ]);
    assert_valid(&world);
    let viewer = id("character", 1);
    assert_eq!(
        view::learned_labels(&world, &viewer, &[id("character", 2)], &at(1, 0), 1).unwrap()["count"],
        1
    );
    assert_eq!(
        view::learned_discover(&world, &viewer, "character", "Target", &at(1, 0), 1).unwrap()["count"],
        1
    );
    assert_eq!(
        view::learned_search(&world, &viewer, "Target", &at(1, 0), 1).unwrap()["count"],
        1
    );
    assert!(view::learned_discover(&world, &viewer, "character", "", &at(1, 0), 1).is_err());
    assert!(view::learned_search(&world, &viewer, "parentage", &at(1, 0), 1).is_err());
    let conflict = fixture(vec![
        knowledge(1, 1, 2, 0, "Target"),
        knowledge(2, 1, 2, 0, "Other name"),
    ]);
    assert!(view::learned_labels(&conflict, &viewer, &[id("character", 2)], &at(1, 0), 1).is_err());
}

fn learned_outputs(world: &World) -> Vec<Vec<u8>> {
    let viewer = id("character", 1);
    let horizon = at(3, 0);
    vec![
        view::learned_labels(
            world,
            &viewer,
            &[id("character", 2), id("character", 3)],
            &horizon,
            10,
        )
        .unwrap(),
        view::learned_discover(world, &viewer, "character", "", &horizon, 10).unwrap(),
        view::learned_search(world, &viewer, "Remembered", &horizon, 10).unwrap(),
        view::learned_context(world, &viewer, &viewer, &horizon, 4, 10, 100_000).unwrap(),
    ]
    .into_iter()
    .map(|value| serde_json::to_vec(&value).unwrap())
    .collect()
}

#[test]
fn complete_character_responses_ignore_private_future_and_canonical_metadata_changes() {
    let base = fixture(vec![
        knowledge(1, 1, 2, 2, "Remembered parent"),
        parentage(1, 1, 3, 0, 0),
    ]);
    let expected = learned_outputs(&base);
    let changed = fixture(vec![
        knowledge(1, 1, 2, 2, "Remembered parent"),
        parentage(1, 1, 4, 0, 0),
        knowledge(2, 2, 3, 0, "Secret remembered name"),
        knowledge(3, 1, 5, 9, "Future remembered name"),
    ]);
    let mut changed = edit(&changed, &id("character", 2), |value| {
        value["title"] = Yaml::String("Different canonical title".into());
        value["aliases"] = serde_yaml::to_value(vec!["Different canonical alias"]).unwrap();
    });
    changed.revision = Some("different revision".into());
    changed.fingerprint = "different fingerprint".into();
    for record in changed.records.values_mut() {
        record.body = "Completely different untimed prose".into();
        record.source_path = format!("elsewhere/{}.md", record.id);
        record.blob_oid = Some("different blob".into());
    }
    assert_valid(&base);
    assert_valid(&changed);
    assert_eq!(learned_outputs(&changed), expected);
    let viewer = id("character", 1);
    assert_eq!(
        view::learned_context(&base, &viewer, &viewer, &at(3, 0), 4, 10, 80)
            .unwrap_err()
            .to_string(),
        view::learned_context(&changed, &viewer, &viewer, &at(3, 0), 4, 10, 80)
            .unwrap_err()
            .to_string()
    );
}

#[test]
fn learned_selectors_use_only_exact_ids_or_unambiguous_explicit_names() {
    let world = fixture(vec![
        knowledge(1, 1, 2, 0, "Shared name"),
        knowledge(2, 1, 3, 0, "Shared name"),
    ]);
    let viewer = id("character", 1);
    assert!(
        view::resolve_learned(&world, &viewer, "Shared name", &at(1, 0), Some("character"))
            .unwrap_err()
            .to_string()
            .contains("ambiguous")
    );
    assert!(
        view::resolve_learned(
            &world,
            &viewer,
            "Canonical person 2",
            &at(1, 0),
            Some("character")
        )
        .is_err()
    );
    assert_eq!(
        view::resolve_learned(
            &world,
            &viewer,
            &id("character", 999),
            &at(1, 0),
            Some("character")
        )
        .unwrap(),
        id("character", 999)
    );
    assert!(
        view::learned_discover(&world, "Canonical person 1", "character", "", &at(1, 0), 10)
            .is_err()
    );
    assert_eq!(
        view::learned_discover(
            &world,
            &viewer,
            "character",
            "",
            &StoryTime::new("other", 10, 0).unwrap(),
            10
        )
        .unwrap()["count"],
        0
    );
}

#[test]
fn context_names_follow_relationship_endpoints_and_exclude_viewer_metadata_and_label_text() {
    let mut subject_belief = knowledge(1, 1, 3, 0, &id("character", 5));
    subject_belief["claim"]["genealogy"]["payload"]["child_id"] = json!(id("character", 2));
    let mut unrelated = knowledge(2, 1, 5, 0, "Unrelated label");
    unrelated["claim"]["genealogy"]["labels"][id("character", 1)] = json!("Viewer label");
    let world = fixture(vec![subject_belief, unrelated]);
    assert_valid(&world);
    let packet = view::learned_context(
        &world,
        &id("character", 1),
        &id("character", 2),
        &at(1, 0),
        4,
        1,
        100_000,
    )
    .unwrap();
    assert_eq!(packet["labels"].as_array().unwrap().len(), 1);
    assert_eq!(packet["labels"][0]["id"], id("character", 3));
    assert_eq!(packet["labels"][0]["title"], id("character", 5));
}

#[test]
fn canonical_names_require_visible_facts_and_fold_historical_organization_titles() {
    let world = canonical_fixture();
    assert_valid(&world);
    let ids = [
        id("character", 1),
        id("character", 2),
        id("character", 3),
        id("character", 4),
        id("character", 5),
    ];
    assert_eq!(
        view::labels(&world, &ids, &at(0, 0), 10).unwrap()["count"],
        0
    );
    assert_eq!(
        view::labels(&world, &ids, &at(0, 1), 10).unwrap()["count"],
        2
    );
    assert_eq!(
        view::labels(&world, &ids, &at(7, 0), 10).unwrap()["count"],
        2
    );
    assert_eq!(
        view::labels(&world, &ids, &at(7, 1), 10).unwrap()["count"],
        3
    );
    let org = [id("organization", 1)];
    assert_eq!(
        view::labels(&world, &org, &at(5, 0), 1).unwrap()["labels"][0]["title"],
        "Old House"
    );
    assert_eq!(
        view::labels(&world, &org, &at(5, 1), 1).unwrap()["labels"][0]["title"],
        "New House"
    );
    assert_eq!(
        view::discover(&world, "organization", "New", &at(5, 0), 1).unwrap()["count"],
        0
    );
    let historical = view::discover(&world, "organization", "Original", &at(5, 1), 1).unwrap();
    assert_eq!(historical["results"][0]["title"], "New House");
    assert_eq!(historical["results"][0]["matchedName"], "Original House");
    assert_eq!(
        view::discover(&world, "organization", "Future", &at(9, 0), 1).unwrap()["count"],
        0
    );
    assert_eq!(
        view::labels(&world, &[id("event", 1)], &at(5, 0), 1).unwrap()["count"],
        0
    );
    assert_eq!(
        view::labels(&world, &[id("event", 1)], &at(5, 1), 1).unwrap()["count"],
        1
    );
    assert_eq!(
        view::labels(&world, &org, &StoryTime::new("other", 99, 0).unwrap(), 1).unwrap()["count"],
        0
    );
}

#[test]
fn canonical_search_uses_visible_literal_structure_and_citations() {
    let world = canonical_fixture();
    assert_eq!(
        view::search(&world, "New", &at(5, 0), 1).unwrap()["count"],
        0
    );
    let result = view::search(&world, "New", &at(5, 1), 1).unwrap();
    assert_eq!(result["count"], 1);
    let literal = &result["results"][0]["matches"][0];
    assert_eq!(literal["transitionId"], id("transition", 102));
    assert_eq!(literal["citation"]["transitionId"], id("transition", 102));
    assert_eq!(literal["citation"]["revision"], "source revision");
    for query in ["Private", "Future", "Canonical"] {
        assert_eq!(
            view::search(&world, query, &at(9, 0), 10).unwrap()["count"],
            0
        );
    }
    assert_eq!(
        view::search(&world, "birth", &at(7, 0), 1).unwrap()["count"],
        0
    );
    assert_eq!(
        view::search(&world, "birth", &at(7, 1), 1).unwrap()["count"],
        1
    );
    assert!(view::search(&world, "organization", &at(9, 0), 1).is_err());
    assert_eq!(
        view::search(&world, "New", &at(9, 0), 1).unwrap()["count"],
        1
    );
}

#[test]
fn context_is_complete_or_error_at_traversal_and_final_unicode_json_budgets() {
    let world = fixture(vec![knowledge(1, 1, 2, 0, "Mère émérite")]);
    let viewer = id("character", 1);
    let mut expected =
        view::learned_context(&world, &viewer, &viewer, &at(1, 0), 4, 10, 100_000).unwrap();
    let exact = loop {
        let size = serde_json::to_string(&expected).unwrap().chars().count();
        if expected["maxCharacters"] == size {
            break size;
        }
        expected["maxCharacters"] = json!(size);
    };
    assert!(serde_json::to_string(&expected).unwrap().len() > exact);
    assert_eq!(
        view::learned_context(&world, &viewer, &viewer, &at(1, 0), 4, 10, exact).unwrap(),
        expected
    );
    assert!(view::learned_context(&world, &viewer, &viewer, &at(1, 0), 4, 10, exact - 1).is_err());
    assert!(view::learned_context(&world, &viewer, &viewer, &at(1, 0), 0, 10, 100_000).is_err());
    assert_eq!(expected["complete"], true);
    assert_eq!(expected["truncated"], false);
    let canonical = canonical_fixture();
    assert!(view::context(&canonical, &viewer, &at(1, 0), 0, 10, 100_000).is_err());
    assert!(view::context(&canonical, &viewer, &at(1, 0), 4, 10, 80).is_err());
    let packet = view::context(&canonical, &viewer, &at(1, 0), 4, 10, 100_000).unwrap();
    assert_eq!(packet["complete"], true);
    assert_eq!(packet["truncated"], false);
    assert!(
        !serde_json::to_string(&packet)
            .unwrap()
            .contains(&id("vital-history", 2))
    );
}

#[test]
fn input_bounds_are_closed_before_view_materialization() {
    let world = fixture(vec![knowledge(1, 1, 2, 0, "Parent")]);
    let viewer = id("character", 1);
    assert!(view::learned_labels(&world, &viewer, &[], &at(1, 0), 10).is_err());
    assert!(
        view::learned_labels(
            &world,
            &viewer,
            &[id("character", 2), id("character", 2)],
            &at(1, 0),
            10
        )
        .is_err()
    );
    assert!(view::learned_discover(&world, &viewer, "scene", "", &at(1, 0), 10).is_err());
    assert!(
        view::learned_discover(&world, &viewer, "character", &"x".repeat(65), &at(1, 0), 10)
            .is_err()
    );
    assert!(view::learned_search(&world, &viewer, "", &at(1, 0), 10).is_err());
    assert!(view::learned_search(&world, &viewer, &"x".repeat(33), &at(1, 0), 10).is_err());
    assert!(view::learned_search(&world, &viewer, "a b c d e f g h i", &at(1, 0), 10).is_err());
    assert!(view::learned_search(&world, &viewer, "parent", &at(1, 0), 0).is_err());
    assert!(view::learned_context(&world, &viewer, &viewer, &at(1, 0), 4, 10, 79).is_err());
}
