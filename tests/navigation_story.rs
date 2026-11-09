use serde_json::{Value as Json, json};
use std::path::PathBuf;
use wedl::{
    model::{Record, StoryTime, World},
    navigation::story::{story_point_trail, story_points},
    record_edit, source, story, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const A: &str = "char_00000000000000000000000001";
const HIDDEN: &str = "char_00000000000000000000000002";
const PLACE: &str = "loc_00000000000000000000000001";
const OTHER: &str = "loc_00000000000000000000000002";
const OBJECT: &str = "obj_00000000000000000000000001";
const HIDDEN_OBJECT: &str = "obj_00000000000000000000000002";
const SCENE: &str = "scene_00000000000000000000000001";
const P1: &str = "sp_00000000000000000000000001";
const P2: &str = "sp_00000000000000000000000002";
const P3: &str = "sp_00000000000000000000000003";
const E1: &str = "event_00000000000000000000000001";
const E2: &str = "event_00000000000000000000000002";
const E3: &str = "event_00000000000000000000000003";

fn record(kind: &str, id: &str, fields: Json) -> Record {
    let mut value = json!({"schema":"wedl/v1","kind":kind,"id":id,"title":id});
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    source::parse_record(
        format!(
            "---\n{}---\nExact authored prose α  \r\n",
            serde_yaml::to_string(&value).unwrap()
        )
        .as_bytes(),
        &format!("story/{id}.md"),
    )
    .unwrap()
}
fn point(id: &str, fields: Json) -> Record {
    let mut value = json!({"activation_policy":"suggest","lifecycle":{"initial_state":"dormant","transitions":[]}});
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    record("story-point", id, value)
}
fn event(id: &str, tick: i64, order: i32) -> Record {
    record(
        "event",
        id,
        json!({"time":{"tick":tick,"order":order},"location":PLACE,"participants":[],"effects":[]}),
    )
}
fn scene() -> Record {
    record(
        "scene",
        SCENE,
        json!({"status":"closed","location":PLACE,"time":{"start":{"tick":1},"current":{"tick":10},"end":{"tick":10}},"participants":[{"character":A,"from":{"tick":5,"order":1},"until":{"tick":7,"order":2}}],"objects":[OBJECT,HIDDEN_OBJECT]}),
    )
}
fn world(extra: Vec<Record>) -> World {
    let mut records = vec![
        record(
            "world",
            WORLD,
            json!({"default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"capabilities":[],"threads":[],"state_keys":{"character":{"ready":{"type":"boolean"}}},"relationship_metrics":{}}),
        ),
        record("character", A, json!({"initial_state":{"ready":true}})),
        record(
            "character",
            HIDDEN,
            json!({"initial_state":{"ready":false},"audience":[]}),
        ),
        record("location", PLACE, json!({})),
        record("location", OTHER, json!({})),
        record("object", OBJECT, json!({})),
        record("object", HIDDEN_OBJECT, json!({"audience":[]})),
    ];
    records.extend(extra);
    source::world_from_records(PathBuf::new(), records, None).unwrap()
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn item<'a>(value: &'a Json, id: &str) -> &'a Json {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["storyPointId"] == id)
        .unwrap()
}
fn change(world: &mut World, id: &str, key: &str, value: Json) {
    let current = &world.records[id];
    let mut fields = serde_json::to_value(&current.frontmatter).unwrap();
    fields[key] = value;
    let changed = record(&current.kind, id, fields);
    world.records.insert(id.into(), changed);
    *world = source::world_from_records(
        world.root.clone(),
        world.records.values().cloned().collect(),
        None,
    )
    .unwrap();
}
fn without_fingerprint(mut value: Json) -> Json {
    fn clear(value: &mut Json) {
        match value {
            Json::Object(values) => {
                values.remove("fingerprint");
                values.values_mut().for_each(clear);
            }
            Json::Array(values) => values.iter_mut().for_each(clear),
            _ => {}
        }
    }
    clear(&mut value);
    value
}

#[test]
fn trail_keeps_exact_dated_transitions_causes_outcomes_and_source_indexes() {
    let world = world(vec![
        event(E1, 5, 1),
        event(E2, 5, 3),
        event(E3, 8, 0),
        point(
            P1,
            json!({"lifecycle":{"initial_state":"dormant","transitions":[
            {"id":"started","state":"active","time":{"tick":5,"order":2},"causing_event":E1,"note":"Exact note α  \r\n"},
            {"id":"finished","state":"resolved","time":{"tick":9},"causing_event":E3}
        ]},"outcome_events":[E3,E2,E1]}),
        ),
    ]);
    let before: Vec<_> = world
        .records
        .values()
        .map(|r| r.raw_bytes.clone())
        .collect();
    let value = story_point_trail(&world, P1, &at(5, 2), None, 10).unwrap();
    let p = &value["storyPoint"];
    assert_eq!(p["storedState"], "active");
    assert_eq!(p["outcomeEvents"], json!([E1]));
    assert_eq!(p["plotTrail"]["transitions"].as_array().unwrap().len(), 1);
    let transition = &p["plotTrail"]["transitions"][0];
    assert_eq!(transition["transitionId"], "started");
    assert_eq!(transition["note"], "Exact note α  \r\n");
    assert_eq!(transition["at"]["tick"], "5");
    assert_eq!(transition["at"]["order"], "2");
    assert_eq!(transition["causingEvent"]["id"], E1);
    assert_eq!(
        transition["citation"]["section"],
        "lifecycle.transitions[0]"
    );
    assert_eq!(
        p["plotTrail"]["outcomeEvents"][0]["citation"]["section"],
        "outcome_events[2]"
    );
    let after = story_point_trail(&world, P1, &at(10, 0), None, 10).unwrap();
    assert_eq!(after["storyPoint"]["storedState"], "resolved");
    assert_eq!(after["storyPoint"]["outcomeEvents"], json!([E1, E2, E3]));
    assert_eq!(
        before,
        world
            .records
            .values()
            .map(|r| r.raw_bytes.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn future_and_withheld_trail_targets_cannot_change_the_admitted_response() {
    let mut world = world(vec![
        event(E1, 2, 0),
        event(E2, 10, 0),
        point(
            P1,
            json!({"lifecycle":{"initial_state":"dormant","transitions":[
            {"id":"started","state":"active","time":{"tick":3},"causing_event":E1},
            {"id":"finished","state":"resolved","time":{"tick":11},"causing_event":E2}
        ]},"outcome_events":[E1,E2]}),
        ),
        point(P2, json!({"audience":[]})),
    ]);
    change(&mut world, E1, "audience", json!([]));
    let before = story_points(&world, &at(5, 0), None, 10).unwrap();
    assert_eq!(before["count"], 1);
    assert_eq!(
        item(&before, P1)["plotTrail"]["transitions"][0]["causingEvent"],
        Json::Null
    );
    assert_eq!(item(&before, P1)["outcomeEvents"], json!([]));
    change(&mut world, E1, "title", json!("withheld title changed"));
    change(&mut world, E2, "title", json!("future title changed"));
    change(&mut world, P2, "title", json!("withheld plot changed"));
    let after = story_points(&world, &at(5, 0), None, 10).unwrap();
    assert_eq!(without_fingerprint(before), without_fingerprint(after));
    assert!(story_point_trail(&world, P2, &at(5, 0), None, 10).is_err());
}

#[test]
fn scene_conditions_use_only_explicit_admitted_context_and_keep_all_candidates() {
    let mut world = world(vec![
        scene(),
        point(
            P1,
            json!({"when":{"scene":{"location":PLACE,"participant":A,"object":OBJECT}}}),
        ),
        point(P2, json!({"when":{"all":[]}})),
    ]);
    change(&mut world, SCENE, "story_points", json!([P2]));
    let before = story_points(&world, &at(6, 0), None, 10).unwrap();
    assert_eq!(item(&before, P1)["conditionSatisfied"], false);
    assert_eq!(item(&before, P2)["conditionSatisfied"], true);
    let selected = story_points(&world, &at(6, 0), Some(SCENE), 10).unwrap();
    assert_eq!(selected["count"], 2);
    assert_eq!(selected["sceneId"], SCENE);
    assert_eq!(item(&selected, P1)["eligible"], true);
    assert_eq!(item(&selected, P2)["eligible"], true);
    assert_eq!(
        item(&story::story_points(&world, &at(6, 0)).unwrap(), P1)["eligible"],
        false
    );
}

#[test]
fn scene_participant_conditions_observe_inclusive_exact_arrival_and_departure() {
    let world = world(vec![
        scene(),
        point(P1, json!({"when":{"scene":{"participant":A}}})),
    ]);
    for (tick, order, expected) in [(5, 0, false), (5, 1, true), (7, 2, true), (7, 3, false)] {
        let value = story_points(&world, &at(tick, order), Some(SCENE), 10).unwrap();
        assert_eq!(
            item(&value, P1)["conditionSatisfied"],
            expected,
            "{tick}/{order}"
        );
    }
}

#[test]
fn selected_scenes_refuse_unsettled_hidden_and_out_of_interval_contexts() {
    let base = world(vec![scene(), point(P1, json!({"when":{"scene":{}}}))]);
    for time in [at(0, 0), at(11, 0), StoryTime::new("other", 6, 0).unwrap()] {
        assert!(story_points(&base, &time, Some(SCENE), 10).is_err());
    }
    for status in ["planned", "abandoned"] {
        let mut changed = base.clone();
        change(&mut changed, SCENE, "status", json!(status));
        assert!(story_points(&changed, &at(6, 0), Some(SCENE), 10).is_err());
    }
    let mut hidden = base.clone();
    change(&mut hidden, SCENE, "audience", json!([]));
    let hidden_error = story_points(&hidden, &at(6, 0), Some(SCENE), 10).unwrap_err();
    let missing = story_points(
        &base,
        &at(6, 0),
        Some("scene_00000000000000000000000009"),
        10,
    )
    .unwrap_err();
    assert_eq!(hidden_error.to_string(), missing.to_string());
}

#[test]
fn active_scene_context_stops_at_its_cursor_and_empty_predicate_needs_a_scene() {
    let mut world = world(vec![scene(), point(P1, json!({"when":{"scene":{}}}))]);
    change(&mut world, SCENE, "status", json!("active"));
    change(
        &mut world,
        SCENE,
        "time",
        json!({"start":{"tick":1},"current":{"tick":7,"order":2}}),
    );
    assert_eq!(
        item(
            &story_points(&world, &at(7, 2), Some(SCENE), 10).unwrap(),
            P1
        )["eligible"],
        true
    );
    assert!(story_points(&world, &at(7, 3), Some(SCENE), 10).is_err());
    assert_eq!(
        item(&story_points(&world, &at(7, 2), None, 10).unwrap(), P1)["eligible"],
        false
    );
}

#[test]
fn scene_conditions_do_not_grant_hidden_target_names_or_invert_hidden_predicates() {
    let mut world = world(vec![
        scene(),
        point(
            P1,
            json!({"when":{"not":{"scene":{"object":HIDDEN_OBJECT}}}}),
        ),
        point(P2, json!({"when":{"scene":{"location":OTHER}}})),
    ]);
    let before = story_points(&world, &at(6, 0), Some(SCENE), 10).unwrap();
    assert_eq!(item(&before, P1)["conditionSatisfied"], false);
    assert_eq!(item(&before, P1)["eligible"], false);
    assert_eq!(item(&before, P2)["conditionSatisfied"], false);
    assert!(
        !serde_json::to_string(&before)
            .unwrap()
            .contains(HIDDEN_OBJECT)
    );
    change(
        &mut world,
        HIDDEN_OBJECT,
        "title",
        json!("withheld target changed"),
    );
    change(&mut world, SCENE, "objects", json!([OBJECT]));
    let after = story_points(&world, &at(6, 0), Some(SCENE), 10).unwrap();
    assert_eq!(without_fingerprint(before), without_fingerprint(after));
}

#[test]
fn withheld_event_condition_is_unavailable_even_inside_not_or_any() {
    let mut world = world(vec![
        event(E1, 1, 0),
        point(P1, json!({"when":{"not":{"event":E1}}})),
        point(P2, json!({"when":{"any":[{"all":[]},{"event":E1}]}})),
    ]);
    change(&mut world, E1, "audience", json!([]));
    let before = story_points(&world, &at(5, 0), None, 10).unwrap();
    for id in [P1, P2] {
        assert_eq!(item(&before, id)["eligible"], false);
        assert_eq!(item(&before, id)["conditionSatisfied"], false);
    }
    assert!(!serde_json::to_string(&before).unwrap().contains(E1));
    change(&mut world, E1, "time", json!({"tick":9}));
    change(&mut world, E1, "title", json!("withheld event changed"));
    let after = story_points(&world, &at(5, 0), None, 10).unwrap();
    assert_eq!(without_fingerprint(before), without_fingerprint(after));
    // Direct canonical evaluation retains its documented unrestricted source read.
    assert_eq!(
        item(&story::story_points(&world, &at(5, 0)).unwrap(), P1)["eligible"],
        true
    );
}

#[test]
fn future_event_and_unestablished_relationship_predicates_keep_boolean_semantics() {
    const REL: &str = "rel_00000000000000000000000001";
    let mut world = world(vec![
        event(E1, 10, 0),
        record(
            "relationship",
            REL,
            json!({"from":A,"to":HIDDEN,"transitions":[{"id":"met","time":{"tick":10},"metrics":{"trust":0.75},"facets":[]}]}),
        ),
        point(P1, json!({"when":{"not":{"event":E1}}})),
        point(
            P2,
            json!({"when":{"not":{"relationship":{"id":REL,"metric":"trust","min":0.5}}}}),
        ),
        point(
            P3,
            json!({"when":{"any":[{"event":E1},{"not":{"event":E1}}]}}),
        ),
    ]);
    change(&mut world, HIDDEN, "audience", json!(["author"]));
    change(
        &mut world,
        WORLD,
        "relationship_metrics",
        json!({"trust":{}}),
    );
    let valid = validation::validate_world(&world);
    assert!(valid.valid, "{:?}", valid.diagnostics);
    let before = story_points(&world, &at(5, 0), None, 10).unwrap();
    for id in [P1, P2, P3] {
        assert_eq!(item(&before, id)["eligible"], true);
        assert_eq!(item(&before, id)["blockedReasons"], json!([]));
    }
    assert!(!serde_json::to_string(&before).unwrap().contains(E1));
    assert!(!serde_json::to_string(&before).unwrap().contains(REL));
    change(&mut world, E1, "title", json!("future event changed"));
    change(
        &mut world,
        REL,
        "title",
        json!("future relationship changed"),
    );
    let after = story_points(&world, &at(5, 0), None, 10).unwrap();
    assert_eq!(without_fingerprint(before), without_fingerprint(after));
    change(&mut world, P1, "when", json!({"event":E1}));
    let positive = story_points(&world, &at(5, 0), None, 10).unwrap();
    assert_eq!(item(&positive, P1)["conditionSatisfied"], false);
    assert_eq!(
        item(&positive, P1)["blockedReasons"],
        json!(["authored condition is not satisfied"])
    );
    assert!(!serde_json::to_string(&positive).unwrap().contains(E1));
}

#[test]
fn withheld_state_owner_and_requirements_never_change_eligibility_or_reasons() {
    let mut world = world(vec![
        point(
            P1,
            json!({"when":{"state":{"entity":HIDDEN,"key":"ready","equals":true}}}),
        ),
        point(P2, json!({"requires":[{"id":P3,"states":["resolved"]}]})),
        point(P3, json!({"audience":[]})),
    ]);
    let before = story_points(&world, &at(5, 0), None, 10).unwrap();
    assert_eq!(before["count"], 2);
    assert_eq!(item(&before, P1)["conditionSatisfied"], false);
    assert_eq!(item(&before, P2)["requirementsSatisfied"], false);
    let serialized = serde_json::to_string(&before).unwrap();
    assert!(!serialized.contains(HIDDEN));
    assert!(!serialized.contains(P3));
    change(&mut world, HIDDEN, "initial_state", json!({"ready":true}));
    change(
        &mut world,
        P3,
        "lifecycle",
        json!({"initial_state":"resolved","transitions":[]}),
    );
    change(
        &mut world,
        P3,
        "title",
        json!("withheld prerequisite changed"),
    );
    let after = story_points(&world, &at(5, 0), None, 10).unwrap();
    assert_eq!(without_fingerprint(before), without_fingerprint(after));
}

#[test]
fn declared_entity_equals_and_unknown_actual_state_stay_unavailable_under_not() {
    let mut world = world(vec![
        point(
            P1,
            json!({"when":{"state":{"entity":A,"key":"location","equals":{"entity":OTHER}}}}),
        ),
        point(
            P2,
            json!({"when":{"not":{"state":{"entity":A,"key":"location","equals":{"entity":PLACE}}}}}),
        ),
        point(
            P3,
            json!({"when":{"state":{"entity":A,"key":"literal","equals":OTHER}}}),
        ),
    ]);
    change(
        &mut world,
        WORLD,
        "state_keys",
        json!({"character":{"ready":{"type":"boolean"},"location":{"type":"entity","kind":"location"},"literal":{"type":"string"}}}),
    );
    change(
        &mut world,
        A,
        "initial_state",
        json!({"ready":true,"location":{"entity":OTHER},"literal":OTHER}),
    );
    change(&mut world, OTHER, "audience", json!([]));
    let before = story_points(&world, &at(5, 0), None, 10).unwrap();
    assert_eq!(item(&before, P1)["eligible"], false);
    assert_eq!(item(&before, P2)["eligible"], false);
    assert_eq!(item(&before, P3)["eligible"], true);
    assert_eq!(
        item(&story::story_points(&world, &at(5, 0)).unwrap(), P1)["eligible"],
        true
    );
    assert!(!serde_json::to_string(&before).unwrap().contains(OTHER));
    change(&mut world, OTHER, "title", json!("hidden location renamed"));
    let after = story_points(&world, &at(5, 0), None, 10).unwrap();
    assert_eq!(without_fingerprint(before), without_fingerprint(after));
}

#[test]
fn hidden_event_effects_and_hidden_typed_operands_are_withheld_before_state_replay() {
    let mut world = world(vec![
        event(E1, 1, 0),
        event(E2, 2, 0),
        point(
            P1,
            json!({"when":{"state":{"entity":A,"key":"ready","equals":false}}}),
        ),
        point(
            P2,
            json!({"when":{"not":{"state":{"entity":A,"key":"ready","equals":false}}}}),
        ),
        point(
            P3,
            json!({"when":{"state":{"entity":A,"key":"location","equals":{"entity":PLACE}}}}),
        ),
    ]);
    change(
        &mut world,
        WORLD,
        "state_keys",
        json!({"character":{"ready":{"type":"boolean"},"location":{"type":"entity","kind":"location"}}}),
    );
    change(
        &mut world,
        A,
        "initial_state",
        json!({"ready":false,"location":{"entity":PLACE}}),
    );
    change(&mut world, OTHER, "audience", json!([]));
    change(&mut world, E1, "audience", json!([]));
    change(
        &mut world,
        E1,
        "effects",
        json!([{"id":"effect_00000000000000000000000001","target":A,"key":"ready","operation":"set","value":true}]),
    );
    change(
        &mut world,
        E2,
        "effects",
        json!([{"id":"effect_00000000000000000000000002","target":A,"key":"location","operation":"set","value":{"entity":OTHER}}]),
    );
    let before = story_points(&world, &at(5, 0), None, 10).unwrap();
    assert_eq!(item(&before, P1)["eligible"], true);
    assert_eq!(item(&before, P2)["eligible"], false);
    assert_eq!(item(&before, P3)["eligible"], true);
    change(
        &mut world,
        E1,
        "effects",
        json!([{"id":"effect_00000000000000000000000001","target":A,"key":"ready","operation":"set","value":false}]),
    );
    change(&mut world, E1, "title", json!("hidden event changed"));
    change(&mut world, OTHER, "title", json!("hidden operand changed"));
    let after = story_points(&world, &at(5, 0), None, 10).unwrap();
    assert_eq!(without_fingerprint(before), without_fingerprint(after));
}

#[test]
fn incomplete_exclusive_group_replay_is_unavailable_without_invalidating_valid_canon() {
    let mut world = world(vec![
        event(E1, 1, 0),
        event(E2, 2, 0),
        point(
            P1,
            json!({"when":{"state":{"entity":A,"key":"right","equals":true}}}),
        ),
        point(
            P2,
            json!({"when":{"not":{"state":{"entity":A,"key":"left","equals":true}}}}),
        ),
    ]);
    change(
        &mut world,
        WORLD,
        "state_keys",
        json!({"character":{
            "ready":{"type":"boolean"},
            "left":{"type":"boolean","exclusive_group":"branch"},
            "right":{"type":"boolean","exclusive_group":"branch"}
        }}),
    );
    change(
        &mut world,
        A,
        "initial_state",
        json!({"ready":true,"left":true}),
    );
    change(&mut world, E1, "audience", json!([]));
    change(
        &mut world,
        E1,
        "effects",
        json!([{"id":"effect_00000000000000000000000001","target":A,"key":"left","operation":"clear"}]),
    );
    change(
        &mut world,
        E2,
        "effects",
        json!([{"id":"effect_00000000000000000000000002","target":A,"key":"right","operation":"set","value":true}]),
    );
    let valid = validation::validate_world(&world);
    assert!(valid.valid, "{:?}", valid.diagnostics);
    assert_eq!(
        item(&story::story_points(&world, &at(5, 0)).unwrap(), P1)["eligible"],
        true
    );
    let before = story_points(&world, &at(5, 0), None, 10).unwrap();
    for id in [P1, P2] {
        assert_eq!(item(&before, id)["eligible"], false);
        assert_eq!(
            item(&before, id)["blockedReasons"],
            json!(["authored condition is unavailable at this horizon"])
        );
    }
    change(
        &mut world,
        E1,
        "title",
        json!("withheld prerequisite changed"),
    );
    let after = story_points(&world, &at(5, 0), None, 10).unwrap();
    assert_eq!(without_fingerprint(before), without_fingerprint(after));
    change(&mut world, E1, "effects", json!([]));
    assert!(story_points(&world, &at(5, 0), None, 10).is_err());
}

#[test]
fn entity_sets_and_bound_profile_conditions_follow_their_declared_reference_types() {
    let mut world = world(vec![
        point(
            P1,
            json!({"when":{"state":{"entity":A,"key":"places","equals":[{"entity":OTHER}]}}}),
        ),
        point(
            P2,
            json!({"when":{"state":{"entity":A,"key":"profile","equals":{"label":OTHER}}}}),
        ),
        point(
            P3,
            json!({"when":{"state":{"entity":A,"key":"linked_profile","equals":{"label":"known","target":{"kind":"record","id":OTHER}}}}}),
        ),
    ]);
    change(&mut world, WORLD, "capabilities", json!(["lore-core-v1"]));
    change(
        &mut world,
        WORLD,
        "profiles",
        json!([{"id":"context","label":"Context","fields":{"type":"object","required":{"label":{"type":"string"}},"optional":{"target":{"type":"entity","kinds":["location"]}}}}]),
    );
    change(
        &mut world,
        WORLD,
        "state_keys",
        json!({"character":{"ready":{"type":"boolean"},"places":{"type":"entity-set","kind":"location"},"profile":{"type":"string"},"linked_profile":{"type":"string"}}}),
    );
    change(
        &mut world,
        WORLD,
        "state_profile_bindings",
        json!({"character":{"profile":"context","linked_profile":"context"}}),
    );
    change(
        &mut world,
        A,
        "initial_state",
        json!({"ready":true,"places":[{"entity":OTHER}],"profile":{"label":OTHER},"linked_profile":{"label":"known","target":{"kind":"record","id":OTHER}}}),
    );
    change(&mut world, OTHER, "audience", json!([]));
    let before = story_points(&world, &at(5, 0), None, 10).unwrap();
    assert_eq!(item(&before, P1)["eligible"], false);
    assert_eq!(item(&before, P2)["eligible"], true);
    assert_eq!(item(&before, P3)["eligible"], false);
    change(&mut world, OTHER, "title", json!("typed endpoint changed"));
    let after = story_points(&world, &at(5, 0), None, 10).unwrap();
    assert_eq!(without_fingerprint(before), without_fingerprint(after));
}

#[test]
fn hidden_knowledge_rows_and_relationship_endpoints_do_not_affect_conditions() {
    const KNOW: &str = "know_00000000000000000000000001";
    const REL: &str = "rel_00000000000000000000000001";
    let mut world = world(vec![
        record(
            "knowledge",
            KNOW,
            json!({"knower":A,"audience":[],"claim":{"key":"gate.open","statement":"The gate is open"},"transitions":[{"id":"learned","time":{"tick":1},"state":"accepted","confidence":1.0,"acquisition":"observed"}]}),
        ),
        record(
            "relationship",
            REL,
            json!({"from":A,"to":HIDDEN,"transitions":[{"id":"met","time":{"tick":1},"metrics":{"trust":0.75},"facets":[]}]}),
        ),
        point(
            P1,
            json!({"when":{"knowledge":{"character":A,"claim":"gate.open","states":["accepted"]}}}),
        ),
        point(
            P2,
            json!({"when":{"not":{"knowledge":{"character":A,"claim":"gate.open","states":["accepted"]}}}}),
        ),
        point(
            P3,
            json!({"when":{"relationship":{"id":REL,"metric":"trust","min":0.5}}}),
        ),
    ]);
    change(
        &mut world,
        WORLD,
        "relationship_metrics",
        json!({"trust":{}}),
    );
    let before = story_points(&world, &at(5, 0), None, 10).unwrap();
    assert_eq!(item(&before, P1)["eligible"], false);
    assert_eq!(item(&before, P2)["eligible"], true);
    assert_eq!(item(&before, P3)["eligible"], false);
    change(
        &mut world,
        KNOW,
        "transitions",
        json!([{"id":"learned","time":{"tick":1},"state":"rejected","confidence":1.0,"acquisition":"observed"}]),
    );
    change(
        &mut world,
        REL,
        "transitions",
        json!([{"id":"met","time":{"tick":1},"metrics":{"trust":0.1},"facets":[]}]),
    );
    let after = story_points(&world, &at(5, 0), None, 10).unwrap();
    assert_eq!(without_fingerprint(before), without_fingerprint(after));
}

#[test]
fn ordinary_condition_results_are_unchanged_when_no_scene_predicate_is_present() {
    let world = world(vec![
        scene(),
        event(E1, 1, 0),
        point(
            P1,
            json!({"when":{"all":[{"event":E1},{"state":{"entity":A,"key":"ready","equals":true}}]}}),
        ),
        point(P2, json!({"when":{"not":{"event":E1}}})),
    ]);
    let ordinary = story::story_points(&world, &at(6, 0)).unwrap();
    for scene in [None, Some(SCENE)] {
        let selected = story_points(&world, &at(6, 0), scene, 10).unwrap();
        for id in [P1, P2] {
            for field in [
                "eligible",
                "storedState",
                "blockedReasons",
                "conditionSatisfied",
                "requirementsSatisfied",
            ] {
                assert_eq!(item(&ordinary, id)[field], item(&selected, id)[field]);
            }
        }
    }
}

#[test]
fn scene_predicates_use_the_same_closed_typed_authoring_and_source_grammar() {
    let world = world(vec![scene(), point(P1, json!({}))]);
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    let request = format!(
        "kind: story-point\nwhen: {}\n",
        json!({"scene":{"location":PLACE,"participant":A,"object":OBJECT}})
    );
    let changed = record_edit::update(&world, P1, &request).unwrap();
    assert_eq!(changed.body, world.records[P1].body);
    let mut records = world.records.clone();
    records.insert(P1.into(), changed);
    let candidate =
        source::world_from_records(world.root.clone(), records.into_values().collect(), None)
            .unwrap();
    let report = validation::validate_world(&candidate);
    assert!(report.valid, "{:?}", report.diagnostics);
    assert_eq!(
        item(
            &story_points(&candidate, &at(6, 0), Some(SCENE), 10).unwrap(),
            P1
        )["eligible"],
        true
    );
    for when in [
        json!({"scene":{"typo":PLACE}}),
        json!({"scene":{"location":null}}),
        json!({"scene":{"participant":OBJECT}}),
        json!({"scene":{"object":A}}),
        json!({"scene":[]}),
    ] {
        let request = format!(
            "kind: story-point\nwhen: {}\n",
            serde_json::to_string(&when).unwrap()
        );
        assert!(
            record_edit::update(&world, P1, &request).is_err(),
            "{request}"
        );
        let mut raw = world.clone();
        change(&mut raw, P1, "when", when);
        assert!(!validation::validate_world(&raw).valid);
    }
}

#[test]
fn exact_extreme_story_times_are_decimal_strings_in_trails() {
    let world = world(vec![
        event(E1, i64::MIN, i32::MIN),
        point(
            P1,
            json!({"lifecycle":{"initial_state":"dormant","transitions":[
                {"id":"first","state":"active","time":{"tick":i64::MIN,"order":i32::MIN},"causing_event":E1},
                {"id":"last","state":"resolved","time":{"tick":9_007_199_254_740_993_i64,"order":i32::MAX}}
            ]}}),
        ),
    ]);
    let value = story_point_trail(&world, P1, &at(i64::MAX, i32::MAX), None, 10).unwrap();
    let trail = &value["storyPoint"]["plotTrail"]["transitions"];
    assert_eq!(trail[0]["at"]["tick"], i64::MIN.to_string());
    assert_eq!(trail[0]["at"]["order"], i32::MIN.to_string());
    assert_eq!(trail[1]["at"]["tick"], "9007199254740993");
    assert_eq!(trail[1]["at"]["order"], i32::MAX.to_string());
}

#[test]
fn navigation_limits_cover_complete_candidates_and_combined_trail_members() {
    let world = world(vec![
        event(E1, 1, 0),
        point(
            P1,
            json!({"outcome_events":[E1],"lifecycle":{"initial_state":"dormant","transitions":[{"id":"t","state":"active","time":{"tick":1}}]}}),
        ),
        point(P2, json!({})),
    ]);
    assert!(
        story_points(&world, &at(2, 0), None, 1)
            .unwrap_err()
            .to_string()
            .contains("incomplete")
    );
    assert!(
        story_point_trail(&world, P1, &at(2, 0), None, 1)
            .unwrap_err()
            .to_string()
            .contains("incomplete")
    );
    assert_eq!(
        story_point_trail(&world, P1, &at(2, 0), None, 2).unwrap()["storyPointId"],
        P1
    );
    for limit in [0, 1001] {
        assert!(story_points(&world, &at(2, 0), None, limit).is_err());
        assert!(story_point_trail(&world, P1, &at(2, 0), None, limit).is_err());
    }
    assert!(story_point_trail(&world, "title alias", &at(2, 0), None, 10).is_err());
    assert!(
        story_point_trail(&world, "sp_00000000000000000000000009", &at(2, 0), None, 10).is_err()
    );
}

#[test]
fn complete_story_byte_bound_refuses_many_individually_valid_large_notes() {
    let note = "x".repeat(900_000);
    let points: Vec<_> = (1..=10)
        .map(|index| point(&format!("sp_{index:026}"), json!({"lifecycle":{"initial_state":"dormant","transitions":[{"id":"started","state":"active","time":{"tick":1},"note":note}]}})))
        .collect();
    assert!(
        points
            .iter()
            .all(|record| record.raw_bytes.len() < 1024 * 1024)
    );
    let mut world = world(points);
    let valid = validation::validate_world(&world);
    assert!(valid.valid, "{:?}", valid.diagnostics);
    let error = story_points(&world, &at(2, 0), None, 10).unwrap_err();
    assert!(error.to_string().contains("8 MiB"));
    assert!(story_point_trail(&world, P1, &at(2, 0), None, 10).is_ok());
    change(
        &mut world,
        "sp_00000000000000000000000010",
        "audience",
        json!([]),
    );
    assert_eq!(
        story_points(&world, &at(2, 0), None, 10).unwrap()["count"],
        9
    );
}
