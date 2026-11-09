//! Authored lore profiles are closed typed data, with exact references and bounds.
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    lore_types::{self, Registry, TypeSpec},
    model::{Record, World},
    source,
};

const HERO: &str = "char_00000000000000000000000001";
const RETIRED: &str = "char_00000000000000000000000002";
const PLACE: &str = "location:harbor";
fn yaml(value: &str) -> Yaml {
    serde_yaml::from_str(value).unwrap()
}
fn record(kind: &str, id: &str, status: &str) -> Record {
    let value = format!(
        "---\nschema: wedl/v1\nkind: {kind}\nid: {id}\ntitle: {id}\nstatus: {status}\n---\n"
    );
    source::parse_record(
        value.as_bytes(),
        &format!("story/{}.md", id.replace(':', "-")),
    )
    .unwrap()
}
fn world() -> World {
    source::world_from_records(
        PathBuf::new(),
        vec![
            record("world", "world_00000000000000000000000000", "canonical"),
            record("character", HERO, "canonical"),
            record("character", RETIRED, "retired"),
            record("location", PLACE, "canonical"),
        ],
        None,
    )
    .unwrap()
}
fn spec(value: &str) -> TypeSpec {
    lore_types::validate_type(&yaml(value)).unwrap()
}
fn profile(fields: &str) -> Registry {
    let fields = yaml(fields);
    let mut value = yaml("[{id: notes, label: Authored notes, fields: {}}]");
    value[0]["fields"] = fields;
    Registry::parse(&value).unwrap()
}

#[test]
fn primitive_types_reject_coercion_and_preserve_all_source_numeric_precision() {
    let world = world();
    for (name, good, bad) in [
        ("string", "prose", "1"),
        ("boolean", "true", "1"),
        ("number", "1.25", "true"),
        ("null", "null", "'null'"),
    ] {
        let schema = spec(&format!("{{type: '{name}'}}"));
        assert!(schema.validate_value(&yaml(good), &world).is_ok());
        assert!(schema.validate_value(&yaml(bad), &world).is_err());
    }
    let number = spec("{type: number}");
    for value in [
        "-9223372036854775808",
        "9007199254740993",
        "18446744073709551615",
        "1e100",
    ] {
        let value = yaml(value);
        let before = value.clone();
        number.validate_value(&value, &world).unwrap();
        assert_eq!(value, before);
    }
    for value in [".nan", ".inf", "-.inf"] {
        assert!(number.validate_value(&yaml(value), &world).is_err());
    }
}

#[test]
fn closed_type_grammar_refuses_defaults_expressions_unknown_forms_and_bad_tokens() {
    for value in [
        "{type: string, default: invented}",
        "{type: number, expression: power}",
        "{$ref: self}",
        "{type: any}",
        "{type: integer}",
        "{type: object, required: {}, optional: {}, wildcard: true}",
        "{type: object, required: {}, optional: null}",
        "{type: object, required: {bad-key: {type: string}}, optional: {}}",
        "{type: object, required: {x: {type: string}}, optional: {x: {type: number}}}",
        "{type: entity, kinds: []}",
        "{type: entity, kinds: [hypothesis]}",
        "{type: entity, kinds: [character, character]}",
        "{type: entity, kinds: [unknown]}",
        "{type: entity, kinds: [true]}",
        "{type: string, extra: null}",
        "!type {type: string}",
        "{type: enum, values: []}",
        "{type: enum, values: [[1]]}",
    ] {
        assert!(lore_types::validate_type(&yaml(value)).is_err(), "{value}");
    }
    for name in ["a", "field_1", &format!("a{}", "b".repeat(63))] {
        assert!(
            lore_types::validate_type(&yaml(&format!(
                "{{type: object, required: {{{name}: {{type: string}}}}, optional: {{}}}}"
            )))
            .is_ok()
        );
    }
}

#[test]
fn optional_fields_remain_absent_and_null_needs_an_explicit_type() {
    let world = world();
    let registry = profile(
        "{type: object, required: {note: {type: string}}, optional: {count: {type: number}, root: {type: 'null'}}}",
    );
    let profile = registry.get("notes").unwrap();
    let value = yaml("{note: ''}");
    let before = value.clone();
    profile.validate_value(&value, &world).unwrap();
    assert_eq!(value, before);
    for bad in [
        "{}",
        "{note: 1}",
        "{note: '', count: null}",
        "{note: '', root: false}",
        "{note: '', extra: value}",
    ] {
        assert!(profile.validate_value(&yaml(bad), &world).is_err(), "{bad}");
    }
    assert!(
        profile
            .validate_value(&yaml("{note: '', count: 0, root: null}"), &world)
            .is_ok()
    );
}

#[test]
fn array_limits_are_explicit_inclusive_and_allow_zero() {
    let world = world();
    let schema = spec("{type: array, items: {type: boolean}, max_items: 2}");
    for good in ["[]", "[true]", "[true, false]"] {
        assert!(schema.validate_value(&yaml(good), &world).is_ok());
    }
    for bad in ["[true, false, true]", "[true, 0]", "{}", "null"] {
        assert!(schema.validate_value(&yaml(bad), &world).is_err());
    }
    let empty = spec("{type: array, items: {type: string}, max_items: 0}");
    assert!(empty.validate_value(&yaml("[]"), &world).is_ok());
    assert!(empty.validate_value(&yaml("[x]"), &world).is_err());
    for limit in ["true", "-1", "1.0", "1025", "null"] {
        assert!(
            lore_types::validate_type(&yaml(&format!(
                "{{type: array, items: {{type: string}}, max_items: {limit}}}"
            )))
            .is_err()
        );
    }
}

#[test]
fn enums_distinguish_json_categories_but_share_exact_numeric_values() {
    let world = world();
    let schema = spec("{type: enum, values: [true, 1, '1', null]}");
    for good in ["true", "1", "1.0", "'1'", "null"] {
        schema.validate_value(&yaml(good), &world).unwrap();
    }
    for bad in ["false", "0", "[]", "{}"][..].iter() {
        assert!(schema.validate_value(&yaml(bad), &world).is_err());
    }
    for values in [
        "[1, 1.0]",
        "[true, true]",
        "[0, -0.0]",
        "[.nan]",
        "[.inf]",
        "[{x: 1}]",
    ] {
        assert!(
            lore_types::validate_type(&yaml(&format!("{{type: enum, values: {values}}}"))).is_err(),
            "{values}"
        );
    }
    let negative_zero = spec("{type: enum, values: [-0.0]}");
    negative_zero.validate_value(&yaml("0"), &world).unwrap();
}

#[test]
fn enum_numeric_comparison_never_rounds_large_integer_alternatives() {
    let world = world();
    let schema =
        spec("{type: enum, values: [9007199254740992, 9007199254740993, 18446744073709551615]}");
    for value in [
        "9007199254740992",
        "9007199254740993",
        "18446744073709551615",
    ] {
        schema.validate_value(&yaml(value), &world).unwrap();
    }
    assert!(
        schema
            .validate_value(&yaml("18446744073709551616.0"), &world)
            .is_err()
    );
    let exact = spec("{type: enum, values: [9007199254740993]}");
    assert!(
        exact
            .validate_value(&yaml("9007199254740992.0"), &world)
            .is_err()
    );
    assert!(
        exact
            .validate_value(&yaml("9007199254740993.0"), &world)
            .is_err()
    );
    assert!(
        lore_types::validate_type(&yaml(
            "{type: enum, values: [9007199254740992, 9007199254740992.0]}"
        ))
        .is_err()
    );
    let minimum = spec("{type: enum, values: [-9223372036854775808]}");
    minimum
        .validate_value(&yaml("-9223372036854775808.0"), &world)
        .unwrap();
}

#[test]
fn entity_values_require_exact_typed_refs_without_a_status_grant() {
    let world = world();
    let schema = spec("{type: entity, kinds: [character]}");
    for id in [HERO, RETIRED] {
        let value = yaml(&format!("{{kind: record, id: '{id}'}}"));
        schema.validate_value(&value, &world).unwrap();
        assert_eq!(
            schema.references(&value, &world).unwrap(),
            vec![("values.id".into(), id.into())]
        );
    }
    for bad in [
        HERO.to_owned(),
        format!("{{kind: record, id: '{PLACE}'}}"),
        format!("{{kind: character, id: '{HERO}'}}"),
        format!("{{kind: record, id: '{HERO}', extra: true}}"),
        "{kind: record, id: char_00000000000000000000000999}".into(),
    ] {
        assert!(schema.validate_value(&yaml(&bad), &world).is_err(), "{bad}");
    }
}

#[test]
fn only_declared_entity_leaves_are_reference_paths() {
    let world = world();
    let schema = spec(
        "{type: object, required: {literal: {type: string}, tokens: {type: array, items: {type: enum, values: [location:harbor]}, max_items: 2}, refs: {type: array, items: {type: object, required: {target: {type: entity, kinds: [character]}}, optional: {}}, max_items: 2}}, optional: {}}",
    );
    let value = yaml(&format!(
        "{{literal: '{HERO}', tokens: ['{PLACE}'], refs: [{{target: {{kind: record, id: '{HERO}'}}}}, {{target: {{kind: record, id: '{RETIRED}'}}}}]}}"
    ));
    let before = value.clone();
    assert_eq!(
        schema.references(&value, &world).unwrap(),
        vec![
            ("values.refs[0].target.id".into(), HERO.into()),
            ("values.refs[1].target.id".into(), RETIRED.into())
        ]
    );
    assert_eq!(value, before);
}

#[test]
fn profile_declarations_are_closed_unique_and_have_object_roots() {
    for value in [
        "{}",
        "null",
        "[{id: notes, label: Notes, fields: {type: string}}]",
        "[{id: notes, label: Notes, version: 1, fields: {type: object, required: {}, optional: {}}}]",
        "[{id: Notes, label: Notes, fields: {type: object, required: {}, optional: {}}}]",
        "[{id: bad_key, label: Notes, fields: {type: object, required: {}, optional: {}}}]",
        "[{id: notes, label: ' ', fields: {type: object, required: {}, optional: {}}}]",
        "[{id: notes, label: Notes, fields: {type: object, required: {}, optional: {}}}, {id: notes, label: Changed, fields: {type: object, required: {}, optional: {}}}]",
    ] {
        assert!(Registry::parse(&yaml(value)).is_err(), "{value}");
    }
    assert!(Registry::parse(&yaml("[]")).unwrap().profiles.is_empty());
    let mut value =
        yaml("[{id: notes, label: Notes, fields: {type: object, required: {}, optional: {}}}]");
    value[0]["label"] = Yaml::String("æ".repeat(256));
    assert!(Registry::parse(&value).is_ok());
    value[0]["label"] = Yaml::String("æ".repeat(257));
    assert!(Registry::parse(&value).is_err());
}

#[test]
fn profile_definitions_change_directly_and_dependents_are_revalidated() {
    let world = world();
    let before = profile("{type: object, required: {note: {type: string}}, optional: {}}");
    let after = profile("{type: object, required: {note: {type: number}}, optional: {}}");
    before
        .get("notes")
        .unwrap()
        .validate_value(&yaml("{note: text}"), &world)
        .unwrap();
    assert!(
        after
            .get("notes")
            .unwrap()
            .validate_value(&yaml("{note: text}"), &world)
            .is_err()
    );
    after
        .get("notes")
        .unwrap()
        .validate_value(&yaml("{note: 2}"), &world)
        .unwrap();
    assert!(after.get("absent").is_none());
    assert_eq!(
        after.get("notes").unwrap().declaration["id"],
        Yaml::String("notes".into())
    );
}

#[test]
fn bounded_data_counts_expanded_nodes_depth_keys_and_array_items_exactly() {
    // 1 root + 15*(1 array + 1024 leaves) + (1 array + 1007 leaves).
    let mut nodes = Yaml::Sequence(vec![Yaml::Sequence(vec![Yaml::Null; 1024]); 15]);
    nodes
        .as_sequence_mut()
        .unwrap()
        .push(Yaml::Sequence(vec![Yaml::Null; 1007]));
    lore_types::bounded(&nodes, "test").unwrap();
    nodes
        .as_sequence_mut()
        .unwrap()
        .last_mut()
        .unwrap()
        .as_sequence_mut()
        .unwrap()
        .push(Yaml::Null);
    assert!(lore_types::bounded(&nodes, "test").is_err());
    let mut deep = Yaml::Null;
    for _ in 0..15 {
        deep = Yaml::Sequence(vec![deep]);
    }
    lore_types::bounded(&deep, "test").unwrap();
    assert!(lore_types::bounded(&Yaml::Sequence(vec![deep]), "test").is_err());
    lore_types::bounded(&Yaml::Sequence(vec![Yaml::Null; 1024]), "test").unwrap();
    assert!(lore_types::bounded(&Yaml::Sequence(vec![Yaml::Null; 1025]), "test").is_err());
    let mut fields = serde_yaml::Mapping::new();
    for index in 0..256 {
        fields.insert(Yaml::String(index.to_string()), Yaml::Null);
    }
    lore_types::bounded(&Yaml::Mapping(fields.clone()), "test").unwrap();
    fields.insert(Yaml::String("extra".into()), Yaml::Null);
    assert!(lore_types::bounded(&Yaml::Mapping(fields), "test").is_err());
}

#[test]
fn value_key_bound_is_separate_from_required_and_optional_schema_maps() {
    let mut schema = yaml("{type: object, required: {first: {type: string}}, optional: {}}");
    for index in 0..256 {
        schema["optional"]
            .as_mapping_mut()
            .unwrap()
            .insert(Yaml::String(format!("f{index}")), yaml("{type: string}"));
    }
    let spec = lore_types::validate_type(&schema).unwrap();
    let world = world();
    let mut value = yaml("{first: value}");
    for index in 0..255 {
        value.as_mapping_mut().unwrap().insert(
            Yaml::String(format!("f{index}")),
            Yaml::String("value".into()),
        );
    }
    spec.validate_value(&value, &world).unwrap();
    value
        .as_mapping_mut()
        .unwrap()
        .insert(Yaml::String("f255".into()), Yaml::String("value".into()));
    assert!(spec.validate_value(&value, &world).is_err());
    schema["optional"]
        .as_mapping_mut()
        .unwrap()
        .insert(Yaml::String("extra".into()), yaml("{type: string}"));
    assert!(lore_types::validate_type(&schema).is_err());
}

#[test]
fn whole_registry_shares_the_node_and_collection_budgets() {
    let mut profiles = Yaml::Sequence(Vec::new());
    for index in 0..1024 {
        profiles.as_sequence_mut().unwrap().push(yaml(&format!("{{id: p-{index}, label: Notes, fields: {{type: object, required: {{}}, optional: {{}}}}}}")));
    }
    Registry::parse(&profiles).unwrap();
    let extra = profiles[0].clone();
    profiles.as_sequence_mut().unwrap().push(extra);
    assert!(Registry::parse(&profiles).is_err());
    profiles.as_sequence_mut().unwrap().pop();
    for profile in profiles.as_sequence_mut().unwrap() {
        profile["fields"]["required"]["note"] = yaml("{type: string}");
    }
    assert!(Registry::parse(&profiles).is_err());
}

#[test]
fn tagged_values_nonstring_keys_and_nonfinite_values_fail_before_typed_processing() {
    for value in [
        "!tag null",
        "{1: text}",
        "{nested: !tag value}",
        "[.nan]",
        "{n: .inf}",
    ] {
        assert!(
            lore_types::bounded(&yaml(value), "test").is_err(),
            "{value}"
        );
    }
    let world = world();
    let schema = spec(
        "{type: object, required: {items: {type: array, items: {type: 'null'}, max_items: 1024}}, optional: {}}",
    );
    let value = yaml("{items: [null, null]}");
    assert!(schema.validate_value(&value, &world).is_ok());
    assert!(
        schema
            .validate_value(&yaml("{items: [null, !tag null]}"), &world)
            .is_err()
    );
}
