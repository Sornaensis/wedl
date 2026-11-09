use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path, process::Command, time::SystemTime};
use tempfile::TempDir;
use tower::ServiceExt;
use wedl::{
    http,
    service::{Operation, Service, TimeInput},
    source,
};

const TICK: i64 = 9_007_199_254_740_993;
const WORLD: &str = "world_00000000000000000000000000";
const A: &str = "char_00000000000000000000000001";
const B: &str = "char_00000000000000000000000002";
const L1: &str = "loc_00000000000000000000000001";
const L2: &str = "loc_00000000000000000000000002";
const E1: &str = "event_00000000000000000000000001";
const E2: &str = "event_00000000000000000000000002";
const E3: &str = "event_00000000000000000000000003";
const DRAFT: &str = "event_00000000000000000000000004";
const SCENE: &str = "scene_00000000000000000000000001";
const CONVERSATION: &str = "conv_00000000000000000000000001";
const PLOT: &str = "sp_00000000000000000000000001";
const SCENE_PLOT: &str = "sp_00000000000000000000000002";
const THREAD: &str = "thread_00000000000000000000000001";
const OTHER_THREAD: &str = "thread_00000000000000000000000002";

fn at() -> TimeInput {
    TimeInput {
        tick: TICK,
        order: 4,
        timeline: Some("main".into()),
    }
}
fn point(order: i32) -> String {
    format!("{{timeline: main, tick: {TICK}, order: {order}}}")
}
fn record(root: &Path, kind: &str, id: &str, fields: &str) {
    let text = format!("---\nschema: wedl/v1\nkind: {kind}\nid: {id}\ntitle: {id}\n{fields}---\n")
        .replace('\n', "\r\n")
        + "Exact navigation prose 🌲  \r\nNo final newline";
    fs::write(root.join("story").join(format!("{id}.md")), text).unwrap();
}
fn fixture() -> TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("story")).unwrap();
    record(
        root.path(),
        "world",
        WORLD,
        &format!(
            "default_timeline: main\ntimelines: [{{id: main, label: Main, origin: {{tick: -9223372036854775808, label: Beginning}}}}, {{id: other, label: Other}}]\ncapabilities: []\ncurrent_time: {}\nthreads: [{{id: {THREAD}, label: Harbor}}, {{id: {OTHER_THREAD}, label: Other}}]\nstate_keys:\n  character:\n    location: {{type: entity, kind: location}}\nrelationship_metrics: {{}}\n",
            point(4)
        ),
    );
    record(root.path(), "location", L1, "");
    record(root.path(), "location", L2, "");
    for id in [A, B] {
        record(
            root.path(),
            "character",
            id,
            &format!(
                "initial_state: {{location: {{entity: {L1}}}}}\nthreads: [{THREAD}, {OTHER_THREAD}]\n"
            ),
        );
    }
    record(
        root.path(),
        "scene",
        SCENE,
        &format!(
            "status: active\nlocation: {L1}\ntime: {{start: {}, current: {}}}\nparticipants: [{{character: {A}, role: speaker}}, {{character: {B}, role: listener}}]\nobjects: []\nconversations: [{CONVERSATION}]\nobservations: []\nenvironments: []\noutcome_events: []\n",
            point(0),
            point(4)
        ),
    );
    record(
        root.path(),
        "conversation",
        CONVERSATION,
        &format!(
            "status: closed\nscene: {SCENE}\nlocation: {L1}\ntime: {{start: {}, end: {}}}\nparticipants: [{{character: {A}}}, {{character: {B}}}]\nturns: [{{id: greeting, at: {}, speaker: {A}, text: Hello., audience: [participants]}}]\nrecollections: []\n",
            point(1),
            point(3),
            point(1)
        ),
    );
    for (id, order, status, location, causes) in [
        (E1, 1, "canonical", L1, ""),
        (E2, 3, "canonical", L2, E1),
        (E3, 6, "canonical", L2, E2),
        (DRAFT, 0, "draft", L1, ""),
    ] {
        record(
            root.path(),
            "event",
            id,
            &format!(
                "status: {status}\ntime: {}\nlocation: {location}\nparticipants: [{{character: {A}, role: speaker}}, {{character: {B}, role: listener}}]\ncauses: [{causes}]\nrelated_story_points: []\neffects: []\nthreads: [{THREAD}]\n",
                point(order)
            ),
        );
    }
    record(
        root.path(),
        "story-point",
        PLOT,
        &format!(
            "activation_policy: suggest\nrequires: []\nwhen: {{all: []}}\nlifecycle: {{initial_state: dormant, transitions: [{{id: activation, state: active, time: {}, causing_event: {E1}}}]}}\noutcome_events: [{E2}, {E3}]\n",
            point(2)
        ),
    );
    record(
        root.path(),
        "story-point",
        SCENE_PLOT,
        &format!(
            "activation_policy: suggest\nrequires: []\nwhen: {{scene: {{participant: {A}, location: {L1}}}}}\nlifecycle: {{initial_state: dormant, transitions: []}}\noutcome_events: []\n"
        ),
    );
    root
}
fn encode(text: &str) -> String {
    text.bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}
fn dated(path: &str) -> String {
    format!(
        "{path}{}tick={TICK}&order=4&timeline=main",
        if path.contains('?') { '&' } else { '?' }
    )
}
fn cli(root: &Path, args: &[String]) -> Value {
    let result = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .args(["--compact", "--repo", root.to_str().unwrap()])
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
fn dated_cli(args: &[&str]) -> Vec<String> {
    args.iter()
        .map(|arg| (*arg).into())
        .chain([
            "--tick".into(),
            TICK.to_string(),
            "--order".into(),
            "4".into(),
            "--timeline".into(),
            "main".into(),
        ])
        .collect()
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
fn image(root: &Path) -> BTreeMap<String, (Vec<u8>, SystemTime)> {
    fn visit(root: &Path, dir: &Path, out: &mut BTreeMap<String, (Vec<u8>, SystemTime)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                visit(root, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(root).unwrap().to_str().unwrap().into(),
                    (
                        fs::read(&path).unwrap(),
                        fs::metadata(&path).unwrap().modified().unwrap(),
                    ),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}

#[tokio::test]
async fn all_navigation_adapters_agree_and_preserve_source_bytes_and_mtimes() {
    let root = fixture();
    let before = image(root.path());
    let world = source::load_world(root.path(), None).unwrap();
    let service = Service::new(world);
    let validation = service.execute(Operation::Validate).unwrap();
    assert_eq!(validation["result"]["valid"], true, "{validation}");
    let cases = vec![
        (
            Operation::Timeline {
                timeline: Some("main".into()),
                at: Some(at()),
                limit: 1000,
            },
            dated("/api/timeline"),
            dated_cli(&["timeline"]),
        ),
        (
            Operation::Whereabouts {
                at: at(),
                characters: Some(vec![A.into()]),
                limit: 1000,
            },
            dated(&format!(
                "/api/whereabouts?characters={}",
                encode(&json!([A]).to_string())
            )),
            dated_cli(&["whereabouts", "--character", A]),
        ),
        (
            Operation::Causal {
                event: E1.into(),
                at: at(),
                direction: "downstream".into(),
                limit: 1000,
            },
            dated(&format!("/api/causal?event={E1}&direction=downstream")),
            dated_cli(&["causal", E1, "--direction", "downstream"]),
        ),
        (
            Operation::StoryPoints {
                at: at(),
                scene: Some(SCENE.into()),
                limit: 1000,
            },
            dated(&format!("/api/story-points?scene={SCENE}")),
            dated_cli(&["story-points", "--scene", SCENE]),
        ),
        (
            Operation::StoryPointTrail {
                story_point: PLOT.into(),
                at: at(),
                scene: None,
                limit: 1000,
            },
            dated(&format!("/api/story-point?id={PLOT}")),
            dated_cli(&["story-point", PLOT]),
        ),
        (
            Operation::Interactions {
                first: A.into(),
                second: B.into(),
                at: at(),
                limit: 1000,
            },
            dated(&format!("/api/interactions?first={A}&second={B}")),
            dated_cli(&["interactions", A, B]),
        ),
        (
            Operation::ThreadMemberships {
                record_ids: vec![A.into(), E1.into(), E3.into()],
                thread_ids: vec![THREAD.into()],
                at: Some(at()),
                limit: 1000,
            },
            dated(&format!(
                "/api/thread-memberships?records={}&threads={}",
                encode(&json!([E3, A, E1]).to_string()),
                encode(&json!([THREAD]).to_string())
            )),
            dated_cli(&[
                "thread-memberships",
                "--record",
                E3,
                "--record",
                A,
                "--record",
                E1,
                "--thread",
                THREAD,
            ]),
        ),
    ];
    for (operation, uri, args) in cases {
        let expected = service.execute(operation).unwrap();
        let (status, response) = get(service.clone(), &uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {response}");
        assert_eq!(response, expected, "{uri}");
        assert_eq!(cli(root.path(), &args), expected, "{args:?}");
        assert_eq!(
            response["result"]["effectiveTime"]["tick"],
            TICK.to_string()
        );
        assert!(
            !response["result"].to_string().contains(E3),
            "future event in {uri}"
        );
    }
    assert_eq!(image(root.path()), before);
    assert_eq!(
        fs::read_dir(root.path()).unwrap().count(),
        1,
        "read must not create a private store or cache"
    );
}

#[tokio::test]
async fn authored_timeline_and_membership_all_time_are_explicit() {
    let root = fixture();
    let service = Service::new(source::load_world(root.path(), None).unwrap());
    let (status, timeline) = get(service.clone(), "/api/timeline?timeline=main").await;
    assert_eq!(status, StatusCode::OK, "{timeline}");
    assert_eq!(
        timeline,
        cli(
            root.path(),
            &["timeline".into(), "--timeline".into(), "main".into()]
        )
    );
    assert_eq!(timeline["result"]["timeScope"], "all-time");
    let points = timeline["result"]["points"].as_array().unwrap();
    assert!(
        points
            .iter()
            .any(|p| p["entity"]["id"] == DRAFT && p["status"] == "draft")
    );
    assert!(points.iter().any(|p| p["entity"]["id"] == E3));
    let path = format!(
        "/api/thread-memberships?records={}&threads={}",
        encode(&json!([E3]).to_string()),
        encode(&json!([THREAD]).to_string())
    );
    let (status, memberships) = get(service, &path).await;
    assert_eq!(status, StatusCode::OK, "{memberships}");
    assert_eq!(
        memberships,
        cli(
            root.path(),
            &[
                "thread-memberships".into(),
                "--record".into(),
                E3.into(),
                "--thread".into(),
                THREAD.into()
            ]
        )
    );
    assert!(memberships["result"].to_string().contains(E3));
    assert!(!memberships["result"].to_string().contains(OTHER_THREAD));
}

#[tokio::test]
async fn navigation_query_validation_and_bounds_are_closed() {
    let root = fixture();
    let service = Service::new(source::load_world(root.path(), None).unwrap());
    let records = encode(&json!([A]).to_string());
    let threads = encode(&json!([THREAD]).to_string());
    for uri in [
        "/api/whereabouts".into(),
        dated("/api/whereabouts?characters=[]"),
        dated(&format!(
            "/api/whereabouts?characters={}",
            encode(&json!([A, A]).to_string())
        )),
        dated("/api/whereabouts?characters=%7B%7D"),
        dated(&format!("/api/causal?event={E1}&direction=sideways")),
        dated(&format!(
            "/api/causal?event={E1}&direction=both&direction=upstream"
        )),
        dated(&format!("/api/interactions?first={A}&second={A}")),
        dated(&format!("/api/interactions?first={A}")),
        dated(&format!("/api/story-point?id={PLOT}&limit=0")),
        dated("/api/story-points?limit=1001"),
        dated("/api/timeline?limit=1"),
        dated("/api/whereabouts?limit=1"),
        format!("/api/thread-memberships?records={records}&threads={threads}&timeline=main"),
        format!("/api/thread-memberships?records={records}&threads={threads}&extra=true"),
    ] {
        let (status, body) = get(service.clone(), &uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
    }
}

#[tokio::test]
async fn scene_selection_changes_conditions_without_filtering_candidates() {
    let root = fixture();
    let service = Service::new(source::load_world(root.path(), None).unwrap());
    let (_, without) = get(service.clone(), &dated("/api/story-points")).await;
    let (_, with) = get(service, &dated(&format!("/api/story-points?scene={SCENE}"))).await;
    let a = without["result"]["items"].as_array().unwrap();
    let b = with["result"]["items"].as_array().unwrap();
    assert_eq!(
        a.iter().map(|p| &p["storyPointId"]).collect::<Vec<_>>(),
        b.iter().map(|p| &p["storyPointId"]).collect::<Vec<_>>()
    );
    assert_eq!(
        a.iter().find(|p| p["storyPointId"] == SCENE_PLOT).unwrap()["eligible"],
        false
    );
    assert_eq!(
        b.iter().find(|p| p["storyPointId"] == SCENE_PLOT).unwrap()["eligible"],
        true
    );
}
