use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use wedl::{
    Error,
    consequence_semantics::Time,
    model::{Record, World},
    retcon_impact::{self, Reference, Request},
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const ANA: &str = "char_00000000000000000000000001";
const OBJECT: &str = "obj_00000000000000000000000001";
const LITERAL: &str = "obj_00000000000000000000000002";
const EVENT: &str = "event_00000000000000000000000001";
const KNOW: &str = "know_00000000000000000000000001";
const DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn record(kind: &str, id: &str, fields: &str, body: &str) -> Record {
    source::parse_record(
        format!(
            "---\nschema: wedl/v1\nkind: {kind}\nid: {id}\ntitle: Test {kind}\n{fields}---\n{body}"
        )
        .as_bytes(),
        &format!("story/{id}.md"),
    )
    .unwrap()
}
fn fixture(extra: Vec<Record>) -> World {
    let mut records = vec![
        record(
            "world",
            WORLD,
            "capabilities: [lore-core-v1, descriptions-v1, briefs-v1]\ndefault_timeline: main\ntimelines: [{id: main, label: Main}]\nthreads: []\nstate_keys:\n  character:\n    score: {type: integer}\n    number: {type: number}\n  object:\n    label: {type: string}\n    holder: {type: entity, kind: character}\nrelationship_metrics: {}\n",
            "World body.\n",
        ),
        record(
            "character",
            ANA,
            "initial_state: {score: 0, number: 0}\n",
            "Actor body.\n",
        ),
        record(
            "object",
            OBJECT,
            &format!("initial_state: {{holder: {{entity: {ANA}}}}}\n"),
            "Object body.\n",
        ),
    ];
    records.extend(extra);
    let world = source::world_from_records(
        std::env::temp_dir().join("wedl-retcon-impact-test"),
        records,
        None,
    )
    .unwrap();
    let valid = validation::validate_world(&world);
    assert!(
        valid.valid,
        "{:?}",
        valid
            .diagnostics
            .iter()
            .filter(|d| d.severity == "error")
            .map(|d| (&d.code, &d.message))
            .take(5)
            .collect::<Vec<_>>()
    );
    world
}
fn request(id: &str) -> Request {
    Request {
        target: Reference::Record { id: id.into() },
        at: Time {
            timeline: "main".into(),
            tick: "10".into(),
            order: "0".into(),
        },
        depth: 32,
        max_items: 1000,
    }
}
fn evaluate(world: &World, request: &Request) -> Json {
    let valid = validation::validate_world(world);
    assert!(
        valid.valid,
        "{:?}",
        valid
            .diagnostics
            .iter()
            .filter(|d| d.severity == "error")
            .map(|d| (&d.code, &d.message))
            .take(5)
            .collect::<Vec<_>>()
    );
    retcon_impact::evaluate(world, world, request, DIGEST).unwrap()
}
fn changed(world: &World, id: &str, edit: impl FnOnce(&mut Yaml, &mut String)) -> World {
    let original = &world.records[id];
    let mut fields = original.frontmatter.clone();
    let mut body = original.body.clone();
    edit(&mut fields, &mut body);
    let new = source::parse_record(
        format!(
            "---\n{}---\n{body}",
            serde_yaml::to_string(&fields).unwrap()
        )
        .as_bytes(),
        &original.source_path,
    )
    .unwrap();
    let mut records: Vec<_> = world
        .records
        .values()
        .filter(|record| record.id != id)
        .cloned()
        .collect();
    records.push(new);
    source::world_from_records(world.root.clone(), records, None).unwrap()
}

#[test]
fn exact_occurrences_are_distinct_transitive_and_deterministic() {
    let world = fixture(vec![
        record(
            "event",
            EVENT,
            &format!(
                "time: {{tick: 3}}\neffects:\n - {{id: effect_00000000000000000000000001, target: {OBJECT}, key: holder, operation: set, value: {{entity: {ANA}}}}}\n - {{id: effect_00000000000000000000000002, target: {OBJECT}, key: label, operation: set, value: named}}\n"
            ),
            "",
        ),
        record(
            "knowledge",
            KNOW,
            &format!(
                "knower: {ANA}\nclaim: {{key: fixture, statement: Known., truth: unknown}}\ntransitions:\n - {{id: learned, time: {{tick: 5}}, state: uncertain, acquisition: heard, confidence: 0.5, causing_event: {EVENT}, source_entity: {ANA}}}\n"
            ),
            "",
        ),
        record(
            "object",
            LITERAL,
            &format!("initial_state: {{label: {ANA}}}\n"),
            &format!("A Markdown mention [{ANA}]({ANA}).\n"),
        ),
    ]);
    let report = evaluate(&world, &request(ANA));
    assert_eq!(report, evaluate(&world, &request(ANA)));
    let items = report["items"].as_array().unwrap();
    assert!(!items.iter().any(|item| item["owner"]["id"] == LITERAL));
    assert!(
        items
            .iter()
            .any(|item| item["relation"] == "acquisition"
                && item["citation"]["memberId"] == "learned")
    );
    assert!(items.iter().any(|item| item["owner"]["id"] == EVENT
        && item["target"]["id"] == OBJECT
        && item["distance"] == 2));
    let positions: Vec<_> = items
        .iter()
        .filter(|item| {
            item["snapshot"] == "base"
                && item["owner"]["id"] == EVENT
                && item["target"]["id"] == OBJECT
        })
        .map(|item| item["citation"]["section"].as_str().unwrap())
        .collect();
    assert_eq!(positions, vec!["effects[0].target", "effects[1].target"]);
    assert_eq!(
        report["candidate"],
        json!({"baseRevision":null,"planSha256":DIGEST})
    );
    assert_eq!(report["canApply"], false);
    assert_eq!(report["writesPerformed"], false);
    assert_eq!(report["mentions"], json!([]));
    assert_eq!(report["semanticSuggestions"], json!([]));
}

#[test]
fn hidden_future_owners_and_future_history_never_become_graph_edges() {
    let future = "event_00000000000000000000000002";
    let hidden = "event_00000000000000000000000003";
    let world = fixture(vec![
        record("event", EVENT, "time: {tick: 2}\neffects: []\n", ""),
        record(
            "event",
            future,
            &format!("time: {{tick: 20}}\ncauses: [{EVENT}]\neffects: []\n"),
            "future secret",
        ),
        record(
            "event",
            hidden,
            &format!("audience: [gm]\ntime: {{tick: 5}}\ncauses: [{EVENT}]\neffects: []\n"),
            "hidden secret",
        ),
        record(
            "knowledge",
            KNOW,
            &format!(
                "knower: {ANA}\nclaim: {{key: fixture, statement: Known., truth: unknown}}\ntransitions:\n - {{id: early, time: {{tick: 5}}, state: uncertain, acquisition: heard, confidence: 0.5}}\n - {{id: future, time: {{tick: 20}}, state: accepted, acquisition: heard, confidence: 0.5, causing_event: {EVENT}}}\n"
            ),
            "",
        ),
    ]);
    let report = evaluate(&world, &request(EVENT));
    assert_eq!(report["items"], json!([]));
    for id in [future, hidden, "missing"] {
        assert!(matches!(
            retcon_impact::evaluate(&world, &world, &request(id), DIGEST),
            Err(Error::NotFound(_))
        ));
    }
    let altered = changed(&world, future, |front, body| {
        front["title"] = "Different future".into();
        *body = "Changed future".into();
    });
    assert_eq!(report, evaluate(&altered, &request(EVENT)));
}

#[test]
fn member_identity_is_owner_scoped_and_correction_history_is_traversable() {
    let world = fixture(vec![]);
    let world = changed(&world, ANA, |front, _| {
        front["descriptions"] = serde_yaml::from_str("- {id: description_00000000000000000000000001, heading: Original, markdown: original, valid: {kind: timeless}, audience: [author], perspectives: [author]}\n- {id: description_00000000000000000000000002, heading: Corrected, markdown: corrected, valid: {kind: timeless}, audience: [author], perspectives: [author], replaces: description_00000000000000000000000001}\n").unwrap();
    });
    let mut query = request(ANA);
    query.target = Reference::Section {
        owner_id: ANA.into(),
        id: "description_00000000000000000000000001".into(),
    };
    let report = evaluate(&world, &query);
    assert!(
        report["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["relation"] == "correction"
                && item["citation"]["sourceOrdinal"] == 1
                && item["citation"]["memberId"] == "description_00000000000000000000000002")
    );
    query.target = Reference::Section {
        owner_id: OBJECT.into(),
        id: "description_00000000000000000000000001".into(),
    };
    assert!(matches!(
        retcon_impact::evaluate(&world, &world, &query, DIGEST),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn transition_and_live_knowledge_targets_remain_distinct_exact_references() {
    let world = fixture(vec![record(
        "knowledge",
        KNOW,
        &format!(
            "knower: {ANA}\nclaim: {{key: fixture, statement: Known., truth: unknown}}\ntransitions: [{{id: shared, time: {{tick: 1}}, state: uncertain, acquisition: heard, confidence: 0.5}}]\n"
        ),
        "",
    )]);
    for target in [
        Reference::Transition {
            owner_id: KNOW.into(),
            id: "shared".into(),
        },
        Reference::Knowledge {
            entity_id: KNOW.into(),
            transition_id: "shared".into(),
        },
    ] {
        let mut query = request(KNOW);
        query.target = target.clone();
        assert_eq!(
            evaluate(&world, &query)["target"],
            serde_json::to_value(target).unwrap()
        );
    }
    let mut query = request(KNOW);
    query.target = Reference::Knowledge {
        entity_id: KNOW.into(),
        transition_id: "absent".into(),
    };
    assert!(retcon_impact::evaluate(&world, &world, &query, DIGEST).is_err());
}

#[test]
fn reverse_cycles_terminate_and_depth_refuses_a_partial_result() {
    let a = "loc_00000000000000000000000001";
    let b = "loc_00000000000000000000000002";
    let world = fixture(vec![
        record("location", a, &format!("links: [{{location: {b}}}]\n"), ""),
        record("location", b, &format!("links: [{{location: {a}}}]\n"), ""),
    ]);
    let mut query = request(a);
    query.depth = 2;
    assert_eq!(
        evaluate(&world, &query)["items"].as_array().unwrap().len(),
        4
    );
    query.depth = 1;
    assert!(
        retcon_impact::evaluate(&world, &world, &query, DIGEST)
            .unwrap_err()
            .to_string()
            .contains("depth")
    );
}

#[test]
fn complete_aggregate_item_limit_counts_both_originals() {
    let world = fixture(vec![]);
    let mut query = request(ANA);
    query.max_items = 2;
    assert_eq!(
        evaluate(&world, &query)["items"].as_array().unwrap().len(),
        2
    );
    query.max_items = 1;
    assert!(
        retcon_impact::evaluate(&world, &world, &query, DIGEST)
            .unwrap_err()
            .to_string()
            .contains("item bound")
    );
}

#[test]
fn source_and_candidate_keep_separate_occurrences_and_candidate_provenance() {
    let base = fixture(vec![]);
    let candidate = changed(&base, OBJECT, |front, _| {
        front["initial_state"] = serde_yaml::from_str("label: plain").unwrap();
    });
    let report = retcon_impact::evaluate(&base, &candidate, &request(ANA), DIGEST).unwrap();
    assert_eq!(report["items"].as_array().unwrap().len(), 1);
    assert_eq!(report["items"][0]["snapshot"], "base");
    assert_eq!(
        report["items"][0]["citation"]["provenance"]["kind"],
        "source"
    );
    let inverse = retcon_impact::evaluate(&candidate, &base, &request(ANA), DIGEST).unwrap();
    assert_eq!(
        inverse["items"][0]["citation"]["provenance"],
        json!({"kind":"candidate","baseRevision":null,"planSha256":DIGEST})
    );
}

#[test]
fn request_schema_is_closed_exact_and_rejects_unsupported_author_sources() {
    let good = serde_json::to_value(request(ANA)).unwrap();
    assert!(retcon_impact::decode(&good.to_string()).is_ok());
    for (key, value) in [
        ("depth", json!(0)),
        ("maxItems", json!(1001)),
        ("undeclared", json!(true)),
    ] {
        let mut bad = good.clone();
        bad[key] = value;
        assert!(retcon_impact::decode(&bad.to_string()).is_err());
    }
    for value in [json!(10), json!("01"), json!("900719925474099300000")] {
        let mut bad = good.clone();
        bad["at"]["tick"] = value;
        assert!(retcon_impact::decode(&bad.to_string()).is_err());
    }
    let duplicate = good
        .to_string()
        .replacen("\"depth\":32", "\"depth\":32,\"depth\":1", 1);
    assert!(retcon_impact::decode(&duplicate).is_err());
    let mut bad = good;
    bad["target"] = json!({"kind":"author-record","id":"segment_00000000000000000000000001"});
    let decoded = retcon_impact::decode(&bad.to_string()).unwrap();
    let canonical_only = fixture(vec![]);
    assert!(matches!(
        retcon_impact::evaluate(&canonical_only, &canonical_only, &decoded, DIGEST),
        Err(Error::NotFound(_))
    ));
    bad["target"]["kind"] = json!("published-author-record");
    assert!(retcon_impact::decode(&bad.to_string()).is_err());
}

#[test]
fn complete_report_byte_bound_refuses_oversized_occurrence_citations() {
    let records = (1..=90).map(|n| {
        let mut event = record("event", &format!("event_{n:026}"), &format!("time: {{tick: 5, order: {n}}}\neffects: [{{id: effect_{n:026}, target: {ANA}, key: score, operation: set, value: 1}}]\n"), "");
        event.source_path = format!("story/{}/{}.md", "p".repeat(1800), event.id);
        event
    }).collect();
    let world = fixture(records);
    assert!(
        retcon_impact::evaluate(&world, &world, &request(ANA), DIGEST)
            .unwrap_err()
            .to_string()
            .contains("262144")
    );
}

#[test]
fn escaped_duplicate_fields_are_rejected_throughout_nested_preview_requests() {
    let world = fixture(vec![]);
    let payload = json!({"plan":{"expectedFingerprint":world.fingerprint,"operations":[{"type":"record.update","entityId":ANA,"patch":"bodyMarkdown: revised"}]},"impactRequest":request(ANA)}).to_string();
    assert!(retcon_impact::decode_preview(&payload).is_ok());
    for (original, duplicated) in [
        ("\"depth\":32", "\"depth\":32,\"dep\\u0074h\":32"),
        ("\"tick\":\"10\"", "\"tick\":\"10\",\"ti\\u0063k\":\"10\""),
        (
            "\"kind\":\"record\"",
            "\"kind\":\"record\",\"ki\\u006ed\":\"record\"",
        ),
        (
            "\"type\":\"record.update\"",
            "\"type\":\"record.update\",\"ty\\u0070e\":\"record.update\"",
        ),
        (
            "\"patch\":\"bodyMarkdown: revised\"",
            "\"patch\":\"bodyMarkdown: revised\",\"pa\\u0074ch\":\"bodyMarkdown: revised\"",
        ),
    ] {
        assert!(
            payload.contains(original),
            "missing fixture position {original}"
        );
        assert!(
            retcon_impact::decode_preview(&payload.replacen(original, duplicated, 1)).is_err(),
            "accepted duplicate {duplicated}"
        );
    }
}

#[test]
fn pure_evaluate_validates_both_original_sources() {
    let base = fixture(vec![]);
    let invalid = changed(&base, OBJECT, |front, _| {
        front["initial_state"]["holder"]["entity"] = "missing".into()
    });
    assert!(!validation::validate_world(&invalid).valid);
    assert!(matches!(
        retcon_impact::evaluate(&base, &invalid, &request(ANA), DIGEST),
        Err(Error::Validation(_))
    ));
    assert!(matches!(
        retcon_impact::evaluate(&invalid, &base, &request(ANA), DIGEST),
        Err(Error::Validation(_))
    ));
}

#[test]
fn nested_profile_references_keep_effect_member_coordinates_and_ignore_literal_fields() {
    let base = fixture(vec![]);
    let configured = changed(&base, WORLD, |front, _| {
        front["state_profile_bindings"] = serde_yaml::from_str("object: {holder: bundle}").unwrap();
        front["profiles"] = serde_yaml::from_str("- id: bundle\n  label: Typed bundle\n  fields:\n    type: object\n    required:\n      refs: {type: array, items: {type: entity, kinds: [character]}, max_items: 8}\n      note: {type: string}\n    optional: {}\n").unwrap();
    });
    let configured = changed(&configured, OBJECT, |front, _| {
        front["initial_state"]["holder"] =
            serde_yaml::from_str(&format!("refs: [{{kind: record, id: {ANA}}}]\nnote: {ANA}"))
                .unwrap();
    });
    let mut records: Vec<_> = configured.records.values().cloned().collect();
    records.push(record("event", EVENT, &format!("time: {{tick: 5}}\neffects:\n - {{id: effect_00000000000000000000000001, target: {OBJECT}, key: label, operation: set, value: literal}}\n - {{id: effect_00000000000000000000000002, target: {OBJECT}, key: holder, operation: set, value: {{refs: [{{kind: record, id: {ANA}}}, {{kind: record, id: {ANA}}}], note: {ANA}}}}}\n"), ""));
    let world = source::world_from_records(base.root.clone(), records, None).unwrap();
    let report = evaluate(&world, &request(ANA));
    let edges: Vec<_> = report["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| {
            item["snapshot"] == "base"
                && item["owner"]["id"] == EVENT
                && item["target"]["id"] == ANA
        })
        .collect();
    assert_eq!(edges.len(), 2);
    assert!(edges.iter().all(|item| item["relation"] == "typed-entity"
        && item["citation"]["sourceOrdinal"] == 1
        && item["citation"]["memberId"] == "effect_00000000000000000000000002"));
    assert!(report["items"].as_array().unwrap().iter().all(|item| {
        !item["citation"]["section"]
            .as_str()
            .unwrap()
            .ends_with(".note")
    }));
}

#[test]
fn live_evidence_targets_resolve_exact_members_and_never_collapse_to_owner_records() {
    let other = "char_00000000000000000000000002";
    let place = "loc_00000000000000000000000001";
    let scene = "scene_00000000000000000000000001";
    let conversation = "conv_00000000000000000000000001";
    let learned = "know_00000000000000000000000002";
    let base = fixture(vec![]);
    let configured = changed(&base, WORLD, |front, _| {
        front["capabilities"] = serde_yaml::from_str("[generational-core-v1, generational-knowledge-v1, lore-core-v1, descriptions-v1, briefs-v1]").unwrap()
    });
    let mut records: Vec<_> = configured.records.values().cloned().collect();
    records.extend([
        record("character", other, "", ""), record("location", place, "", ""),
        record("knowledge", KNOW, &format!("knower: {ANA}\nclaim: {{key: report, statement: An authored report.}}\ntransitions: [{{id: known, time: {{tick: 1}}, state: accepted, confidence: 1, acquisition: told}}]\n"), ""),
        record("scene", scene, &format!("status: closed\nlocation: {place}\ntime: {{start: {{tick: 0}}, current: {{tick: 3}}, end: {{tick: 3}}}}\nparticipants: [{{character: {ANA}, from: {{tick: 1}}}}]\nobservations: [{{id: observed, at: {{tick: 1}}, text: Report, audience: [{ANA}]}}]\n"), ""),
        record("conversation", conversation, &format!("status: closed\nlocation: {place}\ntime: {{start: {{tick: 0}}, end: {{tick: 3}}}}\nparticipants: [{{character: {ANA}}}, {{character: {other}}}]\nturns: [{{id: spoken, at: {{tick: 1}}, speaker: {other}, text: Report, audience: [participants]}}]\nrecollections: [{{id: remembered, at: {{tick: 4}}, character: {ANA}, state: remembered, confidence: 0.8, summary: A memory, exact_turns: [spoken]}}]\n"), ""),
        record("knowledge", learned, &format!("knower: {ANA}\nclaim:\n  key: learned\n  statement: Learned parentage\n  genealogy:\n    kind: parentage\n    payload: {{child_id: {ANA}, parent_id: {other}, basis: adoptive}}\n    valid: {{from: {{timeline: main, tick: 0, order: 0}}}}\n    evidence:\n     - {{kind: knowledge, entity_id: {KNOW}, transition_id: known}}\n     - {{kind: observation, entity_id: {scene}, observation_id: observed}}\n     - {{kind: turn, entity_id: {conversation}, turn_id: spoken}}\n     - {{kind: recollection, entity_id: {conversation}, recollection_id: remembered}}\ntransitions: [{{id: learned, time: {{tick: 5}}, state: accepted, confidence: 1, acquisition: told}}]\n"), ""),
    ]);
    let world = source::world_from_records(base.root.clone(), records, None).unwrap();
    let targets = [
        Reference::Knowledge {
            entity_id: KNOW.into(),
            transition_id: "known".into(),
        },
        Reference::Observation {
            entity_id: scene.into(),
            observation_id: "observed".into(),
        },
        Reference::Turn {
            entity_id: conversation.into(),
            turn_id: "spoken".into(),
        },
        Reference::Recollection {
            entity_id: conversation.into(),
            recollection_id: "remembered".into(),
        },
    ];
    let owner_report = evaluate(&world, &request(ANA));
    let participant = owner_report["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| {
            item["owner"]["id"] == scene
                && item["citation"]["section"] == "participants[0].character"
        })
        .unwrap();
    assert_eq!(
        participant["citation"]["time"],
        json!({"timeline":"main","tick":"1","order":"0"})
    );
    for target in targets {
        let mut query = request(ANA);
        query.target = target.clone();
        let report = evaluate(&world, &query);
        assert!(
            report["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["owner"]["id"] == learned
                    && item["target"] == serde_json::to_value(&target).unwrap()
                    && item["relation"] == "genealogy-evidence")
        );
        assert!(!report["items"].as_array().unwrap().iter().any(|item| {
            item["owner"]["id"] == learned
                && item["citation"]["section"]
                    .as_str()
                    .unwrap()
                    .ends_with(".entity_id")
        }));
    }
}

#[test]
fn public_preview_is_read_only_guards_exact_current_source_and_retains_required_checks() {
    let base = fixture(vec![]);
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("story")).unwrap();
    for record in base.records.values() {
        std::fs::write(
            directory.path().join(&record.source_path),
            &record.raw_bytes,
        )
        .unwrap();
    }
    let world = source::load_world(directory.path(), None).unwrap();
    let mut preview = retcon_impact::PreviewRequest {
        plan: wedl::consequences::Plan {
            expected_fingerprint: world.fingerprint.clone(),
            operations: vec![wedl::consequences::Operation::Update {
                entity_id: ANA.into(),
                patch: "kind: character\nbody: An explicit revised body.\n".into(),
            }],
            analysis: None,
        },
        impact_request: request(ANA),
    };
    let result = retcon_impact::preview(&world, &preview).unwrap();
    assert_eq!(result["outcome"], "ok");
    assert_eq!(result["canApply"], false);
    assert_eq!(result["writesPerformed"], false);
    for record in world.records.values() {
        assert_eq!(
            std::fs::read(directory.path().join(&record.source_path)).unwrap(),
            record.raw_bytes
        );
    }
    assert!(!directory.path().join("author").exists());
    assert!(!directory.path().join(".wedl").exists());
    let mut forged = world.clone();
    forged.records.get_mut(ANA).unwrap().body = "forged model only".into();
    assert!(matches!(
        retcon_impact::preview(&forged, &preview),
        Err(Error::Conflict(_))
    ));
    preview.plan.analysis = Some(wedl::consequence_semantics::Analysis {
        at: preview.impact_request.at.clone(),
        policy: wedl::consequence_semantics::Policy::Required,
        expectations: vec![wedl::consequence_semantics::Expectation {
            id: "required-score".into(),
            predicate: wedl::consequence_semantics::Predicate::StateEquals {
                target_id: ANA.into(),
                key: "score".into(),
                expected_value_raw: "999".into(),
            },
        }],
    });
    assert!(matches!(
        retcon_impact::preview(&world, &preview),
        Err(Error::Conflict(_))
    ));
    preview.plan.analysis.as_mut().unwrap().policy = wedl::consequence_semantics::Policy::Advisory;
    assert!(retcon_impact::preview(&world, &preview).is_ok());
    std::fs::write(
        directory.path().join(&world.records[ANA].source_path),
        [
            world.records[ANA].raw_bytes.as_slice(),
            b"\nA newer live source.\n",
        ]
        .concat(),
    )
    .unwrap();
    assert!(matches!(
        retcon_impact::preview(&world, &preview),
        Err(Error::Conflict(_))
    ));
}

#[test]
fn native_story_point_prerequisite_objects_supply_exact_predicate_edges() {
    let first = "sp_00000000000000000000000001";
    let second = "sp_00000000000000000000000002";
    let world = fixture(vec![
        record(
            "story-point",
            first,
            "activation_policy: manual\nlifecycle: {initial_state: resolved}\n",
            "",
        ),
        record(
            "story-point",
            second,
            &format!(
                "activation_policy: suggest\nlifecycle: {{initial_state: dormant}}\nrequires: [{{id: {first}, states: [resolved]}}]\n"
            ),
            "",
        ),
    ]);
    let report = evaluate(&world, &request(first));
    let items = report["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|item| item["owner"]["id"] == second
        && item["target"]["id"] == first
        && item["relation"] == "predicate"
        && item["citation"]["section"] == "requires[0].id"));
}
