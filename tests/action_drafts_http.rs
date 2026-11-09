//! Private action transports share exact source pins without publishing story facts.
#[path = "support/actions.rs"]
mod fixture;
#[path = "support/family_character.rs"]
mod magic_fixture;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value as Json, json};
use std::{fs, process::Command};
use tower::ServiceExt;
use wedl::{
    action_drafts, action_store, http,
    model::World,
    service::{Operation, Service},
    source,
    writing_frames::FamilySelection,
};

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}
fn review(world: &World, lens: &str) -> action_drafts::ReviewRequest {
    let mut frame = fixture::frame(world, lens);
    frame.at.tick -= 1;
    action_drafts::ReviewRequest {
        draft: fixture::draft(world, &fixture::id(1)),
        frame,
        briefs: vec![],
        families: FamilySelection::default(),
        max_characters: 100_000,
    }
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
fn cli(world: &World, arguments: &[&str]) -> (i32, Json) {
    let output = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .arg("--compact")
        .arg("--repo")
        .arg(&world.root)
        .args(arguments)
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
            panic!(
                "CLI did not return JSON: {}",
                String::from_utf8_lossy(bytes)
            )
        }),
    )
}
fn file(world: &World, name: &str, value: &Json) -> String {
    let path = world.root.parent().unwrap().join(name);
    fs::write(&path, serde_json::to_vec(value).unwrap()).unwrap();
    path.to_str().unwrap().into()
}

#[tokio::test]
async fn cli_private_save_is_reviewable_and_read_transports_are_identical() {
    let (_directory, world) = fixture::fixture();
    let story = fixture::source_image(&world);
    let service = Service::new(world.clone());
    let draft = fixture::draft(&world, &fixture::id(1));
    let save = fixture::request(&world, draft.clone(), None);
    let preview = service
        .execute(Operation::ActionDraftPreview {
            request: save.clone(),
        })
        .unwrap();
    assert_eq!(
        preview["result"],
        action_store::preview(&world, &save).unwrap()
    );
    assert_eq!(preview["result"]["writesPerformed"], false);
    assert!(!world.root.join("author").exists());
    assert!(!world.root.join(".wedl-rust").exists());
    let path = file(&world, "save.json", &serde_json::to_value(&save).unwrap());
    assert_eq!(
        cli(&world, &["action-drafts", "preview", "--file", &path]),
        (0, preview.clone())
    );
    let (code, saved) = cli(&world, &["action-drafts", "save", "--file", &path]);
    assert_eq!(code, 0, "{saved}");
    assert_eq!(
        saved["result"]["afterSha256"],
        preview["result"]["afterSha256"]
    );
    assert_eq!(saved["result"]["writesPerformed"], true);
    let sha = saved["result"]["afterSha256"].as_str().unwrap();
    let list = service.execute(Operation::ActionDraftList).unwrap();
    assert_eq!(list["result"], action_store::list(&world).unwrap());
    assert_eq!(
        request(service.clone(), "GET", "/api/action-drafts/list").await,
        (StatusCode::OK, list.clone())
    );
    assert_eq!(cli(&world, &["action-drafts", "list"]), (0, list));
    let detail = service
        .execute(Operation::ActionDraftDetail {
            id: draft.id.clone(),
            expected_sha256: Some(sha.into()),
        })
        .unwrap();
    assert_eq!(
        detail["result"],
        action_store::detail(&world, &draft.id, Some(sha)).unwrap()
    );
    assert_eq!(
        detail["result"]["draft"]["horizon"]["tick"],
        fixture::TICK.to_string()
    );
    assert_eq!(
        detail["result"]["draft"]["bodyMarkdown"],
        draft.body_markdown
    );
    assert_eq!(detail["result"]["draft"]["proposals"], json!([]));
    assert_eq!(detail["result"]["activeProposalIds"], json!([]));
    assert_eq!(detail["result"]["effectivePhase"], "draft");
    assert_eq!(
        detail["result"]["source"],
        String::from_utf8(action_drafts::serialize(&draft).unwrap()).unwrap()
    );
    assert_eq!(
        request(
            service,
            "GET",
            &format!("/api/action-drafts/detail?id={}&sha256={sha}", draft.id)
        )
        .await,
        (StatusCode::OK, detail.clone())
    );
    assert_eq!(
        cli(
            &world,
            &["action-drafts", "detail", &draft.id, "--sha256", sha]
        ),
        (0, detail)
    );
    assert_eq!(fixture::source_image(&world), story);
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
}

#[tokio::test]
async fn all_review_lenses_use_saved_exact_horizon_and_do_not_admit_private_prose() {
    let (_directory, world) = fixture::fixture();
    let before = fixture::source_image(&world);
    let service = Service::new(world.clone());
    let mut character_half = None;
    for lens in ["author", "character", "dramatic-irony"] {
        let input = review(&world, lens);
        let expected = service
            .execute(Operation::ActionDraftReview {
                request: input.clone(),
            })
            .unwrap();
        assert_eq!(
            expected["result"],
            action_drafts::review(&world, &input).unwrap()
        );
        assert_eq!(
            expected["result"]["frame"]["at"]["tick"],
            fixture::TICK.to_string()
        );
        assert_eq!(expected["result"]["frame"]["at"]["order"], "1");
        assert_eq!(expected["result"]["requiredAnswered"], false);
        assert!(
            !expected["result"]["context"]
                .to_string()
                .contains("Private needle")
        );
        assert!(
            !expected["result"]["context"]
                .to_string()
                .contains("Ask the ferryman")
        );
        if lens == "character" {
            character_half = Some(expected["result"]["context"]["characterPrompt"].clone());
        }
        if lens == "dramatic-irony" {
            assert_eq!(
                character_half.as_ref(),
                Some(&expected["result"]["context"]["characterPrompt"])
            );
        }
        let raw = serde_json::to_string(&input).unwrap();
        let uri = format!("/api/action-drafts/review?request={}", encode(&raw));
        assert_eq!(
            request(service.clone(), "GET", &uri).await,
            (StatusCode::OK, expected.clone())
        );
        let path = file(
            &world,
            "review.json",
            &serde_json::to_value(&input).unwrap(),
        );
        assert_eq!(
            cli(&world, &["action-drafts", "review", "--file", &path]),
            (0, expected)
        );
    }
    assert_eq!(fixture::source_image(&world), before);
    assert!(!world.root.join("author").exists());
    assert!(!world.root.join(".wedl-rust").exists());
}

#[tokio::test]
async fn explicit_empty_answer_and_exact_fresh_review_are_required_for_readiness() {
    let (_directory, world) = fixture::fixture();
    let mut draft = fixture::draft(&world, &fixture::id(1));
    let saved = action_store::save(&world, &fixture::request(&world, draft.clone(), None)).unwrap();
    let sha = saved["afterSha256"].as_str().unwrap();
    draft.phase = "resolution-needed".into();
    draft.questions[0].answer = Some(String::new());
    let mut input = review(&world, "character");
    input.draft = draft.clone();
    let service = Service::new(world.clone());
    let (_, reviewed) = request(
        service.clone(),
        "GET",
        &format!(
            "/api/action-drafts/review?request={}",
            encode(&serde_json::to_string(&input).unwrap())
        ),
    )
    .await;
    assert_eq!(reviewed["result"]["requiredAnswered"], true);
    let confirmation = json!({"frame":reviewed["result"]["frame"],"briefs":[],"families":{},"maxCharacters":100000,"reviewSha256":reviewed["result"]["reviewSha256"],"sufficient":true});
    let good = json!({"expectedFingerprint":world.fingerprint,"expectedSha256":sha,"draft":draft,"review":confirmation});
    for bad in [
        {
            let mut v = good.clone();
            v["draft"].as_object_mut().unwrap().remove("proposals");
            v
        },
        {
            let mut value = good.clone();
            value["review"] = Json::Null;
            value
        },
        {
            let mut value = good.clone();
            value["review"]["sufficient"] = json!(false);
            value
        },
        {
            let mut value = good.clone();
            value["draft"]["bodyMarkdown"] = json!("Changed after review");
            value
        },
        {
            let mut value = good.clone();
            value["draft"]["questions"][0]
                .as_object_mut()
                .unwrap()
                .remove("answer");
            value
        },
    ] {
        let path = file(&world, "bad-save.json", &bad);
        let (code, error) = cli(&world, &["action-drafts", "save", "--file", &path]);
        assert_ne!(code, 0, "{error}");
        assert_eq!(
            action_store::detail(&world, &draft.id, None).unwrap()["sourceSha256"],
            sha
        );
    }
    let path = file(&world, "ready-save.json", &good);
    let (code, saved) = cli(&world, &["action-drafts", "save", "--file", &path]);
    assert_eq!(code, 0, "{saved}");
    assert_eq!(
        action_store::detail(&world, &draft.id, None).unwrap()["draft"]["phase"],
        "resolution-needed"
    );
    // The stored reasoning has no approval or application state.
    assert!(
        !fs::read_to_string(fixture::path(&world, &draft.id))
            .unwrap()
            .contains("approved")
    );
}

#[tokio::test]
async fn draft_routes_reject_writes_open_parameters_and_duplicate_json_before_lookup() {
    let (_directory, world) = fixture::fixture();
    let service = Service::new(world.clone());
    let raw = serde_json::to_string(&review(&world, "author")).unwrap();
    let duplicate = raw.replacen("\"draft\":", "\"draft\":null,\"draft\":", 1);
    for uri in [
        "/api/action-drafts/list?revision=hidden".into(),
        "/api/action-drafts/list?id=hidden".into(),
        format!(
            "/api/action-drafts/detail?id={}&id={}",
            fixture::id(1),
            fixture::id(2)
        ),
        format!(
            "/api/action-drafts/detail?id={}&sha256=wrong",
            fixture::id(1)
        ),
        "/api/action-drafts/detail?id=../story/world".into(),
        format!(
            "/api/action-drafts/review?request={}&approved=true",
            encode(&raw)
        ),
        format!(
            "/api/action-drafts/review?request={}&request={}",
            encode(&raw),
            encode(&raw)
        ),
        format!("/api/action-drafts/review?request={}", encode(&duplicate)),
        "/api/action-drafts/review?request=%FF".into(),
    ] {
        let (status, body) = request(service.clone(), "GET", &uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
        assert_eq!(body["code"], "usage_error");
    }
    for name in ["save", "preview", "delete", "discard", "apply"] {
        assert_eq!(
            request(
                service.clone(),
                "GET",
                &format!("/api/action-drafts/{name}")
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
    }
    for path in [
        "/api/action-drafts/list",
        "/api/action-drafts/detail",
        "/api/action-drafts/review",
        "/api/action-drafts/save",
    ] {
        let (status, error) = request(service.clone(), "POST", path).await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(error["code"], "method_not_allowed");
    }
    assert!(!world.root.join("author").exists());
    assert!(!world.root.join(".wedl-rust").exists());
}

#[tokio::test]
async fn malformed_imports_complete_bounds_and_inaccessible_context_do_not_publish() {
    let (_directory, world) = fixture::fixture();
    let service = Service::new(world.clone());
    let good = serde_json::to_value(review(&world, "character")).unwrap();
    for bad in [
        {
            let mut v = good.clone();
            v["draft"]["phase"] = json!("approved");
            v
        },
        {
            let mut v = good.clone();
            v["draft"]["questions"][0]["answer"] = Json::Null;
            v
        },
        {
            let mut v = good.clone();
            v["draft"]["horizon"]["tick"] = json!(fixture::TICK);
            v
        },
        {
            let mut v = good.clone();
            v["draft"]["horizon"]["tick"] = json!("09007199254740993");
            v
        },
        {
            let mut v = good.clone();
            v["draft"]["evidence"] = Json::Null;
            v
        },
        {
            let mut v = good.clone();
            v["frame"]["workId"] = Json::Null;
            v
        },
        {
            let mut v = good.clone();
            v["maxCharacters"] = json!(255);
            v
        },
    ] {
        let path = file(&world, "invalid-review.json", &bad);
        assert_ne!(
            cli(&world, &["action-drafts", "review", "--file", &path]).0,
            0
        );
        assert_eq!(
            request(
                service.clone(),
                "GET",
                &format!(
                    "/api/action-drafts/review?request={}",
                    encode(&bad.to_string())
                )
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    let mut small = good.clone();
    small["maxCharacters"] = json!(256);
    let (status, error) = request(
        service.clone(),
        "GET",
        &format!(
            "/api/action-drafts/review?request={}",
            encode(&small.to_string())
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("complete action review")
    );
    assert!(error.get("result").is_none());
    let mut hidden = good;
    hidden["frame"]["povId"] = json!("char_00000000000000000000000099");
    let (status, error) = request(
        service,
        "GET",
        &format!(
            "/api/action-drafts/review?request={}",
            encode(&hidden.to_string())
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!error.to_string().contains(&world.fingerprint));
    assert!(!world.root.join("author").exists());
    assert!(!world.root.join(".wedl-rust").exists());
}

#[tokio::test]
async fn private_store_pins_refuse_competing_changes_and_source_snapshot_drift() {
    let (_directory, world) = fixture::fixture();
    let service = Service::new(world.clone());
    let draft = fixture::draft(&world, &fixture::id(1));
    let saved = action_store::save(&world, &fixture::request(&world, draft.clone(), None)).unwrap();
    let old_sha = saved["afterSha256"].as_str().unwrap();
    let mut changed = draft.clone();
    changed.body_markdown.push_str("\nExplicit private change.");
    action_store::save(&world, &fixture::request(&world, changed, Some(old_sha))).unwrap();
    let (status, error) = request(
        service.clone(),
        "GET",
        &format!("/api/action-drafts/detail?id={}&sha256={old_sha}", draft.id),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["code"], "conflict");
    assert!(!error.to_string().contains("Explicit private change"));
    let current = fixture::change_source(&world);
    for uri in [
        "/api/action-drafts/list".to_owned(),
        format!("/api/action-drafts/detail?id={}", draft.id),
    ] {
        let (status, error) = request(service.clone(), "GET", &uri).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(!error.to_string().contains(&current.fingerprint));
    }
    let fresh = action_store::detail(&current, &draft.id, None).unwrap();
    assert_eq!(fresh["sourceChanged"], true);
    assert_eq!(fresh["baseFingerprint"], world.fingerprint);
    let mut historical = current.clone();
    historical.revision = Some("a".repeat(40));
    let historical = Service::new(historical);
    assert!(historical.execute(Operation::ActionDraftList).is_err());
    assert!(
        historical
            .execute(Operation::ActionDraftDetail {
                id: draft.id,
                expected_sha256: None
            })
            .is_err()
    );
    assert!(
        historical
            .execute(Operation::ActionDraftReview {
                request: review(&current, "author")
            })
            .is_err()
    );
}

#[tokio::test]
async fn private_ids_prose_and_counts_never_enter_canonical_reader_transports() {
    let (_directory, world) = fixture::fixture();
    let service = Service::new(world.clone());
    let before = fixture::source_image(&world);
    let uris = [
        "/api/entities".to_owned(),
        "/api/search?q=needle".into(),
        format!(
            "/api/character/search?character={}&q=needle&tick={}&order=1",
            fixture::ACTOR,
            fixture::TICK
        ),
        format!(
            "/api/character/context?character={}&tick={}&order=1&maxCharacters=100000",
            fixture::ACTOR,
            fixture::TICK
        ),
    ];
    let mut originals = Vec::new();
    for uri in &uris {
        let result = request(service.clone(), "GET", uri).await;
        assert_eq!(result.0, StatusCode::OK, "{uri}: {}", result.1);
        originals.push(result);
    }
    action_store::save(
        &world,
        &fixture::request(&world, fixture::draft(&world, &fixture::id(1)), None),
    )
    .unwrap();
    let fresh = Service::new(source::load_world(&world.root, None).unwrap());
    for (uri, original) in uris.iter().zip(originals) {
        let result = request(fresh.clone(), "GET", uri).await;
        assert_eq!(result, original);
        assert!(!result.1.to_string().contains("Private needle"));
        assert!(!result.1.to_string().contains(&fixture::id(1)));
    }
    assert_eq!(fixture::source_image(&world), before);
    assert_eq!(fresh.world().fingerprint, world.fingerprint);
    assert_eq!(fresh.world().records.len(), world.records.len());
}

fn magic_world() -> (tempfile::TempDir, World, String) {
    let input = magic_fixture::edit(
        &magic_fixture::base(),
        &magic_fixture::id("concept", 10),
        |data| {
            data["values"]["number"] = serde_yaml::to_value(9_007_199_254_740_993u64).unwrap();
        },
    );
    magic_fixture::valid(&input);
    let directory = tempfile::tempdir().unwrap();
    for record in input.records.values() {
        let path = directory.path().join(&record.source_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, &record.raw_bytes).unwrap();
    }
    let world = source::load_world(directory.path(), None).unwrap();
    let mut selection = magic_fixture::magic_selection();
    selection
        .restrictions
        .insert("practice".into(), "number".into());
    let families=json!({"magic":{"author":{"rootId":magic_fixture::id("concept",10),"selection":selection},"beliefTargets":[magic_fixture::id("assertion",2)]}}).to_string();
    (directory, world, families)
}

#[tokio::test]
async fn requirement_capture_and_exact_literal_comparison_match_all_transports_and_lenses() {
    use wedl::{action_requirements, writing_frames};
    let (_directory, world, families_raw) = magic_world();
    let service = Service::new(world.clone());
    let before = fixture::source_image(&world);
    let families = writing_frames::decode_families(&families_raw).unwrap();
    let requirement_raw=json!({"targetId":magic_fixture::id("concept",10),"selector":{"field":"number"},"expectedValueRaw":"9007199254740993"}).to_string();
    let requirement = action_requirements::decode_requirement(&requirement_raw).unwrap();
    for lens in ["author", "character", "dramatic-irony"] {
        let frame_json = json!({"fingerprint":world.fingerprint,"work":null,"sceneId":null,"povId":magic_fixture::VIEWER,"at":{"timeline":"main","tick":"6","order":"0"},"lens":lens});
        let frame_raw = frame_json.to_string();
        let frame = writing_frames::decode(&frame_raw).unwrap();
        let path = file(&world, &format!("requirement-{lens}.json"), &frame_json);
        let capture = service
            .execute(Operation::ActionRequirementCapture {
                frame: frame.clone(),
                families: families.clone(),
                max_characters: 100000,
            })
            .unwrap();
        assert_eq!(
            capture["result"],
            action_requirements::capture(&world, &frame, &families, 100000).unwrap()
        );
        assert_eq!(
            capture["result"]["effectivePerspective"],
            if lens == "author" {
                "author"
            } else {
                "character"
            }
        );
        let params = format!(
            "frame={}&families={}&maxCharacters=100000",
            encode(&frame_raw),
            encode(&families_raw)
        );
        assert_eq!(
            request(
                service.clone(),
                "GET",
                &format!("/api/action-requirements/capture?{params}")
            )
            .await,
            (StatusCode::OK, capture.clone())
        );
        assert_eq!(
            cli(
                &world,
                &[
                    "action-requirements",
                    "capture",
                    "--file",
                    &path,
                    "--families",
                    &families_raw,
                    "--max-characters",
                    "100000"
                ]
            ),
            (0, capture)
        );
        let compared = service
            .execute(Operation::ActionRequirementCompare {
                frame: frame.clone(),
                families: families.clone(),
                requirement: requirement.clone(),
                max_characters: 100000,
            })
            .unwrap();
        assert_eq!(
            compared["result"],
            action_requirements::compare(&world, &frame, &families, &requirement, 100000).unwrap()
        );
        assert_eq!(
            compared["result"]["outcome"],
            if lens == "author" {
                "matches"
            } else {
                "unknown"
            }
        );
        if lens == "author" {
            assert_eq!(compared["result"]["actualRaw"], "9007199254740993");
        } else {
            assert!(compared["result"]["actualRaw"].is_null());
            assert_eq!(compared["result"]["citations"], json!([]));
        }
        assert_eq!(
            request(
                service.clone(),
                "GET",
                &format!(
                    "/api/action-requirements/compare?{params}&requirement={}",
                    encode(&requirement_raw)
                )
            )
            .await,
            (StatusCode::OK, compared.clone())
        );
        assert_eq!(
            cli(
                &world,
                &[
                    "action-requirements",
                    "compare",
                    "--file",
                    &path,
                    "--families",
                    &families_raw,
                    "--requirement",
                    &requirement_raw,
                    "--max-characters",
                    "100000"
                ]
            ),
            (0, compared)
        );
        if lens == "author" {
            let mut changed = requirement.clone();
            changed.expected_value_raw = "9007199254740992".into();
            assert_eq!(
                action_requirements::compare(&world, &frame, &families, &changed, 100000).unwrap()
                    ["outcome"],
                "differs"
            );
        }
    }
    assert_eq!(fixture::source_image(&world), before);
    assert!(!world.root.join("author").exists());
    assert!(!world.root.join(".wedl-rust").exists());
}

#[tokio::test]
async fn requirement_routes_reject_unbound_options_duplicate_json_and_large_get_requests() {
    let (_directory, world, families) = magic_world();
    let service = Service::new(world.clone());
    let frame=json!({"fingerprint":world.fingerprint,"work":null,"sceneId":null,"povId":magic_fixture::VIEWER,"at":{"timeline":"main","tick":"6","order":"0"},"lens":"author"}).to_string();
    let params = format!("frame={}&families={}", encode(&frame), encode(&families));
    let requirement=json!({"targetId":magic_fixture::id("concept",10),"selector":{"field":"number"},"expectedValueRaw":"1"}).to_string();
    let duplicate = requirement.replacen(
        "\"expectedValueRaw\":",
        "\"expectedValueRaw\":\"2\",\"expectedValueRaw\":",
        1,
    );
    for uri in [
        format!("/api/action-requirements/capture?{params}&approved=true"),
        format!("/api/action-requirements/capture?{params}&maxCharacters=0"),
        format!("/api/action-requirements/capture?{params}&maxCharacters=255"),
        format!("/api/action-requirements/capture?{params}&maxCharacters=100001"),
        format!(
            "/api/action-requirements/capture?{params}&frame={}",
            encode(&frame)
        ),
        format!(
            "/api/action-requirements/compare?{params}&requirement={}",
            encode(&duplicate)
        ),
        format!("/api/action-drafts/review?request={}", "a".repeat(16385)),
    ] {
        let (status, body) = request(service.clone(), "GET", &uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["code"], "usage_error");
    }
    assert!(!world.root.join("author").exists());
    assert!(!world.root.join(".wedl-rust").exists());
}
