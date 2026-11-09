//! The author helpers share captured source and perform reads only.
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output},
    time::SystemTime,
};
use tower::ServiceExt;
use wedl::{
    authoring_tools::{self, Family, ScaffoldRequest},
    http,
    model::World,
    record_edit,
    service::{Operation, Service},
    source, validation,
};

fn fixture() -> (tempfile::TempDir, World, String) {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("world");
    let world = source::init_world(&root, "Author helper adapter fixture").unwrap();
    let owner = world.world_record().unwrap();
    let mut fields = owner.frontmatter.clone();
    fields["capabilities"] =
        serde_yaml::to_value(["generational-core-v1", "descriptions-v1"]).unwrap();
    fields["current_time"] =
        serde_yaml::from_str("timeline: main\ntick: 9007199254740993\norder: 7\n").unwrap();
    fs::write(
        root.join(&owner.source_path),
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&fields).unwrap(),
            owner.body
        ),
    )
    .unwrap();
    let world = source::load_world(&root, None).unwrap();
    let id = format!("char_{:026}", 1);
    let character = record_edit::create(&world, &format!("kind: character\nid: {id}\ntitle: Exact character\nbody: \"Retained body\\r\\nwithout a final newline\"")).unwrap();
    let path = root.join(&character.source_path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, character.raw_bytes).unwrap();
    let world = source::load_world(root, None).unwrap();
    assert!(validation::validate_world(&world).valid);
    (temporary, world, id)
}

fn image(root: &Path) -> BTreeMap<std::path::PathBuf, (Vec<u8>, SystemTime)> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .map(Result::unwrap)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| {
            (
                entry.path().strip_prefix(root).unwrap().to_owned(),
                (
                    fs::read(entry.path()).unwrap(),
                    entry.metadata().unwrap().modified().unwrap(),
                ),
            )
        })
        .collect()
}

fn cli(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .arg("--compact")
        .arg("--repo")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}

fn input(world: &World, family: Family, entity: Option<String>) -> ScaffoldRequest {
    ScaffoldRequest {
        expected_fingerprint: world.fingerprint.clone(),
        family,
        entity,
        id: None,
        title: None,
    }
}

fn uri(input: &ScaffoldRequest) -> String {
    let encoded: String = serde_json::to_string(input)
        .unwrap()
        .bytes()
        .map(|byte| format!("%{byte:02X}"))
        .collect();
    format!("/api/authoring/scaffold?request={encoded}")
}

async fn request(router: Router, method: Method, uri: &str) -> (StatusCode, Value) {
    let response = router
        .oneshot(
            Request::builder()
                .method(method)
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

#[test]
fn cli_uses_the_shared_capture_and_returns_editable_exact_pinned_plans() {
    let (temporary, world, id) = fixture();
    let before = image(temporary.path());
    let service = Service::new(world.clone());
    let expected = service
        .execute(Operation::AuthoringContext { max_items: 4096 })
        .unwrap();
    assert_eq!(
        expected["result"],
        authoring_tools::context(&world, 4096).unwrap()
    );
    assert_eq!(
        success(cli(&world.root, &["authoring", "context"])),
        expected
    );
    assert_eq!(
        expected["result"]["currentTime"]["tick"],
        "9007199254740993"
    );
    for args in [
        vec![
            "authoring",
            "scaffold",
            "--family",
            "descriptions",
            "--entity",
            &id,
        ],
        vec![
            "authoring",
            "scaffold",
            "--family",
            "descriptions",
            "--entity",
            &id,
            "--expected-fingerprint",
            &world.fingerprint,
        ],
    ] {
        let actual = success(cli(&world.root, &args));
        let expected = service
            .execute(Operation::AuthoringScaffold {
                request: input(&world, Family::Descriptions, Some(id.clone())),
            })
            .unwrap();
        assert_eq!(actual, expected);
        let plan = wedl::consequences::decode(&actual["result"]["plan"].to_string()).unwrap();
        assert_eq!(
            wedl::consequences::preview(&world, &plan, 100000).unwrap()["canonicalNoOp"],
            true
        );
    }
    let default = success(cli(&world.root, &["authoring", "scaffold"]));
    assert_eq!(
        default["result"]["target"]["id"],
        world.world_record().unwrap().id
    );
    let stale = cli(
        &world.root,
        &[
            "authoring",
            "scaffold",
            "--expected-fingerprint",
            &"0".repeat(64),
        ],
    );
    assert_eq!(stale.status.code(), Some(2));
    assert!(stale.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&stale.stderr).unwrap()["code"],
        "conflict"
    );
    assert_eq!(image(temporary.path()), before);
}

#[tokio::test]
async fn native_cli_and_both_routers_return_the_same_context_and_starters_without_writes() {
    let (temporary, world, id) = fixture();
    let before = image(temporary.path());
    let service = Service::new(world.clone());
    let context = service
        .execute(Operation::AuthoringContext { max_items: 4096 })
        .unwrap();
    let mut creation = input(&world, Family::Generational, None);
    creation.id = Some(format!("organization_{:026}", 9));
    creation.title = Some("New explicit organization".into());
    let cases = [
        input(&world, Family::Changeset, None),
        input(&world, Family::Descriptions, Some(id)),
        creation.clone(),
    ];
    for router in [
        http::router(service.clone()),
        http::live_router(service.clone()),
    ] {
        let (status, actual) = request(
            router.clone(),
            Method::GET,
            "/api/authoring/context?maxItems=4096",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(actual, context);
        for input in &cases {
            let expected = service
                .execute(Operation::AuthoringScaffold {
                    request: input.clone(),
                })
                .unwrap();
            let (status, actual) = request(router.clone(), Method::GET, &uri(input)).await;
            assert_eq!(status, StatusCode::OK, "{actual}");
            assert_eq!(actual, expected);
            assert_eq!(
                actual["fingerprint"],
                actual["result"]["plan"]["expectedFingerprint"]
            );
            assert_eq!(actual["result"]["writesPerformed"], false);
        }
    }
    let actual = success(cli(
        &world.root,
        &[
            "authoring",
            "scaffold",
            "--family",
            "generational",
            "--id",
            creation.id.as_deref().unwrap(),
            "--title",
            creation.title.as_deref().unwrap(),
        ],
    ));
    assert_eq!(
        actual,
        service
            .execute(Operation::AuthoringScaffold { request: creation })
            .unwrap()
    );
    assert_eq!(actual["result"]["at"]["tick"], "9007199254740993");
    assert_eq!(image(temporary.path()), before);
}

#[tokio::test]
async fn live_capture_changes_choices_and_refuses_stale_scaffolds_without_falling_back() {
    let (temporary, world, id) = fixture();
    let service = Service::new(world.clone());
    let old_input = input(&world, Family::Descriptions, Some(id.clone()));
    let immutable = http::router(service.clone());
    let live = http::live_router(service);
    let record = &world.records[&id];
    let updated = record_edit::update(
        &world,
        &id,
        "kind: character\ntitle: Changed source title\n",
    )
    .unwrap();
    fs::write(world.root.join(&record.source_path), updated.raw_bytes).unwrap();
    let current = source::load_world(&world.root, None).unwrap();
    let before = image(temporary.path());
    let (status, context) = request(live.clone(), Method::GET, "/api/authoring/context").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(context["fingerprint"], current.fingerprint);
    assert_ne!(context["fingerprint"], world.fingerprint);
    assert!(
        context["result"]["records"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["target"]["id"] == id && row["label"] == "Changed source title")
    );
    let (status, error) = request(live, Method::GET, &uri(&old_input)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["code"], "conflict");
    let (status, original) = request(immutable, Method::GET, &uri(&old_input)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(original["fingerprint"], world.fingerprint);
    assert_eq!(image(temporary.path()), before);
}

#[tokio::test]
async fn unavailable_and_invalid_sources_refuse_context_and_scaffolds_but_static_reference_survives()
 {
    let (temporary, mut world, id) = fixture();
    let input = input(&world, Family::Descriptions, Some(id.clone()));
    world.records.get_mut(&id).unwrap().title = "Forged parsed title".into();
    let service = Service::new(world.clone());
    assert!(
        service
            .execute(Operation::AuthoringContext { max_items: 4096 })
            .is_err()
    );
    assert!(
        service
            .execute(Operation::AuthoringScaffold {
                request: input.clone()
            })
            .is_err()
    );
    fs::remove_dir_all(world.root.join("story")).unwrap();
    let before = image(temporary.path());
    let live = http::live_router(service);
    for uri in ["/api/authoring/context".to_owned(), uri(&input)] {
        let (status, _) = request(live.clone(), Method::GET, &uri).await;
        assert_ne!(status, StatusCode::OK);
    }
    assert_eq!(
        request(live, Method::GET, "/api/authoring/schema").await.0,
        StatusCode::OK
    );
    assert_eq!(image(temporary.path()), before);
    assert!(!world.root.join("story").exists());
}

#[tokio::test]
async fn helper_routes_are_closed_bounded_and_get_only() {
    let (temporary, world, _) = fixture();
    let before = image(temporary.path());
    let service = Service::new(world);
    for router in [http::router(service.clone()), http::live_router(service)] {
        for uri in [
            "/api/authoring/context?maxItems=0",
            "/api/authoring/context?maxItems=4097",
            "/api/authoring/context?maxItems=1",
            "/api/authoring/context?maxItems=4096&maxItems=4096",
            "/api/authoring/context?revision=HEAD",
            "/api/authoring/scaffold",
            "/api/authoring/scaffold?request=%ZZ",
            "/api/authoring/scaffold?request=%7B%7D&entity=x",
            "/api/authoring/scaffold?request=%7B%7D&request=%7B%7D",
        ] {
            let (status, error) = request(router.clone(), Method::GET, uri).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {error}");
            assert_eq!(error["code"], "usage_error");
        }
        for uri in ["/api/authoring/context", "/api/authoring/scaffold"] {
            assert_eq!(
                request(router.clone(), Method::POST, uri).await.0,
                StatusCode::METHOD_NOT_ALLOWED
            );
        }
    }
    assert_eq!(image(temporary.path()), before);
}

#[test]
fn historical_cli_discovery_and_scaffolds_keep_the_selected_original_capture() {
    let (temporary, world, id) = fixture();
    for args in [
        vec!["init", "-q"],
        vec!["add", "."],
        vec![
            "-c",
            "user.name=Author Test",
            "-c",
            "user.email=author@example.invalid",
            "commit",
            "-qm",
            "Fixture",
        ],
    ] {
        let output = Command::new("git")
            .arg("-C")
            .arg(&world.root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let original = source::load_world(&world.root, Some("HEAD")).unwrap();
    let changed =
        record_edit::update(&world, &id, "kind: character\ntitle: Current source only\n").unwrap();
    fs::write(world.root.join(&changed.source_path), changed.raw_bytes).unwrap();
    let before = image(temporary.path());
    let context = success(cli(
        &world.root,
        &["--revision", "HEAD", "authoring", "context"],
    ));
    assert_eq!(
        context,
        Service::new(original.clone())
            .execute(Operation::AuthoringContext { max_items: 4096 })
            .unwrap()
    );
    let scaffold = success(cli(
        &world.root,
        &[
            "--revision",
            "HEAD",
            "authoring",
            "scaffold",
            "--entity",
            &id,
        ],
    ));
    assert_eq!(scaffold["revision"], json!(original.revision));
    assert_eq!(scaffold["result"]["fingerprint"], original.fingerprint);
    assert!(
        scaffold["result"]["retainedSource"]
            .as_str()
            .unwrap()
            .contains("Exact character")
    );
    assert_eq!(image(temporary.path()), before);
}
