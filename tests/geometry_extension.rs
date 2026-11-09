//! Extended geometry preserves explicit coordinate semantics and author-only access.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    Error, character, geometry,
    model::{Record, World},
    source, spatial, validation,
};

const HERO: &str = "char_00000000000000000000000001";
const WORLD: &str = "world_00000000000000000000000000";
fn record(mut value: Json) -> Record {
    value["schema"] = json!(source::SOURCE_SCHEMA);
    if value.get("title").is_none() {
        value["title"] = value["id"].clone()
    }
    let name = value["id"].as_str().unwrap().replace([':', '/'], "-");
    source::parse_record(
        format!(
            "---\n{}---\nPrivate geometry prose.\n",
            serde_yaml::to_string(&value).unwrap()
        )
        .as_bytes(),
        &format!("story/{name}.md"),
    )
    .unwrap()
}
fn world(mut values: Vec<Json>) -> World {
    values.push(json!({"kind":"world","id":WORLD,"capabilities":["spatial-core-v1","geometry-v1"],"default_timeline":"main","timelines":[{"id":"main","label":"Main"}],"state_keys":{}}));
    values.push(json!({"kind":"character","id":HERO,"initial_state":{}}));
    source::world_from_records(
        PathBuf::new(),
        values.into_iter().map(record).collect(),
        Some("geometry-revision".into()),
    )
    .unwrap()
}
fn map(id: &str, dimensions: usize, policy: &str) -> Json {
    json!({"kind":"map","id":id,"crs":"local-planar:test","axis_order":["east","north"],"unit":"m","z_policy":policy,"bounds":{"min":vec![-10;dimensions],"max":vec![10;dimensions]}})
}
fn place(id: &str, map: &str, kind: &str, coordinates: Json) -> Json {
    json!({"kind":"location","id":id,"spatial":{"map_id":map,"geometry":{"kind":kind,"coordinates":coordinates}}})
}
fn anchor(id: &str, from: &str, a: Json, to: &str, b: Json) -> Json {
    json!({"kind":"anchor","id":id,"from":{"map_id":from,"coordinates":a},"to":{"map_id":to,"coordinates":b},"conversion":"Authored correspondence only."})
}
fn edited(base: &World, id: &str, edit: impl FnOnce(&mut Yaml)) -> World {
    let original = &base.records[id];
    let mut value = original.frontmatter.clone();
    edit(&mut value);
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&value).unwrap(),
        original.body
    );
    let replacement = source::parse_record(raw.as_bytes(), &original.source_path).unwrap();
    source::world_from_records(
        base.root.clone(),
        base.records
            .values()
            .map(|record| {
                if record.id == id {
                    replacement.clone()
                } else {
                    record.clone()
                }
            })
            .collect(),
        base.revision.clone(),
    )
    .unwrap()
}
fn yaml(value: &str) -> Yaml {
    serde_yaml::from_str(value).unwrap()
}
fn valid(world: &World) {
    let report = validation::validate_world(world);
    assert!(report.valid, "{:?}", report.diagnostics);
    assert!(geometry::validate(world).is_empty())
}
fn invalid(world: &World) {
    assert!(!geometry::validate(world).is_empty());
    assert!(!validation::validate_world(world).valid);
    assert!(matches!(
        geometry::maps(world, 100),
        Err(Error::Validation(_))
    ))
}
fn ids(value: &Json) -> Vec<&str> {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["locationId"].as_str().unwrap())
        .collect()
}
fn row<'a>(value: &'a Json, id: &str) -> &'a Json {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["locationId"] == id)
        .unwrap()
}

#[test]
fn geographic_and_dimensional_maps_retain_authored_axes_units_origin_and_scale() {
    let mut planar = map("map:planar", 2, "optional-level");
    planar["origin"] = json!({"label":"Author origin","coordinates":[-20,30]});
    planar["scale"] = json!({"value":2,"unit":"pace"});
    let mut earth = map("map:earth", 2, "forbidden");
    earth["crs"] = json!("EPSG:4326");
    earth["axis_order"] = json!(["longitude", "latitude"]);
    earth["unit"] = json!("degree");
    earth["bounds"] = json!({"min":[-180,-90],"max":[180,90]});
    let world = world(vec![
        planar,
        earth,
        map("map:levels", 3, "optional-level"),
        map("map:volume", 3, "required"),
    ]);
    valid(&world);
    let result = geometry::maps(&world, 10).unwrap();
    assert_eq!(result["basis"], "authored-maps");
    let maps = result["items"].as_array().unwrap();
    let earth = maps.iter().find(|m| m["mapId"] == "map:earth").unwrap();
    assert_eq!(earth["axisOrder"], json!(["longitude", "latitude"]));
    assert_eq!(earth["unit"], "degree");
    assert_eq!(
        earth["bounds"],
        json!({"min":["-180","-90"],"max":["180","90"]})
    );
    let planar = maps.iter().find(|m| m["mapId"] == "map:planar").unwrap();
    assert_eq!(planar["origin"]["coordinates"], json!(["-20", "30"]));
    assert_eq!(planar["scale"]["value"], "2");
    assert_eq!(planar["dimensions"], 2);
    assert_eq!(
        maps.iter().find(|m| m["mapId"] == "map:volume").unwrap()["dimensions"],
        3
    );
}

#[test]
fn geometry_bounds_intersect_and_within_are_inclusive_and_distinct() {
    let world = world(vec![
        map("map:main", 2, "forbidden"),
        place(
            "location:line",
            "map:main",
            "line",
            json!([[-5, 0], [5, 0]]),
        ),
        place(
            "location:polygon",
            "map:main",
            "polygon",
            json!([[-4, -4], [4, -4], [4, 4], [-4, 4], [-4, -4]]),
        ),
        place("location:point", "map:main", "point", json!([1, 0])),
    ]);
    valid(&world);
    let result = geometry::bbox(&world, "map:main", [0.0, 0.0], [1.0, 0.0], 3).unwrap();
    assert_eq!(
        ids(&result),
        ["location:line", "location:point", "location:polygon"]
    );
    // The polygon's envelope contains this interior box; bbox is not an edge test.
    assert_eq!(result["relation"], "intersects");
    assert_eq!(row(&result, "location:line")["geometryKind"], "line");
    assert_eq!(
        row(&result, "location:line")["coordinates"],
        json!([["-5", "0"], ["5", "0"]])
    );
    let within =
        geometry::bbox_relation(&world, "map:main", &[0.0, 0.0], &[1.0, 0.0], "within", 3).unwrap();
    assert_eq!(ids(&within), ["location:point"]);
    let all = geometry::bbox_relation(&world, "map:main", &[-5.0, -4.0], &[5.0, 4.0], "within", 3)
        .unwrap();
    assert_eq!(ids(&all), ids(&result));
    assert!(geometry::bbox(&world, "map:main", [0.0, 0.0], [1.0, 0.0], 2).is_err());
    assert!(
        geometry::bbox_relation(&world, "map:main", &[0.0, 0.0], &[1.0, 0.0], "contains", 3)
            .is_err()
    );
}

#[test]
fn proximity_uses_segments_and_filled_polygons_not_their_envelopes() {
    let world = world(vec![
        map("map:main", 2, "forbidden"),
        place(
            "location:segment",
            "map:main",
            "line",
            json!([[-5, 0], [5, 0]]),
        ),
        place(
            "location:corner",
            "map:main",
            "line",
            json!([[0.9, 0.9], [1, 1]]),
        ),
        place(
            "location:filled",
            "map:main",
            "polygon",
            json!([[-4, -4], [4, -4], [4, 4], [-4, 4], [-4, -4]]),
        ),
        place(
            "location:degenerate",
            "map:main",
            "line",
            json!([[2, 2], [2, 2]]),
        ),
    ]);
    valid(&world);
    let near = geometry::nearby(&world, "map:main", [0.0, 0.0], 1.0, 100).unwrap();
    assert_eq!(ids(&near), ["location:filled", "location:segment"]);
    assert_eq!(row(&near, "location:filled")["distance"], 0.0);
    let segment = geometry::nearby(&world, "map:main", [0.0, -1.0], 1.0, 100).unwrap();
    assert_eq!(row(&segment, "location:segment")["distance"], 1.0);
    let edge = geometry::nearby(&world, "map:main", [4.0, 2.0], 0.0, 100).unwrap();
    assert!(ids(&edge).contains(&"location:filled"));
    let degenerate = geometry::nearby(&world, "map:main", [2.0, 2.0], 0.0, 100).unwrap();
    assert!(ids(&degenerate).contains(&"location:degenerate"));
}

#[test]
fn three_dimensional_bounds_and_polygon_plane_distances_use_all_ordinates() {
    let world = world(vec![
        map("map:main", 3, "required"),
        place("location:point", "map:main", "point", json!([0, 0, 4])),
        place(
            "location:segment",
            "map:main",
            "line",
            json!([[-2, 0, 2], [2, 0, 2]]),
        ),
        place(
            "location:polygon",
            "map:main",
            "polygon",
            json!([[-2, -2, 2], [2, -2, 2], [2, 2, 2], [-2, 2, 2], [-2, -2, 2]]),
        ),
    ]);
    valid(&world);
    let at_zero =
        geometry::bbox(&world, "map:main", [-1.0, -1.0, 0.0], [1.0, 1.0, 0.0], 100).unwrap();
    assert!(ids(&at_zero).is_empty());
    let at_two =
        geometry::bbox(&world, "map:main", [-1.0, -1.0, 2.0], [1.0, 1.0, 2.0], 100).unwrap();
    assert_eq!(ids(&at_two), ["location:polygon", "location:segment"]);
    let near = geometry::nearby(&world, "map:main", [0.0, 1.0, 3.0], 1.0, 100).unwrap();
    assert_eq!(ids(&near), ["location:polygon"]);
    assert_eq!(near["items"][0]["distance"], 1.0);
    let outside = geometry::nearby(&world, "map:main", [3.0, 0.0, 3.0], 2.0, 100).unwrap();
    let d = row(&outside, "location:polygon")["distance"]
        .as_f64()
        .unwrap();
    assert!((d - 2.0_f64.sqrt()).abs() < 1e-12);
    assert!(geometry::bbox(&world, "map:main", [0.0, 0.0], [1.0, 1.0], 100).is_err());
    assert!(geometry::nearby(&world, "map:main", [0.0, 0.0], 1.0, 100).is_err());
}

#[test]
fn geographic_nearby_uses_authored_degree_coordinates_without_wrapping_or_units() {
    let mut earth = map("map:earth", 2, "forbidden");
    earth["crs"] = json!("EPSG:4326");
    earth["axis_order"] = json!(["longitude", "latitude"]);
    earth["unit"] = json!("degree");
    earth["bounds"] = json!({"min":[-180,-90],"max":[180,90]});
    let world = world(vec![
        earth,
        place("location:west", "map:earth", "point", json!([-179, 0])),
        place("location:east", "map:earth", "point", json!([179, 0])),
    ]);
    valid(&world);
    let result = geometry::nearby(&world, "map:earth", [180.0, 0.0], 2.0, 100).unwrap();
    assert_eq!(ids(&result), ["location:east"]);
    assert_eq!(result["unit"], "degree");
    assert_eq!(result["items"][0]["distance"], 1.0);
    assert!(geometry::bbox(&world, "map:earth", [170.0, -10.0], [-170.0, 10.0], 100).is_err());
    for (field, value) in [
        ("axis_order", "[east,north]"),
        ("unit", "m"),
        ("bounds", "{min: [-181,-90], max: [180,90]}"),
        ("bounds", "{min: [-180,-90], max: [180,91]}"),
    ] {
        invalid(&edited(&world, "map:earth", |v| v[field] = yaml(value)))
    }
}

#[test]
fn closed_shapes_dimensions_vertex_limits_and_policy_failures_are_validated() {
    let base = world(vec![
        map("map:main", 2, "forbidden"),
        place("location:shape", "map:main", "point", json!([0, 0])),
    ]);
    for value in [
        "{kind: line, coordinates: [[0,0]]}",
        "{kind: line, coordinates: [[0,0],[1,1,1]]}",
        "{kind: polygon, coordinates: [[0,0],[1,0],[0,1]]}",
        "{kind: polygon, coordinates: [[0,0],[1,0],[0,1],[1,1]]}",
        "{kind: polygon, coordinates: [[0,0],[1,0],[0,1],[0,0]], holes: []}",
        "{kind: line, coordinates: [[0,0],[11,1]]}",
        "{kind: line, coordinates: [[0,0],[.nan,1]]}",
        "{kind: line, coordinates: [[0,0],[true,1]]}",
        "{kind: line, coordinates: [[0,0],[!tag 1,1]]}",
        "!tag {kind: point, coordinates: [0,0]}",
    ] {
        invalid(&edited(&base, "location:shape", |v| {
            v["spatial"]["geometry"] = yaml(value)
        }))
    }
    let excessive = Yaml::Sequence((0..10_001).map(|_| yaml("[0,0]")).collect());
    invalid(&edited(&base, "location:shape", |v| {
        v["spatial"]["geometry"]["kind"] = "line".into();
        v["spatial"]["geometry"]["coordinates"] = excessive
    }));
    for (field, value) in [
        ("z_policy", "required"),
        ("z_policy", "unknown"),
        ("bounds", "{min: [-1,-1,-1],max: [1,1,1]}"),
        ("origin", "{label: Origin, coordinates: [0,0,0]}"),
        ("origin", "{label: '',coordinates: [0,0]}"),
        ("scale", "{value: -1,unit: m}"),
        ("scale", "{value: 1,unit: m,extra: true}"),
    ] {
        invalid(&edited(&base, "map:main", |v| v[field] = yaml(value)));
    }
    valid(&edited(&base, "map:main", |v| {
        v["z_policy"] = "optional-level".into()
    }));
}

#[test]
fn degenerate_and_tiny_shapes_do_not_create_false_zero_distances() {
    let world = world(vec![
        map("map:main", 2, "forbidden"),
        place(
            "location:tiny-point",
            "map:main",
            "point",
            json!([1e-200, 0.0]),
        ),
        place(
            "location:tiny-line",
            "map:main",
            "line",
            json!([[1e-200, 0.0], [2e-200, 0.0]]),
        ),
        place(
            "location:flat",
            "map:main",
            "polygon",
            json!([[1.0, 1.0], [2.0, 2.0], [3.0, 3.0], [1.0, 1.0]]),
        ),
    ]);
    valid(&world);
    assert!(ids(&geometry::nearby(&world, "map:main", [0.0, 0.0], 0.0, 100).unwrap()).is_empty());
    let small = geometry::nearby(&world, "map:main", [0.0, 0.0], 1e-200, 100).unwrap();
    assert_eq!(ids(&small), ["location:tiny-line", "location:tiny-point"]);
    assert_eq!(
        row(&small, "location:tiny-line")["distance"]
            .as_f64()
            .unwrap(),
        1e-200
    );
}

#[test]
fn three_dimensional_collinearity_and_tiny_planes_preserve_real_distances() {
    let vertices = json!([
        [0, 0, 0],
        [17, -4, -3],
        [51, -12, -9],
        [119, -28, -21],
        [0, 0, 0]
    ]);
    let mut broad = map("map:main", 3, "required");
    broad["bounds"] = json!({"min":[-200,-200,-200],"max":[200,200,200]});
    let world = world(vec![
        broad,
        place(
            "location:collinear",
            "map:main",
            "polygon",
            vertices.clone(),
        ),
        place("location:line", "map:main", "line", vertices),
        place(
            "location:tiny-face",
            "map:main",
            "polygon",
            json!([
                [0.0, 0.0, 0.0],
                [1e-200, 0.0, 0.0],
                [0.0, 1e-200, 0.0],
                [0.0, 0.0, 0.0]
            ]),
        ),
    ]);
    valid(&world);
    let point = [8.411162761726128, -1.979097120406148, -0.4843228403046109];
    assert!(ids(&geometry::nearby(&world, "map:main", point, 0.6, 100).unwrap()).is_empty());
    let result = geometry::nearby(&world, "map:main", point, 1.0, 100).unwrap();
    let polygon = row(&result, "location:collinear")["distance"]
        .as_f64()
        .unwrap();
    let line = row(&result, "location:line")["distance"].as_f64().unwrap();
    assert_eq!(polygon, line);
    assert!((polygon - 0.9855645994138759).abs() < 1e-12);
    let tiny =
        geometry::nearby(&world, "map:main", [2e-201, 2e-201, 1e-201], 1.1e-201, 100).unwrap();
    assert!(ids(&tiny).contains(&"location:tiny-face"));
    assert_eq!(
        row(&tiny, "location:tiny-face")["distance"]
            .as_f64()
            .unwrap(),
        1e-201
    );
}

#[test]
fn literal_anchors_preserve_direction_and_prose_without_inferred_conversion() {
    let mut retired = anchor(
        "anchor:retired",
        "map:a",
        json!([1, 2]),
        "map:b",
        json!([3, 4, 5]),
    );
    retired["status"] = json!("retired");
    let world = world(vec![
        map("map:a", 2, "forbidden"),
        map("map:b", 3, "required"),
        anchor(
            "anchor:a-b",
            "map:a",
            json!([1, 2]),
            "map:b",
            json!([3, 4, 5]),
        ),
        retired,
    ]);
    valid(&world);
    let result = geometry::anchors(&world, Some("map:a"), Some("map:b"), 1).unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        result["items"][0]["from"],
        json!({"map_id":"map:a","coordinates":["1","2"]})
    );
    assert_eq!(
        result["items"][0]["to"],
        json!({"map_id":"map:b","coordinates":["3","4","5"]})
    );
    assert_eq!(
        result["items"][0]["conversion"],
        "Authored correspondence only."
    );
    assert_eq!(
        result["items"][0]["citation"]["fingerprint"],
        world.fingerprint
    );
    assert!(
        geometry::anchors(&world, Some("map:b"), Some("map:a"), 1).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(geometry::anchors(&world, Some("map:unknown"), None, 100).is_err());
}

#[test]
fn anchors_enforce_exact_map_endpoints_capabilities_and_canonical_references() {
    let base = world(vec![
        map("map:a", 2, "forbidden"),
        map("map:b", 3, "required"),
        anchor(
            "anchor:a-b",
            "map:a",
            json!([1, 2]),
            "map:b",
            json!([3, 4, 5]),
        ),
    ]);
    for (field, value) in [
        ("from", "{map_id: map:a,coordinates: [1,2,3]}"),
        ("from", "{map_id: map:a,coordinates: [20,2]}"),
        ("to", "{map_id: map:a,coordinates: [1,2]}"),
        ("to", "{map_id: map:unknown,coordinates: [1,2]}"),
        ("from", "{map_id: map:a,coordinates: [1,2],extra: true}"),
        ("conversion", "null"),
        ("conversion", "!tag literal"),
    ] {
        invalid(&edited(&base, "anchor:a-b", |v| v[field] = yaml(value)));
    }
    invalid(&edited(&base, "map:b", |v| v["status"] = "retired".into()));
    let retired = edited(
        &edited(&base, "anchor:a-b", |v| v["status"] = "retired".into()),
        "map:b",
        |v| v["status"] = "retired".into(),
    );
    valid(&retired);
    invalid(&edited(&base, WORLD, |v| {
        v["capabilities"] = yaml("[spatial-core-v1]")
    }));
    let changed = edited(&base, "anchor:a-b", |v| {
        v.as_mapping_mut().unwrap().remove("conversion");
    });
    valid(&changed);
}

#[test]
fn geometry_and_anchors_never_create_routes_or_character_evidence() {
    let minimal = world(vec![]);
    let placed = world(vec![
        map("map:a", 2, "forbidden"),
        map("map:b", 2, "forbidden"),
        place("location:a", "map:a", "line", json!([[0, 0], [1, 1]])),
        place(
            "location:b",
            "map:b",
            "polygon",
            json!([[0, 0], [1, 0], [0, 1], [0, 0]]),
        ),
        anchor("anchor:a-b", "map:a", json!([0, 0]), "map:b", json!([0, 0])),
    ]);
    valid(&placed);
    assert_eq!(
        spatial::path(&placed, "location:a", "location:b", None, "hops", None, 100).unwrap()["found"],
        false
    );
    let at = placed.story_time(0, None, 0).unwrap();
    assert_eq!(
        character::context(&minimal, HERO, &at, 100_000).unwrap(),
        character::context(&placed, HERO, &at, 100_000).unwrap()
    );
    assert_eq!(
        character::search(&minimal, HERO, "geometry", &at, 20).unwrap(),
        character::search(&placed, HERO, "geometry", &at, 20).unwrap()
    );
}
