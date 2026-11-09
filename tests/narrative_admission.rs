//! Character narrative projections derive known roots before values, corrections and bounds.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    Error, character,
    model::{Record, StoryTime, World},
    narrative, source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const VIEWER: &str = "char_00000000000000000000000001";
const OTHER: &str = "char_00000000000000000000000002";
const PLACE: &str = "loc_00000000000000000000000001";
const HIDDEN: &str = "loc_00000000000000000000000002";
const CONCEPT: &str = "concept_00000000000000000000000001";
const OTHER_CONCEPT: &str = "concept_00000000000000000000000002";
const EPISODE: &str = "existence_00000000000000000000000001";
const ASSERTION: &str = "assertion_00000000000000000000000001";
const MAP: &str = "map:known";
const HIDDEN_MAP: &str = "map:unlearned";
const ROUTE: &str = "route:known";

fn id(prefix: &str, number: usize) -> String {
    format!("{prefix}_{number:026}")
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
fn reference(id: &str) -> Json {
    json!({"kind":"record","id":id})
}
fn record(kind: &str, id: &str, fields: Json) -> Record {
    let mut data =
        json!({"schema":"wedl/v1","kind":kind,"id":id,"title":format!("AUTHOR_TITLE_{id}")});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let raw = format!(
        "---\n{}---\nOPAQUE_UNTIMED_BODY {}\r\n",
        serde_yaml::to_string(&data).unwrap(),
        id
    );
    source::parse_record(
        raw.as_bytes(),
        &format!("story/{}.md", id.replace(':', "-").replace('/', "-")),
    )
    .unwrap()
}
fn section(number: usize, markdown: &str, valid: Json) -> Json {
    json!({"id":id("description",number),"heading":format!("Section {number}"),"markdown":markdown,
        "valid":valid,"audience":["public"],"perspectives":["ordinary"]})
}
fn transition(tick: i64, state: &str) -> Json {
    json!({"id":format!("learning-{tick}"),"time":point(tick,0),"state":state,"confidence":0.75,"acquisition":"heard"})
}
fn knowledge(number: usize, target: &str) -> Record {
    record(
        "knowledge",
        &id("know", number),
        json!({"knower":VIEWER,
        "claim":{"key":format!("acquired-{number}"),"statement":format!("Learned words {number}"),"object":{"entity":target}},
        "transitions":[transition(5,"accepted")]}),
    )
}
fn from_records(records: Vec<Record>) -> World {
    source::world_from_records(PathBuf::new(), records, Some("narrative-fixture".into())).unwrap()
}
fn valid(world: &World) {
    let report = validation::validate_world(world);
    assert!(
        report.valid,
        "{:?}",
        report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == "error")
            .collect::<Vec<_>>()
    );
}
fn edit(world: &World, target: &str, change: impl FnOnce(&mut Yaml)) -> World {
    let owner = &world.records[target];
    let mut fields = owner.frontmatter.clone();
    change(&mut fields);
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&fields).unwrap(),
        owner.body
    );
    let replacement = source::parse_record(raw.as_bytes(), &owner.source_path).unwrap();
    source::world_from_records(
        world.root.clone(),
        world
            .records
            .values()
            .map(|record| {
                if record.id == target {
                    replacement.clone()
                } else {
                    record.clone()
                }
            })
            .collect(),
        world.revision.clone(),
    )
    .unwrap()
}
fn set(world: &World, target: &str, key: &str, value: Json) -> World {
    edit(world, target, |fields| {
        fields[key] = serde_yaml::to_value(value).unwrap()
    })
}
fn add(world: &World, record: Record) -> World {
    let mut records = world.records.values().cloned().collect::<Vec<_>>();
    records.push(record);
    source::world_from_records(world.root.clone(), records, world.revision.clone()).unwrap()
}
fn world() -> World {
    let mut corrected = section(2, "Current shared observation", interval(0, 9));
    corrected["replaces"] = json!(id("description", 1));
    let mut private = section(3, "PRIVATE_SECTION_SENTINEL", json!({"kind":"timeless"}));
    private["audience"] = json!(["author"]);
    let mut personal = section(4, "Personal audience wording", json!({"kind":"timeless"}));
    personal["audience"] = json!([VIEWER]);
    let mut viewpoint = section(5, "Personal viewpoint wording", json!({"kind":"timeless"}));
    viewpoint["perspectives"] = json!([VIEWER]);
    let spatial = |number, map, replaces: Option<usize>| {
        let mut value = json!({"id":id("spatialhistory",number),"aspect":"geometry","valid":interval(0,9),
            "value":{"map_id":map,"geometry":{"kind":"point","coordinates":[1,2]}}});
        if let Some(previous) = replaces {
            value["replaces"] = json!(id("spatialhistory", previous));
        }
        value
    };
    let records = vec![
        record(
            "world",
            WORLD,
            json!({"capabilities":["generational-core-v1","spatial-core-v1","geometry-v1","route-v1","generational-knowledge-v1","lore-core-v1","descriptions-v1","assertions-v1","existence-v1","spatial-history-v1"],
            "default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"state_keys":{},
            "profiles":[{"id":"observed-lore","label":"Author profile metadata","fields":{"type":"object","required":{"note":{"type":"string"}},"optional":{
                "amount":{"type":"number"},"choice":{"type":"enum","values":[3,"3"]},"absence":{"type":"null"},
                "targets":{"type":"array","max_items":16,"items":{"type":"entity","kinds":["location","lore-concept","existence-episode","assertion"]}},
                "literal":{"type":"object","required":{"kind":{"type":"string"},"id":{"type":"string"}},"optional":{}}}}}],
            "predicates":[{"id":"statement","subject_kinds":["lore-concept"],"value_type":{"type":"string"}}]}),
        ),
        record("character", VIEWER, json!({})),
        record("character", OTHER, json!({})),
        record(
            "location",
            PLACE,
            json!({"descriptions":[section(1,"Old shared observation",interval(0,9)),corrected,private,personal,viewpoint],
            "spatial_history":[spatial(1,MAP,None),spatial(2,HIDDEN_MAP,Some(1))]}),
        ),
        record(
            "location",
            HIDDEN,
            json!({"descriptions":[section(1,"HIDDEN_OWNER_SECTION",json!({"kind":"timeless"}))]}),
        ),
        record(
            "map",
            MAP,
            json!({"crs":"local-planar:test","axis_order":["east","north"],"unit":"m","bounds":{"min":[-10,-10],"max":[10,10]}}),
        ),
        record(
            "map",
            HIDDEN_MAP,
            json!({"crs":"local-planar:test","axis_order":["east","north"],"unit":"m","bounds":{"min":[-10,-10],"max":[10,10]}}),
        ),
        record(
            "route",
            ROUTE,
            json!({"from_location_id":PLACE,"to_location_id":HIDDEN,"direction":"two-way","modes":["foot"],
            "spatial_history":[{"id":id("spatialhistory",3),"aspect":"route-metric","valid":interval(0,9),"value":{"travel_cost":{"value":7,"unit":"coin"}}}]}),
        ),
        record(
            "lore-concept",
            CONCEPT,
            json!({"domain":"test.lore","profile":"observed-lore","values":{"note":"Shared typed lore", "amount":9007199254740993_i64,"choice":3,"absence":null,
            "targets":[reference(PLACE),reference(HIDDEN),reference(OTHER_CONCEPT),reference(EPISODE),reference(ASSERTION)],"literal":{"kind":"record","id":HIDDEN}},
            "valid":interval(0,9),"audience":["public"],"perspectives":["ordinary"],"provenance":"AUTHOR_PROVENANCE_SENTINEL",
            "concept_links":[{"relation":"related","concept_id":OTHER_CONCEPT,"valid":{"kind":"timeless"}}],
            "descriptions":[section(1,"Typed lore description",json!({"kind":"timeless"}))]}),
        ),
        record(
            "lore-concept",
            OTHER_CONCEPT,
            json!({"domain":"test.other","profile":"observed-lore","values":{"note":"UNLEARNED_CONCEPT_SENTINEL"},"valid":{"kind":"timeless"},"audience":["public"],"perspectives":["ordinary"]}),
        ),
        record(
            "existence-episode",
            EPISODE,
            json!({"domain":"test.existence","character_id":OTHER,"episode_kind":"unknown","valid":{"kind":"interval","timeline":"main"},"start_evidence":[],"audience":["public"],"perspectives":["ordinary"],"descriptions":[section(1,"CANONICAL_EPISODE_SENTINEL",json!({"kind":"timeless"}))]}),
        ),
        record(
            "assertion",
            ASSERTION,
            json!({"domain":"test.statement","subject":reference(CONCEPT),"predicate":"statement","value":"CANONICAL_ASSERTION_SENTINEL","classification":"author-fact","valid":{"kind":"timeless"},"evidence":[],"audience":["public"],"perspectives":["ordinary"],"descriptions":[section(1,"CANONICAL_ASSERTION_SECTION",json!({"kind":"timeless"}))]}),
        ),
        knowledge(1, PLACE),
        knowledge(2, CONCEPT),
        knowledge(3, MAP),
        knowledge(4, ROUTE),
    ];
    let world = from_records(records);
    valid(&world);
    world
}
fn desc(world: &World, owner: &str, tick: i64, order: i32) -> Json {
    narrative::descriptions(world, VIEWER, owner, &at(tick, order), 100).unwrap()
}
fn catalog(world: &World) -> Json {
    narrative::lore_catalog(world, VIEWER, None, &at(5, 0), 100).unwrap()
}
fn full_packet(world: &World) -> Json {
    json!({"descriptions":desc(world,PLACE,5,0),"lore":catalog(world),
        "history":narrative::spatial_history(world,VIEWER,PLACE,&at(5,0),100).unwrap(),
        "links":narrative::lore_links(world,VIEWER,CONCEPT,&at(5,0),100).unwrap(),
        "context":character::context(world,VIEWER,&at(5,0),100000).unwrap(),
        "search":character::search(world,VIEWER,"shared",&at(5,0),100).unwrap()})
}

#[test]
fn exact_acquisition_boundaries_and_owner_sections_use_both_character_lenses() {
    let world = world();
    for time in [at(4, i32::MAX), at(5, -1)] {
        assert!(matches!(
            narrative::descriptions(&world, VIEWER, PLACE, &time, 100),
            Err(Error::NotFound(_))
        ));
        assert_eq!(
            narrative::lore_catalog(&world, VIEWER, None, &time, 100).unwrap()["count"],
            0
        );
    }
    let result = desc(&world, PLACE, 5, 0);
    assert_eq!(result["count"], 3);
    assert_eq!(result["items"][0]["sectionId"], id("description", 2));
    assert_eq!(result["items"][0]["replaces"], id("description", 1));
    assert_eq!(result["items"][1]["markdown"], "Personal audience wording");
    assert_eq!(result["items"][2]["markdown"], "Personal viewpoint wording");
    assert_eq!(desc(&world, PLACE, 9, 0)["count"], 3);
    assert_eq!(desc(&world, PLACE, 9, 1)["count"], 2);
    assert!(!result.to_string().contains("PRIVATE_SECTION"));
    assert!(!result.to_string().contains("AUTHOR_TITLE"));
}

#[test]
fn held_plain_rejection_is_known_but_forgetting_and_later_acquisition_are_exact() {
    let world = world();
    for state in [
        "accepted",
        "suspected",
        "uncertain",
        "remembered",
        "rejected",
    ] {
        let held = set(
            &world,
            &id("know", 1),
            "transitions",
            json!([transition(5, state)]),
        );
        valid(&held);
        assert_eq!(desc(&held, PLACE, 5, 0)["count"], 3);
    }
    let updated = set(
        &world,
        &id("know", 1),
        "transitions",
        json!([
            transition(5, "accepted"),
            transition(6, "forgotten"),
            transition(7, "remembered")
        ]),
    );
    assert_eq!(desc(&updated, PLACE, 5, i32::MAX)["count"], 3);
    assert!(narrative::descriptions(&updated, VIEWER, PLACE, &at(6, 0), 100).is_err());
    assert_eq!(desc(&updated, PLACE, 7, 0)["count"], 3);
    let other = set(
        &world,
        &id("know", 1),
        "transitions",
        json!([{"id":"elsewhere","time":{"timeline":"other","tick":0,"order":0},"state":"accepted","confidence":1,"acquisition":"heard"}]),
    );
    valid(&other);
    assert!(narrative::descriptions(&other, VIEWER, PLACE, &at(5, 0), 100).is_err());
}

#[test]
fn scoped_learning_precedes_known_roots_and_omission_does_not_imply_author_only() {
    let world = world();
    for (field, allowed) in [
        ("audience", vec!["public", VIEWER]),
        ("perspectives", vec!["ordinary", VIEWER]),
    ] {
        for label in allowed {
            let updated = set(&world, &id("know", 1), field, json!([label]));
            valid(&updated);
            assert_eq!(desc(&updated, PLACE, 5, 0)["count"], 3);
        }
        for labels in [json!([]), json!(["author"]), json!(["secret"])] {
            let hidden = set(&world, &id("know", 1), field, labels);
            valid(&hidden);
            assert!(narrative::descriptions(&hidden, VIEWER, PLACE, &at(5, 0), 100).is_err());
        }
    }
    let retired = set(&world, &id("know", 1), "status", json!("retired"));
    valid(&retired);
    assert!(narrative::descriptions(&retired, VIEWER, PLACE, &at(5, 0), 100).is_err());
    let other = set(&world, &id("know", 1), "knower", json!(OTHER));
    valid(&other);
    assert!(narrative::descriptions(&other, VIEWER, PLACE, &at(5, 0), 100).is_err());
}

#[test]
fn subject_literal_words_metadata_and_links_never_acquire_canonical_targets() {
    let world = world();
    let claims = [
        json!({"key":"subject","statement":"I heard words","subject":HIDDEN}),
        json!({"key":"literal","statement":HIDDEN,"object":{"text":HIDDEN}}),
        json!({"key":"unrelated","statement":"Words","truth":"true"}),
    ];
    for claim in claims {
        let updated = set(&world, &id("know", 1), "claim", claim);
        valid(&updated);
        assert!(narrative::descriptions(&updated, VIEWER, HIDDEN, &at(5, 0), 100).is_err());
    }
    let metadata = set(
        &world,
        &id("know", 1),
        "transitions",
        json!([{"id":"learned","time":point(5,0),"state":"accepted","confidence":1,"acquisition":"heard","source_entity":HIDDEN}]),
    );
    valid(&metadata);
    assert!(narrative::descriptions(&metadata, VIEWER, HIDDEN, &at(5, 0), 100).is_err());
    assert_eq!(
        narrative::lore_links(&world, VIEWER, CONCEPT, &at(5, 0), 100).unwrap()["count"],
        0
    );
    assert_eq!(catalog(&world)["count"], 1);
}

#[test]
fn exact_targets_share_unavailable_errors_and_never_use_titles_or_aliases() {
    let world = world();
    let expected = narrative::descriptions(&world, VIEWER, "absent", &at(5, 0), 100)
        .unwrap_err()
        .to_string();
    for target in [HIDDEN, &format!("AUTHOR_TITLE_{PLACE}"), EPISODE, ASSERTION] {
        assert_eq!(
            narrative::descriptions(&world, VIEWER, target, &at(5, 0), 100)
                .unwrap_err()
                .to_string(),
            expected
        );
    }
    for section in [
        id("description", 1),
        id("description", 3),
        "missing".into(),
        "Section 2".into(),
    ] {
        assert_eq!(
            narrative::section(&world, VIEWER, PLACE, &section, &at(5, 0))
                .unwrap_err()
                .to_string(),
            expected
        );
    }
    let current =
        narrative::section(&world, VIEWER, PLACE, &id("description", 2), &at(5, 0)).unwrap();
    assert_eq!(current["markdown"], "Current shared observation");
    for viewer in ["absent", PLACE, &format!("AUTHOR_TITLE_{VIEWER}")] {
        assert_eq!(
            narrative::descriptions(&world, viewer, PLACE, &at(5, 0), 100)
                .unwrap_err()
                .to_string(),
            expected
        );
    }
}

#[test]
fn section_corrections_keep_forks_and_hidden_source_insertion_does_not_change_packets() {
    let world = world();
    let before = full_packet(&world);
    let changed = edit(&world, PLACE, |fields| {
        let mut hidden = section(99, "HIDDEN_PREFIX_SENTINEL", json!({"kind":"timeless"}));
        hidden["audience"] = json!(["author"]);
        fields["descriptions"]
            .as_sequence_mut()
            .unwrap()
            .insert(0, serde_yaml::to_value(hidden).unwrap());
        fields["title"] = Yaml::String("CHANGED_HIDDEN_OWNER_TITLE".into());
        fields["private_author_metadata"] = Yaml::String("PRIVATE_METADATA".into());
    });
    valid(&changed);
    assert_eq!(full_packet(&changed), before);
    let fork = edit(&world, PLACE, |fields| {
        let mut fork = section(6, "Independent correction branch", interval(0, 9));
        fork["replaces"] = json!(id("description", 1));
        fields["descriptions"]
            .as_sequence_mut()
            .unwrap()
            .push(serde_yaml::to_value(fork).unwrap());
    });
    valid(&fork);
    let rows = desc(&fork, PLACE, 5, 0);
    assert_eq!(rows["count"], 4);
    assert_eq!(rows["items"][0]["sectionId"], id("description", 2));
    assert_eq!(rows["items"][3]["sectionId"], id("description", 6));
}

#[test]
fn known_concept_validity_status_and_lenses_gate_its_sections_and_values() {
    let world = world();
    assert_eq!(desc(&world, CONCEPT, 9, 0)["count"], 1);
    assert!(narrative::descriptions(&world, VIEWER, CONCEPT, &at(9, 1), 100).is_err());
    assert!(narrative::lore_concept(&world, VIEWER, CONCEPT, &at(9, 1)).is_err());
    for (field, value) in [
        ("status", json!("draft")),
        ("status", json!("retired")),
        ("audience", json!(["author"])),
        ("perspectives", json!(["author"])),
    ] {
        let changed = set(&world, CONCEPT, field, value);
        valid(&changed);
        assert!(narrative::descriptions(&changed, VIEWER, CONCEPT, &at(5, 0), 100).is_err());
        assert_eq!(catalog(&changed)["count"], 0);
    }
}

#[test]
fn typed_lore_prunes_unknown_entities_preserves_literals_and_never_returns_source_envelopes() {
    let world = world();
    let result = narrative::lore_concept(&world, VIEWER, CONCEPT, &at(5, 0)).unwrap();
    assert_eq!(result["values"]["targets"], json!([reference(PLACE)]));
    assert_eq!(
        result["values"]["literal"],
        json!({"kind":"record","id":HIDDEN})
    );
    assert_eq!(result["values"]["amount"], "9007199254740993");
    assert!(result["values"]["absence"].is_null());
    assert_eq!(result["values"]["choice"], "3");
    assert_eq!(
        result["valueTypes"]["fields"]["choice"]["scalarType"],
        "number"
    );
    assert_eq!(result["references"].as_array().unwrap().len(), 1);
    assert_eq!(result["references"][0]["entityId"], PLACE);
    for forbidden in [
        "AUTHOR_TITLE",
        "AUTHOR_PROVENANCE",
        "OPAQUE_UNTIMED_BODY",
        "sourceField",
        "sourcePath",
        "blobOid",
        "fingerprint",
        "revision",
        "CANONICAL_ASSERTION",
        "CANONICAL_EPISODE",
    ] {
        assert!(
            !result.to_string().contains(forbidden),
            "{forbidden}: {result}"
        );
    }
    let changed = edit(&world, CONCEPT, |fields| {
        fields["values"]["choice"] = Yaml::String("3".into())
    });
    valid(&changed);
    let changed = narrative::lore_concept(&changed, VIEWER, CONCEPT, &at(5, 0)).unwrap();
    assert_eq!(changed["values"]["choice"], "3");
    assert_eq!(
        changed["valueTypes"]["fields"]["choice"]["scalarType"],
        "string"
    );
}

#[test]
fn known_links_are_direct_and_hidden_reference_prefixes_do_not_change_projection() {
    let world = add(&world(), knowledge(5, OTHER_CONCEPT));
    valid(&world);
    let before = narrative::lore_concept(&world, VIEWER, CONCEPT, &at(5, 0)).unwrap();
    let links = narrative::lore_links(&world, VIEWER, CONCEPT, &at(5, 0), 100).unwrap();
    assert_eq!(links["count"], 1);
    assert_eq!(links["items"][0]["conceptId"], OTHER_CONCEPT);
    let third = record(
        "lore-concept",
        &id("concept", 3),
        json!({"domain":"test.hidden","profile":"observed-lore","values":{"note":"PRIVATE_THIRD"},"valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]}),
    );
    let inserted = edit(&add(&world, third), CONCEPT, |fields| {
        fields["concept_links"].as_sequence_mut().unwrap().insert(0,serde_yaml::to_value(json!({"relation":"hidden","concept_id":id("concept",3),"valid":{"kind":"timeless"}})).unwrap());
        fields["values"]["targets"]
            .as_sequence_mut()
            .unwrap()
            .insert(
                0,
                serde_yaml::to_value(reference(&id("concept", 3))).unwrap(),
            );
    });
    valid(&inserted);
    assert_eq!(
        narrative::lore_concept(&inserted, VIEWER, CONCEPT, &at(5, 0)).unwrap(),
        before
    );
    assert_eq!(
        narrative::lore_links(&inserted, VIEWER, CONCEPT, &at(5, 0), 100).unwrap(),
        links
    );
}

#[test]
fn complete_limits_are_applied_after_scope_and_domain_selection() {
    let world = world();
    assert_eq!(
        narrative::lore_catalog(&world, VIEWER, None, &at(5, 0), 1).unwrap()["count"],
        1
    );
    let updated = add(&world, knowledge(5, OTHER_CONCEPT));
    valid(&updated);
    assert!(narrative::lore_catalog(&updated, VIEWER, None, &at(5, 0), 1).is_err());
    assert_eq!(
        narrative::lore_catalog(&updated, VIEWER, Some("test.other"), &at(5, 0), 1).unwrap()["count"],
        1
    );
    assert!(narrative::descriptions(&world, VIEWER, PLACE, &at(5, 0), 2).is_err());
    assert_eq!(
        narrative::descriptions(&world, VIEWER, PLACE, &at(5, 0), 3).unwrap()["count"],
        3
    );
    for limit in [0, 1001] {
        assert!(narrative::descriptions(&world, VIEWER, PLACE, &at(5, 0), limit).is_err());
        assert!(narrative::lore_catalog(&world, VIEWER, None, &at(5, 0), limit).is_err());
        assert!(narrative::lore_links(&world, VIEWER, CONCEPT, &at(5, 0), limit).is_err());
        assert!(narrative::spatial_history(&world, VIEWER, PLACE, &at(5, 0), limit).is_err());
    }
}

#[test]
fn spatial_endpoint_admission_precedes_correction_and_character_source_metadata_is_absent() {
    let world = world();
    let before = narrative::spatial_history(&world, VIEWER, PLACE, &at(5, 0), 1).unwrap();
    assert_eq!(before["items"][0]["memberId"], id("spatialhistory", 1));
    assert_eq!(before["items"][0]["value"]["map_id"], MAP);
    assert!(before["items"][0].get("replaces").is_none());
    for forbidden in [
        "title",
        "citation",
        "sourcePath",
        "fingerprint",
        "blobOid",
        "revision",
        HIDDEN_MAP,
    ] {
        assert!(!before.to_string().contains(forbidden), "{before}");
    }
    let learned = add(&world, knowledge(5, HIDDEN_MAP));
    valid(&learned);
    let after = narrative::spatial_history(&learned, VIEWER, PLACE, &at(5, 0), 1).unwrap();
    assert_eq!(after["items"][0]["memberId"], id("spatialhistory", 2));
    assert_eq!(after["items"][0]["replaces"], id("spatialhistory", 1));
    assert_eq!(after["items"][0]["value"]["map_id"], HIDDEN_MAP);
    let hidden = set(&learned, &id("know", 5), "perspectives", json!(["author"]));
    valid(&hidden);
    assert_eq!(
        narrative::spatial_history(&hidden, VIEWER, PLACE, &at(5, 0), 1).unwrap(),
        before
    );
    assert_eq!(
        narrative::spatial_history(&world, VIEWER, ROUTE, &at(5, 0), 1).unwrap()["items"][0]["value"]
            ["travel_cost"]["value"],
        "7"
    );
    assert_eq!(
        narrative::spatial_history(&world, VIEWER, PLACE, &at(9, 0), 1).unwrap()["count"],
        1
    );
    assert_eq!(
        narrative::spatial_history(&world, VIEWER, PLACE, &at(9, 1), 1).unwrap()["count"],
        0
    );
}

#[test]
fn generic_context_and_search_include_only_admitted_sections_before_budgets() {
    let world = world();
    let context = character::context(&world, VIEWER, &at(5, 0), 100000).unwrap();
    assert_eq!(context["descriptions"].as_array().unwrap().len(), 4);
    assert!(context.to_string().contains("Current shared observation"));
    assert!(!context.to_string().contains("PRIVATE_SECTION"));
    assert!(!context.to_string().contains("OPAQUE_UNTIMED_BODY"));
    let found = character::search(&world, VIEWER, "shared", &at(5, 0), 100).unwrap();
    assert!(found.to_string().contains("description"));
    let private_search =
        character::search(&world, VIEWER, "PRIVATE_SECTION_SENTINEL", &at(5, 0), 100).unwrap();
    assert_eq!(private_search["total"], 0);
    assert_eq!(private_search["items"], json!([]));
    let before_budget = character::context(&world, VIEWER, &at(5, 0), 1)
        .unwrap_err()
        .to_string();
    let changed = edit(&world, PLACE, |fields| {
        fields["descriptions"][2]["markdown"] = Yaml::String("Hidden expanded prose ".repeat(2000))
    });
    valid(&changed);
    assert_eq!(
        character::context(&changed, VIEWER, &at(5, 0), 100000).unwrap(),
        context
    );
    assert_eq!(
        character::search(&changed, VIEWER, "shared", &at(5, 0), 100).unwrap(),
        found
    );
    assert_eq!(
        character::context(&changed, VIEWER, &at(5, 0), 1)
            .unwrap_err()
            .to_string(),
        before_budget
    );
}

#[test]
fn linked_learning_admits_its_own_sections_after_affirmative_learning_only() {
    let base = world();
    let known_id = id("know", 10);
    let world = add(
        &base,
        record(
            "knowledge",
            &known_id,
            json!({"knower":VIEWER,
                "claim":{"key":"learned-assertion","statement":"My own learned words","assertion_id":ASSERTION},
                "transitions":[transition(5,"rejected"),transition(6,"accepted"),transition(7,"rejected"),transition(8,"forgotten")],
                "descriptions":[section(1,"My learning description",json!({"kind":"timeless"}))]}),
        ),
    );
    valid(&world);
    for tick in [5, 8] {
        assert!(matches!(
            narrative::descriptions(&world, VIEWER, &known_id, &at(tick, 0), 100),
            Err(Error::NotFound(_))
        ));
    }
    for tick in [6, 7] {
        assert_eq!(desc(&world, &known_id, tick, 0)["count"], 1);
        assert!(matches!(
            narrative::descriptions(&world, VIEWER, ASSERTION, &at(tick, 0), 100),
            Err(Error::NotFound(_))
        ));
    }
    let scoped = set(&world, &known_id, "audience", json!(["author"]));
    valid(&scoped);
    assert!(matches!(
        narrative::descriptions(&scoped, VIEWER, &known_id, &at(6, 0), 100),
        Err(Error::NotFound(_))
    ));

    // An ordinary pointer still cannot turn a canonical assertion or existence
    // episode into learned words or supply its descriptions to generic context.
    let direct = add(
        &add(&world, knowledge(11, ASSERTION)),
        knowledge(12, EPISODE),
    );
    valid(&direct);
    for target in [ASSERTION, EPISODE] {
        assert!(matches!(
            narrative::descriptions(&direct, VIEWER, target, &at(6, 0), 100),
            Err(Error::NotFound(_))
        ));
    }
    let context = character::context(&direct, VIEWER, &at(6, 0), 100000).unwrap();
    assert!(!context.to_string().contains("CANONICAL_ASSERTION_SECTION"));
    assert!(!context.to_string().contains("CANONICAL_EPISODE_SENTINEL"));
}

#[test]
fn genealogy_sections_reuse_current_applicable_held_rules_without_target_grants() {
    let base = world();
    let known_id = id("know", 12);
    let world = add(
        &base,
        record(
            "knowledge",
            &known_id,
            json!({"knower":VIEWER,
                "claim":{"key":"genealogy.vital","statement":"I was told the person died.",
                    "genealogy":{"kind":"vital","payload":{"character_id":OTHER,"state":"dead"},
                        "valid":{"from":point(0,0),"until":point(6,0)},"labels":{OTHER:"A learned name"}}},
                "transitions":[transition(5,"suspected"),transition(6,"rejected"),transition(7,"accepted")],
                "descriptions":[section(1,"My own genealogy note",json!({"kind":"timeless"}))]}),
        ),
    );
    valid(&world);
    assert_eq!(desc(&world, &known_id, 5, 0)["count"], 1);
    for tick in [6, 7] {
        assert!(matches!(
            narrative::descriptions(&world, VIEWER, &known_id, &at(tick, 0), 100),
            Err(Error::NotFound(_))
        ));
    }
    let applicable = edit(&world, &known_id, |fields| {
        fields["claim"]["genealogy"]["valid"]["until"] = serde_yaml::to_value(point(8, 0)).unwrap();
    });
    valid(&applicable);
    assert_eq!(desc(&applicable, &known_id, 7, 0)["count"], 1);
    assert!(matches!(
        narrative::descriptions(&applicable, VIEWER, OTHER, &at(7, 0), 100),
        Err(Error::NotFound(_))
    ));
    let withheld = set(&applicable, &known_id, "perspectives", json!([]));
    valid(&withheld);
    assert!(matches!(
        narrative::descriptions(&withheld, VIEWER, &known_id, &at(7, 0), 100),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn ordinary_known_owner_scope_does_not_infer_event_time_or_canonical_status() {
    let base = world();
    let other = set(
        &set(&base, OTHER, "status", json!("retired")),
        OTHER,
        "descriptions",
        json!([section(
            1,
            "An independently dated account",
            json!({"kind":"timeless"})
        )]),
    );
    let known = add(&other, knowledge(10, OTHER));
    valid(&known);
    assert_eq!(desc(&known, OTHER, 5, 0)["count"], 1);
    let hidden = set(&known, OTHER, "audience", json!(["author"]));
    valid(&hidden);
    assert!(matches!(
        narrative::descriptions(&hidden, VIEWER, OTHER, &at(5, 0), 100),
        Err(Error::NotFound(_))
    ));

    let event_id = id("event", 1);
    let event = record(
        "event",
        &event_id,
        json!({"time":point(100,0),"effects":[],
            "descriptions":[section(1,"Explicitly available forecast",json!({"kind":"timeless"}))]}),
    );
    let future = add(&add(&base, event), knowledge(10, &event_id));
    valid(&future);
    assert_eq!(desc(&future, &event_id, 5, 0)["count"], 1);
    assert!(
        !desc(&future, &event_id, 5, 0)
            .to_string()
            .contains("OPAQUE_UNTIMED_BODY")
    );
}

#[test]
fn large_native_learning_and_section_times_keep_exact_order_and_wire_values() {
    let base = world();
    let tick = 9_007_199_254_740_993_i64;
    let learned = set(
        &base,
        &id("know", 1),
        "transitions",
        json!([transition(tick, "accepted")]),
    );
    let world = set(
        &learned,
        PLACE,
        "descriptions",
        json!([section(1, "One exact instant", interval(tick, tick))]),
    );
    valid(&world);
    assert!(matches!(
        narrative::descriptions(&world, VIEWER, PLACE, &at(tick, -1), 100),
        Err(Error::NotFound(_))
    ));
    let result = desc(&world, PLACE, tick, 0);
    assert_eq!(result["count"], 1);
    assert_eq!(result["effectiveTime"]["tick"], tick.to_string());
    assert_eq!(
        result["items"][0]["valid"]["start"]["tick"],
        tick.to_string()
    );
    assert_eq!(desc(&world, PLACE, tick, 1)["count"], 0);
}

fn reader_errors(world: &World) -> Vec<Error> {
    vec![
        narrative::descriptions(world, VIEWER, PLACE, &at(5, 0), 100).unwrap_err(),
        narrative::section(world, VIEWER, PLACE, &id("description", 2), &at(5, 0)).unwrap_err(),
        narrative::lore_catalog(world, VIEWER, None, &at(5, 0), 100).unwrap_err(),
        narrative::lore_concept(world, VIEWER, CONCEPT, &at(5, 0)).unwrap_err(),
        narrative::lore_links(world, VIEWER, CONCEPT, &at(5, 0), 100).unwrap_err(),
        narrative::spatial_history(world, VIEWER, PLACE, &at(5, 0), 100).unwrap_err(),
    ]
}

#[test]
fn every_public_reader_checks_the_whole_captured_source_without_private_diagnostics() {
    let base = world();
    let mut invalid = Vec::new();
    let mut fields = base.clone();
    fields.records.get_mut(HIDDEN).unwrap().frontmatter["title"] =
        Yaml::String("PRIVATE_DIAGNOSTIC_SENTINEL".into());
    invalid.push(fields);
    let mut bytes = base.clone();
    bytes
        .records
        .get_mut(HIDDEN)
        .unwrap()
        .raw_bytes
        .extend_from_slice(b"extra\n");
    invalid.push(bytes);
    let mut body = base.clone();
    body.records
        .get_mut(HIDDEN)
        .unwrap()
        .body
        .push_str("PRIVATE_DIAGNOSTIC_SENTINEL");
    invalid.push(body);
    let mut fingerprint = base.clone();
    fingerprint.fingerprint = "PRIVATE_DIAGNOSTIC_SENTINEL".into();
    invalid.push(fingerprint);
    invalid.push(edit(&base, HIDDEN, |fields| {
        fields["descriptions"][0]["valid"] = Yaml::Null;
    }));
    invalid.push(set(&base, &id("know", 1), "audience", json!("public")));

    let expected = "character narrative source is invalid; use author validation for diagnostics";
    for world in invalid {
        assert!(!validation::validate_world(&world).valid);
        for error in reader_errors(&world) {
            assert!(matches!(error, Error::Validation(_)));
            assert_eq!(error.to_string(), expected);
            assert!(!error.to_string().contains(HIDDEN));
            assert!(!error.to_string().contains("PRIVATE_DIAGNOSTIC_SENTINEL"));
        }
    }
}

#[test]
fn description_capability_remains_independent_and_absent_source_preserves_empty_context() {
    let base = from_records(vec![
        record(
            "world",
            WORLD,
            json!({"capabilities":[],"default_timeline":"main","timelines":[{"id":"main","label":"Main"}],"state_keys":{}}),
        ),
        record("character", VIEWER, json!({})),
    ]);
    valid(&base);
    let context = character::context(&base, VIEWER, &at(5, 0), 100000).unwrap();
    assert!(context.get("descriptions").is_none());
    let installed = set(&base, WORLD, "capabilities", json!(["descriptions-v1"]));
    let source = set(
        &installed,
        VIEWER,
        "descriptions",
        json!([section(
            1,
            "Self-scoped dated words",
            json!({"kind":"timeless"})
        )]),
    );
    valid(&source);
    assert_eq!(desc(&source, VIEWER, 5, 0)["count"], 1);
    assert_eq!(
        character::context(&source, VIEWER, &at(5, 0), 100000).unwrap()["descriptions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
