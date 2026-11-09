//! Profile-bound state and explicit species composition share one contract across transports.
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path, process::Command};
use tempfile::TempDir;
use tower::ServiceExt;
use wedl::{
    http, lore_state,
    model::{StoryTime, World},
    service::{Operation, Service, TimeInput},
    source, species, validation,
};

const VIEWER: &str = "char_00000000000000000000000001";
const COMMON: &str = "concept_00000000000000000000000001";
const PARENT: &str = "concept_00000000000000000000000002";
const HIDDEN: &str = "concept_00000000000000000000000003";
const INDIVIDUAL: &str = "concept_00000000000000000000000004";
const OTHER: &str = "concept_00000000000000000000000005";
const FUTURE: &str = "concept_00000000000000000000000006";
const HIDDEN_INDIVIDUAL: &str = "concept_00000000000000000000000007";
const TICK: i64 = 9_007_199_254_740_993;
const MAPS: &str = r#"{"traits":"traits","life_stage":"life_stage","culture":"opaque"}"#;

struct Fixture(TempDir);
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("story")).unwrap();
        let result = Self(dir);
        result.write("world", "world_00000000000000000000000000", "world", "Profile readers", &format!(r#"capabilities: [lore-core-v1]
default_timeline: main
timelines: [{{id: main, label: Main}}, {{id: other, label: Other}}]
state_keys:
  character:
    details: {{type: string}}
    ordinary: {{type: string}}
state_profile_bindings:
  character: {{details: bound-details}}
profiles:
  - id: bound-details
    label: Bound details
    fields:
      type: object
      required:
        note: {{type: string}}
        amount: {{type: number}}
      optional:
        choice: {{type: enum, values: [1, '1']}}
        origin: {{type: 'null'}}
        reference: {{type: entity, kinds: [lore-concept]}}
        refs: {{type: array, items: {{type: entity, kinds: [lore-concept]}}, max_items: 8}}
  - id: common-description
    label: Common description
    fields:
      type: object
      required:
        traits: {{type: string}}
      optional:
        parents: {{type: array, items: {{type: entity, kinds: [lore-concept]}}, max_items: 8}}
        life_stage: {{type: 'null'}}
        opaque: {{type: object, required: {{kind: {{type: string}}, id: {{type: string}}}}, optional: {{}}}}
  - id: individual-variation
    label: Individual variation
    fields:
      type: object
      required:
        common: {{type: entity, kinds: [lore-concept]}}
        traits: {{type: string}}
      optional: {{}}
"#));
        result.write("character", VIEWER, "character", "Mira", &format!(
            "initial_state:\n  ordinary: A public unbound note\n  details:\n    note: Bound initial note\n    amount: 18446744073709551615\n    reference: {{kind: record, id: {HIDDEN}}}\n    refs: [{{kind: record, id: {HIDDEN}}}, {{kind: record, id: {PARENT}}}]\n"));
        result.write("knowledge", "know_00000000000000000000000001", "knowledge", "Known lamp", &format!("knower: {VIEWER}\nclaim: {{key: lamp, statement: I saw a lamp}}\ntransitions: [{{id: learned, time: {{tick: 0}}, state: accepted, acquisition: observed, confidence: 1}}]\n"));
        result.concept("common", COMMON, "Common description", "common-description", &format!("traits: Authored common description, life_stage: null, parents: [{{kind: record, id: {HIDDEN}}}, {{kind: record, id: {PARENT}}}], opaque: {{kind: record, id: {HIDDEN}}}"), "author", "{kind: timeless}");
        result.concept(
            "parent",
            PARENT,
            "Parent description",
            "common-description",
            "traits: Authored parent description, parents: []",
            "author",
            "{kind: timeless}",
        );
        result.concept(
            "hidden",
            HIDDEN,
            "Hidden description",
            "common-description",
            "traits: Hidden common words, parents: []",
            "editor",
            "{kind: timeless}",
        );
        for (path, id, title, audience, valid) in [
            (
                "individual",
                INDIVIDUAL,
                "Mira variation",
                "author",
                "{kind: timeless}".to_owned(),
            ),
            (
                "other",
                OTHER,
                "Other variation",
                "author",
                "{kind: timeless}".to_owned(),
            ),
            (
                "future",
                FUTURE,
                "Future variation",
                "author",
                format!(
                    "{{kind: interval, timeline: main, start: {{timeline: main, tick: {}, order: 0}}}}",
                    TICK + 1
                ),
            ),
            (
                "hidden-individual",
                HIDDEN_INDIVIDUAL,
                "Private variation",
                "editor",
                "{kind: timeless}".to_owned(),
            ),
        ] {
            result.concept(
                path,
                id,
                title,
                "individual-variation",
                &format!(
                    "common: {{kind: record, id: {COMMON}}}, traits: Individual authored variation"
                ),
                audience,
                &valid,
            );
        }
        result.write("set", "event_00000000000000000000000001", "event", "Set bound details", &format!("time: {{timeline: main, tick: {TICK}, order: 0}}\nparticipants: []\ncauses: []\neffects:\n  - id: effect_00000000000000000000000001\n    target: {VIEWER}\n    key: details\n    operation: set\n    value: {{note: Updated bound note, amount: {TICK}, choice: 1, origin: null, reference: {{kind: record, id: {COMMON}}}}}\n"));
        result.write("clear", "event_00000000000000000000000002", "event", "Clear bound details", &format!("time: {{timeline: main, tick: {TICK}, order: 1}}\nparticipants: []\ncauses: []\neffects:\n  - id: effect_00000000000000000000000002\n    target: {VIEWER}\n    key: details\n    operation: clear\n"));
        let world = result.world();
        let report = validation::validate_world(&world);
        assert!(report.valid, "{:?}", report.diagnostics);
        result
    }
    fn root(&self) -> &Path {
        self.0.path()
    }
    fn write(&self, path: &str, id: &str, kind: &str, title: &str, fields: &str) {
        fs::write(self.root().join(format!("story/{path}.md")),format!("---\nschema: wedl/v1\nid: {id}\nkind: {kind}\ntitle: {title}\nstatus: canonical\naliases: []\ntags: []\n{fields}---\nAuthored source prose\n")).unwrap();
    }
    fn concept(
        &self,
        path: &str,
        id: &str,
        title: &str,
        profile: &str,
        values: &str,
        audience: &str,
        valid: &str,
    ) {
        self.write(path,id,"lore-concept",title,&format!("domain: species.description\nprofile: {profile}\nvalues: {{{values}}}\nvalid: {valid}\naudience: [{audience}]\nperspectives: [author]\n"));
    }
    fn world(&self) -> World {
        source::load_world(self.root(), None).unwrap()
    }
    fn service(&self) -> Service {
        Service::new(self.world())
    }
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
fn selection() -> species::Selection {
    species::Selection {
        common_profile: "common-description".into(),
        individual_profile: "individual-variation".into(),
        individual_link: "common".into(),
        common_link: Some("parents".into()),
        common_fields: serde_json::from_str(MAPS).unwrap(),
        individual_fields: BTreeMap::from([("traits".into(), "traits".into())]),
    }
}
fn uri(path: &str, values: &[(&str, String)]) -> String {
    fn escaped(value: &str) -> String {
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
        values
            .iter()
            .map(|(key, value)| format!("{}={}", escaped(key), escaped(value)))
            .collect::<Vec<_>>()
            .join("&")
    )
}
fn species_params(id: &str) -> Vec<(&'static str, String)> {
    vec![
        ("id", id.into()),
        ("common_profile", "common-description".into()),
        ("individual_profile", "individual-variation".into()),
        ("individual_link", "common".into()),
        ("common_link", "parents".into()),
        ("common_fields", MAPS.into()),
        ("individual_fields", r#"{"traits":"traits"}"#.into()),
        ("tick", TICK.to_string()),
        ("order", "0".into()),
    ]
}
fn species_args(operation: &str, id: &str) -> Vec<String> {
    vec![
        "species",
        operation,
        id,
        "--common-profile",
        "common-description",
        "--individual-profile",
        "individual-variation",
        "--individual-link",
        "common",
        "--common-link",
        "parents",
        "--common-fields",
        MAPS,
        "--individual-fields",
        r#"{"traits":"traits"}"#,
        "--tick",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain([TICK.to_string(), "--order".into(), "0".into()])
    .collect()
}
fn cli(root: &Path, args: &[String]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .args(["--compact", "--repo", root.to_str().unwrap()])
        .args(args)
        .output()
        .unwrap()
}
async fn get(service: Service, uri: &str) -> (StatusCode, Value) {
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
    assert_eq!(response.headers()["cache-control"], "no-store");
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
async fn parity(
    f: &Fixture,
    operation: Operation,
    core: Value,
    path: &str,
    args: &[String],
) -> Value {
    let expected = f.service().execute(operation).unwrap();
    assert_eq!(expected["result"], core);
    let (status, actual) = get(f.service(), path).await;
    assert_eq!(status, StatusCode::OK, "{actual}");
    assert_eq!(actual, expected);
    let output = cli(f.root(), args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        actual
    );
    actual["result"].clone()
}

#[tokio::test]
async fn bound_state_core_service_cli_http_preserve_exact_time_values_and_sources() {
    let f = Fixture::new();
    let world = f.world();
    for order in [-1, 0, 1] {
        let args = vec![
            "lore".into(),
            "state".into(),
            VIEWER.into(),
            "--tick".into(),
            TICK.to_string(),
            "--order".into(),
            order.to_string(),
        ];
        let path = uri(
            "lore/state",
            &[
                ("record", VIEWER.into()),
                ("tick", TICK.to_string()),
                ("order", order.to_string()),
            ],
        );
        let read = parity(
            &f,
            Operation::LoreState {
                record: VIEWER.into(),
                at: time(order),
                audience: "author".into(),
                view: "author".into(),
            },
            lore_state::state(&world, VIEWER, &point(order), "author", "author").unwrap(),
            &path,
            &args,
        )
        .await;
        assert_eq!(read["at"]["tick"], TICK.to_string());
        match order {
            -1 => {
                assert_eq!(read["state"]["details"]["amount"], u64::MAX.to_string());
                assert!(read["state"]["details"].get("reference").is_none());
                assert_eq!(
                    read["references"][0]["sourceField"],
                    "initial_state.details.refs[1].id"
                );
            }
            0 => {
                assert_eq!(read["state"]["details"]["amount"], TICK.to_string());
                assert_eq!(
                    read["valueTypes"]["details"]["fields"]["choice"]["scalarType"],
                    "number"
                );
                assert!(read["state"]["details"]["origin"].is_null());
                assert_eq!(read["citations"]["details"]["section"], "effects[0].value");
            }
            _ => {
                assert_eq!(read["state"], json!({}));
                assert_eq!(read["valueTypes"], json!({}));
                assert_eq!(read["citations"]["details"]["section"], "effects[0]");
                assert_eq!(
                    read["citations"]["details"]["effectId"],
                    "effect_00000000000000000000000002"
                );
            }
        }
    }
}

#[tokio::test]
async fn species_compose_transports_keep_common_and_individual_statements_separate() {
    let f = Fixture::new();
    let world = f.world();
    let selection = selection();
    let read = parity(
        &f,
        Operation::SpeciesCompose {
            individual: INDIVIDUAL.into(),
            selection: selection.clone(),
            at: Some(time(0)),
            audience: "author".into(),
            view: "author".into(),
        },
        species::compose(
            &world,
            INDIVIDUAL,
            &selection,
            Some(&point(0)),
            "author",
            "author",
        )
        .unwrap(),
        &uri("species/compose", &species_params(INDIVIDUAL)),
        &species_args("compose", INDIVIDUAL),
    )
    .await;
    assert_eq!(read["individual"]["conceptId"], INDIVIDUAL);
    assert_eq!(read["common"].as_array().unwrap().len(), 2);
    assert_eq!(read["common"][0]["conceptId"], COMMON);
    assert_eq!(read["common"][1]["conceptId"], PARENT);
    assert_eq!(
        read["individual"]["fields"]["traits"]["value"],
        "Individual authored variation"
    );
    assert_eq!(
        read["common"][0]["fields"]["traits"]["value"],
        "Authored common description"
    );
    assert!(read["common"][0]["fields"]["life_stage"]["value"].is_null());
    assert_eq!(
        read["common"][0]["fields"]["culture"]["value"]["id"],
        HIDDEN
    );
    assert_eq!(read["common"][0]["bodyKind"], "undated-authored-prose");
    assert!(!read.to_string().contains("Hidden common words"));
}

#[tokio::test]
async fn affected_counts_apply_scope_and_horizon_before_complete_limits() {
    let f = Fixture::new();
    let world = f.world();
    let selected = selection();
    let mut params = species_params(PARENT);
    params.push(("limit", "2".into()));
    let mut args = species_args("affected", PARENT);
    args.extend(["--limit".into(), "2".into()]);
    let read = parity(
        &f,
        Operation::SpeciesAffected {
            common: PARENT.into(),
            selection: selected.clone(),
            at: Some(time(0)),
            audience: "author".into(),
            view: "author".into(),
            limit: 2,
        },
        species::affected(
            &world,
            PARENT,
            &selected,
            Some(&point(0)),
            "author",
            "author",
            2,
        )
        .unwrap(),
        &uri("species/affected", &params),
        &args,
    )
    .await;
    assert_eq!(read["count"], 2);
    assert_eq!(
        read["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["conceptId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![INDIVIDUAL, OTHER]
    );
    params.retain(|(key, _)| !matches!(*key, "tick" | "order"));
    assert_eq!(
        get(f.service(), &uri("species/affected", &params)).await.0,
        StatusCode::BAD_REQUEST
    );
    params
        .iter_mut()
        .find(|(key, _)| *key == "limit")
        .unwrap()
        .1 = "3".into();
    let (status, all) = get(f.service(), &uri("species/affected", &params)).await;
    assert_eq!(status, StatusCode::OK, "{all}");
    assert_eq!(all["result"]["count"], 3);
}

#[tokio::test]
async fn routes_refuse_partial_horizons_duplicate_mappings_unknown_roles_and_grants() {
    let f = Fixture::new();
    for query in [
        vec![("record", VIEWER.into())],
        vec![
            ("record", VIEWER.into()),
            ("tick", "0".into()),
            ("limit", "1".into()),
        ],
        vec![("record", VIEWER.into()), ("tick", "0.5".into())],
        vec![
            ("record", VIEWER.into()),
            ("tick", "9223372036854775808".into()),
        ],
        vec![
            ("record", VIEWER.into()),
            ("tick", "0".into()),
            ("order", "2147483648".into()),
        ],
    ] {
        assert_eq!(
            get(f.service(), &uri("lore/state", &query)).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    let base = species_params(INDIVIDUAL);
    for (key, value) in [
        (
            "common_fields",
            r#"{"traits":"traits","traits":"life_stage"}"#,
        ),
        ("common_fields", r#"{"powers":"traits"}"#),
        ("common_fields", r#"{"traits":1}"#),
        ("common_fields", "[]"),
        ("common_fields", "null"),
        ("common_fields", r#"{"traits":"values.traits"}"#),
        ("common_link", "parents.child"),
        ("limit", "1"),
        ("viewer", VIEWER),
        ("known_records", COMMON),
        ("order", "2147483648"),
        ("timeline", "missing"),
    ] {
        let mut params = base.clone();
        params.retain(|(name, _)| *name != key);
        params.push((key, value.into()));
        let (status, value) = get(f.service(), &uri("species/compose", &params)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{key}: {value}");
    }
    for key in [
        "common_profile",
        "individual_profile",
        "individual_link",
        "tick",
    ] {
        let mut params = base.clone();
        params.retain(|(name, _)| *name != key);
        assert_eq!(
            get(f.service(), &uri("species/compose", &params)).await.0,
            StatusCode::BAD_REQUEST,
            "missing {key}"
        );
    }
    for limit in ["0", "1001"] {
        let mut params = species_params(COMMON);
        params.push(("limit", limit.into()));
        assert_eq!(
            get(f.service(), &uri("species/affected", &params)).await.0,
            StatusCode::BAD_REQUEST
        );
    }
}

#[tokio::test]
async fn missing_and_hidden_state_owners_match_and_generic_state_omits_bound_citations() {
    let f = Fixture::new();
    let mut failures = Vec::new();
    for id in [HIDDEN, "concept_00000000000000000000000999"] {
        failures.push(
            get(
                f.service(),
                &uri(
                    "lore/state",
                    &[("record", id.into()), ("tick", TICK.to_string())],
                ),
            )
            .await,
        );
    }
    assert_eq!(failures[0], failures[1]);
    assert_eq!(failures[0].0, StatusCode::NOT_FOUND);
    let (status, ordinary) = get(
        f.service(),
        &uri(
            "state",
            &[
                ("entity", VIEWER.into()),
                ("tick", TICK.to_string()),
                ("order", "0".into()),
            ],
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{ordinary}");
    assert_eq!(
        ordinary["result"]["state"],
        json!({"ordinary":"A public unbound note"})
    );
    assert!(ordinary["result"]["citations"].get("details").is_none());
    let (_, scoped) = get(
        f.service(),
        &uri(
            "lore/state",
            &[
                ("record", VIEWER.into()),
                ("tick", TICK.to_string()),
                ("order", "0".into()),
                ("audience", "editor".into()),
            ],
        ),
    )
    .await;
    assert!(
        scoped["result"]["state"]["details"]
            .get("reference")
            .is_none()
    );
}

#[tokio::test]
async fn canonical_bound_state_and_common_descriptions_do_not_change_character_responses() {
    let f = Fixture::new();
    let paths = [
        uri(
            "character/context",
            &[
                ("character", VIEWER.into()),
                ("tick", TICK.to_string()),
                ("order", "0".into()),
                ("maxCharacters", "100000".into()),
            ],
        ),
        uri(
            "character/search",
            &[
                ("character", VIEWER.into()),
                ("tick", TICK.to_string()),
                ("q", "lamp".into()),
            ],
        ),
        uri(
            "character/context",
            &[
                ("character", VIEWER.into()),
                ("tick", TICK.to_string()),
                ("maxCharacters", "256".into()),
            ],
        ),
    ];
    let mut before = Vec::new();
    for path in &paths {
        before.push(get(f.service(), path).await);
    }
    let path = f.root().join("story/set.md");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("Updated bound note", "Changed secret bound note"),
    )
    .unwrap();
    let path = f.root().join("story/common.md");
    fs::write(
        &path,
        fs::read_to_string(&path).unwrap().replace(
            "Authored common description",
            "Changed canonical description",
        ),
    )
    .unwrap();
    assert!(validation::validate_world(&f.world()).valid);
    for (index, path) in paths.iter().enumerate() {
        assert_eq!(get(f.service(), path).await, before[index]);
    }
}

#[test]
fn cli_requires_explicit_bindings_and_exact_horizons_without_accepting_duplicate_maps() {
    let f = Fixture::new();
    for args in [
        vec!["lore", "state", VIEWER],
        vec!["species", "compose", INDIVIDUAL],
        vec![
            "species",
            "affected",
            COMMON,
            "--common-profile",
            "common-description",
            "--individual-profile",
            "individual-variation",
            "--individual-link",
            "common",
            "--timeline",
            "main",
        ],
    ] {
        assert!(
            !cli(
                f.root(),
                &args.into_iter().map(str::to_owned).collect::<Vec<_>>()
            )
            .status
            .success()
        );
    }
    let mut args = species_args("compose", INDIVIDUAL);
    let at = args.iter().position(|v| v == "--common-fields").unwrap() + 1;
    args[at] = r#"{"traits":"traits","traits":"life_stage"}"#.into();
    let output = cli(f.root(), &args);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("duplicate"));
    let selected = wedl::service::species_fields_json(r#"{"tr\u0061its":"tr\u0061its"}"#).unwrap();
    assert_eq!(
        selected,
        BTreeMap::from([("traits".into(), "traits".into())])
    );
}
