use serde_json::{Value as Json, json};
use std::path::PathBuf;
use wedl::{
    model::{Record, StoryTime, World},
    navigation::relations::{interactions, thread_memberships},
    source,
};

const WORLD: &str = "world_00000000000000000000000000";
const A: &str = "char_00000000000000000000000001";
const B: &str = "char_00000000000000000000000002";
const HIDDEN: &str = "char_00000000000000000000000003";
const MISSING: &str = "char_00000000000000000000000009";
const PLACE: &str = "loc_00000000000000000000000001";
const E1: &str = "event_00000000000000000000000001";
const E2: &str = "event_00000000000000000000000002";
const E3: &str = "event_00000000000000000000000003";
const SCENE: &str = "scene_00000000000000000000000001";
const HYP: &str = "hyp_00000000000000000000000001";
const T1: &str = "thread_00000000000000000000000001";
const T2: &str = "thread_00000000000000000000000002";
const T3: &str = "thread_00000000000000000000000003";

fn record(kind: &str, id: &str, fields: Json) -> Record {
    let mut value = json!({"schema":"wedl/v1","kind":kind,"id":id,"title":id});
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    source::parse_record(
        format!(
            "---\n{}---\nUninterpreted prose α  \r\n",
            serde_yaml::to_string(&value).unwrap()
        )
        .as_bytes(),
        &format!("story/{id}.md"),
    )
    .unwrap()
}
fn world(extra: Vec<Record>) -> World {
    let mut records = vec![
        record(
            "world",
            WORLD,
            json!({"default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"threads":[{"id":T1,"label":"One"},{"id":T2,"label":"Two"},{"id":T3,"label":"Private incidental"}],"state_keys":{},"capabilities":[]}),
        ),
        record(
            "character",
            A,
            json!({"initial_state":{},"threads":[T1,T3]}),
        ),
        record(
            "character",
            B,
            json!({"initial_state":{},"status":"retired","threads":[T1,T2]}),
        ),
        record(
            "character",
            HIDDEN,
            json!({"initial_state":{},"audience":[],"threads":[T1]}),
        ),
        record("location", PLACE, json!({})),
    ];
    records.extend(extra);
    source::world_from_records(PathBuf::new(), records, None).unwrap()
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn ids(values: &[&str]) -> Vec<String> {
    let mut values: Vec<_> = values.iter().map(|value| value.to_string()).collect();
    values.sort();
    values
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
fn event(id: &str, tick: i64, order: i32, participants: Json) -> Record {
    record(
        "event",
        id,
        json!({"time":{"tick":tick,"order":order},"location":PLACE,"participants":participants,"effects":[]}),
    )
}

#[test]
fn interactions_require_explicit_pair_and_preserve_every_authored_role() {
    let world = world(vec![
        event(
            E1,
            5,
            0,
            json!([{"character":A,"role":"witness"},{"character":B},{"character":A,"role":"observer"},{"character":HIDDEN,"role":"incidental"}]),
        ),
        event(E2, 6, 0, json!([{"character":A,"role":"speaker"}])),
        record(
            "scene",
            SCENE,
            json!({"status":"closed","location":PLACE,"time":{"start":{"tick":0},"current":{"tick":10},"end":{"tick":10}},"participants":[{"character":A},{"character":B}]}),
        ),
    ]);
    let before: Vec<_> = world
        .records
        .values()
        .map(|r| r.raw_bytes.clone())
        .collect();
    let value = interactions(&world, A, B, &at(10, 0), 10).unwrap();
    assert_eq!(value["count"], 1);
    assert_eq!(value["items"][0]["eventId"], E1);
    assert_eq!(
        value["items"][0]["roles"][A],
        json!(["witness", "observer"])
    );
    assert_eq!(value["items"][0]["roles"][B], json!([null]));
    assert!(value["items"][0]["roles"].get(HIDDEN).is_none());
    assert_eq!(value["items"][0]["locationId"], PLACE);
    assert_eq!(value["items"][0]["citation"]["section"], "participants");
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
fn interactions_clip_status_scope_location_and_exact_same_tick_order() {
    let participants = json!([{"character":A},{"character":B}]);
    let mut world = world(vec![
        event(E1, 5, 2, participants.clone()),
        event(E2, 5, 3, participants.clone()),
        event(E3, 5, 1, participants),
    ]);
    change(&mut world, E3, "status", json!("draft"));
    change(&mut world, PLACE, "audience", json!([]));
    let before = interactions(&world, A, B, &at(5, 2), 10).unwrap();
    assert_eq!(before["items"].as_array().unwrap().len(), 1);
    assert_eq!(before["items"][0]["locationId"], Json::Null);
    assert_eq!(before["items"][0]["location"], Json::Null);
    change(&mut world, E2, "title", json!("future secret changed"));
    change(&mut world, E2, "participants", json!([]));
    change(&mut world, E3, "title", json!("unsettled secret changed"));
    change(
        &mut world,
        PLACE,
        "title",
        json!("withheld location changed"),
    );
    let after = interactions(&world, A, B, &at(5, 2), 10).unwrap();
    assert_eq!(without_fingerprint(before), without_fingerprint(after));
    change(&mut world, E1, "perspectives", json!(["unavailable"]));
    assert_eq!(
        interactions(&world, A, B, &at(5, 2), 10).unwrap()["count"],
        0
    );
}

#[test]
fn interaction_times_remain_exact_and_other_timelines_do_not_enter_the_pair() {
    let mut world = world(vec![
        event(
            E1,
            i64::MIN,
            i32::MIN,
            json!([{"character":A},{"character":B}]),
        ),
        event(
            E2,
            9_007_199_254_740_993,
            i32::MAX,
            json!([{"character":A},{"character":B}]),
        ),
        event(E3, 0, 0, json!([{"character":A},{"character":B}])),
    ]);
    change(
        &mut world,
        E3,
        "time",
        json!({"timeline":"other","tick":0,"order":0}),
    );
    let value = interactions(&world, A, B, &at(i64::MAX, i32::MAX), 10).unwrap();
    assert_eq!(value["count"], 2);
    assert_eq!(value["items"][0]["time"]["tick"], i64::MIN.to_string());
    assert_eq!(value["items"][0]["time"]["order"], i32::MIN.to_string());
    assert_eq!(value["items"][1]["time"]["tick"], "9007199254740993");
    assert_eq!(value["items"][1]["time"]["order"], i32::MAX.to_string());
}

#[test]
fn interactions_refuse_unavailable_or_ambiguous_character_selections() {
    let world = world(vec![]);
    let hidden = interactions(&world, A, HIDDEN, &at(0, 0), 10).unwrap_err();
    let absent = interactions(&world, A, MISSING, &at(0, 0), 10).unwrap_err();
    assert_eq!(hidden.to_string(), absent.to_string());
    assert!(interactions(&world, A, A, &at(0, 0), 10).is_err());
    assert!(interactions(&world, A, "title alias", &at(0, 0), 10).is_err());
    assert!(interactions(&world, A, B, &StoryTime::new("unknown", 0, 0).unwrap(), 10).is_err());
}

#[test]
fn memberships_project_only_selected_threads_of_available_requested_records() {
    let mut world = world(vec![
        record(
            "scene",
            SCENE,
            json!({"status":"active","time":{"start":{"tick":1},"current":{"tick":5}},"location":PLACE,"threads":[T1,T3]}),
        ),
        record(
            "hypothesis",
            HYP,
            json!({"status":"proposed","threads":[T1]}),
        ),
        event(E1, 10, 0, json!([])),
        event(E2, 3, 0, json!([])),
    ]);
    change(&mut world, E1, "threads", json!([T1, T3]));
    change(&mut world, E2, "threads", json!([T1]));
    change(&mut world, E2, "status", json!("draft"));
    let value = thread_memberships(
        &world,
        &ids(&[A, B, HIDDEN, MISSING, HYP, SCENE, E1, E2]),
        &ids(&[T1, T2]),
        None,
        10,
    )
    .unwrap();
    assert_eq!(
        value["records"],
        json!([
            {"recordId":A,"threadIds":[T1]},
            {"recordId":B,"threadIds":[T1,T2]},
            {"recordId":E1,"threadIds":[T1]},
            {"recordId":SCENE,"threadIds":[T1]}
        ])
    );
    assert!(!serde_json::to_string(&value).unwrap().contains(T3));
    assert!(!serde_json::to_string(&value).unwrap().contains(HYP));
    assert!(!serde_json::to_string(&value).unwrap().contains(HIDDEN));
    change(&mut world, SCENE, "status", json!("planned"));
    assert_eq!(
        thread_memberships(&world, &ids(&[SCENE]), &ids(&[T1]), None, 10).unwrap()["records"],
        json!([])
    );
}

#[test]
fn dated_memberships_gate_events_and_scene_start_before_reading_membership() {
    let mut world = world(vec![
        event(E1, 5, 2, json!([])),
        event(E2, 5, 3, json!([])),
        record(
            "scene",
            SCENE,
            json!({"status":"closed","time":{"start":{"tick":5,"order":3},"current":{"tick":8},"end":{"tick":8}},"location":PLACE,"threads":[T1]}),
        ),
    ]);
    for id in [E1, E2] {
        change(&mut world, id, "threads", json!([T1]));
    }
    let selected = ids(&[E1, E2, SCENE, HIDDEN]);
    let before = thread_memberships(&world, &selected, &ids(&[T1]), Some(&at(5, 2)), 10).unwrap();
    assert_eq!(before["records"], json!([{"recordId":E1,"threadIds":[T1]}]));
    change(&mut world, E2, "threads", json!([T2]));
    change(&mut world, SCENE, "threads", json!([]));
    change(&mut world, HIDDEN, "threads", json!([T2, T3]));
    let after = thread_memberships(&world, &selected, &ids(&[T1]), Some(&at(5, 2)), 10).unwrap();
    assert_eq!(without_fingerprint(before), without_fingerprint(after));
}

#[test]
fn membership_selectors_are_exact_nonempty_unique_sorted_and_bounded() {
    let world = world(vec![]);
    for records in [
        vec![],
        ids(&[A, A]),
        vec![B.into(), A.into()],
        vec!["not-an-id".into()],
        vec![A.into(); 257],
    ] {
        assert!(thread_memberships(&world, &records, &ids(&[T1]), None, 1000).is_err());
    }
    for threads in [
        vec![],
        ids(&[T1, T1]),
        vec![T2.into(), T1.into()],
        vec!["thread_00000000000000000000000009".into()],
        vec!["not-a-thread".into()],
        vec![T1.into(); 33],
    ] {
        assert!(thread_memberships(&world, &ids(&[A]), &threads, None, 1000).is_err());
    }
    assert!(
        thread_memberships(
            &world,
            &ids(&[A]),
            &ids(&[T1]),
            Some(&StoryTime::new("missing", 0, 0).unwrap()),
            1000
        )
        .is_err()
    );
}

#[test]
fn complete_result_bounds_refuse_instead_of_truncating_both_relation_queries() {
    let world = world(vec![
        event(E1, 0, 0, json!([{"character":A},{"character":B}])),
        event(E2, 1, 0, json!([{"character":A},{"character":B}])),
    ]);
    assert!(
        interactions(&world, A, B, &at(2, 0), 1)
            .unwrap_err()
            .to_string()
            .contains("incomplete")
    );
    assert!(
        thread_memberships(&world, &ids(&[A, B]), &ids(&[T1]), None, 1)
            .unwrap_err()
            .to_string()
            .contains("incomplete")
    );
    for limit in [0, 1001] {
        assert!(interactions(&world, A, B, &at(2, 0), limit).is_err());
        assert!(thread_memberships(&world, &ids(&[A]), &ids(&[T1]), None, limit).is_err());
    }
    assert_eq!(
        interactions(&world, A, B, &at(2, 0), 2).unwrap()["count"],
        2
    );
    assert_eq!(
        thread_memberships(&world, &ids(&[A, B]), &ids(&[T1]), None, 2).unwrap()["count"],
        2
    );
}

#[test]
fn complete_interaction_byte_bound_refuses_many_individually_valid_large_roles() {
    let role = "x".repeat(900_000);
    let events: Vec<_> = (1..=10)
        .map(|index| {
            event(
                &format!("event_{index:026}"),
                index,
                0,
                json!([{"character":A,"role":role},{"character":B}]),
            )
        })
        .collect();
    assert!(
        events
            .iter()
            .all(|record| record.raw_bytes.len() < 1024 * 1024)
    );
    let mut world = world(events);
    let valid = wedl::validation::validate_world(&world);
    assert!(valid.valid, "{:?}", valid.diagnostics);
    let error = interactions(&world, A, B, &at(20, 0), 10).unwrap_err();
    assert!(error.to_string().contains("8 MiB"));
    change(
        &mut world,
        "event_00000000000000000000000010",
        "status",
        json!("draft"),
    );
    assert_eq!(
        interactions(&world, A, B, &at(20, 0), 10).unwrap()["count"],
        9
    );
}
