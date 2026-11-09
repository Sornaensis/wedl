//! Whole transport payloads must depend only on admitted character evidence.
use std::{path::Path, process::Command};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;
use wedl::{
    http,
    model::{Record, World},
    service::Service,
    source, validation,
};

const MINA: &str = "char_4JVNDN6QW0BAFFV379AJJSMEPZ";
const WORKSHOP_SCENE: &str = "scene_0QXK1AT5DC93TB0YZSB1KAEQVC";
const WORKSHOP_CONVERSATION: &str = "conv_4YFR36RGTKN7VE0R5XQFBKH6QT";
const QUAY_CONVERSATION: &str = "conv_3G4YJGZW7NHJ2NB5PESMV0A6S5";

fn world() -> World {
    source::load_world(
        concat!(env!("CARGO_MANIFEST_DIR"), "/examples/tideglass"),
        None,
    )
    .unwrap()
}

fn rebuild(record: &Record) -> Record {
    let text = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&record.frontmatter).unwrap(),
        record.body
    );
    source::parse_record(text.as_bytes(), &record.source_path).unwrap()
}

fn hidden_changes(mut world: World) -> World {
    for id in [
        MINA,
        WORKSHOP_CONVERSATION,
        "know_33RB1GJYPCGCPDA9KYT8GN4PN9",
    ] {
        let record = world.records.get_mut(id).unwrap();
        record.frontmatter["title"] = serde_yaml::Value::String("Hidden renamed metadata".into());
        record.body.push_str(
            "\n## Hidden future prose\nNever include this source text in character results.\n",
        );
        if record.kind == "knowledge" {
            record.frontmatter["claim"]["truth"] = serde_yaml::Value::String("contested".into());
            record.frontmatter["transitions"].as_sequence_mut().unwrap().push(serde_yaml::from_str(
                "id: future-belief\ntime: {tick: 100, order: 0}\nstate: rejected\nconfidence: 1.0\nacquisition: authored\n"
            ).unwrap());
        }
        *record = rebuild(record);
    }
    // Add a high-frequency matching document belonging to another character.
    // Global FTS statistics would change the visible search scores.
    let mut hidden = world.records["know_7EJZ8JXCFWVX6DEX37AY8N62KY"].clone();
    hidden.frontmatter["id"] = serde_yaml::Value::String("know_00000000000000000000000099".into());
    hidden.frontmatter["title"] = serde_yaml::Value::String("Unadmitted record evidence".into());
    hidden.frontmatter["claim"]["key"] =
        serde_yaml::Value::String("private.unadmitted-record".into());
    hidden.frontmatter["claim"]["statement"] = serde_yaml::Value::String("record ".repeat(200));
    hidden.source_path = "story/knowledge/unadmitted-record.md".into();
    let hidden = rebuild(&hidden);
    world.records.insert(hidden.id.clone(), hidden);
    source::world_from_records(
        world.root,
        world.records.into_values().collect(),
        world.revision,
    )
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
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

fn assert_character_envelope(value: &Value) {
    let keys: Vec<_> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["protocol", "result"]);
    assert_eq!(value["protocol"], wedl::CONTRACT);
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
                            "frontmatterYaml",
                            "rawSource",
                            "title",
                            "truth"
                        ]
                        .contains(&key.as_str()),
                        "source metadata leaked: {key}"
                    );
                    inspect(value);
                }
            }
            Value::Array(items) => {
                for item in items {
                    inspect(item);
                }
            }
            _ => {}
        }
    }
    inspect(value);
}

fn write_world(world: &World, root: &Path) {
    std::fs::create_dir_all(root).unwrap();
    for record in world.records.values() {
        let file = root.join(&record.source_path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, &record.raw_bytes).unwrap();
    }
}

fn cli(root: &Path, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .args(["--compact", "--repo", root.to_str().unwrap()])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[tokio::test]
async fn complete_http_and_cli_character_payloads_ignore_hidden_changes() {
    let before = world();
    let after = hidden_changes(before.clone());
    assert!(validation::validate_world(&before).valid);
    let checked = validation::validate_world(&after);
    assert!(checked.valid, "{:?}", checked.diagnostics);
    assert_ne!(before.fingerprint, after.fingerprint);
    let temporary = tempfile::tempdir().unwrap();
    let before_root = temporary.path().join("before");
    let after_root = temporary.path().join("after");
    write_world(&before, &before_root);
    write_world(&after, &after_root);
    let before = Service::new(before);
    let after = Service::new(after);
    for (uri, arguments) in [
        (
            format!("/api/character/context?character={MINA}&tick=20"),
            vec!["character", "context", MINA, "--tick", "20"],
        ),
        (
            format!("/api/character/search?character={MINA}&q=record&tick=20&limit=1"),
            vec![
                "character",
                "search",
                MINA,
                "record",
                "--tick",
                "20",
                "--limit",
                "1",
            ],
        ),
        (
            format!(
                "/api/character/conversation?character={MINA}&id={WORKSHOP_CONVERSATION}&tick=20"
            ),
            vec![
                "character",
                "conversation",
                MINA,
                WORKSHOP_CONVERSATION,
                "--tick",
                "20",
            ],
        ),
    ] {
        let (status, original) = get(before.clone(), &uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {original}");
        assert_character_envelope(&original);
        let (status, modified) = get(after.clone(), &uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {modified}");
        assert_eq!(original, modified, "hidden source affected {uri}");
        assert_eq!(cli(&before_root, &arguments), original);
        assert_eq!(cli(&after_root, &arguments), original);
    }
    let (_, author) = get(before, &format!("/api/context?character={MINA}&tick=20")).await;
    assert!(author["fingerprint"].is_string());
    assert!(author.get("revision").is_some());
    assert_eq!(author["result"]["perspective"], "author");
}

#[tokio::test]
async fn missing_inaccessible_and_name_selected_conversations_are_indistinguishable() {
    let world = world();
    let service = Service::new(world.clone());
    let mut expected = None;
    for id in [
        QUAY_CONVERSATION,
        "conv_00000000000000000000000099",
        MINA,
        "Workshop%20Repair%20Decision",
    ] {
        let result = get(
            service.clone(),
            &format!("/api/character/conversation?character={MINA}&id={id}&tick=20"),
        )
        .await;
        assert_eq!(result.0, StatusCode::NOT_FOUND);
        assert_eq!(result.1["code"], "not_found");
        assert_eq!(result.1["message"], "character view is unavailable");
        let output = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
            .args([
                "--repo",
                world.root.to_str().unwrap(),
                "--compact",
                "character",
                "conversation",
                MINA,
            ])
            .arg(id.replace("%20", " "))
            .args(["--tick", "20"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stderr).unwrap(),
            result.1
        );
        if let Some(expected) = &expected {
            assert_eq!(&result, expected);
        } else {
            expected = Some(result);
        }
    }
    let (status, error) = get(
        service,
        "/api/character/context?character=Mina%20Vale&tick=20",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(error["message"], "character view is unavailable");
}

#[tokio::test]
async fn character_parameters_are_closed_and_author_routes_do_not_switch_views() {
    let service = Service::new(world());
    for uri in [
        format!("/api/character/context?character={MINA}"),
        format!("/api/character/context?character={MINA}&tick=20&includeText=true"),
        format!("/api/character/context?character={MINA}&character={MINA}&tick=20"),
        format!("/api/character/context?character={MINA}&tick=20&maxCharacters=0"),
        format!("/api/character/search?character={MINA}&tick=20&q=record&kind=knowledge"),
        format!("/api/character/search?character={MINA}&tick=20&q=record&limit=0"),
        format!(
            "/api/character/conversation?character={MINA}&tick=20&id={WORKSHOP_CONVERSATION}&viewpoint={MINA}"
        ),
        format!("/api/context?character={MINA}&tick=20&perspective=character"),
        format!("/api/search?q=record&character={MINA}&tick=20"),
        format!("/api/scene?id={WORKSHOP_SCENE}&tick=20&character={MINA}"),
    ] {
        let (status, value) = get(service.clone(), &uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {value}");
        assert_eq!(value["code"], "usage_error");
    }
}

#[tokio::test]
async fn scene_read_uses_author_snapshot_envelope_and_exact_horizon() {
    let world = world();
    let (status, value) = get(
        Service::new(world.clone()),
        &format!("/api/scene?id={WORKSHOP_SCENE}&tick=20&order=0"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["fingerprint"], world.fingerprint);
    assert_eq!(value["result"]["sceneId"], WORKSHOP_SCENE);
    assert_eq!(value["result"]["effectiveTime"]["tick"], "20");
    assert_eq!(value["result"]["effectiveTime"]["order"], "0");
    assert_eq!(
        value["result"]["presentParticipants"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        cli(
            &world.root,
            &["scene", WORKSHOP_SCENE, "--tick", "20", "--order", "0"]
        ),
        value
    );
}
