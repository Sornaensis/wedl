//! Authored environmental accounts preserve exact source, time and uncertainty.
use serde_json::{Value as Json, json};
use std::{collections::BTreeMap, path::PathBuf};
use wedl::{
    Error, assertion, character, description, environment, lore,
    model::{Record, StoryTime, World},
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const PERSON: &str = "char_00000000000000000000000001";
const PLACE: &str = "loc_00000000000000000000000001";
const ROLES: [&str; 7] = [
    "habitat",
    "resource",
    "livelihood",
    "trade-practice",
    "infrastructure",
    "technology",
    "era",
];
fn id(n: usize) -> String {
    format!("concept_{n:026}")
}
fn assertion_id(n: usize) -> String {
    format!("assertion_{n:026}")
}
fn section_id(n: usize) -> String {
    format!("description_{n:026}")
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn reference(id: &str) -> Json {
    json!({"kind":"record","id":id})
}
fn interval(start: (i64, i32), end: (i64, i32)) -> Json {
    json!({"kind":"interval","timeline":"main",
        "start":{"timeline":"main","tick":start.0,"order":start.1},
        "end":{"timeline":"main","tick":end.0,"order":end.1}})
}
fn record(kind: &str, id: &str, fields: Json) -> Record {
    let mut data = json!({"schema":"wedl/v1","kind":kind,"id":id,"title":id});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let raw = format!(
        "---\n{}---\nExact source for {id}.\r\nNo inferred resource model.  \r\n",
        serde_yaml::to_string(&data).unwrap()
    );
    source::parse_record(raw.as_bytes(), &format!("story/{id}.md")).unwrap()
}
fn concept(n: usize, role: &str, values: Json) -> Record {
    record(
        "lore-concept",
        &id(n),
        json!({"domain":"authored.context","profile":format!("context-{role}"),
        "values":values,"valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]}),
    )
}
fn section(n: usize) -> Json {
    json!({"id":section_id(n),"heading":format!("Account {n}"),"markdown":"Exact dated account\r\nTrailing  \n",
        "valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]})
}
fn account(n: usize, subject: usize, predicate: &str, value: Json, classification: &str) -> Record {
    let mut fields = json!({"domain":"authored.context","subject":reference(&id(subject)),"predicate":predicate,
        "value":value,"classification":classification,"valid":{"kind":"timeless"},
        "evidence":[],"audience":["author"],"perspectives":["author"]});
    if classification == "attributed-claim" {
        fields["attributed_to"] = reference(PERSON);
    }
    record("assertion", &assertion_id(n), fields)
}
fn world(mut records: Vec<Record>) -> World {
    let entity = json!({"type":"entity","kinds":["lore-concept"]});
    let links = json!({"type":"array","items":entity,"max_items":1024});
    let literal = json!({"type":"object","required":{"kind":{"type":"string"},"id":{"type":"string"}},"optional":{}});
    let fields = json!({"type":"object","required":{},"optional":{
        "eras":links,"dependencies":links,"resources":links,"habitats":links,"technologies":links,"infrastructures":links,
        "places":{"type":"array","items":{"type":"entity","kinds":["location"]},"max_items":64},
        "quantity":{"type":"object","required":{"amount_text":{"type":"string"}},"optional":{"unit_text":{"type":"string"},"qualification":{"type":"string"}}},
        "note":{"type":"string"},"date_text":{"type":"string"},"number":{"type":"number"},"unknown":{"type":"null"},
        "literal":literal,"other_links":links}});
    let profiles = ROLES
        .iter()
        .copied()
        .chain(["other"])
        .map(|role| json!({"id":format!("context-{role}"),"label":role,"fields":fields}))
        .collect::<Vec<_>>();
    records.push(record("world",WORLD,json!({"capabilities":["lore-core-v1","descriptions-v1","assertions-v1"],
        "timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"default_timeline":"main","state_keys":{},
        "profiles":profiles,"predicates":[
            {"id":"context-availability","subject_kinds":["lore-concept"],"value_type":{"type":"string"}},
            {"id":"context-dependency-evidence","subject_kinds":["lore-concept"],"value_type":{"type":"string"}},
            {"id":"other-account","subject_kinds":["lore-concept"],"value_type":{"type":"string"}}
        ]})));
    records.push(record("character", PERSON, json!({"initial_state":{}})));
    records.push(record("location", PLACE, json!({})));
    source::world_from_records(PathBuf::new(), records, None).unwrap()
}
fn base() -> World {
    world(vec![concept(1, "habitat", json!({"dependencies":[]}))])
}
fn edit(world: &World, id: &str, field: &str, value: Json) -> World {
    let original = &world.records[id];
    let mut data = original.frontmatter.clone();
    data[field] = serde_yaml::to_value(value).unwrap();
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&data).unwrap(),
        original.body
    );
    let replacement = source::parse_record(raw.as_bytes(), &original.source_path).unwrap();
    source::world_from_records(
        world.root.clone(),
        world
            .records
            .values()
            .map(|r| {
                if r.id == id {
                    replacement.clone()
                } else {
                    r.clone()
                }
            })
            .collect(),
        world.revision.clone(),
    )
    .unwrap()
}
fn profile_field(world: &World, role: &str, field: &str, shape: Json) -> World {
    let mut profiles =
        serde_json::to_value(world.world_record().unwrap().get("profiles").unwrap()).unwrap();
    let profile = profiles
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["id"] == format!("context-{role}"))
        .unwrap();
    profile["fields"]["optional"][field] = shape;
    edit(world, WORLD, "profiles", profiles)
}
fn predicate_type(world: &World, shape: Json) -> World {
    let mut predicates =
        serde_json::to_value(world.world_record().unwrap().get("predicates").unwrap()).unwrap();
    predicates[0]["value_type"] = shape;
    edit(world, WORLD, "predicates", predicates)
}
fn add(world: &World, record: Record) -> World {
    let mut records = world.records.values().cloned().collect::<Vec<_>>();
    records.push(record);
    source::world_from_records(world.root.clone(), records, world.revision.clone()).unwrap()
}
fn read(world: &World, roots: &[usize]) -> Json {
    environment::context(
        world,
        &roots.iter().map(|n| id(*n)).collect::<Vec<_>>(),
        &at(5, 0),
        "author",
        "author",
    )
    .unwrap()
}
fn ids(result: &Json) -> Vec<String> {
    result["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["item"]["conceptId"].as_str().unwrap().to_owned())
        .collect()
}
fn valid(world: &World) {
    let report = validation::validate_world(world);
    assert!(report.valid, "{:?}", report.diagnostics);
}

#[test]
fn seven_roles_retain_native_values_evidence_and_exact_citations() {
    let mut records = ROLES
        .iter()
        .enumerate()
        .map(|(n, role)| concept(n + 1, role, json!({})))
        .collect::<Vec<_>>();
    records.push(account(
        1,
        1,
        "context-availability",
        json!("The keeper reports reeds"),
        "attributed-claim",
    ));
    let world = edit(
        &world(records),
        &id(1),
        "values",
        json!({"places":[reference(PLACE)],"eras":[reference(&id(7))],
        "quantity":{"amount_text":"about two dozen","unit_text":"baskets","qualification":"keeper estimate"},
        "number":9007199254740993u64,"unknown":null,"note":"","dependencies":[]}),
    );
    let world = edit(&world, &id(1), "descriptions", json!([section(1)]));
    valid(&world);
    let result = read(&world, &[7, 6, 5, 4, 3, 2, 1]);
    assert_eq!(ids(&result), (1..=7).map(id).collect::<Vec<_>>());
    assert_eq!(result["count"], 7);
    assert_eq!(result["protocol"], "wedl-rust/v1");
    assert_eq!(result["scope"], "author-as-of-environment-context");
    assert_eq!(result["at"]["tick"], "5");
    let catalogue = lore::catalog(&world, Some(&at(5, 0)), "author", "author", None, 100).unwrap();
    for (index, role) in ROLES.iter().enumerate() {
        assert_eq!(result["records"][index]["role"], *role);
        assert_eq!(result["records"][index]["item"], catalogue["items"][index]);
    }
    let values = &result["records"][0]["item"]["values"];
    assert_eq!(values["number"], "9007199254740993");
    assert_eq!(values["quantity"]["amount_text"], "about two dozen");
    assert_eq!(values["unknown"], Json::Null);
    assert_eq!(values["note"], "");
    assert_eq!(values["dependencies"], json!([]));
    assert_eq!(
        result["canon"][0]["result"],
        assertion::as_of(&world, &id(1), Some(&at(5, 0)), "author", "author", 512).unwrap()
    );
    assert_eq!(
        result["sections"][0]["result"],
        description::list(&world, &id(1), Some(&at(5, 0)), "author", "author", 512).unwrap()
    );
    let hooks = result["hooks"].as_array().unwrap();
    assert!(
        hooks
            .iter()
            .any(|h| h["target"] == reference(&assertion_id(1)))
    );
    assert!(hooks.iter().any(|h| h["target"]
        == json!({"kind":"section","ownerId":id(1),"sectionId":section_id(1)})
        && h["citation"]["section"] == "descriptions[0]"));
}

#[test]
fn direct_endpoints_do_not_become_traversal_or_evidence_roots_and_cycles_are_literal() {
    let world = world(vec![
        concept(
            1,
            "habitat",
            json!({"resources":[reference(&id(1)),reference(&id(2))]}),
        ),
        concept(
            2,
            "resource",
            json!({"habitats":[reference(&id(1))],"dependencies":[reference(&id(3))]}),
        ),
        concept(3, "technology", json!({})),
        account(
            1,
            2,
            "context-availability",
            json!("Endpoint account"),
            "author-fact",
        ),
    ]);
    let world = edit(&world, &id(2), "descriptions", json!([section(1)]));
    let result = read(&world, &[1]);
    assert_eq!(ids(&result), vec![id(1), id(2)]);
    assert_eq!(result["links"].as_array().unwrap().len(), 2);
    assert_eq!(result["canon"].as_array().unwrap().len(), 1);
    assert_eq!(result["canon"][0]["result"]["items"], json!([]));
    assert_eq!(result["sections"][0]["result"]["items"], json!([]));
    assert!(!result.to_string().contains("Endpoint account"));
    let explicit = read(&world, &[1, 2]);
    assert_eq!(ids(&explicit), vec![id(1), id(2), id(3)]);
    assert_eq!(explicit["canon"][1]["result"]["count"], 1);
    assert_eq!(explicit["sections"][1]["result"]["count"], 1);
}

#[test]
fn projected_links_keep_original_array_and_concept_link_indexes() {
    let world = world(vec![
        concept(
            1,
            "habitat",
            json!({"resources":[reference(&id(2)),reference(&id(3)),reference(&id(3))]}),
        ),
        concept(2, "resource", json!({})),
        concept(3, "resource", json!({})),
    ]);
    let world = edit(&world, &id(2), "audience", json!(["private"]));
    let world = edit(
        &world,
        &id(1),
        "concept_links",
        json!([
            {"concept_id":id(2),"relation":"hidden","valid":{"kind":"timeless"}},
            {"concept_id":id(3),"relation":"explicit","valid":interval((5,0),(9,0))}
        ]),
    );
    valid(&world);
    let result = read(&world, &[1]);
    assert_eq!(ids(&result), vec![id(1), id(3)]);
    let links = result["links"].as_array().unwrap();
    assert_eq!(links.len(), 3);
    assert_eq!(links[0]["sourceField"], "values.resources[1].id");
    assert_eq!(links[0]["ordinal"], 1);
    assert_eq!(links[1]["sourceField"], "values.resources[2].id");
    assert_eq!(links[2]["sourceField"], "concept_links[1]");
    assert_eq!(links[2]["ordinal"], 1);
    assert_eq!(links[2]["relation"], "explicit");
    assert!(!result.to_string().contains(&id(2)));
    for link in links {
        assert_eq!(link["sourceField"], link["citation"]["section"]);
        assert_eq!(link["targetCitation"]["entityId"], id(3));
    }
}

#[test]
fn familiar_fields_and_ref_shaped_literals_do_not_supply_links() {
    let literal = json!({"type":"object","required":{"kind":{"type":"string"},"id":{"type":"string"}},"optional":{}});
    let world = world(vec![
        concept(
            1,
            "habitat",
            json!({"resources":[reference(&id(2))],"dependencies":reference(&id(2)),"other_links":[reference(&id(2))]}),
        ),
        concept(2, "resource", json!({})),
    ]);
    let world = profile_field(
        &world,
        "habitat",
        "resources",
        json!({"type":"array","items":literal,"max_items":64}),
    );
    let world = profile_field(&world, "habitat", "dependencies", literal);
    valid(&world);
    let result = read(&world, &[1]);
    assert_eq!(ids(&result), vec![id(1)]);
    assert_eq!(result["links"], json!([]));
    assert_eq!(
        result["records"][0]["item"]["values"]["resources"],
        json!([reference(&id(2))])
    );
    let empty = edit(
        &world,
        &id(1),
        "values",
        json!({"resources":[],"dependencies":reference(&id(2))}),
    );
    assert_eq!(read(&empty, &[1])["links"], json!([]));
}

#[test]
fn nested_entity_fields_are_not_root_entity_arrays() {
    let world = world(vec![
        concept(
            1,
            "habitat",
            json!({"resources":[{"id":reference(&id(2))}]}),
        ),
        concept(2, "resource", json!({})),
    ]);
    let world = profile_field(
        &world,
        "habitat",
        "resources",
        json!({"type":"array","max_items":64,"items":{
        "type":"object","required":{"id":{"type":"entity","kinds":["lore-concept"]}},"optional":{}}}),
    );
    valid(&world);
    let result = read(&world, &[1]);
    assert_eq!(ids(&result), vec![id(1)]);
    assert_eq!(result["links"], json!([]));
    assert_eq!(
        result["records"][0]["item"]["references"][0]["sourceField"],
        "values.resources[0].id.id"
    );
}

#[test]
fn unavailable_roots_share_one_error_without_source_details() {
    let base = world(vec![
        concept(1, "habitat", json!({})),
        concept(2, "other", json!({})),
    ]);
    let mut cases = vec![(base.clone(), id(99)), (base.clone(), id(2))];
    for (field, value) in [
        ("audience", json!(["private"])),
        ("perspectives", json!(["private"])),
        ("status", json!("draft")),
        ("status", json!("retired")),
        ("valid", interval((10, 0), (20, 0))),
    ] {
        cases.push((edit(&base, &id(1), field, value), id(1)));
    }
    for (world, root) in cases {
        let error =
            environment::context(&world, &[root], &at(5, 0), "author", "author").unwrap_err();
        assert!(matches!(error, Error::NotFound(_)));
        assert_eq!(error.to_string(), "environment context is unavailable");
    }
}

#[test]
fn exact_native_intervals_apply_independently_to_roots_links_and_sections() {
    let world = world(vec![
        concept(1, "habitat", json!({"eras":[reference(&id(2))]})),
        concept(
            2,
            "era",
            json!({"date_text":"the third season, not an executable date"}),
        ),
    ]);
    let world = edit(&world, &id(2), "valid", interval((5, 0), (5, 1)));
    let world = edit(
        &world,
        &id(1),
        "concept_links",
        json!([{ "concept_id":id(2),"relation":"documented-era","valid":interval((5,1),(5,1))}]),
    );
    let mut s = section(1);
    s["valid"] = interval((5, 1), (5, 2));
    let world = edit(&world, &id(1), "descriptions", json!([s]));
    for (order, records, links, sections) in [
        (-1, 1, 0, 0),
        (0, 2, 1, 0),
        (1, 2, 2, 1),
        (2, 1, 0, 1),
        (3, 1, 0, 0),
    ] {
        let result =
            environment::context(&world, &[id(1)], &at(5, order), "author", "author").unwrap();
        assert_eq!(result["count"], records);
        assert_eq!(result["links"].as_array().unwrap().len(), links);
        assert_eq!(result["sections"][0]["result"]["count"], sections);
    }
    let other = environment::context(
        &world,
        &[id(1)],
        &StoryTime::new("other", 5, 1).unwrap(),
        "author",
        "author",
    )
    .unwrap();
    assert_eq!(other["count"], 1);
    assert_eq!(other["sections"][0]["result"]["count"], 0);
}

#[test]
fn classified_accounts_remain_separate_and_absence_is_explicit_unknown() {
    let world = world(vec![
        concept(
            1,
            "resource",
            json!({"quantity":{"amount_text":"about two dozen"}}),
        ),
        account(
            1,
            1,
            "context-availability",
            json!("Reeds remain available"),
            "author-fact",
        ),
        account(
            2,
            1,
            "context-availability",
            json!("No reeds reported"),
            "attributed-claim",
        ),
        account(
            3,
            1,
            "context-availability",
            json!("No reeds reported"),
            "unresolved",
        ),
        account(
            4,
            1,
            "other-account",
            json!("Unrelated account"),
            "author-fact",
        ),
    ]);
    let result = read(&world, &[1]);
    assert_eq!(result["canon"][0]["result"]["count"], 4);
    assert_eq!(result["competingAccounts"].as_array().unwrap().len(), 1);
    assert_eq!(
        result["competingAccounts"][0]["targets"],
        json!([
            reference(&assertion_id(1)),
            reference(&assertion_id(2)),
            reference(&assertion_id(3))
        ])
    );
    assert_eq!(
        result["canon"][0]["result"]["items"][1]["attributedTo"],
        reference(PERSON)
    );
    assert_eq!(
        result["unknowns"],
        json!([
            {"owner":reference(&id(1)),"stream":"context-availability","basis":"explicit-unresolved","target":reference(&assertion_id(3))},
            {"owner":reference(&id(1)),"stream":"context-dependency-evidence","basis":"no-admitted-account"}
        ])
    );
    assert!(
        !result["hooks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hook| hook["target"] == reference(&assertion_id(4)))
    );
    assert_eq!(
        result["records"][0]["item"]["values"]["quantity"]["amount_text"],
        "about two dozen"
    );
}

#[test]
fn literal_account_comparison_preserves_numeric_string_identity_and_structures() {
    for (shape, left, right, competes) in [
        (
            json!({"type":"enum","values":[1,"1"]}),
            json!(1),
            json!("1"),
            true,
        ),
        (
            json!({"type":"enum","values":[9007199254740993u64,9007199254740995u64]}),
            json!(9007199254740993u64),
            json!(9007199254740995u64),
            true,
        ),
        (json!({"type":"null"}), Json::Null, Json::Null, false),
        (json!({"type":"boolean"}), json!(false), json!(true), true),
        (
            json!({"type":"object","required":{"a":{"type":"string"}},"optional":{}}),
            json!({"a":"one"}),
            json!({"a":"one"}),
            false,
        ),
        (
            json!({"type":"array","items":{"type":"string"},"max_items":4}),
            json!(["one"]),
            json!([]),
            true,
        ),
    ] {
        let world = world(vec![
            concept(1, "resource", json!({})),
            account(1, 1, "context-availability", left, "author-fact"),
            account(2, 1, "context-availability", right, "unresolved"),
        ]);
        let world = predicate_type(&world, shape);
        valid(&world);
        assert_eq!(
            !read(&world, &[1])["competingAccounts"]
                .as_array()
                .unwrap()
                .is_empty(),
            competes
        );
    }
}

#[test]
fn assertion_and_description_corrections_use_the_existing_admitted_results() {
    let world = world(vec![
        concept(1, "resource", json!({})),
        account(
            1,
            1,
            "context-availability",
            json!("First account"),
            "author-fact",
        ),
        account(
            2,
            1,
            "context-availability",
            json!("Corrected account"),
            "author-fact",
        ),
        account(
            3,
            1,
            "context-availability",
            json!("Another correction"),
            "author-fact",
        ),
    ]);
    let world = edit(&world, &assertion_id(2), "replaces", json!(assertion_id(1)));
    let world = edit(&world, &assertion_id(3), "replaces", json!(assertion_id(1)));
    let first = section(1);
    let mut corrected = section(2);
    corrected["replaces"] = json!(section_id(1));
    let world = edit(&world, &id(1), "descriptions", json!([first, corrected]));
    let result = read(&world, &[1]);
    assert_eq!(
        result["canon"][0]["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["assertionId"].clone())
            .collect::<Vec<_>>(),
        vec![json!(assertion_id(2)), json!(assertion_id(3))]
    );
    assert_eq!(
        result["competingAccounts"][0]["targets"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        result["sections"][0]["result"]["items"][0]["sectionId"],
        section_id(2)
    );
    assert_eq!(
        result["sections"][0]["result"]["items"][0]["citation"]["section"],
        "descriptions[1]"
    );
    let hidden = edit(&world, &assertion_id(2), "audience", json!(["private"]));
    let hidden = edit(&hidden, &assertion_id(3), "audience", json!(["private"]));
    let result = read(&hidden, &[1]);
    assert_eq!(
        result["canon"][0]["result"]["items"][0]["assertionId"],
        assertion_id(1)
    );
    assert_eq!(result["competingAccounts"], json!([]));
}

#[test]
fn historical_evidence_is_not_promoted_into_edit_hooks_or_current_citations() {
    let world = world(vec![
        concept(1, "resource", json!({})),
        account(
            1,
            1,
            "context-availability",
            json!("Current account"),
            "author-fact",
        ),
    ]);
    let world = edit(
        &world,
        &assertion_id(1),
        "evidence",
        json!([{"revision":"a".repeat(40),"blob_oid":"b".repeat(64),"path":"story/removed/source.md","target":{"kind":"section","owner_id":"former-owner","id":"old-section"}}]),
    );
    valid(&world);
    let result = read(&world, &[1]);
    assert_eq!(
        result["canon"][0]["result"]["items"][0]["evidence"],
        json!([])
    );
    assert!(!result.to_string().contains("former-owner"));
    assert!(!result.to_string().contains("story/removed/source.md"));
    let hook = result["hooks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["target"] == reference(&assertion_id(1)))
        .unwrap();
    assert_eq!(hook["citation"]["entityId"], assertion_id(1));
    assert_eq!(hook["citation"]["fingerprint"], world.fingerprint);
}

#[test]
fn roots_are_closed_exact_unique_and_bounded() {
    let world = world((1..=9).map(|n| concept(n, "habitat", json!({}))).collect());
    for roots in [
        vec![],
        vec![id(1), id(1)],
        vec!["A habitat title".into()],
        vec![PERSON.into()],
        (1..=9).map(id).collect(),
    ] {
        assert!(matches!(
            environment::context(&world, &roots, &at(5, 0), "author", "author"),
            Err(Error::InvalidInput(_))
        ));
    }
    assert_eq!(read(&world, &(1..=8).collect::<Vec<_>>())["count"], 8);
    assert!(
        environment::context(
            &world,
            &[id(1)],
            &StoryTime::new("missing", 5, 0).unwrap(),
            "author",
            "author"
        )
        .is_err()
    );
    assert!(environment::context(&world, &[id(1)], &at(5, 0), "", "author").is_err());
}

#[test]
fn record_limit_counts_distinct_admitted_selected_endpoints_only() {
    for count in [31, 32] {
        let mut refs = (100..100 + count)
            .map(|n| reference(&id(n)))
            .collect::<Vec<_>>();
        refs.push(reference(&id(100)));
        let mut records = vec![concept(1, "habitat", json!({"resources":refs}))];
        records.extend((100..100 + count).map(|n| concept(n, "resource", json!({}))));
        let world = world(records);
        let result = environment::context(&world, &[id(1)], &at(5, 0), "author", "author");
        if count == 31 {
            assert_eq!(result.unwrap()["count"], 32);
        } else {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("32 selected records")
            );
        }
    }
    let mut records = vec![concept(
        1,
        "habitat",
        json!({"resources":(100..140).map(|n|reference(&id(n))).collect::<Vec<_>>()}),
    )];
    records.extend((100..140).map(|n| concept(n, "other", json!({}))));
    records.extend((200..300).map(|n| concept(n, "resource", json!({}))));
    assert_eq!(read(&world(records), &[1])["count"], 1);
}

#[test]
fn aggregate_evidence_limit_counts_both_independent_streams_after_admission() {
    let mut world = world((1..=8).map(|n| concept(n, "habitat", json!({}))).collect());
    for n in 1..=8 {
        world = edit(
            &world,
            &id(n),
            "descriptions",
            json!((1..=64).map(section).collect::<Vec<_>>()),
        );
    }
    let roots = (1..=8).map(id).collect::<Vec<_>>();
    let at_boundary = environment::context(&world, &roots, &at(5, 0), "author", "author")
        .unwrap_err()
        .to_string();
    assert!(at_boundary.contains("262144-byte"), "{at_boundary}");
    let world = add(
        &world,
        account(
            1,
            1,
            "context-availability",
            json!("One extra admitted account"),
            "author-fact",
        ),
    );
    let too_many = environment::context(&world, &roots, &at(5, 0), "author", "author")
        .unwrap_err()
        .to_string();
    assert!(too_many.contains("512-item"), "{too_many}");
    let world = edit(&world, &assertion_id(1), "audience", json!(["private"]));
    assert!(
        environment::context(&world, &roots, &at(5, 0), "author", "author")
            .unwrap_err()
            .to_string()
            .contains("262144-byte")
    );
}

#[test]
fn complete_report_counts_exact_compact_utf8_bytes_and_newline() {
    let world = edit(&base(), &id(1), "values", json!({"note":""}));
    let result = read(&world, &[1]);
    let spare = 262144 - serde_json::to_vec(&result).unwrap().len() - 1;
    let text = "é".repeat(spare / 2) + &"x".repeat(spare % 2);
    let exact = edit(&world, &id(1), "values", json!({"note":text}));
    let result = read(&exact, &[1]);
    assert_eq!(serde_json::to_vec(&result).unwrap().len() + 1, 262144);
    let over = edit(&world, &id(1), "values", json!({"note":text+"x"}));
    assert!(
        environment::context(&over, &[id(1)], &at(5, 0), "author", "author")
            .unwrap_err()
            .to_string()
            .contains("262144-byte")
    );
}

#[test]
fn contexts_require_the_three_existing_capabilities() {
    for omitted in ["lore-core-v1", "descriptions-v1", "assertions-v1"] {
        let caps = ["lore-core-v1", "descriptions-v1", "assertions-v1"]
            .into_iter()
            .filter(|cap| *cap != omitted)
            .collect::<Vec<_>>();
        let world = edit(&base(), WORLD, "capabilities", json!(caps));
        assert!(
            environment::context(&world, &[id(1)], &at(5, 0), "author", "author")
                .unwrap_err()
                .to_string()
                .contains("require declared")
        );
    }
}

#[test]
fn querying_or_changing_canonical_context_does_not_rewrite_source_or_character_evidence() {
    let before = base();
    let context = character::context(&before, PERSON, &at(5, 0), 100000).unwrap();
    let search = character::search(&before, PERSON, "resource account", &at(5, 0), 100).unwrap();
    let world = edit(&before, &id(1), "descriptions", json!([section(1)]));
    let world = add(
        &world,
        account(
            1,
            1,
            "context-availability",
            json!("A canonical resource account"),
            "author-fact",
        ),
    );
    let raw = world
        .records
        .iter()
        .map(|(id, r)| (id.clone(), r.raw_bytes.clone()))
        .collect::<BTreeMap<_, _>>();
    read(&world, &[1]);
    assert_eq!(
        world
            .records
            .iter()
            .map(|(id, r)| (id.clone(), r.raw_bytes.clone()))
            .collect::<BTreeMap<_, _>>(),
        raw
    );
    assert_eq!(
        character::context(&world, PERSON, &at(5, 0), 100000).unwrap(),
        context
    );
    assert_eq!(
        character::search(&world, PERSON, "resource account", &at(5, 0), 100).unwrap(),
        search
    );
}
