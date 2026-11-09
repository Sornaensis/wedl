//! Exact magic selections and authored values share one read contract across transports.
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
    http, magic,
    model::{StoryTime, World},
    service::{Operation, Service, TimeInput, magic_selection_json},
    source, validation,
};

const VIEWER: &str = "char_00000000000000000000000001";
const TRADITION: &str = "concept_00000000000000000000000001";
const PRACTICE: &str = "concept_00000000000000000000000002";
const REALM: &str = "concept_00000000000000000000000003";
const ARTIFACT: &str = "concept_00000000000000000000000004";
const HIDDEN: &str = "concept_00000000000000000000000005";
const OTHER_TIME: &str = "concept_00000000000000000000000006";
const MISSING: &str = "concept_00000000000000000000000999";
const TICK: i64 = 9_007_199_254_740_993;
const PROFILES: &str =
    r#"{"tradition":"tradition","practice":"practice","realm":"realm","artifact":"artifact"}"#;
const LINKS: &str = r#"{"tradition":["practices","realm"],"practice":["artifact"]}"#;
const FIELDS: &str = r#"{"tradition":"restrictions","practice":"restrictions","realm":"restrictions","artifact":"restrictions"}"#;
const EXCEPTIONS: &str = r#"{"tradition":"exceptions","practice":"exceptions","realm":"exceptions","artifact":"exceptions"}"#;

struct Fixture(TempDir);
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("story")).unwrap();
        let f = Self(dir);
        f.write(
            "world",
            "world_00000000000000000000000000",
            "world",
            "Magic readers",
            r#"capabilities: [lore-core-v1]
default_timeline: main
timelines: [{id: main, label: Main}, {id: other, label: Other}]
profiles:
  - id: tradition
    label: Tradition
    fields:
      type: object
      required:
        practices: {type: array, items: {type: entity, kinds: [lore-concept]}, max_items: 8}
        realm: {type: entity, kinds: [lore-concept]}
        restrictions: {type: string}
      optional:
        exceptions: {type: string}
  - id: practice
    label: Practice
    fields:
      type: object
      required:
        artifact: {type: entity, kinds: [lore-concept]}
        restrictions: {type: enum, values: [1, '1']}
        exceptions: {type: 'null'}
      optional: {}
  - id: realm
    label: Realm
    fields:
      type: object
      required:
        restrictions: {type: array, items: {type: string}, max_items: 8}
      optional:
        exceptions: {type: string}
  - id: artifact
    label: Artifact
    fields:
      type: object
      required:
        restrictions: {type: object, required: {kind: {type: string}, id: {type: string}}, optional: {}}
        exceptions: {type: number}
      optional:
        next: {type: entity, kinds: [lore-concept]}
"#,
        );
        f.write("character", VIEWER, "character", "Mira", "");
        f.write(
            "knowledge",
            "know_00000000000000000000000001",
            "knowledge",
            "Known lamp",
            &format!("knower: {VIEWER}\nclaim: {{key: lamp, statement: I saw a lamp}}\ntransitions: [{{id: learned, time: {{tick: 0}}, state: accepted, acquisition: observed, confidence: 1}}]\n"),
        );
        f.concept(
            "tradition", TRADITION, "Dusk tradition", "tradition",
            &format!("practices: [{{kind: record, id: {HIDDEN}}}, {{kind: record, id: {PRACTICE}}}, {{kind: record, id: {OTHER_TIME}}}], realm: {{kind: record, id: {REALM}}}, restrictions: Only at dusk"),
            "author", "{kind: timeless}",
        );
        f.concept(
            "practice", PRACTICE, "Dated practice", "practice",
            &format!("artifact: {{kind: record, id: {ARTIFACT}}}, restrictions: 1, exceptions: null"),
            "author", &format!("{{kind: interval, timeline: main, start: {{timeline: main, tick: {TICK}, order: 0}}, end: {{timeline: main, tick: {TICK}, order: 2}}}}"),
        );
        f.concept(
            "realm",
            REALM,
            "Open realm",
            "realm",
            "restrictions: []",
            "author",
            "{kind: timeless}",
        );
        f.concept(
            "artifact",
            ARTIFACT,
            "Counted artifact",
            "artifact",
            &format!(
                "restrictions: {{kind: record, id: {HIDDEN}}}, exceptions: 18446744073709551615"
            ),
            "author",
            "{kind: timeless}",
        );
        f.concept(
            "hidden",
            HIDDEN,
            "Private practice",
            "practice",
            &format!(
                "artifact: {{kind: record, id: {ARTIFACT}}}, restrictions: '1', exceptions: null"
            ),
            "editor",
            "{kind: timeless}",
        );
        f.concept(
            "other-time",
            OTHER_TIME,
            "Other timeline practice",
            "practice",
            &format!(
                "artifact: {{kind: record, id: {ARTIFACT}}}, restrictions: '1', exceptions: null"
            ),
            "author",
            "{kind: interval, timeline: other, start: {timeline: other, tick: 0, order: 0}}",
        );
        let report = validation::validate_world(&f.world());
        assert!(report.valid, "{:?}", report.diagnostics);
        f
    }
    fn root(&self) -> &Path {
        self.0.path()
    }
    fn write(&self, path: &str, id: &str, kind: &str, title: &str, fields: &str) {
        fs::write(
            self.root().join(format!("story/{path}.md")),
            format!("---\nschema: wedl/v1\nid: {id}\nkind: {kind}\ntitle: {title}\nstatus: canonical\naliases: []\ntags: []\n{fields}---\nAuthored {title} prose\n"),
        ).unwrap();
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
        self.write(
            path, id, "lore-concept", title,
            &format!("domain: magic.narrative\nprofile: {profile}\nvalues: {{{values}}}\nvalid: {valid}\naudience: [{audience}]\nperspectives: [author]\n"),
        );
    }
    fn world(&self) -> World {
        source::load_world(self.root(), None).unwrap()
    }
    fn service(&self) -> Service {
        Service::new(self.world())
    }
}

fn selection() -> magic::Selection {
    magic_selection_json(PROFILES, Some(LINKS), Some(FIELDS), Some(EXCEPTIONS)).unwrap()
}
fn params(id: &str) -> Vec<(&'static str, String)> {
    vec![
        ("id", id.into()),
        ("profiles", PROFILES.into()),
        ("links", LINKS.into()),
        ("restrictions", FIELDS.into()),
        ("exceptions", EXCEPTIONS.into()),
    ]
}
fn args(id: &str) -> Vec<String> {
    [
        "magic",
        "compose",
        id,
        "--profiles",
        PROFILES,
        "--links",
        LINKS,
        "--restrictions",
        FIELDS,
        "--exceptions",
        EXCEPTIONS,
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
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
            .map(|(k, v)| { format!("{}={}", escaped(k), escaped(v)) })
            .collect::<Vec<_>>()
            .join("&")
    )
}
fn cli(f: &Fixture, args: &[String]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .args(["--compact", "--repo", f.root().to_str().unwrap()])
        .args(args)
        .output()
        .unwrap()
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
async fn parity(f: &Fixture, at: Option<(&str, i64, i32)>) -> Value {
    let selected = selection();
    let world = f.world();
    let point = at.map(|(timeline, tick, order)| StoryTime::new(timeline, tick, order).unwrap());
    let expected = f
        .service()
        .execute(Operation::MagicCompose {
            root: TRADITION.into(),
            selection: selected.clone(),
            at: at.map(|(timeline, tick, order)| TimeInput {
                tick,
                order,
                timeline: Some(timeline.into()),
            }),
            audience: "author".into(),
            view: "author".into(),
        })
        .unwrap();
    assert_eq!(
        expected["result"],
        magic::compose(
            &world,
            TRADITION,
            &selected,
            point.as_ref(),
            "author",
            "author"
        )
        .unwrap()
    );
    let mut query = params(TRADITION);
    let mut flags = args(TRADITION);
    if let Some((timeline, tick, order)) = at {
        query.extend([
            ("timeline", timeline.into()),
            ("tick", tick.to_string()),
            ("order", order.to_string()),
        ]);
        flags.extend([
            "--timeline".into(),
            timeline.into(),
            "--tick".into(),
            tick.to_string(),
            "--order".into(),
            order.to_string(),
        ]);
    }
    let (status, actual) = get(f, &uri("magic/compose", &query)).await;
    assert_eq!(status, StatusCode::OK, "{actual}");
    assert_eq!(actual, expected);
    let output = cli(f, &flags);
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
fn ids(value: &Value) -> Vec<&str> {
    value["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["conceptId"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn composition_preserves_role_order_separate_statements_and_exact_value_types() {
    let f = Fixture::new();
    let fingerprint = f.world().fingerprint;
    let read = parity(&f, Some(("main", TICK, 0))).await;
    assert_eq!(ids(&read), [TRADITION, PRACTICE, ARTIFACT, REALM]);
    assert_eq!(read["rootId"], TRADITION);
    assert_eq!(read["perspective"], "author");
    assert_eq!(read["at"]["tick"], TICK.to_string());
    assert_eq!(read["records"][0]["role"], "tradition");
    assert_eq!(
        read["records"][0]["fields"]["restrictions"]["value"],
        "Only at dusk"
    );
    assert_eq!(read["records"][0]["fields"]["exceptions"]["known"], false);
    assert_eq!(read["records"][1]["fields"]["restrictions"]["value"], "1");
    assert_eq!(
        read["records"][1]["fields"]["restrictions"]["valueType"]["scalarType"],
        "number"
    );
    assert!(read["records"][1]["fields"]["exceptions"]["value"].is_null());
    assert_eq!(read["records"][1]["fields"]["exceptions"]["known"], true);
    assert_eq!(
        read["records"][2]["fields"]["restrictions"]["value"],
        json!({"kind":"record","id":HIDDEN})
    );
    assert_eq!(
        read["records"][2]["fields"]["restrictions"]["references"],
        json!([])
    );
    assert_eq!(
        read["records"][2]["fields"]["exceptions"]["value"],
        u64::MAX.to_string()
    );
    assert_eq!(
        read["records"][3]["fields"]["restrictions"]["value"],
        json!([])
    );
    assert_eq!(read["records"][3]["fields"]["restrictions"]["known"], true);
    assert_eq!(read["records"][3]["fields"]["exceptions"]["known"], false);
    assert_eq!(read["records"][0]["bodyKind"], "undated-authored-prose");
    assert_eq!(
        read["records"][1]["fields"]["restrictions"]["citation"]["section"],
        "values.restrictions"
    );
    assert!(read.to_string().contains("values.practices[1].id"));
    assert!(!read.to_string().contains("Private practice"));
    assert!(!read.to_string().contains("Other timeline practice"));
    assert_eq!(read["linkageComplete"], true);
    assert_eq!(f.world().fingerprint, fingerprint);
}

#[tokio::test]
async fn exact_horizons_all_time_and_order_defaults_match_across_transports() {
    let f = Fixture::new();
    for (order, expected) in [(-1, 2), (0, 4), (2, 4), (3, 2)] {
        let read = parity(&f, Some(("main", TICK, order))).await;
        assert_eq!(read["records"].as_array().unwrap().len(), expected);
        assert_eq!(read["at"]["order"], order.to_string());
    }
    let all = parity(&f, None).await;
    assert!(all["at"].is_null());
    assert_eq!(
        ids(&all),
        [TRADITION, PRACTICE, ARTIFACT, OTHER_TIME, REALM]
    );
    assert_eq!(all["records"][3]["fields"]["restrictions"]["value"], "1");
    assert_eq!(
        all["records"][3]["fields"]["restrictions"]["valueType"]["scalarType"],
        "string"
    );
    let other = parity(&f, Some(("other", TICK, 0))).await;
    assert_eq!(ids(&other), [TRADITION, OTHER_TIME, ARTIFACT, REALM]);
    let mut query = params(TRADITION);
    query.push(("tick", TICK.to_string()));
    let (status, read) = get(&f, &uri("magic/compose", &query)).await;
    assert_eq!(status, StatusCode::OK, "{read}");
    assert_eq!(read["result"]["at"]["order"], i32::MAX.to_string());
    assert_eq!(read["result"]["at"]["timeline"], "main");
    assert_eq!(ids(&read["result"]), [TRADITION, REALM]);
    let mut flags = args(TRADITION);
    flags.extend(["--tick".into(), TICK.to_string()]);
    assert_eq!(
        serde_json::from_slice::<Value>(&cli(&f, &flags).stdout).unwrap(),
        read
    );
}

#[tokio::test]
async fn closed_json_role_maps_reject_duplicates_wrong_shapes_and_unselected_roles() {
    let f = Fixture::new();
    let malformed = [
        (
            "profiles",
            r#"{"tradition":"tradition","tradition":"realm"}"#,
        ),
        (
            "profiles",
            r#"{"tradition":"tradition","\u0074radition":"realm"}"#,
        ),
        ("profiles", "{}"),
        (
            "profiles",
            r#"{"tradition":"tradition","practice":"tradition"}"#,
        ),
        ("profiles", r#"{"power":"tradition"}"#),
        ("profiles", r#"{"tradition":1}"#),
        ("profiles", "[]"),
        ("profiles", "null"),
        ("profiles", r#"{"tradition":"not-declared"}"#),
        (
            "links",
            r#"{"tradition":["practices"],"tradition":["realm"]}"#,
        ),
        ("links", r#"{"tradition":["practices","practices"]}"#),
        ("links", r#"{"tradition":"practices"}"#),
        ("links", r#"{"tradition":[1]}"#),
        ("links", r#"{"tradition":["values.practices"]}"#),
        ("links", r#"{"power":[]}"#),
        ("links", "[]"),
        (
            "restrictions",
            r#"{"tradition":"restrictions","tradition":"exceptions"}"#,
        ),
        ("restrictions", r#"{"power":"restrictions"}"#),
        ("restrictions", r#"{"tradition":null}"#),
        ("restrictions", r#"{"tradition":"values.restrictions"}"#),
        (
            "exceptions",
            r#"{"tradition":"exceptions","tradition":"restrictions"}"#,
        ),
        ("exceptions", r#"{"power":"exceptions"}"#),
        ("exceptions", r#"{"tradition":[]}"#),
        ("exceptions", r#"{"tradition":"exceptions"} trailing"#),
    ];
    for (key, invalid) in malformed {
        let mut query = params(TRADITION);
        query.retain(|(name, _)| *name != key);
        query.push((key, invalid.into()));
        let (status, failure) = get(&f, &uri("magic/compose", &query)).await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "{key}: {invalid}: {failure}"
        );
        assert!(failure.get("result").is_none());
        let mut flags = args(TRADITION);
        let flag = format!("--{key}");
        let index = flags.iter().position(|arg| *arg == flag).unwrap();
        flags[index + 1] = invalid.into();
        let output = cli(&f, &flags);
        assert!(!output.status.success(), "{key}: {invalid}");
        assert!(output.stdout.is_empty());
    }
    for (key, value) in [
        ("links", r#"{"practice":[]}"#),
        ("restrictions", r#"{"practice":"restrictions"}"#),
        ("exceptions", r#"{"practice":"exceptions"}"#),
    ] {
        let query = vec![
            ("id", TRADITION.into()),
            ("profiles", r#"{"tradition":"tradition"}"#.into()),
            (key, value.into()),
        ];
        assert_eq!(
            get(&f, &uri("magic/compose", &query)).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    let fields: Vec<_> = (0..17).map(|n| format!("field_{n}")).collect();
    let too_many = json!({"tradition":fields}).to_string();
    assert!(magic_selection_json(PROFILES, Some(&too_many), None, None).is_err());
    let oversized = " ".repeat(16_385);
    assert!(magic_selection_json(&oversized, None, None, None).is_err());
}

#[tokio::test]
async fn closed_route_and_cli_refuse_partial_time_or_character_grants() {
    let f = Fixture::new();
    for (key, value) in [
        ("order", "0"),
        ("timeline", "main"),
        ("tick", "0.5"),
        ("tick", "9223372036854775808"),
        ("limit", "1"),
        ("viewer", VIEWER),
        ("known_records", PRACTICE),
        ("record", TRADITION),
    ] {
        let mut query = params(TRADITION);
        query.push((key, value.into()));
        let (status, failure) = get(&f, &uri("magic/compose", &query)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{key}: {failure}");
    }
    for (key, value) in [("order", "2147483648"), ("timeline", "missing")] {
        let mut query = params(TRADITION);
        query.extend([("tick", TICK.to_string()), (key, value.into())]);
        assert_eq!(
            get(&f, &uri("magic/compose", &query)).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    for missing in ["id", "profiles"] {
        let mut query = params(TRADITION);
        query.retain(|(key, _)| *key != missing);
        assert_eq!(
            get(&f, &uri("magic/compose", &query)).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    let mut query = params(TRADITION);
    query.push(("id", PRACTICE.into()));
    assert_eq!(
        get(&f, &uri("magic/compose", &query)).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        get(&f, &uri("magic/resolve", &params(TRADITION))).await.0,
        StatusCode::NOT_FOUND
    );
    for extra in [
        vec!["--order", "0"],
        vec!["--timeline", "main"],
        vec!["--tick", "0.5"],
        vec!["--tick", "1", "--order", "2147483648"],
        vec!["--viewer", VIEWER],
        vec!["--limit", "1"],
    ] {
        let mut flags = args(TRADITION);
        flags.extend(extra.into_iter().map(str::to_owned));
        assert!(!cli(&f, &flags).status.success());
    }
    assert!(
        !cli(&f, &["magic".into(), "compose".into(), TRADITION.into()])
            .status
            .success()
    );
}

#[tokio::test]
async fn hidden_missing_unselected_and_nonexact_roots_are_uniformly_unavailable() {
    let f = Fixture::new();
    let mut failures = Vec::new();
    for id in [HIDDEN, MISSING, VIEWER, "Dusk tradition"] {
        failures.push(get(&f, &uri("magic/compose", &params(id))).await);
    }
    assert_eq!(failures[0].0, StatusCode::NOT_FOUND);
    for failure in &failures[1..] {
        assert_eq!(*failure, failures[0]);
    }
    let query = vec![
        ("id", TRADITION.into()),
        ("profiles", r#"{"realm":"realm"}"#.into()),
    ];
    assert_eq!(get(&f, &uri("magic/compose", &query)).await, failures[0]);
    for (key, label) in [("audience", "reader"), ("view", "reader")] {
        let mut query = params(TRADITION);
        query.push((key, label.into()));
        assert_eq!(get(&f, &uri("magic/compose", &query)).await, failures[0]);
    }
    let mut query = params(HIDDEN);
    query.push(("audience", "editor".into()));
    let (status, read) = get(&f, &uri("magic/compose", &query)).await;
    assert_eq!(status, StatusCode::OK, "{read}");
    assert_eq!(ids(&read["result"]), [HIDDEN]);
    assert_eq!(
        read["result"]["records"][0]["fields"]["restrictions"]["valueType"]["scalarType"],
        "string"
    );
    assert!(!read.to_string().contains("Counted artifact"));
}

#[tokio::test]
async fn literal_field_links_stay_unknown_and_visible_cycles_return_no_partial_output() {
    let f = Fixture::new();
    let mut query = params(TRADITION);
    query.retain(|(key, _)| *key != "links");
    query.push(("links",r#"{"tradition":["practices","realm"],"practice":["artifact"],"artifact":["restrictions"]}"#.into()));
    let (status, read) = get(&f, &uri("magic/compose", &query)).await;
    assert_eq!(status, StatusCode::OK, "{read}");
    assert_eq!(read["result"]["linkageComplete"], false);
    assert!(!ids(&read["result"]).contains(&HIDDEN));
    assert!(
        read["result"]["unknowns"]
            .to_string()
            .contains("not-typed-reference")
    );
    f.concept(
        "artifact",ARTIFACT,"Counted artifact","artifact",
        &format!("restrictions: {{kind: record, id: {HIDDEN}}}, exceptions: 18446744073709551615, next: {{kind: record, id: {TRADITION}}}"),
        "author","{kind: timeless}",
    );
    assert!(validation::validate_world(&f.world()).valid);
    let cyclic =
        r#"{"tradition":["practices","realm"],"practice":["artifact"],"artifact":["next"]}"#;
    query.retain(|(key, _)| *key != "links");
    query.push(("links", cyclic.into()));
    let before = f.world().fingerprint;
    let (status, failure) = get(&f, &uri("magic/compose", &query)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{failure}");
    assert!(failure.get("result").is_none());
    let mut flags = args(TRADITION);
    let index = flags.iter().position(|arg| arg == "--links").unwrap();
    flags[index + 1] = cyclic.into();
    let output = cli(&f, &flags);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(f.world().fingerprint, before);
}

#[tokio::test]
async fn magic_canonical_values_do_not_change_generic_character_context_or_search() {
    let f = Fixture::new();
    let requests = [
        uri(
            "character/context",
            &[
                ("character", VIEWER.into()),
                ("tick", TICK.to_string()),
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
    for request in &requests {
        before.push(get(&f, request).await);
    }
    let path = f.root().join("story/tradition.md");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("Only at dusk", "Changed canonical magic wording"),
    )
    .unwrap();
    let path = f.root().join("story/practice.md");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace("restrictions: 1", "restrictions: '1'"),
    )
    .unwrap();
    assert!(validation::validate_world(&f.world()).valid);
    for (index, request) in requests.iter().enumerate() {
        assert_eq!(get(&f, request).await, before[index]);
    }
}
