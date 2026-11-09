//! Author spatial operations share one CLI/HTTP boundary with no story horizon.
use std::{
    path::{Path, PathBuf},
    process::Command,
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;
use wedl::{
    http,
    model::World,
    service::{Operation, Service},
    source, validation,
};

const TOWN: &str = "location:town";
const GATE: &str = "location:town/gate";
const PIER: &str = "location:town/pier";
const ISLAND: &str = "location:island";

struct Fixture {
    _directory: TempDir,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_path_buf();
        std::fs::create_dir(root.join("story")).unwrap();
        let records = [
            ("world", "world_00000000000000000000000000", "Spatial world", "capabilities: [spatial-core-v1, route-v1]\ndefault_timeline: main\ntimelines: [{id: main, label: Main}]\nstate_keys: {}\n".to_owned()),
            ("location", TOWN, "Town", "parent_id: null\n".into()),
            ("location", GATE, "North Gate", format!("parent_id: {TOWN}\n")),
            ("location", PIER, "Pier", format!("parent_id: {TOWN}\n")),
            ("location", ISLAND, "Island", "".into()),
            ("route", "route:town/gate-pier", "Gate to Pier", format!("from_location_id: {GATE}\nto_location_id: {PIER}\ndirection: two-way\nmodes: [foot]\navailability: open\nduration: {{value: 9007199254740993, unit: seconds}}\n")),
            ("route", "route:town/closed-crossing", "Closed Crossing", format!("from_location_id: {PIER}\nto_location_id: {ISLAND}\ndirection: one-way\nmodes: [foot]\navailability: closed\n")),
        ];
        for (index, (kind, id, title, fields)) in records.iter().enumerate() {
            let raw = format!(
                "---\nschema: {}\nkind: {kind}\nid: {id}\ntitle: {title}\nstatus: canonical\naliases: []\ntags: []\n{fields}---\n",
                source::SOURCE_SCHEMA
            );
            std::fs::write(root.join(format!("story/record-{index}.md")), raw).unwrap();
        }
        let value = Self {
            _directory: directory,
            root,
        };
        let report = validation::validate_world(&value.world());
        assert!(report.valid, "{:?}", report.diagnostics);
        value
    }

    fn world(&self) -> World {
        source::load_world(&self.root, None).unwrap()
    }
    fn service(&self) -> Service {
        Service::new(self.world())
    }
    fn cli(&self, args: &[&str]) -> (i32, Value) {
        cli(&self.root, args)
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
    assert_eq!(response.headers()["cache-control"], "no-store");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

fn author_envelope(value: &Value, world: &World) {
    assert_eq!(value["protocol"], wedl::CONTRACT);
    assert_eq!(value["fingerprint"], world.fingerprint);
    assert!(value.get("revision").is_some());
    assert_eq!(value["result"]["perspective"], "author");
    assert_eq!(value["result"]["scope"], "author-all-time-spatial-source");
    assert!(value["result"].get("effectiveTime").is_none());
}

#[tokio::test]
async fn namespaced_location_and_path_reads_match_service_http_and_cli() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let expected = service
        .execute(Operation::SpatialLocation {
            entity: GATE.into(),
            mode: Some("foot".into()),
            limit: 100,
        })
        .unwrap();
    let (status, actual) = get(
        service.clone(),
        "/api/spatial/location?id=location%3Atown%2Fgate&mode=foot&limit=100",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(actual, expected);
    let (code, command) = fixture.cli(&[
        "spatial", "location", GATE, "--mode", "foot", "--limit", "100",
    ]);
    assert_eq!(code, 0, "{command}");
    assert_eq!(command, actual);
    author_envelope(&actual, &fixture.world());
    assert_eq!(actual["result"]["ancestors"][0]["locationId"], TOWN);
    assert_eq!(
        actual["result"]["outgoingRoutes"][0]["duration"]["value"],
        "9007199254740993"
    );

    let expected = service
        .execute(Operation::SpatialPath {
            from: GATE.into(),
            to: PIER.into(),
            mode: Some("foot".into()),
            metric: "hops".into(),
            unit: None,
            limit: 100,
        })
        .unwrap();
    let (status, actual) = get(service, "/api/spatial/path?from=location%3Atown%2Fgate&to=location%3Atown%2Fpier&mode=foot&limit=100").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(actual, expected);
    let (code, command) = fixture.cli(&[
        "spatial", "path", GATE, PIER, "--mode", "foot", "--limit", "100",
    ]);
    assert_eq!(code, 0, "{command}");
    assert_eq!(command, actual);
    assert_eq!(actual["result"]["metric"], "hops");
    assert_eq!(actual["result"]["found"], true);
    assert_eq!(actual["result"]["hops"], 1);
    assert!(
        !fixture.root.join(".wedl-rust").exists(),
        "spatial reads wrote a cache"
    );
}

#[tokio::test]
async fn unreachable_and_zero_hop_paths_are_data_while_missing_endpoints_are_errors() {
    let fixture = Fixture::new();
    for args in [
        ["spatial", "path", GATE, ISLAND, "--mode", "foot"],
        ["spatial", "path", GATE, PIER, "--mode", "boat"],
    ] {
        let (code, value) = fixture.cli(&args);
        assert_eq!(code, 0, "{value}");
        assert_eq!(value["result"]["found"], false);
        assert!(
            value["result"]["locationIds"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(value["result"]["hops"].is_null());
    }
    let (status, value) = get(
        fixture.service(),
        "/api/spatial/path?from=location%3Atown%2Fgate&to=location%3Atown%2Fgate&limit=1",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["result"]["found"], true);
    assert_eq!(value["result"]["hops"], 0);
    assert_eq!(value["result"]["locationIds"], serde_json::json!([GATE]));
    let (code, reverse) = fixture.cli(&["spatial", "path", PIER, GATE]);
    assert_eq!(code, 0, "{reverse}");
    assert_eq!(reverse["result"]["found"], true);

    let (status, error) = get(
        fixture.service(),
        "/api/spatial/path?from=missing&to=location%3Atown%2Fpier",
    )
    .await;
    let (code, cli_error) = fixture.cli(&["spatial", "path", "missing", PIER]);
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(code, 2);
    assert_eq!(error["code"], "not_found");
    assert_eq!(error, cli_error);
}

#[tokio::test]
async fn spatial_parameters_reject_horizons_view_switches_and_bad_bounds() {
    let fixture = Fixture::new();
    for query in [
        "tick=1",
        "timeline=main",
        "order=0",
        "character=someone",
        "view=character",
        "limit=0",
        "limit=1001",
        "limit=bad",
        "mode=",
        "mode=%20",
        "id=duplicate",
    ] {
        let uri = format!("/api/spatial/location?id=location%3Atown%2Fgate&{query}");
        let (status, error) = get(fixture.service(), &uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {error}");
        assert_eq!(error["code"], "usage_error", "{uri}: {error}");
    }
    for query in [
        "tick=1",
        "timeline=main",
        "order=0",
        "view=author",
        "character=someone",
        "from=duplicate",
    ] {
        let uri = format!(
            "/api/spatial/path?from=location%3Atown%2Fgate&to=location%3Atown%2Fpier&{query}"
        );
        let (status, error) = get(fixture.service(), &uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {error}");
        assert_eq!(error["code"], "usage_error");
    }
    for uri in [
        "/api/spatial/location?id=location%3Atown&limit=1",
        "/api/spatial/path?from=location%3Atown%2Fgate&to=location%3Atown%2Fpier&limit=1",
    ] {
        let (status, _) = get(fixture.service(), uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
    }
    let (code, error) = fixture.cli(&["spatial", "location", GATE, "--tick", "0"]);
    assert_eq!(code, 2);
    assert_eq!(error["code"], "usage_error");
}

#[tokio::test]
async fn maintained_tideglass_routes_are_available_through_both_adapters() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/tideglass"));
    let world = source::load_world(root, None).unwrap();
    let (status, actual) = get(
        Service::new(world.clone()),
        "/api/spatial/path?from=Quay&to=Beacon&mode=foot",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{actual}");
    let (code, command) = cli(
        root,
        &["spatial", "path", "Quay", "Beacon", "--mode", "foot"],
    );
    assert_eq!(code, 0, "{command}");
    assert_eq!(command, actual);
    author_envelope(&actual, &world);
    assert_eq!(
        actual["result"]["routeIds"],
        serde_json::json!(["route:quay-beacon"])
    );
    assert_eq!(actual["result"]["hops"], 1);
}
