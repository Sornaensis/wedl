//! Spatial history is an explicit author read across the shared transport boundary.
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;
use tower::ServiceExt;
use wedl::{
    geometry, http,
    model::{StoryTime, World},
    service::{Operation, Service, TimeInput},
    source, spatial, spatial_history, validation,
};

const A: &str = "location:harbor";
const B: &str = "location:landing";
const MAP: &str = "map:harbor";
const OTHER: &str = "map:other";
const ROUTE: &str = "route:harbor-landing";
const VIEWER: &str = "char_00000000000000000000000001";
const TICK: i64 = 9_007_199_254_740_993;
const FIRST: &str = "spatialhistory_00000000000000000000000001";
const CORRECTION: &str = "spatialhistory_00000000000000000000000002";

struct Fixture(TempDir);
impl Fixture {
    fn new(history: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("story")).unwrap();
        let valid = format!(
            "{{kind: interval, timeline: main, start: {{timeline: main, tick: {TICK}, order: -1}}, end: {{timeline: main, tick: {TICK}, order: 1}}}}"
        );
        let placement = if history {
            format!(
                "spatial_history:\n  - id: {FIRST}\n    aspect: placement\n    valid: {valid}\n    value: {{map_id: '{MAP}', geometry: {{kind: point, coordinates: [2,2]}}}}\n  - id: {CORRECTION}\n    aspect: placement\n    valid: {valid}\n    replaces: {FIRST}\n    value: {{map_id: '{MAP}', geometry: {{kind: point, coordinates: [3,4]}}}}\n  - id: spatialhistory_00000000000000000000000003\n    aspect: parentage\n    valid: {valid}\n    value: {{parent_id: null}}\n"
            )
        } else {
            String::new()
        };
        let routes = if history {
            format!(
                "spatial_history:\n  - id: spatialhistory_00000000000000000000000011\n    aspect: route-availability\n    valid: {valid}\n    value: {{availability: closed}}\n  - id: spatialhistory_00000000000000000000000012\n    aspect: route-metric\n    valid: {valid}\n    value: {{duration: {{value: 7, unit: seconds}}}}\n  - id: spatialhistory_00000000000000000000000013\n    aspect: route-metric\n    valid: {valid}\n    value: {{travel_cost: {{value: 9007199254740993, unit: credits}}}}\n"
            )
        } else {
            String::new()
        };
        let records = [
            ("world", "world_00000000000000000000000000", "Spatial histories", "capabilities: [spatial-core-v1, geometry-v1, route-v1, spatial-history-v1]\ndefault_timeline: main\ntimelines: [{id: main, label: Main}, {id: other, label: Other}]\nstate_keys: {}\n".to_owned()),
            ("map", MAP, "Harbor map", "crs: local-planar:harbor\naxis_order: [east, north]\nunit: m\nbounds: {min: [-10,-10], max: [10,10]}\n".into()),
            ("map", OTHER, "Other map", "crs: local-planar:other\naxis_order: [east, north]\nunit: m\nbounds: {min: [-10,-10], max: [10,10]}\n".into()),
            ("location", A, "Harbor", format!("spatial: {{map_id: '{MAP}', geometry: {{kind: point, coordinates: [0,0]}}}}\n")),
            ("location", B, "Landing", format!("parent_id: {A}\nspatial: {{map_id: '{OTHER}', geometry: {{kind: point, coordinates: [8,8]}}}}\n{placement}")),
            ("route", ROUTE, "Harbor crossing", format!("from_location_id: {A}\nto_location_id: {B}\ndirection: two-way\nmodes: [foot]\navailability: open\nduration: {{value: 5, unit: seconds}}\n{routes}")),
            ("character", VIEWER, "Ana", "initial_state: {}\n".into()),
            ("knowledge", "know_00000000000000000000000001", "Lamp", format!("knower: {VIEWER}\nclaim: {{key: lamp, statement: I saw a lamp}}\ntransitions: [{{id: learned, time: {{tick: 0}}, state: accepted, acquisition: observed, confidence: 1}}]\n")),
        ];
        for (index, (kind, id, title, fields)) in records.iter().enumerate() {
            fs::write(dir.path().join(format!("story/{index}.md")), format!("---\nschema: {}\nid: {id}\nkind: {kind}\ntitle: {title}\nstatus: canonical\naliases: []\ntags: []\n{fields}---\nAuthor source prose\n", source::SOURCE_SCHEMA)).unwrap();
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
    (
        output.status.code().unwrap(),
        serde_json::from_slice(bytes)
            .unwrap_or_else(|_| panic!("non-JSON output: {}", String::from_utf8_lossy(bytes))),
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

#[tokio::test]
async fn authored_and_selected_history_match_core_service_cli_and_http() {
    let f = Fixture::new(true);
    let world = f.world();
    let all = parity(
        &f,
        Operation::SpatialHistory {
            record: "Landing".into(),
            at: None,
            limit: 100,
        },
        spatial_history::history(&world, "Landing", None, 100).unwrap(),
        "/api/spatial/history?record=Landing",
        &["spatial", "history", "Landing"],
    )
    .await;
    assert_eq!(all["count"], 3);
    assert!(all["at"].is_null());
    assert_eq!(all["items"][0]["memberId"], FIRST);
    assert_eq!(all["items"][1]["memberId"], CORRECTION);
    assert_eq!(all["items"][1]["replaces"], FIRST);
    assert_eq!(all["items"][1]["citations"].as_array().unwrap().len(), 2);
    let dated = parity(
        &f,
        Operation::SpatialHistory {
            record: B.into(),
            at: Some(at(0)),
            limit: 2,
        },
        spatial_history::history(&world, B, Some(&point(0)), 2).unwrap(),
        "/api/spatial/history?record=location%3Alanding&tick=9007199254740993&order=0&limit=2",
        &[
            "spatial",
            "history",
            B,
            "--tick",
            "9007199254740993",
            "--order",
            "0",
            "--limit",
            "2",
        ],
    )
    .await;
    assert_eq!(dated["count"], 2);
    assert_eq!(dated["at"]["tick"], TICK.to_string());
    assert_eq!(dated["items"][0]["memberId"], CORRECTION);
    assert_eq!(
        dated["items"][0]["valid"]["start"]["tick"],
        TICK.to_string()
    );
    assert_eq!(dated["items"][0]["valid"]["start"]["order"], "-1");
    assert!(dated["items"][1]["value"]["parent_id"].is_null());
    let (status, overflow) = get(
        f.service(),
        "/api/spatial/history?record=location%3Alanding&limit=2",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(overflow.get("result").is_none());
}

#[tokio::test]
async fn history_endpoints_timelines_and_end_of_tick_default_are_exact() {
    let f = Fixture::new(true);
    for (order, count) in [(-2, 0), (-1, 2), (1, 2), (2, 0)] {
        let uri =
            format!("/api/spatial/history?record=location%3Alanding&tick={TICK}&order={order}");
        let (status, value) = get(f.service(), &uri).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["result"]["count"], count);
        assert_eq!(value["result"]["at"]["order"], order.to_string());
    }
    for uri in [
        "/api/spatial/history?record=location%3Alanding&tick=9007199254740993",
        "/api/spatial/history?record=location%3Alanding&tick=9007199254740993&order=0&timeline=other",
    ] {
        let (status, value) = get(f.service(), uri).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["result"]["count"], 0);
    }
    let (code, value) = cli(
        f.root(),
        &["spatial", "history", B, "--tick", "9007199254740993"],
    );
    assert_eq!(code, 0);
    assert_eq!(value["result"]["at"]["order"], i32::MAX.to_string());
}

#[tokio::test]
async fn route_inspection_preserves_static_fallback_and_exact_independent_metrics() {
    let f = Fixture::new(true);
    let world = f.world();
    let plain = parity(
        &f,
        Operation::SpatialRoute {
            route: "Harbor crossing".into(),
            at: None,
        },
        spatial::route_info(&world, "Harbor crossing", None).unwrap(),
        "/api/spatial/route?id=Harbor%20crossing",
        &["spatial", "route", "Harbor crossing"],
    )
    .await;
    assert_eq!(plain["availability"], "open");
    assert_eq!(plain["duration"]["value"], "5");
    assert!(plain["travelCost"].is_null());
    assert!(plain["routeDistance"].is_null());
    let dated = parity(
        &f,
        Operation::SpatialRoute {
            route: ROUTE.into(),
            at: Some(at(0)),
        },
        spatial::route_info(&world, ROUTE, Some(&point(0))).unwrap(),
        "/api/spatial/route?id=route%3Aharbor-landing&tick=9007199254740993&order=0",
        &[
            "spatial",
            "route",
            ROUTE,
            "--tick",
            "9007199254740993",
            "--order",
            "0",
        ],
    )
    .await;
    assert_eq!(dated["availability"], "closed");
    assert_eq!(dated["duration"]["value"], "7");
    assert_eq!(
        dated["travelCost"],
        json!({"value":"9007199254740993","unit":"credits"})
    );
    assert!(dated["routeDistance"].is_null());
    assert_eq!(
        dated["fieldCitations"]["availability"][0]["memberId"],
        "spatialhistory_00000000000000000000000011"
    );
    for order in [-2, 2] {
        let uri = format!("/api/spatial/route?id=route%3Aharbor-landing&tick={TICK}&order={order}");
        let (status, value) = get(f.service(), &uri).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["result"]["availability"], "open");
        assert_eq!(value["result"]["duration"]["value"], "5");
        assert!(value["result"]["travelCost"].is_null());
    }
}

#[tokio::test]
async fn historical_bbox_discovers_moved_in_geometry_before_filtering_and_limits() {
    let f = Fixture::new(true);
    let world = f.world();
    let plain =
        geometry::bbox_relation(&world, MAP, &[2.5, 3.5], &[3.5, 4.5], "within", 1).unwrap();
    assert!(plain["items"].as_array().unwrap().is_empty());
    let dated=parity(&f,Operation::SpatialBboxAt{map:MAP.into(),min:vec![2.5,3.5],max:vec![3.5,4.5],relation:"within".into(),at:at(0),limit:1},geometry::bbox_at(&world,MAP,&[2.5,3.5],&[3.5,4.5],"within",&point(0),1).unwrap(),"/api/spatial/bbox?map=map%3Aharbor&min_x=2.5&min_y=3.5&max_x=3.5&max_y=4.5&relation=within&tick=9007199254740993&order=0&limit=1",&["spatial","bbox",MAP,"--min-x","2.5","--min-y","3.5","--max-x","3.5","--max-y","4.5","--relation","within","--tick","9007199254740993","--order","0","--limit","1"]).await;
    assert_eq!(dated["items"].as_array().unwrap().len(), 1);
    assert_eq!(dated["items"][0]["locationId"], B);
    assert_eq!(dated["items"][0]["coordinates"], json!(["3", "4"]));
    assert_eq!(dated["items"][0]["citation"]["memberId"], CORRECTION);
    assert_eq!(dated["items"][0]["citations"].as_array().unwrap().len(), 2);
    let (status,value)=get(f.service(),"/api/spatial/bbox?map=map%3Aharbor&min_x=-10&min_y=-10&max_x=10&max_y=10&tick=9007199254740993&order=0&limit=1").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(value.get("result").is_none());
}

#[tokio::test]
async fn historical_nearby_uses_effective_coordinates_and_reverts_after_interval() {
    let f = Fixture::new(true);
    let world = f.world();
    let dated=parity(&f,Operation::SpatialNearbyAt{map:MAP.into(),point:vec![3.,4.],radius:0.,at:at(0),limit:1},geometry::nearby_at(&world,MAP,&[3.,4.],0.,&point(0),1).unwrap(),"/api/spatial/nearby?map=map%3Aharbor&x=3&y=4&radius=0&tick=9007199254740993&order=0&limit=1",&["spatial","nearby",MAP,"--x","3","--y","4","--radius","0","--tick","9007199254740993","--order","0","--limit","1"]).await;
    assert_eq!(dated["items"][0]["locationId"], B);
    assert_eq!(dated["items"][0]["distance"], 0.0);
    for (order, expected) in [(1, 0), (2, 1)] {
        let uri = format!(
            "/api/spatial/nearby?map=map%3Aother&x=8&y=8&radius=0&tick={TICK}&order={order}"
        );
        let (status, value) = get(f.service(), &uri).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["result"]["items"].as_array().unwrap().len(), expected);
    }
}

#[tokio::test]
async fn optional_time_and_author_parameters_are_closed_and_capability_is_required() {
    let f = Fixture::new(true);
    for uri in [
        "/api/spatial/history?record=location%3Alanding&order=0",
        "/api/spatial/history?record=location%3Alanding&timeline=main",
        "/api/spatial/history?record=location%3Alanding&tick=0&character=someone",
        "/api/spatial/history?record=location%3Alanding&tick=0&perspective=character",
        "/api/spatial/history?record=location%3Alanding&tick=0&audience=author",
        "/api/spatial/history?record=location%3Alanding&limit=0",
        "/api/spatial/history?record=location%3Alanding&limit=1001",
        "/api/spatial/history?record=location%3Alanding&tick=9223372036854775808",
        "/api/spatial/history?record=location%3Alanding&tick=0&order=-2147483649",
        "/api/spatial/history?record=location%3Alanding&tick=0&tick=1",
        "/api/spatial/history?record=location%3Alanding&tick=0&timeline=absent",
        "/api/spatial/route?id=route%3Aharbor-landing&timeline=main",
        "/api/spatial/route?id=route%3Aharbor-landing&order=0",
        "/api/spatial/route?id=route%3Aharbor-landing&limit=1",
        "/api/spatial/route?id=route%3Aharbor-landing&tick=0&view=author",
        "/api/spatial/bbox?map=map%3Aharbor&min_x=0&min_y=0&max_x=1&max_y=1&order=0",
        "/api/spatial/bbox?map=map%3Aharbor&min_x=0&min_y=0&max_x=1&max_y=1&timeline=main",
        "/api/spatial/nearby?map=map%3Aharbor&x=0&y=0&radius=0&order=0",
        "/api/spatial/nearby?map=map%3Aharbor&x=0&y=0&radius=0&tick=0&view=author",
        "/api/spatial/path?from=location%3Aharbor&to=location%3Alanding&tick=0",
        "/api/spatial/reachable?from=location%3Aharbor&tick=0",
        "/api/spatial/location?id=location%3Aharbor&tick=0",
    ] {
        let (status, value) = get(f.service(), uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {value}");
        assert!(value.get("result").is_none());
    }
    for args in [
        vec!["spatial", "history", B, "--order", "0"],
        vec!["spatial", "route", ROUTE, "--timeline", "main"],
        vec![
            "spatial",
            "nearby",
            MAP,
            "--x",
            "0",
            "--y",
            "0",
            "--radius",
            "0",
            "--timeline",
            "main",
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
            .args(["--compact", "--repo", f.root().to_str().unwrap()])
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
    let no_history = Fixture::new(false);
    let file = no_history.root().join("story/0.md");
    let text = fs::read_to_string(&file).unwrap();
    fs::write(file, text.replace(", spatial-history-v1", "")).unwrap();
    for uri in [
        "/api/spatial/history?record=location%3Alanding",
        "/api/spatial/route?id=route%3Aharbor-landing&tick=0",
        "/api/spatial/nearby?map=map%3Aharbor&x=0&y=0&radius=0&tick=0",
        "/api/spatial/bbox?map=map%3Aharbor&min_x=0&min_y=0&max_x=1&max_y=1&tick=0",
    ] {
        let (status, value) = get(no_history.service(), uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{value}");
        assert!(value.get("result").is_none());
    }
    assert_eq!(
        get(
            no_history.service(),
            "/api/spatial/route?id=route%3Aharbor-landing"
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn static_graph_and_complete_character_responses_ignore_authored_history() {
    let before = Fixture::new(false);
    let after = Fixture::new(true);
    for f in [&before, &after] {
        let (_,path)=get(f.service(),"/api/spatial/path?from=location%3Aharbor&to=location%3Alanding&metric=duration&unit=seconds").await;
        assert_eq!(path["result"]["found"], true);
        assert_eq!(path["result"]["total"]["value"], "5");
    }
    for uri in [
        format!(
            "/api/character/context?character={VIEWER}&tick={TICK}&order=0&maxCharacters=100000"
        ),
        format!("/api/character/search?character={VIEWER}&q=lamp&tick={TICK}&order=0"),
    ] {
        let old = get(before.service(), &uri).await;
        let new = get(after.service(), &uri).await;
        assert_eq!(old.0, StatusCode::OK, "{}", old.1);
        assert_eq!(old, new);
        assert!(new.1.get("fingerprint").is_none());
    }
}
