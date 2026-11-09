//! Authored uncertainty is preserved through validation, listing and comparison.
use std::path::{Path, PathBuf};

use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use wedl::{
    Error,
    chronology::{self, AnnotationFilter},
    model::World,
    source, validation,
};

const OWNER: &str = "world_00000000000000000000000000";
const A: &str = "char_00000000000000000000000001";
const B: &str = "char_00000000000000000000000002";
const C: &str = "calendar_00000000000000000000000001";
const SECOND: &str = "calendar_00000000000000000000000002";
const ISOLATED: &str = "calendar_00000000000000000000000003";
const TABLE: &str = "calendar_00000000000000000000000004";
const ERA: &str = "era_00000000000000000000000001";

fn declarations() -> Json {
    json!({
        "calendars":[
            {"id":C,"label":"Cycle","months":[{"number":1,"days":10},{"number":2,"days":20}],
             "rule":{"kind":"cycle","period":4,"overrides":[{"residue":0,"target_month":2,"delta_days":1}]},
             "epoch":{"civil":{"year":0,"month":1,"day":1},"axis_day":0}},
            {"id":SECOND,"label":"Five","months":[{"number":1,"days":5}],
             "rule":{"kind":"cycle","period":1,"overrides":[]},
             "epoch":{"civil":{"year":0,"month":1,"day":1},"axis_day":0}},
            {"id":ISOLATED,"label":"Isolated","months":[{"number":1,"days":10}],
             "rule":{"kind":"cycle","period":1,"overrides":[]},"epoch":null},
            {"id":TABLE,"label":"Table","months":[{"number":1,"days":10}],
             "rule":{"kind":"table","years":[
               {"year":0,"overrides":[{"intercalary_month":{"number":13,"days":5}}]},
               {"year":2,"overrides":[]}]},
             "epoch":{"civil":{"year":0,"month":1,"day":1},"axis_day":0}}
        ],
        "eras":[{"id":ERA,"label":"Reign","calendar_id":C,"aliases":["R"],
          "display_year_zero":false,"display_epoch":{"display_year":1,"machine_year":0},
          "provenance":["annals"]}],
        "anchors":[{"id":annotation_id(1),"axis_day":0,"story_time":{"timeline":"main","tick":0,"order":0},"provenance":["annals"]}]
    })
}

fn annotation_id(number: u32) -> String {
    format!("chronology_{number:026}")
}

fn annotation(number: u32, value: Json) -> Json {
    json!({"id":annotation_id(number),"provenance":["annals"],"value":value})
}

fn civil(calendar: &str, year: i64, month: Option<u32>, day: Option<u32>) -> Json {
    let mut value = json!({"calendar_id":calendar,"year":year});
    if let Some(month) = month {
        value["month"] = json!(month);
    }
    if let Some(day) = day {
        value["day"] = json!(day);
    }
    value
}

fn date(number: u32, year: i64, month: Option<u32>, day: Option<u32>) -> Json {
    annotation(number, json!({"civil":civil(C,year,month,day)}))
}

fn build(catalog: Json, first: Json, second: Json) -> World {
    let values = [
        json!({"schema":"wedl/v1","kind":"world","id":OWNER,"title":"Annotation world",
               "capabilities":[],"default_timeline":"main","timelines":[{"id":"main","label":"Main"}],
               "state_keys":{},"chronology":catalog}),
        json!({"schema":"wedl/v1","kind":"character","id":A,"title":"Annals","aliases":["Ledger"],
               "status":"canonical","chronology":first}),
        json!({"schema":"wedl/v1","kind":"character","id":B,"title":"Witness","status":"retired",
               "chronology":second}),
    ];
    let records = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let raw = format!(
                "---\n{}---\nRetained prose.\n",
                serde_yaml::to_string(value).unwrap()
            );
            source::parse_record(raw.as_bytes(), &format!("story/record-{index}.md")).unwrap()
        })
        .collect();
    source::world_from_records(PathBuf::new(), records, None).unwrap()
}

fn fixture(first: Json) -> World {
    build(declarations(), first, json!([]))
}

fn all(world: &World) -> Json {
    let report = validation::validate_world(world);
    assert!(report.valid, "{:?}", report.diagnostics);
    chronology::annotations(
        world,
        &AnnotationFilter {
            record: None,
            calendar: None,
            era: None,
            limit: 1000,
        },
    )
    .unwrap()
}

fn compare(world: &World, first: u32, second: u32) -> Json {
    chronology::compare_annotations(world, A, &annotation_id(first), A, &annotation_id(second))
        .unwrap()
}

fn invalid(value: Json) {
    let world = fixture(json!([value]));
    assert!(
        !chronology::validate(&world).is_empty(),
        "accepted invalid annotation"
    );
    assert!(matches!(
        chronology::annotations(
            &world,
            &AnnotationFilter {
                record: None,
                calendar: None,
                era: None,
                limit: 1000
            }
        ),
        Err(Error::Validation(_))
    ));
}

#[test]
fn bundled_annotations_normalize_without_changing_authored_source() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/chronology_conformance");
    let world = source::load_world(root, None).unwrap();
    let fingerprint = world.fingerprint.clone();
    let response = all(&world);
    assert_eq!(response["count"], 13);
    assert_eq!(response["scope"], "author-all-time-chronology-source");
    assert_eq!(response["complete"], true);
    let year = response["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["annotationId"] == "chronology_3123456789abcdefghjkmnpqrs")
        .unwrap();
    assert_eq!(year["normalized"]["precision"], "year");
    assert_eq!(year["normalized"]["lowerDay"], "14813");
    assert_eq!(year["normalized"]["upperDay"], "14871");
    assert_eq!(year["citation"]["fingerprint"], fingerprint);
    let duration = response["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["normalized"]["valueKind"] == "duration")
        .unwrap();
    assert_eq!(duration["value"]["duration"]["value"], "2");
    assert_eq!(duration["normalized"]["reason"], "duration");
    assert_eq!(world.fingerprint, fingerprint);
}

#[test]
fn partial_dates_expand_to_inclusive_calendar_boundaries() {
    let world = fixture(json!([
        date(1, -1, None, None),
        date(2, 0, None, None),
        date(3, 0, Some(2), None),
        date(4, 0, Some(2), Some(21)),
        annotation(5, json!({"civil":civil(TABLE,0,None,None)})),
        annotation(6, json!({"civil":civil(TABLE,2,None,None)}))
    ]));
    let result = all(&world);
    for (index, lower, upper, precision) in [
        (0, "-30", "-1", "year"),
        (1, "0", "30", "year"),
        (2, "10", "30", "month"),
        (3, "30", "30", "day"),
        (4, "0", "14", "year"),
        (5, "15", "24", "year"),
    ] {
        assert_eq!(result["items"][index]["normalized"]["lowerDay"], lower);
        assert_eq!(result["items"][index]["normalized"]["upperDay"], upper);
        assert_eq!(result["items"][index]["normalized"]["precision"], precision);
    }
    invalid(date(1, 1, Some(2), Some(21)));
    invalid(annotation(1, json!({"civil":civil(TABLE,1,None,None)})));
}

#[test]
fn exact_and_interval_relations_respect_inclusive_endpoints() {
    let world = fixture(json!([
        date(1, 0, Some(1), Some(6)),
        annotation(2, json!({"civil":civil(SECOND,1,Some(1),Some(1))})),
        date(3, 0, Some(1), Some(7)),
        date(4, 0, None, None),
        date(5, 0, None, None),
        annotation(
            6,
            json!({"range":{"calendar_id":C,"lower":null,"upper":civil(C,-1,None,None)}})
        ),
        annotation(
            7,
            json!({"range":{"calendar_id":C,"lower":civil(C,0,None,None),"upper":null}})
        ),
        annotation(
            8,
            json!({"range":{"calendar_id":C,"lower":null,"upper":null}})
        ),
        date(9, 0, Some(2), Some(21))
    ]));
    all(&world);
    for (left, right, expected) in [
        (1, 2, "equal"),
        (1, 3, "before"),
        (3, 1, "after"),
        (4, 5, "overlaps"),
        (4, 9, "overlaps"),
        (6, 4, "before"),
        (7, 4, "overlaps"),
        (8, 8, "overlaps"),
    ] {
        assert_eq!(compare(&world, left, right)["relation"], expected);
    }
}

#[test]
fn era_inputs_skip_zero_and_validate_the_whole_partial_interval() {
    let value = |number, year, month: Option<u32>| {
        let mut era = json!({"era_id":ERA,"year":year});
        if let Some(month) = month {
            era["month"] = json!(month);
        }
        annotation(number, json!({"era":era}))
    };
    let world = fixture(json!([
        value(1, -1, None),
        value(2, 1, None),
        date(3, 0, None, None)
    ]));
    let result = all(&world);
    assert_eq!(result["items"][0]["normalized"]["lowerDay"], "-30");
    assert_eq!(result["items"][1]["normalized"]["lowerDay"], "0");
    assert_eq!(compare(&world, 2, 3)["relation"], "overlaps");
    invalid(value(1, 0, None));
    let mut catalog = declarations();
    catalog["eras"][0]["bounds"] =
        json!({"lower":{"year":0,"month":1,"day":5},"upper":{"year":0,"month":2,"day":21}});
    let valid = build(catalog.clone(), json!([value(1, 1, Some(2))]), json!([]));
    all(&valid);
    let invalid = build(catalog, json!([value(1, 1, None)]), json!([]));
    assert!(!chronology::validate(&invalid).is_empty());
}

#[test]
fn isolated_calendars_compare_only_with_the_same_calendar_basis() {
    let world = fixture(json!([
        annotation(1, json!({"civil":civil(ISOLATED,-1,None,None)})),
        annotation(2, json!({"civil":civil(ISOLATED,0,None,None)})),
        date(3, 0, None, None),
    ]));
    let result = all(&world);
    assert_eq!(
        result["items"][0]["normalized"]["basis"],
        format!("calendar:{ISOLATED}")
    );
    assert_eq!(result["items"][0]["normalized"]["lowerDay"], "-10");
    assert_eq!(compare(&world, 1, 2)["relation"], "before");
    let unrelated = compare(&world, 2, 3);
    assert_eq!(unrelated["relation"], "unknown");
    assert_eq!(unrelated["reason"], "no_shared_axis");
}

#[test]
fn finite_approximations_establish_overlap_but_never_order_or_equality() {
    let approx = |number, lower: Json, upper: Json| {
        annotation(
            number,
            json!({"approx":{"display_value":"around","bounds":{"lower":lower,"upper":upper}}}),
        )
    };
    let world = fixture(json!([
        approx(
            1,
            civil(C, 0, Some(1), Some(6)),
            civil(C, 0, Some(1), Some(6))
        ),
        date(2, 0, Some(1), Some(6)),
        date(3, 1, None, None),
        approx(4, Json::Null, civil(C, 0, None, None)),
        approx(5, Json::Null, Json::Null)
    ]));
    all(&world);
    assert_eq!(compare(&world, 1, 2)["relation"], "overlaps");
    for (left, right) in [(1, 3), (4, 2), (5, 2)] {
        let result = compare(&world, left, right);
        assert_eq!(result["relation"], "unknown");
        assert_eq!(result["reason"], "approximate_only");
    }
}

#[test]
fn conflicts_relative_references_and_durations_never_supply_inferred_dates() {
    let world = fixture(json!([
        annotation(
            1,
            json!({"conflict":{"claims":[{"civil":civil(C,0,None,None)},{"civil":civil(C,0,None,None)}]}})
        ),
        date(2, 1, None, None),
        annotation(
            3,
            json!({"relative":{"relation":"before","before_id":A,"after_id":B}})
        ),
        annotation(4, json!({"duration":{"unit":"month","value":-2}})),
    ]));
    let result = all(&world);
    assert_eq!(result["items"][3]["value"]["duration"]["value"], "-2");
    for (left, reason) in [(1, "conflicting_claims"), (3, "relative"), (4, "duration")] {
        let result = compare(&world, left, 2);
        assert_eq!(result["relation"], "unknown");
        assert_eq!(result["reason"], reason);
    }
}

#[test]
fn declared_reference_filters_include_conflict_branches_and_era_calendars() {
    let world = fixture(json!([
        date(1, 0, None, None),
        annotation(
            2,
            json!({"conflict":{"claims":[{"civil":civil(ISOLATED,0,None,None)},{"era":{"era_id":ERA,"year":1}}]}})
        ),
        annotation(3, json!({"relative":{"relation":"after","after_id":A}})),
    ]));
    all(&world);
    let query = |calendar, era| {
        chronology::annotations(
            &world,
            &AnnotationFilter {
                record: None,
                calendar,
                era,
                limit: 10,
            },
        )
        .unwrap()
    };
    assert_eq!(query(Some(C.into()), None)["count"], 2);
    assert_eq!(query(Some(ISOLATED.into()), None)["count"], 1);
    assert_eq!(query(None, Some(ERA.into()))["count"], 1);
    assert_eq!(query(Some(ISOLATED.into()), Some(ERA.into()))["count"], 1);
    for filter in [
        AnnotationFilter {
            record: None,
            calendar: Some("missing".into()),
            era: None,
            limit: 10,
        },
        AnnotationFilter {
            record: None,
            calendar: None,
            era: Some("missing".into()),
            limit: 10,
        },
    ] {
        assert!(matches!(
            chronology::annotations(&world, &filter),
            Err(Error::NotFound(_))
        ));
    }
}

#[test]
fn annotation_identity_is_owner_scoped_and_results_keep_source_ordinal() {
    let world = build(
        declarations(),
        json!([date(2, 0, None, None), date(1, 1, None, None)]),
        json!([date(1, 2, None, None)]),
    );
    let result = all(&world);
    assert_eq!(result["count"], 3);
    assert_eq!(result["items"][0]["annotationId"], annotation_id(1));
    assert_eq!(result["items"][0]["sourceOrdinal"], "1");
    assert_eq!(result["items"][1]["sourceOrdinal"], "0");
    assert_eq!(result["items"][2]["recordStatus"], "retired");
    let relation = chronology::compare_annotations(
        &world,
        "Ledger",
        &annotation_id(1),
        "Witness",
        &annotation_id(1),
    )
    .unwrap();
    assert_eq!(relation["relation"], "before");
    assert!(matches!(
        chronology::compare_annotations(&world, A, &annotation_id(2), B, &annotation_id(2)),
        Err(Error::NotFound(_))
    ));
    for owner in [OWNER, "Annotation world"] {
        assert!(matches!(
            chronology::annotations(
                &world,
                &AnnotationFilter {
                    record: Some(owner.into()),
                    calendar: None,
                    era: None,
                    limit: 10
                }
            ),
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            chronology::compare_annotations(&world, owner, &annotation_id(1), A, &annotation_id(1)),
            Err(Error::InvalidInput(_))
        ));
    }
}

#[test]
fn source_annotations_reject_malformed_shapes_unknown_references_and_bad_bounds() {
    let good = date(1, 0, None, None);
    for (path, bad) in [
        ("id", json!("chronology_bad")),
        ("provenance", json!([])),
        ("role", json!(" ")),
        ("display", Json::Null),
        ("value", json!({})),
        (
            "value",
            json!({"civil":civil(C,0,None,None),"duration":{"unit":"day","value":2}}),
        ),
        ("value", json!({"civil":{"calendar_id":C,"year":0,"day":1}})),
        ("value", json!({"civil":{"calendar_id":C,"year":"0"}})),
        ("value", json!({"civil":{"calendar_id":C,"year":false}})),
        ("value", json!({"civil":{"calendar_id":C,"year":0.0}})),
        ("value", json!({"civil":{"calendar_id":"missing","year":0}})),
        (
            "value",
            json!({"civil":{"calendar_id":C,"year":0,"unknown":true}}),
        ),
        (
            "value",
            json!({"range":{"calendar_id":C,"lower":civil(C,1,None,None),"upper":civil(C,0,None,None)}}),
        ),
        (
            "value",
            json!({"range":{"calendar_id":C,"lower":civil(SECOND,0,None,None),"upper":null}}),
        ),
        (
            "value",
            json!({"approx":{"display_value":"around","bounds":{"lower":civil(C,0,None,None),"upper":civil(SECOND,0,None,None)}}}),
        ),
        (
            "value",
            json!({"relative":{"relation":"before","before_id":"Annals"}}),
        ),
        ("value", json!({"relative":{"relation":"before"}})),
        ("value", json!({"duration":{"unit":"hour","value":1}})),
        (
            "value",
            json!({"conflict":{"claims":[{"civil":civil(C,0,None,None)}]}}),
        ),
    ] {
        let mut bad_annotation = good.clone();
        bad_annotation[path] = bad;
        invalid(bad_annotation);
    }
}

#[test]
fn source_normalization_rejects_overflow_and_preserves_large_exact_values() {
    let world = fixture(json!([
        date(1, 9007199254740993, Some(1), Some(1)),
        annotation(2, json!({"duration":{"unit":"day","value":i64::MAX}}))
    ]));
    let result = all(&world);
    assert_eq!(
        result["items"][0]["value"]["civil"]["year"],
        "9007199254740993"
    );
    assert_eq!(
        result["items"][1]["value"]["duration"]["value"],
        i64::MAX.to_string()
    );
    invalid(date(1, i64::MAX, None, None));
    invalid(annotation(
        1,
        json!({"civil":civil(ISOLATED,i64::MIN,None,None)}),
    ));
    let mut catalog = declarations();
    catalog["eras"][0]["display_epoch"]["machine_year"] = json!(i64::MAX);
    let world = build(
        catalog,
        json!([annotation(1, json!({"era":{"era_id":ERA,"year":2}}))]),
        json!([]),
    );
    assert!(!chronology::validate(&world).is_empty());
}

#[test]
fn opaque_extensions_do_not_validate_references_or_leak_through_annotations() {
    let mut world = fixture(json!([date(1, 0, None, None)]));
    let record = world.records.get_mut(A).unwrap();
    let mut metadata = record.frontmatter.clone();
    metadata["chronology"][0]["x-opaque"] =
        serde_yaml::from_str("{1: !secret hidden, chronology: [invalid]}").unwrap();
    metadata["chronology"][0]["value"]["civil"]["x-calendar"] =
        Yaml::String("calendar_missing".into());
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&metadata).unwrap(),
        record.body
    );
    let replacement = source::parse_record(raw.as_bytes(), &record.source_path).unwrap();
    world.records.insert(A.into(), replacement);
    let world =
        source::world_from_records(PathBuf::new(), world.records.into_values().collect(), None)
            .unwrap();
    let result = all(&world);
    let text = serde_json::to_string(&result).unwrap();
    assert!(!text.contains("hidden"));
    assert!(!text.contains("x-opaque"));
    assert!(!text.contains("calendar_missing"));
}

#[test]
fn duplicate_ids_and_structural_resource_limits_are_rejected() {
    let repeated = fixture(json!([date(1, 0, None, None), date(1, 1, None, None)]));
    assert!(!chronology::validate(&repeated).is_empty());
    let mut case_a = date(1, 0, None, None);
    case_a["id"] = json!("chronology_A0000000000000000000000000");
    let mut case_b = case_a.clone();
    case_b["id"] = json!("chronology_a0000000000000000000000000");
    assert!(!chronology::validate(&fixture(json!([case_a, case_b]))).is_empty());
    let many = (0..501)
        .map(|number| date(number, 0, None, None))
        .collect::<Vec<_>>();
    assert!(!chronology::validate(&fixture(json!(many))).is_empty());
    invalid(annotation(
        1,
        json!({"conflict":{"claims":vec![json!({"duration":{"unit":"day","value":1}});65]}}),
    ));
    let branch =
        json!({"conflict":{"claims":vec![json!({"duration":{"unit":"day","value":1}});64]}});
    invalid(annotation(
        1,
        json!({"conflict":{"claims":vec![branch;64]}}),
    ));
}

#[test]
fn complete_listing_limit_is_applied_after_filters_without_truncation() {
    let world = fixture(json!([
        date(1, 0, None, None),
        annotation(2, json!({"civil":civil(ISOLATED,0,None,None)}))
    ]));
    for limit in [0, 1, 1001] {
        assert!(matches!(
            chronology::annotations(
                &world,
                &AnnotationFilter {
                    record: None,
                    calendar: None,
                    era: None,
                    limit
                }
            ),
            Err(Error::InvalidInput(_))
        ));
    }
    assert_eq!(
        chronology::annotations(
            &world,
            &AnnotationFilter {
                record: Some("Ledger".into()),
                calendar: Some(C.into()),
                era: None,
                limit: 1
            }
        )
        .unwrap()["count"],
        1
    );
    assert_eq!(
        chronology::annotations(
            &world,
            &AnnotationFilter {
                record: Some(B.into()),
                calendar: None,
                era: None,
                limit: 1
            }
        )
        .unwrap()["count"],
        0
    );
}
