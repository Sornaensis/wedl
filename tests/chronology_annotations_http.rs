//! Stored chronology annotations share one author contract across all adapters.
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
    http,
    service::{Operation, Service},
    source,
};

const WORLD: &str = "world_0123456789ABCDEFGHJKMNPQRS";
const ANNALS: &str = "char_0123456789ABCDEFGHJKMNPQRS";
const RANGES: &str = "char_1123456789ABCDEFGHJKMNPQRS";
const UNCERTAIN: &str = "char_2123456789ABCDEFGHJKMNPQRS";
const RELATIVE: &str = "char_3123456789ABCDEFGHJKMNPQRS";
const EMPTY: &str = "char_4123456789ABCDEFGHJKMNPQRS";
const SOLAR: &str = "calendar_0123456789abcdefghjkmnpqrs";
const REGNAL: &str = "calendar_1123456789abcdefghjkmnpqrs";
const ERA: &str = "era_0123456789abcdefghjkmnpqrs";
const FIRST: &str = "chronology_2123456789abcdefghjkmnpqrs";
const LAST: &str = "chronology_3123456789abcdefghjkmnpqrs";
const ERA_DATE: &str = "chronology_4123456789abcdefghjkmnpqrs";
const LEAP: &str = "chronology_C123456789abcdefghjkmnpqrs";
const FEAST: &str = "chronology_D123456789abcdefghjkmnpqrs";
const RANGE: &str = "chronology_E123456789abcdefghjkmnpqrs";
const APPROX: &str = "chronology_7123456789abcdefghjkmnpqrs";
const CONFLICT: &str = "chronology_8123456789abcdefghjkmnpqrs";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/chronology_conformance")
}
fn service(root: &Path) -> Service {
    Service::new(source::load_world(root, None).unwrap())
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
        .unwrap_or_else(|_| panic!("non-JSON response: {}", String::from_utf8_lossy(bytes)));
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
    assert!(actual["result"].get("effectiveTime").is_none());
    actual["result"].clone()
}
fn list(record: Option<&str>, calendar: Option<&str>, era: Option<&str>) -> Operation {
    Operation::ChronologyAnnotations {
        record: record.map(str::to_owned),
        calendar: calendar.map(str::to_owned),
        era: era.map(str::to_owned),
        limit: 100,
    }
}

#[tokio::test]
async fn stored_annotations_and_reference_filters_match_all_adapters() {
    let root = root();
    let all = parity(
        &root,
        list(None, None, None),
        "/api/chronology/annotations",
        &["chronology", "annotations"],
    )
    .await;
    let items = all["items"].as_array().unwrap();
    assert_eq!(items.len(), 13);
    assert_eq!(all["count"], 13);
    assert_eq!(all["complete"], true);
    assert!(items.windows(2).all(|pair| {
        (
            pair[0]["recordId"].as_str().unwrap(),
            pair[0]["annotationId"].as_str().unwrap(),
        ) < (
            pair[1]["recordId"].as_str().unwrap(),
            pair[1]["annotationId"].as_str().unwrap(),
        )
    }));
    let era = items
        .iter()
        .find(|item| item["annotationId"] == ERA_DATE)
        .unwrap();
    assert_eq!(era["recordId"], ANNALS);
    assert_eq!(era["normalized"]["precision"], "year");
    assert_eq!(era["normalized"]["basis"], "axis");
    assert_eq!(era["normalized"]["lowerDay"], "0");
    assert_eq!(era["normalized"]["upperDay"], "59");

    let by_name = parity(
        &root,
        list(Some("Yearly Annals"), None, None),
        "/api/chronology/annotations?record=Yearly%20Annals",
        &["chronology", "annotations", "Yearly Annals"],
    )
    .await;
    assert_eq!(by_name["items"].as_array().unwrap().len(), 5);
    let solar = parity(
        &root,
        list(None, Some(SOLAR), None),
        &format!("/api/chronology/annotations?calendar={SOLAR}"),
        &["chronology", "annotations", "--calendar", SOLAR],
    )
    .await;
    assert_eq!(solar["items"].as_array().unwrap().len(), 9);
    assert!(
        solar["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["annotationId"] == CONFLICT)
    );
    let by_era = parity(
        &root,
        list(Some(ANNALS), Some(SOLAR), Some(ERA)),
        &format!("/api/chronology/annotations?record={ANNALS}&calendar={SOLAR}&era={ERA}"),
        &[
            "chronology",
            "annotations",
            ANNALS,
            "--calendar",
            SOLAR,
            "--era",
            ERA,
        ],
    )
    .await;
    assert_eq!(by_era["items"].as_array().unwrap().len(), 1);
    assert_eq!(by_era["items"][0]["annotationId"], ERA_DATE);
    let disjoint = parity(
        &root,
        list(None, Some(REGNAL), Some(ERA)),
        &format!("/api/chronology/annotations?calendar={REGNAL}&era={ERA}"),
        &[
            "chronology",
            "annotations",
            "--calendar",
            REGNAL,
            "--era",
            ERA,
        ],
    )
    .await;
    assert_eq!(disjoint["count"], 0);
    let empty = parity(
        &root,
        list(Some(EMPTY), None, None),
        &format!("/api/chronology/annotations?record={EMPTY}"),
        &["chronology", "annotations", EMPTY],
    )
    .await;
    assert_eq!(empty["items"], json!([]));
}

#[tokio::test]
async fn pair_relations_preserve_partial_dates_and_uncertain_claims() {
    let root = root();
    for (left_record, left, right_record, right, relation, reason) in [
        (ANNALS, FIRST, ANNALS, LAST, "before", None),
        (ANNALS, LEAP, ANNALS, FEAST, "after", None),
        (ANNALS, LEAP, ANNALS, LEAP, "equal", None),
        (ANNALS, ERA_DATE, ANNALS, ERA_DATE, "overlaps", None),
        (RANGES, RANGE, RANGES, APPROX, "overlaps", None),
        (
            RANGES,
            APPROX,
            ANNALS,
            FIRST,
            "unknown",
            Some("approximate_only"),
        ),
        (
            UNCERTAIN,
            CONFLICT,
            ANNALS,
            LEAP,
            "unknown",
            Some("conflicting_claims"),
        ),
        (
            RELATIVE,
            "chronology_A123456789abcdefghjkmnpqrs",
            ANNALS,
            LEAP,
            "unknown",
            Some("relative"),
        ),
        (
            RELATIVE,
            "chronology_B123456789abcdefghjkmnpqrs",
            ANNALS,
            LEAP,
            "unknown",
            Some("duration"),
        ),
    ] {
        let operation = Operation::ChronologyCompare {
            left_record: left_record.into(),
            left_id: left.into(),
            right_record: right_record.into(),
            right_id: right.into(),
        };
        let uri = format!(
            "/api/chronology/compare?left_record={left_record}&left_id={left}&right_record={right_record}&right_id={right}"
        );
        let result = parity(
            &root,
            operation,
            &uri,
            &[
                "chronology",
                "compare",
                left_record,
                left,
                right_record,
                right,
            ],
        )
        .await;
        assert_eq!(result["relation"], relation, "{result}");
        assert_eq!(result["reason"], json!(reason), "{result}");
        assert_eq!(result["left"]["annotationId"], left);
        assert_eq!(result["right"]["annotationId"], right);
    }
}

#[tokio::test]
async fn annotation_routes_reject_horizons_bad_selectors_and_incomplete_results() {
    let service = service(&root());
    let comparison = format!(
        "/api/chronology/compare?left_record={ANNALS}&left_id={LEAP}&right_record={ANNALS}&right_id={FEAST}"
    );
    for uri in [
        "/api/chronology/annotations?tick=0".to_owned(),
        "/api/chronology/annotations?character=char_0123456789ABCDEFGHJKMNPQRS".into(),
        "/api/chronology/annotations?record=".into(),
        "/api/chronology/annotations?record=%20".into(),
        "/api/chronology/annotations?calendar=".into(),
        "/api/chronology/annotations?era=".into(),
        "/api/chronology/annotations?limit=0".into(),
        "/api/chronology/annotations?limit=1001".into(),
        "/api/chronology/annotations?limit=1".into(),
        "/api/chronology/annotations?limit=100&limit=100".into(),
        format!("/api/chronology/annotations?record={WORLD}"),
        "/api/chronology/compare".into(),
        format!("{comparison}&order=0"),
        format!("{comparison}&view=character"),
        format!("{comparison}&left_id={LEAP}"),
    ] {
        let (status, value) = get(service.clone(), &uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {value}");
        assert!(value.get("result").is_none());
    }
    for uri in [
        "/api/chronology/annotations?calendar=calendar_missing".to_owned(),
        "/api/chronology/annotations?era=era_missing".into(),
        "/api/chronology/annotations?record=Missing".into(),
        format!(
            "/api/chronology/compare?left_record={RANGES}&left_id={LEAP}&right_record={ANNALS}&right_id={FEAST}"
        ),
        format!(
            "/api/chronology/compare?left_record={ANNALS}&left_id={}&right_record={ANNALS}&right_id={FEAST}",
            LEAP.to_uppercase()
        ),
    ] {
        let (status, value) = get(service.clone(), &uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {value}");
    }
    for args in [
        vec!["chronology", "annotations", "--tick", "0"],
        vec!["chronology", "annotations", "--limit", "1"],
        vec!["chronology", "annotations", "--calendar", ""],
        vec!["chronology", "compare", ANNALS, LEAP, ANNALS],
    ] {
        let (code, value) = cli(&root(), &args);
        assert_ne!(code, 0, "{value}");
    }
}

#[tokio::test]
async fn annotation_integer_values_stay_exact_through_cli_and_http() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("story")).unwrap();
    let calendar = "calendar_00000000000000000000000001";
    let record = "char_00000000000000000000000001";
    fs::write(temp.path().join("story/world.md"), format!("---\nschema: wedl/v1\nkind: world\nid: world_00000000000000000000000000\ntitle: Exact annotation integers\ncapabilities: []\ndefault_timeline: main\ntimelines: [{{id: main, label: Main}}]\nstate_keys: {{}}\nchronology:\n  calendars:\n    - id: {calendar}\n      label: One day\n      months: [{{number: 1, days: 1}}]\n      rule: {{kind: cycle, period: 1, overrides: []}}\n      epoch: {{civil: {{year: 0, month: 1, day: 1}}, axis_day: 0}}\n  eras: []\n  anchors: []\n---\n")).unwrap();
    fs::write(temp.path().join("story/character.md"), format!("---\nschema: wedl/v1\nkind: character\nid: {record}\ntitle: Exact dates\nchronology:\n  - id: chronology_00000000000000000000000001\n    provenance: [ledger]\n    value: {{civil: {{calendar_id: {calendar}, year: 9007199254740993, month: 1, day: 1}}}}\n    x-private: !custom ignored-extension\n  - id: chronology_00000000000000000000000002\n    provenance: [ledger]\n    value: {{duration: {{value: 9223372036854775807, unit: day}}}}\n---\n")).unwrap();
    let result = parity(
        temp.path(),
        list(Some(record), None, None),
        &format!("/api/chronology/annotations?record={record}"),
        &["chronology", "annotations", record],
    )
    .await;
    assert_eq!(
        result["items"][0]["value"]["civil"]["year"],
        "9007199254740993"
    );
    assert_eq!(
        result["items"][0]["normalized"]["lowerDay"],
        "9007199254740993"
    );
    assert_eq!(
        result["items"][1]["value"]["duration"]["value"],
        "9223372036854775807"
    );
    assert!(!result.to_string().contains("ignored-extension"));
    assert!(!result.to_string().contains("x-private"));
}
