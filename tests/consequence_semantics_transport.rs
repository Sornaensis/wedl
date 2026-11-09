//! Explicit semantic policy stays inside the reviewed plan and is shared by
//! native, service, CLI and read-only HTTP callers.
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
    consequence_semantics as semantics, consequences, http,
    model::World,
    service::{Operation, Service},
    source, validation,
    writing_frames::FamilySelection,
};

const PROPOSAL: &str = "proposal_00000000000000000000000001";

fn source_fixture() -> (tempfile::TempDir, World) {
    let (directory, world) = fixture::fixture();
    let owner = world.world_record().unwrap();
    let mut config = owner.frontmatter.clone();
    config["state_keys"] = serde_yaml::to_value(json!({"character":{
        "condition":{"type":"string"}, "counter":{"type":"integer"}
    }}))
    .unwrap();
    fs::write(
        world.root.join(&owner.source_path),
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&config).unwrap(),
            owner.body
        ),
    )
    .unwrap();
    change_actor(&world, "before");
    let world = source::load_world(&world.root, None).unwrap();
    assert!(validation::validate_world(&world).valid);
    (directory, world)
}

fn change_actor(world: &World, condition: &str) {
    let actor = &world.records[fixture::ACTOR];
    let mut data = actor.frontmatter.clone();
    data["initial_state"] =
        serde_yaml::to_value(json!({"condition":condition,"counter":fixture::TICK})).unwrap();
    fs::write(
        world.root.join(&actor.source_path),
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&data).unwrap(),
            actor.body
        ),
    )
    .unwrap();
}

fn analysis(expected: &str, policy: &str) -> Json {
    json!({"at":fixture::horizon(),"policy":policy,"expectations":[
        {"id":"condition","predicate":{"kind":"state.equals","targetId":fixture::ACTOR,
            "key":"condition","expectedValueRaw":serde_json::to_string(expected).unwrap()}},
        {"id":"exact-integer","predicate":{"kind":"state.equals","targetId":fixture::ACTOR,
            "key":"counter","expectedValueRaw":fixture::TICK.to_string()}}
    ]})
}

fn plan(world: &World, analysis: Json) -> consequences::Plan {
    consequences::decode(&json!({"expectedFingerprint":world.fingerprint,"operations":[
        {"type":"record.update","entityId":fixture::ACTOR,
         "patch":format!("kind: character\ninitial_state: {{condition: after, counter: {}}}\n",fixture::TICK)}
    ],"analysis":analysis}).to_string()).unwrap()
}

fn save(world: &World, draft: Draft, prior: Option<&str>, ready: bool) -> Json {
    let review = ready.then(|| {
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
            expected_sha256: prior.map(str::to_owned),
            draft,
            review,
        },
    )
    .unwrap()
}

fn proposed(world: &World, plan: consequences::Plan) -> ProposalPreviewRequest {
    let mut draft = fixture::draft(world, &fixture::id(1));
    let created = save(world, draft.clone(), None, false);
    draft.questions[0].answer = Some("The explicit plan and checks are ready to inspect.".into());
    draft.phase = "resolution-needed".into();
    let ready = save(world, draft.clone(), created["sourceSha256"].as_str(), true);
    draft.proposals.push(serde_json::from_value(json!({"id":PROPOSAL,"summary":"Change the explicit initial state.",
        "contextFingerprint":world.fingerprint,"questionAnswers":[{"questionId":draft.questions[0].id,"answer":draft.questions[0].answer}],
        "evidence":[],"resolver":"human","consequences":plan})).unwrap());
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

fn approved(request: &ProposalPreviewRequest, report: &Json) -> ApplyRequest {
    let mut input = json!(request);
    input["reviewSha256"] = report["reviewSha256"].clone();
    input["approved"] = true.into();
    action_outcomes::decode_apply(&input.to_string()).unwrap()
}

fn check_request(world: &World, checks: Json) -> semantics::CheckRequest {
    semantics::decode_check(
        &json!({"expectedFingerprint":world.fingerprint,"analysis":checks,"maxCharacters":100_000})
            .to_string(),
    )
    .unwrap()
}

fn uri(value: &Json) -> String {
    let encoded: String = value
        .to_string()
        .bytes()
        .map(|byte| format!("%{byte:02X}"))
        .collect();
    format!("/api/consequences/check?request={encoded}")
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

#[tokio::test]
async fn source_checks_share_native_service_cli_http_and_never_write() {
    let (_directory, world) = source_fixture();
    let input = check_request(&world, analysis("before", "required"));
    let before = fixture::source_image(&world);
    let native = semantics::check_request(&world, &input).unwrap();
    let service = Service::new(world.clone());
    let result = service
        .execute(Operation::ConsequencesCheck {
            request: input.clone(),
        })
        .unwrap();
    assert_eq!(result["result"], native);
    assert_eq!(
        request(service, "GET", &uri(&json!(input))).await,
        (StatusCode::OK, result.clone())
    );
    let path = world.root.parent().unwrap().join("checks.json");
    fs::write(&path, serde_json::to_vec(&input).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .args(["--compact", "--repo"])
        .arg(&world.root)
        .args(["consequences", "check", "--file"])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Json>(&output.stdout).unwrap(),
        result
    );
    assert_eq!(native["expectationsSatisfied"], true);
    assert_eq!(native["at"]["tick"], fixture::TICK.to_string());
    assert_eq!(native["writesPerformed"], false);
    assert_eq!(before, fixture::source_image(&world));
    assert!(!world.root.join("author").exists());
    assert!(!world.root.join(".wedl-rust").exists());
}

#[test]
fn required_failures_and_unknowns_block_reviewed_apply_without_mutation() {
    for unknown in [false, true] {
        let (_directory, world) = source_fixture();
        let mut checks = analysis("wrong", "required");
        if unknown {
            checks["expectations"][0]["predicate"] = json!({"kind":"knowledge.state",
            "knowledgeId":"know_00000000000000000000000099","state":"accepted"});
        }
        let input = proposed(&world, plan(&world, checks));
        let private = fs::read(fixture::path(&world, &input.action_id)).unwrap();
        let source = fixture::source_image(&world);
        let review = action_outcomes::review(&world, &input).unwrap();
        assert_eq!(review["consequences"]["validation"]["valid"], true);
        assert_eq!(review["canApply"], false);
        assert_eq!(
            review["consequences"]["semanticAnalysis"]["expectations"][0]["outcome"],
            if unknown { "unknown" } else { "fail" }
        );
        assert!(action_outcomes::apply(&world, &approved(&input, &review)).is_err());
        assert_eq!(source, fixture::source_image(&world));
        assert_eq!(
            private,
            fs::read(fixture::path(&world, &input.action_id)).unwrap()
        );
        assert_eq!(
            wedl::publication::status(&world.root).unwrap()["pending"],
            false
        );
    }
}

#[test]
fn advisory_failures_are_visible_and_do_not_replace_explicit_approval() {
    let (_directory, world) = source_fixture();
    let input = proposed(&world, plan(&world, analysis("wrong", "advisory")));
    let report = action_outcomes::review(&world, &input).unwrap();
    let semantic = &report["consequences"]["semanticAnalysis"];
    assert_eq!(semantic["expectationsSatisfied"], false);
    assert_eq!(semantic["applyAllowed"], true);
    assert_eq!(report["canApply"], true);
    let mut approval = approved(&input, &report);
    approval.approved = false;
    assert!(action_outcomes::apply(&world, &approval).is_err());
    approval.approved = true;
    assert_eq!(
        action_outcomes::apply(&world, &approval).unwrap()["writesPerformed"],
        true
    );
    let current = source::load_world(&world.root, None).unwrap();
    let stored = action_store::detail(&current, &input.action_id, None).unwrap();
    assert_eq!(
        stored["draft"]["proposals"][0]["consequences"]["analysis"]["policy"],
        "advisory"
    );
}

#[test]
fn passing_checks_apply_once_and_current_rechecks_do_not_rewrite_history() {
    let (_directory, world) = source_fixture();
    let input = proposed(&world, plan(&world, analysis("after", "required")));
    let report = action_outcomes::review(&world, &input).unwrap();
    assert_eq!(
        report["consequences"]["semanticAnalysis"]["expectationsSatisfied"],
        true
    );
    assert_eq!(report["canApply"], true);
    let approval = approved(&input, &report);
    let applied = action_outcomes::apply(&world, &approval).unwrap();
    let current = source::load_world(&world.root, None).unwrap();
    assert_eq!(current.fingerprint, applied["fingerprint"]);
    assert_eq!(
        action_outcomes::apply(&current, &approval).unwrap()["writesPerformed"],
        false
    );
    let private = fs::read(fixture::path(&current, &input.action_id)).unwrap();
    let verify = |current: &World| {
        let stored = action_store::detail(current, &input.action_id, None).unwrap();
        action_outcomes::verify(
            current,
            &VerifyRequest {
                expected_fingerprint: current.fingerprint.clone(),
                action_id: input.action_id.clone(),
                expected_sha256: stored["sourceSha256"].as_str().unwrap().into(),
                max_characters: 100_000,
            },
        )
        .unwrap()
    };
    let checked = verify(&current);
    assert_eq!(checked["verified"], true);
    assert_eq!(
        checked["semanticVerification"]["expectationsSatisfied"],
        true
    );
    let unrelated = fixture::change_source(&current);
    let checked = verify(&unrelated);
    assert_eq!(checked["verified"], false);
    assert_eq!(
        checked["semanticVerification"]["expectationsSatisfied"],
        true
    );
    change_actor(&unrelated, "later");
    let drifted = source::load_world(&world.root, None).unwrap();
    let checked = verify(&drifted);
    assert_eq!(checked["verified"], false);
    assert_eq!(
        checked["semanticVerification"]["expectationsSatisfied"],
        false
    );
    assert_eq!(
        private,
        fs::read(fixture::path(&drifted, &input.action_id)).unwrap()
    );
}

#[test]
fn semantic_selection_is_immutable_and_changes_complete_plan_bindings() {
    let (_directory, world) = source_fixture();
    let first = plan(&world, analysis("after", "required"));
    let mut changed = first.clone();
    changed.analysis =
        Some(semantics::decode_analysis(&analysis("wrong", "advisory").to_string()).unwrap());
    let a = consequences::preview(&world, &first, 100_000).unwrap();
    let b = consequences::preview(&world, &changed, 100_000).unwrap();
    assert_eq!(a["candidateFingerprint"], b["candidateFingerprint"]);
    assert_ne!(a["planSha256"], b["planSha256"]);
    assert_ne!(a["previewSha256"], b["previewSha256"]);
    let input = proposed(&world, first);
    let stored = action_store::detail(&world, &input.action_id, None).unwrap();
    let mut draft = action_drafts::decode(&stored["draft"].to_string()).unwrap();
    draft.proposals[0].consequences = changed;
    assert!(
        action_store::save(
            &world,
            &SaveRequest {
                expected_fingerprint: world.fingerprint.clone(),
                expected_sha256: Some(input.expected_sha256),
                draft,
                review: None
            }
        )
        .is_err()
    );
}

#[test]
fn invalid_candidates_remain_reviewable_without_semantic_folding() {
    let (_directory, world) = source_fixture();
    let mut plan = plan(&world, analysis("after", "required"));
    plan.operations = vec![consequences::Operation::Update {
        entity_id: fixture::ACTOR.into(),
        patch: "kind: character\ninitial_state: {undeclared: true}\n".into(),
    }];
    let report = consequences::preview(&world, &plan, 100_000).unwrap();
    assert_eq!(report["validation"]["valid"], false);
    assert_eq!(report["semanticAnalysis"]["outcome"], "unavailable");
    assert_eq!(report["semanticAnalysis"]["applyAllowed"], false);
    assert!(report["semanticAnalysis"].get("delta").is_none());
}

#[tokio::test]
async fn closed_read_transport_refuses_stale_duplicate_oversized_and_write_requests() {
    let (_directory, world) = source_fixture();
    let service = Service::new(world.clone());
    let input = json!(check_request(&world, analysis("before", "required")));
    let before = fixture::source_image(&world);
    assert!(http::operation("consequences/check", Some("request={}&request={}")).is_err());
    assert!(http::operation("consequences/check", Some("request={}&unexpected=1")).is_err());
    assert!(
        http::operation(
            "consequences/check",
            Some(&format!("request={}", "x".repeat(16385)))
        )
        .is_err()
    );
    let mut stale = input.clone();
    stale["expectedFingerprint"] = "0".repeat(64).into();
    assert_eq!(
        request(service.clone(), "GET", &uri(&stale)).await.0,
        StatusCode::CONFLICT
    );
    let mut small = input.clone();
    small["maxCharacters"] = 256.into();
    assert_eq!(
        request(service.clone(), "GET", &uri(&small)).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(service.clone(), "POST", &uri(&input)).await.0,
        StatusCode::METHOD_NOT_ALLOWED
    );
    for bad in [
        "null",
        "{\"at\":{},\"policy\":\"required\",\"policy\":\"advisory\",\"expectations\":[]}",
    ] {
        assert!(semantics::decode_analysis(bad).is_err());
    }
    let mut invalid_plan = json!(plan(&world, analysis("after", "required")));
    invalid_plan["analysis"] = Json::Null;
    assert!(consequences::decode(&invalid_plan.to_string()).is_err());
    assert_eq!(before, fixture::source_image(&world));
}
