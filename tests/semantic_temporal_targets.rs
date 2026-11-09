use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    consequence_semantics::{self as semantics, Analysis},
    model::{Record, World},
    runtime, source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const ANA: &str = "char_00000000000000000000000001";
const PLACE: &str = "loc_00000000000000000000000001";
const FUTURE: &str = "env_00000000000000000000000001";
const OTHER: &str = "env_00000000000000000000000002";
const EVENT: &str = "event_00000000000000000000000001";
const SECOND: &str = "event_00000000000000000000000002";
const POINT: &str = "sp_00000000000000000000000001";
const CONCEPT: &str = "concept_00000000000000000000000001";

fn record(kind: &str, id: &str, fields: &str) -> Record {
    source::parse_record(
        format!("---\nschema: wedl/v1\nkind: {kind}\nid: {id}\ntitle: Temporal {kind}\n{fields}---\nLiteral prose.\n").as_bytes(),
        &format!("story/{id}.md"),
    ).unwrap()
}

fn assemble(records: Vec<Record>) -> World {
    let world = source::world_from_records(
        PathBuf::from("/semantic-temporal-does-not-read-source"),
        records,
        None,
    )
    .unwrap();
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    world
}

fn base() -> World {
    assemble(vec![
        record(
            "world",
            WORLD,
            "capabilities: [lore-core-v1]\ndefault_timeline: main\ntimelines: [{id: main, label: Main}, {id: other, label: Other}]\nthreads: []\nstate_keys:\n  character:\n    target: {type: entity}\n    targets: {type: entity-set}\n    literal: {type: string}\n    bundle: {type: string}\nstate_profile_bindings: {character: {bundle: notes}}\nprofiles:\n- id: notes\n  label: Notes\n  fields:\n    type: object\n    required: {text: {type: string}}\n    optional: {target: {type: entity, kinds: [environment]}, empty: {type: 'null'}, count: {type: number}}\nrelationship_metrics: {}\n",
        ),
        record(
            "character",
            ANA,
            &format!(
                "initial_state: {{target: {{entity: {PLACE}}}, targets: [], literal: original, bundle: {{text: original, empty: null, count: 9007199254740993}}}}\n"
            ),
        ),
        record("location", PLACE, ""),
        record(
            "environment",
            FUTURE,
            &format!(
                "targets: [{ANA}]\ntime: {{start: {{timeline: main, tick: 20, order: 1}}, end: {{timeline: main, tick: 30, order: 0}}}}\n"
            ),
        ),
        record(
            "environment",
            OTHER,
            &format!("targets: [{ANA}]\ntime: {{start: {{timeline: other, tick: 1, order: 0}}}}\n"),
        ),
    ])
}

fn with(world: &World, records: Vec<Record>) -> World {
    let mut all = world.records.clone();
    all.extend(
        records
            .into_iter()
            .map(|record| (record.id.clone(), record)),
    );
    assemble(all.into_values().collect())
}

fn edit(world: &World, id: &str, change: impl FnOnce(&mut Yaml)) -> World {
    let mut all = world.records.clone();
    let record = all.get_mut(id).unwrap();
    change(&mut record.frontmatter);
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
    assemble(all.into_values().collect())
}

fn event(id: &str, tick: i64, effects: Json) -> Record {
    record(
        "event",
        id,
        &format!("time: {{tick: {tick}}}\neffects: {}\n", effects),
    )
}
fn effect(index: usize, key: &str, operation: &str, value: Option<Json>) -> Json {
    let mut item =
        json!({"id":format!("effect_{index:026}"),"target":ANA,"key":key,"operation":operation});
    if let Some(value) = value {
        item["value"] = value;
    }
    item
}
fn equals(key: &str, value: Json) -> Json {
    json!({"kind":"state.equals","targetId":ANA,"key":key,"expectedValueRaw":value.to_string()})
}
fn absent(key: &str) -> Json {
    json!({"kind":"state.absent","targetId":ANA,"key":key})
}
fn analysis(tick: i64, order: i32, predicates: Vec<Json>) -> Analysis {
    semantics::decode_analysis(&json!({"at":{"timeline":"main","tick":tick.to_string(),"order":order.to_string()},"policy":"advisory","expectations":predicates.into_iter().enumerate().map(|(index,predicate)|json!({"id":format!("check-{index}"),"predicate":predicate})).collect::<Vec<_>>()}).to_string()).unwrap()
}
fn checks(world: &World, tick: i64, order: i32, predicates: Vec<Json>) -> Json {
    semantics::check(world, &analysis(tick, order, predicates)).unwrap()
}
fn outcomes(report: &Json) -> Vec<&str> {
    report["expectations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["outcome"].as_str().unwrap())
        .collect()
}
fn delta(base: &World, candidate: &World, tick: i64, order: i32) -> Json {
    semantics::analyze(base, candidate, &analysis(tick, order, vec![])).unwrap()["delta"].clone()
}

#[test]
fn scalar_effect_pruning_uses_exact_inclusive_target_time_and_preserves_literals() {
    let source = base();
    let candidate = with(
        &source,
        vec![event(
            EVENT,
            5,
            json!([
                effect(1, "target", "set", Some(json!({"entity":FUTURE}))),
                effect(2, "literal", "set", Some(json!(FUTURE)))
            ]),
        )],
    );
    let before = checks(
        &candidate,
        20,
        0,
        vec![
            equals("target", json!({"entity":PLACE})),
            equals("literal", json!(FUTURE)),
            equals("target", json!({"entity":FUTURE})),
        ],
    );
    assert_eq!(outcomes(&before), ["pass", "pass", "fail"]);
    assert_eq!(
        before["expectations"][0]["citations"][0]["section"],
        "initial_state.target"
    );
    let at = checks(
        &candidate,
        20,
        1,
        vec![equals("target", json!({"entity":FUTURE}))],
    );
    assert_eq!(outcomes(&at), ["pass"]);
    assert_eq!(
        at["expectations"][0]["citations"][0]["section"],
        "effects[0]"
    );
    assert_eq!(
        outcomes(&checks(
            &candidate,
            31,
            0,
            vec![equals("target", json!({"entity":FUTURE}))]
        )),
        ["pass"]
    );
    // Ordinary runtime reads retain their existing fold. Semantic projection
    // operates on the immutable source without replacing its stored operands.
    let ordinary =
        runtime::resolved_state(&candidate, ANA, &candidate.story_time(10, None, 0).unwrap())
            .unwrap();
    assert_eq!(ordinary["target"]["entity"].as_str(), Some(FUTURE));
    assert_eq!(
        source.records[ANA].raw_bytes,
        candidate.records[ANA].raw_bytes
    );
    assert_eq!(
        candidate.records[EVENT].get("effects").unwrap()[0]["value"]["entity"].as_str(),
        Some(FUTURE)
    );
}

#[test]
fn changing_unavailable_scalar_references_leaves_same_horizon_delta_empty() {
    let base = with(
        &base(),
        vec![event(
            EVENT,
            5,
            json!([effect(1, "target", "set", Some(json!({"entity":FUTURE})))]),
        )],
    );
    let candidate = edit(&base, EVENT, |raw| {
        raw["effects"][0]["value"]["entity"] = Yaml::String(OTHER.into())
    });
    assert_eq!(delta(&base, &candidate, 10, 0)["changes"], json!([]));
    let later = delta(&base, &candidate, 20, 1);
    assert_eq!(
        later["changes"][0]["before"]["payload"]["value"]["entity"],
        FUTURE
    );
    assert_eq!(
        later["changes"][0]["after"]["payload"]["value"]["entity"],
        PLACE
    );
}

#[test]
fn later_admitted_effect_retains_original_ordinal_after_withheld_sibling() {
    let candidate = with(
        &base(),
        vec![event(
            EVENT,
            5,
            json!([
                effect(1, "target", "set", Some(json!({"entity":FUTURE}))),
                effect(2, "literal", "set", Some(json!("new literal"))),
                effect(3, "target", "clear", None)
            ]),
        )],
    );
    let report = checks(
        &candidate,
        10,
        0,
        vec![absent("target"), equals("literal", json!("new literal"))],
    );
    assert_eq!(outcomes(&report), ["pass", "pass"]);
    assert_eq!(
        report["expectations"][0]["citations"][0]["section"],
        "effects[2]"
    );
    assert_eq!(
        report["expectations"][0]["citations"][0]["effectId"],
        "effect_00000000000000000000000003"
    );
    assert_eq!(
        report["expectations"][1]["citations"][0]["section"],
        "effects[1]"
    );
}

#[test]
fn unavailable_scalar_effect_keeps_prior_event_value_and_its_citation() {
    let source = with(
        &base(),
        vec![event(EVENT, 1, json!([effect(1, "target", "clear", None)]))],
    );
    let candidate = with(
        &source,
        vec![event(
            SECOND,
            5,
            json!([effect(2, "target", "set", Some(json!({"entity":FUTURE})))]),
        )],
    );
    let report = checks(&candidate, 10, 0, vec![absent("target")]);
    assert_eq!(outcomes(&report), ["pass"]);
    assert_eq!(report["expectations"][0]["citations"][0]["entityId"], EVENT);
    assert_eq!(
        report["expectations"][0]["citations"][0]["effectId"],
        "effect_00000000000000000000000001"
    );
    assert_eq!(delta(&source, &candidate, 10, 0)["changes"], json!([]));
}

#[test]
fn initial_scalar_values_and_expected_operands_retain_native_literal_behavior() {
    let source = edit(&base(), ANA, |raw| {
        raw["initial_state"]["target"]["entity"] = Yaml::String(FUTURE.into())
    });
    let candidate = with(
        &source,
        vec![event(
            EVENT,
            5,
            json!([effect(1, "target", "set", Some(json!({"entity":OTHER})))]),
        )],
    );
    let report = checks(
        &candidate,
        10,
        0,
        vec![
            equals("target", json!({"entity":FUTURE})),
            absent("target"),
            equals("target", json!({"entity":PLACE})),
        ],
    );
    assert_eq!(outcomes(&report), ["pass", "fail", "fail"]);
    assert_eq!(
        report["expectations"][0]["actual"]["observed"]["value"]["entity"],
        FUTURE
    );
    assert_eq!(
        report["expectations"][0]["citations"][0]["section"],
        "initial_state.target"
    );
    assert_eq!(delta(&source, &candidate, 10, 0)["changes"], json!([]));
}

#[test]
fn entity_sets_and_member_operations_keep_native_future_reference_values() {
    let candidate = with(
        &base(),
        vec![event(
            EVENT,
            5,
            json!([
                effect(
                    1,
                    "targets",
                    "set",
                    Some(json!([{"entity":PLACE},{"entity":FUTURE}]))
                ),
                effect(2, "targets", "add-to-set", Some(json!({"entity":OTHER}))),
                effect(
                    3,
                    "targets",
                    "remove-from-set",
                    Some(json!({"entity":PLACE}))
                )
            ]),
        )],
    );
    let report = checks(
        &candidate,
        10,
        0,
        vec![equals(
            "targets",
            json!([{"entity":OTHER},{"entity":FUTURE}]),
        )],
    );
    assert_eq!(outcomes(&report), ["pass"]);
    assert_eq!(
        report["expectations"][0]["actual"]["observed"]["value"],
        json!([{"entity":FUTURE},{"entity":OTHER}])
    );
    assert_eq!(
        report["expectations"][0]["citations"][0]["section"],
        "effects[2]"
    );
}

#[test]
fn bound_profiles_keep_complete_values_even_over_entity_declarations() {
    let source = edit(&base(), WORLD, |raw| {
        raw["state_keys"]["character"]["bundle"]["type"] = Yaml::String("entity".into())
    });
    let value = json!({"text":FUTURE,"target":{"kind":"record","id":FUTURE},"empty":null,"count":9007199254740993u64});
    let candidate = with(
        &source,
        vec![event(
            EVENT,
            5,
            json!([effect(1, "bundle", "set", Some(value.clone()))]),
        )],
    );
    let report = checks(&candidate, 10, 0, vec![equals("bundle", value)]);
    assert_eq!(outcomes(&report), ["pass"]);
    let actual = &report["expectations"][0]["actual"]["observed"];
    assert_eq!(actual["value"]["count"], "9007199254740993");
    assert!(actual["value"]["empty"].is_null());
    assert_eq!(
        actual["value"]["target"],
        json!({"kind":"record","id":FUTURE})
    );
    assert_eq!(actual["value"]["text"], FUTURE);
    assert!(
        actual["valueRaw"]
            .as_str()
            .unwrap()
            .contains("9007199254740993")
    );
}

#[test]
fn semantic_story_eligibility_uses_the_same_filtered_scalar_replay() {
    let source = with(
        &base(),
        vec![record(
            "story-point",
            POINT,
            &format!(
                "activation_policy: suggest\nlifecycle: {{initial_state: dormant, transitions: []}}\nwhen: {{state: {{entity: {ANA}, key: target, equals: {{entity: {PLACE}}}}}}}\n"
            ),
        )],
    );
    let candidate = with(
        &source,
        vec![event(
            EVENT,
            5,
            json!([effect(1, "target", "set", Some(json!({"entity":FUTURE})))]),
        )],
    );
    assert_eq!(delta(&source, &candidate, 10, 0)["changes"], json!([]));
    assert!(
        delta(&source, &candidate, 20, 1)["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|change| change["subject"]["kind"] == "story-point"
                && change["after"]["payload"]["eligible"] == false)
    );
}

#[test]
fn empty_filtered_state_never_falls_back_to_an_unfiltered_effect() {
    let source = edit(&base(), ANA, |raw| {
        raw["initial_state"]
            .as_mapping_mut()
            .unwrap()
            .remove("target");
    });
    let source = with(
        &source,
        vec![record(
            "story-point",
            POINT,
            &format!(
                "activation_policy: suggest\nlifecycle: {{initial_state: dormant, transitions: []}}\nwhen: {{not: {{state: {{entity: {ANA}, key: target, equals: {{entity: {FUTURE}}}}}}}}}\n"
            ),
        )],
    );
    let candidate = with(
        &source,
        vec![event(
            EVENT,
            5,
            json!([effect(1, "target", "set", Some(json!({"entity":FUTURE})))]),
        )],
    );
    let report = checks(
        &candidate,
        10,
        0,
        vec![
            absent("target"),
            equals("target", json!({"entity":FUTURE})),
            json!({"kind":"story-point.state","storyPointId":POINT,"state":"dormant"}),
        ],
    );
    assert_eq!(outcomes(&report), ["pass", "fail", "pass"]);
    assert_eq!(delta(&source, &candidate, 10, 0)["changes"], json!([]));
    assert!(
        delta(&source, &candidate, 20, 1)["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|change| change["subject"]["kind"] == "story-point"
                && change["after"]["payload"]["eligible"] == false)
    );
}

#[test]
fn noncore_target_validity_does_not_gain_an_inferred_semantic_gate() {
    let source = with(
        &base(),
        vec![record(
            "lore-concept",
            CONCEPT,
            "domain: temporal.test\nprofile: notes\nvalues: {text: Source}\nstatus: canonical\nvalid: {kind: interval, timeline: main, start: {timeline: main, tick: 20, order: 0}, end: {timeline: main, tick: 30, order: 0}}\naudience: [author]\nperspectives: [author]\n",
        )],
    );
    let candidate = with(
        &source,
        vec![event(
            EVENT,
            5,
            json!([effect(1, "target", "set", Some(json!({"entity":CONCEPT})))]),
        )],
    );
    for tick in [10, 20, 31] {
        assert_eq!(
            outcomes(&checks(
                &candidate,
                tick,
                0,
                vec![equals("target", json!({"entity":CONCEPT}))]
            )),
            ["pass"]
        );
    }
}
