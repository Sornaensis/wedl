//! Exact event-local folds and explicit contribution/source attribution.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    Error,
    consequence_semantics::Time,
    consequences::{Operation, Plan},
    event_consequences::{self, EventRequest, GroupedRequest},
    model::{Record, World},
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const ANA: &str = "char_00000000000000000000000001";
const BOB: &str = "char_00000000000000000000000002";
const PLACE: &str = "loc_00000000000000000000000001";
const EVENT: &str = "event_00000000000000000000000001";
const LATER: &str = "event_00000000000000000000000002";
const THIRD: &str = "event_00000000000000000000000003";
const KNOW: &str = "know_00000000000000000000000001";
const REL: &str = "rel_00000000000000000000000001";
const POINT: &str = "sp_00000000000000000000000001";
const WHEN_EVENT: &str = "sp_00000000000000000000000002";
const INDIRECT: &str = "sp_00000000000000000000000003";
const SCENE: &str = "scene_00000000000000000000000001";
const BIG: i64 = 9_007_199_254_740_993;

fn y(value: &str) -> Yaml {
    serde_yaml::from_str(value).unwrap()
}
fn record(kind: &str, id: &str, fields: &str) -> Record {
    source::parse_record(format!("---\nschema: wedl/v1\nkind: {kind}\nid: {id}\ntitle: Literal {kind}\n{fields}---\nLiteral source prose.\n").as_bytes(),&format!("story/{id}.md")).unwrap()
}
fn assemble(records: Vec<Record>) -> World {
    source::world_from_records(
        PathBuf::from("/event-consequences-no-filesystem-reads"),
        records,
        Some("a".repeat(40)),
    )
    .unwrap()
}
fn valid(world: &World) {
    let validation = validation::validate_world(world);
    assert!(validation.valid, "{:?}", validation.diagnostics);
}
fn world() -> World {
    let world = assemble(vec![
        record(
            "world",
            WORLD,
            "capabilities: [lore-core-v1]\ndefault_timeline: main\ntimelines: [{id: main, label: Main}, {id: other, label: Other}]\nstate_keys:\n  character:\n    mood: {type: string}\n    count: {type: integer}\n    labels: {type: string-set}\n    notes: {type: string}\nstate_profile_bindings: {character: {notes: notes}}\nprofiles:\n- id: notes\n  label: Typed notes\n  fields:\n    type: object\n    required: {count: {type: number}}\n    optional: {empty: {type: 'null'}}\nrelationship_metrics:\n  trust: {min: -1, max: 1, default: 0}\n  count: {min: 0, max: 1000000000000000000, default: 0}\n",
        ),
        record(
            "character",
            ANA,
            &format!(
                "initial_state: {{mood: calm, count: {BIG}, labels: [key, coin], notes: {{count: {BIG}, empty: null}}}}\n"
            ),
        ),
        record("character", BOB, "initial_state: {mood: unrelated}\n"),
        record("location", PLACE, ""),
    ]);
    valid(&world);
    world
}
fn with(world: &World, extra: Vec<Record>) -> World {
    let mut records = world.records.clone();
    records.extend(extra.into_iter().map(|record| (record.id.clone(), record)));
    let result = assemble(records.into_values().collect());
    valid(&result);
    result
}
fn edit(world: &World, id: &str, change: impl FnOnce(&mut Record)) -> World {
    let mut records = world.records.clone();
    let record = records.get_mut(id).unwrap();
    change(record);
    *record = source::parse_record(
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&record.frontmatter).unwrap(),
            record.body
        )
        .as_bytes(),
        &record.source_path,
    )
    .unwrap();
    let world = assemble(records.into_values().collect());
    valid(&world);
    world
}
fn event(id: &str, tick: i64, order: i32, fields: &str) -> Record {
    record(
        "event",
        id,
        &format!("time: {{timeline: main, tick: {tick}, order: {order}}}\n{fields}"),
    )
}
fn point(id: &str, fields: &str) -> Record {
    record(
        "story-point",
        id,
        &format!(
            "activation_policy: suggest\nlifecycle: {{initial_state: dormant, transitions: []}}\n{fields}"
        ),
    )
}
fn time(tick: i64, order: i32) -> Time {
    Time {
        timeline: "main".into(),
        tick: tick.to_string(),
        order: order.to_string(),
    }
}
fn request(world: &World, tick: i64, order: i32) -> EventRequest {
    EventRequest {
        expected_fingerprint: world.fingerprint.clone(),
        event_id: EVENT.into(),
        at: time(tick, order),
        max_items: 1000,
        max_characters: 1_000_000,
    }
}
fn read(world: &World, tick: i64) -> Json {
    event_consequences::read(world, &request(world, tick, 0)).unwrap()
}
fn update(id: &str, patch: &str) -> Operation {
    Operation::Update {
        entity_id: id.into(),
        patch: patch.into(),
    }
}
fn grouped(world: &World, tick: i64, operations: Vec<Operation>) -> GroupedRequest {
    GroupedRequest {
        plan: Plan {
            expected_fingerprint: world.fingerprint.clone(),
            operations,
            analysis: None,
        },
        at: time(tick, 0),
        focus_event_ids: vec![],
        max_items: 1000,
        max_characters: 1_000_000,
    }
}
fn preview(world: &World, tick: i64, operations: Vec<Operation>) -> Json {
    event_consequences::preview(world, &grouped(world, tick, operations)).unwrap()
}
fn changes(report: &Json) -> &[Json] {
    report["changes"].as_array().unwrap()
}
fn change<'a>(report: &'a Json, kind: &str) -> &'a Json {
    changes(report)
        .iter()
        .find(|row| row["subject"]["kind"] == kind)
        .unwrap()
}
fn current<'a>(report: &'a Json, kind: &str) -> &'a Json {
    report["currentAtHorizon"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["subject"]["kind"] == kind)
        .unwrap()
}
fn group<'a>(report: &'a Json, id: &str) -> &'a Json {
    report["eventGroups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["event"]["id"] == id)
        .unwrap()
}
fn section_names(rows: &Json) -> Vec<String> {
    rows.as_array()
        .unwrap()
        .iter()
        .flat_map(|row| row["sections"].as_array().unwrap())
        .map(|section| section["section"].as_str().unwrap().into())
        .collect()
}

#[test]
fn closed_requests_require_exact_times_ids_complete_bounds_and_no_duplicate_keys() {
    let world = world();
    let value = serde_json::to_value(request(&world, BIG, i32::MIN)).unwrap();
    assert_eq!(
        serde_json::to_value(event_consequences::decode_event(&value.to_string()).unwrap())
            .unwrap(),
        value
    );
    for (field, bad) in [
        ("eventId", json!(" alias ")),
        ("expectedFingerprint", json!("a")),
        ("maxItems", json!(0)),
        ("maxCharacters", json!(255)),
        ("unexpected", json!(true)),
    ] {
        let mut badvalue = value.clone();
        badvalue[field] = bad;
        assert!(
            event_consequences::decode_event(&badvalue.to_string()).is_err(),
            "{badvalue}"
        );
    }
    for bad in [
        json!(BIG),
        json!("09007199254740993"),
        json!("-0"),
        json!(null),
    ] {
        let mut value = value.clone();
        value["at"]["tick"] = bad;
        assert!(event_consequences::decode_event(&value.to_string()).is_err());
    }
    let duplicate = value
        .to_string()
        .replacen('{', "{\"eventId\":\"duplicate\",", 1);
    assert!(event_consequences::decode_event(&duplicate).is_err());
    let request = grouped(&world, 10, vec![update(ANA, "kind: character\n")]);
    let value = serde_json::to_value(&request).unwrap();
    assert!(event_consequences::decode_grouped(&value.to_string()).is_ok());
    let mut missing = value.clone();
    missing.as_object_mut().unwrap().remove("focusEventIds");
    assert!(event_consequences::decode_grouped(&missing.to_string()).is_err());
    let duplicate = value.to_string().replacen(
        "\"plan\":{",
        "\"plan\":{\"expectedFingerprint\":\"duplicate\",",
        1,
    );
    assert!(event_consequences::decode_grouped(&duplicate).is_err());
    let mut repeated = request;
    repeated.focus_event_ids = vec![EVENT.into(), EVENT.into()];
    assert!(
        event_consequences::decode_grouped(&serde_json::to_string(&repeated).unwrap()).is_err()
    );
}

#[test]
fn exact_signed_extrema_compare_at_t_and_retain_the_event_and_other_same_time_writes() {
    for (tick, order) in [(i64::MIN, i32::MIN), (i64::MAX, i32::MAX), (BIG, 17)] {
        let world = with(
            &world(),
            vec![
                event(
                    EVENT,
                    tick,
                    order,
                    &format!(
                        "effects: [{{id: effect_00000000000000000000000001, target: {ANA}, key: mood, operation: set, value: ready}}]\n"
                    ),
                ),
                event(
                    LATER,
                    tick,
                    order,
                    &format!(
                        "effects: [{{id: effect_00000000000000000000000001, target: {BOB}, key: mood, operation: set, value: changed}}]\n"
                    ),
                ),
                point(WHEN_EVENT, &format!("when: {{event: {EVENT}}}\n")),
                point(
                    INDIRECT,
                    &format!("when: {{state: {{entity: {ANA}, key: mood, equals: ready}}}}\n"),
                ),
            ],
        );
        let original = world
            .records
            .iter()
            .map(|(id, record)| (id.clone(), record.raw_bytes.clone()))
            .collect::<Vec<_>>();
        let report = event_consequences::read(&world, &request(&world, tick, order)).unwrap();
        assert_eq!(
            report["eventTime"],
            json!({"timeline":"main","tick":tick.to_string(),"order":order.to_string()})
        );
        assert_eq!(report["comparison"]["at"], report["eventTime"]);
        assert_eq!(report["comparison"]["eventRetained"], true);
        let state = change(&report, "state");
        assert_eq!(state["before"]["payload"]["value"], "calm");
        assert_eq!(state["after"]["payload"]["value"], "ready");
        assert!(
            changes(&report)
                .iter()
                .all(|row| row["subject"]["entityId"] != BOB
                    && row["subject"]["storyPointId"] != WHEN_EVENT)
        );
        assert_eq!(
            change(&report, "story-point")["after"]["payload"]["eligible"],
            true
        );
        assert_eq!(
            world
                .records
                .iter()
                .map(|(id, record)| (id.clone(), record.raw_bytes.clone()))
                .collect::<Vec<_>>(),
            original
        );
        assert_eq!(report["writesPerformed"], false);
        assert_eq!(report["canApply"], false);
    }
}

#[test]
fn equal_overwrites_and_set_reorder_are_not_effects_but_source_writers_remain_visible() {
    let world = with(
        &world(),
        vec![event(
            EVENT,
            10,
            0,
            &format!(
                "effects:\n- {{id: effect_00000000000000000000000001, target: {ANA}, key: mood, operation: set, value: calm}}\n- {{id: effect_00000000000000000000000002, target: {ANA}, key: labels, operation: set, value: [coin, key]}}\n"
            ),
        )],
    );
    let report = read(&world, 10);
    assert!(changes(&report).is_empty());
    assert_eq!(report["effects"].as_array().unwrap().len(), 2);
    assert!(
        report["currentAtHorizon"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["supersession"] == "unchanged")
    );
    let world = with(
        &world,
        vec![event(
            LATER,
            10,
            1,
            &format!(
                "effects: [{{id: effect_00000000000000000000000001, target: {ANA}, key: mood, operation: set, value: calm}}]\n"
            ),
        )],
    );
    let report = event_consequences::read(&world, &request(&world, 10, 1)).unwrap();
    assert!(changes(&report).is_empty());
    let state = report["currentAtHorizon"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["subject"]["key"] == "mood")
        .unwrap();
    assert_eq!(state["supersession"], "superseded");
    assert_eq!(state["snapshot"]["citations"][0]["entityId"], LATER);
    assert_eq!(state["supersedingCitations"][0]["entityId"], LATER);
}

#[test]
fn null_profile_clear_and_exact_large_integer_keep_typed_native_values() {
    let world = with(
        &world(),
        vec![event(
            EVENT,
            BIG,
            0,
            &format!(
                "effects:\n- {{id: effect_00000000000000000000000003, target: {ANA}, key: notes, operation: clear}}\n- {{id: effect_00000000000000000000000004, target: {ANA}, key: count, operation: set, value: {}}}\n",
                BIG + 1
            ),
        )],
    );
    let report = read(&world, BIG);
    let notes = changes(&report)
        .iter()
        .find(|row| row["subject"]["key"] == "notes")
        .unwrap();
    assert_eq!(notes["before"]["payload"]["value"]["empty"], Json::Null);
    assert_eq!(notes["after"]["presence"], "absent");
    assert_eq!(
        notes["after"]["citations"][0]["memberId"],
        "effect_00000000000000000000000003"
    );
    assert_eq!(notes["after"]["citations"][0]["sourceOrdinal"], 0);
    let count = changes(&report)
        .iter()
        .find(|row| row["subject"]["key"] == "count")
        .unwrap();
    assert_eq!(count["before"]["payload"]["valueRaw"], BIG.to_string());
    assert_eq!(count["after"]["payload"]["valueRaw"], (BIG + 1).to_string());
    assert_eq!(count["after"]["payload"]["valueType"]["type"], "integer");
    assert!(
        report["effects"][1]["effectRaw"]
            .as_str()
            .unwrap()
            .contains(&(BIG + 1).to_string())
    );
}

fn histories() -> World {
    let mut point = point(POINT, "");
    point.frontmatter["lifecycle"]["transitions"] = y(&format!(
        "[{{id: activated, time: {{tick: 10}}, state: active, causing_event: {EVENT}}}, {{id: resolved, time: {{tick: 14}}, state: resolved, causing_event: {EVENT}}}, {{id: failed, time: {{tick: 20}}, state: failed, causing_event: {LATER}}}]"
    ));
    point = source::parse_record(
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&point.frontmatter).unwrap(),
            point.body
        )
        .as_bytes(),
        &point.source_path,
    )
    .unwrap();
    with(
        &world(),
        vec![
            event(
                EVENT,
                10,
                0,
                &format!(
                    "effects: [{{id: effect_00000000000000000000000001, target: {ANA}, key: mood, operation: set, value: ready}}]\n"
                ),
            ),
            event(
                LATER,
                20,
                0,
                &format!(
                    "causes: [{EVENT}]\neffects: [{{id: effect_00000000000000000000000001, target: {ANA}, key: mood, operation: set, value: ready}}]\n"
                ),
            ),
            event(THIRD, 40, 0, &format!("causes: [{LATER}]\n")),
            record(
                "knowledge",
                KNOW,
                &format!(
                    "knower: {ANA}\nclaim: {{key: report, statement: Authored belief, truth: unknown}}\ntransitions:\n- {{id: early, time: {{tick: 0}}, state: suspected, confidence: 0.5, acquisition: heard}}\n- {{id: learned, time: {{tick: 10}}, state: accepted, confidence: 0.9, acquisition: heard, causing_event: {EVENT}, note: Exact authored note, source_entity: {BOB}}}\n- {{id: disputed, time: {{tick: 20}}, state: rejected, confidence: 0.8, acquisition: tested, causing_event: {LATER}}}\n- {{id: forgotten, time: {{tick: 30}}, state: forgotten, confidence: 0.8, acquisition: remembered, causing_event: {EVENT}}}\n"
                ),
            ),
            record(
                "relationship",
                REL,
                &format!(
                    "from: {ANA}\nto: {BOB}\ntransitions:\n- {{id: beginning, time: {{tick: -1}}, metrics: {{count: 0}}, facets: []}}\n- {{id: early, time: {{tick: 0}}, metrics: {{trust: 0.1, count: {BIG}}}, facets: [old]}}\n- {{id: trust, time: {{tick: 10}}, metrics: {{trust: 0.8, count: {}}}, facets: [], causing_event: {EVENT}}}\n",
                    BIG + 1
                ),
            ),
            point,
        ],
    )
}

#[test]
fn caused_native_histories_preserve_original_ordinals_and_delayed_transitions_are_history() {
    let world = histories();
    let report = read(&world, 20);
    let knowledge = change(&report, "knowledge");
    assert_eq!(knowledge["before"]["payload"]["state"], "suspected");
    assert_eq!(knowledge["after"]["payload"]["state"], "accepted");
    assert_eq!(knowledge["after"]["citations"][0]["sourceOrdinal"], 1);
    assert_eq!(knowledge["after"]["citations"][0]["memberId"], "learned");
    let rel = change(&report, "relationship");
    assert_eq!(rel["before"]["citations"][0]["sourceOrdinal"], 1);
    assert_eq!(rel["after"]["citations"][0]["sourceOrdinal"], 2);
    assert_eq!(
        rel["before"]["payload"]["metrics"]["count"],
        BIG.to_string()
    );
    assert_eq!(rel["after"]["payload"]["facets"], json!([]));
    let lifecycle = change(&report, "story-point");
    assert_eq!(lifecycle["before"]["payload"]["storedState"], "dormant");
    assert_eq!(lifecycle["after"]["payload"]["storedState"], "active");
    assert_eq!(lifecycle["after"]["citations"][0]["memberId"], "activated");
    let delayed = report["causedTransitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["transitionId"] == "resolved")
        .unwrap();
    assert_eq!(delayed["atEventTime"], false);
    assert_eq!(delayed["time"]["tick"], "14");
    assert!(
        report["causedTransitions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["transitionId"] != "forgotten")
    );
    let knowledge = current(&report, "knowledge");
    assert_eq!(knowledge["snapshot"]["payload"]["state"], "rejected");
    assert_eq!(knowledge["supersession"], "superseded");
    assert_eq!(knowledge["supersedingCitations"][0]["sourceOrdinal"], 2);
    assert_eq!(
        current(&report, "story-point")["snapshot"]["payload"]["storedState"],
        "failed"
    );
    let report = read(&world, 30);
    assert_eq!(current(&report, "knowledge")["supersession"], "unchanged");
    assert_eq!(
        current(&report, "knowledge")["snapshot"]["payload"]["state"],
        "forgotten"
    );
    assert_eq!(
        change(&report, "knowledge")["after"]["payload"]["state"],
        "accepted"
    );
    let report = read(&world, 40);
    assert_eq!(report["causalSuccessors"].as_array().unwrap().len(), 1);
    assert_eq!(report["causalSuccessors"][0]["event"]["id"], LATER);
}

#[test]
fn unrelated_same_time_history_survives_exclusion_and_delayed_only_history_is_not_local_change() {
    let world = edit(&histories(), KNOW, |record| {
        record.frontmatter["transitions"][1]
            .as_mapping_mut()
            .unwrap()
            .remove("causing_event");
    });
    let report = read(&world, 30);
    assert!(
        !changes(&report)
            .iter()
            .any(|row| row["subject"]["kind"] == "knowledge")
    );
    assert_eq!(
        report["causedTransitions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["kind"] == "knowledge")
            .count(),
        1
    );
    assert_eq!(
        current(&report, "knowledge")["snapshot"]["payload"]["state"],
        "forgotten"
    );
    let other = "know_00000000000000000000000002";
    let world = with(
        &histories(),
        vec![record(
            "knowledge",
            other,
            &format!(
                "knower: {ANA}\nclaim: {{key: other, statement: Separate report}}\ntransitions: [{{id: same-time-unrelated, time: {{tick: 10}}, state: rejected, confidence: 0.9, acquisition: tested}}]\n"
            ),
        )],
    );
    let report = read(&world, 10);
    assert!(
        !changes(&report)
            .iter()
            .any(|row| row["subject"]["knowledgeId"] == other)
    );
    assert_eq!(
        current(&report, "knowledge")["snapshot"]["citations"][0]["sourceOrdinal"],
        1
    );
    assert_eq!(current(&report, "knowledge")["supersession"], "unchanged");
}

#[test]
fn first_caused_learning_has_known_absence_and_quiet_event_is_a_complete_empty_read() {
    let world = with(
        &world(),
        vec![
            event(EVENT, 10, 0, ""),
            record(
                "knowledge",
                KNOW,
                &format!(
                    "knower: {ANA}\nclaim: {{key: report, statement: Authored belief, truth: unknown}}\ntransitions: [{{id: first, time: {{tick: 10}}, state: accepted, confidence: 1, acquisition: heard, causing_event: {EVENT}}}]\n"
                ),
            ),
        ],
    );
    let report = read(&world, 10);
    let learned = change(&report, "knowledge");
    assert_eq!(learned["before"]["presence"], "absent");
    assert_eq!(learned["before"]["payload"], Json::Null);
    assert_eq!(learned["after"]["payload"]["state"], "accepted");
    assert_eq!(
        learned["before"]["citations"][0]["fingerprint"],
        world.fingerprint
    );
    let world = with(&world, vec![event(LATER, 10, 0, "")]);
    let mut req = request(&world, 10, 0);
    req.event_id = LATER.into();
    let report = event_consequences::read(&world, &req).unwrap();
    for key in [
        "effects",
        "changes",
        "causedTransitions",
        "outcomes",
        "causalSuccessors",
        "currentAtHorizon",
        "advisories",
    ] {
        assert_eq!(report[key], json!([]), "{key}");
    }
}

#[test]
fn explicit_outcome_halves_are_separate_from_local_changes_and_scene_half_is_normal() {
    let world = with(
        &world(),
        vec![
            event(EVENT, 10, 0, &format!("related_story_points: [{POINT}]\n")),
            point(POINT, ""),
            record(
                "scene",
                SCENE,
                &format!(
                    "status: closed\nlocation: {PLACE}\ntime: {{start: {{tick: 0}}, current: {{tick: 10}}, end: {{tick: 10}}}}\nparticipants: []\noutcome_events: [{EVENT}]\n"
                ),
            ),
        ],
    );
    let report = read(&world, 20);
    assert!(changes(&report).is_empty());
    assert_eq!(report["outcomes"].as_array().unwrap().len(), 2);
    assert_eq!(report["advisories"].as_array().unwrap().len(), 1);
    assert_eq!(
        report["advisories"][0]["citations"][0]["section"],
        "related_story_points"
    );
    let world = edit(&world, POINT, |record| {
        record.frontmatter["outcome_events"] = y(&format!("[{EVENT}]"))
    });
    let report = read(&world, 20);
    assert_eq!(report["advisories"], json!([]));
    assert_eq!(report["outcomes"][1]["reciprocal"], true);
    let world = edit(&world, EVENT, |record| {
        record.frontmatter["related_story_points"] = y("[]")
    });
    let report = read(&world, 20);
    assert_eq!(
        report["advisories"][0]["citations"][0]["section"],
        "outcome_events"
    );
}

#[test]
fn event_read_refuses_wrong_capture_future_foreign_timeline_alias_and_invalid_original() {
    let world = with(&world(), vec![event(EVENT, 10, 0, "")]);
    let mut req = request(&world, 10, 0);
    req.expected_fingerprint = "b".repeat(64);
    assert!(matches!(
        event_consequences::read(&world, &req),
        Err(Error::Conflict(_))
    ));
    assert!(matches!(
        event_consequences::read(&world, &request(&world, 9, 0)),
        Err(Error::NotFound(_))
    ));
    let mut req = request(&world, 10, 0);
    req.at.timeline = "other".into();
    assert!(matches!(
        event_consequences::read(&world, &req),
        Err(Error::NotFound(_))
    ));
    let mut req = request(&world, 10, 0);
    req.event_id = "Literal event".into();
    assert!(matches!(
        event_consequences::read(&world, &req),
        Err(Error::NotFound(_))
    ));
    let mut bad = world.clone();
    bad.records.get_mut(ANA).unwrap().frontmatter["initial_state"]["count"] = y("bad");
    assert!(matches!(
        event_consequences::read(&bad, &request(&bad, 10, 0)),
        Err(Error::Validation(_))
    ));
}

#[test]
fn grouped_changed_effect_source_and_semantic_values_are_separate_from_title_body_and_indirect_eligibility()
 {
    let world = with(
        &world(),
        vec![
            event(
                EVENT,
                10,
                0,
                &format!(
                    "effects: [{{id: effect_00000000000000000000000001, target: {ANA}, key: mood, operation: set, value: calm}}]\n"
                ),
            ),
            point(
                INDIRECT,
                &format!("when: {{state: {{entity: {ANA}, key: mood, equals: ready}}}}\n"),
            ),
        ],
    );
    let report = preview(
        &world,
        10,
        vec![update(
            EVENT,
            &format!(
                "kind: event\ntitle: Renamed event\nbody: Literal new prose.\neffects: [{{id: effect_00000000000000000000000001, target: {ANA}, key: mood, operation: set, value: ready}}]\n"
            ),
        )],
    );
    let event = group(&report, EVENT);
    assert_eq!(event["changes"].as_array().unwrap().len(), 1);
    assert_eq!(event["changes"][0]["subject"]["kind"], "state");
    assert_eq!(section_names(&event["recordChanges"]), vec!["effects"]);
    assert!(
        event["recordChanges"][0]["citations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|value| value["section"] == "effects")
    );
    assert_eq!(
        section_names(&report["unattributedRecordChanges"]),
        vec!["frontmatter.title", "bodyMarkdown"]
    );
    assert_eq!(
        report["unattributedChanges"][0]["subject"]["storyPointId"],
        INDIRECT
    );
    assert_eq!(report["semantics"]["uniqueCausation"], false);
    assert_eq!(report["canApply"], false);
}

#[test]
fn grouped_initial_state_edit_does_not_borrow_unchanged_effect_provenance() {
    let world = with(
        &world(),
        vec![event(
            EVENT,
            10,
            0,
            &format!(
                "effects: [{{id: effect_00000000000000000000000001, target: {ANA}, key: labels, operation: add-to-set, value: map}}]\n"
            ),
        )],
    );
    let report = preview(
        &world,
        10,
        vec![update(
            ANA,
            &format!(
                "kind: character\ninitial_state: {{mood: calm, count: {BIG}, labels: [key, coin, compass], notes: {{count: {BIG}, empty: null}}}}\n"
            ),
        )],
    );
    assert_eq!(report["eventGroups"], json!([]));
    assert_eq!(report["unattributedChanges"].as_array().unwrap().len(), 1);
    assert_eq!(report["unattributedChanges"][0]["subject"]["key"], "labels");
    assert_eq!(
        section_names(&report["unattributedRecordChanges"]),
        vec!["initial_state.labels"]
    );
}

#[test]
fn grouped_mixed_history_sections_remain_unattributed_but_explicit_contributions_group_semantics() {
    let world = histories();
    let original = world.records[KNOW].get("transitions").unwrap().clone();
    let mut changed = original.clone();
    changed[0]["note"] = y("Uncaused source edit");
    changed[1]["confidence"] = y("0.7");
    let patch = format!(
        "kind: knowledge\ntitle: Changed belief title\ntransitions:\n{}",
        serde_yaml::to_string(&changed).unwrap()
    );
    let report = preview(&world, 10, vec![update(KNOW, &patch)]);
    assert_eq!(
        group(&report, EVENT)["changes"].as_array().unwrap().len(),
        1
    );
    assert_eq!(group(&report, EVENT)["recordChanges"], json!([]));
    assert_eq!(
        section_names(&report["unattributedRecordChanges"]),
        vec!["frontmatter.title", "transitions"]
    );
    let mut changed = original;
    changed[1]["confidence"] = y("0.7");
    let report = preview(
        &world,
        10,
        vec![update(
            KNOW,
            &format!(
                "kind: knowledge\ntransitions:\n{}",
                serde_yaml::to_string(&changed).unwrap()
            ),
        )],
    );
    assert_eq!(
        section_names(&group(&report, EVENT)["recordChanges"]),
        vec!["transitions"]
    );
    assert!(
        report["unattributedRecordChanges"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        group(&report, EVENT)["recordChanges"][0]["citations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|citation| citation["sourceOrdinal"].as_u64().unwrap() <= 1)
    );
}

#[test]
fn grouped_future_only_edits_are_not_deletions_and_empty_histories_retain_literal_source() {
    let world = histories();
    let mut transitions = world.records[KNOW].get("transitions").unwrap().clone();
    transitions[3]["note"] = y("Future-only marker");
    let report = preview(
        &world,
        10,
        vec![update(
            KNOW,
            &format!(
                "kind: knowledge\ntransitions:\n{}",
                serde_yaml::to_string(&transitions).unwrap()
            ),
        )],
    );
    assert_eq!(report["eventGroups"], json!([]));
    assert_eq!(report["unattributedChanges"], json!([]));
    assert_eq!(report["unattributedRecordChanges"][0]["change"], "noop");
    assert!(!report.to_string().contains("Future-only marker"));
    let empty = edit(&world, KNOW, |record| {
        record.frontmatter["transitions"] = y("[]")
    });
    let report = preview(
        &empty,
        10,
        vec![update(
            KNOW,
            "kind: knowledge\nbody: Empty history still has authored prose.\n",
        )],
    );
    assert_eq!(
        section_names(&report["unattributedRecordChanges"]),
        vec!["bodyMarkdown"]
    );
    let future = edit(&world, KNOW, |record| {
        record.frontmatter["transitions"] = y(&format!(
            "[{{id: future, time: {{tick: 30}}, state: accepted, confidence: 1, acquisition: heard, causing_event: {EVENT}}}]"
        ))
    });
    let report = preview(
        &future,
        10,
        vec![update(
            KNOW,
            "kind: knowledge\nbody: Withheld future-only history prose.\n",
        )],
    );
    assert_eq!(report["unattributedRecordChanges"], json!([]));
    assert_eq!(report["unattributedChanges"], json!([]));
    assert!(!report.to_string().contains("Withheld future-only"));
}

#[test]
fn grouped_changed_histories_with_two_explicit_events_are_nonexclusive_and_section_is_unattributed()
{
    let world = histories();
    let mut transitions = world.records[KNOW].get("transitions").unwrap().clone();
    transitions[1]["confidence"] = y("0.6");
    transitions[2]["confidence"] = y("0.7");
    let report = preview(
        &world,
        20,
        vec![update(
            KNOW,
            &format!(
                "kind: knowledge\ntransitions:\n{}",
                serde_yaml::to_string(&transitions).unwrap()
            ),
        )],
    );
    for id in [EVENT, LATER] {
        assert_eq!(group(&report, id)["changes"].as_array().unwrap().len(), 1);
        assert_eq!(
            group(&report, id)["changes"][0]["subject"]["knowledgeId"],
            KNOW
        );
        assert_eq!(group(&report, id)["recordChanges"], json!([]));
    }
    assert_eq!(
        section_names(&report["unattributedRecordChanges"]),
        vec!["transitions"]
    );
    assert_eq!(report["changedSubjects"], 1);
}

#[test]
fn grouped_insertions_preserve_original_positions_and_source_only_noops_are_still_groups() {
    let world = histories();
    let mut transitions = world.records[KNOW].get("transitions").unwrap().clone();
    transitions.as_sequence_mut().unwrap().insert(
        1,
        y("{id: inserted, time: {tick: 5}, state: suspected, confidence: 0.5, acquisition: heard}"),
    );
    let report = preview(
        &world,
        10,
        vec![update(
            KNOW,
            &format!(
                "kind: knowledge\ntransitions:\n{}",
                serde_yaml::to_string(&transitions).unwrap()
            ),
        )],
    );
    assert_eq!(report["eventGroups"], json!([]));
    assert_eq!(report["unattributedChanges"], json!([]));
    let citations = report["unattributedRecordChanges"][0]["citations"]
        .as_array()
        .unwrap();
    assert!(
        citations
            .iter()
            .any(|citation| citation["memberId"] == "learned" && citation["sourceOrdinal"] == 2)
    );
    let world = with(
        &world,
        vec![event(
            EVENT,
            10,
            0,
            &format!(
                "effects: [{{id: effect_00000000000000000000000001, target: {ANA}, key: mood, operation: set, value: calm}}]\n"
            ),
        )],
    );
    let report = preview(
        &world,
        10,
        vec![update(
            EVENT,
            &format!(
                "kind: event\neffects: [{{id: effect_00000000000000000000000002, target: {ANA}, key: mood, operation: set, value: calm}}]\n"
            ),
        )],
    );
    assert_eq!(group(&report, EVENT)["changes"], json!([]));
    assert_eq!(
        section_names(&group(&report, EVENT)["recordChanges"]),
        vec!["effects"]
    );
}

#[test]
fn grouped_outcome_source_attribution_and_explicit_empty_focus_are_deterministic() {
    let world = with(
        &world(),
        vec![
            event(EVENT, 10, 0, ""),
            record(
                "scene",
                SCENE,
                &format!(
                    "status: closed\nlocation: {PLACE}\ntime: {{start: {{tick: 0}}, current: {{tick: 10}}, end: {{tick: 10}}}}\nparticipants: []\noutcome_events: []\n"
                ),
            ),
        ],
    );
    let request = grouped(
        &world,
        20,
        vec![update(
            SCENE,
            &format!("kind: scene\noutcome_events: [{EVENT}]\n"),
        )],
    );
    let report = event_consequences::preview(&world, &request).unwrap();
    assert_eq!(
        group(&report, EVENT)["changes"][0]["subject"]["kind"],
        "outcome"
    );
    assert_eq!(
        section_names(&group(&report, EVENT)["recordChanges"]),
        vec!["outcome_events"]
    );
    assert_eq!(
        event_consequences::preview(&world, &request).unwrap(),
        report
    );
    let mut req = grouped(&world, 20, vec![update(ANA, "kind: character\n")]);
    req.focus_event_ids.push(EVENT.into());
    let report = event_consequences::preview(&world, &req).unwrap();
    assert_eq!(group(&report, EVENT)["changes"], json!([]));
    assert_eq!(group(&report, EVENT)["recordChanges"], json!([]));
    req.focus_event_ids = vec![ANA.into()];
    assert!(matches!(
        event_consequences::preview(&world, &req),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn complete_item_character_and_candidate_validation_bounds_refuse_without_partial_reports() {
    let world = histories();
    let report = read(&world, 30);
    let required = report["bounds"]["items"].as_u64().unwrap() as usize;
    let mut req = request(&world, 30, 0);
    req.max_items = required - 1;
    assert!(matches!(
        event_consequences::read(&world, &req),
        Err(Error::InvalidInput(_))
    ));
    let mut req = request(&world, 30, 0);
    req.max_characters = 256;
    assert!(matches!(
        event_consequences::read(&world, &req),
        Err(Error::InvalidInput(_))
    ));
    let mut grouped = grouped(
        &world,
        10,
        vec![update(EVENT, "kind: event\ntitle: Changed title\n")],
    );
    grouped.max_items = 1;
    assert!(matches!(
        event_consequences::preview(&world, &grouped),
        Err(Error::InvalidInput(_))
    ));
    grouped.max_items = 1000;
    grouped.max_characters = 256;
    assert!(matches!(
        event_consequences::preview(&world, &grouped),
        Err(Error::InvalidInput(_))
    ));
    let invalid = GroupedRequest {
        plan: Plan {
            expected_fingerprint: world.fingerprint.clone(),
            operations: vec![Operation::Create {
                draft: format!(
                    "kind: knowledge\nid: know_00000000000000000000000002\ntitle: Invalid candidate\nknower: char_00000000000000000000000009\nclaim: {{key: missing, statement: Missing knower}}\ntransitions: []\n"
                ),
            }],
            analysis: None,
        },
        at: time(10, 0),
        focus_event_ids: vec![],
        max_items: 1000,
        max_characters: 1_000_000,
    };
    assert!(matches!(
        event_consequences::preview(&world, &invalid),
        Err(Error::Validation(_))
    ));
}

#[test]
fn grouped_preview_retains_explicit_plan_checks_at_their_declared_horizon() {
    let world = histories();
    let mut request = grouped(
        &world,
        10,
        vec![update(
            EVENT,
            "kind: event\ntitle: Explicit check preview\n",
        )],
    );
    request.plan.analysis = Some(wedl::consequence_semantics::decode_analysis(&json!({
        "at":{"timeline":"main","tick":"0","order":"0"},"policy":"required",
        "expectations":[{"id":"authored-check","predicate":{"kind":"state.equals","targetId":ANA,"key":"mood","expectedValueRaw":"\"impossible\""}}]
    }).to_string()).unwrap());
    let report = event_consequences::preview(&world, &request).unwrap();
    assert_eq!(report["at"]["tick"], "10");
    assert_eq!(report["semanticAnalysis"]["at"]["tick"], "0");
    assert_eq!(
        report["semanticAnalysis"]["expectations"][0]["id"],
        "authored-check"
    );
    assert_eq!(
        report["semanticAnalysis"]["expectations"][0]["outcome"],
        "fail"
    );
    assert_eq!(report["semanticAnalysis"]["applyAllowed"], false);
    assert_eq!(report["canApply"], false);
}
