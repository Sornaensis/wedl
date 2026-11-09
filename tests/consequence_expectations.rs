use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    Error,
    consequence_semantics::{self as semantics, Analysis, Policy},
    model::{Record, World},
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const ANA: &str = "char_00000000000000000000000001";
const ROWAN: &str = "char_00000000000000000000000002";
const TOKEN: &str = "obj_00000000000000000000000001";
const PLACE: &str = "loc_00000000000000000000000001";
const EVENT: &str = "event_00000000000000000000000001";
const FUTURE: &str = "event_00000000000000000000000002";
const KNOWLEDGE: &str = "know_00000000000000000000000001";
const RELATIONSHIP: &str = "rel_00000000000000000000000001";
const POINT: &str = "sp_00000000000000000000000001";
const SCENE: &str = "scene_00000000000000000000000001";
const ENVIRONMENT: &str = "env_00000000000000000000000001";
const MISSING: &str = "obj_00000000000000000000000999";

fn record(kind: &str, id: &str, fields: &str) -> Record {
    source::parse_record(
        format!("---\nschema: wedl/v1\nkind: {kind}\nid: {id}\ntitle: Fixture {kind}\n{fields}---\nPrivate source prose.\n").as_bytes(),
        &format!("story/{id}.md"),
    ).unwrap()
}

fn assemble(records: Vec<Record>) -> World {
    source::world_from_records(
        PathBuf::from("/semantic-check-does-not-load-source"),
        records,
        None,
    )
    .unwrap()
}

fn world() -> World {
    let world = assemble(vec![
        record(
            "world",
            WORLD,
            "capabilities: [lore-core-v1]\nthreads: []\ndefault_timeline: main\ntimelines: [{id: main, label: Main}, {id: other, label: Other}]\nstate_keys:\n  character:\n    count: {type: integer}\n    number: {type: number}\n    flag: {type: boolean}\n    name: {type: string}\n    labels: {type: string-set}\n    place: {type: entity, kind: location}\n    absent: {type: string}\n    bundle: {type: string}\n  object:\n    condition: {type: string}\n  environment:\n    visible: {type: boolean}\nstate_profile_bindings: {character: {bundle: bundle}}\nprofiles:\n  - id: bundle\n    label: Literal bundle\n    fields:\n      type: object\n      required: {note: {type: string}, count: {type: number}}\n      optional: {empty: {type: 'null'}}\nrelationship_metrics:\n  trust: {min: -1, max: 1, default: 0}\n  big: {min: 0, max: 10000000000000000000}\n  missing: {}\n",
        ),
        record(
            "character",
            ANA,
            &format!(
                "initial_state: {{count: 9007199254740993, number: 1, flag: true, name: Ana, labels: [a, b], place: {{entity: {PLACE}}}, bundle: {{note: retained, count: 18446744073709551615, empty: null}}}}\n"
            ),
        ),
        record("character", ROWAN, ""),
        record("object", TOKEN, "initial_state: {condition: closed}\n"),
        record("location", PLACE, ""),
        record(
            "event",
            EVENT,
            &format!(
                "time: {{tick: 10}}\nrelated_story_points: [{POINT}]\neffects:\n  - {{id: effect_00000000000000000000000001, target: {TOKEN}, key: condition, operation: set, value: open}}\n  - {{id: effect_00000000000000000000000002, target: {ANA}, key: name, operation: clear}}\n"
            ),
        ),
        record("event", FUTURE, "time: {tick: 40}\neffects: []\n"),
        record(
            "knowledge",
            KNOWLEDGE,
            &format!(
                "knower: {ANA}\nclaim: {{key: token.closed, statement: The token is closed, truth: 'false'}}\ntransitions:\n  - {{id: first, time: {{tick: 5}}, state: uncertain, confidence: 0.2, acquisition: heard}}\n  - {{id: accepted, time: {{tick: 10}}, state: accepted, confidence: 0.9, acquisition: heard, causing_event: {EVENT}}}\n  - {{id: rejected, time: {{tick: 20}}, state: rejected, confidence: 0.9, acquisition: tested, causing_event: {EVENT}}}\n  - {{id: forgotten, time: {{tick: 30}}, state: forgotten, confidence: 0.9, acquisition: remembered, causing_event: {EVENT}}}\n"
            ),
        ),
        record(
            "relationship",
            RELATIONSHIP,
            &format!(
                "from: {ANA}\nto: {ROWAN}\nrelationship_kind: trust\ntransitions:\n  - {{id: met, time: {{tick: 5}}, relationship_status: active, metrics: {{trust: 0.75, big: 9007199254740993}}, facets: [ally, companion]}}\n  - {{id: ended, time: {{tick: 20}}, relationship_status: ended, metrics: {{trust: -0.25}}, facets: [], causing_event: {EVENT}}}\n"
            ),
        ),
        record(
            "story-point",
            POINT,
            &format!(
                "activation_policy: suggest\nlifecycle:\n  initial_state: dormant\n  transitions: [{{id: resolved, state: resolved, time: {{tick: 20}}, causing_event: {EVENT}}}]\nwhen: {{state: {{entity: {TOKEN}, key: condition, equals: open}}}}\noutcome_events: [{EVENT}]\n"
            ),
        ),
        record(
            "scene",
            SCENE,
            &format!(
                "status: active\nlocation: {PLACE}\ntime: {{start: {{tick: 5}}, current: {{tick: 10}}}}\nparticipants: [{{character: {ANA}, point_of_view: true}}]\noutcome_events: [{EVENT}]\n"
            ),
        ),
        record(
            "environment",
            ENVIRONMENT,
            &format!(
                "targets: [{ANA}]\ntime: {{start: {{tick: 20}}, end: {{tick: 30}}}}\ninitial_state: {{visible: true}}\n"
            ),
        ),
    ]);
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    world
}

fn edit(world: &World, id: &str, change: impl FnOnce(&mut Yaml)) -> World {
    let mut records = world.records.clone();
    let record = records.get_mut(id).unwrap();
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
    assemble(records.into_values().collect())
}

fn analysis(predicates: Vec<Json>, tick: i64) -> Analysis {
    semantics::decode_analysis(&json!({"at":{"timeline":"main","tick":tick.to_string(),"order":"0"},"policy":"required",
        "expectations":predicates.into_iter().enumerate().map(|(index,predicate)|json!({"id":format!("check-{index}"),"predicate":predicate})).collect::<Vec<_>>()}).to_string()).unwrap()
}

fn check(world: &World, predicates: Vec<Json>, tick: i64) -> Json {
    semantics::check(world, &analysis(predicates, tick)).unwrap()
}

fn equals(target: &str, key: &str, raw: &str) -> Json {
    json!({"kind":"state.equals","targetId":target,"key":key,"expectedValueRaw":raw})
}

fn absent(target: &str, key: &str) -> Json {
    json!({"kind":"state.absent","targetId":target,"key":key})
}

fn outcomes(report: &Json) -> Vec<&str> {
    report["expectations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["outcome"].as_str().unwrap())
        .collect()
}

#[test]
fn seven_explicit_predicates_pass_in_authored_order_without_mutating_source() {
    let world = world();
    let before: Vec<_> = world
        .records
        .values()
        .map(|r| (r.id.clone(), r.raw_bytes.clone(), r.frontmatter.clone()))
        .collect();
    let report = check(
        &world,
        vec![
            equals(TOKEN, "condition", "\"open\""),
            absent(ANA, "name"),
            json!({"kind":"knowledge.state","knowledgeId":KNOWLEDGE,"state":"accepted"}),
            json!({"kind":"relationship.matches","relationshipId":RELATIONSHIP,"values":{"status":"active","metrics":{"trust":"0.75"},"facets":["ally","companion"]}}),
            json!({"kind":"story-point.state","storyPointId":POINT,"state":"dormant"}),
            json!({"kind":"outcome.linked","eventId":EVENT,"targetId":POINT}),
            json!({"kind":"transition.caused","recordId":KNOWLEDGE,"transitionId":"accepted","eventId":EVENT}),
        ],
        10,
    );
    assert_eq!(outcomes(&report), vec!["pass"; 7]);
    assert_eq!(report["applyAllowed"], true);
    assert_eq!(report["expectationsSatisfied"], true);
    assert_eq!(report["writesPerformed"], false);
    for (index, item) in report["expectations"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        assert_eq!(item["id"], format!("check-{index}"));
        assert!(!item["citations"].as_array().unwrap().is_empty());
    }
    assert_eq!(
        before,
        world
            .records
            .values()
            .map(|r| (r.id.clone(), r.raw_bytes.clone(), r.frontmatter.clone()))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        report["at"],
        json!({"timeline":"main","tick":"10","order":"0"})
    );
}

#[test]
fn required_fail_unknown_and_unsupported_block_but_advisory_remains_informational() {
    let world = world();
    let mut request = analysis(
        vec![
            equals(TOKEN, "condition", "\"closed\""),
            absent(MISSING, "condition"),
            absent(ANA, "undeclared"),
        ],
        10,
    );
    let required = semantics::check(&world, &request).unwrap();
    assert_eq!(outcomes(&required), ["fail", "unknown", "unsupported"]);
    assert_eq!(required["expectationsSatisfied"], false);
    assert_eq!(required["applyAllowed"], false);
    request.policy = Policy::Advisory;
    let advisory = semantics::check(&world, &request).unwrap();
    assert_eq!(outcomes(&advisory), outcomes(&required));
    assert_eq!(advisory["expectationsSatisfied"], false);
    assert_eq!(advisory["applyAllowed"], true);
    assert_ne!(advisory["analysisSha256"], required["analysisSha256"]);
    assert_eq!(check(&world, vec![], 10)["expectationsSatisfied"], true);
}

#[test]
fn typed_state_retains_large_integers_sets_profiles_and_numeric_kind() {
    let world = world();
    let bundle = format!(
        "{{\"note\":\"retained\",\"count\":{},\"empty\":null}}",
        u64::MAX
    );
    let report = check(
        &world,
        vec![
            equals(ANA, "count", "9007199254740993"),
            equals(ANA, "count", "9007199254740992"),
            equals(ANA, "number", "1.0"),
            equals(ANA, "number", "1"),
            equals(ANA, "labels", "[\"b\",\"a\"]"),
            equals(ANA, "bundle", &bundle),
            equals(
                ANA,
                "bundle",
                &format!("{{\"note\":\"retained\",\"count\":{}}}", u64::MAX),
            ),
            equals(ANA, "place", &format!("{{\"entity\":\"{PLACE}\"}}")),
        ],
        10,
    );
    assert_eq!(
        outcomes(&report),
        [
            "pass", "fail", "fail", "pass", "pass", "pass", "fail", "pass"
        ]
    );
    let actual = &report["expectations"][0]["actual"]["observed"];
    assert_eq!(actual["valueRaw"], "9007199254740993");
    assert_eq!(actual["value"], "9007199254740993");
    assert!(
        report["expectations"][5]["actual"]["observed"]["valueRaw"]
            .as_str()
            .unwrap()
            .contains(&u64::MAX.to_string())
    );
    for predicate in [
        equals(ANA, "flag", "1"),
        equals(ANA, "count", "true"),
        equals(ANA, "count", "1.0"),
        equals(ANA, "count", "\"9007199254740993\""),
        equals(ANA, "place", "{\"entity\":\"Fixture location\"}"),
    ] {
        assert!(matches!(
            semantics::check(&world, &analysis(vec![predicate], 10)),
            Err(Error::InvalidInput(_))
        ));
    }
}

#[test]
fn absent_state_is_not_null_or_a_missing_owner() {
    let world = world();
    assert_eq!(
        outcomes(&check(
            &world,
            vec![
                absent(ANA, "name"),
                absent(ANA, "bundle"),
                absent(MISSING, "condition")
            ],
            10
        )),
        ["pass", "fail", "unknown"]
    );
    // The native leaf grammar has no whole-key nullable type. A null operand
    // cannot be smuggled in to stand for cleared/missing state.
    assert!(semantics::check(&world, &analysis(vec![equals(ANA, "name", "null")], 10)).is_err());
    assert!(semantics::check(&world, &analysis(vec![equals(ANA, "bundle", "null")], 10)).is_err());
    assert_eq!(
        check(&world, vec![absent(ANA, "name")], 9)["expectations"][0]["outcome"],
        "fail"
    );
}

#[test]
fn exact_references_refuse_aliases_and_wrong_existing_kinds() {
    let world = world();
    for predicate in [
        absent(KNOWLEDGE, "name"),
        json!({"kind":"knowledge.state","knowledgeId":TOKEN,"state":"accepted"}),
        json!({"kind":"relationship.matches","relationshipId":TOKEN,"values":{"status":"active"}}),
        json!({"kind":"story-point.state","storyPointId":ANA,"state":"dormant"}),
        json!({"kind":"outcome.linked","eventId":ANA,"targetId":POINT}),
        json!({"kind":"outcome.linked","eventId":MISSING,"targetId":ANA}),
        json!({"kind":"transition.caused","recordId":TOKEN,"transitionId":"accepted","eventId":EVENT}),
    ] {
        assert!(matches!(
            semantics::check(&world, &analysis(vec![predicate], 10)),
            Err(Error::InvalidInput(_))
        ));
    }
    let mut request = serde_json::to_value(analysis(vec![absent(ANA, "name")], 10)).unwrap();
    request["expectations"][0]["predicate"]["targetId"] = json!("Fixture character");
    assert!(semantics::decode_analysis(&request.to_string()).is_err());
}

#[test]
fn belief_states_remain_separate_from_canonical_truth_and_lifecycle_eligibility() {
    let world = world();
    let report = check(
        &world,
        vec![
            json!({"kind":"knowledge.state","knowledgeId":KNOWLEDGE,"state":"accepted"}),
            equals(TOKEN, "condition", "\"closed\""),
            json!({"kind":"story-point.state","storyPointId":POINT,"state":"dormant"}),
            json!({"kind":"story-point.state","storyPointId":POINT,"state":"active"}),
        ],
        10,
    );
    assert_eq!(outcomes(&report), ["pass", "fail", "pass", "fail"]);
    for (tick, state) in [(20, "rejected"), (30, "forgotten")] {
        assert_eq!(
            check(
                &world,
                vec![json!({"kind":"knowledge.state","knowledgeId":KNOWLEDGE,"state":state})],
                tick
            )["expectations"][0]["outcome"],
            "pass"
        );
    }
}

#[test]
fn explicit_transition_cause_waits_for_its_member_and_never_infers_a_cause() {
    let world = world();
    let delayed = json!({"kind":"transition.caused","recordId":KNOWLEDGE,"transitionId":"forgotten","eventId":EVENT});
    assert_eq!(
        check(&world, vec![delayed.clone()], 20)["expectations"][0]["outcome"],
        "unknown"
    );
    assert_eq!(
        check(&world, vec![delayed], 30)["expectations"][0]["outcome"],
        "pass"
    );
    let report = check(
        &world,
        vec![
            json!({"kind":"transition.caused","recordId":KNOWLEDGE,"transitionId":"first","eventId":EVENT}),
            json!({"kind":"transition.caused","recordId":KNOWLEDGE,"transitionId":"missing","eventId":EVENT}),
            json!({"kind":"transition.caused","recordId":KNOWLEDGE,"transitionId":"accepted","eventId":FUTURE}),
            json!({"kind":"transition.caused","recordId":RELATIONSHIP,"transitionId":"ended","eventId":EVENT}),
            json!({"kind":"transition.caused","recordId":POINT,"transitionId":"resolved","eventId":EVENT}),
        ],
        20,
    );
    assert_eq!(
        outcomes(&report),
        ["fail", "unknown", "unknown", "pass", "pass"]
    );
    assert_eq!(
        report["expectations"][0]["actual"]["causingEvent"],
        json!({"presence":"absent"})
    );
    let ambiguous = edit(&world, RELATIONSHIP, |fm| {
        fm["transitions"][1]["id"] = Yaml::String("met".into())
    });
    let selected = json!({"kind":"transition.caused","recordId":RELATIONSHIP,"transitionId":"met","eventId":EVENT});
    // Duplicate identities now fail complete-source validation even while the
    // second member is future; the latest-value fold cannot hide that failure.
    assert!(matches!(
        semantics::check(&ambiguous, &analysis(vec![selected.clone()], 10)),
        Err(Error::Validation(_))
    ));
    assert!(matches!(
        semantics::check(&ambiguous, &analysis(vec![selected], 20)),
        Err(Error::Validation(_))
    ));
}

#[test]
fn relationships_compare_exact_numeric_tokens_declared_absence_and_ordered_facets() {
    let world = world();
    let item = |values| json!({"kind":"relationship.matches","relationshipId":RELATIONSHIP,"values":values});
    let report = check(
        &world,
        vec![
            item(json!({"metrics":{"trust":"7.500e-1","big":"9007199254740993.0"}})),
            item(json!({"metrics":{"big":"9007199254740992"}})),
            item(json!({"metrics":{"big":"9.007199254740993e15"}})),
            item(json!({"metrics":{"missing":"0"}})),
            item(json!({"metrics":{"undeclared":"0"}})),
            item(json!({"facets":["companion","ally"]})),
        ],
        10,
    );
    assert_eq!(
        outcomes(&report),
        ["pass", "fail", "pass", "fail", "unsupported", "fail"]
    );
    assert_eq!(
        report["expectations"][0]["actual"]["values"]["metrics"]["big"]["valueRaw"],
        "9007199254740993"
    );
    assert_eq!(
        report["expectations"][3]["actual"]["values"]["metrics"]["missing"],
        json!({"presence":"absent"})
    );
    assert!(
        semantics::check(
            &world,
            &analysis(vec![item(json!({"metrics":{"trust":"1.01"}}))], 10)
        )
        .is_err()
    );
}

#[test]
fn selected_metric_definitions_are_closed_and_never_silently_treated_as_unbounded() {
    let world = world();
    let item = |value| json!({"kind":"relationship.matches","relationshipId":RELATIONSHIP,"values":{"metrics":{"trust":value}}});
    let long = edit(&world, WORLD, |fm| {
        fm["relationship_metrics"]["trust"] =
            serde_yaml::from_str("{minimum: -1, maximum: 1}").unwrap()
    });
    assert_eq!(
        check(&long, vec![item("2")], 10)["expectations"][0]["outcome"],
        "unsupported"
    );
    assert_eq!(
        check(&long, vec![item("0.75")], 10)["expectations"][0]["outcome"],
        "unsupported"
    );
    for rules in [
        "{min: -1, minimum: -2, max: 1}",
        "{min: false, max: 1}",
        "{min: 2, max: 1}",
        "{min: -1, max: 1, default: 2}",
        "{min: -1, max: 1, default: missing}",
    ] {
        let unsupported = edit(&world, WORLD, |fm| {
            fm["relationship_metrics"]["trust"] = serde_yaml::from_str(rules).unwrap()
        });
        assert_eq!(
            check(&unsupported, vec![item("0.75")], 10)["expectations"][0]["outcome"],
            "unsupported"
        );
    }
    let out_of_range = edit(&world, RELATIONSHIP, |fm| {
        fm["transitions"][0]["metrics"]["trust"] = Yaml::Number(2.into())
    });
    assert_eq!(
        check(&out_of_range, vec![item("0.75")], 10)["expectations"][0]["outcome"],
        "unsupported"
    );
}

#[test]
fn outcome_links_require_explicit_halves_and_known_unlinked_pairs_fail() {
    let world = world();
    let item = |target| json!({"kind":"outcome.linked","eventId":EVENT,"targetId":target});
    let missing_half = edit(&world, POINT, |fm| {
        fm["outcome_events"] = Yaml::Sequence(vec![])
    });
    assert_eq!(
        outcomes(&check(&missing_half, vec![item(POINT), item(SCENE)], 10)),
        ["fail", "pass"]
    );
    let unlinked = edit(&missing_half, EVENT, |fm| {
        fm["related_story_points"] = Yaml::Sequence(vec![])
    });
    let report = check(&unlinked, vec![item(POINT)], 10);
    assert_eq!(report["expectations"][0]["outcome"], "fail");
    assert_eq!(report["expectations"][0]["actual"]["eventLinked"], false);
    assert_eq!(report["expectations"][0]["actual"]["targetLinked"], false);
    assert_eq!(
        check(&world, vec![item(SCENE)], 9)["expectations"][0]["outcome"],
        "unknown"
    );
    let planned = edit(&world, SCENE, |fm| {
        fm["status"] = Yaml::String("planned".into())
    });
    assert_eq!(
        check(&planned, vec![item(SCENE)], 10)["expectations"][0]["outcome"],
        "pass"
    );
}

#[test]
fn native_family_admission_preserves_status_behavior_and_exact_temporal_boundaries() {
    let world = world();
    let changed = edit(
        &edit(&world, ANA, |fm| {
            fm["status"] = Yaml::String("retired".into())
        }),
        RELATIONSHIP,
        |fm| fm["status"] = Yaml::String("retired".into()),
    );
    assert_eq!(
        outcomes(&check(
            &changed,
            vec![
                equals(ANA, "count", "9007199254740993"),
                json!({"kind":"relationship.matches","relationshipId":RELATIONSHIP,"values":{"status":"active"}})
            ],
            10
        )),
        ["pass", "pass"]
    );
    assert_eq!(
        check(&world, vec![absent(ENVIRONMENT, "visible")], 19)["expectations"][0]["outcome"],
        "unknown"
    );
    assert_eq!(
        check(&world, vec![absent(ENVIRONMENT, "visible")], 20)["expectations"][0]["outcome"],
        "fail"
    );
    assert_eq!(
        check(&world, vec![equals(ENVIRONMENT, "visible", "true")], 31)["expectations"][0]["outcome"],
        "pass"
    );
    assert_eq!(
        check(
            &world,
            vec![json!({"kind":"knowledge.state","knowledgeId":KNOWLEDGE,"state":"accepted"})],
            4
        )["expectations"][0]["outcome"],
        "unknown"
    );
    let mut request = analysis(
        vec![json!({"kind":"knowledge.state","knowledgeId":KNOWLEDGE,"state":"accepted"})],
        10,
    );
    request.at.timeline = "other".into();
    assert_eq!(
        semantics::check(&world, &request).unwrap()["expectations"][0]["outcome"],
        "unknown"
    );
}

#[test]
fn check_decoders_are_closed_duplicate_safe_and_preserve_exact_time_strings() {
    let world = world();
    let good =
        serde_json::to_value(analysis(vec![equals(ANA, "count", "9007199254740993")], 10)).unwrap();
    let mut exact = good.clone();
    exact["at"]["tick"] = json!(i64::MAX.to_string());
    exact["at"]["order"] = json!(i32::MIN.to_string());
    let decoded = semantics::decode_analysis(&exact.to_string()).unwrap();
    assert_eq!(decoded.at.point().unwrap().tick, i64::MAX);
    assert_eq!(decoded.at.point().unwrap().order, i32::MIN);
    for (field, value) in [
        ("tick", json!(10)),
        ("tick", json!("01")),
        ("tick", json!("-0")),
        ("tick", json!("+1")),
        ("tick", json!("9223372036854775808")),
        ("order", json!("2147483648")),
        ("timeline", json!("")),
    ] {
        let mut bad = good.clone();
        bad["at"][field] = value;
        assert!(semantics::decode_analysis(&bad.to_string()).is_err());
    }
    for key in ["at", "policy", "expectations"] {
        let mut bad = good.clone();
        bad.as_object_mut().unwrap().remove(key);
        assert!(semantics::decode_analysis(&bad.to_string()).is_err());
    }
    let duplicate = good.to_string().replacen(
        "\"policy\":\"required\"",
        "\"policy\":\"required\",\"pol\\u0069cy\":\"advisory\"",
        1,
    );
    assert!(semantics::decode_analysis(&duplicate).is_err());
    let raw_duplicate = analysis(vec![equals(ANA, "bundle", "{}")], 10);
    let mut bad = serde_json::to_value(raw_duplicate).unwrap();
    bad["expectations"][0]["predicate"]["expectedValueRaw"] =
        json!("{\"note\":\"one\",\"n\\u006fte\":\"two\"}");
    assert!(semantics::decode_analysis(&bad.to_string()).is_err());
    let request =
        json!({"expectedFingerprint":world.fingerprint,"analysis":good,"maxCharacters":100000});
    assert!(semantics::decode_check(&request.to_string()).is_ok());
    let mut bad = request.clone();
    bad["extra"] = json!(true);
    assert!(semantics::decode_check(&bad.to_string()).is_err());
    let duplicate = request.to_string().replacen(
        "\"maxCharacters\":100000",
        "\"maxCharacters\":100000,\"maxCharacters\":99999",
        1,
    );
    assert!(semantics::decode_check(&duplicate).is_err());
}

#[test]
fn optional_relationship_fields_reject_null_and_metric_operands_never_coerce_numbers() {
    let base=serde_json::to_value(analysis(vec![json!({"kind":"relationship.matches","relationshipId":RELATIONSHIP,"values":{"status":"active"}})],10)).unwrap();
    for values in [
        json!({}),
        json!({"status":null}),
        json!({"metrics":null}),
        json!({"facets":null}),
        json!({"metrics":{"trust":0.75}}),
        json!({"metrics":{"trust":"NaN"}}),
        json!({"metrics":{"trust":"1e309"}}),
        json!({"metrics":{"trust":"01"}}),
        json!({"metrics":{"trust":" 1"}}),
        json!({"metrics":{"trust":"true"}}),
        json!({"facets":[1]}),
        json!({"status":"active","extra":true}),
    ] {
        let mut bad = base.clone();
        bad["expectations"][0]["predicate"]["values"] = values;
        assert!(semantics::decode_analysis(&bad.to_string()).is_err());
    }
    let duplicate = base.to_string().replace(
        "\"values\":{\"status\":\"active\"}",
        "\"values\":{\"metrics\":{\"trust\":\"0\",\"tr\\u0075st\":\"1\"}}",
    );
    assert!(semantics::decode_analysis(&duplicate).is_err());
    for raw in ["9007199254740993.0", "1.0000000000000001", "1e-400"] {
        let value = json!({"at":{"timeline":"main","tick":"10","order":"0"},"policy":"required","expectations":[{"id":"rounded","predicate":equals(ANA,"number",raw)}]});
        assert!(
            semantics::decode_analysis(&value.to_string()).is_err(),
            "{raw}"
        );
    }
}

#[test]
fn complete_counts_operands_and_report_budgets_refuse_without_truncation() {
    let world = world();
    let accepted = analysis((0..100).map(|_| absent(ANA, "absent")).collect(), 10);
    assert_eq!(
        semantics::check(&world, &accepted).unwrap()["expectations"]
            .as_array()
            .unwrap()
            .len(),
        100
    );
    let mut too_many = accepted.clone();
    let mut extra = too_many.expectations[0].clone();
    extra.id = "extra".into();
    too_many.expectations.push(extra);
    assert!(semantics::check(&world, &too_many).is_err());
    let mut duplicate = accepted.clone();
    duplicate.expectations[1].id = duplicate.expectations[0].id.clone();
    assert!(semantics::validate_analysis(&duplicate).is_err());
    let mut huge =
        serde_json::to_value(analysis(vec![equals(ANA, "name", "\"Ana\"")], 10)).unwrap();
    huge["expectations"][0]["predicate"]["expectedValueRaw"] =
        json!(format!("\"{}\"", "x".repeat(65535)));
    assert!(semantics::decode_analysis(&huge.to_string()).is_err());
    let mut deep = huge.clone();
    deep["expectations"][0]["predicate"]["expectedValueRaw"] =
        json!(format!("{}0{}", "[".repeat(33), "]".repeat(33)));
    assert!(semantics::decode_analysis(&deep.to_string()).is_err());
    let mut request=semantics::decode_check(&json!({"expectedFingerprint":world.fingerprint,"analysis":accepted,"maxCharacters":1000000}).to_string()).unwrap();
    let report = semantics::check_request(&world, &request).unwrap();
    let length = serde_json::to_string(&report).unwrap().chars().count();
    request.max_characters = length;
    assert_eq!(
        semantics::check_request(&world, &request).unwrap()["expectations"]
            .as_array()
            .unwrap()
            .len(),
        100
    );
    request.max_characters = length - 100;
    assert!(semantics::check_request(&world, &request).is_err());
}

#[test]
fn source_pin_invalid_source_and_world_identity_are_checked_before_semantic_success() {
    let world = world();
    let analysis = analysis(vec![equals(TOKEN, "condition", "\"open\"")], 10);
    let mut request=semantics::decode_check(&json!({"expectedFingerprint":world.fingerprint,"analysis":analysis,"maxCharacters":100000}).to_string()).unwrap();
    request.expected_fingerprint = "0".repeat(64);
    assert!(matches!(
        semantics::check_request(&world, &request),
        Err(Error::Conflict(_))
    ));
    let invalid = edit(&world, EVENT, |fm| {
        fm["effects"][0]["value"] = Yaml::Bool(true)
    });
    assert!(matches!(
        semantics::check(&invalid, &analysis),
        Err(Error::Validation(_))
    ));
    assert!(matches!(
        semantics::analyze(&world, &invalid, &analysis),
        Err(Error::Validation(_))
    ));
    let other = edit(&world, WORLD, |fm| {
        fm["id"] = Yaml::String("world_00000000000000000000000001".into())
    });
    assert!(matches!(
        semantics::analyze(&world, &other, &analysis),
        Err(Error::InvalidInput(_))
    ));
    let mut bad_time = analysis.clone();
    bad_time.at.timeline = "missing".into();
    assert!(semantics::check(&world, &bad_time).is_err());
}

#[test]
fn exact_report_hash_binds_source_policy_operands_and_requested_budget() {
    let world = world();
    let analysis = analysis(vec![equals(ANA, "labels", "[\"a\",\"b\"]")], 10);
    let first = semantics::check(&world, &analysis).unwrap();
    assert_eq!(first, semantics::check(&world, &analysis).unwrap());
    let mut equivalent = analysis.clone();
    let semantics::Predicate::StateEquals {
        expected_value_raw, ..
    } = &mut equivalent.expectations[0].predicate
    else {
        unreachable!()
    };
    *expected_value_raw = "[ \"a\", \"b\" ]".into();
    let second = semantics::check(&world, &equivalent).unwrap();
    assert_eq!(outcomes(&first), outcomes(&second));
    assert_ne!(first["analysisSha256"], second["analysisSha256"]);
    assert_ne!(first["reportSha256"], second["reportSha256"]);
    let changed = edit(&world, WORLD, |fm| {
        fm["title"] = Yaml::String("A changed title".into())
    });
    assert_ne!(
        first["reportSha256"],
        semantics::check(&changed, &analysis).unwrap()["reportSha256"]
    );
    let mut request=semantics::decode_check(&json!({"expectedFingerprint":world.fingerprint,"analysis":analysis,"maxCharacters":100000}).to_string()).unwrap();
    let first = semantics::check_request(&world, &request).unwrap();
    request.max_characters = 100001;
    assert_ne!(
        first["reportSha256"],
        semantics::check_request(&world, &request).unwrap()["reportSha256"]
    );
}
