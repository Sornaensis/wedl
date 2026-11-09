//! Complete retcon workflows preserve native results and exact originals through
//! CLI, shared service and bounded GET-only author transport.
#[path = "support/actions.rs"]
mod fixture;

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
    action_drafts, event_consequences as events, http,
    model::World,
    retcon_impact as impact, retcon_reviews as reviews,
    service::{Operation, Service},
    source, validation,
};

const REVIEW: &str = "review_00000000000000000000000001";
const EVENT: &str = "event_00000000000000000000000001";

fn image(root: &Path) -> BTreeMap<String, (Vec<u8>, std::time::SystemTime)> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, (Vec<u8>, std::time::SystemTime)>) {
        for item in fs::read_dir(dir).unwrap() {
            let path = item.unwrap().path();
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
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
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
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn commit(root: &Path, label: &str) -> String {
    git(root, &["add", "--", "story"]);
    if root.join("author").exists() {
        git(root, &["add", "--", "author"]);
    }
    git(root, &["commit", "-q", "-m", label]);
    git(root, &["rev-parse", "HEAD"])
}
fn write_fields(world: &World, id: &str, update: impl FnOnce(&mut serde_yaml::Value)) {
    let record = &world.records[id];
    let mut fields = record.frontmatter.clone();
    update(&mut fields);
    fs::write(
        world.root.join(&record.source_path),
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&fields).unwrap(),
            record.body
        ),
    )
    .unwrap();
}
fn world() -> (tempfile::TempDir, World) {
    let (dir, world) = fixture::fixture();
    write_fields(&world, &world.world_record().unwrap().id, |fields| {
        fields["state_keys"] =
            serde_yaml::from_str("character: {count: {type: integer}}\n").unwrap()
    });
    write_fields(&world, fixture::ACTOR, |fields| {
        fields["initial_state"] = serde_yaml::from_str("count: 9007199254740993\n").unwrap()
    });
    fs::write(world.root.join(format!("story/{EVENT}.md")),format!("---\nschema: wedl/v1\nkind: event\nid: {EVENT}\ntitle: Exact event\ntime: {{timeline: main, tick: {}, order: 0}}\neffects:\n - {{id: effect_00000000000000000000000001, target: {}, key: count, operation: set, value: 9007199254740995}}\n---\nExplicit event body.\n",fixture::TICK,fixture::ACTOR)).unwrap();
    let world = source::load_world(&world.root, None).unwrap();
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    (dir, world)
}
fn plan(world: &World) -> Json {
    json!({"expectedFingerprint":world.fingerprint,"operations":[{"type":"record.update","entityId":fixture::PLACE,"patch":"kind: location\ntitle: Revised landing\n"}]})
}
fn impact_input(world: &World) -> Json {
    json!({"plan":plan(world),"impactRequest":{"target":{"kind":"record","id":fixture::PLACE},"at":fixture::horizon(),"depth":32,"maxItems":1000}})
}
fn capture_input(world: &World, base: &str) -> Json {
    let mut request = impact_input(world);
    request["baseRevision"] = base.into();
    request
}
fn save_input(world: &World, analysis: Json) -> Json {
    let decisions = analysis["itemDigests"].as_array().unwrap().iter().take(1).map(|digest| json!({"itemDigest":digest,"disposition":"intentionally-retained","note":"Exact private note.\r\n🌲  "})).collect::<Vec<_>>();
    json!({"expectedFingerprint":world.fingerprint,"expectedSha256":null,"document":{"id":REVIEW,"title":"Original review","analysis":analysis,"decisions":decisions,"bodyMarkdown":"\r\nExact private commentary 🌲\r\nTrailing  "}})
}
fn encoded(value: &Json) -> String {
    value
        .to_string()
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}
fn cli(world: &World, revision: Option<&str>, args: &[&str], input: &Json) -> Output {
    let path = world.root.parent().unwrap().join("retcon-transport.json");
    fs::write(&path, serde_json::to_vec(input).unwrap()).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_wedl-rs"));
    cmd.args(["--compact", "--repo"]).arg(&world.root);
    if let Some(revision) = revision {
        cmd.args(["--revision", revision]);
    }
    cmd.args(args).arg("--file").arg(path).output().unwrap()
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
async fn get(service: Service, route: &str, input: &Json) -> Json {
    let (status, result) = raw(
        service,
        "GET",
        &format!("/api/{route}?request={}", encoded(input)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    result["result"].clone()
}

#[tokio::test]
async fn impact_event_and_grouped_results_match_every_read_transport_without_writes() {
    let (_directory, world) = world();
    let before = image(&world.root);
    let service = Service::new(world.clone());
    let input = impact_input(&world);
    let request = impact::decode_preview(&input.to_string()).unwrap();
    let native = impact::preview(&world, &request).unwrap();
    assert!(!native["items"].as_array().unwrap().is_empty());
    assert_eq!(
        service
            .execute(Operation::RetconImpactPreview { request })
            .unwrap()["result"],
        native
    );
    assert_eq!(
        success(cli(&world, None, &["retcon-impact", "preview"], &input))["result"],
        native
    );
    assert_eq!(
        get(service.clone(), "retcon-impact/preview", &input).await,
        native
    );
    let input = json!({"expectedFingerprint":world.fingerprint,"eventId":EVENT,"at":fixture::horizon(),"maxItems":1000,"maxCharacters":100000});
    let request = events::decode_event(&input.to_string()).unwrap();
    let native = events::read(&world, &request).unwrap();
    assert_eq!(native["comparison"]["eventRetained"], true);
    assert!(native.to_string().contains("9007199254740995"));
    assert_eq!(
        service
            .execute(Operation::EventConsequences { request })
            .unwrap()["result"],
        native
    );
    assert_eq!(
        success(cli(&world, None, &["event-consequences"], &input))["result"],
        native
    );
    assert_eq!(
        get(service.clone(), "event-consequences", &input).await,
        native
    );
    let input = json!({"plan":plan(&world),"at":fixture::horizon(),"focusEventIds":[EVENT],"maxItems":1000,"maxCharacters":100000});
    let request = events::decode_grouped(&input.to_string()).unwrap();
    let native = events::preview(&world, &request).unwrap();
    assert_eq!(
        native["unattributedRecordChanges"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        service
            .execute(Operation::GroupedConsequences { request })
            .unwrap()["result"],
        native
    );
    assert_eq!(
        success(cli(&world, None, &["grouped-consequences"], &input))["result"],
        native
    );
    assert_eq!(get(service, "grouped-consequences", &input).await, native);
    assert_eq!(image(&world.root), before);
}

#[tokio::test]
async fn private_capture_save_preview_and_exact_reads_match_transports_and_preserve_original_analysis()
 {
    let (_directory, world) = world();
    git(&world.root, &["init", "-q"]);
    let base = commit(&world.root, "original");
    let input = capture_input(&world, &base);
    let request = reviews::decode_preview(&input.to_string()).unwrap();
    let service = Service::new(world.clone());
    let before = image(&world.root);
    let native = reviews::preview(&world.root, &request).unwrap();
    let envelope = service
        .execute(Operation::RetconReviewPreview { request })
        .unwrap();
    assert_eq!(envelope["result"], native);
    assert!(envelope.get("fingerprint").is_none());
    assert_eq!(
        success(cli(&world, None, &["retcon-reviews", "preview"], &input))["result"],
        native
    );
    assert_eq!(
        get(service.clone(), "retcon-reviews/preview", &input).await,
        native
    );
    assert_eq!(image(&world.root), before);
    let mut input = save_input(&world, native["analysis"].clone());
    let request = reviews::decode_save(&input.to_string()).unwrap();
    let preview = reviews::save_preview(&world, &request).unwrap();
    assert_eq!(
        service
            .execute(Operation::RetconReviewSavePreview {
                request: request.clone()
            })
            .unwrap()["result"],
        preview
    );
    assert_eq!(
        success(cli(
            &world,
            None,
            &["retcon-reviews", "preview-save"],
            &input
        ))["result"],
        preview
    );
    assert_eq!(
        get(service.clone(), "retcon-reviews/preview-save", &input).await,
        preview
    );
    assert_eq!(image(&world.root), before);
    let canonical = fixture::source_image(&world);
    let saved = success(cli(&world, None, &["retcon-reviews", "save"], &input))["result"].clone();
    assert_eq!(saved["writesPerformed"], true);
    assert_eq!(fixture::source_image(&world), canonical);
    let queue_input = json!({"revision":null,"maxItems":1000});
    let queue_request = reviews::decode_queue(&queue_input.to_string()).unwrap();
    let before = image(&world.root);
    let queue = reviews::queue(&world.root, &queue_request).unwrap();
    assert_eq!(queue["items"][0]["currentness"]["status"], "current");
    assert_eq!(
        service
            .execute(Operation::RetconReviewQueue {
                request: queue_request
            })
            .unwrap()["result"],
        queue
    );
    assert_eq!(
        success(cli(
            &world,
            None,
            &["retcon-reviews", "queue"],
            &queue_input
        ))["result"],
        queue
    );
    assert_eq!(
        get(service.clone(), "retcon-reviews/queue", &queue_input).await,
        queue
    );
    let detail_input = json!({"revision":null,"id":REVIEW,"expectedSha256":saved["sourceSha256"]});
    let request = reviews::decode_detail(&detail_input.to_string()).unwrap();
    let detail = reviews::detail(&world.root, &request).unwrap();
    assert_eq!(detail["document"], input["document"]);
    assert_eq!(
        detail["sourceSha256"],
        action_drafts::sha256(detail["source"].as_str().unwrap().as_bytes())
    );
    assert_eq!(
        service
            .execute(Operation::RetconReviewDetail { request })
            .unwrap()["result"],
        detail
    );
    assert_eq!(
        success(cli(
            &world,
            None,
            &["retcon-reviews", "detail"],
            &detail_input
        ))["result"],
        detail
    );
    assert_eq!(
        get(service, "retcon-reviews/detail", &detail_input).await,
        detail
    );
    assert_eq!(image(&world.root), before);
    input["expectedSha256"] = saved["sourceSha256"].clone();
    let same = success(cli(&world, None, &["retcon-reviews", "save"], &input));
    assert_eq!(same["result"]["writesPerformed"], false);
    assert_eq!(image(&world.root), before);
    input["document"]["analysis"]["baseRevision"] = "1".repeat(40).into();
    assert!(
        !cli(&world, None, &["retcon-reviews", "save"], &input)
            .status
            .success()
    );
    assert_eq!(image(&world.root), before);
}

#[tokio::test]
async fn historical_private_reads_are_independent_of_invalid_live_and_unrelated_service_captures() {
    let (_directory, world) = world();
    git(&world.root, &["init", "-q"]);
    let base = commit(&world.root, "original");
    let preview = reviews::preview(
        &world.root,
        &reviews::decode_preview(&capture_input(&world, &base).to_string()).unwrap(),
    )
    .unwrap();
    let input = save_input(&world, preview["analysis"].clone());
    let saved = reviews::save(&world, &reviews::decode_save(&input.to_string()).unwrap()).unwrap();
    let revision = commit(&world.root, "private review");
    fs::write(
        world.root.join(&world.records[fixture::ACTOR].source_path),
        b"invalid live source",
    )
    .unwrap();
    fs::write(
        world
            .root
            .join(format!("author/retcon-reviews/{REVIEW}.json")),
        b"invalid live private source",
    )
    .unwrap();
    let mut unrelated = world.clone();
    unrelated
        .records
        .get_mut(fixture::ACTOR)
        .unwrap()
        .frontmatter["kind"] = "unknown".into();
    let service = Service::new(unrelated);
    let detail_input =
        json!({"revision":revision,"id":REVIEW,"expectedSha256":saved["sourceSha256"]});
    let request = reviews::decode_detail(&detail_input.to_string()).unwrap();
    let before = image(&world.root);
    let native = reviews::detail(&world.root, &request).unwrap();
    assert_eq!(native["document"], input["document"]);
    assert_eq!(native["currentness"]["status"], "unknown");
    assert!(native["blobOid"].is_string());
    let result = service
        .execute(Operation::RetconReviewDetail { request })
        .unwrap();
    assert_eq!(result["result"], native);
    assert!(result.get("fingerprint").is_none());
    assert_eq!(
        success(cli(
            &world,
            None,
            &["retcon-reviews", "detail"],
            &detail_input
        ))["result"],
        native
    );
    assert_eq!(
        get(service.clone(), "retcon-reviews/detail", &detail_input).await,
        native
    );
    let queue_input = json!({"revision":revision,"maxItems":1000});
    let queue = reviews::queue(
        &world.root,
        &reviews::decode_queue(&queue_input.to_string()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        success(cli(
            &world,
            None,
            &["retcon-reviews", "queue"],
            &queue_input
        ))["result"],
        queue
    );
    assert_eq!(
        get(service, "retcon-reviews/queue", &queue_input).await,
        queue
    );
    assert_eq!(image(&world.root), before);
    assert!(
        !cli(
            &world,
            Some(&base),
            &["retcon-reviews", "detail"],
            &detail_input
        )
        .status
        .success()
    );
}

#[tokio::test]
async fn retcon_http_has_closed_queries_complete_bounds_and_no_mutation_routes() {
    let (_directory, world) = world();
    let service = Service::new(world.clone());
    let before = image(&world.root);
    let input = impact_input(&world);
    let uri = format!("/api/retcon-impact/preview?request={}", encoded(&input));
    for query in [
        format!("{uri}&unexpected=1"),
        format!("{uri}&request={}", encoded(&input)),
        format!("/api/event-consequences?request={}", "x".repeat(16385)),
    ] {
        assert_eq!(
            raw(service.clone(), "GET", &query).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    for route in [
        "retcon-reviews/save",
        "retcon-reviews/apply",
        "retcon-impact/apply",
        "event-consequences/apply",
        "grouped-consequences/apply",
    ] {
        assert_ne!(
            raw(
                service.clone(),
                "GET",
                &format!("/api/{route}?request={}", encoded(&input))
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    for method in ["POST", "PUT", "PATCH", "DELETE"] {
        assert_eq!(
            raw(service.clone(), method, &uri).await.0,
            StatusCode::METHOD_NOT_ALLOWED
        );
    }
    let mut too_small = input;
    too_small["impactRequest"]["maxItems"] = 1.into();
    assert_ne!(
        raw(
            service,
            "GET",
            &format!("/api/retcon-impact/preview?request={}", encoded(&too_small))
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(image(&world.root), before);
}
