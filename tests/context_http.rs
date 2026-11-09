//! Brief and captured writing-frame transports preserve exact selections and admission.
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeMap, fs, path::Path, process::Command};
use tower::ServiceExt;
use wedl::{
    briefs, http,
    model::{StoryTime, World},
    service::{Operation, Service, TimeInput},
    source, validation,
    writing_frames::{self, Frame, Lens},
};

const TICK: i64 = 9_007_199_254_740_993;
const VIEWER: &str = "char_00000000000000000000000001";
const OTHER: &str = "char_00000000000000000000000002";
const PRIVATE: &str = "char_00000000000000000000000003";
const PLACE: &str = "loc_00000000000000000000000001";
const HIDDEN_PLACE: &str = "loc_00000000000000000000000002";
const OBJECT: &str = "obj_00000000000000000000000001";
const SCENE: &str = "scene_00000000000000000000000001";
const HIDDEN_SCENE: &str = "scene_00000000000000000000000002";
const BRIEF: &str = "brief_00000000000000000000000001";
const BLOCKED_BRIEF: &str = "brief_00000000000000000000000002";
const UNKNOWN_BRIEF: &str = "brief_00000000000000000000000003";
const SECTION: &str = "description_00000000000000000000000001";

fn id(prefix: &str, n: usize) -> String {
    format!("{prefix}_{n:026}")
}
fn point(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn time(tick: i64, order: i32) -> TimeInput {
    TimeInput {
        tick,
        order,
        timeline: Some("main".into()),
    }
}
fn source_time(tick: i64, order: i32) -> Json {
    json!({"timeline":"main","tick":tick,"order":order})
}
fn reference(id: &str) -> Json {
    json!({"kind":"record","id":id})
}
fn section(n: usize, hidden: bool) -> Json {
    json!({"id":id("description",n),"heading":if hidden {"Private heading"} else {"Admitted bell account"},
        "markdown":if hidden {"PRIVATE_SECTION_TEXT"} else {"The public harbor bell rings.\n"},
        "valid":{"kind":"interval","timeline":"main","start":source_time(TICK,0),"end":source_time(TICK+1,0)},
        "audience":if hidden {vec!["author"]} else {vec!["author","public"]},
        "perspectives":if hidden {vec!["author"]} else {vec!["author","ordinary"]}})
}

struct Fixture(tempfile::TempDir);
impl Fixture {
    fn new() -> Self {
        let f = Self(tempfile::tempdir().unwrap());
        fs::create_dir(f.root().join("story")).unwrap();
        f.write("world",&id("world",0),"world",json!({"capabilities":["descriptions-v1","briefs-v1"],
            "default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],
            "current_time":source_time(TICK,0)}),"World source body");
        for (name, identifier) in [("viewer", VIEWER), ("other", OTHER), ("private", PRIVATE)] {
            f.write(
                name,
                identifier,
                "character",
                json!({"initial_state":{}}),
                "PRIVATE_CHARACTER_BODY",
            );
        }
        f.write(
            "place",
            PLACE,
            "location",
            json!({"descriptions":[section(1,false),section(2,true)]}),
            "PRIVATE_PLACE_BODY",
        );
        f.write(
            "hidden-place",
            HIDDEN_PLACE,
            "location",
            json!({"audience":["author"],"perspectives":["author"]}),
            "PRIVATE_OTHER_PLACE",
        );
        f.write(
            "object",
            OBJECT,
            "object",
            json!({"descriptions":[section(3,false)]}),
            "PRIVATE_OBJECT_BODY",
        );
        f.write("scene",SCENE,"scene",json!({"status":"closed","location":PLACE,
            "time":{"start":source_time(TICK-1,0),"current":source_time(TICK+2,0),"end":source_time(TICK+2,0)},
            "participants":[{"character":VIEWER,"point_of_view":true},{"character":OTHER}],"objects":[OBJECT],
            "observations":[{"id":"bell","at":source_time(TICK,0),"text":"An audible harbor bell.","audience":["participants"]}]}),"PRIVATE_SCENE_BODY");
        f.write("hidden-scene",HIDDEN_SCENE,"scene",json!({"status":"closed","location":HIDDEN_PLACE,
            "time":{"start":source_time(TICK-1,0),"current":source_time(TICK+2,0),"end":source_time(TICK+2,0)},"participants":[{"character":PRIVATE}],
            "audience":["author"],"perspectives":["author"]}),"PRIVATE_SCENE_TWO");
        f.brief(
            "brief",
            BRIEF,
            json!([{"kind":"section","owner_id":PLACE,"id":SECTION}]),
            "Exact public brief body.\r\nA remembered harbor sign.\n",
        );
        f.brief(
            "blocked-brief",
            BLOCKED_BRIEF,
            json!([reference(PLACE), reference(HIDDEN_PLACE)]),
            "UNAVAILABLE_BRIEF_BODY",
        );
        f.brief(
            "unknown-brief",
            UNKNOWN_BRIEF,
            json!([]),
            "UNACQUIRED_BRIEF_BODY",
        );
        for (n, entity) in [(1, BRIEF), (2, PLACE), (3, BLOCKED_BRIEF)] {
            f.write(&format!("knowledge-{n}"),&id("know",n),"knowledge",json!({"knower":VIEWER,
                "claim":{"key":format!("explicit.{n}"),"statement":format!("I remember the admitted account {n}."),"object":{"entity":entity}},
                "transitions":[{"id":"learned","time":source_time(TICK-1,0),"state":"accepted","confidence":1.0,"acquisition":"told","note":"PRIVATE_ACQUISITION_NOTE"}]}),"PRIVATE_KNOWLEDGE_BODY");
        }
        f.write("private-knowledge",&id("know",4),"knowledge",json!({"knower":PRIVATE,
            "claim":{"key":"private.words","statement":"PRIVATE_BELIEF_WORDS"},
            "transitions":[{"id":"private-learned","time":source_time(TICK-1,0),"state":"accepted","confidence":1.0,"acquisition":"told"}]}),"PRIVATE_KNOWLEDGE_OTHER");
        f.valid();
        f
    }
    fn root(&self) -> &Path {
        self.0.path()
    }
    fn write(&self, name: &str, identifier: &str, kind: &str, fields: Json, body: &str) {
        let mut value = json!({"schema":"wedl/v1","id":identifier,"kind":kind,"title":format!("Source title {name}"),"status":"canonical","aliases":[],"tags":[]});
        value
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        fs::write(
            self.root().join(format!("story/{name}.md")),
            format!("---\n{}---\n{body}", serde_yaml::to_string(&value).unwrap()),
        )
        .unwrap();
    }
    fn brief(&self, name: &str, identifier: &str, refs: Json, body: &str) {
        self.write(name,identifier,"context-brief",json!({"domain":"context.harbor","scope":"world","valid":{"kind":"timeless"},
            "audience":["author","public"],"perspectives":["author","ordinary"],"required_refs":refs}),body);
    }
    fn world(&self) -> World {
        source::load_world(self.root(), None).unwrap()
    }
    fn valid(&self) {
        let report = validation::validate_world(&self.world());
        assert!(report.valid, "{:#?}", report.diagnostics);
    }
    fn frame(&self, lens: Lens) -> Frame {
        Frame {
            fingerprint: self.world().fingerprint,
            work: None,
            scene_id: Some(SCENE.into()),
            pov_id: Some(VIEWER.into()),
            at: point(TICK, 0),
            lens,
        }
    }
    fn edit(&self, name: &str, edit: impl FnOnce(&mut Yaml, &mut String)) {
        let path = self.root().join(format!("story/{name}.md"));
        let record =
            source::parse_record(&fs::read(&path).unwrap(), &format!("story/{name}.md")).unwrap();
        let mut value = record.frontmatter;
        let mut body = record.body;
        edit(&mut value, &mut body);
        fs::write(
            path,
            format!("---\n{}---\n{body}", serde_yaml::to_string(&value).unwrap()),
        )
        .unwrap();
    }
    fn bytes(&self) -> BTreeMap<String, Vec<u8>> {
        fs::read_dir(self.root().join("story"))
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                (
                    path.file_name().unwrap().to_string_lossy().into(),
                    fs::read(path).unwrap(),
                )
            })
            .collect()
    }
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}
fn writing_uri(operation: &str, frame: &Frame, extra: &str) -> String {
    format!(
        "/api/writing/{operation}?frame={}{}",
        encode(&serde_json::to_string(frame).unwrap()),
        extra
    )
}
async fn get(service: Service, uri: &str) -> (StatusCode, Json) {
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
fn cli(f: &Fixture, args: &[&str]) -> (bool, Json) {
    let output = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .arg("--repo")
        .arg(f.root())
        .args(args)
        .output()
        .unwrap();
    let bytes = if output.status.success() {
        &output.stdout
    } else {
        &output.stderr
    };
    (
        output.status.success(),
        serde_json::from_slice(bytes)
            .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(bytes))),
    )
}
fn frame_file(f: &Fixture, frame: &Frame) -> String {
    let path = f.root().join("frame.json");
    fs::write(&path, serde_json::to_string(frame).unwrap()).unwrap();
    path.to_str().unwrap().into()
}
fn safe_character(value: &Json) {
    if let Some(fields) = value.as_object() {
        for (key, value) in fields {
            assert!(
                ![
                    "fingerprint",
                    "revision",
                    "blobOid",
                    "sourcePath",
                    "sourceField",
                    "rawSource",
                    "frontmatterYaml",
                    "provenance",
                    "note"
                ]
                .contains(&key.as_str()),
                "leaked key {key}"
            );
            safe_character(value);
        }
    }
    if let Some(values) = value.as_array() {
        for value in values {
            safe_character(value);
        }
    }
}
fn author_envelope(value: Json, world: &World) -> Json {
    json!({"protocol":wedl::CONTRACT,"fingerprint":world.fingerprint,"revision":world.revision,"result":value})
}

#[tokio::test]
async fn brief_and_three_writing_operations_agree_across_native_service_cli_http_without_source_writes()
 {
    let f = Fixture::new();
    let before = f.bytes();
    let world = f.world();
    let service = Service::new(world.clone());
    let at = point(TICK, 0);
    let tick = TICK.to_string();
    let expected = author_envelope(
        briefs::compose(&world, BRIEF, Some(&at), "author", "author", 100).unwrap(),
        &world,
    );
    assert_eq!(expected["result"]["count"], 2);
    assert_eq!(
        service
            .execute(Operation::BriefCompose {
                brief: BRIEF.into(),
                at: Some(time(TICK, 0)),
                audience: "author".into(),
                view: "author".into(),
                limit: 100
            })
            .unwrap(),
        expected
    );
    assert_eq!(
        cli(
            &f,
            &["briefs", "compose", BRIEF, "--tick", &tick, "--order", "0"]
        )
        .1,
        expected
    );
    assert_eq!(
        get(
            service.clone(),
            &format!("/api/briefs/compose?brief={BRIEF}&tick={TICK}&order=0")
        )
        .await,
        (StatusCode::OK, expected)
    );
    let expected = json!({"protocol":wedl::CONTRACT,"result":briefs::character(&world,VIEWER,BRIEF,&at,100).unwrap()});
    assert_eq!(expected["result"]["count"], 2);
    safe_character(&expected);
    assert_eq!(
        service
            .execute(Operation::CharacterBrief {
                character: VIEWER.into(),
                brief: BRIEF.into(),
                at: time(TICK, 0),
                limit: 100
            })
            .unwrap(),
        expected
    );
    assert_eq!(
        cli(
            &f,
            &[
                "character",
                "brief",
                VIEWER,
                BRIEF,
                "--tick",
                &tick,
                "--order",
                "0"
            ]
        )
        .1,
        expected
    );
    assert_eq!(
        get(
            service.clone(),
            &format!("/api/character/brief?character={VIEWER}&brief={BRIEF}&tick={TICK}&order=0")
        )
        .await,
        (StatusCode::OK, expected)
    );
    for lens in [Lens::Author, Lens::Character, Lens::DramaticIrony] {
        let frame = f.frame(lens);
        let file = frame_file(&f, &frame);
        let selected = vec![BRIEF.to_owned()];
        let reads = [
            (
                "resolve",
                Operation::WritingResolve {
                    frame: frame.clone(),
                },
                writing_frames::resolve(&world, None, &frame).unwrap(),
                vec!["writing", "resolve", "--file", &file],
                String::new(),
            ),
            (
                "choices",
                Operation::WritingChoices {
                    frame: frame.clone(),
                    limit: 100,
                },
                writing_frames::choices(&world, None, &frame, 100).unwrap(),
                vec!["writing", "choices", "--file", &file],
                String::new(),
            ),
            (
                "context",
                Operation::WritingContext {
                    frame: frame.clone(),
                    briefs: selected.clone(),
                    max_characters: 100000,
                },
                writing_frames::context(&world, None, &frame, &selected, 100000).unwrap(),
                vec![
                    "writing",
                    "context",
                    "--file",
                    &file,
                    "--brief",
                    BRIEF,
                    "--max-characters",
                    "100000",
                ],
                format!(
                    "&briefs={}&maxCharacters=100000",
                    encode(&json!([BRIEF]).to_string())
                ),
            ),
        ];
        for (route, operation, native, args, extra) in reads {
            let expected = author_envelope(native, &world);
            assert_eq!(service.execute(operation).unwrap(), expected, "{route}");
            assert_eq!(cli(&f, &args), (true, expected.clone()));
            assert_eq!(
                get(service.clone(), &writing_uri(route, &frame, &extra)).await,
                (StatusCode::OK, expected.clone())
            );
            let selected = if route == "resolve" {
                &expected["result"]
            } else {
                &expected["result"]["frame"]
            };
            assert_eq!(selected["at"]["tick"], TICK.to_string());
            assert_eq!(selected["at"]["order"], "0");
            if route == "context" {
                assert_eq!(
                    expected["result"].get("characterPrompt").is_some(),
                    lens != Lens::Author
                );
                assert_eq!(
                    expected["result"].get("authorMargin").is_some(),
                    lens != Lens::Character
                );
                if let Some(character) = expected["result"].get("characterPrompt") {
                    safe_character(character);
                    assert!(!character.to_string().contains("PRIVATE_"));
                }
            }
        }
    }
    assert_eq!(before, f.bytes());
}

#[tokio::test]
async fn exact_brief_horizons_and_unknown_required_pins_precede_limits_without_becoming_grants() {
    let f = Fixture::new();
    let service = Service::new(f.world());
    for (tick, order, count, outcome) in [
        (TICK - 1, 0, 0, "unknown"),
        (TICK, 0, 2, "available"),
        (TICK + 1, 0, 2, "available"),
        (TICK + 1, 1, 0, "unknown"),
    ] {
        let (status, value) = get(
            service.clone(),
            &format!(
                "/api/character/brief?character={VIEWER}&brief={BRIEF}&tick={tick}&order={order}"
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["result"]["count"], count);
        assert_eq!(value["result"]["outcome"], outcome);
        assert_eq!(value["result"]["effectiveTime"]["tick"], tick.to_string());
    }
    for brief in [
        BLOCKED_BRIEF,
        UNKNOWN_BRIEF,
        "brief_00000000000000000000000099",
    ] {
        for limit in [1, 100] {
            let (status,value)=get(service.clone(),&format!("/api/character/brief?character={VIEWER}&brief={brief}&tick={TICK}&order=0&limit={limit}")).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(value["result"]["outcome"], "unknown");
            assert_eq!(value["result"]["items"], json!([]));
            safe_character(&value);
            assert!(
                !value.to_string().contains("PRIVATE") && !value.to_string().contains(HIDDEN_PLACE)
            );
        }
    }
    let (status, value) = get(
        service.clone(),
        &format!(
            "/api/character/brief?character={VIEWER}&brief={BRIEF}&tick={TICK}&order=0&limit=1"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(value.get("result").is_none());
    let (status, value) = get(service, &format!("/api/briefs/compose?brief={BRIEF}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["result"]["at"], Json::Null);
}

#[tokio::test]
async fn frame_decoder_and_query_shapes_are_closed_before_selection_resolution() {
    let f = Fixture::new();
    let service = Service::new(f.world());
    let frame = f.frame(Lens::Character);
    let base = serde_json::to_value(&frame).unwrap();
    let encoded = serde_json::to_string(&frame).unwrap();
    let mut cases = vec![
        encoded.replacen("\"lens\":", "\"lens\":\"author\",\"lens\":", 1),
        encoded.replacen("\"tick\":", "\"tick\":\"0\",\"tick\":", 1),
        format!("{encoded} null"),
    ];
    for (field, value) in [
        ("workId", json!("work_one")),
        ("knownIds", json!([BRIEF])),
        ("sceneId", json!(4)),
        ("povId", Json::Null),
        ("lens", json!("omniscient")),
        ("fingerprint", json!("A".repeat(64))),
    ] {
        let mut input = base.clone();
        input[field] = value;
        cases.push(input.to_string());
    }
    for (field, value) in [
        ("tick", json!(TICK)),
        ("tick", json!("01")),
        ("tick", json!("-0")),
        ("tick", json!("+1")),
        ("tick", json!("1e3")),
        ("tick", json!("9223372036854775808")),
        ("order", json!("2147483648")),
        ("order", Json::Null),
        ("extra", json!("hidden")),
    ] {
        let mut input = base.clone();
        input["at"][field] = value;
        cases.push(input.to_string());
    }
    for raw in cases {
        assert!(writing_frames::decode(&raw).is_err());
        for route in ["resolve", "choices", "context"] {
            let (status, value) = get(
                service.clone(),
                &format!("/api/writing/{route}?frame={}", encode(&raw)),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{raw}");
            assert!(value.get("result").is_none() && !value.to_string().contains("Source title"));
        }
    }
    for route in ["resolve", "choices", "context"] {
        for extra in [
            "&frame={}",
            "&sceneId=scene_1",
            "&tick=0",
            "&lens=author",
            "&viewer=someone",
            "&knownIds=[]",
            "&audience=author",
            "&workId=work_1",
        ] {
            let (status, _) = get(service.clone(), &writing_uri(route, &frame, extra)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{route}{extra}");
        }
        assert_eq!(
            get(service.clone(), &format!("/api/writing/{route}"))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    for extra in ["&limit=0", "&limit=1001"] {
        assert_eq!(
            get(service.clone(), &writing_uri("choices", &frame, extra))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    for extra in [
        "&briefs={}",
        "&briefs=[1]",
        "&maxCharacters=255",
        "&maxCharacters=100001",
        "&maxCharacters=1e3",
    ] {
        assert_eq!(
            get(service.clone(), &writing_uri("context", &frame, extra))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    for uri in [
        format!("/api/briefs/compose?brief={BRIEF}&order=0"),
        format!("/api/character/brief?brief={BRIEF}&tick={TICK}"),
        format!("/api/character/brief?character={VIEWER}&brief={BRIEF}"),
        format!(
            "/api/character/brief?character={VIEWER}&brief={BRIEF}&tick={TICK}&audience=author"
        ),
        format!("/api/character/brief?character={VIEWER}&brief={BRIEF}&tick={TICK}&scene={SCENE}"),
    ] {
        assert_eq!(
            get(service.clone(), &uri).await.0,
            StatusCode::BAD_REQUEST,
            "{uri}"
        );
    }
    let file = frame_file(&f, &frame);
    let output = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .arg("--repo")
        .arg(f.root())
        .args(["writing", "resolve", "--file", &file, "--tick", "0"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let output = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
        .arg("--repo")
        .arg(f.root())
        .args(["character", "brief", VIEWER, BRIEF])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let mut invalid = base;
    invalid["at"]["tick"] = json!("01");
    fs::write(&file, invalid.to_string()).unwrap();
    let (_, value) = cli(&f, &["writing", "resolve", "--file", &file]);
    let (_, http_value) = get(
        service,
        &format!(
            "/api/writing/resolve?frame={}",
            encode(&invalid.to_string())
        ),
    )
    .await;
    assert_eq!(value, http_value);
}

#[tokio::test]
async fn stale_unavailable_or_wrong_kind_frames_fail_uniformly_without_new_snapshot_pins() {
    let f = Fixture::new();
    let service = Service::new(f.world());
    let base = f.frame(Lens::Character);
    let mut invalid = Vec::new();
    let mut frame = base.clone();
    frame.fingerprint = "f".repeat(64);
    invalid.push(frame);
    for scene in [
        HIDDEN_SCENE,
        PLACE,
        "scene_00000000000000000000000099",
        "Source title scene",
    ] {
        let mut frame = base.clone();
        frame.scene_id = Some(scene.into());
        invalid.push(frame);
    }
    for pov in [
        PLACE,
        "char_00000000000000000000000099",
        "Source title viewer",
    ] {
        let mut frame = base.clone();
        frame.pov_id = Some(pov.into());
        invalid.push(frame);
    }
    let mut frame = base.clone();
    frame.at = point(TICK + 3, 0);
    invalid.push(frame);
    let mut frame = base.clone();
    frame.at = StoryTime::new("other", TICK, 0).unwrap();
    invalid.push(frame);
    let mut reference = None;
    for frame in invalid {
        for route in ["resolve", "choices", "context"] {
            let (status, value) = get(service.clone(), &writing_uri(route, &frame, "")).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{route} {frame:?}");
            if let Some(expected) = &reference {
                assert_eq!(&value, expected);
            } else {
                reference = Some(value.clone());
            }
            assert!(!value.to_string().contains(&base.fingerprint));
            assert!(value.get("fingerprint").is_none());
        }
    }
    let mut unselected = base.clone();
    unselected.lens = Lens::Author;
    unselected.scene_id = None;
    unselected.pov_id = None;
    assert_eq!(
        get(service.clone(), &writing_uri("resolve", &unselected, ""))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        get(service, &writing_uri("context", &unselected, ""))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn character_and_dramatic_components_match_and_required_pins_precede_complete_budget_errors()
{
    let f = Fixture::new();
    let service = Service::new(f.world());
    let frame = f.frame(Lens::Character);
    let selected = format!(
        "&briefs={}&maxCharacters=100000",
        encode(&json!([BRIEF]).to_string())
    );
    let (_, character) = get(service.clone(), &writing_uri("context", &frame, &selected)).await;
    let mut dramatic = frame.clone();
    dramatic.lens = Lens::DramaticIrony;
    let (_, combined) = get(
        service.clone(),
        &writing_uri("context", &dramatic, &selected),
    )
    .await;
    assert_eq!(
        character["result"]["characterPrompt"],
        combined["result"]["characterPrompt"]
    );
    assert!(combined["result"].get("authorMargin").is_some());
    assert!(
        character["result"]["characterPrompt"]
            .to_string()
            .contains(OBJECT)
    );
    let (_, duplicates) = get(
        service.clone(),
        &writing_uri(
            "context",
            &frame,
            &format!(
                "&briefs={}&maxCharacters=100000",
                encode(&json!([BRIEF, BRIEF]).to_string())
            ),
        ),
    )
    .await;
    assert_eq!(character, duplicates);
    let (status, error) = get(
        service.clone(),
        &writing_uri(
            "context",
            &frame,
            &format!(
                "&briefs={}&maxCharacters=256",
                encode(&json!([BLOCKED_BRIEF]).to_string())
            ),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        !error.to_string().contains("needs")
            && !error.to_string().contains("PRIVATE")
            && !error.to_string().contains(HIDDEN_PLACE)
    );
    let (status, error) = get(
        service.clone(),
        &writing_uri(
            "context",
            &frame,
            &format!(
                "&briefs={}&maxCharacters=256",
                encode(&json!([BRIEF]).to_string())
            ),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("complete writing context")
    );
    assert!(error.get("result").is_none());
    let (status, error) = get(service.clone(), &writing_uri("choices", &frame, "&limit=1")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(error.get("result").is_none());
    let other = f.frame(Lens::Author);
    let file = frame_file(&f, &other);
    assert!(
        cli(
            &f,
            &[
                "writing",
                "context",
                "--file",
                &file,
                "--max-characters",
                "100000"
            ]
        )
        .0
    );
    assert_eq!(
        get(service, &writing_uri("context", &frame, &selected))
            .await
            .1,
        character
    );
}

#[tokio::test]
async fn valid_private_edits_change_the_pin_but_not_character_briefs_prompts_or_budget_results() {
    let f = Fixture::new();
    let world = f.world();
    let old_frame = f.frame(Lens::Character);
    let old_service = Service::new(world.clone());
    let brief_uri =
        format!("/api/character/brief?character={VIEWER}&brief={BRIEF}&tick={TICK}&order=0");
    let before_brief = get(old_service.clone(), &brief_uri).await;
    let selected = format!(
        "&briefs={}&maxCharacters=100000",
        encode(&json!([BRIEF]).to_string())
    );
    let before = get(
        old_service.clone(),
        &writing_uri("context", &old_frame, &selected),
    )
    .await;
    let before_budget = get(
        old_service,
        &writing_uri("context", &old_frame, "&maxCharacters=256"),
    )
    .await;
    for name in [
        "viewer",
        "other",
        "private",
        "place",
        "hidden-place",
        "object",
        "scene",
        "hidden-scene",
        "knowledge-1",
        "knowledge-2",
        "knowledge-3",
        "private-knowledge",
        "unknown-brief",
    ] {
        f.edit(name, |value, body| {
            value["title"] = Yaml::String(format!("Renamed private metadata {name}"));
            value["opaque_author_note"] = Yaml::String("PRIVATE_CHANGE".repeat(100));
            body.push_str(&"\nPRIVATE_UNTIMED_CHANGE".repeat(300));
        });
    }
    f.edit("place", |value, _| {
        value["descriptions"]
            .as_sequence_mut()
            .unwrap()
            .insert(0, serde_yaml::to_value(section(9, true)).unwrap());
    });
    f.edit("private-knowledge", |value, _| {
        value["claim"]["statement"] = Yaml::String("harbor bell ".repeat(100));
    });
    f.edit("knowledge-1",|value,_| {value["transitions"].as_sequence_mut().unwrap().push(serde_yaml::to_value(json!({"id":"future-note","time":source_time(TICK+100,0),"state":"rejected","confidence":1.0,"acquisition":"authored","note":"PRIVATE_FUTURE"})).unwrap());});
    f.valid();
    let after_world = f.world();
    assert_ne!(after_world.fingerprint, world.fingerprint);
    let new_service = Service::new(after_world.clone());
    assert_eq!(get(new_service.clone(), &brief_uri).await, before_brief);
    assert_eq!(
        get(
            new_service.clone(),
            &writing_uri("context", &old_frame, &selected)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let mut frame = old_frame;
    frame.fingerprint = after_world.fingerprint;
    let after = get(
        new_service.clone(),
        &writing_uri("context", &frame, &selected),
    )
    .await;
    assert_eq!(after.0, StatusCode::OK);
    assert_eq!(
        before.1["result"]["characterPrompt"],
        after.1["result"]["characterPrompt"]
    );
    safe_character(&after.1["result"]["characterPrompt"]);
    assert_eq!(
        get(
            new_service,
            &writing_uri("context", &frame, "&maxCharacters=256")
        )
        .await,
        before_budget
    );
}

#[tokio::test]
async fn invalid_source_errors_stay_generic_for_every_new_transport() {
    let f = Fixture::new();
    let frame = f.frame(Lens::Character);
    f.edit("private-knowledge", |value, _| {
        value["transitions"][0]["state"] = Yaml::String("broken-private-state".into());
    });
    assert!(!validation::validate_world(&f.world()).valid);
    let service = Service::new(f.world());
    let mut expected = None;
    for uri in [
        format!("/api/briefs/compose?brief={BRIEF}"),
        format!("/api/character/brief?character={VIEWER}&brief={BRIEF}&tick={TICK}"),
        writing_uri("resolve", &frame, ""),
        writing_uri("choices", &frame, ""),
        writing_uri("context", &frame, ""),
    ] {
        let (status, value) = get(service.clone(), &uri).await;
        assert!(!status.is_success());
        if let Some(expected) = &expected {
            assert_eq!(&value, expected);
        } else {
            expected = Some(value.clone());
        }
        assert!(
            !value.to_string().contains("private-knowledge")
                && !value.to_string().contains("broken-private-state")
        );
    }
    let file = frame_file(&f, &frame);
    assert_eq!(
        cli(&f, &["writing", "resolve", "--file", &file]).1,
        expected.unwrap()
    );
}
