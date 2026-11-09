//! Manuscript source, ordering and private saves share one native contract across
//! direct APIs, service, CLI and read-only author HTTP routes.
#[path = "support/actions.rs"]
mod fixture;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value as Json, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output},
};
use tower::ServiceExt;
use wedl::{
    http, manuscript as m,
    model::World,
    service::{Operation, Service},
    source, validation,
};

const FIRST: &str = "segment_00000000000000000000000001";
const SECOND: &str = "segment_00000000000000000000000002";
const WORK: &str = "work_00000000000000000000000001";
const CHAPTER: &str = "chapter_00000000000000000000000001";

fn segment(id: &str, placed: bool) -> String {
    format!(
        "---\r\nschema: wedl-author/v1\r\nkind: manuscript-segment\r\nid: {id}\r\ntitle: Private needleword segment\r\nsource_links:\r\n  - {{kind: section, owner_id: {}, id: {}}}\r\n{}---\r\nExact private needleword prose 🌲\r\n\r\nTrailing  ",
        fixture::ACTOR,
        fixture::SECTION,
        if placed {
            format!(
                "placement: {{timeline: main, tick: {}, order: 1}}\r\npov_character_id: {}\r\n",
                fixture::TICK,
                fixture::ACTOR
            )
        } else {
            String::new()
        }
    )
}
fn work() -> String {
    format!(
        "---\nschema: wedl-author/v1\nkind: work\nid: {WORK}\ntitle: Private needleword work\nduplicate_policy: allow-explicit-reuse\nchapters:\n  - id: {CHAPTER}\n    title: Literal order\n    segments:\n      - {{kind: author-record, id: {FIRST}}}\n      - {{kind: author-record, id: {SECOND}}}\n      - {{kind: author-record, id: {FIRST}}}\n---\nPrivate work commentary.\n"
    )
}
fn setup() -> (tempfile::TempDir, World) {
    let (directory, world) = fixture::fixture();
    for (kind, id, source) in [
        ("manuscript-segment", FIRST, segment(FIRST, true)),
        ("manuscript-segment", SECOND, segment(SECOND, false)),
        ("work", WORK, work()),
    ] {
        let path = world.root.join(m::document_path(kind, id).unwrap());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    (directory, world)
}
fn image(root: &Path) -> BTreeMap<String, (Vec<u8>, std::time::SystemTime)> {
    fn walk(
        root: &Path,
        dir: &Path,
        result: &mut BTreeMap<String, (Vec<u8>, std::time::SystemTime)>,
    ) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let metadata = fs::symlink_metadata(&path).unwrap();
            if metadata.is_dir() {
                walk(root, &path, result);
            } else if metadata.is_file() {
                result.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    (fs::read(&path).unwrap(), metadata.modified().unwrap()),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    walk(root, root, &mut result);
    result
}
fn list_input(revision: Option<&str>) -> Json {
    json!({"revision":revision,"kind":null,"maxItems":1024})
}
fn list(world: &World, revision: Option<&str>) -> Json {
    m::list(
        &world.root,
        &m::decode_list(&list_input(revision).to_string()).unwrap(),
    )
    .unwrap()
}
fn detail_input(revision: Option<&str>, id: &str, sha: Option<&str>) -> Json {
    json!({"revision":revision,"id":id,"expectedSha256":sha})
}
fn detail(world: &World, id: &str) -> Json {
    m::detail(
        &world.root,
        &m::decode_detail(&detail_input(None, id, None).to_string()).unwrap(),
    )
    .unwrap()
}
fn resolve_input(revision: Option<&str>, author_fp: &str) -> Json {
    json!({"revision":revision,"workId":WORK,"expectedAuthorFingerprint":author_fp,"maxOccurrences":4096,"maxCharacters":100000})
}
fn save_input(world: &World, source: String, sha: Option<&str>) -> Json {
    json!({"expectedFingerprint":world.fingerprint,"expectedAuthorFingerprint":list(world,None)["authorFingerprint"],"expectedSha256":sha,"source":source})
}
fn encode(raw: &str) -> String {
    raw.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn cli(world: &World, revision: Option<&str>, verb: &str, input: &Json) -> Output {
    let file = world
        .root
        .parent()
        .unwrap()
        .join("manuscript-transport-input.json");
    fs::write(&file, serde_json::to_vec(input).unwrap()).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_wedl-rs"));
    command.args(["--compact", "--repo"]).arg(&world.root);
    if let Some(revision) = revision {
        command.args(["--revision", revision]);
    }
    command
        .args(["manuscripts", verb, "--file"])
        .arg(file)
        .output()
        .unwrap()
}
fn success(output: Output) -> Json {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
async fn raw(service: Service, method: &str, uri: &str) -> (StatusCode, Json) {
    let response = http::router(service)
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
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap_or(Json::Null))
}
async fn get(service: Service, verb: &str, input: &Json) -> Json {
    let (status, value) = raw(
        service,
        "GET",
        &format!(
            "/api/manuscripts/{verb}?request={}",
            encode(&input.to_string())
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    value
}
fn git(root: &Path, args: &[&str]) -> String {
    let result = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().into()
}
fn commit(root: &Path) -> String {
    git(root, &["add", "--", "story", "author"]);
    git(root, &["commit", "-q", "-m", "original"]);
    git(root, &["rev-parse", "HEAD"])
}

#[tokio::test]
async fn list_detail_and_resolve_are_exact_across_transports_and_read_only() {
    let (_directory, world) = setup();
    let service = Service::new(world.clone());
    let before = image(&world.root);
    let input = list_input(None);
    let request = m::decode_list(&input.to_string()).unwrap();
    let native = m::list(&world.root, &request).unwrap();
    assert_eq!(native["items"].as_array().unwrap().len(), 3);
    let envelope = service
        .execute(Operation::ManuscriptList { request })
        .unwrap();
    assert_eq!(envelope["result"], native);
    assert!(envelope.get("fingerprint").is_none());
    assert!(envelope.get("revision").is_none());
    assert_eq!(success(cli(&world, None, "list", &input))["result"], native);
    assert_eq!(get(service.clone(), "list", &input).await["result"], native);
    let sha = hex::encode(Sha256::digest(segment(FIRST, true)));
    let input = detail_input(None, FIRST, Some(&sha));
    let request = m::decode_detail(&input.to_string()).unwrap();
    let native = m::detail(&world.root, &request).unwrap();
    assert_eq!(native["document"]["rawSource"], segment(FIRST, true));
    assert_eq!(
        native["document"]["frontmatter"]["placement"]["tick"],
        fixture::TICK.to_string()
    );
    assert_eq!(
        service
            .execute(Operation::ManuscriptDetail { request })
            .unwrap()["result"],
        native
    );
    assert_eq!(
        success(cli(&world, None, "detail", &input))["result"],
        native
    );
    assert_eq!(
        get(service.clone(), "detail", &input).await["result"],
        native
    );
    let input = resolve_input(None, native["authorFingerprint"].as_str().unwrap());
    let request = m::decode_resolve(&input.to_string()).unwrap();
    let native = m::resolve(&world.root, &request).unwrap();
    let occurrences = native["chapters"][0]["occurrences"].as_array().unwrap();
    assert_eq!(
        occurrences
            .iter()
            .map(|o| o["segmentId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [FIRST, SECOND, FIRST]
    );
    assert_eq!(
        occurrences
            .iter()
            .map(|o| o["segmentIndex"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert_eq!(native["segments"].as_array().unwrap().len(), 2);
    assert_eq!(native["readerAuthorized"], false);
    assert_eq!(
        service
            .execute(Operation::ManuscriptResolve { request })
            .unwrap()["result"],
        native
    );
    assert_eq!(
        success(cli(&world, None, "resolve", &input))["result"],
        native
    );
    assert_eq!(get(service, "resolve", &input).await["result"], native);
    assert_eq!(image(&world.root), before);
}

#[tokio::test]
async fn preview_and_native_save_preserve_exact_source_and_separate_fingerprints() {
    let (_directory, world) = setup();
    let before_world = source::load_world(&world.root, None).unwrap();
    let detail = detail(&world, FIRST);
    let old_sha = detail["document"]["sourceSha256"].as_str().unwrap();
    let updated = segment(FIRST, true) + "\r\nNew private ending.  ";
    let input = save_input(&world, updated.clone(), Some(old_sha));
    let request = m::decode_save(&input.to_string()).unwrap();
    let before = image(&world.root);
    let preview = m::save_preview(&world, &request).unwrap();
    assert_eq!(preview["afterSource"], updated);
    assert_eq!(preview["beforeSha256"], old_sha);
    assert_eq!(preview["validationComplete"], false);
    assert_eq!(preview["writesPerformed"], false);
    assert_eq!(
        Service::new(world.clone())
            .execute(Operation::ManuscriptSavePreview {
                request: request.clone()
            })
            .unwrap()["result"],
        preview
    );
    assert_eq!(
        success(cli(&world, None, "preview-save", &input))["result"],
        preview
    );
    assert_eq!(
        get(Service::new(world.clone()), "preview-save", &input).await["result"],
        preview
    );
    assert_eq!(image(&world.root), before);
    let saved = success(cli(&world, None, "save", &input))["result"].clone();
    assert_eq!(saved["writesPerformed"], true);
    assert_eq!(saved["afterSource"], updated);
    assert_eq!(
        saved["afterAuthorFingerprint"],
        list(&world, None)["authorFingerprint"]
    );
    assert_ne!(
        saved["afterAuthorFingerprint"],
        saved["beforeAuthorFingerprint"]
    );
    let after_world = source::load_world(&world.root, None).unwrap();
    assert_eq!(after_world.fingerprint, before_world.fingerprint);
    assert_eq!(after_world.records.len(), before_world.records.len());
    assert!(validation::validate_world(&after_world).valid);
    assert_eq!(
        fs::read_to_string(
            world
                .root
                .join(m::document_path("manuscript-segment", FIRST).unwrap())
        )
        .unwrap(),
        updated
    );
    assert!(m::save(&world, &request).is_err());
    let fresh = save_input(
        &world,
        updated,
        Some(saved["afterSha256"].as_str().unwrap()),
    );
    let before = image(&world.root);
    let same = success(cli(&world, None, "save", &fresh))["result"].clone();
    assert_eq!(same["changed"], false);
    assert_eq!(image(&world.root), before);
}

#[tokio::test]
async fn historical_reads_pair_original_private_and_canonical_sources_without_live_fallback() {
    let (_directory, world) = setup();
    git(&world.root, &["init", "-q"]);
    let base = commit(&world.root);
    let service = Service::new(world.clone());
    let input = list_input(Some(&base));
    let original = get(service.clone(), "list", &input).await["result"].clone();
    let resolve = resolve_input(Some(&base), original["authorFingerprint"].as_str().unwrap());
    let expected = m::resolve(
        &world.root,
        &m::decode_resolve(&resolve.to_string()).unwrap(),
    )
    .unwrap();
    assert_eq!(expected["revision"], base);
    assert!(expected["segments"][0]["blobOid"].as_str().is_some());
    fs::write(
        world
            .root
            .join(m::document_path("manuscript-segment", FIRST).unwrap()),
        "invalid live private source",
    )
    .unwrap();
    fs::write(
        world.root.join(&world.records[fixture::ACTOR].source_path),
        "invalid live canonical source",
    )
    .unwrap();
    let before = image(&world.root);
    assert_eq!(
        success(cli(&world, None, "list", &input))["result"],
        original
    );
    assert_eq!(
        get(service.clone(), "list", &input).await["result"],
        original
    );
    assert_eq!(
        success(cli(&world, None, "resolve", &resolve))["result"],
        expected
    );
    assert_eq!(
        get(service.clone(), "resolve", &resolve).await["result"],
        expected
    );
    let source = original["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == FIRST)
        .unwrap();
    let input = detail_input(Some(&base), FIRST, source["sourceSha256"].as_str());
    let result = get(service.clone(), "detail", &input).await["result"].clone();
    assert_eq!(result["document"]["rawSource"], segment(FIRST, true));
    assert_eq!(
        success(cli(&world, None, "detail", &input))["result"],
        result
    );
    assert!(
        !cli(&world, Some(&base), "list", &list_input(None))
            .status
            .success()
    );
    let (code, _) = raw(
        service,
        "GET",
        &format!(
            "/api/manuscripts/resolve?request={}",
            encode(
                &resolve_input(None, original["authorFingerprint"].as_str().unwrap()).to_string()
            )
        ),
    )
    .await;
    assert!(!code.is_success());
    assert_eq!(image(&world.root), before);
}

#[tokio::test]
async fn source_inspection_remains_available_when_current_canonical_references_break() {
    let (_directory, world) = setup();
    let service = Service::new(world.clone());
    fs::write(
        world.root.join(&world.records[fixture::ACTOR].source_path),
        "invalid current story",
    )
    .unwrap();
    let input = list_input(None);
    let native = list(&world, None);
    assert_eq!(get(service.clone(), "list", &input).await["result"], native);
    assert_eq!(success(cli(&world, None, "list", &input))["result"], native);
    let input = detail_input(None, FIRST, None);
    let native = detail(&world, FIRST);
    assert_eq!(
        get(service.clone(), "detail", &input).await["result"],
        native
    );
    assert_eq!(
        success(cli(&world, None, "detail", &input))["result"],
        native
    );
    let input = resolve_input(None, native["authorFingerprint"].as_str().unwrap());
    assert!(!cli(&world, None, "resolve", &input).status.success());
    let (code, _) = raw(
        service,
        "GET",
        &format!(
            "/api/manuscripts/resolve?request={}",
            encode(&input.to_string())
        ),
    )
    .await;
    assert!(!code.is_success());
}

#[tokio::test]
async fn closed_requests_and_bounds_refuse_without_http_write_routes() {
    let (_directory, world) = setup();
    let service = Service::new(world.clone());
    let before = image(&world.root);
    let invalid=[("list",r#"{"revision":null,"kind":null,"maxItems":1024,"maxItems":1}"#.to_string()),("list",r#"{"kind":null,"maxItems":1024}"#.to_string()),("list",r#"{"revision":null,"kind":null,"maxItems":1024,"extra":true}"#.to_string()),("list",r#"{"revision":"HEAD","kind":null,"maxItems":1024}"#.to_string()),("detail",json!({"revision":null,"id":FIRST}).to_string()),("resolve",json!({"revision":null,"workId":WORK,"maxOccurrences":4096,"maxCharacters":100000}).to_string()),("preview-save",json!({"expectedFingerprint":world.fingerprint,"expectedAuthorFingerprint":list(&world,None)["authorFingerprint"],"source":segment(FIRST,true)}).to_string())];
    for (verb, input) in invalid {
        let (code, _) = raw(
            service.clone(),
            "GET",
            &format!("/api/manuscripts/{verb}?request={}", encode(&input)),
        )
        .await;
        assert!(!code.is_success(), "{verb}: {input}");
    }
    for verb in [
        "list",
        "detail",
        "resolve",
        "preview-save",
        "save",
        "delete",
        "rename",
        "apply",
    ] {
        let (code, _) = raw(service.clone(), "POST", &format!("/api/manuscripts/{verb}")).await;
        assert!(!code.is_success());
    }
    for verb in ["save", "delete", "rename", "apply"] {
        let (code, _) = raw(service.clone(), "GET", &format!("/api/manuscripts/{verb}")).await;
        assert!(!code.is_success());
    }
    let mut input = list_input(None);
    input["maxItems"] = 1.into();
    let (code, _) = raw(
        service.clone(),
        "GET",
        &format!(
            "/api/manuscripts/list?request={}",
            encode(&input.to_string())
        ),
    )
    .await;
    assert!(!code.is_success());
    let fp = list(&world, None)["authorFingerprint"]
        .as_str()
        .unwrap()
        .to_owned();
    for (field, value) in [("maxOccurrences", 1), ("maxCharacters", 256)] {
        let mut input = resolve_input(None, &fp);
        input[field] = value.into();
        let (code, _) = raw(
            service.clone(),
            "GET",
            &format!(
                "/api/manuscripts/resolve?request={}",
                encode(&input.to_string())
            ),
        )
        .await;
        assert!(!code.is_success());
    }
    assert_eq!(image(&world.root), before);
}

#[tokio::test]
async fn private_source_never_enters_canonical_catalogues_search_or_character_views() {
    let (_directory, world) = setup();
    let service = Service::new(world.clone());
    for route in [
        "/api/entities".to_string(),
        "/api/search?q=needleword".into(),
        format!(
            "/api/character/search?character={}&q=needleword&tick={}&order=1",
            fixture::ACTOR,
            fixture::TICK
        ),
        format!(
            "/api/character/context?character={}&tick={}&order=1",
            fixture::ACTOR,
            fixture::TICK
        ),
    ] {
        let (status, result) = raw(service.clone(), "GET", &route).await;
        assert_eq!(status, StatusCode::OK, "{route}: {result}");
        let encoded = result.to_string();
        assert!(!encoded.contains("Private needleword"), "{route}: {result}");
        assert!(
            !encoded.contains("Exact private needleword"),
            "{route}: {result}"
        );
        assert!(!encoded.contains(FIRST));
        assert!(!encoded.contains(WORK));
    }
    let old = list(&world, None)["authorFingerprint"].clone();
    let id = fixture::id(3);
    let draft = fixture::draft(&world, &id);
    wedl::action_store::save(&world, &fixture::request(&world, draft, None)).unwrap();
    assert_eq!(list(&world, None)["authorFingerprint"], old);
    let reloaded = source::load_world(&world.root, None).unwrap();
    assert_eq!(reloaded.fingerprint, world.fingerprint);
}

#[tokio::test]
async fn oversized_url_has_complete_native_download_path_and_stale_private_pins_refuse() {
    let (_directory, world) = setup();
    let source = segment("segment_00000000000000000000000003", false)
        + &"Large private paragraph. ".repeat(2000);
    let input = save_input(&world, source, None);
    let service = Service::new(world.clone());
    let uri = format!(
        "/api/manuscripts/preview-save?request={}",
        encode(&input.to_string())
    );
    assert!(uri.len() > 16384);
    let (code, _) = raw(service, "GET", &uri).await;
    assert!(!code.is_success());
    let result = success(cli(&world, None, "preview-save", &input))["result"].clone();
    assert_eq!(result["afterSource"], input["source"]);
    assert_eq!(result["writesPerformed"], false);
    let old = list(&world, None)["authorFingerprint"]
        .as_str()
        .unwrap()
        .to_owned();
    let detail = detail(&world, SECOND);
    let change = save_input(
        &world,
        segment(SECOND, false) + "\nchanged",
        detail["document"]["sourceSha256"].as_str(),
    );
    success(cli(&world, None, "save", &change));
    assert!(!cli(&world, None, "preview-save", &input).status.success());
    assert!(
        !cli(&world, None, "resolve", &resolve_input(None, &old))
            .status
            .success()
    );
    let current = list(&world, None);
    let resolved = success(cli(
        &world,
        None,
        "resolve",
        &resolve_input(None, current["authorFingerprint"].as_str().unwrap()),
    ))["result"]
        .clone();
    assert!(
        resolved["segments"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == SECOND)
            .unwrap()["bodyMarkdown"]
            .as_str()
            .unwrap()
            .ends_with("changed")
    );
}

#[test]
fn save_cli_bounds_canonical_source_before_generic_loading_and_rejects_historical_mode() {
    let (_directory, world) = setup();
    let input = save_input(
        &world,
        segment("segment_00000000000000000000000003", false),
        None,
    );
    let path = world.root.join(&world.records[fixture::ACTOR].source_path);
    let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_len(8 * 1024 * 1024 + 1).unwrap();
    for verb in ["preview-save", "save"] {
        let output = cli(&world, None, verb, &input);
        assert!(!output.status.success());
        let error: Json = serde_json::from_slice(&output.stderr).unwrap();
        assert!(
            error["message"].as_str().unwrap().contains("bound"),
            "{error}"
        );
        let output = cli(&world, Some(&"a".repeat(40)), verb, &input);
        assert!(!output.status.success());
        let error: Json = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(
            error["message"],
            "manuscript saves require the current worktree"
        );
    }
}
