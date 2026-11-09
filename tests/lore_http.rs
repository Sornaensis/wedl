//! Author lore keeps exact typed values and admission semantics across transports.
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::Value;
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;
use tower::ServiceExt;
use wedl::{
    http, lore,
    model::{StoryTime, World},
    service::{Operation, Service, TimeInput},
    source, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const VIEWER: &str = "char_00000000000000000000000001";
const BASE: &str = "concept_00000000000000000000000001";
const TARGET: &str = "concept_00000000000000000000000002";
const HIDDEN: &str = "concept_00000000000000000000000003";
const FUTURE: &str = "concept_00000000000000000000000004";
const DRAFT: &str = "concept_00000000000000000000000005";
const RETIRED: &str = "concept_00000000000000000000000006";
const LOCATION: &str = "location:harbor";
const OLD_LOCATION: &str = "location:retired";
const TICK: i64 = 9_007_199_254_740_993;

struct Fixture(TempDir);
impl Fixture {
    fn new(with_lore: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("story")).unwrap();
        let interval = format!(
            "{{kind: interval, timeline: main, start: {{timeline: main, tick: {TICK}, order: -1}}, end: {{timeline: main, tick: {TICK}, order: 1}}}}"
        );
        let future = format!(
            "{{kind: interval, timeline: main, start: {{timeline: main, tick: {}, order: 0}}}}",
            TICK + 1
        );
        let links = if with_lore {
            format!(
                "concept_links:\n  - {{relation: hidden, concept_id: {HIDDEN}, valid: {{kind: timeless}}}}\n  - {{relation: observes, concept_id: {TARGET}, valid: {{kind: timeless}}}}\n  - {{relation: later, concept_id: {FUTURE}, valid: {future}}}\n"
            )
        } else {
            String::new()
        };
        let profiles = if with_lore {
            "profiles:\n  - id: notes\n    label: Notes\n    fields:\n      type: object\n      required:\n        note: {type: string}\n      optional:\n        amount: {type: number}\n        flag: {type: boolean}\n        empty: {type: 'null'}\n        choice: {type: enum, values: [high, low]}\n        target: {type: entity, kinds: [lore-concept]}\n        refs: {type: array, items: {type: entity, kinds: [location, lore-concept]}, max_items: 8}\n  - id: plain\n    label: Plain\n    fields: {type: object, required: {note: {type: string}}, optional: {}}\n"
        } else {
            ""
        };
        let mut records = vec![
            (
                "world",
                WORLD,
                "Lore fixture",
                "canonical",
                format!(
                    "capabilities: [{}]\ndefault_timeline: main\ntimelines: [{{id: main, label: Main}}, {{id: other, label: Other}}]\nstate_keys: {{}}\n{profiles}{links}",
                    if with_lore { "lore-core-v1" } else { "" }
                ),
            ),
            ("location", LOCATION, "Harbor", "canonical", links.clone()),
            (
                "location",
                OLD_LOCATION,
                "Old Landing",
                "retired",
                links.clone(),
            ),
            (
                "character",
                VIEWER,
                "Ana",
                "canonical",
                format!("initial_state: {{}}\n{links}"),
            ),
            (
                "knowledge",
                "know_00000000000000000000000001",
                "Lamp",
                "canonical",
                format!(
                    "knower: {VIEWER}\nclaim: {{key: lamp, statement: I saw a lamp}}\ntransitions: [{{id: learned, time: {{tick: 0}}, state: accepted, acquisition: observed, confidence: 1}}]\n"
                ),
            ),
        ];
        if with_lore {
            let base_values = format!(
                "note: {HIDDEN}, amount: {TICK}, flag: false, empty: null, choice: high, target: {{kind: record, id: {HIDDEN}}}, refs: [{{kind: record, id: {HIDDEN}}}, {{kind: record, id: {TARGET}}}, {{kind: record, id: '{OLD_LOCATION}'}}]"
            );
            for (id, title, domain, profile, values, valid, audience, status, own_links) in [
                (
                    BASE,
                    "Harbor Custom",
                    "story.tradition",
                    "notes",
                    base_values.as_str(),
                    "{kind: timeless}",
                    "author",
                    "canonical",
                    links.as_str(),
                ),
                (
                    TARGET,
                    "Lantern Custom",
                    "story.ritual",
                    "plain",
                    "note: A dated public custom",
                    interval.as_str(),
                    "author",
                    "canonical",
                    "",
                ),
                (
                    HIDDEN,
                    "Harbor Custom",
                    "story.secret",
                    "plain",
                    "note: A hidden custom",
                    "{kind: timeless}",
                    "editor",
                    "canonical",
                    links.as_str(),
                ),
                (
                    FUTURE,
                    "Future Custom",
                    "story.future",
                    "plain",
                    "note: A future custom",
                    future.as_str(),
                    "author",
                    "canonical",
                    "",
                ),
                (
                    DRAFT,
                    "Draft Custom",
                    "story.draft",
                    "plain",
                    "note: A draft custom",
                    "{kind: timeless}",
                    "author",
                    "draft",
                    "",
                ),
                (
                    RETIRED,
                    "Retired Custom",
                    "story.retired",
                    "plain",
                    "note: A retired custom",
                    "{kind: timeless}",
                    "author",
                    "retired",
                    "",
                ),
            ] {
                records.push(("lore-concept", id, title, status, format!("domain: {domain}\nprofile: {profile}\nvalues: {{{values}}}\nvalid: {valid}\naudience: [{audience}]\nperspectives: [author]\n{own_links}")));
            }
        }
        for (index, (kind, id, title, status, fields)) in records.iter().enumerate() {
            fs::write(dir.path().join(format!("story/{index}.md")), format!("---\nschema: {}\nid: {id}\nkind: {kind}\ntitle: {title}\nstatus: {status}\naliases: []\ntags: []\n{fields}---\nAuthor source prose\n", source::SOURCE_SCHEMA)).unwrap();
        }
        let result = Self(dir);
        let report = validation::validate_world(&result.world());
        assert!(report.valid, "{:?}", report.diagnostics);
        result
    }
    fn root(&self) -> &Path {
        self.0.path()
    }
    fn world(&self) -> World {
        source::load_world(self.root(), None).unwrap()
    }
    fn service(&self) -> Service {
        Service::new(self.world())
    }
}
fn at(order: i32) -> TimeInput {
    TimeInput {
        tick: TICK,
        order,
        timeline: Some("main".into()),
    }
}
fn point(order: i32) -> StoryTime {
    StoryTime::new("main", TICK, order).unwrap()
}
fn cli(root: &Path, args: &[&str]) -> (i32, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .args(["--compact", "--repo", root.to_str().unwrap()])
        .args(args)
        .output()
        .unwrap();
    let bytes = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    (
        output.status.code().unwrap(),
        serde_json::from_slice(bytes)
            .unwrap_or_else(|_| panic!("non-JSON output: {}", String::from_utf8_lossy(bytes))),
    )
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
async fn parity(f: &Fixture, operation: Operation, core: Value, uri: &str, args: &[&str]) -> Value {
    let service = f.service();
    let expected = service.execute(operation).unwrap();
    assert_eq!(expected["result"], core);
    let (status, actual) = get(service.clone(), uri).await;
    assert_eq!(status, StatusCode::OK, "{actual}");
    assert_eq!(actual, expected);
    let (code, command) = cli(f.root(), args);
    assert_eq!(code, 0, "{command}");
    assert_eq!(command, actual);
    assert_eq!(actual["fingerprint"], service.world().fingerprint);
    assert_eq!(actual["result"]["perspective"], "author");
    actual["result"].clone()
}

#[tokio::test]
async fn profile_registry_and_detail_match_core_service_cli_and_http() {
    let f = Fixture::new(true);
    let world = f.world();
    let profiles = parity(
        &f,
        Operation::LoreProfiles { limit: 2 },
        lore::profiles(&world, 2).unwrap(),
        "/api/lore/profiles?limit=2",
        &["lore", "profiles", "--limit", "2"],
    )
    .await;
    assert_eq!(profiles["count"], 2);
    assert_eq!(profiles["scope"], "author-all-time-lore");
    assert!(profiles["at"].is_null());
    assert_eq!(profiles["items"][0]["id"], "notes");
    let profile = parity(
        &f,
        Operation::LoreProfile {
            profile: "notes".into(),
        },
        lore::profile(&world, "notes").unwrap(),
        "/api/lore/profile?id=notes",
        &["lore", "profile", "notes"],
    )
    .await;
    assert_eq!(profile["fields"]["optional"]["refs"]["max_items"], "8");
    assert_eq!(profile["fields"]["optional"]["empty"]["type"], "null");
    assert_eq!(
        profile["fields"]["optional"]["choice"]["enumValueTypes"],
        serde_json::json!(["string", "string"])
    );
    assert_eq!(profile["citation"]["section"], "profiles[0]");
    let (status, overflow) = get(f.service(), "/api/lore/profiles?limit=1").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(overflow.get("result").is_none());
    assert_eq!(
        get(f.service(), "/api/lore/profile?id=missing").await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn catalog_filters_scope_domain_and_status_before_complete_limits() {
    let f = Fixture::new(true);
    let world = f.world();
    let all = parity(
        &f,
        Operation::LoreCatalog {
            at: None,
            audience: "author".into(),
            view: "author".into(),
            domain: None,
            limit: 3,
        },
        lore::catalog(&world, None, "author", "author", None, 3).unwrap(),
        "/api/lore/catalog?limit=3",
        &["lore", "catalog", "--limit", "3"],
    )
    .await;
    assert_eq!(all["count"], 3);
    assert_eq!(
        all["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["conceptId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [BASE, TARGET, FUTURE]
    );
    let filtered = parity(
        &f,
        Operation::LoreCatalog {
            at: Some(at(0)),
            audience: "author".into(),
            view: "author".into(),
            domain: Some("story.ritual".into()),
            limit: 1,
        },
        lore::catalog(
            &world,
            Some(&point(0)),
            "author",
            "author",
            Some("story.ritual"),
            1,
        )
        .unwrap(),
        "/api/lore/catalog?tick=9007199254740993&order=0&domain=story.ritual&limit=1",
        &[
            "lore",
            "catalog",
            "--tick",
            "9007199254740993",
            "--order",
            "0",
            "--domain",
            "story.ritual",
            "--limit",
            "1",
        ],
    )
    .await;
    assert_eq!(filtered["count"], 1);
    assert_eq!(filtered["items"][0]["conceptId"], TARGET);
    let (status, overflow) = get(f.service(), "/api/lore/catalog?limit=2").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(overflow.get("result").is_none());
    for query in ["domain=story.rit", "view=reader", "audience=reader"] {
        let (status, result) =
            get(f.service(), &format!("/api/lore/catalog?{query}&limit=1")).await;
        assert_eq!(status, StatusCode::OK, "{result}");
        assert_eq!(result["result"]["count"], 0);
    }
    let (status, editor) = get(f.service(), "/api/lore/catalog?audience=editor&limit=1").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(editor["result"]["items"][0]["conceptId"], HIDDEN);
}

#[tokio::test]
async fn concept_projection_distinguishes_typed_refs_from_literals_and_keeps_source_indexes() {
    let f = Fixture::new(true);
    let world = f.world();
    let result = parity(
        &f,
        Operation::LoreConcept {
            concept: "Harbor Custom".into(),
            at: Some(at(0)),
            audience: "author".into(),
            view: "author".into(),
        },
        lore::concept(&world, "Harbor Custom", Some(&point(0)), "author", "author").unwrap(),
        "/api/lore/concept?id=Harbor%20Custom&tick=9007199254740993&order=0",
        &[
            "lore",
            "concept",
            "Harbor Custom",
            "--tick",
            "9007199254740993",
            "--order",
            "0",
        ],
    )
    .await;
    assert_eq!(result["conceptId"], BASE);
    assert_eq!(result["values"]["note"], HIDDEN);
    assert_eq!(result["values"]["amount"], TICK.to_string());
    assert_eq!(result["valueTypes"]["fields"]["amount"]["type"], "number");
    assert_eq!(result["valueTypes"]["fields"]["note"]["type"], "string");
    assert_eq!(result["values"]["flag"], false);
    assert!(result["values"]["empty"].is_null());
    assert_eq!(result["values"]["choice"], "high");
    assert_eq!(result["valueTypes"]["fields"]["choice"]["type"], "enum");
    assert_eq!(
        result["valueTypes"]["fields"]["choice"]["scalarType"],
        "string"
    );
    assert!(result["values"].get("target").is_none());
    assert!(result["valueTypes"]["fields"].get("target").is_none());
    assert_eq!(result["values"]["refs"].as_array().unwrap().len(), 2);
    assert_eq!(result["values"]["refs"][0]["id"], TARGET);
    assert_eq!(result["values"]["refs"][1]["id"], OLD_LOCATION);
    let refs = result["references"].as_array().unwrap();
    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0]["sourceField"], "values.refs[1].id");
    assert_eq!(refs[0]["citation"]["section"], "values.refs[1].id");
    assert_eq!(refs[0]["targetCitation"]["section"], "record");
    assert_eq!(refs[1]["sourceField"], "values.refs[2].id");
    assert_eq!(refs[1]["kind"], "location");
    assert_eq!(refs[1]["title"], "Old Landing");
    assert_eq!(result["conceptLinks"].as_array().unwrap().len(), 1);
    assert_eq!(result["conceptLinks"][0]["conceptId"], TARGET);
    let mut unavailable = None;
    for id in [HIDDEN, DRAFT, RETIRED, "concept_00000000000000000000000999"] {
        let response = get(f.service(), &format!("/api/lore/concept?id={id}")).await;
        assert_eq!(response.0, StatusCode::NOT_FOUND);
        if let Some(expected) = &unavailable {
            assert_eq!(&response, expected);
        }
        unavailable = Some(response);
    }
}

#[tokio::test]
async fn inclusive_horizons_preserve_large_ticks_orders_and_independent_timelines() {
    let f = Fixture::new(true);
    for (order, count) in [(-2, 0), (-1, 1), (0, 1), (1, 1), (2, 0)] {
        let (status, result) = get(
            f.service(),
            &format!("/api/lore/catalog?tick={TICK}&order={order}&domain=story.ritual&limit=1"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{result}");
        assert_eq!(result["result"]["count"], count);
        assert_eq!(result["result"]["at"]["tick"], TICK.to_string());
        assert_eq!(result["result"]["at"]["order"], order.to_string());
        assert_eq!(result["result"]["scope"], "author-as-of-lore");
        if count == 1 {
            assert_eq!(
                result["result"]["items"][0]["valid"]["start"]["tick"],
                TICK.to_string()
            );
            assert_eq!(result["result"]["items"][0]["valid"]["end"]["order"], "1");
        }
    }
    for query in [
        format!("tick={TICK}"),
        format!("tick={TICK}&order=0&timeline=other"),
    ] {
        let (status, result) = get(
            f.service(),
            &format!("/api/lore/catalog?{query}&domain=story.ritual"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(result["result"]["count"], 0);
    }
    let (_, other) = get(
        f.service(),
        &format!("/api/lore/catalog?tick={TICK}&order=0&timeline=other"),
    )
    .await;
    assert_eq!(other["result"]["count"], 1);
    assert_eq!(other["result"]["items"][0]["conceptId"], BASE);
}

#[tokio::test]
async fn links_filter_targets_and_concept_owners_before_limits_but_keep_ordinary_author_owners() {
    let f = Fixture::new(true);
    let world = f.world();
    let links = parity(
        &f,
        Operation::LoreLinks {
            record: "Harbor".into(),
            at: Some(at(0)),
            audience: "author".into(),
            view: "author".into(),
            limit: 1,
        },
        lore::links(&world, "Harbor", Some(&point(0)), "author", "author", 1).unwrap(),
        "/api/lore/links?record=Harbor&tick=9007199254740993&order=0&limit=1",
        &[
            "lore",
            "links",
            "Harbor",
            "--tick",
            "9007199254740993",
            "--order",
            "0",
            "--limit",
            "1",
        ],
    )
    .await;
    assert_eq!(links["ownerId"], LOCATION);
    assert_eq!(links["ownerKind"], "location");
    assert_eq!(links["count"], 1);
    assert_eq!(links["items"][0]["title"], "Lantern Custom");
    assert_eq!(links["items"][0]["citation"]["section"], "concept_links[1]");
    for owner in [WORLD, OLD_LOCATION, BASE] {
        let (status, value) = get(
            f.service(),
            &format!("/api/lore/links?record={owner}&tick={TICK}&order=0&limit=1"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{value}");
        assert_eq!(value["result"]["ownerId"], owner);
        assert_eq!(value["result"]["count"], 1);
    }
    let (status, overflow) = get(f.service(), "/api/lore/links?record=Harbor&limit=1").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(overflow.get("result").is_none());
    for owner in [HIDDEN, DRAFT, RETIRED] {
        let (status, value) = get(f.service(), &format!("/api/lore/links?record={owner}")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(value.get("result").is_none());
    }
}

#[tokio::test]
async fn lore_parameters_are_closed_and_partial_horizons_limits_and_missing_capability_fail() {
    let f = Fixture::new(true);
    for uri in [
        "/api/lore/profiles?tick=0",
        "/api/lore/profiles?audience=author",
        "/api/lore/profiles?limit=0",
        "/api/lore/profiles?limit=1001",
        "/api/lore/profile?id=notes&limit=1",
        "/api/lore/profile?id=notes&view=author",
        "/api/lore/profile",
        "/api/lore/profile?id=Notes",
        "/api/lore/catalog?order=0",
        "/api/lore/catalog?timeline=main",
        "/api/lore/catalog?limit=0",
        "/api/lore/catalog?limit=1001",
        "/api/lore/catalog?domain=story",
        "/api/lore/catalog?domain=",
        "/api/lore/catalog?audience=",
        "/api/lore/catalog?view=%20",
        "/api/lore/catalog?audience=author%0Aeditor",
        "/api/lore/catalog?perspective=character",
        "/api/lore/catalog?character=Ana",
        "/api/lore/catalog?tick=9223372036854775808",
        "/api/lore/catalog?tick=0&order=-2147483649",
        "/api/lore/catalog?tick=0&timeline=absent",
        "/api/lore/catalog?tick=0&tick=1",
        "/api/lore/concept",
        "/api/lore/concept?id=Harbor&limit=1",
        "/api/lore/concept?id=Harbor&domain=story.lore",
        "/api/lore/concept?id=Harbor&order=0",
        "/api/lore/concept?id=Harbor&view=",
        "/api/lore/links",
        "/api/lore/links?record=Harbor&limit=0",
        "/api/lore/links?record=Harbor&limit=1001",
        "/api/lore/links?record=Harbor&timeline=main",
        "/api/lore/links?record=Harbor&domain=story.lore",
        "/api/lore/links?record=Harbor&viewer=Ana",
    ] {
        let (status, value) = get(f.service(), uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {value}");
        assert!(value.get("result").is_none());
    }
    for args in [
        vec!["lore", "catalog", "--order", "0"],
        vec!["lore", "concept", BASE, "--timeline", "main"],
        vec!["lore", "links", LOCATION, "--order", "0"],
        vec!["lore", "profiles", "--audience", "author"],
        vec!["lore", "profile", "notes", "--tick", "0"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
            .args(["--compact", "--repo", f.root().to_str().unwrap()])
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
    let no_lore = Fixture::new(false);
    for uri in [
        "/api/lore/profiles",
        "/api/lore/profile?id=notes",
        "/api/lore/catalog",
        "/api/lore/concept?id=Harbor",
        "/api/lore/links?record=Harbor",
    ] {
        let (status, value) = get(no_lore.service(), uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(value["code"], "validation_error");
        assert!(value.get("result").is_none());
    }
}

#[tokio::test]
async fn complete_character_context_and_search_ignore_authored_lore_profiles_and_links() {
    let before = Fixture::new(false);
    let after = Fixture::new(true);
    for uri in [
        format!(
            "/api/character/context?character={VIEWER}&tick={TICK}&order=0&maxCharacters=100000"
        ),
        format!("/api/character/search?character={VIEWER}&q=lamp&tick={TICK}&order=0"),
        format!("/api/character/search?character={VIEWER}&q=custom&tick={TICK}&order=0"),
    ] {
        let old = get(before.service(), &uri).await;
        let new = get(after.service(), &uri).await;
        assert_eq!(old.0, StatusCode::OK, "{}", old.1);
        assert_eq!(old, new);
        assert!(new.1.get("fingerprint").is_none());
    }
}
