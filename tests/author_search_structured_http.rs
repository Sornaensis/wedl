//! Structured author search is one native operation across its transports.
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value as Json, json};
use std::{
    collections::BTreeMap,
    fs,
    process::{Command, Output},
    time::SystemTime,
};
use tower::ServiceExt;
use wedl::{
    compiler::{self, AuthorSearchSelection},
    http,
    model::{Record, StoryTime, World},
    retrieval::Mode,
    service::{Operation, Service, TimeInput},
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const HERO: &str = "char_00000000000000000000000001";
const EVENT: &str = "event_00000000000000000000000001";
const EFFECT: &str = "effect_00000000000000000000000001";
const T1: &str = "thread_00000000000000000000000001";
const T2: &str = "thread_00000000000000000000000002";
const TICK: i64 = 9_007_199_254_740_993;
const MODES: [Mode; 3] = [Mode::Lexical, Mode::Semantic, Mode::Hybrid];

fn record(kind: &str, id: &str, title: &str, fields: Json) -> Record {
    let mut fields = fields.as_object().unwrap().clone();
    fields.extend(
        json!({"schema":"wedl/v1","kind":kind,"id":id,"title":title})
            .as_object()
            .unwrap()
            .clone(),
    );
    source::parse_record(
        format!("---\n{}---\n", serde_yaml::to_string(&fields).unwrap()).as_bytes(),
        &format!("story/{id}.md"),
    )
    .unwrap()
}

fn fixture(value: &str) -> (tempfile::TempDir, World) {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("story")).unwrap();
    let records = vec![
        record(
            "world",
            WORLD,
            "Transport",
            json!({
                "capabilities":[],"default_timeline":"main",
                "timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],
                "threads":[{"id":T1,"label":"First"},{"id":T2,"label":"Empty"}],
                "state_keys":{"character":{"note":{"type":"string"}}}
            }),
        ),
        record(
            "character",
            HERO,
            "Witness",
            json!({"initial_state":{"note":"calm"}}),
        ),
        record(
            "event",
            EVENT,
            "Change",
            json!({
                "time":{"tick":TICK,"order":-7},"threads":[T1],
                "effects":[{"id":EFFECT,"target":HERO,"key":"note","operation":"set","value":value}]
            }),
        ),
    ];
    for record in records {
        fs::write(directory.path().join(record.source_path), record.raw_bytes).unwrap();
    }
    let world = source::load_world(directory.path(), None).unwrap();
    let checked = validation::validate_world(&world);
    assert!(checked.valid, "{:?}", checked.diagnostics);
    (directory, world)
}

fn selection(order: i32, timeline: &str, threads: Option<Vec<String>>) -> AuthorSearchSelection {
    AuthorSearchSelection {
        at: Some(StoryTime::new(timeline, TICK, order).unwrap()),
        thread_ids: threads,
    }
}

fn operation(mode: Mode, selection: &AuthorSearchSelection, limit: usize) -> Operation {
    Operation::SearchWithSelection {
        query: "transportneedle".into(),
        limit,
        kind: Some("event".into()),
        mode,
        at: selection.at.as_ref().map(|at| TimeInput {
            tick: at.tick,
            order: at.order,
            timeline: Some(at.timeline.clone()),
        }),
        thread_ids: selection.thread_ids.clone(),
    }
}

fn envelope(world: &World, matches: Json) -> Json {
    json!({"protocol":wedl::CONTRACT,"fingerprint":world.fingerprint,"revision":world.revision,
        "result":{"scope":matches["scope"],"matches":matches}})
}

fn wire(value: &Json) -> Json {
    // Compare exactly after the shared JSON serialization/float parse boundary.
    serde_json::from_slice(&serde_json::to_vec(value).unwrap()).unwrap()
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

async fn get(service: Service, query: &str) -> (StatusCode, Json) {
    let response = http::router(service)
        .oneshot(
            Request::builder()
                .uri(format!("/api/search?q=transportneedle&kind=event&{query}"))
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

fn cli(world: &World, mode: Mode, limit: usize, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .arg("--repo")
        .arg(&world.root)
        .arg("--compact")
        .args([
            "search",
            "transportneedle",
            "--kind",
            "event",
            "--mode",
            mode.as_str(),
            "--limit",
            &limit.to_string(),
        ])
        .args(args)
        .output()
        .unwrap()
}

fn ok(output: &Output) -> Json {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn source_image(world: &World) -> BTreeMap<String, (Vec<u8>, SystemTime)> {
    world
        .records
        .values()
        .map(|record| {
            let path = world.root.join(&record.source_path);
            (
                record.source_path.clone(),
                (
                    fs::read(&path).unwrap(),
                    fs::metadata(path).unwrap().modified().unwrap(),
                ),
            )
        })
        .collect()
}

#[tokio::test]
async fn structured_members_match_native_service_cli_and_http_at_exact_large_times() {
    let (_directory, world) = fixture("transportneedle");
    let source_before = source_image(&world);
    let service = Service::new(world.clone());
    let tick = TICK.to_string();
    for mode in MODES {
        for (order, timeline, admitted) in [
            (-8, "main", false),
            (-7, "main", true),
            (-7, "other", false),
        ] {
            let selection = selection(order, timeline, None);
            let native = compiler::search_with_selection(
                &world,
                "transportneedle",
                100,
                Some("event"),
                mode,
                &selection,
            )
            .unwrap();
            assert_eq!(!native["items"].as_array().unwrap().is_empty(), admitted);
            if admitted {
                let effect = native["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|item| item["documentKind"] == "event-effect")
                    .unwrap();
                assert_eq!(effect["memberId"], EFFECT);
                assert_eq!(
                    effect["at"],
                    json!({"timeline":"main","tick":tick,"order":"-7"})
                );
                assert_eq!(effect["citation"]["section"], "effects[0]");
            }
            let expected = envelope(&world, native);
            assert_eq!(
                service.execute(operation(mode, &selection, 100)).unwrap(),
                expected
            );
            let expected = wire(&expected);
            let order = order.to_string();
            assert_eq!(
                ok(&cli(
                    &world,
                    mode,
                    100,
                    &["--tick", &tick, "--order", &order, "--timeline", timeline]
                )),
                expected
            );
            assert_eq!(
                get(
                    service.clone(),
                    &format!("limit=100&mode={mode}&tick={tick}&order={order}&timeline={timeline}")
                )
                .await,
                (StatusCode::OK, expected)
            );
        }
    }
    assert_eq!(source_image(&world), source_before);
    assert!(!world.root.join(".wedl-rust").exists());
}

#[tokio::test]
async fn structured_search_preserves_existing_default_order_and_explicit_thread_grouping() {
    let (_directory, world) = fixture("transportneedle");
    let service = Service::new(world.clone());
    let tick = TICK.to_string();
    for mode in MODES {
        for thread in [T1, T2] {
            let selection = selection(i32::MAX, "main", Some(vec![thread.into()]));
            let native = compiler::search_with_selection(
                &world,
                "transportneedle",
                100,
                Some("event"),
                mode,
                &selection,
            )
            .unwrap();
            assert_eq!(
                !native["items"].as_array().unwrap().is_empty(),
                thread == T1
            );
            let expected = wire(&envelope(&world, native));
            assert_eq!(
                ok(&cli(
                    &world,
                    mode,
                    100,
                    &["--tick", &tick, "--thread", thread]
                )),
                expected
            );
            assert_eq!(
                get(
                    service.clone(),
                    &format!(
                        "limit=100&mode={mode}&tick={tick}&threads={}",
                        encode(&json!([thread]).to_string())
                    )
                )
                .await,
                (StatusCode::OK, expected)
            );
        }
    }
}

#[tokio::test]
async fn transports_do_not_infer_an_author_horizon_or_extend_the_all_time_corpus() {
    let (_directory, world) = fixture("transportneedle");
    let service = Service::new(world.clone());
    for mode in MODES {
        let native =
            compiler::search_with_mode(&world, "transportneedle", 100, Some("event"), mode)
                .unwrap();
        assert_eq!(native["items"], json!([]));
        let expected = wire(&envelope(&world, native));
        assert_eq!(ok(&cli(&world, mode, 100, &[])), expected);
        assert_eq!(
            get(service.clone(), &format!("limit=100&mode={mode}")).await,
            (StatusCode::OK, expected)
        );
    }
}

#[tokio::test]
async fn structured_input_bounds_fail_across_transports_before_small_limits_and_empty_threads() {
    let (_directory, world) = fixture(&"transportneedle ".repeat(40_000));
    let source_before = source_image(&world);
    let service = Service::new(world.clone());
    let tick = TICK.to_string();
    for mode in MODES {
        let selection = selection(-7, "main", Some(vec![T2.into()]));
        let expected = compiler::search_with_selection(
            &world,
            "transportneedle",
            1,
            Some("event"),
            mode,
            &selection,
        )
        .unwrap_err()
        .json();
        assert_eq!(expected["code"], "usage_error");
        assert!(expected["message"].as_str().unwrap().contains("bounds"));
        assert_eq!(
            service
                .execute(operation(mode, &selection, 1))
                .unwrap_err()
                .json(),
            expected
        );
        let output = cli(
            &world,
            mode,
            1,
            &["--tick", &tick, "--order", "-7", "--thread", T2],
        );
        assert!(!output.status.success());
        assert_eq!(
            serde_json::from_slice::<Json>(&output.stderr).unwrap(),
            expected
        );
        assert_eq!(
            get(
                service.clone(),
                &format!(
                    "limit=1&mode={mode}&tick={tick}&order=-7&threads={}",
                    encode(&json!([T2]).to_string())
                )
            )
            .await,
            (StatusCode::BAD_REQUEST, expected)
        );
    }
    assert_eq!(source_image(&world), source_before);
    assert!(!world.root.join(".wedl-rust").exists());
}
