//! Expanded geometry, portal topology and overlays share one author transport boundary.
use std::{fs, path::Path, process::Command};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;
use wedl::{
    geometry, http,
    model::{StoryTime, World},
    service::{Operation, Service, TimeInput},
    source, spatial, validation,
};

const MAP: &str = "map:town";
const MAP3: &str = "map:tower";
const EARTH: &str = "map:earth";
const A: &str = "location:a-gate";
const B: &str = "location:b-line";
const C: &str = "location:c-square";
const D: &str = "location:d-high";
const E: &str = "location:e-low";
const F: &str = "location:f-world";
const TICK: i64 = 9_007_199_254_740_993;

struct Fixture(TempDir);
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("story")).unwrap();
        let records = [
            ("world","world_00000000000000000000000000","Spatial extension","canonical","capabilities: [spatial-core-v1, geometry-v1, route-v1, overlay-v1]\ndefault_timeline: main\ntimelines: [{id: main, label: Main}, {id: other, label: Other}]\nstate_keys: {}\n".into()),
            ("map",MAP,"Town map","canonical","crs: local-planar:town\naxis_order: [east, north]\nunit: metre\nbounds: {min: [-20,-20], max: [20,20]}\norigin: {label: Gate origin, coordinates: [0,0]}\nscale: {value: 9007199254740993, unit: drawing-units}\n".into()),
            ("map",MAP3,"Tower map","canonical","crs: local-planar:tower\naxis_order: [east, north]\nunit: metre\nz_policy: required\nbounds: {min: [-20,-20,-10], max: [20,20,10]}\n".into()),
            ("map",EARTH,"Earth map","canonical","crs: EPSG:4326\naxis_order: [longitude, latitude]\nunit: degree\nbounds: {min: [-180,-90], max: [180,90]}\n".into()),
            ("location",A,"Gate","canonical",format!("spatial: {{map_id: '{MAP}', geometry: {{kind: point, coordinates: [0,0]}}}}\n")),
            ("location",B,"Lane","canonical",format!("spatial: {{map_id: '{MAP}', geometry: {{kind: line, coordinates: [[-5,0],[5,0]]}}}}\n")),
            ("location",C,"Square","canonical",format!("spatial: {{map_id: '{MAP}', geometry: {{kind: polygon, coordinates: [[2,2],[6,2],[6,6],[2,6],[2,2]]}}}}\n")),
            ("location",D,"High platform","canonical",format!("spatial: {{map_id: '{MAP3}', geometry: {{kind: point, coordinates: [0,0,3]}}}}\n")),
            ("location",E,"Low beam","canonical",format!("spatial: {{map_id: '{MAP3}', geometry: {{kind: line, coordinates: [[-5,0,0],[5,0,0]]}}}}\n")),
            ("location",F,"World gate","canonical",format!("spatial: {{map_id: '{EARTH}', geometry: {{kind: point, coordinates: [12,55]}}}}\n")),
            ("route","route:gate-lane","Gate lane","canonical",format!("from_location_id: '{A}'\nto_location_id: '{B}'\ndirection: one-way\nmodes: [foot]\navailability: open\nduration: {{value: 2, unit: seconds}}\n")),
            ("portal","portal:lane-high","Lane high","canonical",format!("from_location_id: '{B}'\nto: '{D}'\nmodes: [foot]\n")),
            ("portal","portal:gate-position","Gate position","canonical",format!("from_location_id: '{A}'\nto: {{map_id: '{EARTH}', coordinates: [12,55]}}\nmodes: [foot]\n")),
            ("portal","portal:high-square","Retired portal","retired",format!("from_location_id: '{D}'\nto: '{C}'\nmodes: [foot]\n")),
            ("anchor","anchor:town-earth","Town Earth correspondence","canonical",format!("from: {{map_id: '{MAP}', coordinates: [0,0]}}\nto: {{map_id: '{EARTH}', coordinates: [12,55]}}\nconversion: A literal author note, not an executable transform.\n")),
            ("anchor","anchor:town-tower","Town tower correspondence","canonical",format!("from: {{map_id: '{MAP}', coordinates: [0,0]}}\nto: {{map_id: '{MAP3}', coordinates: [0,0,3]}}\n")),
            ("overlay","overlay:always","Always","canonical",format!("lifecycle: static\nmembership: {{location_ids: ['{A}']}}\naudience: [author]\nperspectives: [author]\n")),
            ("overlay","overlay:bounded","Bounded","canonical",format!("lifecycle: time-bounded\nmembership: {{location_ids: ['{A}']}}\naudience: [author]\nperspectives: [author]\nvalid:\n  start: {{timeline: main, tick: {TICK}, order: -1}}\n  end: {{timeline: main, tick: {TICK}, order: 1}}\n")),
            ("overlay","overlay:reader","Reader atlas","canonical",format!("lifecycle: static\nmembership: {{location_ids: ['{A}']}}\naudience: [reader]\nperspectives: [map]\n")),
        ];
        for (index, (kind, id, title, status, fields)) in records.iter().enumerate() {
            fs::write(dir.path().join(format!("story/{index}.md")),format!("---\nschema: {}\nid: {id}\nkind: {kind}\ntitle: {title}\nstatus: {status}\naliases: []\ntags: []\n{fields}---\n",source::SOURCE_SCHEMA)).unwrap();
        }
        let result = Self(dir);
        let report = validation::validate_world(&result.world());
        assert!(report.valid, "{:?}", report.diagnostics);
        result
    }
    fn root(&self) -> &Path {
        self.0.path()
    }
    fn world(&self) -> World {
        source::load_world(self.root(), None).unwrap()
    }
    fn service(&self) -> Service {
        Service::new(self.world())
    }
}
fn at(order: i32) -> TimeInput {
    TimeInput {
        tick: TICK,
        order,
        timeline: Some("main".into()),
    }
}
fn point(order: i32) -> StoryTime {
    StoryTime::new("main", TICK, order).unwrap()
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
    assert_eq!(response.headers()["cache-control"], "no-store");
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
async fn parity(f: &Fixture, operation: Operation, core: Value, uri: &str, args: &[&str]) -> Value {
    let service = f.service();
    let expected = service.execute(operation).unwrap();
    assert_eq!(expected["result"], core);
    let (status, actual) = get(service.clone(), uri).await;
    assert_eq!(status, StatusCode::OK, "{actual}");
    assert_eq!(actual, expected);
    let (code, command) = cli(f.root(), args);
    assert_eq!(code, 0, "{command}");
    assert_eq!(command, actual);
    assert_eq!(actual["fingerprint"], service.world().fingerprint);
    assert_eq!(actual["result"]["perspective"], "author");
    actual["result"].clone()
}
fn ids(value: &Value) -> Vec<&str> {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["locationId"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn map_and_literal_anchor_catalogues_match_all_interfaces() {
    let f = Fixture::new();
    let world = f.world();
    let maps = parity(
        &f,
        Operation::SpatialMaps { limit: 100 },
        geometry::maps(&world, 100).unwrap(),
        "/api/spatial/maps",
        &["spatial", "maps"],
    )
    .await;
    assert_eq!(maps["items"].as_array().unwrap().len(), 3);
    let town = maps["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["mapId"] == MAP)
        .unwrap();
    assert_eq!(town["scale"]["value"], "9007199254740993");
    assert_eq!(town["origin"]["coordinates"], json!(["0", "0"]));
    let tower = maps["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["mapId"] == MAP3)
        .unwrap();
    assert_eq!(tower["dimensions"], 3);
    assert_eq!(tower["zPolicy"], "required");
    let anchors = parity(
        &f,
        Operation::SpatialAnchors {
            from_map: Some("Town map".into()),
            to_map: Some(EARTH.into()),
            limit: 100,
        },
        geometry::anchors(&world, Some("Town map"), Some(EARTH), 100).unwrap(),
        "/api/spatial/anchors?from_map=Town%20map&to_map=map%3Aearth",
        &[
            "spatial",
            "anchors",
            "--from-map",
            "Town map",
            "--to-map",
            EARTH,
        ],
    )
    .await;
    assert_eq!(anchors["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        anchors["items"][0]["from"]["coordinates"],
        json!(["0", "0"])
    );
    assert_eq!(
        anchors["items"][0]["to"]["coordinates"],
        json!(["12", "55"])
    );
    assert!(anchors.get("transform").is_none());
    assert!(
        anchors["items"][0]["conversion"]
            .as_str()
            .unwrap()
            .contains("literal")
    );
}

#[tokio::test]
async fn shape_box_relations_and_geometric_distance_match_all_interfaces() {
    let f = Fixture::new();
    let world = f.world();
    for (relation, expected) in [("intersects", vec![A, B]), ("within", vec![A])] {
        let uri = format!(
            "/api/spatial/bbox?map=map%3Atown&min_x=-1&min_y=-1&max_x=1&max_y=1&relation={relation}"
        );
        let result = parity(
            &f,
            Operation::SpatialBbox {
                map: MAP.into(),
                min: vec![-1.0, -1.0],
                max: vec![1.0, 1.0],
                relation: relation.into(),
                limit: 100,
            },
            geometry::bbox_relation(&world, MAP, &[-1.0, -1.0], &[1.0, 1.0], relation, 100)
                .unwrap(),
            &uri,
            &[
                "spatial",
                "bbox",
                MAP,
                "--min-x",
                "-1",
                "--min-y",
                "-1",
                "--max-x",
                "1",
                "--max-y",
                "1",
                "--relation",
                relation,
            ],
        )
        .await;
        assert_eq!(ids(&result), expected);
    }
    let polygon = parity(
        &f,
        Operation::SpatialNearby {
            map: MAP.into(),
            point: vec![4.0, 4.0],
            radius: 0.0,
            limit: 100,
        },
        geometry::nearby(&world, MAP, [4.0, 4.0], 0.0, 100).unwrap(),
        "/api/spatial/nearby?map=map%3Atown&x=4&y=4&radius=0",
        &[
            "spatial", "nearby", MAP, "--x", "4", "--y", "4", "--radius", "0",
        ],
    )
    .await;
    assert_eq!(ids(&polygon), [C]);
    assert_eq!(polygon["items"][0]["geometryKind"], "polygon");
    assert_eq!(polygon["items"][0]["distance"], 0.0);
    let (_, line) = get(
        f.service(),
        "/api/spatial/nearby?map=map%3Atown&x=0&y=1&radius=1",
    )
    .await;
    assert_eq!(ids(&line["result"]), [A, B]);
}

#[tokio::test]
async fn third_axis_and_geographic_coordinates_are_not_silently_projected() {
    let f = Fixture::new();
    let world = f.world();
    let bbox = parity(
        &f,
        Operation::SpatialBbox {
            map: MAP3.into(),
            min: vec![-1.0, -1.0, 2.0],
            max: vec![1.0, 1.0, 4.0],
            relation: "intersects".into(),
            limit: 100,
        },
        geometry::bbox(&world, MAP3, [-1.0, -1.0, 2.0], [1.0, 1.0, 4.0], 100).unwrap(),
        "/api/spatial/bbox?map=map%3Atower&min_x=-1&min_y=-1&max_x=1&max_y=1&min_z=2&max_z=4",
        &[
            "spatial", "bbox", MAP3, "--min-x", "-1", "--min-y", "-1", "--max-x", "1", "--max-y",
            "1", "--min-z", "2", "--max-z", "4",
        ],
    )
    .await;
    assert_eq!(ids(&bbox), [D]);
    assert_eq!(bbox["items"][0]["coordinates"], json!(["0", "0", "3"]));
    let nearby = parity(
        &f,
        Operation::SpatialNearby {
            map: MAP3.into(),
            point: vec![0.0, 0.0, 4.0],
            radius: 1.0,
            limit: 100,
        },
        geometry::nearby(&world, MAP3, [0.0, 0.0, 4.0], 1.0, 100).unwrap(),
        "/api/spatial/nearby?map=map%3Atower&x=0&y=0&z=4&radius=1",
        &[
            "spatial", "nearby", MAP3, "--x", "0", "--y", "0", "--z", "4", "--radius", "1",
        ],
    )
    .await;
    assert_eq!(ids(&nearby), [D]);
    let earth = parity(
        &f,
        Operation::SpatialNearby {
            map: EARTH.into(),
            point: vec![12.0, 55.0],
            radius: 0.0,
            limit: 100,
        },
        geometry::nearby(&world, EARTH, [12.0, 55.0], 0.0, 100).unwrap(),
        "/api/spatial/nearby?map=map%3Aearth&x=12&y=55&radius=0",
        &[
            "spatial", "nearby", EARTH, "--x", "12", "--y", "55", "--radius", "0",
        ],
    )
    .await;
    assert_eq!(ids(&earth), [F]);
    assert_eq!(earth["unit"], "degree");
    assert_eq!(earth["axisOrder"], json!(["longitude", "latitude"]));
}

#[tokio::test]
async fn portal_hops_and_reachability_never_invent_route_metrics_or_position_edges() {
    let f = Fixture::new();
    let world = f.world();
    let path = parity(
        &f,
        Operation::SpatialPath {
            from: A.into(),
            to: D.into(),
            mode: Some("foot".into()),
            metric: "hops".into(),
            unit: None,
            limit: 100,
        },
        spatial::path(&world, A, D, Some("foot"), "hops", None, 100).unwrap(),
        "/api/spatial/path?from=location%3Aa-gate&to=location%3Ad-high&mode=foot",
        &["spatial", "path", A, D, "--mode", "foot"],
    )
    .await;
    assert_eq!(path["found"], true);
    assert_eq!(path["hops"], 2);
    assert_eq!(path["routeIds"], json!(["route:gate-lane"]));
    assert_eq!(path["portalIds"], json!(["portal:lane-high"]));
    let reachable = parity(
        &f,
        Operation::SpatialReachable {
            from: A.into(),
            mode: None,
            limit: 100,
        },
        spatial::reachable(&world, A, None, 100).unwrap(),
        "/api/spatial/reachable?from=location%3Aa-gate",
        &["spatial", "reachable", A],
    )
    .await;
    assert_eq!(reachable["locationIds"], json!([A, B, D]));
    let location = parity(
        &f,
        Operation::SpatialLocation {
            entity: A.into(),
            mode: None,
            limit: 100,
        },
        spatial::location(&world, A, None, 100).unwrap(),
        "/api/spatial/location?id=location%3Aa-gate",
        &["spatial", "location", A],
    )
    .await;
    assert_eq!(location["outgoingPortals"][0]["targetKind"], "position");
    assert_eq!(location["outgoingPortals"][0]["position"]["map_id"], EARTH);
    for args in [
        vec![
            "spatial", "path", A, D, "--metric", "duration", "--unit", "seconds",
        ],
        vec!["spatial", "path", D, A],
        vec!["spatial", "path", A, F],
    ] {
        assert_eq!(cli(f.root(), &args).1["result"]["found"], false);
    }
    let (_, boat) = get(
        f.service(),
        "/api/spatial/reachable?from=location%3Aa-gate&mode=boat&limit=1",
    )
    .await;
    assert_eq!(boat["result"]["locationIds"], json!([A]));
}

#[tokio::test]
async fn overlay_time_filters_preserve_large_exact_coordinates_across_interfaces() {
    let f = Fixture::new();
    let world = f.world();
    for (order, count) in [(-2, 1), (-1, 2), (1, 2), (2, 1)] {
        let o = order.to_string();
        let t = TICK.to_string();
        let uri = format!("/api/spatial/overlays?location=location%3Aa-gate&tick={t}&order={o}");
        let result = parity(
            &f,
            Operation::SpatialOverlays {
                location: A.into(),
                at: at(order),
                audience: "author".into(),
                view: "author".into(),
                overlay: None,
                limit: 100,
            },
            spatial::overlays(&world, A, &point(order), "author", "author", None, 100).unwrap(),
            &uri,
            &["spatial", "overlays", A, "--tick", &t, "--order", &o],
        )
        .await;
        assert_eq!(result["count"], count);
        assert_eq!(result["effectiveTime"]["tick"], t);
        assert_eq!(result["effectiveTime"]["order"], o);
        assert_eq!(result["scope"], "author-as-of-overlay-membership");
    }
    let reader=parity(&f,Operation::SpatialOverlays{location:A.into(),at:at(0),audience:"reader".into(),view:"map".into(),overlay:Some("overlay:reader".into()),limit:1},spatial::overlays(&world,A,&point(0),"reader","map",Some("overlay:reader"),1).unwrap(),"/api/spatial/overlays?location=location%3Aa-gate&tick=9007199254740993&order=0&audience=reader&view=map&overlay=overlay%3Areader&limit=1",&["spatial","overlays",A,"--tick","9007199254740993","--order","0","--audience","reader","--view","map","--overlay","overlay:reader","--limit","1"]).await;
    assert_eq!(reader["count"], 1);
    assert_eq!(reader["perspective"], "author");
    let (_, other) = get(
        f.service(),
        "/api/spatial/overlays?location=location%3Aa-gate&tick=9007199254740993&timeline=other",
    )
    .await;
    assert_eq!(other["result"]["count"], 1);
}

#[tokio::test]
async fn overlay_explicit_scope_errors_match_and_filter_before_result_limits() {
    let f = Fixture::new();
    let mut failure = None;
    for overlay in ["overlay:reader", "overlay:missing"] {
        let uri =
            format!("/api/spatial/overlays?location=location%3Aa-gate&tick=0&overlay={overlay}");
        let (status, error) = get(f.service(), &uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (code, command) = cli(
            f.root(),
            &[
                "spatial",
                "overlays",
                A,
                "--tick",
                "0",
                "--overlay",
                overlay,
            ],
        );
        assert_eq!(code, 2);
        assert_eq!(command, error);
        if let Some(old) = &failure {
            assert_eq!(old, &error)
        } else {
            failure = Some(error);
        }
    }
    let (status, empty) = get(
        f.service(),
        "/api/spatial/overlays?location=location%3Ac-square&tick=9007199254740993&order=0&limit=1",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(empty["result"]["count"], 0);
    let (status, one) = get(
        f.service(),
        "/api/spatial/overlays?location=location%3Aa-gate&tick=0&limit=1",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(one["result"]["count"], 1);
    let (status, limit) = get(
        f.service(),
        "/api/spatial/overlays?location=location%3Aa-gate&tick=9007199254740993&order=0&limit=1",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(limit.get("result").is_none());
    let (status, outside) = get(
        f.service(),
        "/api/spatial/overlays?location=location%3Aa-gate&tick=0&overlay=overlay%3Abounded",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(outside["result"]["count"], 0);
}

#[tokio::test]
async fn new_query_parameters_are_closed_dimension_checked_and_complete_or_error() {
    let f = Fixture::new();
    for uri in [
        "/api/spatial/anchors?tick=0",
        "/api/spatial/anchors?limit=1",
        "/api/spatial/anchors?from_map=map%3Atown&from_map=map%3Atower",
        "/api/spatial/reachable?from=location%3Aa-gate&limit=2",
        "/api/spatial/reachable?from=location%3Aa-gate&tick=0",
        "/api/spatial/reachable?from=location%3Aa-gate&metric=duration",
        "/api/spatial/reachable?from=location%3Aa-gate&limit=1001",
        "/api/spatial/bbox?map=map%3Atown&min_x=-1&min_y=-1&max_x=1&max_y=1&min_z=0",
        "/api/spatial/bbox?map=map%3Atown&min_x=-1&min_y=-1&max_x=1&max_y=1&max_z=0",
        "/api/spatial/bbox?map=map%3Atown&min_x=-1&min_y=-1&max_x=1&max_y=1&min_z=0&max_z=1",
        "/api/spatial/bbox?map=map%3Atower&min_x=-1&min_y=-1&max_x=1&max_y=1",
        "/api/spatial/bbox?map=map%3Atown&min_x=-1&min_y=-1&max_x=1&max_y=1&relation=touches",
        "/api/spatial/bbox?map=map%3Atown&min_x=-1&min_y=-1&max_x=1&max_y=1&relation=within&relation=intersects",
        "/api/spatial/bbox?map=map%3Atown&min_x=-1&min_y=-1&max_x=1&max_y=1&limit=1",
        "/api/spatial/nearby?map=map%3Atower&x=0&y=0&radius=1",
        "/api/spatial/nearby?map=map%3Atown&x=0&y=0&z=0&radius=1",
        "/api/spatial/nearby?map=map%3Atown&x=NaN&y=0&radius=1",
        "/api/spatial/nearby?map=map%3Atown&x=0&y=0&radius=-1",
        "/api/spatial/overlays?location=location%3Aa-gate",
        "/api/spatial/overlays?location=location%3Aa-gate&tick=0&order=2147483648",
        "/api/spatial/overlays?location=location%3Aa-gate&tick=9223372036854775808",
        "/api/spatial/overlays?location=location%3Aa-gate&tick=0&audience=",
        "/api/spatial/overlays?location=location%3Aa-gate&tick=0&view=",
        "/api/spatial/overlays?location=location%3Aa-gate&tick=0&overlay=Always",
        "/api/spatial/overlays?location=location%3Aa-gate&tick=0&perspective=character",
        "/api/spatial/overlays?location=location%3Aa-gate&tick=0&character=char_00000000000000000000000001",
    ] {
        let (status, error) = get(f.service(), uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {error}");
        assert!(error.get("result").is_none());
    }
    let bad_z = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .args([
            "--repo",
            f.root().to_str().unwrap(),
            "spatial",
            "bbox",
            MAP,
            "--min-x",
            "-1",
            "--min-y",
            "-1",
            "--max-x",
            "1",
            "--max-y",
            "1",
            "--min-z",
            "0",
        ])
        .output()
        .unwrap();
    assert!(!bad_z.status.success());
    assert!(String::from_utf8_lossy(&bad_z.stderr).contains("--max-z"));
}
