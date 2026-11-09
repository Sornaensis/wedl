//! Specialized character families preserve admission across all public transports.
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
    environment, http, magic,
    model::{StoryTime, World},
    service::{Operation, Service, TimeInput},
    social, source, species, validation,
    writing_frames::{self, Frame, Lens},
};

const TICK: i64 = 9_007_199_254_740_993;
const VIEWER: &str = "char_00000000000000000000000001";
const OTHER: &str = "char_00000000000000000000000002";
const PLACE: &str = "loc_00000000000000000000000001";
const SCENE: &str = "scene_00000000000000000000000001";
fn id(kind: &str, n: usize) -> String {
    format!("{kind}_{n:026}")
}
fn concept(n: usize) -> String {
    id("concept", n)
}
fn reference(n: usize) -> Json {
    json!({"kind":"record","id":concept(n)})
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
fn source_time(tick: i64, order: i32) -> Json {
    json!({"timeline":"main","tick":tick,"order":order})
}
fn interval() -> Json {
    json!({"kind":"interval","timeline":"main","start":source_time(TICK,0),"end":source_time(TICK,2)})
}
fn entity() -> Json {
    json!({"type":"entity","kinds":["lore-concept"]})
}
fn entities() -> Json {
    json!({"type":"array","items":entity(),"max_items":64})
}
fn section(n: usize, hidden: bool) -> Json {
    json!({"id":id("description",n),"heading":if hidden {"PRIVATE_HEADING"} else {"Dated account"},
        "markdown":if hidden {"PRIVATE_SECTION"} else {"A dated **literal** account.\r\n"},
        "valid":interval(),"audience":if hidden {vec!["author"]} else {vec!["author","public"]},
        "perspectives":if hidden {vec!["author"]} else {vec!["author","ordinary"]}})
}
fn selected_species() -> species::Selection {
    species::Selection {
        common_profile: "species-common".into(),
        individual_profile: "species-individual".into(),
        individual_link: "common".into(),
        common_link: Some("parents".into()),
        common_fields: BTreeMap::from([("traits".into(), "traits".into())]),
        individual_fields: BTreeMap::from([("traits".into(), "traits".into())]),
    }
}
fn selected_magic() -> magic::Selection {
    serde_json::from_value(json!({"profiles":{"tradition":"magic-tradition"},
        "links":{"tradition":["links"]},"restrictions":{},"exceptions":{}}))
    .unwrap()
}
fn selected_social() -> social::Selection {
    social::Selection {
        language: concept(10),
        sense: Some(concept(11)),
        form: Some(String::new()),
        contexts: vec![concept(16)],
    }
}
fn families_json() -> Json {
    json!({"species":{"individualId":concept(2),"selection":selected_species(),"beliefTargets":[id("assertion",2)]},
        "magic":{"author":{"rootId":concept(4),"selection":selected_magic()},"beliefTargets":[id("assertion",2)]},
        "social":{"language":concept(10),"sense":concept(11),"form":"","contexts":[concept(16)],"beliefTargets":[id("assertion",2)]},
        "environment":{"contexts":[concept(20)],"beliefTargets":[id("assertion",2)]}})
}

struct Fixture(tempfile::TempDir);
impl Fixture {
    fn new() -> Self {
        let f = Self(tempfile::tempdir().unwrap());
        fs::create_dir(f.root().join("story")).unwrap();
        let mut profiles = Vec::new();
        for (name, required, optional) in [
            (
                "species-common",
                json!({"traits":{"type":"string"}}),
                json!({"parents":entities()}),
            ),
            (
                "species-individual",
                json!({"common":entities(),"traits":{"type":"string"}}),
                json!({}),
            ),
            (
                "magic-tradition",
                json!({}),
                json!({"links":entities(),"account":{"type":"string"}}),
            ),
            ("social-language", json!({}), json!({"scripts":entities()})),
            (
                "social-sense",
                json!({"language":entity(),"gloss":{"type":"string"}}),
                json!({}),
            ),
            (
                "social-era-spelling",
                json!({"language":entity(),"sense":entity(),"form":{"type":"string"}}),
                json!({"scripts":entities()}),
            ),
            ("social-script", json!({}), json!({})),
            (
                "social-culture",
                json!({}),
                json!({"account":{"type":"string"}}),
            ),
            (
                "context-technology",
                json!({}),
                json!({"dependencies":entities(),"amount":{"type":"number"}}),
            ),
            ("context-resource", json!({}), json!({})),
        ] {
            profiles.push(json!({"id":name,"label":format!("PRIVATE_PROFILE_{name}"),
            "fields":{"type":"object","required":required,"optional":optional}}));
        }
        f.write("world",&id("world",0),"world",json!({"capabilities":["lore-core-v1","descriptions-v1","assertions-v1"],
            "default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],
            "current_time":source_time(TICK,0),"profiles":profiles,
            "predicates":[{"id":"family-account","subject_kinds":["lore-concept"],"value_type":{"type":"string"}}]}));
        f.write("viewer", VIEWER, "character", json!({}));
        f.write("other", OTHER, "character", json!({}));
        f.write("place", PLACE, "location", json!({}));
        f.write("scene",SCENE,"scene",json!({"status":"closed","location":PLACE,
            "time":{"start":source_time(TICK-1,0),"current":source_time(TICK+2,0),"end":source_time(TICK+2,0)},
            "participants":[{"character":VIEWER,"point_of_view":true}]}));
        f.concept(
            1,
            "species-common",
            json!({"traits":"Common authored traits","parents":[]}),
            json!({"descriptions":[section(1,false),section(2,true)]}),
        );
        f.concept(
            2,
            "species-individual",
            json!({"common":[reference(1)],"traits":"Individual variation"}),
            json!({"descriptions":[section(1,false)]}),
        );
        f.concept(
            3,
            "species-common",
            json!({"traits":"UNACQUIRED_TRAITS","parents":[]}),
            json!({}),
        );
        f.concept(
            6,
            "species-individual",
            json!({"common":[reference(1)],"traits":"Second individual"}),
            json!({}),
        );
        f.concept(
            4,
            "magic-tradition",
            json!({"links":[],"account":"CANONICAL_MAGIC_PROFILE_ACCOUNT"}),
            json!({"descriptions":[section(1,false)]}),
        );
        f.concept(
            10,
            "social-language",
            json!({"scripts":[reference(14)]}),
            json!({}),
        );
        f.concept(
            11,
            "social-sense",
            json!({"language":reference(10),"gloss":"Exact sense"}),
            json!({}),
        );
        for (n, form) in [(12, ""), (13, "bank")] {
            f.concept(n,"social-era-spelling",json!({"language":reference(10),"sense":reference(11),"form":form,"scripts":[reference(14)]}),json!({}));
        }
        for n in [14, 15] {
            f.concept(n, "social-script", json!({}), json!({}));
        }
        f.concept(
            16,
            "social-culture",
            json!({"account":"Authored communal account"}),
            json!({"descriptions":[section(1,false)]}),
        );
        let mut replacement = section(2, false);
        replacement["replaces"] = json!(id("description", 1));
        replacement["markdown"] =
            json!("Corrected dated method.\r\n<script>window.privateLeak=true</script>\n");
        f.concept(
            20,
            "context-technology",
            json!({"dependencies":[reference(21)],"amount":9_007_199_254_740_993_u64}),
            json!({"descriptions":[section(1,false),replacement,section(3,true)]}),
        );
        for n in [21, 22] {
            f.concept(n, "context-resource", json!({}), json!({}));
        }
        for (n, subject) in [(1, 2), (2, 4)] {
            f.write(&format!("assertion-{n}"),&id("assertion",n),"assertion",json!({"subject":reference(subject),
                "domain":"family.transport","predicate":"family-account","value":format!("PRIVATE_CANONICAL_ACCOUNT_{n}"),"classification":"author-fact", "valid":interval(),"evidence":[],
                "audience":["author"],"perspectives":["author"]}));
        }
        for (n, entity) in [1, 2, 4, 6, 10, 11, 12, 13, 14, 16, 20, 21]
            .into_iter()
            .enumerate()
        {
            f.write(&format!("knowledge-{n}"),&id("know",n),"knowledge",json!({"knower":VIEWER,
                "claim":{"key":format!("known.{n}"),"statement":format!("I learned record {n}."),"object":{"entity":concept(entity)}},
                "transitions":[{"id":"acquired","time":source_time(TICK-1,0),"state":"accepted","confidence":1,"acquisition":"told","note":"PRIVATE_ACQUISITION_NOTE"}]}));
        }
        f.write("learned",&id("know",100),"knowledge",json!({"knower":VIEWER,
            "claim":{"key":"learned.account","statement":"The literal account I was taught.","assertion_id":id("assertion",2)},
            "transitions":[
                {"id":"learned","time":source_time(TICK-1,0),"state":"accepted","confidence":0.7,"acquisition":"told","note":"PRIVATE_NOTE"},
                {"id":"rejected","time":source_time(TICK,0),"state":"rejected","confidence":0.2,"acquisition":"inferred"},
                {"id":"forgotten","time":source_time(TICK,3),"state":"forgotten","confidence":0,"acquisition":"inferred"}]}));
        f.valid();
        f
    }
    fn root(&self) -> &Path {
        self.0.path()
    }
    fn write(&self, name: &str, identifier: &str, kind: &str, fields: Json) {
        let mut value = json!({"schema":"wedl/v1","id":identifier,"kind":kind,"title":format!("PRIVATE_TITLE_{name}"),"status":"canonical","aliases":[],"tags":[]});
        value
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        fs::write(
            self.root().join(format!("story/{name}.md")),
            format!(
                "---\n{}---\nPRIVATE_UNTIMED_BODY_{name}\r\n",
                serde_yaml::to_string(&value).unwrap()
            ),
        )
        .unwrap();
    }
    fn concept(&self, n: usize, profile: &str, values: Json, extra: Json) {
        let mut fields = json!({"domain":"family.transport","profile":profile,"values":values,"valid":{"kind":"timeless"},
            "audience":["author","public"],"perspectives":["author","ordinary"]});
        fields
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        self.write(&format!("concept-{n}"), &concept(n), "lore-concept", fields);
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
            at: point(0),
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
                let p = entry.unwrap().path();
                (
                    p.file_name().unwrap().to_string_lossy().into(),
                    fs::read(p).unwrap(),
                )
            })
            .collect()
    }
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn uri(route: &str, params: &[(&str, String)]) -> String {
    format!(
        "/api/{route}?{}",
        params
            .iter()
            .map(|(k, v)| format!("{k}={}", encode(v)))
            .collect::<Vec<_>>()
            .join("&")
    )
}
fn params() -> Vec<(&'static str, String)> {
    vec![
        ("character", VIEWER.into()),
        ("tick", TICK.to_string()),
        ("order", "0".into()),
        ("timeline", "main".into()),
    ]
}
fn species_params(root: usize) -> Vec<(&'static str, String)> {
    vec![
        ("id", concept(root)),
        ("common_profile", "species-common".into()),
        ("individual_profile", "species-individual".into()),
        ("individual_link", "common".into()),
        ("common_link", "parents".into()),
        ("common_fields", r#"{"traits":"traits"}"#.into()),
        ("individual_fields", r#"{"traits":"traits"}"#.into()),
    ]
}
fn character_envelope(value: Json) -> Json {
    json!({"protocol":wedl::CONTRACT,"result":value})
}
fn author_envelope(value: Json, world: &World) -> Json {
    json!({"protocol":wedl::CONTRACT,"fingerprint":world.fingerprint,"revision":world.revision,"result":value})
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
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}
fn cli(f: &Fixture, args: &[String]) -> (bool, Json) {
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
fn safe_character(value: &Json) {
    if let Some(fields) = value.as_object() {
        for (k, v) in fields {
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
                    "note",
                    "title",
                    "citation",
                    "canon"
                ]
                .contains(&k.as_str()),
                "leaked {k}"
            );
            safe_character(v);
        }
    }
    if let Some(values) = value.as_array() {
        for v in values {
            safe_character(v);
        }
    }
    assert!(!value.to_string().contains("PRIVATE_"));
}
struct Read {
    route: &'static str,
    query: Vec<(&'static str, String)>,
    args: Vec<String>,
    operation: Operation,
    native: Json,
}
fn reads(world: &World) -> Vec<Read> {
    let selected = selected_species();
    let beliefs = vec![id("assertion", 2)];
    let at = point(0);
    let species_args = vec![
        "--common-profile",
        "species-common",
        "--individual-profile",
        "species-individual",
        "--individual-link",
        "common",
        "--common-link",
        "parents",
        "--common-fields",
        r#"{"traits":"traits"}"#,
        "--individual-fields",
        r#"{"traits":"traits"}"#,
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    let mut result = Vec::new();
    for affected in [false, true] {
        let root = if affected { 1 } else { 2 };
        let name = if affected {
            "species-affected"
        } else {
            "species"
        };
        let mut query = params();
        query.extend(species_params(root));
        let mut args = vec![
            "character".into(),
            name.into(),
            VIEWER.into(),
            concept(root),
        ];
        args.extend(species_args.clone());
        if !affected {
            query.push(("beliefTargets", json!(beliefs).to_string()));
            args.extend(["--belief-target".into(), beliefs[0].clone()]);
        }
        args.extend([
            "--tick".into(),
            TICK.to_string(),
            "--order".into(),
            "0".into(),
            "--timeline".into(),
            "main".into(),
        ]);
        result.push(Read {
            route: if affected {
                "character/species-affected"
            } else {
                "character/species"
            },
            query,
            args,
            operation: if affected {
                Operation::CharacterSpeciesAffected {
                    character: VIEWER.into(),
                    common: concept(1),
                    selection: selected.clone(),
                    at: time(0),
                    limit: 100,
                }
            } else {
                Operation::CharacterSpeciesCompose {
                    character: VIEWER.into(),
                    individual: concept(2),
                    selection: selected.clone(),
                    belief_targets: beliefs.clone(),
                    at: time(0),
                }
            },
            native: if affected {
                species::character_affected(world, VIEWER, &concept(1), &selected, &at, 100)
                    .unwrap()
            } else {
                species::character_compose(world, VIEWER, &concept(2), &selected, &beliefs, &at)
                    .unwrap()
            },
        });
    }
    for family in ["magic", "social", "environment"] {
        let mut query = params();
        query.push(("beliefTargets", json!(beliefs).to_string()));
        let mut args = vec!["character".into(), family.into(), VIEWER.into()];
        let (operation, native) = match family {
            "magic" => (
                Operation::CharacterMagic {
                    character: VIEWER.into(),
                    belief_targets: beliefs.clone(),
                    at: time(0),
                },
                magic::character(world, VIEWER, &beliefs, &at).unwrap(),
            ),
            "social" => {
                query.extend([
                    ("language", concept(10)),
                    ("sense", concept(11)),
                    ("form", String::new()),
                    ("contexts", json!([concept(16)]).to_string()),
                ]);
                args.extend([
                    concept(10),
                    "--sense".into(),
                    concept(11),
                    "--form".into(),
                    String::new(),
                    "--context".into(),
                    concept(16),
                ]);
                (
                    Operation::CharacterSocialContext {
                        character: VIEWER.into(),
                        selected: selected_social(),
                        belief_targets: beliefs.clone(),
                        at: time(0),
                    },
                    social::character_context(world, VIEWER, &selected_social(), &beliefs, &at)
                        .unwrap(),
                )
            }
            _ => {
                query.push(("contexts", json!([concept(20)]).to_string()));
                args.extend(["--context".into(), concept(20)]);
                (
                    Operation::CharacterEnvironmentContext {
                        character: VIEWER.into(),
                        roots: vec![concept(20)],
                        belief_targets: beliefs.clone(),
                        at: time(0),
                    },
                    environment::character_context(world, VIEWER, &[concept(20)], &beliefs, &at)
                        .unwrap(),
                )
            }
        };
        args.extend([
            "--belief-target".into(),
            beliefs[0].clone(),
            "--tick".into(),
            TICK.to_string(),
            "--order".into(),
            "0".into(),
            "--timeline".into(),
            "main".into(),
        ]);
        result.push(Read {
            route: match family {
                "magic" => "character/magic",
                "social" => "character/social",
                _ => "character/environment",
            },
            query,
            args,
            operation,
            native,
        });
    }
    result
}

#[tokio::test]
async fn five_family_reads_agree_across_native_service_cli_http_at_exact_large_time() {
    let f = Fixture::new();
    let before = f.bytes();
    let world = f.world();
    let service = Service::new(world.clone());
    for read in reads(&world) {
        let expected = character_envelope(read.native);
        safe_character(&expected);
        assert_eq!(
            expected["result"]["effectiveTime"]["tick"],
            TICK.to_string()
        );
        assert_eq!(expected["result"]["effectiveTime"]["order"], "0");
        assert_eq!(
            service.execute(read.operation).unwrap(),
            expected,
            "{}",
            read.route
        );
        assert_eq!(
            cli(&f, &read.args),
            (true, expected.clone()),
            "{}",
            read.route
        );
        assert_eq!(
            get(service.clone(), &uri(read.route, &read.query)).await,
            (StatusCode::OK, expected),
            "{}",
            read.route
        );
    }
    assert_eq!(before, f.bytes());
}

#[tokio::test]
async fn family_routes_are_closed_and_character_magic_accepts_only_explicit_learned_targets() {
    let f = Fixture::new();
    let world = f.world();
    let service = Service::new(world.clone());
    for read in reads(&world) {
        let base = uri(read.route, &read.query);
        for extra in [
            "audience=author",
            "view=author",
            "scene=scene_00000000000000000000000001",
            "knownIds=%5B%5D",
            "character=duplicate",
            "tick=0",
            "unexpected=x",
        ] {
            assert_eq!(
                get(service.clone(), &format!("{base}&{extra}")).await.0,
                StatusCode::BAD_REQUEST,
                "{} {extra}",
                read.route
            );
        }
        for missing in ["tick", "character"] {
            let filtered = read
                .query
                .iter()
                .filter(|(k, _)| *k != missing)
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(
                get(service.clone(), &uri(read.route, &filtered)).await.0,
                StatusCode::BAD_REQUEST
            );
        }
    }
    let mut query = params();
    query.push(("beliefTargets", json!([id("assertion", 2)]).to_string()));
    let base = uri("character/magic", &query);
    for extra in [
        "id=concept_00000000000000000000000004",
        "profiles=%7B%7D",
        "links=%7B%7D",
        "contexts=%5B%5D",
        "limit=1",
    ] {
        assert_eq!(
            get(service.clone(), &format!("{base}&{extra}")).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    for bad in [
        json!([]),
        json!([id("assertion", 2), id("assertion", 2)]),
        json!([concept(4)]),
        json!([true]),
        json!({}),
        json!((1..=9).map(|n| id("assertion", n)).collect::<Vec<_>>()),
    ] {
        let mut query = params();
        query.push(("beliefTargets", bad.to_string()));
        assert_eq!(
            get(service.clone(), &uri("character/magic", &query))
                .await
                .0,
            StatusCode::BAD_REQUEST,
            "{bad}"
        );
    }
    let mut unknown = params();
    unknown.push(("beliefTargets", json!([id("assertion", 99)]).to_string()));
    let (status, value) = get(service.clone(), &uri("character/magic", &unknown)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["result"]["count"], 0);
    assert_eq!(value["result"]["outcome"], "unknown");
    let value = get(service, &base).await.1;
    assert_eq!(value["result"]["count"], 1);
    assert!(
        value
            .to_string()
            .contains("The literal account I was taught.")
    );
    assert!(
        !value
            .to_string()
            .contains("CANONICAL_MAGIC_PROFILE_ACCOUNT")
    );
    assert!(value.to_string().contains("rejected"));
}

#[tokio::test]
async fn exact_endpoints_empty_social_form_and_complete_limits_survive_transport() {
    let f = Fixture::new();
    let world = f.world();
    let service = Service::new(world.clone());
    for order in [0, 2, 3] {
        let mut query = params();
        query.iter_mut().find(|(k, _)| *k == "order").unwrap().1 = order.to_string();
        query.push(("beliefTargets", json!([id("assertion", 2)]).to_string()));
        let (status, value) = get(service.clone(), &uri("character/magic", &query)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["result"]["count"], if order == 3 { 0 } else { 1 });
        let mut q = params();
        q.iter_mut().find(|(k, _)| *k == "order").unwrap().1 = order.to_string();
        q.extend(species_params(2));
        let value = get(service.clone(), &uri("character/species", &q)).await.1;
        let sections = value["result"]["sections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|owner| owner["result"]["count"].as_u64().unwrap())
            .sum::<u64>();
        assert_eq!(sections, if order == 3 { 0 } else { 2 });
    }
    let social = reads(&world)
        .into_iter()
        .find(|r| r.route == "character/social")
        .unwrap();
    assert_eq!(social.native["selection"]["form"], "");
    assert!(social.native.to_string().contains(&concept(12)));
    assert!(!social.native.to_string().contains(&concept(13)));
    let mut query = params();
    query.extend(species_params(1));
    query.push(("limit", "1".into()));
    let (status, error) = get(service.clone(), &uri("character/species-affected", &query)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(error["message"].as_str().unwrap().contains("incomplete"));
    assert!(error.get("result").is_none());
    query.last_mut().unwrap().1 = "2".into();
    let value = get(service, &uri("character/species-affected", &query))
        .await
        .1;
    assert_eq!(value["result"]["count"], 2);
}

#[tokio::test]
async fn all_four_family_selections_match_three_lenses_without_changing_the_five_field_frame() {
    let f = Fixture::new();
    let before = f.bytes();
    let world = f.world();
    let service = Service::new(world.clone());
    let raw = families_json().to_string();
    let selected = writing_frames::decode_families(&raw).unwrap();
    let mut character = None;
    for lens in [Lens::Character, Lens::Author, Lens::DramaticIrony] {
        let frame = f.frame(lens);
        let file = f.root().join("frame.json");
        fs::write(&file, serde_json::to_string(&frame).unwrap()).unwrap();
        let native =
            writing_frames::context_with_families(&world, None, &frame, &[], &selected, 100000)
                .unwrap();
        assert_eq!(native["frame"].as_object().unwrap().len(), 6);
        if let Some(prompt) = native.get("characterPrompt") {
            safe_character(prompt);
            assert_eq!(prompt["familyContexts"].as_object().unwrap().len(), 4);
            for read in reads(&world)
                .into_iter()
                .filter(|r| r.route != "character/species-affected")
            {
                assert_eq!(
                    prompt["familyContexts"][read.route.rsplit('/').next().unwrap()],
                    read.native
                );
            }
            if lens == Lens::Character {
                character = Some(prompt.clone());
            } else {
                assert_eq!(Some(prompt), character.as_ref());
            }
        }
        if let Some(margin) = native.get("authorMargin") {
            assert_eq!(margin["familyContexts"].as_object().unwrap().len(), 4);
            assert!(
                !margin["familyContexts"]["species"]["canon"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
            assert!(
                !margin["familyContexts"]["magic"]["sections"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
        }
        let expected = author_envelope(native, &world);
        assert_eq!(
            service
                .execute(Operation::WritingFamilyContext {
                    frame: frame.clone(),
                    briefs: vec![],
                    families: selected.clone(),
                    max_characters: 100000
                })
                .unwrap(),
            expected
        );
        let args = vec![
            "writing".into(),
            "context".into(),
            "--file".into(),
            file.to_str().unwrap().into(),
            "--families".into(),
            raw.clone(),
            "--max-characters".into(),
            "100000".into(),
        ];
        assert_eq!(cli(&f, &args), (true, expected.clone()));
        let query = vec![
            ("frame", serde_json::to_string(&frame).unwrap()),
            ("families", raw.clone()),
            ("maxCharacters", "100000".into()),
        ];
        assert_eq!(
            get(service.clone(), &uri("writing/context", &query)).await,
            (StatusCode::OK, expected)
        );
        let old = writing_frames::context(&world, None, &frame, &[], 100000).unwrap();
        let empty = writing_frames::decode_families("{}").unwrap();
        assert_eq!(
            old,
            writing_frames::context_with_families(&world, None, &frame, &[], &empty, 100000)
                .unwrap()
        );
        assert!(old["characterPrompt"].get("familyContexts").is_none());
        assert!(old["authorMargin"].get("familyContexts").is_none());
    }
    assert_eq!(before, f.bytes());
}

#[tokio::test]
async fn writing_family_json_is_closed_and_magic_branches_remain_independent() {
    let f = Fixture::new();
    let world = f.world();
    let service = Service::new(world.clone());
    let frame = f.frame(Lens::Character);
    let base = vec![
        ("frame", serde_json::to_string(&frame).unwrap()),
        ("maxCharacters", "100000".into()),
    ];
    for raw in [
        r#"{"environment":null}"#,
        r#"{"environment":{"contexts":[],"sceneId":"x"}}"#,
        r#"{"environment":{"contexts":[],"contexts":[]}}"#,
        r#"{"species":{"individualId":"x","selection":{}}}"#,
        r#"{"social":{"language":"x","contexts":[]}}"#,
        r#"{"knownIds":[]}"#,
        r#"{"magic":{"author":null}}"#,
    ] {
        let mut query = base.clone();
        query.push(("families", raw.into()));
        assert_eq!(
            get(service.clone(), &uri("writing/context", &query))
                .await
                .0,
            StatusCode::BAD_REQUEST,
            "{raw}"
        );
    }
    let mut magic = families_json()["magic"].clone();
    magic["author"]["rootId"] = json!(concept(99));
    magic["author"]["selection"]["profiles"] = json!({"tradition":"nonexistent-profile"});
    let selected = writing_frames::decode_families(&json!({"magic":magic}).to_string()).unwrap();
    let native =
        writing_frames::context_with_families(&world, None, &frame, &[], &selected, 100000)
            .unwrap();
    assert_eq!(
        native["characterPrompt"]["familyContexts"]["magic"],
        magic::character(&world, VIEWER, &[id("assertion", 2)], &point(0)).unwrap()
    );
    let mut query = base.clone();
    query.push(("families", serde_json::to_string(&selected).unwrap()));
    assert_eq!(
        get(service.clone(), &uri("writing/context", &query))
            .await
            .0,
        StatusCode::OK
    );
    for lens in [Lens::Author, Lens::Character, Lens::DramaticIrony] {
        let frame = f.frame(lens);
        let no_author = writing_frames::decode_families(
            &json!({"magic":{"beliefTargets":[id("assertion",2)]}}).to_string(),
        )
        .unwrap();
        assert_eq!(
            writing_frames::context_with_families(&world, None, &frame, &[], &no_author, 100000)
                .is_ok(),
            lens == Lens::Character
        );
        let no_beliefs = writing_frames::decode_families(
            &json!({"magic":{"author":families_json()["magic"]["author"]}}).to_string(),
        )
        .unwrap();
        assert_eq!(
            writing_frames::context_with_families(&world, None, &frame, &[], &no_beliefs, 100000)
                .is_ok(),
            lens == Lens::Author
        );
    }
    for operation in ["resolve", "choices"] {
        let query = vec![
            ("frame", serde_json::to_string(&frame).unwrap()),
            ("families", "{}".into()),
        ];
        assert_eq!(
            get(
                service.clone(),
                &uri(&format!("writing/{operation}"), &query)
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    let mut query = base;
    query.extend([("families", "{}".into()), ("families", "{}".into())]);
    assert_eq!(
        get(service, &uri("writing/context", &query)).await.0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn private_edits_and_unadmitted_reference_prefixes_leave_character_responses_and_budgets_identical()
 {
    let f = Fixture::new();
    let world = f.world();
    let service = Service::new(world.clone());
    let mut before = Vec::new();
    for read in reads(&world) {
        before.push((
            read.route,
            read.query.clone(),
            get(service.clone(), &uri(read.route, &read.query)).await,
        ));
    }
    let raw = families_json().to_string();
    let selected = writing_frames::decode_families(&raw).unwrap();
    let frame = f.frame(Lens::Character);
    let packet =
        writing_frames::context_with_families(&world, None, &frame, &[], &selected, 100000)
            .unwrap();
    let budget = writing_frames::context_with_families(&world, None, &frame, &[], &selected, 256)
        .unwrap_err()
        .to_string();
    for name in [
        "concept-1",
        "concept-2",
        "concept-3",
        "concept-4",
        "concept-10",
        "concept-11",
        "concept-12",
        "concept-13",
        "concept-14",
        "concept-15",
        "concept-16",
        "concept-20",
        "concept-21",
        "concept-22",
        "learned",
        "knowledge-0",
    ] {
        f.edit(name, |value, body| {
            value["title"] = Yaml::String("CHANGED_PRIVATE_TITLE".into());
            *body = "CHANGED_PRIVATE_BODY".into();
        });
    }
    f.edit("concept-2", |value, _| {
        value["values"]["common"]
            .as_sequence_mut()
            .unwrap()
            .insert(0, serde_yaml::to_value(reference(3)).unwrap())
    });
    f.edit("concept-10", |value, _| {
        value["values"]["scripts"]
            .as_sequence_mut()
            .unwrap()
            .insert(0, serde_yaml::to_value(reference(15)).unwrap())
    });
    f.edit("concept-20", |value, _| {
        value["values"]["dependencies"]
            .as_sequence_mut()
            .unwrap()
            .insert(0, serde_yaml::to_value(reference(22)).unwrap())
    });
    f.edit("concept-1", |value, _| {
        value["descriptions"]
            .as_sequence_mut()
            .unwrap()
            .insert(0, serde_yaml::to_value(section(99, true)).unwrap())
    });
    f.edit("assertion-2", |value, _| {
        value["value"] = Yaml::String("CHANGED_CURRENT_CANON".into())
    });
    f.edit("world", |value, _| {
        for profile in value["profiles"].as_sequence_mut().unwrap() {
            profile["label"] = Yaml::String("Changed hidden profile label".into());
        }
    });
    f.valid();
    let changed = f.world();
    assert_ne!(world.fingerprint, changed.fingerprint);
    let service = Service::new(changed.clone());
    for (route, query, expected) in before {
        assert_eq!(
            get(service.clone(), &uri(route, &query)).await,
            expected,
            "{route}"
        );
    }
    let refreshed = f.frame(Lens::Character);
    let after =
        writing_frames::context_with_families(&changed, None, &refreshed, &[], &selected, 100000)
            .unwrap();
    assert_eq!(packet["characterPrompt"], after["characterPrompt"]);
    assert_eq!(
        budget,
        writing_frames::context_with_families(&changed, None, &refreshed, &[], &selected, 256)
            .unwrap_err()
            .to_string()
    );
    let query = vec![
        ("frame", serde_json::to_string(&frame).unwrap()),
        ("families", raw),
        ("maxCharacters", "100000".into()),
    ];
    let (status, error) = get(service, &uri("writing/context", &query)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!error.to_string().contains(&changed.fingerprint));
    assert!(!error.to_string().contains("PRIVATE"));
}

#[tokio::test]
async fn unavailable_roots_and_invalid_worlds_do_not_emit_partial_family_results_or_private_diagnostics()
 {
    let f = Fixture::new();
    let world = f.world();
    let service = Service::new(world.clone());
    let mut unavailable = None;
    for root in [
        concept(3),
        concept(99),
        OTHER.into(),
        "PRIVATE_TITLE_concept-2".into(),
    ] {
        let mut query = params();
        query.extend(species_params(2));
        query.iter_mut().find(|(k, _)| *k == "id").unwrap().1 = root;
        let response = get(service.clone(), &uri("character/species", &query)).await;
        assert_eq!(response.0, StatusCode::NOT_FOUND);
        assert!(response.1.get("result").is_none());
        if let Some(expected) = &unavailable {
            assert_eq!(&response, expected);
        } else {
            unavailable = Some(response);
        }
    }
    let before = reads(&world);
    f.edit("assertion-2", |value, _| {
        value["predicate"] = Yaml::String("private-broken-predicate".into())
    });
    let invalid = Service::new(f.world());
    for read in before {
        let (status, error) = get(invalid.clone(), &uri(read.route, &read.query)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{} {error}", read.route);
        assert!(error.get("result").is_none());
        assert!(!error.to_string().contains("private-broken-predicate"));
        assert!(!error.to_string().contains("assertion-2.md"));
    }
}
