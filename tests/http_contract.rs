use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;
use wedl::{
    http::{self, INDEX},
    model::World,
    service::{Operation, Service, TimeInput},
    source,
};

fn world() -> World {
    source::load_world(
        concat!(env!("CARGO_MANIFEST_DIR"), "/examples/tideglass"),
        None,
    )
    .unwrap()
}

async fn response(service: Service, uri: &str) -> (StatusCode, Value) {
    router_response(http::router(service), uri).await
}

async fn router_response(router: axum::Router, uri: &str) -> (StatusCode, Value) {
    let response = router
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
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn http_and_shared_service_return_identical_source() {
    let service = Service::new(world());
    let expected = service
        .execute(Operation::EntityShow {
            entity: "Mina Vale".into(),
        })
        .unwrap();
    let (status, actual) = response(service, "/api/entity?id=Mina%20Vale").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(actual, expected);
    assert_eq!(actual["protocol"], wedl::CONTRACT);
    assert!(actual["result"]["frontmatterYaml"].is_string());
}

#[tokio::test]
async fn unknown_and_duplicate_query_fields_never_succeed() {
    let service = Service::new(world());
    for path in [
        "/api/search?q=gate&perspective=character",
        "/api/search?q=gate&at=20",
        "/api/entities?kind=character&kind=event",
        "/api/knowledge?character=Mina&tick=20&includeForgotten=yes",
        "/api/status?arbitrary=true",
        "/api/timeline?order=1",
        "/api/state?entity=Mina&tick=9223372036854775808",
        "/api/entity?id=%ZZ",
        "/api/entities/%FF",
        "/api/story-points",
        "/api/causal?tick=20",
        "/api/environments?entity=Mina&tick=20&inferLocation=true",
    ] {
        let (status, body) = response(service.clone(), path).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}: {body}");
        assert_eq!(body["code"], "usage_error");
    }
}

#[tokio::test]
async fn story_routes_preserve_explicit_selectors_and_horizon() {
    let world = world();
    let event = world.resolve("Log Discovery").unwrap().id.clone();
    let target = world
        .resolve("Quay Flood")
        .unwrap()
        .list("targets")
        .unwrap()[0]
        .as_str()
        .unwrap()
        .to_owned();
    let service = Service::new(world);
    let at = TimeInput {
        tick: 20,
        order: i32::MAX,
        timeline: None,
    };
    for (uri, operation) in [
        (
            "/api/story-points?tick=20".to_owned(),
            Operation::StoryPoints {
                at: at.clone(),
                scene: None,
                limit: 1000,
            },
        ),
        (
            format!("/api/causal?event={event}&tick=20"),
            Operation::Causal {
                event,
                at: at.clone(),
                direction: "upstream".into(),
                limit: 1000,
            },
        ),
        (
            format!("/api/environments?entity={target}&tick=20"),
            Operation::Environments { entity: target, at },
        ),
    ] {
        let expected = service.execute(operation).unwrap();
        let (status, actual) = response(service.clone(), &uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {actual}");
        assert_eq!(actual, expected);
    }
}

fn cli_json(args: &[&str]) -> (i32, Value) {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
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
            .unwrap_or_else(|_| panic!("non-JSON CLI output: {}", String::from_utf8_lossy(bytes))),
    )
}

#[test]
fn cli_initializes_previews_and_applies_one_confirmed_body_edit() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("new-world");
    let path = root.to_str().unwrap();
    let (code, initialized) = cli_json(&["--compact", "init", path, "--title", "Small World"]);
    assert_eq!(code, 0, "{initialized}");
    assert_eq!(initialized["result"]["sourceSchema"], source::SOURCE_SCHEMA);
    let fingerprint = initialized["fingerprint"].as_str().unwrap();
    let body_path = temporary.path().join("body.md");
    let body = "# Small World\n\nAn exact replacement with no final newline.";
    std::fs::write(&body_path, body).unwrap();
    let body_file = body_path.to_str().unwrap();
    let before = std::fs::read(root.join("story/world.md")).unwrap();
    let (code, preview) = cli_json(&[
        "--compact",
        "--repo",
        path,
        "preview-body",
        "Small World",
        "--file",
        body_file,
        "--expected-fingerprint",
        fingerprint,
    ]);
    assert_eq!(code, 0, "{preview}");
    assert_eq!(std::fs::read(root.join("story/world.md")).unwrap(), before);
    let digest = preview["result"]["afterSha256"].as_str().unwrap();
    let (code, applied) = cli_json(&[
        "--compact",
        "--repo",
        path,
        "apply-body",
        "Small World",
        "--file",
        body_file,
        "--expected-fingerprint",
        fingerprint,
        "--expected-after-sha256",
        digest,
    ]);
    assert_eq!(code, 0, "{applied}");
    let loaded = source::load_world(&root, None).unwrap();
    assert_eq!(loaded.world_record().unwrap().body, body);
    assert_ne!(loaded.fingerprint, fingerprint);
    let (code, stale) = cli_json(&[
        "--compact",
        "--repo",
        path,
        "apply-body",
        "Small World",
        "--file",
        body_file,
        "--expected-fingerprint",
        fingerprint,
        "--expected-after-sha256",
        digest,
    ]);
    assert_eq!(code, 2);
    assert_eq!(stale["code"], "conflict");
    assert_eq!(
        source::load_world(&root, None)
            .unwrap()
            .world_record()
            .unwrap()
            .body,
        body
    );
    let blocked = temporary.path().join("historical-init");
    let (code, error) = cli_json(&[
        "--compact",
        "--revision",
        "HEAD",
        "init",
        blocked.to_str().unwrap(),
    ]);
    assert_eq!(code, 2);
    assert_eq!(error["code"], "usage_error");
    assert!(!blocked.exists());
}

#[tokio::test]
async fn not_found_and_post_errors_are_structured() {
    let service = Service::new(world());
    let (status, body) = response(service.clone(), "/api/entity?id=no-such-entity").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "not_found");
    let (status, body) = response(service.clone(), "/api/not-implemented").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "not_found");
    let response = http::router(service)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/status")
                .header("host", "localhost:8765")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["code"], "method_not_allowed");
}

#[tokio::test]
async fn nested_ids_and_opaque_yaml_are_preserved() {
    let mut world = world();
    let raw = "---\nschema: wedl/v1\nkind: location\nid: location:town/harbor\ntitle: A nested harbor\ndomain: setting\nstatus: canonical\ntags: []\naliases: []\nx-opaque:\n  9223372036854775807: exact integer key\n---\n</script><img src=x onerror=alert(1)>\n";
    let record = source::parse_record(raw.as_bytes(), "story/locations/nested.md").unwrap();
    world.records.insert(record.id.clone(), record);
    let world = source::world_from_records(
        world.root,
        world.records.into_values().collect(),
        world.revision,
    )
    .unwrap();
    let (status, body) = response(
        Service::new(world),
        "/api/entity?id=location%3Atown%2Fharbor",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["rawSource"], raw);
    assert!(
        body["result"]["frontmatterYaml"]
            .as_str()
            .unwrap()
            .contains("9223372036854775807")
    );
    assert!(
        body["result"]["bodyMarkdown"]
            .as_str()
            .unwrap()
            .contains("<img")
    );
    assert!(!INDEX.contains("innerHTML"));
    assert!(!INDEX.contains("document.write"));
    assert!(INDEX.contains(".textContent = record.title"));
}

#[tokio::test]
async fn local_author_server_rejects_rebinding_hosts_and_foreign_origins() {
    let service = Service::new(world());
    for (host, origin) in [
        ("attacker.invalid:8765", None),
        ("localhost:8765", Some("https://attacker.invalid")),
        ("localhost:8765", Some("http://localhost:9999")),
    ] {
        let mut request = Request::builder().uri("/api/status").header("host", host);
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        let response = http::router(service.clone())
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    let response = http::router(service)
        .oneshot(
            Request::builder()
                .uri("/")
                .header("host", "localhost:8765")
                .header("origin", "http://localhost:8765")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_none()
    );
}

#[tokio::test]
async fn invalid_world_allows_diagnostics_but_not_reads() {
    let mut world = world();
    let id = world.world_record().unwrap().id.clone();
    world.records.get_mut(&id).unwrap().frontmatter["default_timeline"] =
        serde_yaml::Value::String("undeclared".into());
    let service = Service::new(world);
    let (status, body) = response(service.clone(), "/api/validate").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"]["valid"], false);
    let (status, body) = response(service, "/api/entities").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "validation_error");
}

#[tokio::test]
async fn live_worktree_reuse_preserves_exact_refresh_and_refusals() {
    use std::fs;

    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().join("world");
    let mut startup = source::init_world(&root, "Live world").unwrap();
    let world_id = startup.world_record().unwrap().id.clone();
    startup.records.get_mut(&world_id).unwrap().title = "Untrusted startup".into();
    let app = http::live_router(Service::new(startup));

    let (status, initial) = router_response(app.clone(), "/api/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(initial["result"]["world"], "Live world");
    assert_eq!(initial["result"]["validation"]["valid"], true);

    let path = root.join("story/z-old.md");
    let raw = "---\nschema: wedl/v1\nkind: object\nid: obj_00000000000000000000000001\ntitle: Live lamp\ndomain: setting\nstatus: canonical\ntags: []\naliases: []\n---\nOld lamp\n";
    fs::write(&path, raw).unwrap();
    let uri = "/api/entity?id=obj_00000000000000000000000001";
    let (status, added) = router_response(app.clone(), uri).await;
    assert_eq!(status, StatusCode::OK, "{added}");
    assert_eq!(added["result"]["bodyMarkdown"], "Old lamp\n");
    assert_ne!(added["fingerprint"], initial["fingerprint"]);
    assert_eq!(
        router_response(app.clone(), uri).await,
        (StatusCode::OK, added.clone())
    );

    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let edited = raw.replace("Old lamp", "New lamp");
    fs::write(&path, &edited).unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    let (status, changed) = router_response(app.clone(), uri).await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["result"]["bodyMarkdown"], "New lamp\n");
    assert_ne!(changed["fingerprint"], added["fingerprint"]);

    let renamed = root.join("story/z-new.md");
    fs::rename(&path, &renamed).unwrap();
    let (status, moved) = router_response(app.clone(), uri).await;
    assert_eq!(status, StatusCode::OK, "{moved}");
    assert_eq!(moved["result"]["path"], "story/z-new.md");
    assert_ne!(moved["fingerprint"], changed["fingerprint"]);

    fs::write(
        &renamed,
        edited.replace("status: canonical", "status: invalid"),
    )
    .unwrap();
    let (status, invalid) = router_response(app.clone(), "/api/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(invalid["result"]["validation"]["valid"], false);
    let (status, refusal) = router_response(app.clone(), uri).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(refusal["code"], "validation_error");

    fs::write(&renamed, edited).unwrap();
    assert_eq!(
        router_response(app.clone(), uri).await,
        (StatusCode::OK, moved.clone())
    );
    let state = root.join(".wedl-rust");
    fs::create_dir_all(&state).unwrap();
    let journal = state.join("pending-outcome.json");
    fs::write(&journal, b"pending").unwrap();
    let (status, refusal) = router_response(app.clone(), uri).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(refusal["message"].as_str().unwrap().contains("pending"));
    fs::remove_file(journal).unwrap();

    assert_eq!(
        router_response(app.clone(), uri).await,
        (StatusCode::OK, moved)
    );
    fs::remove_file(renamed).unwrap();
    let (status, _) = router_response(app, uri).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[test]
fn time_query_keeps_full_integer_precision_and_bind_is_loopback_only() {
    let operation = http::operation(
        "state",
        Some("entity=Mina&tick=-9223372036854775808&order=-2147483648"),
    )
    .unwrap();
    let Operation::State { at, .. } = operation else {
        panic!("wrong operation")
    };
    assert_eq!(at.tick, i64::MIN);
    assert_eq!(at.order, i32::MIN);
    assert!(http::loopback_address("127.0.0.1", 0).is_ok());
    assert!(http::loopback_address("::1", 8765).is_ok());
    assert!(http::loopback_address("0.0.0.0", 8765).is_err());
    assert!(http::loopback_address("attacker.invalid", 8765).is_err());
}
