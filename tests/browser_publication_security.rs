//! The live browser boundary adds no alternate writer or source authority.
#[path = "support/actions.rs"]
mod base;

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, HeaderValue, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value as Json, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, SystemTime},
};
use tower::ServiceExt;
use wedl::{
    http, manuscript,
    model::World,
    revision_review,
    service::{Operation, Service},
    source,
};

const SAVE: &str = "/api/action-drafts/save";
const WORK: &str = "work_00000000000000000000000001";
const WRITE_ROUTES: [&str; 6] = [
    SAVE,
    "/api/action-drafts/apply-outcome",
    "/api/edit-drafts/save",
    "/api/edit-drafts/apply",
    "/api/manuscripts/save",
    "/api/retcon-reviews/save",
];

struct CliServer(Child);

impl CliServer {
    fn start(root: &Path, revision: Option<&str>, live_author: bool) -> (Self, SocketAddr) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_wedl-rs"));
        command.arg("--repo").arg(root);
        if let Some(revision) = revision {
            command.args(["--revision", revision]);
        }
        command.args(["serve", "--port", "0"]);
        if live_author {
            command.arg("--live-author");
        }
        let mut server = Self(
            command
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let stderr = BufReader::new(server.0.stderr.take().unwrap());
        let (send, receive) = mpsc::channel();
        thread::spawn(move || {
            for line in stderr.lines() {
                if matches!(&line, Ok(text) if text.trim().is_empty()) {
                    continue;
                }
                let _ = send.send(line);
                break;
            }
        });
        let line = receive
            .recv_timeout(Duration::from_secs(15))
            .unwrap()
            .unwrap();
        let address = line
            .strip_prefix("Ready: http://")
            .and_then(|ready| ready.split_once(' '))
            .unwrap_or_else(|| panic!("CLI did not report a listening address: {line}"))
            .0
            .parse()
            .unwrap();
        (server, address)
    }
}

impl Drop for CliServer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn cli_request(address: SocketAddr, method: &str, path: &str, body: &str) -> (u16, Json) {
    let timeout = Some(Duration::from_secs(5));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(5)).unwrap();
    stream.set_read_timeout(timeout).unwrap();
    stream.set_write_timeout(timeout).unwrap();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nOrigin: http://{address}\r\nX-WEDL-Write: 1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, serde_json::from_str(body).unwrap())
}

fn post(uri: &str, bytes: impl Into<Body>) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("host", "localhost:8765")
        .header("origin", "http://localhost:8765")
        .header("content-type", "application/json")
        .header("x-wedl-write", "1")
        .body(bytes.into())
        .unwrap()
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("host", "localhost:8765")
        .body(Body::empty())
        .unwrap()
}

async fn call(app: &Router, request: Request<Body>) -> (StatusCode, HeaderMap, Json) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or(Json::Null);
    (status, headers, value)
}

fn request(world: &World) -> String {
    serde_json::to_string(&base::request(
        world,
        base::draft(world, &base::id(1)),
        None,
    ))
    .unwrap()
}

fn image(root: &Path) -> BTreeMap<String, (Vec<u8>, SystemTime)> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .map(Result::unwrap)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| {
            (
                entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                (
                    fs::read(entry.path()).unwrap(),
                    fs::metadata(entry.path()).unwrap().modified().unwrap(),
                ),
            )
        })
        .collect()
}

fn encoded(value: &str) -> String {
    value.bytes().map(|byte| format!("%{byte:02X}")).collect()
}

#[test]
fn cli_worktree_serve_is_read_only_until_live_author_is_enabled() {
    let (_directory, world) = base::fixture();
    let (_read_only, address) = CliServer::start(&world.root, None, false);
    let (status, session) = cli_request(address, "GET", "/api/session", "");
    assert_eq!(status, 200);
    assert_eq!(
        session["result"],
        json!({"mode":"read-only-author","writesEnabled":false})
    );
    let (status, initial) = cli_request(address, "GET", "/api/status", "");
    assert_eq!(status, 200);
    assert_eq!(initial["fingerprint"], world.fingerprint);
    let before = image(&world.root);
    let valid_save = request(&world);
    for route in WRITE_ROUTES {
        let (status, result) = cli_request(address, "POST", route, &valid_save);
        assert_eq!(status, 405, "{route}: {result}");
    }
    assert_eq!(image(&world.root), before);

    let place = world.records.get(base::PLACE).unwrap();
    let source_path = world.root.join(&place.source_path);
    let original = fs::read_to_string(&source_path).unwrap();
    fs::write(
        &source_path,
        original.replace("A landing.", "A new landing."),
    )
    .unwrap();
    let current = source::load_world(&world.root, None).unwrap();
    let (status, refreshed) = cli_request(address, "GET", "/api/status", "");
    assert_eq!(status, 200);
    assert_ne!(current.fingerprint, world.fingerprint);
    assert_eq!(refreshed["fingerprint"], current.fingerprint);

    let (_live, address) = CliServer::start(&world.root, None, true);
    let (status, session) = cli_request(address, "GET", "/api/session", "");
    assert_eq!(status, 200);
    assert_eq!(
        session["result"],
        json!({"mode":"live-author","writesEnabled":true})
    );
    let (status, saved) = cli_request(address, "POST", SAVE, &request(&current));
    assert_eq!(status, 200, "{saved}");
    assert!(base::path(&current, &base::id(1)).is_file());
}

#[tokio::test]
async fn live_capabilities_are_read_only_and_fixed_or_historical_routers_remain_read_only() {
    let (_directory, world) = base::fixture();
    let before = image(&world.root);
    let live = http::live_router(Service::new(world.clone()));
    let (status, headers, value) = call(&live, get("/api/session")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["fingerprint"], world.fingerprint);
    assert_eq!(
        value["result"],
        json!({"mode":"live-author","writesEnabled":true})
    );
    assert_eq!(headers["cache-control"], "no-store");
    assert!(headers.get("access-control-allow-origin").is_none());
    assert_eq!(
        call(&live, get("/api/session?unknown=true")).await.0,
        StatusCode::BAD_REQUEST
    );
    let fixed = http::router(Service::new(world.clone()));
    let mut pinned = world.clone();
    pinned.revision = Some("a".repeat(40));
    let historical = http::live_router(Service::new(pinned));
    for app in [fixed, historical] {
        assert_eq!(
            call(&app, get("/api/session")).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            call(&app, post(SAVE, request(&world))).await.0,
            StatusCode::METHOD_NOT_ALLOWED
        );
    }
    assert_eq!(image(&world.root), before);
}

#[tokio::test]
async fn write_host_origin_and_custom_header_are_single_exact_and_mandatory() {
    let (_directory, world) = base::fixture();
    let app = http::live_router(Service::new(world.clone()));
    let raw = request(&world);
    let before = image(&world.root);
    for name in ["host", "origin", "x-wedl-write"] {
        let mut missing = post(SAVE, raw.clone());
        missing.headers_mut().remove(name);
        assert_eq!(
            call(&app, missing).await.0,
            StatusCode::FORBIDDEN,
            "missing {name}"
        );
        let mut duplicate = post(SAVE, raw.clone());
        let value = duplicate.headers()[name].clone();
        duplicate.headers_mut().append(name, value);
        assert_eq!(
            call(&app, duplicate).await.0,
            StatusCode::FORBIDDEN,
            "duplicate {name}"
        );
    }
    for (name, value) in [
        ("host", "example.invalid:8765"),
        ("host", "localhost.example.invalid:8765"),
        ("host", "localhost:08765"),
        ("host", "localhost:65536"),
        ("host", "user@localhost:8765"),
        ("host", "localhost.:8765"),
        ("origin", "null"),
        ("origin", "https://localhost:8765"),
        ("origin", "http://localhost:8766"),
        ("origin", "http://localhost:8765/"),
        ("origin", "http://localhost:8765/path"),
        ("origin", "http://localhost:8765?x=1"),
        ("origin", "http://localhost:8765#x"),
        ("origin", "http://user@localhost:8765"),
        ("origin", "http://localhost:8765.example.invalid"),
        ("x-wedl-write", "true"),
        ("x-wedl-write", "0"),
        ("x-wedl-write", "1,1"),
    ] {
        let mut bad = post(SAVE, raw.clone());
        bad.headers_mut()
            .insert(name, HeaderValue::from_str(value).unwrap());
        let (status, _, response) = call(&app, bad).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{name}:{value} {response}");
    }
    assert_eq!(image(&world.root), before);
    assert!(!world.root.join("author").exists());
    assert!(!world.root.join(".wedl-rust").exists());
}

#[tokio::test]
async fn canonical_loopback_origins_pass_the_guard_but_do_not_bypass_native_decoding() {
    let (_directory, world) = base::fixture();
    let app = http::live_router(Service::new(world.clone()));
    for host in [
        "localhost",
        "localhost:8765",
        "127.0.0.1:8765",
        "[::1]:8765",
    ] {
        let mut request = post(SAVE, "{}");
        request
            .headers_mut()
            .insert("host", HeaderValue::from_str(host).unwrap());
        request.headers_mut().insert(
            "origin",
            HeaderValue::from_str(&format!("http://{host}")).unwrap(),
        );
        let (status, _, value) = call(&app, request).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{host}: {value}");
        assert_eq!(value["code"], "usage_error");
    }
    assert!(!world.root.join("author").exists());
}

#[tokio::test]
async fn writes_require_json_and_cannot_be_triggered_by_forms_gets_or_preflight() {
    let (_directory, world) = base::fixture();
    let app = http::live_router(Service::new(world.clone()));
    let before = image(&world.root);
    let raw = request(&world);
    for media in [
        None,
        Some("text/plain"),
        Some("application/x-www-form-urlencoded"),
        Some("multipart/form-data; boundary=x"),
        Some("application/json, text/plain"),
    ] {
        let mut request = post(SAVE, raw.clone());
        request.headers_mut().remove("content-type");
        if let Some(media) = media {
            request
                .headers_mut()
                .insert("content-type", HeaderValue::from_str(media).unwrap());
        }
        assert_eq!(
            call(&app, request).await.0,
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
    }
    let mut duplicate = post(SAVE, raw.clone());
    duplicate
        .headers_mut()
        .append("content-type", HeaderValue::from_static("application/json"));
    assert_eq!(
        call(&app, duplicate).await.0,
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    for method in ["GET", "HEAD", "PUT", "PATCH", "DELETE", "OPTIONS"] {
        let mut request = post(SAVE, raw.clone());
        *request.method_mut() = method.parse().unwrap();
        let (status, headers, _) = call(&app, request).await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "{method}");
        assert!(headers.get("access-control-allow-origin").is_none());
        assert!(headers.get("access-control-allow-headers").is_none());
    }
    assert_eq!(image(&world.root), before);
    let mut accepted = post(SAVE, raw);
    accepted.headers_mut().insert(
        "content-type",
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    assert_eq!(call(&app, accepted).await.0, StatusCode::OK);
}

#[tokio::test]
async fn post_surface_is_an_exact_whitelist_and_has_no_raw_plan_or_record_writer() {
    let (_directory, world) = base::fixture();
    let app = http::live_router(Service::new(world.clone()));
    let before = image(&world.root);
    for route in [
        "/api/consequences/apply",
        "/api/record/create",
        "/api/record/update",
        "/api/body/apply",
        "/api/scene-control/apply",
        "/api/current-time/apply",
        "/api/action-outcomes/apply",
        "/api/action-drafts/save/extra",
        "/api/reader-presentation/preview",
        "/api/session",
    ] {
        assert_eq!(
            call(&app, post(route, request(&world))).await.0,
            StatusCode::METHOD_NOT_ALLOWED,
            "{route}"
        );
    }
    for route in WRITE_ROUTES {
        assert_eq!(
            call(&app, post(&format!("{route}?apply=true"), "{}"))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        let bypass =
            json!({"expectedFingerprint":world.fingerprint,"approved":true,"operations":[]})
                .to_string();
        assert_eq!(
            call(&app, post(route, bypass)).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(image(&world.root), before);
}

#[tokio::test]
async fn actual_body_bytes_are_bounded_before_decoding_on_every_writer() {
    let (_directory, world) = base::fixture();
    let app = http::live_router(Service::new(world.clone()));
    let before = image(&world.root);
    for (route, limit) in [
        (SAVE, 512 * 1024),
        ("/api/action-drafts/apply-outcome", 16 * 1024),
        ("/api/edit-drafts/save", 512 * 1024),
        ("/api/edit-drafts/apply", 16 * 1024),
        ("/api/manuscripts/save", 8 * 1024 * 1024),
        ("/api/retcon-reviews/save", 4 * 1024 * 1024),
    ] {
        let (status, _, value) = call(&app, post(route, vec![b' '; limit + 1])).await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{route}: {value}");
        assert_eq!(value["code"], "request_too_large");
    }
    assert_eq!(
        call(&app, post(SAVE, vec![b' '; 512 * 1024])).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(&app, post(SAVE, vec![0xff, 0xfe])).await.0,
        StatusCode::BAD_REQUEST
    );
    let duplicate = request(&world).replace(
        "\"expectedFingerprint\":",
        "\"expected\\u0046ingerprint\":\"bad\",\"expectedFingerprint\":",
    );
    assert_eq!(
        call(&app, post(SAVE, duplicate)).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(image(&world.root), before);
}

#[tokio::test]
async fn live_reads_and_writes_capture_current_source_without_repinning_stale_requests() {
    let (_directory, world) = base::fixture();
    let live = http::live_router(Service::new(world.clone()));
    let fixed = http::router(Service::new(world.clone()));
    let stale = request(&world);
    let changed = base::change_source(&world);
    let (status, _, value) = call(&live, get("/api/status")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["fingerprint"], changed.fingerprint);
    assert_eq!(
        call(&fixed, get("/api/status")).await.2["fingerprint"],
        world.fingerprint
    );
    assert_eq!(
        call(&live, get("/api/session")).await.2["fingerprint"],
        changed.fingerprint
    );
    let before = image(&world.root);
    assert_eq!(call(&live, post(SAVE, stale)).await.0, StatusCode::CONFLICT);
    assert_eq!(image(&world.root), before);
    assert_eq!(
        call(&live, post(SAVE, request(&changed))).await.0,
        StatusCode::OK
    );
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        changed.fingerprint
    );
}

#[tokio::test]
async fn concurrent_saves_with_one_prior_hash_have_one_exact_winner() {
    let (_directory, world) = base::fixture();
    let app = http::live_router(Service::new(world.clone()));
    let canonical_before = base::source_image(&world);
    let saved = call(&app, post(SAVE, request(&world))).await;
    assert_eq!(saved.0, StatusCode::OK);
    let hash = saved.2["result"]["sourceSha256"].as_str().unwrap();
    let mut left = base::draft(&world, &base::id(1));
    left.intent = "First explicit change.".into();
    let mut right = left.clone();
    right.intent = "Second explicit change.".into();
    let left_request = serde_json::to_string(&base::request(&world, left, Some(hash))).unwrap();
    let right_request = serde_json::to_string(&base::request(&world, right, Some(hash))).unwrap();
    let (left, right) = tokio::join!(
        call(&app, post(SAVE, left_request)),
        call(&app, post(SAVE, right_request))
    );
    assert!(matches!(
        (left.0, right.0),
        (StatusCode::OK, StatusCode::CONFLICT) | (StatusCode::CONFLICT, StatusCode::OK)
    ));
    let winner = if left.0 == StatusCode::OK {
        left.2
    } else {
        right.2
    };
    let detail = call(
        &app,
        get(&format!("/api/action-drafts/detail?id={}", base::id(1))),
    )
    .await;
    assert_eq!(detail.0, StatusCode::OK);
    assert_eq!(
        detail.2["result"]["sourceSha256"],
        winner["result"]["sourceSha256"]
    );
    assert_eq!(base::source_image(&world), canonical_before);
}

#[tokio::test]
async fn malformed_current_sources_and_pending_publications_never_fall_back_to_cached_worlds() {
    let (_directory, world) = base::fixture();
    let app = http::live_router(Service::new(world.clone()));
    let owner = world.world_record().unwrap();
    let path = world.root.join(&owner.source_path);
    fs::write(&path, "not a source record").unwrap();
    let before = image(&world.root);
    for uri in ["/api/status", "/api/validate", "/api/session"] {
        let (status, _, value) = call(&app, get(uri)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {value}");
        assert!(value.get("fingerprint").is_none());
    }
    assert_eq!(
        call(&app, post(SAVE, request(&world))).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(image(&world.root), before);
    fs::write(&path, &owner.raw_bytes).unwrap();
    let state = world.root.join(".wedl-rust");
    fs::create_dir_all(&state).unwrap();
    fs::write(state.join("pending-outcome.json"), "{}").unwrap();
    let before = image(&world.root);
    assert_eq!(call(&app, get("/api/status")).await.0, StatusCode::CONFLICT);
    assert_eq!(
        call(&app, post(SAVE, request(&world))).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(image(&world.root), before);
}

fn git(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

#[tokio::test]
async fn explicit_historical_manuscript_and_revision_reads_survive_live_parse_corruption() {
    let (_directory, world) = base::fixture();
    let work_path = world
        .root
        .join(manuscript::document_path("work", WORK).unwrap());
    fs::create_dir_all(work_path.parent().unwrap()).unwrap();
    fs::write(&work_path, format!("---\nschema: wedl-author/v1\nkind: work\nid: {WORK}\ntitle: Explicit original work\nduplicate_policy: allow-explicit-reuse\nchapters: []\n---\nOriginal private prose.\n")).unwrap();
    git(&world.root, &["init", "-q"]);
    git(&world.root, &["add", "--", "story", "author"]);
    git(
        &world.root,
        &["commit", "-q", "-m", "Original explicit sources"],
    );
    let revision = git(&world.root, &["rev-parse", "HEAD"]);
    let (_historical, address) = CliServer::start(&world.root, Some(&revision), true);
    let (status, snapshot) = cli_request(address, "GET", "/api/status", "");
    assert_eq!(status, 200);
    assert_eq!(snapshot["revision"], revision);
    assert_eq!(cli_request(address, "GET", "/api/session", "").0, 404);
    for route in WRITE_ROUTES {
        assert_eq!(cli_request(address, "POST", route, "{}").0, 405, "{route}");
    }
    let service = Service::new(world.clone());
    let app = http::live_router(service.clone());
    let list = json!({"revision":revision,"kind":null,"maxItems":100});
    let detail = json!({"revision":revision,"id":WORK,"expectedSha256":null});
    let resolve = json!({"revision":revision,"workId":WORK,"expectedAuthorFingerprint":null,"maxOccurrences":10,"maxCharacters":100000});
    let review = json!({"baseRevision":revision,"leftRevision":revision,"rightRevision":revision,
        "at":base::horizon(),"maxItems":100,"maxCharacters":100000});
    let original = [
        (
            "manuscripts/list",
            list.clone(),
            service
                .execute(Operation::ManuscriptList {
                    request: manuscript::decode_list(&list.to_string()).unwrap(),
                })
                .unwrap(),
        ),
        (
            "manuscripts/detail",
            detail.clone(),
            service
                .execute(Operation::ManuscriptDetail {
                    request: manuscript::decode_detail(&detail.to_string()).unwrap(),
                })
                .unwrap(),
        ),
        (
            "manuscripts/resolve",
            resolve.clone(),
            service
                .execute(Operation::ManuscriptResolve {
                    request: manuscript::decode_resolve(&resolve.to_string()).unwrap(),
                })
                .unwrap(),
        ),
        (
            "revision-review",
            review.clone(),
            service
                .execute(Operation::RevisionReview {
                    request: revision_review::decode(&review.to_string()).unwrap(),
                })
                .unwrap(),
        ),
    ];
    fs::write(
        world.root.join(&world.world_record().unwrap().source_path),
        "invalid live source",
    )
    .unwrap();
    let before = image(&world.root);
    for (route, input, expected) in original {
        let uri = format!("/api/{route}?request={}", encoded(&input.to_string()));
        let (status, _, value) = call(&app, get(&uri)).await;
        assert_eq!(status, StatusCode::OK, "{route}: {value}");
        assert_eq!(value, expected, "{route}");
        assert!(value.get("fingerprint").is_none());
        assert!(value.get("revision").is_none());
    }
    assert_eq!(
        call(&app, get("/api/status")).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(image(&world.root), before);
}
