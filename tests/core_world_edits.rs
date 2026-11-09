//! Declaration edits validate every dependent record in the final candidate.
use serde_json::{Value as Json, json};
use std::path::PathBuf;
use wedl::{
    authoring, consequences,
    model::{Record, StoryTime, World},
    record_edit, runtime, source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const ANA: &str = "char_00000000000000000000000001";
const BOB: &str = "char_00000000000000000000000002";
const PLACE: &str = "loc_00000000000000000000000001";
const OTHER: &str = "loc_00000000000000000000000002";
const REL: &str = "rel_00000000000000000000000001";
const EVENT: &str = "event_00000000000000000000000001";
const THREAD: &str = "thread_00000000000000000000000001";

fn record(kind: &str, id: &str, fields: Json) -> Record {
    let mut value = json!({"schema":"wedl/v1","kind":kind,"id":id,"title":id});
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    source::parse_record(
        format!(
            "---\n# Authored comment.\n{}---\nExact prose α  \r\n",
            serde_yaml::to_string(&value).unwrap()
        )
        .as_bytes(),
        &format!("story/{id}.md"),
    )
    .unwrap()
}
fn fixture() -> World {
    let world = source::world_from_records(PathBuf::from("/core-world-no-fs"), vec![
        record("world",WORLD,json!({"domain":"authored.world","capabilities":[],"threads":[{"id":THREAD,"label":"One"}],"timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"default_timeline":"main","current_time":{"timeline":"main","tick":0,"order":0},"state_keys":{},"relationship_metrics":{"trust":{"min":-1,"max":1,"default":0},"spare":{}},"x-opaque":{"n":null,"a":[1,1.0,"1"]}})),
        record("character",ANA,json!({"initial_state":{},"threads":[THREAD],"x-opaque":{"literal":null}})),
        record("character",BOB,json!({"initial_state":{}})),
        record("location",PLACE,json!({})),
        record("location",OTHER,json!({})),
        record("event",EVENT,json!({"time":{"timeline":"main","tick":0,"order":0},"effects":[]})),
        record("relationship",REL,json!({"from":ANA,"to":BOB,"transitions":[{"id":"met","time":{"tick":0},"metrics":{"trust":0.5},"facets":[]}]})),
    ], None).unwrap();
    valid(&world);
    world
}
fn valid(world: &World) {
    let result = validation::validate_world(world);
    assert!(result.valid, "{:?}", result.diagnostics);
}
fn replaced(world: &World, record: Record) -> World {
    let mut records = world.records.clone();
    records.insert(record.id.clone(), record);
    source::world_from_records(world.root.clone(), records.into_values().collect(), None).unwrap()
}
fn preview(world: &World, patch: &str) -> Json {
    authoring::preview_record_update(world, WORLD, patch, &world.fingerprint).unwrap()
}
fn applied(world: &World, id: &str, patch: &str) -> World {
    let next = replaced(world, record_edit::update(world, id, patch).unwrap());
    valid(&next);
    next
}

#[test]
fn world_and_common_edits_preserve_omissions_prose_and_exact_noops() {
    let world = fixture();
    let patch = format!(
        "kind: world\ndomain: authored.revised\nstatus: canonical\ncapabilities: [lore-core-v1]\ntimelines: [{{id: main, label: Main, origin: {{tick: -9223372036854775808, label: Beginning}}}}, {{id: other, label: Other}}]\ndefault_timeline: other\nthreads: [{{id: {THREAD}, label: Renamed}}]\nrelationship_metrics: {{trust: {{min: -1, max: 1, default: 0}}}}\nsection_audiences: {{Notes: [author, self, {ANA}]}}\naudience: [public, author]\nperspectives: [reader, author]\n"
    );
    let updated = applied(&world, WORLD, &patch);
    assert_eq!(updated.records[WORLD].body, world.records[WORLD].body);
    assert_eq!(
        updated.records[WORLD].get("x-opaque"),
        world.records[WORLD].get("x-opaque")
    );
    assert_eq!(
        updated.records[WORLD].get("current_time"),
        world.records[WORLD].get("current_time")
    );
    for (id, record) in &world.records {
        if id != WORLD {
            assert_eq!(updated.records[id].raw_bytes, record.raw_bytes);
        }
    }
    let no_op = record_edit::update(&updated, WORLD, &patch).unwrap();
    assert_eq!(no_op.raw_bytes, updated.records[WORLD].raw_bytes);
    assert_eq!(
        updated.records[WORLD].list("audience").unwrap()[0].as_str(),
        Some("public")
    );
}

#[test]
fn common_scope_metadata_replaces_whole_maps_and_retains_authored_array_order() {
    let world = fixture();
    let patch = format!(
        "kind: character\ndomain: authored.people\nstatus: retired\nsection_audiences: {{Private: [self, {BOB}], Public: [public]}}\naudience: [reader, author]\nperspectives: [unreliable, author]\n"
    );
    let changed = applied(&world, ANA, &patch);
    assert_eq!(changed.records[ANA].status(), "retired");
    assert_eq!(
        changed.records[ANA].get("x-opaque"),
        world.records[ANA].get("x-opaque")
    );
    let cleared = applied(
        &changed,
        ANA,
        "kind: character\nsection_audiences: {}\naudience: []\nperspectives: []\nthreads: []\n",
    );
    assert!(
        cleared.records[ANA]
            .get("section_audiences")
            .unwrap()
            .as_mapping()
            .unwrap()
            .is_empty()
    );
    assert!(cleared.records[ANA].list("audience").unwrap().is_empty());
    assert!(cleared.records[ANA].list("threads").unwrap().is_empty());
    assert_eq!(cleared.records[ANA].body, world.records[ANA].body);
}

#[test]
fn removals_of_used_threads_timelines_and_metrics_return_invalid_reviewable_candidates() {
    let world = fixture();
    for patch in [
        "kind: world\nthreads: []\n",
        "kind: world\nrelationship_metrics: {}\n",
        "kind: world\ntimelines: [{id: other, label: Other}]\ndefault_timeline: other\n",
        "kind: world\ndefault_timeline: missing\n",
    ] {
        let result = preview(&world, patch);
        assert_eq!(result["validation"]["valid"], false, "{patch}: {result}");
        assert_eq!(result["canApply"], false);
    }
    let valid_removal = preview(
        &world,
        "kind: world\ntimelines: [{id: main, label: Main}]\nrelationship_metrics: {trust: {}}\n",
    );
    assert_eq!(
        valid_removal["validation"]["valid"], true,
        "{valid_removal}"
    );
}

#[test]
fn capabilities_cannot_be_removed_while_a_record_uses_them() {
    let world = applied(
        &fixture(),
        WORLD,
        "kind: world\ncapabilities: [spatial-core-v1, route-v1]\n",
    );
    let route = record_edit::create(&world,&format!("kind: route\nid: route:harbor/path\ntitle: Path\nfrom_location_id: {PLACE}\nto_location_id: {OTHER}\ndirection: one-way\nmodes: [walk]\n")).unwrap();
    let world = replaced(&world, route);
    valid(&world);
    let result = preview(&world, "kind: world\ncapabilities: []\n");
    assert_eq!(result["validation"]["valid"], false, "{result}");
    let missing_prerequisite = preview(&world, "kind: world\ncapabilities: [route-v1]\n");
    assert_eq!(
        missing_prerequisite["validation"]["valid"], false,
        "{missing_prerequisite}"
    );
}

#[test]
fn a_plan_can_replace_declarations_and_all_dependents_together() {
    let world = fixture();
    let plan = consequences::Plan {
        expected_fingerprint: world.fingerprint.clone(),
        analysis: None,
        operations: vec![
            consequences::Operation::Update {
                entity_id: WORLD.into(),
                patch: "kind: world\nthreads: []\nrelationship_metrics: {}\n".into(),
            },
            consequences::Operation::Update {
                entity_id: ANA.into(),
                patch: "kind: character\nthreads: []\n".into(),
            },
            consequences::Operation::Update {
                entity_id: REL.into(),
                patch: "kind: relationship\ntransitions: []\n".into(),
            },
        ],
    };
    let result = consequences::preview(&world, &plan, 1_000_000).unwrap();
    assert_eq!(result["validation"]["valid"], true, "{result}");
    assert_eq!(result["canApply"], false);
    assert_eq!(world.records[ANA].list("threads").unwrap().len(), 1);
}

#[test]
fn common_and_world_shapes_reject_misspellings_tags_duplicates_and_invalid_enums() {
    let world = fixture();
    for patch in [
        "kind: world\nstatus: retired",
        "kind: world\ndomain: null",
        "kind: world\ncapabilities: [unknown-v1]",
        "kind: world\ncapabilities: [route-v1, spatial-core-v1]",
        "kind: world\ncapabilities: [lore-core-v1, lore-core-v1]",
        "kind: world\ntimelines: []",
        "kind: world\ntimelines: [{id: main, label: A}, {id: main, label: B}]",
        "kind: world\ntimelines: [{id: main, label: A, origin: {tick: true, label: Start}}]",
        "kind: world\ntimelines: [{id: main, label: A, origin: {tick: 0, label: Start, invented: true}}]",
        "kind: world\nthreads: [{id: not-a-thread, label: Name}]",
        "kind: world\nrelationship_metrics: {trust: {default: '0'}}",
        "kind: world\nrelationship_metrics: {trust: {unknown: 0}}",
        "kind: world\nrelationship_metrics: {trust: {default: .inf}}",
        "kind: world\naudience: [true]",
        "kind: world\nsection_audiences: {' Notes ': [author], notes: [public]}",
        "kind: world\nsection_audiences: {Public: [participants]}",
    ] {
        assert!(
            record_edit::update(&world, WORLD, patch).is_err(),
            "{patch}"
        );
    }
    for patch in [
        "kind: character\nstatus: draft",
        "kind: character\nthreads: [not-a-thread]",
        "kind: character\nsection_audiences: {Notes: [participants]}",
        "kind: character\naudience: [!tagged author]",
    ] {
        assert!(record_edit::update(&world, ANA, patch).is_err(), "{patch}");
    }
}

#[test]
fn section_audience_character_references_resolve_only_in_the_complete_candidate() {
    let world = fixture();
    let unknown = "char_00000000000000000000000099";
    let patch = format!("kind: character\nsection_audiences: {{Notes: [{unknown}]}}\n");
    let result = authoring::preview_record_update(&world, ANA, &patch, &world.fingerprint).unwrap();
    assert_eq!(result["validation"]["valid"], false);
    let plan = consequences::Plan {
        expected_fingerprint: world.fingerprint.clone(),
        analysis: None,
        operations: vec![
            consequences::Operation::Update {
                entity_id: ANA.into(),
                patch,
            },
            consequences::Operation::Create {
                draft: format!("kind: character\nid: {unknown}\ntitle: New reader\n"),
            },
        ],
    };
    let result = consequences::preview(&world, &plan, 1_000_000).unwrap();
    assert_eq!(result["validation"]["valid"], true, "{result}");
}

#[test]
fn section_participants_token_is_available_only_for_scene_and_conversation_kinds() {
    let world = fixture();
    for (kind, id, time) in [
        (
            "scene",
            "scene_00000000000000000000000001",
            "{start: {tick: 0}, current: {tick: 0}}",
        ),
        (
            "conversation",
            "conv_00000000000000000000000001",
            "{start: {tick: 0}}",
        ),
    ] {
        let draft = format!(
            "kind: {kind}\nid: {id}\ntitle: Notes\nlocation: {PLACE}\ntime: {time}\nsection_audiences: {{Notes: [participants, self]}}\n"
        );
        valid(&replaced(
            &world,
            record_edit::create(&world, &draft).unwrap(),
        ));
    }
}

#[test]
fn registry_edits_preserve_authored_metric_values_without_filling_or_clipping_them() {
    let world = fixture();
    let changed = applied(
        &world,
        WORLD,
        "kind: world\nrelationship_metrics: {trust: {min: 0, max: 0.1, default: 99}, new_metric: {default: 1}}\n",
    );
    assert_eq!(changed.records[REL].raw_bytes, world.records[REL].raw_bytes);
    let result =
        runtime::relationships(&changed, ANA, &StoryTime::new("main", 0, 0).unwrap()).unwrap();
    assert_eq!(result["items"][0]["metrics"], json!({"trust":0.5}));
}

#[test]
fn raw_candidate_checks_close_new_core_field_loopholes_without_reinterpreting_literals() {
    let world = fixture();
    for (kind, id, fields) in [
        (
            "world",
            WORLD,
            json!({"capabilities":[],"timelines":[{"id":"main","label":"Main","origin":{"tick":1.0,"label":"Start"}}],"default_timeline":"main","relationship_metrics":{"trust":{}}}),
        ),
        (
            "relationship",
            REL,
            json!({"from":PLACE,"to":BOB,"transitions":[]}),
        ),
        (
            "relationship",
            REL,
            json!({"from":ANA,"to":BOB,"transitions":[{"id":"a","time":{"tick":0},"metrics":{"undeclared":0.5}}]}),
        ),
        ("character", ANA, json!({"audience":true})),
    ] {
        let candidate = replaced(&world, record(kind, id, fields));
        assert!(
            !validation::validate_world(&candidate).valid,
            "{kind}: {id}"
        );
    }
    let literal = applied(
        &world,
        ANA,
        &format!(
            "kind: character\ndomain: '{REL}'\naudience: ['{REL}']\nperspectives: ['{REL}']\n"
        ),
    );
    valid(&literal);
}
