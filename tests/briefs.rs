//! Required brief material admits atomically, before counts and packet limits.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    Error, briefs, description,
    model::{Record, StoryTime, World},
    narrative, source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const VIEWER: &str = "char_00000000000000000000000001";
const OTHER: &str = "char_00000000000000000000000002";
const PLACE: &str = "loc_00000000000000000000000001";
const PLACE2: &str = "loc_00000000000000000000000002";
const BRIEF: &str = "brief_00000000000000000000000001";
fn id(prefix: &str, n: usize) -> String {
    format!("{prefix}_{n:026}")
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn point(tick: i64, order: i32) -> Json {
    json!({"timeline":"main","tick":tick,"order":order})
}
fn interval(start: i64, end: i64) -> Json {
    json!({"kind":"interval","timeline":"main","start":point(start,0),"end":point(end,0)})
}
fn rr(id: &str) -> Json {
    json!({"kind":"record","id":id})
}
fn sr(owner: &str, n: usize) -> Json {
    json!({"kind":"section","owner_id":owner,"id":id("description",n)})
}
fn tr(owner: &str, member: &str) -> Json {
    json!({"kind":"transition","owner_id":owner,"id":member})
}
fn record(kind: &str, id: &str, fields: Json, body: &str) -> Record {
    let mut data =
        json!({"schema":"wedl/v1","kind":kind,"id":id,"title":format!("AUTHOR_TITLE_{id}")});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let bytes = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&data).unwrap(),
        body
    );
    source::parse_record(
        bytes.as_bytes(),
        &format!("story/{}.md", id.replace(':', "-")),
    )
    .unwrap()
}
fn from(records: Vec<Record>) -> World {
    source::world_from_records(PathBuf::new(), records, Some("brief-fixture".into())).unwrap()
}
fn add(world: &World, record: Record) -> World {
    let mut records = world.records.values().cloned().collect::<Vec<_>>();
    records.retain(|r| r.id != record.id);
    records.push(record);
    from(records)
}
fn edit(world: &World, id: &str, f: impl FnOnce(&mut Yaml)) -> World {
    let old = &world.records[id];
    let mut data = old.frontmatter.clone();
    f(&mut data);
    let bytes = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&data).unwrap(),
        old.body
    );
    add(
        world,
        source::parse_record(bytes.as_bytes(), &old.source_path).unwrap(),
    )
}
fn set(world: &World, id: &str, key: &str, value: Json) -> World {
    edit(world, id, |data| {
        data[key] = serde_yaml::to_value(value).unwrap()
    })
}
fn valid(world: &World) {
    let report = validation::validate_world(world);
    assert!(
        report.valid,
        "{:?}",
        report
            .diagnostics
            .iter()
            .filter(|d| d.severity == "error")
            .collect::<Vec<_>>()
    );
}
fn section(n: usize, text: &str) -> Json {
    json!({"id":id("description",n),"heading":format!("Section {n}"),"markdown":text,"valid":{"kind":"timeless"},"audience":["author","public"],"perspectives":["author","ordinary"]})
}
fn transition(tick: i64, state: &str) -> Json {
    json!({"id":format!("t{tick}"),"time":point(tick,0),"state":state,"confidence":0.8,"acquisition":"heard","note":"PRIVATE_ACQUISITION_NOTE"})
}
fn knowledge(n: usize, target: &str) -> Record {
    record(
        "knowledge",
        &id("know", n),
        json!({"knower":VIEWER,"claim":{"key":format!("known-{n}"),"statement":format!("Known words {n}"),"object":{"entity":target}},"transitions":[transition(5,"accepted")]}),
        "PRIVATE_KNOWLEDGE_BODY",
    )
}
fn brief(n: usize, refs: Vec<Json>) -> Record {
    record(
        "context-brief",
        &id("brief", n),
        json!({"domain":"setting.harbor","scope":"world","valid":{"kind":"timeless"},"audience":["author","public"],"perspectives":["author","ordinary"],"required_refs":refs}),
        "EXPLICIT_BRIEF_BODY\r\n",
    )
}
fn world() -> World {
    from(vec![
        record(
            "world",
            WORLD,
            json!({"default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"state_keys":{},"capabilities":["descriptions-v1","briefs-v1"]}),
            "WORLD_BODY",
        ),
        record("character", VIEWER, json!({}), "PRIVATE_VIEWER_BODY"),
        record("character", OTHER, json!({}), "PRIVATE_OTHER_BODY"),
        record(
            "location",
            PLACE,
            json!({"descriptions":[section(1,"Required harbor words")]}),
            "PRIVATE_LOCATION_BODY",
        ),
        record(
            "location",
            PLACE2,
            json!({"descriptions":[section(1,"Required harbor words")]}),
            "OTHER_LOCATION_BODY",
        ),
        brief(1, vec![sr(PLACE, 1)]),
        knowledge(1, PLACE),
        knowledge(2, BRIEF),
    ])
}
fn author(world: &World, tick: i64, limit: usize) -> Json {
    briefs::compose(world, BRIEF, Some(&at(tick, 0)), "author", "author", limit).unwrap()
}
fn character(world: &World, tick: i64, limit: usize) -> Json {
    briefs::character(world, VIEWER, BRIEF, &at(tick, 0), limit).unwrap()
}

#[test]
fn independent_brief_capability_and_three_exact_scope_forms_validate() {
    let w = world();
    valid(&w);
    let no_lore = set(&w, BRIEF, "required_refs", json!([]));
    let no_lore = set(
        &no_lore,
        WORLD,
        "capabilities",
        json!(["descriptions-v1", "briefs-v1"]),
    );
    valid(&no_lore);
    assert_eq!(author(&no_lore, 0, 10)["count"], 1);
    let mut standalone = no_lore;
    for owner in [PLACE, PLACE2] {
        standalone = edit(&standalone, owner, |data| {
            data.as_mapping_mut()
                .unwrap()
                .remove(Yaml::String("descriptions".into()));
        });
    }
    standalone = set(&standalone, WORLD, "capabilities", json!(["briefs-v1"]));
    valid(&standalone);
    assert_eq!(author(&standalone, 0, 10)["count"], 1);
    let region = edit(&w, BRIEF, |data| {
        data["scope"] = Yaml::String("region".into());
        data["scope_ref"] = serde_yaml::to_value(rr(PLACE)).unwrap();
    });
    valid(&region);
    assert_eq!(author(&region, 5, 10)["count"], 2);
    let request: Yaml = serde_yaml::from_str(include_str!(
        "../examples/authoring/19-world-chronology.yaml"
    ))
    .unwrap();
    let eras = request["chronology"]["eras"].as_sequence().unwrap();
    let era = eras[0]["id"].as_str().unwrap();
    let chrono = edit(&w, WORLD, |data| {
        data["chronology"] = request["chronology"].clone()
    });
    let scoped = edit(&chrono, BRIEF, |data| {
        data["scope"] = Yaml::String("era".into());
        data["scope_ref"] =
            serde_yaml::to_value(json!({"kind":"section","owner_id":WORLD,"id":era})).unwrap();
    });
    valid(&scoped);
    assert_eq!(author(&scoped, 5, 10)["outcome"], "available");
    assert_eq!(character(&scoped, 5, 10)["outcome"], "unknown");
    let calendar = request["chronology"]["calendars"][0]["id"]
        .as_str()
        .unwrap();
    let wrong = set(
        &scoped,
        BRIEF,
        "scope_ref",
        json!({"kind":"section","owner_id":WORLD,"id":calendar}),
    );
    assert!(!validation::validate_world(&wrong).valid);
}

#[test]
fn source_rejects_missing_null_mixed_duplicate_and_foreign_member_pins() {
    let w = world();
    let bad = [
        ("scope", json!("other")),
        ("scope", json!([])),
        ("scope_ref", rr(PLACE)),
        ("audience", json!([])),
        ("audience", json!(["public", "author"])),
        ("perspectives", json!([true])),
        ("domain", json!("undotted")),
        ("status", json!(true)),
        ("required_refs", Json::Null),
        ("required_refs", json!([rr(PLACE), rr(PLACE)])),
        (
            "required_refs",
            json!([{"kind":"record","id":PLACE,"owner_id":PLACE}]),
        ),
        (
            "required_refs",
            json!([{"kind":"section","owner_id":VIEWER,"id":id("description",1)}]),
        ),
        ("required_refs", json!([{"kind":"record","id":"missing"}])),
    ];
    for (key, value) in bad {
        assert!(
            !validation::validate_world(&set(&w, BRIEF, key, value.clone())).valid,
            "{key}: {value}"
        );
    }
    let missing = edit(&w, BRIEF, |data| {
        data.as_mapping_mut()
            .unwrap()
            .remove(Yaml::String("required_refs".into()));
    });
    assert!(!validation::validate_world(&missing).valid);
    assert!(
        !validation::validate_world(&set(&w, WORLD, "capabilities", json!(["descriptions-v1"])))
            .valid
    );
    assert!(
        !validation::validate_world(&set(
            &w,
            BRIEF,
            "required_refs",
            json!(vec![rr(PLACE); 1025])
        ))
        .valid
    );
}

#[test]
fn authored_prose_and_opaque_metadata_are_preserved_with_a_real_body_bound() {
    let w = set(
        &world(),
        BRIEF,
        "x-notes",
        json!({"literal":"missing_looks_like_id","nested":{"chronology":"opaque"}}),
    );
    valid(&w);
    let value = author(&w, 5, 10);
    assert_eq!(
        value["items"][0]["value"]["bodyMarkdown"],
        "EXPLICIT_BRIEF_BODY\r\n"
    );
    assert!(value["items"][0]["value"].get("x-notes").is_none());
    let mut big = w.records[BRIEF].clone();
    big.body = "x".repeat(1024 * 1024 + 1);
    let fields = serde_json::to_value(&big.frontmatter).unwrap();
    let big = record("context-brief", BRIEF, fields, &big.body);
    assert!(!validation::validate_world(&add(&w, big)).valid);
}

#[test]
fn closure_keeps_authored_depth_first_order_and_exact_member_identity_across_cycles() {
    let w = add(&world(), brief(2, vec![sr(PLACE2, 1), rr(BRIEF)]));
    let w = set(
        &w,
        BRIEF,
        "required_refs",
        json!([sr(PLACE, 1), rr(&id("brief", 2)), sr(PLACE2, 1)]),
    );
    valid(&w);
    let value = author(&w, 5, 10);
    let targets = value["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["target"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        targets,
        vec![
            rr(BRIEF),
            json!({"kind":"section","ownerId":PLACE,"id":id("description",1)}),
            rr(&id("brief", 2)),
            json!({"kind":"section","ownerId":PLACE2,"id":id("description",1)})
        ]
    );
    assert_eq!(value["count"], 4);
    assert_eq!(
        value["items"][1]["value"]["markdown"],
        value["items"][3]["value"]["markdown"]
    );
}

#[test]
fn hidden_late_pin_refuses_atomically_before_result_limit_and_private_details() {
    let w = set(
        &world(),
        BRIEF,
        "required_refs",
        json!([sr(PLACE, 1), rr(PLACE2)]),
    );
    let w = set(&w, PLACE2, "audience", json!(["private"]));
    valid(&w);
    let denied = author(&w, 5, 1);
    assert_eq!(denied["outcome"], "unknown");
    assert_eq!(denied["items"], json!([]));
    let changed = set(&w, PLACE2, "title", json!("NEW_PRIVATE_TITLE"));
    assert_eq!(author(&changed, 5, 1), denied);
    assert!(!denied.to_string().contains("PRIVATE"));
    let available = set(&w, PLACE2, "audience", json!(["author"]));
    assert!(matches!(
        briefs::compose(&available, BRIEF, Some(&at(5, 0)), "author", "author", 1),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn selecting_brief_or_pin_never_grants_character_learning() {
    let w = world();
    valid(&w);
    assert_eq!(character(&w, 4, 10)["outcome"], "unknown");
    let value = character(&w, 5, 10);
    assert_eq!(value["count"], 2);
    assert!(value.to_string().contains("EXPLICIT_BRIEF_BODY"));
    for forbidden in [
        "PRIVATE_LOCATION_BODY",
        "PRIVATE_VIEWER_BODY",
        "PRIVATE_ACQUISITION_NOTE",
        "sourcePath",
        "fingerprint",
        "revision",
    ] {
        assert!(!value.to_string().contains(forbidden), "{forbidden}");
    }
    assert!(
        value["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["citations"] == json!([]))
    );
    let unknown = set(&w, BRIEF, "required_refs", json!([sr(PLACE2, 1)]));
    assert_eq!(character(&unknown, 5, 10)["outcome"], "unknown");
    let grant = add(&unknown, knowledge(3, PLACE2));
    assert_eq!(character(&grant, 5, 10)["outcome"], "available");
    let hidden = set(&grant, &id("know", 3), "perspectives", json!(["author"]));
    assert_eq!(character(&hidden, 5, 10)["outcome"], "unknown");
}

#[test]
fn ordinary_character_record_pins_expose_identity_without_untimed_title_or_body() {
    let w = set(
        &world(),
        BRIEF,
        "required_refs",
        json!([rr(PLACE), rr(VIEWER)]),
    );
    valid(&w);
    let value = character(&w, 5, 10);
    assert_eq!(
        value["items"][1]["value"],
        json!({"recordId":PLACE,"kind":"location"})
    );
    let before = value.clone();
    let w = set(&w, PLACE, "title", json!("CHANGED_PRIVATE_RECORD_TITLE"));
    assert_eq!(character(&w, 5, 10), before);
    assert!(!value.to_string().contains("PRIVATE_LOCATION_BODY"));
}

#[test]
fn own_brief_status_validity_and_scopes_gate_generic_sections_and_required_composition() {
    let w = set(
        &world(),
        BRIEF,
        "descriptions",
        json!([section(3, "Brief-owned section")]),
    );
    for patch in [
        json!({"status":"draft"}),
        json!({"status":"retired"}),
        json!({"valid":interval(10,20)}),
    ] {
        let changed = edit(&w, BRIEF, |data| {
            for (k, v) in patch.as_object().unwrap() {
                data[k.as_str()] = serde_yaml::to_value(v).unwrap();
            }
        });
        valid(&changed);
        assert_eq!(author(&changed, 5, 10)["outcome"], "unknown");
        assert_eq!(character(&changed, 5, 10)["outcome"], "unknown");
        assert!(
            description::list(&changed, BRIEF, Some(&at(5, 0)), "author", "author", 10).is_err()
        );
        assert!(narrative::descriptions(&changed, VIEWER, BRIEF, &at(5, 0), 10).is_err());
    }
}

#[test]
fn section_corrections_and_exact_member_availability_run_before_pin_composition() {
    let w = world();
    let old = section(1, "Old words");
    let mut current = section(2, "Corrected words");
    current["replaces"] = json!(id("description", 1));
    let w = set(&w, PLACE, "descriptions", json!([old, current]));
    valid(&w);
    assert_eq!(author(&w, 5, 10)["outcome"], "unknown");
    assert_eq!(character(&w, 5, 10)["outcome"], "unknown");
    let selected = set(&w, BRIEF, "required_refs", json!([sr(PLACE, 2)]));
    assert_eq!(
        character(&selected, 5, 10)["items"][1]["value"]["markdown"],
        "Corrected words"
    );
    assert_eq!(
        briefs::compose(&w, BRIEF, None, "author", "author", 10).unwrap()["outcome"],
        "available"
    );
}

fn assertions_world() -> World {
    let w = set(
        &world(),
        WORLD,
        "capabilities",
        json!([
            "lore-core-v1",
            "descriptions-v1",
            "assertions-v1",
            "briefs-v1"
        ]),
    );
    let w = set(&w, WORLD, "profiles", json!([]));
    let w = set(
        &w,
        WORLD,
        "predicates",
        json!([{"id":"claim","subject_kinds":["location"],"value_type":{"type":"string"}}]),
    );
    let assertion = record(
        "assertion",
        &id("assertion", 1),
        json!({"domain":"claims.harbor","subject":rr(PLACE),"predicate":"claim","value":"PRIVATE_CANONICAL_VALUE","classification":"author-fact","valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"],"evidence":[]}),
        "PRIVATE_ASSERTION_BODY",
    );
    let w = add(&w, assertion);
    let learned = record(
        "knowledge",
        &id("know", 3),
        json!({"knower":VIEWER,"claim":{"key":"heard","statement":"Mistaken learned ferry words","assertion_id":id("assertion",1)},"transitions":[transition(4,"rejected"),transition(5,"suspected"),transition(6,"rejected"),transition(7,"forgotten"),transition(8,"remembered")]}),
        "PRIVATE_LINKED_BODY",
    );
    let w = add(&w, learned);
    set(
        &w,
        BRIEF,
        "required_refs",
        json!([rr(&id("assertion", 1)), tr(&id("know", 3), "t5")]),
    )
}

#[test]
fn canonical_assertion_pins_expand_only_learned_words_and_dedup_the_actual_transition() {
    let w = assertions_world();
    valid(&w);
    let value = character(&w, 5, 10);
    assert_eq!(value["count"], 2);
    assert_eq!(
        value["items"][1]["target"],
        json!({"kind":"transition","ownerId":id("know",3),"id":"t5"})
    );
    assert!(value.to_string().contains("Mistaken learned ferry words"));
    assert!(!value.to_string().contains("PRIVATE_CANONICAL_VALUE"));
    assert!(!value.to_string().contains("PRIVATE_ASSERTION_BODY"));
    let changed = set(
        &w,
        &id("assertion", 1),
        "value",
        json!("Changed canonical truth"),
    );
    assert_eq!(character(&changed, 5, 10), value);
    assert_eq!(character(&w, 7, 10)["outcome"], "unknown");
    assert_eq!(
        character(&w, 6, 10)["items"][1]["value"]["state"],
        "rejected"
    );
    assert_eq!(character(&w, 8, 10)["outcome"], "available");
}

#[test]
fn historic_knowledge_member_requires_current_holding_and_affirmative_learning_by_that_member() {
    let w = assertions_world();
    let early = set(
        &w,
        BRIEF,
        "required_refs",
        json!([tr(&id("know", 3), "t4")]),
    );
    assert_eq!(character(&early, 6, 10)["outcome"], "unknown");
    let selected = set(
        &w,
        BRIEF,
        "required_refs",
        json!([tr(&id("know", 3), "t5")]),
    );
    assert_eq!(
        character(&selected, 6, 10)["items"][1]["value"]["state"],
        "suspected"
    );
    assert_eq!(character(&selected, 7, 10)["outcome"], "unknown");
    assert_eq!(
        character(&selected, 8, 10)["items"][1]["value"]["transitionId"],
        "t5"
    );
    assert_eq!(
        character(&selected, 8, 10)["items"][1]["value"]["time"]["tick"],
        "5"
    );
}

#[test]
fn knowing_someone_elses_knowledge_id_never_exposes_their_claim_or_transition() {
    let w = world();
    let other = record(
        "knowledge",
        &id("know", 3),
        json!({"knower":OTHER,"claim":{"key":"other","statement":"PRIVATE_OTHER_CLAIM"},"transitions":[transition(5,"accepted")]}),
        "PRIVATE_OTHER_KNOWLEDGE_BODY",
    );
    let w = add(&add(&w, other), knowledge(4, &id("know", 3)));
    for target in [rr(&id("know", 3)), tr(&id("know", 3), "t5")] {
        let selected = set(&w, BRIEF, "required_refs", json!([target]));
        valid(&selected);
        assert_eq!(character(&selected, 5, 10)["outcome"], "unknown");
    }
}

#[test]
fn canonical_generational_pins_and_sections_require_initialization_and_historical_labels() {
    let w = set(
        &world(),
        WORLD,
        "capabilities",
        json!(["generational-core-v1", "descriptions-v1", "briefs-v1"]),
    );
    let organization = record(
        "organization",
        &id("organization", 1),
        json!({"organization_kind":"institution","initialization":{"transition_id":id("transition",1),"transition_kind":"organization-initialize","applicability":{"applicability_kind":"instant","point":point(20,0)},"payload":{"title":"Historical guild name","aliases":[]}},"transitions":[],"descriptions":[section(1,"Guild section")]}),
        "LATER_CANONICAL_BODY",
    );
    let w = add(&w, organization);
    for target in [
        rr(&id("organization", 1)),
        tr(&id("organization", 1), &id("transition", 1)),
        sr(&id("organization", 1), 1),
    ] {
        let selected = set(&w, BRIEF, "required_refs", json!([target]));
        valid(&selected);
        assert_eq!(author(&selected, 19, 10)["outcome"], "unknown");
        let value = author(&selected, 20, 10);
        assert_eq!(value["outcome"], "available");
        assert!(!value.to_string().contains("AUTHOR_TITLE_organization"));
        assert!(!value.to_string().contains("LATER_CANONICAL_BODY"));
    }
}

#[test]
fn exact_large_coordinates_and_inclusive_bounds_remain_independent_of_era_scope() {
    let w = set(
        &world(),
        BRIEF,
        "valid",
        interval(9_007_199_254_740_993, 9_007_199_254_740_994),
    );
    valid(&w);
    assert_eq!(author(&w, 9_007_199_254_740_992, 10)["outcome"], "unknown");
    let available = author(&w, 9_007_199_254_740_993, 10);
    assert_eq!(
        available["items"][0]["value"]["valid"]["start"]["tick"],
        "9007199254740993"
    );
    assert_eq!(
        author(&w, 9_007_199_254_740_994, 10)["outcome"],
        "available"
    );
    assert_eq!(
        briefs::compose(
            &w,
            BRIEF,
            Some(&at(9_007_199_254_740_994, 1)),
            "author",
            "author",
            10
        )
        .unwrap()["outcome"],
        "unknown"
    );
    assert_eq!(
        briefs::compose(
            &w,
            BRIEF,
            Some(&StoryTime::new("other", 9_007_199_254_740_993, 0).unwrap()),
            "author",
            "author",
            10
        )
        .unwrap()["outcome"],
        "unknown"
    );
}

#[test]
fn invalid_captured_bytes_fields_and_fingerprint_fail_with_generic_character_errors() {
    let original = world();
    for mode in 0..3 {
        let mut w = original.clone();
        if mode == 0 {
            w.records
                .get_mut(PLACE)
                .unwrap()
                .raw_bytes
                .extend_from_slice(b"stale");
        } else if mode == 1 {
            w.records.get_mut(PLACE).unwrap().frontmatter["title"] =
                Yaml::String("Changed without bytes".into());
        } else {
            w.fingerprint = "forged".into();
        }
        for error in [
            briefs::compose(&w, BRIEF, Some(&at(5, 0)), "author", "author", 10).unwrap_err(),
            briefs::character(&w, VIEWER, BRIEF, &at(5, 0), 10).unwrap_err(),
        ] {
            assert!(matches!(error, Error::Validation(_)));
            assert!(!error.to_string().contains("Changed without bytes"));
            assert!(!error.to_string().contains(PLACE));
        }
    }
}

#[test]
fn unavailable_brief_identity_and_limit_arguments_are_closed() {
    let w = world();
    let unknown = briefs::compose(
        &w,
        &id("brief", 999),
        Some(&at(5, 0)),
        "author",
        "author",
        10,
    )
    .unwrap();
    assert_eq!(unknown["outcome"], "unknown");
    assert_eq!(unknown["count"], 0);
    for limit in [0, 1001] {
        assert!(briefs::compose(&w, BRIEF, Some(&at(5, 0)), "author", "author", limit).is_err());
    }
    for label in ["", "author\npublic"] {
        assert!(briefs::compose(&w, BRIEF, Some(&at(5, 0)), label, "author", 10).is_err());
    }
    assert!(briefs::compose(&w, "A title", Some(&at(5, 0)), "author", "author", 10).is_err());
}

#[test]
fn known_typed_lore_pin_prunes_unlearned_endpoints_before_output() {
    let w = set(
        &world(),
        WORLD,
        "capabilities",
        json!(["lore-core-v1", "descriptions-v1", "briefs-v1"]),
    );
    let w = set(
        &w,
        WORLD,
        "profiles",
        json!([{"id":"observed","label":"Author schema","fields":{"type":"object","required":{"amount":{"type":"number"},"targets":{"type":"array","max_items":8,"items":{"type":"entity","kinds":["location"]}}},"optional":{}}}]),
    );
    let concept = id("concept", 1);
    let w = add(
        &w,
        record(
            "lore-concept",
            &concept,
            json!({"domain":"setting.harbor","profile":"observed","valid":{"kind":"timeless"},"audience":["author","public"],"perspectives":["author","ordinary"],"values":{"amount":9007199254740993i64,"targets":[rr(PLACE2),rr(PLACE)]}}),
            "PRIVATE_CONCEPT_BODY",
        ),
    );
    let w = add(&w, knowledge(3, &concept));
    let w = set(&w, BRIEF, "required_refs", json!([rr(&concept)]));
    valid(&w);
    let value = character(&w, 5, 10);
    assert_eq!(
        value["items"][1]["value"]["values"]["amount"],
        "9007199254740993"
    );
    assert_eq!(
        value["items"][1]["value"]["values"]["targets"],
        json!([rr(PLACE)])
    );
    assert!(!value.to_string().contains("PRIVATE_CONCEPT_BODY"));
    assert!(!value.to_string().contains("sourceField"));
    let changed = set(&w, PLACE2, "title", json!("Changed unavailable endpoint"));
    assert_eq!(character(&changed, 5, 10), value);
}

#[test]
fn genealogy_knowledge_pin_keeps_its_own_learned_payload_and_expiry() {
    let w = set(
        &world(),
        WORLD,
        "capabilities",
        json!([
            "generational-core-v1",
            "generational-knowledge-v1",
            "descriptions-v1",
            "briefs-v1"
        ]),
    );
    let learned = record(
        "knowledge",
        &id("know", 3),
        json!({"knower":VIEWER,"claim":{"key":"PRIVATE_GENEALOGY_KEY","statement":"PRIVATE_AUTHOR_STATEMENT","genealogy":{"kind":"parentage","payload":{"child_id":VIEWER,"parent_id":OTHER,"basis":"biological"},"valid":{"from":point(0,0),"until":point(8,0)},"labels":{VIEWER:"Learned child name",OTHER:"Learned parent name"}}},"transitions":[transition(5,"suspected"),transition(7,"rejected")]}),
        "PRIVATE_GENEALOGY_BODY",
    );
    let w = add(&w, learned);
    let w = set(
        &w,
        BRIEF,
        "required_refs",
        json!([tr(&id("know", 3), "t5")]),
    );
    valid(&w);
    let value = character(&w, 5, 10);
    assert_eq!(value["outcome"], "available");
    assert_eq!(
        value["items"][1]["value"]["labels"][OTHER],
        "Learned parent name"
    );
    for forbidden in [
        "PRIVATE_GENEALOGY_KEY",
        "PRIVATE_AUTHOR_STATEMENT",
        "PRIVATE_GENEALOGY_BODY",
        "AUTHOR_TITLE_char",
    ] {
        assert!(!value.to_string().contains(forbidden));
    }
    assert_eq!(character(&w, 7, 10)["outcome"], "unknown");
    assert_eq!(character(&w, 9, 10)["outcome"], "unknown");
}

#[test]
fn existence_pin_uses_native_author_episode_and_never_treats_pointer_as_character_payload() {
    let w = set(
        &world(),
        WORLD,
        "capabilities",
        json!([
            "generational-core-v1",
            "descriptions-v1",
            "existence-v1",
            "briefs-v1"
        ]),
    );
    let source: Yaml = serde_yaml::from_str(include_str!(
        "../examples/authoring/127-embodied-episode.yaml"
    ))
    .unwrap();
    let mut fields = serde_json::to_value(source).unwrap();
    fields.as_object_mut().unwrap().remove("body");
    let episode = id("existence", 1);
    let w = add(
        &w,
        record(
            "existence-episode",
            &episode,
            fields,
            "PRIVATE_EPISODE_BODY",
        ),
    );
    let w = add(&w, knowledge(3, &episode));
    let w = set(&w, BRIEF, "required_refs", json!([rr(&episode)]));
    valid(&w);
    let value = author(&w, 5, 10);
    assert_eq!(value["items"][1]["value"]["episodeKind"], "embodied");
    assert_eq!(author(&w, 10, 10)["outcome"], "unknown");
    assert_eq!(character(&w, 5, 10)["outcome"], "unknown");
    assert!(!value.to_string().contains("PRIVATE_EPISODE_BODY"));
}

#[test]
fn historical_learning_confidence_keeps_probability_numbers_and_requires_source_field() {
    let w = assertions_world();
    let w = set(
        &w,
        BRIEF,
        "required_refs",
        json!([tr(&id("know", 3), "t5")]),
    );
    for confidence in [0, 1] {
        let changed = edit(&w, &id("know", 3), |data| {
            data["transitions"][1]["confidence"] = serde_yaml::to_value(confidence).unwrap()
        });
        valid(&changed);
        let value = character(&changed, 6, 10);
        assert_eq!(
            value["items"][1]["value"]["confidence"].as_f64(),
            Some(f64::from(confidence))
        );
        assert!(!value["items"][1]["value"]["confidence"].is_string());
    }
    let missing = edit(&w, &id("know", 3), |data| {
        data["transitions"][1]
            .as_mapping_mut()
            .unwrap()
            .remove(Yaml::String("confidence".into()));
    });
    assert!(!validation::validate_world(&missing).valid);
    assert!(matches!(
        briefs::character(&missing, VIEWER, BRIEF, &at(6, 0), 10),
        Err(Error::Validation(_))
    ));
}

#[test]
fn one_exact_episode_pin_does_not_spend_its_limit_on_unrelated_episodes() {
    let w = set(
        &world(),
        WORLD,
        "capabilities",
        json!([
            "generational-core-v1",
            "descriptions-v1",
            "existence-v1",
            "briefs-v1"
        ]),
    );
    let mut records = w.records.values().cloned().collect::<Vec<_>>();
    for n in 1..=1001 {
        records.push(record("existence-episode",&id("existence",n),json!({"domain":"character.existence","character_id":VIEWER,"episode_kind":"unknown","valid":{"kind":"interval","timeline":"main"},"start_evidence":[],"audience":["author"],"perspectives":["author"]}),""));
    }
    let w = set(
        &from(records),
        BRIEF,
        "required_refs",
        json!([rr(&id("existence", 1))]),
    );
    valid(&w);
    assert!(matches!(
        wedl::existence::list(&w, VIEWER, Some(&at(5, 0)), "author", "author", 1000),
        Err(Error::InvalidInput(_))
    ));
    let value = author(&w, 5, 2);
    assert_eq!(value["count"], 2);
    assert_eq!(value["items"][1]["value"]["episodeId"], id("existence", 1));
}

#[test]
fn typed_author_endpoints_cannot_bypass_a_briefs_own_applicability() {
    let w = set(
        &world(),
        WORLD,
        "capabilities",
        json!(["lore-core-v1", "descriptions-v1", "briefs-v1"]),
    );
    let w = set(
        &w,
        WORLD,
        "profiles",
        json!([{"id":"brief-link","label":"Brief link","fields":{"type":"object","required":{"setting":{"type":"entity","kinds":["context-brief"]}},"optional":{}}}]),
    );
    let concept = id("concept", 1);
    let w = add(
        &w,
        record(
            "lore-concept",
            &concept,
            json!({"domain":"setting.harbor","profile":"brief-link","valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"],"values":{"setting":rr(BRIEF)}}),
            "",
        ),
    );
    valid(&w);
    assert_eq!(
        wedl::lore::concept(&w, &concept, Some(&at(5, 0)), "author", "author").unwrap()["values"]["setting"],
        rr(BRIEF)
    );
    for patch in [
        json!({"status":"draft"}),
        json!({"status":"retired"}),
        json!({"valid":interval(10,20)}),
        json!({"audience":["private"]}),
    ] {
        let changed = edit(&w, BRIEF, |data| {
            for (k, v) in patch.as_object().unwrap() {
                data[k.as_str()] = serde_yaml::to_value(v).unwrap();
            }
        });
        valid(&changed);
        let value =
            wedl::lore::concept(&changed, &concept, Some(&at(5, 0)), "author", "author").unwrap();
        assert!(value["values"].get("setting").is_none());
        assert_eq!(value["references"], json!([]));
    }
}

#[test]
fn full_author_closure_unites_independent_owner_member_and_nested_brief_labels() {
    let w = set(&world(), BRIEF, "audience", json!(["public"]));
    let w = set(&w, BRIEF, "perspectives", json!(["ordinary"]));
    let w = set(&w, PLACE, "audience", json!(["author"]));
    let w = set(&w, PLACE, "perspectives", json!(["author"]));
    let mut member = section(1, "Required public member in an author owner");
    member["audience"] = json!(["public"]);
    member["perspectives"] = json!(["ordinary"]);
    let w = set(&w, PLACE, "descriptions", json!([member]));
    let w = add(&w, brief(2, vec![sr(PLACE, 1), rr(BRIEF)]));
    let w = set(&w, &id("brief", 2), "audience", json!(["author"]));
    let w = set(
        &w,
        BRIEF,
        "required_refs",
        json!([rr(&id("brief", 2)), sr(PLACE, 1)]),
    );
    valid(&w);
    for audience in ["author", "public"] {
        for view in ["author", "ordinary"] {
            assert_eq!(
                briefs::compose(&w, BRIEF, Some(&at(5, 0)), audience, view, 10).unwrap()["outcome"],
                "unknown"
            );
        }
    }
    let value = briefs::compose_full_author(&w, BRIEF, Some(&at(5, 0)), 3).unwrap();
    assert_eq!(value["outcome"], "available");
    assert_eq!(value["count"], 3);
    assert_eq!(value["items"][1]["target"], rr(&id("brief", 2)));
    assert_eq!(
        value["items"][2]["value"]["markdown"],
        "Required public member in an author owner"
    );
    assert!(briefs::compose_full_author(&w, BRIEF, Some(&at(5, 0)), 2).is_err());
    assert_eq!(character(&w, 5, 10)["outcome"], "unknown");
}

#[test]
fn full_author_late_withheld_pin_closes_whole_union_before_limit() {
    let w = set(
        &world(),
        BRIEF,
        "required_refs",
        json!([rr(PLACE), rr(PLACE2)]),
    );
    let w = set(&w, PLACE, "audience", json!(["public"]));
    let w = set(&w, PLACE2, "audience", json!(["private"]));
    valid(&w);
    let value = briefs::compose_full_author(&w, BRIEF, Some(&at(5, 0)), 1).unwrap();
    assert_eq!(value["outcome"], "unknown");
    assert_eq!(value["count"], 0);
    assert_eq!(value["items"], json!([]));
    assert!(!value.to_string().contains(PLACE2));
    let changed = set(&w, PLACE2, "title", json!("A hidden title changed"));
    assert_eq!(
        briefs::compose_full_author(&changed, BRIEF, Some(&at(5, 0)), 1).unwrap(),
        value
    );
}

#[test]
fn full_author_typed_endpoints_are_independently_admitted_without_string_pruning() {
    let w = set(
        &world(),
        WORLD,
        "capabilities",
        json!(["lore-core-v1", "descriptions-v1", "briefs-v1"]),
    );
    let w = set(
        &w,
        WORLD,
        "profiles",
        json!([{"id":"union","label":"Union","fields":{"type":"object","required":{"literal":{"type":"string"},"targets":{"type":"array","max_items":8,"items":{"type":"entity","kinds":["location"]}}},"optional":{}}}]),
    );
    let concept = id("concept", 1);
    let w = add(
        &w,
        record(
            "lore-concept",
            &concept,
            json!({"domain":"setting.harbor","profile":"union","valid":{"kind":"timeless"},"audience":["public"],"perspectives":["ordinary"],"values":{"literal":PLACE2,"targets":[rr(PLACE2),rr(PLACE)]}}),
            "Concept prose",
        ),
    );
    let w = set(&w, PLACE, "audience", json!(["author"]));
    let w = set(&w, PLACE, "perspectives", json!(["author"]));
    let w = set(&w, PLACE2, "audience", json!(["private"]));
    let w = set(&w, BRIEF, "required_refs", json!([rr(&concept)]));
    valid(&w);
    let result = briefs::compose_full_author(&w, BRIEF, Some(&at(5, 0)), 10).unwrap();
    let value = &result["items"][1]["value"];
    assert_eq!(value["values"]["literal"], PLACE2);
    assert_eq!(value["values"]["targets"], json!([rr(PLACE)]));
    assert_eq!(
        value["references"][0]["sourceField"],
        "values.targets[1].id"
    );
    assert_eq!(value["references"][0]["entityId"], PLACE);
}

#[test]
fn full_author_assertions_admit_subjects_and_values_from_other_union_labels() {
    let w = assertions_world();
    let assertion = id("assertion", 1);
    let w = set(
        &w,
        WORLD,
        "predicates",
        json!([{"id":"claim","subject_kinds":["location"],"value_type":{"type":"entity","kinds":["location"]}}]),
    );
    let w = set(&w, &assertion, "value", rr(PLACE2));
    let w = set(&w, PLACE, "audience", json!(["public"]));
    let w = set(&w, PLACE2, "perspectives", json!(["ordinary"]));
    let w = set(&w, BRIEF, "required_refs", json!([rr(&assertion)]));
    valid(&w);
    assert_eq!(author(&w, 5, 10)["outcome"], "unknown");
    let full = briefs::compose_full_author(&w, BRIEF, Some(&at(5, 0)), 10).unwrap();
    assert_eq!(full["outcome"], "available");
    assert_eq!(full["items"][1]["value"]["value"], rr(PLACE2));
    let hidden = set(&w, PLACE2, "audience", json!(["private"]));
    assert_eq!(
        briefs::compose_full_author(&hidden, BRIEF, Some(&at(5, 0)), 1).unwrap()["outcome"],
        "unknown"
    );
}

#[test]
fn full_author_episode_and_character_endpoints_use_independent_labels() {
    let w = set(
        &world(),
        WORLD,
        "capabilities",
        json!([
            "generational-core-v1",
            "descriptions-v1",
            "existence-v1",
            "briefs-v1"
        ]),
    );
    let episode = id("existence", 1);
    let w = add(
        &w,
        record(
            "existence-episode",
            &episode,
            json!({"domain":"character.existence","character_id":VIEWER,"episode_kind":"unknown","valid":{"kind":"interval","timeline":"main"},"start_evidence":[],"audience":["public"],"perspectives":["ordinary"]}),
            "UNSCOPED_EPISODE_BODY",
        ),
    );
    let w = set(&w, VIEWER, "audience", json!(["author"]));
    let w = set(&w, VIEWER, "perspectives", json!(["author"]));
    let w = set(&w, BRIEF, "required_refs", json!([rr(&episode)]));
    valid(&w);
    let full = briefs::compose_full_author(&w, BRIEF, Some(&at(5, 0)), 10).unwrap();
    assert_eq!(full["outcome"], "available");
    assert_eq!(full["items"][1]["value"]["characterId"], VIEWER);
    assert!(!full.to_string().contains("UNSCOPED_EPISODE_BODY"));
    let hidden = set(&w, VIEWER, "audience", json!(["private"]));
    assert_eq!(
        briefs::compose_full_author(&hidden, BRIEF, Some(&at(5, 0)), 10).unwrap()["outcome"],
        "unknown"
    );
}

#[test]
fn ordinary_required_record_bodies_obey_exact_event_environment_and_aggregate_times() {
    let event = id("event", 10);
    let environment = id("env", 10);
    let scene = id("scene", 10);
    let conversation = id("conv", 10);
    let mut w = world();
    for (kind, identifier, fields) in [
        ("event", &event, json!({"time":point(5, 1),"effects":[]})),
        (
            "environment",
            &environment,
            json!({"targets":[PLACE],"time":{"start":point(5,1),"end":point(6,0)}}),
        ),
        (
            "scene",
            &scene,
            json!({"status":"closed","location":PLACE,"time":{"start":point(0,0),"current":point(6,0),"end":point(6,0)},"participants":[]}),
        ),
        (
            "conversation",
            &conversation,
            json!({"status":"closed","location":PLACE,"time":{"start":point(0,0),"end":point(6,0)},"participants":[],"turns":[],"recollections":[]}),
        ),
    ] {
        w = add(
            &w,
            record(kind, identifier, fields, &format!("{kind}_DATED_BODY")),
        );
    }
    valid(&w);
    for (identifier, horizon, included) in [
        (&event, at(5, 0), false),
        (&event, at(5, 1), true),
        (&environment, at(5, 0), false),
        (&environment, at(6, 0), true),
        (&environment, at(6, 1), false),
        (&scene, at(5, 1), false),
        (&scene, at(6, 0), true),
        (&conversation, at(5, 1), false),
        (&conversation, at(6, 0), true),
        (&event, StoryTime::new("other", 10, 0).unwrap(), false),
    ] {
        let selected = set(&w, BRIEF, "required_refs", json!([rr(identifier)]));
        for value in [
            briefs::compose(&selected, BRIEF, Some(&horizon), "author", "author", 10).unwrap(),
            briefs::compose_full_author(&selected, BRIEF, Some(&horizon), 10).unwrap(),
        ] {
            assert_eq!(value["outcome"], "available");
            assert_eq!(value["items"][1]["value"]["bodyIncluded"], included);
            assert_eq!(
                value["items"][1]["value"]["bodyMarkdown"],
                if included {
                    w.records[identifier].body.as_str()
                } else {
                    ""
                }
            );
        }
        let all = briefs::compose_full_author(&selected, BRIEF, None, 10).unwrap();
        assert_eq!(
            all["items"][1]["value"]["bodyMarkdown"],
            w.records[identifier].body
        );
    }
}

#[test]
fn ordinary_required_knowledge_body_waits_for_first_transition_without_granting_learning() {
    let knowledge_id = id("know", 1);
    let w = set(&world(), BRIEF, "required_refs", json!([rr(&knowledge_id)]));
    for (tick, included) in [(4, false), (5, true)] {
        let full = briefs::compose_full_author(&w, BRIEF, Some(&at(tick, 0)), 10).unwrap();
        assert_eq!(full["items"][1]["value"]["bodyIncluded"], included);
        if tick == 5 {
            assert!(
                !character(&w, tick, 10)
                    .to_string()
                    .contains("PRIVATE_KNOWLEDGE_BODY")
            );
        }
    }
}

#[test]
fn full_author_corrections_keep_exact_section_pins_and_all_time_history() {
    let w = set(&world(), PLACE, "audience", json!(["public"]));
    let mut old = section(1, "Superseded author section");
    old["audience"] = json!(["author"]);
    old["perspectives"] = json!(["author"]);
    let mut current = old.clone();
    current["id"] = json!(id("description", 2));
    current["heading"] = json!("Current author section");
    current["markdown"] = json!("Replacement author prose");
    current["replaces"] = json!(id("description", 1));
    let w = set(&w, PLACE, "descriptions", json!([old, current]));
    valid(&w);
    assert_eq!(
        briefs::compose_full_author(&w, BRIEF, Some(&at(5, 0)), 10).unwrap()["outcome"],
        "unknown"
    );
    let historic = briefs::compose_full_author(&w, BRIEF, None, 10).unwrap();
    assert_eq!(
        historic["items"][1]["value"]["markdown"],
        "Superseded author section"
    );
    let selected = set(&w, BRIEF, "required_refs", json!([sr(PLACE, 2)]));
    let result = briefs::compose_full_author(&selected, BRIEF, Some(&at(5, 0)), 10).unwrap();
    assert_eq!(result["items"][1]["value"]["ownerId"], PLACE);
    assert_eq!(
        result["items"][1]["value"]["markdown"],
        "Replacement author prose"
    );
    assert_eq!(
        result["items"][1]["citations"][0]["section"],
        "descriptions[1]"
    );
}

#[test]
fn story_point_transition_pins_resolve_nested_lifecycle_with_exact_time_and_source_ordinal() {
    let point_id = id("sp", 1);
    let tick = 9_007_199_254_740_993_i64;
    let w = add(
        &world(),
        record(
            "story-point",
            &point_id,
            json!({"activation_policy":"manual","lifecycle":{"initial_state":"dormant","transitions":[
                {"id":"earlier","time":point(tick,-1),"state":"active","note":"Earlier lifecycle words"},
                {"id":"selected","time":point(tick,0),"state":"resolved","note":"Exact selected lifecycle words"},
                {"id":"future","time":point(tick,1),"state":"cancelled","note":"Future lifecycle words"}
            ]}}),
            "Unrelated story-point body",
        ),
    );
    let selected = set(
        &w,
        BRIEF,
        "required_refs",
        json!([tr(&point_id, "selected")]),
    );
    valid(&selected);
    for (horizon, available) in [
        (at(tick, -1), false),
        (at(tick, 0), true),
        (at(tick, 1), true),
        (StoryTime::new("other", tick, 0).unwrap(), false),
    ] {
        for result in [
            briefs::compose(&selected, BRIEF, Some(&horizon), "author", "author", 10).unwrap(),
            briefs::compose_full_author(&selected, BRIEF, Some(&horizon), 10).unwrap(),
        ] {
            assert_eq!(
                result["outcome"],
                if available { "available" } else { "unknown" }
            );
            if !available {
                assert_eq!(result["items"], json!([]));
                continue;
            }
            let item = &result["items"][1];
            assert_eq!(
                item["target"],
                json!({"kind":"transition","ownerId":point_id,"id":"selected"})
            );
            assert_eq!(item["value"]["id"], "selected");
            assert_eq!(item["value"]["state"], "resolved");
            assert_eq!(item["value"]["time"]["tick"], tick.to_string());
            assert_eq!(item["value"]["time"]["order"], "0");
            assert_eq!(item["value"]["note"], "Exact selected lifecycle words");
            assert_eq!(item["citations"][0]["section"], "lifecycle.transitions[1]");
            assert!(!result.to_string().contains("Future lifecycle words"));
        }
    }
    let future = set(&w, BRIEF, "required_refs", json!([tr(&point_id, "future")]));
    let before_future = at(tick, 0);
    let at_future = at(tick, 1);
    for full in [false, true] {
        let compose = |at| {
            if full {
                briefs::compose_full_author(&future, BRIEF, at, 10).unwrap()
            } else {
                briefs::compose(&future, BRIEF, at, "author", "author", 10).unwrap()
            }
        };
        assert_eq!(compose(Some(&before_future))["outcome"], "unknown");
        for result in [compose(Some(&at_future)), compose(None)] {
            assert_eq!(result["outcome"], "available");
            assert_eq!(
                result["items"][1]["citations"][0]["section"],
                "lifecycle.transitions[2]"
            );
        }
    }
}

#[test]
fn story_point_member_resolution_does_not_adopt_top_level_transition_metadata() {
    let point_id = id("sp", 1);
    let w = add(
        &world(),
        record(
            "story-point",
            &point_id,
            json!({"activation_policy":"manual","lifecycle":{"initial_state":"dormant","transitions":[
                {"id":"selected","time":point(5,0),"state":"active","note":"Nested operational member"}
            ]},"transitions":[
                {"id":"selected","time":point(0,0),"state":"failed","note":"OPAQUE_TOP_LEVEL_DECOY"},
                {"id":"top-level-only","time":point(0,0),"state":"failed"}
            ]}),
            "",
        ),
    );
    let selected = set(
        &w,
        BRIEF,
        "required_refs",
        json!([tr(&point_id, "selected")]),
    );
    valid(&selected);
    for result in [
        briefs::compose(&selected, BRIEF, Some(&at(5, 0)), "author", "author", 10).unwrap(),
        briefs::compose_full_author(&selected, BRIEF, Some(&at(5, 0)), 10).unwrap(),
    ] {
        assert_eq!(result["items"][1]["value"]["state"], "active");
        assert_eq!(
            result["items"][1]["citations"][0]["section"],
            "lifecycle.transitions[0]"
        );
        assert!(!result.to_string().contains("OPAQUE_TOP_LEVEL_DECOY"));
    }
    let only_opaque = set(
        &w,
        BRIEF,
        "required_refs",
        json!([tr(&point_id, "top-level-only")]),
    );
    assert!(!validation::validate_world(&only_opaque).valid);
}
