//! Exact calendar and anchor reads have one contract across library, CLI and HTTP.
use std::{
    path::{Path, PathBuf},
    process::Command,
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;
use wedl::{
    chronology::CivilDate,
    http,
    service::{Operation, Service, TimeInput},
    source,
};

const SOLAR: &str = "calendar_0123456789abcdefghjkmnpqrs";
const REGNAL: &str = "calendar_1123456789abcdefghjkmnpqrs";
const ISOLATED: &str = "calendar_2123456789abcdefghjkmnpqrs";
const ERA: &str = "era_0123456789abcdefghjkmnpqrs";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/chronology_conformance")
}
fn service(root: &Path) -> Service {
    Service::new(source::load_world(root, None).unwrap())
}
fn civil(year: i64, month: u32, day: u32) -> CivilDate {
    CivilDate { year, month, day }
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
    let value = serde_json::from_slice(bytes)
        .unwrap_or_else(|_| panic!("non-JSON output: {}", String::from_utf8_lossy(bytes)));
    (output.status.code().unwrap(), value)
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
    assert_eq!(actual["result"]["perspective"], "author");
    assert_eq!(
        actual["result"]["scope"],
        "author-all-time-chronology-source"
    );
    actual["result"].clone()
}

#[tokio::test]
async fn calendar_date_era_and_conversion_reads_match_all_adapters() {
    let root = root();
    let catalogue = parity(
        &root,
        Operation::ChronologyCalendars { limit: 100 },
        "/api/chronology/calendars",
        &["chronology", "calendars"],
    )
    .await;
    assert_eq!(catalogue["items"].as_array().unwrap().len(), 3);
    assert_eq!(catalogue["items"][0]["calendarId"], SOLAR);

    let negative = parity(
        &root,
        Operation::ChronologyDate {
            calendar: SOLAR.into(),
            value: civil(-249, 1, 1),
            era: Some(ERA.into()),
        },
        &format!("/api/chronology/date?calendar={SOLAR}&year=-249&month=1&day=1&era={ERA}"),
        &[
            "chronology",
            "date",
            SOLAR,
            "--year",
            "-249",
            "--month",
            "1",
            "--day",
            "1",
            "--era",
            ERA,
        ],
    )
    .await;
    assert_eq!(
        negative["date"],
        json!({"year":"-249","month":"1","day":"1"})
    );
    assert_eq!(negative["era"]["year"], "-249");
    assert!(
        negative["formatted"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
    );

    let converted = parity(
        &root,
        Operation::ChronologyConvert {
            from: SOLAR.into(),
            to: REGNAL.into(),
            value: civil(0, 2, 4),
        },
        &format!("/api/chronology/convert?from={SOLAR}&to={REGNAL}&year=0&month=2&day=4"),
        &[
            "chronology",
            "convert",
            SOLAR,
            REGNAL,
            "--year",
            "0",
            "--month",
            "2",
            "--day",
            "4",
        ],
    )
    .await;
    assert_eq!(converted["found"], true);
    assert_eq!(
        converted["target"],
        json!({"year":"0","month":"13","day":"5"})
    );
    assert_eq!(converted["axisDay"], "34");

    let isolated = parity(
        &root,
        Operation::ChronologyDate {
            calendar: ISOLATED.into(),
            value: civil(-1, 1, 2),
            era: None,
        },
        &format!("/api/chronology/date?calendar={ISOLATED}&year=-1&month=1&day=2"),
        &[
            "chronology",
            "date",
            ISOLATED,
            "--year",
            "-1",
            "--month",
            "1",
            "--day",
            "2",
        ],
    )
    .await;
    assert!(isolated["axisDay"].is_null());
    assert_eq!(isolated["axisReason"], "no_epoch");
    let no_epoch = parity(
        &root,
        Operation::ChronologyConvert {
            from: ISOLATED.into(),
            to: SOLAR.into(),
            value: civil(0, 1, 1),
        },
        &format!("/api/chronology/convert?from={ISOLATED}&to={SOLAR}&year=0&month=1&day=1"),
        &[
            "chronology",
            "convert",
            ISOLATED,
            SOLAR,
            "--year",
            "0",
            "--month",
            "1",
            "--day",
            "1",
        ],
    )
    .await;
    assert_eq!(no_epoch["found"], false);
    assert_eq!(no_epoch["reason"], "no_epoch");
}

#[tokio::test]
async fn anchors_match_exact_dates_and_story_orders_without_interpolation() {
    let root = root();
    let anchor = parity(
        &root,
        Operation::ChronologyToStory {
            calendar: SOLAR.into(),
            value: civil(0, 1, 1),
            timeline: None,
        },
        &format!("/api/chronology/to-story?calendar={SOLAR}&year=0&month=1&day=1"),
        &[
            "chronology",
            "to-story",
            SOLAR,
            "--year",
            "0",
            "--month",
            "1",
            "--day",
            "1",
        ],
    )
    .await;
    assert_eq!(anchor["found"], true);
    assert_eq!(
        anchor["storyTime"],
        json!({"timeline":"main","tick":"0","order":"0"})
    );
    assert_eq!(anchor["anchorIds"].as_array().unwrap().len(), 1);

    let reverse = parity(
        &root,
        Operation::ChronologyFromStory {
            calendar: SOLAR.into(),
            at: TimeInput {
                tick: 1,
                order: 0,
                timeline: Some("main".into()),
            },
        },
        &format!("/api/chronology/from-story?calendar={SOLAR}&tick=1&timeline=main"),
        &[
            "chronology",
            "from-story",
            SOLAR,
            "--tick",
            "1",
            "--timeline",
            "main",
        ],
    )
    .await;
    assert_eq!(reverse["found"], true);
    assert_eq!(reverse["axisDay"], "365");
    assert_eq!(reverse["date"], json!({"year":"6","month":"1","day":"10"}));
    for (uri, args) in [
        (
            format!("/api/chronology/to-story?calendar={SOLAR}&year=0&month=1&day=2"),
            vec![
                "chronology",
                "to-story",
                SOLAR,
                "--year",
                "0",
                "--month",
                "1",
                "--day",
                "2",
            ],
        ),
        (
            format!("/api/chronology/from-story?calendar={SOLAR}&tick=0&order=1"),
            vec![
                "chronology",
                "from-story",
                SOLAR,
                "--tick",
                "0",
                "--order",
                "1",
            ],
        ),
    ] {
        let (status, result) = get(service(&root), &uri).await;
        let (code, command) = cli(&root, &args);
        assert_eq!(status, StatusCode::OK);
        assert_eq!(code, 0);
        assert_eq!(result, command);
        assert_eq!(result["result"]["found"], false);
        assert_eq!(result["result"]["reason"], "no_match");
    }
}

fn exact_fixture() -> TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("story")).unwrap();
    std::fs::write(
        directory.path().join("story/world.md"),
        format!(
            r#"---
schema: wedl/v1
kind: world
id: world_00000000000000000000000000
title: Exact dates
capabilities: []
default_timeline: main
timelines: [{{id: main, label: Main}}]
state_keys: {{}}
chronology:
  calendars:
  - id: {SOLAR}
    label: One-day years
    months: [{{number: 1, days: 1}}]
    rule: {{kind: cycle, period: 1, overrides: []}}
    epoch: {{civil: {{year: 0, month: 1, day: 1}}, axis_day: 0}}
  eras: []
  anchors:
  - id: chronology_0123456789abcdefghjkmnpqrs
    axis_day: 9007199254740993
    story_time: {{timeline: main, tick: 9007199254740993, order: -2147483648}}
    provenance: [exact-source]
---
"#
        ),
    )
    .unwrap();
    directory
}

#[tokio::test]
async fn chronology_transports_preserve_large_and_negative_integer_coordinates() {
    let fixture = exact_fixture();
    let root = fixture.path();
    let value = parity(
        root,
        Operation::ChronologyDate {
            calendar: SOLAR.into(),
            value: civil(9_007_199_254_740_993, 1, 1),
            era: None,
        },
        &format!("/api/chronology/date?calendar={SOLAR}&year=9007199254740993&month=1&day=1"),
        &[
            "chronology",
            "date",
            SOLAR,
            "--year",
            "9007199254740993",
            "--month",
            "1",
            "--day",
            "1",
        ],
    )
    .await;
    assert_eq!(value["date"]["year"], "9007199254740993");
    assert_eq!(value["axisDay"], "9007199254740993");
    let value = parity(
        root,
        Operation::ChronologyFromStory {
            calendar: SOLAR.into(),
            at: TimeInput {
                tick: 9_007_199_254_740_993,
                order: i32::MIN,
                timeline: None,
            },
        },
        &format!(
            "/api/chronology/from-story?calendar={SOLAR}&tick=9007199254740993&order=-2147483648"
        ),
        &[
            "chronology",
            "from-story",
            SOLAR,
            "--tick",
            "9007199254740993",
            "--order",
            "-2147483648",
        ],
    )
    .await;
    assert_eq!(value["found"], true);
    assert_eq!(value["date"]["year"], "9007199254740993");
    let value = parity(
        root,
        Operation::ChronologyDate {
            calendar: SOLAR.into(),
            value: civil(i64::MIN, 1, 1),
            era: None,
        },
        &format!("/api/chronology/date?calendar={SOLAR}&year=-9223372036854775808&month=1&day=1"),
        &[
            "chronology",
            "date",
            SOLAR,
            "--year",
            "-9223372036854775808",
            "--month",
            "1",
            "--day",
            "1",
        ],
    )
    .await;
    assert_eq!(value["axisDay"], "-9223372036854775808");
    assert!(!root.join(".wedl-rust").exists());
}

#[tokio::test]
async fn chronology_queries_reject_malformed_coordinates_and_unrelated_parameters() {
    let root = root();
    let service = service(&root);
    for base in [
        "/api/chronology/calendars?limit=100".into(),
        format!("/api/chronology/date?calendar={SOLAR}&year=0&month=1&day=1"),
        format!("/api/chronology/convert?from={SOLAR}&to={REGNAL}&year=0&month=1&day=1"),
        format!("/api/chronology/to-story?calendar={SOLAR}&year=0&month=1&day=1"),
        format!("/api/chronology/from-story?calendar={SOLAR}&tick=0"),
    ] {
        for extra in ["character=someone", "view=author", "unexpected=x"] {
            let uri = format!("{base}&{extra}");
            let (status, error) = get(service.clone(), &uri).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {error}");
            assert_eq!(error["code"], "usage_error");
        }
    }
    for year in [
        "%2B1",
        "-0",
        "01",
        "1.0",
        "1e2",
        "true",
        "9223372036854775808",
        "",
        "%201",
    ] {
        let uri = format!("/api/chronology/date?calendar={SOLAR}&year={year}&month=1&day=1");
        let (status, error) = get(service.clone(), &uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {error}");
        assert_eq!(error["code"], "usage_error");
    }
    for query in [
        "month=0&day=1",
        "month=-1&day=1",
        "month=01&day=1",
        "month=1&day=0",
        "month=1&day=1.0",
        "month=1&day=4294967296",
        "month=1",
        "month=1&day=1&day=2",
        "month=1&day=1&tick=0",
        "month=1&day=1&timeline=main",
        "month=1&day=1&era=",
    ] {
        let uri = format!("/api/chronology/date?calendar={SOLAR}&year=0&{query}");
        let (status, error) = get(service.clone(), &uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {error}");
        assert_eq!(error["code"], "usage_error");
    }
    for uri in [
        "/api/chronology/calendars?limit=0".into(),
        "/api/chronology/calendars?limit=1001".into(),
        format!("/api/chronology/from-story?calendar={SOLAR}&tick=0&order=2147483648"),
        format!("/api/chronology/from-story?calendar={SOLAR}&tick=0&order=-0"),
        format!("/api/chronology/from-story?calendar={SOLAR}&tick=01"),
        format!("/api/chronology/from-story?calendar={SOLAR}&tick=0&timeline="),
        format!("/api/chronology/from-story?calendar={SOLAR}&tick=0&year=0"),
    ] {
        let (status, error) = get(service.clone(), &uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {error}");
        assert_eq!(error["code"], "usage_error");
    }
    for year in ["+1", "-0", "01", "1.0", "9223372036854775808"] {
        let (code, error) = cli(
            &root,
            &[
                "chronology",
                "date",
                SOLAR,
                "--year",
                year,
                "--month",
                "1",
                "--day",
                "1",
            ],
        );
        assert_eq!(code, 2, "accepted {year}");
        assert_eq!(error["code"], "usage_error");
    }
    for args in [
        vec![
            "chronology",
            "date",
            SOLAR,
            "--year",
            "0",
            "--month",
            "1",
            "--day",
            "1",
            "--tick",
            "0",
        ],
        vec![
            "chronology",
            "to-story",
            SOLAR,
            "--year",
            "0",
            "--month",
            "1",
            "--day",
            "1",
            "--order",
            "0",
        ],
        vec![
            "chronology",
            "from-story",
            SOLAR,
            "--tick",
            "0",
            "--order",
            "2147483648",
        ],
    ] {
        let (code, error) = cli(&root, &args);
        assert_eq!(code, 2);
        assert_eq!(error["code"], "usage_error");
    }
}
