//! Learned generations share one evidence-only contract across core, service, CLI and HTTP.
use std::{fs, path::PathBuf, process::Command};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use serde_yaml::Value as Yaml;
use tower::ServiceExt;
use wedl::{
    generational::{self, LineageDirection, discovery, knowledge},
    http,
    model::{Record, StoryTime, World},
    service::{Operation, Service, TimeInput},
    source, validation,
};

const MINA: &str = "char_4JVNDN6QW0BAFFV379AJJSMEPZ";
const SELA: &str = "char_39E9EWWC017E40519033S4RW4X";
const CORA: &str = "char_46VTBJZ9TNMTF7G14N5GKYQ9C1";
const ORG: &str = "organization_4YNRVZSRYTGBB27YKD7NW1T7CZ";
const GUILD: &str = "organization_5JNW6KPDP187ZKYZ2JRE9TP1ZG";
const LEGACY: &str = "legacy_4N3B8XV3V1QN8E13H8V5CHWDPQ";
const TEMPLATE: &str = "know_33RB1GJYPCGCPDA9KYT8GN4PN9";
const KINSHIP: &str = "kinship_6FMPV333HWER1D3NXKSSNP6AZ9";
const KINDS: [&str; 7] = [
    "parentage",
    "union",
    "organization",
    "affiliation",
    "tenure",
    "claim",
    "vital",
];

fn id(index: usize) -> String {
    format!("know_{:026}", index + 800)
}
fn at(tick: i64, order: i32) -> TimeInput {
    TimeInput {
        tick,
        order,
        timeline: Some("main".into()),
    }
}
fn point(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn transition(index: usize, state: &str, tick: i64, order: i32) -> Value {
    json!({"id":format!("learned-{index}-{tick}-{order}"),"time":{"timeline":"main","tick":tick,"order":order},
        "state":state,"confidence":1.0,"acquisition":"told"})
}
fn rebuilt(record: &Record) -> Record {
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&record.frontmatter).unwrap(),
        record.body
    );
    source::parse_record(raw.as_bytes(), &record.source_path).unwrap()
}
struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let mut world = source::load_world(
            concat!(env!("CARGO_MANIFEST_DIR"), "/examples/tideglass"),
            None,
        )
        .unwrap();
        let world_id = world.world_record().unwrap().id.clone();
        let world_record = world.records.get_mut(&world_id).unwrap();
        world_record.frontmatter["capabilities"]
            .as_sequence_mut()
            .unwrap()
            .push(Yaml::String("generational-knowledge-v1".into()));
        *world_record = rebuilt(world_record);
        let payloads = [
            json!({"child_id":MINA,"parent_id":SELA,"basis":"adoptive"}),
            json!({"participant_ids":[SELA,MINA],"state":"formed"}),
            json!({"organization_id":ORG,"parent_id":GUILD}),
            json!({"character_id":MINA,"organization_id":ORG,"role":"learned keeper"}),
            json!({"legacy_id":LEGACY,"holder_id":MINA,"basis":"de-facto"}),
            json!({"legacy_id":LEGACY,"claimant_id":SELA,"state":"disputed"}),
            json!({"character_id":MINA,"state":"dead"}),
        ];
        for (index, (kind, payload)) in KINDS.into_iter().zip(payloads).enumerate() {
            let mut record = world.records[TEMPLATE].clone();
            let mut labels = serde_json::Map::new();
            for (endpoint, label) in [
                (MINA, "Learned child"),
                (SELA, "Learned elder"),
                (ORG, "Learned circle"),
                (GUILD, "Learned council"),
                (LEGACY, "Learned office"),
            ] {
                if payload.to_string().contains(endpoint) {
                    labels.insert(endpoint.into(), json!(label));
                }
            }
            record.frontmatter["id"] = Yaml::String(id(index));
            record.frontmatter["title"] = Yaml::String(format!("Private {kind} source title"));
            record.frontmatter["knower"] = Yaml::String(MINA.into());
            record.frontmatter["claim"] = serde_yaml::to_value(json!({"key":format!("learned.{kind}"),"statement":"Unadmitted outer claim statement",
                "genealogy":{"kind":kind,"payload":payload,"valid":{"from":{"timeline":"main","tick":0,"order":0}},"labels":labels,"evidence":[]}})).unwrap();
            record.frontmatter["transitions"] =
                serde_yaml::to_value(vec![transition(index, "accepted", 10, 2)]).unwrap();
            record.source_path = format!("story/knowledge/learned-{kind}.md");
            record.body = "\n# Private learned source body\n".into();
            let record = rebuilt(&record);
            world.records.insert(record.id.clone(), record);
        }
        let directory = tempfile::tempdir().unwrap();
        for record in world.records.values() {
            let path = directory.path().join(&record.source_path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, &record.raw_bytes).unwrap();
        }
        let fixture = Self {
            root: directory.path().to_path_buf(),
            _directory: directory,
        };
        fixture.valid();
        fixture
    }
    fn world(&self) -> World {
        source::load_world(&self.root, None).unwrap()
    }
    fn service(&self) -> Service {
        Service::new(self.world())
    }
    fn valid(&self) {
        let report = validation::validate_world(&self.world());
        assert!(report.valid, "{:?}", report.diagnostics);
    }
    fn edit(&self, record_id: &str, edit: impl FnOnce(&mut Yaml)) {
        let mut record = self.world().records[record_id].clone();
        edit(&mut record.frontmatter);
        let record = rebuilt(&record);
        fs::write(self.root.join(&record.source_path), &record.raw_bytes).unwrap();
    }
}
fn cli(fixture: &Fixture, args: &[String]) -> (i32, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .args(["--compact", "--repo", fixture.root.to_str().unwrap()])
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
            .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(bytes))),
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
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
fn private_envelope(value: &Value) {
    assert_eq!(
        value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["protocol", "result"]
    );
    assert_eq!(value["result"]["perspective"], "character");
    assert_eq!(value["result"]["characterId"], MINA);
    fn inspect(value: &Value) {
        match value {
            Value::Object(fields) => {
                for (key, value) in fields {
                    assert!(
                        ![
                            "fingerprint",
                            "revision",
                            "blobOid",
                            "sourcePath",
                            "source_path",
                            "frontmatterYaml",
                            "rawSource",
                            "truth"
                        ]
                        .contains(&key.as_str()),
                        "metadata leaked: {key}"
                    );
                    inspect(value);
                }
            }
            Value::Array(values) => {
                for value in values {
                    inspect(value);
                }
            }
            _ => {}
        }
    }
    inspect(value);
    for hidden in [
        "Private parentage source title",
        "Unadmitted outer claim statement",
        "Private learned source body",
    ] {
        assert!(
            !value.to_string().contains(hidden),
            "unadmitted source leaked"
        );
    }
}
async fn parity(
    fixture: &Fixture,
    operation: Operation,
    core: Value,
    path: &str,
    params: &str,
    args: &[&str],
    learned: bool,
) -> Value {
    let expected = fixture.service().execute(operation).unwrap();
    assert_eq!(expected["result"], core);
    let prefix = if learned {
        "generational/learned/"
    } else {
        "generational/"
    };
    let viewer = if learned {
        format!("viewer={MINA}&")
    } else {
        String::new()
    };
    let (status, actual) = get(
        fixture.service(),
        &format!("/api/{prefix}{path}?{viewer}{params}&tick=20&order=0"),
    )
    .await;
    let mut command = vec!["generational".to_owned()];
    if learned {
        command.extend(["learned".into(), MINA.into()]);
    }
    command.push(path.into());
    command.extend(args.iter().map(|item| (*item).into()));
    command.extend(["--tick".into(), "20".into(), "--order".into(), "0".into()]);
    let (code, output) = cli(fixture, &command);
    assert_eq!(status, StatusCode::OK, "{actual}");
    assert_eq!(code, 0, "{output}");
    assert_eq!(actual, expected);
    assert_eq!(output, expected);
    if learned {
        private_envelope(&actual);
    } else {
        assert_eq!(actual["fingerprint"], fixture.world().fingerprint);
    }
    core
}

#[tokio::test]
async fn all_seven_assertion_kinds_match_core_service_cli_and_http() {
    let fixture = Fixture::new();
    let world = fixture.world();
    for kind in KINDS {
        let result = parity(
            &fixture,
            Operation::GenerationalLearnedAssertions {
                viewer: MINA.into(),
                at: at(20, 0),
                kind: Some(kind.into()),
                endpoint: None,
                held_history: false,
                limit: 100,
            },
            knowledge::assertions(&world, MINA, &point(20, 0), Some(kind), None, false, 100)
                .unwrap(),
            "assertions",
            &format!("kind={kind}"),
            &["--kind", kind],
            true,
        )
        .await;
        let items = result["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["kind"], kind);
        assert_eq!(items[0]["learnedAt"]["tick"], "10");
        assert_eq!(items[0]["learnedAt"]["order"], "2");
    }
}

#[tokio::test]
async fn learned_relationships_and_held_history_match_all_four_interfaces() {
    let fixture = Fixture::new();
    let world = fixture.world();
    let horizon = point(20, 0);
    parity(
        &fixture,
        Operation::GenerationalLearnedFamily {
            viewer: MINA.into(),
            character: "Learned child".into(),
            at: at(20, 0),
            limit: 100,
        },
        knowledge::family(&world, MINA, MINA, &horizon, 100).unwrap(),
        "family",
        "character=Learned%20child",
        &["Learned child"],
        true,
    )
    .await;
    for (name, direction, subject) in [
        ("ancestors", LineageDirection::Ancestors, MINA),
        ("descendants", LineageDirection::Descendants, SELA),
    ] {
        parity(
            &fixture,
            Operation::GenerationalLearnedLineage {
                viewer: MINA.into(),
                character: subject.into(),
                at: at(20, 0),
                direction,
                max_depth: 8,
                limit: 100,
            },
            knowledge::lineage(&world, MINA, subject, &horizon, direction, 8, 100).unwrap(),
            name,
            &format!("character={subject}"),
            &[subject],
            true,
        )
        .await;
    }
    parity(
        &fixture,
        Operation::GenerationalLearnedPath {
            viewer: MINA.into(),
            from: MINA.into(),
            to: SELA.into(),
            at: at(20, 0),
            max_hops: 8,
            limit: 100,
        },
        knowledge::kinship_path(&world, MINA, MINA, SELA, &horizon, 8, 100).unwrap(),
        "path",
        &format!("from={MINA}&to={SELA}"),
        &[MINA, SELA],
        true,
    )
    .await;
    parity(
        &fixture,
        Operation::GenerationalLearnedOrganization {
            viewer: MINA.into(),
            organization: "Learned circle".into(),
            at: at(20, 0),
            max_depth: 8,
            limit: 100,
        },
        knowledge::organization(&world, MINA, ORG, &horizon, 8, 100).unwrap(),
        "organization",
        "organization=Learned%20circle",
        &["Learned circle"],
        true,
    )
    .await;
    parity(
        &fixture,
        Operation::GenerationalLearnedLegacy {
            viewer: MINA.into(),
            legacy: "Learned office".into(),
            at: at(20, 0),
            limit: 100,
        },
        knowledge::legacy(&world, MINA, LEGACY, &horizon, 100).unwrap(),
        "legacy",
        "legacy=Learned%20office",
        &["Learned office"],
        true,
    )
    .await;
    parity(
        &fixture,
        Operation::GenerationalLearnedVital {
            viewer: MINA.into(),
            character: MINA.into(),
            at: at(20, 0),
            limit: 100,
        },
        knowledge::vital(&world, MINA, MINA, &horizon, 100).unwrap(),
        "vital",
        &format!("character={MINA}"),
        &[MINA],
        true,
    )
    .await;
    parity(
        &fixture,
        Operation::GenerationalLearnedAssertions {
            viewer: MINA.into(),
            at: at(20, 0),
            kind: None,
            endpoint: Some(MINA.into()),
            held_history: true,
            limit: 100,
        },
        knowledge::assertions(&world, MINA, &horizon, None, Some(MINA), true, 100).unwrap(),
        "history",
        &format!("endpoint={MINA}"),
        &[MINA],
        true,
    )
    .await;
}

#[tokio::test]
async fn author_and_learned_labels_discovery_search_and_context_match_all_interfaces() {
    let fixture = Fixture::new();
    let world = fixture.world();
    let horizon = point(20, 0);
    let ids = vec![MINA.into(), SELA.into()];
    for learned in [false, true] {
        let viewer = learned.then(|| MINA.to_owned());
        parity(
            &fixture,
            Operation::GenerationalLabels {
                viewer: viewer.clone(),
                ids: ids.clone(),
                at: at(20, 0),
                limit: 100,
            },
            if learned {
                discovery::learned_labels(&world, MINA, &ids, &horizon, 100)
            } else {
                discovery::labels(&world, &ids, &horizon, 100)
            }
            .unwrap(),
            "labels",
            &format!("ids={MINA},{SELA}"),
            &[MINA, SELA],
            learned,
        )
        .await;
        let prefix = if learned { "learned" } else { "Mina" };
        parity(
            &fixture,
            Operation::GenerationalDiscover {
                viewer: viewer.clone(),
                kind: "character".into(),
                prefix: prefix.into(),
                at: at(20, 0),
                limit: 100,
            },
            if learned {
                discovery::learned_discover(&world, MINA, "character", prefix, &horizon, 100)
            } else {
                discovery::discover(&world, "character", prefix, &horizon, 100)
            }
            .unwrap(),
            "discover",
            &format!("kind=character&prefix={prefix}"),
            &["character", "--prefix", prefix],
            learned,
        )
        .await;
        let query = if learned { "learned" } else { "parentage" };
        parity(
            &fixture,
            Operation::GenerationalSearch {
                viewer: viewer.clone(),
                text: query.into(),
                at: at(20, 0),
                limit: 100,
            },
            if learned {
                discovery::learned_search(&world, MINA, query, &horizon, 100)
            } else {
                discovery::search(&world, query, &horizon, 100)
            }
            .unwrap(),
            "search",
            &format!("q={query}"),
            &[query],
            learned,
        )
        .await;
        parity(
            &fixture,
            Operation::GenerationalContext {
                viewer,
                subject: MINA.into(),
                at: at(20, 0),
                max_depth: 8,
                limit: 100,
                max_chars: 1_000_000,
            },
            if learned {
                discovery::learned_context(&world, MINA, MINA, &horizon, 8, 100, 1_000_000)
            } else {
                discovery::context(&world, MINA, &horizon, 8, 100, 1_000_000)
            }
            .unwrap(),
            "context",
            &format!("subject={MINA}&max_chars=1000000"),
            &[MINA, "--max-chars", "1000000"],
            learned,
        )
        .await;
    }
}

#[tokio::test]
async fn exact_horizons_rejection_forgetting_and_validity_are_preserved_in_transport() {
    let fixture = Fixture::new();
    for (tick, order, count) in [
        (-1_i64, 0, 0),
        (10, 1, 0),
        (10, 2, 7),
        (9_007_199_254_740_993, i32::MAX, 7),
    ] {
        let (status, result) = get(
            fixture.service(),
            &format!(
                "/api/generational/learned/assertions?viewer={MINA}&tick={tick}&order={order}"
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{result}");
        private_envelope(&result);
        assert_eq!(result["result"]["items"].as_array().unwrap().len(), count);
        assert_eq!(result["result"]["effectiveTime"]["tick"], tick.to_string());
        assert_eq!(
            result["result"]["effectiveTime"]["order"],
            order.to_string()
        );
    }
    fixture.edit(&id(0), |record| {
        record["transitions"]
            .as_sequence_mut()
            .unwrap()
            .push(serde_yaml::to_value(transition(10, "rejected", 20, 0)).unwrap())
    });
    fixture.edit(&id(1), |record| {
        record["transitions"]
            .as_sequence_mut()
            .unwrap()
            .push(serde_yaml::to_value(transition(11, "forgotten", 20, 0)).unwrap())
    });
    fixture.edit(&id(6), |record| {
        record["claim"]["genealogy"]["valid"]["until"] =
            serde_yaml::to_value(json!({"timeline":"main","tick":19,"order":0})).unwrap()
    });
    fixture.valid();
    for (held, count) in [(false, 4), (true, 5)] {
        let (status,result)=get(fixture.service(),&format!("/api/generational/learned/assertions?viewer={MINA}&tick=20&order=0&held_history={held}")).await;
        assert_eq!(status, StatusCode::OK, "{result}");
        assert_eq!(result["result"]["items"].as_array().unwrap().len(), count);
        assert!(!result.to_string().contains(&id(0)));
        assert!(!result.to_string().contains(&id(1)));
    }
    let (status, labels) = get(
        fixture.service(),
        &format!("/api/generational/learned/labels?viewer={MINA}&ids={MINA}&tick=20&order=0"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(labels.to_string().contains("Learned child"));
}

#[tokio::test]
async fn learned_routes_are_closed_and_fail_before_returning_partial_results() {
    let fixture = Fixture::new();
    let base = format!("viewer={MINA}&tick=20");
    for (path, params) in [
        ("assertions", format!("{base}&perspective=author")),
        ("assertions", format!("{base}&viewer={MINA}")),
        ("assertions", format!("{base}&held_history=yes")),
        ("assertions", format!("{base}&held_history=True")),
        ("assertions", format!("{base}&kind=")),
        ("assertions", format!("{base}&endpoint=")),
        ("assertions", format!("{base}&limit=0")),
        ("assertions", format!("{base}&limit=1001")),
        ("assertions", format!("{base}&limit=1")),
        (
            "assertions",
            format!("viewer={MINA}&tick=9223372036854775808"),
        ),
        ("assertions", format!("viewer={MINA}&tick=20.5")),
        ("assertions", format!("{base}&order=2147483648")),
        ("assertions", format!("{base}&endpoint=%FF")),
        ("family", format!("tick=20&character={MINA}")),
        ("ancestors", format!("{base}&character={MINA}&max_depth=33")),
        ("ancestors", format!("{base}&character={MINA}&max_depth=0")),
        ("path", format!("{base}&from={MINA}&to={SELA}&max_hops=0")),
        (
            "organization",
            format!("{base}&organization={ORG}&include_former=true"),
        ),
        ("labels", format!("{base}&ids={MINA},{MINA}")),
        ("labels", format!("{base}&ids={MINA},")),
        ("discover", format!("{base}&kind=knowledge")),
        ("context", format!("{base}&subject={MINA}&max_chars=79")),
        ("context", format!("{base}&subject={MINA}&max_chars=80")),
        (
            "history",
            format!("{base}&endpoint={MINA}&held_history=false"),
        ),
    ] {
        let (status, error) = get(
            fixture.service(),
            &format!("/api/generational/learned/{path}?{params}"),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}?{params}: {error}");
        assert!(error.get("result").is_none());
        assert!(error.get("fingerprint").is_none());
        assert!(error.get("revision").is_none());
    }
    let (status, error) = get(
        fixture.service(),
        &format!("/api/generational/family?character={MINA}&tick=20&viewer={MINA}"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
    let (status, _) = get(
        fixture.service(),
        &format!("/api/generational/learned/organization-legacies?{base}&organization={ORG}"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn canonical_titles_are_never_character_selectors_and_learned_ambiguity_is_explicit() {
    let fixture = Fixture::new();
    let (status, unknown) = get(
        fixture.service(),
        &format!("/api/generational/learned/family?viewer={MINA}&character=Mina%20Vale&tick=20"),
    )
    .await;
    assert!(status.is_client_error(), "{unknown}");
    let (status, viewer) = get(
        fixture.service(),
        &format!(
            "/api/generational/learned/family?viewer=Learned%20child&character={MINA}&tick=20"
        ),
    )
    .await;
    assert!(status.is_client_error(), "{viewer}");
    // Context resolves a character subject: an identically named organization
    // must not turn that otherwise unambiguous character selector into an error.
    fixture.edit(&id(2), |record| {
        record["claim"]["genealogy"]["labels"][ORG] = Yaml::String("Learned child".into())
    });
    fixture.valid();
    parity(
        &fixture,
        Operation::GenerationalContext {
            viewer: Some(MINA.into()),
            subject: "Learned child".into(),
            at: at(20, 0),
            max_depth: 8,
            limit: 100,
            max_chars: 1_000_000,
        },
        discovery::learned_context(
            &fixture.world(),
            MINA,
            "Learned child",
            &point(20, 0),
            8,
            100,
            1_000_000,
        )
        .unwrap(),
        "context",
        "subject=Learned%20child&max_chars=1000000",
        &["Learned child", "--max-chars", "1000000"],
        true,
    )
    .await;
    fixture.edit(&id(0), |record| {
        record["claim"]["genealogy"]["labels"][SELA] = Yaml::String("Learned child".into())
    });
    fixture.valid();
    let service = fixture.service();
    let expected = service
        .execute(Operation::GenerationalLearnedFamily {
            viewer: MINA.into(),
            character: "Learned child".into(),
            at: at(20, i32::MAX),
            limit: 100,
        })
        .unwrap_err()
        .json();
    let (status, error) = get(
        service,
        &format!(
            "/api/generational/learned/family?viewer={MINA}&character=Learned%20child&tick=20"
        ),
    )
    .await;
    let args = [
        "generational",
        "learned",
        MINA,
        "family",
        "Learned child",
        "--tick",
        "20",
    ]
    .map(str::to_owned);
    let (code, output) = cli(&fixture, &args);
    assert!(status.is_client_error());
    assert_eq!(code, 2);
    assert_eq!(error, expected);
    assert_eq!(output, expected);
    assert!(error.get("fingerprint").is_none());
}

#[tokio::test]
async fn hidden_source_changes_leave_character_successes_and_failures_byte_identical() {
    let fixture = Fixture::new();
    let before = fixture.service();
    let paths = [
        format!("family?character={MINA}"),
        format!("path?from={MINA}&to={CORA}"),
        "assertions?held_history=true".into(),
        format!("labels?ids={MINA},{SELA}"),
        "discover?kind=character&prefix=learned".into(),
        "search?q=learned".into(),
        format!("context?subject={MINA}&max_chars=1000000"),
        "family?character=Mina%20Vale".into(),
        format!("assertions?limit=1"),
    ];
    let mut expected = Vec::new();
    for path in &paths {
        expected.push(
            get(
                before.clone(),
                &format!("/api/generational/learned/{path}&viewer={MINA}&tick=20&order=0"),
            )
            .await,
        );
    }
    fixture.edit(MINA, |record| {
        record["title"] = Yaml::String("Hidden renamed canonical character".into())
    });
    fixture.edit(KINSHIP, |record| {
        record["title"] = Yaml::String("Hidden changed canonical family".into())
    });
    fixture.edit(&id(0), |record| {
        record["title"] = Yaml::String("Hidden changed knowledge metadata".into());
        record["claim"]["statement"] = Yaml::String("Hidden replacement outer statement".into());
        record["transitions"]
            .as_sequence_mut()
            .unwrap()
            .push(serde_yaml::to_value(transition(99, "rejected", 100, 0)).unwrap());
    });
    fixture.edit(TEMPLATE, |record| {
        record["claim"]["statement"] = Yaml::String("Hidden unrelated ordinary belief".into())
    });
    fixture.valid();
    let after = fixture.service();
    assert_ne!(before.world().fingerprint, after.world().fingerprint);
    for (path, expected) in paths.iter().zip(expected) {
        let actual = get(
            after.clone(),
            &format!("/api/generational/learned/{path}&viewer={MINA}&tick=20&order=0"),
        )
        .await;
        assert_eq!(actual, expected, "{path}");
        if actual.0.is_success() {
            private_envelope(&actual.1);
        }
    }
}

#[tokio::test]
async fn existing_author_reads_retain_canonical_semantics_with_learned_assertions_present() {
    let fixture = Fixture::new();
    let world = fixture.world();
    let expected = generational::legacy(&world, LEGACY, &point(20, 0), 100).unwrap();
    assert_eq!(expected["holders"][0]["value"]["holderId"], CORA);
    parity(
        &fixture,
        Operation::GenerationalLegacy {
            legacy: LEGACY.into(),
            at: at(20, 0),
            limit: 100,
        },
        expected,
        "legacy",
        &format!("legacy={LEGACY}"),
        &[LEGACY],
        false,
    )
    .await;
}
