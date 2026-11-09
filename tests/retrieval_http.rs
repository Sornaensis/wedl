//! Explicit retrieval modes preserve lexical compatibility and character admission.
#[path = "support/retrieval.rs"]
mod fixture;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value as Json, json};
use std::{collections::BTreeMap, fs, process::Command};
use tower::ServiceExt;
use wedl::{
    character, compiler, http,
    model::World,
    retrieval::Mode,
    service::{Operation, Service, TimeInput},
    source, validation,
};

struct Fixture(tempfile::TempDir);
impl Fixture {
    fn new(world: &World) -> Self {
        let f = Self(fixture::save(world));
        let report = validation::validate_world(&f.world());
        assert!(report.valid, "{:?}", report.diagnostics);
        f
    }
    fn world(&self) -> World {
        source::load_world(self.0.path(), None).unwrap()
    }
    fn bytes(&self) -> BTreeMap<String, Vec<u8>> {
        self.world()
            .records
            .values()
            .map(|r| {
                (
                    r.source_path.clone(),
                    fs::read(self.0.path().join(&r.source_path)).unwrap(),
                )
            })
            .collect()
    }
    fn cache_exists(&self) -> bool {
        self.0.path().join(".wedl-rust").exists()
    }
}
fn time() -> TimeInput {
    TimeInput {
        tick: fixture::TICK,
        order: 0,
        timeline: Some("main".into()),
    }
}
fn name(mode: Mode) -> &'static str {
    match mode {
        Mode::Lexical => "lexical",
        Mode::Semantic => "semantic",
        Mode::Hybrid => "hybrid",
    }
}
fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn uri(character: bool, query: &str, mode: Option<&str>, limit: usize) -> String {
    let mut result = format!(
        "/api/{}?q={}&limit={limit}",
        if character {
            "character/search"
        } else {
            "search"
        },
        encode(query)
    );
    if character {
        result.push_str(&format!(
            "&character={}&tick={}&order=0&timeline=main",
            fixture::VIEWER,
            fixture::TICK
        ));
    }
    if let Some(mode) = mode {
        result.push_str(&format!("&mode={}", encode(mode)));
    }
    result
}
async fn get(service: Service, uri: &str) -> (StatusCode, Json) {
    let response = http::router(service)
        .oneshot(
            Request::builder()
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
fn cli(
    f: &Fixture,
    character: bool,
    query: &str,
    mode: Option<&str>,
    limit: usize,
    extra: &[&str],
) -> (bool, Json) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_wedl-rs"));
    command.arg("--repo").arg(f.0.path());
    if character {
        command.args([
            "character",
            "search",
            fixture::VIEWER,
            query,
            "--tick",
            &fixture::TICK.to_string(),
            "--order",
            "0",
            "--timeline",
            "main",
        ]);
    } else {
        command.args(["search", query]);
    }
    if let Some(mode) = mode {
        command.args(["--mode", mode]);
    }
    command.args(["--limit", &limit.to_string()]).args(extra);
    let output = command.output().unwrap();
    let bytes = if output.status.success() {
        &output.stdout
    } else {
        &output.stderr
    };
    (
        output.status.success(),
        serde_json::from_slice(bytes)
            .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(bytes))),
    )
}
fn envelope(world: &World, character: bool, native: Json) -> Json {
    if character {
        json!({"protocol":wedl::CONTRACT,"result":native})
    } else {
        json!({"protocol":wedl::CONTRACT,"fingerprint":world.fingerprint,"revision":world.revision,
        "result":{"scope":"author-all-time","matches":native}})
    }
}
fn result(value: &Json, character: bool) -> &Json {
    if character {
        &value["result"]
    } else {
        &value["result"]["matches"]
    }
}
// Compare the actual serialization boundary: the existing serde_json float
// parser can round an emitted cosine by one ulp without its float_roundtrip
// feature. Native/service values are still compared directly, with no tolerance.
fn wire(value: &Json) -> Json {
    serde_json::from_slice(&serde_json::to_vec(value).unwrap()).unwrap()
}

fn safe_character(value: &Json) {
    if let Some(fields) = value.as_object() {
        for (key, child) in fields {
            assert!(
                ![
                    "fingerprint",
                    "revision",
                    "blobOid",
                    "sourcePath",
                    "sourceField",
                    "frontmatterYaml",
                    "rawSource",
                    "provenance",
                    "title",
                    "truth"
                ]
                .contains(&key.as_str()),
                "leaked {key}"
            );
            safe_character(child);
        }
    }
    if let Some(items) = value.as_array() {
        for item in items {
            safe_character(item);
        }
    }
}

#[tokio::test]
async fn three_modes_match_native_service_cli_and_http_with_exact_character_time() {
    let f = Fixture::new(&fixture::world());
    let before = f.bytes();
    let world = f.world();
    let service = Service::new(world.clone());
    for character in [false, true] {
        for mode in [Mode::Lexical, Mode::Semantic, Mode::Hybrid] {
            let native = if character {
                character::search_with_mode(
                    &world,
                    fixture::VIEWER,
                    "ferry",
                    &fixture::horizon(),
                    20,
                    mode,
                )
                .unwrap()
            } else {
                compiler::search_with_mode(&world, "ferry", 20, None, mode).unwrap()
            };
            let expected = envelope(&world, character, native);
            let operation = if character {
                Operation::CharacterSearchWithMode {
                    character: fixture::VIEWER.into(),
                    query: "ferry".into(),
                    at: time(),
                    limit: 20,
                    mode,
                }
            } else {
                Operation::SearchWithMode {
                    query: "ferry".into(),
                    limit: 20,
                    kind: None,
                    mode,
                }
            };
            assert_eq!(service.execute(operation).unwrap(), expected);
            assert_eq!(
                cli(&f, character, "ferry", Some(name(mode)), 20, &[]),
                (true, wire(&expected))
            );
            assert_eq!(
                get(
                    service.clone(),
                    &uri(character, "ferry", Some(name(mode)), 20)
                )
                .await,
                (StatusCode::OK, wire(&expected))
            );
            let data = result(&expected, character);
            assert!(!data["items"].as_array().unwrap().is_empty());
            if mode == Mode::Lexical {
                assert!(data.get("mode").is_none());
                assert!(data.get("retrieval").is_none());
            } else {
                assert_eq!(data["mode"], name(mode));
                assert!(data["retrieval"].is_object());
                for item in data["items"].as_array().unwrap() {
                    assert!(item["score"].as_f64().unwrap().is_finite());
                    for key in [
                        "lexicalRank",
                        "lexicalScore",
                        "semanticRank",
                        "semanticScore",
                    ] {
                        assert!(item.get(key).is_some(), "{key}");
                    }
                    if let Some(score) = item["semanticScore"].as_f64() {
                        assert!(score.is_finite() && (-1.000000001..=1.000000001).contains(&score));
                    }
                    if mode == Mode::Hybrid {
                        assert!(item["score"].as_f64().unwrap() > 0.0);
                    }
                }
            }
            if character {
                safe_character(&expected);
                assert_eq!(data["effectiveTime"]["tick"], fixture::TICK.to_string());
                assert_eq!(data["effectiveTime"]["order"], "0");
            }
        }
    }
    assert_eq!(before, f.bytes());
}

#[tokio::test]
async fn omitted_and_explicit_lexical_mode_preserve_the_complete_old_wire_contract() {
    let f = Fixture::new(&fixture::world());
    let world = f.world();
    let service = Service::new(world.clone());
    for character in [false, true] {
        let native = if character {
            character::search(&world, fixture::VIEWER, "ferry", &fixture::horizon(), 2).unwrap()
        } else {
            compiler::search(&world, "ferry", 2, None).unwrap()
        };
        let expected = envelope(&world, character, native);
        let operation = if character {
            Operation::CharacterSearch {
                character: fixture::VIEWER.into(),
                query: "ferry".into(),
                at: time(),
                limit: 2,
            }
        } else {
            Operation::Search {
                query: "ferry".into(),
                limit: 2,
                kind: None,
            }
        };
        assert_eq!(service.execute(operation).unwrap(), expected);
        for mode in [None, Some("lexical")] {
            assert_eq!(
                get(service.clone(), &uri(character, "ferry", mode, 2)).await,
                (StatusCode::OK, wire(&expected))
            );
            assert_eq!(
                cli(&f, character, "ferry", mode, 2, &[]),
                (true, wire(&expected))
            );
        }
    }
}

#[tokio::test]
async fn explicit_modes_have_closed_parameters_and_retain_literal_query_and_limit_validation() {
    let f = Fixture::new(&fixture::world());
    let service = Service::new(f.world());
    for character in [false, true] {
        for bad in [
            "",
            "Semantic",
            "SEMANTIC",
            " semantic",
            "hybrid ",
            "vector",
            "lsa",
        ] {
            let (status, error) =
                get(service.clone(), &uri(character, "ferry", Some(bad), 20)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}: {error}");
            assert_eq!(error["code"], "usage_error");
            assert!(error.get("result").is_none());
            assert!(!cli(&f, character, "ferry", Some(bad), 20, &[]).0);
        }
        for mode in ["lexical", "semantic", "hybrid"] {
            let base = uri(character, "ferry", Some(mode), 20);
            for extra in [
                "mode=semantic",
                "%6Dode=semantic",
                "model=remote",
                "provider=local",
                "dimensions=64",
                "minScore=0.5",
                "audience=public",
                "view=ordinary",
                "scene=scene_00000000000000000000000001",
                "knownIds=%5B%5D",
                "families=%7B%7D",
                "frame=%7B%7D",
            ] {
                assert_eq!(
                    get(service.clone(), &format!("{base}&{extra}")).await.0,
                    StatusCode::BAD_REQUEST,
                    "{extra}"
                );
            }
            for (query, limit) in [
                ("".to_owned(), 20),
                ("  ".into(), 20),
                ("x".repeat(4097), 20),
                ("é".repeat(2049), 20),
                (vec!["ferry"; 65].join(" "), 20),
                ("ferry".into(), 0),
                ("ferry".into(), 101),
            ] {
                assert_eq!(
                    get(service.clone(), &uri(character, &query, Some(mode), limit))
                        .await
                        .0,
                    StatusCode::BAD_REQUEST
                );
            }
            if character {
                assert_eq!(
                    get(service.clone(), &format!("{base}&kind=knowledge"))
                        .await
                        .0,
                    StatusCode::BAD_REQUEST
                );
                for missing in ["character", "tick"] {
                    let removed = base
                        .split('&')
                        .filter(|part| !part.starts_with(&format!("{missing}=")))
                        .collect::<Vec<_>>()
                        .join("&");
                    assert_eq!(
                        get(service.clone(), &removed).await.0,
                        StatusCode::BAD_REQUEST
                    );
                }
            } else {
                assert_eq!(
                    get(service.clone(), &format!("{base}&order=0")).await.0,
                    StatusCode::BAD_REQUEST
                );
            }
        }
    }
}

#[tokio::test]
async fn author_kind_filter_is_exact_and_new_modes_never_build_persistent_indexes() {
    let f = Fixture::new(&fixture::world());
    let before = f.bytes();
    let world = f.world();
    let service = Service::new(world.clone());
    assert!(!f.cache_exists());
    for mode in [Mode::Semantic, Mode::Hybrid] {
        let native =
            compiler::search_with_mode(&world, "ferry", 20, Some("knowledge"), mode).unwrap();
        for item in native["items"].as_array().unwrap() {
            assert_eq!(item["kind"], "knowledge");
        }
        let expected = envelope(&world, false, native);
        assert_eq!(
            service
                .execute(Operation::SearchWithMode {
                    query: "ferry".into(),
                    limit: 20,
                    kind: Some("knowledge".into()),
                    mode
                })
                .unwrap(),
            expected
        );
        assert_eq!(
            cli(
                &f,
                false,
                "ferry",
                Some(name(mode)),
                20,
                &["--kind", "knowledge"]
            ),
            (true, wire(&expected))
        );
        let path = format!(
            "{}&kind=knowledge",
            uri(false, "ferry", Some(name(mode)), 20)
        );
        assert_eq!(
            get(service.clone(), &path).await,
            (StatusCode::OK, wire(&expected))
        );
        assert!(!f.cache_exists());
        assert_eq!(
            get(service.clone(), &uri(true, "ferry", Some(name(mode)), 20))
                .await
                .0,
            StatusCode::OK
        );
        assert!(!f.cache_exists());
    }
    assert_eq!(before, f.bytes());
}

#[tokio::test]
async fn private_future_edits_leave_full_character_rankings_totals_scores_and_snippets_unchanged() {
    let original = fixture::world();
    let f = Fixture::new(&original);
    let altered = Fixture::new(&fixture::private_edit(&original));
    let world = f.world();
    let changed = altered.world();
    assert_ne!(world.fingerprint, changed.fingerprint);
    let before = f.bytes();
    let after = altered.bytes();
    for mode in ["lexical", "semantic", "hybrid"] {
        for query in ["ferry", "vessel", "zxqvunknownterm"] {
            for limit in [1, 20] {
                let path = uri(true, query, Some(mode), limit);
                let expected = get(Service::new(world.clone()), &path).await;
                assert_eq!(expected.0, StatusCode::OK);
                safe_character(&expected.1);
                assert_eq!(get(Service::new(changed.clone()), &path).await, expected);
                assert_eq!(
                    cli(&f, true, query, Some(mode), limit, &[]),
                    cli(&altered, true, query, Some(mode), limit, &[])
                );
            }
        }
    }
    assert_eq!(f.bytes(), before);
    assert_eq!(altered.bytes(), after);
    assert!(!f.cache_exists());
    assert!(!altered.cache_exists());
}

#[tokio::test]
async fn missing_character_prior_is_an_explicit_error_and_does_not_fall_back_to_lexical() {
    let f = Fixture::new(&fixture::without_prior(&fixture::world()));
    let world = f.world();
    let service = Service::new(world.clone());
    let (status, lexical) = get(service.clone(), &uri(true, "ferry", None, 20)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!lexical["result"]["items"].as_array().unwrap().is_empty());
    for mode in [Mode::Semantic, Mode::Hybrid] {
        let native = character::search_with_mode(
            &world,
            fixture::VIEWER,
            "ferry",
            &fixture::horizon(),
            20,
            mode,
        )
        .unwrap_err()
        .json();
        let (status, error) = get(service.clone(), &uri(true, "ferry", Some(name(mode)), 20)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error, native);
        assert!(error.get("result").is_none());
        assert_eq!(
            cli(&f, true, "ferry", Some(name(mode)), 20, &[]),
            (false, error)
        );
    }
    assert!(!f.cache_exists());
}

#[tokio::test]
async fn semantic_only_matches_and_oov_empty_results_are_reported_without_scope_expansion() {
    let f = Fixture::new(&fixture::world());
    let service = Service::new(f.world());
    let (_, lexical) = get(service.clone(), &uri(true, "vessel", None, 20)).await;
    assert_eq!(lexical["result"]["total"], 0);
    for mode in ["semantic", "hybrid"] {
        let (status, value) = get(service.clone(), &uri(true, "vessel", Some(mode), 20)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(value["result"]["total"].as_u64().unwrap() > 0);
        safe_character(&value);
        for item in value["result"]["items"].as_array().unwrap() {
            assert!(item["semanticRank"].as_u64().is_some());
            assert!(item["lexicalRank"].is_null());
        }
        let (status, empty) = get(
            service.clone(),
            &uri(true, "zxqvunknownterm", Some(mode), 20),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(empty["result"]["total"], 0);
        assert_eq!(empty["result"]["items"], json!([]));
        assert_eq!(empty["result"]["mode"], mode);
        safe_character(&empty);
    }
}

#[tokio::test]
async fn invalid_source_errors_remain_generic_on_every_character_mode() {
    let original = fixture::world();
    let broken = fixture::set(
        &original,
        fixture::LINKED_ASSERTION,
        "predicate",
        json!("private-unknown-predicate"),
    );
    let f = Fixture(fixture::save(&broken));
    let world = f.world();
    assert!(!validation::validate_world(&world).valid);
    let service = Service::new(world);
    let mut expected = None;
    for mode in ["lexical", "semantic", "hybrid"] {
        let response = get(service.clone(), &uri(true, "ferry", Some(mode), 20)).await;
        assert_eq!(response.0, StatusCode::BAD_REQUEST);
        assert_eq!(response.1["code"], "validation_error");
        assert!(response.1.get("result").is_none());
        assert!(!response.1.to_string().contains("private-unknown-predicate"));
        assert!(!response.1.to_string().contains(fixture::LINKED_ASSERTION));
        assert_eq!(
            cli(&f, true, "ferry", Some(mode), 20, &[]),
            (false, response.1.clone())
        );
        if let Some(expected) = &expected {
            assert_eq!(&response, expected);
        } else {
            expected = Some(response);
        }
    }
    assert!(!f.cache_exists());
}
