//! Spatial history reuses reviewed single-record publication without changing static source.
use serde_json::Value as Json;
use serde_yaml::Value as Yaml;
use std::{fs, process::Command};
use wedl::{
    Error, authoring, character, geometry,
    model::{Record, StoryTime, World},
    record_edit, source, spatial, spatial_history, validation,
};

const HARBOR: &str = "loc_00000000000000000000000001";
const LANDING: &str = "location:harbor/landing";
const ANA: &str = "char_00000000000000000000000001";
const MAP: &str = "map:harbor";
const ROUTE: &str = "route:harbor/steps";
const FIRST: &str = "spatialhistory_00000000000000000000000001";
const CORRECTION: &str = "spatialhistory_00000000000000000000000002";
const REQUESTS: [&str; 6] = [
    include_str!("../examples/authoring/58-landing-placement-history.yaml"),
    include_str!("../examples/authoring/59-landing-history-correction.yaml"),
    include_str!("../examples/authoring/60-landing-parentage-history.yaml"),
    include_str!("../examples/authoring/61-route-history.yaml"),
    include_str!("../examples/authoring/62-route-history-correction.yaml"),
    include_str!("../examples/authoring/63-clear-landing-history.yaml"),
];
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn unchanged(world: &World) {
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
}
fn retained(before: &Record, after: &Record) {
    assert_eq!(before.body, after.body);
    for (key, value) in before.frontmatter.as_mapping().unwrap() {
        if key.as_str() != Some("spatial_history") {
            assert_eq!(after.frontmatter.get(key), Some(value), "{key:?}");
        }
    }
}
fn save(world: &World, target: Option<&str>, request: &str) -> World {
    let preview = match target {
        Some(id) => authoring::preview_record_update(world, id, request, &world.fingerprint),
        None => authoring::preview_record_create(world, request, &world.fingerprint),
    }
    .unwrap();
    assert_eq!(preview["canApply"], true, "{preview}");
    unchanged(world);
    let hash = preview["afterSha256"].as_str().unwrap();
    match target {
        Some(id) => authoring::apply_record_update(world, id, request, &world.fingerprint, hash),
        None => authoring::apply_record_create(world, request, &world.fingerprint, hash),
    }
    .unwrap();
    let saved = source::load_world(&world.root, None).unwrap();
    assert_eq!(saved.fingerprint, preview["candidateFingerprint"]);
    assert_eq!(
        fs::read(world.root.join(preview["sourcePath"].as_str().unwrap())).unwrap(),
        preview["afterSource"].as_str().unwrap().as_bytes()
    );
    let report = validation::validate_world(&saved);
    assert!(report.valid, "{:?}", report.diagnostics);
    saved
}
fn refused(world: &World, target: &str, request: &str) {
    let preview =
        authoring::preview_record_update(world, target, request, &world.fingerprint).unwrap();
    assert_eq!(preview["canApply"], false, "{request}: {preview}");
    let result = authoring::apply_record_update(
        world,
        target,
        request,
        &world.fingerprint,
        preview["afterSha256"].as_str().unwrap(),
    );
    assert!(matches!(result, Err(Error::Validation(_))), "{result:?}");
    unchanged(world);
}
fn fixture() -> (tempfile::TempDir, World) {
    let dir = tempfile::tempdir().unwrap();
    let mut world =
        source::init_world(&dir.path().join("world"), "Reviewed spatial history").unwrap();
    let path = world.root.join("story/world.md");
    fs::write(
        &path,
        fs::read_to_string(&path).unwrap().replace(
            "capabilities: []",
            "capabilities: [spatial-core-v1, geometry-v1, route-v1, spatial-history-v1]",
        ),
    )
    .unwrap();
    world = source::load_world(&world.root, None).unwrap();
    for request in [
        include_str!("../examples/authoring/01-location.yaml"),
        include_str!("../examples/authoring/02-character.yaml"),
        include_str!("../examples/authoring/04-event.yaml"),
        include_str!("../examples/authoring/05-knowledge.yaml"),
        include_str!("../examples/authoring/11-landing.yaml"),
        include_str!("../examples/authoring/12-route.yaml"),
        include_str!("../examples/authoring/15-map.yaml"),
    ] {
        world = save(&world, None, request);
    }
    for (id, request) in [
        (
            HARBOR,
            include_str!("../examples/authoring/16-harbor-placement.yaml"),
        ),
        (
            LANDING,
            include_str!("../examples/authoring/17-landing-placement.yaml"),
        ),
        (
            MAP,
            include_str!("../examples/authoring/18-map-bounds.yaml"),
        ),
    ] {
        world = save(&world, Some(id), request);
    }
    (dir, world)
}
fn ids(value: &Json) -> Vec<&str> {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["locationId"].as_str().unwrap())
        .collect()
}
fn request_with_value(request: &str, index: usize, value: Yaml) -> String {
    let mut parsed: Yaml = serde_yaml::from_str(request).unwrap();
    parsed["spatial_history"][index]["value"] = value;
    serde_yaml::to_string(&parsed).unwrap()
}
fn yaml(value: &str) -> Yaml {
    serde_yaml::from_str(value).unwrap()
}

#[test]
fn location_examples_retain_static_fields_corrections_and_exact_clear_behavior() {
    let (_dir, mut world) = fixture();
    let original = world.records[LANDING].clone();
    for request in &REQUESTS[..3] {
        world = save(&world, Some(LANDING), request);
        retained(&original, &world.records[LANDING]);
    }
    let all = spatial_history::history(&world, LANDING, None, 100).unwrap();
    assert_eq!(all["count"], 3);
    assert_eq!(all["items"][0]["memberId"], FIRST);
    assert_eq!(all["items"][1]["replaces"], FIRST);
    let dated = spatial_history::history(&world, LANDING, Some(&at(20, 0)), 100).unwrap();
    assert_eq!(dated["count"], 2);
    assert_eq!(dated["items"][0]["memberId"], CORRECTION);
    assert!(dated["items"][1]["value"]["parent_id"].is_null());
    assert_eq!(dated["items"][0]["citations"].as_array().unwrap().len(), 2);
    for point in [at(20, 0), at(29, 0)] {
        let nearby = geometry::nearby_at(&world, MAP, &[9.0, 10.0], 0.0, &point, 100).unwrap();
        assert!(ids(&nearby).contains(&LANDING));
    }
    assert!(
        !ids(&geometry::nearby_at(&world, MAP, &[9.0, 10.0], 0.0, &at(29, 1), 100).unwrap())
            .contains(&LANDING)
    );
    assert!(
        !ids(&geometry::nearby(&world, MAP, [9.0, 10.0], 0.0, 100).unwrap()).contains(&LANDING)
    );
    let noop = record_edit::update(&world, LANDING, "kind: location\n").unwrap();
    assert_eq!(noop.raw_bytes, world.records[LANDING].raw_bytes);
    world = save(&world, Some(LANDING), REQUESTS[5]);
    retained(&original, &world.records[LANDING]);
    assert_eq!(
        world.records[LANDING]
            .list("spatial_history")
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        spatial_history::history(&world, LANDING, Some(&at(25, 0)), 100).unwrap()["count"],
        0
    );
    assert!(
        ids(&geometry::nearby_at(&world, MAP, &[3.0, 4.0], 0.0, &at(25, 0), 100).unwrap())
            .contains(&LANDING)
    );
}

#[test]
fn route_examples_keep_independent_values_and_static_path_semantics() {
    let (_dir, mut world) = fixture();
    let original = world.records[ROUTE].clone();
    for request in &REQUESTS[3..5] {
        world = save(&world, Some(ROUTE), request);
        retained(&original, &world.records[ROUTE]);
    }
    let dated = spatial::route_info(&world, ROUTE, Some(&at(25, 0))).unwrap();
    assert_eq!(dated["availability"], "closed");
    assert_eq!(dated["routeDistance"]["value"], "25");
    assert_eq!(dated["duration"]["value"], "60");
    assert_eq!(dated["travelCost"]["value"], "7");
    assert_eq!(
        dated["fieldCitations"]["travelCost"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let expired = spatial::route_info(&world, ROUTE, Some(&at(29, 1))).unwrap();
    assert_eq!(expired["availability"], "open");
    assert_eq!(expired["duration"]["value"], "45");
    assert!(expired["travelCost"].is_null());
    assert!(
        expired["fieldCitations"]["travelCost"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let static_route = spatial::route_info(&world, ROUTE, None).unwrap();
    assert_eq!(static_route["availability"], "open");
    assert_eq!(static_route["routeDistance"]["value"], "30");
    let path = spatial::path(
        &world,
        HARBOR,
        LANDING,
        None,
        "duration",
        Some("seconds"),
        100,
    )
    .unwrap();
    assert_eq!(path["found"], true);
    assert_eq!(path["total"]["value"], "45");
}

#[test]
fn invalid_references_dimensions_capabilities_and_owner_aspects_cannot_publish() {
    let (_dir, world) = fixture();
    for value in [
        "{map_id: map:missing, geometry: {kind: point, coordinates: [8,9]}}",
        "{map_id: map:harbor, geometry: {kind: point, coordinates: [8,9,1]}}",
        "{map_id: map:harbor, geometry: {kind: point, coordinates: [80,90]}}",
    ] {
        refused(
            &world,
            LANDING,
            &request_with_value(REQUESTS[0], 0, yaml(value)),
        );
    }
    let self_parent =
        request_with_value(REQUESTS[2], 2, yaml(&format!("{{parent_id: '{LANDING}'}}")));
    refused(&world, LANDING, &self_parent);
    refused(
        &world,
        LANDING,
        &REQUESTS[3].replacen("kind: route", "kind: location", 1),
    );
    let mut conflict: Yaml = serde_yaml::from_str(REQUESTS[1]).unwrap();
    conflict["spatial_history"][1]
        .as_mapping_mut()
        .unwrap()
        .remove(&Yaml::String("replaces".into()));
    refused(&world, LANDING, &serde_yaml::to_string(&conflict).unwrap());
    let mut wrong_target: Yaml = serde_yaml::from_str(REQUESTS[1]).unwrap();
    wrong_target["spatial_history"][1]["replaces"] =
        yaml("spatialhistory_00000000000000000000000999");
    refused(
        &world,
        LANDING,
        &serde_yaml::to_string(&wrong_target).unwrap(),
    );
    let path = world.root.join("story/world.md");
    fs::write(
        &path,
        fs::read_to_string(&path)
            .unwrap()
            .replace(", spatial-history-v1", ""),
    )
    .unwrap();
    let without_capability = source::load_world(&world.root, None).unwrap();
    assert!(validation::validate_world(&without_capability).valid);
    refused(&without_capability, LANDING, REQUESTS[0]);
}

#[test]
fn publication_refuses_a_cycle_exposed_only_after_history_expires() {
    let (_dir, world) = fixture();
    let world = save(&world, Some(LANDING), REQUESTS[2]);
    let parent_request = format!(
        "kind: location\nspatial_history:\n  - id: spatialhistory_00000000000000000000000009\n    aspect: parentage\n    valid:\n      kind: interval\n      timeline: main\n      start: {{timeline: main, tick: 20, order: 0}}\n      end: {{timeline: main, tick: 29, order: 0}}\n    value: {{parent_id: '{LANDING}'}}\n"
    );
    let world = save(&world, Some(HARBOR), &parent_request);
    let mut shorter: Yaml = serde_yaml::from_str(REQUESTS[2]).unwrap();
    shorter["spatial_history"][2]["valid"]["end"]["tick"] = yaml("28");
    refused(&world, LANDING, &serde_yaml::to_string(&shorter).unwrap());
    assert_eq!(
        spatial_history::history(&world, HARBOR, Some(&at(28, 1)), 100).unwrap()["count"],
        1
    );
}

#[test]
fn malformed_history_requests_are_closed_and_never_mutate_source() {
    let (_dir, world) = fixture();
    let invalid = [
        "kind: location\nspatial_history: null\n".to_owned(),
        "kind: location\nspatial_history: {}\n".to_owned(),
        REQUESTS[0].replace(FIRST, "spatialhistory_bad"),
        REQUESTS[0].replace("aspect: placement", "aspect: placement\n    chronology: []"),
        REQUESTS[0].replace("start: {timeline: main, tick: 20, order: 0}", "start: null"),
        REQUESTS[0].replace("coordinates: [8, 9]", "coordinates: !position [8, 9]"),
        REQUESTS[0].replace("kind: interval", "kind: interval\n      unknown: true"),
        REQUESTS[0].replace("tick: 20, order: 0", "tick: 20.5, order: 0"),
    ];
    for request in invalid {
        assert!(
            matches!(
                record_edit::update(&world, LANDING, &request),
                Err(Error::InvalidInput(_))
            ),
            "{request}"
        );
        unchanged(&world);
    }
    assert!(record_edit::update(&world, ANA, "kind: character\nspatial_history: []\n").is_err());
    // Null is valid only in the declared parent payload, not as a deletion operator.
    assert!(record_edit::update(&world, LANDING, REQUESTS[2]).is_ok());
}

#[test]
fn reviewed_spatial_histories_never_admit_character_evidence() {
    let (_dir, before) = fixture();
    let mut after = before.clone();
    for (target, request) in [(LANDING, REQUESTS[2]), (ROUTE, REQUESTS[4])] {
        after = save(&after, Some(target), request);
    }
    assert_ne!(before.fingerprint, after.fingerprint);
    for point in [at(19, 0), at(25, 0), at(30, 0)] {
        assert_eq!(
            character::context(&before, ANA, &point, 100_000).unwrap(),
            character::context(&after, ANA, &point, 100_000).unwrap()
        );
        for query in ["lamp", "landing", "coin", "spatialhistory"] {
            assert_eq!(
                character::search(&before, ANA, query, &point, 20).unwrap(),
                character::search(&after, ANA, query, &point, 20).unwrap()
            );
        }
        assert_eq!(
            character::context(&before, ANA, &point, 1)
                .unwrap_err()
                .json(),
            character::context(&after, ANA, &point, 1)
                .unwrap_err()
                .json()
        );
    }
}

#[test]
fn all_six_numbered_requests_preview_and_save_through_cli_with_exact_bytes() {
    let (_dir, mut world) = fixture();
    for (index, request) in REQUESTS.iter().enumerate() {
        let target = if index == 3 || index == 4 {
            ROUTE
        } else {
            LANDING
        };
        let request_path = world
            .root
            .parent()
            .unwrap()
            .join(format!("request-{}.yaml", index + 58));
        fs::write(&request_path, request).unwrap();
        let args = [
            "--compact",
            "--repo",
            world.root.to_str().unwrap(),
            "record",
            "update",
            target,
            "--file",
            request_path.to_str().unwrap(),
            "--expected-fingerprint",
            &world.fingerprint,
        ];
        let preview = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
            .args(args)
            .output()
            .unwrap();
        assert!(
            preview.status.success(),
            "{}",
            String::from_utf8_lossy(&preview.stderr)
        );
        let preview: Json = serde_json::from_slice(&preview.stdout).unwrap();
        let preview = &preview["result"];
        assert_eq!(preview["canApply"], true, "{preview}");
        unchanged(&world);
        let applied = Command::new(env!("CARGO_BIN_EXE_wedl-rs"))
            .args(args)
            .args([
                "--apply",
                "--expected-after-sha256",
                preview["afterSha256"].as_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            applied.status.success(),
            "{}",
            String::from_utf8_lossy(&applied.stderr)
        );
        assert_eq!(
            fs::read(world.root.join(preview["sourcePath"].as_str().unwrap())).unwrap(),
            preview["afterSource"].as_str().unwrap().as_bytes()
        );
        world = source::load_world(&world.root, None).unwrap();
        assert_eq!(world.fingerprint, preview["candidateFingerprint"]);
    }
    assert!(validation::validate_world(&world).valid);
    assert!(
        world.records[LANDING]
            .list("spatial_history")
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        world.records[ROUTE].list("spatial_history").unwrap().len(),
        5
    );
}
