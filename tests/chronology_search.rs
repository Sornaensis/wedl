//! Date predicates preserve exact bounds, uncertainty and author-only scope.
use std::path::{Path, PathBuf};

use serde_json::{Value as Json, json};
use wedl::{
    Error,
    chronology::{self, AnnotationQuery, DateInput, DatePredicate, EraMatchMode},
    model::World,
    source,
};

const WORLD: &str = "world_00000000000000000000000000";
const OWNER: &str = "char_00000000000000000000000001";
const C: &str = "calendar_00000000000000000000000001";
const SECOND: &str = "calendar_00000000000000000000000002";
const ISOLATED: &str = "calendar_00000000000000000000000003";
const TABLE: &str = "calendar_00000000000000000000000004";
const ERA: &str = "era_00000000000000000000000001";
const LOCAL_ERA: &str = "era_00000000000000000000000002";

fn catalog() -> Json {
    let calendar = |id: &str, days: i64, epoch: bool| {
        json!({
            "id":id,"label":id,"months":[{"number":1,"days":days}],
            "rule":{"kind":"cycle","period":1,"overrides":[]},
            "epoch":epoch.then(||json!({"civil":{"year":0,"month":1,"day":1},"axis_day":0})),
        })
    };
    json!({
        "calendars":[calendar(C,10,true),calendar(SECOND,5,true),calendar(ISOLATED,10,false),
            {"id":TABLE,"label":"Table","months":[{"number":1,"days":10}],
             "rule":{"kind":"table","years":[{"year":0,"overrides":[]},{"year":2,"overrides":[]}]},
             "epoch":{"civil":{"year":0,"month":1,"day":1},"axis_day":0}}],
        "eras":[
            {"id":ERA,"label":"Reign","calendar_id":C,"aliases":[],"display_year_zero":false,
             "display_epoch":{"display_year":1,"machine_year":0},"provenance":["ledger"],
             "bounds":{"lower":{"year":0,"month":1,"day":2},"upper":{"year":0,"month":1,"day":4}}},
            {"id":LOCAL_ERA,"label":"Local","calendar_id":ISOLATED,"aliases":[],"display_year_zero":true,
             "display_epoch":{"display_year":0,"machine_year":0},"provenance":["ledger"]}
        ],"anchors":[],
    })
}

fn id(number: u32) -> String {
    format!("chronology_{number:026}")
}
fn civil(calendar: &str, year: i64, day: Option<u32>) -> Json {
    let mut value = json!({"calendar_id":calendar,"year":year});
    if let Some(day) = day {
        value["month"] = json!(1);
        value["day"] = json!(day);
    }
    value
}
fn claim(number: u32, value: Json) -> Json {
    json!({"id":id(number),"provenance":["ledger"],"value":value})
}
fn date(number: u32, calendar: &str, year: i64, day: Option<u32>) -> Json {
    claim(number, json!({"civil":civil(calendar,year,day)}))
}
fn range(number: u32, lower: Option<u32>, upper: Option<u32>) -> Json {
    claim(
        number,
        json!({"range":{"calendar_id":C,
        "lower":lower.map(|day|civil(C,0,Some(day))),
        "upper":upper.map(|day|civil(C,0,Some(day)))}}),
    )
}
fn approx(number: u32, lower: Option<u32>, upper: Option<u32>) -> Json {
    claim(
        number,
        json!({"approx":{"display_value":"circa","bounds":{
        "lower":lower.map(|day|civil(C,0,Some(day))),
        "upper":upper.map(|day|civil(C,0,Some(day)))}}}),
    )
}
fn world(annotations: Json) -> World {
    let records = [
        json!({"schema":"wedl/v1","kind":"world","id":WORLD,"title":"Query world","capabilities":[],
               "default_timeline":"main","timelines":[{"id":"main","label":"Main"}],"state_keys":{},"chronology":catalog()}),
        json!({"schema":"wedl/v1","kind":"character","id":OWNER,"title":"Annals","aliases":["Ledger"],
               "status":"retired","chronology":annotations}),
    ].iter().enumerate().map(|(index,value)| {
        let raw = format!("---\n{}---\nExact retained prose.\n",serde_yaml::to_string(value).unwrap());
        source::parse_record(raw.as_bytes(),&format!("story/{index}.md")).unwrap()
    }).collect();
    let world = source::world_from_records(PathBuf::new(), records, None).unwrap();
    assert!(
        chronology::validate(&world).is_empty(),
        "{:?}",
        chronology::validate(&world)
    );
    world
}
fn input(calendar: &str, year: i64, day: Option<u32>) -> DateInput {
    DateInput::Civil {
        calendar: calendar.into(),
        year,
        month: day.map(|_| 1),
        day,
    }
}
fn query(predicate: DatePredicate, value: DateInput) -> AnnotationQuery {
    AnnotationQuery {
        predicate,
        value,
        upper: None,
        record: None,
        calendar: None,
        era: None,
        era_mode: EraMatchMode::Authored,
        limit: 1000,
    }
}
fn search(world: &World, predicate: DatePredicate, value: DateInput) -> Json {
    chronology::search_annotations(world, &query(predicate, value)).unwrap()
}
fn ids(result: &Json) -> Vec<String> {
    result["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["annotationId"].as_str().unwrap().to_owned())
        .collect()
}
fn advisory(result: &Json, name: &str) -> u64 {
    result["advisories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == name)
        .map_or(0, |row| row["count"].as_u64().unwrap())
}
fn range_input(calendar: &str, lower: Option<DateInput>, upper: Option<DateInput>) -> DateInput {
    DateInput::Range {
        calendar: calendar.into(),
        lower: lower.map(Box::new),
        upper: upper.map(Box::new),
    }
}

#[test]
fn on_date_reports_interval_identity_and_inclusive_overlap_in_stable_order() {
    let world = world(json!([
        date(3, C, 0, Some(5)),
        date(1, C, 0, None),
        range(2, Some(5), Some(7)),
        date(4, C, 0, Some(8))
    ]));
    let before = world.fingerprint.clone();
    let result = search(&world, DatePredicate::OnDate, input(C, 0, Some(5)));
    assert_eq!(ids(&result), [id(1), id(2), id(3)]);
    assert_eq!(result["items"][0]["relation"], "overlaps");
    assert_eq!(result["items"][1]["relation"], "overlaps");
    assert_eq!(result["items"][2]["relation"], "exact");
    assert_eq!(result["items"][2]["sourceOrdinal"], "0");
    assert_eq!(result["items"][2]["citation"]["field"], "chronology[0]");
    let whole = search(&world, DatePredicate::OnDate, input(C, 0, None));
    assert_eq!(whole["items"][0]["relation"], "exact");
    assert_eq!(result["perspective"], "author");
    assert_eq!(result["scope"], "author-all-time-chronology-source");
    assert_eq!(result["items"][0]["recordStatus"], "retired");
    assert_eq!(result["complete"], true);
    assert_eq!(world.fingerprint, before);
}

#[test]
fn before_and_after_require_strict_finite_relevant_endpoints() {
    let world = world(json!([
        date(1, C, 0, Some(1)),
        date(2, C, 0, None),
        range(3, None, Some(3)),
        range(4, Some(7), None),
        approx(5, Some(1), Some(2)),
        date(6, C, 0, Some(5))
    ]));
    let before = search(&world, DatePredicate::Before, input(C, 0, Some(5)));
    assert_eq!(ids(&before), [id(1), id(3)]);
    assert_eq!(advisory(&before, "approximate-relation-excluded"), 1);
    let after = search(&world, DatePredicate::After, input(C, 0, Some(5)));
    assert_eq!(ids(&after), [id(4)]);
    assert_eq!(after["items"][0]["relation"], "after");
    let before_month = search(&world, DatePredicate::Before, input(C, 0, None));
    assert!(ids(&before_month).is_empty());
}

#[test]
fn overlap_includes_finite_approximations_but_other_predicates_do_not() {
    let world = world(json!([
        date(1, C, 0, Some(5)),
        approx(2, Some(4), Some(6)),
        approx(3, None, Some(6)),
        approx(4, None, None)
    ]));
    let overlaps = search(&world, DatePredicate::Overlaps, input(C, 0, Some(5)));
    assert_eq!(ids(&overlaps), [id(1), id(2)]);
    assert_eq!(advisory(&overlaps, "approximate-overlap-included"), 1);
    assert_eq!(advisory(&overlaps, "noncomparable-excluded"), 2);
    let exact = search(&world, DatePredicate::OnDate, input(C, 0, Some(5)));
    assert_eq!(ids(&exact), [id(1)]);
    assert_eq!(advisory(&exact, "approximate-relation-excluded"), 1);
    let approximate = DateInput::Approx {
        display: "about".into(),
        bounds: Box::new(range_input(
            C,
            Some(input(C, 0, Some(4))),
            Some(input(C, 0, Some(6))),
        )),
    };
    let overlap_query = search(&world, DatePredicate::Overlaps, approximate.clone());
    assert_eq!(ids(&overlap_query), [id(1), id(2)]);
    assert_eq!(overlap_query["normalized"]["approximate"], true);
    assert!(
        chronology::search_annotations(&world, &query(DatePredicate::OnDate, approximate)).is_err()
    );
}

#[test]
fn between_uses_first_lower_and_second_upper_on_the_shared_axis() {
    let world = world(json!([
        date(1, C, 0, Some(1)),
        date(2, C, 0, Some(10)),
        date(3, C, 1, Some(1)),
        approx(4, Some(2), Some(3))
    ]));
    let mut request = query(DatePredicate::Between, input(C, 0, Some(1)));
    request.upper = Some(input(SECOND, 1, None));
    let result = chronology::search_annotations(&world, &request).unwrap();
    assert_eq!(result["normalized"]["lowerDay"], "0");
    assert_eq!(result["normalized"]["upperDay"], "9");
    assert_eq!(ids(&result), [id(1), id(2)]);
    assert_eq!(advisory(&result, "approximate-relation-excluded"), 1);
    request.upper = Some(input(C, -1, None));
    assert!(chronology::search_annotations(&world, &request).is_err());
    request.upper = Some(input(ISOLATED, 1, None));
    assert!(chronology::search_annotations(&world, &request).is_err());
    request.upper = Some(DateInput::Approx {
        display: "traditional".into(),
        bounds: Box::new(range_input(C, None, None)),
    });
    assert!(chronology::search_annotations(&world, &request).is_err());
}

#[test]
fn open_ranges_can_overlap_but_order_queries_need_the_relevant_bound() {
    let world = world(json!([
        date(1, C, -1, None),
        date(2, C, 0, None),
        date(3, C, 1, None)
    ]));
    let open = range_input(C, None, None);
    assert_eq!(
        ids(&search(&world, DatePredicate::Overlaps, open.clone())),
        [id(1), id(2), id(3)]
    );
    for predicate in [DatePredicate::Before, DatePredicate::After] {
        assert!(chronology::search_annotations(&world, &query(predicate, open.clone())).is_err());
    }
    let lower = range_input(C, Some(input(C, 0, Some(1))), None);
    assert_eq!(ids(&search(&world, DatePredicate::Before, lower)), [id(1)]);
    let upper = range_input(C, None, Some(input(C, 0, Some(10))));
    assert_eq!(ids(&search(&world, DatePredicate::After, upper)), [id(3)]);
}

#[test]
fn era_filter_is_an_independent_overlap_even_for_disjoint_query_windows() {
    let world = world(json!([
        range(1, Some(1), Some(10)),
        date(2, C, 0, Some(9)),
        claim(3, json!({"era":{"era_id":ERA,"year":1,"month":1,"day":3}})),
        date(4, SECOND, 0, Some(3))
    ]));
    let mut request = query(DatePredicate::OnDate, input(C, 0, Some(9)));
    request.era = Some(ERA.into());
    request.era_mode = EraMatchMode::OverlapsBounds;
    let result = chronology::search_annotations(&world, &request).unwrap();
    assert_eq!(ids(&result), [id(1)]);
    assert_eq!(result["items"][0]["relation"], "overlaps");
    request.value = input(C, 0, Some(3));
    let bounded = chronology::search_annotations(&world, &request).unwrap();
    assert_eq!(ids(&bounded), [id(1), id(3), id(4)]);
    request.era_mode = EraMatchMode::Authored;
    let authored = chronology::search_annotations(&world, &request).unwrap();
    assert_eq!(ids(&authored), [id(3)]);
    request.era = Some(LOCAL_ERA.into());
    request.era_mode = EraMatchMode::OverlapsBounds;
    assert!(ids(&chronology::search_annotations(&world, &request).unwrap()).is_empty());
}

#[test]
fn era_operands_skip_zero_and_reject_partial_dates_crossing_era_bounds() {
    let world = world(json!([date(1, C, 0, Some(3))]));
    let era = |year, month, day| DateInput::Era {
        era: ERA.into(),
        year,
        month,
        day,
    };
    assert_eq!(
        ids(&search(
            &world,
            DatePredicate::OnDate,
            era(1, Some(1), Some(3))
        )),
        [id(1)]
    );
    for value in [
        era(0, Some(1), Some(3)),
        era(1, None, None),
        era(1, Some(1), Some(1)),
    ] {
        assert!(
            chronology::search_annotations(&world, &query(DatePredicate::OnDate, value)).is_err()
        );
    }
}

#[test]
fn isolated_calendars_only_match_the_same_local_basis() {
    let world = world(json!([
        date(1, C, 0, Some(3)),
        date(2, SECOND, 0, Some(3)),
        date(3, ISOLATED, 0, Some(3))
    ]));
    assert_eq!(
        ids(&search(&world, DatePredicate::OnDate, input(C, 0, Some(3)))),
        [id(1), id(2)]
    );
    let result = search(&world, DatePredicate::OnDate, input(ISOLATED, 0, Some(3)));
    assert_eq!(ids(&result), [id(3)]);
    assert_eq!(
        result["normalized"]["basis"],
        format!("calendar:{ISOLATED}")
    );
}

#[test]
fn conflicting_and_qualitative_operands_are_explicitly_unavailable() {
    let world = world(json!([date(1, C, 0, Some(3))]));
    let conflict = DateInput::Conflict {
        claims: vec![input(C, 0, Some(3)), input(C, 0, Some(3))],
    };
    let result = search(&world, DatePredicate::OnDate, conflict);
    assert_eq!(result["available"], false);
    assert_eq!(result["complete"], false);
    assert_eq!(result["reason"], "conflicting_claims");
    assert_eq!(result["count"], 0);
    let qualitative = DateInput::Approx {
        display: "traditional".into(),
        bounds: Box::new(range_input(C, None, None)),
    };
    let result = search(&world, DatePredicate::Overlaps, qualitative);
    assert_eq!(result["available"], false);
    assert_eq!(result["complete"], false);
    assert_eq!(result["reason"], "approximate_only");
    let invalid_branch = DateInput::Conflict {
        claims: vec![input(C, 0, Some(3)), input(C, 0, Some(11))],
    };
    assert!(
        chronology::search_annotations(&world, &query(DatePredicate::OnDate, invalid_branch))
            .is_err()
    );
}

#[test]
fn exclusion_advisories_follow_explicit_basis_and_unknown_branches() {
    let local = json!({"civil":civil(ISOLATED,0,Some(3))});
    let world = world(json!([
        claim(
            1,
            json!({"conflict":{"claims":[local.clone(),local.clone()]}})
        ),
        claim(
            2,
            json!({"conflict":{"claims":[local,{"duration":{"unit":"day","value":1}}]}})
        ),
        claim(
            3,
            json!({"relative":{"relation":"before","before_id":OWNER}})
        ),
        approx(4, None, None),
        claim(
            5,
            json!({"conflict":{"claims":[{"era":{"era_id":ERA,"year":1,"month":1,"day":3}},{"civil":civil(C,0,Some(3))}]}})
        )
    ]));
    let result = search(&world, DatePredicate::OnDate, input(C, 0, Some(3)));
    assert_eq!(advisory(&result, "noncomparable-excluded"), 4);
    let mut request = query(DatePredicate::OnDate, input(C, 0, Some(3)));
    request.era = Some(ERA.into());
    assert_eq!(
        advisory(
            &chronology::search_annotations(&world, &request).unwrap(),
            "noncomparable-excluded"
        ),
        1
    );
}

#[test]
fn filters_resolve_before_empty_results_and_limits_apply_after_filtering() {
    let world = world(json!([date(2, C, 0, Some(3)), date(1, SECOND, 0, Some(3))]));
    let mut request = query(DatePredicate::OnDate, input(C, 0, Some(3)));
    request.limit = 1;
    assert!(
        chronology::search_annotations(&world, &request)
            .unwrap_err()
            .to_string()
            .contains("exceeds limit")
    );
    request.calendar = Some(C.into());
    request.record = Some("Ledger".into());
    assert_eq!(
        ids(&chronology::search_annotations(&world, &request).unwrap()),
        [id(2)]
    );
    request.record = Some(WORLD.into());
    assert!(chronology::search_annotations(&world, &request).is_err());
    request.record = None;
    request.calendar = Some("missing".into());
    assert!(matches!(
        chronology::search_annotations(&world, &request),
        Err(Error::NotFound(_))
    ));
    request.calendar = None;
    request.era = Some("missing".into());
    assert!(matches!(
        chronology::search_annotations(&world, &request),
        Err(Error::NotFound(_))
    ));
    request.era = None;
    for limit in [0, 1001] {
        request.limit = limit;
        assert!(chronology::search_annotations(&world, &request).is_err());
    }
}

#[test]
fn closed_date_json_requires_exact_decimal_strings_and_structural_types() {
    let valid =
        json!({"civil":{"calendar_id":C,"year":"-9223372036854775808","month":"1","day":"2"}});
    assert_eq!(
        DateInput::parse_json(&valid.to_string())
            .unwrap()
            .json()
            .unwrap(),
        valid
    );
    for bad in [
        json!(1),
        json!(true),
        json!("+1"),
        json!("01"),
        json!("-0"),
        json!("1.0"),
        json!("9223372036854775808"),
    ] {
        let mut value = valid.clone();
        value["civil"]["year"] = bad;
        assert!(DateInput::parse_json(&value.to_string()).is_err());
    }
    for value in [
        json!({"civil":{"calendar_id":C,"year":"0","day":"1"}}),
        json!({"civil":{"calendar_id":C,"year":"0","x-hidden":true}}),
        json!({"civil":{"calendar_id":C,"year":"0"},"era":{"era_id":ERA,"year":"1"}}),
        json!({"range":{"calendar_id":C,"lower":{"calendar_id":SECOND,"year":"0"},"upper":null}}),
        json!({"approx":{"display_value":"around","bounds":{"lower":null,"upper":null}}}),
        json!({"duration":{"unit":"day","value":"1"}}),
    ] {
        assert!(
            DateInput::parse_json(&value.to_string()).is_err(),
            "{value}"
        );
    }
    let advanced = json!({"approx":{"display_value":"around","bounds":{"calendar_id":C,"lower":{"calendar_id":C,"year":"0"},"upper":null}}});
    assert_eq!(
        DateInput::parse_json(&advanced.to_string())
            .unwrap()
            .json()
            .unwrap(),
        advanced
    );
    for value in [
        format!(r#"{{"civil":{{"calendar_id":"{C}","year":"0","year":"1"}}}}"#),
        format!(r#"{{"civil":{{"calendar_id":"{C}","year":"0","\u0079ear":"1"}}}}"#),
        format!(
            r#"{{"civil":{{"calendar_id":"{C}","year":"0"}},"civil":{{"calendar_id":"{C}","year":"1"}}}}"#
        ),
    ] {
        assert!(
            DateInput::parse_json(&value).is_err(),
            "duplicate fields accepted: {value}"
        );
    }
    let supplementary = format!(
        r#"{{"approx":{{"display_value":"\ud83d\ude00","bounds":{{"calendar_id":"{C}","lower":null,"upper":null}}}}}}"#
    );
    assert_eq!(
        DateInput::parse_json(&supplementary)
            .unwrap()
            .json()
            .unwrap()["approx"]["display_value"],
        "😀"
    );
    // Transport nesting uses JSON's 128-container budget independently of
    // the typed algebra's 64-level limit. Each conflict adds three containers.
    let leaf = input(C, 0, None);
    let mut nested = leaf.clone();
    for _ in 0..41 {
        nested = DateInput::Conflict {
            claims: vec![leaf.clone(), nested],
        };
    }
    assert!(DateInput::parse_json(&nested.json().unwrap().to_string()).is_ok());
    nested = DateInput::Conflict {
        claims: vec![leaf, nested],
    };
    assert!(
        DateInput::parse_json(&nested.json().unwrap().to_string())
            .unwrap_err()
            .to_string()
            .contains("recursion limit")
    );
}

#[test]
fn missing_table_years_are_unavailable_without_concealing_invalid_operands() {
    let world = world(json!([]));
    let result = search(&world, DatePredicate::OnDate, input(TABLE, 1, None));
    assert_eq!(result["available"], false);
    assert_eq!(result["complete"], false);
    assert_eq!(result["reason"], "table_gap");
    let value = range_input(
        TABLE,
        Some(input(TABLE, 1, None)),
        Some(input(TABLE, 2, None)),
    );
    assert_eq!(
        search(&world, DatePredicate::Overlaps, value)["reason"],
        "table_gap"
    );
    let mut request = query(DatePredicate::Between, input(TABLE, 1, None));
    request.upper = Some(input(C, 0, Some(11)));
    assert!(chronology::search_annotations(&world, &request).is_err());
    let conflict = DateInput::Conflict {
        claims: vec![input(TABLE, 1, None), input(C, 0, Some(11))],
    };
    assert!(
        chronology::search_annotations(&world, &query(DatePredicate::OnDate, conflict)).is_err()
    );
}

#[test]
fn hand_built_operands_cannot_bypass_structural_or_resource_guards() {
    let world = world(json!([]));
    let malformed = [
        DateInput::Approx {
            display: "around".into(),
            bounds: Box::new(input(C, 0, None)),
        },
        DateInput::Conflict {
            claims: vec![input(C, 0, None)],
        },
        range_input(
            C,
            Some(DateInput::Era {
                era: ERA.into(),
                year: 1,
                month: Some(1),
                day: Some(3),
            }),
            None,
        ),
        input(C, 0, Some(11)),
        DateInput::Civil {
            calendar: C.into(),
            year: 0,
            month: None,
            day: Some(1),
        },
        input(C, i64::MAX, None),
    ];
    for value in malformed {
        assert!(
            chronology::search_annotations(&world, &query(DatePredicate::OnDate, value)).is_err()
        );
    }
    let mut deep = input(C, 0, None);
    for _ in 0..65 {
        deep = DateInput::Conflict {
            claims: vec![input(C, 0, None), deep],
        };
    }
    assert!(deep.json().is_err());
    let branch = DateInput::Conflict {
        claims: vec![input(C, 0, None); 64],
    };
    let large = DateInput::Conflict {
        claims: vec![branch; 64],
    };
    assert!(large.json().is_err());
    let mut missing_upper = query(DatePredicate::Between, input(C, 0, None));
    assert!(chronology::search_annotations(&world, &missing_upper).is_err());
    missing_upper.predicate = DatePredicate::OnDate;
    missing_upper.upper = Some(input(C, 0, None));
    assert!(chronology::search_annotations(&world, &missing_upper).is_err());
}

#[test]
fn bundled_chronology_search_retains_all_authored_uncertainty() {
    let world = source::load_world(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/chronology_conformance"),
        None,
    )
    .unwrap();
    let solar = "calendar_0123456789abcdefghjkmnpqrs";
    let result = search(
        &world,
        DatePredicate::Overlaps,
        range_input(solar, None, None),
    );
    assert_eq!(result["count"], 9);
    assert_eq!(advisory(&result, "approximate-overlap-included"), 1);
    assert_eq!(advisory(&result, "noncomparable-excluded"), 4);
    assert!(
        result["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["normalized"]["valueKind"] != "conflict")
    );
}
