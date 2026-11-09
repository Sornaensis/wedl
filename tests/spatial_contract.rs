use serde_json::json;
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    model::{Record, World},
    source, spatial, validation,
};

fn record(kind: &str, id: &str, fields: &str) -> Record {
    let text = format!(
        "---\nschema: {}\nkind: {kind}\nid: {id}\ntitle: {id}\n{fields}---\nUntimed author prose\n",
        source::SOURCE_SCHEMA
    );
    source::parse_record(
        text.as_bytes(),
        &format!("story/{}-{}.md", kind, id.replace([':', '/'], "-")),
    )
    .unwrap()
}
fn world(mut records: Vec<Record>) -> World {
    records.push(record("world","world_00000000000000000000000000","status: canonical\ncapabilities: [spatial-core-v1, route-v1]\ndefault_timeline: main\ntimelines: [{id: main, label: Main}]\nstate_keys: {}\n"));
    source::world_from_records(PathBuf::new(), records, None).unwrap()
}
fn loc(id: &str, fields: &str) -> Record {
    record("location", id, fields)
}
fn route(id: &str, from: &str, to: &str, fields: &str) -> Record {
    record(
        "route",
        id,
        &format!(
            "from_location_id: {from}\nto_location_id: {to}\ndirection: one-way\nmodes: [foot]\n{fields}"
        ),
    )
}
fn base() -> World {
    world(vec![
        loc("location:root", "parent_id: null\n"),
        loc("location:a", "parent_id: location:root\n"),
        loc("location:b", "parent_id: location:root\n"),
        loc("location:c", "parent_id: location:a\n"),
        loc("location:island", ""),
        route("route:ab", "location:a", "location:b", ""),
    ])
}
fn edited(world: &World, id: &str, edit: impl FnOnce(&mut Yaml)) -> World {
    let mut records = world.records.clone();
    let record = records.get_mut(id).unwrap();
    edit(&mut record.frontmatter);
    let text = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&record.frontmatter).unwrap(),
        record.body
    );
    *record = source::parse_record(text.as_bytes(), &record.source_path).unwrap();
    source::world_from_records(
        world.root.clone(),
        records.into_values().collect(),
        world.revision.clone(),
    )
    .unwrap()
}
fn yaml(text: &str) -> Yaml {
    serde_yaml::from_str(text).unwrap()
}
fn valid(world: &World) {
    let report = validation::validate_world(world);
    assert!(report.valid, "{:#?}", report.diagnostics);
    assert!(spatial::validate(world).is_empty());
}
fn invalid(world: &World) {
    assert!(!spatial::validate(world).is_empty());
}

#[test]
fn authored_containment_returns_nearest_parents_and_direct_children() {
    let world = base();
    valid(&world);
    let c = spatial::location(&world, "location:c", None, 100).unwrap();
    assert_eq!(c["parentId"], "location:a");
    assert_eq!(
        c["ancestors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["locationId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["location:a", "location:root"]
    );
    assert!(c["children"].as_array().unwrap().is_empty());
    let root = spatial::location(&world, "location:root", None, 100).unwrap();
    assert!(root["parentId"].is_null());
    assert_eq!(
        root["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["locationId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["location:a", "location:b"]
    );
    assert_eq!(c["scope"], "author-all-time-spatial-source");
    assert_eq!(c["perspective"], "author");
    assert!(c.get("effectiveTime").is_none());
    assert!(
        !serde_json::to_string(&c)
            .unwrap()
            .contains("Untimed author prose")
    );
}

#[test]
fn wrong_parent_shapes_kinds_legacy_aliases_and_cycles_are_invalid() {
    let world = base();
    for value in [
        "true",
        "[]",
        "{}",
        "''",
        "location:missing",
        "route:ab",
        "location:a",
        "!tag null",
        "!tag location:root",
    ] {
        invalid(&edited(&world, "location:a", |v| {
            v["parent_id"] = yaml(value)
        }));
    }
    invalid(&edited(&world, "location:a", |v| {
        v["parent"] = yaml("location:root")
    }));
    let cycle = edited(&world, "location:root", |v| {
        v["parent_id"] = yaml("location:c")
    });
    assert!(
        spatial::validate(&cycle)
            .iter()
            .any(|d| d.code == "WDL-SPATIAL-002")
    );
    assert!(spatial::location(&cycle, "location:a", None, 100).is_err());
    let retired = edited(&world, "location:root", |v| v["status"] = yaml("retired"));
    invalid(&retired);
}

#[test]
fn directed_modes_availability_and_retired_routes_control_edges() {
    let world = base();
    let forward = spatial::path(
        &world,
        "location:a",
        "location:b",
        Some("foot"),
        "hops",
        None,
        100,
    )
    .unwrap();
    assert_eq!(forward["found"], true);
    assert_eq!(forward["hops"], 1);
    assert_eq!(forward["routeIds"], json!(["route:ab"]));
    assert_eq!(
        spatial::path(&world, "location:b", "location:a", None, "hops", None, 100).unwrap()["found"],
        false
    );
    assert_eq!(
        spatial::path(
            &world,
            "location:a",
            "location:b",
            Some("boat"),
            "hops",
            None,
            100
        )
        .unwrap()["found"],
        false
    );
    for availability in ["closed", "restricted", "unknown"] {
        let changed = edited(&world, "route:ab", |v| {
            v["availability"] = yaml(availability)
        });
        valid(&changed);
        assert!(
            spatial::location(&changed, "location:a", None, 100).unwrap()["outgoingRoutes"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            spatial::path(
                &changed,
                "location:a",
                "location:b",
                None,
                "hops",
                None,
                100
            )
            .unwrap()["found"],
            false
        );
    }
    let retired = edited(&world, "route:ab", |v| v["status"] = yaml("retired"));
    valid(&retired);
    assert_eq!(
        spatial::path(
            &retired,
            "location:a",
            "location:b",
            None,
            "hops",
            None,
            100
        )
        .unwrap()["found"],
        false
    );
    let reverse = edited(&world, "route:ab", |v| v["direction"] = yaml("two-way"));
    valid(&reverse);
    let value = spatial::path(
        &reverse,
        "location:b",
        "location:a",
        None,
        "hops",
        None,
        100,
    )
    .unwrap();
    assert_eq!(value["locationIds"], json!(["location:b", "location:a"]));
    assert_eq!(value["edges"][0]["fromLocationId"], "location:b");
    assert_eq!(value["edges"][0]["toLocationId"], "location:a");
}

#[test]
fn route_shapes_and_typed_metrics_are_checked_for_every_status() {
    let world = base();
    for (field, value) in [
        ("from_location_id", "location:missing"),
        ("to_location_id", "location:a"),
        ("to_location_id", "route:ab"),
        ("direction", "both"),
        ("direction", "!tag one-way"),
        ("modes", "[]"),
        ("modes", "[foot, foot]"),
        ("modes", "[true]"),
        ("modes", "!tag [foot]"),
        ("availability", "null"),
        ("availability", "maybe"),
        ("uncertainty", "false"),
        ("status", "draft"),
        ("status", "!tag canonical"),
        ("route_distance", "{value: -1, unit: m}"),
        ("route_distance", "{value: .nan, unit: m}"),
        ("route_distance", "{value: .inf, unit: m}"),
        ("route_distance", "{value: true, unit: m}"),
        ("route_distance", "{value: 1, unit: ''}"),
        ("route_distance", "{value: 1, unit: m, extra: true}"),
        ("route_distance", "{value: !tag 1, unit: m}"),
        ("route_distance", "{value: 1, unit: !tag m}"),
        ("route_distance", "!tag {value: 1, unit: m}"),
    ] {
        invalid(&edited(&world, "route:ab", |v| v[field] = yaml(value)));
    }
    let retired = edited(&world, "route:ab", |v| {
        v["status"] = yaml("retired");
        v["modes"] = yaml("[]");
    });
    invalid(&retired);
    let retired_endpoint = edited(&world, "location:b", |v| v["status"] = yaml("retired"));
    invalid(&retired_endpoint);
    let absent = edited(&world, "route:ab", |v| {
        v.as_mapping_mut().unwrap().remove("direction");
    });
    invalid(&absent);
}

#[test]
fn authored_metrics_keep_integer_precision_without_affecting_hop_choice() {
    let world = edited(&base(), "route:ab", |v| {
        v["route_distance"] = yaml("{value: 18446744073709551615, unit: m}");
        v["travel_cost"] = yaml("{value: 0.25, unit: coins}");
        v["duration"] = yaml("{value: 0, unit: hours}");
        v["uncertainty"] = yaml("estimated");
    });
    valid(&world);
    let value = spatial::path(&world, "location:a", "location:b", None, "hops", None, 100).unwrap();
    assert_eq!(value["metric"], "hops");
    assert_eq!(
        value["edges"][0]["routeDistance"]["value"],
        "18446744073709551615"
    );
    assert_eq!(value["edges"][0]["travelCost"]["value"], 0.25);
    assert_eq!(value["edges"][0]["duration"]["value"], "0");
    assert_eq!(value["hops"], 1);
    let unknown =
        spatial::path(&base(), "location:a", "location:b", None, "hops", None, 100).unwrap();
    assert!(unknown["edges"][0].get("duration").is_none());
    assert!(value.get("duration").is_none());
}

#[test]
fn fewest_hops_and_stable_route_ids_handle_cycles_and_parallel_routes() {
    let records = vec![
        loc("location:a", ""),
        loc("location:b", ""),
        loc("location:c", ""),
        loc("location:d", ""),
        route(
            "route:z",
            "location:a",
            "location:c",
            "route_distance: {value: 0, unit: m}\n",
        ),
        route(
            "route:a",
            "location:a",
            "location:b",
            "route_distance: {value: 1000, unit: m}\n",
        ),
        route("route:aa", "location:a", "location:b", ""),
        route("route:bd", "location:b", "location:d", ""),
        route("route:cd", "location:c", "location:d", ""),
        route("route:cycle", "location:b", "location:a", ""),
    ];
    let first = world(records.clone());
    valid(&first);
    let value = spatial::path(&first, "location:a", "location:d", None, "hops", None, 100).unwrap();
    assert_eq!(value["routeIds"], json!(["route:a", "route:bd"]));
    assert_eq!(value["hops"], 2);
    let second = world(records.into_iter().rev().collect());
    assert_eq!(
        value,
        spatial::path(&second, "location:a", "location:d", None, "hops", None, 100).unwrap()
    );
    let mut direct = first
        .records
        .values()
        .filter(|r| r.kind != "world")
        .cloned()
        .collect::<Vec<_>>();
    direct.push(route("route:zz-direct", "location:a", "location:d", ""));
    assert_eq!(
        spatial::path(
            &world(direct),
            "location:a",
            "location:d",
            None,
            "hops",
            None,
            100
        )
        .unwrap()["routeIds"],
        json!(["route:zz-direct"])
    );
}

#[test]
fn zero_hop_unreachable_and_all_time_source_provenance_are_explicit() {
    let mut world = base();
    world.revision = Some("a".repeat(40));
    let same = spatial::path(&world, "location:a", "location:a", None, "hops", None, 1).unwrap();
    assert_eq!(same["found"], true);
    assert_eq!(same["hops"], 0);
    assert_eq!(same["locationIds"], json!(["location:a"]));
    assert_eq!(same["edges"], json!([]));
    let unavailable = spatial::path(
        &world,
        "location:a",
        "location:island",
        None,
        "hops",
        None,
        100,
    )
    .unwrap();
    assert_eq!(unavailable["found"], false);
    assert!(unavailable["hops"].is_null());
    for field in ["locationIds", "routeIds", "edges"] {
        assert_eq!(unavailable[field], json!([]));
    }
    let value = spatial::location(&world, "location:a", None, 100).unwrap();
    assert_eq!(value["citation"]["fingerprint"], world.fingerprint);
    assert_eq!(value["citation"]["revision"], "a".repeat(40));
    assert_eq!(
        value["outgoingRoutes"][0]["citation"]["entityId"],
        "route:ab"
    );
    assert!(
        value["outgoingRoutes"][0]["citation"]
            .get("sourceOrdinal")
            .is_none()
    );
    assert!(
        spatial::path(
            &world,
            "location:a",
            "location:absent",
            None,
            "hops",
            None,
            100
        )
        .is_err()
    );
    let retired = edited(&world, "location:island", |v| v["status"] = yaml("retired"));
    valid(&retired);
    assert!(spatial::location(&retired, "location:island", None, 100).is_err());
}

#[test]
fn complete_result_bounds_fail_instead_of_truncating() {
    let world = base();
    for limit in [0, 1001] {
        assert!(spatial::location(&world, "location:a", None, limit).is_err());
    }
    for mode in ["", " "] {
        assert!(
            spatial::path(
                &world,
                "location:a",
                "location:b",
                Some(mode),
                "hops",
                None,
                100
            )
            .is_err()
        );
    }
    assert!(spatial::location(&world, "location:root", None, 1).is_err());
    assert!(spatial::location(&world, "location:c", None, 1).is_err());
    assert!(spatial::path(&world, "location:a", "location:b", None, "hops", None, 1).is_err());
    assert_eq!(
        spatial::path(&world, "location:a", "location:b", None, "hops", None, 2).unwrap()["hops"],
        1
    );
    let mut records = world
        .records
        .values()
        .filter(|r| r.kind != "world")
        .cloned()
        .collect::<Vec<_>>();
    records.push(route("route:ac", "location:a", "location:c", ""));
    assert!(spatial::location(&crate::world(records), "location:a", None, 1).is_err());
}

#[test]
fn geometry_containment_and_location_links_never_invent_route_edges() {
    let mut records = base()
        .records
        .into_values()
        .filter(|r| r.kind != "world")
        .collect::<Vec<_>>();
    records.push(record("map", "map:test", "crs: local-planar:test\naxis_order: [east, north]\nunit: pace\nbounds: {min: [-1,-1], max: [1,1]}\n"));
    let world = edited(&world(records), "world_00000000000000000000000000", |v| {
        v["capabilities"] = yaml("[spatial-core-v1, geometry-v1, route-v1]");
    });
    let world = edited(&world, "location:a", |v| {
        v["links"] = yaml("[location:island]");
        v["spatial"] = yaml("{map_id: map:test, geometry: {kind: point, coordinates: [0,0]}}");
    });
    valid(&world);
    assert_eq!(
        spatial::path(&world, "location:a", "location:c", None, "hops", None, 100).unwrap()["found"],
        false
    );
    assert_eq!(
        spatial::path(
            &world,
            "location:a",
            "location:root",
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
            "location:a",
            "location:island",
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
fn bounded_traversal_refuses_excessive_expansion_even_when_unreachable() {
    let mut records = vec![loc("location:unreachable", "")];
    for index in 0..=10_000 {
        let id = format!("location:n-{index:05}");
        records.push(loc(&id, ""));
        if index > 0 {
            records.push(route(
                &format!("route:r-{index:05}"),
                &format!("location:n-{:05}", index - 1),
                &id,
                "duration: {value: 1, unit: tick}\n",
            ));
        }
    }
    let world = world(records);
    for metric in ["hops", "duration"] {
        let error = spatial::path(
            &world,
            "location:n-00000",
            "location:unreachable",
            None,
            metric,
            None,
            100,
        )
        .unwrap_err();
        assert!(error.to_string().contains("expansion budget"), "{error}");
    }
}

#[test]
fn all_corpora_validate_and_tideglass_uses_explicit_inverse_routes() {
    let mut locations = 0;
    let mut routes = 0;
    for name in [
        "tideglass",
        "ash_archive",
        "frontiersmen",
        "chronology_conformance",
    ] {
        let world = source::load_world(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join(name),
            None,
        )
        .unwrap();
        valid(&world);
        locations += world
            .records
            .values()
            .filter(|r| r.kind == "location")
            .count();
        routes += world.records.values().filter(|r| r.kind == "route").count();
        if name == "tideglass" {
            assert_eq!(
                spatial::path(&world, "Quay", "Beacon", Some("foot"), "hops", None, 100).unwrap()["routeIds"],
                json!(["route:quay-beacon"])
            );
            assert_eq!(
                spatial::path(&world, "Beacon", "Quay", None, "hops", None, 100).unwrap()["routeIds"],
                json!(["route:beacon-quay"])
            );
            assert_eq!(
                spatial::location(&world, "Quay", None, 100).unwrap()["parentId"],
                "loc_2DW2BZ5PN3XBMD2FWQA4Q61V1E"
            );
        }
    }
    assert_eq!((locations, routes), (58, 2));
}
