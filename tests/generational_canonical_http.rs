//! Canonical organization, succession and vital reads share one author-horizon contract.
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
    generational, http,
    model::{StoryTime, World},
    service::{Operation, Service, TimeInput},
    source, validation,
};

const ORG: &str = "organization_4YNRVZSRYTGBB27YKD7NW1T7CZ";
const GUILD: &str = "organization_5JNW6KPDP187ZKYZ2JRE9TP1ZG";
const AFFILIATION: &str = "affiliation_3163J7T1VFYEY88H175PSMKJR5";
const LEGACY: &str = "legacy_4N3B8XV3V1QN8E13H8V5CHWDPQ";
const TENURE: &str = "tenure_7XHCEMFF7JDR55YP3NCW76C2F0";
const CLAIM: &str = "claim_289DFQ0S7Z2YDEFBJXY2H5YPXH";
const VITAL: &str = "vital_7YPSPVFBM15VGK0NS739PNBQ18";
const SELA: &str = "char_39E9EWWC017E40519033S4RW4X";
const CORA: &str = "char_46VTBJZ9TNMTF7G14N5GKYQ9C1";
const PARENTAGE: &str = "kinship_6FMPV333HWER1D3NXKSSNP6AZ9";
const UNION: &str = "union_04W52R482EPFAG76MX1Y0DK2M1";

fn at(tick: i64, order: i32, timeline: &str) -> TimeInput {
    TimeInput {
        tick,
        order,
        timeline: Some(timeline.into()),
    }
}
fn point(tick: i64, order: i32, timeline: &str) -> StoryTime {
    StoryTime::new(timeline, tick, order).unwrap()
}
fn transition(number: u32, kind: &str, tick: i64, order: i32, payload: Value) -> Yaml {
    serde_yaml::to_value(json!({"transition_id":format!("transition_{number:026}"),"transition_kind":kind,
      "applicability":{"applicability_kind":"instant","point":{"timeline":"main","tick":tick,"order":order}},"payload":payload})).unwrap()
}
struct Fixture {
    _directory: Option<tempfile::TempDir>,
    root: PathBuf,
}
impl Fixture {
    fn packaged() -> Self {
        Self {
            _directory: None,
            root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/tideglass"),
        }
    }
    fn copied() -> Self {
        let source = Self::packaged().world();
        let directory = tempfile::tempdir().unwrap();
        for record in source.records.values() {
            let path = directory.path().join(&record.source_path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, &record.raw_bytes).unwrap();
        }
        Self {
            root: directory.path().to_path_buf(),
            _directory: Some(directory),
        }
    }
    fn world(&self) -> World {
        source::load_world(&self.root, None).unwrap()
    }
    fn service(&self) -> Service {
        Service::new(self.world())
    }
    fn edit(&self, id: &str, edit: impl FnOnce(&mut Yaml)) {
        let world = self.world();
        let record = &world.records[id];
        let mut metadata = record.frontmatter.clone();
        edit(&mut metadata);
        fs::write(
            self.root.join(&record.source_path),
            format!(
                "---\n{}---\n{}",
                serde_yaml::to_string(&metadata).unwrap(),
                record.body
            ),
        )
        .unwrap();
    }
    fn valid(&self) {
        let report = validation::validate_world(&self.world());
        assert!(report.valid, "{:?}", report.diagnostics);
    }
}
fn cli(fixture: &Fixture, args: &[&str]) -> (i32, Value) {
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
async fn parity(
    fixture: &Fixture,
    operation: Operation,
    core: Value,
    uri: &str,
    args: &[&str],
) -> Value {
    let service = fixture.service();
    let expected = service.execute(operation).unwrap();
    assert_eq!(expected["result"], core);
    let (status, actual) = get(service.clone(), uri).await;
    let (code, command) = cli(fixture, args);
    assert_eq!(status, StatusCode::OK, "{actual}");
    assert_eq!(code, 0, "{command}");
    assert_eq!(actual, expected);
    assert_eq!(command, expected);
    assert_eq!(actual["protocol"], wedl::CONTRACT);
    assert_eq!(actual["fingerprint"], service.world().fingerprint);
    assert_eq!(core["scope"], "author-as-of-generational-source");
    assert_eq!(core["perspective"], "author");
    assert_eq!(core["complete"], true);
    core
}

#[tokio::test]
async fn tideglass_organization_legacy_list_and_vital_match_all_four_interfaces() {
    let fixture = Fixture::packaged();
    fixture.valid();
    let world = fixture.world();
    let horizon = point(20, 0, "main");
    let organization = parity(
        &fixture,
        Operation::GenerationalOrganization {
            organization: "beacon cooperative".into(),
            at: at(20, 0, "main"),
            max_depth: 8,
            include_former: false,
            limit: 100,
        },
        generational::organization(&world, "beacon cooperative", &horizon, 8, false, 100).unwrap(),
        "/api/generational/organization?organization=beacon%20cooperative&tick=20&order=0",
        &[
            "generational",
            "organization",
            "beacon cooperative",
            "--tick",
            "20",
            "--order",
            "0",
        ],
    )
    .await;
    assert_eq!(organization["available"], true);
    assert_eq!(organization["recordId"], ORG);
    assert_eq!(organization["roles"].as_array().unwrap().len(), 1);
    assert_eq!(
        organization["roles"][0]["value"]["role"],
        "repair coordinator"
    );
    assert!(organization.get("formerRoles").is_none());
    let legacy = parity(
        &fixture,
        Operation::GenerationalLegacy {
            legacy: LEGACY.into(),
            at: at(20, 0, "main"),
            limit: 100,
        },
        generational::legacy(&world, LEGACY, &horizon, 100).unwrap(),
        &format!("/api/generational/legacy?legacy={LEGACY}&tick=20&order=0"),
        &[
            "generational",
            "legacy",
            LEGACY,
            "--tick",
            "20",
            "--order",
            "0",
        ],
    )
    .await;
    assert_eq!(legacy["holderCount"], 1);
    assert_eq!(legacy["holders"][0]["value"]["holderId"], CORA);
    assert_eq!(legacy["claims"][0]["lifecycle"], "disputed");
    assert_eq!(legacy["count"], 2);
    assert_eq!(legacy["successionCount"], 0);
    let list = parity(
        &fixture,
        Operation::GenerationalOrganizationLegacies {
            organization: ORG.into(),
            at: at(20, 0, "main"),
            limit: 100,
        },
        generational::organization_legacies(&world, ORG, &horizon, 100).unwrap(),
        &format!("/api/generational/organization-legacies?organization={ORG}&tick=20&order=0"),
        &[
            "generational",
            "organization-legacies",
            ORG,
            "--tick",
            "20",
            "--order",
            "0",
        ],
    )
    .await;
    assert_eq!(list["count"], 1);
    assert_eq!(list["items"][0]["recordId"], LEGACY);
    let vital = parity(
        &fixture,
        Operation::GenerationalVital {
            character: SELA.into(),
            at: at(20, 0, "main"),
            limit: 100,
        },
        generational::vital(&world, SELA, &horizon, 100).unwrap(),
        &format!("/api/generational/vital?character={SELA}&tick=20&order=0"),
        &[
            "generational",
            "vital",
            SELA,
            "--tick",
            "20",
            "--order",
            "0",
        ],
    )
    .await;
    assert_eq!(vital["available"], true);
    assert_eq!(vital["count"], 1);
    assert_eq!(vital["ambiguous"], false);
    assert_eq!(vital["items"][0]["recordId"], VITAL);
}

#[tokio::test]
async fn history_covers_eight_kinds_with_inclusive_initialization_and_transition_cutoffs() {
    let fixture = Fixture::packaged();
    let world = fixture.world();
    for (record, last) in [
        (PARENTAGE, Some(0)),
        (UNION, Some(0)),
        (ORG, None),
        (AFFILIATION, Some(0)),
        (LEGACY, None),
        (TENURE, Some(0)),
        (CLAIM, Some(20)),
        (VITAL, Some(-30)),
    ] {
        let mut horizons = vec![(-40, -1, 0), (-40, 0, 1)];
        if let Some(tick) = last {
            horizons.extend([(tick, -1, 1), (tick, 0, 2)]);
        }
        for (tick, order, count) in horizons {
            let tick_text = tick.to_string();
            let order_text = order.to_string();
            let result = parity(
                &fixture,
                Operation::GenerationalHistory {
                    record: record.into(),
                    at: at(tick, order, "main"),
                    limit: 100,
                },
                generational::history(&world, record, &point(tick, order, "main"), 100).unwrap(),
                &format!("/api/generational/history?record={record}&tick={tick}&order={order}"),
                &[
                    "generational",
                    "history",
                    record,
                    "--tick",
                    &tick_text,
                    "--order",
                    &order_text,
                ],
            )
            .await;
            assert_eq!(result["count"], count);
            assert_eq!(result["available"], count > 0);
            for entry in result["history"].as_array().unwrap() {
                assert!(entry["time"]["tick"].is_string());
                assert!(entry["time"]["order"].is_string());
                assert!(entry["citation"].is_object());
            }
        }
    }
}

#[tokio::test]
async fn organization_hierarchy_and_former_affiliations_use_the_selected_horizon() {
    let fixture = Fixture::copied();
    fixture.edit(ORG, |record| {
        record["parent_id"] = Yaml::String(GUILD.into())
    });
    fixture.edit(AFFILIATION, |record| {
        record["transitions"]
            .as_sequence_mut()
            .unwrap()
            .push(transition(1, "affiliation-end", 10, 3, json!({})))
    });
    fixture.valid();
    let world = fixture.world();
    for (order, include_former, current, former) in
        [(2, false, 1, 0), (3, false, 0, 0), (3, true, 0, 1)]
    {
        let order_text = order.to_string();
        let former_text = include_former.to_string();
        let mut args = vec![
            "generational",
            "organization",
            ORG,
            "--tick",
            "10",
            "--order",
            &order_text,
            "--max-depth",
            "1",
        ];
        if include_former {
            args.push("--include-former");
        }
        let result=parity(&fixture,
          Operation::GenerationalOrganization{organization:ORG.into(),at:at(10,order,"main"),max_depth:1,include_former,limit:100},
          generational::organization(&world,ORG,&point(10,order,"main"),1,include_former,100).unwrap(),
          &format!("/api/generational/organization?organization={ORG}&tick=10&order={order}&max_depth=1&include_former={former_text}"),&args).await;
        assert_eq!(result["parentPath"][0]["targetId"], GUILD);
        assert_eq!(result["roles"].as_array().unwrap().len(), current);
        if include_former {
            assert_eq!(result["formerRoles"].as_array().unwrap().len(), former);
        } else {
            assert!(result.get("formerRoles").is_none());
        }
    }
    let (status, error) = get(
        fixture.service(),
        &format!("/api/generational/organization?organization={ORG}&tick=10&max_depth=0"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
    assert!(error.get("result").is_none());
}

#[tokio::test]
async fn legacy_vacancy_preserves_claims_without_appointing_a_claimant() {
    let fixture = Fixture::copied();
    fixture.edit(TENURE, |record| {
        record["transitions"]
            .as_sequence_mut()
            .unwrap()
            .push({
                let mut vacancy=transition(2, "tenure-vacate", 30, 2, json!({"holder_id":null}));
                vacancy["applicability"]=serde_yaml::to_value(json!({"applicability_kind":"inclusive-interval","first":{"timeline":"main","tick":30,"order":2},"last":{"timeline":"main","tick":40,"order":0}})).unwrap();
                vacancy
            })
    });
    fixture.valid();
    let world = fixture.world();
    for (tick, order, holders, lifecycle) in [
        (30, 1, 1, "holding"),
        (30, 2, 0, "vacant-interval"),
        (40, 0, 0, "vacant-interval"),
        (40, 1, 0, "expired"),
    ] {
        let tick_text = tick.to_string();
        let order_text = order.to_string();
        let result = parity(
            &fixture,
            Operation::GenerationalLegacy {
                legacy: LEGACY.into(),
                at: at(tick, order, "main"),
                limit: 100,
            },
            generational::legacy(&world, LEGACY, &point(tick, order, "main"), 100).unwrap(),
            &format!("/api/generational/legacy?legacy={LEGACY}&tick={tick}&order={order}"),
            &[
                "generational",
                "legacy",
                LEGACY,
                "--tick",
                &tick_text,
                "--order",
                &order_text,
            ],
        )
        .await;
        assert_eq!(result["holderCount"], holders);
        assert_eq!(result["tenures"].as_array().unwrap().len(), 1);
        assert_eq!(result["tenures"][0]["lifecycle"], lifecycle);
        assert_eq!(result["claims"][0]["recordId"], CLAIM);
        assert_eq!(result["claims"][0]["lifecycle"], "disputed");
        assert_eq!(result["succession"], json!([]));
    }
}

#[tokio::test]
async fn new_queries_preserve_large_ticks_default_order_and_timeline_isolation() {
    let fixture = Fixture::copied();
    let world_id = fixture.world().world_record().unwrap().id.clone();
    fixture.edit(&world_id, |record| {
        record["timelines"]
            .as_sequence_mut()
            .unwrap()
            .push(serde_yaml::to_value(json!({"id":"other","label":"Other"})).unwrap())
    });
    fixture.valid();
    let world = fixture.world();
    let tick = 9_007_199_254_740_993;
    let tick_text = tick.to_string();
    for timeline in ["main", "other"] {
        let horizon = point(tick, i32::MAX, timeline);
        let common = at(tick, i32::MAX, timeline);
        let cases = [
            (
                "organization",
                "organization",
                ORG,
                Operation::GenerationalOrganization {
                    organization: ORG.into(),
                    at: common.clone(),
                    max_depth: 8,
                    include_former: false,
                    limit: 100,
                },
                generational::organization(&world, ORG, &horizon, 8, false, 100).unwrap(),
            ),
            (
                "legacy",
                "legacy",
                LEGACY,
                Operation::GenerationalLegacy {
                    legacy: LEGACY.into(),
                    at: common.clone(),
                    limit: 100,
                },
                generational::legacy(&world, LEGACY, &horizon, 100).unwrap(),
            ),
            (
                "organization-legacies",
                "organization",
                ORG,
                Operation::GenerationalOrganizationLegacies {
                    organization: ORG.into(),
                    at: common.clone(),
                    limit: 100,
                },
                generational::organization_legacies(&world, ORG, &horizon, 100).unwrap(),
            ),
            (
                "vital",
                "character",
                SELA,
                Operation::GenerationalVital {
                    character: SELA.into(),
                    at: common.clone(),
                    limit: 100,
                },
                generational::vital(&world, SELA, &horizon, 100).unwrap(),
            ),
        ];
        for (name, key, selector, operation, core) in cases {
            let result = parity(
                &fixture,
                operation,
                core,
                &format!(
                    "/api/generational/{name}?{key}={selector}&tick={tick_text}&timeline={timeline}"
                ),
                &[
                    "generational",
                    name,
                    selector,
                    "--tick",
                    &tick_text,
                    "--timeline",
                    timeline,
                ],
            )
            .await;
            assert_eq!(
                result["effectiveTime"],
                json!({"tick":tick_text,"order":"2147483647","timeline":timeline})
            );
            assert_eq!(result["available"], timeline == "main");
        }
    }
}

#[tokio::test]
async fn vital_disclosure_and_multiple_histories_preserve_authored_uncertainty() {
    let fixture = Fixture::copied();
    let original_transitions = fixture.world().records[VITAL].frontmatter["transitions"].clone();
    for disclosure in ["unknown", "withheld"] {
        fixture.edit(VITAL, |record| {
            record["disclosure"] = Yaml::String(disclosure.into());
            record["transitions"] = if disclosure == "unknown" {
                Yaml::Sequence(vec![])
            } else {
                original_transitions.clone()
            };
        });
        fixture.valid();
        let world = fixture.world();
        let result = parity(
            &fixture,
            Operation::GenerationalVital {
                character: SELA.into(),
                at: at(20, 0, "main"),
                limit: 1,
            },
            generational::vital(&world, SELA, &point(20, 0, "main"), 1).unwrap(),
            &format!("/api/generational/vital?character={SELA}&tick=20&order=0&limit=1"),
            &[
                "generational",
                "vital",
                SELA,
                "--tick",
                "20",
                "--order",
                "0",
                "--limit",
                "1",
            ],
        )
        .await;
        assert_eq!(result["available"], false);
        assert_eq!(result["reason"], "unknown");
        assert_eq!(result["count"], 0);
        assert_eq!(result["items"], json!([]));
        assert_eq!(result["states"], json!([]));
        assert_eq!(result["vital"], Value::Null);
        let history = parity(
            &fixture,
            Operation::GenerationalHistory {
                record: VITAL.into(),
                at: at(20, 0, "main"),
                limit: 1,
            },
            generational::history(&world, VITAL, &point(20, 0, "main"), 1).unwrap(),
            &format!("/api/generational/history?record={VITAL}&tick=20&order=0&limit=1"),
            &[
                "generational",
                "history",
                VITAL,
                "--tick",
                "20",
                "--order",
                "0",
                "--limit",
                "1",
            ],
        )
        .await;
        assert_eq!(history["available"], false);
        assert_eq!(history["reason"], "undisclosed");
        assert_eq!(history["history"], json!([]));
        assert!(history.get("citations").is_none());
    }
    fixture.edit(VITAL, |record| {
        record["disclosure"] = Yaml::String("known".into())
    });
    let world = fixture.world();
    let original = &world.records[VITAL];
    let alternate_id = "vital_00000000000000000000000002";
    let mut alternate = original.frontmatter.clone();
    alternate["id"] = Yaml::String(alternate_id.into());
    alternate["title"] = Yaml::String("Another authored vital history".into());
    alternate["initialization"] = transition(31, "vital-initialize", -40, 0, json!({}));
    alternate["transitions"] = Yaml::Sequence(vec![
        transition(32, "vital-birth", -30, 0, json!({})),
        transition(33, "vital-death", 10, 3, json!({})),
    ]);
    let path = PathBuf::from(&original.source_path).with_file_name(format!("{alternate_id}.md"));
    fs::write(
        fixture.root.join(path),
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&alternate).unwrap(),
            original.body
        ),
    )
    .unwrap();
    fixture.valid();
    let world = fixture.world();
    for (order, states, ambiguous) in [
        (2, json!(["living"]), false),
        (3, json!(["dead", "living"]), true),
    ] {
        let order_text = order.to_string();
        let result = parity(
            &fixture,
            Operation::GenerationalVital {
                character: SELA.into(),
                at: at(10, order, "main"),
                limit: 100,
            },
            generational::vital(&world, SELA, &point(10, order, "main"), 100).unwrap(),
            &format!("/api/generational/vital?character={SELA}&tick=10&order={order}"),
            &[
                "generational",
                "vital",
                SELA,
                "--tick",
                "10",
                "--order",
                &order_text,
            ],
        )
        .await;
        assert_eq!(result["count"], 2);
        assert_eq!(result["states"], states);
        assert_eq!(result["ambiguous"], ambiguous);
        if ambiguous {
            assert_eq!(result["vital"], Value::Null);
        } else {
            assert_eq!(result["vital"], "living");
        }
    }
}

#[tokio::test]
async fn closed_queries_reject_invalid_flags_horizons_kinds_and_complete_limits() {
    let fixture = Fixture::packaged();
    for uri in [
        format!("/api/generational/organization?organization={ORG}"),
        format!("/api/generational/legacy?legacy={LEGACY}"),
        format!("/api/generational/organization-legacies?organization={ORG}"),
        format!("/api/generational/vital?character={SELA}"),
        format!("/api/generational/organization?organization={ORG}&tick=0&include_former=1"),
        format!("/api/generational/organization?organization={ORG}&tick=0&include_former=True"),
        format!("/api/generational/organization?organization={ORG}&tick=0&max_depth=33"),
        format!("/api/generational/organization?organization={ORG}&tick=0&max_depth=-1"),
        format!(
            "/api/generational/organization?organization={ORG}&tick=0&include_former=true&include_former=false"
        ),
        format!("/api/generational/legacy?legacy={LEGACY}&tick=20&limit=1"),
        format!("/api/generational/organization?organization={ORG}&tick=20&limit=1"),
        format!("/api/generational/vital?character={SELA}&tick=20&limit=1"),
        format!("/api/generational/legacy?legacy={LEGACY}&tick=20&include_former=true"),
        format!("/api/generational/organization-legacies?organization={ORG}&tick=20&max_depth=1"),
        format!("/api/generational/vital?character={SELA}&tick=20&view=character"),
        format!("/api/generational/vital?character={SELA}&tick=20&perspective=author"),
        format!("/api/generational/legacy?legacy={LEGACY}&tick=20&limit=0"),
        format!("/api/generational/organization-legacies?organization={ORG}&tick=20&limit=1001"),
        format!("/api/generational/vital?character={SELA}&tick=9223372036854775808"),
        format!("/api/generational/vital?character={SELA}&tick=20&order=2147483648"),
        format!("/api/generational/legacy?legacy={ORG}&tick=20"),
        format!("/api/generational/vital?character={VITAL}&tick=20"),
    ] {
        let (status, value) = get(fixture.service(), &uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {value}");
        assert!(value.get("result").is_none());
    }
    for args in [
        vec!["generational", "organization", ORG],
        vec![
            "generational",
            "organization",
            ORG,
            "--tick",
            "0",
            "--max-depth",
            "33",
        ],
        vec![
            "generational",
            "legacy",
            LEGACY,
            "--tick",
            "20",
            "--limit",
            "1",
        ],
        vec![
            "generational",
            "vital",
            SELA,
            "--tick",
            "20",
            "--view",
            "character",
        ],
        vec![
            "generational",
            "organization-legacies",
            ORG,
            "--tick",
            "20",
            "--include-former",
        ],
    ] {
        let (code, value) = cli(&fixture, &args);
        assert_eq!(code, 2, "{value}");
        assert!(value.get("result").is_none());
    }
}
