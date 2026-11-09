//! Canonical propositions retain alternatives, exact types and independent applicability.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeMap, path::PathBuf};
use wedl::{
    Error, assertion, character,
    model::{Record, StoryTime, World},
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const ANA: &str = "char_00000000000000000000000001";
const OTHER: &str = "char_00000000000000000000000002";
const CONCEPT: &str = "concept_00000000000000000000000001";
fn id(n: usize) -> String {
    format!("assertion_{n:026}")
}
fn reference(id: &str) -> Json {
    json!({"kind":"record","id":id})
}
fn yaml(value: &str) -> Yaml {
    serde_yaml::from_str(value).unwrap()
}
fn record(kind: &str, id: &str, fields: Json) -> Record {
    let mut data = yaml(&format!(
        "schema: wedl/v1\nkind: {kind}\nid: {id}\ntitle: {id}\n"
    ));
    data.as_mapping_mut().unwrap().extend(
        serde_yaml::to_value(fields)
            .unwrap()
            .as_mapping()
            .unwrap()
            .clone(),
    );
    let raw = format!(
        "---\n{}---\nAn authored body for {id}.\r\nRetain mixed\rline endings and trailing  \n",
        serde_yaml::to_string(&data).unwrap()
    );
    source::parse_record(raw.as_bytes(), &format!("story/{id}.md")).unwrap()
}
fn proposition(n: usize, value: Json) -> Record {
    record(
        "assertion",
        &id(n),
        json!({"domain":"test.account","subject":reference(ANA),
        "predicate":"has-account","value":value,"classification":"author-fact","valid":{"kind":"timeless"},
        "evidence":[],"audience":["author"],"perspectives":["author"]}),
    )
}
fn world(value_type: Json, records: Vec<Record>) -> World {
    let mut all = vec![
        record(
            "world",
            WORLD,
            json!({
                "capabilities":["lore-core-v1","assertions-v1"],"default_timeline":"main",
                "timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"state_keys":{},
                "profiles":[{"id":"account-subject","label":"An account subject","fields":{"type":"object","required":{},"optional":{}}}],
                "predicates":[{"id":"has-account","subject_kinds":["character","lore-concept"],"value_type":value_type},
                    {"id":"other-account","subject_kinds":["character","lore-concept"],"value_type":{"type":"string"}}]
            }),
        ),
        record("character", ANA, json!({"initial_state":{}})),
        record("character", OTHER, json!({"initial_state":{}})),
        record(
            "lore-concept",
            CONCEPT,
            json!({"profile":"account-subject","domain":"test.subject","values":{},
        "valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]}),
        ),
    ];
    all.extend(records);
    source::world_from_records(PathBuf::new(), all, None).unwrap()
}
fn base() -> World {
    world(
        json!({"type":"string"}),
        vec![proposition(1, json!("First authored account"))],
    )
}
fn edit(world: &World, id: &str, field: &str, value: Json) -> World {
    edit_yaml(world, id, field, serde_yaml::to_value(value).unwrap())
}
fn edit_yaml(world: &World, id: &str, field: &str, value: Yaml) -> World {
    let original = &world.records[id];
    let mut data = original.frontmatter.clone();
    data[field] = value;
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&data).unwrap(),
        original.body
    );
    let replacement = source::parse_record(raw.as_bytes(), &original.source_path).unwrap();
    source::world_from_records(
        PathBuf::new(),
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
        None,
    )
    .unwrap()
}
fn add(world: &World, record: Record) -> World {
    let mut records = world.records.values().cloned().collect::<Vec<_>>();
    records.push(record);
    source::world_from_records(PathBuf::new(), records, None).unwrap()
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn list(world: &World, target: &str, at: Option<&StoryTime>) -> Json {
    assertion::as_of(world, target, at, "author", "author", 1000).unwrap()
}
fn ids(result: &Json) -> Vec<String> {
    result["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["assertionId"].as_str().unwrap().to_owned())
        .collect()
}
fn valid(world: &World) {
    let result = validation::validate_world(world);
    assert!(result.valid, "{:?}", result.diagnostics);
}
fn invalid(world: &World) {
    assert!(!validation::validate_world(world).valid);
    assert!(assertion::as_of(world, ANA, None, "author", "author", 100).is_err());
}
fn historical(target: Json) -> Json {
    json!({"revision":"a".repeat(40),"path":"story/removed/account.md","blob_oid":"b".repeat(64),"target":target})
}

#[test]
fn predicate_declarations_are_current_closed_and_independent_of_assertion_installation() {
    let declarations =
        json!([{"id":"is-known","subject_kinds":["character"],"value_type":{"type":"boolean"}}]);
    let no_use = edit(
        &world(json!({"type":"string"}), vec![]),
        WORLD,
        "capabilities",
        json!(["lore-core-v1"]),
    );
    valid(&no_use);
    assert_eq!(assertion::predicates(&no_use, 2).unwrap()["count"], 2);
    assert!(assertion::as_of(&no_use, ANA, None, "author", "author", 100).is_err());
    assert!(
        assertion::validate_predicates_request(&serde_yaml::to_value(&declarations).unwrap())
            .is_ok()
    );
    let mut variants = vec![
        json!({}),
        json!(null),
        json!([declarations[0].clone(), declarations[0].clone()]),
    ];
    for (field, value) in [
        ("version", json!(1)),
        ("id", json!("Bad")),
        ("subject_kinds", json!([])),
        ("subject_kinds", json!(["character", "character"])),
        ("subject_kinds", json!(["hypothesis"])),
        ("subject_kinds", json!(["unknown"])),
        ("value_type", json!({"type":"boolean","default":true})),
    ] {
        let mut variant = declarations.clone();
        variant[0][field] = value;
        variants.push(variant);
    }
    for value in variants {
        assert!(
            assertion::validate_predicates_request(&serde_yaml::to_value(value).unwrap()).is_err()
        );
    }
    let missing_lore = edit(&no_use, WORLD, "capabilities", json!([]));
    assert!(!validation::validate_world(&missing_lore).valid);
    let misplaced = edit(&no_use, ANA, "predicates", declarations);
    assert!(!validation::validate_world(&misplaced).valid);
}

#[test]
fn classifications_and_attribution_retain_contradictory_accounts_without_a_winner() {
    let mut source = world(
        json!({"type":"string"}),
        vec![
            proposition(1, json!("Blue")),
            proposition(2, json!("Red")),
            proposition(3, json!("Green")),
        ],
    );
    source = edit(&source, &id(2), "classification", json!("attributed-claim"));
    source = edit(&source, &id(2), "attributed_to", reference(OTHER));
    source = edit(&source, &id(3), "classification", json!("unresolved"));
    source = edit(
        &source,
        &id(1),
        "provenance",
        json!("An explicit author account"),
    );
    valid(&source);
    let result = list(&source, ANA, Some(&at(5, 0)));
    assert_eq!(ids(&result), [id(1), id(2), id(3)]);
    assert_eq!(
        result["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["classification"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["author-fact", "attributed-claim", "unresolved"]
    );
    assert_eq!(result["items"][1]["attributedTo"], reference(OTHER));
    assert_eq!(
        result["items"][0]["provenance"],
        "An explicit author account"
    );
    assert_eq!(
        result["items"][1]["references"][1]["sourceField"],
        "attributed_to.id"
    );
    assert!(
        result["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["evidence"] == json!([]) && v.get("body").is_none())
    );
    assert_eq!(list(&source, &id(2), Some(&at(5, 0)))["count"], 1);
}

#[test]
fn source_shapes_require_exact_types_references_classification_and_attribution() {
    let source = base();
    for (field, value) in [
        ("classification", json!("accepted")),
        ("classification", json!("attributed-claim")),
        ("attributed_to", reference(ANA)),
        ("subject", json!(ANA)),
        ("subject", reference("char_00000000000000000000000099")),
        ("predicate", json!("missing")),
        ("value", json!(false)),
        ("status", json!("cancelled")),
        ("domain", json!("undotted")),
        ("audience", json!([])),
        ("audience", json!(["z", "a"])),
        ("perspectives", json!(["author", "author"])),
        ("provenance", json!(null)),
        ("evidence", json!(null)),
        ("replaces", json!(null)),
        ("valid", json!({"kind":"unknown"})),
    ] {
        invalid(&edit(&source, &id(1), field, value));
    }
    let wrong_attributor = edit(
        &edit(&source, &id(1), "classification", json!("attributed-claim")),
        &id(1),
        "attributed_to",
        reference(&id(1)),
    );
    invalid(&wrong_attributor);
    invalid(&edit(
        &source,
        WORLD,
        "capabilities",
        json!(["lore-core-v1"]),
    ));
    for status in ["draft", "retired"] {
        valid(&edit(&source, &id(1), "status", json!(status)));
    }
}

#[test]
fn historical_evidence_is_closed_syntax_and_never_current_reference_authority() {
    let missing = "removed-historical-record";
    let citations = json!([
        historical(reference(missing)),
        historical(json!({"kind":"section","owner_id":missing,"id":"old-heading"})),
        historical(json!({"kind":"transition","owner_id":missing,"id":"old-transition"}))
    ]);
    let source = edit(&base(), &id(1), "evidence", citations.clone());
    valid(&source);
    assert_eq!(
        assertion::references(&source, &source.records[&id(1)]).unwrap(),
        vec![("subject.id".into(), ANA.into())]
    );
    let result = list(&source, ANA, None);
    assert_eq!(result["items"][0]["evidence"], json!([]));
    assert!(!result.to_string().contains(missing));
    assert_eq!(result["items"][0]["citation"]["entityId"], id(1));
    let mut mutations = vec![json!([citations[0].clone(), citations[0].clone()])];
    for (field, value) in [
        ("revision", json!("A".repeat(40))),
        ("blob_oid", json!("g".repeat(40))),
        ("path", json!("../private.md")),
        ("path", json!("/absolute.md")),
        ("path", json!("C:/old.md")),
        ("path", json!("story//old.md")),
        ("path", json!("story/./old.md")),
        ("path", json!("story\\old.md")),
        ("target", json!({"kind":"unknown","id":"x"})),
        ("target", json!({"kind":"record","id":"x","owner_id":"y"})),
        ("target", json!({"kind":"section","id":"x"})),
        ("target", json!({"kind":[],"id":"x"})),
    ] {
        let mut citation = citations[0].clone();
        citation[field] = value;
        mutations.push(json!([citation]));
    }
    for value in mutations {
        assert!(
            assertion::validate_evidence_request(&serde_yaml::to_value(value).unwrap()).is_err()
        );
    }
}

#[test]
fn replacements_retain_exact_subject_predicate_and_validity_and_reject_cycles() {
    let source = edit(
        &world(
            json!({"type":"string"}),
            vec![proposition(1, json!("Blue")), proposition(2, json!("Red"))],
        ),
        &id(2),
        "replaces",
        json!(id(1)),
    );
    valid(&source);
    for (field, value) in [
        ("subject", reference(OTHER)),
        ("predicate", json!("other-account")),
        ("valid", json!({"kind":"interval","timeline":"main"})),
        ("replaces", json!("assertion_00000000000000000000000099")),
    ] {
        invalid(&edit(&source, &id(2), field, value));
    }
    invalid(&edit(&source, &id(1), "replaces", json!(id(2))));
    invalid(&edit(&source, &id(2), "replaces", json!(id(2))));
    assert_eq!(ids(&list(&source, ANA, Some(&at(0, 0)))), [id(2)]);
    assert_eq!(ids(&list(&source, ANA, None)), [id(1), id(2)]);
    assert_eq!(list(&source, &id(1), Some(&at(0, 0)))["count"], 0);
}

#[test]
fn replacement_chains_and_forks_are_explicit_and_do_not_use_lexical_edit_order() {
    let mut source = world(
        json!({"type":"string"}),
        vec![
            proposition(1, json!("Corrected")),
            proposition(2, json!("Alternative")),
            proposition(3, json!("Old")),
            proposition(4, json!("Original")),
        ],
    );
    source = edit(&source, &id(3), "replaces", json!(id(4)));
    source = edit(&source, &id(1), "replaces", json!(id(3)));
    source = edit(&source, &id(2), "replaces", json!(id(3)));
    valid(&source);
    let result = list(&source, ANA, Some(&at(0, 0)));
    assert_eq!(ids(&result), [id(1), id(2)]);
    assert_eq!(result["items"][0]["replaces"], id(3));
    assert_eq!(
        result["items"][0]["references"][1]["sourceField"],
        "replaces"
    );
    assert_eq!(list(&source, ANA, None)["count"], 4);
}

#[test]
fn exact_inclusive_applicability_preserves_timelines_and_all_time_history() {
    let coordinate = 9_007_199_254_740_993i64;
    let mut source = world(
        json!({"type":"string"}),
        vec![
            proposition(1, json!("Dated")),
            proposition(2, json!("Other timeline")),
        ],
    );
    source = edit(
        &source,
        &id(1),
        "valid",
        json!({"kind":"interval","timeline":"main",
        "start":{"timeline":"main","tick":coordinate,"order":-1},"end":{"timeline":"main","tick":coordinate,"order":0}}),
    );
    source = edit(
        &source,
        &id(2),
        "valid",
        json!({"kind":"interval","timeline":"other"}),
    );
    valid(&source);
    for (order, count) in [(-2, 0), (-1, 1), (0, 1), (1, 0)] {
        let result = list(&source, ANA, Some(&at(coordinate, order)));
        assert_eq!(result["count"], count);
        assert_eq!(result["at"]["tick"], coordinate.to_string());
        if count == 1 {
            assert_eq!(
                result["items"][0]["valid"]["end"]["tick"],
                coordinate.to_string()
            );
        }
    }
    let other = StoryTime::new("other", 0, i32::MAX).unwrap();
    assert_eq!(ids(&list(&source, ANA, Some(&other))), [id(2)]);
    assert_eq!(ids(&list(&source, ANA, None)), [id(1), id(2)]);
    invalid(&edit(
        &source,
        &id(1),
        "valid",
        json!({"kind":"interval","timeline":"main","start":{"timeline":"other","tick":0,"order":0}}),
    ));
    invalid(&edit(
        &source,
        &id(1),
        "valid",
        json!({"kind":"interval","timeline":"main","start":{"timeline":"main","tick":true,"order":0}}),
    ));
}

#[test]
fn scopes_status_and_target_matching_precede_complete_result_limits() {
    let mut source = world(
        json!({"type":"string"}),
        vec![
            proposition(1, json!("Visible")),
            proposition(2, json!("Audience hidden")),
            proposition(3, json!("View hidden")),
            proposition(4, json!("Draft")),
            proposition(5, json!("Other subject")),
        ],
    );
    source = edit(&source, &id(2), "audience", json!(["private"]));
    source = edit(&source, &id(3), "perspectives", json!(["private"]));
    source = edit(&source, &id(4), "status", json!("draft"));
    source = edit(&source, &id(5), "subject", reference(OTHER));
    valid(&source);
    let result = assertion::as_of(&source, ANA, Some(&at(0, 0)), "author", "author", 1).unwrap();
    assert_eq!(ids(&result), [id(1)]);
    assert_eq!(
        assertion::as_of(&source, ANA, None, "private", "author", 1).unwrap()["items"][0]["assertionId"],
        id(2)
    );
    assert_eq!(
        assertion::as_of(&source, ANA, None, "author", "private", 1).unwrap()["items"][0]["assertionId"],
        id(3)
    );
    let exposed = edit(&source, &id(2), "audience", json!(["author"]));
    assert!(assertion::as_of(&exposed, ANA, None, "author", "author", 1).is_err());
    for target in ["missing", &id(2), &id(3), &id(4)] {
        assert_eq!(list(&source, target, None)["count"], 0);
    }
    let by_title = edit(&source, &id(1), "title", json!("Visible title"));
    assert_eq!(list(&by_title, "Visible title", None)["count"], 0);
}

#[test]
fn hidden_subjects_and_ordinary_scope_labels_do_not_leak_proposition_payloads() {
    let source = edit(&base(), ANA, "audience", json!(["secret"]));
    valid(&source);
    assert_eq!(list(&source, ANA, None)["items"], json!([]));
    assert_eq!(list(&source, &id(1), None)["items"], json!([]));
    let source = edit(&base(), ANA, "perspectives", json!(["secret"]));
    assert_eq!(list(&source, ANA, None)["count"], 0);
    // Ordinary owners retain authored status/time semantics; their own optional scopes still apply.
    let retired = edit(&base(), ANA, "status", json!("retired"));
    valid(&retired);
    assert_eq!(list(&retired, ANA, Some(&at(i64::MIN, 0)))["count"], 1);
    let concept_subject = edit(&base(), &id(1), "subject", reference(CONCEPT));
    let hidden_concept = edit(&concept_subject, CONCEPT, "status", json!("retired"));
    valid(&hidden_concept);
    assert_eq!(list(&hidden_concept, CONCEPT, None)["count"], 0);
}

#[test]
fn withheld_typed_values_or_attributors_cannot_suppress_admitted_old_accounts() {
    for attribution in [false, true] {
        let mut source = world(
            json!({"type":"entity","kinds":["character"]}),
            vec![
                proposition(1, reference(ANA)),
                proposition(2, reference(ANA)),
            ],
        );
        source = edit(&source, OTHER, "audience", json!(["secret"]));
        source = edit(&source, &id(2), "replaces", json!(id(1)));
        source = if attribution {
            let source = edit(&source, &id(2), "classification", json!("attributed-claim"));
            edit(&source, &id(2), "attributed_to", reference(OTHER))
        } else {
            edit(&source, &id(2), "value", reference(OTHER))
        };
        valid(&source);
        let result =
            assertion::as_of(&source, ANA, Some(&at(0, 0)), "author", "author", 1).unwrap();
        assert_eq!(ids(&result), [id(1)]);
        assert!(!result.to_string().contains(OTHER));
        assert!(!result.to_string().contains(&id(2)));
        let admitted = edit(&source, OTHER, "audience", json!(["author"]));
        assert_eq!(ids(&list(&admitted, ANA, Some(&at(0, 0)))), [id(2)]);
    }
}

#[test]
fn private_replacements_and_hidden_predecessors_leave_no_correction_authority() {
    let source = edit(
        &world(
            json!({"type":"string"}),
            vec![proposition(1, json!("Old")), proposition(2, json!("New"))],
        ),
        &id(2),
        "replaces",
        json!(id(1)),
    );
    let hidden_new = edit(&source, &id(2), "audience", json!(["secret"]));
    assert_eq!(ids(&list(&hidden_new, ANA, Some(&at(0, 0)))), [id(1)]);
    let hidden_old = edit(&source, &id(1), "audience", json!(["secret"]));
    let result = list(&hidden_old, ANA, Some(&at(0, 0)));
    assert_eq!(ids(&result), [id(2)]);
    assert!(result["items"][0].get("replaces").is_none());
    assert!(!result.to_string().contains(&id(1)));
}

#[test]
fn projected_objects_and_arrays_still_supply_authored_replacement_values() {
    for (spec, value, projected) in [
        (
            json!({"type":"object","required":{},"optional":{"endpoint":{"type":"entity","kinds":["character"]}}}),
            json!({"endpoint":reference(OTHER)}),
            json!({}),
        ),
        (
            json!({"type":"array","items":{"type":"entity","kinds":["character"]},"max_items":8}),
            json!([reference(OTHER)]),
            json!([]),
        ),
    ] {
        let mut source = world(
            spec,
            vec![proposition(1, value.clone()), proposition(2, value)],
        );
        source = edit(&source, &id(2), "replaces", json!(id(1)));
        source = edit(&source, OTHER, "audience", json!(["secret"]));
        valid(&source);
        let result = list(&source, ANA, Some(&at(0, 0)));
        assert_eq!(ids(&result), [id(2)]);
        assert_eq!(result["items"][0]["value"], projected);
        assert_eq!(result["items"][0]["replaces"], id(1));
        assert!(!result.to_string().contains(OTHER));
    }
}

#[test]
fn typed_projection_preserves_original_indexes_and_keeps_ref_shaped_literals() {
    let value_type = json!({"type":"object","required":{
        "links":{"type":"array","items":{"type":"entity","kinds":["character","lore-concept"]},"max_items":8},
        "literal":{"type":"object","required":{"kind":{"type":"string"},"id":{"type":"string"}},"optional":{}}},"optional":{}});
    let value = json!({"links":[reference(OTHER),reference(ANA),reference(CONCEPT)],"literal":{"kind":"record","id":"not-a-live-record"}});
    let mut source = world(value_type, vec![proposition(1, value)]);
    source = edit(&source, OTHER, "audience", json!(["secret"]));
    source = edit(&source, CONCEPT, "status", json!("retired"));
    valid(&source);
    let result = list(&source, ANA, None);
    let item = &result["items"][0];
    assert_eq!(item["value"]["links"], json!([reference(ANA)]));
    assert_eq!(item["value"]["literal"]["id"], "not-a-live-record");
    assert_eq!(
        item["valueType"]["fields"]["links"]["items"],
        json!([{"type":"entity"}])
    );
    assert_eq!(item["references"].as_array().unwrap().len(), 2);
    assert_eq!(item["references"][1]["sourceField"], "value.links[1].id");
    assert_eq!(
        item["references"][1]["citation"]["section"],
        "value.links[1].id"
    );
    assert!(!result.to_string().contains(CONCEPT));
    assert!(!result.to_string().contains(OTHER));
}

#[test]
fn exact_numbers_enum_scalar_types_and_null_empty_values_survive_projection() {
    let mut number = world(json!({"type":"number"}), vec![proposition(1, json!(0))]);
    number = edit_yaml(&number, &id(1), "value", yaml("18446744073709551615"));
    valid(&number);
    assert_eq!(
        list(&number, ANA, None)["items"][0]["value"],
        "18446744073709551615"
    );
    let float = edit_yaml(&number, &id(1), "value", yaml("1.0"));
    assert_eq!(
        list(&float, ANA, None)["items"][0]["value"].as_f64(),
        Some(1.0)
    );
    let enum_world = world(
        json!({"type":"enum","values":[1,"1",true,null]}),
        vec![
            proposition(1, json!(1)),
            proposition(2, json!("1")),
            proposition(3, json!(true)),
            proposition(4, json!(null)),
        ],
    );
    valid(&enum_world);
    let result = list(&enum_world, ANA, None);
    assert_eq!(result["items"][0]["value"], result["items"][1]["value"]);
    assert_eq!(result["items"][0]["valueType"]["scalarType"], "number");
    assert_eq!(result["items"][1]["valueType"]["scalarType"], "string");
    assert_eq!(
        assertion::predicates(&enum_world, 2).unwrap()["items"][0]["value_type"]["enumValueTypes"],
        json!(["number", "string", "boolean", "null"])
    );
    let null = world(json!({"type":"null"}), vec![proposition(1, json!(null))]);
    valid(&null);
    assert!(list(&null, ANA, None)["items"][0]["value"].is_null());
    let empty = world(
        json!({"type":"array","items":{"type":"string"},"max_items":8}),
        vec![proposition(1, json!([]))],
    );
    valid(&empty);
    assert_eq!(list(&empty, ANA, None)["items"][0]["value"], json!([]));
}

#[test]
fn current_predicate_edits_validate_historical_and_noncanonical_dependents() {
    let mut source = base();
    source = edit(&source, &id(1), "status", json!("retired"));
    source = edit(
        &source,
        &id(1),
        "valid",
        json!({"kind":"interval","timeline":"main","end":{"timeline":"main","tick":-1,"order":0}}),
    );
    valid(&source);
    let changed = edit(
        &source,
        WORLD,
        "predicates",
        json!([{"id":"has-account","subject_kinds":["character"],"value_type":{"type":"number"}}]),
    );
    invalid(&changed);
    invalid(&edit(&source, WORLD, "predicates", json!([])));
}

#[test]
fn queries_are_pure_and_canonical_edits_do_not_teach_or_change_character_evidence() {
    let knowledge = record(
        "knowledge",
        "know_00000000000000000000000001",
        json!({"knower":ANA,
        "claim":{"key":"remembered-account","statement":"I learned an independent account."},
        "transitions":[{"id":"first-learning","time":{"timeline":"main","tick":0,"order":0},"state":"accepted","confidence":0.75,"acquisition":"heard"}]}),
    );
    let source = add(&base(), knowledge);
    valid(&source);
    let raw = source
        .records
        .iter()
        .map(|(id, r)| (id.clone(), r.raw_bytes.clone()))
        .collect::<BTreeMap<_, _>>();
    let before = character::context(&source, ANA, &at(1, 0), 100_000).unwrap();
    let search = character::search(&source, ANA, "account", &at(1, 0), 100).unwrap();
    let budget = character::context(&source, ANA, &at(1, 0), 1)
        .unwrap_err()
        .to_string();
    list(&source, ANA, Some(&at(1, 0)));
    assertion::predicates(&source, 2).unwrap();
    assert_eq!(
        raw,
        source
            .records
            .iter()
            .map(|(id, r)| (id.clone(), r.raw_bytes.clone()))
            .collect()
    );
    let mut changed = edit(
        &source,
        &id(1),
        "value",
        json!("A different canonical secret account"),
    );
    changed = edit(
        &changed,
        &id(1),
        "title",
        json!("Private proposition title"),
    );
    changed = edit(
        &changed,
        &id(1),
        "classification",
        json!("attributed-claim"),
    );
    changed = edit(&changed, &id(1), "attributed_to", reference(CONCEPT));
    changed = edit(
        &changed,
        &id(1),
        "evidence",
        json!([historical(reference("old-secret"))]),
    );
    valid(&changed);
    assert_eq!(
        character::context(&changed, ANA, &at(1, 0), 100_000).unwrap(),
        before
    );
    assert_eq!(
        character::search(&changed, ANA, "account", &at(1, 0), 100).unwrap(),
        search
    );
    assert_eq!(
        character::context(&changed, ANA, &at(1, 0), 1)
            .unwrap_err()
            .to_string(),
        budget
    );
    let learned = edit(
        &changed,
        "know_00000000000000000000000001",
        "claim",
        json!({"key":"learned","statement":"A separately learned assertion","assertion_id":id(1)}),
    );
    valid(&learned);
    let learned_items =
        wedl::learned_assertions::list(&learned, ANA, &id(1), &at(1, 0), 100).unwrap();
    assert_eq!(learned_items["count"], 1);
    assert_eq!(
        learned_items["items"][0]["statement"],
        "A separately learned assertion"
    );
    assert!(learned_items["items"][0].get("value").is_none());
}

#[test]
fn query_bounds_and_scope_arguments_fail_without_partial_results() {
    let source = base();
    for limit in [0, 1001] {
        assert!(matches!(
            assertion::as_of(&source, ANA, None, "author", "author", limit),
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            assertion::predicates(&source, limit),
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(assertion::predicates(&source, 1).is_err());
    for (audience, view) in [("", "author"), ("author", "bad\nlabel")] {
        assert!(assertion::as_of(&source, ANA, None, audience, view, 100).is_err());
    }
    assert!(assertion::as_of(&source, " ", None, "author", "author", 100).is_err());
    assert!(
        assertion::as_of(
            &source,
            ANA,
            Some(&StoryTime::new("undeclared", 0, 0).unwrap()),
            "author",
            "author",
            100
        )
        .is_err()
    );
}
