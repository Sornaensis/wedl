//! Geometry is a bounded author read of explicit placements, never travel topology.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    Error, geometry,
    model::{Record, World},
    record_edit, source, spatial, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const SAFE: f64 = 9_007_199_254_740_991.0;

fn record(kind: &str, id: &str, fields: &str) -> Record {
    let source = format!(
        "---\nschema: wedl/v1\nkind: {kind}\nid: {id}\ntitle: {id}\n{fields}---\nAuthored prose stays intact.\n"
    );
    source::parse_record(
        source.as_bytes(),
        &format!("story/{}.md", id.replace([':', '/'], "-")),
    )
    .unwrap()
}
fn world(mut records: Vec<Record>) -> World {
    records.push(record("world", WORLD, "capabilities: [spatial-core-v1, geometry-v1]\ndefault_timeline: main\ntimelines: [{id: main, label: Main}]\nstate_keys: {}\n"));
    source::world_from_records(PathBuf::new(), records, None).unwrap()
}
fn map(id: &str, extra: &str) -> Record {
    record(
        "map",
        id,
        &format!(
            "crs: local-planar:test\naxis_order: [east, north]\nunit: pace\nbounds: {{min: [-10, -10], max: [10, 10]}}\n{extra}"
        ),
    )
}
fn point(id: &str, map: &str, coordinates: &str, extra: &str) -> Record {
    record(
        "location",
        id,
        &format!(
            "spatial: {{map_id: {map}, geometry: {{kind: point, coordinates: {coordinates}}}}}\n{extra}"
        ),
    )
}
fn base() -> World {
    world(vec![
        map("map:main", "aliases: [Main Map]\n"),
        map("map:other", ""),
        map("map:retired", "status: retired\n"),
        point("location:a-farther", "map:main", "[3, 4]", ""),
        point("location:corner", "map:main", "[1, 1]", ""),
        point("location:edge", "map:main", "[-10, 10]", ""),
        point("location:z-origin", "map:main", "[0, 0]", ""),
        point("location:other", "map:other", "[0, 0]", ""),
        point(
            "location:retired",
            "map:main",
            "[0, 0]",
            "status: retired\n",
        ),
        record("location", "location:unplaced", "spatial: null\n"),
    ])
}
fn yaml(value: &str) -> Yaml {
    serde_yaml::from_str(value).unwrap()
}
fn edited(world: &World, id: &str, edit: impl FnOnce(&mut Yaml)) -> World {
    let mut records = world.records.clone();
    let record = records.get_mut(id).unwrap();
    edit(&mut record.frontmatter);
    let source = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&record.frontmatter).unwrap(),
        record.body
    );
    *record = source::parse_record(source.as_bytes(), &record.source_path).unwrap();
    source::world_from_records(
        world.root.clone(),
        records.into_values().collect(),
        world.revision.clone(),
    )
    .unwrap()
}
fn valid(world: &World) {
    assert!(
        geometry::validate(world).is_empty(),
        "{:?}",
        geometry::validate(world)
    );
    let report = validation::validate_world(world);
    assert!(report.valid, "{:?}", report.diagnostics);
}
fn invalid(world: &World) {
    assert!(!geometry::validate(world).is_empty());
    assert!(matches!(
        geometry::maps(world, 100),
        Err(Error::Validation(_))
    ));
    assert!(!validation::validate_world(world).valid);
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
fn catalogue_retains_declared_units_axes_bounds_and_source_citations() {
    let world = base();
    valid(&world);
    let result = geometry::maps(&world, 100).unwrap();
    assert_eq!(result["scope"], "author-all-time-spatial-source");
    assert_eq!(result["perspective"], "author");
    assert_eq!(result["basis"], "authored-maps");
    assert_eq!(result["items"].as_array().unwrap().len(), 2);
    assert_eq!(result["items"][0]["mapId"], "map:main");
    assert_eq!(result["items"][1]["mapId"], "map:other");
    assert_eq!(result["items"][0]["axisOrder"], json!(["east", "north"]));
    assert_eq!(
        result["items"][0]["bounds"],
        json!({"min":["-10","-10"],"max":["10","10"]})
    );
    assert_eq!(result["items"][0]["unit"], "pace");
    assert_eq!(
        result["items"][0]["citation"]["fingerprint"],
        world.fingerprint
    );
    assert_eq!(
        result["items"][0]["citation"]["sourcePath"],
        world.records["map:main"].source_path
    );
}

#[test]
fn inclusive_boxes_and_euclidean_radii_return_complete_id_ordered_matches() {
    let world = base();
    let single = geometry::bbox(&world, "MAIN MAP", [1.0, 1.0], [1.0, 1.0], 100).unwrap();
    assert_eq!(ids(&single), ["location:corner"]);
    let edge = geometry::bbox(&world, "map:main", [-10.0, 10.0], [-10.0, 10.0], 100).unwrap();
    assert_eq!(ids(&edge), ["location:edge"]);
    let all = geometry::bbox(&world, "map:main", [-20.0, -20.0], [20.0, 20.0], 100).unwrap();
    assert_eq!(
        ids(&all),
        [
            "location:a-farther",
            "location:corner",
            "location:edge",
            "location:z-origin"
        ]
    );
    let zero = geometry::nearby(&world, "map:main", [0.0, 0.0], 0.0, 100).unwrap();
    assert_eq!(ids(&zero), ["location:z-origin"]);
    assert_eq!(zero["items"][0]["distance"], 0.0);
    // The square candidate [1,1] is outside radius 1; boundaries at distance 5 count.
    assert_eq!(
        ids(&geometry::nearby(&world, "map:main", [0.0, 0.0], 1.0, 100).unwrap()),
        ["location:z-origin"]
    );
    let nearby = geometry::nearby(&world, "map:main", [0.0, 0.0], 5.0, 100).unwrap();
    assert_eq!(
        ids(&nearby),
        ["location:a-farther", "location:corner", "location:z-origin"]
    );
    assert_eq!(nearby["items"][0]["distance"], 5.0);
    assert_eq!(nearby["items"][0]["coordinates"], json!(["3", "4"]));
    assert_eq!(nearby["basis"], "same-map-authored-geometry");
    assert_eq!(nearby["unit"], "pace");
    assert_eq!(
        ids(&geometry::nearby(&world, "map:main", [20.0, 10.0], 30.0, 100).unwrap()),
        ids(&all)
    );
}

#[test]
fn map_isolation_and_authored_links_do_not_create_placement_or_travel() {
    let world = edited(&base(), "location:unplaced", |value| {
        value["parent_id"] = yaml("location:z-origin");
        value["links"] = yaml("[{location: location:z-origin}]");
    });
    valid(&world);
    let value = geometry::nearby(&world, "map:other", [0.0, 0.0], 0.0, 100).unwrap();
    assert_eq!(ids(&value), ["location:other"]);
    let value = geometry::bbox(&world, "map:main", [-10.0, -10.0], [10.0, 10.0], 100).unwrap();
    assert!(!ids(&value).contains(&"location:unplaced"));
    assert_eq!(
        spatial::path(
            &world,
            "location:z-origin",
            "location:a-farther",
            None,
            "hops",
            None,
            100
        )
        .unwrap()["found"],
        false
    );
    assert_eq!(
        spatial::path(
            &world,
            "location:unplaced",
            "location:z-origin",
            None,
            "hops",
            None,
            100
        )
        .unwrap()["found"],
        false
    );
}

#[test]
fn map_shapes_are_closed_where_typed_and_unsupported_coordinate_systems_fail() {
    let world = base();
    for (field, value) in [
        ("crs", "EPSG:4326"),
        ("crs", "'local-planar: '"),
        ("crs", "null"),
        ("crs", "!tag local-planar:town"),
        ("axis_order", "[north, east]"),
        ("axis_order", "[east, north, up]"),
        ("axis_order", "[!tag east, north]"),
        ("unit", "''"),
        ("unit", "true"),
        ("unit", "!tag pace"),
        ("z_policy", "required"),
        ("z_policy", "null"),
        ("bounds", "{min: [0,0], max: [0,1]}"),
        ("bounds", "{min: [1,0], max: [0,1]}"),
        ("bounds", "{min: [0,0,0], max: [1,1,1]}"),
        ("bounds", "{min: [0,0], max: [1,1], extra: 1}"),
        ("bounds", "{min: [0,0]}"),
        ("bounds", "!tag {min: [0,0], max: [1,1]}"),
        ("bounds", "{min: [0,0], max: ['1',1]}"),
        ("bounds", "{min: [0,0], max: [true,1]}"),
        ("status", "draft"),
        ("status", "null"),
    ] {
        invalid(&edited(&world, "map:main", |source| {
            source[field] = yaml(value)
        }));
    }
    for field in ["crs", "axis_order", "unit", "bounds"] {
        invalid(&edited(&world, "map:main", |source| {
            source.as_mapping_mut().unwrap().remove(field);
        }));
    }
    valid(&edited(&world, "map:main", |source| {
        source["z_policy"] = yaml("optional-level");
    }));
    valid(&edited(&world, "map:main", |source| {
        source["z_policy"] = yaml("forbidden");
        source["x-author-note"] = yaml("{value: null}");
    }));
}

#[test]
fn placement_shapes_references_and_source_bounds_are_checked_in_every_status() {
    let world = base();
    for value in [
        "!tag null",
        "[]",
        "{map_id: map:main}",
        "{map_id: map:main, geometry: {kind: point, coordinates: [0,0]}, extra: 1}",
        "{map_id: Main Map, geometry: {kind: point, coordinates: [0,0]}}",
        "{map_id: location:z-origin, geometry: {kind: point, coordinates: [0,0]}}",
        "{map_id: map:missing, geometry: {kind: point, coordinates: [0,0]}}",
        "{map_id: map:retired, geometry: {kind: point, coordinates: [0,0]}}",
        "{map_id: map:main, geometry: {kind: point, coordinates: [0,0,0]}}",
        "{map_id: map:main, geometry: {kind: point, coordinates: ['0',0]}}",
        "{map_id: map:main, geometry: {kind: point, coordinates: [0,0], unit: pace}}",
        "{map_id: map:main, geometry: !tag {kind: point, coordinates: [0,0]}}",
        "{map_id: map:main, geometry: {kind: point, coordinates: [!tag 0,0]}}",
        "{map_id: map:main, geometry: {kind: point, coordinates: [10.01,0]}}",
        "{map_id: map:main, geometry: {kind: point, coordinates: [0,-10.01]}}",
    ] {
        invalid(&edited(&world, "location:z-origin", |source| {
            source["spatial"] = yaml(value)
        }));
    }
    for geometry in [
        "{kind: line, coordinates: [[0,0],[1,1]]}",
        "{kind: polygon, coordinates: [[0,0],[1,0],[0,1],[0,0]]}",
    ] {
        valid(&edited(&world, "location:z-origin", |source| {
            source["spatial"]["geometry"] = yaml(geometry);
        }));
    }
    invalid(&edited(&world, "location:retired", |source| {
        source["spatial"]["geometry"]["coordinates"] = yaml("[20,0]")
    }));
    invalid(&edited(&world, "map:retired", |source| {
        source["unit"] = yaml("null")
    }));
    valid(&edited(&world, "location:retired", |source| {
        source["spatial"]["map_id"] = yaml("map:retired")
    }));
    valid(&edited(&world, "location:z-origin", |source| {
        source.as_mapping_mut().unwrap().remove("spatial");
    }));
    valid(&edited(&world, "location:z-origin", |source| {
        source["spatial"] = Yaml::Null
    }));
}

#[test]
fn geometry_capabilities_are_explicit_and_unused_geometry_does_not_require_them() {
    let world = base();
    for capabilities in ["[]", "[spatial-core-v1]", "[geometry-v1]"] {
        let changed = edited(&world, WORLD, |source| {
            source["capabilities"] = yaml(capabilities)
        });
        let diagnostics = geometry::validate(&changed);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "NAR-CAPABILITY-001"
                    && d.entity_id.as_deref() == Some("map:main"))
        );
        assert!(diagnostics.iter().any(|d| d.code == "NAR-CAPABILITY-001"
            && d.entity_id.as_deref() == Some("location:z-origin")));
        invalid(&changed);
    }
    let empty = edited(
        &self::world(vec![record(
            "location",
            "location:unplaced",
            "spatial: null\n",
        )]),
        WORLD,
        |source| source["capabilities"] = yaml("[]"),
    );
    valid(&empty);
    assert!(
        geometry::maps(&empty, 100).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn safe_integer_boundary_is_exact_and_unsafe_or_nonfinite_source_values_fail() {
    let world = world(vec![
        record(
            "map",
            "map:large",
            "crs: local-planar:large\naxis_order: [east, north]\nunit: steps\nbounds: {min: [-9007199254740991, -1], max: [9007199254740991, 1]}\n",
        ),
        point("location:large", "map:large", "[9007199254740991, 0]", ""),
    ]);
    valid(&world);
    let value = geometry::bbox(&world, "map:large", [SAFE, 0.0], [SAFE, 0.0], 100).unwrap();
    assert_eq!(
        value["items"][0]["coordinates"],
        json!(["9007199254740991", "0"])
    );
    let nearby = geometry::nearby(&world, "map:large", [SAFE - 1.0, 0.0], 1.0, 100).unwrap();
    assert_eq!(nearby["items"][0]["distance"], 1.0);
    assert_eq!(
        ids(&geometry::nearby(&world, "map:large", [-SAFE, 0.0], f64::MAX, 100).unwrap()),
        ["location:large"]
    );
    for value in [
        "9007199254740992",
        "9007199254740993",
        "18446744073709551615",
        "-9007199254740992",
        ".nan",
        ".inf",
        "-.inf",
        "!tag 1",
    ] {
        invalid(&edited(&world, "location:large", |source| {
            source["spatial"]["geometry"]["coordinates"] = yaml(&format!("[{value}, 0]"))
        }));
        invalid(&edited(&world, "map:large", |source| {
            source["bounds"]["max"] = yaml(&format!("[{value}, 1]"))
        }));
    }
}

#[test]
fn malformed_queries_unavailable_maps_and_complete_result_limits_fail_closed() {
    let world = base();
    for limit in [0, 1001] {
        assert!(matches!(
            geometry::maps(&world, limit),
            Err(Error::InvalidInput(_))
        ));
        assert!(geometry::bbox(&world, "map:main", [0.0, 0.0], [1.0, 1.0], limit).is_err());
        assert!(geometry::nearby(&world, "map:main", [0.0, 0.0], 1.0, limit).is_err());
    }
    for value in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        SAFE + 1.0,
        -SAFE - 1.0,
    ] {
        assert!(matches!(
            geometry::bbox(&world, "map:main", [value, 0.0], [1.0, 1.0], 100),
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            geometry::nearby(&world, "map:main", [0.0, value], 1.0, 100),
            Err(Error::InvalidInput(_))
        ));
    }
    for radius in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            geometry::nearby(&world, "map:main", [0.0, 0.0], radius, 100),
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(geometry::bbox(&world, "map:main", [1.0, 0.0], [0.0, 1.0], 100).is_err());
    for reference in ["map:missing", "map:retired", "location:z-origin"] {
        assert!(geometry::bbox(&world, reference, [0.0, 0.0], [1.0, 1.0], 100).is_err());
        assert!(geometry::nearby(&world, reference, [0.0, 0.0], 1.0, 100).is_err());
    }
    assert!(geometry::maps(&world, 1).is_err());
    assert!(geometry::bbox(&world, "map:main", [-10.0, -10.0], [10.0, 10.0], 3).is_err());
    assert!(geometry::nearby(&world, "map:main", [0.0, 0.0], 5.0, 2).is_err());
    assert_eq!(
        ids(&geometry::bbox(&world, "map:main", [1.0, 1.0], [1.0, 1.0], 1).unwrap()),
        ["location:corner"]
    );
    assert!(ids(&geometry::nearby(&world, "map:main", [30.0, 30.0], 0.0, 1).unwrap()).is_empty());
}

#[test]
fn maintained_corpora_validate_and_geometry_reads_and_no_ops_preserve_source() {
    let mut counts = (0, 0);
    for name in [
        "tideglass",
        "ash_archive",
        "frontiersmen",
        "chronology_conformance",
    ] {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join(name);
        let world = source::load_world(&root, None).unwrap();
        valid(&world);
        let catalogue = geometry::maps(&world, 100).unwrap();
        if name == "tideglass" {
            assert_eq!(catalogue["items"].as_array().unwrap().len(), 1);
            let all = geometry::bbox(
                &world,
                "map:tideglass-town",
                [-100.0, -100.0],
                [100.0, 100.0],
                100,
            )
            .unwrap();
            assert_eq!(all["items"].as_array().unwrap().len(), 7);
            let near =
                geometry::nearby(&world, "Tideglass Town Map", [0.0, -20.0], 10.0, 100).unwrap();
            assert_eq!(near["items"].as_array().unwrap().len(), 2);
            assert_eq!(near["unit"], "pace");
        } else {
            assert!(catalogue["items"].as_array().unwrap().is_empty());
        }
        for record in world
            .records
            .values()
            .filter(|record| record.kind == "map" || record.get("spatial").is_some())
        {
            let request = format!(
                "kind: {}\ntitle: {}\n",
                record.kind,
                serde_json::to_string(&record.title).unwrap()
            );
            let unchanged = record_edit::update(&world, &record.id, &request).unwrap();
            assert_eq!(unchanged.raw_bytes, record.raw_bytes);
            assert_eq!(unchanged.source_path, record.source_path);
            assert_eq!(unchanged.blob_oid, record.blob_oid);
            if record.kind == "map" {
                counts.0 += 1;
            } else {
                counts.1 += 1;
            }
        }
        assert_eq!(
            source::load_world(&world.root, None).unwrap().fingerprint,
            world.fingerprint
        );
    }
    assert_eq!(counts, (1, 7));
}
