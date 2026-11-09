//! Explicit possibility discovery preserves canonical results and shares the
//! exact bounded author workflow across native, service, CLI and HTTP callers.
#[path = "support/actions.rs"]
mod support;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value as Json, json};
use std::{
    fs,
    io::Write,
    process::{Command, Output, Stdio},
};
use tower::ServiceExt;
use wedl::{
    compiler::{self, AuthorSearchSelection},
    http, hypotheses,
    model::{StoryTime, World},
    retrieval::Mode,
    service::{Operation, Service, TimeInput},
    source, validation,
};

const HYP: &str = "hyp_00000000000000000000000001";
const NEW: &str = "hyp_00000000000000000000000002";
const THREAD: &str = "thread_00000000000000000000000001";
const MODES: [Mode; 3] = [Mode::Lexical, Mode::Semantic, Mode::Hybrid];

fn fixture() -> (tempfile::TempDir, World) {
    let (directory, world) = support::fixture();
    let owner = world.world_record().unwrap();
    let mut fields = owner.frontmatter.clone();
    fields["threads"] = serde_yaml::to_value(json!([{"id":THREAD,"label":"Harbor"}])).unwrap();
    fs::write(
        world.root.join(&owner.source_path),
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&fields).unwrap(),
            owner.body
        ),
    )
    .unwrap();
    let place = &world.records[support::PLACE];
    let mut fields = place.frontmatter.clone();
    fields["threads"] = serde_yaml::to_value([THREAD]).unwrap();
    fs::write(
        world.root.join(&place.source_path),
        format!(
            "---\n{}---\nA canonical harbor landing.\n",
            serde_yaml::to_string(&fields).unwrap()
        ),
    )
    .unwrap();
    let hypothesis = json!({"schema":"wedl/v1","kind":"hypothesis","id":HYP,
        "title":"Harbor possibility 🌲","status":"open","domain":"author.possibilities",
        "statement":"The harbor may be closed.","subjects":[support::PLACE],
        "alternatives":["The harbor stays open.","The harbor closes."],
        "context":"A future possibility, never canonical evidence.",
        "placement":{"context":"A future possibility, never canonical evidence.","timeline":"main"}});
    fs::write(
        world.root.join(format!("story/{HYP}.md")),
        format!(
            "---\n{}---\nPrivate hypothesis body.\r\n",
            serde_yaml::to_string(&hypothesis).unwrap()
        ),
    )
    .unwrap();
    let world = source::load_world(&world.root, None).unwrap();
    let report = validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    (directory, world)
}

fn request(world: &World, action: Json) -> hypotheses::Request {
    hypotheses::decode(
        &json!({"expectedFingerprint":world.fingerprint,"action":action}).to_string(),
    )
    .unwrap()
}
fn create_action() -> Json {
    json!({"type":"create","id":NEW,"title":"Another harbor possibility",
        "statement":"The ferry may arrive.","subjects":[support::ACTOR,support::PLACE],
        "alternatives":["Arrival","No arrival"],"context":"Before the decision",
        "placement":{"timeline":"main","sceneId":support::SCENE,"locationId":support::PLACE}})
}
fn envelope(world: &World, result: Json) -> Json {
    json!({"protocol":wedl::CONTRACT,"fingerprint":world.fingerprint,"revision":world.revision,"result":result})
}
fn wire(value: &Json) -> Json {
    serde_json::from_slice(&serde_json::to_vec(value).unwrap()).unwrap()
}
fn encode(raw: &str) -> String {
    raw.bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}
async fn get(service: Service, path: &str) -> (StatusCode, Json) {
    let response = http::router(service)
        .oneshot(
            Request::builder()
                .uri(format!("/api/{path}"))
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
fn cli(world: &World, args: &[&str], input: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_wedl-rs"));
    command
        .arg("--repo")
        .arg(&world.root)
        .arg("--compact")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut process = command.spawn().unwrap();
    if let Some(input) = input {
        process
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }
    drop(process.stdin.take());
    process.wait_with_output().unwrap()
}
fn output(output: Output) -> Json {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[tokio::test]
async fn catalogue_and_detail_are_identical_complete_author_reads() {
    let (_directory, world) = fixture();
    let before = support::source_image(&world);
    let service = Service::new(world.clone());
    let native = hypotheses::list(&world, Some("open"), Some("HARBOR"), 100, 100000).unwrap();
    let expected = envelope(&world, native);
    assert_eq!(
        service
            .execute(Operation::HypothesisList {
                status: Some("open".into()),
                text: Some("HARBOR".into()),
                max_items: 100,
                max_characters: 100000
            })
            .unwrap(),
        expected
    );
    let (status, result) = get(service.clone(), "hypotheses?status=open&text=HARBOR").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result, wire(&expected));
    assert_eq!(
        output(cli(
            &world,
            &["hypotheses", "list", "--status", "open", "--text", "HARBOR"],
            None
        )),
        wire(&expected)
    );
    let expected = envelope(&world, hypotheses::detail(&world, HYP, 100000).unwrap());
    assert_eq!(
        service
            .execute(Operation::HypothesisDetail {
                hypothesis_id: HYP.into(),
                max_characters: 100000
            })
            .unwrap(),
        expected
    );
    let (status, result) = get(service, &format!("hypothesis?hypothesisId={HYP}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result, wire(&expected));
    assert_eq!(
        output(cli(&world, &["hypotheses", "detail", HYP], None)),
        wire(&expected)
    );
    assert_eq!(support::source_image(&world), before);
    assert!(!world.root.join(".wedl-rust").exists());
}

#[tokio::test]
async fn all_three_previews_export_the_exact_plan_without_source_changes() {
    let (_directory, world) = fixture();
    let before = support::source_image(&world);
    let service = Service::new(world.clone());
    for action in [
        create_action(),
        json!({"type":"adopt","hypothesisId":HYP,
        "canonicalRecords":[support::PLACE],"note":"Existing reference only"}),
        json!({"type":"reject","hypothesisId":HYP,"note":"Discard this possibility"}),
    ] {
        let request = request(&world, action);
        let encoded = serde_json::to_string(&request).unwrap();
        let native = hypotheses::preview(&world, &request, 100000).unwrap();
        assert_eq!(
            native["plan"],
            serde_json::to_value(hypotheses::plan(&world, &request).unwrap()).unwrap()
        );
        assert_eq!(native["writesPerformed"], false);
        assert_eq!(native["canApply"], false);
        let expected = envelope(&world, native);
        assert_eq!(
            service
                .execute(Operation::HypothesisPreview {
                    request,
                    max_characters: 100000
                })
                .unwrap(),
            expected
        );
        let (status, result) = get(
            service.clone(),
            &format!("hypotheses/preview?request={}", encode(&encoded)),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(result, wire(&expected));
        assert_eq!(
            output(cli(
                &world,
                &["hypotheses", "preview", "--file", "-"],
                Some(&encoded)
            )),
            wire(&expected)
        );
    }
    assert_eq!(support::source_image(&world), before);
    assert!(!world.root.join(".wedl-rust").exists());
}

#[test]
fn hypothesis_search_preserves_exact_canonical_results_in_every_mode_and_horizon() {
    let (_directory, world) = fixture();
    for mode in MODES {
        for at in [
            None,
            Some(StoryTime::new("main", support::TICK, 1).unwrap()),
        ] {
            let selection = AuthorSearchSelection {
                at,
                thread_ids: None,
            };
            let canonical =
                compiler::search_with_selection(&world, "harbor", 20, None, mode, &selection)
                    .unwrap();
            let combined =
                compiler::search_with_hypotheses(&world, "harbor", 20, None, mode, &selection)
                    .unwrap();
            assert_eq!(combined["matches"], canonical);
            assert_eq!(combined["matches"]["hypothesesIncluded"], false);
            assert_eq!(combined["hypothesisMatches"]["scope"], "author-all-time");
            assert_eq!(combined["hypothesisMatches"]["included"], true);
            assert_eq!(
                combined["hypothesisMatches"]["items"],
                serde_json::to_value(hypotheses::source_search(&world, "harbor", 20).unwrap())
                    .unwrap()
            );
            assert_eq!(combined["hypothesisMatches"]["items"][0]["entityId"], HYP);
        }
    }
}

#[tokio::test]
async fn hypothesis_search_matches_native_service_cli_http_for_all_modes() {
    let (_directory, world) = fixture();
    let service = Service::new(world.clone());
    let tick = support::TICK.to_string();
    for mode in MODES {
        let at = StoryTime::new("main", support::TICK, 1).unwrap();
        let native = compiler::search_with_hypotheses(
            &world,
            "harbor",
            3,
            None,
            mode,
            &AuthorSearchSelection {
                at: Some(at.clone()),
                thread_ids: None,
            },
        )
        .unwrap();
        let expected = envelope(&world, native);
        assert_eq!(
            service
                .execute(Operation::SearchWithHypotheses {
                    query: "harbor".into(),
                    limit: 3,
                    kind: None,
                    mode,
                    at: Some(TimeInput {
                        tick: at.tick,
                        order: at.order,
                        timeline: Some(at.timeline)
                    }),
                    thread_ids: None
                })
                .unwrap(),
            expected
        );
        let (status,result) = get(service.clone(),&format!("search?q=harbor&limit=3&mode={}&tick={tick}&order=1&timeline=main&includeHypotheses=true",mode.as_str())).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(result, wire(&expected));
        assert_eq!(
            output(cli(
                &world,
                &[
                    "search",
                    "harbor",
                    "--mode",
                    mode.as_str(),
                    "--limit",
                    "3",
                    "--tick",
                    &tick,
                    "--order",
                    "1",
                    "--timeline",
                    "main",
                    "--include-hypotheses"
                ],
                None
            )),
            wire(&expected)
        );
    }
}

#[test]
fn thread_and_kind_filters_suppress_only_the_independent_hypothesis_lane() {
    let (_directory, world) = fixture();
    for mode in MODES {
        for (kind, thread_ids, reason) in [
            (None, Some(vec![THREAD.into()]), "thread-selection"),
            (Some("location"), None, "kind-filter"),
        ] {
            let selection = AuthorSearchSelection {
                at: None,
                thread_ids,
            };
            let canonical =
                compiler::search_with_selection(&world, "harbor", 20, kind, mode, &selection)
                    .unwrap();
            let combined =
                compiler::search_with_hypotheses(&world, "harbor", 20, kind, mode, &selection)
                    .unwrap();
            assert_eq!(combined["matches"], canonical);
            assert_eq!(combined["hypothesisMatches"]["included"], false);
            assert_eq!(combined["hypothesisMatches"]["suppressedReason"], reason);
            assert_eq!(combined["hypothesisMatches"]["items"], json!([]));
        }
        let combined = compiler::search_with_hypotheses(
            &world,
            "harbor",
            1,
            Some("hypothesis"),
            mode,
            &AuthorSearchSelection::default(),
        )
        .unwrap();
        assert_eq!(combined["matches"]["items"], json!([]));
        assert_eq!(
            combined["hypothesisMatches"]["items"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}

#[tokio::test]
async fn false_opt_in_preserves_default_envelope_and_character_routes_reject_it() {
    let (_directory, world) = fixture();
    let service = Service::new(world.clone());
    let (status, original) = get(service.clone(), "search?q=harbor").await;
    assert_eq!(status, StatusCode::OK);
    let (status, explicit) = get(service, "search?q=harbor&includeHypotheses=false").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(explicit, original);
    assert!(original["result"].get("hypothesisMatches").is_none());
    assert!(
        http::operation(
            "character/search",
            Some(&format!(
                "character={}&q=harbor&tick=1&includeHypotheses=true",
                support::ACTOR
            ))
        )
        .is_err()
    );
    assert!(
        !cli(
            &world,
            &[
                "character",
                "search",
                support::ACTOR,
                "harbor",
                "--tick",
                "1",
                "--include-hypotheses"
            ],
            None
        )
        .status
        .success()
    );
}

#[test]
fn closed_hypothesis_routes_reject_unknown_duplicate_null_and_malformed_controls() {
    for (path, query) in [
        ("hypotheses", "tick=1"),
        ("hypotheses", "status=open&status=rejected"),
        ("hypotheses", "maxItems=0"),
        ("hypotheses", "maxItems=1001"),
        ("hypotheses", "maxCharacters=255"),
        ("hypothesis", "hypothesisId=x&extra=1"),
        ("hypothesis", "maxCharacters=100000"),
        ("hypotheses/preview", "request=null"),
        ("hypotheses/preview", "request=%7B%7D&maxItems=2"),
        ("search", "q=harbor&includeHypotheses=1"),
        (
            "search",
            "q=harbor&includeHypotheses=true&includeHypotheses=false",
        ),
    ] {
        assert!(
            http::operation(path, Some(query)).is_err(),
            "{path}?{query}"
        );
    }
}

#[tokio::test]
async fn stale_requests_and_incomplete_budgets_fail_across_transports() {
    let (_directory, world) = fixture();
    let service = Service::new(world.clone());
    let mut request = request(&world, create_action());
    request.expected_fingerprint = "0".repeat(64);
    let encoded = serde_json::to_string(&request).unwrap();
    assert!(
        service
            .execute(Operation::HypothesisPreview {
                request,
                max_characters: 100000
            })
            .is_err()
    );
    let (status, _) = get(
        service.clone(),
        &format!("hypotheses/preview?request={}", encode(&encoded)),
    )
    .await;
    assert!(!status.is_success());
    assert!(
        !cli(
            &world,
            &["hypotheses", "preview", "--file", "-"],
            Some(&encoded)
        )
        .status
        .success()
    );
    for path in [
        "hypotheses?maxCharacters=256".to_owned(),
        format!("hypothesis?hypothesisId={HYP}&maxCharacters=256"),
    ] {
        let (status, _) = get(service.clone(), &path).await;
        assert!(!status.is_success());
    }
    assert!(
        !cli(
            &world,
            &["hypotheses", "list", "--max-characters", "256"],
            None
        )
        .status
        .success()
    );
}
