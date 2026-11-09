use serde_json::Value as Json;
use std::{collections::BTreeMap, path::PathBuf};
use wedl::{
    model::{Record, StoryTime, World},
    source, story,
};

fn record(id: &str, kind: &str, extra: &str) -> Record {
    let yaml = format!("schema: wedl/v1\nid: {id}\nkind: {kind}\ntitle: {id}\n{extra}");
    Record {
        id: id.into(),
        kind: kind.into(),
        title: id.into(),
        source_path: format!("story/{id}.md"),
        frontmatter: serde_yaml::from_str(&yaml).unwrap(),
        body: "Future prose is not an eligibility fact".into(),
        raw_bytes: yaml.into_bytes(),
        blob_oid: None,
    }
}
fn world(records: Vec<Record>) -> World {
    let config = record(
        "world",
        "world",
        "default_timeline: main\ntimelines: [{id: main}, {id: alternate}]\nstate_keys:\n  character:\n    score: {type: integer}\n    moods: {type: string-set}\n    location: {type: entity, kind: location}\n  object:\n    condition: {type: string}\nrelationship_metrics:\n  trust: {min: -1, max: 1, default: 0}\n",
    );
    let mut all = BTreeMap::from([(config.id.clone(), config)]);
    all.extend(
        records
            .into_iter()
            .map(|record| (record.id.clone(), record)),
    );
    World {
        root: PathBuf::new(),
        records: all,
        fingerprint: "fixture".into(),
        revision: None,
    }
}
fn point(id: &str, extra: &str) -> Record {
    record(
        id,
        "story-point",
        &format!(
            "activation_policy: suggest\nlifecycle: {{initial_state: dormant, transitions: []}}\n{extra}"
        ),
    )
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime {
        timeline: "main".into(),
        tick,
        order,
    }
}
fn item<'a>(value: &'a Json, id: &str) -> &'a Json {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["storyPointId"] == id)
        .unwrap()
}
fn replace(world: &mut World, id: &str, key: &str, yaml: &str) {
    world.records.get_mut(id).unwrap().frontmatter[key] = serde_yaml::from_str(yaml).unwrap();
}

#[test]
fn eligibility_is_a_pure_read_of_authored_lifecycle_and_requirements() {
    let mut world = world(vec![
        point("first", ""),
        point(
            "second",
            "requires: [{id: first, states: [resolved]}]\nwhen: {event: cause}\n",
        ),
        record("cause", "event", "time: {tick: 12}\n"),
    ]);
    replace(
        &mut world,
        "first",
        "lifecycle",
        "{initial_state: dormant, transitions: [{id: done, state: resolved, time: {tick: 10}}]}",
    );
    let original = world.records["first"].frontmatter.clone();
    let before = story::story_points(&world, &at(9, 0)).unwrap();
    assert_eq!(item(&before, "first")["eligible"], true);
    assert_eq!(item(&before, "first")["storedState"], "dormant");
    assert_eq!(item(&before, "second")["requirementsSatisfied"], false);
    assert_eq!(item(&before, "second")["conditionSatisfied"], false);
    assert_eq!(
        item(&before, "second")["blockedReasons"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let between = story::story_points(&world, &at(10, 0)).unwrap();
    assert_eq!(item(&between, "first")["storedState"], "resolved");
    assert_eq!(item(&between, "first")["eligible"], false);
    assert_eq!(item(&between, "second")["requirementsSatisfied"], true);
    assert_eq!(item(&between, "second")["eligible"], false);
    assert_eq!(
        item(&story::story_points(&world, &at(12, 0)).unwrap(), "second")["eligible"],
        true
    );
    assert_eq!(world.records["first"].frontmatter, original);
    assert_eq!(
        item(&story::story_points(&world, &at(0, 0)).unwrap(), "first")["storedState"],
        "dormant"
    );
}

#[test]
fn all_condition_kinds_use_exact_historical_values() {
    let world = world(vec![
        record(
            "hero",
            "character",
            "initial_state: {score: 9223372036854775807, moods: [calm, ready]}\n",
        ),
        record("friend", "character", ""),
        record("cause", "event", "time: {tick: 5}\n"),
        record(
            "memory",
            "knowledge",
            "knower: hero\nclaim: {key: gate.open, statement: The gate is open}\ntransitions:\n- {id: learn, time: {tick: 5}, state: accepted, confidence: 0.8}\n- {id: forget, time: {tick: 8}, state: forgotten}\n",
        ),
        record(
            "trust",
            "relationship",
            "from: hero\nto: friend\ntransitions: [{id: trust-now, time: {tick: 5}, metrics: {trust: 0.5}}]\n",
        ),
        point(
            "ready",
            "when:\n  all:\n  - state: {entity: hero, key: score, equals: 9223372036854775807}\n  - state: {entity: hero, key: moods, equals: [ready, calm]}\n  - knowledge: {character: hero, claim: gate.open, states: [accepted], minimum_confidence: 0.7}\n  - relationship: {id: trust, metric: trust, min: 0.4, max: 0.6}\n  - any: [{event: cause}, {not: {event: cause}}]\n",
        ),
    ]);
    assert!(story::validate(&world).is_empty());
    assert_eq!(
        item(&story::story_points(&world, &at(4, 0)).unwrap(), "ready")["eligible"],
        false
    );
    assert_eq!(
        item(&story::story_points(&world, &at(5, 0)).unwrap(), "ready")["eligible"],
        true
    );
    assert_eq!(
        item(&story::story_points(&world, &at(8, 0)).unwrap(), "ready")["eligible"],
        false
    );
}

#[test]
fn old_fields_malformed_rules_and_wrong_typed_values_are_rejected() {
    let base = world(vec![
        record("hero", "character", "initial_state: {score: 1}\n"),
        point("p", ""),
    ]);
    for key in ["trigger", "dependencies", "on_activate", "repeat_policy"] {
        let mut candidate = base.clone();
        replace(&mut candidate, "p", key, "{}");
        assert!(
            !story::validate(&candidate).is_empty(),
            "accepted obsolete {key}"
        );
    }
    for expression in [
        "true",
        "null",
        "[]",
        "{all: [], any: []}",
        "{unknown: {}}",
        "{event: missing}",
        "{state: {entity: hero, key: score, equals: 1.0}}",
        "{state: {entity: hero, key: missing, equals: 1}}",
        "{knowledge: {character: hero, claim: x, states: [], minimum_confidence: 0}}",
        "{knowledge: {character: hero, claim: x, states: [accepted], minimum_confidence: 1.1}}",
    ] {
        let mut candidate = base.clone();
        replace(&mut candidate, "p", "when", expression);
        assert!(
            !story::validate(&candidate).is_empty(),
            "accepted malformed {expression}"
        );
        assert!(story::story_points(&candidate, &at(0, 0)).is_err());
    }
    let mut candidate = base.clone();
    replace(&mut candidate, "p", "priority", "0.5");
    assert!(!story::validate(&candidate).is_empty());
}

#[test]
fn lifecycle_validation_checks_states_duplicates_and_future_causes() {
    let base = world(vec![
        record("cause", "event", "time: {tick: 10}\n"),
        point("p", ""),
    ]);
    for lifecycle in [
        "{initial_state: waiting}",
        "{initial_state: dormant, transitions: [{id: one, state: active, time: {tick: 0}}, {id: one, state: resolved, time: {tick: 1}}]}",
        "{initial_state: dormant, transitions: [{id: one, state: active, time: {tick: 9}, causing_event: cause}]}",
        "{initial_state: dormant, transitions: [{id: one, state: active, time: {tick: true}}]}",
    ] {
        let mut candidate = base.clone();
        replace(&mut candidate, "p", "lifecycle", lifecycle);
        assert!(!story::validate(&candidate).is_empty());
    }
    let mut equal = base.clone();
    replace(
        &mut equal,
        "p",
        "lifecycle",
        "{initial_state: dormant, transitions: [{id: a, state: active, time: {tick: 10}, causing_event: cause}, {id: b, state: resolved, time: {tick: 10}}]}",
    );
    assert!(story::story_points(&equal, &at(10, 0)).is_err());
    replace(
        &mut equal,
        "p",
        "lifecycle",
        "{initial_state: dormant, transitions: [{id: a, state: active, time: {tick: 10}, causing_event: cause}, {id: b, state: resolved, time: {tick: 10, order: 1}}]}",
    );
    let result = story::story_points(&equal, &at(10, 1)).unwrap();
    assert_eq!(item(&result, "p")["storedState"], "resolved");
    assert_eq!(item(&result, "p")["transitionId"], "b");
}

#[test]
fn cyclic_requirements_and_excessive_expression_depth_fail() {
    let cyclic = world(vec![
        point("a", "requires: [{id: b, states: [resolved]}]\n"),
        point("b", "requires: [{id: a, states: [resolved]}]\n"),
    ]);
    assert!(
        story::validate(&cyclic)
            .iter()
            .any(|d| d.message.contains("cyclic"))
    );
    assert!(story::story_points(&cyclic, &at(0, 0)).is_err());
    let mut deep = world(vec![point("p", "")]);
    let mut value: serde_yaml::Value = serde_yaml::from_str("{all: []}").unwrap();
    for _ in 0..34 {
        value = serde_yaml::Value::Mapping(serde_yaml::Mapping::from_iter([(
            serde_yaml::Value::String("not".into()),
            value,
        )]));
    }
    deep.records.get_mut("p").unwrap().frontmatter["when"] = value;
    assert!(
        story::validate(&deep)
            .iter()
            .any(|d| d.message.contains("depth"))
    );
}

#[test]
fn explicit_causal_ancestors_are_deduplicated_and_never_include_the_future() {
    let mut world = world(vec![
        record("a", "event", "time: {tick: 1}\n"),
        record("b", "event", "time: {tick: 5}\ncauses: [a]\n"),
        record("c", "event", "time: {tick: 10}\ncauses: [b, a]\n"),
        record("future", "event", "time: {tick: 11}\ncauses: [c]\n"),
    ]);
    let value = story::causal(&world, "c", &at(10, 0)).unwrap();
    assert_eq!(value["nodes"].as_array().unwrap().len(), 3);
    assert_eq!(value["edges"].as_array().unwrap().len(), 3);
    assert_eq!(value["nodes"][0]["eventId"], "a");
    assert_eq!(value["nodes"][2]["eventId"], "c");
    assert!(story::causal(&world, "c", &at(9, 0)).is_err());
    replace(&mut world, "b", "time", "{tick: 10}");
    assert!(story::causal(&world, "c", &at(10, 0)).is_err());
    assert!(
        story::validate(&world)
            .iter()
            .any(|d| d.message.contains("strictly earlier"))
    );
}

#[test]
fn causal_walk_has_a_finite_bound_and_rejects_noncanonical_causes() {
    let mut records = vec![record("e0", "event", "time: {tick: 0}\n")];
    for index in 1..1001 {
        records.push(record(
            &format!("e{index}"),
            "event",
            &format!("time: {{tick: {index}}}\ncauses: [e{}]\n", index - 1),
        ));
    }
    let too_large = world(records);
    assert!(
        story::causal(&too_large, "e1000", &at(1000, 0))
            .unwrap_err()
            .to_string()
            .contains("1000 nodes")
    );
    let draft = world(vec![
        record("a", "event", "status: draft\ntime: {tick: 1}\n"),
        record("b", "event", "time: {tick: 2}\ncauses: [a]\n"),
    ]);
    assert!(story::causal(&draft, "b", &at(2, 0)).is_err());
}

#[test]
fn environments_use_explicit_targets_and_inclusive_intervals() {
    let mut world = world(vec![
        record("quay", "location", ""),
        record(
            "hero",
            "character",
            "initial_state: {location: {entity: quay}}\n",
        ),
        record(
            "rain",
            "environment",
            "targets: [quay]\ntime: {start: {tick: 2}, end: {tick: 8}}\nconditions: {weather: rain, visibility: 2, exact_count: 9223372036854775807, fraction: 0.5}\nsensory: [cold]\n",
        ),
    ]);
    assert!(story::validate(&world).is_empty());
    assert_eq!(
        story::environments(&world, "quay", &at(2, 0)).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        story::environments(&world, "quay", &at(8, 0)).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let conditions =
        story::environments(&world, "quay", &at(5, 0)).unwrap()["items"][0]["conditions"].clone();
    assert_eq!(conditions["exact_count"], "9223372036854775807");
    assert_eq!(conditions["visibility"], "2");
    assert_eq!(conditions["fraction"], 0.5);
    assert!(
        story::environments(&world, "quay", &at(8, 1)).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        story::environments(&world, "hero", &at(5, 0)).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    replace(
        &mut world,
        "rain",
        "time",
        "{start: {tick: 8}, end: {tick: 2}}",
    );
    assert!(!story::validate(&world).is_empty());
    assert!(story::environments(&world, "quay", &at(5, 0)).is_err());
}

#[test]
fn malformed_environment_conditions_refuse_without_serialization_panics() {
    let base = world(vec![
        record("quay", "location", ""),
        record(
            "rain",
            "environment",
            "targets: [quay]\ntime: {start: {tick: 0}}\n",
        ),
    ]);
    for invalid in [
        "{weather: {nested: rain}}",
        "{weather: [rain]}",
        "{weather: .nan}",
        "{? [a, b]: rain}",
        "{weather: !special rain}",
    ] {
        let mut candidate = base.clone();
        replace(&mut candidate, "rain", "conditions", invalid);
        assert!(
            !story::validate(&candidate).is_empty(),
            "accepted {invalid}"
        );
        assert!(story::environments(&candidate, "quay", &at(0, 0)).is_err());
    }
}

#[test]
fn revised_real_fixtures_validate_and_replay_at_authored_boundaries() {
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut total = 0;
    for name in [
        "tideglass",
        "ash_archive",
        "frontiersmen",
        "chronology_conformance",
    ] {
        let world = source::load_world(examples.join(name), None).unwrap();
        let diagnostics = story::validate(&world);
        assert!(diagnostics.is_empty(), "{name}: {diagnostics:?}");
        total += world
            .records
            .values()
            .filter(|r| r.kind == "story-point")
            .count();
    }
    assert_eq!(total, 47);
    let tideglass = source::load_world(examples.join("tideglass"), None).unwrap();
    let account = &tideglass
        .resolve("Account for the Log")
        .or_else(|_| tideglass.resolve("Account for Log"))
        .unwrap()
        .id;
    assert_eq!(
        item(
            &story::story_points(&tideglass, &at(20, 1)).unwrap(),
            account
        )["storedState"],
        "active"
    );
    assert_eq!(
        item(
            &story::story_points(&tideglass, &at(20, 2)).unwrap(),
            account
        )["storedState"],
        "resolved"
    );
    let ferry = &tideglass.resolve("Restore the Ferry").unwrap().id;
    assert_eq!(
        item(&story::story_points(&tideglass, &at(40, 0)).unwrap(), ferry)["storedState"],
        "active"
    );
    let resolved = story::story_points(&tideglass, &at(40, 1)).unwrap();
    assert_eq!(item(&resolved, ferry)["storedState"], "resolved");
    assert_eq!(
        item(&resolved, ferry)["outcomeEvents"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let causal = story::causal(&tideglass, "Ferry Resumed", &at(40, 1)).unwrap();
    assert!(causal["nodes"].as_array().unwrap().len() > 3);
    let ash = source::load_world(examples.join("ash_archive"), None).unwrap();
    let audit = &ash.resolve("Audit the Missing Cards").unwrap().id;
    assert_eq!(
        item(&story::story_points(&ash, &at(0, 0)).unwrap(), audit)["storedState"],
        "dormant"
    );
    assert_eq!(
        item(&story::story_points(&ash, &at(56, 30)).unwrap(), audit)["storedState"],
        "resolved"
    );
}
