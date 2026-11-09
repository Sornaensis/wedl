use serde_yaml::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use wedl::{
    model::{StoryTime, World},
    runtime, source, state_schema as state, validation,
};

const HERO: &str = "char_00000000000000000000000001";
const FRIEND: &str = "char_00000000000000000000000002";
const PLACE: &str = "loc_00000000000000000000000004";
const DEFINITIONS: &str = "name: {type: string}\nalive: {type: boolean}\ncount: {type: integer}\nrate: {type: number}\nfriend: {type: entity, kind: character}\nlabels: {type: string-set}\nfriends: {type: entity-set, kind: character}\ncurrent_room: {type: entity, kind: location, exclusive_group: placement}\ncarried_by: {type: entity, kind: character, exclusive_group: placement}\n";

fn yaml(text: &str) -> Value {
    serde_yaml::from_str(text).unwrap()
}

fn world(definitions: &str, initial: &str, events: &[String]) -> World {
    let definitions = definitions
        .lines()
        .map(|line| format!("    {line}\n"))
        .collect::<String>();
    let sources = [
        format!(
            "schema: wedl/v1\nkind: world\nid: world_00000000000000000000000000\ntitle: Typed state\ncapabilities: []\nthreads: []\ndefault_timeline: main\ntimelines: [{{id: main, label: Main}}]\nstate_keys:\n  character:\n{definitions}"
        ),
        format!(
            "schema: wedl/v1\nkind: character\nid: {HERO}\ntitle: Hero\ninitial_state: {initial}\n"
        ),
        format!("schema: wedl/v1\nkind: character\nid: {FRIEND}\ntitle: Friend\n"),
        "schema: wedl/v1\nkind: object\nid: obj_00000000000000000000000003\ntitle: Token\n".into(),
        format!("schema: wedl/v1\nkind: location\nid: {PLACE}\ntitle: Pier\n"),
    ];
    let records = sources
        .iter()
        .chain(events.iter())
        .enumerate()
        .map(|(index, text)| {
            source::parse_record(
                format!("---\n{text}---\n").as_bytes(),
                &format!("story/{index}.md"),
            )
            .unwrap()
        })
        .collect();
    source::world_from_records(PathBuf::from("."), records, None).unwrap()
}

fn event(number: usize, effects: &[(&str, &str, Option<&str>)]) -> String {
    let mut text = format!(
        "schema: wedl/v1\nkind: event\nid: event_{number:026}\ntitle: Change {number}\ntime: {{tick: {number}}}\neffects:\n"
    );
    for (index, (operation, key, value)) in effects.iter().enumerate() {
        text.push_str(&format!(
            "- {{id: effect_{:026}, target: {HERO}, key: {key}, operation: {operation}",
            number * 100 + index
        ));
        if let Some(value) = value {
            text.push_str(&format!(", value: {value}"));
        }
        text.push_str("}\n");
    }
    text
}

fn at(tick: i64) -> StoryTime {
    StoryTime::new("main", tick, i32::MAX).unwrap()
}

#[test]
fn all_seven_types_work_without_scalar_coercion_and_integer_wire_is_exact() {
    let world = world(
        DEFINITIONS,
        &format!(
            "{{name: Hero, alive: false, count: 9223372036854775807, rate: 1.25, friend: {{entity: {FRIEND}}}, labels: [red, Red], friends: [{{entity: {FRIEND}}}]}}"
        ),
        &[],
    );
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    let raw = runtime::resolved_state(&world, HERO, &at(0)).unwrap();
    assert_eq!(raw["count"].as_i64(), Some(i64::MAX));
    assert_eq!(raw["alive"].as_bool(), Some(false));
    let wire = runtime::state(&world, HERO, &at(0)).unwrap();
    assert_eq!(wire["state"]["count"], i64::MAX.to_string());
    assert_eq!(wire["state"]["rate"], 1.25);
    assert_eq!(wire["state"]["alive"], false);
    assert_eq!(wire["state"]["labels"], serde_json::json!(["red", "Red"]));
}

#[test]
fn declarations_are_closed_and_legacy_spellings_are_errors() {
    for declaration in [
        "string",
        "{}",
        "{type: bool}",
        "{type: int}",
        "{type: float}",
        "{type: entity, entity_kind: character}",
        "{type: entity, exclusive: true}",
        "{type: string, kind: character}",
        "{type: entity, kind: unknown}",
        "{type: string, exclusive_group: ''}",
        "{type: string, exclusive_group: true}",
        "{type: entity, kind: null}",
        "{type: string, default: text}",
        "{type: !tag string}",
    ] {
        assert!(
            state::validate_definition(&yaml(declaration)).is_err(),
            "accepted {declaration}"
        );
    }
    for kind in [
        "string",
        "boolean",
        "integer",
        "number",
        "entity",
        "string-set",
        "entity-set",
    ] {
        assert!(state::validate_definition(&yaml(&format!("{{type: {kind}}}"))).is_ok());
    }
    let world = world("unused: {type: unknown}", "{}", &[]);
    let report = validation::validate_world(&world);
    assert!(!report.valid);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "WDL-STATE-SCHEMA" && d.severity == "error")
    );
}

#[test]
fn invalid_scalar_values_do_not_pass_as_similar_types() {
    let world = world(DEFINITIONS, "{}", &[]);
    for (key, invalid) in [
        ("name", "1"),
        ("name", "null"),
        ("name", "true"),
        ("name", "!tag text"),
        ("alive", "1"),
        ("alive", "'false'"),
        ("alive", "null"),
        ("count", "true"),
        ("count", "1.0"),
        ("count", "'1'"),
        ("count", "9223372036854775808"),
        ("count", "null"),
        ("count", "!tag 1"),
        ("rate", ".nan"),
        ("rate", ".inf"),
        ("rate", "-.inf"),
        ("rate", "true"),
        ("rate", "'1.5'"),
        ("rate", "null"),
    ] {
        let definition = state::field_definition(&world, "character", key).unwrap();
        assert!(
            state::validate_value(&world, &definition, &yaml(invalid)).is_err(),
            "{key} accepted {invalid}"
        );
    }
    let integer = state::field_definition(&world, "character", "count").unwrap();
    for valid in [i64::MIN.to_string(), i64::MAX.to_string(), "0".into()] {
        assert!(state::validate_value(&world, &integer, &yaml(&valid)).is_ok());
    }
    let number = state::field_definition(&world, "character", "rate").unwrap();
    for valid in ["0", "-0.5", "1e100"] {
        assert!(state::validate_value(&world, &number, &yaml(valid)).is_ok());
    }
}

#[test]
fn entities_are_closed_typed_references_and_sets_have_no_duplicates() {
    let world = world(DEFINITIONS, "{}", &[]);
    let entity = state::field_definition(&world, "character", "friend").unwrap();
    for value in [
        format!("'{FRIEND}'"),
        "null".into(),
        format!("{{entity: {PLACE}}}"),
        "{entity: char_00000000000000000000000099}".into(),
        format!("{{entity: {FRIEND}, label: Friend}}"),
        "{entity: true}".into(),
    ] {
        assert!(
            state::validate_value(&world, &entity, &yaml(&value)).is_err(),
            "accepted {value}"
        );
    }
    let entities = state::field_definition(&world, "character", "friends").unwrap();
    assert!(
        state::validate_value(
            &world,
            &entities,
            &yaml(&format!("[{{entity: {FRIEND}}}, {{entity: {FRIEND}}}]"))
        )
        .is_err()
    );
    assert!(
        state::validate_value(&world, &entities, &yaml(&format!("[{{entity: {PLACE}}}]"))).is_err()
    );
    let strings = state::field_definition(&world, "character", "labels").unwrap();
    for value in ["null", "text", "[a, a]", "[a, false]", "[a, 1]"] {
        assert!(state::validate_value(&world, &strings, &yaml(value)).is_err());
    }
    assert!(state::validate_value(&world, &strings, &yaml("[a, A, 'a ']")).is_ok());
}

#[test]
fn replay_supports_whole_set_assignment_and_single_item_operations() {
    let friend = format!("{{entity: {FRIEND}}}");
    let initial = format!("{{labels: [old], friends: [], count: -9223372036854775808}}");
    let events = [
        event(
            1,
            &[
                ("set", "labels", Some("[red, blue]")),
                ("add-to-set", "labels", Some("blue")),
                ("add-to-set", "labels", Some("green")),
                ("remove-from-set", "labels", Some("red")),
                ("add-to-set", "friends", Some(&friend)),
                ("add-to-set", "friends", Some(&friend)),
                ("set", "count", Some("9223372036854775807")),
            ],
        ),
        event(
            2,
            &[
                ("remove-from-set", "friends", Some(&friend)),
                ("clear", "count", None),
            ],
        ),
    ];
    let world = world(DEFINITIONS, &initial, &events);
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    let first = runtime::state(&world, HERO, &at(1)).unwrap();
    assert_eq!(
        first["state"]["labels"],
        serde_json::json!(["blue", "green"])
    );
    assert_eq!(first["state"]["friends"].as_array().unwrap().len(), 1);
    assert_eq!(first["state"]["count"], i64::MAX.to_string());
    let second = runtime::state(&world, HERO, &at(2)).unwrap();
    assert_eq!(second["state"]["friends"], serde_json::json!([]));
    assert!(second["state"].get("count").is_none());
    assert_eq!(
        second["citations"]["count"]["entityId"],
        "event_00000000000000000000000002"
    );
    assert_eq!(
        runtime::state(&world, HERO, &at(0)).unwrap()["state"]["count"],
        i64::MIN.to_string()
    );
}

#[test]
fn invalid_effects_are_errors_even_for_drafts_and_clear_never_ignores_value() {
    for (operation, key, value) in [
        ("add-to-set", "name", Some("coin")),
        ("remove-from-set", "count", Some("1")),
        ("add-to-set", "labels", Some("[coin]")),
        ("add-to-set", "friends", Some("null")),
        ("set", "labels", Some("[same, same]")),
        ("set", "count", Some("false")),
        ("clear", "name", Some("null")),
        ("clear", "name", Some("unused")),
        ("set", "unknown", Some("text")),
        ("set", "name", None),
    ] {
        let event = event(1, &[(operation, key, value)]);
        for raw in [event.clone(), format!("{event}status: draft\n")] {
            let world = world(DEFINITIONS, "{}", &[raw]);
            let report = validation::validate_world(&world);
            assert!(!report.valid, "accepted {operation} {key} {value:?}");
            if operation == "clear" {
                assert!(
                    report
                        .diagnostics
                        .iter()
                        .any(|d| d.code == "WDL-STATE-018" && d.severity == "error")
                );
            }
        }
    }
}

#[test]
fn exclusivity_uses_presence_and_is_checked_after_each_effect() {
    let world = world(
        "first: {type: string-set, exclusive_group: slot}\nsecond: {type: boolean, exclusive_group: slot}",
        "{first: [], second: false}",
        &[],
    );
    assert!(
        validation::validate_world(&world)
            .diagnostics
            .iter()
            .any(|d| d.code == "WDL-STATE-015")
    );
    assert!(runtime::resolved_state(&world, HERO, &at(0)).is_err());
    let initial = format!("{{current_room: {{entity: {PLACE}}}}}");
    let holder = format!("{{entity: {FRIEND}}}");
    let bad = self::world(
        DEFINITIONS,
        &initial,
        &[event(
            1,
            &[
                ("set", "carried_by", Some(&holder)),
                ("clear", "current_room", None),
            ],
        )],
    );
    assert!(!validation::validate_world(&bad).valid);
    let good = self::world(
        DEFINITIONS,
        &initial,
        &[event(
            1,
            &[
                ("clear", "current_room", None),
                ("set", "carried_by", Some(&holder)),
            ],
        )],
    );
    let report = validation::validate_world(&good);
    assert!(report.valid, "{:?}", report.diagnostics);
    let wire = runtime::state(&good, HERO, &at(1)).unwrap();
    assert!(wire["state"].get("current_room").is_none());
    assert_eq!(wire["state"]["carried_by"]["entity"], FRIEND);
    assert!(wire["citations"].get("current_room").is_some());
}

#[test]
fn failed_effect_leaves_the_supplied_state_unchanged() {
    let initial = format!("{{current_room: {{entity: {PLACE}}}}}");
    let world = world(DEFINITIONS, &initial, &[]);
    let record = world.resolve(HERO).unwrap();
    let mut values = state::initial_state(&world, record).unwrap();
    let before = values.clone();
    let effect = yaml(&format!(
        "{{target: {HERO}, key: carried_by, operation: set, value: {{entity: {FRIEND}}}}}"
    ));
    assert!(state::apply_effect(&world, record, &mut values, &effect).is_err());
    assert_eq!(values, before);
}

#[test]
fn typed_equality_preserves_scalar_identity_and_set_membership() {
    assert!(!state::values_equal(&yaml("true"), &yaml("1")));
    assert!(!state::values_equal(&yaml("1"), &yaml("1.0")));
    assert!(!state::values_equal(&yaml("'1'"), &yaml("1")));
    let definition = state::validate_definition(&yaml("{type: string-set}")).unwrap();
    assert!(state::values_equal_for_definition(
        &definition,
        &yaml("[a, b]"),
        &yaml("[b, a]")
    ));
    assert!(!state::values_equal(&yaml("[a, b]"), &yaml("[b, a]")));
    assert!(!state::values_equal_for_definition(
        &definition,
        &yaml("[a, B]"),
        &yaml("[b, a]")
    ));
}

#[test]
fn state_declarations_in_all_examples_pass_strict_checks() {
    for name in [
        "tideglass",
        "ash_archive",
        "frontiersmen",
        "chronology_conformance",
    ] {
        let world = source::load_world(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join(name),
            None,
        )
        .unwrap();
        let report = validation::validate_world(&world);
        assert!(
            report.valid,
            "{name}: {:?}",
            report
                .diagnostics
                .iter()
                .filter(|d| d.severity == "error")
                .collect::<Vec<_>>()
        );
        assert!(
            !report
                .diagnostics
                .iter()
                .any(|d| d.code == "RUST-STATE-TYPE")
        );
    }
}

#[test]
fn missing_declarations_do_not_infer_types() {
    let world = world("name: {type: string}", "{count: 1}", &[]);
    assert!(!validation::validate_world(&world).valid);
    assert!(runtime::resolved_state(&world, HERO, &at(0)).is_err());
    let mut state = BTreeMap::new();
    let effect = yaml(&format!(
        "{{target: {HERO}, key: missing, operation: clear}}"
    ));
    assert!(
        state::apply_effect(&world, world.resolve(HERO).unwrap(), &mut state, &effect).is_err()
    );
    assert!(state.is_empty());
}
