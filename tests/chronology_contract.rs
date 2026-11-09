use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::path::{Path, PathBuf};
use wedl::{
    Error,
    chronology::{self, CivilDate},
    model::{StoryTime, World},
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const CYCLE: &str = "calendar_00000000000000000000000001";
const SECOND: &str = "calendar_00000000000000000000000002";
const ISOLATED: &str = "calendar_00000000000000000000000003";
const ERA: &str = "era_00000000000000000000000001";

fn day(year: i64, month: u32, day: u32) -> CivilDate {
    CivilDate { year, month, day }
}
fn sample() -> Json {
    json!({
        "calendars": [
            {"id":CYCLE,"label":"Cycle","months":[{"number":1,"days":10}],
             "rule":{"kind":"cycle","period":4,"overrides":[{"residue":0,"target_month":1,"delta_days":1}]},
             "epoch":{"civil":{"year":0,"month":1,"day":1},"axis_day":0}},
            {"id":SECOND,"label":"Five","months":[{"number":1,"days":5}],
             "rule":{"kind":"cycle","period":1,"overrides":[]},
             "epoch":{"civil":{"year":0,"month":1,"day":1},"axis_day":0}},
            {"id":ISOLATED,"label":"Isolated","months":[{"number":1,"days":10}],
             "rule":{"kind":"cycle","period":1,"overrides":[]},"epoch":null}
        ],
        "eras":[{"id":ERA,"calendar_id":CYCLE,"label":"Reign","aliases":[],
                 "display_year_zero":false,"display_epoch":{"display_year":1,"machine_year":0},
                 "bounds":{"lower":{"year":-2,"month":1,"day":1},"upper":{"year":2,"month":1,"day":10}},
                 "provenance":["annals"]}],
        "anchors":[
            {"id":"chronology_00000000000000000000000001","axis_day":0,"story_time":{"timeline":"main","tick":0,"order":0},"provenance":["first"]},
            {"id":"chronology_00000000000000000000000002","axis_day":11,"story_time":{"timeline":"main","tick":100,"order":2},"provenance":["later"]}
        ]
    })
}
fn world_value(value: Yaml) -> World {
    let mut front: Yaml = serde_yaml::from_str(&format!("schema: wedl/v1\nkind: world\nid: {WORLD}\ntitle: Calendars\ncapabilities: []\ndefault_timeline: main\ntimelines: [{{id: main, label: Main}}, {{id: branch, label: Branch}}]\nstate_keys: {{}}\n")).unwrap();
    front["chronology"] = value;
    let source = format!(
        "---\n{}---\nCalendar prose.\n",
        serde_yaml::to_string(&front).unwrap()
    );
    let record = source::parse_record(source.as_bytes(), "story/world.md").unwrap();
    source::world_from_records(PathBuf::new(), vec![record], None).unwrap()
}
fn make_world(value: Json) -> World {
    world_value(serde_yaml::to_value(value).unwrap())
}
fn valid(world: &World) {
    assert!(
        chronology::validate(world).is_empty(),
        "{:?}",
        chronology::validate(world)
    );
    let report = validation::validate_world(world);
    assert!(report.valid, "{:?}", report.diagnostics);
}
fn invalid(value: Json) {
    let world = make_world(value);
    assert!(!chronology::validate(&world).is_empty());
    assert!(matches!(
        chronology::calendars(&world, 100),
        Err(Error::Validation(_))
    ));
}
fn civil(value: &Json) -> CivilDate {
    day(
        value["year"].as_str().unwrap().parse().unwrap(),
        value["month"].as_str().unwrap().parse().unwrap(),
        value["day"].as_str().unwrap().parse().unwrap(),
    )
}

#[test]
fn packaged_catalogue_works_without_changing_source_or_annotations() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/chronology_conformance");
    let world = source::load_world(root, None).unwrap();
    valid(&world);
    let result = chronology::calendars(&world, 100).unwrap();
    assert_eq!(result["scope"], "author-all-time-chronology-source");
    assert_eq!(result["perspective"], "author");
    assert_eq!(result["count"], 3);
    assert_eq!(result["complete"], true);
    let solar = result["items"][0]["calendarId"].as_str().unwrap();
    let regnal = result["items"][1]["calendarId"].as_str().unwrap();
    let converted = chronology::convert(&world, solar, regnal, day(0, 2, 4)).unwrap();
    assert_eq!(
        converted["target"],
        json!({"year":"0","month":"13","day":"5"})
    );
    assert_eq!(converted["axisDay"], "34");
    assert_eq!(
        result["items"][0]["citation"]["fingerprint"],
        world.fingerprint
    );
    assert_eq!(result["items"][0]["months"][0]["days"], "31");
    assert!(!result.to_string().contains("yearly_annals"));
}

#[test]
fn cycles_use_floor_arithmetic_and_round_trip_negative_and_huge_years() {
    let world = make_world(sample());
    valid(&world);
    for (year, expected) in [(-5, -51), (-4, -41), (-1, -10), (0, 0), (1, 11), (4, 41)] {
        assert_eq!(
            chronology::date(&world, CYCLE, day(year, 1, 1), None).unwrap()["axisDay"],
            expected.to_string()
        );
    }
    for year in [-100000000000, -1000, -4, -1, 0, 1, 4, 1000, 100000000000] {
        let last = if year % 4 == 0 { 11 } else { 10 };
        for original in [day(year, 1, 1), day(year, 1, last)] {
            let forward = chronology::convert(&world, CYCLE, SECOND, original).unwrap();
            let reverse =
                chronology::convert(&world, SECOND, CYCLE, civil(&forward["target"])).unwrap();
            assert_eq!(civil(&reverse["target"]), original);
        }
    }
    assert!(matches!(
        chronology::date(&world, CYCLE, day(-1, 1, 11), None),
        Err(Error::InvalidInput(_))
    ));
    assert!(chronology::date(&world, CYCLE, day(-4, 1, 11), None).is_ok());
}

#[test]
fn isolated_calendars_format_local_dates_without_inventing_an_axis() {
    let world = make_world(sample());
    let result = chronology::date(&world, ISOLATED, day(i64::MIN, 1, 1), None).unwrap();
    assert_eq!(result["date"]["year"], i64::MIN.to_string());
    assert_eq!(result["axisDay"], Json::Null);
    assert_eq!(result["axisReason"], "no_epoch");
    assert_eq!(result["found"], true);
    for (from, to) in [(ISOLATED, CYCLE), (CYCLE, ISOLATED)] {
        let result = chronology::convert(&world, from, to, day(0, 1, 1)).unwrap();
        assert_eq!(result["found"], false);
        assert_eq!(result["reason"], "no_epoch");
    }
    assert!(matches!(
        chronology::convert(&world, ISOLATED, CYCLE, day(0, 1, 0)),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn public_axis_overflow_is_explicit_while_local_formatting_still_works() {
    let world = make_world(sample());
    for year in [i64::MIN, i64::MAX] {
        let result = chronology::date(&world, CYCLE, day(year, 1, 1), None).unwrap();
        assert_eq!(result["found"], true);
        assert_eq!(result["axisDay"], Json::Null);
        assert_eq!(result["axisReason"], "overflow");
        assert!(matches!(
            chronology::convert(&world, CYCLE, SECOND, day(year, 1, 1)),
            Err(Error::InvalidInput(_))
        ));
    }
    // The large absolute ordinal cancels its explicit epoch with i128 arithmetic.
    let mut source = sample();
    source["calendars"][0]["epoch"] =
        json!({"civil":{"year":i64::MAX,"month":1,"day":1},"axis_day":i64::MIN});
    let anchored = make_world(source);
    valid(&anchored);
    assert_eq!(
        chronology::date(&anchored, CYCLE, day(i64::MAX, 1, 1), None).unwrap()["axisDay"],
        i64::MIN.to_string()
    );
}

#[test]
fn explicit_epochs_can_start_inside_a_negative_or_positive_year() {
    let mut value = sample();
    value["calendars"][0]["epoch"] = json!({"civil":{"year":7,"month":1,"day":2},"axis_day":100});
    let world = make_world(value);
    assert_eq!(
        chronology::date(&world, CYCLE, day(7, 1, 2), None).unwrap()["axisDay"],
        "100"
    );
    assert_eq!(
        chronology::date(&world, CYCLE, day(-1, 1, 1), None).unwrap()["axisDay"],
        "17"
    );
    let converted = chronology::convert(&world, CYCLE, SECOND, day(7, 1, 2)).unwrap();
    assert_eq!(civil(&converted["target"]), day(20, 1, 1));
}

#[test]
fn table_calendars_use_only_authored_rows_and_intercalary_months() {
    let mut value = sample();
    value["calendars"][1]["rule"] = json!({"kind":"table","years":[
        {"year":0,"overrides":[{"intercalary_month":{"number":13,"days":2,"label":"Feast"}}]},
        {"year":100000000000_i64,"overrides":[]}
    ]});
    let world = make_world(value);
    valid(&world);
    let result = chronology::convert(&world, SECOND, CYCLE, day(0, 13, 2)).unwrap();
    assert_eq!(result["axisDay"], "6");
    assert_eq!(
        chronology::date(&world, SECOND, day(100000000000, 1, 1), None).unwrap()["axisDay"],
        "7"
    );
    assert_eq!(
        chronology::convert(&world, SECOND, CYCLE, day(1, 1, 1)).unwrap()["reason"],
        "table_gap"
    );
    assert_eq!(
        chronology::convert(&world, CYCLE, SECOND, day(-1, 1, 1)).unwrap()["reason"],
        "disconnected_table"
    );
    assert_eq!(
        chronology::convert(&world, CYCLE, SECOND, day(2, 1, 1)).unwrap()["reason"],
        "disconnected_table"
    );
    assert!(matches!(
        chronology::date(&world, SECOND, day(100000000000, 13, 1), None),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn eras_skip_year_zero_and_enforce_inclusive_bounds() {
    let world = make_world(sample());
    for (machine, display) in [(-2, -2), (-1, -1), (0, 1), (1, 2), (2, 3)] {
        assert_eq!(
            chronology::date(&world, CYCLE, day(machine, 1, 1), Some(ERA)).unwrap()["era"]["year"],
            display.to_string()
        );
    }
    assert!(chronology::date(&world, CYCLE, day(2, 1, 10), Some(ERA)).is_ok());
    assert!(matches!(
        chronology::date(&world, CYCLE, day(3, 1, 1), Some(ERA)),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        chronology::date(&world, SECOND, day(0, 1, 1), Some(ERA)),
        Err(Error::InvalidInput(_))
    ));
    let mut source = sample();
    source["eras"][0]["display_year_zero"] = json!(true);
    source["eras"][0]["display_epoch"]["display_year"] = json!(0);
    assert_eq!(
        chronology::date(&make_world(source), CYCLE, day(0, 1, 1), Some(ERA)).unwrap()["era"]["year"],
        "0"
    );
}

#[test]
fn anchors_match_exact_days_ticks_and_orders_without_interpolation() {
    let world = make_world(sample());
    let matched = chronology::to_story(&world, CYCLE, day(1, 1, 1), None).unwrap();
    assert_eq!(
        matched["storyTime"],
        json!({"timeline":"main","tick":"100","order":"2"})
    );
    assert_eq!(matched["provenance"], json!(["later"]));
    assert_eq!(
        chronology::to_story(&world, CYCLE, day(0, 1, 2), None).unwrap()["reason"],
        "no_match"
    );
    assert_eq!(
        chronology::from_story(&world, CYCLE, &StoryTime::new("main", 100, 0).unwrap()).unwrap()["reason"],
        "no_match"
    );
    let reversed =
        chronology::from_story(&world, CYCLE, &StoryTime::new("main", 100, 2).unwrap()).unwrap();
    assert_eq!(reversed["date"], json!({"year":"1","month":"1","day":"1"}));
    assert!(matches!(
        chronology::to_story(&world, CYCLE, day(0, 1, 1), Some("unknown")),
        Err(Error::InvalidInput(_))
    ));
    assert_eq!(
        chronology::to_story(&world, ISOLATED, day(0, 1, 1), None).unwrap()["reason"],
        "no_epoch"
    );
}

#[test]
fn anchor_aliases_preserve_evidence_and_timelines_remain_independent() {
    let mut value = sample();
    let mut alias = value["anchors"][0].clone();
    alias["id"] = json!("chronology_00000000000000000000000003");
    alias["provenance"] = json!(["copy", "first"]);
    let mut branch = value["anchors"][0].clone();
    branch["id"] = json!("chronology_00000000000000000000000004");
    branch["story_time"] = json!({"timeline":"branch","tick":999,"order":3});
    value["anchors"]
        .as_array_mut()
        .unwrap()
        .extend([alias, branch]);
    let world = make_world(value);
    valid(&world);
    let main = chronology::to_story(&world, CYCLE, day(0, 1, 1), None).unwrap();
    assert_eq!(main["anchorIds"].as_array().unwrap().len(), 2);
    assert_eq!(main["provenance"], json!(["copy", "first"]));
    let branch = chronology::to_story(&world, CYCLE, day(0, 1, 1), Some("branch")).unwrap();
    assert_eq!(branch["storyTime"]["tick"], "999");
}

#[test]
fn contradictory_or_crossed_anchors_are_rejected() {
    for replacement in [
        json!({"id":"chronology_00000000000000000000000001","axis_day":11,"story_time":{"timeline":"main","tick":100,"order":2},"provenance":["x"]}),
        json!({"id":"chronology_00000000000000000000000002","axis_day":0,"story_time":{"timeline":"main","tick":1,"order":0},"provenance":["x"]}),
        json!({"id":"chronology_00000000000000000000000002","axis_day":11,"story_time":{"timeline":"main","tick":0,"order":0},"provenance":["x"]}),
        json!({"id":"chronology_00000000000000000000000002","axis_day":11,"story_time":{"timeline":"main","tick":-1,"order":0},"provenance":["x"]}),
        json!({"id":"chronology_00000000000000000000000002","axis_day":11,"story_time":{"timeline":"missing","tick":1,"order":0},"provenance":["x"]}),
    ] {
        let mut value = sample();
        value["anchors"][1] = replacement;
        invalid(value);
    }
}

#[test]
fn calendar_mechanics_and_closed_declarations_reject_unsupported_values() {
    for replacement in [
        json!({"kind":"cycle","period":0,"overrides":[]}),
        json!({"kind":"cycle","period":501,"overrides":[]}),
        json!({"kind":"cycle","period":4,"overrides":[{"residue":4,"target_month":1,"delta_days":1}]}),
        json!({"kind":"cycle","period":4,"overrides":[{"residue":0,"target_month":1,"delta_days":-10}]}),
        json!({"kind":"cycle","period":4,"overrides":[{"residue":0,"target_month":1,"delta_days":1},{"residue":0,"target_month":1,"delta_days":2}]}),
        json!({"kind":"cycle","period":4,"overrides":[{"residue":0,"intercalary_month":{"number":1,"days":5}}]}),
        json!({"kind":"table","years":[]}),
        json!({"kind":"table","years":[{"year":0,"overrides":[]},{"year":0,"overrides":[]}]}),
        json!({"kind":"gregorian"}),
        json!({"kind":"cycle","period":1,"overrides":[],"typo":true}),
    ] {
        let mut value = sample();
        value["calendars"][0]["rule"] = replacement;
        invalid(value);
    }
    let mut value = sample();
    value["calendars"][0]["epoch"]["civil"]["day"] = json!(12);
    invalid(value);
    let mut value = sample();
    value["eras"][0]["display_epoch"]["display_year"] = json!(0);
    invalid(value);
    let mut value = sample();
    value["eras"][0]["bounds"]["upper"]["year"] = json!(-3);
    invalid(value);
    let mut value = sample();
    value["anchors"][0]["story_time"]["order"] = json!(i64::MAX);
    invalid(value);
    let mut value = sample();
    value["unknown"] = json!(true);
    invalid(value);
}

#[test]
fn typed_declarations_reject_tags_strings_booleans_and_floating_coordinates() {
    for coordinate in [json!(true), json!(1.0), json!("1"), Json::Null] {
        let mut value = sample();
        value["calendars"][0]["months"][0]["days"] = coordinate;
        invalid(value);
    }
    let mut value: Yaml = serde_yaml::to_value(sample()).unwrap();
    value["calendars"][0]["epoch"] = serde_yaml::from_str("!unsupported null").unwrap();
    assert!(!chronology::validate(&world_value(value)).is_empty());
    let mut value: Yaml = serde_yaml::to_value(sample()).unwrap();
    value["calendars"][0]["rule"]["kind"] = serde_yaml::from_str("!unsupported cycle").unwrap();
    assert!(!chronology::validate(&world_value(value)).is_empty());
}

#[test]
fn ignored_extensions_stay_opaque_and_do_not_break_catalogue_serialization() {
    let mut value: Yaml = serde_yaml::to_value(sample()).unwrap();
    value["calendars"][0]["x-private"] =
        serde_yaml::from_str("!unknown {true: hidden-value}").unwrap();
    value["calendars"][0]["months"][0]["x-private"] =
        serde_yaml::from_str("{[1, 2]: hidden-value}").unwrap();
    value["eras"][0]["x-private"] = serde_yaml::from_str("!unknown null").unwrap();
    let world = world_value(value);
    valid(&world);
    let result = chronology::calendars(&world, 100).unwrap();
    assert!(!result.to_string().contains("x-private"));
    assert!(!result.to_string().contains("hidden-value"));
    assert_eq!(
        chronology::date(&world, CYCLE, day(0, 1, 1), None).unwrap()["axisDay"],
        "0"
    );
}

#[test]
fn catalogue_is_complete_sorted_and_requires_exact_identifiers() {
    let mut value = sample();
    value["calendars"].as_array_mut().unwrap().reverse();
    value["calendars"][0]["x-note"] = json!({"anything":true});
    let world = make_world(value);
    valid(&world);
    let catalogue = chronology::calendars(&world, 3).unwrap();
    assert_eq!(catalogue["items"][0]["calendarId"], CYCLE);
    for limit in [0, 2, 1001] {
        assert!(matches!(
            chronology::calendars(&world, limit),
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        chronology::date(&world, "Cycle", day(0, 1, 1), None),
        Err(Error::NotFound(_))
    ));
    assert!(matches!(
        chronology::date(&world, CYCLE, day(0, 1, 1), Some("Reign")),
        Err(Error::NotFound(_))
    ));
    let empty = make_world(json!({"calendars":[],"eras":[],"anchors":[]}));
    assert_eq!(chronology::calendars(&empty, 1).unwrap()["count"], 0);
}
