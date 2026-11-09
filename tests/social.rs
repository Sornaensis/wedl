//! Explicit terminology and cultural context over the shared author admission gate.
use serde_json::{Value as Json, json};
use std::{collections::BTreeMap, path::PathBuf};
use wedl::{
    Error, assertion, character, description,
    model::{Record, StoryTime, World},
    social::{self, Selection},
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const PERSON: &str = "char_00000000000000000000000001";
fn id(n: usize) -> String {
    format!("concept_{n:026}")
}
fn reference(n: usize) -> Json {
    json!({"kind":"record","id":id(n)})
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn interval(start: i64, end: i64) -> Json {
    json!({"kind":"interval","timeline":"main","start":{"timeline":"main","tick":start,"order":0},
        "end":{"timeline":"main","tick":end,"order":0}})
}
fn entity() -> Json {
    json!({"type":"entity","kinds":["lore-concept"]})
}
fn entities() -> Json {
    json!({"type":"array","items":entity(),"max_items":64})
}
fn literal() -> Json {
    json!({"type":"object","required":{"kind":{"type":"string"},"id":{"type":"string"}},"optional":{}})
}
fn profiles() -> Vec<Json> {
    [
        ("language", json!({}), json!({"scripts":entities(),"note":{"type":"string"}})),
        ("script", json!({}), json!({"note":{"type":"string"}})),
        ("sense", json!({"language":entity(),"gloss":{"type":"string"}}), json!({"note":{"type":"string"}})),
        ("register", json!({"language":entity(),"label":{"type":"string"}}), json!({})),
        ("era-spelling", json!({"language":entity(),"sense":entity(),"form":{"type":"string"}}),
            json!({"scripts":entities(),"register":entity(),"unresolved":{"type":"null"},"number":{"type":"number"},"choice":{"type":"enum","values":[1,"1"]}})),
        ("culture", json!({}), json!({"languages":entities(),"religions":entities(),"note":{"type":"string"}})),
        ("religion", json!({}), json!({"cultures":entities(),"languages":entities(),"note":{"type":"string"}})),
        ("membership", json!({"member":{"type":"entity","kinds":["character","organization","lore-concept"]},
            "group":entity(),"relation":{"type":"enum","values":["cultural-membership","religious-membership","language-use"]}}),json!({"note":{"type":"string"}})),
    ].into_iter().map(|(role,required,optional)| json!({"id":format!("social-{role}"),"label":role,
        "fields":{"type":"object","required":required,"optional":optional}})).collect()
}
fn record(kind: &str, id: &str, fields: Json) -> Record {
    let mut data = json!({"schema":"wedl/v1","kind":kind,"id":id,"title":id});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let raw = format!(
        "---\n{}---\n# Undated account 2040\r\nLiteral source  \rTail\n",
        serde_yaml::to_string(&data).unwrap()
    );
    source::parse_record(raw.as_bytes(), &format!("story/{id}.md")).unwrap()
}
fn concept(n: usize, role: &str, values: Json) -> Record {
    record(
        "lore-concept",
        &id(n),
        json!({"profile":format!("social-{role}"),"domain":"authored.social",
        "values":values,"valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]}),
    )
}
fn world(mut records: Vec<Record>) -> World {
    records.insert(0, record("world", WORLD, json!({
        "capabilities":["lore-core-v1","descriptions-v1","assertions-v1"],"default_timeline":"main",
        "timelines":[{"id":"main","label":"Main"},{"id":"branch","label":"Branch"}],"state_keys":{},
        "profiles":profiles(),"predicates":[{"id":"social-doctrine","subject_kinds":["lore-concept"],"value_type":{"type":"string"}},
            {"id":"social-number","subject_kinds":["lore-concept"],"value_type":{"type":"number"}}]
    })));
    records.push(record("character", PERSON, json!({"initial_state":{}})));
    source::world_from_records(PathBuf::new(), records, None).unwrap()
}
fn base() -> World {
    let mut world = world(vec![
        concept(
            1,
            "language",
            json!({"scripts":[reference(3)],"note":"River language"}),
        ),
        concept(2, "language", json!({"scripts":[],"note":"Hill language"})),
        concept(3, "script", json!({"note":"Harbor script"})),
        concept(
            4,
            "sense",
            json!({"language":reference(1),"gloss":"Water beside a river"}),
        ),
        concept(
            5,
            "sense",
            json!({"language":reference(2),"gloss":"A place to keep credit"}),
        ),
        concept(
            6,
            "register",
            json!({"language":reference(1),"label":"Ceremonial"}),
        ),
        concept(
            7,
            "register",
            json!({"language":reference(2),"label":"Formal"}),
        ),
        concept(
            8,
            "era-spelling",
            json!({"language":reference(1),"sense":reference(4),"form":"banc"}),
        ),
        concept(
            9,
            "era-spelling",
            json!({"language":reference(1),"sense":reference(4),"form":"bank","register":reference(6),"number":9007199254740993u64,"choice":1,"unresolved":null}),
        ),
        concept(
            10,
            "era-spelling",
            json!({"language":reference(1),"sense":reference(4),"form":"banka"}),
        ),
        concept(
            11,
            "era-spelling",
            json!({"language":reference(1),"sense":reference(4),"form":"banque"}),
        ),
        concept(
            12,
            "era-spelling",
            json!({"language":reference(2),"sense":reference(5),"form":"bank"}),
        ),
        concept(
            13,
            "culture",
            json!({"languages":[reference(2)],"religions":[reference(14)],"note":"An explicit diaspora"}),
        ),
        concept(
            14,
            "religion",
            json!({"cultures":[reference(13)],"languages":[reference(2)]}),
        ),
        concept(
            15,
            "membership",
            json!({"member":{"kind":"record","id":PERSON},"group":reference(14),"relation":"religious-membership","note":"An explicitly described exception"}),
        ),
    ]);
    for (n, start, end) in [(8, -20, -1), (9, 0, 9), (10, 10, 20), (11, 2, 5)] {
        world = patch(&world, &id(n), "valid", interval(start, end));
    }
    world
}
fn patch(world: &World, id: &str, field: &str, value: Json) -> World {
    let mut data = world.records[id].frontmatter.clone();
    data[field] = serde_yaml::to_value(value).unwrap();
    replace_record(world, id, data, &world.records[id].body)
}
fn replace_record(world: &World, id: &str, data: serde_yaml::Value, body: &str) -> World {
    let old = &world.records[id];
    let raw = format!("---\n{}---\n{body}", serde_yaml::to_string(&data).unwrap());
    let mut records = world.records.clone();
    records.insert(
        id.into(),
        source::parse_record(raw.as_bytes(), &old.source_path).unwrap(),
    );
    source::world_from_records(
        world.root.clone(),
        records.into_values().collect(),
        world.revision.clone(),
    )
    .unwrap()
}
fn add(world: &World, record: Record) -> World {
    let mut records = world.records.clone();
    assert!(records.insert(record.id.clone(), record).is_none());
    source::world_from_records(
        world.root.clone(),
        records.into_values().collect(),
        world.revision.clone(),
    )
    .unwrap()
}
fn selection() -> Selection {
    Selection {
        language: id(1),
        sense: Some(id(4)),
        form: None,
        contexts: vec![],
    }
}
fn read(world: &World, selected: &Selection, time: &StoryTime) -> Json {
    social::context(world, selected, time, "author", "author").unwrap()
}
fn row<'a>(result: &'a Json, n: usize) -> &'a Json {
    &result["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["item"]["conceptId"] == id(n))
        .unwrap()["item"]
}
fn ids(result: &Json, role: &str) -> Vec<String> {
    result["records"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["role"] == role)
        .map(|v| v["item"]["conceptId"].as_str().unwrap().to_owned())
        .collect()
}
fn section(n: usize, text: &str) -> Json {
    json!({"id":format!("description_{n:026}"),"heading":"Account","markdown":text,
        "valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]})
}
fn account(n: usize, owner: usize, class: &str, value: &str) -> Record {
    let mut fields = json!({"domain":"authored.social","subject":reference(owner),"predicate":"social-doctrine","value":value,
        "classification":class,"valid":{"kind":"timeless"},"evidence":[],"audience":["author"],"perspectives":["author"]});
    if class == "attributed-claim" {
        fields["attributed_to"] = json!({"kind":"record","id":PERSON});
    }
    record("assertion", &format!("assertion_{n:026}"), fields)
}
fn change_type(world: &World, profile: &str, field: &str, ty: Json) -> World {
    let mut profiles =
        serde_json::to_value(world.world_record().unwrap().get("profiles").unwrap()).unwrap();
    let p = profiles
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["id"] == profile)
        .unwrap();
    let required = p["fields"]["required"]
        .as_object()
        .unwrap()
        .contains_key(field);
    p["fields"][if required { "required" } else { "optional" }][field] = ty;
    patch(world, WORLD, "profiles", profiles)
}

#[test]
fn complete_social_context_keeps_source_roles_exact_values_and_body() {
    let world = base();
    let check = validation::validate_world(&world);
    assert!(check.valid, "{:?}", check.diagnostics);
    let mut selected = selection();
    selected.contexts = vec![id(13), id(14), id(15)];
    let result = read(&world, &selected, &at(3, 0));
    assert_eq!(result["protocol"], "wedl-rust/v1");
    assert_eq!(result["perspective"], "author");
    assert_eq!(result["count"], 9);
    assert_eq!(ids(&result, "spelling"), vec![id(9), id(11)]);
    assert_eq!(row(&result, 9)["values"]["number"], "9007199254740993");
    assert_eq!(row(&result, 9)["values"]["unresolved"], Json::Null);
    assert_eq!(
        row(&result, 9)["valueTypes"]["fields"]["choice"]["type"],
        "enum"
    );
    assert_eq!(
        row(&result, 9)["valueTypes"]["fields"]["choice"]["scalarType"],
        "number"
    );
    assert_eq!(row(&result, 1)["body"], world.records[&id(1)].body);
    assert_eq!(result["at"]["tick"], "3");
    let order = result["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["item"]["conceptId"].as_str().unwrap())
        .collect::<Vec<_>>();
    let mut sorted = order.clone();
    sorted.sort();
    assert_eq!(order, sorted);
}

#[test]
fn era_spellings_keep_inclusive_same_tick_boundaries_and_overlap() {
    let world = base();
    for (tick, order, expected) in [
        (-1, 0, vec![id(8)]),
        (-1, 1, vec![]),
        (0, 0, vec![id(9)]),
        (3, 0, vec![id(9), id(11)]),
        (9, 0, vec![id(9)]),
        (9, 1, vec![]),
        (10, 0, vec![id(10)]),
    ] {
        assert_eq!(
            ids(&read(&world, &selection(), &at(tick, order)), "spelling"),
            expected
        );
    }
    assert!(
        ids(
            &read(
                &world,
                &selection(),
                &StoryTime::new("branch", 3, 0).unwrap()
            ),
            "spelling"
        )
        .is_empty()
    );
}

#[test]
fn homonyms_remain_different_language_sense_and_citation_identities() {
    let world = base();
    let mut river = selection();
    river.form = Some("bank".into());
    let hill = Selection {
        language: id(2),
        sense: Some(id(5)),
        form: Some("bank".into()),
        contexts: vec![],
    };
    let left = read(&world, &river, &at(3, 0));
    let right = read(&world, &hill, &at(3, 0));
    assert_eq!(ids(&left, "spelling"), vec![id(9)]);
    assert_eq!(ids(&right, "spelling"), vec![id(12)]);
    assert_ne!(
        row(&left, 4)["values"]["gloss"],
        row(&right, 5)["values"]["gloss"]
    );
    assert_ne!(row(&left, 9)["citation"], row(&right, 12)["citation"]);
}

#[test]
fn form_is_exact_literal_unicode_case_and_empty_text() {
    let mut world = base();
    let mut selected = selection();
    for text in ["BANK", "bank ", "char_00000000000000000000000001"] {
        selected.form = Some(text.into());
        assert!(ids(&read(&world, &selected, &at(3, 0)), "spelling").is_empty());
    }
    for text in ["é", ""] {
        let mut values = row(&read(&world, &selection(), &at(3, 0)), 9)["values"].clone();
        // Rebuild the numeric source field without using its wire string.
        values["number"] = json!(9007199254740993u64);
        values["choice"] = json!(1);
        values["form"] = json!(text);
        world = patch(&world, &id(9), "values", values);
        selected.form = Some(text.into());
        assert_eq!(
            ids(&read(&world, &selected, &at(3, 0)), "spelling"),
            vec![id(9)]
        );
        selected.form = Some("e\u{301}".into());
        assert!(ids(&read(&world, &selected, &at(3, 0)), "spelling").is_empty());
    }
}

#[test]
fn numeric_wire_text_cannot_be_selected_as_a_string_form() {
    let mut world = base();
    world = change_type(
        &world,
        "social-era-spelling",
        "form",
        json!({"type":"number"}),
    );
    for n in [8, 9, 10, 11, 12] {
        let mut values =
            serde_json::to_value(world.records[&id(n)].get("values").unwrap()).unwrap();
        if values.get("number").is_some() {
            values["number"] = json!(9007199254740993u64);
            values["choice"] = json!(1);
        }
        values["form"] = json!(9007199254740993u64);
        world = patch(&world, &id(n), "values", values);
    }
    let mut selected = selection();
    selected.form = Some("9007199254740993".into());
    assert!(validation::validate_world(&world).valid);
    assert!(ids(&read(&world, &selected, &at(3, 0)), "spelling").is_empty());
}

#[test]
fn optional_sense_changes_discovery_but_not_explicit_evidence_owners() {
    let world = add(
        &base(),
        concept(
            16,
            "sense",
            json!({"language":reference(1),"gloss":"Another sense"}),
        ),
    );
    let world = add(
        &world,
        concept(
            17,
            "era-spelling",
            json!({"language":reference(1),"sense":reference(16),"form":"bank"}),
        ),
    );
    let world = add(
        &world,
        concept(
            18,
            "register",
            json!({"language":reference(1),"label":"Everyday"}),
        ),
    );
    let mut selected = selection();
    selected.sense = None;
    selected.form = Some("bank".into());
    let result = read(&world, &selected, &at(3, 0));
    assert_eq!(ids(&result, "sense"), vec![id(4), id(16)]);
    assert_eq!(ids(&result, "spelling"), vec![id(9), id(17)]);
    assert_eq!(result["sections"].as_array().unwrap().len(), 1);
    assert_eq!(result["sections"][0]["ownerId"], id(1));
    selected.sense = Some(id(4));
    let result = read(&world, &selected, &at(3, 0));
    assert_eq!(ids(&result, "sense"), vec![id(4)]);
    assert_eq!(ids(&result, "register"), vec![id(6), id(18)]);
}

#[test]
fn culture_membership_and_concept_links_do_not_imply_extra_context_roots() {
    let world = patch(
        &base(),
        &id(13),
        "concept_links",
        json!([{"relation":"schism-from","concept_id":id(14),"valid":{"kind":"timeless"}}]),
    );
    let mut selected = selection();
    selected.contexts = vec![id(13), id(15)];
    let result = read(&world, &selected, &at(3, 0));
    assert!(ids(&result, "religion").is_empty());
    assert_eq!(ids(&result, "language"), vec![id(1)]);
    assert_eq!(row(&result, 15)["values"]["member"]["id"], PERSON);
    assert_eq!(
        row(&result, 15)["values"]["relation"],
        "religious-membership"
    );
    assert_eq!(
        row(&result, 13)["conceptLinks"][0]["relation"],
        "schism-from"
    );
    assert_eq!(result["sections"].as_array().unwrap().len(), 4);
}

#[test]
fn native_sections_and_assertion_corrections_remain_separate_complete_evidence() {
    let mut first = section(1, "Original wording\r\n");
    first["valid"] = interval(0, 9);
    let mut corrected = section(2, "Corrected wording  \n");
    corrected["valid"] = interval(0, 9);
    corrected["replaces"] = first["id"].clone();
    let world = patch(&base(), &id(1), "descriptions", json!([first, corrected]));
    let world = add(&world, account(1, 14, "author-fact", "Common doctrine"));
    let world = add(
        &world,
        account(2, 14, "attributed-claim", "Individual exception"),
    );
    let world = add(&world, account(3, 14, "unresolved", "Unresolved origin"));
    let world = add(
        &world,
        account(4, 14, "author-fact", "Corrected common doctrine"),
    );
    let world = patch(
        &world,
        "assertion_00000000000000000000000004",
        "replaces",
        json!("assertion_00000000000000000000000001"),
    );
    let mut selected = selection();
    selected.contexts = vec![id(14)];
    let result = read(&world, &selected, &at(3, 0));
    assert_eq!(
        result["sections"][0]["result"],
        description::list(&world, &id(1), Some(&at(3, 0)), "author", "author", 512).unwrap()
    );
    assert_eq!(
        result["canon"][2]["result"],
        assertion::as_of(&world, &id(14), Some(&at(3, 0)), "author", "author", 512).unwrap()
    );
    assert_eq!(result["canon"][2]["result"]["count"], 3);
    assert_eq!(
        result["sections"][0]["result"]["items"][0]["citation"]["section"],
        "descriptions[1]"
    );
    let account = &result["canon"][2]["result"]["items"][0];
    assert_eq!(account["classification"], "attributed-claim");
    assert_eq!(account["attributedTo"]["id"], PERSON);
}

#[test]
fn discovered_records_do_not_supply_assertion_or_description_evidence() {
    let world = patch(
        &base(),
        &id(9),
        "descriptions",
        json!([section(
            1,
            "Do not promote a discovered spelling to an evidence root"
        )]),
    );
    let world = add(
        &world,
        account(1, 6, "author-fact", "Do not promote a discovered register"),
    );
    let result = read(&world, &selection(), &at(3, 0));
    assert_eq!(result["canon"].as_array().unwrap().len(), 2);
    assert_eq!(result["sections"].as_array().unwrap().len(), 2);
    assert!(!result.to_string().contains("Do not promote"));
}

#[test]
fn required_roots_share_one_unavailable_error_across_scope_status_time_and_role() {
    let world = base();
    let selected = selection();
    let error = social::context(
        &world,
        &Selection {
            language: id(999),
            ..selected.clone()
        },
        &at(3, 0),
        "author",
        "author",
    )
    .unwrap_err();
    assert!(matches!(error, Error::NotFound(_)));
    let text = error.to_string();
    for changed in [
        patch(&world, &id(1), "audience", json!(["private"])),
        patch(&world, &id(1), "perspectives", json!(["private"])),
        patch(&world, &id(1), "status", json!("draft")),
        patch(&world, &id(1), "status", json!("retired")),
        patch(&world, &id(1), "valid", interval(10, 20)),
    ] {
        assert_eq!(
            social::context(&changed, &selected, &at(3, 0), "author", "author")
                .unwrap_err()
                .to_string(),
            text
        );
    }
    for wrong in [
        Selection {
            language: id(3),
            ..selected.clone()
        },
        Selection {
            sense: Some(id(5)),
            ..selected.clone()
        },
        Selection {
            contexts: vec![id(3)],
            ..selected
        },
    ] {
        assert_eq!(
            social::context(&world, &wrong, &at(3, 0), "author", "author")
                .unwrap_err()
                .to_string(),
            text
        );
    }
}

#[test]
fn hidden_incidental_endpoints_are_filtered_before_discovery() {
    let world = patch(&base(), &id(2), "audience", json!(["private"]));
    let result = read(&world, &selection(), &at(3, 0));
    assert_eq!(ids(&result, "spelling"), vec![id(9), id(11)]);
    let world = patch(&world, &id(4), "audience", json!(["private"]));
    let selected = Selection {
        sense: None,
        ..selection()
    };
    let result = read(&world, &selected, &at(3, 0));
    assert!(ids(&result, "sense").is_empty());
    assert!(ids(&result, "spelling").is_empty());
    assert!(!result.to_string().contains(&id(4)));
}

#[test]
fn hidden_script_members_do_not_renumber_surviving_reference_citations() {
    let world = add(
        &base(),
        concept(16, "script", json!({"note":"Second script"})),
    );
    let world = patch(
        &world,
        &id(1),
        "values",
        json!({"scripts":[reference(3),reference(16)]}),
    );
    let world = patch(&world, &id(3), "audience", json!(["private"]));
    let result = read(&world, &selection(), &at(3, 0));
    assert_eq!(ids(&result, "script"), vec![id(16)]);
    assert_eq!(
        row(&result, 1)["references"][0]["sourceField"],
        "values.scripts[1].id"
    );
    assert_eq!(row(&result, 1)["values"]["scripts"], json!([reference(16)]));
    let world = patch(&world, &id(16), "audience", json!(["private"]));
    let result = read(&world, &selection(), &at(3, 0));
    assert_eq!(row(&result, 1)["values"]["scripts"], json!([]));
    assert!(ids(&result, "script").is_empty());
}

#[test]
fn ref_shaped_literal_scripts_and_registers_are_never_followed() {
    let world = change_type(
        &base(),
        "social-language",
        "scripts",
        json!({"type":"array","items":literal(),"max_items":64}),
    );
    let world = change_type(&world, "social-era-spelling", "register", literal());
    let mut values = serde_json::to_value(world.records[&id(9)].get("values").unwrap()).unwrap();
    values["number"] = json!(9007199254740993u64);
    values["choice"] = json!(1);
    values["register"] = reference(7);
    let world = patch(&world, &id(9), "values", values);
    assert!(validation::validate_world(&world).valid);
    let result = read(&world, &selection(), &at(3, 0));
    assert!(ids(&result, "script").is_empty());
    assert_eq!(ids(&result, "register"), vec![id(6)]);
    assert_eq!(row(&result, 1)["values"]["scripts"], json!([reference(3)]));
    assert_eq!(row(&result, 9)["values"]["register"], reference(7));
}

#[test]
fn literal_language_does_not_supply_sense_or_spelling_membership() {
    let world = change_type(&base(), "social-sense", "language", literal());
    let mut selected = selection();
    selected.sense = None;
    let result = read(&world, &selected, &at(3, 0));
    assert!(ids(&result, "sense").is_empty());
    assert!(ids(&result, "spelling").is_empty());
    assert!(matches!(
        social::context(&world, &selection(), &at(3, 0), "author", "author"),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn visible_wrong_role_associations_and_inconsistent_sense_language_refuse() {
    let world = patch(
        &base(),
        &id(1),
        "values",
        json!({"scripts":[reference(13)]}),
    );
    assert!(validation::validate_world(&world).valid);
    assert!(matches!(
        social::context(&world, &selection(), &at(3, 0), "author", "author"),
        Err(Error::NotFound(_))
    ));
    let world = patch(
        &base(),
        &id(9),
        "values",
        json!({"language":reference(1),"sense":reference(5),"form":"bank"}),
    );
    let selected = Selection {
        sense: None,
        ..selection()
    };
    assert!(matches!(
        social::context(&world, &selected, &at(3, 0), "author", "author"),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn selected_record_bound_is_exact_and_unrelated_or_hidden_rows_do_not_count() {
    let mut world = base();
    for n in 100..126 {
        world = add(
            &world,
            concept(
                n,
                "register",
                json!({"language":reference(1),"label":format!("Register {n}")}),
            ),
        );
    }
    assert_eq!(read(&world, &selection(), &at(3, 0))["count"], 32);
    world = add(
        &world,
        concept(
            126,
            "register",
            json!({"language":reference(1),"label":"One too many"}),
        ),
    );
    assert!(
        social::context(&world, &selection(), &at(3, 0), "author", "author")
            .unwrap_err()
            .to_string()
            .contains("32")
    );
    world = patch(&world, &id(126), "audience", json!(["private"]));
    for n in 200..240 {
        world = add(
            &world,
            concept(
                n,
                "register",
                json!({"language":reference(2),"label":"Other language"}),
            ),
        );
    }
    assert_eq!(read(&world, &selection(), &at(3, 0))["count"], 32);
}

#[test]
fn selectors_are_closed_exact_and_literal_form_bound_counts_utf8_bytes() {
    let world = base();
    for selected in [
        Selection {
            language: "River language".into(),
            ..selection()
        },
        Selection {
            sense: Some(PERSON.into()),
            ..selection()
        },
        Selection {
            contexts: vec![id(13), id(13)],
            ..selection()
        },
        Selection {
            contexts: (100..109).map(id).collect(),
            ..selection()
        },
        Selection {
            form: Some("å".repeat(2049)),
            ..selection()
        },
    ] {
        assert!(matches!(
            social::context(&world, &selected, &at(3, 0), "author", "author"),
            Err(Error::InvalidInput(_))
        ));
    }
    let selected = Selection {
        form: Some("å".repeat(2048)),
        ..selection()
    };
    assert!(social::context(&world, &selected, &at(3, 0), "author", "author").is_ok());
    assert!(
        social::context(
            &world,
            &selection(),
            &StoryTime::new("absent", 3, 0).unwrap(),
            "author",
            "author"
        )
        .is_err()
    );
    assert!(social::context(&world, &selection(), &at(3, 0), " ", "author").is_err());
}

#[test]
fn all_three_existing_capabilities_are_required_without_implicit_installation() {
    let world = base();
    for missing in ["lore-core-v1", "assertions-v1", "descriptions-v1"] {
        let caps = ["lore-core-v1", "assertions-v1", "descriptions-v1"]
            .into_iter()
            .filter(|c| *c != missing)
            .collect::<Vec<_>>();
        let changed = patch(&world, WORLD, "capabilities", json!(caps));
        assert!(
            social::context(&changed, &selection(), &at(3, 0), "author", "author").is_err(),
            "{missing}"
        );
    }
}

#[test]
fn aggregate_evidence_limit_counts_only_admitted_items() {
    let world = patch(
        &base(),
        &id(1),
        "descriptions",
        json!((1..=256).map(|n| section(n, "")).collect::<Vec<_>>()),
    );
    let world = patch(
        &world,
        &id(4),
        "descriptions",
        json!((1..=257).map(|n| section(n, "")).collect::<Vec<_>>()),
    );
    assert!(validation::validate_world(&world).valid);
    assert!(
        social::context(&world, &selection(), &at(3, 0), "author", "author")
            .unwrap_err()
            .to_string()
            .contains("512")
    );
    let world = patch(
        &world,
        &id(1),
        "descriptions",
        json!(
            (1..=256)
                .map(|n| {
                    let mut s = section(n, "");
                    s["audience"] = json!(["private"]);
                    s
                })
                .collect::<Vec<_>>()
        ),
    );
    let world = patch(
        &world,
        &id(4),
        "descriptions",
        json!(
            (1..=257)
                .map(|n| {
                    let mut s = section(n, "");
                    s["audience"] = json!(["private"]);
                    s
                })
                .collect::<Vec<_>>()
        ),
    );
    let result = read(&world, &selection(), &at(3, 0));
    assert_eq!(result["sections"][0]["result"]["count"], 0);
    assert_eq!(result["sections"][1]["result"]["count"], 0);
}

#[test]
fn compact_report_byte_bound_includes_newline_and_never_clips_body() {
    let world = base();
    let world = replace_record(
        &world,
        &id(1),
        world.records[&id(1)].frontmatter.clone(),
        "",
    );
    let empty = read(&world, &selection(), &at(3, 0));
    let spare = 262144 - serde_json::to_vec(&empty).unwrap().len() - 1;
    let exact = replace_record(
        &world,
        &id(1),
        world.records[&id(1)].frontmatter.clone(),
        &"x".repeat(spare),
    );
    let value = read(&exact, &selection(), &at(3, 0));
    assert_eq!(serde_json::to_vec(&value).unwrap().len() + 1, 262144);
    assert_eq!(row(&value, 1)["body"].as_str().unwrap().len(), spare);
    let too_large = replace_record(
        &world,
        &id(1),
        world.records[&id(1)].frontmatter.clone(),
        &"x".repeat(spare + 1),
    );
    assert!(
        social::context(&too_large, &selection(), &at(3, 0), "author", "author")
            .unwrap_err()
            .to_string()
            .contains("262144")
    );
}

#[test]
fn reading_never_mutates_source_or_adds_character_knowledge() {
    let base = base();
    let time = at(3, 0);
    let before = character::context(&base, PERSON, &time, 100000).unwrap();
    let world = patch(
        &base,
        &id(1),
        "descriptions",
        json!([section(1, "A canonical language fact never learned")]),
    );
    let world = add(
        &world,
        account(1, 14, "author-fact", "New canonical doctrine never learned"),
    );
    let bytes = world
        .records
        .iter()
        .map(|(id, r)| (id.clone(), r.raw_bytes.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut selected = selection();
    selected.contexts = vec![id(14)];
    let result = read(&world, &selected, &time);
    assert!(result.to_string().contains("never learned"));
    assert_eq!(
        character::context(&world, PERSON, &time, 100000).unwrap(),
        before
    );
    assert_eq!(
        world
            .records
            .iter()
            .map(|(id, r)| (id.clone(), r.raw_bytes.clone()))
            .collect::<BTreeMap<_, _>>(),
        bytes
    );
}
