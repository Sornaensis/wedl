//! Author existence and governance retain the native contract across closed transports.
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
    existence, governance, http,
    model::{StoryTime, World},
    service::{self, Operation, Service, TimeInput},
    source, validation,
};

const TICK: i64 = 9_007_199_254_740_993;
const CHARACTER: &str = "char_00000000000000000000000001";
const HIDDEN_CHARACTER: &str = "char_00000000000000000000000002";
const LOCATION: &str = "loc_00000000000000000000000001";
const HIDDEN_LOCATION: &str = "loc_00000000000000000000000002";
const ORGANIZATION: &str = "organization_00000000000000000000000001";
const OVERLAY: &str = "overlay:transport/timed";

fn id(prefix: &str, n: u32) -> String {
    format!("{prefix}_{n:026}")
}
fn reference(id: &str) -> Value {
    json!({"kind":"record","id":id})
}
fn interval(timeline: &str) -> Value {
    json!({"kind":"interval","timeline":timeline,"start":{"timeline":timeline,"tick":TICK,"order":0},"end":{"timeline":timeline,"tick":TICK,"order":2}})
}
fn evidence() -> Value {
    json!([{"revision":"a".repeat(40),"path":"story/removed.md","blob_oid":"b".repeat(64),"target":{"kind":"record","id":id("char",99)}}])
}
fn point(order: i32) -> StoryTime {
    StoryTime::new("main", TICK, order).unwrap()
}
fn time(order: i32) -> TimeInput {
    TimeInput {
        tick: TICK,
        order,
        timeline: None,
    }
}
fn binding(profile: &str) -> governance::Binding {
    service::governance_binding_json(profile, r#"{"territory":"area","party":"issuer","basis":"basis","subjects":"subjects","obligations":"duties","supersedes":"predecessor"}"#).unwrap()
}

struct Fixture(TempDir);
impl Fixture {
    fn new() -> Self {
        let f = Self(tempfile::tempdir().unwrap());
        fs::create_dir(f.root().join("story")).unwrap();
        let party = json!({"type":"entity","kinds":["character","organization","lore-concept"]});
        let fields = json!({"type":"object","required":{
            "area":{"type":"entity","kinds":["location","overlay"]},"issuer":party,
            "basis":{"type":"enum","values":["legal","de-facto","claimed","contested"]}},
            "optional":{"subjects":{"type":"array","items":party,"max_items":8},
                "duties":{"type":"object","required":{"count":{"type":"number"},"known_null":{"type":"null"}},"optional":{}},
                "predecessor":{"type":"entity","kinds":["lore-concept"]}}});
        let mut nullable = fields.clone();
        nullable["optional"]["duties"] = json!({"type":"null"});
        f.write("world",&id("world",0),"world",json!({
            "capabilities":["generational-core-v1","spatial-core-v1","overlay-v1","lore-core-v1","descriptions-v1","assertions-v1","existence-v1","collective-parties-v1"],
            "default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],
            "profiles":[{"id":"governance-policy","label":"Policies","fields":fields},
                {"id":"governance-null","label":"Known null","fields":nullable},
                {"id":"collective-party","label":"Named collective","fields":{"type":"object","required":{},"optional":{}}}],
            "predicates":[{"id":"policy-account","subject_kinds":["lore-concept"],"value_type":{"type":"string"}}]
        }));
        f.write("character", CHARACTER, "character", json!({}));
        f.write(
            "hidden-character",
            HIDDEN_CHARACTER,
            "character",
            json!({"audience":["editor"]}),
        );
        f.write("location", LOCATION, "location", json!({}));
        f.write(
            "hidden-location",
            HIDDEN_LOCATION,
            "location",
            json!({"audience":["editor"]}),
        );
        f.write("organization",ORGANIZATION,"organization",json!({"organization_kind":"institution","parent_id":null,"location_id":LOCATION,
            "initialization":{"transition_id":id("transition",1),"transition_kind":"organization-initialize","applicability":{"applicability_kind":"instant","point":{"timeline":"main","tick":0,"order":0}},"payload":{"title":"Authored council","aliases":[]}},"transitions":[]}));
        f.write("overlay",OVERLAY,"overlay",json!({"lifecycle":"time-bounded","membership":{"location_ids":[LOCATION]},"audience":["author"],"perspectives":["author"],
            "valid":{"start":{"timeline":"main","tick":TICK,"order":0},"end":{"timeline":"main","tick":TICK,"order":2}}}));
        f.write("knowledge",&id("know",1),"knowledge",json!({"knower":CHARACTER,"claim":{"key":"lamp","statement":"I saw a lamp"},"transitions":[{"id":"learned","time":{"tick":0},"state":"accepted","acquisition":"observed","confidence":1}]}));
        f.concept(50, "collective-party", json!({}), json!({}));
        f.policy(1,LOCATION,CHARACTER,"legal",json!({"duties":{"count":9_007_199_254_740_993_u64,"known_null":null},"subjects":[reference(CHARACTER),reference(ORGANIZATION),reference(&id("concept",50))]}),json!({}));
        f.policy(
            2,
            LOCATION,
            ORGANIZATION,
            "de-facto",
            json!({"predecessor":reference(&id("concept",1))}),
            json!({}),
        );
        f.policy(
            3,
            OVERLAY,
            &id("concept", 50),
            "claimed",
            json!({}),
            json!({}),
        );
        f.concept(4,"governance-null",json!({"area":reference(LOCATION),"issuer":reference(CHARACTER),"basis":"contested","duties":null}),json!({}));
        f.policy(
            5,
            LOCATION,
            HIDDEN_CHARACTER,
            "contested",
            json!({}),
            json!({}),
        );
        f.policy(
            6,
            LOCATION,
            CHARACTER,
            "claimed",
            json!({}),
            json!({"audience":["editor"]}),
        );
        f.assertion(1, "Old rule", json!({}));
        f.assertion(2, "Revised rule", json!({"replaces":id("assertion",1)}));
        f.assertion(
            3,
            "Alternative rule",
            json!({"classification":"unresolved"}),
        );
        for (n, kind, valid) in [
            (1, "embodied", interval("main")),
            (2, "constructed", interval("main")),
            (4, "other", interval("other")),
        ] {
            f.episode(n,CHARACTER,kind,valid,json!({"start_evidence":evidence(),"end_evidence":evidence(),"provenance":"Authored account"}));
        }
        f.episode(
            3,
            CHARACTER,
            "unknown",
            json!({"kind":"interval","timeline":"main"}),
            json!({"start_evidence":[]}),
        );
        f.episode(
            5,
            CHARACTER,
            "embodied",
            interval("main"),
            json!({"start_evidence":evidence(),"end_evidence":evidence(),"audience":["editor"]}),
        );
        f.episode(
            6,
            HIDDEN_CHARACTER,
            "embodied",
            interval("main"),
            json!({"start_evidence":evidence(),"end_evidence":evidence()}),
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
        let mut fields = json!({"domain":"facts.authority","profile":profile,"values":values,"valid":interval("main"),"audience":["author"],"perspectives":["author"]});
        fields
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        self.write(
            &format!("concept-{n}"),
            &id("concept", n),
            "lore-concept",
            fields,
        );
    }
    fn policy(
        &self,
        n: u32,
        area: &str,
        issuer: &str,
        basis: &str,
        extra_values: Value,
        extra: Value,
    ) {
        let mut values = json!({"area":reference(area),"issuer":reference(issuer),"basis":basis});
        values
            .as_object_mut()
            .unwrap()
            .extend(extra_values.as_object().unwrap().clone());
        self.concept(n, "governance-policy", values, extra);
    }
    fn assertion(&self, n: u32, value: &str, extra: Value) {
        let mut fields = json!({"domain":"facts.account","subject":reference(&id("concept",1)),"predicate":"policy-account","value":value,"classification":"author-fact","valid":interval("main"),"evidence":[],"audience":["author"],"perspectives":["author"]});
        fields
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        self.write(
            &format!("assertion-{n}"),
            &id("assertion", n),
            "assertion",
            fields,
        );
    }
    fn episode(&self, n: u32, character: &str, kind: &str, valid: Value, extra: Value) {
        let mut fields = json!({"domain":"facts.existence","character_id":character,"episode_kind":kind,"valid":valid,"audience":["author"],"perspectives":["author"]});
        fields
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        self.write(
            &format!("existence-{n}"),
            &id("existence", n),
            "existence-episode",
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
    fn escape(value: &str) -> String {
        value
            .bytes()
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
async fn parity(
    f: &Fixture,
    domain: &str,
    target: &str,
    selected: &governance::Binding,
    order: Option<i32>,
) -> Value {
    let world = f.world();
    let at = order.map(point);
    let mut params = vec![
        (
            if domain == "existence" {
                "character"
            } else {
                "target"
            },
            target.into(),
        ),
        ("audience", "author".into()),
        ("view", "author".into()),
        ("limit", "100".into()),
    ];
    let mut args = vec![
        domain.into(),
        if domain == "existence" {
            "list".into()
        } else {
            "query".into()
        },
        target.into(),
    ];
    let (operation, core) = if domain == "existence" {
        (
            Operation::ExistenceList {
                character: target.into(),
                at: order.map(time),
                audience: "author".into(),
                view: "author".into(),
                limit: 100,
            },
            existence::list(&world, target, at.as_ref(), "author", "author", 100).unwrap(),
        )
    } else {
        let roles = serde_json::to_string(&selected.roles).unwrap();
        params.extend([
            ("profile", selected.profile.clone()),
            ("roles", roles.clone()),
        ]);
        args.extend([
            "--profile".into(),
            selected.profile.clone(),
            "--roles".into(),
            roles,
        ]);
        (
            Operation::GovernanceQuery {
                target: target.into(),
                binding: selected.clone(),
                at: order.map(time),
                audience: "author".into(),
                view: "author".into(),
                limit: 100,
            },
            governance::query(
                &world,
                target,
                selected,
                at.as_ref(),
                "author",
                "author",
                100,
            )
            .unwrap(),
        )
    };
    args.extend([
        "--audience".into(),
        "author".into(),
        "--view".into(),
        "author".into(),
        "--limit".into(),
        "100".into(),
    ]);
    if let Some(order) = order {
        params.extend([("tick", TICK.to_string()), ("order", order.to_string())]);
        args.extend([
            "--tick".into(),
            TICK.to_string(),
            format!("--order={order}"),
        ]);
    }
    let expected = f.service().execute(operation).unwrap();
    assert_eq!(expected["result"], core);
    let route = if domain == "existence" {
        "existence/list"
    } else {
        "governance/query"
    };
    let (status, actual) = get(f, &uri(route, &params)).await;
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

#[tokio::test]
async fn existence_core_service_cli_http_preserve_independent_exact_episodes_and_history() {
    let f = Fixture::new();
    let b = binding("governance-policy");
    let result = parity(&f, "existence", CHARACTER, &b, Some(1)).await;
    assert_eq!(result["at"]["tick"], TICK.to_string());
    assert_eq!(result["count"], 3);
    assert_eq!(
        result["items"][0]["valid"]["start"]["tick"],
        TICK.to_string()
    );
    assert_eq!(result["items"][0]["startEvidence"], evidence());
    assert_eq!(
        result["items"][0]["evidenceStatus"],
        "unverified-historical"
    );
    assert_eq!(result["items"][2]["episodeKind"], "unknown");
    assert_eq!(result["items"][2]["startEvidence"], json!([]));
    for row in result["items"].as_array().unwrap() {
        assert!(row.get("body").is_none() && row.get("characterTitle").is_none());
        assert!(row["citation"].to_string().contains("story/existence-"));
    }
    for (order, count) in [(-1, 1), (0, 3), (2, 3), (3, 1)] {
        assert_eq!(
            parity(&f, "existence", CHARACTER, &b, Some(order)).await["count"],
            count
        );
    }
    let history = parity(&f, "existence", CHARACTER, &b, None).await;
    assert_eq!(history["count"], 4);
    assert_eq!(history["scope"], "author-all-time-existence");
    assert_eq!(history["at"], Value::Null);
    assert!(!history.to_string().contains("existence-5"));
}

#[tokio::test]
async fn governance_core_service_cli_http_preserve_roles_accounts_parties_and_null() {
    let f = Fixture::new();
    let b = binding("governance-policy");
    let result = parity(&f, "governance", LOCATION, &b, Some(1)).await;
    assert_eq!(result["at"]["tick"], TICK.to_string());
    assert_eq!(result["count"], 3);
    let text = result.to_string();
    assert!(
        text.contains("9007199254740993")
            && text.contains("Revised rule")
            && text.contains("Alternative rule")
    );
    assert!(!text.contains("Old rule") && !text.contains("hidden-character"));
    assert!(result["items"].as_array().unwrap().iter().all(|row| {
        row["conceptId"] != id("concept", 5) && row["conceptId"] != id("concept", 6)
    }));
    assert!(
        text.contains("organization")
            && text.contains(ORGANIZATION)
            && text.contains(&id("concept", 50))
    );
    let first = &result["items"][0];
    assert_eq!(first["conceptId"], id("concept", 1));
    assert_eq!(first["roleKnown"]["obligations"], true);
    assert_eq!(result["items"][1]["roleKnown"]["obligations"], false);
    assert!(result["items"][1].get("supersedesCitation").is_some());
    let nullable = parity(
        &f,
        "governance",
        LOCATION,
        &binding("governance-null"),
        Some(1),
    )
    .await;
    assert_eq!(nullable["count"], 1);
    assert_eq!(nullable["items"][0]["roleKnown"]["obligations"], true);
    assert_eq!(nullable["items"][0]["roles"]["obligations"], Value::Null);
    let history = parity(&f, "governance", LOCATION, &b, None).await;
    assert_eq!(history["count"], 2);
    assert!(history.to_string().contains("Old rule"));
    assert!(!history.to_string().contains("concept-3"));
    assert_eq!(
        parity(&f, "governance", OVERLAY, &b, Some(1)).await["count"],
        1
    );
    assert_eq!(
        parity(&f, "governance", OVERLAY, &b, None).await["count"],
        0
    );
    for (order, count) in [(-1, 0), (0, 3), (2, 3), (3, 0)] {
        assert_eq!(
            parity(&f, "governance", LOCATION, &b, Some(order)).await["count"],
            count
        );
    }
}

#[tokio::test]
async fn fact_gets_and_cli_reject_partial_horizons_extras_duplicate_controls_and_bad_bounds() {
    let f = Fixture::new();
    let b = binding("governance-policy");
    for domain in ["existence", "governance"] {
        let route = if domain == "existence" {
            "existence/list"
        } else {
            "governance/query"
        };
        let mut base = if domain == "existence" {
            vec![("character", CHARACTER.into())]
        } else {
            vec![
                ("target", LOCATION.into()),
                ("profile", b.profile.clone()),
                ("roles", serde_json::to_string(&b.roles).unwrap()),
            ]
        };
        let mut cases = vec![
            ("viewer", CHARACTER),
            ("mode", "character"),
            ("owner", CHARACTER),
            ("profile_version", "1"),
            ("order", "0"),
            ("timeline", "main"),
            ("audience", ""),
            ("view", "\n"),
            ("limit", "0"),
            ("limit", "1001"),
            ("limit", "1.0"),
            ("tick", "1.0"),
        ];
        cases.push(if domain == "existence" {
            ("target", LOCATION)
        } else {
            ("character", CHARACTER)
        });
        for (key, value) in cases {
            let mut params = base.clone();
            params.push((key, value.into()));
            let (status, body) = get(&f, &uri(route, &params)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{domain} {key} {body}");
        }
        for extra in [
            [("tick", TICK.to_string()), ("timeline", "missing".into())],
            [("tick", TICK.to_string()), ("order", "2147483648".into())],
        ] {
            let mut params = base.clone();
            params.extend(extra);
            assert_eq!(
                get(&f, &uri(route, &params)).await.0,
                StatusCode::BAD_REQUEST
            );
        }
        base.extend([("limit", "100".into()), ("limit", "100".into())]);
        assert_eq!(get(&f, &uri(route, &base)).await.0, StatusCode::BAD_REQUEST);
        let mut cli_base = vec![
            domain.into(),
            if domain == "existence" {
                "list".into()
            } else {
                "query".into()
            },
            if domain == "existence" {
                CHARACTER.into()
            } else {
                LOCATION.into()
            },
        ];
        if domain == "governance" {
            cli_base.extend([
                "--profile".into(),
                b.profile.clone(),
                "--roles".into(),
                serde_json::to_string(&b.roles).unwrap(),
            ]);
        }
        for args in [
            ["--order", "0"],
            ["--timeline", "main"],
            ["--viewer", CHARACTER],
            ["--limit", "0"],
            ["--profile-version", "1"],
        ] {
            let mut selected = cli_base.clone();
            selected.extend(args.map(String::from));
            assert!(!cli(&f, &selected).status.success(), "{domain} {args:?}");
        }
    }
    for (path, params) in [
        ("existence/list", vec![]),
        ("governance/query", vec![("target", LOCATION.into())]),
    ] {
        assert_eq!(
            get(&f, &uri(path, &params)).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    for path in ["existence/query", "governance/list"] {
        assert_eq!(
            get(&f, &format!("/api/{path}")).await.0,
            StatusCode::NOT_FOUND
        );
    }
    assert!(
        existence::list(
            &f.world(),
            CHARACTER,
            Some(&point(1)),
            "author",
            "author",
            1
        )
        .is_err()
    );
    assert!(
        governance::query(
            &f.world(),
            LOCATION,
            &b,
            Some(&point(1)),
            "author",
            "author",
            1
        )
        .is_err()
    );
}

#[tokio::test]
async fn governance_role_json_is_closed_complete_unique_and_strict_in_all_transports() {
    let f = Fixture::new();
    let good = serde_json::to_value(binding("governance-policy").roles).unwrap();
    let mut malformed=vec!["{}".into(),"[]".into(),"null".into(),"{territory: area}".into(),format!("{good} trailing"),format!("{good}\n{{}}"),r#"{"territory":"area","territory":null,"party":null,"basis":null,"subjects":null,"obligations":null,"supersedes":null}"#.into()];
    for (key, value) in [
        ("unexpected", json!(null)),
        ("territory", json!(3)),
        ("party", json!(["issuer"])),
        ("basis", json!({"field":"basis"})),
        ("territory", json!("area.child")),
        ("party", json!("area")),
        ("territory", json!("")),
    ] {
        let mut modified = good.clone();
        modified[key] = value;
        malformed.push(modified.to_string());
    }
    for roles in malformed {
        assert!(
            service::governance_binding_json("governance-policy", &roles).is_err(),
            "{roles}"
        );
        let params = [
            ("target", LOCATION.into()),
            ("profile", "governance-policy".into()),
            ("roles", roles.clone()),
        ];
        assert_eq!(
            get(&f, &uri("governance/query", &params)).await.0,
            StatusCode::BAD_REQUEST,
            "{roles}"
        );
        assert!(
            !cli(
                &f,
                &[
                    "governance".into(),
                    "query".into(),
                    LOCATION.into(),
                    "--profile".into(),
                    "governance-policy".into(),
                    "--roles".into(),
                    roles
                ]
            )
            .status
            .success()
        );
    }
    for profile in ["", " governance-policy", "profile.version"] {
        assert!(
            service::governance_binding_json(profile, &good.to_string()).is_err(),
            "{profile}"
        );
    }
    let all_null = r#"{"territory":null,"party":null,"basis":null,"subjects":null,"obligations":null,"supersedes":null}"#;
    let unbound = service::governance_binding_json("governance-policy", all_null).unwrap();
    let result = parity(&f, "governance", LOCATION, &unbound, Some(1)).await;
    assert_eq!(result["count"], 4);
    for row in result["items"].as_array().unwrap() {
        assert_eq!(row["jurisdiction"], "unknown");
        assert!(
            row["roleKnown"]
                .as_object()
                .unwrap()
                .values()
                .all(|known| known == false)
        );
        assert_eq!(row["references"], json!([]));
    }
}

#[tokio::test]
async fn missing_hidden_wrong_kind_and_unknown_bindings_disclose_no_target_labels() {
    let f = Fixture::new();
    let b = binding("governance-policy");
    for (domain, targets, key) in [
        (
            "existence",
            vec!["missing", HIDDEN_CHARACTER, LOCATION, "character"],
            "characterId",
        ),
        (
            "governance",
            vec!["missing", HIDDEN_LOCATION, CHARACTER, "location"],
            "targetId",
        ),
    ] {
        let mut empty = None;
        for target in targets {
            let mut result = parity(&f, domain, target, &b, Some(1)).await;
            assert_eq!(result["count"], 0);
            assert_eq!(result["items"], json!([]));
            result.as_object_mut().unwrap().remove(key);
            if let Some(expected) = &empty {
                assert_eq!(&result, expected);
            } else {
                empty = Some(result);
            }
        }
    }
    assert_eq!(
        parity(
            &f,
            "governance",
            LOCATION,
            &binding("unknown-profile"),
            Some(1)
        )
        .await["count"],
        0
    );
    let mut incompatible = b.clone();
    incompatible
        .roles
        .insert("territory".into(), Some("duties".into()));
    incompatible.roles.insert("obligations".into(), None);
    assert_eq!(
        parity(&f, "governance", LOCATION, &incompatible, Some(1)).await["count"],
        0
    );
}

#[tokio::test]
async fn author_facts_preserve_source_and_leave_complete_character_responses_unchanged() {
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
            query: "Revised".into(),
            at: time(1),
            limit: 20,
        },
    ];
    let baseline = operations
        .iter()
        .cloned()
        .map(|o| format!("{:?}", f.service().execute(o)))
        .collect::<Vec<_>>();
    let b = binding("governance-policy");
    parity(&f, "existence", CHARACTER, &b, Some(1)).await;
    parity(&f, "governance", LOCATION, &b, Some(1)).await;
    let after = f.world();
    assert_eq!(before.fingerprint, after.fingerprint);
    for (id, record) in &before.records {
        assert_eq!(record.raw_bytes, after.records[id].raw_bytes);
    }
    for (path, old, new) in [
        (
            "existence-1",
            "Authored account",
            "Changed existence account",
        ),
        ("assertion-2", "Revised rule", "Changed policy rule"),
        (
            "hidden-character",
            "hidden-character",
            "Changed private label",
        ),
    ] {
        let file = f.root().join(format!("story/{path}.md"));
        fs::write(&file, fs::read_to_string(&file).unwrap().replace(old, new)).unwrap();
    }
    assert!(validation::validate_world(&f.world()).valid);
    assert_ne!(before.fingerprint, f.world().fingerprint);
    for (i, operation) in operations.into_iter().enumerate() {
        assert_eq!(baseline[i], format!("{:?}", f.service().execute(operation)));
    }
    let snapshot = Service::new(before);
    let operation = Operation::GovernanceQuery {
        target: LOCATION.into(),
        binding: b,
        at: Some(time(1)),
        audience: "author".into(),
        view: "author".into(),
        limit: 100,
    };
    let old = snapshot.execute(operation.clone()).unwrap();
    let current = f.service().execute(operation).unwrap();
    assert!(
        old.to_string().contains("Revised rule")
            && !old.to_string().contains("Changed policy rule")
    );
    assert!(
        current.to_string().contains("Changed policy rule")
            && !current.to_string().contains("Revised rule")
    );
}
