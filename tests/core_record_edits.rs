//! Core edits share the original candidate, exact-source and writer contracts.
use serde_json::{Value as Json, json};
use std::{fs, path::PathBuf};
use wedl::{
    Error, authoring, consequences,
    model::{Record, StoryTime, World},
    record_edit, runtime, source, story, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const ANA: &str = "char_00000000000000000000000001";
const BOB: &str = "char_00000000000000000000000002";
const PLACE: &str = "loc_00000000000000000000000001";
const OBJECT: &str = "obj_00000000000000000000000001";
const EVENT: &str = "event_00000000000000000000000001";
const ENV: &str = "env_00000000000000000000000001";
const REL: &str = "rel_00000000000000000000000001";
const INVERSE: &str = "rel_00000000000000000000000002";
const POINT: &str = "sp_00000000000000000000000001";
const SECOND: &str = "sp_00000000000000000000000002";
const HYP: &str = "hyp_00000000000000000000000001";

fn record(kind: &str, id: &str, fields: Json) -> Record {
    let mut value = json!({"schema":"wedl/v1","kind":kind,"id":id,"title":id});
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let header = format!(
        "---\n# Preserve comments for unchanged drafts.\n{}---\n",
        serde_yaml::to_string(&value).unwrap()
    )
    .replace('\n', "\r\n");
    let mut record = source::parse_record(
        format!("{header}Authored α  \r\n\t").as_bytes(),
        &format!("story/{id}.md"),
    )
    .unwrap();
    record.blob_oid = Some("a".repeat(40));
    record
}
fn fixture() -> World {
    let world = source::world_from_records(PathBuf::from("/core-edits-no-fs"), vec![
        record("world", WORLD, json!({"capabilities":[],"threads":[],"timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"default_timeline":"main","state_keys":{"character":{"mood":{"type":"string"}}},"relationship_metrics":{"trust":{"min":-1,"max":1,"default":0},"count":{}}})),
        record("character", ANA, json!({"initial_state":{"mood":"calm"},"x-opaque":{"v":[null,1,1.0,"1"]}})),
        record("character", BOB, json!({"initial_state":{}})),
        record("location", PLACE, json!({})),
        record("object", OBJECT, json!({"initial_state":{},"x-opaque":{"empty":null}})),
        record("event", EVENT, json!({"time":{"tick":5},"effects":[]})),
    ], None).unwrap();
    assert_valid(&world);
    world
}
fn assert_valid(world: &World) {
    let report = validation::validate_world(world);
    assert!(report.valid, "{:?}", report.diagnostics);
}
fn add(world: &World, record: Record) -> World {
    let mut records = world.records.clone();
    records.insert(record.id.clone(), record);
    source::world_from_records(world.root.clone(), records.into_values().collect(), None).unwrap()
}
fn create(world: &World, kind: &str, id: &str, fields: &str) -> World {
    let record = record_edit::create(
        world,
        &format!("kind: {kind}\nid: {id}\ntitle: New {kind}\n{fields}"),
    )
    .unwrap();
    let result = add(world, record);
    assert_valid(&result);
    result
}
fn populated() -> World {
    let mut world = fixture();
    world = create(
        &world,
        "environment",
        ENV,
        &format!(
            "time: {{start: {{tick: 0}}, end: null}}\ntargets: [{ANA}, {PLACE}]\nconditions: {{rain: true, unknown: null, count: 9007199254740993, literal: '{REL}'}}\nsensory: [Wet stone]\n"
        ),
    );
    world = create(
        &world,
        "relationship",
        REL,
        &format!(
            "from: {ANA}\nto: {BOB}\nrelationship_kind: friends\ntransitions: [{{id: met, time: {{tick: 5}}, causing_event: {EVENT}, metrics: {{trust: 0.5, count: 9007199254740993}}, facets: [friend], note: ''}}]\n"
        ),
    );
    world = create(
        &world,
        "story-point",
        POINT,
        &format!(
            "activation_policy: suggest\nlifecycle: {{initial_state: dormant, transitions: []}}\nwhen: {{all: [{{event: {EVENT}}}, {{state: {{entity: {ANA}, key: mood, equals: calm}}}}, {{relationship: {{id: {REL}, metric: trust, min: 0.4}}}}]}}\noutcome_events: [{EVENT}]\npriority: -1\n"
        ),
    );
    create(
        &world,
        "hypothesis",
        HYP,
        &format!(
            "statement: Maybe the rain will stop\nsubjects: [{ANA}]\nalternatives: [A possibility, A possibility]\ncontext: An authored question\nplacement: {{context: Before the next scene, timeline: main, event: {EVENT}, location: null}}\n"
        ),
    )
}
fn at(tick: i64) -> StoryTime {
    StoryTime::new("main", tick, 0).unwrap()
}
fn disk(world: &World) -> (tempfile::TempDir, World) {
    let dir = tempfile::tempdir().unwrap();
    for record in world.records.values() {
        let path = dir.path().join(&record.source_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, &record.raw_bytes).unwrap();
    }
    let loaded = source::load_world(dir.path(), None).unwrap();
    (dir, loaded)
}

#[test]
fn four_new_core_kinds_create_native_records_without_invented_lifecycle_or_prose() {
    let world = populated();
    for (id, directory, status) in [
        (ENV, "environments", "canonical"),
        (REL, "relationships", "canonical"),
        (POINT, "story-points", "canonical"),
        (HYP, "hypotheses", "open"),
    ] {
        let record = &world.records[id];
        assert_eq!(record.source_path, format!("story/{directory}/{id}.md"));
        assert_eq!(record.status(), status);
        assert_eq!(record.body, "");
        assert!(record.list("aliases").unwrap().is_empty());
        assert!(record.list("tags").unwrap().is_empty());
    }
    assert!(!world.records.contains_key(INVERSE));
    assert_eq!(
        world.records[REL].get("transitions").unwrap()[0]["metrics"]["count"].as_i64(),
        Some(9_007_199_254_740_993)
    );
    assert!(world.records[ENV].get("conditions").unwrap()["unknown"].is_null());
    assert_eq!(world.records[HYP].list("alternatives").unwrap().len(), 2);
}

#[test]
fn environment_and_relationship_reads_preserve_literal_values_and_authored_direction() {
    let world = populated();
    let environment = story::environments(&world, ANA, &at(10)).unwrap();
    assert_eq!(
        environment["items"][0]["conditions"]["count"],
        "9007199254740993"
    );
    assert_eq!(environment["items"][0]["conditions"]["literal"], REL);
    assert_eq!(
        runtime::relationships(&world, BOB, &at(10)).unwrap()["items"],
        json!([])
    );
    let relationship = runtime::relationships(&world, ANA, &at(10)).unwrap();
    assert_eq!(
        relationship["items"][0]["metrics"]["count"],
        json!(9_007_199_254_740_993_i64)
    );
    assert_eq!(relationship["items"][0]["status"], "active");
}

#[test]
fn story_point_edits_preserve_explicit_lifecycle_even_when_eligibility_changes() {
    let world = populated();
    let original = world.records[POINT].raw_bytes.clone();
    assert_eq!(
        story::story_points(&world, &at(4)).unwrap()["items"][0]["eligible"],
        false
    );
    assert_eq!(
        story::story_points(&world, &at(5)).unwrap()["items"][0]["eligible"],
        true
    );
    assert_eq!(world.records[POINT].raw_bytes, original);
    let request = format!(
        "kind: story-point\nlifecycle: {{initial_state: dormant, transitions: [{{id: chosen, state: active, time: {{tick: 6}}, causing_event: {EVENT}}}]}}\n"
    );
    let next = add(
        &world,
        record_edit::update(&world, POINT, &request).unwrap(),
    );
    assert_valid(&next);
    assert_eq!(
        story::story_points(&next, &at(6)).unwrap()["items"][0]["storedState"],
        "active"
    );
    assert_eq!(
        next.records[EVENT].raw_bytes,
        world.records[EVENT].raw_bytes
    );
    assert_eq!(next.records[HYP].status(), "open");
}

#[test]
fn core_updates_preserve_omissions_and_noops_and_replace_complete_collections() {
    let world = populated();
    for (id, kind) in [
        (ENV, "environment"),
        (REL, "relationship"),
        (POINT, "story-point"),
        (HYP, "hypothesis"),
    ] {
        let before = &world.records[id];
        let unchanged = record_edit::update(&world, id, &format!("kind: {kind}\n")).unwrap();
        assert_eq!(unchanged.raw_bytes, before.raw_bytes);
        let after =
            record_edit::update(&world, id, &format!("kind: {kind}\ntitle: Edited\n")).unwrap();
        assert_eq!(after.body, before.body);
        for (key, value) in before.frontmatter.as_mapping().unwrap() {
            if key.as_str() != Some("title") {
                assert_eq!(after.frontmatter.get(key), Some(value));
            }
        }
    }
    for (id, patch, field) in [
        (
            ENV,
            "kind: environment\nconditions: {}\nsensory: []\n",
            "conditions",
        ),
        (
            REL,
            "kind: relationship\ntransitions: []\ninverse: null\n",
            "transitions",
        ),
        (
            POINT,
            "kind: story-point\nrequires: []\nwhen: {all: []}\noutcome_events: []\n",
            "outcome_events",
        ),
    ] {
        let after = record_edit::update(&world, id, patch).unwrap();
        assert!(
            after
                .get(field)
                .unwrap()
                .as_mapping()
                .is_some_and(|v| v.is_empty())
                || after.list(field).is_some_and(|v| v.is_empty())
        );
        assert_valid(&add(&world, after));
    }
}

#[test]
fn object_affordances_preserve_order_duplicates_empty_strings_and_absence() {
    let world = fixture();
    assert!(world.records[OBJECT].get("object_affordances").is_none());
    let no_op = record_edit::update(&world, OBJECT, "kind: object\n").unwrap();
    assert_eq!(no_op.raw_bytes, world.records[OBJECT].raw_bytes);
    let request = "kind: object\nobject_affordances: [open, '', open, '  exact  ']\n";
    let added = record_edit::update(&world, OBJECT, request).unwrap();
    assert_eq!(
        serde_json::to_value(added.get("object_affordances").unwrap()).unwrap(),
        json!(["open", "", "open", "  exact  "])
    );
    assert_eq!(added.body, world.records[OBJECT].body);
    assert_eq!(added.get("x-opaque"), world.records[OBJECT].get("x-opaque"));
    let next = add(&world, added);
    let cleared =
        record_edit::update(&next, OBJECT, "kind: object\nobject_affordances: []\n").unwrap();
    assert!(cleared.list("object_affordances").unwrap().is_empty());
    assert_valid(&add(&next, cleared));
    for patch in [
        "kind: object\nobject_affordances: [true]",
        "kind: object\nobject_affordances: null",
        "kind: character\nobject_affordances: []",
    ] {
        let id = if patch.starts_with("kind: character") {
            ANA
        } else {
            OBJECT
        };
        assert!(record_edit::update(&world, id, patch).is_err());
    }
}

#[test]
fn environment_initial_state_uses_existing_declarations_and_stays_separate_from_conditions() {
    let world = fixture();
    let updated = record_edit::update(&world, WORLD, "kind: world\nstate_keys: {character: {mood: {type: string}}, environment: {visible: {type: boolean}}}\n").unwrap();
    let world = add(&world, updated);
    let world = create(
        &world,
        "environment",
        ENV,
        &format!(
            "time: {{start: {{tick: 0}}}}\ntargets: [{PLACE}]\nconditions: {{visible: 'an authored condition label'}}\ninitial_state: {{visible: true}}\n"
        ),
    );
    let before = world.records[ENV].get("conditions").unwrap().clone();
    let changed = add(
        &world,
        record_edit::update(
            &world,
            ENV,
            "kind: environment\ninitial_state: {visible: false}\n",
        )
        .unwrap(),
    );
    assert_valid(&changed);
    assert_eq!(changed.records[ENV].get("conditions"), Some(&before));
    let rejected = authoring::preview_record_update(
        &world,
        ENV,
        "kind: environment\ninitial_state: {visible: wrong-type}\n",
        &world.fingerprint,
    )
    .unwrap();
    assert_eq!(rejected["validation"]["valid"], false);
}

#[test]
fn relationship_facets_preserve_literal_order_duplicates_and_empty_entries() {
    let world = populated();
    let request = "kind: relationship\ntransitions: [{id: authored, time: {tick: 5}, facets: [friend, '', friend, '  exact  ']}]\n";
    let changed = add(&world, record_edit::update(&world, REL, request).unwrap());
    assert_valid(&changed);
    let expected = json!(["friend", "", "friend", "  exact  "]);
    assert_eq!(
        runtime::relationships(&changed, ANA, &at(5)).unwrap()["items"][0]["facets"],
        expected
    );
    assert_eq!(
        serde_json::to_value(&changed.records[REL].get("transitions").unwrap()[0]["facets"])
            .unwrap(),
        expected
    );
    assert!(
        record_edit::update(
            &world,
            REL,
            "kind: relationship\ntransitions: [{id: authored, time: {tick: 5}, facets: [true]}]\n"
        )
        .is_err()
    );
}

#[test]
fn ordinary_scope_labels_remain_literal_while_knowledge_keeps_its_unique_label_contract() {
    let world = fixture();
    let labels = json!(["author", "", "author", "  exact  "]);
    for (id, kind) in [(WORLD, "world"), (ANA, "character")] {
        let patch = format!("kind: {kind}\naudience: {labels}\nperspectives: {labels}\n");
        let updated = record_edit::update(&world, id, &patch).unwrap();
        let next = add(&world, updated.clone());
        assert_valid(&next);
        assert_eq!(
            serde_json::to_value(updated.get("audience").unwrap()).unwrap(),
            labels
        );
        assert_eq!(
            serde_json::to_value(updated.get("perspectives").unwrap()).unwrap(),
            labels
        );
        assert_eq!(
            record_edit::update(&next, id, &patch).unwrap().raw_bytes,
            updated.raw_bytes
        );
    }
    let knowledge = format!(
        "kind: knowledge\nid: know_00000000000000000000000001\ntitle: A claim\nknower: {ANA}\nclaim: {{key: note, statement: Text}}\naudience: [author, author]\n"
    );
    assert!(record_edit::create(&world, &knowledge).is_err());
}

#[test]
fn raw_relationship_defaults_and_extensions_survive_while_typed_replacements_stay_closed() {
    let world = fixture();
    let raw = record(
        "relationship",
        REL,
        json!({"from":ANA,"to":BOB,"inverse":null,"relationship_kind":null,
        "transitions":[{"id":"authored","time":{"tick":5,"x-position":{"chronology":"literal"}},
            "relationship_status":null,"metrics":null,"facets":null,"note":null,"causing_event":null,
            "x-metadata":{"chronology":["literal"],"value":null}}]}),
    );
    let world = add(&world, raw.clone());
    assert_valid(&world);
    let read = runtime::relationships(&world, ANA, &at(5)).unwrap();
    assert_eq!(read["items"][0]["status"], "active");
    assert_eq!(read["items"][0]["metrics"], json!({}));
    assert_eq!(read["items"][0]["facets"], json!([]));
    assert_eq!(
        record_edit::update(&world, REL, "kind: relationship\n")
            .unwrap()
            .raw_bytes,
        raw.raw_bytes
    );
    let renamed = record_edit::update(&world, REL, "kind: relationship\ntitle: Renamed\n").unwrap();
    assert_eq!(renamed.get("transitions"), raw.get("transitions"));
    for metrics in [
        json!({"trust":true}),
        json!({"trust":"authored unsupported value"}),
        json!("unsupported shape"),
    ] {
        let raw = record(
            "relationship",
            REL,
            json!({"from":ANA,"to":BOB,
            "transitions":[{"id":"authored","time":{"tick":5},"metrics":metrics,"facets":[true,null,"literal"]}]}),
        );
        let changed = add(&world, raw.clone());
        assert_valid(&changed);
        assert_eq!(
            record_edit::update(&changed, REL, "kind: relationship\n")
                .unwrap()
                .raw_bytes,
            raw.raw_bytes
        );
        if metrics.is_object() {
            let read = runtime::relationships(&changed, ANA, &at(5)).unwrap();
            assert_eq!(read["items"][0]["metrics"], metrics);
            assert_eq!(read["items"][0]["facets"], json!([true, null, "literal"]));
        } else {
            assert!(runtime::relationships(&changed, ANA, &at(5)).is_err());
        }
    }
    for transition in [
        "{id: authored, time: {tick: 5}, metrics: null}",
        "{id: authored, time: {tick: 5}, metrics: {trust: true}}",
        "{id: authored, time: {tick: 5}, facets: [true]}",
        "{id: authored, time: {tick: 5}, x-metadata: {literal: true}}",
        "{id: authored, time: {tick: 5, x-position: {literal: true}}}",
    ] {
        assert!(
            record_edit::update(
                &world,
                REL,
                &format!("kind: relationship\ntransitions: [{transition}]\n")
            )
            .is_err(),
            "{transition}"
        );
    }
}

#[test]
fn new_core_shapes_are_closed_and_reject_wrong_literal_types() {
    let world = populated();
    for (id, patch) in [
        (ENV, "kind: environment\nconditions: {nested: []}"),
        (ENV, "kind: environment\nconditions: {bad: .nan}"),
        (ENV, "kind: environment\ntime: {start: null}"),
        (ENV, "kind: environment\ntargets: []"),
        (ENV, "kind: environment\nsensory: [same, same]"),
        (
            REL,
            "kind: relationship\ntransitions: [{id: x, time: {tick: 5}, metrics: {trust: '0.5'}}]",
        ),
        (
            REL,
            "kind: relationship\ntransitions: [{id: x, time: {tick: 5}, invented: true}]",
        ),
        (
            REL,
            "kind: relationship\ntransitions: [{id: x, time: {tick: 5}}, {id: x, time: {tick: 6}}]",
        ),
        (POINT, "kind: story-point\nactivation_policy: automatic"),
        (
            POINT,
            "kind: story-point\nlifecycle: {initial_state: eligible}",
        ),
        (POINT, "kind: story-point\nwhen: {all: [], any: []}"),
        (POINT, "kind: story-point\nwhen: {unknown: true}"),
        (POINT, "kind: story-point\npriority: 1.0"),
        (POINT, "kind: story-point\npriority: 2147483648"),
        (POINT, "kind: story-point\ntrigger: {}"),
        (HYP, "kind: hypothesis\nthreads: []"),
        (HYP, "kind: hypothesis\naudience: [author]"),
        (HYP, "kind: hypothesis\nchronology: []"),
        (HYP, "kind: hypothesis\nstatus: canonical"),
    ] {
        assert!(record_edit::update(&world, id, patch).is_err(), "{patch}");
    }
}

#[test]
fn complete_candidates_refuse_bad_references_temporal_causes_and_story_cycles() {
    let world = populated();
    for (id, patch) in [
        (ENV, format!("kind: environment\ntargets: [char_00000000000000000000000099]\n")),
        (ENV, "kind: environment\ntime: {start: {tick: 5}, end: {tick: 4}}\n".into()),
        (REL, format!("kind: relationship\ninverse: {INVERSE}\n")),
        (REL, "kind: relationship\ntransitions: [{id: x, time: {tick: 5}, metrics: {undeclared: 1}}]\n".into()),
        (REL, format!("kind: relationship\ntransitions: [{{id: x, time: {{tick: 4}}, causing_event: {EVENT}}}]\n")),
        (REL, format!("kind: relationship\ntransitions: [{{id: x, time: {{timeline: other, tick: 5}}, causing_event: {EVENT}}}]\n")),
        (POINT, format!("kind: story-point\nrequires: [{{id: {POINT}, states: [resolved]}}]\n")),
        (POINT, format!("kind: story-point\nwhen: {{relationship: {{id: {REL}, metric: missing, min: 0}}}}\n")),
        (HYP, "kind: hypothesis\nplacement: {timeline: absent}\n".into()),
    ] {
        let preview = authoring::preview_record_update(&world, id, &patch, &world.fingerprint).unwrap();
        assert_eq!(preview["canApply"], false, "{patch}: {preview}");
        assert_eq!(preview["validation"]["valid"], false);
    }
}

#[test]
fn a_complete_plan_accepts_new_core_forward_references_and_validates_all_records_together() {
    let world = fixture();
    let plan = consequences::Plan {
        expected_fingerprint: world.fingerprint.clone(),
        analysis: None,
        operations: vec![
            consequences::Operation::Create {
                draft: format!(
                    "kind: relationship\nid: {REL}\ntitle: A to B\nfrom: {ANA}\nto: {BOB}\ninverse: {INVERSE}\n"
                ),
            },
            consequences::Operation::Create {
                draft: format!(
                    "kind: relationship\nid: {INVERSE}\ntitle: B to A\nfrom: {BOB}\nto: {ANA}\ninverse: {REL}\n"
                ),
            },
            consequences::Operation::Create {
                draft: format!(
                    "kind: story-point\nid: {POINT}\ntitle: First\nactivation_policy: manual\nlifecycle: {{initial_state: dormant}}\nrequires: [{{id: {SECOND}, states: [resolved]}}]\n"
                ),
            },
            consequences::Operation::Create {
                draft: format!(
                    "kind: story-point\nid: {SECOND}\ntitle: Second\nactivation_policy: suggest\nlifecycle: {{initial_state: resolved}}\n"
                ),
            },
            consequences::Operation::Update {
                entity_id: EVENT.into(),
                patch: format!("kind: event\nrelated_story_points: [{POINT}]\n"),
            },
        ],
    };
    let result = consequences::preview(&world, &plan, 1_000_000).unwrap();
    assert_eq!(result["validation"]["valid"], true, "{result}");
    assert_eq!(result["canApply"], false);
    assert_eq!(world.records.len(), 6);
}

#[test]
fn guarded_core_publication_uses_the_same_exact_preview_and_rejects_stale_hashes() {
    let (_dir, world) = disk(&populated());
    let request = "kind: environment\nconditions: {fog: true}\nsensory: []\n";
    let preview =
        authoring::preview_record_update(&world, ENV, request, &world.fingerprint).unwrap();
    assert_eq!(preview["canApply"], true, "{preview}");
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
    assert!(matches!(
        authoring::apply_record_update(&world, ENV, request, &world.fingerprint, &"0".repeat(64)),
        Err(Error::Conflict(_))
    ));
    authoring::apply_record_update(
        &world,
        ENV,
        request,
        &world.fingerprint,
        preview["afterSha256"].as_str().unwrap(),
    )
    .unwrap();
    let saved = source::load_world(&world.root, None).unwrap();
    assert_eq!(saved.fingerprint, preview["candidateFingerprint"]);
    assert_eq!(
        saved.records[ENV].raw_bytes,
        preview["afterSource"].as_str().unwrap().as_bytes()
    );
    for (id, record) in &world.records {
        if id != ENV {
            assert_eq!(saved.records[id].raw_bytes, record.raw_bytes);
        }
    }
    assert!(matches!(
        authoring::apply_record_update(
            &world,
            ENV,
            request,
            &world.fingerprint,
            preview["afterSha256"].as_str().unwrap()
        ),
        Err(Error::Conflict(_))
    ));
}

#[test]
fn guarded_create_for_each_new_core_kind_saves_the_exact_preview() {
    let (_dir, mut world) = disk(&fixture());
    for (kind, id, fields) in [
        (
            "environment",
            ENV,
            format!("targets: [{PLACE}]\ntime: {{start: {{tick: 0}}}}\n"),
        ),
        ("relationship", REL, format!("from: {ANA}\nto: {BOB}\n")),
        (
            "story-point",
            POINT,
            "activation_policy: manual\nlifecycle: {initial_state: dormant}\n".into(),
        ),
        (
            "hypothesis",
            HYP,
            format!(
                "statement: An authored possibility\ncontext: Pending\nsubjects: [{ANA}]\nalternatives: [Perhaps]\nplacement: {{context: Pending}}\n"
            ),
        ),
    ] {
        let request = format!("kind: {kind}\nid: {id}\ntitle: {kind}\n{fields}");
        let preview =
            authoring::preview_record_create(&world, &request, &world.fingerprint).unwrap();
        assert_eq!(preview["canApply"], true, "{preview}");
        authoring::apply_record_create(
            &world,
            &request,
            &world.fingerprint,
            preview["afterSha256"].as_str().unwrap(),
        )
        .unwrap();
        world = source::load_world(&world.root, None).unwrap();
        assert_eq!(
            world.records[id].raw_bytes,
            preview["afterSource"].as_str().unwrap().as_bytes()
        );
    }
}
