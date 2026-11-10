use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::TempDir;
use tower::ServiceExt;
use wedl::{http, service::Service, source, validation};

const WORLD: &str = "world_00000000000000000000000000";
const ANA: &str = "char_00000000000000000000000001";
const MINA: &str = "char_00000000000000000000000002";
const HIDDEN: &str = "char_00000000000000000000000003";
const DOCK: &str = "loc_00000000000000000000000001";
const BLUFF: &str = "loc_00000000000000000000000002";
const SCENE: &str = "scene_00000000000000000000000001";
const OTHER_SCENE: &str = "scene_00000000000000000000000002";
const EARLY: &str = "event_00000000000000000000000001";
const LATE: &str = "event_00000000000000000000000002";
const OTHER: &str = "event_00000000000000000000000003";
const REMEMBERED: &str = "event_00000000000000000000000004";
const LENS: &str = "obj_00000000000000000000000001";
const COMPASS: &str = "obj_00000000000000000000000002";
const HIDDEN_ITEM: &str = "obj_00000000000000000000000003";
const STORY_POINT: &str = "sp_00000000000000000000000001";
const ROUTE: &str = "route:harbor/east";
const LORE: &str = "concept_00000000000000000000000001";
const LEDGER: &str = "concept_00000000000000000000000002";
const HIDDEN_LORE: &str = "concept_00000000000000000000000003";
const K1: &str = "know_00000000000000000000000001";
const K2: &str = "know_00000000000000000000000002";
const K3: &str = "know_00000000000000000000000003";
const K4: &str = "know_00000000000000000000000004";
const K5: &str = "know_00000000000000000000000005";
const K6: &str = "know_00000000000000000000000006";
const DIGEST: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
const TICK: i64 = 9_007_199_254_740_993;

fn time(timeline: &str, order: i32) -> Value {
    json!({"timeline":timeline, "tick":TICK, "order":order})
}

fn record(root: &Path, kind: &str, id: &str, title: &str, fields: Value) {
    let mut front = json!({"schema":"wedl/v1", "kind":kind, "id":id, "title":title});
    front
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let raw = format!(
        "---\n{}---\nAuthor-only body PRIVATE_BODY ref:{HIDDEN} ref:route:harbor/east sha256:{DIGEST} /draft/story/hidden.md\n",
        serde_yaml::to_string(&front).unwrap()
    );
    let filename = id.replace([':', '/'], "-");
    fs::write(root.join("story").join(format!("{filename}.md")), raw).unwrap();
}

fn fixture() -> (TempDir, Service) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path();
    fs::create_dir(path.join("story")).unwrap();
    record(
        path,
        "world",
        WORLD,
        &format!("Fixture harbor ref:{HIDDEN}"),
        json!({
            "default_timeline":"main",
            "timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],
        "current_time":time("main", 4), "capabilities":["spatial-core-v1", "route-v1", "lore-core-v1"], "threads":[],
        "state_keys":{}, "relationship_metrics":{},
        "profiles":[{"id":"notes","label":"Notes","fields":{"type":"object","required":{"note":{"type":"string"}},"optional":{}}}],
        "chronology":{
            "calendars":[{
                "id":"calendar_00000000000000000000000001", "label":"Harbor calendar",
                "months":[{"number":1,"days":1}], "rule":{"kind":"cycle","period":1,"overrides":[]},
                "epoch":{"civil":{"year":0,"month":1,"day":1},"axis_day":0}
            }],
            "eras":[],
            "anchors":[
                {"id":"chronology_00000000000000000000000001","axis_day":0,"story_time":time("main",1),"provenance":["Harbor ledger"]},
                {"id":"chronology_00000000000000000000000002","axis_day":1,"story_time":time("main",2),"provenance":["Harbor ledger"]}
            ]
        }
        }),
    );
    record(
        path,
        "location",
        DOCK,
        "Dock",
        json!({
            "concept_links":[
                {"relation":"documents", "concept_id":LEDGER, "valid":{"kind":"timeless"}},
                {"relation":"withheld", "concept_id":HIDDEN_LORE, "valid":{"kind":"timeless"}}
            ]
        }),
    );
    record(path, "location", BLUFF, "Bluff", json!({}));
    record(
        path,
        "route",
        ROUTE,
        "East footpath",
        json!({
            "from_location_id":DOCK, "to_location_id":BLUFF, "direction":"one-way",
            "modes":["foot"], "availability":"closed"
        }),
    );
    record(path, "object", LENS, "Lantern lens", json!({}));
    record(path, "object", COMPASS, "Loose compass", json!({}));
    record(
        path,
        "object",
        HIDDEN_ITEM,
        "PRIVATE_SCENE_OBJECT",
        json!({"audience":[]}),
    );
    for (id, title, fields) in [
        (
            ANA,
            "Ana",
            json!({"initial_state":{},"aliases":["PRIVATE_ALIAS"]}),
        ),
        (MINA, "Mina", json!({"initial_state":{}})),
        (
            HIDDEN,
            "PRIVATE_NAME",
            json!({"initial_state":{},"audience":[]}),
        ),
    ] {
        record(path, "character", id, title, fields);
    }
    record(
        path,
        "scene",
        SCENE,
        "Harbor assessment",
        json!({
            "status":"active", "location":DOCK,
            "time":{"start":time("main",0), "current":time("main",4)},
            "participants":[{"character":ANA},{"character":MINA,"from":time("main",2)}],
            "objects":[LENS,HIDDEN_ITEM], "conversations":[], "observations":[], "environments":[], "outcome_events":[]
        }),
    );
    record(
        path,
        "scene",
        OTHER_SCENE,
        "PRIVATE_OTHER_SCENE",
        json!({
            "status":"closed", "location":DOCK,
            "time":{"start":time("main",0), "current":time("main",1), "end":time("main",1)},
            "participants":[{"character":MINA}],
            "objects":[], "conversations":[], "observations":[], "environments":[], "outcome_events":[]
        }),
    );
    for (id, label, timeline, order) in [
        (EARLY, "First signal", "main", 1),
        (LATE, "Second signal", "main", 2),
        (OTHER, "First signal", "other", 1),
        (REMEMBERED, "Old bell", "main", 0),
    ] {
        let participants = match id {
            EARLY => json!([{"character":ANA}, {"character":MINA,"role":"AUTHOR_ROLE_SECRET"}]),
            LATE => json!([{"character":MINA}]),
            _ => json!([]),
        };
        record(
            path,
            "event",
            id,
            label,
            json!({
                "time":time(timeline,order), "location":DOCK, "participants":participants,
                "causes":if id == LATE { vec![EARLY] } else { Vec::new() },
                "effects":[], "related_story_points":[]
            }),
        );
    }
    record(
        path,
        "story-point",
        STORY_POINT,
        "Signal repaired",
        json!({
            "activation_policy":"suggest", "lifecycle":{"initial_state":"dormant","transitions":[]},
            "outcome_events":[LATE]
        }),
    );
    record(
        path,
        "lore-concept",
        LORE,
        "Log excerpt",
        json!({
            "domain":"harbor.notes", "profile":"notes", "values":{"note":"Observed signal"},
        "audience":["public"], "perspectives":["ordinary"],
            "valid":{"kind":"timeless"}, "concept_links":[
                {"relation":"supports", "concept_id":LEDGER, "valid":{"kind":"interval","timeline":"main","start":time("main",2)}},
                {"relation":"withheld", "concept_id":HIDDEN_LORE, "valid":{"kind":"timeless"}}
            ]
        }),
    );
    for (id, label, fields) in [
        (LEDGER, "Signal ledger", json!({"audience":["public"]})),
        (HIDDEN_LORE, "PRIVATE_LORE", json!({"audience":["private"]})),
    ] {
        let mut metadata = json!({
            "domain":"harbor.notes", "profile":"notes", "values":{"note":"Reference"},
            "valid":{"kind":"timeless"}, "perspectives":["ordinary"]
        });
        metadata
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        record(path, "lore-concept", id, label, metadata);
    }
    for (id, viewer, target, title, order) in [
        (K1, ANA, EARLY, "Saw the first signal", 1),
        (K2, ANA, LATE, "Saw the second signal", 2),
        (K3, MINA, EARLY, "Observed the first signal", 1),
        (K4, ANA, LORE, "Read the log excerpt", 1),
        (K5, ANA, LEDGER, "Read the signal ledger", 1),
        (K6, ANA, REMEMBERED, "Heard about the old bell", 2),
    ] {
        record(
            path,
            "knowledge",
            id,
            "PRIVATE_BELIEF_TITLE",
            json!({
                "knower":viewer, "claim":{"key":"internal", "statement":title,"truth":"true", "object":{"entity":target}},
                "transitions":[{"id":"observed","time":time("main",order),"state":"accepted","acquisition":"observed","confidence":1.0}]
            }),
        );
    }
    let world = source::load_world(path, None).unwrap();
    let validation = validation::validate_world(&world);
    assert!(validation.valid, "{validation:#?}");
    (root, Service::new(world))
}

fn query(fields: &[(&str, &str)]) -> String {
    fields
        .iter()
        .map(|(name, value)| {
            let value = value
                .bytes()
                .map(|byte| {
                    if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                        (byte as char).to_string()
                    } else {
                        format!("%{byte:02X}")
                    }
                })
                .collect::<String>();
            format!("{name}={value}")
        })
        .collect::<Vec<_>>()
        .join("&")
}

async fn get(app: &Router, endpoint: &str, fields: &[(&str, &str)]) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/compendium/{endpoint}?{}", query(fields)))
                .header("host", "localhost:8765")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

async fn read(app: &Router, endpoint: &str, fields: &[(&str, &str)]) -> Value {
    let (status, value) = get(app, endpoint, fields).await;
    assert_eq!(status, StatusCode::OK, "{endpoint} {fields:?}: {value}");
    value
}

fn view(result: &Value) -> &str {
    result["viewToken"].as_str().unwrap()
}

fn handle<'a>(value: &'a Value, label: &str) -> &'a str {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["label"] == label)
        .unwrap_or_else(|| panic!("missing {label:?} in {value}"))["handle"]
        .as_str()
        .unwrap()
}

fn card<'a>(value: &'a Value, title: &str) -> Option<&'a str> {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["title"] == title)
        .and_then(|row| row["handle"].as_str())
}

async fn named(app: &Router, token: &str, mode: &str, timeline: &str) -> (String, Value) {
    let timelines = read(
        app,
        "choices",
        &[("view", token), ("menu", "timeline"), ("mode", mode)],
    )
    .await;
    let timeline = handle(&timelines, timeline).to_owned();
    let moments = read(
        app,
        "choices",
        &[
            ("view", token),
            ("menu", "moment"),
            ("mode", mode),
            ("timeline", &timeline),
        ],
    )
    .await;
    (timeline, moments)
}

async fn character(app: &Router, token: &str, name: &str, explicit_scene: bool) -> Value {
    let mode = "character";
    let (timeline, moments) = named(app, token, mode, "Main").await;
    let moment = handle(&moments, name);
    let mut fields = vec![
        ("view", token),
        ("mode", mode),
        ("timeline", &timeline),
        ("moment", moment),
    ];
    let mut choices = fields.clone();
    choices.push(("menu", "viewpoint"));
    let viewpoints = read(app, "choices", &choices).await;
    let viewer = handle(&viewpoints, "Ana");
    fields.push(("viewpoint", viewer));
    let scenes = if explicit_scene {
        let mut choices = fields.clone();
        choices.push(("menu", "scene"));
        Some(read(app, "choices", &choices).await)
    } else {
        None
    };
    if let Some(scenes) = &scenes {
        let fake = "a".repeat(64);
        let mut forged = fields.clone();
        forged.push(("scene", &fake));
        let (status, error) = get(app, "context", &forged).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(error["code"], "unavailable");
        fields.push(("scene", handle(scenes, "Harbor assessment")));
    }
    read(app, "context", &fields).await
}

#[tokio::test]
async fn named_author_positions_are_exact_and_global_rows_are_deduplicated() {
    let (_root, service) = fixture();
    let app = http::router(service);
    let global = read(&app, "context", &[]).await;
    let token = view(&global);
    assert_eq!(global["labels"]["mode"], "all");
    let directory = read(&app, "page", &[("view", token)]).await;
    assert_eq!(global["labels"]["title"], "Fixture harbor Reference");
    assert!(card(&directory, "Second signal").is_some());
    assert_eq!(
        directory["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["title"] == "Dock")
            .count(),
        1
    );
    assert_eq!(
        directory["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["title"] == "First signal")
            .count(),
        2
    );
    let (timeline, moments) = named(&app, token, "as-of", "Main").await;
    let no_matches = read(
        &app,
        "page",
        &[
            ("view", token),
            ("q", "Nothing matches here"),
            ("kind", "event"),
            ("timeline", &timeline),
        ],
    )
    .await;
    assert_eq!(no_matches["total"], 0);
    assert_eq!(
        no_matches["facets"]["kinds"],
        json!([{"handle":"event","label":"Event","count":0}])
    );
    assert_eq!(
        no_matches["facets"]["timelines"],
        json!([{"handle":timeline,"label":"Main","count":0}])
    );
    let detail = read(
        &app,
        "record",
        &[
            ("view", token),
            ("record", card(&directory, "Dock").unwrap()),
        ],
    )
    .await;
    assert!(
        detail["sections"][0]["text"]
            .as_str()
            .unwrap()
            .contains("PRIVATE_BODY Reference Reference Reference Reference")
    );
    for raw in [HIDDEN, "route:harbor/east", DIGEST, "story/hidden.md"] {
        assert!(!detail.to_string().contains(raw));
        assert_eq!(
            read(&app, "page", &[("view", token), ("q", raw)]).await["total"],
            0
        );
    }
    let (other, other_moments) = named(&app, token, "as-of", "Other").await;
    assert_ne!(
        handle(&moments, "First signal"),
        handle(&other_moments, "First signal")
    );
    for (name, later) in [("First signal", false), ("Second signal", true)] {
        let dated = read(
            &app,
            "context",
            &[
                ("view", token),
                ("mode", "as-of"),
                ("timeline", &timeline),
                ("moment", handle(&moments, name)),
            ],
        )
        .await;
        let page = read(&app, "page", &[("view", view(&dated))]).await;
        assert_eq!(card(&page, "Second signal").is_some(), later);
        assert_eq!(dated["labels"]["moment"], name);
        assert!(!page.to_string().contains(&TICK.to_string()));
    }
    let (status, _) = get(
        &app,
        "context",
        &[
            ("view", token),
            ("mode", "as-of"),
            ("timeline", &other),
            ("moment", handle(&moments, "First signal")),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let first = read(&app, "page", &[("view", token), ("limit", "1")]).await;
    let cursor = first["nextCursor"].as_str().unwrap();
    let next = read(
        &app,
        "page",
        &[("view", token), ("limit", "1"), ("cursor", cursor)],
    )
    .await;
    assert_ne!(first["items"], next["items"]);
    assert_eq!(first["facets"], next["facets"]);
    let (status, _) = get(
        &app,
        "page",
        &[
            ("view", token),
            ("limit", "1"),
            ("cursor", cursor),
            ("q", "Dock"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let calendars = read(
        &app,
        "choices",
        &[("view", token), ("mode", "as-of"), ("menu", "calendar")],
    )
    .await;
    let calendar = handle(&calendars, "Harbor calendar");
    let filters = [
        ("view", token),
        ("mode", "as-of"),
        ("timeline", timeline.as_str()),
        ("calendar", calendar),
        ("date", "1-1-1"),
    ];
    let mut date_menu = filters.to_vec();
    date_menu.push(("menu", "moment"));
    let dated = read(&app, "choices", &date_menu).await;
    assert_eq!(dated["items"].as_array().unwrap().len(), 1);
    let mut context = filters.to_vec();
    context.push(("moment", handle(&dated, "Second signal")));
    assert_eq!(
        read(&app, "context", &context).await["labels"]["moment"],
        "Second signal"
    );
    assert!(!calendars.to_string().contains("calendar_"));
    assert!(!dated.to_string().contains(&TICK.to_string()));
}

#[tokio::test]
async fn character_admission_precedes_choices_filters_details_and_scene_enrichment() {
    let (_root, service) = fixture();
    let app = http::router(service);
    let global = read(&app, "context", &[]).await;
    let token = view(&global);
    let author_page = read(&app, "page", &[("view", token)]).await;
    let author_mina = card(&author_page, "Mina").unwrap();
    let ordinary = character(&app, token, "First signal", false).await;
    let character_token = view(&ordinary);
    assert_eq!(ordinary["labels"]["viewpoint"], "Ana");
    assert_eq!(ordinary["capabilities"]["workbench"], false);
    assert!(ordinary["labels"]["scene"].is_null());
    let page = read(&app, "page", &[("view", character_token)]).await;
    for title in [
        "Second signal",
        "Mina",
        "Dock",
        "Harbor assessment",
        "PRIVATE_NAME",
    ] {
        assert!(card(&page, title).is_none(), "{title} appeared: {page}");
    }
    let learned = card(&page, "Saw the first signal").unwrap();
    let detail = read(
        &app,
        "record",
        &[("view", character_token), ("record", learned)],
    )
    .await;
    assert_eq!(detail["sections"][0]["text"], "Saw the first signal");
    assert!(!detail.to_string().contains("PRIVATE"));
    assert!(!detail.to_string().contains(K1));
    let (transplanted_status, transplanted) = get(
        &app,
        "record",
        &[("view", character_token), ("record", author_mina)],
    )
    .await;
    let (missing_status, missing) = get(
        &app,
        "record",
        &[("view", character_token), ("record", &"a".repeat(64))],
    )
    .await;
    assert_eq!(transplanted_status, StatusCode::NOT_FOUND);
    assert_eq!(
        (transplanted_status, transplanted),
        (missing_status, missing)
    );
    for q in ["PRIVATE_ALIAS", "PRIVATE_BODY", "PRIVATE_NAME", ANA] {
        let result = read(&app, "page", &[("view", character_token), ("q", q)]).await;
        assert_eq!(result["total"], 0, "{result}");
    }
    let (timeline, moments) = named(&app, character_token, "character", "Main").await;
    assert!(
        moments["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["label"] != "Second signal")
    );
    let (status, _) = get(
        &app,
        "choices",
        &[
            ("view", character_token),
            ("mode", "as-of"),
            ("menu", "moment"),
            ("timeline", &timeline),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let enhanced = character(&app, token, "Second signal", true).await;
    let enhanced_page = read(&app, "page", &[("view", view(&enhanced))]).await;
    assert_eq!(enhanced["labels"]["scene"], "Harbor assessment");
    for title in ["Mina", "Dock", "Harbor assessment"] {
        assert!(card(&enhanced_page, title).is_some(), "{enhanced_page}");
    }
    let later = character(&app, token, "Second signal", false).await;
    let later_page = read(&app, "page", &[("view", view(&later))]).await;
    assert!(card(&later_page, "Second signal").is_some());
    let (_, moments) = named(&app, token, "character", "Main").await;
    assert!(!moments.to_string().contains(EARLY));
    assert!(!moments.to_string().contains(&TICK.to_string()));
}

#[tokio::test]
async fn changing_character_viewpoint_does_not_disclose_the_destination_characters_scenes() {
    let (_root, service) = fixture();
    let app = http::router(service);
    let author = read(&app, "context", &[]).await;
    let ana = character(&app, view(&author), "Second signal", true).await;
    let (timeline, moments) = named(&app, view(&ana), "character", "Main").await;
    let fields = [
        ("view", view(&ana)),
        ("mode", "character"),
        ("timeline", &timeline),
        ("moment", handle(&moments, "First signal")),
    ];
    let mut viewpoint_menu = fields.to_vec();
    viewpoint_menu.push(("menu", "viewpoint"));
    let viewpoints = read(&app, "choices", &viewpoint_menu).await;
    let mina = handle(&viewpoints, "Mina");
    let mut change = fields.to_vec();
    change.push(("viewpoint", mina));
    let mut scene_menu = change.clone();
    scene_menu.push(("menu", "scene"));
    let withheld = read(&app, "choices", &scene_menu).await;
    assert_eq!(withheld["items"], json!([]));
    assert!(withheld["nextCursor"].is_null());
    let switched = read(&app, "context", &change).await;
    assert_eq!(switched["labels"]["viewpoint"], "Mina");
    assert!(switched["labels"]["scene"].is_null());

    let (timeline, moments) = named(&app, view(&switched), "character", "Main").await;
    let fields = [
        ("view", view(&switched)),
        ("mode", "character"),
        ("timeline", &timeline),
        ("moment", handle(&moments, "First signal")),
    ];
    let mut viewpoint_menu = fields.to_vec();
    viewpoint_menu.push(("menu", "viewpoint"));
    let viewpoints = read(&app, "choices", &viewpoint_menu).await;
    let mut scene_menu = fields.to_vec();
    scene_menu.extend([
        ("viewpoint", handle(&viewpoints, "Mina")),
        ("menu", "scene"),
    ]);
    let own = read(&app, "choices", &scene_menu).await;
    assert!(handle(&own, "PRIVATE_OTHER_SCENE").len() == 64);
}

#[tokio::test]
async fn current_generation_changes_fence_old_view_and_fixed_router_stays_pinned() {
    let (root, service) = fixture();
    let fixed = http::router(service.clone());
    let current = http::live_router(service);
    let before_fixed = read(&fixed, "context", &[]).await;
    let before_current = read(&current, "context", &[]).await;
    let (status, _) = get(&current, "page", &[("view", view(&before_fixed))]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let old = read(&current, "page", &[("view", view(&before_current))]).await;
    assert!(card(&old, "Ana").is_some());
    let path = root.path().join("story").join(format!("{ANA}.md"));
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let raw = fs::read_to_string(&path)
        .unwrap()
        .replace("title: Ana", "title: Ada");
    fs::write(&path, raw).unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    let (status, error) = get(&current, "page", &[("view", view(&before_current))]).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["code"], "context_changed");
    let fresh = read(&current, "context", &[]).await;
    let new = read(&current, "page", &[("view", view(&fresh))]).await;
    assert!(card(&new, "Ada").is_some());
    let fixed = read(&fixed, "page", &[("view", view(&before_fixed))]).await;
    assert!(card(&fixed, "Ana").is_some());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

fn linked(value: &Value, label: &str, relation: &str, direction: &str) -> bool {
    value["items"].as_array().unwrap().iter().any(|item| {
        item["label"] == label
            && item["relation"] == relation
            && item["direction"] == direction
            && item["handle"].is_string()
    })
}

async fn graph_root(app: &Router, token: &str, root: &str) -> Value {
    read(app, "graph", &[("view", token), ("root", root)]).await
}

#[tokio::test]
async fn graph_projects_causal_scene_route_and_story_outcome_edges_on_the_named_view() {
    let (_root, service) = fixture();
    let app = http::router(service);
    let global = read(&app, "context", &[]).await;
    let token = view(&global);
    let (timeline, moments) = named(&app, token, "as-of", "Main").await;
    let (alternate, _) = named(&app, token, "as-of", "Other").await;
    let preview = read(&app, "graph", &[("view", token), ("timeline", &alternate)]).await;
    assert_eq!(preview["timeline"]["label"], "Other");
    assert_eq!(preview["items"].as_array().unwrap().len(), 1);
    assert_eq!(preview["items"][0]["label"], "First signal");
    // Previewing the second timeline did not replace the global directory.
    let all = read(&app, "page", &[("view", token), ("limit", "48")]).await;
    assert!(card(&all, "Second signal").is_some());

    for (name, before) in [("First signal", true), ("Second signal", false)] {
        let dated = read(
            &app,
            "context",
            &[
                ("view", token),
                ("mode", "as-of"),
                ("timeline", &timeline),
                ("moment", handle(&moments, name)),
            ],
        )
        .await;
        let dated_token = view(&dated);
        let page = read(&app, "page", &[("view", dated_token), ("limit", "48")]).await;
        let early = card(&page, "First signal").unwrap();
        let relationships = graph_root(&app, dated_token, early).await;
        assert_eq!(
            linked(&relationships, "Second signal", "cause", "outgoing"),
            !before
        );
        assert!(linked(
            &relationships,
            "Mina",
            "event-participant",
            "outgoing"
        ));
        assert!(relationships.to_string().contains("AUTHOR_ROLE_SECRET"));
        let story = graph_root(&app, dated_token, card(&page, "Signal repaired").unwrap()).await;
        assert_eq!(
            linked(&story, "Second signal", "story-outcome", "outgoing"),
            !before
        );
        if !before {
            let late = graph_root(&app, dated_token, card(&page, "Second signal").unwrap()).await;
            assert!(linked(&late, "First signal", "cause", "incoming"));
            assert!(linked(
                &late,
                "Signal repaired",
                "story-outcome",
                "incoming"
            ));
        }
    }
    let dock = card(&all, "Dock").unwrap();
    let bluff = card(&all, "Bluff").unwrap();
    let dock_links = graph_root(&app, token, dock).await;
    assert!(linked(&dock_links, "Bluff", "route", "outgoing"));
    assert!(dock_links.to_string().contains("East footpath · Closed"));
    let bluff_links = graph_root(&app, token, bluff).await;
    assert!(linked(&bluff_links, "Dock", "route", "incoming"));
    assert!(!linked(&bluff_links, "Dock", "route", "outgoing"));
    let route = graph_root(&app, token, card(&all, "East footpath").unwrap()).await;
    assert!(linked(&route, "Dock", "route-origin", "outgoing"));
    assert!(linked(&route, "Bluff", "route-destination", "outgoing"));
    for data in [&dock_links, &bluff_links, &route] {
        for raw in [DOCK, BLUFF, ROUTE, &TICK.to_string()] {
            assert!(!data.to_string().contains(raw));
        }
    }
    let scene = graph_root(&app, token, card(&all, "Harbor assessment").unwrap()).await;
    assert!(linked(&scene, "Lantern lens", "scene-object", "outgoing"));
    assert!(!scene.to_string().contains("PRIVATE_SCENE_OBJECT"));
    let disconnected = read(
        &app,
        "choices",
        &[("view", token), ("menu", "record"), ("q", "Loose compass")],
    )
    .await;
    let compass = handle(&disconnected, "Loose compass");
    assert_eq!(graph_root(&app, token, compass).await["items"], json!([]));
    assert_eq!(
        read(&app, "record", &[("view", token), ("record", compass)]).await["record"]["title"],
        "Loose compass"
    );
}

#[tokio::test]
async fn graph_fences_hidden_edge_evidence_and_continuations() {
    let (root, service) = fixture();
    let app = http::router(service);
    let author = read(&app, "context", &[]).await;
    let token = view(&author);
    let all = read(&app, "page", &[("view", token), ("limit", "48")]).await;
    let author_root = card(&all, "Second signal").unwrap();
    let ana = character(&app, token, "Second signal", true).await;
    let character_token = view(&ana);
    let page = read(&app, "page", &[("view", character_token), ("limit", "48")]).await;
    assert!(card(&page, "Old bell").is_some());
    let (_, moments) = named(&app, character_token, "character", "Main").await;
    assert!(
        moments["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["label"] != "Old bell"),
        "an event known now cannot label a destination before it was learned"
    );
    // Both endpoints are independently admitted. Learning the event name
    // does not disclose the participant relation that Ana did not witness.
    assert!(card(&page, "Mina").is_some());
    let late = graph_root(&app, character_token, card(&page, "Second signal").unwrap()).await;
    assert_eq!(late["causalAvailable"], false);
    assert_eq!(late["items"], json!([]));
    let early = graph_root(&app, character_token, card(&page, "First signal").unwrap()).await;
    assert!(linked(&early, "Mina", "event-participant", "outgoing"));
    assert!(!early.to_string().contains("AUTHOR_ROLE_SECRET"));
    let scene = graph_root(
        &app,
        character_token,
        card(&page, "Harbor assessment").unwrap(),
    )
    .await;
    assert!(linked(&scene, "Mina", "scene-participant", "outgoing"));
    assert!(linked(&scene, "Lantern lens", "scene-object", "outgoing"));
    assert!(!scene.to_string().contains("PRIVATE_SCENE_OBJECT"));
    let earlier = character(&app, token, "First signal", true).await;
    let earlier_token = view(&earlier);
    let earlier_page = read(&app, "page", &[("view", earlier_token), ("limit", "48")]).await;
    let earlier_scene = graph_root(
        &app,
        earlier_token,
        card(&earlier_page, "Harbor assessment").unwrap(),
    )
    .await;
    assert!(linked(
        &earlier_scene,
        "Lantern lens",
        "scene-object",
        "outgoing"
    ));
    assert!(!linked(
        &earlier_scene,
        "Mina",
        "scene-participant",
        "outgoing"
    ));
    let dock = graph_root(&app, character_token, card(&page, "Dock").unwrap()).await;
    assert!(!dock.to_string().contains("East footpath"));
    let (transplanted_status, transplanted) = get(
        &app,
        "graph",
        &[("view", character_token), ("root", author_root)],
    )
    .await;
    let (missing_status, missing) = get(
        &app,
        "graph",
        &[("view", character_token), ("root", &"a".repeat(64))],
    )
    .await;
    assert_eq!(transplanted_status, StatusCode::NOT_FOUND);
    assert_eq!(
        (transplanted_status, transplanted),
        (missing_status, missing)
    );

    let first = read(
        &app,
        "graph",
        &[("view", token), ("root", author_root), ("limit", "1")],
    )
    .await;
    let cursor = first["nextCursor"].as_str().unwrap();
    let next = read(
        &app,
        "graph",
        &[
            ("view", token),
            ("root", author_root),
            ("limit", "1"),
            ("cursor", cursor),
        ],
    )
    .await;
    assert_ne!(first["items"], next["items"]);
    for (endpoint, fields) in [
        (
            "graph",
            vec![
                ("view", token),
                ("root", author_root),
                ("limit", "2"),
                ("cursor", cursor),
            ],
        ),
        (
            "graph",
            vec![
                ("view", token),
                ("root", author_root),
                ("limit", "1"),
                ("cursor", cursor),
                ("q", "signal"),
            ],
        ),
        (
            "graph",
            vec![
                ("view", token),
                ("root", card(&all, "Dock").unwrap()),
                ("limit", "1"),
                ("cursor", cursor),
            ],
        ),
        (
            "choices",
            vec![
                ("view", token),
                ("menu", "record"),
                ("limit", "1"),
                ("cursor", cursor),
            ],
        ),
    ] {
        assert_eq!(get(&app, endpoint, &fields).await.0, StatusCode::NOT_FOUND);
    }
    let hidden = root.path().join("story").join(format!("{HIDDEN}.md"));
    let source = fs::read_to_string(&hidden)
        .unwrap()
        .replace("PRIVATE_NAME", "Mina");
    fs::write(hidden, source).unwrap();
    let other = http::router(Service::new(source::load_world(root.path(), None).unwrap()));
    let other_author = read(&other, "context", &[]).await;
    let other_ana = character(&other, view(&other_author), "Second signal", true).await;
    let candidates = read(
        &other,
        "choices",
        &[
            ("view", view(&other_ana)),
            ("menu", "record"),
            ("q", "Mina"),
        ],
    )
    .await;
    assert_eq!(candidates["items"].as_array().unwrap().len(), 1);
    assert!(handle(&candidates, "Mina").len() == 64);
    let other_page = read(
        &other,
        "page",
        &[("view", view(&other_ana)), ("limit", "48")],
    )
    .await;
    let other_late = graph_root(
        &other,
        view(&other_ana),
        card(&other_page, "Second signal").unwrap(),
    )
    .await;
    assert_eq!(other_late["items"], late["items"]);
}

#[tokio::test]
async fn lore_edges_require_exact_validity_and_independently_admitted_endpoints() {
    let (_root, service) = fixture();
    let native = wedl::lore::links(service.world(), DOCK, None, "public", "ordinary", 48).unwrap();
    assert_eq!(native["items"].as_array().unwrap().len(), 1);
    assert_eq!(native["items"][0]["conceptId"], LEDGER);
    let app = http::router(service);
    let author = read(&app, "context", &[]).await;
    let all = read(&app, "page", &[("view", view(&author)), ("limit", "48")]).await;
    let links = graph_root(&app, view(&author), card(&all, "Log excerpt").unwrap()).await;
    assert!(linked(&links, "Signal ledger", "lore", "outgoing"));
    assert!(!links.to_string().contains("PRIVATE_LORE"));
    let dock = graph_root(&app, view(&author), card(&all, "Dock").unwrap()).await;
    let ledger = graph_root(&app, view(&author), card(&all, "Signal ledger").unwrap()).await;
    assert!(linked(&dock, "Signal ledger", "lore", "outgoing"));
    assert!(linked(&ledger, "Dock", "lore", "incoming"));
    assert!(!dock.to_string().contains("PRIVATE_LORE"));
    for (name, available) in [("First signal", false), ("Second signal", true)] {
        let character = character(&app, view(&author), name, false).await;
        let token = view(&character);
        let directory = read(&app, "page", &[("view", token), ("limit", "48")]).await;
        let excerpt = card(&directory, "Log excerpt").unwrap();
        let ledger = card(&directory, "Signal ledger").unwrap();
        let from = graph_root(&app, token, excerpt).await;
        let to = graph_root(&app, token, ledger).await;
        assert_eq!(
            linked(&from, "Signal ledger", "lore", "outgoing"),
            available
        );
        assert_eq!(linked(&to, "Log excerpt", "lore", "incoming"), available);
        assert!(!linked(&to, "Dock", "lore", "incoming"));
        assert!(!from.to_string().contains(HIDDEN_LORE));
        assert!(!from.to_string().contains("PRIVATE_LORE"));
    }
    let enriched = character(&app, view(&author), "Second signal", true).await;
    let token = view(&enriched);
    let admitted = read(&app, "page", &[("view", token), ("limit", "48")]).await;
    let dock = graph_root(&app, token, card(&admitted, "Dock").unwrap()).await;
    let ledger = graph_root(&app, token, card(&admitted, "Signal ledger").unwrap()).await;
    assert!(linked(&dock, "Signal ledger", "lore", "outgoing"));
    assert!(linked(&ledger, "Dock", "lore", "incoming"));
    assert!(!dock.to_string().contains("PRIVATE_LORE"));
}
