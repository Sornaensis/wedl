//! One exact author-horizon contract behind CLI, service and GET family reads.
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use wedl::{
    generational::LineageDirection,
    http,
    service::{Operation, Service, TimeInput},
    source, validation,
};

const OWNER: &str = "world_00000000000000000000000000";
const A: &str = "char_00000000000000000000000001";
const B: &str = "char_00000000000000000000000002";
const C: &str = "char_00000000000000000000000003";
const D: &str = "char_00000000000000000000000004";
const AB: &str = "kinship_00000000000000000000000001";
const BC: &str = "kinship_00000000000000000000000002";
const UNION: &str = "union_00000000000000000000000001";

fn at(tick: i64, order: i32, timeline: &str) -> TimeInput {
    TimeInput {
        tick,
        order,
        timeline: Some(timeline.into()),
    }
}

fn transition(number: u32, kind: &str, tick: i64, order: i32, payload: Value) -> Value {
    json!({"transition_id":format!("transition_{number:026}"),"transition_kind":kind,
      "applicability":{"applicability_kind":"instant","point":{"timeline":"main","tick":tick,"order":order}},"payload":payload})
}

struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
}
impl Fixture {
    fn new(start: i64) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_path_buf();
        fs::create_dir(root.join("story")).unwrap();
        let records = [
            json!({"kind":"world","id":OWNER,"title":"Family world","capabilities":["generational-core-v1"],
            "default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"state_keys":{}}),
            json!({"kind":"character","id":A,"title":"First Parent","aliases":["Elder"]}),
            json!({"kind":"character","id":B,"title":"Middle Child"}),
            json!({"kind":"character","id":C,"title":"Youngest Child"}),
            json!({"kind":"character","id":D,"title":"Union Partner"}),
            json!({"kind":"parentage","id":AB,"title":"First parentage","parent_id":A,"child_id":B,
            "initialization":transition(1,"parentage-initialize",start,0,json!({"basis":"biological"})),
            "transitions":[transition(2,"parentage-confirm",start+10,3,json!({"basis":"biological"})),
              transition(3,"parentage-end",start+20,7,json!({}))]}),
            json!({"kind":"parentage","id":BC,"title":"Second parentage","parent_id":B,"child_id":C,
            "initialization":transition(4,"parentage-initialize",start,0,json!({"basis":"adoptive"})),"transitions":[]}),
            json!({"kind":"union","id":UNION,"title":"Partnership","participant_ids":[A,D],
            "initialization":transition(5,"union-initialize",start,0,json!({"participant_ids":[A,D]})),
            "transitions":[transition(6,"union-form",start+10,3,json!({"participant_ids":[A,D]})),
              transition(7,"union-reconcile",start+15,0,json!({"participant_ids":[A,C,D]})),
              transition(8,"union-end",start+20,7,json!({}))]}),
        ];
        for (index, mut record) in records.into_iter().enumerate() {
            record["schema"] = json!(source::SOURCE_SCHEMA);
            record["status"] = json!("canonical");
            record["domain"] = json!("tests.family");
            record["tags"] = json!([]);
            if record.get("aliases").is_none() {
                record["aliases"] = json!([]);
            }
            record["threads"] = json!([]);
            record["audience"] = json!(["public"]);
            record["perspectives"] = json!(["ordinary"]);
            let raw = format!(
                "---\n{}---\nRetained prose.\n",
                serde_yaml::to_string(&record).unwrap()
            );
            fs::write(root.join(format!("story/record-{index}.md")), raw).unwrap();
        }
        let fixture = Self {
            _directory: directory,
            root,
        };
        let report = validation::validate_world(fixture.service().world());
        assert!(report.valid, "{:?}", report.diagnostics);
        fixture
    }
    fn service(&self) -> Service {
        Service::new(source::load_world(&self.root, None).unwrap())
    }
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
            .unwrap_or_else(|_| panic!("non-JSON: {}", String::from_utf8_lossy(bytes))),
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
async fn parity(fixture: &Fixture, operation: Operation, uri: &str, args: &[&str]) -> Value {
    let service = fixture.service();
    let expected = service.execute(operation).unwrap();
    let (status, actual) = get(service.clone(), uri).await;
    let (code, command) = cli(&fixture.root, args);
    assert_eq!(status, StatusCode::OK, "{actual}");
    assert_eq!(code, 0, "{command}");
    assert_eq!(actual, expected);
    assert_eq!(actual, command);
    assert_eq!(actual["protocol"], wedl::CONTRACT);
    assert_eq!(actual["fingerprint"], service.world().fingerprint);
    assert_eq!(
        actual["result"]["scope"],
        "author-as-of-generational-source"
    );
    actual["result"].clone()
}

#[tokio::test]
async fn family_interfaces_share_exact_transition_boundaries_and_timeline_isolation() {
    let fixture = Fixture::new(0);
    for (tick, order, timeline) in [
        (-1, i32::MAX, "main"),
        (0, 0, "main"),
        (10, 2, "main"),
        (10, 3, "main"),
        (20, 6, "main"),
        (20, 7, "main"),
        (10, 3, "other"),
    ] {
        let tick_text = tick.to_string();
        let order_text = order.to_string();
        let result=parity(&fixture,Operation::GenerationalFamily{character:B.into(),at:at(tick,order,timeline),limit:100},
          &format!("/api/generational/family?character={B}&tick={tick}&order={order}&timeline={timeline}"),
          &["generational","family",B,"--tick",&tick_text,"--order",&order_text,"--timeline",timeline]).await;
        assert_eq!(result["complete"], true);
        let active = timeline == "main" && (tick, order) >= (0, 0) && (tick, order) < (20, 7);
        assert_eq!(
            result["parents"].as_array().unwrap().len(),
            usize::from(active)
        );
    }
}

#[tokio::test]
async fn lineage_and_path_adapters_preserve_bounded_parentage_without_union_inference() {
    let fixture = Fixture::new(0);
    for (name, character, direction, depth, count) in [
        ("ancestors", B, LineageDirection::Ancestors, 1, 1),
        ("ancestors", C, LineageDirection::Ancestors, 2, 2),
        ("descendants", "Elder", LineageDirection::Descendants, 2, 2),
        ("descendants", C, LineageDirection::Descendants, 0, 0),
    ] {
        let depth_text = depth.to_string();
        let result = parity(
            &fixture,
            Operation::GenerationalLineage {
                character: character.into(),
                at: at(10, 3, "main"),
                direction,
                max_depth: depth,
                limit: 100,
            },
            &format!(
                "/api/generational/{name}?character={character}&tick=10&order=3&max_depth={depth}"
            ),
            &[
                "generational",
                name,
                character,
                "--tick",
                "10",
                "--order",
                "3",
                "--max-depth",
                &depth_text,
            ],
        )
        .await;
        assert_eq!(result["count"], count);
        assert_eq!(result["complete"], true);
        for item in result["items"].as_array().unwrap() {
            assert!(item["generationDistance"].is_string());
            for edge in item["edges"].as_array().unwrap() {
                assert!(edge["recordId"].as_str().unwrap().starts_with("kinship_"));
                assert!(!edge["citations"].as_array().unwrap().is_empty());
            }
        }
    }
    for (from, to, hops, found, distance) in [
        (C, A, 2, true, Some("2")),
        (C, B, 1, true, Some("1")),
        (C, D, 8, false, None),
        (C, C, 0, true, Some("0")),
    ] {
        let hops_text = hops.to_string();
        let result = parity(
            &fixture,
            Operation::GenerationalPath {
                from: from.into(),
                to: to.into(),
                at: at(10, 3, "main"),
                max_hops: hops,
                limit: 100,
            },
            &format!("/api/generational/path?from={from}&to={to}&tick=10&order=3&max_hops={hops}"),
            &[
                "generational",
                "path",
                from,
                to,
                "--tick",
                "10",
                "--order",
                "3",
                "--max-hops",
                &hops_text,
            ],
        )
        .await;
        assert_eq!(result["found"], found);
        if let Some(distance) = distance {
            assert_eq!(result["distance"], distance);
        } else {
            assert_eq!(result["reason"], "no_match");
            assert_eq!(result["edges"], json!([]));
        }
    }
}

#[tokio::test]
async fn history_and_union_membership_reflect_only_the_selected_horizon() {
    let fixture = Fixture::new(0);
    for (tick, order, count, lifecycle) in [
        (0, 0, 1, "declared"),
        (10, 2, 1, "declared"),
        (10, 3, 2, "formed"),
        (15, 0, 3, "formed"),
        (20, 7, 4, "ended"),
    ] {
        let tick_text = tick.to_string();
        let order_text = order.to_string();
        let result = parity(
            &fixture,
            Operation::GenerationalHistory {
                record: UNION.into(),
                at: at(tick, order, "main"),
                limit: 100,
            },
            &format!("/api/generational/history?record={UNION}&tick={tick}&order={order}"),
            &[
                "generational",
                "history",
                UNION,
                "--tick",
                &tick_text,
                "--order",
                &order_text,
            ],
        )
        .await;
        assert_eq!(result["available"], true);
        assert_eq!(result["count"], count);
        assert_eq!(result["history"].as_array().unwrap().len(), count as usize);
        assert_eq!(result["lifecycle"], lifecycle);
        assert_eq!(result["active"], lifecycle != "ended");
        for entry in result["history"].as_array().unwrap() {
            let point = &entry["time"];
            assert!(
                (
                    point["tick"].as_str().unwrap().parse::<i64>().unwrap(),
                    point["order"].as_str().unwrap().parse::<i32>().unwrap()
                ) <= (tick, order)
            );
            assert!(entry["citation"].is_object());
        }
    }
    for (tick, order, count) in [(14, i32::MAX, 0), (15, 0, 1), (20, 6, 1), (20, 7, 0)] {
        let result = fixture
            .service()
            .execute(Operation::GenerationalFamily {
                character: C.into(),
                at: at(tick, order, "main"),
                limit: 100,
            })
            .unwrap();
        assert_eq!(result["result"]["unions"].as_array().unwrap().len(), count);
    }
    for (tick, timeline, reason) in [
        (-1, "main", "before_initialization"),
        (20, "other", "different_timeline"),
    ] {
        let tick_text = tick.to_string();
        let result = parity(
            &fixture,
            Operation::GenerationalHistory {
                record: AB.into(),
                at: at(tick, i32::MAX, timeline),
                limit: 100,
            },
            &format!("/api/generational/history?record={AB}&tick={tick}&timeline={timeline}"),
            &[
                "generational",
                "history",
                AB,
                "--tick",
                &tick_text,
                "--timeline",
                timeline,
            ],
        )
        .await;
        assert_eq!(result["available"], false);
        assert_eq!(result["reason"], reason);
        assert_eq!(result["history"], json!([]));
    }
}

#[tokio::test]
async fn exact_large_ticks_and_default_end_of_tick_survive_every_adapter() {
    let start = 9_007_199_254_740_993_i64;
    let fixture = Fixture::new(start);
    let tick = start + 10;
    let tick_text = tick.to_string();
    let result = parity(
        &fixture,
        Operation::GenerationalFamily {
            character: B.into(),
            at: TimeInput {
                tick,
                order: i32::MAX,
                timeline: None,
            },
            limit: 100,
        },
        &format!("/api/generational/family?character={B}&tick={tick}"),
        &["generational", "family", B, "--tick", &tick_text],
    )
    .await;
    assert_eq!(
        result["effectiveTime"],
        json!({"timeline":"main","tick":tick_text,"order":"2147483647"})
    );
    assert_eq!(result["parents"][0]["lifecycle"], "confirmed");
    let result = parity(
        &fixture,
        Operation::GenerationalHistory {
            record: AB.into(),
            at: at(tick, 3, "main"),
            limit: 100,
        },
        &format!("/api/generational/history?record={AB}&tick={tick}&order=3"),
        &[
            "generational",
            "history",
            AB,
            "--tick",
            &tick_text,
            "--order",
            "3",
        ],
    )
    .await;
    assert_eq!(result["history"][0]["time"]["tick"], start.to_string());
    assert_eq!(result["history"][1]["time"]["tick"], tick_text);
}

#[tokio::test]
async fn author_selectors_keep_exact_ids_and_unambiguous_title_or_alias_matching() {
    let fixture = Fixture::new(0);
    for (selector, encoded, expected_id) in
        [("middle child", "middle%20child", B), ("elder", "elder", A)]
    {
        let result = parity(
            &fixture,
            Operation::GenerationalFamily {
                character: selector.into(),
                at: at(10, i32::MAX, "main"),
                limit: 100,
            },
            &format!("/api/generational/family?character={encoded}&tick=10"),
            &["generational", "family", selector, "--tick", "10"],
        )
        .await;
        assert_eq!(result["characterId"], expected_id);
    }
    for (selector, status) in [
        (A.to_uppercase(), StatusCode::NOT_FOUND),
        (AB.into(), StatusCode::BAD_REQUEST),
    ] {
        let service = fixture.service();
        let expected = service
            .execute(Operation::GenerationalFamily {
                character: selector.clone(),
                at: at(10, i32::MAX, "main"),
                limit: 100,
            })
            .unwrap_err()
            .json();
        let (actual_status, actual) = get(
            service,
            &format!("/api/generational/family?character={selector}&tick=10"),
        )
        .await;
        let (code, command) = cli(
            &fixture.root,
            &["generational", "family", &selector, "--tick", "10"],
        );
        assert_eq!(actual_status, status);
        assert_eq!(code, 2);
        assert_eq!(actual, expected);
        assert_eq!(command, expected);
    }
    let world = source::load_world(&fixture.root, None).unwrap();
    let record = &world.records[D];
    let mut metadata = record.frontmatter.clone();
    metadata["aliases"] = serde_yaml::to_value(["Elder"]).unwrap();
    fs::write(
        fixture.root.join(&record.source_path),
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&metadata).unwrap(),
            record.body
        ),
    )
    .unwrap();
    let service = fixture.service();
    let expected = service
        .execute(Operation::GenerationalFamily {
            character: "Elder".into(),
            at: at(10, i32::MAX, "main"),
            limit: 100,
        })
        .unwrap_err()
        .json();
    let (status, actual) = get(service, "/api/generational/family?character=Elder&tick=10").await;
    let (code, command) = cli(
        &fixture.root,
        &["generational", "family", "Elder", "--tick", "10"],
    );
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(code, 2);
    assert_eq!(expected["code"], "ambiguous_reference");
    assert_eq!(actual, expected);
    assert_eq!(command, expected);
    let result = parity(
        &fixture,
        Operation::GenerationalFamily {
            character: A.into(),
            at: at(10, i32::MAX, "main"),
            limit: 100,
        },
        &format!("/api/generational/family?character={A}&tick=10"),
        &["generational", "family", A, "--tick", "10"],
    )
    .await;
    assert_eq!(result["characterId"], A);
}

#[tokio::test]
async fn complete_limits_and_noncanonical_histories_are_explicit() {
    let fixture = Fixture::new(0);
    for uri in [
        format!("/api/generational/family?character={B}&tick=10&limit=1"),
        format!("/api/generational/ancestors?character={C}&tick=10&max_depth=2&limit=1"),
        format!("/api/generational/history?record={AB}&tick=20&limit=2"),
        format!("/api/generational/ancestors?character={C}&tick=10&max_depth=1"),
        format!("/api/generational/descendants?character={A}&tick=10&max_depth=0"),
        format!("/api/generational/path?from={C}&to={A}&tick=10&max_hops=1"),
    ] {
        let (status, value) = get(fixture.service(), &uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{value}");
        assert!(value.get("result").is_none());
    }
    let world = source::load_world(&fixture.root, None).unwrap();
    let record = &world.records[AB];
    let mut metadata = record.frontmatter.clone();
    metadata["status"] = serde_yaml::Value::String("draft".into());
    fs::write(
        fixture.root.join(&record.source_path),
        format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&metadata).unwrap(),
            record.body
        ),
    )
    .unwrap();
    let result = parity(
        &fixture,
        Operation::GenerationalHistory {
            record: AB.into(),
            at: at(10, i32::MAX, "main"),
            limit: 100,
        },
        &format!("/api/generational/history?record={AB}&tick=10"),
        &["generational", "history", AB, "--tick", "10"],
    )
    .await;
    assert_eq!(result["available"], false);
    assert_eq!(result["reason"], "noncanonical");
    assert_eq!(result["history"], json!([]));
    let family = fixture
        .service()
        .execute(Operation::GenerationalFamily {
            character: B.into(),
            at: at(10, i32::MAX, "main"),
            limit: 100,
        })
        .unwrap();
    assert_eq!(family["result"]["parents"], json!([]));
}

#[tokio::test]
async fn closed_routes_refuse_missing_horizons_invalid_limits_and_viewpoint_parameters() {
    let fixture = Fixture::new(0);
    for uri in [
        format!("/api/generational/family?character={B}"),
        "/api/generational/family?tick=0".into(),
        format!("/api/generational/family?character={B}&tick=0&tick=1"),
        format!("/api/generational/family?character={B}&tick=0&view=character"),
        format!("/api/generational/family?character={B}&tick=0&perspective=author"),
        format!("/api/generational/family?character={B}&tick=0&limit=0"),
        format!("/api/generational/family?character={B}&tick=0&limit=1001"),
        format!("/api/generational/family?character={B}&tick=9223372036854775808"),
        format!("/api/generational/family?character={B}&tick=0&order=2147483648"),
        format!("/api/generational/family?character={B}&tick=0&timeline=missing"),
        format!("/api/generational/ancestors?character={C}&tick=0&max_depth=33"),
        format!("/api/generational/descendants?character={A}&tick=0&max_depth=-1"),
        format!("/api/generational/path?from={A}&tick=0"),
        format!("/api/generational/path?from={A}&to={C}&tick=0&max_hops=33"),
        format!("/api/generational/path?from={A}&to={C}&tick=0&character={B}"),
        format!("/api/generational/history?record={AB}&tick=0&max_depth=1"),
    ] {
        let (status, value) = get(fixture.service(), &uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {value}");
        assert!(value.get("result").is_none());
    }
    for args in [
        vec!["generational", "family", B],
        vec![
            "generational",
            "path",
            A,
            C,
            "--tick",
            "0",
            "--max-hops",
            "33",
        ],
        vec![
            "generational",
            "ancestors",
            C,
            "--tick",
            "0",
            "--max-depth",
            "33",
        ],
        vec![
            "generational",
            "history",
            AB,
            "--tick",
            "0",
            "--view",
            "character",
        ],
    ] {
        let (code, value) = cli(&fixture.root, &args);
        assert_eq!(code, 2, "{value}");
        assert!(value.get("result").is_none());
    }
}
