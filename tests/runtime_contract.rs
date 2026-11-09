use serde_json::json;
use std::{collections::BTreeMap, path::PathBuf};
use wedl::{
    Error,
    model::{Record, StoryTime, World},
    runtime,
};

fn record(id: &str, kind: &str, extra: &str) -> Record {
    let yaml = format!("schema: wedl/v1\nid: {id}\nkind: {kind}\ntitle: {id}\n{extra}");
    Record {
        id: id.into(),
        kind: kind.into(),
        title: id.into(),
        source_path: format!("story/{id}.md"),
        frontmatter: serde_yaml::from_str(&yaml).unwrap(),
        body: "FUTURE_SECRET must never leak into an as-of context".into(),
        raw_bytes: yaml.into_bytes(),
        blob_oid: None,
    }
}

fn world(records: Vec<Record>) -> World {
    let root = record(
        "world",
        "world",
        "default_timeline: main\ntimelines: [{id: main}, {id: alternate}]\ncurrent_time: {tick: 10, order: 0}\nstate_keys:\n  character:\n    mood: {type: string}\n    items: {type: string-set}\n    obsolete: {type: boolean}\n",
    );
    let mut all = BTreeMap::from([(root.id.clone(), root)]);
    all.extend(
        records
            .into_iter()
            .map(|record| (record.id.clone(), record)),
    );
    World {
        root: PathBuf::new(),
        records: all,
        fingerprint: "fixture-snapshot".into(),
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

#[test]
fn state_replays_canonical_effects_with_deterministic_ties_and_clear_provenance() {
    let world = world(vec![
        record(
            "hero",
            "character",
            "initial_state: {mood: calm, items: [key], obsolete: true}\n",
        ),
        record(
            "event-z",
            "event",
            "time: {tick: 10, order: 2}\neffects:\n- {id: set-last, target: hero, key: mood, operation: set, value: relieved}\n- {target: hero, key: items, operation: add-to-set, value: coin}\n- {target: hero, key: items, operation: add-to-set, value: coin}\n- {target: hero, key: items, operation: remove-from-set, value: key}\n- {id: clear-old, target: hero, key: obsolete, operation: clear}\n",
        ),
        record(
            "event-a",
            "event",
            "time: {tick: 10, order: 2}\neffects: [{target: hero, key: mood, operation: set, value: worried}]\n",
        ),
        record(
            "draft",
            "event",
            "status: draft\ntime: {tick: 1}\neffects: [{target: hero, key: mood, operation: set, value: impossible}]\n",
        ),
        record(
            "other-timeline",
            "event",
            "time: {timeline: alternate, tick: 2}\neffects: [{target: hero, key: mood, operation: set, value: alternate}]\n",
        ),
    ]);
    let before = runtime::state(&world, "hero", &at(10, 1)).unwrap();
    assert_eq!(before["state"]["mood"], "calm");
    let after = runtime::state(&world, "hero", &at(10, 2)).unwrap();
    assert_eq!(after["state"], json!({"mood":"relieved","items":["coin"]}));
    assert_eq!(after["citations"]["mood"]["entityId"], "event-z");
    assert_eq!(after["citations"]["obsolete"]["effectId"], "clear-old");
    assert_eq!(
        after["citations"]["obsolete"]["revision"],
        "fixture-revision"
    );
    // Querying the past again must not reuse/mutate the later replay.
    assert_eq!(
        runtime::state(&world, "hero", &at(0, 0)).unwrap()["state"]["mood"],
        "calm"
    );
}

#[test]
fn knowledge_is_acquired_at_the_authored_time_and_latest_ordinal_wins() {
    let world = world(vec![
        record("hero", "character", ""),
        record(
            "memory",
            "knowledge",
            "knower: hero\nclaim: {key: gate.open, statement: The gate is open}\ntransitions:\n- {id: guess, time: {tick: 10}, state: suspected, confidence: 0.2}\n- {id: learn, time: {tick: 10}, state: accepted, confidence: 0.9}\n- {id: forget, time: {tick: 20}, state: forgotten}\n",
        ),
    ]);
    assert!(
        runtime::knowledge(&world, "hero", &at(9, 0), false).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let known = runtime::knowledge(&world, "hero", &at(10, 0), false).unwrap();
    assert_eq!(known["items"][0]["state"], "accepted");
    assert_eq!(known["items"][0]["transitionId"], "learn");
    assert_eq!(known["items"][0]["time"]["tick"], "10");
    assert!(
        runtime::knowledge(&world, "hero", &at(20, 0), false).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        runtime::knowledge(&world, "hero", &at(20, 0), true).unwrap()["items"][0]["state"],
        "forgotten"
    );
}

#[test]
fn relationships_are_directional_and_historical() {
    let world = world(vec![
        record("alice", "character", ""),
        record("bob", "character", ""),
        record(
            "trust",
            "relationship",
            "from: alice\nto: bob\nrelationship_kind: trust\ntransitions:\n- {id: old, time: {tick: 0}, metrics: {trust: 0.2}, facets: [colleague]}\n- {id: new, time: {tick: 10}, relationship_status: ended, metrics: {trust: -0.5}}\n",
        ),
    ]);
    let before = runtime::relationships(&world, "alice", &at(9, 0)).unwrap();
    assert_eq!(before["items"][0]["metrics"]["trust"], 0.2);
    assert_eq!(before["items"][0]["status"], "active");
    assert_eq!(
        runtime::relationships(&world, "alice", &at(10, 0)).unwrap()["items"][0]["status"],
        "ended"
    );
    assert!(
        runtime::relationships(&world, "bob", &at(10, 0)).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn transcript_separates_actions_and_recollections_and_filters_the_future() {
    let world = world(vec![
        record("hero", "character", ""),
        record(
            "dialogue",
            "conversation",
            "time: {start: {tick: 0}, end: {tick: 10}}\nturns:\n- {id: speech, at: {tick: 1}, speaker: hero, text: Hello}\n- {id: gesture, at: {tick: 2}, kind: action, actors: [hero], text: Waves}\n- {id: future, at: {tick: 10}, speaker: hero, text: Later secret}\nrecollections:\n- {id: memory, at: {tick: 20}, character: hero, summary: A different interpretation}\n",
        ),
    ]);
    let view = runtime::conversation(&world, "dialogue", &at(2, 0)).unwrap();
    assert_eq!(view["beats"].as_array().unwrap().len(), 2);
    assert_eq!(view["verbatimTurns"].as_array().unwrap().len(), 1);
    assert!(view["recollections"].as_array().unwrap().is_empty());
    assert!(
        !serde_json::to_string(&view)
            .unwrap()
            .contains("Later secret")
    );
    assert_eq!(
        runtime::conversation(&world, "dialogue", &at(20, 0)).unwrap()["recollections"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn context_is_explicitly_author_local_and_omits_untimed_prose() {
    let world = world(vec![
        record("hero", "character", "initial_state: {mood: calm}\n"),
        record(
            "scene",
            "scene",
            "status: closed\ntime: {start: {tick: 0}, current: {tick: 10}, end: {tick: 10}}\nparticipants: [{character: hero, from: {tick: 2}, until: {tick: 8}}]\n",
        ),
    ]);
    let packet = runtime::context(&world, "hero", &at(8, 0), 8000).unwrap();
    let encoded = serde_json::to_string(&packet).unwrap();
    assert_eq!(packet["perspective"], "author");
    assert_eq!(packet["scenes"].as_array().unwrap().len(), 1);
    assert!(!encoded.contains("FUTURE_SECRET"));
    assert!(encoded.chars().count() <= 8000);
    assert!(runtime::context(&world, "hero", &at(8, 0), 20).is_err());
    assert!(
        runtime::context(&world, "hero", &at(1, 0), 8000).unwrap()["scenes"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        runtime::context(&world, "hero", &at(9, 0), 8000).unwrap()["scenes"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn invalid_timelines_effect_operations_and_set_types_fail_explicitly() {
    let mut world = world(vec![
        record("hero", "character", "initial_state: {items: 42}\n"),
        record(
            "change",
            "event",
            "time: {tick: 1}\neffects: [{target: hero, key: items, operation: add-to-set, value: coin}]\n",
        ),
    ]);
    assert!(matches!(
        runtime::state(&world, "hero", &at(1, 0)),
        Err(Error::Validation(_))
    ));
    world.records.insert(
        "hero".into(),
        record("hero", "character", "initial_state: {items: []}\n"),
    );
    world.records.insert("change".into(),record("change","event","time: {tick: 1}\neffects: [{target: hero, key: items, operation: increment, value: 1}]\n"));
    assert!(matches!(
        runtime::state(&world, "hero", &at(1, 0)),
        Err(Error::Unsupported(_))
    ));
    assert!(matches!(
        runtime::timeline(&world, "missing", None),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        runtime::timeline(&world, "alternate", Some(&at(0, 0))),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn temporal_wire_values_preserve_large_integer_coordinates() {
    let world = world(vec![
        record("hero", "character", ""),
        record(
            "last-event",
            "event",
            "time: {tick: 9223372036854775807, order: 2147483647}\neffects: []\n",
        ),
    ]);
    let value = runtime::timeline(&world, "main", Some(&at(i64::MAX, i32::MAX))).unwrap();
    assert_eq!(value["events"][0]["time"]["tick"], "9223372036854775807");
    assert_eq!(value["effectiveTime"]["order"], "2147483647");
}

#[test]
fn author_knowledge_retains_explicit_linked_claims_without_canonical_joins() {
    let world = world(vec![
        record("hero", "character", ""),
        record(
            "typed",
            "knowledge",
            "knower: hero\nclaim: {key: learned, statement: A learned claim, assertion_id: claim-one}\ntransitions: [{id: learned, time: {tick: 1}, state: suspected, confidence: 0.5, acquisition: heard, note: Author learning note}]\n",
        ),
    ]);
    let read = runtime::knowledge(&world, "hero", &at(1, 0), false).unwrap();
    assert_eq!(read["items"][0]["claim"]["assertion_id"], "claim-one");
    assert_eq!(read["items"][0]["statement"], "A learned claim");
    assert_eq!(read["items"][0]["note"], "Author learning note");
    assert!(read["items"][0].get("value").is_none());
}

#[test]
fn tideglass_preserves_recorded_core_semantics_at_twelve_horizons() {
    use sha2::{Digest, Sha256};
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/tideglass_python_reference.json")).unwrap();
    assert_eq!(expected["provenance"]["commit"], wedl::UPSTREAM_COMMIT);
    let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/tideglass");
    // A corpus edit cannot silently continue using stale reference answers.
    for (path, hash) in expected["provenance"]["sourceFilesSha256"]
        .as_object()
        .unwrap()
    {
        let actual = hex::encode(Sha256::digest(std::fs::read(corpus.join(path)).unwrap()));
        assert_eq!(
            actual,
            hash.as_str().unwrap(),
            "reference source changed: {path}"
        );
    }
    let world = wedl::source::load_world(&corpus, None).unwrap();
    let cases = expected["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 348);
    for case in cases {
        let point = StoryTime {
            timeline: case["time"]["timeline"].as_str().unwrap().into(),
            tick: case["time"]["tick"].as_str().unwrap().parse().unwrap(),
            order: case["time"]["order"].as_str().unwrap().parse().unwrap(),
        };
        let operation = case["operation"].as_str().unwrap();
        let target = case["target"].as_str().unwrap();
        let actual = match operation {
            "state" => runtime::state(&world, target, &point).unwrap()["state"].clone(),
            "knowledge" | "relationships" => {
                let value = if operation == "knowledge" {
                    runtime::knowledge(&world, target, &point, false)
                } else {
                    runtime::relationships(&world, target, &point)
                }
                .unwrap();
                let mut items = value["items"].as_array().unwrap().clone();
                for item in &mut items {
                    item.as_object_mut().unwrap().remove("citation");
                }
                let id_key = if operation == "knowledge" {
                    "knowledgeId"
                } else {
                    "relationshipId"
                };
                items.sort_by(|a, b| a[id_key].as_str().cmp(&b[id_key].as_str()));
                json!(items)
            }
            _ => panic!("unexpected reference operation"),
        };
        assert_eq!(
            actual, case["expected"],
            "{operation} {target} at {}:{}",
            point.tick, point.order
        );
    }
}
