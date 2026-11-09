//! Explicit author horizons and thread grouping share exact native transport results.
#[path = "support/actions.rs"]
mod support;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value as Json, json};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output},
};
use tower::ServiceExt;
use wedl::{
    compiler::{self, AuthorSearchSelection},
    http,
    model::{StoryTime, World},
    retrieval::Mode,
    service::{Operation, Service, TimeInput},
    source, validation,
};

const T1: &str = "thread_00000000000000000000000001";
const T2: &str = "thread_00000000000000000000000002";
const T3: &str = "thread_00000000000000000000000003";
const MODES: [Mode; 3] = [Mode::Lexical, Mode::Semantic, Mode::Hybrid];

fn write(world: &World, kind: &str, id: &str, title: &str, fields: Json, body: &str) {
    let mut fields = fields.as_object().unwrap().clone();
    fields.extend(
        json!({"schema":"wedl/v1","kind":kind,"id":id,"title":title})
            .as_object()
            .unwrap()
            .clone(),
    );
    fs::write(
        world.root.join(format!("story/{id}.md")),
        format!(
            "---\n{}---\n{body}",
            serde_yaml::to_string(&fields).unwrap()
        ),
    )
    .unwrap();
}

fn fixture() -> (tempfile::TempDir, World) {
    let (directory, world) = support::fixture();
    let owner = world.world_record().unwrap();
    let mut fields = owner.frontmatter.clone();
    fields["threads"] = serde_yaml::to_value(json!([
        {"id":T1,"label":"Harbor"},{"id":T2,"label":"Journey"},{"id":T3,"label":"Unassigned"}
    ]))
    .unwrap();
    fields["timelines"] = serde_yaml::to_value(json!([
        {"id":"main","label":"Main"},{"id":"other","label":"Other"}
    ]))
    .unwrap();
    fs::write(
        world.root.join(&owner.source_path),
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&fields).unwrap(),
            owner.body
        ),
    )
    .unwrap();
    for (number, title, body, threads) in [
        (1, "Pier", "harbor pier vessel", vec![T1]),
        (2, "Ship", "ship harbor vessel", vec![T2]),
        (3, "Library", "library book scroll harbor", vec![T1, T2]),
    ] {
        write(
            &world,
            "object",
            &format!("obj_{number:026}"),
            title,
            json!({"threads":threads}),
            body,
        );
    }
    write(
        &world,
        "event",
        "event_00000000000000000000000001",
        "Future identity",
        json!({
            "time":{"timeline":"main","tick":support::TICK,"order":2},"threads":[T1]
        }),
        "futureharbor vessel summary",
    );
    let world = source::load_world(&world.root, None).unwrap();
    let checked = validation::validate_world(&world);
    assert!(checked.valid, "{:?}", checked.diagnostics);
    (directory, world)
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

async fn get(service: Service, query: &str) -> (StatusCode, Json) {
    let response = http::router(service)
        .oneshot(
            Request::builder()
                .uri(format!("/api/search?{query}"))
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

fn cli(world: &World, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .arg("--repo")
        .arg(&world.root)
        .arg("--compact")
        .args(args)
        .output()
        .unwrap()
}

fn output_json(output: &Output) -> Json {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn envelope(world: &World, matches: Json) -> Json {
    json!({"protocol":wedl::CONTRACT,"fingerprint":world.fingerprint,"revision":world.revision,
        "result":{"scope":matches["scope"],"matches":matches}})
}

fn wire(value: &Json) -> Json {
    // Compare transports after the same JSON parse. The default serde_json
    // float decoder can round a BM25/LSA score by one ULP; native/service
    // equality above the wire remains exact.
    serde_json::from_slice(&serde_json::to_vec(value).unwrap()).unwrap()
}

fn operation(mode: Mode, at: Option<StoryTime>, threads: Option<Vec<String>>) -> Operation {
    Operation::SearchWithSelection {
        query: "harbor".into(),
        limit: 100,
        kind: Some("object".into()),
        mode,
        at: at.map(|at| TimeInput {
            tick: at.tick,
            order: at.order,
            timeline: Some(at.timeline),
        }),
        thread_ids: threads,
    }
}

fn image(root: &Path) -> BTreeMap<String, (Vec<u8>, std::time::SystemTime)> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .map(Result::unwrap)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| {
            (
                entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                (
                    fs::read(entry.path()).unwrap(),
                    fs::metadata(entry.path()).unwrap().modified().unwrap(),
                ),
            )
        })
        .collect()
}

#[tokio::test]
async fn author_selection_matches_native_service_cli_and_http_in_every_mode() {
    let (_directory, world) = fixture();
    let service = Service::new(world.clone());
    let tick = support::TICK.to_string();
    for mode in MODES {
        for (has_time, has_threads) in [(true, false), (false, true), (true, true)] {
            let at = has_time.then(|| StoryTime::new("main", support::TICK, -7).unwrap());
            let threads = has_threads.then(|| vec![T1.to_owned(), T2.to_owned()]);
            let selection = AuthorSearchSelection {
                at: at.clone(),
                thread_ids: threads.clone(),
            };
            let expected = envelope(
                &world,
                compiler::search_with_selection(
                    &world,
                    "harbor",
                    100,
                    Some("object"),
                    mode,
                    &selection,
                )
                .unwrap(),
            );
            assert_eq!(
                service.execute(operation(mode, at, threads)).unwrap(),
                expected
            );
            let expected = wire(&expected);
            let mut args = vec![
                "search",
                "harbor",
                "--kind",
                "object",
                "--limit",
                "100",
                "--mode",
                mode.as_str(),
            ];
            let mut query = format!("q=harbor&kind=object&limit=100&mode={mode}");
            if has_time {
                args.extend(["--tick", &tick, "--order", "-7", "--timeline", "main"]);
                query.push_str(&format!("&tick={tick}&order=-7&timeline=main"));
                assert_eq!(expected["result"]["matches"]["at"]["tick"], tick);
            }
            if has_threads {
                // The CLI normalizes order, while every shared operation still
                // rejects duplicates instead of silently deduplicating them.
                args.extend(["--thread", T2, "--thread", T1]);
                query.push_str(&format!(
                    "&threads={}",
                    encode(&json!([T1, T2]).to_string())
                ));
                assert_eq!(expected["result"]["matches"]["threadIds"], json!([T1, T2]));
            }
            assert_eq!(
                output_json(&cli(&world, &args)),
                expected,
                "CLI {mode} time={has_time} threads={has_threads}"
            );
            assert_eq!(
                get(service.clone(), &query).await,
                (StatusCode::OK, expected),
                "HTTP {mode} time={has_time} threads={has_threads}"
            );
        }
    }
}

#[tokio::test]
async fn omitted_selection_keeps_legacy_author_search_routes_and_results_unchanged() {
    let (_directory, world) = fixture();
    let service = Service::new(world.clone());
    for mode in MODES {
        let expected = envelope(
            &world,
            compiler::search_with_mode(&world, "harbor", 100, Some("object"), mode).unwrap(),
        );
        let direct = service
            .execute(Operation::SearchWithMode {
                query: "harbor".into(),
                limit: 100,
                kind: Some("object".into()),
                mode,
            })
            .unwrap();
        assert_eq!(direct, expected);
        assert_eq!(
            service.execute(operation(mode, None, None)).unwrap(),
            expected
        );
        let expected = wire(&expected);
        assert_eq!(
            output_json(&cli(
                &world,
                &[
                    "search",
                    "harbor",
                    "--kind",
                    "object",
                    "--limit",
                    "100",
                    "--mode",
                    mode.as_str()
                ]
            )),
            expected
        );
        assert_eq!(
            get(
                service.clone(),
                &format!("q=harbor&kind=object&limit=100&mode={mode}")
            )
            .await,
            (StatusCode::OK, expected)
        );
    }
    let expected = envelope(
        &world,
        compiler::search(&world, "harbor", 100, Some("object")).unwrap(),
    );
    assert_eq!(
        service
            .execute(Operation::Search {
                query: "harbor".into(),
                limit: 100,
                kind: Some("object".into())
            })
            .unwrap(),
        expected
    );
    let expected = wire(&expected);
    assert_eq!(
        output_json(&cli(
            &world,
            &["search", "harbor", "--kind", "object", "--limit", "100"]
        )),
        expected
    );
    assert_eq!(
        get(service, "q=harbor&kind=object&limit=100").await,
        (StatusCode::OK, expected)
    );
}

#[tokio::test]
async fn selected_horizons_preserve_full_integer_ranges_and_existing_tick_defaults() {
    let (_directory, world) = fixture();
    let service = Service::new(world.clone());
    for (tick, order, timeline) in [
        (i64::MIN, i32::MIN, "main"),
        (i64::MAX, i32::MAX, "main"),
        (support::TICK, 1, "other"),
    ] {
        let at = StoryTime::new(timeline, tick, order).unwrap();
        let selected = AuthorSearchSelection {
            at: Some(at.clone()),
            thread_ids: None,
        };
        let expected = envelope(
            &world,
            compiler::search_with_selection(
                &world,
                "harbor",
                100,
                Some("object"),
                Mode::Lexical,
                &selected,
            )
            .unwrap(),
        );
        assert_eq!(
            service
                .execute(operation(Mode::Lexical, Some(at), None))
                .unwrap(),
            expected
        );
        let expected = wire(&expected);
        let tick = tick.to_string();
        let order = order.to_string();
        assert_eq!(
            output_json(&cli(
                &world,
                &[
                    "search",
                    "harbor",
                    "--kind",
                    "object",
                    "--limit",
                    "100",
                    "--tick",
                    &tick,
                    "--order",
                    &order,
                    "--timeline",
                    timeline
                ]
            )),
            expected
        );
        assert_eq!(
            get(
                service.clone(),
                &format!(
                    "q=harbor&kind=object&limit=100&tick={tick}&order={order}&timeline={timeline}"
                )
            )
            .await,
            (StatusCode::OK, expected)
        );
    }
    let tick = support::TICK.to_string();
    let expected = envelope(
        &world,
        compiler::search_with_selection(
            &world,
            "harbor",
            100,
            Some("object"),
            Mode::Lexical,
            &AuthorSearchSelection {
                at: Some(StoryTime::new("main", support::TICK, i32::MAX).unwrap()),
                thread_ids: None,
            },
        )
        .unwrap(),
    );
    let expected = wire(&expected);
    assert_eq!(
        output_json(&cli(
            &world,
            &[
                "search", "harbor", "--kind", "object", "--limit", "100", "--tick", &tick
            ]
        )),
        expected
    );
    assert_eq!(
        get(
            service,
            &format!("q=harbor&kind=object&limit=100&tick={tick}")
        )
        .await,
        (StatusCode::OK, expected)
    );
}

#[tokio::test]
async fn http_author_selection_is_closed_and_rejects_nonexact_threads_and_bad_horizons() {
    let (_directory, world) = fixture();
    let service = Service::new(world.clone());
    let mut bad = vec![
        "q=harbor&order=0".to_owned(),
        "q=harbor&timeline=main".into(),
        "q=harbor&tick=".into(),
        "q=harbor&tick=1.0".into(),
        "q=harbor&tick=9223372036854775808".into(),
        "q=harbor&tick=-9223372036854775809".into(),
        "q=harbor&tick=0&order=2147483648".into(),
        "q=harbor&tick=0&order=-2147483649".into(),
        "q=harbor&tick=0&order=1.5".into(),
        "q=harbor&tick=0&timeline=absent".into(),
        "q=harbor&tick=0&timeline=".into(),
        "q=harbor&tick=0&tick=1".into(),
        "q=harbor&tick=0&%74ick=1".into(),
        "q=harbor&at=0".into(),
        "q=harbor&all_time=true".into(),
        "q=harbor&character=any".into(),
        format!("q=harbor&thread={T1}"),
        "q=harbor&known_ids=[]".into(),
        "q=harbor&tick=0&mode=unknown".into(),
        "q=harbor&tick=0&limit=0".into(),
        "q=harbor&tick=0&limit=101".into(),
        "q=&tick=0".into(),
    ];
    for raw in [
        "null".to_owned(),
        "{}".into(),
        "[]".into(),
        "[true]".into(),
        "[1]".into(),
        "[null]".into(),
        json!(T1).to_string(),
        json!([T1, T1]).to_string(),
        json!([T2, T1]).to_string(),
        json!(["Harbor"]).to_string(),
        json!(["thread_0000000000000000000000000a"]).to_string(),
        json!(["thread_00000000000000000000000009"]).to_string(),
        json!([format!(" {T1}")]).to_string(),
        json!(vec![T1; 33]).to_string(),
        format!("[\"{}\"]", "x".repeat(16_384)),
    ] {
        bad.push(format!("q=harbor&threads={}", encode(&raw)));
    }
    let selected = encode(&json!([T1]).to_string());
    bad.push(format!("q=harbor&threads={selected}&threads={selected}"));
    bad.push(format!("q=harbor&threads={selected}&%74hreads={selected}"));
    for query in bad {
        let (status, error) = get(service.clone(), &query).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {error}");
        assert!(error.get("result").is_none(), "{error}");
    }
}

#[test]
fn repeated_cli_threads_are_sorted_but_duplicates_and_partial_horizons_are_rejected() {
    let (_directory, world) = fixture();
    for args in [
        vec!["search", "harbor", "--thread", T1, "--thread", T1],
        vec!["search", "harbor", "--thread", "Harbor"],
        vec![
            "search",
            "harbor",
            "--thread",
            "thread_00000000000000000000000009",
        ],
        vec!["search", "harbor", "--order", "0"],
        vec!["search", "harbor", "--timeline", "main"],
        vec!["search", "harbor", "--tick", "9223372036854775808"],
        vec!["search", "harbor", "--tick", "0", "--order", "2147483648"],
    ] {
        let output = cli(&world, &args);
        assert!(!output.status.success(), "{args:?}");
        assert!(
            output.stdout.is_empty(),
            "unexpected partial output for {args:?}"
        );
    }
}

#[tokio::test]
async fn selected_transports_are_read_only_without_consulting_private_sources_or_fts_cache() {
    let (_directory, world) = fixture();
    fs::create_dir_all(world.root.join("author/works")).unwrap();
    fs::write(
        world
            .root
            .join("author/works/work_00000000000000000000000001.md"),
        "Malformed private source must not enter author search\n",
    )
    .unwrap();
    let service = Service::new(world.clone());
    let tick = support::TICK.to_string();
    let threads = encode(&json!([T1, T2]).to_string());
    let before = image(&world.root);
    for mode in MODES {
        let expected = service
            .execute(operation(
                mode,
                Some(StoryTime::new("main", support::TICK, 1).unwrap()),
                Some(vec![T1.into(), T2.into()]),
            ))
            .unwrap();
        let expected = wire(&expected);
        assert_eq!(get(service.clone(),&format!("q=harbor&kind=object&limit=100&mode={mode}&tick={tick}&order=1&threads={threads}")).await,(StatusCode::OK,expected.clone()));
        assert_eq!(
            output_json(&cli(
                &world,
                &[
                    "search",
                    "harbor",
                    "--kind",
                    "object",
                    "--limit",
                    "100",
                    "--mode",
                    mode.as_str(),
                    "--tick",
                    &tick,
                    "--order",
                    "1",
                    "--thread",
                    T1,
                    "--thread",
                    T2
                ]
            )),
            expected
        );
    }
    assert_eq!(image(&world.root), before);
    assert!(!world.root.join(".wedl-rust").exists());
    let compiled = compiler::compile(&world).unwrap();
    fs::write(
        &compiled.path,
        b"unusable compiled cache; selected search is request-local\n",
    )
    .unwrap();
    let before = image(&world.root);
    for mode in MODES {
        let expected = service
            .execute(operation(
                mode,
                Some(StoryTime::new("main", support::TICK, 1).unwrap()),
                None,
            ))
            .unwrap();
        let expected = wire(&expected);
        assert_eq!(
            get(
                service.clone(),
                &format!("q=harbor&kind=object&limit=100&mode={mode}&tick={tick}&order=1")
            )
            .await,
            (StatusCode::OK, expected.clone())
        );
        assert_eq!(
            output_json(&cli(
                &world,
                &[
                    "search",
                    "harbor",
                    "--kind",
                    "object",
                    "--limit",
                    "100",
                    "--mode",
                    mode.as_str(),
                    "--tick",
                    &tick,
                    "--order",
                    "1"
                ]
            )),
            expected
        );
    }
    assert_eq!(image(&world.root), before);
}
