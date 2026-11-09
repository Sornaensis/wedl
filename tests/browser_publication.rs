//! The live author adapter publishes only native saved/reviewed contracts.
//! The existing fixed-snapshot router and its GET-only regressions stay separate.
#[path = "support/actions.rs"]
mod fixture;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value as Json, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    process::Command,
    time::SystemTime,
};
use tower::ServiceExt;
use wedl::{
    action_drafts::{self, Draft, ReviewConfirmation, ReviewRequest, SaveRequest},
    action_outcomes, action_store, edit_drafts as edits, http, manuscript,
    model::World,
    retcon_reviews,
    service::Service,
    source,
    writing_frames::FamilySelection,
};

const OBJECT: &str = "obj_00000000000000000000000090";
const PROPOSAL: &str = "proposal_00000000000000000000000001";
const EDIT: &str = "edit_00000000000000000000000001";
const SEGMENT: &str = "segment_00000000000000000000000001";
const RETCON: &str = "review_00000000000000000000000001";
const BUDGET: usize = 100_000;
const OUTCOME_BODY: &str = "Reviewed browser outcome 🌲\r\nwithout final newline";
const EDIT_BODY: &str = "\r\nRetained edit 🌲\r\nTrailing  ";

type Image = BTreeMap<String, (Vec<u8>, SystemTime)>;

fn image(root: &Path, folders: &[&str]) -> Image {
    let mut result = BTreeMap::new();
    for folder in folders {
        let path = root.join(folder);
        if !path.exists() {
            continue;
        }
        for item in walkdir::WalkDir::new(path) {
            let item = item.unwrap();
            if item.file_type().is_file() {
                result.insert(
                    item.path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    (
                        fs::read(item.path()).unwrap(),
                        fs::metadata(item.path()).unwrap().modified().unwrap(),
                    ),
                );
            }
        }
    }
    result
}

fn changed(before: &Image, after: &Image) -> BTreeSet<String> {
    before
        .keys()
        .chain(after.keys())
        .filter(|path| before.get(*path) != after.get(*path))
        .cloned()
        .collect()
}

fn uri(path: &str, parameters: &[(&str, &str)]) -> String {
    let query = parameters
        .iter()
        .map(|(key, value)| {
            let encoded = value
                .bytes()
                .map(|byte| format!("%{byte:02X}"))
                .collect::<String>();
            format!("{key}={encoded}")
        })
        .collect::<Vec<_>>()
        .join("&");
    if query.is_empty() {
        format!("/api/{path}")
    } else {
        format!("/api/{path}?{query}")
    }
}

async fn call(app: &Router, method: &str, uri: &str, input: Option<&Json>) -> (StatusCode, Json) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "127.0.0.1:8765");
    let body = if let Some(input) = input {
        request = request
            .header("origin", "http://127.0.0.1:8765")
            .header("content-type", "application/json")
            .header("x-wedl-write", "1");
        Body::from(input.to_string())
    } else {
        Body::empty()
    };
    let response = app
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Json::Null))
}

async fn get(app: &Router, path: &str, parameters: &[(&str, &str)]) -> Json {
    let (status, result) = call(app, "GET", &uri(path, parameters), None).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    result
}

async fn post(app: &Router, path: &str, input: &Json) -> Json {
    let (status, result) = call(app, "POST", &uri(path, &[]), Some(input)).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    result
}

async fn confirmation(app: &Router, world: &World, draft: &Draft) -> ReviewConfirmation {
    let request = ReviewRequest {
        draft: draft.clone(),
        frame: fixture::frame(world, "author"),
        briefs: vec![],
        families: FamilySelection::default(),
        max_characters: BUDGET,
    };
    let before = image(&world.root, &["story", "author"]);
    let native = action_drafts::review(world, &request).unwrap();
    let result = get(
        app,
        "action-drafts/review",
        &[("request", &json!(request).to_string())],
    )
    .await;
    assert_eq!(result["result"], native);
    assert_eq!(
        result["result"]["frame"]["at"]["tick"],
        fixture::TICK.to_string()
    );
    assert_eq!(result["result"]["requiredAnswered"], true);
    assert_eq!(image(&world.root, &["story", "author"]), before);
    ReviewConfirmation {
        frame: request.frame,
        briefs: request.briefs,
        families: request.families,
        max_characters: BUDGET,
        review_sha256: native["reviewSha256"].as_str().unwrap().into(),
        sufficient: true,
    }
}

async fn save_action(
    app: &Router,
    world: &World,
    draft: Draft,
    prior: Option<String>,
    reviewed: bool,
) -> Json {
    let review = if reviewed {
        Some(confirmation(app, world, &draft).await)
    } else {
        None
    };
    let request = SaveRequest {
        expected_fingerprint: world.fingerprint.clone(),
        expected_sha256: prior,
        draft,
        review,
    };
    let expected = action_store::preview(world, &request).unwrap();
    let canonical = image(&world.root, &["story"]);
    let saved = post(app, "action-drafts/save", &json!(request)).await;
    assert_eq!(saved["fingerprint"], world.fingerprint);
    assert_eq!(saved["result"]["writesPerformed"], true);
    assert_eq!(saved["result"]["sourceSha256"], expected["sourceSha256"]);
    assert_eq!(
        fs::read(fixture::path(world, &request.draft.id)).unwrap(),
        expected["afterSource"].as_str().unwrap().as_bytes()
    );
    assert_eq!(image(&world.root, &["story"]), canonical);
    let detail = get(
        app,
        "action-drafts/detail",
        &[
            ("id", &request.draft.id),
            ("sha256", saved["result"]["sourceSha256"].as_str().unwrap()),
        ],
    )
    .await;
    assert_eq!(detail["result"]["draft"], json!(request.draft));
    saved["result"].clone()
}

async fn proposed(
    app: &Router,
    world: &World,
    no_op: bool,
) -> action_store::ProposalPreviewRequest {
    let mut draft = fixture::draft(world, &fixture::id(1));
    let initial = save_action(app, world, draft.clone(), None, false).await;
    draft.questions[0].answer = Some(String::new());
    draft.phase = "resolution-needed".into();
    let ready = save_action(
        app,
        world,
        draft.clone(),
        Some(initial["sourceSha256"].as_str().unwrap().into()),
        true,
    )
    .await;
    let operations = if no_op {
        json!([{"type":"record.update","entityId":fixture::ACTOR,"patch":"kind: character\n"}])
    } else {
        json!([
            {"type":"record.update","entityId":fixture::ACTOR,
                "patch":"kind: character\nbody: \"Reviewed browser outcome 🌲\\r\\nwithout final newline\"\n"},
            {"type":"record.create","draft":format!("kind: object\nid: {OBJECT}\ntitle: Explicit browser object\nbody: 'Authored outcome object.'\n")}
        ])
    };
    draft.proposals.push(
        serde_json::from_value(json!({
            "id":PROPOSAL,"summary":"Explicit reviewed browser proposal.",
            "contextFingerprint":world.fingerprint,"evidence":[],"resolver":"human",
            "questionAnswers":[{"questionId":draft.questions[0].id,"answer":""}],
            "consequences":{"expectedFingerprint":world.fingerprint,"operations":operations}
        }))
        .unwrap(),
    );
    draft.phase = "proposed".into();
    let saved = save_action(
        app,
        world,
        draft,
        Some(ready["sourceSha256"].as_str().unwrap().into()),
        true,
    )
    .await;
    action_store::ProposalPreviewRequest {
        expected_fingerprint: world.fingerprint.clone(),
        action_id: fixture::id(1),
        expected_sha256: saved["sourceSha256"].as_str().unwrap().into(),
        proposal_id: PROPOSAL.into(),
        max_characters: BUDGET,
    }
}

fn outcome_parameters(input: &action_store::ProposalPreviewRequest) -> Vec<(&str, String)> {
    vec![
        ("expectedFingerprint", input.expected_fingerprint.clone()),
        ("id", input.action_id.clone()),
        ("sha256", input.expected_sha256.clone()),
        ("proposalId", input.proposal_id.clone()),
        ("maxCharacters", input.max_characters.to_string()),
    ]
}

async fn outcome_review(
    app: &Router,
    world: &World,
    input: &action_store::ProposalPreviewRequest,
) -> Json {
    let before = image(&world.root, &["story", "author"]);
    let parameters = outcome_parameters(input);
    let borrowed = parameters
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect::<Vec<_>>();
    let result = get(app, "action-drafts/review-outcome", &borrowed).await;
    assert_eq!(
        result["result"],
        action_outcomes::review(world, input).unwrap()
    );
    assert_eq!(result["result"]["canApply"], true);
    assert_eq!(result["result"]["writesPerformed"], false);
    assert_eq!(image(&world.root, &["story", "author"]), before);
    result["result"].clone()
}

fn approved(input: &action_store::ProposalPreviewRequest, review: &Json) -> Json {
    let mut value = json!(input);
    value["reviewSha256"] = review["reviewSha256"].clone();
    value["approved"] = true.into();
    value
}

#[tokio::test]
async fn live_action_saves_reviewed_outcome_and_exact_retry_refresh_source_without_restarting() {
    let (_directory, world) = fixture::fixture();
    let app = http::live_router(Service::new(world.clone()));
    assert_eq!(
        get(&app, "session", &[]).await["result"],
        json!({"mode":"live-author","writesEnabled":true})
    );
    let input = proposed(&app, &world, false).await;
    let review = outcome_review(&app, &world, &input).await;
    let value = approved(&input, &review);
    let before = image(&world.root, &["story", "author"]);
    for (key, replacement, status) in [
        ("approved", json!(false), StatusCode::BAD_REQUEST),
        ("reviewSha256", json!("f".repeat(64)), StatusCode::CONFLICT),
        (
            "expectedSha256",
            json!("e".repeat(64)),
            StatusCode::CONFLICT,
        ),
    ] {
        let mut invalid = value.clone();
        invalid[key] = replacement;
        assert_eq!(
            call(
                &app,
                "POST",
                "/api/action-drafts/apply-outcome",
                Some(&invalid)
            )
            .await
            .0,
            status
        );
        assert_eq!(image(&world.root, &["story", "author"]), before);
    }
    let before_source = image(&world.root, &["story"]);
    let result = post(&app, "action-drafts/apply-outcome", &value).await;
    assert_eq!(result["fingerprint"], world.fingerprint);
    assert_eq!(result["result"]["writesPerformed"], true);
    let fresh = source::load_world(&world.root, None).unwrap();
    assert_eq!(
        fresh.fingerprint,
        review["consequences"]["candidateFingerprint"]
    );
    assert_eq!(result["result"]["fingerprint"], fresh.fingerprint);
    assert_eq!(fresh.records[fixture::ACTOR].body, OUTCOME_BODY);
    assert!(fresh.records.contains_key(OBJECT));
    let files = review["consequences"]["files"].as_array().unwrap();
    let expected_paths = files
        .iter()
        .map(|file| {
            let source_path = file["sourcePath"].as_str().unwrap();
            assert_eq!(
                fs::read(world.root.join(source_path)).unwrap(),
                file["afterSource"].as_str().unwrap().as_bytes()
            );
            source_path.to_owned()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        changed(&before_source, &image(&world.root, &["story"])),
        expected_paths
    );
    assert_eq!(
        get(&app, "status", &[]).await["fingerprint"],
        fresh.fingerprint
    );
    let entity = get(&app, "entity", &[("id", fixture::ACTOR)]).await;
    assert_eq!(entity["fingerprint"], fresh.fingerprint);
    assert_eq!(entity["result"]["bodyMarkdown"], OUTCOME_BODY);
    let detail = get(
        &app,
        "action-drafts/detail",
        &[
            ("id", input.action_id.as_str()),
            ("sha256", result["result"]["sourceSha256"].as_str().unwrap()),
        ],
    )
    .await;
    assert_eq!(detail["result"]["draft"]["phase"], "applied");
    assert_eq!(
        detail["result"]["draft"]["appliedReceipt"],
        result["result"]["receipt"]
    );
    let after = image(&world.root, &["story", "author"]);
    let retry = post(&app, "action-drafts/apply-outcome", &value).await;
    assert_eq!(retry["fingerprint"], fresh.fingerprint);
    assert_eq!(retry["result"]["alreadyApplied"], true);
    assert_eq!(retry["result"]["writesPerformed"], false);
    let native = action_outcomes::apply(
        &fresh,
        &action_outcomes::decode_apply(&value.to_string()).unwrap(),
    )
    .unwrap();
    assert_eq!(retry["result"], native);
    let parameters = outcome_parameters(&input);
    let borrowed = parameters
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        call(
            &app,
            "GET",
            &uri("action-drafts/review-outcome", &borrowed),
            None
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(image(&world.root, &["story", "author"]), after);
}

#[tokio::test]
async fn live_no_op_outcome_updates_private_receipt_without_rewriting_canonical_source() {
    let (_directory, world) = fixture::fixture();
    let app = http::live_router(Service::new(world.clone()));
    let input = proposed(&app, &world, true).await;
    let review = outcome_review(&app, &world, &input).await;
    let before = image(&world.root, &["story"]);
    let private = fs::read(fixture::path(&world, &input.action_id)).unwrap();
    let result = post(
        &app,
        "action-drafts/apply-outcome",
        &approved(&input, &review),
    )
    .await;
    assert_eq!(result["result"]["canonicalNoOp"], true);
    assert_eq!(result["result"]["changedFiles"], 0);
    assert_eq!(result["result"]["writesPerformed"], true);
    assert_eq!(
        get(&app, "status", &[]).await["fingerprint"],
        world.fingerprint
    );
    assert_eq!(image(&world.root, &["story"]), before);
    assert_ne!(
        fs::read(fixture::path(&world, &input.action_id)).unwrap(),
        private
    );
    let detail = get(
        &app,
        "action-drafts/detail",
        &[("id", input.action_id.as_str())],
    )
    .await;
    assert_eq!(detail["result"]["draft"]["phase"], "applied");
    assert_eq!(
        detail["result"]["draft"]["appliedReceipt"],
        result["result"]["receipt"]
    );
}

#[tokio::test]
async fn live_retained_edit_publishes_exact_reviewed_body_and_receipt_then_retries_read_only() {
    let (_directory, world) = fixture::fixture();
    let app = http::live_router(Service::new(world.clone()));
    let target = &world.records[fixture::ACTOR];
    let draft = edits::decode(&json!({
        "id":EDIT,"title":"Browser retained edit","worldId":world.world_record().unwrap().id,
        "targetId":target.id,"baseFingerprint":world.fingerprint,
        "baseSourceSha256":action_drafts::sha256(&target.raw_bytes),"baseSourcePath":target.source_path,
        "patch":"kind: character\ntitle: Explicit browser revision\n","bodyMode":"replace",
        "bodyMarkdown":EDIT_BODY,"phase":"draft"
    }).to_string()).unwrap();
    let save = edits::SaveRequest {
        expected_fingerprint: world.fingerprint.clone(),
        expected_sha256: None,
        draft,
    };
    let before = image(&world.root, &["story"]);
    let preview = edits::preview(&world, &save).unwrap();
    let saved = post(&app, "edit-drafts/save", &json!(save)).await;
    assert_eq!(saved["fingerprint"], world.fingerprint);
    assert_eq!(saved["result"]["sourceSha256"], preview["sourceSha256"]);
    assert_eq!(
        fs::read(world.root.join(format!("author/edits/{EDIT}.json"))).unwrap(),
        preview["afterSource"].as_str().unwrap().as_bytes()
    );
    assert_eq!(image(&world.root, &["story"]), before);
    let request = edits::ReviewRequest {
        expected_fingerprint: world.fingerprint.clone(),
        edit_id: EDIT.into(),
        expected_sha256: saved["result"]["sourceSha256"].as_str().unwrap().into(),
        max_characters: BUDGET,
    };
    let all = image(&world.root, &["story", "author"]);
    let report = get(
        &app,
        "edit-drafts/review",
        &[
            ("expectedFingerprint", &world.fingerprint),
            ("id", EDIT),
            ("sha256", &request.expected_sha256),
            ("maxCharacters", "100000"),
        ],
    )
    .await;
    assert_eq!(report["result"], edits::review(&world, &request).unwrap());
    assert_eq!(report["result"]["canApply"], true);
    assert_eq!(image(&world.root, &["story", "author"]), all);
    let mut application = json!(request);
    application["reviewSha256"] = report["result"]["reviewSha256"].clone();
    application["approved"] = false.into();
    assert_eq!(
        call(&app, "POST", "/api/edit-drafts/apply", Some(&application))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(image(&world.root, &["story", "author"]), all);
    application["approved"] = true.into();
    let applied = post(&app, "edit-drafts/apply", &application).await;
    assert_eq!(applied["fingerprint"], world.fingerprint);
    let fresh = source::load_world(&world.root, None).unwrap();
    assert_eq!(
        fresh.fingerprint,
        report["result"]["consequences"]["candidateFingerprint"]
    );
    assert_eq!(fresh.records[fixture::ACTOR].body, EDIT_BODY);
    assert_eq!(
        changed(&before, &image(&world.root, &["story"])),
        BTreeSet::from([target.source_path.clone()])
    );
    let reviewed = &report["result"]["consequences"]["files"][0];
    assert_eq!(
        fs::read(world.root.join(&target.source_path)).unwrap(),
        reviewed["afterSource"].as_str().unwrap().as_bytes()
    );
    assert_eq!(
        get(&app, "status", &[]).await["fingerprint"],
        fresh.fingerprint
    );
    let detail = get(&app, "edit-drafts/detail", &[("id", EDIT)]).await;
    assert_eq!(detail["result"]["draft"]["phase"], "applied");
    assert_eq!(
        detail["result"]["draft"]["appliedReceipt"],
        applied["result"]["receipt"]
    );
    let after = image(&world.root, &["story", "author"]);
    let retry = post(&app, "edit-drafts/apply", &application).await;
    assert_eq!(retry["result"]["alreadyApplied"], true);
    assert_eq!(retry["result"]["writesPerformed"], false);
    assert_eq!(
        retry["result"],
        edits::apply(
            &fresh,
            &edits::decode_apply(&application.to_string()).unwrap()
        )
        .unwrap()
    );
    assert_eq!(
        call(&app, "POST", "/api/edit-drafts/save", Some(&json!(save)))
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(image(&world.root, &["story", "author"]), after);
}

#[tokio::test]
async fn live_manuscript_save_preserves_exact_bytes_and_separates_private_from_canonical_fingerprints()
 {
    let (_directory, world) = fixture::fixture();
    let app = http::live_router(Service::new(world.clone()));
    let list = json!({"revision":null,"kind":null,"maxItems":1024});
    let before_list = get(&app, "manuscripts/list", &[("request", &list.to_string())]).await;
    let source = format!(
        "---\r\nschema: wedl-author/v1\r\nkind: manuscript-segment\r\nid: {SEGMENT}\r\ntitle: Browser private manuscript\r\nsource_links: [{{kind: record, id: {}}}]\r\n---\r\nExact private source 🌲\r\nTrailing  ",
        fixture::ACTOR
    );
    let mut input = json!({"expectedFingerprint":world.fingerprint,
        "expectedAuthorFingerprint":before_list["result"]["authorFingerprint"],
        "expectedSha256":null,"source":source});
    let request = manuscript::decode_save(&input.to_string()).unwrap();
    let canonical = image(&world.root, &["story"]);
    let preview = manuscript::save_preview(&world, &request).unwrap();
    let saved = post(&app, "manuscripts/save", &input).await;
    assert_eq!(saved["fingerprint"], world.fingerprint);
    assert_eq!(saved["result"]["writesPerformed"], true);
    assert_eq!(saved["result"]["afterSource"], source);
    assert_eq!(saved["result"]["afterSha256"], preview["afterSha256"]);
    assert_eq!(
        saved["result"]["afterAuthorFingerprint"],
        preview["afterAuthorFingerprint"]
    );
    assert_eq!(
        fs::read(
            world
                .root
                .join(manuscript::document_path("manuscript-segment", SEGMENT).unwrap())
        )
        .unwrap(),
        source.as_bytes()
    );
    assert_eq!(
        get(&app, "status", &[]).await["fingerprint"],
        world.fingerprint
    );
    assert_eq!(image(&world.root, &["story"]), canonical);
    let after_list = get(&app, "manuscripts/list", &[("request", &list.to_string())]).await;
    assert_eq!(
        after_list["result"]["authorFingerprint"],
        saved["result"]["afterAuthorFingerprint"]
    );
    assert_ne!(
        before_list["result"]["authorFingerprint"],
        after_list["result"]["authorFingerprint"]
    );
    let after = image(&world.root, &["story", "author"]);
    assert_eq!(
        call(&app, "POST", "/api/manuscripts/save", Some(&input))
            .await
            .0,
        StatusCode::CONFLICT
    );
    input["expectedSha256"] = saved["result"]["afterSha256"].clone();
    input["expectedAuthorFingerprint"] = saved["result"]["afterAuthorFingerprint"].clone();
    let repeated = post(&app, "manuscripts/save", &input).await;
    assert_eq!(repeated["result"]["changed"], false);
    assert_eq!(repeated["result"]["writesPerformed"], false);
    assert_eq!(image(&world.root, &["story", "author"]), after);
}

fn git(root: &Path, arguments: &[&str]) -> String {
    let result = Command::new("git")
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
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().into()
}

#[tokio::test]
async fn live_retcon_save_retains_original_analysis_and_changes_only_private_source() {
    let (_directory, world) = fixture::fixture();
    git(&world.root, &["init", "-q"]);
    git(&world.root, &["add", "--", "story"]);
    git(&world.root, &["commit", "-q", "-m", "Original source"]);
    let base = git(&world.root, &["rev-parse", "HEAD"]);
    let analysis_input = json!({"baseRevision":base,
        "plan":{"expectedFingerprint":world.fingerprint,"operations":[
            {"type":"record.update","entityId":fixture::PLACE,"patch":"kind: location\ntitle: Reviewed landing\n"}]},
        "impactRequest":{"target":{"kind":"record","id":fixture::PLACE},
            "at":fixture::horizon(),"depth":32,"maxItems":1000}});
    let analysis = retcon_reviews::preview(
        &world.root,
        &retcon_reviews::decode_preview(&analysis_input.to_string()).unwrap(),
    )
    .unwrap();
    let input = json!({"expectedFingerprint":world.fingerprint,"expectedSha256":null,
        "document":{"id":RETCON,"title":"Private browser review","analysis":analysis["analysis"],
            "decisions":[],"bodyMarkdown":"\r\nPrivate review 🌲\r\nTrailing  "}});
    let request = retcon_reviews::decode_save(&input.to_string()).unwrap();
    let preview = retcon_reviews::save_preview(&world, &request).unwrap();
    let app = http::live_router(Service::new(world.clone()));
    let canonical = image(&world.root, &["story"]);
    let saved = post(&app, "retcon-reviews/save", &input).await;
    assert_eq!(saved["result"]["writesPerformed"], true);
    assert_eq!(saved["result"]["sourceSha256"], preview["sourceSha256"]);
    assert_eq!(image(&world.root, &["story"]), canonical);
    assert_eq!(
        get(&app, "status", &[]).await["fingerprint"],
        world.fingerprint
    );
    let detail =
        json!({"revision":null,"id":RETCON,"expectedSha256":saved["result"]["sourceSha256"]});
    let actual = get(
        &app,
        "retcon-reviews/detail",
        &[("request", &detail.to_string())],
    )
    .await;
    assert_eq!(
        actual["result"],
        retcon_reviews::detail(
            &world.root,
            &retcon_reviews::decode_detail(&detail.to_string()).unwrap()
        )
        .unwrap()
    );
    assert_eq!(
        actual["result"]["document"]["analysis"],
        analysis["analysis"]
    );
}
