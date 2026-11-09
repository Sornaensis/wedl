//! Same-horizon semantic comparisons retain native folds and exact typed values.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    consequence_semantics::{self, Analysis},
    model::{Record, World},
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const ANA: &str = "char_00000000000000000000000001";
const BOB: &str = "char_00000000000000000000000002";
const PLACE: &str = "loc_00000000000000000000000001";
const EVENT: &str = "event_00000000000000000000000001";
const OTHER_EVENT: &str = "event_00000000000000000000000002";
const KNOWLEDGE: &str = "know_00000000000000000000000001";
const RELATIONSHIP: &str = "rel_00000000000000000000000001";
const POINT: &str = "sp_00000000000000000000000001";
const OTHER_POINT: &str = "sp_00000000000000000000000002";
const SCENE: &str = "scene_00000000000000000000000001";
const ENVIRONMENT: &str = "env_00000000000000000000000001";
const BIG: i64 = 9_007_199_254_740_993;

fn yaml(value: &str) -> Yaml {
    serde_yaml::from_str(value).unwrap()
}

fn record(kind: &str, id: &str, fields: &str) -> Record {
    source::parse_record(
        format!(
            "---\nschema: wedl/v1\nkind: {kind}\nid: {id}\ntitle: Semantic {kind}\n{fields}---\nLiteral source prose.\n"
        )
        .as_bytes(),
        &format!("story/{id}.md"),
    )
    .unwrap()
}

fn assemble(records: Vec<Record>) -> World {
    source::world_from_records(
        PathBuf::from("/semantic-analysis-does-not-read-this-directory"),
        records,
        None,
    )
    .unwrap()
}

fn world() -> World {
    let config = record(
        "world",
        WORLD,
        "capabilities: [lore-core-v1]\ndefault_timeline: main\ntimelines: [{id: main, label: Main}, {id: other, label: Other}]\nstate_keys:\n  character:\n    mood: {type: string}\n    count: {type: integer}\n    number: {type: number}\n    labels: {type: string-set}\n    notes: {type: string}\n  environment:\n    warning: {type: boolean}\nstate_profile_bindings: {character: {notes: notes}}\nprofiles:\n- id: notes\n  label: Bound notes\n  fields:\n    type: object\n    required: {count: {type: number}}\n    optional: {empty: {type: 'null'}, text: {type: string}}\nrelationship_metrics:\n  trust: {min: -1, max: 1, default: 0}\n  count: {min: 0, max: 1000000000000000000, default: 0}\n",
    );
    assemble(vec![
        config,
        record(
            "character",
            ANA,
            &format!(
                "initial_state: {{mood: calm, count: {BIG}, number: 1, labels: [key, coin], notes: {{count: {BIG}, empty: null}}}}\n"
            ),
        ),
        record("character", BOB, ""),
        record("location", PLACE, ""),
    ])
}

fn with(world: &World, extra: Vec<Record>) -> World {
    let mut records = world.records.clone();
    records.extend(extra.into_iter().map(|record| (record.id.clone(), record)));
    assemble(records.into_values().collect())
}

fn edit(world: &World, id: &str, change: impl FnOnce(&mut Record)) -> World {
    let mut records = world.records.clone();
    let item = records.get_mut(id).unwrap();
    change(item);
    *item = source::parse_record(
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&item.frontmatter).unwrap(),
            item.body
        )
        .as_bytes(),
        &item.source_path,
    )
    .unwrap();
    assemble(records.into_values().collect())
}

fn analysis(timeline: &str, tick: i64, order: i32) -> Analysis {
    consequence_semantics::decode_analysis(
        &json!({"at":{"timeline":timeline,"tick":tick.to_string(),"order":order.to_string()},
            "policy":"advisory","expectations":[]})
        .to_string(),
    )
    .unwrap()
}

fn delta(base: &World, candidate: &World, tick: i64, order: i32) -> Json {
    for source in [base, candidate] {
        let report = validation::validate_world(source);
        assert!(report.valid, "{:?}", report.diagnostics);
    }
    consequence_semantics::analyze(base, candidate, &analysis("main", tick, order)).unwrap()["delta"]
        .clone()
}

fn changes(value: &Json) -> &[Json] {
    value["changes"].as_array().unwrap()
}

fn state<'a>(value: &'a Json, key: &str) -> &'a Json {
    changes(value)
        .iter()
        .find(|change| {
            change["subject"]["kind"] == "state"
                && change["subject"]["entityId"] == ANA
                && change["subject"]["key"] == key
        })
        .unwrap()
}

fn event(id: &str, tick: i64, order: i32, fields: &str) -> Record {
    record(
        "event",
        id,
        &format!("time: {{timeline: main, tick: {tick}, order: {order}}}\n{fields}"),
    )
}

fn knowledge(tick: i64, state: &str) -> Record {
    record(
        "knowledge",
        KNOWLEDGE,
        &format!(
            "knower: {ANA}\nclaim: {{key: report, statement: A disputed report, truth: unknown}}\ntransitions: [{{id: first, time: {{timeline: main, tick: {tick}, order: 0}}, state: {state}, confidence: 0.5, acquisition: heard}}]\n"
        ),
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

#[test]
fn state_delta_uses_identical_exact_large_horizon_and_integer_values() {
    let base = world();
    let candidate = with(
        &base,
        vec![event(
            EVENT,
            BIG,
            i32::MAX,
            &format!(
                "effects: [{{id: effect_00000000000000000000000001, target: {ANA}, key: count, operation: set, value: {}}}]\n",
                BIG + 1
            ),
        )],
    );
    assert!(changes(&delta(&base, &candidate, BIG, i32::MAX - 1)).is_empty());
    let report = delta(&base, &candidate, BIG, i32::MAX);
    let change = state(&report, "count");
    assert_eq!(change["before"]["payload"]["value"], BIG.to_string());
    assert_eq!(change["after"]["payload"]["value"], (BIG + 1).to_string());
    assert_eq!(change["before"]["payload"]["valueRaw"], BIG.to_string());
    assert_eq!(report["at"]["tick"], BIG.to_string());
    assert_eq!(report["at"]["order"], i32::MAX.to_string());
    assert_eq!(report["comparison"], "same-horizon");
    assert_eq!(report["writesPerformed"], false);
    assert_eq!(report["canApply"], false);
}

#[test]
fn source_metadata_and_provenance_only_changes_are_not_semantic_changes() {
    let base = world();
    let candidate = with(
        &edit(&base, ANA, |record| {
            record.frontmatter["title"] = yaml("A differently titled actor");
            record.body = "New exact source prose\r\n".into();
            record.source_path = "story/moved-character.md".into();
        }),
        vec![event(
            EVENT,
            1,
            0,
            &format!(
                "effects: [{{id: effect_00000000000000000000000001, target: {ANA}, key: mood, operation: set, value: calm}}]\n"
            ),
        )],
    );
    assert_ne!(base.fingerprint, candidate.fingerprint);
    assert!(changes(&delta(&base, &candidate, 10, 0)).is_empty());
}

#[test]
fn typed_set_order_is_ignored_but_membership_is_not() {
    let base = world();
    let reordered = edit(&base, ANA, |record| {
        record.frontmatter["initial_state"]["labels"] = yaml("[coin, key]");
    });
    assert!(changes(&delta(&base, &reordered, 10, 0)).is_empty());
    let candidate = edit(&reordered, ANA, |record| {
        record.frontmatter["initial_state"]["labels"] = yaml("[coin, key, map]");
    });
    let report = delta(&base, &candidate, 10, 0);
    assert_eq!(changes(&report).len(), 1);
    assert_eq!(
        state(&report, "labels")["after"]["payload"]["value"],
        json!(["coin", "key", "map"])
    );
}

#[test]
fn bound_profile_null_and_clear_are_distinct_and_keep_exact_nested_integers() {
    let base = world();
    let candidate = with(
        &base,
        vec![event(
            EVENT,
            5,
            0,
            &format!(
                "effects: [{{id: effect_00000000000000000000000001, target: {ANA}, key: notes, operation: clear}}]\n"
            ),
        )],
    );
    let report = delta(&base, &candidate, 5, 0);
    let change = state(&report, "notes");
    assert_eq!(change["before"]["presence"], "present");
    assert_eq!(change["before"]["payload"]["value"]["empty"], Json::Null);
    assert_eq!(
        change["before"]["payload"]["value"]["count"],
        BIG.to_string()
    );
    assert!(
        change["before"]["payload"]["valueRaw"]
            .as_str()
            .unwrap()
            .contains(&BIG.to_string())
    );
    assert_eq!(change["before"]["payload"]["profileId"], "notes");
    assert_eq!(change["after"]["presence"], "absent");
    assert_eq!(change["after"]["payload"], Json::Null);
    assert_eq!(
        change["after"]["citations"][0]["effectId"],
        "effect_00000000000000000000000001"
    );
}

#[test]
fn native_state_number_equality_preserves_integer_and_float_types() {
    let base = world();
    let candidate = edit(&base, ANA, |record| {
        record.frontmatter["initial_state"]["number"] = yaml("1.0");
    });
    let report = delta(&base, &candidate, 0, 0);
    assert_eq!(changes(&report).len(), 1);
    assert_eq!(
        state(&report, "number")["before"]["payload"]["valueRaw"],
        "1"
    );
    assert_eq!(
        state(&report, "number")["after"]["payload"]["valueRaw"],
        "1.0"
    );
}

#[test]
fn unrelated_future_other_timeline_and_noncanonical_event_writers_do_not_change_delta() {
    let base = world();
    let future = event(
        EVENT,
        11,
        0,
        &format!(
            "effects: [{{id: effect_00000000000000000000000001, target: {ANA}, key: mood, operation: set, value: later}}]\n"
        ),
    );
    let other = record(
        "event",
        OTHER_EVENT,
        &format!(
            "time: {{timeline: other, tick: 0}}\neffects: [{{id: effect_00000000000000000000000001, target: {ANA}, key: mood, operation: set, value: elsewhere}}]\n"
        ),
    );
    let draft = record(
        "event",
        "event_00000000000000000000000003",
        &format!(
            "status: draft\ntime: {{tick: 0}}\neffects: [{{id: effect_00000000000000000000000001, target: {ANA}, key: mood, operation: set, value: planned}}]\n"
        ),
    );
    let candidate = with(&base, vec![future, other, draft, knowledge(11, "accepted")]);
    assert!(changes(&delta(&base, &candidate, 10, 0)).is_empty());
}

#[test]
fn event_prose_never_supplies_knowledge_and_authored_forgotten_beliefs_remain_distinct_from_truth()
{
    let base = world();
    let candidate = with(
        &base,
        vec![event(EVENT, 1, 0, "body: Ana believes the report.\n")],
    );
    assert!(changes(&delta(&base, &candidate, 10, 0)).is_empty());
    let learned = with(&candidate, vec![knowledge(5, "forgotten")]);
    let report = delta(&candidate, &learned, 5, 0);
    assert_eq!(changes(&report).len(), 1);
    let change = &changes(&report)[0];
    assert_eq!(change["subject"]["kind"], "knowledge");
    assert_eq!(change["before"]["presence"], "absent");
    assert_eq!(change["after"]["payload"]["state"], "forgotten");
    assert!(change["after"]["payload"].get("truth").is_none());
    assert_eq!(
        learned.records[KNOWLEDGE].get("claim").unwrap()["truth"].as_str(),
        Some("unknown")
    );
}

#[test]
fn complete_future_only_history_is_absent_but_retired_knowledge_is_unavailable() {
    let base = with(&world(), vec![knowledge(12, "accepted")]);
    let candidate = with(&base, vec![knowledge(5, "accepted")]);
    let report = delta(&base, &candidate, 10, 0);
    assert_eq!(changes(&report).len(), 1);
    assert_eq!(changes(&report)[0]["before"]["presence"], "absent");
    assert_eq!(changes(&report)[0]["before"]["payload"], Json::Null);
    assert_eq!(
        changes(&report)[0]["before"]["citations"][0]["section"],
        "transitions"
    );
    assert_eq!(changes(&report)[0]["comparable"], true);
    assert_eq!(report["incomparableSubjects"], 0);
    let retired = edit(&candidate, KNOWLEDGE, |record| {
        record.frontmatter["status"] = yaml("retired")
    });
    let report = delta(&candidate, &retired, 10, 0);
    assert_eq!(changes(&report)[0]["after"]["presence"], "unavailable");
}

#[test]
fn relationship_delta_keeps_native_status_direction_and_exact_metrics() {
    let base = with(
        &world(),
        vec![record(
            "relationship",
            RELATIONSHIP,
            &format!(
                "status: retired\nfrom: {ANA}\nto: {BOB}\ntransitions: [{{id: first, time: {{tick: 1}}, relationship_status: active, metrics: {{count: {BIG}, trust: 0.5}}, facets: [friend]}}]\n"
            ),
        )],
    );
    let candidate = edit(&base, RELATIONSHIP, |record| {
        record.frontmatter["transitions"][0]["metrics"]["count"] = yaml(&(BIG + 1).to_string());
    });
    let report = delta(&base, &candidate, 10, 0);
    assert_eq!(changes(&report).len(), 1);
    let change = &changes(&report)[0];
    assert_eq!(
        change["subject"],
        json!({"kind":"relationship","relationshipId":RELATIONSHIP})
    );
    assert_eq!(
        change["before"]["payload"]["metrics"]["count"],
        BIG.to_string()
    );
    assert_eq!(
        change["after"]["payload"]["metrics"]["count"],
        (BIG + 1).to_string()
    );
    let renamed = edit(&base, RELATIONSHIP, |record| {
        record.frontmatter["transitions"][0]["note"] = yaml("Changed prose only");
        record.frontmatter["transitions"][0]["metrics"]["count"] = yaml("9007199254740992.0");
    });
    assert_eq!(changes(&delta(&base, &renamed, 10, 0)).len(), 1);
}

#[test]
fn numeric_metric_spelling_and_note_changes_do_not_create_delta() {
    let base = with(
        &world(),
        vec![record(
            "relationship",
            RELATIONSHIP,
            &format!(
                "from: {ANA}\nto: {BOB}\ntransitions: [{{id: first, time: {{tick: 1}}, relationship_status: active, metrics: {{count: 1}}, facets: [friend]}}]\n"
            ),
        )],
    );
    let candidate = edit(&base, RELATIONSHIP, |record| {
        record.frontmatter["transitions"][0]["note"] = yaml("New source note");
        record.frontmatter["transitions"][0]["metrics"]["count"] = yaml("1.0");
    });
    assert!(changes(&delta(&base, &candidate, 10, 0)).is_empty());
}

#[test]
fn indirect_story_eligibility_and_explicit_outcome_links_are_separate_changes() {
    let base = with(
        &world(),
        vec![
            point(
                POINT,
                &format!("when: {{state: {{entity: {ANA}, key: mood, equals: ready}}}}\n"),
            ),
            point(
                OTHER_POINT,
                &format!("requires: [{{id: {POINT}, states: [resolved]}}]\n"),
            ),
        ],
    );
    let candidate = with(
        &base,
        vec![event(
            EVENT,
            5,
            0,
            &format!(
                "effects: [{{id: effect_00000000000000000000000001, target: {ANA}, key: mood, operation: set, value: ready}}]\nrelated_story_points: [{POINT}]\n"
            ),
        )],
    );
    let candidate = edit(&candidate, POINT, |record| {
        record.frontmatter["lifecycle"]["transitions"] = yaml(&format!(
            "[{{id: complete, time: {{tick: 5}}, state: resolved, causing_event: {EVENT}}}]"
        ));
        record.frontmatter["outcome_events"] = yaml(&format!("[{EVENT}]"));
    });
    let report = delta(&base, &candidate, 5, 0);
    assert_eq!(changes(&report).len(), 4);
    let indirect = changes(&report)
        .iter()
        .find(|change| change["subject"]["storyPointId"] == OTHER_POINT)
        .unwrap();
    assert_eq!(
        indirect["before"]["payload"]["requirementsSatisfied"],
        false
    );
    assert_eq!(indirect["after"]["payload"]["eligible"], true);
    let link = changes(&report)
        .iter()
        .find(|change| change["subject"]["kind"] == "outcome")
        .unwrap();
    assert_eq!(
        link["after"]["payload"],
        json!({"targetKind":"story-point","targetLinked":true,"eventLinked":true})
    );
    assert!(
        changes(&report)
            .iter()
            .all(|change| change.get("causedBy").is_none())
    );
}

#[test]
fn removing_explicit_link_uses_false_flags_and_scene_end_does_not_erase_authored_outcome() {
    let base = with(
        &world(),
        vec![
            event(EVENT, 5, 0, ""),
            record(
                "scene",
                SCENE,
                &format!(
                    "status: closed\nlocation: {PLACE}\ntime: {{start: {{tick: 0}}, current: {{tick: 5}}, end: {{tick: 5}}}}\nparticipants: []\noutcome_events: [{EVENT}]\n"
                ),
            ),
        ],
    );
    let candidate = edit(&base, SCENE, |record| {
        record.frontmatter["outcome_events"] = yaml("[]")
    });
    let report = delta(&base, &candidate, 20, 0);
    assert_eq!(changes(&report).len(), 1);
    assert_eq!(
        changes(&report)[0]["before"]["payload"]["targetLinked"],
        true
    );
    assert_eq!(changes(&report)[0]["after"]["presence"], "present");
    assert_eq!(
        changes(&report)[0]["after"]["payload"]["targetLinked"],
        false
    );
}

#[test]
fn story_point_transition_provenance_alone_is_not_a_behavioral_delta() {
    let base = with(&world(), vec![point(POINT, "")]);
    let base = edit(&base, POINT, |record| {
        record.frontmatter["lifecycle"]["transitions"] =
            yaml("[{id: original, time: {tick: 5}, state: active}]");
    });
    let candidate = edit(&base, POINT, |record| {
        record.frontmatter["lifecycle"]["transitions"] =
            yaml("[{id: renamed, time: {tick: 6}, state: active, note: Updated source prose}]");
    });
    assert!(changes(&delta(&base, &candidate, 10, 0)).is_empty());
    assert_eq!(changes(&delta(&base, &candidate, 5, 0)).len(), 1);
}

#[test]
fn environment_state_admission_uses_start_and_exact_timeline_not_interval_end() {
    let base = with(
        &world(),
        vec![record(
            "environment",
            ENVIRONMENT,
            &format!(
                "targets: [{PLACE}]\ntime: {{start: {{tick: 5}}, end: {{tick: 8}}}}\ninitial_state: {{warning: false}}\n"
            ),
        )],
    );
    let candidate = edit(&base, ENVIRONMENT, |record| {
        record.frontmatter["initial_state"]["warning"] = Yaml::Bool(true)
    });
    assert!(changes(&delta(&base, &candidate, 4, 0)).is_empty());
    assert_eq!(changes(&delta(&base, &candidate, 20, 0)).len(), 1);
    let report =
        consequence_semantics::analyze(&base, &candidate, &analysis("other", 20, 0)).unwrap();
    assert!(changes(&report["delta"]).is_empty());
}

#[test]
fn snapshots_cannot_change_world_identity_or_fold_invalid_source() {
    let base = world();
    let other = edit(&base, WORLD, |record| {
        record.frontmatter["id"] = yaml("world_00000000000000000000000001");
    });
    assert!(consequence_semantics::analyze(&base, &other, &analysis("main", 0, 0)).is_err());
    let invalid = edit(&base, ANA, |record| {
        record.frontmatter["initial_state"]["count"] = Yaml::Bool(true)
    });
    assert!(consequence_semantics::analyze(&base, &invalid, &analysis("main", 0, 0)).is_err());
    assert!(consequence_semantics::analyze(&base, &base, &analysis("missing", 0, 0)).is_err());
}

#[test]
fn complete_subject_and_source_bounds_refuse_without_partial_delta() {
    let base = world();
    let mut records = base.records.values().cloned().collect::<Vec<_>>();
    for index in 10..220 {
        records.push(record("character", &format!("char_{index:026}"), ""));
    }
    let too_many_subjects = assemble(records);
    assert!(
        consequence_semantics::analyze(
            &too_many_subjects,
            &too_many_subjects,
            &analysis("main", 0, 0)
        )
        .is_err()
    );
    let mut oversized = base.clone();
    oversized.records.get_mut(ANA).unwrap().raw_bytes = vec![b'x'; 32 * 1024 * 1024 + 1];
    assert!(
        consequence_semantics::analyze(&oversized, &oversized, &analysis("main", 0, 0)).is_err()
    );
}
