//! Character narrative transports admit the same evidence without source metadata.
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{fs, path::Path, process::Command};
use tower::ServiceExt;
use wedl::{
    http, learned_assertions,
    model::{StoryTime, World},
    narrative,
    service::{Operation, Service, TimeInput},
    source, validation,
};

const TICK: i64 = 9_007_199_254_740_993;
const VIEWER: &str = "char_00000000000000000000000001";
const OTHER: &str = "char_00000000000000000000000002";
const PLACE: &str = "loc_00000000000000000000000001";
const UNKNOWN_PLACE: &str = "loc_00000000000000000000000002";
const CONCEPT: &str = "concept_00000000000000000000000001";
const UNKNOWN_CONCEPT: &str = "concept_00000000000000000000000002";
const PRIVATE_CONCEPT: &str = "concept_00000000000000000000000003";
const ASSERTION: &str = "assertion_00000000000000000000000001";
const KNOWLEDGE: &str = "know_00000000000000000000000001";

fn id(prefix: &str, n: usize) -> String {
    format!("{prefix}_{n:026}")
}
fn point(order: i32) -> StoryTime {
    StoryTime::new("main", TICK, order).unwrap()
}
fn time(order: i32) -> TimeInput {
    TimeInput {
        tick: TICK,
        order,
        timeline: Some("main".into()),
    }
}
fn interval() -> Json {
    json!({"kind":"interval","timeline":"main","start":{"timeline":"main","tick":TICK,"order":0},
        "end":{"timeline":"main","tick":TICK,"order":2}})
}
fn reference(id: &str) -> Json {
    json!({"kind":"record","id":id})
}
fn transition(id: &str, tick: i64, order: i32, state: &str) -> Json {
    json!({"id":id,"time":{"timeline":"main","tick":tick,"order":order},"state":state,
        "confidence":0.75,"acquisition":"told"})
}
fn section(n: usize, author: bool) -> Json {
    json!({"id":id("description",n),"heading":format!("Account {n}"),
        "markdown":format!("The shared harbor bell, account {n}."),"valid":interval(),
        "audience":[if author{"author"}else{"public"}],
        "perspectives":[if author{"author"}else{"ordinary"}]})
}

struct Fixture(tempfile::TempDir);
impl Fixture {
    fn new() -> Self {
        let f = Self(tempfile::tempdir().unwrap());
        fs::create_dir(f.root().join("story")).unwrap();
        f.write("world",&id("world",0),"world",json!({"default_timeline":"main",
            "timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],
            "capabilities":["generational-core-v1","spatial-core-v1","lore-core-v1","descriptions-v1",
                "assertions-v1","existence-v1","collective-parties-v1","spatial-history-v1"],
            "profiles":[{"id":"admission-note","label":"Private profile label","fields":{"type":"object",
                "required":{"amount":{"type":"number"},"nothing":{"type":"null"}},
                "optional":{"links":{"type":"array","items":{"type":"entity","kinds":["location","lore-concept"]},"max_items":8}}}}],
            "predicates":[{"id":"account","subject_kinds":["lore-concept"],"value_type":{"type":"string"}}]}));
        f.write("viewer",VIEWER,"character",json!({"descriptions":[section(1,false),section(2,false)],
            "concept_links":[{"concept_id":CONCEPT,"relation":"learned-about","valid":{"kind":"timeless"}}]}));
        f.write("other", OTHER, "character", json!({}));
        f.write("place",PLACE,"location",json!({"descriptions":[section(1,false)],
            "spatial_history":[{"id":id("spatialhistory",1),"aspect":"parentage","valid":interval(),"value":{"parent_id":null}}]}));
        f.write("unknown-place", UNKNOWN_PLACE, "location", json!({}));
        for (name, identifier, scoped) in [
            ("concept", CONCEPT, false),
            ("unknown-concept", UNKNOWN_CONCEPT, false),
            ("private-concept", PRIVATE_CONCEPT, true),
        ] {
            f.write(name,identifier,"lore-concept",json!({"domain":"admission.notes","profile":"admission-note",
                "values":{"amount":9_007_199_254_740_993_u64,"nothing":null,"links":[reference(PLACE),reference(UNKNOWN_CONCEPT)]},
                "valid":interval(),"audience":[if scoped{"author"}else{"public"}],
                "perspectives":[if scoped{"author"}else{"ordinary"}],"provenance":"Private provenance",
                "descriptions":[section(1,false)]}));
        }
        let expired = json!({"kind":"interval","timeline":"main","end":{"timeline":"main","tick":TICK-10,"order":0}});
        f.write("assertion",ASSERTION,"assertion",json!({"domain":"admission.accounts","subject":reference(CONCEPT),
            "predicate":"account","value":"Canonical confidential wording","classification":"author-fact",
            "valid":expired,"evidence":[],"audience":["author"],"perspectives":["author"]}));
        f.write("correction",&id("assertion",2),"assertion",json!({"domain":"admission.accounts","subject":reference(CONCEPT),
            "predicate":"account","value":"Canonical correction remains separate","classification":"author-fact",
            "valid":expired,"replaces":ASSERTION,"evidence":[],"audience":["author"],"perspectives":["author"]}));
        f.knowledge(1,VIEWER,json!({"key":"heard.account","statement":"I heard a shared harbor bell","assertion_id":ASSERTION}),
            json!([transition("heard",TICK-2,0,"accepted"),transition("doubted",TICK,1,"rejected"),
                transition("forgot",TICK,3,"forgotten"),transition("recalled",TICK,4,"remembered")]),json!({}));
        f.knowledge(2,VIEWER,json!({"key":"never.acquired","statement":"Unacquired rejected words","assertion_id":ASSERTION}),
            json!([transition("rejected-only",TICK-1,0,"rejected")]),json!({}));
        f.knowledge(3,OTHER,json!({"key":"other.account","statement":"Another character's hidden words","assertion_id":ASSERTION}),
            json!([transition("other-heard",TICK-2,0,"accepted")]),json!({}));
        f.knowledge(4,VIEWER,json!({"key":"future.account","statement":"Future hidden words","assertion_id":ASSERTION}),
            json!([transition("future-heard",TICK+50,0,"accepted")]),json!({}));
        f.knowledge(5,VIEWER,json!({"key":"private.account","statement":"Private scoped words","assertion_id":ASSERTION}),
            json!([transition("private-heard",TICK-2,0,"accepted")]),json!({"audience":["author"],"perspectives":["author"]}));
        for (n, endpoint) in [(10, PLACE), (11, CONCEPT), (12, PRIVATE_CONCEPT)] {
            f.knowledge(n,VIEWER,json!({"key":format!("familiar.{n}"),"statement":format!("I know the harbor account {n}"),"object":{"entity":endpoint}}),
                json!([transition(&format!("acquired-{n}"),TICK-3,0,"accepted")]),json!({}));
        }
        f.valid();
        f
    }
    fn root(&self) -> &Path {
        self.0.path()
    }
    fn write(&self, name: &str, identifier: &str, kind: &str, fields: Json) {
        let mut value = json!({"schema":"wedl/v1","id":identifier,"kind":kind,"title":format!("Private title {name}"),
            "status":"canonical","aliases":[format!("Private alias {name}")],"tags":[]});
        value
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        fs::write(
            self.root().join(format!("story/{name}.md")),
            format!(
                "---\n{}---\nPrivate untimed body for {name}.\r\n",
                serde_yaml::to_string(&value).unwrap()
            ),
        )
        .unwrap();
    }
    fn knowledge(&self, n: usize, knower: &str, claim: Json, transitions: Json, extra: Json) {
        let mut fields = json!({"knower":knower,"claim":claim,"transitions":transitions});
        fields
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        self.write(
            &format!("knowledge-{n}"),
            &id("know", n),
            "knowledge",
            fields,
        );
    }
    fn world(&self) -> World {
        source::load_world(self.root(), None).unwrap()
    }
    fn service(&self) -> Service {
        Service::new(self.world())
    }
    fn valid(&self) {
        let report = validation::validate_world(&self.world());
        assert!(report.valid, "{:?}", report.diagnostics);
    }
    fn edit(&self, id: &str, change: impl FnOnce(&mut Yaml)) {
        let world = self.world();
        let record = &world.records[id];
        let mut fields = record.frontmatter.clone();
        change(&mut fields);
        fs::write(
            self.root().join(&record.source_path),
            format!(
                "---\n{}---\n{}",
                serde_yaml::to_string(&fields).unwrap(),
                record.body
            ),
        )
        .unwrap();
    }
    fn image(&self) -> Vec<(String, Vec<u8>)> {
        self.world()
            .records
            .values()
            .map(|r| (r.source_path.clone(), r.raw_bytes.clone()))
            .collect()
    }
}
fn uri(path: &str, params: &[(&str, String)]) -> String {
    fn escape(s: &str) -> String {
        s.bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                    (b as char).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect()
    }
    format!(
        "/api/{path}?{}",
        params
            .iter()
            .map(|(k, v)| format!("{}={}", escape(k), escape(v)))
            .collect::<Vec<_>>()
            .join("&")
    )
}
async fn get(f: &Fixture, path: &str) -> (StatusCode, Json) {
    let response = http::router(f.service())
        .oneshot(
            Request::builder()
                .uri(path)
                .header("host", "localhost:8765")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}
fn cli(f: &Fixture, args: &[String]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .args(["--compact", "--repo", f.root().to_str().unwrap()])
        .args(args)
        .output()
        .unwrap()
}
fn params(order: i32) -> Vec<(&'static str, String)> {
    vec![
        ("character", VIEWER.into()),
        ("tick", TICK.to_string()),
        ("order", order.to_string()),
        ("timeline", "main".into()),
    ]
}
fn assert_envelope(value: &Json) {
    assert_eq!(
        value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["protocol", "result"]
    );
    assert_eq!(value["protocol"], wedl::CONTRACT);
    assert_eq!(value["result"]["perspective"], "character");
    assert_eq!(value["result"]["characterId"], VIEWER);
    fn inspect(value: &Json) {
        match value {
            Json::Object(fields) => {
                for (key, value) in fields {
                    assert!(
                        ![
                            "fingerprint",
                            "revision",
                            "sourcePath",
                            "sourceField",
                            "blobOid",
                            "frontmatterYaml",
                            "rawSource",
                            "title",
                            "body",
                            "provenance",
                            "truth",
                            "notes",
                            "acquisition"
                        ]
                        .contains(&key.as_str()),
                        "leaked {key}"
                    );
                    inspect(value);
                }
            }
            Json::Array(items) => {
                for item in items {
                    inspect(item)
                }
            }
            _ => {}
        }
    }
    inspect(value);
}
struct Case {
    name: &'static str,
    params: Vec<(&'static str, String)>,
    args: Vec<String>,
    operation: Operation,
    native: Json,
}
fn cases(f: &Fixture, order: i32) -> Vec<Case> {
    let world = f.world();
    let at = point(order);
    let mut out = Vec::new();
    for name in [
        "assertions",
        "descriptions",
        "section",
        "lore",
        "concept",
        "links",
        "spatial-history",
        "existence",
        "governance",
    ] {
        let mut query = params(order);
        let mut args = vec!["character".into(), name.into(), VIEWER.into()];
        let (operation, native) = match name {
            "assertions" => {
                query.push(("target", ASSERTION.into()));
                args.push(ASSERTION.into());
                (
                    Operation::CharacterAssertions {
                        character: VIEWER.into(),
                        target: ASSERTION.into(),
                        at: time(order),
                        limit: 100,
                    },
                    learned_assertions::list(&world, VIEWER, ASSERTION, &at, 100).unwrap(),
                )
            }
            "descriptions" => {
                query.push(("owner", VIEWER.into()));
                args.push(VIEWER.into());
                (
                    Operation::CharacterDescriptions {
                        character: VIEWER.into(),
                        owner: VIEWER.into(),
                        at: time(order),
                        limit: 100,
                    },
                    narrative::descriptions(&world, VIEWER, VIEWER, &at, 100).unwrap(),
                )
            }
            "section" => {
                query.extend([("owner", VIEWER.into()), ("section", id("description", 1))]);
                args.extend([VIEWER.into(), id("description", 1)]);
                (
                    Operation::CharacterSection {
                        character: VIEWER.into(),
                        owner: VIEWER.into(),
                        section: id("description", 1),
                        at: time(order),
                    },
                    narrative::section(&world, VIEWER, VIEWER, &id("description", 1), &at).unwrap(),
                )
            }
            "lore" => {
                query.push(("domain", "admission.notes".into()));
                args.extend(["--domain".into(), "admission.notes".into()]);
                (
                    Operation::CharacterLore {
                        character: VIEWER.into(),
                        domain: Some("admission.notes".into()),
                        at: time(order),
                        limit: 100,
                    },
                    narrative::lore_catalog(&world, VIEWER, Some("admission.notes"), &at, 100)
                        .unwrap(),
                )
            }
            "concept" => {
                query.push(("target", CONCEPT.into()));
                args.push(CONCEPT.into());
                (
                    Operation::CharacterConcept {
                        character: VIEWER.into(),
                        target: CONCEPT.into(),
                        at: time(order),
                    },
                    narrative::lore_concept(&world, VIEWER, CONCEPT, &at).unwrap(),
                )
            }
            "links" => {
                query.push(("owner", VIEWER.into()));
                args.push(VIEWER.into());
                (
                    Operation::CharacterLinks {
                        character: VIEWER.into(),
                        owner: VIEWER.into(),
                        at: time(order),
                        limit: 100,
                    },
                    narrative::lore_links(&world, VIEWER, VIEWER, &at, 100).unwrap(),
                )
            }
            "spatial-history" => {
                query.push(("owner", PLACE.into()));
                args.push(PLACE.into());
                (
                    Operation::CharacterSpatialHistory {
                        character: VIEWER.into(),
                        owner: PLACE.into(),
                        at: time(order),
                        limit: 100,
                    },
                    narrative::spatial_history(&world, VIEWER, PLACE, &at, 100).unwrap(),
                )
            }
            "existence" => {
                query.push(("target", VIEWER.into()));
                args.push(VIEWER.into());
                (
                    Operation::CharacterExistence {
                        character: VIEWER.into(),
                        target: VIEWER.into(),
                        at: time(order),
                        limit: 100,
                    },
                    narrative::existence(&world, VIEWER, VIEWER, &at, 100).unwrap(),
                )
            }
            "governance" => {
                query.push(("target", PLACE.into()));
                args.push(PLACE.into());
                (
                    Operation::CharacterGovernance {
                        character: VIEWER.into(),
                        target: PLACE.into(),
                        at: time(order),
                        limit: 100,
                    },
                    narrative::governance(&world, VIEWER, PLACE, &at, 100).unwrap(),
                )
            }
            _ => unreachable!(),
        };
        args.extend([
            "--tick".into(),
            TICK.to_string(),
            "--order".into(),
            order.to_string(),
            "--timeline".into(),
            "main".into(),
        ]);
        out.push(Case {
            name,
            params: query,
            args,
            operation,
            native,
        });
    }
    out
}

#[tokio::test]
async fn all_nine_operations_share_native_service_cli_http_exact_values_and_safe_envelopes() {
    let f = Fixture::new();
    let source = f.image();
    for case in cases(&f, 0) {
        let service = f.service().execute(case.operation).unwrap();
        assert_eq!(service["result"], case.native, "{}", case.name);
        let (status, http) = get(&f, &uri(&format!("character/{}", case.name), &case.params)).await;
        assert_eq!(status, StatusCode::OK, "{http}");
        assert_eq!(http, service);
        assert_envelope(&http);
        let command = cli(&f, &case.args);
        assert!(
            command.status.success(),
            "{}",
            String::from_utf8_lossy(&command.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<Json>(&command.stdout).unwrap(),
            http
        );
        assert_eq!(http["result"]["effectiveTime"]["tick"], TICK.to_string());
        match case.name {
            "assertions" => {
                assert_eq!(http["result"]["count"], 1);
                assert_eq!(
                    http["result"]["items"][0]["time"]["tick"],
                    (TICK - 2).to_string()
                );
                assert_eq!(http["result"]["items"][0]["citations"], json!([]));
            }
            "descriptions" => assert_eq!(http["result"]["count"], 2),
            "section" => assert_eq!(http["result"]["sectionId"], id("description", 1)),
            "lore" => {
                assert_eq!(http["result"]["count"], 1);
                assert_eq!(
                    http["result"]["items"][0]["values"]["amount"],
                    TICK.to_string()
                );
            }
            "concept" => {
                assert_eq!(http["result"]["values"]["amount"], TICK.to_string());
                assert_eq!(http["result"]["values"]["links"], json!([reference(PLACE)]));
            }
            "links" | "spatial-history" => assert_eq!(http["result"]["count"], 1),
            "existence" => {
                assert_eq!(http["result"]["count"], 0);
                assert_eq!(http["result"]["items"], json!([]));
            }
            "governance" => {
                assert_eq!(http["result"]["count"], 1);
                let item = &http["result"]["items"][0];
                assert_eq!(item["jurisdiction"], "unknown");
                assert_eq!(item["basis"], Json::Null);
                assert_eq!(item["parties"], json!([]));
            }
            _ => unreachable!(),
        }
    }
    assert_eq!(f.image(), source);
}

#[tokio::test]
async fn learning_rejection_forgetting_and_exact_member_endpoints_keep_distinct_meanings() {
    let f = Fixture::new();
    for (order, count, state) in [
        (0, 1, "accepted"),
        (1, 1, "rejected"),
        (2, 1, "rejected"),
        (3, 0, ""),
        (4, 1, "remembered"),
    ] {
        let mut query = params(order);
        query.push(("target", ASSERTION.into()));
        let (status, value) = get(&f, &uri("character/assertions", &query)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["result"]["count"], count);
        if count == 1 {
            assert_eq!(value["result"]["items"][0]["state"], state);
        }
        query.pop();
        query.push(("target", PLACE.into()));
        let (_, government) = get(&f, &uri("character/governance", &query)).await;
        assert_eq!(government["result"]["count"], count);
    }
    for (name, key, target) in [
        ("descriptions", "owner", VIEWER),
        ("lore", "domain", "admission.notes"),
        ("spatial-history", "owner", PLACE),
    ] {
        for (order, count) in [
            (-1, 0),
            (0, if name == "descriptions" { 2 } else { 1 }),
            (2, if name == "descriptions" { 2 } else { 1 }),
            (3, 0),
        ] {
            let mut query = params(order);
            query.push((key, target.into()));
            let (status, value) = get(&f, &uri(&format!("character/{name}"), &query)).await;
            assert_eq!(status, StatusCode::OK, "{name}: {value}");
            assert_eq!(value["result"]["count"], count);
        }
    }
    let mut query = params(0);
    query.push(("target", KNOWLEDGE.into()));
    let (_, value) = get(&f, &uri("character/assertions", &query)).await;
    assert_eq!(value["result"]["count"], 1);
    assert!(!value.to_string().contains("Canonical confidential"));
}

#[tokio::test]
async fn character_routes_require_exact_horizons_and_reject_author_scope_or_grant_parameters() {
    let f = Fixture::new();
    for case in cases(&f, 0) {
        for field in ["character", "tick"] {
            let query = case
                .params
                .iter()
                .filter(|(key, _)| *key != field)
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(
                get(&f, &uri(&format!("character/{}", case.name), &query))
                    .await
                    .0,
                StatusCode::BAD_REQUEST
            );
        }
        for (key, value) in [
            ("audience", "author"),
            ("view", "author"),
            ("viewer", OTHER),
            ("knownRecords", "[]"),
            ("scope", "author"),
            ("mode", "author"),
            ("scene", "scene_00000000000000000000000001"),
            ("profile", "admission-note"),
            ("roles", "{}"),
            ("revision", "main"),
        ] {
            let mut query = case.params.clone();
            query.push((key, value.into()));
            assert_eq!(
                get(&f, &uri(&format!("character/{}", case.name), &query))
                    .await
                    .0,
                StatusCode::BAD_REQUEST,
                "{} {key}",
                case.name
            );
        }
        let mut duplicate = case.params.clone();
        duplicate.push(("character", VIEWER.into()));
        assert_eq!(
            get(&f, &uri(&format!("character/{}", case.name), &duplicate))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        for (key, value) in [
            ("tick", "1e3"),
            ("tick", "9007199254740993.0"),
            ("order", "2147483648"),
            ("timeline", "missing"),
        ] {
            let query = case
                .params
                .iter()
                .map(|(k, v)| (*k, if *k == key { value.into() } else { v.clone() }))
                .collect::<Vec<_>>();
            assert!(
                !get(&f, &uri(&format!("character/{}", case.name), &query))
                    .await
                    .0
                    .is_success(),
                "{} accepted {key}={value}",
                case.name
            );
        }
        let mut no_tick = case.args.clone();
        let index = no_tick.iter().position(|s| s == "--tick").unwrap();
        no_tick.drain(index..index + 2);
        assert!(!cli(&f, &no_tick).status.success());
        let mut author_flag = case.args.clone();
        author_flag.extend(["--audience".into(), "author".into()]);
        assert!(!cli(&f, &author_flag).status.success());
        for value in ["0", "1001", "-1"] {
            let mut query = case.params.clone();
            query.push(("limit", value.into()));
            assert_eq!(
                get(&f, &uri(&format!("character/{}", case.name), &query))
                    .await
                    .0,
                StatusCode::BAD_REQUEST
            );
        }
    }
}

#[tokio::test]
async fn unavailable_owners_and_viewers_are_uniform_and_complete_limits_never_truncate() {
    let f = Fixture::new();
    for (name, key) in [
        ("descriptions", "owner"),
        ("section", "owner"),
        ("concept", "target"),
        ("links", "owner"),
        ("spatial-history", "owner"),
    ] {
        let mut failures = Vec::new();
        for target in [
            UNKNOWN_CONCEPT,
            PRIVATE_CONCEPT,
            "missing-record",
            "Private title concept",
            ASSERTION,
        ] {
            let mut query = params(0);
            query.push((key, target.into()));
            if name == "section" {
                query.push(("section", id("description", 1)));
            }
            let (status, value) = get(&f, &uri(&format!("character/{name}"), &query)).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{name}: {value}");
            failures.push(value);
        }
        assert!(failures.windows(2).all(|w| w[0] == w[1]), "{name}");
    }
    for case in cases(&f, 0) {
        let failures = ["missing-viewer", PLACE, "Private title viewer"].map(|viewer| {
            let query = case
                .params
                .iter()
                .map(|(key, value)| {
                    (
                        *key,
                        if *key == "character" {
                            viewer.into()
                        } else {
                            value.clone()
                        },
                    )
                })
                .collect::<Vec<_>>();
            uri(&format!("character/{}", case.name), &query)
        });
        let mut values = Vec::new();
        for route in failures {
            let (status, value) = get(&f, &route).await;
            assert_eq!(status, StatusCode::NOT_FOUND);
            values.push(value);
        }
        assert!(values.windows(2).all(|w| w[0] == w[1]));
    }
    let mut query = params(0);
    query.extend([("owner", VIEWER.into()), ("limit", "1".into())]);
    let (status, value) = get(&f, &uri("character/descriptions", &query)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(value.get("result").is_none());
}

#[tokio::test]
async fn hidden_source_changes_leave_every_character_payload_search_and_budget_unchanged() {
    let f = Fixture::new();
    let case_routes = cases(&f, 0)
        .into_iter()
        .map(|case| uri(&format!("character/{}", case.name), &case.params))
        .collect::<Vec<_>>();
    let mut routes = case_routes;
    let mut query = params(0);
    query.push(("maxCharacters", "100000".into()));
    routes.push(uri("character/context", &query));
    query.pop();
    query.push(("q", "harbor".into()));
    routes.push(uri("character/search", &query));
    query.pop();
    query.push(("maxCharacters", "256".into()));
    routes.push(uri("character/context", &query));
    let mut before = Vec::new();
    for route in &routes {
        before.push(get(&f, route).await);
    }
    let original = f.world();
    for record in original.records.values() {
        let mut fields = record.frontmatter.clone();
        fields["title"] = Yaml::String("Changed private canonical title".into());
        fields["aliases"] = serde_yaml::to_value(["Changed private alias"]).unwrap();
        fields["author_note"] = Yaml::String("Hidden metadata is not evidence".repeat(20));
        fs::write(
            f.root().join(&record.source_path),
            format!(
                "---\n{}---\nChanged private untimed body.\n",
                serde_yaml::to_string(&fields).unwrap()
            ),
        )
        .unwrap();
    }
    f.edit(VIEWER, |fields| {
        fields["descriptions"]
            .as_sequence_mut()
            .unwrap()
            .insert(0, serde_yaml::to_value(section(3, true)).unwrap());
        fields["concept_links"].as_sequence_mut().unwrap().insert(
            0,
            serde_yaml::from_str(&format!(
                "concept_id: {UNKNOWN_CONCEPT}\nrelation: hidden-link\nvalid: {{kind: timeless}}\n"
            ))
            .unwrap(),
        );
    });
    f.edit(CONCEPT, |fields| {
        fields["values"]["links"] =
            serde_yaml::to_value([reference(UNKNOWN_CONCEPT), reference(PLACE)]).unwrap();
        fields["provenance"] = Yaml::String("Changed source provenance".into());
    });
    f.edit(ASSERTION, |fields| {
        fields["value"] = Yaml::String("Changed canonical truth words".into());
    });
    f.edit(&id("know", 4), |fields| {
        fields["claim"]["statement"] = Yaml::String("harbor ".repeat(200));
    });
    f.edit(KNOWLEDGE, |fields| {
        fields["transitions"].as_sequence_mut().unwrap().push(
            serde_yaml::to_value(transition("far-future", TICK + 100, 0, "uncertain")).unwrap(),
        );
    });
    let old_path = f.root().join("story/unknown-concept.md");
    fs::rename(old_path, f.root().join("story/renamed-unknown.md")).unwrap();
    f.valid();
    for (route, before) in routes.iter().zip(before) {
        assert_eq!(get(&f, route).await, before, "{route}");
    }
}

#[tokio::test]
async fn invalid_source_is_generic_across_character_transports_and_never_discloses_diagnostics() {
    let f = Fixture::new();
    let cases = cases(&f, 0);
    f.edit(ASSERTION, |fields| {
        fields["value"] = serde_yaml::from_str("[private-invalid-value]").unwrap();
    });
    assert!(!validation::validate_world(&f.world()).valid);
    let mut failures = Vec::new();
    for case in cases {
        let (status, value) =
            get(&f, &uri(&format!("character/{}", case.name), &case.params)).await;
        assert!(!status.is_success());
        assert!(value.get("result").is_none());
        let encoded = value.to_string();
        assert!(!encoded.contains(ASSERTION));
        assert!(!encoded.contains("private-invalid-value"));
        assert!(!encoded.contains("story/"));
        failures.push(value);
        let command = cli(&f, &case.args);
        assert!(!command.status.success());
        assert_eq!(
            serde_json::from_slice::<Json>(&command.stderr).unwrap(),
            failures.last().unwrap().clone()
        );
    }
    assert!(failures.windows(2).all(|w| w[0] == w[1]));
}
