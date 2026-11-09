//! Captured requirements compare admitted literals without authorizing outcomes.
#[path = "support/family_character.rs"]
mod fixture;
use fixture::*;
use serde_json::{Value as Json, json};
use std::fs;
use wedl::{
    action_requirements::{self, Requirement, Selector},
    model::World,
    writing_frames::{self, FamilySelection, Frame, Lens},
};

fn frame(world: &World, lens: Lens) -> Frame {
    Frame {
        fingerprint: world.fingerprint.clone(),
        work: None,
        scene_id: None,
        pov_id: Some(VIEWER.into()),
        at: at(6, 0),
        lens,
    }
}

fn families() -> FamilySelection {
    writing_frames::decode_families(
        &json!({"magic":{
            "author":{"rootId":id("concept",10),"selection":magic_selection()},
            "beliefTargets":[id("assertion",2)]
        }})
        .to_string(),
    )
    .unwrap()
}

fn field(expected: &str, selected_field: &str) -> Requirement {
    Requirement {
        target_id: id("concept", 10),
        selector: Selector::Field {
            field: selected_field.into(),
        },
        expected_value_raw: expected.into(),
    }
}

fn assertion_requirement(n: usize, expected: &str) -> Requirement {
    Requirement {
        target_id: id("concept", 10),
        selector: Selector::Assertion {
            assertion_id: id("assertion", n),
        },
        expected_value_raw: expected.into(),
    }
}

fn compare(world: &World, families: &FamilySelection, requirement: &Requirement) -> Json {
    action_requirements::compare(
        world,
        &frame(world, Lens::Author),
        families,
        requirement,
        1_000_000,
    )
    .unwrap()
}

fn select_field(name: &str) -> FamilySelection {
    let mut families = families();
    families
        .magic
        .as_mut()
        .unwrap()
        .author
        .as_mut()
        .unwrap()
        .selection
        .restrictions
        .insert("practice".into(), name.into());
    families
}

#[test]
fn capture_and_compare_are_explicit_bound_read_only_operations() {
    let directory = tempfile::tempdir().unwrap();
    let mut world = base();
    world.root = directory.path().into();
    valid(&world);
    let source = world
        .records
        .values()
        .map(|record| (&record.id, &record.raw_bytes))
        .collect::<Vec<_>>();
    let captured =
        action_requirements::capture(&world, &frame(&world, Lens::Author), &families(), 1_000_000)
            .unwrap();
    assert_eq!(captured["protocol"], wedl::CONTRACT);
    assert_eq!(captured["scope"], "action-requirement-capture");
    assert_eq!(captured["complete"], true);
    assert_eq!(captured["context"]["linkageComplete"], true);
    assert_eq!(captured["captureSha256"].as_str().unwrap().len(), 64);
    assert_eq!(
        captured,
        action_requirements::capture(&world, &frame(&world, Lens::Author), &families(), 1_000_000)
            .unwrap()
    );
    let matches = compare(
        &world,
        &families(),
        &field("\"Current canonical practice\"", "restrictions"),
    );
    assert_eq!(matches["outcome"], "matches");
    assert_eq!(matches["captureSha256"], captured["captureSha256"]);
    assert_eq!(matches["actualRaw"], "\"Current canonical practice\"");
    assert_eq!(matches["citations"][0]["entityId"], id("concept", 10));
    assert_eq!(matches["citations"][0]["section"], "values.restrictions");
    assert_eq!(
        compare(
            &world,
            &families(),
            &field("\"A different requirement\"", "restrictions")
        )["outcome"],
        "differs"
    );
    assert_eq!(
        source,
        world
            .records
            .values()
            .map(|record| (&record.id, &record.raw_bytes))
            .collect::<Vec<_>>()
    );
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    for forbidden in [
        "approval",
        "approved",
        "applied",
        "sufficient",
        "outcomePlan",
    ] {
        assert!(captured.get(forbidden).is_none());
    }
}

#[test]
fn capture_refuses_incomplete_selected_linkage_and_unrelated_families() {
    let world = base();
    let mut incomplete = families();
    incomplete
        .magic
        .as_mut()
        .unwrap()
        .author
        .as_mut()
        .unwrap()
        .selection
        .links
        .insert("practice".into(), vec!["absent".into()]);
    let author = incomplete.magic.as_ref().unwrap().author.as_ref().unwrap();
    assert_eq!(
        wedl::magic::compose(
            &world,
            &author.root_id,
            &author.selection,
            Some(&at(6, 0)),
            "author",
            "author"
        )
        .unwrap()["linkageComplete"],
        false
    );
    assert!(
        action_requirements::capture(&world, &frame(&world, Lens::Author), &incomplete, 1_000_000)
            .is_err()
    );
    let unrelated = writing_frames::decode_families(&json!({"magic":{"author":{"rootId":id("concept",10),"selection":magic_selection()}},"environment":{"contexts":[id("concept",1)]}}).to_string()).unwrap();
    assert!(
        action_requirements::capture(&world, &frame(&world, Lens::Author), &unrelated, 1_000_000)
            .is_err()
    );
    assert!(
        action_requirements::capture(
            &world,
            &frame(&world, Lens::Author),
            &FamilySelection::default(),
            1_000_000
        )
        .is_err()
    );
}

#[test]
fn source_frame_and_selection_changes_require_fresh_capture() {
    let world = base();
    let selected = frame(&world, Lens::Author);
    let original = action_requirements::capture(&world, &selected, &families(), 1_000_000).unwrap();
    let changed = set(
        &world,
        &id("concept", 10),
        "values",
        json!({"restrictions":"Changed","links":[],"exceptions":[]}),
    );
    assert!(action_requirements::capture(&changed, &selected, &families(), 1_000_000).is_err());
    let fresh = action_requirements::capture(
        &changed,
        &frame(&changed, Lens::Author),
        &families(),
        1_000_000,
    )
    .unwrap();
    assert_ne!(fresh["captureSha256"], original["captureSha256"]);
    assert_eq!(
        compare(
            &changed,
            &families(),
            &field("\"Current canonical practice\"", "restrictions")
        )["outcome"],
        "differs"
    );
    let other_selection = select_field("exceptions");
    assert_ne!(
        action_requirements::capture(&world, &selected, &other_selection, 1_000_000).unwrap()["captureSha256"],
        original["captureSha256"]
    );
    let mut malformed = selected;
    malformed.at.timeline = "absent".into();
    assert!(action_requirements::capture(&world, &malformed, &families(), 1_000_000).is_err());
}

#[test]
fn exact_large_horizons_and_complete_output_budgets_are_preserved() {
    let world = base();
    let mut selected = frame(&world, Lens::Author);
    selected.at = at(9_007_199_254_740_993, 0);
    let captured = action_requirements::capture(&world, &selected, &families(), 1_000_000).unwrap();
    assert_eq!(captured["frame"]["at"]["tick"], "9007199254740993");
    for limit in [0, 1, 1_000_001] {
        assert!(action_requirements::capture(&world, &selected, &families(), limit).is_err());
    }
    let oversized = set(
        &world,
        &id("concept", 10),
        "values",
        json!({"restrictions":"x".repeat(270_000),"links":[],"exceptions":[]}),
    );
    valid(&oversized);
    assert!(
        action_requirements::capture(
            &oversized,
            &frame(&oversized, Lens::Author),
            &families(),
            1_000_000
        )
        .is_err()
    );
}

#[test]
fn field_operands_distinguish_missing_null_empty_arrays_and_ambiguous_selection() {
    let world = base();
    assert_eq!(
        compare(&world, &select_field("nothing"), &field("null", "nothing"))["outcome"],
        "unknown"
    );
    let with_null = edit(&world, &id("concept", 10), |data| {
        data["values"]["nothing"] = serde_yaml::Value::Null
    });
    assert_eq!(
        compare(
            &with_null,
            &select_field("nothing"),
            &field("null", "nothing")
        )["outcome"],
        "matches"
    );
    assert_eq!(
        compare(
            &with_null,
            &select_field("nothing"),
            &field("[]", "nothing")
        )["outcome"],
        "differs"
    );
    assert_eq!(
        compare(&world, &families(), &field("[]", "exceptions"))["outcome"],
        "matches"
    );
    assert_eq!(
        compare(&world, &families(), &field("null", "exceptions"))["outcome"],
        "differs"
    );
    // Both roles selecting the same root name do not create an unambiguous operand.
    let ambiguous = select_field("exceptions");
    assert_eq!(
        compare(&world, &ambiguous, &field("[]", "exceptions"))["outcome"],
        "unknown"
    );
    let mut unknown_target = field("null", "restrictions");
    unknown_target.target_id = id("concept", 999);
    let result = compare(&world, &families(), &unknown_target);
    assert_eq!(result["outcome"], "unknown");
    assert!(result["actualRaw"].is_null());
    assert_eq!(result["citations"], json!([]));
}

#[test]
fn literal_object_key_order_is_irrelevant_and_array_order_is_significant() {
    let world = edit(&base(), &id("concept", 10), |data| {
        data["values"]["literal"] =
            serde_yaml::to_value(json!({"kind":"record","id":"not-an-entity-grant"})).unwrap();
        data["values"]["exceptions"] = serde_yaml::to_value(json!(["first", "second"])).unwrap();
    });
    valid(&world);
    assert_eq!(
        compare(
            &world,
            &select_field("literal"),
            &field(
                "{\"id\":\"not-an-entity-grant\",\"kind\":\"record\"}",
                "literal"
            )
        )["outcome"],
        "matches"
    );
    assert_eq!(
        compare(
            &world,
            &families(),
            &field("[\"first\",\"second\"]", "exceptions")
        )["outcome"],
        "matches"
    );
    assert_eq!(
        compare(
            &world,
            &families(),
            &field("[\"second\",\"first\"]", "exceptions")
        )["outcome"],
        "differs"
    );
}

#[test]
fn exact_decimal_comparison_preserves_large_integers_and_scalar_types() {
    let world = edit(&base(), &id("concept", 10), |data| {
        data["values"]["number"] = serde_yaml::to_value(9_007_199_254_740_993u64).unwrap()
    });
    for expected in [
        "9007199254740993",
        "9007199254740993.0",
        "9.007199254740993e15",
    ] {
        assert_eq!(
            compare(&world, &select_field("number"), &field(expected, "number"))["outcome"],
            "matches",
            "{expected}"
        );
    }
    for expected in ["9007199254740992", "\"9007199254740993\"", "true"] {
        assert_eq!(
            compare(&world, &select_field("number"), &field(expected, "number"))["outcome"],
            "differs"
        );
    }
    let one = edit(&world, &id("concept", 10), |data| {
        data["values"]["number"] = serde_yaml::from_str("1.0").unwrap()
    });
    for expected in ["1", "1.0", "1e0", "0.1e1"] {
        assert_eq!(
            compare(&one, &select_field("number"), &field(expected, "number"))["outcome"],
            "matches"
        );
    }
    let zero = edit(&world, &id("concept", 10), |data| {
        data["values"]["number"] = serde_yaml::from_str("-0.0").unwrap()
    });
    assert_eq!(
        compare(&zero, &select_field("number"), &field("0e+3", "number"))["outcome"],
        "matches"
    );
}

#[test]
fn source_float_and_enum_identity_follow_native_used_types() {
    let world = edit(&base(), &id("concept", 10), |data| {
        data["values"]["number"] = serde_yaml::from_str("9007199254740993.0").unwrap();
        data["values"]["choice"] = serde_yaml::to_value("1").unwrap();
    });
    // The existing YAML float codec already represents this as the adjacent
    // binary64 integer. Comparison does not pretend to recover lost raw digits.
    assert_eq!(
        compare(
            &world,
            &select_field("number"),
            &field("9007199254740993", "number")
        )["outcome"],
        "differs"
    );
    assert_eq!(
        compare(&world, &select_field("choice"), &field("\"1\"", "choice"))["outcome"],
        "matches"
    );
    assert_eq!(
        compare(&world, &select_field("choice"), &field("1", "choice"))["outcome"],
        "differs"
    );
}

#[test]
fn assertion_operands_require_one_undisputed_current_author_fact() {
    let world = base();
    assert_eq!(
        compare(
            &world,
            &families(),
            &assertion_requirement(2, "\"CANONICAL_MAGIC\"")
        )["outcome"],
        "matches"
    );
    assert_eq!(
        compare(
            &world,
            &families(),
            &assertion_requirement(2, "\"different\"")
        )["outcome"],
        "differs"
    );
    let other = add(
        &world,
        assertion(3, &id("concept", 10), "A competing explanation"),
    );
    let disputed = set(
        &other,
        &id("assertion", 3),
        "classification",
        json!("unresolved"),
    );
    valid(&disputed);
    for selected in [2, 3] {
        let result = compare(
            &disputed,
            &families(),
            &assertion_requirement(selected, "\"CANONICAL_MAGIC\""),
        );
        assert_eq!(result["outcome"], "unknown");
        assert!(result["actualRaw"].is_null());
        assert_eq!(result["citations"], json!([]));
    }
    let hidden_competitor = set(
        &disputed,
        &id("assertion", 3),
        "audience",
        json!(["hidden"]),
    );
    assert_eq!(
        compare(
            &hidden_competitor,
            &families(),
            &assertion_requirement(2, "\"CANONICAL_MAGIC\"")
        )["outcome"],
        "matches"
    );
}

#[test]
fn native_assertion_corrections_do_not_select_a_disputed_winner() {
    let world = add(
        &base(),
        assertion(3, &id("concept", 10), "Corrected explanation"),
    );
    let corrected = set(
        &world,
        &id("assertion", 3),
        "replaces",
        json!(id("assertion", 2)),
    );
    valid(&corrected);
    assert_eq!(
        compare(
            &corrected,
            &families(),
            &assertion_requirement(2, "\"CANONICAL_MAGIC\"")
        )["outcome"],
        "unknown"
    );
    assert_eq!(
        compare(
            &corrected,
            &families(),
            &assertion_requirement(3, "\"Corrected explanation\"")
        )["outcome"],
        "matches"
    );
    let forked = add(
        &corrected,
        assertion(4, &id("concept", 10), "Another retained explanation"),
    );
    assert_eq!(
        compare(
            &forked,
            &families(),
            &assertion_requirement(3, "\"Corrected explanation\"")
        )["outcome"],
        "unknown"
    );
}

#[test]
fn character_and_dramatic_capture_only_learned_words_and_compare_unknown() {
    let world = base();
    let mut selected = families();
    let author = selected.magic.as_mut().unwrap().author.as_mut().unwrap();
    author.root_id = id("concept", 999);
    author
        .selection
        .profiles
        .insert("practice".into(), "missing-profile".into());
    for lens in [Lens::Character, Lens::DramaticIrony] {
        let captured =
            action_requirements::capture(&world, &frame(&world, lens), &selected, 1_000_000)
                .unwrap();
        assert_eq!(captured["effectivePerspective"], "character");
        assert!(
            captured["context"]
                .to_string()
                .contains("The learned magic explanation")
        );
        no_author_material(&captured["context"]);
        let result = action_requirements::compare(
            &world,
            &frame(&world, lens),
            &selected,
            &assertion_requirement(2, "\"CANONICAL_MAGIC\""),
            1_000_000,
        )
        .unwrap();
        assert_eq!(result["outcome"], "unknown");
        assert_eq!(result["citations"], json!([]));
        assert!(result["actualRaw"].is_null());
    }
}

#[test]
fn character_learning_horizon_and_empty_unknown_streams_remain_native() {
    let world = base();
    let mut selected_frame = frame(&world, Lens::Character);
    for (tick, expected) in [(4, 0), (5, 1), (6, 1), (8, 0), (10, 1)] {
        selected_frame.at = at(tick, 0);
        let result =
            action_requirements::capture(&world, &selected_frame, &families(), 1_000_000).unwrap();
        assert_eq!(result["context"]["count"], expected);
        assert_eq!(result["complete"], true);
    }
    let mut selected = families();
    selected.magic.as_mut().unwrap().belief_targets = vec![id("assertion", 999)];
    assert_eq!(
        action_requirements::capture(&world, &selected_frame, &selected, 1_000_000).unwrap()["context"]
            ["outcome"],
        "unknown"
    );
}

#[test]
fn requirement_json_is_closed_duplicate_safe_and_never_accepts_a_capture() {
    let valid = json!({"targetId":id("concept",10),"selector":{"field":"restrictions"},"expectedValueRaw":"null"});
    assert!(action_requirements::decode_requirement(&valid.to_string()).is_ok());
    for bad in [
        json!({"targetId":id("concept",10),"selector":{"field":"restrictions","assertionId":id("assertion",2)},"expectedValueRaw":"null"}),
        json!({"targetId":id("concept",10),"selector":{},"expectedValueRaw":"null"}),
        json!({"targetId":id("concept",10),"selector":{"field":"restrictions"},"expectedValueRaw":null}),
        json!({"targetId":id("concept",10),"selector":{"field":"restrictions"},"expectedValueRaw":"null","capture":{}}),
        json!({"targetId":"Some title","selector":{"field":"restrictions"},"expectedValueRaw":"null"}),
    ] {
        assert!(action_requirements::decode_requirement(&bad.to_string()).is_err());
    }
    let duplicate = valid.to_string().replacen(
        "\"selector\":",
        "\"targetId\":\"duplicate\",\"selector\":",
        1,
    );
    assert!(action_requirements::decode_requirement(&duplicate).is_err());
    let nested_duplicate = valid.to_string().replace(
        "\"field\":\"restrictions\"",
        "\"field\":\"restrictions\",\"field\":\"exceptions\"",
    );
    assert!(action_requirements::decode_requirement(&nested_duplicate).is_err());
}

#[test]
fn exact_operand_parser_rejects_nonjson_duplicates_and_complete_bounds() {
    let input = |raw: &str| {
        json!({"targetId":id("concept",10),"selector":{"field":"restrictions"},"expectedValueRaw":raw}).to_string()
    };
    for raw in [
        "",
        "01",
        "+1",
        "-",
        "1.",
        "1e",
        "NaN",
        "Infinity",
        "true false",
        "[1,]",
        "{\"x\":1,\"x\":2}",
        "{\"a\":1,\"\\u0061\":2}",
        "\"\\ud800\"",
        "1e1025",
        "1e-1025",
        "0.1e-1024",
    ] {
        assert!(
            action_requirements::decode_requirement(&input(raw)).is_err(),
            "{raw}"
        );
    }
    for raw in [
        "1e1024",
        "1e-1024",
        "0.1e1025",
        "\"é🦀\"",
        "{\"a\":[true,null,1.0]}",
    ] {
        assert!(
            action_requirements::decode_requirement(&input(raw)).is_ok(),
            "{raw}"
        );
    }
    assert!(action_requirements::decode_requirement(&input(&"9".repeat(257))).is_err());
    assert!(
        action_requirements::decode_requirement(&input(&format!("\"{}\"", "x".repeat(16_384))))
            .is_err()
    );
    assert!(
        action_requirements::decode_requirement(&input(&format!(
            "{}0{}",
            "[".repeat(32),
            "]".repeat(32)
        )))
        .is_ok()
    );
    assert!(
        action_requirements::decode_requirement(&input(&format!(
            "{}0{}",
            "[".repeat(33),
            "]".repeat(33)
        )))
        .is_err()
    );
}

#[test]
fn unsupported_actual_operand_is_unknown_without_clipping_capture() {
    let world = set(
        &base(),
        &id("concept", 10),
        "values",
        json!({"restrictions":"x".repeat(17_000),"links":[],"exceptions":[]}),
    );
    let captured =
        action_requirements::capture(&world, &frame(&world, Lens::Author), &families(), 1_000_000)
            .unwrap();
    assert_eq!(
        captured["context"]["records"][0]["fields"]["restrictions"]["value"]
            .as_str()
            .unwrap()
            .len(),
        17_000
    );
    let result = compare(
        &world,
        &families(),
        &field("\"short explicit operand\"", "restrictions"),
    );
    assert_eq!(result["outcome"], "unknown");
    assert!(result["actualRaw"].is_null());
    assert_eq!(result["citations"], json!([]));
}

#[test]
fn capture_evidence_cap_refuses_a_complete_oversized_explanation_set() {
    let mut world = base();
    for n in 3..259 {
        world = add(
            &world,
            assertion(n, &id("concept", 10), "Another retained proposition"),
        );
    }
    valid(&world);
    let error =
        action_requirements::capture(&world, &frame(&world, Lens::Author), &families(), 1_000_000)
            .unwrap_err();
    assert!(error.to_string().contains("evidence bounds"), "{error}");
}
