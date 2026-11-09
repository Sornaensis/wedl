//! Learned generational literals have their own provenance and two clocks.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    generational::{LineageDirection, knowledge},
    model::{Record, StoryTime, World},
    source, validation,
};

fn id(kind: &str, n: u32) -> String {
    let prefix = match kind {
        "character" => "char",
        "knowledge" => "know",
        "conversation" => "conv",
        "location" => "loc",
        "parentage" => "kinship",
        _ => kind,
    };
    format!("{prefix}_{n:026}")
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn point(tick: i64, order: i32) -> Json {
    json!({"timeline":"main","tick":tick,"order":order})
}
fn state(n: u32, tick: i64, order: i32, name: &str) -> Json {
    json!({"id":format!("learning-{n}"),"time":{"tick":tick,"order":order},"state":name,"confidence":1,"acquisition":"told"})
}
fn tr(n: u32, kind: &str, payload: Json) -> Json {
    json!({"transition_id":id("transition",n),"transition_kind":kind,
        "applicability":{"applicability_kind":"instant","point":point(-30,0)},"payload":payload})
}
fn belief(n: u32, kind: &str, payload: Json) -> Json {
    json!({"kind":"knowledge","id":id("knowledge",n),"title":"AUTHOR_ONLY_TITLE",
        "knower":id("character",1),"claim":{"key":"AUTHOR_ONLY_KEY","statement":"AUTHOR_ONLY_PROSE",
        "genealogy":{"kind":kind,"payload":payload,"valid":{"from":point(-10,0)}}},
        "transitions":[state(n,2,0,"accepted")]})
}
fn parentage(n: u32, child: u32, parent: u32) -> Json {
    belief(
        n,
        "parentage",
        json!({"child_id":id("character",child),"parent_id":id("character",parent),"basis":"adoptive"}),
    )
}
fn record(mut value: Json) -> Record {
    value["schema"] = json!(source::SOURCE_SCHEMA);
    let raw = format!(
        "---\n{}---\nAUTHOR_ONLY_BODY\n",
        serde_yaml::to_string(&value).unwrap()
    );
    source::parse_record(
        raw.as_bytes(),
        &format!(
            "story/AUTHOR_ONLY_PATH-{}.md",
            value["id"].as_str().unwrap()
        ),
    )
    .unwrap()
}
fn world(extra: Vec<Json>) -> World {
    let mut records = vec![
        record(json!({"kind":"world","id":id("world",1),"title":"Beliefs",
        "capabilities":["generational-core-v1","generational-knowledge-v1"],"default_timeline":"main",
        "timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"state_keys":{}})),
        record(json!({"kind":"location","id":id("location",1),"title":"AUTHOR_ONLY_PLACE"})),
    ];
    records.extend((1..=8).map(|n|record(json!({"kind":"character","id":id("character",n),"title":format!("AUTHOR_ONLY_PERSON_{n}")}))));
    records.extend((1..=3).map(|n|record(json!({"kind":"organization","id":id("organization",n),"title":format!("AUTHOR_ONLY_ORG_{n}"),
        "organization_kind":"institution","initialization":tr(1000+n,"organization-initialize",json!({"title":format!("AUTHOR_ONLY_ORG_{n}"),"aliases":[]})),"transitions":[]}))));
    records.push(record(json!({"kind":"legacy","id":id("legacy",1),"title":"AUTHOR_ONLY_LEGACY","legacy_kind":"office",
        "organization_id":id("organization",1),"initialization":tr(2000,"legacy-initialize",json!({"title":"AUTHOR_ONLY_LEGACY","aliases":[]})),"transitions":[]})));
    records.extend(extra.into_iter().map(record));
    source::world_from_records(PathBuf::new(), records, Some("AUTHOR_ONLY_REVISION".into()))
        .unwrap()
}
fn edit(base: &World, target: &str, change: impl FnOnce(&mut Yaml)) -> World {
    let mut records: Vec<_> = base.records.values().cloned().collect();
    let target = records
        .iter_mut()
        .find(|record| record.id == target)
        .unwrap();
    change(&mut target.frontmatter);
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&target.frontmatter).unwrap(),
        target.body
    );
    *target = source::parse_record(raw.as_bytes(), &target.source_path).unwrap();
    source::world_from_records(base.root.clone(), records, base.revision.clone()).unwrap()
}
fn yaml(value: Json) -> Yaml {
    serde_yaml::to_value(value).unwrap()
}
fn valid(world: &World) {
    assert!(
        knowledge::validate(world).is_empty(),
        "{:?}",
        knowledge::validate(world)
    );
    let report = validation::validate_world(world);
    assert!(report.valid, "{:?}", report.diagnostics);
}
fn read(world: &World, tick: i64, order: i32, held_history: bool) -> Json {
    knowledge::assertions(
        world,
        &id("character", 1),
        &at(tick, order),
        None,
        None,
        held_history,
        100,
    )
    .unwrap()
}
fn all_payloads() -> Vec<(&'static str, Json)> {
    vec![
        (
            "parentage",
            json!({"child_id":id("character",1),"parent_id":id("character",2),"basis":"biological"}),
        ),
        (
            "union",
            json!({"participant_ids":[id("character",1),id("character",2)],"state":"ended"}),
        ),
        (
            "organization",
            json!({"organization_id":id("organization",1),"parent_id":null}),
        ),
        (
            "affiliation",
            json!({"character_id":id("character",1),"organization_id":id("organization",1),"role":null}),
        ),
        (
            "tenure",
            json!({"legacy_id":id("legacy",1),"holder_id":null,"basis":"de-facto"}),
        ),
        (
            "claim",
            json!({"legacy_id":id("legacy",1),"claimant_id":id("character",2),"state":"disputed"}),
        ),
        (
            "vital",
            json!({"character_id":id("character",1),"state":"dead"}),
        ),
    ]
}

#[test]
fn all_seven_literals_roundtrip_without_author_metadata_or_truth_comparison() {
    let base = world(
        all_payloads()
            .into_iter()
            .enumerate()
            .map(|(n, (kind, payload))| belief(n as u32 + 1, kind, payload))
            .collect(),
    );
    valid(&base);
    let result = read(&base, 2, 0, false);
    assert_eq!(result["count"], 7);
    assert_eq!(result["complete"], true);
    assert_eq!(result["items"][2]["value"]["parent_id"], Json::Null);
    assert_eq!(result["items"][4]["value"]["holder_id"], Json::Null);
    let wire = serde_json::to_string(&result).unwrap();
    for secret in [
        "AUTHOR_ONLY",
        "fingerprint",
        "revision",
        "sourcePath",
        "blobOid",
        "claimKey",
        "statement",
    ] {
        assert!(!wire.contains(secret), "{secret}: {wire}");
    }
    for record in base
        .records
        .values()
        .filter(|record| record.kind == "knowledge")
    {
        let parsed = source::parse_record(&record.raw_bytes, &record.source_path).unwrap();
        assert_eq!(parsed.frontmatter, record.frontmatter);
    }
}

#[test]
fn learning_and_applicability_have_independent_exact_inclusive_clocks() {
    let tick = 9_007_199_254_740_993_i64;
    let mut historical = parentage(1, 1, 2);
    historical["claim"]["genealogy"]["valid"] = json!({"from":point(-10,-1),"until":point(-1,2)});
    historical["transitions"] = json!([state(1, tick, 1, "suspected")]);
    let mut current = parentage(2, 1, 3);
    current["claim"]["genealogy"]["valid"] = json!({"from":point(tick,2),"until":point(tick,3)});
    current["transitions"] = json!([state(2, tick, 1, "accepted")]);
    let base = world(vec![historical, current]);
    valid(&base);
    assert_eq!(read(&base, tick, 0, true)["count"], 0);
    let history = read(&base, tick, 1, true);
    assert_eq!(history["count"], 2);
    assert_eq!(history["items"][0]["applicable"], false);
    assert_eq!(history["items"][0]["learnedAt"]["tick"], tick.to_string());
    assert_eq!(history["items"][1]["applicable"], false);
    assert_eq!(read(&base, tick, 1, false)["count"], 0);
    for order in [2, 3] {
        let result = read(&base, tick, order, false);
        assert_eq!(result["count"], 1);
        assert_eq!(result["items"][0]["knowledgeId"], id("knowledge", 2));
    }
    assert_eq!(read(&base, tick, 4, false)["count"], 0);
    assert_eq!(read(&base, tick, 4, true)["count"], 2);
    assert_eq!(
        knowledge::assertions(
            &base,
            &id("character", 1),
            &StoryTime::new("other", tick, 3).unwrap(),
            None,
            None,
            true,
            100
        )
        .unwrap()["count"],
        0
    );
}

#[test]
fn latest_held_state_controls_admission_and_retains_the_first_learning_time() {
    let mut value = parentage(1, 1, 2);
    value["transitions"] = json!([
        state(1, 0, 0, "rejected"),
        state(2, 1, 1, "suspected"),
        state(3, 2, 0, "rejected"),
        state(4, 3, 0, "forgotten"),
        state(5, 4, 0, "remembered"),
        state(6, 5, 0, "accepted")
    ]);
    value["transitions"][5]["confidence"] = json!(0.8);
    let base = world(vec![value]);
    valid(&base);
    for (tick, order, count) in [
        (0, 0, 0),
        (1, 0, 0),
        (1, 1, 1),
        (2, 0, 0),
        (3, 0, 0),
        (4, 0, 1),
        (5, 0, 1),
    ] {
        assert_eq!(read(&base, tick, order, false)["count"], count);
        assert_eq!(read(&base, tick, order, true)["count"], count);
    }
    let item = &read(&base, 4, 0, false)["items"][0];
    assert_eq!(item["learnedAt"], json!(at(1, 1)));
    assert_eq!(item["time"], json!(at(4, 0)));
    assert_eq!(item["transitionId"], "learning-5");
    assert_eq!(item["citations"][0]["time"], json!(at(4, 0)));
    assert_eq!(item["uncertain"], true);
    assert_eq!(read(&base, 5, 0, false)["items"][0]["uncertain"], true);
}

#[test]
fn closed_assertions_reject_invalid_references_labels_times_and_transitions() {
    let base = world(vec![parentage(1, 1, 2)]);
    valid(&base);
    let cases = vec![
        ("/claim/genealogy/extra", json!(true)),
        ("/claim/genealogy/payload/extra", json!(true)),
        ("/claim/genealogy/kind", json!("family")),
        ("/claim/genealogy/payload/basis", json!("rumor")),
        ("/claim/genealogy/payload/parent_id", json!(null)),
        (
            "/claim/genealogy/payload/parent_id",
            json!(id("organization", 1)),
        ),
        (
            "/claim/genealogy/payload/parent_id",
            json!(id("character", 999)),
        ),
        (
            "/claim/genealogy/labels",
            json!({id("character",3):"Not an asserted endpoint"}),
        ),
        ("/claim/genealogy/labels", json!({id("character",2):" "})),
        (
            "/claim/genealogy/labels",
            json!({id("character",2):"x".repeat(201)}),
        ),
        ("/claim/genealogy/valid/from/tick", json!(true)),
        ("/claim/genealogy/valid/from/tick", json!("-10")),
        ("/claim/genealogy/valid/from/tick", json!(u64::MAX)),
        ("/claim/genealogy/valid/from/order", json!(i64::MAX)),
        ("/claim/genealogy/valid/from/timeline", json!("undeclared")),
        ("/claim/genealogy/valid/until", point(-11, 0)),
        (
            "/claim/genealogy/valid/until",
            json!({"timeline":"other","tick":20,"order":0}),
        ),
        ("/claim/genealogy/valid/from", json!({"tick":-10})),
        (
            "/claim/genealogy/evidence",
            json!([{"kind":"source_entity","entity_id":id("character",2)}]),
        ),
        ("/claim/subject", json!(id("character", 1))),
        ("/transitions/0/state", json!("maybe")),
        ("/transitions/0/confidence", json!(true)),
        ("/transitions/0/confidence", json!(-0.1)),
        ("/transitions/0/acquisition", json!("learned")),
    ];
    for (path, replacement) in cases {
        let changed = edit(&base, &id("knowledge", 1), |value| {
            let mut json = serde_json::to_value(&*value).unwrap();
            let (parent, key) = path.rsplit_once('/').unwrap();
            json.pointer_mut(parent).unwrap()[key] = replacement;
            *value = yaml(json);
        });
        assert!(!knowledge::validate(&changed).is_empty(), "accepted {path}");
        assert!(
            !validation::validate_world(&changed).valid,
            "global accepted {path}"
        );
    }
    for replacement in [
        json!([]),
        json!([state(1, 2, 0, "rejected")]),
        json!([state(1, 3, 0, "accepted"), state(2, 2, 0, "remembered")]),
        json!([state(1, 2, 0, "accepted"), state(2, 2, 0, "remembered")]),
    ] {
        let changed = edit(&base, &id("knowledge", 1), |value| {
            value["transitions"] = yaml(replacement)
        });
        assert!(!knowledge::validate(&changed).is_empty());
        assert!(!validation::validate_world(&changed).valid);
    }
    let other = edit(&base, &id("knowledge", 1), |value| {
        value["transitions"][0]["time"]["timeline"] = yaml(json!("other"))
    });
    assert!(
        knowledge::validate(&other)
            .iter()
            .any(|diagnostic| diagnostic.code == "GEN-KNOWLEDGE-004")
    );
    let no_capability = edit(&base, &id("world", 1), |value| {
        value["capabilities"] = yaml(json!(["generational-core-v1"]))
    });
    assert!(
        knowledge::validate(&no_capability)
            .iter()
            .any(|diagnostic| diagnostic.code == "GEN-KNOWLEDGE-001")
    );
}

#[test]
fn nulls_and_bounded_union_rosters_are_kind_specific_and_tags_are_not_literals() {
    for (n, (kind, payload)) in all_payloads().into_iter().enumerate() {
        let base = world(vec![belief(n as u32 + 1, kind, payload)]);
        valid(&base);
        let changed = edit(&base, &id("knowledge", n as u32 + 1), |value| {
            let original = value["claim"]["genealogy"]["payload"].clone();
            value["claim"]["genealogy"]["payload"] =
                Yaml::Tagged(Box::new(serde_yaml::value::TaggedValue {
                    tag: serde_yaml::value::Tag::new("custom"),
                    value: original,
                }));
        });
        assert!(!knowledge::validate(&changed).is_empty());
    }
    for participants in [
        json!([id("character", 2), id("character", 1)]),
        json!([id("character", 1), id("character", 1)]),
        json!([id("character", 1)]),
    ] {
        let base = world(vec![belief(
            1,
            "union",
            json!({"participant_ids":participants,"state":"formed"}),
        )]);
        assert!(!knowledge::validate(&base).is_empty());
    }
    let oversized = world(vec![belief(
        1,
        "union",
        json!({"participant_ids":(1..=33).map(|n|id("character",n)).collect::<Vec<_>>(),"state":"formed"}),
    )]);
    assert!(!knowledge::validate(&oversized).is_empty());
}

#[test]
fn mistaken_cyclic_and_contradictory_beliefs_remain_literal_cited_graph_evidence() {
    let mut uncertain = parentage(2, 2, 3);
    uncertain["transitions"][0]["state"] = json!("uncertain");
    let base = world(vec![
        parentage(1, 1, 2),
        uncertain,
        parentage(3, 3, 1),
        parentage(4, 1, 4),
        parentage(5, 1, 2),
    ]);
    valid(&base);
    let family = knowledge::family(
        &base,
        &id("character", 1),
        &id("character", 1),
        &at(2, 0),
        100,
    )
    .unwrap();
    assert_eq!(family["parents"].as_array().unwrap().len(), 3);
    assert_eq!(family["children"].as_array().unwrap().len(), 1);
    let ancestry = knowledge::lineage(
        &base,
        &id("character", 1),
        &id("character", 1),
        &at(2, 0),
        LineageDirection::Ancestors,
        4,
        100,
    )
    .unwrap();
    assert_eq!(ancestry["count"], 3);
    let three = ancestry["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["targetId"] == id("character", 3))
        .unwrap();
    assert_eq!(three["generationDistance"], "2");
    assert_eq!(three["uncertain"], true);
    assert_eq!(three["edges"][0]["knowledgeId"], id("knowledge", 1));
    assert_eq!(three["edges"][1]["knowledgeId"], id("knowledge", 2));
    assert!(
        knowledge::lineage(
            &base,
            &id("character", 1),
            &id("character", 1),
            &at(2, 0),
            LineageDirection::Ancestors,
            1,
            100
        )
        .is_err()
    );
    let path = knowledge::kinship_path(
        &base,
        &id("character", 1),
        &id("character", 2),
        &id("character", 4),
        &at(2, 0),
        4,
        100,
    )
    .unwrap();
    assert_eq!(path["found"], true);
    assert_eq!(path["distance"], "2");
    assert_eq!(path["edges"][0]["from"], id("character", 2));
    assert_eq!(path["edges"][0]["to"], path["edges"][1]["from"]);
    assert_eq!(path["edges"][1]["to"], id("character", 4));
}

#[test]
fn no_canonical_bridge_or_canonical_existence_or_name_is_exposed() {
    let canonical = json!({"kind":"parentage","id":id("parentage",1),"title":"AUTHOR_ONLY_CANON",
        "child_id":id("character",2),"parent_id":id("character",3),
        "initialization":tr(3000,"parentage-initialize",json!({"basis":"biological"})),"transitions":[]});
    let base = world(vec![parentage(1, 1, 2), canonical]);
    valid(&base);
    let result = knowledge::kinship_path(
        &base,
        &id("character", 1),
        &id("character", 1),
        &id("character", 3),
        &at(2, 0),
        4,
        100,
    )
    .unwrap();
    assert_eq!(result["found"], false);
    let absent = knowledge::family(
        &base,
        &id("character", 1),
        &id("character", 999),
        &at(2, 0),
        1,
    )
    .unwrap();
    assert_eq!(absent["count"], 0);
    let mut changed = base.clone();
    changed.records.insert(
        id("character", 999),
        record(json!({"kind":"character","id":id("character",999),"title":"Unlearned person"})),
    );
    assert_eq!(
        absent,
        knowledge::family(
            &changed,
            &id("character", 1),
            &id("character", 999),
            &at(2, 0),
            1
        )
        .unwrap()
    );
    assert!(
        knowledge::family(
            &base,
            "AUTHOR_ONLY_PERSON_1",
            &id("character", 1),
            &at(2, 0),
            100
        )
        .is_err()
    );
    assert!(
        knowledge::family(
            &base,
            &id("character", 1),
            "AUTHOR_ONLY_PERSON_1",
            &at(2, 0),
            100
        )
        .is_err()
    );
}

#[test]
fn organization_roles_vacancies_competing_claims_and_vital_contradictions_survive() {
    let base = world(vec![
        belief(
            1,
            "organization",
            json!({"organization_id":id("organization",1),"parent_id":id("organization",2)}),
        ),
        belief(
            2,
            "organization",
            json!({"organization_id":id("organization",2),"parent_id":id("organization",3)}),
        ),
        belief(
            3,
            "affiliation",
            json!({"character_id":id("character",2),"organization_id":id("organization",1),"role":"Keeper"}),
        ),
        belief(
            4,
            "tenure",
            json!({"legacy_id":id("legacy",1),"holder_id":null,"basis":"legal"}),
        ),
        belief(
            5,
            "tenure",
            json!({"legacy_id":id("legacy",1),"holder_id":id("character",2),"basis":"de-facto"}),
        ),
        belief(
            6,
            "claim",
            json!({"legacy_id":id("legacy",1),"claimant_id":id("character",3),"state":"disputed"}),
        ),
        belief(
            7,
            "claim",
            json!({"legacy_id":id("legacy",1),"claimant_id":id("character",4),"state":"recognized"}),
        ),
        belief(
            8,
            "vital",
            json!({"character_id":id("character",2),"state":"living"}),
        ),
        belief(
            9,
            "vital",
            json!({"character_id":id("character",2),"state":"dead"}),
        ),
    ]);
    valid(&base);
    let organization = knowledge::organization(
        &base,
        &id("character", 1),
        &id("organization", 1),
        &at(2, 0),
        3,
        100,
    )
    .unwrap();
    assert_eq!(organization["paths"].as_array().unwrap().len(), 2);
    assert_eq!(organization["affiliations"][0]["value"]["role"], "Keeper");
    let legacy =
        knowledge::legacy(&base, &id("character", 1), &id("legacy", 1), &at(2, 0), 100).unwrap();
    assert_eq!(legacy["tenures"].as_array().unwrap().len(), 2);
    assert_eq!(legacy["tenures"][0]["value"]["holder_id"], Json::Null);
    assert_eq!(legacy["claims"].as_array().unwrap().len(), 2);
    assert_eq!(legacy["holders"].as_array().unwrap().len(), 1);
    assert_eq!(legacy["vacancies"].as_array().unwrap().len(), 1);
    assert_eq!(legacy["succession"], json!([]));
    let vital = knowledge::vital(
        &base,
        &id("character", 1),
        &id("character", 2),
        &at(2, 0),
        100,
    )
    .unwrap();
    assert_eq!(vital["states"], json!(["dead", "living"]));
    assert_eq!(vital["conflicting"], true);
    assert_eq!(vital["available"], true);
    assert_eq!(vital["vital"], Json::Null);
    assert_eq!(vital["assertions"].as_array().unwrap().len(), 2);
}

fn evidence_world() -> World {
    let mut source = parentage(50, 1, 2);
    source["claim"] = json!({"key":"report","statement":"AUTHOR_ONLY_EVIDENCE_TEXT"});
    source["transitions"] = json!([state(50, -2, 0, "accepted")]);
    let scene = json!({"kind":"scene","id":id("scene",1),"title":"AUTHOR_ONLY_SCENE","status":"closed","location":id("location",1),
        "time":{"start":{"tick":-3},"current":{"tick":0},"end":{"tick":0}},"participants":[{"character":id("character",1)}],
        "observations":[{"id":"observed-report","at":{"tick":-2},"until":{"tick":-1},"text":"Report","audience":[id("character",1)]}]});
    let conversation = json!({"kind":"conversation","id":id("conversation",1),"title":"AUTHOR_ONLY_CONVERSATION","status":"closed","location":id("location",1),
        "time":{"start":{"tick":-3},"end":{"tick":0}},"participants":[{"character":id("character",1)},{"character":id("character",2)}],
        "turns":[{"id":"spoken-report","at":{"tick":-2},"speaker":id("character",2),"text":"Report","audience":["participants"]}],
        "recollections":[{"id":"remembered-report","at":{"tick":1},"character":id("character",1),"state":"remembered","confidence":0.8,"summary":"A fallible memory","exact_turns":["spoken-report"]}]});
    let mut learned = parentage(1, 1, 2);
    learned["claim"]["genealogy"]["evidence"] = json!([
        {"kind":"knowledge","entity_id":id("knowledge",50),"transition_id":"learning-50"},
        {"kind":"observation","entity_id":id("scene",1),"observation_id":"observed-report"},
        {"kind":"turn","entity_id":id("conversation",1),"turn_id":"spoken-report"},
        {"kind":"recollection","entity_id":id("conversation",1),"recollection_id":"remembered-report"},
    ]);
    world(vec![source, scene, conversation, learned])
}

#[test]
fn four_evidence_kinds_are_admitted_at_historical_source_time_and_first_learning() {
    let base = evidence_world();
    valid(&base);
    let result = read(&base, 2, 0, false);
    assert_eq!(result["count"], 1);
    let proof = &result["items"][0]["evidence"];
    assert_eq!(proof.as_array().unwrap().len(), 4);
    assert_eq!(proof[0]["time"], json!(at(-2, 0)));
    assert_eq!(proof[1]["time"], json!(at(-2, 0)));
    assert_eq!(proof[3]["time"], json!(at(1, 0)));
    assert!(
        !serde_json::to_string(&result)
            .unwrap()
            .contains("AUTHOR_ONLY")
    );
    let later_rejected = edit(&base, &id("knowledge", 50), |value| {
        value["transitions"]
            .as_sequence_mut()
            .unwrap()
            .push(yaml(state(51, 3, 0, "rejected")))
    });
    valid(&later_rejected);
    assert_eq!(result, read(&later_rejected, 2, 0, false));
    assert_eq!(read(&base, 4, 0, false), read(&later_rejected, 4, 0, false));
}

#[test]
fn evidence_rejects_wrong_recipients_future_items_private_speech_and_superseded_memory() {
    let base = evidence_world();
    valid(&base);
    let mutations: Vec<(&str, Box<dyn Fn(&mut Yaml)>)> = vec![
        (
            "knowledge",
            Box::new(|value| value["knower"] = yaml(json!(id("character", 2)))),
        ),
        (
            "knowledge",
            Box::new(|value| value["transitions"][0]["state"] = yaml(json!("rejected"))),
        ),
        (
            "knowledge",
            Box::new(|value| value["transitions"][0]["time"] = yaml(json!({"tick":2,"order":1}))),
        ),
        (
            "scene",
            Box::new(|value| value["observations"][0]["audience"] = yaml(json!(["author"]))),
        ),
        (
            "scene",
            Box::new(|value| value["observations"][0]["at"] = yaml(json!({"tick":2,"order":1}))),
        ),
        (
            "conversation",
            Box::new(|value| value["turns"][0]["audience"] = yaml(json!([id("character", 2)]))),
        ),
        (
            "conversation",
            Box::new(|value| value["turns"][0]["kind"] = yaml(json!("action"))),
        ),
        (
            "conversation",
            Box::new(|value| {
                value["recollections"][0]["character"] = yaml(json!(id("character", 2)))
            }),
        ),
        (
            "conversation",
            Box::new(|value| value["recollections"][0]["at"] = yaml(json!({"tick":2,"order":1}))),
        ),
        (
            "conversation",
            Box::new(|value| {
                value["recollections"].as_sequence_mut().unwrap().push(yaml(json!({"id":"forgotten-memory","at":{"tick":2},"character":id("character",1),"state":"forgotten","confidence":0,"summary":"Gone"})))
            }),
        ),
    ];
    for (kind, mutate) in mutations {
        let target = id(kind, if kind == "knowledge" { 50 } else { 1 });
        let changed = edit(&base, &target, mutate);
        assert!(
            knowledge::validate(&changed)
                .iter()
                .any(|diagnostic| diagnostic.code == "GEN-KNOWLEDGE-006"),
            "accepted mutation of {kind}"
        );
    }
    for index in 0..4 {
        let changed = edit(&base, &id("knowledge", 1), |value| {
            let evidence = &mut value["claim"]["genealogy"]["evidence"][index];
            let field = [
                "transition_id",
                "observation_id",
                "turn_id",
                "recollection_id",
            ][index];
            evidence[field] = yaml(json!("not-an-authored-item"));
        });
        assert!(
            knowledge::validate(&changed)
                .iter()
                .any(|diagnostic| diagnostic.code == "GEN-KNOWLEDGE-006")
        );
    }
    let duplicate = edit(&base, &id("knowledge", 1), |value| {
        let first = value["claim"]["genealogy"]["evidence"][0].clone();
        value["claim"]["genealogy"]["evidence"]
            .as_sequence_mut()
            .unwrap()
            .push(first);
    });
    assert!(!knowledge::validate(&duplicate).is_empty());
}

#[test]
fn admission_and_subject_filters_precede_limits_and_hidden_changes_preserve_complete_json() {
    let base = world(vec![parentage(1, 1, 2)]);
    valid(&base);
    let expected = knowledge::family(
        &base,
        &id("character", 1),
        &id("character", 1),
        &at(2, 0),
        1,
    )
    .unwrap();
    let mut additions = vec![parentage(1, 1, 2)];
    for n in 2..=120 {
        let mut item = parentage(n, 3, 4);
        match n % 4 {
            0 => item["knower"] = json!(id("character", 2)),
            1 => item["transitions"][0]["time"] = json!({"tick":3}),
            2 => item["claim"]["genealogy"]["valid"]["until"] = point(1, 0),
            _ => (),
        }
        additions.push(item);
    }
    let mut expanded = world(additions);
    valid(&expanded);
    expanded.revision = Some("DIFFERENT_AUTHOR_REVISION".into());
    expanded.fingerprint = "DIFFERENT_WORLD_FINGERPRINT".into();
    for record in expanded.records.values_mut() {
        record.title = "DIFFERENT_CANONICAL_TITLE".into();
        record.source_path = "private/moved.md".into();
        record.body = "DIFFERENT_AUTHOR_BODY".into();
        record.blob_oid = Some("DIFFERENT_BLOB".into());
    }
    assert_eq!(
        expected,
        knowledge::family(
            &expanded,
            &id("character", 1),
            &id("character", 1),
            &at(2, 0),
            1
        )
        .unwrap()
    );
    assert_eq!(
        knowledge::assertions(
            &expanded,
            &id("character", 1),
            &at(2, 0),
            Some("parentage"),
            Some(&id("character", 1)),
            false,
            1
        )
        .unwrap()["count"],
        1
    );
    assert!(
        knowledge::assertions(
            &expanded,
            &id("character", 1),
            &at(2, 0),
            Some("parentage"),
            None,
            false,
            1
        )
        .is_err()
    );
    let duplicate = world(vec![parentage(1, 1, 2), parentage(2, 1, 3)]);
    assert!(
        knowledge::family(
            &duplicate,
            &id("character", 1),
            &id("character", 1),
            &at(2, 0),
            1
        )
        .is_err()
    );
    for invalid_limit in [0, 1001] {
        assert!(
            knowledge::family(
                &base,
                &id("character", 1),
                &id("character", 1),
                &at(2, 0),
                invalid_limit
            )
            .is_err()
        );
    }
}
