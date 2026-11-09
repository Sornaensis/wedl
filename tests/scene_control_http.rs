//! Coordinated scene plans retain one exact source snapshot across all adapters.
#[path = "support/scene_control.rs"]
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
    Error, consequences, http,
    model::World,
    scene_control,
    service::{Operation, Service},
    source, validation,
};

const BUDGET: usize = 100_000;
const NEW_SCENE: &str = "scene_00000000000000000000000003";
const EVENT: &str = "event_00000000000000000000000001";

fn input(world: &World, action: Json) -> Json {
    json!({"expectedFingerprint":world.fingerprint,
        "at":support::at(support::ORDER + 1),"action":action})
}

fn advance(world: &World) -> Json {
    input(
        world,
        json!({"type":"scene.advance","sceneId":support::SCENE}),
    )
}

fn uri(parameters: &[(&str, &str)]) -> String {
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
    format!("/api/scene-control/preview?{query}")
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
        .unwrap_or_else(|_| panic!("non-JSON CLI result: {}", String::from_utf8_lossy(bytes)));
    (output.status.success(), value)
}

async fn parity(world: &World, value: &Json) -> Json {
    let raw = value.to_string();
    let request = scene_control::decode(&raw).unwrap();
    let native = scene_control::preview(world, &request, BUDGET).unwrap();
    let service = Service::new(world.clone());
    let expected = service
        .execute(Operation::SceneControlPreview {
            request,
            max_characters: BUDGET,
        })
        .unwrap();
    assert_eq!(expected["result"], native);
    let (status, response) = http_call(service, "GET", &uri(&[("request", &raw)])).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response, expected);
    assert_eq!(
        cli(world, &["scene-control", "preview"], &raw),
        (true, expected)
    );
    native
}

#[tokio::test]
async fn every_scene_action_matches_native_service_cli_and_http_and_exports_a_replayable_plan() {
    let (_directory, world) = support::fixture();
    let before = support::source_image(&world);
    let actions = [
        json!({"type":"scene.create","sceneId":NEW_SCENE,"title":"Exact arrival 🌲",
            "locationId":support::DESTINATION,"characterIds":[support::ACTOR],
            "event":{"id":EVENT,"title":"Arrival event"}}),
        json!({"type":"scene.advance","sceneId":support::SCENE}),
        json!({"type":"scene.close","sceneId":support::SCENE}),
        json!({"type":"character.move","characterIds":[support::ACTOR],
            "locationId":support::DESTINATION,"sceneId":support::OTHER_SCENE,
            "event":{"id":EVENT,"title":"Movement event"}}),
    ];
    for action in actions {
        let result = parity(&world, &input(&world, action)).await;
        assert_eq!(result["request"]["at"]["tick"], support::TICK.to_string());
        assert_eq!(
            result["request"]["at"]["order"],
            (support::ORDER + 1).to_string()
        );
        assert_eq!(result["complete"], true);
        assert_eq!(result["writesPerformed"], false);
        assert_eq!(result["canApply"], false);
        assert_eq!(result["preview"]["validation"]["valid"], true, "{result}");
        let exported = result["plan"].to_string();
        let plan = consequences::decode(&exported).unwrap();
        assert_eq!(
            consequences::preview(&world, &plan, BUDGET).unwrap(),
            result["preview"]
        );
        let replayed = Service::new(world.clone())
            .execute(Operation::ConsequencesPreview {
                plan,
                max_characters: BUDGET,
            })
            .unwrap();
        assert_eq!(replayed["result"], result["preview"]);
        assert_eq!(
            cli(&world, &["consequences", "preview"], &exported),
            (true, replayed)
        );
        let proposed = support::candidate(&world, &result);
        assert!(validation::validate_world(&proposed).valid);
        assert_eq!(
            proposed.fingerprint,
            result["preview"]["candidateFingerprint"]
        );
        assert_eq!(support::source_image(&world), before);
    }
    assert!(!world.root.join(".wedl-rust").exists());
}

#[tokio::test]
async fn scene_control_wire_refuses_duplicate_unknown_missing_and_inexact_fields() {
    let (_directory, world) = support::fixture();
    let before = support::source_image(&world);
    let service = Service::new(world.clone());
    let value = advance(&world);
    let raw = value.to_string();
    let mut malformed = vec![
        "{}".to_owned(),
        format!("{{\"at\":null,{}}}", &raw[1..raw.len() - 1]),
        raw.replace(
            "\"type\":\"scene.advance\"",
            "\"type\":\"scene.close\",\"type\":\"scene.advance\"",
        ),
    ];
    for (path, replacement) in [
        (vec!["apply"], json!(true)),
        (vec!["at", "tick"], json!(support::TICK)),
        (vec!["at", "order"], json!(support::ORDER + 1)),
        (vec!["at", "order"], json!("08")),
        (vec!["at", "tick"], json!("+9007199254740993")),
        (vec!["at", "extra"], json!(true)),
        (vec!["action", "type"], json!("scene.unknown")),
        (vec!["action", "characterIds"], json!([support::ACTOR])),
    ] {
        let mut bad = value.clone();
        match path.as_slice() {
            [key] => bad[*key] = replacement,
            [first, second] => bad[*first][*second] = replacement,
            _ => unreachable!(),
        }
        malformed.push(bad.to_string());
    }
    let mut missing = value.clone();
    missing["at"].as_object_mut().unwrap().remove("order");
    malformed.push(missing.to_string());
    for raw in malformed {
        assert!(scene_control::decode(&raw).is_err(), "{raw}");
        let (status, response) =
            http_call(service.clone(), "GET", &uri(&[("request", &raw)])).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
        assert!(!cli(&world, &["scene-control", "preview"], &raw).0);
    }
    assert_eq!(support::source_image(&world), before);
}

#[tokio::test]
async fn stale_scene_snapshots_fail_all_adapters_without_producing_a_plan() {
    let (_directory, world) = support::fixture();
    let before = support::source_image(&world);
    let mut value = advance(&world);
    value["expectedFingerprint"] = "f".repeat(64).into();
    let raw = value.to_string();
    let input = scene_control::decode(&raw).unwrap();
    assert!(matches!(
        scene_control::plan(&world, &input),
        Err(Error::Conflict(_))
    ));
    let service = Service::new(world.clone());
    assert!(matches!(
        service.execute(Operation::SceneControlPreview {
            request: input,
            max_characters: BUDGET,
        }),
        Err(Error::Conflict(_))
    ));
    let (status, response) = http_call(service, "GET", &uri(&[("request", &raw)])).await;
    assert_eq!(status, StatusCode::CONFLICT, "{response}");
    assert!(response.get("result").is_none());
    assert!(!cli(&world, &["scene-control", "preview"], &raw).0);
    assert_eq!(support::source_image(&world), before);
}

#[tokio::test]
async fn closed_scenes_are_not_reopened_by_scene_control_transports() {
    let (_directory, original) = support::fixture();
    let closed = support::changed(&original, support::SCENE, |fields| {
        fields["status"] = "closed".into();
        fields["time"]["end"] =
            serde_yaml::to_value(support::point(support::TICK, support::ORDER)).unwrap();
    });
    let record = &closed.records[support::SCENE];
    fs::write(closed.root.join(&record.source_path), &record.raw_bytes).unwrap();
    let world = source::load_world(&closed.root, None).unwrap();
    assert!(validation::validate_world(&world).valid);
    let before = support::source_image(&world);
    for kind in ["scene.advance", "scene.close"] {
        let value = input(&world, json!({"type":kind,"sceneId":support::SCENE}));
        let raw = value.to_string();
        let input = scene_control::decode(&raw).unwrap();
        assert!(scene_control::plan(&world, &input).is_err());
        let service = Service::new(world.clone());
        assert!(
            service
                .execute(Operation::SceneControlPreview {
                    request: input,
                    max_characters: BUDGET,
                })
                .is_err()
        );
        assert_eq!(
            http_call(service, "GET", &uri(&[("request", &raw)]))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        assert!(!cli(&world, &["scene-control", "preview"], &raw).0);
    }
    assert_eq!(support::source_image(&world), before);
}

#[tokio::test]
async fn scene_control_refuses_incomplete_budgets_and_closed_query_violations() {
    let (_directory, world) = support::fixture();
    let before = support::source_image(&world);
    let service = Service::new(world.clone());
    let raw = advance(&world).to_string();
    for budget in [255, 256, 100_001] {
        let limit = budget.to_string();
        let (status, response) = http_call(
            service.clone(),
            "GET",
            &uri(&[("request", &raw), ("maxCharacters", &limit)]),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
        assert!(
            service
                .execute(Operation::SceneControlPreview {
                    request: scene_control::decode(&raw).unwrap(),
                    max_characters: budget,
                })
                .is_err()
        );
        assert!(
            !cli(
                &world,
                &["scene-control", "preview", "--max-characters", &limit],
                &raw
            )
            .0
        );
    }
    for query in [
        vec![],
        vec![("request", raw.as_str()), ("request", raw.as_str())],
        vec![("request", raw.as_str()), ("unknown", "true")],
        vec![("request", raw.as_str()), ("maxCharacters", "1e4")],
    ] {
        assert_eq!(
            http_call(service.clone(), "GET", &uri(&query)).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(support::source_image(&world), before);
}

#[tokio::test]
async fn scene_control_http_exposes_only_read_requests_and_never_an_apply_route() {
    let (_directory, world) = support::fixture();
    let before = support::source_image(&world);
    let raw = advance(&world).to_string();
    let service = Service::new(world.clone());
    let target = uri(&[("request", &raw)]);
    assert_eq!(
        http_call(service.clone(), "GET", &target).await.0,
        StatusCode::OK
    );
    for method in ["POST", "PUT", "PATCH", "DELETE"] {
        assert_eq!(
            http_call(service.clone(), method, &target).await.0,
            StatusCode::METHOD_NOT_ALLOWED
        );
    }
    assert_eq!(
        http_call(service, "GET", "/api/scene-control/apply")
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(support::source_image(&world), before);
    assert!(!world.root.join(".wedl-rust").exists());
}
