//! All conversation controls are pure, exact native Plan previews on transports.
#[path = "support/conversation_control.rs"]
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
    process::{Command, Stdio},
};
use tower::ServiceExt;
use wedl::{
    Error, consequences, conversation_control, http,
    model::World,
    service::{Operation, Service},
    source, validation,
};

const BUDGET: usize = 100_000;

fn uri(parameters: &[(&str, &str)]) -> String {
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
    format!("/api/conversation-control/preview?{query}")
}

async fn http_call(service: Service, method: &str, uri: &str) -> (StatusCode, Json) {
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

fn cli(world: &World, arguments: &[&str], raw: &str) -> (bool, Json) {
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
        .write_all(raw.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let bytes = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    let value = serde_json::from_slice(bytes)
        .unwrap_or_else(|_| panic!("non-JSON CLI: {}", String::from_utf8_lossy(bytes)));
    (output.status.success(), value)
}

async fn parity(world: &World, input: &Json) -> Json {
    let raw = input.to_string();
    let request = conversation_control::decode(&raw).unwrap();
    let native = conversation_control::preview(world, &request, BUDGET).unwrap();
    let service = Service::new(world.clone());
    let expected = service
        .execute(Operation::ConversationControlPreview {
            request,
            max_characters: BUDGET,
        })
        .unwrap();
    assert_eq!(expected["result"], native);
    assert_eq!(
        http_call(service, "GET", &uri(&[("request", &raw)])).await,
        (StatusCode::OK, expected.clone())
    );
    assert_eq!(
        cli(world, &["conversation-control", "preview"], &raw),
        (true, expected)
    );
    native
}

#[tokio::test]
async fn create_speech_action_and_interruption_share_native_cli_http_and_complete_plan_replay() {
    let (_directory, world) = support::fixture();
    let before = support::source_image(&world);
    let mut interrupted = support::speech(&world);
    interrupted["action"]["interruptLast"] = true.into();
    let mut no_interrupt = support::speech(&world);
    no_interrupt["action"]["interruptLast"] = false.into();
    for input in [
        support::create(&world),
        support::speech(&world),
        support::action(&world),
        interrupted,
        no_interrupt,
    ] {
        let report = parity(&world, &input).await;
        assert_eq!(report["request"], input);
        assert_eq!(report["request"]["at"]["tick"], support::TICK.to_string());
        assert_eq!(
            report["request"]["at"]["order"],
            (support::ORDER + 1).to_string()
        );
        assert_eq!(report["complete"], true);
        assert_eq!(report["canApply"], false);
        assert_eq!(report["writesPerformed"], false);
        assert_eq!(report["preview"]["validation"]["valid"], true);
        let raw = report["plan"].to_string();
        let plan = consequences::decode(&raw).unwrap();
        assert_eq!(
            consequences::preview(&world, &plan, BUDGET).unwrap(),
            report["preview"]
        );
        let replay = Service::new(world.clone())
            .execute(Operation::ConsequencesPreview {
                plan,
                max_characters: BUDGET,
            })
            .unwrap();
        assert_eq!(
            cli(&world, &["consequences", "preview"], &raw),
            (true, replay)
        );
        let proposed = support::candidate(&world, &report);
        assert!(validation::validate_world(&proposed).valid);
        assert_eq!(
            proposed.fingerprint,
            report["preview"]["candidateFingerprint"]
        );
        assert_eq!(support::source_image(&world), before);
    }
    assert!(!world.root.join(".wedl-rust").exists());
}

#[tokio::test]
async fn public_wire_rejects_duplicates_inexact_times_optional_nulls_and_cross_kind_fields() {
    let (_directory, world) = support::fixture();
    let before = support::source_image(&world);
    let value = support::speech(&world);
    let raw = value.to_string();
    let mut invalid = vec![
        "{}".to_owned(),
        format!("{{\"at\":null,{}}}", &raw[1..raw.len() - 1]),
        raw.replace(
            "\"kind\":\"speech\"",
            "\"kind\":\"action\",\"kind\":\"speech\"",
        ),
    ];
    for (path, replacement) in [
        (vec!["apply"], json!(true)),
        (vec!["at", "tick"], json!(support::TICK)),
        (vec!["at", "order"], json!(8)),
        (vec!["at", "order"], json!("08")),
        (vec!["at", "tick"], json!("+9007199254740993")),
        (vec!["at", "tick"], json!("9223372036854775808")),
        (vec!["at", "extra"], json!(false)),
        (vec!["action", "speakerId"], Json::Null),
        (vec!["action", "addresseeId"], Json::Null),
        (vec!["action", "interruptLast"], Json::Null),
        (vec!["action", "actorIds"], json!([support::ACTOR])),
        (vec!["action", "turnId"], json!(" ")),
        (vec!["action", "kind"], json!("narration")),
        (vec!["action", "text"], json!("\r\n ")),
        (
            vec!["action", "text"],
            json!("x".repeat(conversation_control::MAX_REQUEST_BYTES)),
        ),
    ] {
        let mut bad = value.clone();
        match path.as_slice() {
            [key] => bad[*key] = replacement,
            [a, b] => bad[*a][*b] = replacement,
            _ => unreachable!(),
        }
        invalid.push(bad.to_string());
    }
    let mut missing = value.clone();
    missing["at"].as_object_mut().unwrap().remove("order");
    invalid.push(missing.to_string());
    for field in ["speakerId", "addresseeId", "interruptLast"] {
        let mut bad = support::action(&world);
        bad["action"][field] = if field == "interruptLast" {
            json!(false)
        } else {
            json!(support::ACTOR)
        };
        invalid.push(bad.to_string());
    }
    let mut duplicate = support::create(&world);
    duplicate["action"]["characterIds"] = json!([support::ACTOR, support::ACTOR]);
    invalid.push(duplicate.to_string());
    for raw in invalid {
        assert!(conversation_control::decode(&raw).is_err(), "{raw}");
        assert_eq!(
            http_call(
                Service::new(world.clone()),
                "GET",
                &uri(&[("request", &raw)])
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        assert!(!cli(&world, &["conversation-control", "preview"], &raw).0);
    }
    assert_eq!(support::source_image(&world), before);
}

#[tokio::test]
async fn stale_snapshot_and_inactive_conversation_refuse_without_a_plan_or_source_writes() {
    let (_directory, original) = support::fixture();
    let before = support::source_image(&original);
    let mut stale = support::speech(&original);
    stale["expectedFingerprint"] = "f".repeat(64).into();
    let raw = stale.to_string();
    let request = conversation_control::decode(&raw).unwrap();
    assert!(matches!(
        conversation_control::plan(&original, &request),
        Err(Error::Conflict(_))
    ));
    assert_eq!(
        http_call(
            Service::new(original.clone()),
            "GET",
            &uri(&[("request", &raw)])
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert!(!cli(&original, &["conversation-control", "preview"], &raw).0);
    assert_eq!(support::source_image(&original), before);

    let closed = support::changed(&original, support::CONVERSATION, |fields| {
        fields["status"] = "closed".into();
        fields["time"]["end"] =
            serde_yaml::to_value(support::point(support::TICK, support::ORDER)).unwrap();
    });
    let record = &closed.records[support::CONVERSATION];
    fs::write(closed.root.join(&record.source_path), &record.raw_bytes).unwrap();
    let world = source::load_world(&closed.root, None).unwrap();
    assert!(validation::validate_world(&world).valid);
    let before = support::source_image(&world);
    let raw = support::speech(&world).to_string();
    assert_eq!(
        http_call(
            Service::new(world.clone()),
            "GET",
            &uri(&[("request", &raw)])
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert!(!cli(&world, &["conversation-control", "preview"], &raw).0);
    assert_eq!(support::source_image(&world), before);
}

#[tokio::test]
async fn public_preview_budget_and_query_are_closed_and_incomplete_results_refuse() {
    let (_directory, world) = support::fixture();
    let before = support::source_image(&world);
    let raw = support::create(&world).to_string();
    for budget in [255, 256, 100_001] {
        let text = budget.to_string();
        assert_eq!(
            http_call(
                Service::new(world.clone()),
                "GET",
                &uri(&[("request", &raw), ("maxCharacters", &text)])
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        assert!(
            Service::new(world.clone())
                .execute(Operation::ConversationControlPreview {
                    request: conversation_control::decode(&raw).unwrap(),
                    max_characters: budget
                })
                .is_err()
        );
        assert!(
            !cli(
                &world,
                &["conversation-control", "preview", "--max-characters", &text],
                &raw
            )
            .0
        );
    }
    for parameters in [
        vec![],
        vec![("request", raw.as_str()), ("request", raw.as_str())],
        vec![("request", raw.as_str()), ("unknown", "1")],
        vec![("request", raw.as_str()), ("maxCharacters", "1e4")],
    ] {
        assert_eq!(
            http_call(Service::new(world.clone()), "GET", &uri(&parameters))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(support::source_image(&world), before);
}

#[tokio::test]
async fn conversation_control_exposes_no_direct_write_route_even_on_live_author_router() {
    let (_directory, world) = support::fixture();
    let before = support::source_image(&world);
    let raw = support::speech(&world).to_string();
    let target = uri(&[("request", &raw)]);
    assert_eq!(
        http_call(Service::new(world.clone()), "GET", &target)
            .await
            .0,
        StatusCode::OK
    );
    for method in ["POST", "PUT", "PATCH", "DELETE"] {
        assert_eq!(
            http_call(Service::new(world.clone()), method, &target)
                .await
                .0,
            StatusCode::METHOD_NOT_ALLOWED
        );
    }
    assert_eq!(
        http_call(
            Service::new(world.clone()),
            "GET",
            "/api/conversation-control/apply"
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    for target in [
        "/api/conversation-control/preview",
        "/api/conversation-control/apply",
    ] {
        let response = http::live_router(Service::new(world.clone()))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(target)
                    .header("host", "127.0.0.1:8765")
                    .header("origin", "http://127.0.0.1:8765")
                    .header("content-type", "application/json")
                    .header("x-wedl-write", "1")
                    .body(Body::from(raw.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(matches!(
            response.status(),
            StatusCode::METHOD_NOT_ALLOWED | StatusCode::NOT_FOUND
        ));
    }
    assert_eq!(support::source_image(&world), before);
    assert!(!world.root.join(".wedl-rust").exists());
}
