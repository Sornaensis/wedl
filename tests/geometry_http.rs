//! Weighted paths and same-map geometry use the same author contract in all adapters.
use std::{path::Path, process::Command};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;
use wedl::{
    http,
    service::{Operation, Service},
    source, validation,
};

const MAP: &str = "map:town/center";
const A: &str = "location:town/gate";
const B: &str = "location:town/pier";
const C: &str = "location:town/tower";

struct Fixture(TempDir);
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("story")).unwrap();
        let mut records = vec![
            ("world", "world_00000000000000000000000000", "Geometry world", "capabilities: [spatial-core-v1, geometry-v1, route-v1]\ndefault_timeline: main\ntimelines: [{id: main, label: Main}]\nstate_keys: {}\n".to_owned()),
            ("map", MAP, "Town map", "crs: local-planar:town\naxis_order: [east, north]\nunit: metre\nbounds: {min: [-10, -10], max: [10, 10]}\n".into()),
            ("map", "map:world/island", "Island map", "crs: local-planar:island\naxis_order: [east, north]\nunit: metre\nbounds: {min: [-10, -10], max: [10, 10]}\n".into()),
            ("location", A, "Gate", format!("spatial: {{map_id: '{MAP}', geometry: {{kind: point, coordinates: [-3, -4]}}}}\n")),
            ("location", B, "Pier", format!("spatial: {{map_id: '{MAP}', geometry: {{kind: point, coordinates: [0, 0]}}}}\n")),
            ("location", C, "Tower", format!("spatial: {{map_id: '{MAP}', geometry: {{kind: point, coordinates: [3, 4]}}}}\n")),
            ("location", "location:island", "Island", "spatial: {map_id: 'map:world/island', geometry: {kind: point, coordinates: [0, 0]}}\n".into()),
        ];
        for (id, from, to, mode, duration, unit, distance, cost) in [
            (
                "route:town/gate-pier",
                A,
                B,
                "foot",
                "9007199254740993",
                "seconds",
                "1.25",
                "0.1",
            ),
            (
                "route:town/pier-tower",
                B,
                C,
                "foot",
                "2",
                "seconds",
                "2.5",
                "0.2",
            ),
            (
                "route:town/direct",
                A,
                C,
                "foot",
                "9007199254740996",
                "seconds",
                "5",
                "0.4",
            ),
            ("route:town/ferry", A, C, "boat", "1", "hours", "9", "1"),
        ] {
            records.push(("route", id, id, format!(
                "from_location_id: {from}\nto_location_id: {to}\ndirection: one-way\nmodes: [{mode}]\navailability: open\nduration: {{value: {duration}, unit: {unit}}}\nroute_distance: {{value: {distance}, unit: metre}}\ntravel_cost: {{value: {cost}, unit: coins}}\n"
            )));
        }
        for (index, (kind, id, title, fields)) in records.iter().enumerate() {
            std::fs::write(directory.path().join(format!("story/record-{index}.md")), format!(
                "---\nschema: {}\nkind: {kind}\nid: {id}\ntitle: {title}\nstatus: canonical\naliases: []\ntags: []\n{fields}---\n", source::SOURCE_SCHEMA
            )).unwrap();
        }
        let fixture = Self(directory);
        let world = source::load_world(fixture.root(), None).unwrap();
        let report = validation::validate_world(&world);
        assert!(report.valid, "{:?}", report.diagnostics);
        fixture
    }

    fn root(&self) -> &Path {
        self.0.path()
    }
    fn service(&self) -> Service {
        Service::new(source::load_world(self.root(), None).unwrap())
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
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn parity(root: &Path, operation: Operation, uri: &str, args: &[&str]) -> Value {
    let service = Service::new(source::load_world(root, None).unwrap());
    let expected = service.execute(operation).unwrap();
    let (status, actual) = get(service.clone(), uri).await;
    assert_eq!(status, StatusCode::OK, "{actual}");
    assert_eq!(actual, expected);
    let (code, command) = cli(root, args);
    assert_eq!(code, 0, "{command}");
    assert_eq!(command, actual);
    assert_eq!(actual["protocol"], wedl::CONTRACT);
    assert_eq!(actual["fingerprint"], service.world().fingerprint);
    assert!(actual.get("revision").is_some());
    assert_eq!(actual["result"]["perspective"], "author");
    assert_eq!(actual["result"]["scope"], "author-all-time-spatial-source");
    assert!(actual["result"].get("effectiveTime").is_none());
    actual["result"].clone()
}

#[tokio::test]
async fn map_bbox_and_radius_reads_match_cli_http_and_service() {
    let fixture = Fixture::new();
    let maps = parity(
        fixture.root(),
        Operation::SpatialMaps { limit: 100 },
        "/api/spatial/maps",
        &["spatial", "maps"],
    )
    .await;
    assert_eq!(maps["items"].as_array().unwrap().len(), 2);
    assert_eq!(maps["items"][0]["mapId"], MAP);
    assert_eq!(maps["items"][0]["bounds"]["min"], json!(["-10", "-10"]));

    let bbox = parity(
        fixture.root(),
        Operation::SpatialBbox {
            map: MAP.into(),
            min: vec![-3.0, -4.0],
            max: vec![0.0, 0.0],
            relation: "intersects".into(),
            limit: 100,
        },
        "/api/spatial/bbox?map=map%3Atown%2Fcenter&min_x=-3&min_y=-4&max_x=0&max_y=0",
        &[
            "spatial", "bbox", MAP, "--min-x", "-3", "--min-y", "-4", "--max-x", "0", "--max-y",
            "0",
        ],
    )
    .await;
    assert_eq!(bbox["mapId"], MAP);
    assert_eq!(
        bbox["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["locationId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [A, B]
    );
    assert_eq!(bbox["items"][0]["coordinates"], json!(["-3", "-4"]));

    let nearby = parity(
        fixture.root(),
        Operation::SpatialNearby {
            map: MAP.into(),
            point: vec![0.0, 0.0],
            radius: 5.0,
            limit: 100,
        },
        "/api/spatial/nearby?map=map%3Atown%2Fcenter&x=0&y=0&radius=5",
        &[
            "spatial", "nearby", MAP, "--x", "0", "--y", "0", "--radius", "5",
        ],
    )
    .await;
    assert_eq!(
        nearby["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["locationId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [A, B, C]
    );
    assert_eq!(nearby["items"][0]["distance"], 5.0);
    assert_eq!(nearby["items"][1]["distance"], 0.0);
    assert_eq!(nearby["items"][2]["distance"], 5.0);
    let (status, zero) = get(
        fixture.service(),
        "/api/spatial/nearby?map=map%3Atown%2Fcenter&x=0&y=0&radius=0",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(zero["result"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(zero["result"]["items"][0]["locationId"], B);
    let fractional = parity(
        fixture.root(),
        Operation::SpatialNearby {
            map: MAP.into(),
            point: vec![-3.25, -4.0],
            radius: 0.25,
            limit: 100,
        },
        "/api/spatial/nearby?map=map%3Atown%2Fcenter&x=-3.25&y=-4&radius=0.25",
        &[
            "spatial", "nearby", MAP, "--x", "-3.25", "--y", "-4", "--radius", "0.25",
        ],
    )
    .await;
    assert_eq!(fractional["items"].as_array().unwrap().len(), 1);
    assert_eq!(fractional["items"][0]["locationId"], A);
    assert_eq!(fractional["items"][0]["distance"], 0.25);
    let (status, large_radius) = get(
        fixture.service(),
        "/api/spatial/nearby?map=map%3Atown%2Fcenter&x=0&y=0&radius=1e300",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{large_radius}");
    assert_eq!(large_radius["result"]["items"].as_array().unwrap().len(), 3);
    assert!(
        !fixture.root().join(".wedl-rust").exists(),
        "geometry reads wrote a cache"
    );
}

#[tokio::test]
async fn weighted_paths_preserve_exact_integer_and_decimal_totals() {
    let fixture = Fixture::new();
    for (metric, unit, total) in [
        ("duration", "seconds", "9007199254740995"),
        ("route_distance", "metre", "3.75"),
        ("travel_cost", "coins", "0.3"),
    ] {
        let uri = format!(
            "/api/spatial/path?from=location%3Atown%2Fgate&to=location%3Atown%2Ftower&mode=foot&metric={metric}&unit={unit}"
        );
        let result = parity(
            fixture.root(),
            Operation::SpatialPath {
                from: A.into(),
                to: C.into(),
                mode: Some("foot".into()),
                metric: metric.into(),
                unit: Some(unit.into()),
                limit: 100,
            },
            &uri,
            &[
                "spatial", "path", A, C, "--mode", "foot", "--metric", metric, "--unit", unit,
            ],
        )
        .await;
        assert_eq!(result["metric"], metric);
        assert_eq!(result["total"], json!({"value": total, "unit": unit}));
        assert_eq!(result["hops"], 2);
        assert_eq!(
            result["routeIds"],
            json!(["route:town/gate-pier", "route:town/pier-tower"])
        );
    }
    let (code, hops) = cli(fixture.root(), &["spatial", "path", A, C, "--mode", "foot"]);
    assert_eq!(code, 0);
    assert_eq!(hops["result"]["routeIds"], json!(["route:town/direct"]));
    assert!(hops["result"]["total"].is_null());
    let (status, inferred) = get(fixture.service(), "/api/spatial/path?from=location%3Atown%2Fgate&to=location%3Atown%2Ftower&mode=foot&metric=duration").await;
    assert_eq!(status, StatusCode::OK, "{inferred}");
    assert_eq!(
        inferred["result"]["total"],
        json!({"value": "9007199254740995", "unit": "seconds"})
    );
    let (status, ambiguous) = get(
        fixture.service(),
        "/api/spatial/path?from=location%3Atown%2Fgate&to=location%3Atown%2Ftower&metric=duration",
    )
    .await;
    let (code, command) = cli(
        fixture.root(),
        &["spatial", "path", A, C, "--metric", "duration"],
    );
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(code, 2);
    assert_eq!(ambiguous["code"], "usage_error");
    assert_eq!(ambiguous, command);
}

#[tokio::test]
async fn geometry_and_weighted_parameters_are_closed_and_bounded() {
    let fixture = Fixture::new();
    let service = fixture.service();
    for base in [
        "/api/spatial/maps?limit=100",
        "/api/spatial/bbox?map=map%3Atown%2Fcenter&min_x=-10&min_y=-10&max_x=10&max_y=10",
        "/api/spatial/nearby?map=map%3Atown%2Fcenter&x=0&y=0&radius=5",
        "/api/spatial/path?from=location%3Atown%2Fgate&to=location%3Atown%2Ftower",
    ] {
        for extra in [
            "tick=0",
            "timeline=main",
            "order=0",
            "character=someone",
            "view=author",
            "unexpected=x",
            "limit=0",
            "limit=1001",
        ] {
            // Geometry now accepts an explicit history horizon; the no-capability
            // refusal and dated successes are covered by spatial_history_http.
            if extra == "tick=0" && (base.contains("/bbox?") || base.contains("/nearby?")) {
                continue;
            }
            let uri = format!("{base}&{extra}");
            let (status, error) = get(service.clone(), &uri).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {error}");
            assert_eq!(error["code"], "usage_error");
        }
    }
    for uri in [
        "/api/spatial/maps?limit=1",
        "/api/spatial/bbox?map=map%3Atown%2Fcenter&min_x=0&min_y=0&max_x=-1&max_y=0",
        "/api/spatial/bbox?map=map%3Atown%2Fcenter&min_x=-10&min_y=-10&max_x=10&max_y=10&limit=1",
        "/api/spatial/nearby?map=map%3Atown%2Fcenter&x=0&y=0&radius=5&limit=1",
        "/api/spatial/nearby?map=map%3Atown%2Fcenter&x=0&y=0&radius=-1",
        "/api/spatial/nearby?map=map%3Atown%2Fcenter&x=0&y=0&radius=1e309",
        "/api/spatial/nearby?map=map%3Atown%2Fcenter&x=bad&y=0&radius=1",
        "/api/spatial/nearby?map=map%3Atown%2Fcenter&x=&y=0&radius=1",
        "/api/spatial/nearby?map=map%3Atown%2Fcenter&x=0&y=0",
        "/api/spatial/nearby?map=map%3Atown%2Fcenter&x=0&x=1&y=0&radius=5",
        "/api/spatial/path?from=Gate&to=Tower&metric=bogus",
        "/api/spatial/path?from=Gate&to=Tower&metric=",
        "/api/spatial/path?from=Gate&to=Tower&metric=duration&unit=",
        "/api/spatial/path?from=Gate&to=Tower&unit=seconds",
    ] {
        let (status, error) = get(service.clone(), uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {error}");
        assert_eq!(error["code"], "usage_error");
    }
    for value in [
        "NaN",
        "inf",
        "-inf",
        "1e309",
        "9007199254740992",
        "-9007199254740992",
    ] {
        let uri = format!("/api/spatial/nearby?map=map%3Atown%2Fcenter&x={value}&y=0&radius=1");
        let (status, error) = get(service.clone(), &uri).await;
        let (code, command) = cli(
            fixture.root(),
            &[
                "spatial", "nearby", MAP, "--x", value, "--y", "0", "--radius", "1",
            ],
        );
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {error}");
        assert_eq!(code, 2, "{command}");
        assert_eq!(error, command);
    }
    let (status, error) = get(service, "/api/spatial/nearby?map=missing&x=0&y=0&radius=1").await;
    let (code, command) = cli(
        fixture.root(),
        &[
            "spatial", "nearby", "missing", "--x", "0", "--y", "0", "--radius", "1",
        ],
    );
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(code, 2);
    assert_eq!(error, command);
}

#[tokio::test]
async fn tideglass_geometry_is_available_through_all_adapters() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/tideglass"));
    let maps = parity(
        root,
        Operation::SpatialMaps { limit: 100 },
        "/api/spatial/maps",
        &["spatial", "maps"],
    )
    .await;
    assert!(
        maps["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["mapId"] == "map:tideglass-town")
    );
    let result = parity(
        root,
        Operation::SpatialNearby {
            map: "map:tideglass-town".into(),
            point: vec![0.0, -20.0],
            radius: 0.0,
            limit: 100,
        },
        "/api/spatial/nearby?map=map%3Atideglass-town&x=0&y=-20&radius=0",
        &[
            "spatial",
            "nearby",
            "map:tideglass-town",
            "--x",
            "0",
            "--y",
            "-20",
            "--radius",
            "0",
        ],
    )
    .await;
    assert_eq!(result["items"].as_array().unwrap().len(), 1);
    assert_eq!(result["items"][0]["title"], "Quay");
    assert_eq!(result["items"][0]["distance"], 0.0);
}
