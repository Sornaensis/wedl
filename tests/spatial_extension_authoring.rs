//! Geometry, explicit connections and overlays share the existing reviewed writer.
use serde_json::Value as Json;
use serde_yaml::Value as Yaml;
use std::{fs, path::Path, process::Command};
use wedl::{
    Error, authoring, character, chronology, geometry,
    model::{StoryTime, World},
    record_edit, source, spatial, validation,
};

const HARBOR: &str = "loc_00000000000000000000000001";
const ANA: &str = "char_00000000000000000000000001";
const MAP: &str = "map:harbor";
const LEVELS: &str = "map:harbor/levels";
const COAST: &str = "map:harbor/coast";
const TOWER: &str = "location:harbor/watchtower";
const ANCHOR: &str = "anchor:harbor/survey";
const PORTAL: &str = "portal:harbor/watchtower";
const OVERLAY: &str = "overlay:harbor/flood-warning";
const CREATES: [&str; 11] = [
    include_str!("../examples/authoring/45-boardwalk-line.yaml"),
    include_str!("../examples/authoring/46-courtyard-polygon.yaml"),
    include_str!("../examples/authoring/47-level-map.yaml"),
    include_str!("../examples/authoring/48-watchtower-point.yaml"),
    include_str!("../examples/authoring/49-geographic-map.yaml"),
    include_str!("../examples/authoring/50-geographic-point.yaml"),
    include_str!("../examples/authoring/51-survey-anchor.yaml"),
    include_str!("../examples/authoring/52-watchtower-portal.yaml"),
    include_str!("../examples/authoring/53-position-portal.yaml"),
    include_str!("../examples/authoring/54-static-overlay.yaml"),
    include_str!("../examples/authoring/55-dated-overlay.yaml"),
];
const RETIRE: &str = include_str!("../examples/authoring/56-retire-portal.yaml");
const WINDOW: &str = include_str!("../examples/authoring/57-overlay-window.yaml");

fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn unchanged(world: &World) {
    assert_eq!(
        source::load_world(&world.root, None).unwrap().fingerprint,
        world.fingerprint
    );
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
fn refused(world: &World, target: Option<&str>, request: &str) {
    let preview = match target {
        Some(id) => authoring::preview_record_update(world, id, request, &world.fingerprint),
        None => authoring::preview_record_create(world, request, &world.fingerprint),
    }
    .unwrap();
    assert_eq!(preview["canApply"], false, "{request}: {preview}");
    let hash = preview["afterSha256"].as_str().unwrap();
    let result = match target {
        Some(id) => authoring::apply_record_update(world, id, request, &world.fingerprint, hash),
        None => authoring::apply_record_create(world, request, &world.fingerprint, hash),
    };
    assert!(matches!(result, Err(Error::Validation(_))), "{result:?}");
    unchanged(world);
}
fn fixture() -> (tempfile::TempDir, World) {
    let dir = tempfile::tempdir().unwrap();
    let mut world =
        source::init_world(&dir.path().join("world"), "Extended spatial authoring").unwrap();
    let path = world.root.join("story/world.md");
    fs::write(
        &path,
        fs::read_to_string(&path).unwrap().replace(
            "capabilities: []",
            "capabilities: [spatial-core-v1, geometry-v1, route-v1, overlay-v1]",
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
            "location:harbor/landing",
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
fn populated() -> (tempfile::TempDir, World) {
    let (dir, mut world) = fixture();
    for request in CREATES {
        world = save(&world, None, request);
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

#[test]
fn geometry_examples_publish_shapes_levels_geography_and_literal_anchors() {
    let (_dir, world) = populated();
    let crossing =
        geometry::bbox_relation(&world, MAP, &[9.9, 9.9], &[10.1, 10.1], "intersects", 100)
            .unwrap();
    assert!(ids(&crossing).contains(&"location:harbor/boardwalk"));
    assert!(geometry::bbox_relation(&world,MAP,&[9.9,9.9],&[10.1,10.1],"within",100).unwrap()["items"].as_array().unwrap().is_empty());
    assert!(
        ids(&geometry::nearby(&world, MAP, [0.0, 0.0], 0.0, 100).unwrap())
            .contains(&"location:harbor/courtyard")
    );
    assert_eq!(
        ids(&geometry::nearby(&world, LEVELS, [3.0, 4.0, 2.0], 0.0, 100).unwrap()),
        [TOWER]
    );
    assert_eq!(
        ids(&geometry::nearby(&world, COAST, [12.5, 55.7], 0.0, 100).unwrap()),
        ["location:harbor/survey"]
    );
    let maps = geometry::maps(&world, 100).unwrap();
    let level = maps["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["mapId"] == LEVELS)
        .unwrap();
    assert_eq!(level["dimensions"], 3);
    assert_eq!(level["origin"]["coordinates"][2], "0");
    assert_eq!(level["scale"]["value"], "1");
    let anchors = geometry::anchors(&world, Some(MAP), Some(COAST), 100).unwrap();
    assert_eq!(anchors["items"][0]["anchorId"], ANCHOR);
    assert_eq!(anchors["items"][0]["from"]["coordinates"][0], "0");
    assert!(
        geometry::anchors(&world, Some(COAST), Some(MAP), 100).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    for request in CREATES {
        let request: Yaml = serde_yaml::from_str(request).unwrap();
        let id = request["id"].as_str().unwrap();
        let kind = request["kind"].as_str().unwrap();
        assert_eq!(
            record_edit::update(&world, id, &format!("kind: {kind}\n"))
                .unwrap()
                .raw_bytes,
            world.records[id].raw_bytes
        );
    }
}

#[test]
fn portal_retirement_preserves_source_and_never_supplies_weighted_or_snapped_edges() {
    let (_dir, world) = populated();
    let path = spatial::path(&world, HARBOR, TOWER, Some("foot"), "hops", None, 100).unwrap();
    assert_eq!(path["found"], true);
    assert_eq!(path["portalIds"][0], PORTAL);
    assert!(path["routeIds"].as_array().unwrap().is_empty());
    assert_eq!(
        spatial::path(&world, TOWER, HARBOR, Some("foot"), "hops", None, 100).unwrap()["found"],
        false
    );
    assert_eq!(
        spatial::path(&world, HARBOR, TOWER, Some("climb"), "hops", None, 100).unwrap()["found"],
        false
    );
    assert_eq!(
        spatial::path(
            &world,
            HARBOR,
            TOWER,
            None,
            "duration",
            Some("seconds"),
            100
        )
        .unwrap()["found"],
        false
    );
    assert!(
        spatial::reachable(&world, HARBOR, Some("foot"), 100).unwrap()["locationIds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| id == TOWER)
    );
    let before = &world.records[PORTAL];
    let saved = save(&world, Some(PORTAL), RETIRE);
    assert_eq!(saved.records[PORTAL].body, before.body);
    assert_eq!(saved.records[PORTAL].get("to"), before.get("to"));
    assert_eq!(
        spatial::path(&saved, HARBOR, TOWER, Some("foot"), "hops", None, 100).unwrap()["found"],
        false
    );
}

#[test]
fn overlay_window_updates_preserve_membership_and_exact_inclusive_scope() {
    let (_dir, world) = populated();
    for (tick, order, count) in [(9, 0, 1), (10, -1, 1), (10, 0, 2), (12, 0, 2), (12, 1, 1)] {
        assert_eq!(
            spatial::overlays(
                &world,
                HARBOR,
                &at(tick, order),
                "author",
                "author",
                None,
                100
            )
            .unwrap()["count"],
            count
        );
    }
    assert_eq!(
        spatial::overlays(&world, HARBOR, &at(11, 0), "reader", "author", None, 100).unwrap()["count"],
        0
    );
    let hidden = spatial::overlays(
        &world,
        HARBOR,
        &at(11, 0),
        "reader",
        "author",
        Some(OVERLAY),
        100,
    )
    .unwrap_err()
    .to_string();
    let missing = spatial::overlays(
        &world,
        HARBOR,
        &at(11, 0),
        "reader",
        "author",
        Some("overlay:absent"),
        100,
    )
    .unwrap_err()
    .to_string();
    assert_eq!(hidden, missing);
    let saved = save(&world, Some(OVERLAY), WINDOW);
    for key in ["membership", "audience", "perspectives"] {
        assert_eq!(
            saved.records[OVERLAY].get(key),
            world.records[OVERLAY].get(key)
        );
    }
    assert_eq!(saved.records[OVERLAY].body, world.records[OVERLAY].body);
    assert_eq!(
        spatial::overlays(&saved, HARBOR, &at(14, 0), "author", "author", None, 100).unwrap()["count"],
        2
    );
    assert_eq!(
        spatial::overlays(&saved, HARBOR, &at(14, 1), "author", "author", None, 100).unwrap()["count"],
        1
    );
}

#[test]
fn invalid_references_dimensions_and_intervals_cannot_publish() {
    let (_dir, world) = populated();
    for (id, request) in [
        (
            ANCHOR,
            "kind: anchor\nto: {map_id: map:harbor, coordinates: [1,1]}\n",
        ),
        (
            ANCHOR,
            "kind: anchor\nto: {map_id: map:harbor/coast, coordinates: [180,55]}\n",
        ),
        (PORTAL, "kind: portal\nto: location:missing\n"),
        (PORTAL, "kind: portal\nto: loc_00000000000000000000000001\n"),
        (
            PORTAL,
            "kind: portal\nto: {map_id: map:harbor/levels, coordinates: [3,4]}\n",
        ),
        (
            OVERLAY,
            "kind: overlay\nvalid: {start: {timeline: main, tick: 12, order: 1}, end: {timeline: main, tick: 12, order: 0}}\n",
        ),
        (
            OVERLAY,
            "kind: overlay\nvalid: {start: {timeline: absent, tick: 10, order: 0}, end: {timeline: absent, tick: 12, order: 0}}\n",
        ),
        (OVERLAY, "kind: overlay\nlifecycle: static\n"),
        (
            "overlay:harbor/maintenance",
            "kind: overlay\nlifecycle: time-bounded\n",
        ),
        (LEVELS, "kind: map\nz_policy: forbidden\n"),
        (COAST, "kind: map\nunit: m\n"),
    ] {
        refused(&world, Some(id), request);
    }
    let directory = tempfile::tempdir().unwrap();
    let blank = source::init_world(&directory.path().join("world"), "No spatial opt-in").unwrap();
    refused(
        &blank,
        None,
        "kind: overlay\nid: overlay:empty\ntitle: Empty\nlifecycle: static\nmembership: {location_ids: []}\naudience: []\nperspectives: []\n",
    );
}

#[test]
fn malformed_drafts_and_unsupported_fields_fail_without_mutation() {
    let (_dir, world) = populated();
    for (id, request) in [
        (
            ANCHOR,
            "kind: anchor\nfrom: {map_id: map:harbor, coordinates: [0,0], extra: true}\n",
        ),
        (ANCHOR, "kind: anchor\nconversion: null\n"),
        (PORTAL, "kind: portal\ndirection: two-way\n"),
        (
            PORTAL,
            "kind: portal\nduration: {value: 1, unit: seconds}\n",
        ),
        (PORTAL, "kind: portal\nmodes: [walk, climb]\n"),
        (PORTAL, "kind: portal\nstatus: active\n"),
        (
            OVERLAY,
            "kind: overlay\nmembership: {location_ids: [loc_00000000000000000000000001, loc_00000000000000000000000001]}\n",
        ),
        (OVERLAY, "kind: overlay\nvalid: null\n"),
        (
            OVERLAY,
            "kind: overlay\nvalid: {start: {tick: 1}, end: {tick: 2}}\n",
        ),
        (
            COAST,
            "kind: map\norigin: {label: mark, coordinates: [true,0]}\n",
        ),
        (COAST, "kind: map\nscale: {value: -1, unit: degree}\n"),
    ] {
        assert!(
            matches!(
                record_edit::update(&world, id, request),
                Err(Error::InvalidInput(_))
            ),
            "{request}"
        );
    }
    unchanged(&world);
}

#[test]
fn new_kinds_keep_common_annotations_and_never_grant_character_evidence() {
    let (_dir, mut world) = fixture();
    let before = character::context(&world, ANA, &at(11, 0), 100_000).unwrap();
    let search = character::search(&world, ANA, "harbor", &at(11, 0), 100).unwrap();
    for request in CREATES {
        world = save(&world, None, request);
    }
    let owner = world.world_record().unwrap().id.clone();
    world = save(
        &world,
        Some(&owner),
        include_str!("../examples/authoring/19-world-chronology.yaml"),
    );
    for (index, (kind, id)) in [("anchor", ANCHOR), ("portal", PORTAL), ("overlay", OVERLAY)]
        .into_iter()
        .enumerate()
    {
        let update = format!(
            "kind: {kind}\nchronology:\n- id: chronology_{:026}\n  value: {{civil: {{calendar_id: calendar_00000000000000000000000001, year: 1}}}}\n  provenance: [survey]\n",
            index + 50
        );
        world = save(&world, Some(id), &update);
        assert_eq!(
            chronology::annotations(
                &world,
                &chronology::AnnotationFilter {
                    record: Some(id.into()),
                    calendar: None,
                    era: None,
                    limit: 100
                }
            )
            .unwrap()["count"],
            1
        );
    }
    assert_eq!(
        before,
        character::context(&world, ANA, &at(11, 0), 100_000).unwrap()
    );
    assert_eq!(
        search,
        character::search(&world, ANA, "harbor", &at(11, 0), 100).unwrap()
    );
}

fn cli(root: &Path, args: &[&str]) -> Json {
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

#[test]
fn all_thirteen_new_numbered_requests_preview_and_save_through_cli() {
    let (dir, mut world) = fixture();
    for (id, request) in CREATES
        .into_iter()
        .map(|request| (None, request))
        .chain([(Some(PORTAL), RETIRE), (Some(OVERLAY), WINDOW)])
    {
        let path = dir.path().join("request.yaml");
        fs::write(&path, request).unwrap();
        let mut args = vec!["record", if id.is_some() { "update" } else { "create" }];
        if let Some(id) = id {
            args.push(id);
        }
        args.extend([
            "--file",
            path.to_str().unwrap(),
            "--expected-fingerprint",
            &world.fingerprint,
        ]);
        let preview = cli(&world.root, &args)["result"].clone();
        assert_eq!(preview["canApply"], true, "{preview}");
        unchanged(&world);
        args.extend([
            "--apply",
            "--expected-after-sha256",
            preview["afterSha256"].as_str().unwrap(),
        ]);
        cli(&world.root, &args);
        world = source::load_world(&world.root, None).unwrap();
        assert_eq!(world.fingerprint, preview["candidateFingerprint"]);
        assert_eq!(
            fs::read(world.root.join(preview["sourcePath"].as_str().unwrap())).unwrap(),
            preview["afterSource"].as_str().unwrap().as_bytes()
        );
    }
    assert_eq!(world.records[PORTAL].status(), "retired");
    assert_eq!(
        spatial::overlays(&world, HARBOR, &at(14, 0), "author", "author", None, 100).unwrap()["count"],
        2
    );
}
