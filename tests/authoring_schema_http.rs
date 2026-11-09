//! Static authoring guidance must be identical across adapters and independent
//! of canonical source capture, validation, Git state and publication fences.
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path, process::Command, time::SystemTime};
use tower::ServiceExt;
use wedl::{
    authoring_schema, http,
    service::{Operation, Service},
    source,
};

fn expected() -> Value {
    json!({"protocol":wedl::CONTRACT,"result":authoring_schema::schema()})
}

fn image(root: &Path) -> BTreeMap<std::path::PathBuf, (Vec<u8>, SystemTime)> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .map(|entry| entry.unwrap())
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
fn cli_guidance_succeeds_without_opening_the_selected_repository_or_git() {
    let temporary = tempfile::tempdir().unwrap();
    fs::write(temporary.path().join("sentinel"), "exact caller bytes\r\n").unwrap();
    let nonexistent = temporary.path().join("not-a-world");
    let before = image(temporary.path());
    for compact in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_wedl-rs"));
        command
            .current_dir(temporary.path())
            .arg("--repo")
            .arg(&nonexistent);
        if compact {
            command.arg("--compact");
        }
        let output = command
            .args(["authoring", "schema"])
            .env("GIT_TRACE", temporary.path().join("unexpected-trace"))
            .env("GIT_CONFIG_GLOBAL", temporary.path().join("sentinel"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            expected()
        );
        assert_eq!(image(temporary.path()), before);
        assert!(!nonexistent.exists());
    }
}

#[test]
fn cli_static_guidance_refuses_revision_and_unknown_selection() {
    let temporary = tempfile::tempdir().unwrap();
    for arguments in [
        vec!["--revision", "HEAD", "authoring", "schema"],
        vec!["authoring", "schema", "--kind", "character"],
        vec!["authoring", "unknown"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
            .current_dir(temporary.path())
            .args(arguments)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["code"], "usage_error");
        assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 0);
    }
}

#[tokio::test]
async fn native_and_both_author_routers_share_guidance_without_source_metadata_or_writes() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("world");
    let world = source::init_world(&root, "Private author title").unwrap();
    let service = Service::new(world);
    let before = image(temporary.path());
    assert_eq!(
        service.execute(Operation::AuthoringSchema).unwrap(),
        expected()
    );
    for router in [http::router(service.clone()), http::live_router(service)] {
        let (status, actual) = request(router, Method::GET, "/api/authoring/schema").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(actual, expected());
        assert!(actual.get("fingerprint").is_none());
        assert!(actual.get("revision").is_none());
        assert!(!actual.to_string().contains("Private author title"));
        assert_eq!(image(temporary.path()), before);
    }
}

#[tokio::test]
async fn static_reference_survives_unavailable_source_while_current_reads_refuse() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("world");
    let mut world = source::init_world(&root, "Unavailable source control").unwrap();
    world.records.values_mut().next().unwrap().title = "Invalid parsed/raw projection".into();
    let service = Service::new(world);
    assert_eq!(
        service.execute(Operation::AuthoringSchema).unwrap(),
        expected()
    );
    assert!(
        service
            .execute(Operation::EntityList {
                kind: None,
                text: None
            })
            .is_err()
    );
    // The deliberate fixture deletion is before the preservation snapshot.
    fs::remove_dir_all(root.join("story")).unwrap();
    let before = image(temporary.path());
    let router = http::live_router(service);
    let (status, actual) = request(router.clone(), Method::GET, "/api/authoring/schema").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(actual, expected());
    let (status, _) = request(router, Method::GET, "/api/status").await;
    assert_ne!(status, StatusCode::OK);
    assert_eq!(image(temporary.path()), before);
    assert!(!root.join("story").exists());
}

#[tokio::test]
async fn guidance_http_is_closed_and_read_only() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("world");
    let service = Service::new(source::init_world(&root, "Closed reference").unwrap());
    let before = image(temporary.path());
    for router in [http::router(service.clone()), http::live_router(service)] {
        for uri in [
            "/api/authoring/schema?revision=HEAD",
            "/api/authoring/schema?kind=character",
            "/api/authoring/schema?kind=world&kind=character",
            "/api/authoring/schema?request=%ZZ",
        ] {
            let (status, body) = request(router.clone(), Method::GET, uri).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
            assert_eq!(body["code"], "usage_error");
        }
        let (status, _) = request(router, Method::POST, "/api/authoring/schema").await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    }
    assert_eq!(image(temporary.path()), before);
}
