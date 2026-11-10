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

const FIXTURE: &str = include_str!("fixtures/compendium-disclosure.json");
const ID_ADA: &str = "char_00000000000000000000000001";

fn write_record(root: &Path, mut record: Value, tick: &str, variant: &str, body: &str) {
    let name = record["id"].as_str().unwrap().replace([':', '/'], "-");
    let override_body = record.as_object_mut().unwrap().remove("$body");
    let body = override_body
        .as_ref()
        .and_then(Value::as_str)
        .unwrap_or(body);
    record["schema"] = json!("wedl/v1");
    let text = serde_json::to_string(&record)
        .unwrap()
        .replace("\"$tick\"", tick)
        .replace("$withheldEdge", &format!("matrix-private-edge-{variant}"));
    fs::write(
        root.join(format!("{name}.md")),
        format!("---\n{text}\n---\n{body}\n"),
    )
    .unwrap();
}

fn fixture(data: &Value, variant: &Value) -> (TempDir, Service, Vec<String>) {
    let directory = tempfile::tempdir().unwrap();
    let story = directory.path().join("story");
    fs::create_dir(&story).unwrap();
    let tick = data["tick"].as_str().unwrap();
    let name = variant["name"].as_str().unwrap();
    let mut forbidden = vec![
        tick.to_owned(),
        "location:matrix-hidden".into(),
        "map:matrix-hidden".into(),
        "story/location-matrix-hidden.md".into(),
        "story/map-matrix-hidden.md".into(),
        "MATRIX_AUTHOR_DOCK_PROSE".into(),
    ];
    for record in data["records"].as_array().unwrap() {
        let id = record["id"].as_str().unwrap();
        forbidden.push(id.into());
        forbidden.push(format!("story/{}.md", id.replace([':', '/'], "-")));
        write_record(&story, record.clone(), tick, name, "");
    }
    for (index, evidence) in data["knowledge"].as_array().unwrap().iter().enumerate() {
        let title = evidence[0].as_str().unwrap();
        let id = format!("know_{:026}", index + 1);
        forbidden.push(id.clone());
        forbidden.push(format!("story/{id}.md"));
        write_record(
            &story,
            json!({
                "kind":"knowledge","id":id,"title":"Private note",
                "knower":ID_ADA,"claim":{"key":format!("known-{index}"),"statement":format!("I know {title}."),
                    "truth":"true","object":{"entity":evidence[1]}},
                "transitions":[{"id":"observed","time":{"timeline":"main","tick":"$tick","order":evidence[2]},
                    "state":"accepted","acquisition":"observed","confidence":1}]
            }),
            tick,
            name,
            "",
        );
    }
    for scope in ["MAP", "ALIAS", "PROSE"] {
        forbidden.push(format!("MATRIX_PRIVATE_{scope}_{name}"));
    }
    forbidden.push(format!("matrix-private-edge-{name}"));
    forbidden.extend(
        variant["point"]
            .as_array()
            .unwrap()
            .iter()
            .map(Value::to_string),
    );
    write_record(
        &story,
        json!({
            "kind":"map","id":"map:matrix-hidden","title":format!("MATRIX_PRIVATE_MAP_{name}"),
            "audience":[],"crs":"local-planar:hidden","axis_order":["east","north"],"unit":"pace",
            "bounds":{"min":[0,0],"max":[variant["bound"],variant["bound"]]}
        }),
        tick,
        name,
        "",
    );
    write_record(
        &story,
        json!({
            "kind":"location","id":"location:matrix-hidden","title":"Dock",
            "aliases":[format!("MATRIX_PRIVATE_ALIAS_{name}")],"audience":[],
            "spatial":{"map_id":"map:matrix-hidden","geometry":{"kind":"point","coordinates":variant["point"]}}
        }),
        tick,
        name,
        &format!("MATRIX_PRIVATE_PROSE_{name}"),
    );
    let world = source::load_world(directory.path(), None).unwrap();
    let checked = validation::validate_world(&world);
    assert!(checked.valid, "{checked:#?}");
    forbidden.push(world.fingerprint.clone());
    if let Some(raw) = world.fingerprint.strip_prefix("sha256:") {
        forbidden.push(raw.into());
    }
    (directory, Service::new(world), forbidden)
}

async fn read(app: &Router, endpoint: &str, fields: &[(&str, &str)]) -> (StatusCode, Value) {
    let query = fields
        .iter()
        .map(|(key, value)| {
            assert!(
                value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            );
            format!("{key}={value}")
        })
        .collect::<Vec<_>>()
        .join("&");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/compendium/{endpoint}?{query}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["referrer-policy"], "no-referrer");
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

async fn ok(app: &Router, endpoint: &str, fields: &[(&str, &str)]) -> Value {
    let (status, value) = read(app, endpoint, fields).await;
    assert_eq!(status, StatusCode::OK, "{endpoint}: {value}");
    value
}

fn token(value: &Value) -> &str {
    value["viewToken"].as_str().unwrap()
}

fn choice<'a>(value: &'a Value, label: &str) -> &'a str {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["label"] == label)
        .and_then(|item| item["handle"].as_str())
        .unwrap()
}

fn shape(value: &Value, fields: &[&str]) {
    let object = value.as_object().unwrap();
    assert_eq!(object.len(), fields.len());
    assert!(fields.iter().all(|field| object.contains_key(*field)));
}

fn menu_shape(menu: &Value) {
    shape(menu, &["viewToken", "items", "nextCursor"]);
    for item in menu["items"].as_array().unwrap() {
        shape(item, &["handle", "label", "kind", "descriptor"]);
    }
}

fn card_shape(card: &Value) {
    shape(card, &["handle", "href", "title", "kind", "kindLabel"]);
}

fn record_shape(detail: &Value) {
    shape(detail, &["viewToken", "record", "sections"]);
    card_shape(&detail["record"]);
    for section in detail["sections"].as_array().unwrap() {
        shape(section, &["heading", "text"]);
    }
}

fn locations_shape(locations: &Value) {
    shape(locations, &["items", "nextCursor"]);
    for item in locations["items"].as_array().unwrap() {
        shape(item, &["title", "kindLabel", "href", "details"]);
        for detail in item["details"].as_array().unwrap() {
            shape(detail, &["label", "value"]);
        }
    }
}

fn scan(value: &Value, forbidden: &[String]) {
    let text = value.to_string();
    for sentinel in forbidden {
        assert!(
            !text.contains(sentinel),
            "scoped response contained a forbidden fixture token"
        );
    }
}

fn address<'a>(href: &'a str, forbidden: &[String]) -> &'a str {
    let address = href.strip_prefix("/compendium/v1/").unwrap();
    assert_eq!(address.len(), 1592);
    let decoded = hex::decode(address).unwrap();
    let decoded = String::from_utf8_lossy(&decoded);
    for sentinel in forbidden {
        assert!(
            !href.contains(sentinel) && !decoded.contains(sentinel),
            "bookmark contained a forbidden fixture token"
        );
    }
    address
}

fn stable(value: &Value) -> Value {
    match value {
        Value::Array(values) => json!(values.iter().map(stable).collect::<Vec<_>>()),
        Value::Object(fields) => fields
            .iter()
            .map(|(key, value)| {
                let opaque = matches!(
                    key.as_str(),
                    "viewToken" | "handle" | "href" | "nextCursor" | "directory" | "graph" | "map"
                ) && (value.is_string() || value.is_null());
                (
                    key.clone(),
                    if opaque {
                        json!(!value.is_null())
                    } else {
                        stable(value)
                    },
                )
            })
            .collect::<serde_json::Map<_, _>>()
            .into(),
        _ => value.clone(),
    }
}

async fn resolve_links(
    app: &Router,
    rows: &[Value],
    title_field: &str,
    context: &Value,
    forbidden: &[String],
) -> Vec<Value> {
    let mut destinations = Vec::new();
    for row in rows {
        let Some(href) = row["href"].as_str() else {
            assert!(row["handle"].is_null());
            destinations.push(Value::Null);
            continue;
        };
        let link = address(href, forbidden);
        let restored = ok(app, "bookmark", &[("address", link)]).await;
        assert_eq!(restored["destination"]["kind"], "record");
        let handle = restored["destination"]["handle"].as_str().unwrap();
        let detail = ok(
            app,
            "record",
            &[("view", token(&restored)), ("record", handle)],
        )
        .await;
        record_shape(&detail);
        assert_eq!(detail["record"]["title"], row[title_field]);
        assert_eq!(restored["labels"], context["labels"]);
        scan(&detail, forbidden);
        if let Some(handle) = row["handle"].as_str() {
            let direct = ok(
                app,
                "record",
                &[("view", token(context)), ("record", handle)],
            )
            .await;
            record_shape(&direct);
            scan(&direct, forbidden);
            assert_eq!(stable(&direct), stable(&detail));
        }
        destinations.push(json!([stable(&restored["labels"]), stable(&detail)]));
    }
    destinations
}

async fn moment_choices(app: &Router, view: &str, mode: &str, label: &str) -> (String, Value) {
    let timelines = ok(
        app,
        "choices",
        &[("view", view), ("menu", "timeline"), ("mode", mode)],
    )
    .await;
    menu_shape(&timelines);
    let timeline = choice(&timelines, label).to_owned();
    let moments = ok(
        app,
        "choices",
        &[
            ("view", view),
            ("menu", "moment"),
            ("mode", mode),
            ("timeline", &timeline),
        ],
    )
    .await;
    menu_shape(&moments);
    (timeline, moments)
}

fn names(page: &Value) -> Vec<&str> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["title"].as_str().unwrap())
        .collect()
}

async fn observations(app: Router, forbidden: &[String]) -> Value {
    let author = ok(&app, "context", &[]).await;
    shape(
        &author,
        &[
            "viewToken",
            "labels",
            "capabilities",
            "href",
            "destination",
            "routes",
        ],
    );
    let author_token = token(&author);
    let all = ok(&app, "page", &[("view", author_token), ("limit", "48")]).await;
    assert!(names(&all).contains(&"Second tide"));
    let (author_timeline, author_moments) =
        moment_choices(&app, author_token, "as-of", "Harbor chronology").await;
    let first = ok(
        &app,
        "context",
        &[
            ("view", author_token),
            ("mode", "as-of"),
            ("timeline", &author_timeline),
            ("moment", choice(&author_moments, "First tide")),
        ],
    )
    .await;
    let first_author = ok(&app, "page", &[("view", token(&first)), ("limit", "48")]).await;
    assert!(!names(&first_author).contains(&"Second tide"));
    let author_map = ok(&app, "map", &[("view", author_token)]).await;
    shape(
        &author_map,
        &[
            "viewToken",
            "status",
            "viewport",
            "map",
            "choices",
            "features",
            "locations",
            "plot",
        ],
    );
    assert_eq!(author_map["map"]["label"], "Harbor grid");
    scan(&author_map, forbidden);
    shape(&author_map["choices"], &["items", "nextCursor"]);
    for item in author_map["choices"]["items"].as_array().unwrap() {
        shape(item, &["handle", "label"]);
    }
    let features = author_map["features"].as_array().unwrap();
    for feature in features {
        shape(feature, &["title", "href", "drawing"]);
    }
    locations_shape(&author_map["locations"]);
    let author_tokens = forbidden
        .iter()
        .filter(|token| token.as_str() != "MATRIX_AUTHOR_DOCK_PROSE")
        .cloned()
        .collect::<Vec<_>>();
    let map_targets = resolve_links(&app, features, "title", &author, &author_tokens).await;
    let map_list_targets = resolve_links(
        &app,
        author_map["locations"]["items"].as_array().unwrap(),
        "title",
        &author,
        &author_tokens,
    )
    .await;

    let (timeline, moments) =
        moment_choices(&app, author_token, "character", "Harbor chronology").await;
    let (other, other_moments) =
        moment_choices(&app, author_token, "character", "Alternate chronology").await;
    assert_ne!(
        choice(&moments, "First tide"),
        choice(&other_moments, "First tide")
    );
    let (mut frames, mut captured_scene) = (Vec::new(), None);
    for (label, use_scene) in [
        ("First tide", false),
        ("First tide", true),
        ("Second tide", false),
    ] {
        let fields = [
            ("view", author_token),
            ("mode", "character"),
            ("timeline", timeline.as_str()),
            ("moment", choice(&moments, label)),
        ];
        let mut selectors = fields.to_vec();
        selectors.push(("menu", "viewpoint"));
        let views = ok(&app, "choices", &selectors).await;
        menu_shape(&views);
        let mut fields = fields.to_vec();
        fields.push(("viewpoint", choice(&views, "Ada River")));
        let mut selectors = fields.clone();
        selectors.push(("menu", "scene"));
        let scenes = ok(&app, "choices", &selectors).await;
        menu_shape(&scenes);
        if use_scene {
            captured_scene = Some(choice(&scenes, "Night watch").to_owned());
            fields.push(("scene", captured_scene.as_deref().unwrap()));
        }
        if label == "Second tide" {
            assert_eq!(scenes["items"], json!([]));
            let mut expired = fields.clone();
            expired.push(("scene", captured_scene.as_deref().unwrap()));
            let mut wrong = fields.clone();
            wrong[2].1 = &other;
            let mut forged = fields.clone();
            let fake = "f".repeat(64);
            forged.push(("scene", &fake));
            let unavailable = read(&app, "context", &expired).await;
            assert_eq!(unavailable.0, StatusCode::NOT_FOUND);
            assert_eq!(read(&app, "context", &wrong).await, unavailable);
            assert_eq!(read(&app, "context", &forged).await, unavailable);
            scan(&unavailable.1, forbidden);
        }
        let active = ok(&app, "context", &fields).await;
        let view = token(&active);
        let page = ok(&app, "page", &[("view", view), ("limit", "48")]).await;
        shape(
            &page,
            &["viewToken", "items", "facets", "total", "nextCursor"],
        );
        let visible = names(&page);
        assert_eq!(visible.contains(&"Bea Finch"), label == "Second tide");
        assert_eq!(visible.contains(&"Bronze token"), label == "Second tide");
        assert_eq!(visible.contains(&"Signal lantern"), use_scene);
        assert!(visible.contains(&"Loose compass"));
        let cards = page["items"].as_array().unwrap();
        for card in cards {
            card_shape(card);
        }
        let dock = cards.iter().find(|row| row["title"] == "Dock").unwrap()["handle"]
            .as_str()
            .unwrap();
        let graph = ok(&app, "graph", &[("view", view), ("root", dock)]).await;
        shape(
            &graph,
            &["viewToken", "items", "nextCursor", "causalAvailable"],
        );
        let edges = graph["items"].as_array().unwrap();
        for edge in edges {
            shape(
                edge,
                &[
                    "handle",
                    "href",
                    "label",
                    "kind",
                    "descriptor",
                    "relation",
                    "direction",
                ],
            );
        }
        assert_eq!(
            graph["items"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| row["label"] == "Pier ledger")
                .count(),
            1
        );
        let locations = ok(&app, "locations", &[("view", view), ("limit", "2")]).await;
        shape(&locations, &["viewToken", "locations"]);
        locations_shape(&locations["locations"]);
        assert_eq!(
            read(&app, "map", &[("view", view)]).await.0,
            StatusCode::NOT_FOUND
        );
        let cursor = locations["locations"]["nextCursor"].as_str().unwrap();
        let continued = ok(
            &app,
            "locations",
            &[("view", view), ("limit", "2"), ("cursor", cursor)],
        )
        .await;
        shape(&continued, &["viewToken", "locations"]);
        locations_shape(&continued["locations"]);
        assert_eq!(
            read(
                &app,
                "graph",
                &[("view", view), ("root", dock), ("cursor", cursor)]
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
        for value in [&active, &page, &graph, &locations, &continued] {
            scan(value, forbidden);
        }
        let directory_targets = resolve_links(&app, cards, "title", &active, forbidden).await;
        let graph_targets = resolve_links(&app, edges, "label", &active, forbidden).await;
        let location_targets = resolve_links(
            &app,
            locations["locations"]["items"].as_array().unwrap(),
            "title",
            &active,
            forbidden,
        )
        .await;
        let continued_targets = resolve_links(
            &app,
            continued["locations"]["items"].as_array().unwrap(),
            "title",
            &active,
            forbidden,
        )
        .await;
        frames.push(json!([
            stable(&active),
            stable(&scenes),
            stable(&page),
            stable(&graph),
            stable(&locations),
            stable(&continued),
            directory_targets,
            graph_targets,
            location_targets,
            continued_targets
        ]));
    }
    json!([stable(&author_map), map_targets, map_list_targets, frames])
}

#[tokio::test]
async fn withheld_source_preserves_admitted_http_frames_shapes_and_destinations() {
    let data: Value = serde_json::from_str(FIXTURE).unwrap();
    let mut pair = Vec::new();
    for variant in data["variants"].as_array().unwrap() {
        let (_fixture, service, forbidden) = fixture(&data, variant);
        pair.push(observations(http::router(service), &forbidden).await);
    }
    assert_eq!(pair[0], pair[1]);
}
