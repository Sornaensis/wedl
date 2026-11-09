//! Map edits and placements share the reviewed single-record save boundary.
use std::{fs, path::PathBuf};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use wedl::{Error, authoring, geometry, model::World, record_edit, source, validation};

const MAP: &str = "map:harbor/main";
const LOCATION: &str = "location:harbor/pier";
const MAP_FIELDS: &str = "crs: local-planar:harbor\naxis_order: [east, north]\nunit: m\nbounds: {min: [-10, -10], max: [10, 10]}\n";

struct Fixture {
    _directory: TempDir,
    world: World,
}

impl Fixture {
    fn new(capabilities: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("world");
        source::init_world(&root, "Map authoring").unwrap();
        if capabilities {
            let path = root.join("story/world.md");
            let raw = fs::read_to_string(&path).unwrap();
            fs::write(
                path,
                raw.replace(
                    "capabilities: []",
                    "capabilities: [spatial-core-v1, geometry-v1]",
                ),
            )
            .unwrap();
        }
        Self {
            _directory: directory,
            world: source::load_world(&root, None).unwrap(),
        }
    }

    fn save(&mut self, target: Option<&str>, request: &str) -> Value {
        let fingerprint = self.world.fingerprint.clone();
        let preview = match target {
            Some(id) => authoring::preview_record_update(&self.world, id, request, &fingerprint),
            None => authoring::preview_record_create(&self.world, request, &fingerprint),
        }
        .unwrap();
        assert_eq!(
            source::load_world(&self.world.root, None)
                .unwrap()
                .fingerprint,
            fingerprint
        );
        assert_eq!(preview["canApply"], true, "{preview}");
        let hash = preview["afterSha256"].as_str().unwrap();
        match target {
            Some(id) => {
                authoring::apply_record_update(&self.world, id, request, &fingerprint, hash)
            }
            None => authoring::apply_record_create(&self.world, request, &fingerprint, hash),
        }
        .unwrap();
        self.world = source::load_world(&self.world.root, None).unwrap();
        assert_eq!(self.world.fingerprint, preview["candidateFingerprint"]);
        let saved = &self.world.records[preview["entityId"].as_str().unwrap()];
        assert_eq!(
            saved.raw_bytes,
            preview["afterSource"].as_str().unwrap().as_bytes()
        );
        assert!(validation::validate_world(&self.world).valid);
        preview
    }

    fn refuse(&self, target: Option<&str>, request: &str) -> Value {
        let fingerprint = &self.world.fingerprint;
        let preview = match target {
            Some(id) => authoring::preview_record_update(&self.world, id, request, fingerprint),
            None => authoring::preview_record_create(&self.world, request, fingerprint),
        }
        .unwrap();
        assert_eq!(preview["canApply"], false, "{preview}");
        let hash = preview["afterSha256"].as_str().unwrap();
        let applied = match target {
            Some(id) => authoring::apply_record_update(&self.world, id, request, fingerprint, hash),
            None => authoring::apply_record_create(&self.world, request, fingerprint, hash),
        };
        assert!(matches!(applied, Err(Error::Validation(_))));
        assert_eq!(
            source::load_world(&self.world.root, None)
                .unwrap()
                .fingerprint,
            *fingerprint
        );
        preview
    }

    fn populate(&mut self) {
        self.save(
            None,
            &format!("kind: map\nid: {MAP}\ntitle: Harbor Map\n{MAP_FIELDS}body: 'Map prose.'\n"),
        );
        self.save(
            None,
            &format!("kind: location\nid: {LOCATION}\ntitle: Pier\nbody: 'Pier prose.'\n"),
        );
    }
}

fn placement(map: &str, point: &str) -> String {
    format!(
        "kind: location\nspatial: {{map_id: {map}, geometry: {{kind: point, coordinates: {point}}}}}\n"
    )
}

#[test]
fn maps_and_placements_create_native_records_and_feed_queries() {
    let mut fixture = Fixture::new(true);
    fixture.populate();
    let map = &fixture.world.records[MAP];
    assert_eq!(
        map.source_path,
        format!(
            "story/maps/map-{}.md",
            hex::encode(Sha256::digest(MAP.as_bytes()))
        )
    );
    assert_eq!(map.status(), "canonical");
    assert_eq!(map.string("schema"), Some("wedl/v1"));
    assert_eq!(map.list("aliases").unwrap().len(), 0);
    assert!(map.get("initial_state").is_none());
    fixture.save(Some(LOCATION), &placement(MAP, "[3, 4]"));
    assert_eq!(fixture.world.records[LOCATION].body, "Pier prose.");
    let near = geometry::nearby(&fixture.world, "Harbor Map", [0.0, 0.0], 5.0, 100).unwrap();
    assert_eq!(near["items"][0]["locationId"], LOCATION);
    assert_eq!(near["items"][0]["distance"], 5.0);
    assert_eq!(near["unit"], "m");
    fixture.save(
        Some(MAP),
        "kind: map\nbounds: {min: [-20, -20], max: [20, 20]}\n",
    );
    assert_eq!(fixture.world.records[MAP].body, "Map prose.");
    assert_eq!(
        geometry::bbox(&fixture.world, MAP, [3.0, 4.0], [3.0, 4.0], 100).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn map_requests_require_closed_shapes_and_supported_geometry() {
    let fixture = Fixture::new(true);
    let valid = format!("kind: map\nid: {MAP}\ntitle: Harbor Map\n{MAP_FIELDS}");
    for (from, to) in [
        ("crs: local-planar:harbor\n", ""),
        ("axis_order: [east, north]\n", ""),
        ("unit: m\n", ""),
        ("bounds: {min: [-10, -10], max: [10, 10]}\n", ""),
        ("local-planar:harbor", "'local-planar: '"),
        ("[east, north]", "[north, east]"),
        ("unit: m", "unit: 3"),
        ("min: [-10, -10]", "min: [-10]"),
        ("min: [-10, -10]", "min: [-10, -10, 0]"),
        ("min: [-10, -10]", "min: [10, -10]"),
        ("max: [10, 10]", "max: [10, 10], extra: 1"),
        ("max: [10, 10]", "max: [9007199254740992, 10]"),
        ("max: [10, 10]", "max: [.inf, 10]"),
        ("max: [10, 10]", "max: [true, 10]"),
    ] {
        let request = valid.replace(from, to);
        assert!(
            matches!(
                record_edit::create(&fixture.world, &request),
                Err(Error::InvalidInput(_))
            ),
            "accepted {request}"
        );
    }
    for extra in [
        "status: draft",
        "z_unit: level",
        "bounds: null",
        "unknown: true",
        "crs: !tag local-planar:town",
    ] {
        assert!(
            record_edit::create(&fixture.world, &format!("{valid}{extra}\n")).is_err(),
            "accepted {extra}"
        );
    }
    for id in [
        "map:../outside",
        "map:/absolute",
        "map:Town",
        "map:con",
        "map:town//main",
    ] {
        assert!(record_edit::create(&fixture.world, &valid.replace(MAP, id)).is_err());
    }
}

#[test]
fn malformed_placement_requests_fail_without_changing_source() {
    let mut fixture = Fixture::new(true);
    fixture.populate();
    for fields in [
        "spatial: null",
        "spatial: {}",
        "spatial: {map_id: map:missing}",
        "spatial: {map_id: map:harbor/main, geometry: {kind: point, coordinates: [0,0], extra: true}}",
        "spatial: {map_id: map:harbor/main, geometry: {kind: point, coordinates: [0,0]}, extra: true}",
        "spatial: {map_id: map:harbor/main, geometry: {kind: point, coordinates: ['0',0]}}",
        "spatial: {map_id: map:harbor/main, geometry: {kind: point, coordinates: [9007199254740992,0]}}",
        "spatial: {map_id: map:harbor/main, geometry: {kind: point, coordinates: [!tag 0,0]}}",
    ] {
        let request = format!("kind: location\n{fields}\n");
        assert!(
            matches!(
                record_edit::update(&fixture.world, LOCATION, &request),
                Err(Error::InvalidInput(_))
            ),
            "accepted {request}"
        );
    }
    assert_eq!(
        source::load_world(&fixture.world.root, None)
            .unwrap()
            .fingerprint,
        fixture.world.fingerprint
    );
}

#[test]
fn references_capabilities_and_map_shrinking_are_reviewable_failures() {
    let missing_capability = Fixture::new(false);
    missing_capability.refuse(
        None,
        &format!("kind: map\nid: {MAP}\ntitle: Harbor Map\n{MAP_FIELDS}"),
    );
    let mut fixture = Fixture::new(true);
    fixture.populate();
    fixture.refuse(Some(LOCATION), &placement("map:missing", "[0,0]"));
    fixture.refuse(Some(LOCATION), &placement(MAP, "[11,0]"));
    // These are valid shapes whose map-dependent meaning is rejected in preview.
    fixture.refuse(Some(LOCATION), &placement(MAP, "[0,0,0]"));
    fixture.refuse(Some(MAP), "kind: map\ncrs: EPSG:4326\n");
    fixture.refuse(Some(MAP), "kind: map\nz_policy: required\n");
    fixture.save(Some(LOCATION), &placement(MAP, "[3,4]"));
    fixture.refuse(Some(MAP), "kind: map\nbounds: {min: [-2,-2], max: [2,2]}\n");
    assert_eq!(
        geometry::nearby(&fixture.world, MAP, [0.0, 0.0], 5.0, 100).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn placement_updates_replace_whole_values_and_omission_preserves_them() {
    let mut fixture = Fixture::new(true);
    fixture.populate();
    fixture.save(Some(LOCATION), &placement(MAP, "[3,4]"));
    fixture.save(Some(LOCATION), "kind: location\ntitle: Renamed Pier\n");
    assert_eq!(
        fixture.world.records[LOCATION].frontmatter["spatial"]["geometry"]["coordinates"],
        serde_yaml::from_str::<serde_yaml::Value>("[3,4]").unwrap()
    );
    fixture.save(Some(LOCATION), &placement(MAP, "[-1,-2]"));
    assert_eq!(
        geometry::bbox(&fixture.world, MAP, [3.0, 4.0], [3.0, 4.0], 100).unwrap()["items"],
        json!([])
    );
    let changed = geometry::bbox(&fixture.world, MAP, [-1.0, -2.0], [-1.0, -2.0], 100).unwrap();
    assert_eq!(changed["items"][0]["locationId"], LOCATION);
    assert_eq!(fixture.world.records[LOCATION].body, "Pier prose.");
}

#[test]
fn packaged_map_noops_preserve_bytes_and_structured_updates_retain_opaque_metadata() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/tideglass");
    let world = source::load_world(&root, None).unwrap();
    let id = "map:tideglass-town";
    let original = &world.records[id];
    let no_op = record_edit::update(&world, id, "kind: map\n").unwrap();
    assert_eq!(no_op.raw_bytes, original.raw_bytes);
    let changed = record_edit::update(&world, id, "kind: map\ntitle: Renamed Town Map\n").unwrap();
    assert_eq!(changed.body, original.body);
    assert_eq!(changed.source_path, original.source_path);
    for field in [
        "bounds",
        "axis_order",
        "unit",
        "crs",
        "domain",
        "section_audiences",
    ] {
        assert_eq!(changed.get(field), original.get(field));
    }
    let mut records = world.records.clone();
    records.insert(id.into(), changed);
    let candidate =
        source::world_from_records(root, records.into_values().collect(), None).unwrap();
    assert!(validation::validate_world(&candidate).valid);
}
