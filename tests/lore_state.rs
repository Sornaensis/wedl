//! Explicit profile bindings replace leaf contracts without changing author replay.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    Error, lore_state,
    model::{Record, StoryTime, World},
    runtime, source, state_schema, story, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const HERO: &str = "char_00000000000000000000000001";
const OBJECT: &str = "obj_00000000000000000000000001";
const LOCATION: &str = "loc_00000000000000000000000001";
const VISIBLE: &str = "concept_00000000000000000000000001";
const HIDDEN: &str = "concept_00000000000000000000000002";
const FUTURE: &str = "concept_00000000000000000000000003";
const SET: &str = "event_00000000000000000000000001";
const CLEAR: &str = "event_00000000000000000000000002";
const DRAFT: &str = "event_00000000000000000000000003";
const MISSING: &str = "char_00000000000000000000000999";

fn yaml(value: &str) -> Yaml {
    serde_yaml::from_str(value).unwrap()
}
fn record(kind: &str, id: &str, title: &str, fields: &str) -> Record {
    source::parse_record(
        format!("---\nschema: wedl/v1\nkind: {kind}\nid: {id}\ntitle: {title}\n{fields}---\nAuthored prose\n").as_bytes(),
        &format!("story/{id}.md"),
    ).unwrap()
}
fn assemble(records: Vec<Record>) -> World {
    source::world_from_records(PathBuf::from("."), records, None).unwrap()
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn values(note: &str, amount: u64) -> String {
    format!(
        "{{note: '{note}', amount: {amount}, choice: 1, empty: null, entity: {MISSING}, chronology: calendar_missing, refs: [{{kind: record, id: {HIDDEN}}}, {{kind: record, id: {LOCATION}}}, {{kind: record, id: {VISIBLE}}}, {{kind: record, id: {FUTURE}}}]}}"
    )
}
fn base() -> World {
    let profiles = "profiles:\n  - id: notes\n    label: Notes\n    fields:\n      type: object\n      required:\n        note: {type: string}\n        amount: {type: number}\n        choice: {type: enum, values: [1, '1', true, null]}\n      optional:\n        refs: {type: array, items: {type: entity, kinds: [location, lore-concept]}, max_items: 8}\n        empty: {type: 'null'}\n        entity: {type: string}\n        chronology: {type: string}\n        unused: {type: string}\n  - id: literal\n    label: Literal entity field\n    fields: {type: object, required: {entity: {type: string}}, optional: {}}\n  - id: plain\n    label: Concept note\n    fields: {type: object, required: {note: {type: string}}, optional: {}}\n";
    let mut records = vec![
        record(
            "world",
            WORLD,
            "Bound state world",
            &format!(
                "capabilities: [lore-core-v1]\ndefault_timeline: main\ntimelines: [{{id: main, label: Main}}, {{id: other, label: Other}}]\nstate_keys:\n  character:\n    notes: {{type: string, exclusive_group: topic}}\n    other: {{type: string, exclusive_group: topic}}\n    mood: {{type: string}}\n    labels: {{type: string-set}}\n  object:\n    holder: {{type: entity, kind: character}}\nstate_profile_bindings: {{character: {{notes: notes}}, object: {{holder: literal}}}}\n{profiles}"
            ),
        ),
        record(
            "character",
            HERO,
            "Keeper",
            &format!(
                "initial_state: {{notes: {}, mood: calm, labels: [a,b]}}\n",
                values("initial", 9_007_199_254_740_993)
            ),
        ),
        record(
            "object",
            OBJECT,
            "Token",
            &format!("initial_state: {{holder: {{entity: {MISSING}}}}}\n"),
        ),
        record("location", LOCATION, "Old landing", "status: retired\n"),
        record(
            "event",
            SET,
            "Changed notes",
            &format!(
                "time: {{timeline: main, tick: 2, order: 1}}\neffects:\n  - {{id: effect_00000000000000000000000001, target: {HERO}, key: notes, operation: set, value: {}}}\n  - {{id: effect_00000000000000000000000002, target: {HERO}, key: mood, operation: set, value: ready}}\n",
                values("changed", u64::MAX)
            ),
        ),
        record(
            "event",
            CLEAR,
            "Cleared notes",
            &format!(
                "time: {{timeline: main, tick: 4, order: 0}}\neffects: [{{id: effect_00000000000000000000000003, target: {HERO}, key: notes, operation: clear}}]\n"
            ),
        ),
        record(
            "event",
            DRAFT,
            "Draft change",
            &format!(
                "status: draft\ntime: {{timeline: main, tick: 3, order: 0}}\neffects: [{{id: effect_00000000000000000000000004, target: {HERO}, key: notes, operation: set, value: {}}}]\n",
                values("draft", 2)
            ),
        ),
    ];
    for (id, title, audience, valid) in [
        (VISIBLE, "Visible custom", "author", "{kind: timeless}"),
        (HIDDEN, "Keeper", "editor", "{kind: timeless}"),
        (
            FUTURE,
            "Later custom",
            "author",
            "{kind: interval, timeline: main, start: {timeline: main, tick: 5, order: 0}}",
        ),
    ] {
        records.push(record("lore-concept",id,title,&format!("domain: story.custom\nprofile: plain\nvalues: {{note: {title}}}\nvalid: {valid}\naudience: [{audience}]\nperspectives: [author]\n")));
    }
    assemble(records)
}
fn edited(world: &World, id: &str, edit: impl FnOnce(&mut Yaml)) -> World {
    let mut records = world.records.clone();
    let record = records.get_mut(id).unwrap();
    edit(&mut record.frontmatter);
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
fn valid(world: &World) {
    let report = validation::validate_world(world);
    assert!(report.valid, "{:?}", report.diagnostics);
}
fn invalid(world: &World) {
    assert!(!validation::validate_world(world).valid);
}
fn view(world: &World, tick: i64, order: i32) -> Json {
    lore_state::state(world, HERO, &at(tick, order), "author", "author").unwrap()
}

#[test]
fn binding_grammar_requires_existing_declared_keys_and_exact_profile_ids() {
    let world = base();
    valid(&world);
    let bindings = lore_state::Bindings::parse(&world).unwrap();
    assert_eq!(bindings.get("character", "notes").unwrap().id, "notes");
    assert!(bindings.get("character", "mood").is_none());
    for value in [
        "null",
        "[]",
        "{world: {notes: notes}}",
        "{hypothesis: {notes: notes}}",
        "{unknown: {notes: notes}}",
        "{character: []}",
        "{character: {missing: notes}}",
        "{character: {notes: missing}}",
        "{character: {notes: {id: notes}}}",
        "{character: {notes: null}}",
        "{character: {notes: Notes}}",
        "{character: {notes: 1}}",
        "{event: {}}",
    ] {
        let changed = edited(&world, WORLD, |v| v["state_profile_bindings"] = yaml(value));
        assert!(lore_state::Bindings::parse(&changed).is_err(), "{value}");
        invalid(&changed);
    }
    let bad = edited(&world, WORLD, |v| {
        v["state_keys"]["character"]["notes"] = yaml("{type: unknown}")
    });
    invalid(&bad);
}

#[test]
fn binding_request_grammar_leaves_candidate_references_reviewable() {
    assert!(
        lore_state::validate_request(&yaml("{character: {not_yet_declared: future-profile}}"))
            .is_ok()
    );
    assert!(lore_state::validate_request(&yaml("{}")).is_ok());
    for value in [
        "null",
        "[]",
        "{character: {notes: {id: notes}}}",
        "{character: {notes: !profile notes}}",
        "{character: {notes: null}}",
        "{hypothesis: {notes: notes}}",
    ] {
        assert!(
            matches!(
                lore_state::validate_request(&yaml(value)),
                Err(Error::InvalidInput(_))
            ),
            "{value}"
        );
    }
}

#[test]
fn binding_presence_requires_capability_and_is_world_only_even_when_empty() {
    let world = base();
    let no_capability = edited(&world, WORLD, |v| v["capabilities"] = yaml("[]"));
    assert!(lore_state::Bindings::parse(&no_capability).is_err());
    let empty = edited(&no_capability, WORLD, |v| {
        v["state_profile_bindings"] = yaml("{}")
    });
    assert!(lore_state::Bindings::parse(&empty).is_err());
    let absent = edited(&empty, WORLD, |v| {
        v.as_mapping_mut()
            .unwrap()
            .remove(&yaml("state_profile_bindings"));
    });
    assert!(lore_state::Bindings::parse(&absent).is_ok());
    let wrong_owner = edited(&world, HERO, |v| v["state_profile_bindings"] = yaml("{}"));
    assert!(
        lore_state::validate(&wrong_owner)
            .iter()
            .any(|d| d.entity_id.as_deref() == Some(HERO))
    );
    invalid(&wrong_owner);
}

#[test]
fn bound_profiles_override_leaf_types_and_unbound_validation_remains_strict() {
    let world = base();
    valid(&world);
    let definition = state_schema::field_definition(&world, "character", "notes").unwrap();
    assert_eq!(definition.value_type, state_schema::StateType::String);
    assert!(definition.profile.is_some());
    assert!(state_schema::validate_value(&world, &definition, &yaml("plain scalar")).is_err());
    let ordinary = state_schema::field_definition(&world, "character", "mood").unwrap();
    assert!(ordinary.profile.is_none());
    assert!(state_schema::validate_value(&world, &ordinary, &yaml("{note: text}")).is_err());
    let removed = edited(&world, WORLD, |v| v["state_profile_bindings"] = yaml("{}"));
    invalid(&removed);
    assert!(runtime::resolved_state(&removed, HERO, &at(0, 0)).is_err());
}

#[test]
fn authored_set_clear_and_status_replay_preserve_exact_time_and_raw_values() {
    let world = base();
    let before = runtime::resolved_state(&world, HERO, &at(2, 0)).unwrap();
    assert_eq!(before["notes"]["note"], yaml("initial"));
    let during = runtime::resolved_state(&world, HERO, &at(2, 1)).unwrap();
    assert_eq!(during["notes"]["note"], yaml("changed"));
    assert_eq!(during["notes"]["amount"].as_u64(), Some(u64::MAX));
    assert_eq!(
        runtime::resolved_state(&world, HERO, &at(3, 0)).unwrap()["notes"],
        during["notes"]
    );
    assert!(
        !runtime::resolved_state(&world, HERO, &at(4, 0))
            .unwrap()
            .contains_key("notes")
    );
    let other = StoryTime::new("other", 100, 0).unwrap();
    assert_eq!(
        runtime::resolved_state(&world, HERO, &other).unwrap()["notes"]["note"],
        yaml("initial")
    );
}

#[test]
fn bound_collections_refuse_set_item_operations_and_fail_atomically() {
    let world = base();
    let record = &world.records[HERO];
    let mut state = runtime::resolved_state(&world, HERO, &at(0, 0)).unwrap();
    let before = state.clone();
    for operation in ["add-to-set", "remove-from-set"] {
        let effect = yaml(&format!(
            "{{target: {HERO}, key: notes, operation: {operation}, value: {}}}",
            values("new", 1)
        ));
        assert!(state_schema::apply_effect(&world, record, &mut state, &effect).is_err());
        assert_eq!(state, before);
        let changed = edited(&world, SET, |v| {
            v["effects"][0]["operation"] = yaml(operation)
        });
        invalid(&changed);
    }
    let clear_value = yaml(&format!(
        "{{target: {HERO}, key: notes, operation: clear, value: null}}"
    ));
    assert!(state_schema::apply_effect(&world, record, &mut state, &clear_value).is_err());
    assert_eq!(state, before);
}

#[test]
fn profile_changes_check_initial_values_and_draft_event_values() {
    let world = base();
    let changed = edited(&world, WORLD, |v| {
        v["profiles"][0]["fields"]["required"]["additional"] = yaml("{type: boolean}")
    });
    invalid(&changed);
    assert!(runtime::resolved_state(&changed, HERO, &at(0, 0)).is_err());
    let draft = edited(&world, DRAFT, |v| {
        v["effects"][0]["value"]["amount"] = yaml("wrong")
    });
    let report = validation::validate_world(&draft);
    assert!(report.diagnostics.iter().any(|d| d.severity == "error"
        && d.entity_id.as_deref() == Some(DRAFT)
        && d.code.starts_with("WDL-STATE")));
    let missing = edited(&world, WORLD, |v| {
        v["profiles"].as_sequence_mut().unwrap().remove(0);
    });
    assert!(lore_state::Bindings::parse(&missing).is_err());
}

#[test]
fn bindings_preserve_exclusivity_and_clear_means_actual_absence() {
    let world = base();
    let conflict = edited(&world, HERO, |v| {
        v["initial_state"]["other"] = yaml("second topic")
    });
    invalid(&conflict);
    let mut state = runtime::resolved_state(&world, HERO, &at(0, 0)).unwrap();
    let record = &world.records[HERO];
    let other = yaml(&format!(
        "{{target: {HERO}, key: other, operation: set, value: second}}"
    ));
    let before = state.clone();
    assert!(state_schema::apply_effect(&world, record, &mut state, &other).is_err());
    assert_eq!(state, before);
    state_schema::apply_effect(
        &world,
        record,
        &mut state,
        &yaml(&format!("{{target: {HERO}, key: notes, operation: clear}}")),
    )
    .unwrap();
    state_schema::apply_effect(&world, record, &mut state, &other).unwrap();
    assert!(!state.contains_key("notes"));
    assert_eq!(state["other"], yaml("second"));
}

#[test]
fn typed_reference_paths_ignore_id_looking_literals_and_keep_original_indexes() {
    let world = base();
    valid(&world);
    let refs = lore_state::references(&world, &world.records[HERO]).unwrap();
    assert_eq!(refs.len(), 4);
    assert_eq!(
        refs[0],
        ("initial_state.notes.refs[0].id".into(), HIDDEN.into())
    );
    assert_eq!(
        refs[3],
        ("initial_state.notes.refs[3].id".into(), FUTURE.into())
    );
    assert!(!refs.iter().any(|(_, id)| id == MISSING));
    assert!(
        lore_state::references(&world, &world.records[OBJECT])
            .unwrap()
            .is_empty()
    );
    let effects = lore_state::references(&world, &world.records[SET]).unwrap();
    assert_eq!(
        effects[2],
        ("effects[0].value.refs[2].id".into(), VISIBLE.into())
    );
    let bad = edited(&world, HERO, |v| {
        v["initial_state"]["notes"]["refs"][0]["id"] = yaml(MISSING)
    });
    invalid(&bad);
}

#[test]
fn generic_state_and_context_omit_bound_values_and_their_citations() {
    let world = base();
    for point in [at(0, 0), at(2, 1), at(4, 0)] {
        let state = runtime::state(&world, HERO, &point).unwrap();
        assert!(state["state"].get("notes").is_none());
        assert!(state["citations"].get("notes").is_none());
        assert!(state["state"].get("mood").is_some());
        let context = runtime::context(&world, HERO, &point, 100_000).unwrap();
        assert_eq!(context["state"], state);
        assert!(!serde_json::to_string(&context).unwrap().contains(HIDDEN));
    }
    let object = runtime::state(&world, OBJECT, &at(0, 0)).unwrap();
    assert_eq!(object["state"], json!({}));
    assert_eq!(object["citations"], json!({}));
}

#[test]
fn scoped_state_projects_current_refs_precise_types_and_only_bound_values() {
    let world = base();
    let read = view(&world, 1, 0);
    assert_eq!(read["scope"], "author-as-of-lore");
    assert_eq!(read["state"]["notes"]["amount"], "9007199254740993");
    assert_eq!(read["state"]["notes"]["choice"], "1");
    assert_eq!(
        read["valueTypes"]["notes"]["fields"]["choice"],
        json!({"type":"enum","scalarType":"number"})
    );
    assert_eq!(read["profiles"]["notes"], "notes");
    assert!(read["state"].get("mood").is_none());
    assert!(read["state"]["notes"].get("unused").is_none());
    assert_eq!(
        read["state"]["notes"]["refs"],
        json!([{"kind":"record","id":LOCATION},{"kind":"record","id":VISIBLE}])
    );
    assert_eq!(
        read["references"][0]["sourceField"],
        "initial_state.notes.refs[1].id"
    );
    assert_eq!(read["references"][0]["stateKey"], "notes");
    assert_eq!(
        read["references"][1]["sourceField"],
        "initial_state.notes.refs[2].id"
    );
    assert_eq!(read["citations"]["notes"]["section"], "initial_state.notes");
    assert_eq!(read["state"]["notes"]["entity"], MISSING);
    assert!(read["state"]["notes"]["empty"].is_null());
}

#[test]
fn last_write_citations_follow_exact_effect_members_and_retain_explicit_clear() {
    let world = base();
    let read = view(&world, 2, 1);
    assert_eq!(read["state"]["notes"]["amount"], u64::MAX.to_string());
    assert_eq!(read["citations"]["notes"]["entityId"], SET);
    assert_eq!(read["citations"]["notes"]["section"], "effects[0].value");
    assert_eq!(
        read["citations"]["notes"]["effectId"],
        "effect_00000000000000000000000001"
    );
    assert_eq!(
        read["references"][0]["sourceField"],
        "effects[0].value.refs[1].id"
    );
    assert_eq!(read["references"][0]["citation"]["entityId"], SET);
    let cleared = view(&world, 4, 0);
    for field in ["state", "profiles", "valueTypes"] {
        assert_eq!(cleared[field], json!({}));
    }
    assert_eq!(cleared["citations"]["notes"]["entityId"], CLEAR);
    assert_eq!(cleared["citations"]["notes"]["section"], "effects[0]");
    assert_eq!(cleared["references"], json!([]));
}

#[test]
fn scope_and_owner_resolution_precede_alias_ambiguity_and_hidden_reference_projection() {
    let world = base();
    assert_eq!(
        lore_state::state(&world, "Keeper", &at(1, 0), "author", "author").unwrap()["entityId"],
        HERO
    );
    assert!(matches!(
        lore_state::state(&world, "Keeper", &at(1, 0), "editor", "author"),
        Err(Error::Ambiguous(_))
    ));
    let absent = lore_state::state(&world, "missing-owner", &at(1, 0), "author", "author")
        .unwrap_err()
        .json();
    let hidden = lore_state::state(&world, HIDDEN, &at(1, 0), "author", "author")
        .unwrap_err()
        .json();
    assert_eq!(absent, hidden);
    let reader = lore_state::state(&world, HERO, &at(1, 0), "reader", "author").unwrap();
    assert_eq!(
        reader["state"]["notes"]["refs"],
        json!([{"kind":"record","id":LOCATION}])
    );
    assert_eq!(reader["state"]["notes"]["note"], "initial");
    assert!(lore_state::state(&world, HERO, &at(1, 0), "", "author").is_err());
    assert!(
        lore_state::state(
            &world,
            HERO,
            &StoryTime::new("absent", 1, 0).unwrap(),
            "author",
            "author"
        )
        .is_err()
    );
}

#[test]
fn bound_equality_is_structural_and_nested_arrays_keep_their_order() {
    let world = base();
    let world = edited(&world, WORLD, |v| {
        v["state_keys"]["character"]["notes"]["type"] = yaml("string-set")
    });
    valid(&world);
    let definition = state_schema::field_definition(&world, "character", "notes").unwrap();
    let a = yaml(&values("first", 1));
    let mut b = a.clone();
    b["refs"].as_sequence_mut().unwrap().swap(1, 2);
    assert!(!state_schema::values_equal_for_definition(
        &definition,
        &a,
        &b
    ));
    assert!(state_schema::values_equal_for_definition(
        &definition,
        &a,
        &a
    ));
    b = a.clone();
    b["amount"] = yaml("1.0");
    assert!(!state_schema::values_equal_for_definition(
        &definition,
        &a,
        &b
    ));
}

#[test]
fn typed_state_predicates_use_internal_values_without_unmasking_generic_state() {
    let world = base();
    let mut records = world.records.values().cloned().collect::<Vec<_>>();
    records.push(record("story-point","point_00000000000000000000000001","Profile condition",&format!("activation_policy: suggest\nlifecycle: {{initial_state: dormant}}\nwhen: {{state: {{entity: {HERO}, key: notes, equals: {}}}}}\n",values("initial",9_007_199_254_740_993))));
    let world = assemble(records);
    let diagnostics = story::validate(&world);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let points = story::story_points(&world, &at(1, 0)).unwrap();
    assert_eq!(points["items"][0]["eligible"], true);
    let points = story::story_points(&world, &at(2, 1)).unwrap();
    assert_eq!(points["items"][0]["eligible"], false);
    assert!(
        runtime::state(&world, HERO, &at(1, 0)).unwrap()["state"]
            .get("notes")
            .is_none()
    );
}
