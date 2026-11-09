//! Authored metaphysical descriptions stay explicit, scoped and non-inferential.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeMap, path::PathBuf};
use wedl::{
    Error, character,
    magic::{self, Selection},
    model::{Record, StoryTime, World},
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const PERSON: &str = "char_00000000000000000000000001";
const ROLES: [&str; 4] = ["tradition", "practice", "realm", "artifact"];
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
    let raw = format!(
        "---\n{}---\nThe authored account for {id}.\r\nA vow is a narrative restriction.\rUnresolved cause.\nTrailing  \r\n",
        serde_yaml::to_string(&data).unwrap()
    );
    source::parse_record(raw.as_bytes(), &format!("story/{id}.md")).unwrap()
}
fn concept(n: usize, role: &str, values: Json) -> Record {
    record(
        "lore-concept",
        &id(n),
        serde_yaml::to_value(json!({
            "profile":format!("magic-{role}"),"domain":"authored.magic","values":values,
            "valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]
        }))
        .unwrap(),
    )
}
fn world(records: Vec<Record>) -> World {
    let fields = yaml(
        "type: object\nrequired: {}\noptional:\n  links: {type: array, items: {type: entity, kinds: [lore-concept, character]}, max_items: 1024}\n  more_links: {type: array, items: {type: entity, kinds: [lore-concept]}, max_items: 1024}\n  link: {type: entity, kinds: [lore-concept]}\n  restrictions: {type: string}\n  exceptions: {type: array, items: {type: string}, max_items: 8}\n  uncertain: {type: 'null'}\n  number: {type: number}\n  choice: {type: enum, values: [1, '1']}\n  opaque: {type: object, required: {kind: {type: string}, id: {type: string}}, optional: {}}\n  opaque_list: {type: array, items: {type: object, required: {kind: {type: string}, id: {type: string}}, optional: {}}, max_items: 8}\n",
    );
    let profiles = ["tradition", "practice", "realm", "artifact", "other"]
        .iter()
        .map(|role| {
            json!({"id":format!("magic-{role}"),"label":format!("Authored {role}"),"fields":fields})
        })
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
            "tradition",
            json!({"links":[],"restrictions":"Tradition statement","exceptions":[],"number":9007199254740993u64,"choice":1}),
        ),
        concept(
            2,
            "practice",
            json!({"links":[reference(1),reference(3),reference(4)],"restrictions":"Practice vow","exceptions":["Independently authored festival exception"],"choice":"1"}),
        ),
        concept(
            3,
            "realm",
            json!({"links":[],"exceptions":[],"uncertain":null}),
        ),
        concept(
            4,
            "artifact",
            json!({"links":[],"restrictions":"Artifact statement","exceptions":[]}),
        ),
    ])
}
fn selection() -> Selection {
    Selection {
        profiles: ROLES
            .iter()
            .map(|role| (role.to_string(), format!("magic-{role}")))
            .collect(),
        links: ROLES
            .iter()
            .map(|role| (role.to_string(), vec!["links".into()]))
            .collect(),
        restrictions: ROLES
            .iter()
            .map(|role| (role.to_string(), "restrictions".into()))
            .collect(),
        exceptions: ROLES
            .iter()
            .map(|role| (role.to_string(), "exceptions".into()))
            .collect(),
    }
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn compose(world: &World, selected: &Selection) -> Json {
    magic::compose(world, &id(2), selected, Some(&at(5, 0)), "author", "author").unwrap()
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
fn record_ids(result: &Json) -> Vec<String> {
    result["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| record["conceptId"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn four_explicit_roles_keep_restrictions_exceptions_prose_and_citations_separate() {
    let world = edit(
        &base(),
        &id(1),
        "provenance",
        json!("An authored tradition account"),
    );
    assert!(validation::validate_world(&world).valid);
    let result = compose(&world, &selection());
    assert_eq!(record_ids(&result), vec![id(2), id(1), id(3), id(4)]);
    assert_eq!(result["protocol"], "wedl-rust/v1");
    assert_eq!(result["perspective"], "author");
    assert_eq!(result["scope"], "author-as-of-lore");
    assert_eq!(result["rootId"], id(2));
    let records = result["records"].as_array().unwrap();
    assert_eq!(
        records
            .iter()
            .map(|r| r["role"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["practice", "tradition", "realm", "artifact"]
    );
    assert_eq!(
        records[0]["fields"]["restrictions"]["value"],
        "Practice vow"
    );
    assert_eq!(
        records[1]["fields"]["restrictions"]["value"],
        "Tradition statement"
    );
    assert_eq!(
        records[0]["fields"]["exceptions"]["value"],
        json!(["Independently authored festival exception"])
    );
    assert_eq!(records[1]["fields"]["exceptions"]["value"], json!([]));
    assert_eq!(
        records[2]["fields"]["restrictions"]["reason"],
        "not-supplied-or-visible"
    );
    assert!(records[2]["fields"]["restrictions"].get("value").is_none());
    assert_eq!(records[1]["provenance"], "An authored tradition account");
    for item in records {
        let record = &world.records[item["conceptId"].as_str().unwrap()];
        assert_eq!(item["body"], record.body);
        assert_eq!(item["bodyKind"], "undated-authored-prose");
        assert_eq!(item["citation"]["entityId"], record.id);
        assert_eq!(item["citation"]["sourcePath"], record.source_path);
        assert_eq!(item["citation"]["blobOid"], json!(record.blob_oid));
        assert_eq!(item["citation"]["fingerprint"], world.fingerprint);
    }
    assert_eq!(
        records[0]["fields"]["restrictions"]["citation"]["section"],
        "values.restrictions"
    );
    assert_eq!(result["links"][0]["ownerId"], id(2));
    assert_eq!(result["links"][0]["entityId"], id(1));
    assert_eq!(
        result["links"][0]["citation"]["section"],
        "values.links[0].id"
    );
    assert_eq!(result["links"][0]["targetCitation"]["entityId"], id(1));
    assert_eq!(result["linkageComplete"], true);
    assert_eq!(result["unknowns"], json!([]));
    assert!(result.get("restrictions").is_none());
}

#[test]
fn independent_field_selections_distinguish_missing_null_empty_and_exact_scalar_identity() {
    let mut selected = selection();
    selected
        .restrictions
        .insert("tradition".into(), "number".into());
    selected
        .exceptions
        .insert("tradition".into(), "choice".into());
    selected
        .restrictions
        .insert("practice".into(), "choice".into());
    selected
        .restrictions
        .insert("realm".into(), "uncertain".into());
    selected.restrictions.remove("artifact");
    let result = compose(&base(), &selected);
    let records = &result["records"];
    assert_eq!(
        records[1]["fields"]["restrictions"]["value"],
        "9007199254740993"
    );
    assert_eq!(
        records[1]["fields"]["restrictions"]["valueType"]["type"],
        "number"
    );
    assert_eq!(records[1]["fields"]["exceptions"]["value"], "1");
    assert_eq!(
        records[1]["fields"]["exceptions"]["valueType"]["scalarType"],
        "number"
    );
    assert_eq!(records[0]["fields"]["restrictions"]["value"], "1");
    assert_eq!(
        records[0]["fields"]["restrictions"]["valueType"]["scalarType"],
        "string"
    );
    assert_eq!(records[2]["fields"]["restrictions"]["known"], true);
    assert!(records[2]["fields"]["restrictions"]["value"].is_null());
    assert_eq!(records[2]["fields"]["exceptions"]["value"], json!([]));
    assert_eq!(
        records[3]["fields"]["restrictions"],
        json!({"known":false,"field":null,"reason":"unselected"})
    );
    // Missing presentation fields do not claim that the selected linkage is unknown.
    assert_eq!(result["linkageComplete"], true);
}

#[test]
fn caller_assigned_roles_do_not_come_from_profile_names_titles_or_prose() {
    let mut selected = selection();
    selected
        .profiles
        .insert("tradition".into(), "magic-practice".into());
    selected
        .profiles
        .insert("practice".into(), "magic-tradition".into());
    let result = compose(&base(), &selected);
    assert_eq!(result["records"][0]["role"], "tradition");
    assert_eq!(result["records"][1]["role"], "practice");
}

#[test]
fn literal_strings_ref_shaped_objects_and_concept_links_never_supply_edges() {
    let mut world = edit(
        &base(),
        &id(2),
        "values",
        json!({"restrictions":id(1),"opaque":reference(1),"opaque_list":[reference(1)]}),
    );
    world = edit(
        &world,
        &id(2),
        "concept_links",
        json!([{"relation":"tradition","concept_id":id(1),"valid":{"kind":"timeless"}}]),
    );
    let mut selected = selection();
    for field in ["restrictions", "opaque", "opaque_list", "link"] {
        selected.links.insert("practice".into(), vec![field.into()]);
        selected
            .restrictions
            .insert("practice".into(), field.into());
        let result = compose(&world, &selected);
        assert_eq!(record_ids(&result), vec![id(2)]);
        assert_eq!(result["links"], json!([]));
        assert_eq!(result["linkageComplete"], false);
        assert_eq!(
            result["unknowns"][0]["sourceField"],
            format!("values.{field}")
        );
        assert_eq!(
            result["unknowns"][0]["reason"],
            if field == "link" {
                "absent-or-unavailable"
            } else {
                "not-typed-reference"
            }
        );
        if field == "opaque" {
            assert_eq!(
                result["records"][0]["fields"]["restrictions"]["value"],
                reference(1)
            );
            assert_eq!(
                result["records"][0]["fields"]["restrictions"]["references"],
                json!([])
            );
        }
    }
}

#[test]
fn empty_projected_arrays_require_declared_entity_element_types() {
    let mut selected = selection();
    let world = edit(
        &base(),
        &id(2),
        "values",
        json!({"links":[],"exceptions":[],"opaque_list":[]}),
    );
    for field in ["exceptions", "opaque_list", "links"] {
        selected.links.insert("practice".into(), vec![field.into()]);
        let result = compose(&world, &selected);
        assert_eq!(record_ids(&result), vec![id(2)]);
        assert_eq!(result["linkageComplete"], field == "links");
        if field != "links" {
            assert_eq!(result["unknowns"][0]["reason"], "not-typed-reference");
        }
    }
    let mut filtered = edit(&world, &id(2), "values", json!({"links":[reference(1)]}));
    filtered = edit(&filtered, &id(1), "status", json!("retired"));
    let result = compose(&filtered, &selected);
    assert_eq!(result["links"], json!([]));
    assert_eq!(result["unknowns"], json!([]));
    assert_eq!(result["linkageComplete"], true);
}

#[test]
fn traversal_preserves_selected_field_and_authored_member_order_with_shared_records_once() {
    let world = world(vec![
        concept(1, "tradition", json!({"links":[reference(3)]})),
        concept(
            2,
            "practice",
            json!({"link":reference(1),"more_links":[reference(4),reference(1)],"links":[]}),
        ),
        concept(3, "realm", json!({"links":[]})),
        concept(4, "artifact", json!({"links":[reference(3)]})),
    ]);
    let mut selected = selection();
    selected
        .links
        .insert("practice".into(), vec!["link".into(), "more_links".into()]);
    let result = compose(&world, &selected);
    assert_eq!(record_ids(&result), vec![id(2), id(1), id(3), id(4)]);
    let edges = result["links"].as_array().unwrap();
    assert_eq!(
        edges
            .iter()
            .map(|e| e["entityId"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        vec![id(1), id(3), id(4), id(3), id(1)]
    );
    assert_eq!(edges[0]["sourceField"], "values.link.id");
    assert_eq!(edges[4]["sourceField"], "values.more_links[1].id");
    assert_eq!(result["unknowns"], json!([]));
}

#[test]
fn hidden_references_are_removed_before_traversal_and_keep_surviving_source_positions() {
    let mut world = world(vec![
        concept(1, "tradition", json!({"links":[],"restrictions":"Visible"})),
        concept(2, "practice", json!({"links":[reference(3),reference(1)]})),
        concept(
            3,
            "realm",
            json!({"links":[reference(3)],"restrictions":"WITHHELD magic"}),
        ),
    ]);
    world = edit(&world, &id(3), "audience", json!(["hidden"]));
    let mut selected = selection();
    selected
        .restrictions
        .insert("practice".into(), "links".into());
    let result = compose(&world, &selected);
    assert_eq!(record_ids(&result), vec![id(2), id(1)]);
    assert_eq!(result["linkageComplete"], true);
    assert_eq!(result["unknowns"], json!([]));
    assert_eq!(result["links"][0]["sourceField"], "values.links[1].id");
    let field = &result["records"][0]["fields"]["restrictions"];
    assert_eq!(field["value"], json!([reference(1)]));
    assert_eq!(field["references"][0]["sourceField"], "values.links[1].id");
    assert!(!result.to_string().contains("WITHHELD"));
    assert!(!result.to_string().contains(&id(3)));
}

#[test]
fn missing_link_fields_and_visible_unselected_targets_remain_explicit_unknowns() {
    let world = world(vec![
        concept(
            1,
            "other",
            json!({"links":[],"restrictions":"Unselected profile statement"}),
        ),
        concept(
            2,
            "practice",
            json!({"links":[reference(1),{"kind":"record","id":PERSON}]}),
        ),
    ]);
    let mut selected = selection();
    selected
        .links
        .insert("practice".into(), vec!["link".into(), "links".into()]);
    let result = compose(&world, &selected);
    assert_eq!(record_ids(&result), vec![id(2)]);
    assert_eq!(result["links"].as_array().unwrap().len(), 2);
    assert_eq!(
        result["unknowns"],
        json!([
            {"ownerId":id(2),"sourceField":"values.link","reason":"absent-or-unavailable"},
            {"ownerId":id(2),"sourceField":"values.links[0].id","reason":"target-profile-unavailable"},
            {"ownerId":id(2),"sourceField":"values.links[1].id","reason":"target-profile-unavailable"}
        ])
    );
    assert_eq!(result["linkageComplete"], false);
    assert!(!result.to_string().contains("Unselected profile statement"));
}

#[test]
fn withheld_fanout_never_consumes_bounds_and_hidden_scalar_matches_absent() {
    let mut records = vec![concept(
        2,
        "practice",
        json!({"links":(0..1024).map(|n|reference(100+n%65)).collect::<Vec<_>>()}),
    )];
    records.extend((100..165).map(|n| {
        let original = concept(n, "realm", json!({"links":[reference(n)]}));
        let mut fields = original.frontmatter;
        fields["audience"] = yaml("[hidden]");
        record("lore-concept", &id(n), fields)
    }));
    let world = world(records);
    let selected = selection();
    let result = compose(&world, &selected);
    assert_eq!(record_ids(&result), vec![id(2)]);
    assert_eq!(result["links"], json!([]));
    assert_eq!(result["unknowns"], json!([]));
    assert_eq!(result["linkageComplete"], true);
    // A withheld scalar supplies the same unknown as an absent scalar. It does
    // not disclose a target ID, target title, cycle or withheld-field marker.
    let hidden = edit(&world, &id(2), "values", json!({"link":reference(100)}));
    let absent = edit(&world, &id(2), "values", json!({}));
    let mut selected = selected;
    selected
        .links
        .insert("practice".into(), vec!["link".into()]);
    let hidden = compose(&hidden, &selected);
    let absent = compose(&absent, &selected);
    for field in ["links", "unknowns", "linkageComplete"] {
        assert_eq!(hidden[field], absent[field]);
    }
    assert_eq!(hidden["unknowns"][0]["reason"], "absent-or-unavailable");
    assert!(!hidden.to_string().contains(&id(100)));
}

#[test]
fn missing_hidden_inactive_wrong_profile_and_alias_roots_have_one_unavailable_result() {
    let selected = selection();
    let missing = magic::compose(&base(), &id(999), &selected, None, "author", "author")
        .unwrap_err()
        .json();
    for (field, value) in [
        ("audience", json!(["hidden"])),
        ("perspectives", json!(["hidden"])),
        ("status", json!("draft")),
        ("status", json!("retired")),
        ("profile", json!("magic-other")),
    ] {
        let world = edit(&base(), &id(2), field, value);
        assert_eq!(
            magic::compose(&world, &id(2), &selected, None, "author", "author")
                .unwrap_err()
                .json(),
            missing
        );
    }
    let world = edit(&base(), &id(2), "aliases", json!(["Named practice"]));
    assert_eq!(
        magic::compose(
            &world,
            "Named practice",
            &selected,
            None,
            "author",
            "author"
        )
        .unwrap_err()
        .json(),
        missing
    );
    assert_eq!(
        magic::compose(&world, PERSON, &selected, None, "author", "author")
            .unwrap_err()
            .json(),
        missing
    );
}

#[test]
fn exact_horizons_admit_only_the_authored_practice_for_each_era() {
    let mut world = world(vec![
        concept(1, "tradition", json!({"links":[]})),
        concept(
            2,
            "practice",
            json!({"links":[reference(1)],"restrictions":"First-era vow"}),
        ),
        concept(
            3,
            "practice",
            json!({"links":[reference(1)],"restrictions":"Second-era silence"}),
        ),
        concept(
            4,
            "practice",
            json!({"links":[reference(1)],"restrictions":"Third-era lantern"}),
        ),
    ]);
    for (id, start, end) in [(id(2), 0, 9), (id(3), 10, 19), (id(4), 20, 29)] {
        world = edit(
            &world,
            &id,
            "valid",
            json!({"kind":"interval","timeline":"main","start":{"timeline":"main","tick":start,"order":0},"end":{"timeline":"main","tick":end,"order":1}}),
        );
    }
    let selected = selection();
    for (n, tick, wording) in [
        (2, 5, "First-era vow"),
        (3, 15, "Second-era silence"),
        (4, 25, "Third-era lantern"),
    ] {
        let result = magic::compose(
            &world,
            &id(n),
            &selected,
            Some(&at(tick, 0)),
            "author",
            "author",
        )
        .unwrap();
        assert_eq!(
            result["records"][0]["fields"]["restrictions"]["value"],
            wording
        );
        assert_eq!(record_ids(&result), vec![id(n), id(1)]);
        for other in [2, 3, 4].into_iter().filter(|value| *value != n) {
            assert!(matches!(
                magic::compose(
                    &world,
                    &id(other),
                    &selected,
                    Some(&at(tick, 0)),
                    "author",
                    "author"
                ),
                Err(Error::NotFound(_))
            ));
        }
        let all_time = magic::compose(&world, &id(n), &selected, None, "author", "author").unwrap();
        assert_eq!(all_time["scope"], "author-all-time-lore");
        assert!(all_time["at"].is_null());
    }
    assert!(
        magic::compose(
            &world,
            &id(2),
            &selected,
            Some(&at(9, 1)),
            "author",
            "author"
        )
        .is_ok()
    );
    assert!(
        magic::compose(
            &world,
            &id(2),
            &selected,
            Some(&at(9, 2)),
            "author",
            "author"
        )
        .is_err()
    );
    assert!(
        magic::compose(
            &world,
            &id(2),
            &selected,
            Some(&StoryTime::new("other", 5, 0).unwrap()),
            "author",
            "author"
        )
        .is_err()
    );
}

#[test]
fn cycles_refuse_a_partial_composition() {
    for next in [1, 2] {
        let world = world(vec![
            concept(1, "tradition", json!({"links":[reference(next)]})),
            concept(2, "practice", json!({"links":[reference(1)]})),
        ]);
        assert!(validation::validate_world(&world).valid);
        let error =
            magic::compose(&world, &id(2), &selection(), None, "author", "author").unwrap_err();
        assert!(matches!(error, Error::InvalidInput(_)));
        assert!(error.to_string().contains("cycle"));
    }
}

#[test]
fn root_depth_zero_allows_sixteen_edges_and_refuses_the_next() {
    for length in [17, 18] {
        let records = (0..length)
            .map(|offset| {
                concept(
                    100 + offset,
                    "practice",
                    json!({"links":if offset+1<length {vec![reference(101+offset)]} else {vec![]}}),
                )
            })
            .collect();
        let world = world(records);
        let result = magic::compose(&world, &id(100), &selection(), None, "author", "author");
        if length == 17 {
            assert_eq!(result.unwrap()["records"].as_array().unwrap().len(), 17);
        } else {
            assert!(result.unwrap_err().to_string().contains("depth 16"));
        }
    }
}

#[test]
fn record_bound_includes_the_root_and_unselected_visible_targets() {
    for role in ["tradition", "other"] {
        for count in [63, 64] {
            let mut targets = (100..100 + count).map(reference).collect::<Vec<_>>();
            targets.push(reference(100));
            let mut records = vec![concept(2, "practice", json!({"links":targets}))];
            records.extend((100..100 + count).map(|n| concept(n, role, json!({"links":[]}))));
            let world = world(records);
            let result = magic::compose(&world, &id(2), &selection(), None, "author", "author");
            if count == 63 {
                let result = result.unwrap();
                assert_eq!(
                    result["records"].as_array().unwrap().len(),
                    if role == "tradition" { 64 } else { 1 }
                );
                assert_eq!(
                    result["unknowns"].as_array().unwrap().len(),
                    if role == "tradition" { 0 } else { 63 }
                );
            } else {
                assert!(result.unwrap_err().to_string().contains("64 records"));
            }
        }
    }
}

#[test]
fn visit_bound_includes_the_root_and_every_repeated_link() {
    for count in [1023, 1024] {
        let world = world(vec![
            concept(1, "tradition", json!({"links":[]})),
            concept(2, "practice", json!({"links":vec![reference(1);count]})),
        ]);
        let result = magic::compose(&world, &id(2), &selection(), None, "author", "author");
        if count == 1023 {
            let result = result.unwrap();
            assert_eq!(record_ids(&result), vec![id(2), id(1)]);
            assert_eq!(result["links"].as_array().unwrap().len(), 1023);
        } else {
            assert!(result.unwrap_err().to_string().contains("1024 visits"));
        }
    }
}

#[test]
fn selected_roles_fields_and_link_lists_are_closed_and_bounded() {
    for (field, value) in [
        ("profiles", json!({})),
        ("profiles", json!({"power":"magic-practice"})),
        ("profiles", json!({"practice":"bad profile"})),
        (
            "profiles",
            json!({"practice":"magic-practice","realm":"magic-practice"}),
        ),
        ("links", json!({"power":["links"]})),
        ("links", json!({"practice":["links","links"]})),
        ("links", json!({"practice":["values.links"]})),
        ("restrictions", json!({"power":"restrictions"})),
        ("restrictions", json!({"practice":"bad-field"})),
        ("exceptions", json!({"practice":""})),
    ] {
        let mut value_object = serde_json::to_value(selection()).unwrap();
        value_object[field] = value;
        assert!(
            serde_json::from_value::<Selection>(value_object)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    let mut extra = serde_json::to_value(selection()).unwrap();
    extra["infer_outcomes"] = json!(true);
    assert!(serde_json::from_value::<Selection>(extra).is_err());
    let mut selected = selection();
    selected.profiles.remove("realm");
    assert!(selected.validate().is_err());
    selected.links.remove("realm");
    selected.restrictions.remove("realm");
    selected.exceptions.remove("realm");
    assert!(selected.validate().is_ok());
    for count in [16, 17] {
        selected.links.insert(
            "practice".into(),
            (0..count).map(|n| format!("field_{n}")).collect(),
        );
        assert_eq!(selected.validate().is_ok(), count == 16);
    }
    let mut selected = selection();
    selected
        .profiles
        .insert("practice".into(), "undeclared".into());
    assert!(
        magic::compose(&base(), &id(2), &selected, None, "author", "author")
            .unwrap_err()
            .to_string()
            .contains("existing exact profile IDs")
    );
    let minimal: Selection =
        serde_json::from_value(json!({"profiles":{"practice":"magic-practice"}})).unwrap();
    let result = compose(&base(), &minimal);
    assert_eq!(record_ids(&result), vec![id(2)]);
    assert_eq!(result["links"], json!([]));
    assert_eq!(
        result["records"][0]["fields"]["restrictions"]["reason"],
        "unselected"
    );
}

#[test]
fn unrelated_catalogue_records_do_not_consume_the_composition_budget() {
    let mut records = vec![concept(2, "practice", json!({"links":[]}))];
    records.extend((100..1101).map(|n| concept(n, "other", json!({"links":[]}))));
    let world = world(records);
    assert_eq!(record_ids(&compose(&world, &selection())), vec![id(2)]);
}

#[test]
fn author_composition_never_changes_source_or_character_evidence() {
    let world = base();
    let raw = world
        .records
        .iter()
        .map(|(id, r)| (id.clone(), r.raw_bytes.clone()))
        .collect::<BTreeMap<_, _>>();
    let context = character::context(&world, PERSON, &at(5, 0), 100_000).unwrap();
    let search = character::search(&world, PERSON, "Practice vow", &at(5, 0), 100).unwrap();
    compose(&world, &selection());
    magic::compose(&world, &id(2), &selection(), None, "author", "author").unwrap();
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
        character::search(&world, PERSON, "Practice vow", &at(5, 0), 100).unwrap(),
        search
    );
    assert!(!context.to_string().contains("Practice vow"));
}
