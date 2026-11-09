use serde_json::{Value as Json, json};
use serde_yaml::{Mapping, Value as Yaml};
use std::{collections::BTreeSet, path::PathBuf};
use wedl::{
    model::{Record, World},
    source, spatial, validation,
};

fn record(kind: &str, id: &str, fields: &str) -> Record {
    let text = format!(
        "---\nschema: {}\nkind: {kind}\nid: {id}\ntitle: {id}\n{fields}---\n",
        source::SOURCE_SCHEMA
    );
    source::parse_record(
        text.as_bytes(),
        &format!("story/{}.md", id.replace(':', "-")),
    )
    .unwrap()
}

fn route(id: &str, from: &str, to: &str, fields: &str) -> Record {
    let mut values: Mapping = serde_yaml::from_str(&format!(
        "from_location_id: location:{from}\nto_location_id: location:{to}\ndirection: one-way\nmodes: [foot]\n"
    )).unwrap();
    if !fields.is_empty() {
        values.extend(serde_yaml::from_str::<Mapping>(fields).unwrap());
    }
    record(
        "route",
        &format!("route:{id}"),
        &serde_yaml::to_string(&values).unwrap(),
    )
}

fn world(mut routes: Vec<Record>) -> World {
    let mut locations = BTreeSet::from([
        "location:a".to_owned(),
        "location:d".to_owned(),
        "location:island".to_owned(),
    ]);
    for route in &routes {
        for field in ["from_location_id", "to_location_id"] {
            let Yaml::String(id) = route.get(field).unwrap() else {
                panic!("expected location ID");
            };
            locations.insert(id.clone());
        }
    }
    routes.extend(locations.iter().map(|id| record("location", id, "")));
    routes.push(record("world", "world_00000000000000000000000000", "capabilities: [spatial-core-v1, route-v1]\ndefault_timeline: main\ntimelines: [{id: main, label: Main}]\nstate_keys: {}\n"));
    let world = source::world_from_records(
        PathBuf::new(),
        routes,
        Some("weighted-test-revision".into()),
    )
    .unwrap();
    let validation = validation::validate_world(&world);
    assert!(validation.valid, "{:#?}", validation.diagnostics);
    world
}

fn path(world: &World, metric: &str, unit: Option<&str>) -> Json {
    spatial::path(world, "location:a", "location:d", None, metric, unit, 100).unwrap()
}

#[test]
fn each_metric_uses_its_authored_weight_and_hops_remains_independent() {
    let world = world(vec![
        route(
            "direct",
            "a",
            "d",
            "route_distance: {value: 10, unit: m}\ntravel_cost: {value: 1, unit: coin}\nduration: {value: 100, unit: hour}\n",
        ),
        route(
            "ab",
            "a",
            "b",
            "route_distance: {value: 2, unit: m}\ntravel_cost: {value: 4, unit: coin}\nduration: {value: 3, unit: hour}\n",
        ),
        route(
            "bd",
            "b",
            "d",
            "route_distance: {value: 2, unit: m}\ntravel_cost: {value: 4, unit: coin}\nduration: {value: 3, unit: hour}\n",
        ),
        route(
            "ac",
            "a",
            "c",
            "route_distance: {value: 3, unit: m}\ntravel_cost: {value: 5, unit: coin}\nduration: {value: 1, unit: hour}\n",
        ),
        route(
            "cd",
            "c",
            "d",
            "route_distance: {value: 3, unit: m}\ntravel_cost: {value: 5, unit: coin}\nduration: {value: 1, unit: hour}\n",
        ),
    ]);
    for (metric, routes, value, unit) in [
        ("route_distance", vec!["route:ab", "route:bd"], "4", "m"),
        ("travel_cost", vec!["route:direct"], "1", "coin"),
        ("duration", vec!["route:ac", "route:cd"], "2", "hour"),
    ] {
        let result = path(&world, metric, None);
        assert_eq!(result["metric"], metric);
        assert_eq!(result["routeIds"], json!(routes));
        assert_eq!(result["total"], json!({"value":value,"unit":unit}));
    }
    let hops = path(&world, "hops", None);
    assert_eq!(hops["routeIds"], json!(["route:direct"]));
    assert_eq!(hops["hops"], 1);
    assert!(hops["total"].is_null());
}

#[test]
fn integers_above_float_precision_and_totals_above_u64_remain_exact() {
    let precision = world(vec![
        route(
            "a-expensive",
            "a",
            "d",
            "duration: {value: 9007199254740993, unit: tick}\n",
        ),
        route(
            "z-cheaper",
            "a",
            "d",
            "duration: {value: 9007199254740992, unit: tick}\n",
        ),
    ]);
    let result = path(&precision, "duration", None);
    assert_eq!(result["routeIds"], json!(["route:z-cheaper"]));
    assert_eq!(result["total"]["value"], "9007199254740992");
    let large_sum = world(vec![
        route(
            "ab",
            "a",
            "b",
            "travel_cost: {value: 18446744073709551615, unit: coin}\n",
        ),
        route(
            "bd",
            "b",
            "d",
            "travel_cost: {value: 18446744073709551615, unit: coin}\n",
        ),
    ]);
    let result = path(&large_sum, "travel_cost", None);
    assert_eq!(
        result["total"],
        json!({"value":"36893488147419103230","unit":"coin"})
    );
    assert_eq!(
        result["edges"][0]["travelCost"]["value"],
        "18446744073709551615"
    );
}

#[test]
fn decimal_ordering_addition_and_full_finite_exponent_range_are_exact() {
    let decimal = world(vec![
        route(
            "a-rounded",
            "a",
            "d",
            "duration: {value: 0.30000000000000004, unit: hour}\n",
        ),
        route("z-first", "a", "b", "duration: {value: 0.1, unit: hour}\n"),
        route("z-last", "b", "d", "duration: {value: 0.2, unit: hour}\n"),
    ]);
    let result = path(&decimal, "duration", None);
    assert_eq!(result["routeIds"], json!(["route:z-first", "route:z-last"]));
    assert_eq!(result["total"]["value"], "0.3");
    let extremes = world(vec![
        route("ab", "a", "b", "duration: {value: 1.0e308, unit: tick}\n"),
        route("bd", "b", "d", "duration: {value: 5.0e-324, unit: tick}\n"),
    ]);
    assert_eq!(
        path(&extremes, "duration", None)["total"]["value"],
        format!("1{}.{}5", "0".repeat(308), "0".repeat(323))
    );
    let fractional = world(vec![
        route("ab", "a", "b", "duration: {value: 12, unit: hour}\n"),
        route("bd", "b", "d", "duration: {value: 0.025, unit: hour}\n"),
    ]);
    assert_eq!(
        path(&fractional, "duration", None)["total"]["value"],
        "12.025"
    );
}

#[test]
fn only_successful_same_unit_paths_participate_in_unit_inference() {
    let mixed = world(vec![
        route("ab", "a", "b", "route_distance: {value: 1, unit: km}\n"),
        route("bd", "b", "d", "route_distance: {value: 1, unit: m}\n"),
        route(
            "dead",
            "a",
            "island",
            "route_distance: {value: 1, unit: league}\n",
        ),
    ]);
    let result = path(&mixed, "route_distance", None);
    assert_eq!(result["found"], false);
    assert_eq!(result["locationIds"], json!([]));
    assert!(result["total"].is_null());
    let unique = world(vec![
        route("direct", "a", "d", "route_distance: {value: 10, unit: m}\n"),
        route(
            "dead",
            "a",
            "island",
            "route_distance: {value: 1, unit: league}\n",
        ),
    ]);
    assert_eq!(
        path(&unique, "route_distance", None)["total"],
        json!({"value":"10","unit":"m"})
    );
    assert_eq!(path(&unique, "route_distance", Some("M"))["found"], false);
    let ambiguous = world(vec![
        route(
            "meters",
            "a",
            "d",
            "route_distance: {value: 1000, unit: m}\n",
        ),
        route(
            "kilometers",
            "a",
            "d",
            "route_distance: {value: 1, unit: km}\n",
        ),
    ]);
    let error = spatial::path(
        &ambiguous,
        "location:a",
        "location:d",
        None,
        "route_distance",
        None,
        100,
    )
    .unwrap_err();
    assert_eq!(error.code(), "usage_error");
    assert!(error.to_string().contains("specify unit"));
    assert_eq!(
        path(&ambiguous, "route_distance", Some("km"))["routeIds"],
        json!(["route:kilometers"])
    );
}

#[test]
fn missing_metrics_are_excluded_and_never_fall_back_to_hops_or_zero() {
    let world = world(vec![
        route("direct", "a", "d", ""),
        route("ab", "a", "b", "duration: {value: 0, unit: hour}\n"),
        route("bd", "b", "d", "route_distance: {value: 2, unit: m}\n"),
    ]);
    assert_eq!(path(&world, "hops", None)["found"], true);
    for metric in ["duration", "route_distance", "travel_cost"] {
        let result = path(&world, metric, None);
        assert_eq!(result["found"], false);
        assert_eq!(result["routeIds"], json!([]));
        assert_eq!(result["edges"], json!([]));
        assert!(result["hops"].is_null());
        assert!(result["total"].is_null());
    }
}

#[test]
fn zero_cycles_parallel_routes_and_equal_totals_have_stable_ties() {
    let mut routes = vec![
        route("z-first", "a", "b", "duration: {value: 0, unit: hour}\n"),
        route("a-last", "b", "d", "duration: {value: 0, unit: hour}\n"),
        route("a-first", "a", "c", "duration: {value: 0, unit: hour}\n"),
        route("z-last", "c", "d", "duration: {value: 0, unit: hour}\n"),
        route("cycle", "c", "a", "duration: {value: 0, unit: hour}\n"),
        route("z-parallel", "a", "c", "duration: {value: 0, unit: hour}\n"),
    ];
    let result = path(&world(routes.clone()), "duration", None);
    assert_eq!(result["routeIds"], json!(["route:a-first", "route:z-last"]));
    assert_eq!(result["total"]["value"], "0");
    routes.reverse();
    assert_eq!(
        path(&world(routes.clone()), "duration", None)["routeIds"],
        result["routeIds"]
    );
    routes.push(route(
        "zz-direct",
        "a",
        "d",
        "duration: {value: 0, unit: hour}\n",
    ));
    assert_eq!(
        path(&world(routes), "duration", None)["routeIds"],
        json!(["route:zz-direct"])
    );
}

#[test]
fn weighted_traversal_preserves_direction_mode_status_and_availability() {
    let world = world(vec![
        route(
            "ab",
            "a",
            "b",
            "direction: two-way\nduration: {value: 1, unit: hour}\n",
        ),
        route("bd", "b", "d", "duration: {value: 1, unit: hour}\n"),
        route(
            "boat",
            "a",
            "d",
            "modes: [boat]\nduration: {value: 0, unit: hour}\n",
        ),
        route(
            "closed",
            "a",
            "d",
            "availability: closed\nduration: {value: 0, unit: hour}\n",
        ),
        route(
            "restricted",
            "a",
            "d",
            "availability: restricted\nduration: {value: 0, unit: hour}\n",
        ),
        route(
            "unknown",
            "a",
            "d",
            "availability: unknown\nduration: {value: 0, unit: hour}\n",
        ),
        route(
            "retired",
            "a",
            "d",
            "status: retired\nduration: {value: 0, unit: hour}\n",
        ),
    ]);
    let foot = spatial::path(
        &world,
        "location:a",
        "location:d",
        Some("foot"),
        "duration",
        None,
        100,
    )
    .unwrap();
    assert_eq!(foot["routeIds"], json!(["route:ab", "route:bd"]));
    assert_eq!(foot["total"]["value"], "2");
    let boat = spatial::path(
        &world,
        "location:a",
        "location:d",
        Some("boat"),
        "duration",
        None,
        100,
    )
    .unwrap();
    assert_eq!(boat["routeIds"], json!(["route:boat"]));
    let reverse = spatial::path(
        &world,
        "location:b",
        "location:a",
        Some("foot"),
        "duration",
        None,
        100,
    )
    .unwrap();
    assert_eq!(reverse["edges"][0]["fromLocationId"], "location:b");
    assert_eq!(reverse["edges"][0]["toLocationId"], "location:a");
    assert_eq!(
        spatial::path(
            &world,
            "location:d",
            "location:a",
            Some("foot"),
            "duration",
            None,
            100
        )
        .unwrap()["found"],
        false
    );
    let citation = &foot["edges"][0]["citation"];
    assert_eq!(citation["entityId"], "route:ab");
    assert_eq!(citation["revision"], "weighted-test-revision");
    assert_eq!(citation["fingerprint"], world.fingerprint);
    assert!(citation["sourcePath"].is_string());
    assert_eq!(
        citation["blobOid"],
        json!(world.records["route:ab"].blob_oid)
    );
    assert_eq!(foot["scope"], "author-all-time-spatial-source");
    assert_eq!(foot["perspective"], "author");
    assert!(foot.get("effectiveTime").is_none());
}

#[test]
fn zero_hop_totals_and_complete_result_bounds_are_explicit() {
    let world = world(vec![
        route("ab", "a", "b", "duration: {value: 1, unit: hour}\n"),
        route("bd", "b", "d", "duration: {value: 1, unit: hour}\n"),
    ]);
    for unit in [None, Some("custom-unit")] {
        let result = spatial::path(
            &world,
            "location:a",
            "location:a",
            None,
            "duration",
            unit,
            1,
        )
        .unwrap();
        assert_eq!(result["total"], json!({"value":"0","unit":unit}));
        assert_eq!(result["hops"], 0);
        assert_eq!(result["locationIds"], json!(["location:a"]));
        assert_eq!(result["routeIds"], json!([]));
    }
    assert!(
        spatial::path(
            &world,
            "location:a",
            "location:d",
            None,
            "duration",
            None,
            2
        )
        .unwrap_err()
        .to_string()
        .contains("complete spatial result")
    );
    assert_eq!(
        spatial::path(
            &world,
            "location:a",
            "location:d",
            None,
            "duration",
            None,
            3
        )
        .unwrap()["locationIds"],
        json!(["location:a", "location:b", "location:d"])
    );
    assert_eq!(
        spatial::path(
            &world,
            "location:a",
            "location:island",
            None,
            "duration",
            None,
            1
        )
        .unwrap()["found"],
        false
    );
    for (metric, unit) in [
        ("hops", Some("hour")),
        ("Hops", None),
        ("cost", None),
        ("duration", Some("")),
        ("duration", Some("  ")),
    ] {
        assert_eq!(
            spatial::path(&world, "location:a", "location:d", None, metric, unit, 100)
                .unwrap_err()
                .code(),
            "usage_error"
        );
    }
}
