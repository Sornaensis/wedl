//! Shared date inputs retain precision, era semantics and authored anchor facts.
use serde_json::{Value as Json, json};
use std::path::PathBuf;
use wedl::{
    Error,
    chronology::{self, DateInput, DateTarget},
    model::World,
    source, validation,
};

const CYCLE: &str = "calendar_00000000000000000000000001";
const FIVE: &str = "calendar_00000000000000000000000002";
const ISOLATED: &str = "calendar_00000000000000000000000003";
const TABLE: &str = "calendar_00000000000000000000000004";
const ERA: &str = "era_00000000000000000000000001";
const FIVE_ERA: &str = "era_00000000000000000000000002";
const LOCAL_ERA: &str = "era_00000000000000000000000003";

fn civil(calendar: &str, year: i64, month: Option<u32>, day: Option<u32>) -> DateInput {
    DateInput::Civil {
        calendar: calendar.into(),
        year,
        month,
        day,
    }
}
fn exact(calendar: &str, year: i64, day: u32) -> DateInput {
    civil(calendar, year, Some(1), Some(day))
}
fn era(era: &str, year: i64, month: Option<u32>, day: Option<u32>) -> DateInput {
    DateInput::Era {
        era: era.into(),
        year,
        month,
        day,
    }
}
fn range(calendar: &str, lower: Option<DateInput>, upper: Option<DateInput>) -> DateInput {
    DateInput::Range {
        calendar: calendar.into(),
        lower: lower.map(Box::new),
        upper: upper.map(Box::new),
    }
}
fn anchor(number: u32, axis: i64, timeline: &str, tick: i64, order: i32) -> Json {
    json!({"id":format!("chronology_{number:026}"),"axis_day":axis,
        "story_time":{"timeline":timeline,"tick":tick,"order":order},
        "provenance":[format!("source-{number}")]})
}
fn declarations() -> Json {
    json!({
        "calendars":[
            {"id":CYCLE,"label":"Cycle","months":[{"number":1,"days":10}],
             "rule":{"kind":"cycle","period":4,"overrides":[{"residue":0,"target_month":1,"delta_days":1}]},
             "epoch":{"civil":{"year":0,"month":1,"day":1},"axis_day":0}},
            {"id":FIVE,"label":"Five","months":[{"number":1,"days":5}],
             "rule":{"kind":"cycle","period":1,"overrides":[]},
             "epoch":{"civil":{"year":0,"month":1,"day":1},"axis_day":0}},
            {"id":ISOLATED,"label":"Local","months":[{"number":1,"days":10}],
             "rule":{"kind":"cycle","period":1,"overrides":[]},"epoch":null},
            {"id":TABLE,"label":"Table","months":[{"number":1,"days":10}],
             "rule":{"kind":"table","years":[
               {"year":0,"overrides":[{"intercalary_month":{"number":13,"days":5}}]},
               {"year":2,"overrides":[]}]},
             "epoch":{"civil":{"year":0,"month":1,"day":1},"axis_day":0}}
        ],
        "eras":[
            {"id":ERA,"calendar_id":CYCLE,"label":"Reign","aliases":[],
             "display_year_zero":false,"display_epoch":{"display_year":1,"machine_year":0},
             "bounds":{"lower":{"year":-2,"month":1,"day":1},"upper":{"year":2,"month":1,"day":10}},
             "provenance":["annals"]},
            {"id":FIVE_ERA,"calendar_id":FIVE,"label":"Age","aliases":[],
             "display_year_zero":true,"display_epoch":{"display_year":100,"machine_year":0},"provenance":["annals"]},
            {"id":LOCAL_ERA,"calendar_id":ISOLATED,"label":"Local era","aliases":[],
             "display_year_zero":false,"display_epoch":{"display_year":1,"machine_year":0},"provenance":["annals"]}
        ],
        "anchors":[anchor(1,0,"main",0,0),anchor(2,0,"main",0,0),
                   anchor(3,10,"main",7,1),anchor(4,11,"main",7,2),
                   anchor(5,0,"branch",1000,0),anchor(6,21,"main",8,0)]
    })
}
fn with_declarations(declaration: Json) -> World {
    let raw = json!({"schema":"wedl/v1","kind":"world",
        "id":"world_00000000000000000000000000","title":"Dates",
        "capabilities":[],"default_timeline":"main","state_keys":{},
        "timelines":[{"id":"main","label":"Main"},{"id":"branch","label":"Branch"}],
        "chronology":declaration});
    let bytes = format!(
        "---\n{}---\nRetained text.\n",
        serde_yaml::to_string(&raw).unwrap()
    );
    let record = source::parse_record(bytes.as_bytes(), "story/world.md").unwrap();
    let world = source::world_from_records(PathBuf::new(), vec![record], None).unwrap();
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    world
}
fn fixture() -> World {
    with_declarations(declarations())
}

#[test]
fn era_inputs_skip_zero_and_format_with_the_authored_precision() {
    let world = fixture();
    for (display, machine, lower) in [(-1, "-1", "-10"), (1, "0", "0"), (3, "2", "21")] {
        let value = era(ERA, display, None, None);
        let result = chronology::format_date(&world, &value).unwrap();
        assert_eq!(result["formatted"], format!("{ERA}:{display}"));
        assert_eq!(result["civil"]["year"], machine);
        assert_eq!(result["civil"]["month"], Json::Null);
        assert_eq!(result["precision"], "year");
        assert_eq!(result["axisDay"], Json::Null);
        assert_eq!(result["axisInterval"]["lowerDay"], lower);
        assert_eq!(result["scope"], "author-all-time-chronology-source");
        assert_eq!(result["citation"]["fingerprint"], world.fingerprint);
    }
    let month = chronology::format_date(&world, &era(ERA, 1, Some(1), None)).unwrap();
    assert_eq!(month["formatted"], format!("{ERA}:1-1"));
    assert_eq!(month["precision"], "month");
    let day = chronology::format_date(&world, &era(ERA, 1, Some(1), Some(11))).unwrap();
    assert_eq!(day["formatted"], format!("{ERA}:1-1-11"));
    assert_eq!(day["axisDay"], "10");
    assert!(chronology::format_date(&world, &era(ERA, 0, None, None)).is_err());
    assert!(chronology::format_date(&world, &era(ERA, 4, None, None)).is_err());
}

#[test]
fn entire_partial_era_interval_must_fit_bounds_without_clipping() {
    let mut declaration = declarations();
    declaration["eras"][0]["bounds"]["lower"] = json!({"year":0,"month":1,"day":2});
    declaration["eras"][0]["bounds"]["upper"] = json!({"year":0,"month":1,"day":10});
    let world = with_declarations(declaration);
    assert!(chronology::format_date(&world, &era(ERA, 1, None, None)).is_err());
    assert!(chronology::format_date(&world, &era(ERA, 1, Some(1), None)).is_err());
    for day in [2, 10] {
        assert_eq!(
            chronology::format_date(&world, &era(ERA, 1, Some(1), Some(day))).unwrap()["found"],
            true
        );
    }
    for day in [1, 11] {
        assert!(chronology::format_date(&world, &era(ERA, 1, Some(1), Some(day))).is_err());
    }
}

#[test]
fn formatting_retains_large_years_without_requiring_an_axis() {
    let world = fixture();
    for year in [i64::MIN, i64::MAX] {
        let result = chronology::format_date(&world, &civil(CYCLE, year, None, None)).unwrap();
        assert_eq!(result["found"], true);
        assert_eq!(result["civil"]["year"], year.to_string());
        assert_eq!(result["axisInterval"], Json::Null);
        assert_eq!(result["axisReason"], "overflow");
        let isolated = chronology::format_date(&world, &civil(ISOLATED, year, None, None)).unwrap();
        assert_eq!(isolated["found"], true);
        assert_eq!(isolated["axisReason"], "no_epoch");
    }
    let local = chronology::format_date(&world, &era(LOCAL_ERA, 1, Some(1), Some(1))).unwrap();
    assert_eq!(local["found"], true);
    assert_eq!(local["axisReason"], "no_epoch");
    // Display arithmetic, unlike axis availability, must still fit signed i64.
    let mut declaration = declarations();
    declaration["eras"][1]["display_epoch"] =
        json!({"display_year":i64::MIN,"machine_year":i64::MAX});
    let world = with_declarations(declaration);
    assert!(chronology::format_date(&world, &era(FIVE_ERA, i64::MAX, None, None)).is_err());
}

#[test]
fn conversion_accepts_era_sources_and_targets_without_losing_integer_precision() {
    let world = fixture();
    let input = era(ERA, 1, Some(1), Some(11));
    let result =
        chronology::convert_date(&world, &input, &DateTarget::Era(FIVE_ERA.into())).unwrap();
    assert_eq!(result["axisDay"], "10");
    assert_eq!(
        result["targetCivil"],
        json!({"year":"2","month":"1","day":"1"})
    );
    assert_eq!(result["target"]["era"]["year"], "102");
    assert_eq!(result["formatted"], format!("{FIVE_ERA}:102-1-1"));
    let back = chronology::convert_date(
        &world,
        &era(FIVE_ERA, 102, Some(1), Some(1)),
        &DateTarget::Era(ERA.into()),
    )
    .unwrap();
    assert_eq!(back["target"], input.json().unwrap());
    let negative = chronology::convert_date(
        &world,
        &era(ERA, -1, Some(1), Some(1)),
        &DateTarget::Calendar(FIVE.into()),
    )
    .unwrap();
    assert_eq!(negative["target"]["civil"]["year"], "-2");
    let huge = 9007199254740993_i64;
    let exact = chronology::convert_date(
        &world,
        &exact(FIVE, huge, 1),
        &DateTarget::Calendar(FIVE.into()),
    )
    .unwrap();
    assert_eq!(exact["target"]["civil"]["year"], huge.to_string());
    assert_eq!(exact["axisDay"], "45035996273704965");
}

#[test]
fn conversion_refuses_partial_uncertain_and_unavailable_dates() {
    let world = fixture();
    for value in [civil(CYCLE, 0, None, None), era(ERA, 1, Some(1), None)] {
        let result =
            chronology::convert_date(&world, &value, &DateTarget::Calendar(FIVE.into())).unwrap();
        assert_eq!(result["found"], false);
        assert_eq!(result["reason"], "conversion_exactness");
    }
    let local = chronology::convert_date(
        &world,
        &exact(ISOLATED, 0, 1),
        &DateTarget::Calendar(FIVE.into()),
    )
    .unwrap();
    assert_eq!(local["reason"], "no_epoch");
    let isolated_target = chronology::convert_date(
        &world,
        &exact(FIVE, 0, 1),
        &DateTarget::Era(LOCAL_ERA.into()),
    )
    .unwrap();
    assert_eq!(isolated_target["reason"], "no_epoch");
    let outside =
        chronology::convert_date(&world, &exact(CYCLE, 3, 1), &DateTarget::Era(ERA.into()));
    assert!(matches!(outside, Err(Error::InvalidInput(_))));
    assert!(
        chronology::convert_date(
            &world,
            &range(CYCLE, None, None),
            &DateTarget::Calendar(FIVE.into())
        )
        .is_err()
    );
    assert!(
        chronology::convert_date(
            &world,
            &exact(CYCLE, 0, 1),
            &DateTarget::Era("missing".into())
        )
        .is_err()
    );
}

#[test]
fn table_rows_and_intercalary_months_remain_available_only_when_authored() {
    let world = fixture();
    let table = civil(TABLE, 0, Some(13), None);
    let formatted = chronology::format_date(&world, &table).unwrap();
    assert_eq!(formatted["bounds"]["upper"]["day"], "5");
    assert_eq!(
        formatted["axisInterval"],
        json!({"lowerDay":"10","upperDay":"14"})
    );
    for input in [civil(TABLE, 1, None, None), exact(TABLE, 1, 1)] {
        assert_eq!(
            chronology::format_date(&world, &input).unwrap()["reason"],
            "table_gap"
        );
        assert_eq!(
            chronology::story_times(&world, &input, None, None, 100).unwrap()["reason"],
            "table_gap"
        );
    }
    let missing = chronology::convert_date(
        &world,
        &exact(TABLE, 1, 1),
        &DateTarget::Calendar(FIVE.into()),
    )
    .unwrap();
    assert_eq!(missing["reason"], "table_gap");
    let disconnected = chronology::convert_date(
        &world,
        &exact(CYCLE, -1, 1),
        &DateTarget::Calendar(TABLE.into()),
    )
    .unwrap();
    assert_eq!(disconnected["reason"], "disconnected_table");
    let partial = chronology::story_times(&world, &table, None, Some("main"), 100).unwrap();
    assert_eq!(partial["count"], 2);
    assert!(chronology::format_date(&world, &civil(TABLE, 2, Some(13), None)).is_err());
}

#[test]
fn interval_anchor_reads_preserve_cardinality_timelines_aliases_and_provenance() {
    let world = fixture();
    let before = world.fingerprint.clone();
    let result =
        chronology::story_times(&world, &civil(CYCLE, 0, None, None), None, None, 100).unwrap();
    assert_eq!(result["available"], true);
    assert_eq!(result["complete"], true);
    assert_eq!(result["mapping"], "ambiguous");
    assert_eq!(result["count"], 3);
    assert_eq!(result["timeline"], Json::Null);
    assert_eq!(result["items"][0]["storyTime"]["timeline"], "branch");
    assert_eq!(result["items"][1]["anchorIds"].as_array().unwrap().len(), 2);
    assert_eq!(
        result["items"][1]["provenance"],
        json!(["source-1", "source-2"])
    );
    assert_eq!(
        result["items"][1]["citation"]["field"],
        "chronology.anchors"
    );
    assert_eq!(result["items"][1]["citation"]["fingerprint"], before);
    let selected = chronology::story_times(
        &world,
        &civil(CYCLE, 0, None, None),
        None,
        Some("main"),
        100,
    )
    .unwrap();
    assert_eq!(selected["count"], 2);
    assert_eq!(selected["items"][1]["axisDay"], "10");
    assert_eq!(selected["items"][1]["storyTime"]["tick"], "7");
    assert_eq!(selected["items"][1]["storyTime"]["order"], "1");
    assert_eq!(world.fingerprint, before);
}

#[test]
fn exact_and_era_intervals_never_interpolate_missing_anchor_days() {
    let world = fixture();
    let unique =
        chronology::story_times(&world, &era(ERA, 2, None, None), None, None, 100).unwrap();
    assert_eq!(unique["mapping"], "unique");
    assert_eq!(unique["count"], 1);
    assert_eq!(unique["items"][0]["axisDay"], "11");
    assert_eq!(unique["items"][0]["storyTime"]["order"], "2");
    let none =
        chronology::story_times(&world, &exact(CYCLE, 0, 2), None, Some("main"), 100).unwrap();
    assert_eq!(none["available"], true);
    assert_eq!(none["complete"], true);
    assert_eq!(none["mapping"], "none");
    assert_eq!(none["count"], 0);
    assert_eq!(none["reason"], "no_match");
    assert!(
        chronology::story_times(&world, &exact(CYCLE, 0, 1), None, Some("missing"), 100).is_err()
    );
}

#[test]
fn two_operand_anchor_intervals_use_first_lower_and_second_upper_inclusively() {
    let world = fixture();
    let lower = exact(CYCLE, 0, 11);
    let upper = civil(FIVE, 2, None, None); // 10..14; left 10 includes both main anchors.
    let result = chronology::story_times(&world, &lower, Some(&upper), Some("main"), 100).unwrap();
    assert_eq!(
        result["normalized"],
        json!({"basis":"axis","lowerDay":"10","upperDay":"14"})
    );
    assert_eq!(result["count"], 2);
    assert!(chronology::story_times(&world, &exact(CYCLE, 2, 1), Some(&upper), None, 100).is_err());
    assert!(
        chronology::story_times(&world, &range(CYCLE, None, None), Some(&upper), None, 100)
            .is_err()
    );
    let local =
        chronology::story_times(&world, &lower, Some(&exact(ISOLATED, 0, 1)), None, 100).unwrap();
    assert_eq!(local["reason"], "no_epoch");
    // An unavailable first operand must not conceal an invalid second operand.
    for unavailable in [exact(TABLE, 1, 1), exact(ISOLATED, 0, 1)] {
        assert!(
            chronology::story_times(&world, &unavailable, Some(&exact(CYCLE, 0, 12)), None, 100)
                .is_err()
        );
        let reversed = range(CYCLE, Some(exact(CYCLE, 2, 1)), Some(exact(CYCLE, 0, 1)));
        assert!(chronology::story_times(&world, &unavailable, Some(&reversed), None, 100).is_err());
        assert!(
            chronology::story_times(
                &world,
                &unavailable,
                Some(&exact(CYCLE, i64::MAX, 1)),
                None,
                100
            )
            .is_err()
        );
    }
}

#[test]
fn open_ranges_map_authored_anchors_but_uncertainty_never_does() {
    let world = fixture();
    let all = range(CYCLE, None, None);
    let result = chronology::story_times(&world, &all, None, None, 100).unwrap();
    assert_eq!(result["count"], 5);
    assert_eq!(result["normalized"]["lowerDay"], Json::Null);
    assert_eq!(result["normalized"]["upperDay"], Json::Null);
    let earlier = range(CYCLE, None, Some(exact(CYCLE, 0, 11)));
    assert_eq!(
        chronology::story_times(&world, &earlier, None, None, 100).unwrap()["count"],
        3
    );
    let finite = range(CYCLE, Some(exact(CYCLE, 0, 1)), Some(exact(CYCLE, 2, 1)));
    for bounds in [finite, all] {
        let approximate = DateInput::Approx {
            display: "around then".into(),
            bounds: Box::new(bounds),
        };
        let result = chronology::story_times(&world, &approximate, None, None, 100).unwrap();
        assert_eq!(result["reason"], "approximate_only");
        assert_eq!(result["mapping"], Json::Null);
        assert_eq!(result["complete"], false);
        assert_eq!(result["count"], 0);
    }
    let conflict = DateInput::Conflict {
        claims: vec![exact(CYCLE, 0, 1), exact(CYCLE, 0, 1)],
    };
    assert_eq!(
        chronology::story_times(&world, &conflict, None, None, 100).unwrap()["reason"],
        "conflicting_claims"
    );
    let missing = range(TABLE, Some(exact(TABLE, 1, 1)), None);
    assert_eq!(
        chronology::story_times(&world, &missing, None, None, 100).unwrap()["reason"],
        "table_gap"
    );
    let isolated = chronology::story_times(
        &world,
        &civil(ISOLATED, i64::MAX, None, None),
        None,
        None,
        100,
    )
    .unwrap();
    assert_eq!(isolated["reason"], "no_epoch");
}

#[test]
fn anchor_results_are_complete_or_error_after_timeline_filtering() {
    let world = fixture();
    let day = exact(CYCLE, 0, 1);
    assert!(chronology::story_times(&world, &day, None, None, 1).is_err());
    assert_eq!(
        chronology::story_times(&world, &day, None, Some("main"), 1).unwrap()["count"],
        1
    );
    for limit in [0, 1001] {
        assert!(chronology::story_times(&world, &day, None, None, limit).is_err());
    }
    let bad = civil(CYCLE, 0, None, Some(1));
    assert!(chronology::format_date(&world, &bad).is_err());
    assert!(chronology::story_times(&world, &bad, None, None, 100).is_err());
    for day in [0, 12] {
        assert!(chronology::format_date(&world, &exact(CYCLE, 0, day)).is_err());
    }
}
