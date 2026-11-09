//! Date queries keep one exact, author-only contract across CLI, service and GET.
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use wedl::{
    chronology::{AnnotationQuery, DateInput, DatePredicate, DateTarget, EraMatchMode},
    http,
    service::{Operation, Service},
    source,
};

const SOLAR: &str = "calendar_0123456789abcdefghjkmnpqrs";
const REGNAL: &str = "calendar_1123456789abcdefghjkmnpqrs";
const ISOLATED: &str = "calendar_2123456789abcdefghjkmnpqrs";
const ERA: &str = "era_0123456789abcdefghjkmnpqrs";
const ANNALS: &str = "char_0123456789ABCDEFGHJKMNPQRS";
const ERA_ANNOTATION: &str = "chronology_4123456789abcdefghjkmnpqrs";
const APPROX: &str = "chronology_7123456789abcdefghjkmnpqrs";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/chronology_conformance")
}
fn service(root: &Path) -> Service {
    Service::new(source::load_world(root, None).unwrap())
}
fn civil(year: i64, month: Option<u32>, day: Option<u32>) -> DateInput {
    DateInput::Civil {
        calendar: SOLAR.into(),
        year,
        month,
        day,
    }
}
fn era(year: i64, month: Option<u32>, day: Option<u32>) -> DateInput {
    DateInput::Era {
        era: ERA.into(),
        year,
        month,
        day,
    }
}
fn query(predicate: DatePredicate, value: DateInput, upper: Option<DateInput>) -> AnnotationQuery {
    AnnotationQuery {
        predicate,
        value,
        upper,
        record: None,
        calendar: None,
        era: None,
        era_mode: EraMatchMode::Authored,
        limit: 100,
    }
}
fn cli(root: &Path, args: &[&str]) -> (i32, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .args(["--compact", "--repo", root.to_str().unwrap()])
        .args(args)
        .output()
        .unwrap();
    let bytes = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    (
        output.status.code().unwrap(),
        serde_json::from_slice(bytes)
            .unwrap_or_else(|_| panic!("non-JSON: {}", String::from_utf8_lossy(bytes))),
    )
}
async fn get(service: Service, uri: &str) -> (StatusCode, Value) {
    let response = http::router(service)
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("host", "localhost:8765")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
async fn parity(root: &Path, operation: Operation, uri: &str, args: &[&str]) -> Value {
    let service = service(root);
    let expected = service.execute(operation).unwrap();
    let (status, actual) = get(service.clone(), uri).await;
    let (code, command) = cli(root, args);
    assert_eq!(status, StatusCode::OK, "{actual}");
    assert_eq!(code, 0, "{command}");
    assert_eq!(actual, expected);
    assert_eq!(actual, command);
    assert_eq!(actual["protocol"], wedl::CONTRACT);
    assert_eq!(actual["fingerprint"], service.world().fingerprint);
    let result = actual["result"].clone();
    assert_eq!(result["perspective"], "author");
    assert_eq!(result["scope"], "author-all-time-chronology-source");
    assert!(result.get("effectiveTime").is_none());
    result
}
fn encode(text: &str) -> String {
    text.bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

#[tokio::test]
async fn date_search_predicates_and_era_modes_match_all_adapters() {
    let root = root();
    for (name, predicate, year, count) in [
        ("on_date", DatePredicate::OnDate, 0, 3),
        ("before", DatePredicate::Before, 0, 2),
        ("after", DatePredicate::After, 0, 3),
        ("overlaps", DatePredicate::Overlaps, 2, 3),
        ("between", DatePredicate::Between, 0, 5),
    ] {
        let upper = if name == "between" {
            Some(civil(2, None, None))
        } else {
            None
        };
        let year_text = year.to_string();
        let mut args = vec![
            "chronology",
            "search",
            name,
            "--calendar",
            SOLAR,
            "--year",
            &year_text,
        ];
        let mut uri =
            format!("/api/chronology/search?predicate={name}&calendar={SOLAR}&year={year}");
        if upper.is_some() {
            args.extend(["--upper-year", "2"]);
            uri.push_str("&upper_year=2");
        }
        let result = parity(
            &root,
            Operation::ChronologySearch {
                query: query(predicate, civil(year, None, None), upper),
            },
            &uri,
            &args,
        )
        .await;
        assert_eq!(result["count"], count, "{name}");
        assert_eq!(result["complete"], true);
        let has_approx = result["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["annotationId"] == APPROX);
        assert_eq!(has_approx, name == "overlaps");
        if name == "on_date" {
            assert_eq!(result["items"][0]["annotationId"], ERA_ANNOTATION);
            assert_eq!(result["items"][0]["relation"], "exact");
        }
    }
    for (mode_name, mode, count) in [
        ("authored", EraMatchMode::Authored, 1),
        ("overlaps_bounds", EraMatchMode::OverlapsBounds, 2),
    ] {
        let mut request = query(DatePredicate::OnDate, civil(0, None, None), None);
        request.record = Some(ANNALS.into());
        request.calendar = Some(SOLAR.into());
        request.era = Some(ERA.into());
        request.era_mode = mode;
        let result = parity(&root, Operation::ChronologySearch { query: request },
            &format!("/api/chronology/search?predicate=on_date&calendar={SOLAR}&year=0&record={ANNALS}&calendar_filter={SOLAR}&era_filter={ERA}&era_mode={mode_name}"),
            &["chronology", "search", "on_date", "--calendar", SOLAR, "--year", "0", "--record", ANNALS, "--calendar-filter", SOLAR, "--era-filter", ERA, "--era-mode", mode_name]).await;
        // The explicit calendar filter excludes the Regnal feast, even though it shares the axis.
        assert_eq!(result["count"], count);
    }
}

#[tokio::test]
async fn partial_era_formatting_and_exact_conversions_match_all_adapters() {
    let root = root();
    let formatted = parity(
        &root,
        Operation::ChronologyFormat {
            value: era(0, None, None),
        },
        &format!("/api/chronology/format?era={ERA}&year=0"),
        &["chronology", "format", "--era", ERA, "--year", "0"],
    )
    .await;
    assert_eq!(formatted["found"], true);
    assert_eq!(formatted["precision"], "year");
    assert_eq!(
        formatted["axisInterval"],
        json!({"lowerDay":"0","upperDay":"59"})
    );
    assert!(formatted["axisDay"].is_null());
    let converted = parity(
        &root,
        Operation::ChronologyConvertDate {
            value: era(0, Some(1), Some(1)),
            target: DateTarget::Calendar(REGNAL.into()),
        },
        &format!(
            "/api/chronology/convert-date?era={ERA}&year=0&month=1&day=1&target_calendar={REGNAL}"
        ),
        &[
            "chronology",
            "convert-date",
            "--era",
            ERA,
            "--year",
            "0",
            "--month",
            "1",
            "--day",
            "1",
            "--target-calendar",
            REGNAL,
        ],
    )
    .await;
    assert_eq!(converted["found"], true);
    assert_eq!(converted["target"]["civil"]["calendar_id"], REGNAL);
    assert_eq!(converted["axisDay"], "0");
    let target_era = parity(
        &root,
        Operation::ChronologyConvertDate {
            value: civil(-1, Some(1), Some(1)),
            target: DateTarget::Era(ERA.into()),
        },
        &format!(
            "/api/chronology/convert-date?calendar={SOLAR}&year=-1&month=1&day=1&target_era={ERA}"
        ),
        &[
            "chronology",
            "convert-date",
            "--calendar",
            SOLAR,
            "--year",
            "-1",
            "--month",
            "1",
            "--day",
            "1",
            "--target-era",
            ERA,
        ],
    )
    .await;
    assert_eq!(target_era["target"]["era"]["year"], "-1");
    let partial = parity(
        &root,
        Operation::ChronologyConvertDate {
            value: era(0, None, None),
            target: DateTarget::Calendar(SOLAR.into()),
        },
        &format!("/api/chronology/convert-date?era={ERA}&year=0&target_calendar={SOLAR}"),
        &[
            "chronology",
            "convert-date",
            "--era",
            ERA,
            "--year",
            "0",
            "--target-calendar",
            SOLAR,
        ],
    )
    .await;
    assert_eq!(partial["found"], false);
    assert_eq!(partial["reason"], "conversion_exactness");
}

#[tokio::test]
async fn inclusive_interval_anchor_lists_do_not_interpolate() {
    let root = root();
    let result = parity(
        &root,
        Operation::ChronologyStoryTimes {
            value: civil(0, None, None),
            upper: Some(civil(7, None, None)),
            timeline: None,
            limit: 100,
        },
        &format!("/api/chronology/story-times?calendar={SOLAR}&year=0&upper_year=7"),
        &[
            "chronology",
            "story-times",
            "--calendar",
            SOLAR,
            "--year",
            "0",
            "--upper-year",
            "7",
        ],
    )
    .await;
    assert_eq!(result["count"], 2);
    assert_eq!(result["mapping"], "ambiguous");
    assert!(result["timeline"].is_null());
    assert_eq!(result["items"][0]["axisDay"], "0");
    assert_eq!(result["items"][1]["axisDay"], "365");
    assert_eq!(
        result["items"][1]["storyTime"],
        json!({"timeline":"main","tick":"1","order":"0"})
    );
    assert!(
        !result["items"][0]["anchorIds"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let absent = parity(
        &root,
        Operation::ChronologyStoryTimes {
            value: civil(1, None, None),
            upper: None,
            timeline: Some("main".into()),
            limit: 100,
        },
        &format!("/api/chronology/story-times?calendar={SOLAR}&year=1&timeline=main"),
        &[
            "chronology",
            "story-times",
            "--calendar",
            SOLAR,
            "--year",
            "1",
            "--timeline",
            "main",
        ],
    )
    .await;
    assert_eq!(absent["count"], 0);
    assert_eq!(absent["mapping"], "none");
    assert_eq!(absent["reason"], "no_match");
    let unavailable = parity(
        &root,
        Operation::ChronologyStoryTimes {
            value: DateInput::Civil {
                calendar: ISOLATED.into(),
                year: 0,
                month: None,
                day: None,
            },
            upper: None,
            timeline: None,
            limit: 100,
        },
        &format!("/api/chronology/story-times?calendar={ISOLATED}&year=0"),
        &[
            "chronology",
            "story-times",
            "--calendar",
            ISOLATED,
            "--year",
            "0",
        ],
    )
    .await;
    assert_eq!(unavailable["found"], false);
    assert_eq!(unavailable["reason"], "no_epoch");
    let (status, _) = get(
        service(&root),
        &format!("/api/chronology/story-times?calendar={SOLAR}&year=0&upper_year=7&limit=1"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn advanced_json_operands_preserve_uncertainty_and_open_bounds() {
    let root = root();
    let range = json!({"range":{"calendar_id":SOLAR,"lower":null,"upper":{"calendar_id":SOLAR,"year":"0"}}}).to_string();
    let result = parity(
        &root,
        Operation::ChronologySearch {
            query: query(
                DatePredicate::Overlaps,
                DateInput::parse_json(&range).unwrap(),
                None,
            ),
        },
        &format!(
            "/api/chronology/search?predicate=overlaps&value={}",
            encode(&range)
        ),
        &["chronology", "search", "overlaps", "--value", &range],
    )
    .await;
    assert!(result["normalized"]["lowerDay"].is_null());
    assert_eq!(result["normalized"]["upperDay"], "59");
    assert_eq!(result["count"], 5);
    let approximate = json!({"approx":{"display_value":"about year two","bounds":{"calendar_id":SOLAR,"lower":{"calendar_id":SOLAR,"year":"2"},"upper":{"calendar_id":SOLAR,"year":"2"}}}}).to_string();
    let result = parity(
        &root,
        Operation::ChronologySearch {
            query: query(
                DatePredicate::Overlaps,
                DateInput::parse_json(&approximate).unwrap(),
                None,
            ),
        },
        &format!(
            "/api/chronology/search?predicate=overlaps&value={}",
            encode(&approximate)
        ),
        &["chronology", "search", "overlaps", "--value", &approximate],
    )
    .await;
    assert_eq!(result["normalized"]["approximate"], true);
    assert_eq!(result["count"], 3);
    let conflict = json!({"conflict":{"claims":[{"civil":{"calendar_id":SOLAR,"year":"0"}},{"civil":{"calendar_id":SOLAR,"year":"1"}}]}}).to_string();
    let unresolved = parity(
        &root,
        Operation::ChronologySearch {
            query: query(
                DatePredicate::OnDate,
                DateInput::parse_json(&conflict).unwrap(),
                None,
            ),
        },
        &format!(
            "/api/chronology/search?predicate=on_date&value={}",
            encode(&conflict)
        ),
        &["chronology", "search", "on_date", "--value", &conflict],
    )
    .await;
    assert_eq!(unresolved["available"], false);
    assert_eq!(unresolved["reason"], "conflicting_claims");
    assert_eq!(unresolved["items"], json!([]));
    let lower = json!({"civil":{"calendar_id":SOLAR,"year":"0"}}).to_string();
    let upper = json!({"era":{"era_id":ERA,"year":"2"}}).to_string();
    let between = parity(
        &root,
        Operation::ChronologySearch {
            query: query(
                DatePredicate::Between,
                DateInput::parse_json(&lower).unwrap(),
                Some(DateInput::parse_json(&upper).unwrap()),
            ),
        },
        &format!(
            "/api/chronology/search?predicate=between&value={}&upper_value={}",
            encode(&lower),
            encode(&upper)
        ),
        &[
            "chronology",
            "search",
            "between",
            "--value",
            &lower,
            "--upper-value",
            &upper,
        ],
    )
    .await;
    assert_eq!(between["count"], 5);
}

#[tokio::test]
async fn date_query_adapters_reject_ambiguous_lossy_and_horizon_inputs() {
    let root = root();
    let service = service(&root);
    let base = format!("calendar={SOLAR}&year=0");
    let malformed = json!({"civil":{"calendar_id":SOLAR,"year":0}}).to_string();
    for uri in [
        "/api/chronology/format".into(),
        format!("/api/chronology/format?{base}&era={ERA}"),
        format!("/api/chronology/format?{base}&value=%7B%7D"),
        format!("/api/chronology/format?{base}&day=1"),
        format!("/api/chronology/format?calendar={SOLAR}&year=-0"),
        format!("/api/chronology/format?calendar={SOLAR}&year=01"),
        format!("/api/chronology/format?calendar={SOLAR}&year=%2B1"),
        format!("/api/chronology/format?calendar={SOLAR}&year=9223372036854775808"),
        format!("/api/chronology/format?{base}&month=0"),
        format!("/api/chronology/format?{base}&month=01"),
        format!("/api/chronology/format?value={}", encode(&malformed)),
        format!("/api/chronology/format?{base}&tick=0"),
        format!("/api/chronology/format?{base}&character={ANNALS}"),
        format!("/api/chronology/convert-date?{base}"),
        format!("/api/chronology/convert-date?{base}&target_calendar={SOLAR}&target_era={ERA}"),
        format!("/api/chronology/search?{base}"),
        format!("/api/chronology/search?{base}&predicate=equal"),
        format!("/api/chronology/search?{base}&predicate=between"),
        format!("/api/chronology/search?{base}&predicate=on_date&upper_year=1"),
        format!("/api/chronology/search?{base}&predicate=on_date&era_mode=authored"),
        format!(
            "/api/chronology/search?{base}&predicate=on_date&era_filter={ERA}&era_mode=overlaps"
        ),
        format!("/api/chronology/search?{base}&predicate=on_date&calendar_filter="),
        format!("/api/chronology/search?{base}&predicate=on_date&limit=1"),
        format!("/api/chronology/story-times?{base}&upper_month=1"),
        format!("/api/chronology/story-times?{base}&upper_year=2&upper_value=%7B%7D"),
        format!("/api/chronology/story-times?{base}&limit=1001"),
        format!("/api/chronology/story-times?{base}&view=character"),
        format!("/api/chronology/search?{base}&predicate=on_date&predicate=on_date"),
    ] {
        let (status, value) = get(service.clone(), &uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {value}");
        assert!(value.get("result").is_none());
    }
    for args in [
        vec!["chronology", "format", "--calendar", SOLAR, "--year", "-0"],
        vec![
            "chronology",
            "format",
            "--calendar",
            SOLAR,
            "--era",
            ERA,
            "--year",
            "0",
        ],
        vec!["chronology", "format", "--value", &malformed],
        vec![
            "chronology",
            "search",
            "on_date",
            "--calendar",
            SOLAR,
            "--year",
            "0",
            "--era-mode",
            "authored",
        ],
        vec![
            "chronology",
            "story-times",
            "--calendar",
            SOLAR,
            "--year",
            "0",
            "--upper-month",
            "1",
        ],
        vec![
            "chronology",
            "format",
            "--calendar",
            SOLAR,
            "--year",
            "0",
            "--tick",
            "0",
        ],
    ] {
        assert_ne!(cli(&root, &args).0, 0, "{args:?}");
    }
}

#[tokio::test]
async fn large_date_and_anchor_integers_remain_exact_through_adapters() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("story")).unwrap();
    let calendar = "calendar_00000000000000000000000001";
    fs::write(temp.path().join("story/world.md"), format!("---\nschema: wedl/v1\nkind: world\nid: world_00000000000000000000000000\ntitle: Large exact date queries\ncapabilities: []\ndefault_timeline: main\ntimelines: [{{id: main, label: Main}}]\nstate_keys: {{}}\nchronology:\n  calendars:\n    - id: {calendar}\n      label: One day\n      months: [{{number: 1, days: 1}}]\n      rule: {{kind: cycle, period: 1, overrides: []}}\n      epoch: {{civil: {{year: 0, month: 1, day: 1}}, axis_day: 0}}\n  eras: []\n  anchors:\n    - id: chronology_00000000000000000000000001\n      axis_day: 9007199254740993\n      story_time: {{timeline: main, tick: 9007199254740995, order: 2147483647}}\n      provenance: [exact-ledger]\n---\n")).unwrap();
    let date = DateInput::Civil {
        calendar: calendar.into(),
        year: 9007199254740993,
        month: Some(1),
        day: Some(1),
    };
    let result = parity(
        temp.path(),
        Operation::ChronologyStoryTimes {
            value: date,
            upper: None,
            timeline: None,
            limit: 100,
        },
        &format!(
            "/api/chronology/story-times?calendar={calendar}&year=9007199254740993&month=1&day=1"
        ),
        &[
            "chronology",
            "story-times",
            "--calendar",
            calendar,
            "--year",
            "9007199254740993",
            "--month",
            "1",
            "--day",
            "1",
        ],
    )
    .await;
    assert_eq!(result["mapping"], "unique");
    assert_eq!(result["items"][0]["axisDay"], "9007199254740993");
    assert_eq!(result["items"][0]["storyTime"]["tick"], "9007199254740995");
    assert_eq!(result["items"][0]["storyTime"]["order"], "2147483647");
}
