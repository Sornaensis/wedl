//! Complete private proposals share one pure candidate across native and transports.
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
    action_store::{self, ProposalPreviewRequest},
    consequences, http,
    model::World,
    service::{Operation, Service},
    source,
    writing_frames::FamilySelection,
};

const NEW: &str = "char_00000000000000000000000099";
fn proposal_id(n: usize) -> String {
    format!("proposal_{n:026}")
}
fn plan(world: &World) -> consequences::Plan {
    consequences::decode(&json!({"expectedFingerprint":world.fingerprint,"operations":[
        {"type":"record.update","entityId":fixture::SCENE,"patch":format!(
            "kind: scene\r\n# Keep the exact numeric token in this raw proposal.\r\ntime:\r\n  start: {{timeline: main, tick: {}, order: 0}}\r\n  current: {{timeline: main, tick: {}, order: 1}}\r\n  end: {{timeline: main, tick: {}, order: 0}}\r\nbody: \"Candidate needle 🌲\\r\\nTrailing space  \"\r\n",fixture::TICK-1,fixture::TICK,fixture::TICK+1)},
        {"type":"record.create","draft":format!("kind: character\r\nid: {NEW}\r\ntitle: Private candidate\r\ninitial_state: {{}}\r\nbody: \"Never published candidate\\r\\n\"\r\n")}
    ]}).to_string()).unwrap()
}
fn confirmation(world: &World, draft: &Draft) -> ReviewConfirmation {
    let input = ReviewRequest {
        draft: draft.clone(),
        frame: fixture::frame(world, "author"),
        briefs: vec![],
        families: FamilySelection::default(),
        max_characters: 100_000,
    };
    let reviewed = action_drafts::review(world, &input).unwrap();
    ReviewConfirmation {
        frame: input.frame,
        briefs: input.briefs,
        families: input.families,
        max_characters: input.max_characters,
        review_sha256: reviewed["reviewSha256"].as_str().unwrap().into(),
        sufficient: true,
    }
}
fn save(world: &World, draft: Draft, sha: Option<&str>, confirmed: bool) -> Json {
    let review = confirmed.then(|| confirmation(world, &draft));
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
fn append(world: &World, draft: &mut Draft, n: usize, supersedes: Option<String>) {
    let value = json!({"id":proposal_id(n),"summary":format!("Human alternative {n}"),"contextFingerprint":world.fingerprint,
        "questionAnswers":draft.questions.iter().filter_map(|q|q.answer.as_ref().map(|answer|json!({"questionId":q.id,"answer":answer}))).collect::<Vec<_>>(),
        "evidence":[],"resolver":"human","consequences":plan(world)});
    let mut proposal: action_drafts::Proposal = serde_json::from_value(value).unwrap();
    proposal.supersedes = supersedes;
    draft.proposals.push(proposal);
    draft.phase = "proposed".into();
}
fn stored(world: &World) -> (Draft, String) {
    let mut draft = fixture::draft(world, &fixture::id(1));
    let first = save(world, draft.clone(), None, false);
    draft.phase = "resolution-needed".into();
    draft.questions[0].answer = Some(String::new());
    let ready = save(world, draft.clone(), first["sourceSha256"].as_str(), true);
    append(world, &mut draft, 1, None);
    let proposed = save(world, draft.clone(), ready["sourceSha256"].as_str(), true);
    (draft, proposed["sourceSha256"].as_str().unwrap().into())
}
fn preview_request(world: &World, draft: &Draft, sha: &str) -> ProposalPreviewRequest {
    ProposalPreviewRequest {
        expected_fingerprint: world.fingerprint.clone(),
        action_id: draft.id.clone(),
        expected_sha256: sha.into(),
        proposal_id: draft.proposals.last().unwrap().id.clone(),
        max_characters: 100_000,
    }
}
fn uri(input: &ProposalPreviewRequest) -> String {
    format!(
        "/api/action-drafts/preview-proposal?expectedFingerprint={}&id={}&sha256={}&proposalId={}&maxCharacters={}",
        input.expected_fingerprint,
        input.action_id,
        input.expected_sha256,
        input.proposal_id,
        input.max_characters
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
    (status, serde_json::from_slice(&bytes).unwrap())
}
fn cli(world: &World, args: &[&str]) -> (i32, Json) {
    let output = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .arg("--compact")
        .arg("--repo")
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
        serde_json::from_slice(bytes).unwrap_or_else(|_| {
            panic!("CLI returned non-JSON: {}", String::from_utf8_lossy(bytes))
        }),
    )
}
fn file(world: &World, name: &str, value: &Json) -> String {
    let path = world.root.parent().unwrap().join(name);
    fs::write(&path, serde_json::to_vec(value).unwrap()).unwrap();
    path.to_str().unwrap().into()
}
fn all_files(root: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(all_files(&path));
        } else {
            files.push((path.clone(), fs::read(&path).unwrap()));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

#[tokio::test]
async fn saved_proposal_preview_has_exact_native_service_cli_http_parity_and_no_writes() {
    let (_directory, world) = fixture::fixture();
    let source_before = fixture::source_image(&world);
    let (draft, sha) = stored(&world);
    let private_before = fs::read(fixture::path(&world, &draft.id)).unwrap();
    let all_before = all_files(&world.root);
    let input = preview_request(&world, &draft, &sha);
    let service = Service::new(world.clone());
    let result = service
        .execute(Operation::ActionProposalPreview {
            request: input.clone(),
        })
        .unwrap();
    assert_eq!(
        result["result"],
        action_store::proposal_preview(&world, &input).unwrap()
    );
    assert_eq!(
        request(service.clone(), "GET", &uri(&input)).await,
        (StatusCode::OK, result.clone())
    );
    let path = file(&world, "proposal-preview.json", &json!(input));
    assert_eq!(
        cli(
            &world,
            &["action-drafts", "preview-proposal", "--file", &path]
        ),
        (0, result.clone())
    );
    let candidate = &result["result"]["consequences"];
    assert_eq!(candidate["validation"]["valid"], true);
    assert_eq!(candidate["changedFiles"], 2);
    assert_eq!(candidate["canApply"], false);
    assert_eq!(result["result"]["writesPerformed"], false);
    assert!(
        candidate["files"][0]["afterSource"]
            .as_str()
            .unwrap()
            .contains("9007199254740993")
    );
    assert!(
        candidate["files"][0]["afterSource"]
            .as_str()
            .unwrap()
            .ends_with("Candidate needle 🌲\r\nTrailing space  ")
    );
    assert!(
        candidate["files"][0]["diff"]
            .as_str()
            .unwrap()
            .contains("Candidate needle")
    );
    let detail = action_store::detail(&world, &draft.id, Some(&sha)).unwrap();
    assert_eq!(detail["effectivePhase"], "proposed");
    assert_eq!(detail["activeProposalIds"], json!([proposal_id(1)]));
    assert_eq!(
        detail["draft"]["proposals"][0]["consequences"],
        json!(plan(&world))
    );
    assert_eq!(
        detail["draft"]["horizon"]["tick"],
        fixture::TICK.to_string()
    );
    assert_eq!(fixture::source_image(&world), source_before);
    assert_eq!(
        fs::read(fixture::path(&world, &draft.id)).unwrap(),
        private_before
    );
    assert_eq!(
        source::load_world(&world.root, None).unwrap().records.len(),
        world.records.len()
    );
    assert_eq!(all_files(&world.root), all_before);
}

#[tokio::test]
async fn standalone_complete_plan_is_cli_only_and_keeps_raw_yaml_strings() {
    let (_directory, world) = fixture::fixture();
    let source_before = fixture::source_image(&world);
    let plan = plan(&world);
    let service = Service::new(world.clone());
    let expected = service
        .execute(Operation::ConsequencesPreview {
            plan: plan.clone(),
            max_characters: 1_000_000,
        })
        .unwrap();
    assert_eq!(
        expected["result"],
        consequences::preview(&world, &plan, 1_000_000).unwrap()
    );
    let path = file(&world, "exact-plan.json", &json!(plan));
    assert_eq!(
        cli(
            &world,
            &[
                "consequences",
                "preview",
                "--file",
                &path,
                "--max-characters",
                "1000000"
            ]
        ),
        (0, expected)
    );
    assert_eq!(
        consequences::decode(&fs::read_to_string(path).unwrap()).unwrap(),
        plan
    );
    for method in ["GET", "POST"] {
        assert_ne!(
            request(service.clone(), method, "/api/consequences/preview")
                .await
                .0,
            StatusCode::OK
        );
    }
    assert_eq!(fixture::source_image(&world), source_before);
    assert!(!world.root.join("author").exists());
    assert!(!world.root.join(".wedl-rust").exists());
}

#[tokio::test]
async fn closed_preview_routes_reject_duplicates_unknowns_writes_and_bad_bounds() {
    let (_directory, world) = fixture::fixture();
    let (draft, sha) = stored(&world);
    let service = Service::new(world.clone());
    let input = preview_request(&world, &draft, &sha);
    let good = uri(&input);
    for suffix in [
        "&id=bad",
        "&sha256=bad",
        "&proposalId=bad",
        "&expectedFingerprint=bad",
        "&maxCharacters=1",
        "&revision=HEAD",
        "&plan=%7B%7D",
        "&apply=true",
        "&request=%7B%7D",
    ] {
        let (status, error) = request(service.clone(), "GET", &(good.clone() + suffix)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{suffix}: {error}");
        assert!(error.get("result").is_none());
    }
    for limit in ["255", "1000001", "-1", "1.5", "1e5"] {
        assert_eq!(
            request(
                service.clone(),
                "GET",
                &good.replace("maxCharacters=100000", &format!("maxCharacters={limit}"))
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    for method in ["POST", "PUT", "DELETE"] {
        assert_eq!(
            request(service.clone(), method, &good).await.0,
            StatusCode::METHOD_NOT_ALLOWED
        );
    }
    for missing in ["expectedFingerprint", "id", "sha256", "proposalId"] {
        let shortened = good
            .split('&')
            .filter(|p| {
                !p.split('?')
                    .next_back()
                    .unwrap()
                    .starts_with(&format!("{missing}="))
            })
            .collect::<Vec<_>>()
            .join("&");
        let shortened = if shortened.starts_with("/api/") {
            shortened
        } else {
            format!("/api/action-drafts/preview-proposal?{shortened}")
        };
        assert_eq!(
            request(service.clone(), "GET", &shortened).await.0,
            StatusCode::BAD_REQUEST,
            "{missing}"
        );
    }
    let raw = serde_json::to_string(&input).unwrap();
    for bad in [
        raw.replacen("\"actionId\":", "\"actionId\":null,\"actionId\":", 1),
        raw.replacen("{", "{\"unknown\":true,", 1),
    ] {
        let path = world.root.parent().unwrap().join("bad-preview.json");
        fs::write(&path, bad).unwrap();
        assert_ne!(
            cli(
                &world,
                &[
                    "action-drafts",
                    "preview-proposal",
                    "--file",
                    path.to_str().unwrap()
                ]
            )
            .0,
            0
        );
    }
}

#[tokio::test]
async fn private_hash_source_pin_active_proposal_and_phase_are_required() {
    let (_directory, world) = fixture::fixture();
    let (mut draft, sha) = stored(&world);
    let service = Service::new(world.clone());
    let input = preview_request(&world, &draft, &sha);
    for changed in [
        ProposalPreviewRequest {
            expected_sha256: "a".repeat(64),
            ..input.clone()
        },
        ProposalPreviewRequest {
            expected_fingerprint: "b".repeat(64),
            ..input.clone()
        },
        ProposalPreviewRequest {
            proposal_id: proposal_id(99),
            ..input.clone()
        },
    ] {
        let (status, error) = request(service.clone(), "GET", &uri(&changed)).await;
        assert_ne!(status, StatusCode::OK);
        assert!(!error.to_string().contains("Candidate needle"));
    }
    draft.phase = "stale".into();
    let stale = save(&world, draft.clone(), Some(&sha), false);
    let first = ProposalPreviewRequest {
        expected_sha256: stale["sourceSha256"].as_str().unwrap().into(),
        ..input.clone()
    };
    assert_ne!(
        request(service.clone(), "GET", &uri(&first)).await.0,
        StatusCode::OK
    );
    append(&world, &mut draft, 2, Some(proposal_id(1)));
    let second = save(&world, draft.clone(), stale["sourceSha256"].as_str(), true);
    let current = preview_request(&world, &draft, second["sourceSha256"].as_str().unwrap());
    assert_eq!(
        request(service.clone(), "GET", &uri(&current)).await.0,
        StatusCode::OK
    );
    assert_ne!(
        request(
            service.clone(),
            "GET",
            &uri(&ProposalPreviewRequest {
                proposal_id: proposal_id(1),
                ..current.clone()
            })
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        action_store::detail(&world, &draft.id, None).unwrap()["activeProposalIds"],
        json!([proposal_id(2)])
    );
    let changed = fixture::change_source(&world);
    assert_eq!(
        request(service, "GET", &uri(&current)).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        action_store::detail(&changed, &draft.id, None).unwrap()["effectivePhase"],
        "stale"
    );
    assert_ne!(
        request(
            Service::new(changed.clone()),
            "GET",
            &uri(&ProposalPreviewRequest {
                expected_fingerprint: changed.fingerprint.clone(),
                ..current
            })
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn complete_preview_budgets_fail_without_partial_data_and_no_apply_route_exists() {
    let (_directory, world) = fixture::fixture();
    let (draft, sha) = stored(&world);
    let private_before = fs::read(fixture::path(&world, &draft.id)).unwrap();
    let story = fixture::source_image(&world);
    let service = Service::new(world.clone());
    let input = ProposalPreviewRequest {
        max_characters: 256,
        ..preview_request(&world, &draft, &sha)
    };
    let (status, error) = request(service.clone(), "GET", &uri(&input)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(error.get("result").is_none());
    assert!(!error.to_string().contains("beforeSource"));
    let path = file(&world, "small-plan.json", &json!(plan(&world)));
    assert_ne!(
        cli(
            &world,
            &[
                "consequences",
                "preview",
                "--file",
                &path,
                "--max-characters",
                "256"
            ]
        )
        .0,
        0
    );
    for route in ["apply", "approve", "save", "delete"] {
        assert_ne!(
            request(
                service.clone(),
                "GET",
                &format!("/api/action-drafts/{route}")
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    assert_eq!(fixture::source_image(&world), story);
    assert_eq!(
        fs::read(fixture::path(&world, &draft.id)).unwrap(),
        private_before
    );
}

#[tokio::test]
async fn private_candidate_history_never_enters_canonical_reads_and_revision_is_rejected() {
    let (_directory, world) = fixture::fixture();
    let service = Service::new(world.clone());
    let routes = [
        "/api/entities".to_string(),
        "/api/search?q=candidate".into(),
        format!(
            "/api/character/context?character={}&tick={}&order=1&maxCharacters=100000",
            fixture::ACTOR,
            fixture::TICK
        ),
    ];
    let mut before = Vec::new();
    for route in &routes {
        before.push(request(service.clone(), "GET", route).await);
    }
    let (draft, sha) = stored(&world);
    let input = preview_request(&world, &draft, &sha);
    assert_eq!(
        request(service.clone(), "GET", &uri(&input)).await.0,
        StatusCode::OK
    );
    for (route, expected) in routes.iter().zip(before) {
        let actual = request(service.clone(), "GET", route).await;
        assert_eq!(actual, expected);
        assert!(!actual.1.to_string().contains(&proposal_id(1)));
        assert!(!actual.1.to_string().contains("Candidate needle"));
    }
    let mut historical = world.clone();
    historical.revision = Some("a".repeat(40));
    assert!(
        Service::new(historical)
            .execute(Operation::ActionProposalPreview { request: input })
            .is_err()
    );
}
