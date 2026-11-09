use std::path::PathBuf;
use wedl::{
    model::World,
    source::{self, SOURCE_SCHEMA},
    validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const HERO: &str = "char_00000000000000000000000001";
const PLACE: &str = "loc_00000000000000000000000002";
const BEFORE: &str = "event_00000000000000000000000003";
const EVENT: &str = "event_00000000000000000000000004";
const KNOWLEDGE: &str = "know_00000000000000000000000005";
const EFFECT: &str = "effect_00000000000000000000000006";

fn world() -> World {
    let definitions=[
        (WORLD,"world","capabilities: []\ndefault_timeline: main\ntimelines: [{id: main, label: Main}, {id: other, label: Other}]\nthreads: []\nstate_keys:\n  character:\n    condition: {type: string}\n".into()),
        (HERO,"character","initial_state: {condition: ready}\npronouns: {subject: they, object: them}\n".into()),
        (PLACE,"location","".into()),
        (BEFORE,"event",format!("time: {{tick: 5}}\nlocation: {PLACE}\nparticipants: [{{character: {HERO}, role: witness}}]\ncauses: []\neffects: []\n")),
        (EVENT,"event",format!("time: {{tick: 10}}\nlocation: {PLACE}\nparticipants: [{{character: {HERO}, role: witness}}]\ncauses: [{BEFORE}]\neffects: [{{id: {EFFECT}, target: {HERO}, key: condition, operation: set, value: tired}}]\n")),
        (KNOWLEDGE,"knowledge",format!("knower: {HERO}\nclaim: {{key: gate.open, statement: The gate is open, truth: 'true', subject: {PLACE}, predicate: is-open, object: {{text: the gate}}}}\ntransitions: [{{id: learned, time: {{tick: 10}}, state: accepted, confidence: 0.8, acquisition: observed, causing_event: {EVENT}, source_entity: {PLACE}}}]\n")),
    ];
    let records=definitions.into_iter().map(|(id,kind,extra)|{
        let markdown=format!("---\nschema: {SOURCE_SCHEMA}\nid: {id}\nkind: {kind}\ntitle: {id}\nstatus: canonical\naliases: []\ntags: []\n{extra}---\nAuthored prose.\n");
        source::parse_record(markdown.as_bytes(),&format!("story/{id}.md")).unwrap()
    }).collect();
    source::world_from_records(PathBuf::new(), records, None).unwrap()
}
fn set(world: &mut World, id: &str, field: &str, yaml: &str) {
    world.records.get_mut(id).unwrap().frontmatter[field] = serde_yaml::from_str(yaml).unwrap();
}
fn source_bytes(record: &wedl::model::Record) -> Vec<u8> {
    format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&record.frontmatter).unwrap(),
        record.body
    )
    .into_bytes()
}
fn report(world: &World) -> validation::ValidationReport {
    // Exercise actual authored bytes, including the source/model identity gate.
    let records = world
        .records
        .values()
        .map(|record| source::parse_record(&source_bytes(record), &record.source_path).unwrap())
        .collect();
    let candidate =
        source::world_from_records(world.root.clone(), records, world.revision.clone()).unwrap();
    validation::validate_world(&candidate)
}
fn valid(world: &World) {
    let report = report(world);
    assert!(
        report.valid,
        "unexpected diagnostics: {:?}",
        report.diagnostics
    );
}
fn invalid(world: &World, field: &str) {
    let report = report(world);
    assert!(!report.valid, "accepted malformed {field}");
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.severity == "error"
                && d.field.as_deref().is_some_and(|f| f.starts_with(field))),
        "no diagnostic for {field}: {:?}",
        report.diagnostics
    );
}

#[test]
fn valid_core_records_and_optional_reference_absence_are_accepted() {
    let mut candidate = world();
    valid(&candidate);
    candidate.records.get_mut(KNOWLEDGE).unwrap().frontmatter["transitions"][0]["causing_event"] =
        serde_yaml::Value::Null;
    candidate.records.get_mut(KNOWLEDGE).unwrap().frontmatter["transitions"][0]["source_entity"] =
        serde_yaml::Value::Null;
    set(&mut candidate, EVENT, "location", "null");
    set(&mut candidate, HERO, "role", "''");
    // One character may have multiple explicitly authored event roles.
    set(
        &mut candidate,
        EVENT,
        "participants",
        &format!("[{{character: {HERO}, role: witness}}, {{character: {HERO}, role: observer}}]"),
    );
    valid(&candidate);
}

#[test]
fn malformed_event_arrays_members_and_reference_kinds_fail() {
    let base = world();
    for (field, value) in [
        ("participants", "null".to_owned()),
        ("participants", "{}".into()),
        ("participants", "[false]".into()),
        ("participants", format!("[{{character: {PLACE}}}]")),
        ("participants", "[{character: 'Friendly nickname'}]".into()),
        ("participants", format!("[{{character: {HERO}, role: []}}]")),
        (
            "participants",
            format!("[{{character: {HERO}, unexpected: true}}]"),
        ),
        ("location", format!("'{HERO}'")),
        ("location", "[place]".into()),
        ("causes", "null".into()),
        ("causes", format!("[{HERO}]")),
        ("causes", format!("[{BEFORE},{BEFORE}]")),
        ("related_story_points", format!("[{HERO}]")),
        ("related_story_points", "{}".into()),
        ("effects", "null".into()),
        ("effects", "{}".into()),
        ("effects", "[false]".into()),
        ("time", "{tick: 10, typo: true}".into()),
        ("time", "{tick: 10, timeline: absent}".into()),
    ] {
        let mut candidate = base.clone();
        set(&mut candidate, EVENT, field, &value);
        invalid(&candidate, field);
    }
}

#[test]
fn effects_are_closed_require_ids_and_preserve_operation_value_rules() {
    let base = world();
    for (key, value) in [
        ("id", "''"),
        ("id", "true"),
        ("operation", "increment"),
        ("key", "' '"),
        ("extra", "1"),
    ] {
        let mut candidate = base.clone();
        candidate.records.get_mut(EVENT).unwrap().frontmatter["effects"][0][key] =
            serde_yaml::from_str(value).unwrap();
        invalid(&candidate, "effects");
    }
    let mut duplicate = base.clone();
    let effect = duplicate.records[EVENT].frontmatter["effects"][0].clone();
    duplicate.records.get_mut(EVENT).unwrap().frontmatter["effects"]
        .as_sequence_mut()
        .unwrap()
        .push(effect);
    invalid(&duplicate, "effects");
    let mut clear = base.clone();
    clear.records.get_mut(EVENT).unwrap().frontmatter["effects"][0]["operation"] =
        serde_yaml::Value::String("clear".into());
    invalid(&clear, "effects");
    clear.records.get_mut(EVENT).unwrap().frontmatter["effects"][0]
        .as_mapping_mut()
        .unwrap()
        .remove("value");
    valid(&clear);
    clear.records.get_mut(EVENT).unwrap().frontmatter["effects"][0]["operation"] =
        serde_yaml::Value::String("set".into());
    invalid(&clear, "effects");
}

#[test]
fn knowledge_claims_are_closed_and_truth_is_a_string_enum() {
    let base = world();
    for claim in [
        "null",
        "[]",
        "{key: x}",
        "{key: '', statement: valid}",
        "{key: x, statement: ' '}",
        "{key: x, statement: valid, truth: true}",
        "{key: x, statement: valid, truth: certain}",
        "{key: x, statement: valid, author_truth_status: 'true'}",
        "{key: x, statement: valid, object: bare-literal}",
        "{key: x, statement: valid, object: {}}",
        "{key: x, statement: valid, object: {text: ' '}}",
        "{key: x, statement: valid, object: {text: value, entity: missing}}",
        "{key: x, statement: valid, predicate: []}",
    ] {
        let mut candidate = base.clone();
        set(&mut candidate, KNOWLEDGE, "claim", claim);
        invalid(&candidate, "claim");
    }
    let mut wrong_knower = base.clone();
    set(&mut wrong_knower, KNOWLEDGE, "knower", PLACE);
    invalid(&wrong_knower, "knower");
    for truth in ["true", "false", "unknown", "contested"] {
        let mut candidate = base.clone();
        candidate.records.get_mut(KNOWLEDGE).unwrap().frontmatter["claim"]["truth"] =
            serde_yaml::Value::String(truth.into());
        valid(&candidate);
    }
}

#[test]
fn knowledge_transitions_require_their_typed_fields_and_finite_confidence() {
    let base = world();
    for transitions in [
        "null",
        "{}",
        "[true]",
        "[{id: learned, time: {tick: 10}, state: accepted}]",
    ] {
        let mut candidate = base.clone();
        set(&mut candidate, KNOWLEDGE, "transitions", transitions);
        invalid(&candidate, "transitions");
    }
    for (key, value) in [
        ("id", "''"),
        ("state", "true"),
        ("state", "canonical"),
        ("acquisition", "[]"),
        ("acquisition", "guessed"),
        ("confidence", "true"),
        ("confidence", "'0.8'"),
        ("confidence", ".nan"),
        ("confidence", ".inf"),
        ("confidence", "-0.1"),
        ("confidence", "1.1"),
        ("time", "{tick: 10, extra: 0}"),
        ("note", "[]"),
        ("unknown", "42"),
    ] {
        let mut candidate = base.clone();
        candidate.records.get_mut(KNOWLEDGE).unwrap().frontmatter["transitions"][0][key] =
            serde_yaml::from_str(value).unwrap();
        invalid(&candidate, "transitions");
    }
    for key in ["id", "time", "state", "confidence", "acquisition"] {
        let mut candidate = base.clone();
        candidate.records.get_mut(KNOWLEDGE).unwrap().frontmatter["transitions"][0]
            .as_mapping_mut()
            .unwrap()
            .remove(key);
        invalid(&candidate, "transitions");
    }
}

#[test]
fn knowledge_transition_ids_are_unique_and_causes_cannot_come_from_the_future() {
    let base = world();
    let mut duplicate = base.clone();
    let mut next = duplicate.records[KNOWLEDGE].frontmatter["transitions"][0].clone();
    next["time"]["tick"] = serde_yaml::from_str("11").unwrap();
    duplicate.records.get_mut(KNOWLEDGE).unwrap().frontmatter["transitions"]
        .as_sequence_mut()
        .unwrap()
        .push(next);
    invalid(&duplicate, "transitions");
    for time in ["{tick: 9}", "{tick: 10, timeline: other}"] {
        let mut candidate = base.clone();
        candidate.records.get_mut(KNOWLEDGE).unwrap().frontmatter["transitions"][0]["time"] =
            serde_yaml::from_str(time).unwrap();
        invalid(&candidate, "transitions");
    }
    let mut draft = base.clone();
    set(&mut draft, EVENT, "status", "draft");
    invalid(&draft, "transitions");
    let mut wrong_kind = base.clone();
    wrong_kind.records.get_mut(KNOWLEDGE).unwrap().frontmatter["transitions"][0]["causing_event"] =
        serde_yaml::Value::String(HERO.into());
    invalid(&wrong_kind, "transitions");
    let mut prior = base.clone();
    prior.records.get_mut(KNOWLEDGE).unwrap().frontmatter["transitions"][0]["causing_event"] =
        serde_yaml::Value::String(BEFORE.into());
    valid(&prior);
}

#[test]
fn common_labels_and_character_pronouns_reject_malformed_values() {
    let base = world();
    for field in ["aliases", "tags"] {
        for value in ["null", "{}", "[true]", "[' ']", "[same, same]"] {
            let mut candidate = base.clone();
            set(&mut candidate, HERO, field, value);
            invalid(&candidate, field);
        }
    }
    for value in [
        "null",
        "[]",
        "{subject: they}",
        "{subject: they, object: []}",
        "{subject: they, object: them, possessive: ''}",
        "{subject: they, object: them, extra: form}",
    ] {
        let mut candidate = base.clone();
        set(&mut candidate, HERO, "pronouns", value);
        invalid(&candidate, "pronouns");
    }
    let mut title = base.clone();
    title.records.get_mut(HERO).unwrap().title = " ".into();
    set(&mut title, HERO, "title", "' '");
    let error =
        source::parse_record(&source_bytes(&title.records[HERO]), "story/hero.md").unwrap_err();
    assert!(
        error
            .to_string()
            .contains("title must be a nonempty string")
    );
    let mut status = base.clone();
    set(&mut status, EVENT, "status", "active");
    invalid(&status, "status");
}

#[test]
fn record_shaped_strings_in_prose_remain_literals() {
    let mut candidate = world();
    let literal = "loc_00000000000000000000000999";
    for key in ["key", "statement", "predicate"] {
        candidate.records.get_mut(KNOWLEDGE).unwrap().frontmatter["claim"][key] =
            serde_yaml::Value::String(literal.into());
    }
    candidate.records.get_mut(KNOWLEDGE).unwrap().frontmatter["claim"]["object"]["text"] =
        serde_yaml::Value::String(literal.into());
    candidate.records.get_mut(KNOWLEDGE).unwrap().frontmatter["transitions"][0]["note"] =
        serde_yaml::Value::String(literal.into());
    candidate.records.get_mut(KNOWLEDGE).unwrap().body = literal.into();
    set(&mut candidate, HERO, "tags", &format!("[{literal}]"));
    valid(&candidate);
    candidate.records.get_mut(KNOWLEDGE).unwrap().frontmatter["claim"]["subject"] =
        serde_yaml::Value::String(literal.into());
    invalid(&candidate, "claim.subject");
}

#[test]
fn supplied_worlds_keep_all_legitimate_core_records() {
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut events = 0;
    let mut knowledge = 0;
    for name in [
        "tideglass",
        "ash_archive",
        "frontiersmen",
        "chronology_conformance",
    ] {
        let world = source::load_world(examples.join(name), None).unwrap();
        events += world.records.values().filter(|r| r.kind == "event").count();
        knowledge += world
            .records
            .values()
            .filter(|r| r.kind == "knowledge")
            .count();
        valid(&world);
    }
    assert_eq!(events, 105);
    assert_eq!(knowledge, 133);
}
