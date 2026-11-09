//! Authored concepts are typed source, with explicit time and presentation scope.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    Error, lore,
    model::{Record, StoryTime, World},
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const BASE: &str = "concept_00000000000000000000000001";
const TARGET: &str = "concept_00000000000000000000000002";
const HIDDEN: &str = "concept_00000000000000000000000003";
const FUTURE: &str = "concept_00000000000000000000000004";
const DRAFT: &str = "concept_00000000000000000000000005";
const RETIRED: &str = "concept_00000000000000000000000006";
const OTHER: &str = "concept_00000000000000000000000007";
const OLD_LOCATION: &str = "location:retired";
const BIG: i64 = 9_007_199_254_740_993;

fn yaml(value: &str) -> Yaml {
    serde_yaml::from_str(value).unwrap()
}
fn record(kind: &str, id: &str, title: &str, fields: &str) -> Record {
    let source = format!(
        "---\nschema: {}\nkind: {kind}\nid: {id}\ntitle: {title}\n{fields}---\nExact authored body\n",
        source::SOURCE_SCHEMA
    );
    source::parse_record(
        source.as_bytes(),
        &format!("story/{}.md", id.replace(':', "-")),
    )
    .unwrap()
}
fn assemble(records: Vec<Record>) -> World {
    source::world_from_records(PathBuf::new(), records, None).unwrap()
}
fn base() -> World {
    let links = format!(
        "concept_links:\n  - {{relation: hidden, concept_id: {HIDDEN}, valid: {{kind: timeless}}}}\n  - {{relation: observes, concept_id: {TARGET}, valid: {{kind: interval, timeline: main, start: {{timeline: main, tick: 2, order: 0}}, end: {{timeline: main, tick: 6, order: 0}}}}}}\n  - {{relation: later, concept_id: {FUTURE}, valid: {{kind: timeless}}}}\n  - {{relation: retired, concept_id: {RETIRED}, valid: {{kind: timeless}}}}\n  - {{relation: elsewhere, concept_id: {OTHER}, valid: {{kind: timeless}}}}\n"
    );
    let profiles = "profiles:\n  - id: plain\n    label: Plain notes\n    fields: {type: object, required: {note: {type: string}}, optional: {}}\n  - id: notes\n    label: Detailed notes\n    fields:\n      type: object\n      required:\n        note: {type: string}\n        amount: {type: number}\n        choice: {type: enum, values: [1, '1', true, null, 9007199254740993]}\n      optional:\n        flag: {type: boolean}\n        empty: {type: 'null'}\n        target: {type: entity, kinds: [lore-concept]}\n        refs: {type: array, items: {type: entity, kinds: [location, lore-concept]}, max_items: 8}\n        unused: {type: enum, values: [UNUSED_SECRET]}\n        nested:\n          type: object\n          required: {reason: {type: string}}\n          optional:\n            target: {type: entity, kinds: [lore-concept]}\n            selection: {type: array, items: {type: enum, values: [2, '2']}, max_items: 2}\n";
    let mut records = vec![
        record(
            "world",
            WORLD,
            "Lore world",
            &format!(
                "capabilities: [lore-core-v1]\ndefault_timeline: main\ntimelines: [{{id: main, label: Main}}, {{id: other, label: Other}}]\nstate_keys: {{}}\n{profiles}{links}"
            ),
        ),
        record(
            "location",
            OLD_LOCATION,
            "Old Landing",
            &format!("status: retired\n{links}"),
        ),
    ];
    let values = format!(
        "{{note: {HIDDEN}, amount: {BIG}, choice: 1, flag: false, empty: null, target: {{kind: record, id: {HIDDEN}}}, refs: [{{kind: record, id: {HIDDEN}}}, {{kind: record, id: '{OLD_LOCATION}'}}, {{kind: record, id: {TARGET}}}], nested: {{reason: {HIDDEN}, target: {{kind: record, id: {HIDDEN}}}}}}}"
    );
    for (id, title, domain, profile, values, valid, audience, status, extra) in [
        (
            BASE,
            "Harbor Custom",
            "story.tradition",
            "notes",
            values.as_str(),
            "{kind: timeless}",
            "author",
            "canonical",
            links.as_str(),
        ),
        (
            TARGET,
            "Lantern Custom",
            "story.ritual",
            "plain",
            "{note: Lanterns}",
            "{kind: timeless}",
            "author",
            "canonical",
            "",
        ),
        (
            HIDDEN,
            "Harbor Custom",
            "story.secret",
            "plain",
            "{note: Hidden}",
            "{kind: timeless}",
            "editor",
            "canonical",
            "",
        ),
        (
            FUTURE,
            "Future Custom",
            "story.future",
            "plain",
            "{note: Future}",
            "{kind: interval, timeline: main, start: {timeline: main, tick: 10, order: 0}, end: {timeline: main, tick: 20, order: 0}}",
            "author",
            "canonical",
            "",
        ),
        (
            DRAFT,
            "Draft Custom",
            "story.draft",
            "plain",
            "{note: Draft}",
            "{kind: timeless}",
            "author",
            "draft",
            "",
        ),
        (
            RETIRED,
            "Retired Custom",
            "story.retired",
            "plain",
            "{note: Retired}",
            "{kind: timeless}",
            "author",
            "retired",
            "",
        ),
        (
            OTHER,
            "Other Custom",
            "story.other",
            "plain",
            "{note: Other timeline}",
            "{kind: interval, timeline: other}",
            "author",
            "canonical",
            "",
        ),
    ] {
        records.push(record("lore-concept", id, title, &format!("domain: {domain}\nprofile: {profile}\nvalues: {values}\nvalid: {valid}\naudience: [{audience}]\nperspectives: [author]\nstatus: {status}\n{extra}")));
    }
    assemble(records)
}
fn edited(world: &World, id: &str, edit: impl FnOnce(&mut Yaml)) -> World {
    let mut records = world.records.clone();
    let record = records.get_mut(id).unwrap();
    edit(&mut record.frontmatter);
    let source = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&record.frontmatter).unwrap(),
        record.body
    );
    *record = source::parse_record(source.as_bytes(), &record.source_path).unwrap();
    assemble(records.into_values().collect())
}
fn at(timeline: &str, tick: i64, order: i32) -> StoryTime {
    StoryTime::new(timeline, tick, order).unwrap()
}
fn ids(value: &Json) -> Vec<&str> {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["conceptId"].as_str().unwrap())
        .collect()
}
fn valid(world: &World) {
    assert!(
        lore::validate(world).is_empty(),
        "{:?}",
        lore::validate(world)
    );
    let report = validation::validate_world(world);
    assert!(report.valid, "{:?}", report.diagnostics);
}
fn invalid(world: &World) {
    assert!(!lore::validate(world).is_empty());
    assert!(lore::profiles(world, 100).is_err());
    assert!(lore::catalog(world, None, "author", "author", None, 100).is_err());
}

#[test]
fn full_profile_catalogue_is_explicit_sorted_and_cites_original_declaration_positions() {
    let world = base();
    valid(&world);
    let result = lore::profiles(&world, 2).unwrap();
    assert_eq!(result["items"][0]["id"], "notes");
    assert_eq!(result["items"][1]["id"], "plain");
    assert_eq!(result["items"][0]["citation"]["section"], "profiles[1]");
    assert_eq!(result["items"][1]["citation"]["section"], "profiles[0]");
    assert_eq!(result["items"][0]["citation"]["entityId"], WORLD);
    assert_eq!(
        result["items"][0]["fields"]["optional"]["unused"]["values"],
        json!(["UNUSED_SECRET"])
    );
    assert!(lore::profiles(&world, 1).is_err());
    assert!(matches!(
        lore::profile(&world, "Detailed notes"),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        lore::profile(&world, "missing"),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn mixed_enum_wire_values_retain_exact_selected_and_declared_scalar_types() {
    let world = base();
    let profile = lore::profile(&world, "notes").unwrap();
    let choice = &profile["fields"]["required"]["choice"];
    assert_eq!(
        choice["values"],
        json!(["1", "1", true, null, BIG.to_string()])
    );
    assert_eq!(
        choice["enumValueTypes"],
        json!(["number", "string", "boolean", "null", "number"])
    );
    assert_eq!(
        profile["fields"]["optional"]["nested"]["optional"]["selection"]["items"]["enumValueTypes"],
        json!(["number", "string"])
    );
    for (source, expected, kind) in [
        ("1", json!("1"), "number"),
        ("'1'", json!("1"), "string"),
        ("true", json!(true), "boolean"),
        ("null", Json::Null, "null"),
        ("9007199254740993", json!(BIG.to_string()), "number"),
    ] {
        let world = edited(&world, BASE, |v| v["values"]["choice"] = yaml(source));
        let result = lore::concept(&world, BASE, None, "author", "author").unwrap();
        assert_eq!(result["values"]["choice"], expected);
        assert_eq!(
            result["valueTypes"]["fields"]["choice"],
            json!({"type":"enum","scalarType":kind})
        );
        assert!(
            !serde_json::to_string(&result["valueTypes"])
                .unwrap()
                .contains("values")
        );
    }
}

#[test]
fn typed_projection_omits_hidden_leaves_and_compacts_arrays_without_rewriting_source_paths() {
    let world = base();
    let result = lore::concept(&world, BASE, Some(&at("main", 5, 0)), "author", "author").unwrap();
    assert!(result["values"].get("target").is_none());
    assert_eq!(result["values"]["note"], HIDDEN);
    assert_eq!(result["values"]["nested"], json!({"reason":HIDDEN}));
    assert_eq!(result["values"]["amount"], BIG.to_string());
    assert_eq!(result["valueTypes"]["fields"]["amount"]["type"], "number");
    assert_eq!(
        result["values"]["refs"],
        json!([{"kind":"record","id":OLD_LOCATION},{"kind":"record","id":TARGET}])
    );
    assert_eq!(
        result["valueTypes"]["fields"]["refs"]["items"],
        json!([{"type":"entity"},{"type":"entity"}])
    );
    assert!(result["valueTypes"]["fields"].get("unused").is_none());
    assert!(
        !serde_json::to_string(&result)
            .unwrap()
            .contains("UNUSED_SECRET")
    );
    assert_eq!(result["references"][0]["sourceField"], "values.refs[1].id");
    assert_eq!(
        result["references"][0]["citation"]["section"],
        "values.refs[1].id"
    );
    assert_eq!(result["references"][0]["entityId"], OLD_LOCATION);
    assert_eq!(result["references"][0]["title"], "Old Landing");
    assert_eq!(result["references"][1]["sourceField"], "values.refs[2].id");
    assert_eq!(
        result["references"][1]["targetCitation"]["entityId"],
        TARGET
    );
    assert_eq!(result["body"], "Exact authored body\n");
}

#[test]
fn nested_array_enum_shapes_follow_surviving_values_only() {
    let world = edited(&base(), BASE, |v| {
        v["values"]["nested"]["selection"] = yaml("[2, '2']");
    });
    let result = lore::concept(&world, BASE, None, "author", "author").unwrap();
    assert_eq!(result["values"]["nested"]["selection"], json!(["2", "2"]));
    assert_eq!(
        result["valueTypes"]["fields"]["nested"]["fields"]["selection"]["items"],
        json!([{"type":"enum","scalarType":"number"},{"type":"enum","scalarType":"string"}])
    );
}

#[test]
fn catalogue_filters_status_time_and_labels_before_the_complete_result_bound() {
    let world = base();
    let result =
        lore::catalog(&world, Some(&at("main", 5, 0)), "author", "author", None, 2).unwrap();
    assert_eq!(ids(&result), [BASE, TARGET]);
    assert_eq!(result["count"], 2);
    assert!(lore::catalog(&world, Some(&at("main", 5, 0)), "author", "author", None, 1).is_err());
    let domain = lore::catalog(
        &world,
        Some(&at("main", 5, 0)),
        "author",
        "author",
        Some("story.tradition"),
        1,
    )
    .unwrap();
    assert_eq!(ids(&domain), [BASE]);
    assert_eq!(
        domain["items"][0]["references"].as_array().unwrap().len(),
        2
    );
    let editor = lore::catalog(&world, None, "editor", "author", None, 1).unwrap();
    assert_eq!(ids(&editor), [HIDDEN]);
    assert_eq!(
        lore::catalog(&world, None, "author", "reader", None, 1).unwrap()["count"],
        0
    );
    for limit in [0, 1001, usize::MAX] {
        assert!(lore::catalog(&world, None, "author", "author", None, limit).is_err());
        assert!(lore::profiles(&world, limit).is_err());
        assert!(lore::links(&world, WORLD, None, "author", "author", limit).is_err());
    }
}

#[test]
fn undated_reads_span_timelines_while_dated_reads_keep_timelines_independent() {
    let world = base();
    let all = lore::catalog(&world, None, "author", "author", None, 4).unwrap();
    assert_eq!(ids(&all), [BASE, TARGET, FUTURE, OTHER]);
    assert_eq!(all["scope"], "author-all-time-lore");
    assert!(all["at"].is_null());
    let other = lore::catalog(
        &world,
        Some(&at("other", i64::MIN, i32::MIN)),
        "author",
        "author",
        None,
        3,
    )
    .unwrap();
    assert_eq!(ids(&other), [BASE, TARGET, OTHER]);
    assert!(
        lore::concept(
            &world,
            OTHER,
            Some(&at("main", 1000, 0)),
            "author",
            "author"
        )
        .is_err()
    );
    assert!(lore::concept(&world, BASE, Some(&at("missing", 0, 0)), "author", "author").is_err());
}

#[test]
fn validity_bounds_are_inclusive_exact_and_compare_tick_before_order() {
    let interval = format!(
        "{{kind: interval, timeline: main, start: {{timeline: main, tick: {BIG}, order: -2147483648}}, end: {{timeline: main, tick: {BIG}, order: 2147483647}}}}"
    );
    let world = edited(&base(), FUTURE, |v| v["valid"] = yaml(&interval));
    for order in [i32::MIN, -1, 0, i32::MAX] {
        let result = lore::concept(
            &world,
            FUTURE,
            Some(&at("main", BIG, order)),
            "author",
            "author",
        )
        .unwrap();
        assert_eq!(result["at"]["tick"], BIG.to_string());
        assert_eq!(result["at"]["order"], order.to_string());
        assert_eq!(result["valid"]["start"]["tick"], BIG.to_string());
        assert_eq!(result["valid"]["start"]["order"], i32::MIN.to_string());
    }
    for point in [
        at("main", BIG - 1, i32::MAX),
        at("main", BIG + 1, i32::MIN),
        at("other", BIG, 0),
    ] {
        assert!(lore::concept(&world, FUTURE, Some(&point), "author", "author").is_err());
    }
}

#[test]
fn each_omitted_interval_bound_is_open_and_null_is_not_an_omitted_bound() {
    let world = base();
    for (interval, admitted, unavailable) in [
        (
            "{kind: interval, timeline: main, start: {timeline: main, tick: 0, order: 0}}",
            at("main", i64::MAX, i32::MAX),
            at("main", 0, -1),
        ),
        (
            "{kind: interval, timeline: main, end: {timeline: main, tick: 0, order: 0}}",
            at("main", i64::MIN, i32::MIN),
            at("main", 0, 1),
        ),
    ] {
        let changed = edited(&world, FUTURE, |v| v["valid"] = yaml(interval));
        assert!(lore::concept(&changed, FUTURE, Some(&admitted), "author", "author").is_ok());
        assert!(lore::concept(&changed, FUTURE, Some(&unavailable), "author", "author").is_err());
    }
    for invalid in [
        "{kind: interval, timeline: main, start: null}",
        "{kind: interval, timeline: main, end: null}",
    ] {
        assert!(lore::validate_valid_request(&yaml(invalid)).is_err());
    }
}

#[test]
fn validity_request_grammar_rejects_missing_defaults_coercion_tags_and_unknown_fields() {
    for value in [
        "null",
        "[]",
        "{}",
        "!valid {kind: timeless}",
        "{kind: !tag timeless}",
        "{kind: timeless, timeline: main}",
        "{kind: timeless, extra: null}",
        "{kind: interval}",
        "{kind: unknown}",
        "{kind: interval, timeline: ''}",
        "{kind: interval, timeline: main, start: {tick: 1, order: 0}}",
        "{kind: interval, timeline: main, start: {timeline: main, tick: 1}}",
        "{kind: interval, timeline: main, start: {timeline: other, tick: 1, order: 0}}",
        "{kind: interval, timeline: main, start: {timeline: main, tick: '1', order: 0}}",
        "{kind: interval, timeline: main, start: {timeline: main, tick: 1.0, order: 0}}",
        "{kind: interval, timeline: main, start: {timeline: main, tick: true, order: 0}}",
        "{kind: interval, timeline: main, start: {timeline: main, tick: 9223372036854775808, order: 0}}",
        "{kind: interval, timeline: main, start: {timeline: main, tick: 1, order: 2147483648}}",
        "{kind: interval, timeline: main, start: {timeline: main, tick: 1, order: '0'}}",
        "{kind: interval, timeline: main, start: {timeline: main, tick: 1, order: 0, extra: null}}",
        "{kind: interval, timeline: main, start: {timeline: main, tick: 1, order: 1}, end: {timeline: main, tick: 1, order: 0}}",
    ] {
        assert!(
            lore::validate_valid_request(&yaml(value)).is_err(),
            "{value}"
        );
    }
    assert!(
        lore::validate_valid_request(&yaml("{kind: interval, timeline: candidate-only}")).is_ok()
    );
    invalid(&edited(&base(), BASE, |v| {
        v["valid"] = yaml("{kind: interval, timeline: candidate-only}")
    }));
}

#[test]
fn visible_selector_resolution_ignores_hidden_future_draft_and_retired_aliases() {
    let mut world = base();
    for id in [BASE, HIDDEN, FUTURE, DRAFT, RETIRED] {
        world = edited(&world, id, |v| v["aliases"] = yaml("[shared]"));
    }
    let point = at("main", 5, 0);
    for reference in [BASE, "Harbor Custom", "SHARED"] {
        assert_eq!(
            lore::concept(&world, reference, Some(&point), "author", "author").unwrap()["conceptId"],
            BASE
        );
    }
    let absent = lore::concept(&world, "missing", Some(&point), "author", "author")
        .unwrap_err()
        .to_string();
    for reference in [HIDDEN, FUTURE, DRAFT, RETIRED, "Hidden custom"] {
        assert_eq!(
            lore::concept(&world, reference, Some(&point), "author", "author")
                .unwrap_err()
                .to_string(),
            absent
        );
    }
    let ambiguous = edited(&world, TARGET, |v| v["aliases"] = yaml("[shared]"));
    assert!(matches!(
        lore::concept(&ambiguous, "shared", Some(&point), "author", "author"),
        Err(Error::Ambiguous(_))
    ));
    assert_eq!(
        lore::concept(&ambiguous, BASE, Some(&point), "author", "author").unwrap()["conceptId"],
        BASE
    );
}

#[test]
fn links_filter_target_scope_and_validity_before_bounds_and_cite_original_positions() {
    let world = base();
    let point = at("main", 5, 0);
    for owner in [BASE, WORLD, OLD_LOCATION] {
        let result = lore::links(&world, owner, Some(&point), "author", "author", 1).unwrap();
        assert_eq!(ids(&result), [TARGET]);
        assert_eq!(
            result["items"][0]["citation"]["section"],
            "concept_links[1]"
        );
        assert_eq!(result["items"][0]["citation"]["entityId"], owner);
        assert_eq!(result["items"][0]["title"], "Lantern Custom");
        assert_eq!(result["items"][0]["targetCitation"]["entityId"], TARGET);
    }
    assert_eq!(
        lore::links(&world, BASE, Some(&at("main", 6, 0)), "author", "author", 1).unwrap()["count"],
        1
    );
    assert_eq!(
        lore::links(&world, BASE, Some(&at("main", 6, 1)), "author", "author", 1).unwrap()["count"],
        0
    );
    assert_eq!(
        ids(&lore::links(&world, BASE, None, "author", "author", 3).unwrap()),
        [TARGET, FUTURE, OTHER]
    );
    assert!(lore::links(&world, BASE, None, "author", "author", 2).is_err());
    assert!(lore::links(&world, HIDDEN, None, "author", "author", 10).is_err());
}

#[test]
fn link_owner_resolution_also_filters_hidden_aliases_before_ambiguity() {
    let world = base();
    let result = lore::links(
        &world,
        "Harbor Custom",
        Some(&at("main", 5, 0)),
        "author",
        "author",
        1,
    )
    .unwrap();
    assert_eq!(result["ownerId"], BASE);
    assert_eq!(result["ownerKind"], "lore-concept");
    let absent = lore::links(&world, "absent", None, "author", "author", 1)
        .unwrap_err()
        .to_string();
    for owner in [HIDDEN, DRAFT, RETIRED] {
        assert_eq!(
            lore::links(&world, owner, None, "author", "author", 1)
                .unwrap_err()
                .to_string(),
            absent
        );
    }
    let exact_hidden = edited(&world, OLD_LOCATION, |v| {
        v["aliases"] = yaml(&format!("[{HIDDEN}]"));
    });
    assert_eq!(
        lore::links(&exact_hidden, HIDDEN, None, "author", "author", 1)
            .unwrap_err()
            .to_string(),
        absent
    );
    let world = edited(&world, OLD_LOCATION, |v| {
        v["aliases"] = yaml("[Harbor Custom]")
    });
    assert!(matches!(
        lore::links(
            &world,
            "Harbor Custom",
            Some(&at("main", 5, 0)),
            "author",
            "author",
            1
        ),
        Err(Error::Ambiguous(_))
    ));
}

#[test]
fn link_grammar_is_closed_ordered_and_requires_exact_existing_concept_endpoints() {
    let good = format!("[{{relation: observes, concept_id: {TARGET}, valid: {{kind: timeless}}}}]");
    assert!(lore::validate_links_request(&yaml(&good)).is_ok());
    for value in [
        "null".to_owned(),
        "{}".to_owned(),
        "[null]".to_owned(),
        format!("[{{relation: Observes, concept_id: {TARGET}, valid: {{kind: timeless}}}}]"),
        format!("[{{relation: observes_it, concept_id: {TARGET}, valid: {{kind: timeless}}}}]"),
        format!("[{{relation: observes, concept_id: {TARGET}}}]"),
        format!(
            "[{{relation: observes, concept_id: {TARGET}, valid: {{kind: timeless}}, extra: null}}]"
        ),
        "[{relation: observes, concept_id: Lantern Custom, valid: {kind: timeless}}]".to_owned(),
        format!(
            "[{{relation: observes, concept_id: '{OLD_LOCATION}', valid: {{kind: timeless}}}}]"
        ),
        format!("[!tag {{relation: observes, concept_id: {TARGET}, valid: {{kind: timeless}}}}]"),
    ] {
        assert!(
            lore::validate_links_request(&yaml(&value)).is_err(),
            "{value}"
        );
    }
    let absent = "[{relation: observes, concept_id: concept_00000000000000000000000999, valid: {kind: timeless}}]";
    assert!(lore::validate_links_request(&yaml(absent)).is_ok());
    invalid(&edited(&base(), WORLD, |v| {
        v["concept_links"] = yaml(absent)
    }));
    let duplicate = format!(
        "[{{relation: observes, concept_id: {TARGET}, valid: {{kind: timeless}}}}, {{relation: observes, concept_id: {TARGET}, valid: {{kind: timeless}}}}]"
    );
    let world = edited(&base(), WORLD, |v| v["concept_links"] = yaml(&duplicate));
    valid(&world);
    assert_eq!(
        lore::links(&world, WORLD, None, "author", "author", 2).unwrap()["count"],
        2
    );
}

#[test]
fn hypothesis_records_cannot_own_canonical_concept_links_even_when_empty() {
    let mut records: Vec<_> = base().records.into_values().collect();
    records.push(record(
        "hypothesis",
        "hyp_00000000000000000000000001",
        "Maybe",
        "concept_links: []\n",
    ));
    let world = assemble(records);
    assert!(
        lore::validate(&world).iter().any(
            |d| d.field.as_deref() == Some("concept_links") && d.message.contains("hypotheses")
        )
    );
}

#[test]
fn typed_reference_walker_never_infers_edges_from_literal_strings() {
    let world = base();
    let refs = lore::references(&world, &world.records[BASE]).unwrap();
    assert!(refs.contains(&("values.target.id".into(), HIDDEN.into())));
    assert!(refs.contains(&("values.nested.target.id".into(), HIDDEN.into())));
    assert!(refs.contains(&("values.refs[1].id".into(), OLD_LOCATION.into())));
    assert!(refs.contains(&("concept_links[3].concept_id".into(), RETIRED.into())));
    assert!(
        !refs
            .iter()
            .any(|(path, _)| path == "values.note" || path == "values.nested.reason")
    );
    let without_links = edited(&world, BASE, |v| v["concept_links"] = yaml("[]"));
    assert_eq!(
        lore::references(&without_links, &without_links.records[BASE])
            .unwrap()
            .len(),
        5
    );
    for target in [
        "{kind: record, id: concept_00000000000000000000000999}",
        "{kind: record, id: 'location:retired'}",
        "{kind: record, id: Lantern Custom}",
        "{kind: record, id: null}",
        "{kind: record, id: concept_00000000000000000000000002, extra: null}",
    ] {
        invalid(&edited(&world, BASE, |v| {
            v["values"]["target"] = yaml(target)
        }));
    }
}

#[test]
fn every_concept_status_validates_but_only_canonical_is_published() {
    let world = base();
    for status in ["canonical", "draft", "retired"] {
        let changed = edited(&world, BASE, |v| v["status"] = yaml(status));
        valid(&changed);
        assert_eq!(
            lore::concept(&changed, BASE, None, "author", "author").is_ok(),
            status == "canonical"
        );
        invalid(&edited(&changed, BASE, |v| {
            v["values"]["amount"] = yaml("wrong")
        }));
    }
    let default = edited(&world, BASE, |v| {
        v.as_mapping_mut().unwrap().remove("status");
    });
    valid(&default);
    assert!(lore::concept(&default, BASE, None, "author", "author").is_ok());
    for status in ["null", "true", "active", "!tag canonical"] {
        invalid(&edited(&world, BASE, |v| v["status"] = yaml(status)));
    }
}

#[test]
fn concept_required_fields_and_nested_contracts_are_checked_without_rejecting_common_metadata() {
    let world = base();
    for field in [
        "domain",
        "profile",
        "values",
        "valid",
        "audience",
        "perspectives",
    ] {
        invalid(&edited(&world, BASE, |v| {
            v.as_mapping_mut().unwrap().remove(field);
        }));
    }
    for (field, value) in [
        ("domain", "undotted"),
        ("domain", "story..custom"),
        ("domain", "story.bad_id"),
        ("profile", "Detailed notes"),
        ("profile", "undeclared"),
        ("profile", "{id: notes, version: 1}"),
        ("values", "[]"),
        ("provenance", "{}"),
        ("audience", "[]"),
        ("audience", "[author, author]"),
        ("audience", "[editor, author]"),
        ("audience", "[null]"),
        ("perspectives", "[true]"),
        ("perspectives", "!tag [author]"),
    ] {
        invalid(&edited(&world, BASE, |v| v[field] = yaml(value)));
    }
    let changed = edited(&world, BASE, |v| {
        v["extra_metadata"] = yaml("{retained: true}");
        v["provenance"] = yaml("''");
        v["audience"] = yaml("[author, 'editor room']");
        v["perspectives"] = yaml("['reader draft']");
    });
    valid(&changed);
    let result = lore::concept(&changed, BASE, None, "editor room", "reader draft").unwrap();
    assert_eq!(result["provenance"], "");
    assert!(result.get("extra_metadata").is_none());
}

#[test]
fn all_lore_declarations_require_the_capability_including_empty_arrays() {
    let world = base();
    invalid(&edited(&world, WORLD, |v| v["capabilities"] = yaml("[]")));
    for fields in ["profiles: []\n", "concept_links: []\n"] {
        let world = assemble(vec![record(
            "world",
            WORLD,
            "Empty",
            &format!(
                "capabilities: []\ndefault_timeline: main\ntimelines: [{{id: main, label: Main}}]\nstate_keys: {{}}\n{fields}"
            ),
        )]);
        assert!(!lore::validate(&world).is_empty());
    }
    let world = assemble(vec![record(
        "world",
        WORLD,
        "Empty",
        "capabilities: [lore-core-v1]\ndefault_timeline: main\ntimelines: [{id: main, label: Main}]\nstate_keys: {}\nconcept_links: []\n",
    )]);
    valid(&world);
    assert_eq!(lore::profiles(&world, 1).unwrap()["count"], 0);
    assert_eq!(
        lore::catalog(&world, None, "author", "author", None, 1).unwrap()["count"],
        0
    );
    assert_eq!(
        lore::links(&world, WORLD, None, "author", "author", 1).unwrap()["count"],
        0
    );
}

#[test]
fn profiles_belong_to_the_world_and_all_lore_source_is_validated_before_queries() {
    let world = base();
    invalid(&edited(&world, OLD_LOCATION, |v| {
        v["profiles"] = yaml("[]")
    }));
    invalid(&edited(&world, HIDDEN, |v| v["values"] = yaml("{note: 1}")));
    invalid(&edited(&world, RETIRED, |v| {
        v["valid"] = yaml("{kind: timeless, ignored: true}")
    }));
    invalid(&edited(&world, WORLD, |v| {
        v["profiles"][0]["extra"] = yaml("true")
    }));
}

#[test]
fn source_records_and_bytes_remain_unchanged_after_all_author_reads() {
    let world = base();
    let before = world.records.clone();
    lore::profiles(&world, 2).unwrap();
    lore::profile(&world, "notes").unwrap();
    lore::catalog(&world, None, "author", "author", None, 4).unwrap();
    lore::concept(&world, BASE, None, "author", "author").unwrap();
    lore::links(&world, WORLD, None, "author", "author", 3).unwrap();
    for (id, record) in &world.records {
        assert_eq!(record.frontmatter, before[id].frontmatter);
        assert_eq!(record.raw_bytes, before[id].raw_bytes);
        assert_eq!(record.body, before[id].body);
    }
}
