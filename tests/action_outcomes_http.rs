//! Reviewed outcomes share the native contract; HTTP remains read-only.
#[path = "support/actions.rs"]
mod fixture;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value as Json, json};
use std::{fs, process::Command};
use tower::ServiceExt;
use wedl::{
    action_drafts::{self, Draft, ReviewConfirmation, ReviewRequest, SaveRequest},
    action_outcomes::{self, ApplyRequest, VerifyRequest},
    action_store::{self, ProposalPreviewRequest},
    http,
    model::World,
    publication,
    service::{Operation, Service},
    source,
    writing_frames::FamilySelection,
};

const OBJECT: &str = "obj_00000000000000000000000090";
const PROPOSAL: &str = "proposal_00000000000000000000000001";

fn save(world: &World, draft: Draft, sha: Option<&str>, confirmed: bool) -> Json {
    let review = confirmed.then(|| {
        let input = ReviewRequest {
            draft: draft.clone(),
            frame: fixture::frame(world, "author"),
            briefs: vec![],
            families: FamilySelection::default(),
            max_characters: 100_000,
        };
        let report = action_drafts::review(world, &input).unwrap();
        ReviewConfirmation {
            frame: input.frame,
            briefs: input.briefs,
            families: input.families,
            max_characters: input.max_characters,
            review_sha256: report["reviewSha256"].as_str().unwrap().into(),
            sufficient: true,
        }
    });
    action_store::save(
        world,
        &SaveRequest {
            expected_fingerprint: world.fingerprint.clone(),
            expected_sha256: sha.map(str::to_owned),
            draft,
            review,
        },
    )
    .unwrap()
}

fn stored(world: &World) -> ProposalPreviewRequest {
    let mut draft = fixture::draft(world, &fixture::id(1));
    let created = save(world, draft.clone(), None, false);
    draft.phase = "resolution-needed".into();
    draft.questions[0].answer =
        Some("An explicitly authored alternative is ready for review.".into());
    let ready = save(world, draft.clone(), created["sourceSha256"].as_str(), true);
    draft.proposals.push(serde_json::from_value(json!({
        "id":PROPOSAL,"summary":"Replace exact prose and create an object.",
        "contextFingerprint":world.fingerprint,"evidence":[],"resolver":"human",
        "questionAnswers":[{"questionId":draft.questions[0].id,"answer":draft.questions[0].answer}],
        "consequences":{"expectedFingerprint":world.fingerprint,"operations":[
            {"type":"record.update","entityId":fixture::ACTOR,
                "patch":"kind: character\nbody: \"Reviewed outcome 🌲\\r\\nwithout final newline\"\n"},
            {"type":"record.create","draft":format!("kind: object\nid: {OBJECT}\ntitle: Explicitly created object\nbody: 'No implicit knowledge.'\n")}
        ]}
    })).unwrap());
    draft.phase = "proposed".into();
    let saved = save(world, draft, ready["sourceSha256"].as_str(), true);
    ProposalPreviewRequest {
        expected_fingerprint: world.fingerprint.clone(),
        action_id: fixture::id(1),
        expected_sha256: saved["sourceSha256"].as_str().unwrap().into(),
        proposal_id: PROPOSAL.into(),
        max_characters: 100_000,
    }
}

fn apply_request(world: &World, review: &ProposalPreviewRequest) -> ApplyRequest {
    let report = action_outcomes::review(world, review).unwrap();
    let mut value = json!(review);
    value["reviewSha256"] = report["reviewSha256"].clone();
    value["approved"] = true.into();
    action_outcomes::decode_apply(&value.to_string()).unwrap()
}

fn verification(world: &World) -> VerifyRequest {
    let detail = action_store::detail(world, &fixture::id(1), None).unwrap();
    VerifyRequest {
        expected_fingerprint: world.fingerprint.clone(),
        action_id: fixture::id(1),
        expected_sha256: detail["sourceSha256"].as_str().unwrap().into(),
        max_characters: 100_000,
    }
}

fn review_uri(input: &ProposalPreviewRequest) -> String {
    format!(
        "/api/action-drafts/review-outcome?expectedFingerprint={}&id={}&sha256={}&proposalId={}&maxCharacters={}",
        input.expected_fingerprint,
        input.action_id,
        input.expected_sha256,
        input.proposal_id,
        input.max_characters
    )
}
fn verify_uri(input: &VerifyRequest) -> String {
    format!(
        "/api/action-drafts/verify-outcome?expectedFingerprint={}&id={}&sha256={}&maxCharacters={}",
        input.expected_fingerprint, input.action_id, input.expected_sha256, input.max_characters
    )
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
fn cli(world: &World, args: &[&str]) -> (i32, Json) {
    let output = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .args(["--compact", "--repo"])
        .arg(&world.root)
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
fn file(world: &World, name: &str, value: &Json) -> String {
    let path = world.root.parent().unwrap().join(name);
    fs::write(&path, serde_json::to_vec(value).unwrap()).unwrap();
    path.to_str().unwrap().into()
}

#[tokio::test]
async fn outcome_review_has_native_service_cli_http_parity_and_no_writes() {
    let (_directory, world) = fixture::fixture();
    let input = stored(&world);
    let before = fixture::source_image(&world);
    let private = fs::read(fixture::path(&world, &input.action_id)).unwrap();
    let service = Service::new(world.clone());
    let result = service
        .execute(Operation::ActionOutcomeReview {
            request: input.clone(),
        })
        .unwrap();
    assert_eq!(
        result["result"],
        action_outcomes::review(&world, &input).unwrap()
    );
    assert_eq!(
        request(service, "GET", &review_uri(&input)).await,
        (StatusCode::OK, result.clone())
    );
    let path = file(&world, "outcome-review.json", &json!(input));
    assert_eq!(
        cli(
            &world,
            &["action-drafts", "review-outcome", "--file", &path]
        ),
        (0, result.clone())
    );
    assert_eq!(result["result"]["writesPerformed"], false);
    assert_eq!(result["result"]["canApply"], true);
    assert_eq!(result["result"]["consequences"]["canApply"], false);
    assert_eq!(
        result["result"]["draft"]["horizon"]["tick"],
        fixture::TICK.to_string()
    );
    assert_eq!(before, fixture::source_image(&world));
    assert_eq!(
        private,
        fs::read(fixture::path(&world, &input.action_id)).unwrap()
    );
    assert_eq!(publication::status(&world.root).unwrap()["pending"], false);
}

#[tokio::test]
async fn cli_apply_retry_and_read_only_verification_preserve_exact_source() {
    let (_directory, world) = fixture::fixture();
    let review = stored(&world);
    let input = apply_request(&world, &review);
    let path = file(&world, "outcome-apply.json", &json!(input));
    let (code, applied) = cli(&world, &["action-drafts", "apply-outcome", "--file", &path]);
    assert_eq!(code, 0, "{applied}");
    assert_eq!(applied["result"]["writesPerformed"], true);
    let current = source::load_world(&world.root, None).unwrap();
    assert_eq!(
        current.records[fixture::ACTOR].body,
        "Reviewed outcome 🌲\r\nwithout final newline"
    );
    assert!(current.records.contains_key(OBJECT));
    let before_retry = fixture::source_image(&current);
    let private = fs::read(fixture::path(&current, &review.action_id)).unwrap();
    let (code, repeated) = cli(
        &current,
        &["action-drafts", "apply-outcome", "--file", &path],
    );
    assert_eq!(code, 0, "{repeated}");
    assert_eq!(repeated["result"]["writesPerformed"], false);
    assert_eq!(repeated["result"]["alreadyApplied"], true);
    assert_eq!(before_retry, fixture::source_image(&current));
    assert_eq!(
        private,
        fs::read(fixture::path(&current, &review.action_id)).unwrap()
    );
    let input = verification(&current);
    let service = Service::new(current.clone());
    let expected = service
        .execute(Operation::ActionOutcomeVerify {
            request: input.clone(),
        })
        .unwrap();
    assert_eq!(expected["result"]["verified"], true);
    assert_eq!(expected["result"]["writesPerformed"], false);
    assert_eq!(
        request(service, "GET", &verify_uri(&input)).await,
        (StatusCode::OK, expected.clone())
    );
    let path = file(&current, "outcome-verify.json", &json!(input));
    assert_eq!(
        cli(
            &current,
            &["action-drafts", "verify-outcome", "--file", &path]
        ),
        (0, expected)
    );
    assert_eq!(publication::status(&world.root).unwrap()["pending"], false);
}

#[tokio::test]
async fn outcome_queries_are_closed_and_http_never_applies_or_recovers() {
    let (_directory, world) = fixture::fixture();
    let input = stored(&world);
    let service = Service::new(world.clone());
    let valid = review_uri(&input);
    for suffix in [
        "&approved=true",
        "&id=another",
        "&maxCharacters=100000",
        "&reviewSha256=x",
    ] {
        assert_eq!(
            request(service.clone(), "GET", &(valid.clone() + suffix))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        request(service.clone(), "GET", "/api/action-drafts/review-outcome")
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    for route in [
        "/api/action-drafts/apply-outcome",
        "/api/consequences/recover",
        "/api/consequences/publication-status",
    ] {
        assert_eq!(
            request(service.clone(), "GET", route).await.0,
            StatusCode::NOT_FOUND
        );
    }
    for method in ["POST", "PUT", "PATCH", "DELETE"] {
        assert_eq!(
            request(service.clone(), method, &valid).await.0,
            StatusCode::METHOD_NOT_ALLOWED
        );
    }
    let unapproved = json!({"expectedFingerprint":input.expected_fingerprint,"actionId":input.action_id,
        "expectedSha256":input.expected_sha256,"proposalId":input.proposal_id,"maxCharacters":100000,
        "reviewSha256":"0".repeat(64),"approved":false});
    let path = file(&world, "unapproved.json", &unapproved);
    assert_eq!(
        cli(&world, &["action-drafts", "apply-outcome", "--file", &path]).0,
        2
    );
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
}

#[tokio::test]
async fn old_server_snapshot_refuses_outcome_reads_after_publication() {
    let (_directory, world) = fixture::fixture();
    let input = stored(&world);
    let old_service = Service::new(world.clone());
    let applied = old_service
        .execute(Operation::ActionOutcomeApply {
            request: apply_request(&world, &input),
        })
        .unwrap();
    assert_eq!(applied["result"]["writesPerformed"], true);
    assert_eq!(
        request(old_service.clone(), "GET", &review_uri(&input))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let current = source::load_world(&world.root, None).unwrap();
    assert_eq!(
        request(old_service, "GET", &verify_uri(&verification(&current)))
            .await
            .0,
        StatusCode::CONFLICT
    );
}

#[test]
fn publication_cli_can_inspect_recovery_state_when_source_cannot_load() {
    let (_directory, world) = fixture::fixture();
    fs::write(
        world.root.join(&world.world_record().unwrap().source_path),
        b"broken source",
    )
    .unwrap();
    assert!(source::load_world(&world.root, None).is_err());
    let (code, status) = cli(&world, &["consequences", "publication-status"]);
    assert_eq!(code, 0, "{status}");
    assert_eq!(status["result"]["pending"], false);
    assert_eq!(
        cli(
            &world,
            &["--revision", "HEAD", "consequences", "publication-status"]
        )
        .0,
        2
    );
    assert_eq!(
        cli(
            &world,
            &[
                "consequences",
                "recover",
                "--expected-journal-sha256",
                &"0".repeat(64)
            ]
        )
        .0,
        2
    );
    assert_eq!(
        fs::read(world.root.join(&world.world_record().unwrap().source_path)).unwrap(),
        b"broken source"
    );
}

#[test]
fn verification_distinguishes_unrelated_drift_from_changed_applied_files() {
    let (_directory, world) = fixture::fixture();
    let review = stored(&world);
    action_outcomes::apply(&world, &apply_request(&world, &review)).unwrap();
    let current = source::load_world(&world.root, None).unwrap();
    let path = current
        .root
        .join(&current.records[fixture::PLACE].source_path);
    let mut bytes = fs::read(&path).unwrap();
    bytes.extend_from_slice(b"Unrelated later prose.\n");
    fs::write(path, bytes).unwrap();
    let changed = source::load_world(&world.root, None).unwrap();
    let report = action_outcomes::verify(&changed, &verification(&changed)).unwrap();
    assert_eq!(report["verified"], false);
    assert_eq!(report["sourceMatches"], false);
    assert_eq!(report["filesMatch"], true);
    let path = changed.root.join(&changed.records[OBJECT].source_path);
    let mut bytes = fs::read(&path).unwrap();
    bytes.extend_from_slice(b"Changed applied file.\n");
    fs::write(path, bytes).unwrap();
    let changed = source::load_world(&changed.root, None).unwrap();
    let report = action_outcomes::verify(&changed, &verification(&changed)).unwrap();
    assert_eq!(report["filesMatch"], false);
    assert!(
        report["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["entityId"] == OBJECT && file["status"] == "changed")
    );
}
