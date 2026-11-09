//! Calendar claims must not masquerade as story-time data. The reserved name
//! applies to structural objects, never arbitrary author-defined keys.
use std::path::PathBuf;

use serde_yaml::Value;
use wedl::{model::World, source, validation};

const OWNER: &str = "world_00000000000000000000000000";
const HERO: &str = "char_00000000000000000000000001";
const OTHER: &str = "char_00000000000000000000000002";
const RELATION: &str = "rel_00000000000000000000000001";
const ENVIRONMENT: &str = "env_00000000000000000000000001";
const EVENT: &str = "event_00000000000000000000000001";
const ORGANIZATION: &str = "organization_00000000000000000000000001";

fn fixture() -> World {
    let definitions = [
        (OWNER, "world", "capabilities: [generational-core-v1]\ndefault_timeline: main\ntimelines: [{id: main, label: Main}]\ncurrent_time: {tick: 0}\nstate_keys: {character: {chronology: {type: boolean}}}\nrelationship_metrics: {chronology: {}}\n".into()),
        (HERO, "character", "initial_state: {chronology: true}\n".into()),
        (OTHER, "character", "".into()),
        (RELATION, "relationship", format!("from: {HERO}\nto: {OTHER}\ntransitions: [{{id: first, time: {{tick: 0}}, metrics: {{chronology: 0.5}}, facets: []}}]\n")),
        (ENVIRONMENT, "environment", format!("time: {{start: {{tick: 0}}}}\ntargets: [{HERO}]\nconditions: {{chronology: unchanged}}\n")),
        (EVENT, "event", format!("time: {{tick: 0}}\nparticipants: [{{character: {HERO}}}]\neffects: [{{id: effect_00000000000000000000000001, target: {HERO}, key: chronology, operation: set, value: false}}]\n")),
        (ORGANIZATION, "organization", "organization_kind: institution\ninitialization: {transition_id: transition_00000000000000000000000071, transition_kind: organization-initialize, applicability: {applicability_kind: instant, point: {timeline: main, tick: 0, order: 0}}, payload: {title: First, aliases: []}}\ntransitions: [{transition_id: transition_00000000000000000000000072, transition_kind: organization-rename, applicability: {applicability_kind: instant, point: {timeline: main, tick: 1, order: 0}}, payload: {title: Next, aliases: []}}]\n".into()),
    ];
    let records = definitions.into_iter().map(|(id, kind, extra): (&str, &str, String)| {
        let raw = format!("---\nschema: wedl/v1\nid: {id}\nkind: {kind}\ntitle: {id}\n{extra}---\nRetained prose.\n");
        source::parse_record(raw.as_bytes(), &format!("story/{id}.md")).unwrap()
    }).collect();
    let world = source::world_from_records(PathBuf::new(), records, None).unwrap();
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    world
}

fn report(world: World) -> validation::ValidationReport {
    let records = world
        .records
        .values()
        .map(|record| {
            let raw = format!(
                "---\n{}---\n{}",
                serde_yaml::to_string(&record.frontmatter).unwrap(),
                record.body
            );
            source::parse_record(raw.as_bytes(), &record.source_path).unwrap()
        })
        .collect();
    validation::validate_world(
        &source::world_from_records(world.root, records, world.revision).unwrap(),
    )
}

fn nested(world: World, owner: &str, field: &str) {
    let report = report(world);
    assert!(!report.valid, "accepted nested operational chronology");
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|item| {
            item.code == "WDL-CHRON-003"
                && item.entity_id.as_deref() == Some(owner)
                && item.field.as_deref() == Some(field)
        })
        .unwrap_or_else(|| panic!("missing exact {field} diagnostic: {:?}", report.diagnostics));
    assert_eq!(diagnostic.severity, "error");
    assert_eq!(
        diagnostic.path.as_deref(),
        Some(format!("story/{owner}.md").as_str())
    );
}

#[test]
fn calendar_claims_inside_story_positions_are_not_silently_ignored() {
    let base = fixture();
    let value = serde_yaml::from_str("[not-a-story-position]").unwrap();
    let mut world = base.clone();
    world.records.get_mut(OWNER).unwrap().frontmatter["current_time"]["chronology"] = value;
    nested(world, OWNER, "current_time.chronology");

    let mut world = base.clone();
    world.records.get_mut(RELATION).unwrap().frontmatter["transitions"][0]["time"]["chronology"] =
        Value::Null;
    nested(world, RELATION, "transitions[0].time.chronology");

    let mut world = base;
    world.records.get_mut(OWNER).unwrap().frontmatter["timelines"][0]["chronology"] =
        Value::Sequence(vec![]);
    nested(world, OWNER, "timelines[0].chronology");
}

#[test]
fn operational_members_and_payloads_cannot_own_date_annotations() {
    let base = fixture();
    let mut world = base.clone();
    world.records.get_mut(RELATION).unwrap().frontmatter["transitions"][0]["chronology"] =
        Value::Sequence(vec![]);
    nested(world, RELATION, "transitions[0].chronology");

    let mut world = base.clone();
    world.records.get_mut(EVENT).unwrap().frontmatter["effects"][0]["chronology"] =
        Value::Sequence(vec![]);
    nested(world, EVENT, "effects[0].chronology");

    let mut world = base;
    world.records.get_mut(ORGANIZATION).unwrap().frontmatter["transitions"][0]["payload"]["chronology"] =
        Value::Sequence(vec![]);
    nested(world, ORGANIZATION, "transitions[0].payload.chronology");
}

#[test]
fn author_defined_names_and_opaque_extensions_are_not_reserved_namespaces() {
    let mut world = fixture();
    let opaque =
        serde_yaml::from_str("{chronology: [!private hidden], 42: {chronology: null}}").unwrap();
    world.records.get_mut(OWNER).unwrap().frontmatter["current_time"]["x-metadata"] = opaque;
    world.records.get_mut(RELATION).unwrap().frontmatter["transitions"][0]["x-metadata"] =
        serde_yaml::from_str("{chronology: {not: a date}}").unwrap();
    world.records.get_mut(HERO).unwrap().frontmatter["x-metadata"] =
        serde_yaml::from_str("{chronology: true}").unwrap();
    let report = report(world);
    assert!(report.valid, "{:?}", report.diagnostics);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|item| item.code == "WDL-CHRON-003")
    );
}
