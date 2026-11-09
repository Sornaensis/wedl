//! Exact authored sections and propositions share one read contract across transports.
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;
use tower::ServiceExt;
use wedl::{
    assertion, description, http,
    model::{StoryTime, World},
    service::{Operation, Service, TimeInput},
    source, validation,
};

const OWNER: &str = "char_00000000000000000000000001";
const HIDDEN_OWNER: &str = "char_00000000000000000000000002";
const EMPTY_OWNER: &str = "char_00000000000000000000000003";
const CONCEPT: &str = "concept_00000000000000000000000001";
const HIDDEN_CONCEPT: &str = "concept_00000000000000000000000002";
const MISSING: &str = "char_00000000000000000000000999";
const TICK: i64 = 9_007_199_254_740_993;
const MARKDOWN: &str = "A revised **account**.\r\nAn ID-looking literal stays prose.\r<script>window.unexpected = true</script>\nTrailing  \r\n";
fn section_id(n: u32) -> String {
    format!("description_{n:026}")
}
fn assertion_id(n: u32) -> String {
    format!("assertion_{n:026}")
}
fn reference(id: &str) -> Value {
    json!({"kind":"record","id":id})
}
fn valid(timeline: &str, start: i64, end: i64) -> Value {
    json!({"kind":"interval","timeline":timeline,"start":{"timeline":timeline,"tick":start,"order":0},"end":{"timeline":timeline,"tick":end,"order":2}})
}
fn section(n: u32, markdown: &str, validity: Value, audience: &str) -> Value {
    json!({"id":section_id(n),"heading":format!("Section {n}"),"markdown":markdown,"valid":validity,"audience":[audience],"perspectives":["author"]})
}
fn account(text: &str, count: u64) -> Value {
    json!({"text":text,"count":count,"ratio":1.0,"literal":{"kind":"record","id":MISSING}})
}
struct Fixture(TempDir);
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("story")).unwrap();
        let f = Self(dir);
        f.write("world", "world_00000000000000000000000000", "world", "Narrative readers", json!({
            "capabilities":["lore-core-v1","descriptions-v1","assertions-v1"],
            "default_timeline":"main", "timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],
            "profiles":[{"id":"concept-data","label":"Concept data","fields":{"type":"object","required":{},"optional":{}}}],
            "predicates":[
                {"id":"account","subject_kinds":["character"],"value_type":{"type":"object","required":{
                    "text":{"type":"string"},"count":{"type":"number"},"ratio":{"type":"number"},
                    "literal":{"type":"object","required":{"kind":{"type":"string"},"id":{"type":"string"}},"optional":{}}
                },"optional":{"reference":{"type":"entity","kinds":["lore-concept"]}}}},
                {"id":"nullable","subject_kinds":["character"],"value_type":{"type":"null"}},
                {"id":"markers","subject_kinds":["character"],"value_type":{"type":"array","items":{"type":"string"},"max_items":8}},
                {"id":"choice","subject_kinds":["character"],"value_type":{"type":"enum","values":[1,"1"]}}
            ]
        }));
        let interval = valid("main", TICK, TICK);
        let mut revised = section(3, MARKDOWN, interval.clone(), "author");
        revised["replaces"] = json!(section_id(2));
        f.write(
            "owner",
            OWNER,
            "character",
            "Mira",
            json!({"initial_state":{},"descriptions":[
                section(1,"Timeless section",json!({"kind":"timeless"}),"author"),
                section(2,"Replaced account",interval.clone(),"author"), revised,
                section(4,"Independent alternative",interval.clone(),"author"),
                section(5,"Hidden section",json!({"kind":"timeless"}),"editor"),
                section(6,"Other timeline",valid("other",TICK,TICK),"author"),
                section(7,"Future section",valid("main",TICK+1,TICK+2),"author")
            ]}),
        );
        f.write("hidden-owner",HIDDEN_OWNER,"character","Private owner",json!({"audience":["editor"],"perspectives":["author"],"descriptions":[section(1,"Private owner's section",json!({"kind":"timeless"}),"author")]}));
        f.write(
            "empty-owner",
            EMPTY_OWNER,
            "character",
            "Empty owner",
            json!({}),
        );
        f.write("knowledge","know_00000000000000000000000001","knowledge","Known lamp",json!({
            "knower":OWNER,"claim":{"key":"lamp","statement":"I saw a lamp"},
            "transitions":[{"id":"learned","time":{"tick":0},"state":"accepted","acquisition":"observed","confidence":1}]
        }));
        for (path, id, audience) in [
            ("concept", CONCEPT, "author"),
            ("hidden-concept", HIDDEN_CONCEPT, "editor"),
        ] {
            f.write(path,id,"lore-concept",path,json!({"domain":"narrative.material","profile":"concept-data","values":{},"valid":{"kind":"timeless"},"audience":[audience],"perspectives":["author"]}));
        }
        f.assertion(
            1,
            "account",
            account("Original account", 9_007_199_254_740_993),
            interval.clone(),
            json!({}),
        );
        f.assertion(
            2,
            "account",
            account("Revised account", 9_007_199_254_740_995),
            interval.clone(),
            json!({"replaces":assertion_id(1)}),
        );
        f.assertion(3, "account", account("An attributed alternative",7), interval.clone(), json!({
            "classification":"attributed-claim","attributed_to":reference(OWNER),
            "evidence":[{"revision":"a".repeat(40),"path":"story/removed.md","blob_oid":"b".repeat(64),"target":{"kind":"record","id":MISSING}}]
        }));
        f.assertion(
            4,
            "nullable",
            Value::Null,
            json!({"kind":"timeless"}),
            json!({"classification":"unresolved"}),
        );
        f.assertion(
            5,
            "markers",
            json!([]),
            json!({"kind":"timeless"}),
            json!({}),
        );
        f.assertion(6, "choice", json!(1), json!({"kind":"timeless"}), json!({}));
        f.assertion(
            7,
            "choice",
            json!("1"),
            json!({"kind":"timeless"}),
            json!({}),
        );
        f.assertion(
            8,
            "account",
            account("Private account", 8),
            json!({"kind":"timeless"}),
            json!({"audience":["editor"]}),
        );
        f.assertion(
            9,
            "account",
            account("Other timeline", 9),
            valid("other", TICK, TICK),
            json!({}),
        );
        f.assertion(
            10,
            "account",
            account("Future account", 10),
            valid("main", TICK + 1, TICK + 2),
            json!({}),
        );
        let mut hidden_value = account("Unavailable correction", 11);
        hidden_value["reference"] = reference(HIDDEN_CONCEPT);
        f.assertion(
            11,
            "account",
            hidden_value,
            interval.clone(),
            json!({"replaces":assertion_id(2),"classification":"attributed-claim","attributed_to":reference(HIDDEN_OWNER)}),
        );
        let mut visible_value = account("Visible reference", u64::MAX);
        visible_value["reference"] = reference(CONCEPT);
        f.assertion(
            12,
            "account",
            visible_value,
            json!({"kind":"timeless"}),
            json!({}),
        );
        f.assertion(
            13,
            "account",
            account("Hidden attribution", 13),
            interval.clone(),
            json!({"classification":"attributed-claim","attributed_to":reference(HIDDEN_OWNER)}),
        );
        f.assertion(
            14,
            "account",
            account("Private subject", 14),
            interval,
            json!({"subject":reference(HIDDEN_OWNER)}),
        );
        let report = validation::validate_world(&f.world());
        assert!(report.valid, "{:?}", report.diagnostics);
        f
    }
    fn root(&self) -> &Path {
        self.0.path()
    }
    fn write(&self, path: &str, id: &str, kind: &str, title: &str, fields: Value) {
        let mut value = json!({"schema":"wedl/v1","id":id,"kind":kind,"title":title,"status":"canonical","aliases":[],"tags":[]});
        value
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        let yaml = serde_yaml::to_string(&value).unwrap();
        fs::write(
            self.root().join(format!("story/{path}.md")),
            format!("---\n{yaml}---\nOpaque authored body for {title}.\r\n"),
        )
        .unwrap();
    }
    fn assertion(&self, n: u32, predicate: &str, value: Value, valid: Value, extra: Value) {
        let mut fields = json!({"subject":reference(OWNER),"domain":"narrative.account","predicate":predicate,"value":value,"classification":"author-fact","valid":valid,"evidence":[],"audience":["author"],"perspectives":["author"]});
        fields
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        self.write(
            &format!("assertion-{n}"),
            &assertion_id(n),
            "assertion",
            &format!("Account {n}"),
            fields,
        );
    }
    fn world(&self) -> World {
        source::load_world(self.root(), None).unwrap()
    }
    fn service(&self) -> Service {
        Service::new(self.world())
    }
}
fn cli(f: &Fixture, args: &[String]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .args(["--compact", "--repo", f.root().to_str().unwrap()])
        .args(args)
        .output()
        .unwrap()
}
fn uri(path: &str, params: &[(&str, String)]) -> String {
    fn escape(v: &str) -> String {
        v.bytes()
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
async fn get(f: &Fixture, path: &str) -> (StatusCode, Value) {
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
async fn parity(
    f: &Fixture,
    path: &str,
    target: &str,
    member: Option<&str>,
    at: Option<(&str, i64, i32)>,
) -> Value {
    let world = f.world();
    let point = at.map(|(timeline, tick, order)| StoryTime::new(timeline, tick, order).unwrap());
    let input = at.map(|(timeline, tick, order)| TimeInput {
        timeline: Some(timeline.into()),
        tick,
        order,
    });
    let mut params = vec![];
    let mut args = path.split('/').map(str::to_string).collect::<Vec<_>>();
    let (operation, core) = match path {
        "descriptions/list" => {
            params.push(("owner", target.into()));
            args.push(target.into());
            (
                Operation::DescriptionList {
                    owner: target.into(),
                    at: input,
                    audience: "author".into(),
                    view: "author".into(),
                    limit: 100,
                },
                description::list(&world, target, point.as_ref(), "author", "author", 100).unwrap(),
            )
        }
        "descriptions/section" => {
            let section = member.unwrap();
            params.extend([("owner", target.into()), ("section", section.into())]);
            args.extend([target.into(), section.into()]);
            (
                Operation::DescriptionSection {
                    owner: target.into(),
                    section: section.into(),
                    at: input,
                    audience: "author".into(),
                    view: "author".into(),
                },
                description::section(&world, target, section, point.as_ref(), "author", "author")
                    .unwrap(),
            )
        }
        "assertions/list" => {
            params.push(("target", target.into()));
            args.push(target.into());
            (
                Operation::AssertionList {
                    target: target.into(),
                    at: input,
                    audience: "author".into(),
                    view: "author".into(),
                    limit: 100,
                },
                assertion::as_of(&world, target, point.as_ref(), "author", "author", 100).unwrap(),
            )
        }
        "assertions/predicates" => (
            Operation::AssertionPredicates { limit: 100 },
            assertion::predicates(&world, 100).unwrap(),
        ),
        _ => panic!("unknown parity path"),
    };
    if let Some((timeline, tick, order)) = at {
        params.extend([
            ("timeline", timeline.into()),
            ("tick", tick.to_string()),
            ("order", order.to_string()),
        ]);
        args.extend([
            "--timeline".into(),
            timeline.into(),
            "--tick".into(),
            tick.to_string(),
            "--order".into(),
            order.to_string(),
        ]);
    }
    let expected = f.service().execute(operation).unwrap();
    assert_eq!(expected["result"], core);
    let (status, actual) = get(f, &uri(path, &params)).await;
    assert_eq!(status, StatusCode::OK, "{actual}");
    assert_eq!(actual, expected);
    let output = cli(f, &args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        expected
    );
    core
}
fn ids(value: &Value, field: &str) -> Vec<String> {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item[field].as_str().unwrap().into())
        .collect()
}
fn item(value: &Value, n: u32) -> &Value {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["assertionId"] == assertion_id(n))
        .unwrap()
}

#[tokio::test]
async fn all_four_queries_agree_and_preserve_exact_markdown_values_and_source_citations() {
    let f = Fixture::new();
    let at = Some(("main", TICK, 1));
    let sections = parity(&f, "descriptions/list", OWNER, None, at).await;
    assert_eq!(
        ids(&sections, "sectionId"),
        [section_id(1), section_id(3), section_id(4)]
    );
    assert_eq!(sections["at"]["tick"], TICK.to_string());
    let section = parity(&f, "descriptions/section", OWNER, Some(&section_id(3)), at).await;
    assert_eq!(section["markdown"], MARKDOWN);
    assert_eq!(section["citation"]["section"], "descriptions[2]");
    assert_eq!(
        section["target"],
        json!({"kind":"section","ownerId":OWNER,"sectionId":section_id(3)})
    );
    assert_eq!(section["valid"]["start"]["tick"], TICK.to_string());
    let assertions = parity(&f, "assertions/list", OWNER, None, at).await;
    assert_eq!(
        ids(&assertions, "assertionId"),
        [2, 3, 4, 5, 6, 7, 12].map(assertion_id)
    );
    assert_eq!(item(&assertions, 2)["value"]["count"], "9007199254740995");
    assert_eq!(item(&assertions, 2)["value"]["ratio"].as_f64(), Some(1.0));
    assert_eq!(item(&assertions, 2)["value"]["literal"]["id"], MISSING);
    assert_eq!(item(&assertions, 3)["classification"], "attributed-claim");
    assert_eq!(item(&assertions, 3)["attributedTo"], reference(OWNER));
    assert_eq!(item(&assertions, 3)["evidence"], json!([]));
    assert_eq!(item(&assertions, 4)["classification"], "unresolved");
    assert!(item(&assertions, 4)["value"].is_null());
    assert_eq!(item(&assertions, 5)["value"], json!([]));
    assert_eq!(item(&assertions, 6)["value"], "1");
    assert_eq!(item(&assertions, 7)["value"], "1");
    assert_eq!(item(&assertions, 6)["valueType"]["scalarType"], "number");
    assert_eq!(item(&assertions, 7)["valueType"]["scalarType"], "string");
    assert_eq!(
        item(&assertions, 12)["value"]["count"],
        u64::MAX.to_string()
    );
    assert!(
        item(&assertions, 12)["references"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["sourceField"] == "value.reference.id" && r["entityId"] == CONCEPT)
    );
    let predicates = parity(&f, "assertions/predicates", "", None, None).await;
    assert_eq!(predicates["count"], 4);
    let choice = predicates["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "choice")
        .unwrap();
    assert_eq!(
        choice["value_type"]["enumValueTypes"],
        json!(["number", "string"])
    );
}

#[tokio::test]
async fn exact_horizon_boundaries_default_order_other_timeline_and_history_agree() {
    let f = Fixture::new();
    for (timeline, tick, order, descriptions, assertions) in [
        ("main", TICK, -1, vec![1], vec![4, 5, 6, 7, 12]),
        ("main", TICK, 0, vec![1, 3, 4], vec![2, 3, 4, 5, 6, 7, 12]),
        ("main", TICK, 2, vec![1, 3, 4], vec![2, 3, 4, 5, 6, 7, 12]),
        ("main", TICK, 3, vec![1], vec![4, 5, 6, 7, 12]),
        ("main", TICK + 1, 0, vec![1, 7], vec![4, 5, 6, 7, 10, 12]),
        ("other", TICK, 1, vec![1, 6], vec![4, 5, 6, 7, 9, 12]),
        ("main", i64::MIN, i32::MIN, vec![1], vec![4, 5, 6, 7, 12]),
    ] {
        let at = Some((timeline, tick, order));
        let sections = parity(&f, "descriptions/list", OWNER, None, at).await;
        assert_eq!(
            ids(&sections, "sectionId"),
            descriptions.into_iter().map(section_id).collect::<Vec<_>>()
        );
        let propositions = parity(&f, "assertions/list", OWNER, None, at).await;
        assert_eq!(
            ids(&propositions, "assertionId"),
            assertions.into_iter().map(assertion_id).collect::<Vec<_>>()
        );
    }
    let history = parity(&f, "descriptions/list", OWNER, None, None).await;
    assert_eq!(
        ids(&history, "sectionId"),
        [1, 2, 3, 4, 6, 7].map(section_id)
    );
    assert!(history["at"].is_null());
    let history = parity(&f, "assertions/list", OWNER, None, None).await;
    assert_eq!(
        ids(&history, "assertionId"),
        [1, 2, 3, 4, 5, 6, 7, 9, 10, 12].map(assertion_id)
    );
    assert!(history["at"].is_null());
    for (path, key) in [
        ("descriptions/list", "owner"),
        ("assertions/list", "target"),
    ] {
        let (status, wire) = get(
            &f,
            &uri(path, &[(key, OWNER.into()), ("tick", TICK.to_string())]),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(wire["result"]["at"]["order"], i32::MAX.to_string());
        let output = cli(
            &f,
            &[
                path.split('/').next().unwrap().into(),
                "list".into(),
                OWNER.into(),
                "--tick".into(),
                TICK.to_string(),
            ],
        );
        assert!(output.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            wire
        );
    }
}

#[tokio::test]
async fn assertion_projection_and_attribution_admission_precede_replacement_and_limits() {
    let f = Fixture::new();
    let result = parity(&f, "assertions/list", OWNER, None, Some(("main", TICK, 1))).await;
    assert!(ids(&result, "assertionId").contains(&assertion_id(2)));
    for hidden in [8, 11, 13, 14] {
        assert!(!ids(&result, "assertionId").contains(&assertion_id(hidden)));
    }
    for n in [1, 11, 13, 14] {
        let result = parity(
            &f,
            "assertions/list",
            &assertion_id(n),
            None,
            Some(("main", TICK, 1)),
        )
        .await;
        assert_eq!(result["count"], 0);
    }
    assert_eq!(
        parity(&f, "assertions/list", &assertion_id(1), None, None).await["count"],
        1
    );
    let (status, editor) = get(
        &f,
        &uri(
            "assertions/list",
            &[
                ("target", OWNER.into()),
                ("audience", "editor".into()),
                ("limit", "1".into()),
            ],
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&editor["result"], "assertionId"), [assertion_id(8)]);
    for (path, key, limit) in [
        ("descriptions/list", "owner", 3),
        ("assertions/list", "target", 7),
        ("assertions/predicates", "", 4),
    ] {
        let mut params = vec![("limit", limit.to_string())];
        if !key.is_empty() {
            params.extend([
                (key, OWNER.into()),
                ("tick", TICK.to_string()),
                ("order", "1".into()),
            ]);
        }
        let (status, result) = get(&f, &uri(path, &params)).await;
        assert_eq!(status, StatusCode::OK, "{result}");
        params[0].1 = (limit - 1).to_string();
        let (status, result) = get(&f, &uri(path, &params)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{result}");
        assert!(result.get("result").is_none());
    }
    // A visible attribution admits the correction's remaining projected object.
    // Its hidden nested reference is omitted; it does not withhold the whole row.
    let path = f.root().join("story/assertion-11.md");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace(HIDDEN_OWNER, OWNER),
    )
    .unwrap();
    let projected = parity(&f, "assertions/list", OWNER, None, Some(("main", TICK, 1))).await;
    assert!(ids(&projected, "assertionId").contains(&assertion_id(11)));
    assert!(!ids(&projected, "assertionId").contains(&assertion_id(2)));
    assert!(item(&projected, 11)["value"].get("reference").is_none());
    assert!(
        item(&projected, 11)["valueType"]["fields"]
            .get("reference")
            .is_none()
    );
    assert!(
        !item(&projected, 11)["references"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["entityId"] == HIDDEN_CONCEPT)
    );
}

#[tokio::test]
async fn description_lookup_does_not_reveal_missing_hidden_or_replaced_members() {
    let f = Fixture::new();
    let mut errors = vec![];
    for (owner, section) in [
        (OWNER, section_id(2)),
        (OWNER, section_id(5)),
        (OWNER, section_id(999)),
        (HIDDEN_OWNER, section_id(1)),
        (MISSING, section_id(1)),
        ("Mira", section_id(1)),
    ] {
        let (status, error) = get(
            &f,
            &uri(
                "descriptions/section",
                &[
                    ("owner", owner.into()),
                    ("section", section),
                    ("tick", TICK.to_string()),
                    ("order", "1".into()),
                ],
            ),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        errors.push(error);
    }
    assert!(errors.iter().all(|e| e == &errors[0]));
    let old = parity(
        &f,
        "descriptions/section",
        OWNER,
        Some(&section_id(2)),
        None,
    )
    .await;
    assert_eq!(old["markdown"], "Replaced account");
    assert_eq!(
        parity(&f, "descriptions/list", EMPTY_OWNER, None, None).await["count"],
        0
    );
    for target in [MISSING, HIDDEN_OWNER, "Mira"] {
        assert_eq!(
            parity(&f, "assertions/list", target, None, None).await["items"],
            json!([])
        );
    }
}

#[tokio::test]
async fn query_and_cli_shapes_are_closed_and_never_accept_character_grants() {
    let f = Fixture::new();
    for path in [
        "descriptions/list",
        "descriptions/section",
        "assertions/list",
        "assertions/predicates",
    ] {
        let mut base = match path {
            "descriptions/list" => vec![("owner", OWNER.into())],
            "descriptions/section" => vec![("owner", OWNER.into()), ("section", section_id(1))],
            "assertions/list" => vec![("target", OWNER.into())],
            _ => vec![],
        };
        for (key, value) in [
            ("viewer", OWNER),
            ("character", OWNER),
            ("known_records", OWNER),
            ("mode", "character"),
            ("unexpected", "x"),
        ] {
            let mut params = base.clone();
            params.push((key, value.into()));
            let (status, error) = get(&f, &uri(path, &params)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{path}: {error}");
        }
        if path == "assertions/predicates" {
            for key in [
                "tick", "order", "timeline", "audience", "view", "target", "section",
            ] {
                let (status, error) = get(&f, &uri(path, &[(key, "0".into())])).await;
                assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
            }
        } else {
            for (key, value) in [
                ("order", "0"),
                ("timeline", "main"),
                ("tick", "1.5"),
                ("tick", "9223372036854775808"),
            ] {
                let mut params = base.clone();
                params.push((key, value.into()));
                let (status, error) = get(&f, &uri(path, &params)).await;
                assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
            }
            base.extend([("tick", TICK.to_string()), ("order", "2147483648".into())]);
            assert_eq!(get(&f, &uri(path, &base)).await.0, StatusCode::BAD_REQUEST);
        }
        let bad = match path {
            "descriptions/section" => format!(
                "/api/{path}?owner={OWNER}&section={}&limit=1",
                section_id(1)
            ),
            "descriptions/list" => format!("/api/{path}?owner={OWNER}&owner={OWNER}"),
            "assertions/list" => format!("/api/{path}?target={OWNER}&target={OWNER}"),
            _ => format!("/api/{path}?limit=1&limit=1"),
        };
        assert_eq!(get(&f, &bad).await.0, StatusCode::BAD_REQUEST);
    }
    for args in [
        vec!["descriptions", "list"],
        vec!["descriptions", "section", OWNER],
        vec!["assertions", "list"],
        vec!["descriptions", "list", OWNER, "--order", "0"],
        vec!["assertions", "list", OWNER, "--timeline", "main"],
        vec!["assertions", "predicates", "--tick", "0"],
        vec![
            "descriptions",
            "section",
            OWNER,
            "description_00000000000000000000000001",
            "--limit",
            "1",
        ],
        vec!["assertions", "list", OWNER, "--viewer", OWNER],
    ] {
        let output = cli(
            &f,
            &args.into_iter().map(str::to_string).collect::<Vec<_>>(),
        );
        assert!(!output.status.success());
    }
    for (path, key) in [
        ("descriptions/list", "owner"),
        ("assertions/list", "target"),
        ("assertions/predicates", ""),
    ] {
        for limit in ["0", "1001", "-1", "1.0"] {
            let mut params = vec![("limit", limit.into())];
            if !key.is_empty() {
                params.push((key, OWNER.into()));
            }
            assert_eq!(
                get(&f, &uri(path, &params)).await.0,
                StatusCode::BAD_REQUEST
            );
        }
    }
}

#[tokio::test]
async fn narrative_reads_and_canonical_edits_leave_character_responses_and_source_independent() {
    let f = Fixture::new();
    let before = f.world();
    let operations = [
        Operation::CharacterContext {
            character: OWNER.into(),
            at: TimeInput {
                tick: TICK,
                order: 1,
                timeline: None,
            },
            max_characters: 100_000,
        },
        Operation::CharacterContext {
            character: OWNER.into(),
            at: TimeInput {
                tick: TICK,
                order: 1,
                timeline: None,
            },
            max_characters: 80,
        },
        Operation::CharacterSearch {
            character: OWNER.into(),
            query: "lamp".into(),
            at: TimeInput {
                tick: TICK,
                order: 1,
                timeline: None,
            },
            limit: 20,
        },
        Operation::CharacterSearch {
            character: OWNER.into(),
            query: "account".into(),
            at: TimeInput {
                tick: TICK,
                order: 1,
                timeline: None,
            },
            limit: 20,
        },
    ];
    let baseline = operations
        .iter()
        .cloned()
        .map(|op| format!("{:?}", f.service().execute(op)))
        .collect::<Vec<_>>();
    parity(&f, "descriptions/list", OWNER, None, None).await;
    parity(&f, "assertions/list", OWNER, None, None).await;
    let read = f.world();
    assert_eq!(before.fingerprint, read.fingerprint);
    for (id, record) in &before.records {
        assert_eq!(read.records[id].raw_bytes, record.raw_bytes);
    }
    let path = f.root().join("story/owner.md");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("Timeless section", "A different canonical description"),
    )
    .unwrap();
    let path = f.root().join("story/assertion-2.md");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("Revised account", "Different canonical account"),
    )
    .unwrap();
    let changed = f.world();
    assert_ne!(changed.fingerprint, before.fingerprint);
    let report = validation::validate_world(&changed);
    assert!(report.valid, "{:?}", report.diagnostics);
    assert_eq!(
        operations
            .into_iter()
            .map(|op| format!("{:?}", f.service().execute(op)))
            .collect::<Vec<_>>(),
        baseline
    );
}
