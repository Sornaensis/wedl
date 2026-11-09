//! Author family contexts keep the core contract intact across CLI and closed GET routes.
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
    environment, http,
    model::{StoryTime, World},
    service::{Operation, Service, TimeInput},
    social, source, validation,
};

const TICK: i64 = 9_007_199_254_740_993;
const CHARACTER: &str = "char_00000000000000000000000001";
fn id(n: u32) -> String {
    format!("concept_{n:026}")
}
fn aid(n: u32) -> String {
    format!("assertion_{n:026}")
}
fn did(n: u32) -> String {
    format!("description_{n:026}")
}
fn reference(n: u32) -> Value {
    json!({"kind":"record","id":id(n)})
}
fn entity() -> Value {
    json!({"type":"entity","kinds":["lore-concept"]})
}
fn entities() -> Value {
    json!({"type":"array","items":entity(),"max_items":64})
}
fn interval() -> Value {
    json!({"kind":"interval","timeline":"main","start":{"timeline":"main","tick":TICK,"order":0},"end":{"timeline":"main","tick":TICK,"order":2}})
}
fn time(order: i32) -> TimeInput {
    TimeInput {
        tick: TICK,
        order,
        timeline: None,
    }
}
fn point(order: i32) -> StoryTime {
    StoryTime::new("main", TICK, order).unwrap()
}
fn section(n: u32, text: &str) -> Value {
    json!({"id":did(n),"heading":"Exact source","markdown":text,"valid":interval(),"audience":["author"],"perspectives":["author"]})
}

struct Fixture(TempDir);
impl Fixture {
    fn new() -> Self {
        let f = Self(tempfile::tempdir().unwrap());
        fs::create_dir(f.root().join("story")).unwrap();
        let mut profiles = vec![];
        for (name, required, optional) in [
            ("social-language", json!({}), json!({"scripts":entities()})),
            (
                "social-sense",
                json!({"language":entity(),"gloss":{"type":"string"}}),
                json!({}),
            ),
            (
                "social-era-spelling",
                json!({"language":entity(),"sense":entity(),"form":{"type":"string"}}),
                json!({"scripts":entities(),"register":entity()}),
            ),
            ("social-script", json!({}), json!({})),
            (
                "social-register",
                json!({"language":entity(),"label":{"type":"string"}}),
                json!({}),
            ),
            (
                "social-culture",
                json!({}),
                json!({"note":{"type":"string"}}),
            ),
        ] {
            profiles.push(json!({"id":name,"label":name,"fields":{"type":"object","required":required,"optional":optional}}));
        }
        for role in [
            "habitat",
            "resource",
            "technology",
            "livelihood",
            "trade-practice",
            "infrastructure",
            "era",
        ] {
            profiles.push(json!({"id":format!("context-{role}"),"label":role,"fields":{"type":"object","required":{},"optional":{
                "dependencies":entities(),"resources":entities(),"eras":entities(),"note":{"type":"string"},
                "quantity":{"type":"object","required":{"amount_text":{"type":"string"}},"optional":{"unit_text":{"type":"string"}}}
            }}}));
        }
        f.write("world","world_00000000000000000000000000","world",json!({
            "capabilities":["lore-core-v1","descriptions-v1","assertions-v1"],"default_timeline":"main",
            "timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"profiles":profiles,
            "predicates":[
                {"id":"context-availability","subject_kinds":["lore-concept"],"value_type":{"type":"string"}},
                {"id":"context-dependency-evidence","subject_kinds":["lore-concept"],"value_type":{"type":"string"}},
                {"id":"social-doctrine","subject_kinds":["lore-concept"],"value_type":{"type":"string"}},
                {"id":"exact-count","subject_kinds":["lore-concept"],"value_type":{"type":"number"}}
            ]
        }));
        f.write("character", CHARACTER, "character", json!({}));
        f.write("knowledge","know_00000000000000000000000001","knowledge",json!({"knower":CHARACTER,"claim":{"key":"lamp","statement":"I saw a lamp"},"transitions":[{"id":"learned","time":{"tick":0},"state":"accepted","acquisition":"observed","confidence":1}]}));
        f.concept(
            1,
            "social-language",
            json!({"scripts":[reference(6)]}),
            json!({}),
        );
        f.concept(
            2,
            "social-sense",
            json!({"language":reference(1),"gloss":"River bank"}),
            json!({}),
        );
        for (n, form) in [(3, "bank"), (4, ""), (5, " Bank ")] {
            f.concept(n,"social-era-spelling",json!({"language":reference(1),"sense":reference(2),"form":form,"scripts":[reference(6)],"register":reference(7)}),json!({"valid":interval()}));
        }
        f.concept(6, "social-script", json!({}), json!({}));
        f.concept(
            7,
            "social-register",
            json!({"language":reference(1),"label":"Harbor usage"}),
            json!({}),
        );
        f.concept(8,"social-culture",json!({"note":"Diaspora is explicitly authored."}),json!({"descriptions":[section(1,"A **dated** communal account.\r\nNo inferred membership.")]}));
        f.concept(
            9,
            "social-language",
            json!({}),
            json!({"audience":["editor"]}),
        );
        for (n, role) in [
            (20, "habitat"),
            (21, "resource"),
            (23, "livelihood"),
            (24, "trade-practice"),
            (25, "infrastructure"),
            (26, "era"),
        ] {
            f.concept(n, &format!("context-{role}"), json!({}), json!({}));
        }
        let mut correction = section(
            2,
            "Revised **method**.\r\n<script>window.unexpected = true</script>\n",
        );
        correction["replaces"] = json!(did(1));
        f.concept(22,"context-technology",json!({"dependencies":[reference(27),reference(21),reference(22)],"eras":[reference(26)],"quantity":{"amount_text":"9007199254740993","unit_text":"authored baskets"},"note":"Literal method account"}),json!({"descriptions":[section(1,"Old method"),correction],"concept_links":[{"concept_id":id(20),"relation":"situated-in","valid":interval()}]}));
        f.concept(
            27,
            "context-resource",
            json!({"note":"Hidden resource title"}),
            json!({"audience":["editor"]}),
        );
        f.assertion(1, 22, "context-availability", json!("Plentiful"), json!({}));
        f.assertion(
            2,
            22,
            "context-availability",
            json!("Scarce"),
            json!({"replaces":aid(1)}),
        );
        f.assertion(3,22,"context-availability",json!("A different account"),json!({"classification":"attributed-claim","attributed_to":{"kind":"record","id":CHARACTER}}));
        f.assertion(
            4,
            22,
            "context-dependency-evidence",
            json!("Unresolved source"),
            json!({"classification":"unresolved"}),
        );
        f.assertion(
            5,
            22,
            "exact-count",
            json!(9_007_199_254_740_993_u64),
            json!({}),
        );
        f.assertion(
            6,
            8,
            "social-doctrine",
            json!("The authored communal rule"),
            json!({}),
        );
        let report = validation::validate_world(&f.world());
        assert!(report.valid, "{:?}", report.diagnostics);
        f
    }
    fn root(&self) -> &Path {
        self.0.path()
    }
    fn write(&self, path: &str, id: &str, kind: &str, fields: Value) {
        let mut value = json!({"schema":"wedl/v1","id":id,"kind":kind,"title":path,"status":"canonical","aliases":[],"tags":[]});
        value
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        fs::write(
            self.root().join(format!("story/{path}.md")),
            format!(
                "---\n{}---\nOpaque body for {path}.\r\n",
                serde_yaml::to_string(&value).unwrap()
            ),
        )
        .unwrap();
    }
    fn concept(&self, n: u32, profile: &str, values: Value, extra: Value) {
        let mut fields = json!({"domain":"family.context","profile":profile,"values":values,"valid":{"kind":"timeless"},"audience":["author"],"perspectives":["author"]});
        fields
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        self.write(&format!("concept-{n}"), &id(n), "lore-concept", fields);
    }
    fn assertion(&self, n: u32, owner: u32, predicate: &str, value: Value, extra: Value) {
        let mut fields = json!({"domain":"family.account","subject":reference(owner),"predicate":predicate,"value":value,"classification":"author-fact","valid":interval(),"evidence":[],"audience":["author"],"perspectives":["author"]});
        fields
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        self.write(&format!("assertion-{n}"), &aid(n), "assertion", fields);
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
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
async fn parity(f: &Fixture, domain: &str, selection: &social::Selection, order: i32) -> Value {
    let world = f.world();
    let at = point(order);
    let mut params = vec![
        (
            "contexts",
            serde_json::to_string(&selection.contexts).unwrap(),
        ),
        ("tick", TICK.to_string()),
        ("order", order.to_string()),
        ("audience", "author".into()),
        ("view", "author".into()),
    ];
    let mut args = vec![domain.into(), "context".into()];
    let (operation, core) = if domain == "social" {
        args.push(selection.language.clone());
        params.push(("language", selection.language.clone()));
        for (flag, value) in [("sense", &selection.sense), ("form", &selection.form)] {
            if let Some(value) = value {
                args.extend([format!("--{flag}"), value.clone()]);
                params.push((flag, value.clone()));
            }
        }
        (
            Operation::SocialContext {
                selected: selection.clone(),
                at: time(order),
                audience: "author".into(),
                view: "author".into(),
            },
            social::context(&world, selection, &at, "author", "author").unwrap(),
        )
    } else {
        (
            Operation::EnvironmentContext {
                roots: selection.contexts.clone(),
                at: time(order),
                audience: "author".into(),
                view: "author".into(),
            },
            environment::context(&world, &selection.contexts, &at, "author", "author").unwrap(),
        )
    };
    for root in &selection.contexts {
        args.extend(["--context".into(), root.clone()]);
    }
    args.extend([
        "--tick".into(),
        TICK.to_string(),
        "--order".into(),
        order.to_string(),
        "--audience".into(),
        "author".into(),
        "--view".into(),
        "author".into(),
    ]);
    let expected = f.service().execute(operation).unwrap();
    assert_eq!(expected["result"], core);
    let (status, actual) = get(f, &uri(&format!("{domain}/context"), &params)).await;
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
fn social_selection() -> social::Selection {
    social::Selection {
        language: id(1),
        sense: Some(id(2)),
        form: None,
        contexts: vec![id(8)],
    }
}
fn environmental_selection() -> social::Selection {
    social::Selection {
        language: String::new(),
        sense: None,
        form: None,
        contexts: vec![id(22)],
    }
}
fn spellings(value: &Value) -> Vec<String> {
    value["records"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["role"] == "spelling")
        .map(|v| v["item"]["values"]["form"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn social_core_service_cli_and_http_preserve_exact_forms_sections_and_accounts() {
    let f = Fixture::new();
    let mut selected = social_selection();
    let result = parity(&f, "social", &selected, 1).await;
    assert_eq!(result["at"]["tick"], TICK.to_string());
    assert_eq!(spellings(&result), vec!["bank", "", " Bank "]);
    assert_eq!(result["canon"].as_array().unwrap().len(), 3);
    assert!(
        result
            .to_string()
            .contains("A **dated** communal account.\\r\\n")
    );
    for (form, expected) in [
        ("bank", vec!["bank"]),
        ("", vec![""]),
        (" Bank ", vec![" Bank "]),
        ("BANK", vec![]),
        ("bank ", vec![]),
    ] {
        selected.form = Some(form.into());
        let actual = parity(&f, "social", &selected, 1).await;
        assert_eq!(spellings(&actual), expected);
    }
}

#[tokio::test]
async fn environment_transports_preserve_projection_corrections_and_literal_quantities() {
    let f = Fixture::new();
    let result = parity(&f, "environment", &environmental_selection(), 1).await;
    assert_eq!(result["at"]["tick"], TICK.to_string());
    let text = result.to_string();
    assert!(text.contains("9007199254740993"));
    assert!(!text.contains("Hidden resource title"));
    assert!(!text.contains(&id(27)));
    assert!(text.contains("values.dependencies[1]"));
    assert!(text.contains("Revised **method**."));
    assert!(!text.contains("Old method"));
    assert!(
        text.contains("Scarce")
            && text.contains("A different account")
            && !text.contains("Plentiful")
    );
    assert!(text.contains("explicit-unresolved"));
    assert!(!result["competingAccounts"].as_array().unwrap().is_empty());
    assert_eq!(result["canon"].as_array().unwrap().len(), 1);
    assert_eq!(result["sections"].as_array().unwrap().len(), 1);
    let mut all = environmental_selection();
    all.contexts = vec![id(20), id(21), id(22), id(23), id(24), id(25), id(26)];
    let result = parity(&f, "environment", &all, 1).await;
    assert_eq!(result["records"].as_array().unwrap().len(), 7);
    assert_eq!(result["canon"].as_array().unwrap().len(), 7);
}

#[tokio::test]
async fn exact_boundaries_and_default_end_of_tick_share_the_existing_time_contract() {
    let f = Fixture::new();
    let social = social_selection();
    let env = environmental_selection();
    for order in [-1, 0, 2, 3] {
        let s = parity(&f, "social", &social, order).await;
        assert_eq!(
            spellings(&s).len(),
            if (0..=2).contains(&order) { 3 } else { 0 }
        );
        let e = parity(&f, "environment", &env, order).await;
        assert_eq!(
            e["sections"][0]["result"]["count"],
            if (0..=2).contains(&order) { 1 } else { 0 }
        );
    }
    for domain in ["social", "environment"] {
        let mut params = vec![("tick", TICK.to_string())];
        let mut args = vec![domain.into(), "context".into()];
        if domain == "social" {
            params.push(("language", id(1)));
            args.push(id(1));
        } else {
            params.push(("contexts", serde_json::to_string(&vec![id(22)]).unwrap()));
            args.extend(["--context".into(), id(22)]);
        }
        args.extend(["--tick".into(), TICK.to_string()]);
        let (status, result) = get(&f, &uri(&format!("{domain}/context"), &params)).await;
        assert_eq!(status, StatusCode::OK, "{result}");
        assert_eq!(result["result"]["at"]["order"], i32::MAX.to_string());
        let output = cli(&f, &args);
        assert!(output.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            result
        );
        params.push(("timeline", "other".into()));
        let (status, result) = get(&f, &uri(&format!("{domain}/context"), &params)).await;
        assert_eq!(status, StatusCode::OK, "{result}");
        assert_eq!(result["result"]["at"]["timeline"], "other");
    }
}

#[tokio::test]
async fn requests_require_horizons_and_reject_unknown_duplicate_or_wrongly_typed_selectors() {
    let f = Fixture::new();
    for domain in ["social", "environment"] {
        let base = if domain == "social" {
            vec![("language", id(1))]
        } else {
            vec![("contexts", serde_json::to_string(&vec![id(22)]).unwrap())]
        };
        for extra in [
            vec![],
            vec![("order", "0".into())],
            vec![("timeline", "main".into())],
        ] {
            let mut params = base.clone();
            params.extend(extra);
            let (status, _) = get(&f, &uri(&format!("{domain}/context"), &params)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }
        for (key, value) in [
            ("limit", "100"),
            ("viewer", CHARACTER),
            ("character", CHARACTER),
            ("mode", "character"),
            ("tick", "invalid"),
            ("order", "2147483648"),
            ("timeline", "missing"),
        ] {
            let mut params = base.clone();
            params.push(("tick", TICK.to_string()));
            params.push((key, value.into()));
            let (status, _) = get(&f, &uri(&format!("{domain}/context"), &params)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{domain} {key}");
        }
        for value in [
            "null".to_owned(),
            "{}".into(),
            "\"concept\"".into(),
            "[1]".into(),
            "[null]".into(),
            "[\"\"]".into(),
            format!("[\"{}\",\"{}\"]", id(8), id(8)),
            serde_json::to_string(&(1..=9).map(id).collect::<Vec<_>>()).unwrap(),
            "[] true".into(),
        ] {
            let mut params = base
                .iter()
                .filter(|(k, _)| *k != "contexts")
                .cloned()
                .collect::<Vec<_>>();
            params.extend([("tick", TICK.to_string()), ("contexts", value.clone())]);
            let (status, _) = get(&f, &uri(&format!("{domain}/context"), &params)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{domain} contexts={value}");
        }
        let mut args = vec![domain.into(), "context".into()];
        if domain == "social" {
            args.push(id(1));
        } else {
            args.extend(["--context".into(), id(22)]);
        }
        assert!(!cli(&f, &args).status.success());
        args.extend([
            "--tick".into(),
            TICK.to_string(),
            "--viewer".into(),
            CHARACTER.into(),
        ]);
        assert!(!cli(&f, &args).status.success());
    }
    for contexts in [None, Some("[]")] {
        let mut params = vec![("tick", TICK.to_string())];
        if let Some(v) = contexts {
            params.push(("contexts", v.into()));
        }
        let (status, _) = get(&f, &uri("environment/context", &params)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let params = vec![
        ("language", id(1)),
        ("tick", TICK.to_string()),
        ("sense", "".into()),
    ];
    assert_eq!(
        get(&f, &uri("social/context", &params)).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        get(
            &f,
            &uri(
                "environment/context",
                &[
                    ("contexts", format!("[\"{}\"]", id(22))),
                    ("tick", TICK.to_string()),
                    ("form", "bank".into())
                ]
            )
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn hidden_missing_and_wrong_role_roots_have_one_unavailable_response() {
    let f = Fixture::new();
    for (domain, targets) in [
        ("social", vec![9, 999, 20]),
        ("environment", vec![27, 999, 1]),
    ] {
        let mut expected = None;
        for target in targets {
            let mut params = vec![("tick", TICK.to_string())];
            params.push(if domain == "social" {
                ("language", id(target))
            } else {
                ("contexts", format!("[\"{}\"]", id(target)))
            });
            let (status, result) = get(&f, &uri(&format!("{domain}/context"), &params)).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{result}");
            assert!(!result.to_string().contains(&id(target)));
            if let Some(expected) = &expected {
                assert_eq!(&result, expected);
            } else {
                expected = Some(result);
            }
        }
    }
}

#[tokio::test]
async fn author_family_reads_preserve_source_and_do_not_feed_generic_character_evidence() {
    let f = Fixture::new();
    let before = f.world();
    let operations = [
        Operation::CharacterContext {
            character: CHARACTER.into(),
            at: time(1),
            max_characters: 100_000,
        },
        Operation::CharacterContext {
            character: CHARACTER.into(),
            at: time(1),
            max_characters: 80,
        },
        Operation::CharacterSearch {
            character: CHARACTER.into(),
            query: "lamp".into(),
            at: time(1),
            limit: 20,
        },
        Operation::CharacterSearch {
            character: CHARACTER.into(),
            query: "Scarce".into(),
            at: time(1),
            limit: 20,
        },
    ];
    let baseline = operations
        .iter()
        .cloned()
        .map(|o| format!("{:?}", f.service().execute(o)))
        .collect::<Vec<_>>();
    parity(&f, "social", &social_selection(), 1).await;
    parity(&f, "environment", &environmental_selection(), 1).await;
    let after = f.world();
    assert_eq!(before.fingerprint, after.fingerprint);
    for (id, record) in &before.records {
        assert_eq!(record.raw_bytes, after.records[id].raw_bytes);
    }
    for (path, old, new) in [
        (
            "concept-22",
            "Revised **method**.",
            "A different canonical method.",
        ),
        ("assertion-2", "Scarce", "Abundant"),
    ] {
        let p = f.root().join(format!("story/{path}.md"));
        fs::write(&p, fs::read_to_string(&p).unwrap().replace(old, new)).unwrap();
    }
    let report = validation::validate_world(&f.world());
    assert!(report.valid, "{:?}", report.diagnostics);
    assert_ne!(before.fingerprint, f.world().fingerprint);
    for (i, o) in operations.into_iter().enumerate() {
        assert_eq!(baseline[i], format!("{:?}", f.service().execute(o)));
    }
    let snapshot = Service::new(before);
    let op = Operation::EnvironmentContext {
        roots: vec![id(22)],
        at: time(1),
        audience: "author".into(),
        view: "author".into(),
    };
    let old = snapshot.execute(op.clone()).unwrap();
    let current = f.service().execute(op).unwrap();
    assert!(old.to_string().contains("Scarce") && !old.to_string().contains("Abundant"));
    assert!(current.to_string().contains("Abundant") && !current.to_string().contains("Scarce"));
}
