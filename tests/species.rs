//! Explicit authored common/individual statements, never synthesized traits.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeMap, path::PathBuf};
use wedl::{
    Error, character,
    model::{Record, StoryTime, World},
    source,
    species::{self, Selection},
    validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const PERSON: &str = "char_00000000000000000000000001";
fn id(n: usize) -> String {
    format!("concept_{n:026}")
}
fn yaml(value: &str) -> Yaml {
    serde_yaml::from_str(value).unwrap()
}
fn reference(n: usize) -> Json {
    json!({"kind":"record","id":id(n)})
}
fn record(kind: &str, id: &str, fields: Yaml) -> Record {
    let mut data = yaml(&format!(
        "schema: wedl/v1\nkind: {kind}\nid: {id}\ntitle: {id}\n"
    ));
    data.as_mapping_mut()
        .unwrap()
        .extend(fields.as_mapping().unwrap().clone());
    let body = format!(
        "---\n{}---\nUndated authored prose for {id}.\n",
        serde_yaml::to_string(&data).unwrap()
    );
    source::parse_record(body.as_bytes(), &format!("story/{id}.md")).unwrap()
}
fn concept(n: usize, profile: &str, values: Json) -> Record {
    record(
        "lore-concept",
        &id(n),
        serde_yaml::to_value(json!({
            "profile":profile,"domain":"authored.description","values":values,
            "valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]
        }))
        .unwrap(),
    )
}
fn world(records: Vec<Record>) -> World {
    let mut fields = yaml(
        "type: object\nrequired:\n  traits: {type: string}\noptional:\n  common: {type: entity, kinds: [lore-concept]}\n  parent: {type: entity, kinds: [lore-concept]}\n  parents: {type: array, items: {type: entity, kinds: [lore-concept, character]}, max_items: 1024}\n  individual: {type: entity, kinds: [character]}\n  taxonomy: {type: string}\n  life_stage: {type: 'null'}\n  habitat: {type: array, items: {type: string}, max_items: 8}\n  culture: {type: string}\n  language: {type: string}\n  religion: {type: string}\n  number: {type: number}\n  choice: {type: enum, values: [1, '1']}\n  opaque: {type: object, required: {kind: {type: string}, id: {type: string}}, optional: {}}\n",
    );
    fields["optional"]["opaque_list"] = yaml(
        "{type: array, items: {type: object, required: {kind: {type: string}, id: {type: string}}, optional: {}}, max_items: 8}",
    );
    let profiles = [
        "common-description",
        "individual-variation",
        "other-profile",
    ]
    .iter()
    .map(|profile| json!({"id":profile,"label":profile,"fields":fields}))
    .collect::<Vec<_>>();
    let mut all = vec![
        record(
            "world",
            WORLD,
            serde_yaml::to_value(json!({
                "capabilities":["lore-core-v1"],"default_timeline":"main",
                "timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],
                "state_keys":{},"profiles":profiles
            }))
            .unwrap(),
        ),
        record("character", PERSON, yaml("initial_state: {}\n")),
    ];
    all.extend(records);
    source::world_from_records(PathBuf::new(), all, None).unwrap()
}
fn base() -> World {
    world(vec![
        concept(
            1,
            "common-description",
            json!({"traits":"Common statement","taxonomy":"An authored taxonomy","life_stage":null,"habitat":[],"culture":"Separate common culture","parents":[],"number":9007199254740993u64,"choice":1}),
        ),
        concept(
            2,
            "individual-variation",
            json!({"traits":"Own variation","common":reference(1),"individual":{"kind":"record","id":PERSON},"language":"Separate individual language","choice":"1"}),
        ),
    ])
}
fn selection() -> Selection {
    Selection {
        common_profile: "common-description".into(),
        individual_profile: "individual-variation".into(),
        individual_link: "common".into(),
        common_link: None,
        common_fields: BTreeMap::from([
            ("traits".into(), "traits".into()),
            ("taxonomy".into(), "taxonomy".into()),
            ("life_stage".into(), "life_stage".into()),
            ("habitat".into(), "habitat".into()),
            ("culture".into(), "culture".into()),
        ]),
        individual_fields: BTreeMap::from([
            ("traits".into(), "traits".into()),
            ("language".into(), "language".into()),
            ("habitat".into(), "habitat".into()),
        ]),
    }
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn compose(world: &World, selected: &Selection) -> Json {
    species::compose(world, &id(2), selected, Some(&at(5, 0)), "author", "author").unwrap()
}
fn edit(world: &World, id: &str, field: &str, value: Json) -> World {
    let before = &world.records[id];
    let mut fields = before.frontmatter.clone();
    fields[field] = serde_yaml::to_value(value).unwrap();
    let replacement = record(&before.kind, id, fields);
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
fn common_ids(result: &Json) -> Vec<String> {
    result["common"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["conceptId"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn common_and_individual_roles_are_separate_exact_cited_statements() {
    let world = base();
    assert!(validation::validate_world(&world).valid);
    let result = compose(&world, &selection());
    assert_eq!(
        result["common"][0]["fields"]["traits"]["value"],
        "Common statement"
    );
    assert_eq!(
        result["individual"]["fields"]["traits"]["value"],
        "Own variation"
    );
    assert_eq!(result["common"][0]["fields"].as_object().unwrap().len(), 7);
    assert_eq!(result["individual"]["fields"].as_object().unwrap().len(), 7);
    let common = &result["common"][0]["fields"];
    assert_eq!(common["life_stage"]["known"], true);
    assert!(common["life_stage"]["value"].is_null());
    assert_eq!(common["habitat"]["value"], json!([]));
    assert_eq!(common["religion"]["reason"], "unselected");
    assert!(common["religion"].get("value").is_none());
    assert_eq!(
        result["individual"]["fields"]["habitat"]["reason"],
        "not-supplied-or-visible"
    );
    assert_eq!(common["culture"]["citation"]["section"], "values.culture");
    assert_eq!(common["culture"]["citation"]["entityId"], id(1));
    assert_eq!(result["individual"]["bodyKind"], "undated-authored-prose");
    assert_eq!(result["linkageComplete"], true);
    assert_eq!(result["unknowns"], json!([]));
    assert!(result.get("traits").is_none());
}

#[test]
fn independent_field_selections_keep_integer_and_enum_scalar_identity() {
    let mut selected = selection();
    selected
        .common_fields
        .insert("traits".into(), "number".into());
    selected
        .common_fields
        .insert("language".into(), "choice".into());
    selected
        .individual_fields
        .insert("traits".into(), "choice".into());
    let result = compose(&base(), &selected);
    assert_eq!(
        result["common"][0]["fields"]["traits"]["value"],
        "9007199254740993"
    );
    assert_eq!(
        result["common"][0]["fields"]["traits"]["valueType"]["type"],
        "number"
    );
    assert_eq!(result["common"][0]["fields"]["language"]["value"], "1");
    assert_eq!(
        result["common"][0]["fields"]["language"]["valueType"]["scalarType"],
        "number"
    );
    assert_eq!(result["individual"]["fields"]["traits"]["value"], "1");
    assert_eq!(
        result["individual"]["fields"]["traits"]["valueType"]["scalarType"],
        "string"
    );
}

#[test]
fn literal_strings_objects_and_concept_links_never_supply_common_ancestry() {
    let mut selected = selection();
    let mut world = edit(
        &base(),
        &id(2),
        "values",
        json!({"traits":id(1),"opaque":reference(1)}),
    );
    world = edit(
        &world,
        &id(2),
        "concept_links",
        json!([{"relation":"common","concept_id":id(1),"valid":{"kind":"timeless"}}]),
    );
    for field in ["traits", "opaque", "common"] {
        selected.individual_link = field.into();
        let result = compose(&world, &selected);
        assert_eq!(result["common"], json!([]));
        assert_eq!(result["linkageComplete"], false);
        assert_eq!(
            result["unknowns"][0]["sourceField"],
            format!("values.{field}")
        );
        assert_eq!(
            result["unknowns"][0]["reason"],
            if field == "common" {
                "absent-or-unavailable"
            } else {
                "not-typed-reference"
            }
        );
    }
}

#[test]
fn empty_arrays_require_a_declared_entity_element_type_to_supply_linkage() {
    let mut world = edit(
        &base(),
        &id(2),
        "values",
        json!({"traits":"Individual","habitat":[],"opaque_list":[],"parents":[]}),
    );
    let mut selected = selection();
    for field in ["habitat", "opaque_list"] {
        selected.individual_link = field.into();
        let result = compose(&world, &selected);
        assert_eq!(result["common"], json!([]));
        assert_eq!(result["linkageComplete"], false);
        assert_eq!(result["unknowns"][0]["reason"], "not-typed-reference");
    }
    selected.individual_link = "parents".into();
    let authored_empty = compose(&world, &selected);
    assert_eq!(authored_empty["linkageComplete"], true);
    assert_eq!(authored_empty["unknowns"], json!([]));
    world = edit(
        &world,
        &id(2),
        "values",
        json!({"traits":"Individual","parents":[reference(1)]}),
    );
    world = edit(&world, &id(1), "status", json!("retired"));
    let filtered_empty = compose(&world, &selected);
    assert_eq!(filtered_empty["common"], authored_empty["common"]);
    assert_eq!(
        filtered_empty["linkageComplete"],
        authored_empty["linkageComplete"]
    );
    assert_eq!(filtered_empty["unknowns"], authored_empty["unknowns"]);
}

#[test]
fn traversal_preserves_authored_order_and_shared_ancestors_once() {
    let world = world(vec![
        concept(
            1,
            "common-description",
            json!({"traits":"First","parents":[reference(3)]}),
        ),
        concept(
            2,
            "individual-variation",
            json!({"traits":"Individual","parents":[reference(1),reference(4),reference(1)]}),
        ),
        concept(
            3,
            "common-description",
            json!({"traits":"Shared","parents":[]}),
        ),
        concept(
            4,
            "common-description",
            json!({"traits":"Second","parents":[reference(3)]}),
        ),
    ]);
    let mut selected = selection();
    selected.individual_link = "parents".into();
    selected.common_link = Some("parents".into());
    let result = compose(&world, &selected);
    assert_eq!(common_ids(&result), vec![id(1), id(3), id(4)]);
    assert_eq!(result["linkageComplete"], true);
    let affected =
        species::affected(&world, &id(3), &selected, None, "author", "author", 1).unwrap();
    assert_eq!(affected["count"], 1);
    assert_eq!(affected["items"][0]["conceptId"], id(2));
}

#[test]
fn hidden_array_targets_are_filtered_before_traversal_and_original_citations_survive() {
    let mut world = world(vec![
        concept(1, "common-description", json!({"traits":"Visible"})),
        concept(
            2,
            "individual-variation",
            json!({"traits":"Individual","parents":[reference(3),reference(1)]}),
        ),
        concept(
            3,
            "common-description",
            json!({"traits":"Secret","parent":reference(3)}),
        ),
    ]);
    world = edit(&world, &id(3), "audience", json!(["hidden"]));
    let mut selected = selection();
    selected.individual_link = "parents".into();
    selected
        .individual_fields
        .insert("habitat".into(), "parents".into());
    let result = compose(&world, &selected);
    assert_eq!(common_ids(&result), vec![id(1)]);
    assert_eq!(result["linkageComplete"], true);
    assert_eq!(result["unknowns"], json!([]));
    let field = &result["individual"]["fields"]["habitat"];
    assert_eq!(field["value"], json!([reference(1)]));
    assert_eq!(
        field["references"][0]["sourceField"],
        "values.parents[1].id"
    );
    assert!(!result.to_string().contains("Secret"));
    // A hidden cycle does not participate in traversal.
    assert!(!result.to_string().contains(&id(3)));
}

#[test]
fn missing_hidden_draft_retired_wrong_profile_and_alias_selectors_are_unavailable() {
    let selected = selection();
    let missing = species::compose(&base(), &id(999), &selected, None, "author", "author")
        .unwrap_err()
        .json();
    for (field, value) in [
        ("audience", json!(["hidden"])),
        ("perspectives", json!(["hidden"])),
        ("status", json!("draft")),
        ("status", json!("retired")),
        ("profile", json!("other-profile")),
    ] {
        let world = edit(&base(), &id(2), field, value);
        assert_eq!(
            species::compose(&world, &id(2), &selected, None, "author", "author")
                .unwrap_err()
                .json(),
            missing
        );
    }
    let world = edit(&base(), &id(2), "aliases", json!(["Known variation"]));
    assert_eq!(
        species::compose(
            &world,
            "Known variation",
            &selected,
            None,
            "author",
            "author"
        )
        .unwrap_err()
        .json(),
        missing
    );
    let unavailable = species::affected(&world, &id(999), &selected, None, "author", "author", 100)
        .unwrap_err()
        .json();
    assert_eq!(unavailable, missing);
}

#[test]
fn horizons_and_scope_apply_before_confirmed_paths_and_affected_limits() {
    let mut world = world(vec![
        concept(1, "common-description", json!({"traits":"Common"})),
        concept(
            2,
            "individual-variation",
            json!({"traits":"Current","common":reference(1)}),
        ),
        concept(
            3,
            "individual-variation",
            json!({"traits":"Future","common":reference(1)}),
        ),
        concept(
            4,
            "individual-variation",
            json!({"traits":"Hidden","common":reference(1)}),
        ),
    ]);
    world = edit(
        &world,
        &id(2),
        "valid",
        json!({"kind":"interval","timeline":"main","start":{"timeline":"main","tick":1,"order":1},"end":{"timeline":"main","tick":5,"order":0}}),
    );
    world = edit(
        &world,
        &id(3),
        "valid",
        json!({"kind":"interval","timeline":"other","start":{"timeline":"other","tick":1,"order":0}}),
    );
    world = edit(&world, &id(4), "audience", json!(["hidden"]));
    let selected = selection();
    assert_eq!(
        species::affected(
            &world,
            &id(1),
            &selected,
            Some(&at(5, 0)),
            "author",
            "author",
            1
        )
        .unwrap()["count"],
        1
    );
    assert_eq!(
        species::affected(
            &world,
            &id(1),
            &selected,
            Some(&at(5, 1)),
            "author",
            "author",
            1
        )
        .unwrap()["count"],
        0
    );
    assert_eq!(
        species::affected(
            &world,
            &id(1),
            &selected,
            Some(&StoryTime::new("other", 5, 0).unwrap()),
            "author",
            "author",
            1
        )
        .unwrap()["items"][0]["conceptId"],
        id(3)
    );
    assert!(species::affected(&world, &id(1), &selected, None, "author", "author", 1).is_err());
    assert_eq!(
        species::affected(&world, &id(1), &selected, None, "author", "author", 2).unwrap()["count"],
        2
    );
}

#[test]
fn confirmed_affected_paths_retain_unknown_branches_without_filling_values() {
    let world = world(vec![
        concept(1, "common-description", json!({"traits":"Known common"})),
        concept(
            2,
            "individual-variation",
            json!({"traits":"Individual","parents":[reference(1),reference(3)]}),
        ),
        concept(3, "other-profile", json!({"traits":"Wrong role"})),
    ]);
    let mut selected = selection();
    selected.individual_link = "parents".into();
    selected.common_link = Some("parent".into());
    let result = compose(&world, &selected);
    assert_eq!(common_ids(&result), vec![id(1)]);
    assert_eq!(result["unknowns"][0]["reason"], "absent-or-unavailable");
    assert_eq!(
        result["unknowns"][1]["reason"],
        "target-profile-unavailable"
    );
    assert_eq!(result["unknowns"][1]["sourceField"], "values.parents[1].id");
    let affected =
        species::affected(&world, &id(1), &selected, None, "author", "author", 1).unwrap();
    assert_eq!(affected["count"], 1);
    assert_eq!(affected["items"][0]["linkageComplete"], false);
    assert_eq!(affected["items"][0]["unknowns"], result["unknowns"]);
}

#[test]
fn cycles_refuse_complete_composition_and_affected_results() {
    let world = world(vec![
        concept(
            1,
            "common-description",
            json!({"traits":"First","parent":reference(3)}),
        ),
        concept(
            2,
            "individual-variation",
            json!({"traits":"Individual","common":reference(1)}),
        ),
        concept(
            3,
            "common-description",
            json!({"traits":"Second","parent":reference(1)}),
        ),
    ]);
    assert!(validation::validate_world(&world).valid);
    let mut selected = selection();
    selected.common_link = Some("parent".into());
    for result in [
        species::compose(&world, &id(2), &selected, None, "author", "author"),
        species::affected(&world, &id(1), &selected, None, "author", "author", 100),
    ] {
        assert!(matches!(result, Err(Error::InvalidInput(_))));
        assert!(result.unwrap_err().to_string().contains("cycle"));
    }
}

#[test]
fn explicit_depth_boundary_is_inclusive_and_exhaustion_is_an_error() {
    for length in [16, 17] {
        let mut records = vec![concept(
            2,
            "individual-variation",
            json!({"traits":"Individual","common":reference(100)}),
        )];
        for index in 0..length {
            records.push(concept(100+index,"common-description",json!({"traits":"Common","parents":if index+1<length{vec![reference(101+index)]}else{vec![]}})));
        }
        let world = world(records);
        let mut selected = selection();
        selected.common_link = Some("parents".into());
        let result = species::compose(&world, &id(2), &selected, None, "author", "author");
        if length == 16 {
            assert_eq!(result.unwrap()["common"].as_array().unwrap().len(), 16);
        } else {
            assert!(result.unwrap_err().to_string().contains("depth 16"));
        }
    }
}

#[test]
fn record_bound_counts_the_individual_and_deduplicates_repeated_links() {
    for common_count in [63, 64] {
        let mut targets = (100..100 + common_count).map(reference).collect::<Vec<_>>();
        targets.push(reference(100));
        let mut records = vec![concept(
            2,
            "individual-variation",
            json!({"traits":"Individual","parents":targets}),
        )];
        for index in 100..100 + common_count {
            records.push(concept(
                index,
                "common-description",
                json!({"traits":"Common"}),
            ));
        }
        let world = world(records);
        let mut selected = selection();
        selected.individual_link = "parents".into();
        let result = species::compose(&world, &id(2), &selected, None, "author", "author");
        if common_count == 63 {
            assert_eq!(result.unwrap()["common"].as_array().unwrap().len(), 63);
        } else {
            assert!(result.unwrap_err().to_string().contains("64 records"));
        }
    }
}

#[test]
fn unrelated_concepts_do_not_consume_a_catalogue_or_affected_result_budget() {
    let mut records = vec![
        concept(1, "common-description", json!({"traits":"Common"})),
        concept(
            2,
            "individual-variation",
            json!({"traits":"Individual","common":reference(1)}),
        ),
    ];
    for n in 100..1101 {
        records.push(concept(n, "other-profile", json!({"traits":"Unrelated"})));
    }
    let world = world(records);
    let selected = selection();
    assert_eq!(common_ids(&compose(&world, &selected)), vec![id(1)]);
    assert_eq!(
        species::affected(&world, &id(1), &selected, None, "author", "author", 1).unwrap()["count"],
        1
    );
}

#[test]
fn one_profile_can_have_both_explicit_roles_and_selection_input_is_closed() {
    let world = world(vec![
        concept(
            1,
            "common-description",
            json!({"traits":"Common","parents":[]}),
        ),
        concept(
            2,
            "common-description",
            json!({"traits":"Individual","common":reference(1)}),
        ),
    ]);
    let mut selected = selection();
    selected.individual_profile = "common-description".into();
    assert_eq!(common_ids(&compose(&world, &selected)), vec![id(1)]);
    assert_eq!(
        species::affected(&world, &id(1), &selected, None, "author", "author", 1).unwrap()["count"],
        1
    );
    for (field, value) in [
        ("common_profile", json!("Bad token")),
        ("individual_link", json!("values.common")),
        ("common_link", json!("")),
        ("common_fields", json!({"anatomy":"traits"})),
        ("individual_fields", json!({"traits":"bad-field"})),
    ] {
        let mut data = serde_json::to_value(selection()).unwrap();
        data[field] = value;
        assert!(
            serde_json::from_value::<Selection>(data)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    let mut data = serde_json::to_value(selection()).unwrap();
    data["infer_traits"] = json!(true);
    assert!(serde_json::from_value::<Selection>(data).is_err());
    let mut selected = selection();
    selected.common_profile = "undeclared".into();
    assert!(species::compose(&world, &id(2), &selected, None, "author", "author").is_err());
    for limit in [0, 1001] {
        assert!(
            species::affected(
                &world,
                &id(1),
                &selection(),
                None,
                "author",
                "author",
                limit
            )
            .is_err()
        );
    }
}

#[test]
fn author_compositions_never_change_source_or_generic_character_evidence() {
    let world = base();
    let selected = selection();
    let raw = world
        .records
        .iter()
        .map(|(id, r)| (id.clone(), r.raw_bytes.clone()))
        .collect::<BTreeMap<_, _>>();
    let context = character::context(&world, PERSON, &at(5, 0), 100_000).unwrap();
    let search = character::search(&world, PERSON, "Common statement", &at(5, 0), 100).unwrap();
    compose(&world, &selected);
    species::affected(&world, &id(1), &selected, None, "author", "author", 100).unwrap();
    assert_eq!(
        world
            .records
            .iter()
            .map(|(id, r)| (id.clone(), r.raw_bytes.clone()))
            .collect::<BTreeMap<_, _>>(),
        raw
    );
    assert_eq!(
        character::context(&world, PERSON, &at(5, 0), 100_000).unwrap(),
        context
    );
    assert_eq!(
        character::search(&world, PERSON, "Common statement", &at(5, 0), 100).unwrap(),
        search
    );
    assert!(!context.to_string().contains("Common statement"));
}
