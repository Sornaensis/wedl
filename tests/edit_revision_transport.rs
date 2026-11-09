//! Author workflows keep exact bytes, explicit approval and original Git captures
//! across native calls, CLI and the read-only local adapter.
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
    action_drafts, action_store, edit_drafts as edits, http,
    model::World,
    revision_review,
    service::{Operation, Service},
    source,
};

const EDIT: &str = "edit_00000000000000000000000001";

fn image(root: &Path) -> BTreeMap<String, (Vec<u8>, std::time::SystemTime)> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, (Vec<u8>, std::time::SystemTime)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let meta = fs::symlink_metadata(&path).unwrap();
            if meta.is_dir() {
                walk(root, &path, out)
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
fn draft(world: &World, patch: &str) -> edits::Draft {
    let target = &world.records[fixture::ACTOR];
    edits::decode(&json!({"id":EDIT,"title":"Retained integration edit","worldId":world.world_record().unwrap().id,
        "targetId":target.id,"baseFingerprint":world.fingerprint,"baseSourceSha256":action_drafts::sha256(&target.raw_bytes),
        "baseSourcePath":target.source_path,"patch":patch,"bodyMode":"preserve",
        "bodyMarkdown":"\r\nRetained 🌲 prose.\r\nTrailing  ","phase":"draft"}).to_string()).unwrap()
}
fn save_request(world: &World, draft: edits::Draft, sha: Option<String>) -> edits::SaveRequest {
    edits::SaveRequest {
        expected_fingerprint: world.fingerprint.clone(),
        expected_sha256: sha,
        draft,
    }
}
fn review_request(world: &World, sha: &str) -> edits::ReviewRequest {
    edits::ReviewRequest {
        expected_fingerprint: world.fingerprint.clone(),
        edit_id: EDIT.into(),
        expected_sha256: sha.into(),
        max_characters: 100_000,
    }
}
fn cli(world: &World, revision: Option<&str>, args: &[&str], input: Option<&Json>) -> Output {
    let path = world.root.parent().unwrap().join("transport-request.json");
    if let Some(input) = input {
        fs::write(&path, serde_json::to_vec(input).unwrap()).unwrap();
    }
    let mut command = Command::new(env!("CARGO_BIN_EXE_wedl-rs"));
    command.args(["--compact", "--repo"]).arg(&world.root);
    if let Some(revision) = revision {
        command.args(["--revision", revision]);
    }
    command.args(args);
    if input.is_some() {
        command.arg("--file").arg(path);
    }
    command.output().unwrap()
}
fn success(output: Output) -> Json {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn encoded(value: &Json) -> String {
    value
        .to_string()
        .bytes()
        .map(|byte| format!("%{byte:02X}"))
        .collect()
}
async fn request(service: Service, method: &str, uri: &str) -> (StatusCode, Json) {
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
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Json::Null))
}
fn review_uri(input: &edits::ReviewRequest) -> String {
    format!(
        "/api/edit-drafts/review?expectedFingerprint={}&id={}&sha256={}&maxCharacters={}",
        input.expected_fingerprint, input.edit_id, input.expected_sha256, input.max_characters
    )
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
    git(root, &["add", "--", "story", "author"]);
    git(root, &["commit", "-q", "-m", label]);
    git(root, &["rev-parse", "HEAD"])
}
fn title(world: &World, text: &str) {
    let record = &world.records[fixture::ACTOR];
    let mut fields = record.frontmatter.clone();
    fields["title"] = text.into();
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

#[tokio::test]
async fn unfinished_edit_preview_and_private_save_have_native_cli_http_parity_without_canonical_writes()
 {
    let (_directory, world) = fixture::fixture();
    let input = save_request(
        &world,
        draft(&world, "kind: character\nunknown: [unfinished\r\n"),
        None,
    );
    let before = image(&world.root);
    let canonical = fixture::source_image(&world);
    let native = edits::preview(&world, &input).unwrap();
    assert_eq!(native["canSave"], true);
    assert_eq!(native["writesPerformed"], false);
    let service = Service::new(world.clone());
    let envelope = service
        .execute(Operation::EditDraftPreview {
            request: input.clone(),
        })
        .unwrap();
    assert_eq!(envelope["result"], native);
    assert_eq!(
        request(
            service.clone(),
            "GET",
            &format!(
                "/api/edit-drafts/preview?request={}",
                encoded(&json!(input))
            )
        )
        .await,
        (StatusCode::OK, envelope.clone())
    );
    assert_eq!(
        success(cli(
            &world,
            None,
            &["edit-drafts", "preview"],
            Some(&json!(input))
        )),
        envelope
    );
    assert_eq!(image(&world.root), before);
    let saved = success(cli(
        &world,
        None,
        &["edit-drafts", "save"],
        Some(&json!(input)),
    ));
    assert_eq!(fixture::source_image(&world), canonical);
    let sha = saved["result"]["sourceSha256"].as_str().unwrap();
    let detail = service
        .execute(Operation::EditDraftDetail {
            id: EDIT.into(),
            expected_sha256: Some(sha.into()),
        })
        .unwrap();
    assert_eq!(detail["result"]["draft"]["patch"], input.draft.patch);
    assert_eq!(
        detail["result"]["draft"]["bodyMarkdown"],
        input.draft.body_markdown
    );
    let before = image(&world.root);
    assert_eq!(
        request(
            service.clone(),
            "GET",
            &format!("/api/edit-drafts/detail?id={EDIT}&sha256={sha}")
        )
        .await,
        (StatusCode::OK, detail.clone())
    );
    assert_eq!(
        success(cli(
            &world,
            None,
            &["edit-drafts", "detail", EDIT, "--sha256", sha],
            None
        )),
        detail
    );
    assert_eq!(
        request(service.clone(), "GET", "/api/edit-drafts/list")
            .await
            .1,
        service.execute(Operation::EditDraftList).unwrap()
    );
    assert_eq!(image(&world.root), before);
}

#[tokio::test]
async fn invalid_retained_patch_is_reviewable_but_cannot_be_approved_into_canonical_source() {
    let (_directory, world) = fixture::fixture();
    let saved = edits::save(
        &world,
        &save_request(&world, draft(&world, "kind: character\nbroken: [\n"), None),
    )
    .unwrap();
    let input = review_request(&world, saved["sourceSha256"].as_str().unwrap());
    let service = Service::new(world.clone());
    let before = image(&world.root);
    let native = edits::review(&world, &input).unwrap();
    assert_eq!(native["canApply"], false);
    assert_eq!(native["validation"]["valid"], false);
    assert!(native["consequences"].is_null());
    let envelope = service
        .execute(Operation::EditDraftReview {
            request: input.clone(),
        })
        .unwrap();
    assert_eq!(envelope["result"], native);
    assert_eq!(
        request(service, "GET", &review_uri(&input)).await,
        (StatusCode::OK, envelope.clone())
    );
    assert_eq!(
        success(cli(
            &world,
            None,
            &["edit-drafts", "review"],
            Some(&json!(input))
        )),
        envelope
    );
    let mut apply = json!(input);
    apply["reviewSha256"] = native["reviewSha256"].clone();
    apply["approved"] = true.into();
    assert!(
        !cli(&world, None, &["edit-drafts", "apply"], Some(&apply))
            .status
            .success()
    );
    assert_eq!(image(&world.root), before);
}

#[tokio::test]
async fn approved_cli_edit_publishes_exact_reviewed_source_and_terminal_receipt_then_retries_read_only()
 {
    let (_directory, world) = fixture::fixture();
    let mut retained = draft(&world, "kind: character\ntitle: Explicitly revised actor\n");
    retained.body_mode = edits::BodyMode::Replace;
    let saved = edits::save(&world, &save_request(&world, retained.clone(), None)).unwrap();
    let input = review_request(&world, saved["sourceSha256"].as_str().unwrap());
    let before = image(&world.root);
    let native = edits::review(&world, &input).unwrap();
    assert_eq!(native["canApply"], true);
    let service = Service::new(world.clone());
    let envelope = service
        .execute(Operation::EditDraftReview {
            request: input.clone(),
        })
        .unwrap();
    assert_eq!(
        request(service.clone(), "GET", &review_uri(&input)).await,
        (StatusCode::OK, envelope.clone())
    );
    assert_eq!(
        success(cli(
            &world,
            None,
            &["edit-drafts", "review"],
            Some(&json!(input))
        )),
        envelope
    );
    assert_eq!(image(&world.root), before);
    let mut approved = json!(input);
    approved["reviewSha256"] = native["reviewSha256"].clone();
    approved["approved"] = false.into();
    assert!(
        !cli(&world, None, &["edit-drafts", "apply"], Some(&approved))
            .status
            .success()
    );
    assert_eq!(image(&world.root), before);
    approved["approved"] = true.into();
    let applied = success(cli(
        &world,
        None,
        &["edit-drafts", "apply"],
        Some(&approved),
    ));
    assert_eq!(applied["result"]["writesPerformed"], true);
    let fresh = source::load_world(&world.root, None).unwrap();
    assert_eq!(
        fresh.fingerprint,
        native["consequences"]["candidateFingerprint"]
    );
    let file = &native["consequences"]["files"][0];
    assert_eq!(
        fs::read_to_string(fresh.root.join(file["sourcePath"].as_str().unwrap())).unwrap(),
        file["afterSource"].as_str().unwrap()
    );
    assert_eq!(fresh.records[fixture::ACTOR].body, retained.body_markdown);
    let detail = edits::detail(&fresh, EDIT, applied["result"]["sourceSha256"].as_str()).unwrap();
    let mut restored = detail["draft"].clone();
    restored["phase"] = "draft".into();
    restored.as_object_mut().unwrap().remove("appliedReceipt");
    assert_eq!(restored, json!(retained));
    let after = image(&world.root);
    let retry = success(cli(
        &fresh,
        None,
        &["edit-drafts", "apply"],
        Some(&approved),
    ));
    assert_eq!(retry["result"]["alreadyApplied"], true);
    assert_eq!(retry["result"]["writesPerformed"], false);
    assert_eq!(image(&world.root), after);
    assert_eq!(
        request(service, "GET", &review_uri(&input)).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(image(&world.root), after);
}

#[tokio::test]
async fn edit_http_queries_are_closed_pinned_bounded_and_never_write() {
    let (_directory, world) = fixture::fixture();
    let input = save_request(
        &world,
        draft(&world, "kind: character\ntitle: Reviewed\n"),
        None,
    );
    let saved = edits::save(&world, &input).unwrap();
    let mut review = review_request(&world, saved["sourceSha256"].as_str().unwrap());
    let service = Service::new(world.clone());
    let before = image(&world.root);
    let routes = [
        "/api/edit-drafts/list?unknown=1".to_owned(),
        format!("/api/edit-drafts/detail?id={EDIT}&id={EDIT}"),
        format!(
            "/api/edit-drafts/detail?id={EDIT}&sha256={}",
            "0".repeat(64)
        ),
        format!("{}&unexpected=true", review_uri(&review)),
        format!("{}&maxCharacters=100000", review_uri(&review)),
    ];
    for route in routes {
        assert!(
            request(service.clone(), "GET", &route)
                .await
                .0
                .is_client_error(),
            "{route}"
        );
    }
    review.max_characters = 256;
    assert!(
        request(service.clone(), "GET", &review_uri(&review))
            .await
            .0
            .is_client_error()
    );
    let mut large = json!(input);
    large["draft"]["patch"] = "x".repeat(6000).into();
    assert!(
        request(
            service.clone(),
            "GET",
            &format!("/api/edit-drafts/preview?request={}", encoded(&large))
        )
        .await
        .0
        .is_client_error()
    );
    for path in ["/api/edit-drafts/save", "/api/edit-drafts/apply"] {
        assert_eq!(
            request(service.clone(), "GET", path).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            request(service.clone(), "POST", path).await.0,
            StatusCode::METHOD_NOT_ALLOWED
        );
    }
    assert_eq!(image(&world.root), before);
}

#[tokio::test]
async fn historical_private_transports_keep_original_blobs_and_authored_evidence_without_live_fallback()
 {
    let (_directory, world) = fixture::fixture();
    let saved = edits::save(
        &world,
        &save_request(&world, draft(&world, "unfinished: [\r\n"), None),
    )
    .unwrap();
    let mut action = fixture::draft(&world, &fixture::id(1));
    action.evidence=serde_json::from_value(json!([{"revision":"1".repeat(40),"path":"story/missing.md","blob_oid":"2".repeat(40),"target":{"kind":"transition","owner_id":"old-owner","id":"old-member"}}])).unwrap();
    action_store::save(&world, &fixture::request(&world, action.clone(), None)).unwrap();
    git(&world.root, &["init", "-q"]);
    let revision = commit(&world.root, "retained private history");
    let selected = source::load_world(&world.root, Some(&revision)).unwrap();
    let original = edits::detail(&selected, EDIT, None).unwrap();
    title(&world, "Later live title");
    fs::write(
        world.root.join("author/edits").join(format!("{EDIT}.json")),
        b"malformed live private bytes",
    )
    .unwrap();
    fs::write(
        fixture::path(&world, &action.id),
        b"different malformed action bytes",
    )
    .unwrap();
    let before = image(&world.root);
    let service = Service::new(selected.clone());
    let edit = service
        .execute(Operation::EditDraftDetail {
            id: EDIT.into(),
            expected_sha256: saved["sourceSha256"].as_str().map(str::to_owned),
        })
        .unwrap();
    assert_eq!(edit["result"]["source"], original["source"]);
    assert_eq!(edit["result"]["blobOid"], original["blobOid"]);
    assert_eq!(edit["result"]["currentness"]["state"], "stale");
    assert_eq!(edit["result"]["revision"], revision);
    assert_eq!(
        request(
            service.clone(),
            "GET",
            &format!("/api/edit-drafts/detail?id={EDIT}")
        )
        .await,
        (StatusCode::OK, edit.clone())
    );
    assert_eq!(
        success(cli(
            &world,
            Some(&revision),
            &["edit-drafts", "detail", EDIT],
            None
        )),
        edit
    );
    let retained = service
        .execute(Operation::ActionDraftDetail {
            id: action.id.clone(),
            expected_sha256: None,
        })
        .unwrap();
    assert_eq!(retained["result"]["draft"], json!(action));
    assert_eq!(
        request(
            service.clone(),
            "GET",
            &format!("/api/action-drafts/detail?id={}", action.id)
        )
        .await,
        (StatusCode::OK, retained.clone())
    );
    assert_eq!(
        success(cli(
            &world,
            Some(&revision),
            &["action-drafts", "detail", &action.id],
            None
        )),
        retained
    );
    let attempted = save_request(
        &selected,
        draft(&selected, "kind: character\n"),
        saved["sourceSha256"].as_str().map(str::to_owned),
    );
    assert!(
        service
            .execute(Operation::EditDraftSave {
                request: attempted.clone()
            })
            .is_err()
    );
    assert!(
        !cli(
            &world,
            Some(&revision),
            &["edit-drafts", "save"],
            Some(&json!(attempted))
        )
        .status
        .success()
    );
    assert_eq!(image(&world.root), before);
}

fn branching() -> (tempfile::TempDir, World, revision_review::Request) {
    let (directory, world) = fixture::fixture();
    fs::create_dir(world.root.join("author")).unwrap();
    fs::write(
        world.root.join("author").join("retained-marker"),
        b"Original private bytes stay outside revision review.\n",
    )
    .unwrap();
    git(&world.root, &["init", "-q"]);
    let base = commit(&world.root, "base");
    title(&world, "Left title");
    let left = commit(&world.root, "left");
    git(&world.root, &["reset", "--hard", &base]);
    title(&world, "Right title");
    let right = commit(&world.root, "right");
    let input = revision_review::decode(
        &json!({"baseRevision":base,"leftRevision":left,"rightRevision":right,
        "at":fixture::horizon(),"maxItems":1000,"maxCharacters":100000})
        .to_string(),
    )
    .unwrap();
    (directory, world, input)
}

#[tokio::test]
async fn original_revision_review_matches_transports_even_when_live_source_and_publication_are_unavailable()
 {
    let (_directory, world, input) = branching();
    let native = revision_review::review(&world.root, &input).unwrap();
    assert_eq!(native["writesPerformed"], false);
    assert_eq!(native["canApply"], false);
    assert_eq!(native["at"]["tick"], fixture::TICK.to_string());
    assert!(!native["conflicts"].as_array().unwrap().is_empty());
    let service = Service::new(world.clone());
    let expected = json!({"protocol":wedl::CONTRACT,"result":native});
    assert_eq!(
        service
            .execute(Operation::RevisionReview {
                request: input.clone()
            })
            .unwrap(),
        expected
    );
    fs::remove_file(world.root.join(&world.records[fixture::ACTOR].source_path)).unwrap();
    let invalid_capture = source::load_world(&world.root, None).unwrap();
    assert!(!wedl::validation::validate_world(&invalid_capture).valid);
    let invalid_service = Service::new(invalid_capture);
    fs::write(
        world.root.join(&world.world_record().unwrap().source_path),
        b"invalid live source",
    )
    .unwrap();
    fs::create_dir_all(world.root.join(".wedl-rust")).unwrap();
    fs::write(
        world.root.join(".wedl-rust/pending-outcome.json"),
        b"pending live transaction",
    )
    .unwrap();
    let before = image(&world.root);
    assert_eq!(
        success(cli(&world, None, &["revision-review"], Some(&json!(input)))),
        expected
    );
    assert_eq!(
        request(
            service.clone(),
            "GET",
            &format!("/api/revision-review?request={}", encoded(&json!(input)))
        )
        .await,
        (StatusCode::OK, expected.clone())
    );
    assert_eq!(
        invalid_service
            .execute(Operation::RevisionReview {
                request: input.clone()
            })
            .unwrap(),
        expected
    );
    assert_eq!(
        request(
            invalid_service,
            "GET",
            &format!("/api/revision-review?request={}", encoded(&json!(input)))
        )
        .await,
        (StatusCode::OK, expected.clone())
    );
    assert_eq!(
        service
            .execute(Operation::RevisionReview { request: input })
            .unwrap(),
        expected
    );
    assert_eq!(image(&world.root), before);
}

#[tokio::test]
async fn revision_queries_refuse_unpinned_ambiguous_and_incomplete_requests_without_side_effects() {
    let (_directory, world, input) = branching();
    let service = Service::new(world.clone());
    let before = image(&world.root);
    let mut unpinned = json!(input);
    unpinned["baseRevision"] = "HEAD".into();
    assert!(
        !cli(&world, None, &["revision-review"], Some(&unpinned))
            .status
            .success()
    );
    assert!(
        request(
            service.clone(),
            "GET",
            &format!("/api/revision-review?request={}", encoded(&unpinned))
        )
        .await
        .0
        .is_client_error()
    );
    let query = format!("/api/revision-review?request={}", encoded(&json!(input)));
    assert!(
        request(
            service.clone(),
            "GET",
            &(query.clone() + "&unexpected=true")
        )
        .await
        .0
        .is_client_error()
    );
    assert!(
        request(service.clone(), "GET", &(query.clone() + "&request=%7B%7D"))
            .await
            .0
            .is_client_error()
    );
    let mut small = json!(input);
    small["maxCharacters"] = 256.into();
    assert!(
        request(
            service.clone(),
            "GET",
            &format!("/api/revision-review?request={}", encoded(&small))
        )
        .await
        .0
        .is_client_error()
    );
    let mut nonancestor = json!(input);
    nonancestor["baseRevision"] = nonancestor["leftRevision"].clone();
    assert!(
        !cli(&world, None, &["revision-review"], Some(&nonancestor))
            .status
            .success()
    );
    assert!(
        !cli(
            &world,
            Some(&input.base_revision),
            &["revision-review"],
            Some(&json!(input))
        )
        .status
        .success()
    );
    assert_eq!(
        request(service, "POST", &query).await.0,
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(image(&world.root), before);
}
