//! Operator policies and transport integration for the isolated reader and
//! author simulation. All successful rows are derived from admitted originals.
#[path = "support/actions.rs"]
mod base;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value as Json, json};
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
    reader, reader_config,
    service::{Operation, Service},
    source, validation,
};
const FIRST: &str = "segment_00000000000000000000000001";
const SECOND: &str = "segment_00000000000000000000000002";
const WORK: &str = "work_00000000000000000000000001";
const CHAPTER: &str = "chapter_00000000000000000000000001";
const PLAN: &str = "reveal_00000000000000000000000001";
const TEXT: &str = "revealtext_00000000000000000000000001";
const SECTION: &str = "description_00000000000000000000000011";
const HIDDEN: &str = "description_00000000000000000000000012";
const FUTURE: &str = "description_00000000000000000000000013";
fn raw(fields: Json, body: &str) -> String {
    format!(
        "---\n{}---\n{body}",
        serde_yaml::to_string(&fields).unwrap()
    )
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
fn setup() -> (tempfile::TempDir, World, String) {
    let (directory, world) = base::fixture();
    let owner = &world.records[base::PLACE];
    let mut fields = owner.frontmatter.clone();
    fields["title"] = "Ordinary owner title".into();
    fields["section_audiences"] = serde_yaml::from_str("Secrets: [author]\n").unwrap();
    fields["descriptions"]=serde_yaml::to_value(json!([
        {"id":SECTION,"heading":"Public bank","markdown":"Public river bank.","valid":{"kind":"timeless"},"audience":["public"],"perspectives":["ordinary"]},
        {"id":HIDDEN,"heading":"Hidden sibling","markdown":"Hidden sibling secret.","valid":{"kind":"timeless"},"audience":["author"],"perspectives":["ordinary"]},
        {"id":FUTURE,"heading":"Future section","markdown":"Future secret.","valid":{"kind":"interval","timeline":"main","start":{"timeline":"main","tick":30,"order":0}},"audience":["public"],"perspectives":["ordinary"]}
    ])).unwrap();
    fs::write(world.root.join(&owner.source_path),format!("---\n{}---\nPublic introduction.\n\n# Secrets\nHidden ancestor prose.\n## Ordinary child\nHidden child prose.\n\n# Open\nPublic open prose.\n",serde_yaml::to_string(&fields).unwrap())).unwrap();
    let world = source::load_world(&world.root, None).unwrap();
    let validation = validation::validate_world(&world);
    assert!(validation.valid, "{:?}", validation.diagnostics);
    for (id, tick) in [(FIRST, 20), (SECOND, -20)] {
        let fields = json!({"schema":"wedl-author/v1","kind":"manuscript-segment","id":id,"title":"Unrevealed segment title","source_links":[{"kind":"record","id":base::PLACE}],"placement":{"timeline":"main","tick":tick,"order":0}});
        put(
            &world,
            "manuscript-segment",
            id,
            &raw(fields, "Unrevealed manuscript prose.\r\n"),
        );
    }
    let work = json!({"schema":"wedl-author/v1","kind":"work","id":WORK,"title":"Private work title","duplicate_policy":"allow-explicit-reuse","chapters":[{"id":CHAPTER,"title":"Private chapter label","segments":[{"kind":"author-record","id":FIRST},{"kind":"author-record","id":SECOND},{"kind":"author-record","id":FIRST}]}]});
    put(&world, "work", WORK, &raw(work, "Private work notes."));
    let plan = json!({"schema":"wedl-author/v1","kind":"reveal-plan","id":PLAN,"title":"Private plan title","work":{"kind":"author-record","id":WORK},"entries":[
        {"milestone":{"chapter_id":CHAPTER,"segment_index":0},"admissions":[{"kind":"section","target":{"kind":"section","owner_id":base::PLACE,"id":SECTION}}]},
        {"milestone":{"chapter_id":CHAPTER,"segment_index":1},"admissions":[{"kind":"record","target":{"kind":"record","id":base::PLACE}}]},
        {"milestone":{"chapter_id":CHAPTER,"segment_index":2},"admissions":[{"kind":"text","id":TEXT,"markdown":"Exact Straße ﬃ Σ text.\r\nTrailing  "}]}]});
    put(
        &world,
        "reveal-plan",
        PLAN,
        &raw(plan, "Private spoiler notes.\r\n"),
    );
    git(&world.root, &["init", "-q"]);
    git(&world.root, &["add", "--", "story", "author"]);
    git(
        &world.root,
        &["commit", "-q", "-m", "Original reader source"],
    );
    let revision = git(&world.root, &["rev-parse", "HEAD"]);
    (directory, world, revision)
}
fn put(world: &World, kind: &str, id: &str, source: &str) {
    let path = world.root.join(m::document_path(kind, id).unwrap());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
}
fn scope(world: &World) -> Json {
    json!({"worldId":world.world_record().unwrap().id,"recordIds":[base::PLACE],"sections":null})
}
fn public(world: &World, revision: &str) -> Json {
    json!({"schema":reader_config::SCHEMA,"kind":"public","revision":revision,"workId":WORK,"planId":PLAN,"canonicalScope":scope(world),"generation":"original-public","maxOccurrence":2,"cursorSecret":"11".repeat(32),"publicationId":"river-story","at":{"timeline":"main","tick":"20","order":"0"},"textPolicy":"all-at-pinned-plan"})
}
fn simulation(world: &World, revision: &str) -> Json {
    let mut value = public(world, revision);
    for key in ["publicationId", "at", "textPolicy"] {
        value.as_object_mut().unwrap().remove(key);
    }
    value["kind"] = "author-simulation".into();
    value["generation"] = "original-preview".into();
    value["principalId"] = "local-author".into();
    value["authorPermissions"] = scope(world);
    value
}
fn request(revision: &str, occurrence: usize) -> Json {
    json!({"protocol":"wedl-reader-presentation/v1","publicationId":"river-story","revision":revision,"occurrence":occurrence,"limit":20,"cursor":null})
}
fn preview(revision: &str, occurrence: usize) -> Json {
    let mut value = request(revision, occurrence);
    value.as_object_mut().unwrap().remove("publicationId");
    value["workId"] = WORK.into();
    value["planId"] = PLAN.into();
    value["operation"] = "page".into();
    value["at"] = json!({"timeline":"main","tick":"20","order":"0"});
    value["textPolicy"] = "all-at-pinned-plan".into();
    value
}
fn image(root: &Path) -> BTreeMap<String, (Vec<u8>, std::time::SystemTime)> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, (Vec<u8>, std::time::SystemTime)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let meta = fs::symlink_metadata(&path).unwrap();
            if meta.is_dir() {
                walk(root, &path, out);
            } else if meta.is_file() {
                out.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    (fs::read(&path).unwrap(), meta.modified().unwrap()),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}
fn encoded(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
async fn send(router: axum::Router, method: &str, uri: &str, body: &str) -> (StatusCode, Json) {
    let response = router
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("host", "localhost:8766")
                .header("content-type", "application/json")
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap_or(Json::Null))
}
fn cli(world: &World, args: &[&str], request: &Json, policy: Option<&Json>) -> Output {
    let parent = world.root.parent().unwrap();
    let path = parent.join("reader-request.json");
    fs::write(&path, request.to_string()).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_wedl-rs"));
    command
        .args(["--compact", "--repo"])
        .arg(&world.root)
        .args(args)
        .arg("--file")
        .arg(&path);
    if let Some(policy) = policy {
        let path = parent.join("reader-policy.json");
        fs::write(&path, policy.to_string()).unwrap();
        command.arg("--policy").arg(path);
    }
    command.output().unwrap()
}
fn success(output: Output) -> Json {
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn no_private(value: &Json) {
    let text = value.to_string();
    for secret in [
        "Private work",
        "Private plan",
        "Private chapter",
        "Private spoiler",
        "Unrevealed",
        "Hidden sibling",
        "Hidden ancestor",
        "Hidden child",
        "Future secret",
        WORK,
        PLAN,
        FIRST,
        SECOND,
        "author/",
        "sourceSha256",
        "authorFingerprint",
        "blobOid",
    ] {
        assert!(!text.contains(secret), "leaked {secret}: {text}");
    }
}

#[tokio::test]
async fn public_reader_native_cli_http_match_with_section_only_first_reveal() {
    let (_directory, world, revision) = setup();
    let policy = public(&world, &revision);
    let service = reader::PresentationService::new(Some(world.root.clone()));
    service
        .install_public(Some(
            reader_config::decode_public(&world.root, &policy.to_string()).unwrap(),
        ))
        .unwrap();
    let before = image(&world.root);
    for occurrence in 0..3 {
        let input = request(&revision, occurrence);
        let native = service
            .execute(input.to_string().as_bytes(), reader::Operation::Page)
            .unwrap();
        assert_eq!(native["outcome"], "available");
        no_private(&native);
        if occurrence == 0 {
            assert_eq!(native["count"], 1);
            assert_eq!(native["items"][0]["kind"], "section");
            assert!(!native.to_string().contains("Ordinary owner title"));
            assert!(!native.to_string().contains("Public introduction"));
        }
        assert_eq!(
            success(cli(&world, &["reader", "page"], &input, Some(&policy))),
            native
        );
        let (status, value) = send(
            reader::router(service.clone()),
            "POST",
            "/api/reader-presentation/page",
            &input.to_string(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{value}");
        assert_eq!(value, native);
    }
    let mut input = request(&revision, 2);
    input["query"] = "STRASSE FFI σ".into();
    let native = service
        .execute(input.to_string().as_bytes(), reader::Operation::Search)
        .unwrap();
    assert_eq!(native["count"], 1);
    assert_eq!(
        native["items"][0]["markdown"],
        "Exact Straße ﬃ Σ text.\r\nTrailing  "
    );
    assert_eq!(
        success(cli(&world, &["reader", "search"], &input, Some(&policy))),
        native
    );
    let (status, value) = send(
        reader::router(service),
        "POST",
        "/api/reader-presentation/search",
        &input.to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value, native);
    assert_eq!(image(&world.root), before);
}

#[tokio::test]
async fn author_simulation_intersects_installed_permissions_and_has_no_unrelated_envelope() {
    let (_directory, world, revision) = setup();
    let policy = simulation(&world, &revision);
    let service = Service::new(world.clone());
    service
        .reader()
        .install_simulation(Some(
            reader_config::decode_simulation(&world.root, &policy.to_string()).unwrap(),
        ))
        .unwrap();
    let input = preview(&revision, 2);
    let native = service
        .reader()
        .preview(input.to_string().as_bytes())
        .unwrap();
    no_private(&native);
    let envelope = service
        .execute(Operation::ReaderPreview {
            request: input.to_string(),
        })
        .unwrap();
    assert_eq!(envelope["result"], native);
    assert!(envelope.get("fingerprint").is_none());
    assert!(envelope.get("revision").is_none());
    assert_eq!(
        success(cli(&world, &["reader", "preview"], &input, Some(&policy))),
        native
    );
    let (status, value) = send(
        http::router(service.clone()),
        "GET",
        &format!(
            "/api/reader-presentation/preview?request={}",
            encoded(&input.to_string())
        ),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["result"], native);
    let mut limited = policy;
    limited["authorPermissions"]["recordIds"] = json!([]);
    service
        .reader()
        .install_simulation(Some(
            reader_config::decode_simulation(&world.root, &limited.to_string()).unwrap(),
        ))
        .unwrap();
    let mut input = input;
    input["textPolicy"] = "none".into();
    let answer = service
        .reader()
        .preview(input.to_string().as_bytes())
        .unwrap();
    assert_eq!(answer["items"], json!([]));
    assert_eq!(answer["count"], 0);
    service.reader().install_simulation(None).unwrap();
    let (status, value) = send(
        http::router(service),
        "GET",
        &format!(
            "/api/reader-presentation/preview?request={}",
            encoded(&input.to_string())
        ),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(value["result"]["outcome"], "unavailable");
}

#[tokio::test]
async fn reveal_resolution_and_exact_plan_store_share_author_transports_only() {
    let (_directory, world, revision) = setup();
    let input = json!({"revision":revision,"planId":PLAN,"expectedAuthorFingerprint":null,"maxOccurrences":4096,"maxCharacters":100000});
    let request = m::decode_reveal_resolve(&input.to_string()).unwrap();
    let native = m::resolve_reveal(&world.root, &request).unwrap();
    assert_eq!(native["plan"]["bodyMarkdown"], "Private spoiler notes.\r\n");
    assert_eq!(native["entries"].as_array().unwrap().len(), 3);
    assert_eq!(native["readerAuthorized"], false);
    let before = image(&world.root);
    assert_eq!(
        Service::new(world.clone())
            .execute(Operation::RevealResolve { request })
            .unwrap()["result"],
        native
    );
    assert_eq!(
        success(cli(&world, &["reveal-plans", "resolve"], &input, None))["result"],
        native
    );
    let (status, value) = send(
        http::router(Service::new(world.clone())),
        "GET",
        &format!(
            "/api/reveal-plans/resolve?request={}",
            encoded(&input.to_string())
        ),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["result"], native);
    let service = reader::PresentationService::new(Some(world.root.clone()));
    for path in [
        "/api/reveal-plans/resolve",
        "/api/manuscripts/detail",
        "/api/reader-presentation/preview",
        "/api/status",
        "/api/entities",
        "/api/session",
        "/api/search",
    ] {
        let (status, _) = send(reader::router(service.clone()), "GET", path, "").await;
        assert!(!status.is_success(), "{path}");
    }
    assert_eq!(image(&world.root), before);
}

#[tokio::test]
async fn reader_originals_ignore_broken_live_source_and_policy_ceiling_precedes_lookup() {
    let (_directory, world, revision) = setup();
    let policy = public(&world, &revision);
    let service = reader::PresentationService::new(Some(world.root.clone()));
    service
        .install_public(Some(
            reader_config::decode_public(&world.root, &policy.to_string()).unwrap(),
        ))
        .unwrap();
    let input = request(&revision, 2);
    let expected = service
        .execute(input.to_string().as_bytes(), reader::Operation::Page)
        .unwrap();
    fs::write(
        world.root.join(&world.records[base::PLACE].source_path),
        "invalid live canonical",
    )
    .unwrap();
    fs::write(
        world
            .root
            .join(m::document_path("reveal-plan", PLAN).unwrap()),
        "invalid live reveal",
    )
    .unwrap();
    assert_eq!(
        service
            .execute(input.to_string().as_bytes(), reader::Operation::Page)
            .unwrap(),
        expected
    );
    assert_eq!(
        success(cli(&world, &["reader", "page"], &input, Some(&policy))),
        expected
    );
    let before = image(&world.root);
    service.install_public(None).unwrap();
    fs::rename(world.root.join(".git"), world.root.join("hidden-git")).unwrap();
    let unavailable = service
        .execute(input.to_string().as_bytes(), reader::Operation::Page)
        .unwrap();
    assert_eq!(unavailable["outcome"], "unavailable");
    assert!(!unavailable.to_string().contains("git"));
    fs::rename(world.root.join("hidden-git"), world.root.join(".git")).unwrap();
    assert_eq!(image(&world.root), before);
}

#[tokio::test]
async fn cursors_are_bound_to_operation_exact_query_page_size_and_installed_policy() {
    let (_directory, world, revision) = setup();
    let policy = public(&world, &revision);
    let service = reader::PresentationService::new(Some(world.root.clone()));
    service
        .install_public(Some(
            reader_config::decode_public(&world.root, &policy.to_string()).unwrap(),
        ))
        .unwrap();
    let mut input = request(&revision, 2);
    input["limit"] = 1.into();
    let first = service
        .execute(input.to_string().as_bytes(), reader::Operation::Page)
        .unwrap();
    assert!(first["nextCursor"].as_str().is_some());
    let cursor = first["nextCursor"].clone();
    input["cursor"] = cursor.clone();
    let second = service
        .execute(input.to_string().as_bytes(), reader::Operation::Page)
        .unwrap();
    assert_eq!(second["outcome"], "available");
    assert_ne!(first["items"], second["items"]);
    for (field, value) in [
        ("limit", json!(2)),
        ("occurrence", json!(1)),
        ("publicationId", json!("other")),
    ] {
        let mut changed = input.clone();
        changed[field] = value;
        assert_eq!(
            service
                .execute(changed.to_string().as_bytes(), reader::Operation::Page)
                .unwrap()["outcome"],
            "unavailable"
        );
    }
    let mut changed = input.clone();
    changed["query"] = "public".into();
    assert_eq!(
        service
            .execute(changed.to_string().as_bytes(), reader::Operation::Search)
            .unwrap()["outcome"],
        "unavailable"
    );
    let mut new_policy = policy;
    new_policy["generation"] = "replaced".into();
    service
        .install_public(Some(
            reader_config::decode_public(&world.root, &new_policy.to_string()).unwrap(),
        ))
        .unwrap();
    assert_eq!(
        service
            .execute(input.to_string().as_bytes(), reader::Operation::Page)
            .unwrap()["outcome"],
        "unavailable"
    );
    let (status, result) = send(
        reader::router(service),
        "POST",
        "/api/reader-presentation/page",
        &input.to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(result["outcome"], "unavailable");
}

#[test]
fn operator_policy_codec_rejects_duplicate_coercions_unknown_fields_and_oversized_input() {
    let (_directory, world, revision) = setup();
    let good = public(&world, &revision);
    assert!(reader_config::decode_public(&world.root, &good.to_string()).is_ok());
    for (field, value) in [
        ("schema", json!("wedl-reader-policy/v2")),
        ("kind", json!("author-simulation")),
        ("cursorSecret", json!(["11".repeat(32)])),
        ("cursorSecret", json!("A".repeat(64))),
        ("maxOccurrence", json!(true)),
        ("at", json!({"timeline":"main","tick":20,"order":"0"})),
        ("at", json!({"timeline":"main","tick":"-0","order":"0"})),
        ("extra", json!(true)),
    ] {
        let mut bad = good.clone();
        bad[field] = value;
        assert!(
            reader_config::decode_public(&world.root, &bad.to_string()).is_err(),
            "{field}"
        );
    }
    let mut bad = good.clone();
    bad["canonicalScope"]["recordIds"] = json!([base::PLACE, base::PLACE]);
    assert!(reader_config::decode_public(&world.root, &bad.to_string()).is_err());
    bad = good.clone();
    bad["canonicalScope"]
        .as_object_mut()
        .unwrap()
        .remove("sections");
    assert!(reader_config::decode_public(&world.root, &bad.to_string()).is_err());
    let duplicate =
        good.to_string()
            .replacen("\"sections\":null", "\"sections\":null,\"sections\":{}", 1);
    assert!(reader_config::decode_public(&world.root, &duplicate).is_err());
    let duplicate = good.to_string().replacen(
        "\"cursorSecret\":",
        "\"cursorSecret\":\"discarded\",\"cursorSecret\":",
        1,
    );
    assert!(reader_config::decode_public(&world.root, &duplicate).is_err());
    assert!(reader_config::decode_public(&world.root, &" ".repeat(1024 * 1024 + 1)).is_err());
}

#[tokio::test]
async fn default_closed_reader_requests_are_unavailable_without_repository_or_grant_routes() {
    let missing = tempfile::tempdir().unwrap().path().join("missing");
    let service = reader::PresentationService::new(Some(missing.clone()));
    let input = request(&"a".repeat(40), 0);
    let result = service
        .execute(input.to_string().as_bytes(), reader::Operation::Page)
        .unwrap();
    assert_eq!(result["outcome"], "unavailable");
    assert!(!missing.exists());
    for patch in [
        json!({"readerId":"invented"}),
        json!({"grant":"all"}),
        json!({"revision":"HEAD"}),
        json!({"occurrence":true}),
        json!({"cursor":"bad"}),
    ] {
        let mut candidate = input.clone();
        candidate
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        let (status, value) = send(
            reader::router(service.clone()),
            "POST",
            "/api/reader-presentation/page",
            &candidate.to_string(),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{value}");
    }
    let (status, _) = send(
        reader::router(service.clone()),
        "GET",
        "/api/reader-presentation/page",
        "",
    )
    .await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    let (status, _) = send(
        reader::router(service),
        "POST",
        "/api/reader-presentation/page?grant=all",
        &input.to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
