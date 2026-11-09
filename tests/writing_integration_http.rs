//! Current-time plans and work-bound Writing retain their exact native request
//! and source boundaries across service, CLI and read-only HTTP adapters.
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
    path::Path,
    process::{Command, Stdio},
};
use tower::ServiceExt;
use wedl::{
    current_time, http,
    manuscript::{self, Snapshot},
    model::{StoryTime, World},
    service::{Operation, Service},
    source, validation,
    writing_frames::{self, Frame, Lens, WorkSelection},
};

const WORK: &str = "work_00000000000000000000000001";
const SEGMENT: &str = "segment_00000000000000000000000001";

fn authors(world: &World, prose: &str) -> Snapshot {
    let values = [
        (
            json!({"schema":"wedl-author/v1","kind":"manuscript-segment","id":SEGMENT,
                "title":"PRIVATE_SEGMENT_TITLE","source_links":[{"kind":"record","id":support::ACTOR}],
                "placement":{"timeline":"main","tick":support::TICK+100,"order":-1},
                "pov_character_id":support::ACTOR}),
            prose,
        ),
        (
            json!({"schema":"wedl-author/v1","kind":"work","id":WORK,
                "title":"PRIVATE_WORK_TITLE","duplicate_policy":"reject","chapters":[{
                    "id":"chapter_00000000000000000000000001","title":"PRIVATE_CHAPTER_TITLE",
                    "segments":[{"kind":"author-record","id":SEGMENT}]}]}),
            "PRIVATE_WORK_NOTES\r\n",
        ),
    ];
    let records = values
        .into_iter()
        .map(|(fields, body)| {
            let path = manuscript::document_path(
                fields["kind"].as_str().unwrap(),
                fields["id"].as_str().unwrap(),
            )
            .unwrap();
            let raw = format!(
                "---\n{}---\n{body}",
                serde_yaml::to_string(&fields).unwrap()
            );
            fs::create_dir_all(world.root.join(&path).parent().unwrap()).unwrap();
            fs::write(world.root.join(&path), &raw).unwrap();
            manuscript::parse_document(raw.as_bytes(), &path).unwrap()
        })
        .collect();
    manuscript::snapshot_from_records(records, None).unwrap()
}

fn frame(world: &World, authors: Option<&Snapshot>, lens: Lens) -> Frame {
    Frame {
        fingerprint: world.fingerprint.clone(),
        work: authors.map(|authors| WorkSelection {
            work_id: WORK.into(),
            author_fingerprint: authors.fingerprint.clone(),
        }),
        scene_id: Some(support::SCENE.into()),
        pov_id: Some(support::ACTOR.into()),
        at: support::horizon(),
        lens,
    }
}

fn active_fixture() -> (tempfile::TempDir, World) {
    let (directory, world) = support::fixture();
    for id in [world.world_record().unwrap().id.as_str(), support::SCENE] {
        let record = &world.records[id];
        let mut fields = record.frontmatter.clone();
        if record.kind == "world" {
            fields["current_time"] =
                serde_yaml::to_value(support::point(support::TICK, 1)).unwrap();
        } else {
            fields["status"] = "active".into();
        }
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
    let world = source::load_world(&world.root, None).unwrap();
    assert!(validation::validate_world(&world).valid);
    (directory, world)
}

fn source_image(root: &Path) -> Vec<(String, Vec<u8>, std::time::SystemTime)> {
    let mut result = Vec::new();
    for folder in ["story", "author"] {
        let path = root.join(folder);
        if !path.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(path) {
            let entry = entry.unwrap();
            if entry.file_type().is_file() {
                result.push((
                    entry
                        .path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    fs::read(entry.path()).unwrap(),
                    fs::metadata(entry.path()).unwrap().modified().unwrap(),
                ));
            }
        }
    }
    result.sort_by(|left, right| left.0.cmp(&right.0));
    result
}

fn uri(path: &str, parameters: &[(&str, &str)]) -> String {
    let query = parameters
        .iter()
        .map(|(key, value)| {
            let value = value
                .bytes()
                .map(|byte| format!("%{byte:02X}"))
                .collect::<String>();
            format!("{key}={value}")
        })
        .collect::<Vec<_>>()
        .join("&");
    format!("/api/{path}?{query}")
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

fn cli(world: &World, arguments: &[&str], input: &str) -> (bool, Json) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_wedl-rs"));
    command.args(["--compact", "--repo"]).arg(&world.root);
    if let Some(revision) = &world.revision {
        command.args(["--revision", revision]);
    }
    let mut child = command
        .args(arguments)
        .args(["--file", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let bytes = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    let value = serde_json::from_slice(bytes)
        .unwrap_or_else(|_| panic!("non-JSON CLI result: {}", String::from_utf8_lossy(bytes)));
    (output.status.success(), value)
}

async fn parity(
    world: &World,
    operation: Operation,
    path: &str,
    parameters: &[(&str, &str)],
    arguments: &[&str],
    input: &str,
    native: Json,
) -> Json {
    let service = Service::new(world.clone());
    let expected = service.execute(operation).unwrap();
    assert_eq!(expected["result"], native);
    let (status, http) = request(service, "GET", &uri(path, parameters)).await;
    assert_eq!(status, StatusCode::OK, "{http}");
    assert_eq!(http, expected);
    assert_eq!(cli(world, arguments, input), (true, expected));
    http["result"].clone()
}

#[tokio::test]
async fn shared_time_native_service_cli_and_http_preserve_exact_plan_and_all_source() {
    let (_directory, world) = active_fixture();
    let before = source_image(&world.root);
    let input = current_time::Request {
        expected_fingerprint: world.fingerprint.clone(),
        at: StoryTime::new("main", support::TICK, 2).unwrap(),
    };
    let raw = serde_json::to_string(&input).unwrap();
    let native = current_time::preview(&world, &input, 100_000).unwrap();
    let result = parity(
        &world,
        Operation::CurrentTimePreview {
            request: input,
            max_characters: 100_000,
        },
        "current-time/preview",
        &[("request", &raw)],
        &["current-time", "preview"],
        &raw,
        native,
    )
    .await;
    assert_eq!(result["request"]["at"]["tick"], support::TICK.to_string());
    assert_eq!(result["activeSceneIds"], json!([support::SCENE]));
    assert_eq!(result["plan"]["operations"].as_array().unwrap().len(), 2);
    assert_eq!(result["preview"]["validation"]["valid"], true);
    assert_eq!(result["writesPerformed"], false);
    assert_eq!(result["canApply"], false);
    assert_eq!(source_image(&world.root), before);
}

#[tokio::test]
async fn invalid_clock_candidate_is_a_complete_reviewable_read_and_routes_never_write() {
    let (_directory, world) = active_fixture();
    let before = source_image(&world.root);
    let input = current_time::Request {
        expected_fingerprint: world.fingerprint.clone(),
        at: StoryTime::new("main", support::TICK - 2, 0).unwrap(),
    };
    let raw = serde_json::to_string(&input).unwrap();
    let native = current_time::preview(&world, &input, 100_000).unwrap();
    let result = parity(
        &world,
        Operation::CurrentTimePreview {
            request: input,
            max_characters: 100_000,
        },
        "current-time/preview",
        &[("request", &raw), ("maxCharacters", "100000")],
        &["current-time", "preview", "--max-characters", "100000"],
        &raw,
        native,
    )
    .await;
    assert_eq!(result["preview"]["validation"]["valid"], false);
    assert_eq!(result["preview"]["files"].as_array().unwrap().len(), 2);
    assert_eq!(result["complete"], true);
    let service = Service::new(world.clone());
    let target = uri("current-time/preview", &[("request", &raw)]);
    assert_eq!(
        request(service.clone(), "POST", &target).await.0,
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(
        request(service, "GET", "/api/current-time/apply").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(source_image(&world.root), before);
}

#[tokio::test]
async fn current_time_transport_refuses_malformed_fields_stale_pins_and_incomplete_budgets() {
    let (_directory, world) = active_fixture();
    let service = Service::new(world.clone());
    let value = json!({"expectedFingerprint":world.fingerprint,"at":{"timeline":"main","tick":support::TICK.to_string(),"order":"2"}});
    let raw = value.to_string();
    let mut malformed = vec![
        "{}".into(),
        format!("{{\"at\":null,{}}}", &raw[1..raw.len() - 1]),
    ];
    for (key, replacement) in [
        ("extra", json!(true)),
        (
            "at",
            json!({"timeline":"main","tick":support::TICK,"order":"2"}),
        ),
    ] {
        let mut bad = value.clone();
        bad[key] = replacement;
        malformed.push(bad.to_string());
    }
    for bad in malformed {
        let (status, result) = request(
            service.clone(),
            "GET",
            &uri("current-time/preview", &[("request", &bad)]),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{result}");
        assert!(!cli(&world, &["current-time", "preview"], &bad).0);
    }
    for budget in ["255", "100001", "1e4", "null", "256"] {
        let (status, result) = request(
            service.clone(),
            "GET",
            &uri(
                "current-time/preview",
                &[("request", &raw), ("maxCharacters", budget)],
            ),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{result}");
    }
    for parameters in [
        vec![("request", raw.as_str()), ("extra", "true")],
        vec![("request", raw.as_str()), ("request", raw.as_str())],
        vec![],
    ] {
        assert_eq!(
            request(
                service.clone(),
                "GET",
                &uri("current-time/preview", &parameters)
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    for budget in [255, 100_001] {
        assert!(
            service
                .execute(Operation::CurrentTimePreview {
                    request: current_time::decode(&raw).unwrap(),
                    max_characters: budget
                })
                .is_err()
        );
        assert!(
            !cli(
                &world,
                &[
                    "current-time",
                    "preview",
                    "--max-characters",
                    &budget.to_string()
                ],
                &raw
            )
            .0
        );
    }
    let mut stale = value;
    stale["expectedFingerprint"] = "f".repeat(64).into();
    let (status, result) = request(
        service,
        "GET",
        &uri("current-time/preview", &[("request", &stale.to_string())]),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{result}");
    assert!(result.get("plan").is_none());
}

#[tokio::test]
async fn work_bound_resolve_choices_and_context_share_all_adapters_without_private_admission() {
    let (_directory, world) = support::fixture();
    let authors = authors(&world, "PRIVATE_PROSE 🌲\r\n  Literal trailing space  ");
    let before = source_image(&world.root);
    for lens in [Lens::Author, Lens::Character, Lens::DramaticIrony] {
        let selected = frame(&world, Some(&authors), lens);
        let raw = serde_json::to_string(&selected).unwrap();
        let resolved = parity(
            &world,
            Operation::WritingResolve {
                frame: selected.clone(),
            },
            "writing/resolve",
            &[("frame", &raw)],
            &["writing", "resolve"],
            &raw,
            writing_frames::resolve(&world, Some(&authors), &selected).unwrap(),
        )
        .await;
        assert_eq!(resolved, json!(selected));
        let choices = parity(
            &world,
            Operation::WritingChoices {
                frame: selected.clone(),
                limit: 100,
            },
            "writing/choices",
            &[("frame", &raw)],
            &["writing", "choices"],
            &raw,
            writing_frames::choices(&world, Some(&authors), &selected, 100).unwrap(),
        )
        .await;
        if lens == Lens::Author {
            assert_eq!(
                choices["works"],
                json!([{"id":WORK,"label":"PRIVATE_WORK_TITLE"}])
            );
            assert_eq!(choices["authorFingerprint"], authors.fingerprint);
        } else {
            assert_eq!(choices["works"], json!([]));
            assert!(choices["authorFingerprint"].is_null());
            assert!(!choices.to_string().contains("PRIVATE_"));
        }
        let context = parity(
            &world,
            Operation::WritingContext {
                frame: selected.clone(),
                briefs: vec![],
                max_characters: 100_000,
            },
            "writing/context",
            &[("frame", &raw), ("maxCharacters", "100000")],
            &["writing", "context", "--max-characters", "100000"],
            &raw,
            writing_frames::context(&world, Some(&authors), &selected, &[], 100_000).unwrap(),
        )
        .await;
        let unbound = frame(&world, None, lens);
        let plain = writing_frames::context(&world, None, &unbound, &[], 100_000).unwrap();
        assert_eq!(context["characterPrompt"], plain["characterPrompt"]);
        assert_eq!(context["authorMargin"], plain["authorMargin"]);
        assert_eq!(context["frame"]["at"], json!(support::horizon()));
        assert!(!context.to_string().contains("PRIVATE_"));
        let (status, family_context) = request(
            Service::new(world.clone()),
            "GET",
            &uri(
                "writing/context",
                &[
                    ("frame", &raw),
                    ("maxCharacters", "100000"),
                    ("families", "{}"),
                ],
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{family_context}");
        assert_eq!(family_context["result"], context);
    }
    assert_eq!(source_image(&world.root), before);
}

#[tokio::test]
async fn writing_transport_refuses_five_field_frames_malformed_work_and_closed_query_errors() {
    let (_directory, world) = support::fixture();
    let authors = authors(&world, "PRIVATE_PROSE");
    let value = json!(frame(&world, Some(&authors), Lens::Character));
    let service = Service::new(world.clone());
    let mut old = value.clone();
    old.as_object_mut().unwrap().remove("work");
    let mut malformed = vec![old.to_string()];
    for work in [
        json!(WORK),
        json!({"workId":WORK}),
        json!({"workId":WORK,"authorFingerprint":authors.fingerprint,"occurrence":0}),
    ] {
        let mut bad = value.clone();
        bad["work"] = work;
        malformed.push(bad.to_string());
    }
    malformed.push(value.to_string().replace(
        &format!("\"workId\":\"{WORK}\""),
        &format!("\"workId\":\"{WORK}\",\"workId\":\"{WORK}\""),
    ));
    for bad in malformed {
        for name in ["resolve", "choices", "context"] {
            let (status, result) = request(
                service.clone(),
                "GET",
                &uri(&format!("writing/{name}"), &[("frame", &bad)]),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{result}");
            assert!(!cli(&world, &["writing", name], &bad).0);
        }
    }
    let raw = value.to_string();
    for (path, field, bad) in [
        ("writing/context", "maxCharacters", "255"),
        ("writing/context", "maxCharacters", "100001"),
        ("writing/choices", "limit", "0"),
        ("writing/choices", "limit", "1001"),
        ("writing/resolve", "workId", WORK),
    ] {
        assert_eq!(
            request(
                service.clone(),
                "GET",
                &uri(path, &[("frame", &raw), (field, bad)])
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        request(
            service.clone(),
            "GET",
            &uri("writing/resolve", &[("frame", &raw), ("frame", &raw)])
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(service, "POST", &uri("writing/resolve", &[("frame", &raw)]))
            .await
            .0,
        StatusCode::METHOD_NOT_ALLOWED
    );
}

#[tokio::test]
async fn private_edits_stale_bound_transport_pins_without_changing_unbound_character_components() {
    let (_directory, world) = support::fixture();
    let original_authors = authors(&world, "PRIVATE_ORIGINAL_PROSE");
    let service = Service::new(world.clone());
    let bound = frame(&world, Some(&original_authors), Lens::Character);
    let raw = serde_json::to_string(&bound).unwrap();
    let target = uri(
        "writing/context",
        &[("frame", &raw), ("maxCharacters", "100000")],
    );
    let original = request(service.clone(), "GET", &target).await;
    assert_eq!(original.0, StatusCode::OK);
    let changed_authors = authors(&world, "PRIVATE_CHANGED_PROSE 🌲");
    let stale = request(service.clone(), "GET", &target).await;
    assert_eq!(stale.0, StatusCode::NOT_FOUND);
    assert_eq!(
        stale.1["message"],
        "writing frame is unavailable; select it again"
    );
    assert!(
        service
            .execute(Operation::WritingResolve {
                frame: bound.clone()
            })
            .is_err()
    );
    assert!(
        !cli(
            &world,
            &["writing", "context", "--max-characters", "100000"],
            &raw
        )
        .0
    );
    for lens in [Lens::Character, Lens::DramaticIrony] {
        for selected in [
            frame(&world, None, lens),
            frame(&world, Some(&changed_authors), lens),
        ] {
            let raw = serde_json::to_string(&selected).unwrap();
            let (status, result) = request(
                service.clone(),
                "GET",
                &uri(
                    "writing/context",
                    &[("frame", &raw), ("maxCharacters", "100000")],
                ),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{result}");
            assert_eq!(
                result["result"]["characterPrompt"],
                original.1["result"]["characterPrompt"]
            );
            assert!(!result["result"].to_string().contains("PRIVATE_"));
        }
    }
    let selected = frame(&world, Some(&changed_authors), Lens::Character);
    let raw = serde_json::to_string(&selected).unwrap();
    support::change_source(&world);
    let (status, result) =
        request(service, "GET", &uri("writing/resolve", &[("frame", &raw)])).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{result}");
    assert!(!cli(&world, &["writing", "resolve"], &raw).0);
}

fn git(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

#[tokio::test]
async fn historical_work_transports_pair_original_commit_sources_without_live_fallback() {
    let (_directory, current) = support::fixture();
    let authors = authors(&current, "PRIVATE_ORIGINAL_PROSE");
    git(&current.root, &["init", "-q"]);
    git(
        &current.root,
        &["config", "user.name", "Writing integration tests"],
    );
    git(
        &current.root,
        &["config", "user.email", "writing@example.invalid"],
    );
    git(&current.root, &["add", "--", "story", "author"]);
    git(
        &current.root,
        &["commit", "-q", "-m", "Original paired source"],
    );
    let revision = git(&current.root, &["rev-parse", "HEAD"]);
    let original = source::load_world(&current.root, Some(&revision)).unwrap();
    let selected = frame(&original, Some(&authors), Lens::DramaticIrony);
    let raw = serde_json::to_string(&selected).unwrap();
    let native = writing_frames::with_author_snapshot(&original, &selected, false, |authors| {
        writing_frames::context(&original, authors, &selected, &[], 100_000)
    })
    .unwrap();
    fs::remove_file(current.root.join(&authors.documents[WORK].source_path)).unwrap();
    fs::write(
        current.root.join(&authors.documents[SEGMENT].source_path),
        b"Invalid live private source",
    )
    .unwrap();
    support::change_source(&current);
    let before = source_image(&current.root);
    let result = parity(
        &original,
        Operation::WritingContext {
            frame: selected.clone(),
            briefs: vec![],
            max_characters: 100_000,
        },
        "writing/context",
        &[("frame", &raw), ("maxCharacters", "100000")],
        &["writing", "context", "--max-characters", "100000"],
        &raw,
        native,
    )
    .await;
    assert_eq!(result["frame"], json!(selected));
    let current = source::load_world(&current.root, None).unwrap();
    assert_eq!(
        request(
            Service::new(current),
            "GET",
            &uri("writing/context", &[("frame", &raw)])
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(source_image(&original.root), before);
}

#[tokio::test]
async fn writing_full_author_brief_union_agrees_across_native_service_cli_and_http() {
    let (_directory, original) = active_fixture();
    const BRIEF: &str = "brief_00000000000000000000000001";
    for record in original
        .records
        .values()
        .filter(|record| record.kind == "world" || record.id == support::ACTOR)
    {
        let mut fields = record.frontmatter.clone();
        if record.kind == "world" {
            fields["capabilities"]
                .as_sequence_mut()
                .unwrap()
                .push("briefs-v1".into());
        } else {
            fields["audience"] = serde_yaml::to_value(json!(["author"])).unwrap();
        }
        fs::write(
            original.root.join(&record.source_path),
            format!(
                "---\n{}---\n{}",
                serde_yaml::to_string(&fields).unwrap(),
                record.body
            ),
        )
        .unwrap();
    }
    let fields = json!({"schema":"wedl/v1","kind":"context-brief","id":BRIEF,"title":"Union brief","domain":"writing.union","scope":"world","valid":{"kind":"timeless"},"audience":["public"],"perspectives":["ordinary"],"required_refs":[{"kind":"record","id":support::ACTOR}]});
    fs::write(
        original.root.join("story/union-brief.md"),
        format!(
            "---\n{}---\nUnion brief prose.\n",
            serde_yaml::to_string(&fields).unwrap()
        ),
    )
    .unwrap();
    let world = source::load_world(&original.root, None).unwrap();
    let report = validation::validate_world(&world);
    assert!(report.valid, "{report:?}");
    let before = source_image(&world.root);
    let selected = frame(&world, None, Lens::Author);
    let raw = serde_json::to_string(&selected).unwrap();
    let briefs = serde_json::to_string(&[BRIEF]).unwrap();
    let result = parity(
        &world,
        Operation::WritingContext {
            frame: selected.clone(),
            briefs: vec![BRIEF.into()],
            max_characters: 100_000,
        },
        "writing/context",
        &[
            ("frame", &raw),
            ("briefs", &briefs),
            ("maxCharacters", "100000"),
        ],
        &[
            "writing",
            "context",
            "--brief",
            BRIEF,
            "--max-characters",
            "100000",
        ],
        &raw,
        writing_frames::context(&world, None, &selected, &[BRIEF.into()], 100_000).unwrap(),
    )
    .await;
    let expected =
        wedl::briefs::compose_full_author(&world, BRIEF, Some(&selected.at), 1000).unwrap();
    assert_eq!(result["authorMargin"]["briefs"], expected["items"]);
    assert_eq!(source_image(&world.root), before);
}
