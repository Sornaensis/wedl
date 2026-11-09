use serde_json::Value as Json;
use std::{collections::BTreeMap, path::PathBuf};
use wedl::{
    model::{Record, StoryTime, World},
    navigation::timeline::timeline,
    source,
};

fn record(id: &str, kind: &str, fields: &str) -> Record {
    let yaml = format!("schema: wedl/v1\nid: {id}\nkind: {kind}\ntitle: {id}\n{fields}");
    Record {
        id: id.into(),
        kind: kind.into(),
        title: id.into(),
        source_path: format!("story/{id}.md"),
        frontmatter: serde_yaml::from_str(&yaml).unwrap(),
        body: "# Heading\n\nAn authored summary.\nSecond paragraph.\n".into(),
        raw_bytes: yaml.into_bytes(),
        blob_oid: None,
    }
}
fn world(records: Vec<Record>) -> World {
    let config = record(
        "world",
        "world",
        "default_timeline: main\ntimelines:\n- id: main\n  label: Main story\n  origin: {tick: -9223372036854775808, label: Origin}\n- {id: other, label: Other story}\n",
    );
    let mut values = BTreeMap::from([(config.id.clone(), config)]);
    values.extend(
        records
            .into_iter()
            .map(|record| (record.id.clone(), record)),
    );
    World {
        root: PathBuf::new(),
        records: values,
        fingerprint: "fixture".into(),
        revision: Some("fixture-revision".into()),
    }
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime {
        timeline: "main".into(),
        tick,
        order,
    }
}
fn event(id: &str, tick: i64, order: i32, status: &str) -> Record {
    record(
        id,
        "event",
        &format!("status: {status}\ntime: {{tick: {tick}, order: {order}}}\n"),
    )
}
fn find<'a>(result: &'a Json, key: &str, id: &str) -> &'a Json {
    result[key]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["entity"]["id"] == id)
        .unwrap()
}
fn replace(world: &mut World, id: &str, field: &str, value: &str) {
    world.records.get_mut(id).unwrap().frontmatter[field] = serde_yaml::from_str(value).unwrap();
}

#[test]
fn chronology_includes_all_event_statuses_and_exact_ordinal_metadata() {
    let world = world(vec![
        event("early", i64::MIN, i32::MIN, "canonical"),
        event("draft", 9_007_199_254_740_993, 1, "draft"),
        event("retired", 9_007_199_254_740_993, 2, "retired"),
        event("latest", i64::MAX, i32::MAX, "canonical"),
        record("elsewhere", "event", "time: {timeline: other, tick: 0}\n"),
    ]);
    let result = timeline(&world, "main", None, 1000).unwrap();
    assert_eq!(result["protocol"], "wedl-rust/v1");
    assert_eq!(result["timeline"], "main");
    assert_eq!(result["timelineDefinition"]["label"], "Main story");
    assert_eq!(
        result["timelineDefinition"]["origin"]["tick"],
        i64::MIN.to_string()
    );
    assert_eq!(result["temporalSemantics"]["spacing"], "ordinal");
    assert_eq!(result["temporalSemantics"]["durationSemantics"], "none");
    assert_eq!(
        result["temporalSemantics"]["intervalEndpoints"],
        "inclusive"
    );
    let points = result["points"].as_array().unwrap();
    assert_eq!(
        points
            .iter()
            .map(|row| row["entity"]["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["early", "draft", "retired", "latest"]
    );
    assert_eq!(points[0]["at"]["tick"], i64::MIN.to_string());
    assert_eq!(points[1]["at"]["tick"], "9007199254740993");
    assert_eq!(points[3]["at"]["order"], i32::MAX.to_string());
    assert_eq!(result["events"].as_array().unwrap().len(), 2);
    assert_eq!(
        points[0]["summary"],
        "An authored summary. Second paragraph."
    );
    assert!(result["spans"].as_array().unwrap().is_empty());
}

#[test]
fn all_time_timeline_includes_lifecycle_and_authored_span_states() {
    let world = world(vec![
        record("hero", "character", ""),
        record("room", "location", ""),
        event("cause", 2, 0, "canonical"),
        event("outcome", 8, 0, "canonical"),
        record(
            "required",
            "story-point",
            "activation_policy: manual\nlifecycle: {initial_state: dormant, transitions: []}\n",
        ),
        record(
            "plot",
            "story-point",
            "status: retired\nactivation_policy: manual\nlifecycle:\n  initial_state: dormant\n  transitions:\n  - {id: start, state: active, time: {tick: 2}, causing_event: cause, note: Dated note}\n  - {id: finish, state: resolved, time: {tick: 8}}\nrequires: [{id: required, states: [resolved]}]\noutcome_events: [outcome]\n",
        ),
        record(
            "scene",
            "scene",
            "status: planned\ntime: {start: {tick: 1}, current: {tick: 1}, end: {tick: 10}}\nlocation: room\nparticipants: [{character: hero, from: {tick: 2}, until: {tick: 8}, role: witness, point_of_view: true}]\nconversations: [conversation]\nstory_points: [plot]\n",
        ),
        record(
            "conversation",
            "conversation",
            "status: draft\ntime: {start: {tick: 3}, end: {tick: 8}}\nscene: scene\nlocation: room\nparticipants: [{character: hero}]\n",
        ),
    ]);
    let result = timeline(&world, "main", None, 1000).unwrap();
    let plot = find(&result, "points", "plot");
    assert_eq!(plot["kind"], "plot-transition");
    assert_eq!(plot["status"], "retired");
    assert_eq!(plot["transitionId"], "start");
    assert_eq!(plot["causingEvent"]["title"], "cause");
    assert_eq!(plot["outcomeEvents"][0]["id"], "outcome");
    assert_eq!(plot["requirements"][0]["id"], "required");
    assert_eq!(plot["requirements"][0]["states"][0], "resolved");
    assert_eq!(plot["citation"]["section"], "lifecycle.transitions[0]");
    let scene = find(&result, "spans", "scene");
    assert_eq!(scene["status"], "planned");
    assert_eq!(scene["participants"][0]["from"]["tick"], "2");
    assert_eq!(scene["participants"][0]["until"]["tick"], "8");
    assert_eq!(scene["participants"][0]["role"], "witness");
    assert_eq!(scene["participants"][0]["pointOfView"], true);
    assert_eq!(scene["conversations"][0]["title"], "conversation");
    assert_eq!(scene["plotThreads"][0]["id"], "plot");
    let conversation = find(&result, "spans", "conversation");
    assert_eq!(conversation["scene"]["title"], "scene");
    assert_eq!(conversation["participants"][0]["from"]["tick"], "3");
    assert_eq!(conversation["participants"][0]["until"]["tick"], "8");
}

#[test]
fn horizon_admits_links_before_names_and_omits_future_span_boundaries() {
    let mut world = world(vec![
        record("hero", "character", ""),
        record("hidden-character", "character", "audience: []\n"),
        record("room", "location", ""),
        record("hidden-room", "location", "perspectives: []\n"),
        event("visible", 5, 1, "canonical"),
        event("future", 5, 2, "canonical"),
        record("hidden", "event", "time: {tick: 0}\naudience: []\n"),
        record(
            "plot",
            "story-point",
            "activation_policy: manual\nlifecycle: {initial_state: dormant, transitions: [{id: admitted, state: active, time: {tick: 5, order: 1}, causing_event: future}, {id: future-member, state: resolved, time: {tick: 5, order: 2}, note: SECRET}] }\noutcome_events: [visible, future, hidden]\n",
        ),
        record(
            "scene",
            "scene",
            "status: active\ntime: {start: {tick: 1}, current: {tick: 5, order: 2}}\nlocation: hidden-room\nparticipants: [{character: hero, from: {tick: 2}}, {character: hidden-character}, {character: future-hero, from: {tick: 5, order: 2}}]\nconversations: [conversation, future-conversation]\n",
        ),
        record("future-hero", "character", ""),
        record(
            "conversation",
            "conversation",
            "status: closed\ntime: {start: {tick: 1}, end: {tick: 5, order: 2}}\nscene: scene\nlocation: room\nparticipants: [{character: hero}]\n",
        ),
        record(
            "future-conversation",
            "conversation",
            "status: active\ntime: {start: {tick: 5, order: 2}}\nlocation: room\n",
        ),
    ]);
    replace(&mut world, "visible", "location", "hidden-room");
    replace(
        &mut world,
        "visible",
        "participants",
        "[{character: hero, role: witness}, {character: hidden-character, role: SECRET}]",
    );
    replace(&mut world, "visible", "causes", "[hidden]");
    let result = timeline(&world, "main", Some(&at(5, 1)), 1000).unwrap();
    assert_eq!(result["points"].as_array().unwrap().len(), 2);
    let visible = find(&result, "points", "visible");
    assert!(visible["location"].is_null());
    assert_eq!(visible["participants"].as_array().unwrap().len(), 1);
    assert_eq!(visible["participants"][0]["title"], "hero");
    assert!(visible["causes"].as_array().unwrap().is_empty());
    let plot = find(&result, "points", "plot");
    assert!(plot["causingEvent"].is_null());
    assert_eq!(plot["outcomeEvents"].as_array().unwrap().len(), 1);
    let scene = find(&result, "spans", "scene");
    assert!(scene["current"].is_null());
    assert!(scene["end"].is_null());
    assert!(scene["location"].is_null());
    assert_eq!(scene["participants"].as_array().unwrap().len(), 1);
    assert!(scene["participants"][0]["until"].is_null());
    assert_eq!(scene["summary"], "");
    assert_eq!(scene["conversations"].as_array().unwrap().len(), 1);
    let conversation = find(&result, "spans", "conversation");
    assert!(conversation["end"].is_null());
    assert_eq!(conversation["summary"], "");
    assert!(!result.to_string().contains("SECRET"));
    assert!(!result.to_string().contains("hidden-"));
    assert!(!result.to_string().contains("future-"));
    world.records.get_mut("future").unwrap().title = "MUTATED FUTURE TITLE".into();
    world.records.get_mut("hidden-character").unwrap().title = "MUTATED HIDDEN TITLE".into();
    world.records.get_mut("future-conversation").unwrap().body = "MUTATED FUTURE BODY".into();
    replace(
        &mut world,
        "scene",
        "time",
        "{start: {tick: 1}, current: {tick: 500}}",
    );
    replace(
        &mut world,
        "conversation",
        "time",
        "{start: {tick: 1}, end: {tick: 700}}",
    );
    assert_eq!(
        result,
        timeline(&world, "main", Some(&at(5, 1)), 1000).unwrap()
    );
}

#[test]
fn inclusive_participant_end_and_scene_cursor_are_exact_source_coordinates() {
    let world = world(vec![
        record("hero", "character", ""),
        record("room", "location", ""),
        record(
            "scene",
            "scene",
            "status: active\ntime: {start: {tick: 5, order: 1}, current: {tick: 5, order: 3}}\nlocation: room\nparticipants: [{character: hero}]\n",
        ),
    ]);
    let before = timeline(&world, "main", Some(&at(5, 0)), 1000).unwrap();
    assert!(before["spans"].as_array().unwrap().is_empty());
    let start = timeline(&world, "main", Some(&at(5, 1)), 1000).unwrap();
    assert_eq!(start["spans"][0]["participants"][0]["from"]["order"], "1");
    assert!(start["spans"][0]["participants"][0]["until"].is_null());
    let end = timeline(&world, "main", Some(&at(5, 3)), 1000).unwrap();
    assert_eq!(end["spans"][0]["participants"][0]["until"]["order"], "3");
    assert_eq!(end["spans"][0]["current"]["order"], "3");
    assert_eq!(end["spans"][0]["summary"], "");
}

#[test]
fn canonical_event_lane_retains_native_participant_roles_and_omits_drafts() {
    let mut world = world(vec![
        record("hero", "character", ""),
        record("room", "location", ""),
        event("first", 1, 0, "canonical"),
        event("draft", 1, 1, "draft"),
    ]);
    replace(&mut world, "first", "location", "room");
    replace(
        &mut world,
        "first",
        "participants",
        "[{character: hero, role: speaker}]",
    );
    let old = wedl::runtime::timeline(&world, "main", None).unwrap();
    let new = timeline(&world, "main", None, 1000).unwrap();
    assert_eq!(old["events"], new["events"]);
    assert_eq!(new["points"].as_array().unwrap().len(), 2);
}

#[test]
fn result_bounds_cover_combined_points_spans_and_admitted_text() {
    let mut world = world(vec![
        event("one", 1, 0, "canonical"),
        record(
            "span",
            "conversation",
            "status: draft\ntime: {start: {tick: 1}}\n",
        ),
    ]);
    assert!(
        timeline(&world, "main", None, 1)
            .unwrap_err()
            .to_string()
            .contains("incomplete")
    );
    assert!(timeline(&world, "main", None, 2).is_ok());
    assert!(timeline(&world, "main", None, 0).is_err());
    assert!(timeline(&world, "main", None, 1001).is_err());
    world.records.get_mut("one").unwrap().body = "x".repeat(8 * 1024 * 1024 + 1);
    assert!(
        timeline(&world, "main", None, 1000)
            .unwrap_err()
            .to_string()
            .contains("8 MiB")
    );
    replace(&mut world, "one", "audience", "[]");
    assert!(timeline(&world, "main", None, 1).is_ok());
}

#[test]
fn invalid_span_order_and_mismatched_timelines_are_refused() {
    let mut world = world(vec![record(
        "scene",
        "scene",
        "status: planned\ntime: {start: {tick: 2}, current: {tick: 1}}\n",
    )]);
    assert!(timeline(&world, "main", None, 1000).is_err());
    replace(
        &mut world,
        "scene",
        "time",
        "{start: {tick: 1}, current: {timeline: other, tick: 2}}",
    );
    assert!(timeline(&world, "main", None, 1000).is_err());
    replace(
        &mut world,
        "scene",
        "time",
        "{start: {tick: 1}, current: {tick: 3}, end: {tick: 2}}",
    );
    assert!(timeline(&world, "main", None, 1000).is_err());
    assert!(timeline(&world, "undeclared", None, 1000).is_err());
    assert!(timeline(&world, "other", Some(&at(1, 0)), 1000).is_err());
}

#[test]
fn invalid_future_lifecycle_cause_is_withheld_even_after_the_event_occurs() {
    let world = world(vec![
        event("later", 9, 0, "canonical"),
        record(
            "plot",
            "story-point",
            "activation_policy: manual\nlifecycle: {initial_state: dormant, transitions: [{id: change, state: active, time: {tick: 2}, causing_event: later}]}\n",
        ),
    ]);
    for horizon in [None, Some(at(10, 0))] {
        let result = timeline(&world, "main", horizon.as_ref(), 1000).unwrap();
        assert!(find(&result, "points", "plot")["causingEvent"].is_null());
    }
}

#[test]
fn timeline_reads_the_example_worlds_without_mutating_source_or_state() {
    for name in [
        "ash_archive",
        "tideglass",
        "frontiersmen",
        "chronology_conformance",
    ] {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join(name);
        let world = source::load_world(&path, None).unwrap();
        let before: Vec<_> = world
            .records
            .values()
            .map(|record| {
                (
                    record.id.clone(),
                    record.raw_bytes.clone(),
                    record.frontmatter.clone(),
                )
            })
            .collect();
        timeline(&world, &world.default_timeline(), None, 1000).unwrap();
        timeline(
            &world,
            &world.default_timeline(),
            Some(&world.current_time().unwrap()),
            1000,
        )
        .unwrap();
        for (id, raw, fields) in before {
            assert_eq!(world.records[&id].raw_bytes, raw);
            assert_eq!(world.records[&id].frontmatter, fields);
            assert_eq!(
                std::fs::read(path.join(&world.records[&id].source_path)).unwrap(),
                raw
            );
        }
    }
}
