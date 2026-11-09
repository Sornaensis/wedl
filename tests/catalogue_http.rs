//! One closed catalogue request is shared by native, CLI and read-only HTTP.
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::SystemTime,
};
use tempfile::TempDir;
use tower::ServiceExt;
use wedl::{
    catalogue, http,
    service::{Operation, Service},
    source,
};

const TICK: i64 = 9_007_199_254_740_993;
const WORLD: &str = "world_00000000000000000000000000";
const A: &str = "char_00000000000000000000000001";
const B: &str = "char_00000000000000000000000002";
const HIDDEN: &str = "char_00000000000000000000000003";
const MISSING: &str = "char_00000000000000000000000009";
const PLACE: &str = "loc_00000000000000000000000001";
const SCENE: &str = "scene_00000000000000000000000001";
const E1: &str = "event_00000000000000000000000001";
const FUTURE: &str = "event_00000000000000000000000002";
const THREAD: &str = "thread_00000000000000000000000001";
const OTHER_THREAD: &str = "thread_00000000000000000000000002";

fn point(order: i32) -> Value {
    json!({"timeline":"main","tick":TICK,"order":order})
}
fn record(root: &Path, kind: &str, id: &str, title: &str, fields: Value) {
    let mut front = json!({"schema":"wedl/v1","kind":kind,"id":id,"title":title});
    front
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let raw = format!("---\n{}---\n", serde_yaml::to_string(&front).unwrap()).replace('\n', "\r\n")
        + "Exact catalogue prose 🌲  \r\nNo final newline";
    fs::write(root.join("story").join(format!("{id}.md")), raw).unwrap();
}
fn fixture() -> TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("story")).unwrap();
    record(
        root.path(),
        "world",
        WORLD,
        "Catalogue fixture",
        json!({
            "default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],
            "current_time":point(7),"capabilities":[],
            "threads":[{"id":THREAD,"label":"Harbor"},{"id":OTHER_THREAD,"label":"Other"}],
            "state_keys":{"character":{"location":{"type":"entity","kind":"location"}}},"relationship_metrics":{}
        }),
    );
    record(root.path(), "location", PLACE, "Dock", json!({}));
    for (id, title) in [(A, "Ana"), (B, "Béa")] {
        record(
            root.path(),
            "character",
            id,
            title,
            json!({"initial_state":{"location":{"entity":PLACE}},"threads":[THREAD,OTHER_THREAD]}),
        );
    }
    record(
        root.path(),
        "character",
        HIDDEN,
        "PRIVATE_TITLE",
        json!({"initial_state":{},"audience":[]}),
    );
    record(
        root.path(),
        "scene",
        SCENE,
        "Dock scene",
        json!({
            "status":"active","location":PLACE,"time":{"start":point(0),"current":point(7)},
            "participants":[{"character":A},{"character":B}],"objects":[],"conversations":[],"observations":[],"environments":[],"outcome_events":[]
        }),
    );
    for (id, order) in [(E1, 1), (FUTURE, 9)] {
        record(
            root.path(),
            "event",
            id,
            if id == E1 {
                "Earlier signal"
            } else {
                "Future signal"
            },
            json!({
                "time":point(order),"location":PLACE,"participants":[{"character":A},{"character":B}],
                "causes":[],"effects":[],"related_story_points":[],"threads":[THREAD]
            }),
        );
    }
    root
}
fn service(root: &Path) -> Service {
    let service = Service::new(source::load_world(root, None).unwrap());
    let value = service.execute(Operation::Validate).unwrap();
    assert_eq!(value["result"]["valid"], true, "{value}");
    service
}
fn page(service: &Service) -> Value {
    json!({"protocol":"wedl-core-catalogue/v1","operation":"page","fingerprint":service.world().fingerprint,
        "at":{"timeline":"main","tick":TICK.to_string(),"order":"7"},"lens":{"kind":"author-as-of"},
        "filters":{"kinds":["character"],"profiles":[],"eras":[],"name":""},"sort":"name-asc",
        "q":null,"searchMode":null,"threadIds":[],"limit":1,"cursor":null})
}
fn labels(service: &Service) -> Value {
    json!({"protocol":"wedl-core-catalogue/v1","operation":"labels","fingerprint":service.world().fingerprint,
        "at":{"timeline":"main","tick":TICK.to_string(),"order":"7"},"lens":{"kind":"author-as-of"},
        "targets":[{"kind":"record","id":A},{"kind":"record","id":HIDDEN},{"kind":"record","id":MISSING}]})
}
fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
async fn get(app: axum::Router, operation: &str, raw: &str) -> (StatusCode, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/catalogue/{operation}?request={}",
                    encode(raw)
                ))
                .header("host", "localhost:8765")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}
fn cli(root: &Path, operation: &str, raw: &str) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .args([
            "--compact",
            "--repo",
            root.to_str().unwrap(),
            "catalogue",
            operation,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(raw.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn image(root: &Path) -> BTreeMap<String, (Vec<u8>, SystemTime)> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .map(Result::unwrap)
        .filter(|e| e.file_type().is_file())
        .map(|e| {
            (
                e.path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned(),
                (
                    fs::read(e.path()).unwrap(),
                    e.metadata().unwrap().modified().unwrap(),
                ),
            )
        })
        .collect()
}
async fn parity(root: &Path, service: &Service, request: &Value) -> Value {
    let raw = request.to_string();
    let name = request["operation"].as_str().unwrap();
    let native = catalogue::read(service.world(), &catalogue::decode(&raw).unwrap()).unwrap();
    let expected = service
        .execute(Operation::Catalogue {
            request: catalogue::decode(&raw).unwrap(),
        })
        .unwrap();
    assert_eq!(expected["result"], native);
    let (status, actual) = get(http::router(service.clone()), name, &raw).await;
    assert_eq!(status, StatusCode::OK, "{actual}");
    assert_eq!(actual, expected);
    assert_eq!(cli(root, name, &raw), expected);
    native
}

#[tokio::test]
async fn all_adapters_share_pages_labels_and_exact_continuation_without_writes() {
    let root = fixture();
    let service = service(root.path());
    let before = image(root.path());
    let mut request = page(&service);
    let first = parity(root.path(), &service, &request).await;
    assert_eq!(first["total"], 2);
    assert_eq!(first["items"][0]["id"], A);
    assert_eq!(first["context"]["at"]["tick"], TICK.to_string());
    assert!(first["nextCursor"].is_string());
    request["cursor"] = first["nextCursor"].clone();
    let next = parity(root.path(), &service, &request).await;
    assert_eq!(next["total"], 2);
    assert_eq!(next["items"][0]["id"], B);
    assert_eq!(next["facets"], first["facets"]);
    assert!(next["nextCursor"].is_null());
    let result = parity(root.path(), &service, &labels(&service)).await;
    assert_eq!(result["items"][0]["title"], "Ana");
    for row in &result["items"].as_array().unwrap()[1..] {
        assert_eq!(row["status"], "unavailable");
        assert!(row["title"].is_null());
        assert!(row["kind"].is_null());
    }
    assert!(!result.to_string().contains("PRIVATE_TITLE"));
    assert_eq!(image(root.path()), before);
    assert_eq!(
        fs::read_dir(root.path()).unwrap().count(),
        1,
        "reads must not create a cache or private author store"
    );
}

#[tokio::test]
async fn author_all_time_and_character_scene_lenses_are_explicit() {
    let root = fixture();
    let service = service(root.path());
    let mut request = page(&service);
    request["filters"]["kinds"] = json!(["event"]);
    request["limit"] = json!(100);
    assert_eq!(parity(root.path(), &service, &request).await["total"], 1);
    request["lens"] = json!({"kind":"author-all-time","timeline":"main"});
    request["at"] = Value::Null;
    let all_time = parity(root.path(), &service, &request).await;
    assert_eq!(all_time["total"], 2);
    assert!(
        all_time["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["id"] == FUTURE)
    );
    request = page(&service);
    request["lens"] = json!({"kind":"character","characterId":A,"sceneId":SCENE});
    request["limit"] = json!(100);
    let character = parity(root.path(), &service, &request).await;
    assert_eq!(character["total"], 2);
    assert!(!character.to_string().contains("PRIVATE_TITLE"));
    request["at"]["order"] = json!("8");
    let (status, response) = get(http::router(service), "page", &request.to_string()).await;
    assert_ne!(
        status,
        StatusCode::OK,
        "scene access must stop at the current position: {response}"
    );
}

#[tokio::test]
async fn route_command_and_request_fields_are_closed_before_source_selection() {
    let root = fixture();
    let service = service(root.path());
    let good = page(&service);
    let mut cases = vec![
        "{}".to_owned(),
        format!("{} trailing", good),
        format!("{{\"operation\":\"labels\",{}", &good.to_string()[1..]),
    ];
    for (key, value) in [
        ("scope", json!(["author"])),
        ("cursor", json!("not-a-cursor")),
        ("limit", json!(0)),
        ("limit", json!(101)),
        ("fingerprint", json!("wrong")),
    ] {
        let mut bad = good.clone();
        bad[key] = value;
        cases.push(bad.to_string());
    }
    for raw in cases {
        let (status, response) = get(http::router(service.clone()), "page", &raw).await;
        assert_ne!(status, StatusCode::OK, "{raw}: {response}");
        assert!(response.get("result").is_none());
    }
    let (status, _) = get(http::router(service.clone()), "labels", &good.to_string()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let file = tempfile::NamedTempFile::new().unwrap();
    fs::write(file.path(), good.to_string()).unwrap();
    let wrong = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .args([
            "--compact",
            "--repo",
            root.path().to_str().unwrap(),
            "catalogue",
            "labels",
            "--file",
            file.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!wrong.status.success());
    assert!(wrong.stdout.is_empty());
    let encoded = encode(&good.to_string());
    for query in [
        format!("request={encoded}&request={encoded}"),
        format!("request={encoded}&audience=author"),
    ] {
        assert!(http::operation("catalogue/page", Some(&query)).is_err());
    }
}

#[tokio::test]
async fn percent_encoded_transport_preserves_the_entire_decoded_byte_bound() {
    let root = fixture();
    let service = service(root.path());
    let raw = page(&service).to_string();
    let padded = format!(
        "{}{}",
        " ".repeat(catalogue::MAX_REQUEST_BYTES - raw.len()),
        raw
    );
    assert!(encode(&padded).len() > 16_384);
    let expected = service
        .execute(Operation::Catalogue {
            request: catalogue::decode(&padded).unwrap(),
        })
        .unwrap();
    let (status, result) = get(http::router(service.clone()), "page", &padded).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result, expected);
    assert_eq!(cli(root.path(), "page", &padded), expected);
    let (status, _) = get(http::router(service), "page", &format!(" {padded}")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        http::operation("entities", Some(&format!("text={}", "a".repeat(16_384)))).is_err(),
        "other routes retain their encoded query bound"
    );
}

#[tokio::test]
async fn live_source_changes_refuse_stale_requests_and_bound_continuation() {
    let root = fixture();
    let service = service(root.path());
    let app = http::live_router(service.clone());
    let mut request = page(&service);
    let (status, first) = get(app.clone(), "page", &request.to_string()).await;
    assert_eq!(status, StatusCode::OK);
    request["cursor"] = first["result"]["nextCursor"].clone();
    let file = root.path().join(format!("story/{B}.md"));
    let changed = fs::read_to_string(&file)
        .unwrap()
        .replace("title: Béa", "title: Changed Béa");
    fs::write(&file, changed).unwrap();
    let before = image(root.path());
    let (status, response) = get(app.clone(), "page", &request.to_string()).await;
    assert_ne!(
        status,
        StatusCode::OK,
        "stale source must be refused: {response}"
    );
    let current = source::load_world(root.path(), None).unwrap();
    assert_ne!(current.fingerprint, service.world().fingerprint);
    request["fingerprint"] = json!(current.fingerprint);
    let (status, response) = get(app.clone(), "page", &request.to_string()).await;
    assert_ne!(
        status,
        StatusCode::OK,
        "old continuation must be refused on new source: {response}"
    );
    request["cursor"] = Value::Null;
    let (status, response) = get(app, "page", &request.to_string()).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(image(root.path()), before);
}
