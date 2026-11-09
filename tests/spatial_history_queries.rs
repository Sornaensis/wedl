//! Dated spatial consumers retain exact authored history and static query behavior.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    Error, character, geometry,
    model::{Record, StoryTime, World},
    source, spatial, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const HERO: &str = "char_00000000000000000000000001";
const A: &str = "location:moving";
const B: &str = "location:fixed";
const C: &str = "location:unplaced";
const ROUTE: &str = "route:lane";

fn record(mut fields: Json) -> Record {
    fields["schema"] = json!(source::SOURCE_SCHEMA);
    if fields.get("title").is_none() {
        fields["title"] = fields["id"].clone();
    }
    let path = format!(
        "story/{}.md",
        fields["id"].as_str().unwrap().replace([':', '/'], "-")
    );
    source::parse_record(
        format!(
            "---\n{}---\nRetained author source prose.\n",
            serde_yaml::to_string(&fields).unwrap()
        )
        .as_bytes(),
        &path,
    )
    .unwrap()
}
fn history_id(n: u32) -> String {
    format!("spatialhistory_{n:026}")
}
fn point(tick: i64, order: i32) -> Json {
    json!({"timeline":"main","tick":tick,"order":order})
}
fn valid_interval(start: (i64, i32), end: (i64, i32)) -> Json {
    json!({"kind":"interval","timeline":"main","start":point(start.0,start.1),"end":point(end.0,end.1)})
}
fn member(n: u32, aspect: &str, value: Json, valid: Json, replaces: Option<u32>) -> Json {
    let mut value = json!({"id":history_id(n),"aspect":aspect,"valid":valid,"value":value});
    if let Some(previous) = replaces {
        value["replaces"] = json!(history_id(previous));
    }
    value
}
fn placement(map: &str, coords: Json) -> Json {
    json!({"map_id":map,"geometry":{"kind":"point","coordinates":coords}})
}
fn map(id: &str, dimensions: usize) -> Json {
    json!({"kind":"map","id":id,"crs":"local-planar:history","axis_order":["east","north"],"unit":"m",
        "z_policy":if dimensions==3 {"required"}else{"forbidden"},
        "bounds":{"min":vec![-100;dimensions],"max":vec![100;dimensions]}})
}
fn world() -> World {
    let interval = valid_interval((10, 0), (20, 0));
    let values = vec![
        json!({"kind":"world","id":WORLD,"capabilities":["spatial-core-v1","geometry-v1","route-v1","spatial-history-v1"],
            "default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"state_keys":{}}),
        json!({"kind":"character","id":HERO,"initial_state":{}}),
        json!({"kind":"knowledge","id":"know_00000000000000000000000001","knower":HERO,
            "claim":{"key":"lane","statement":"I remember a lane."},
            "transitions":[{"id":"learned","time":{"tick":0},"state":"accepted","acquisition":"observed","confidence":1}]}),
        map("map:a", 2),
        map("map:b", 2),
        map("map:levels", 3),
        json!({"kind":"location","id":A,"spatial":placement("map:a",json!([-50,-50])),
        "spatial_history":[
            member(1,"geometry",placement("map:a",json!([1,1])),interval.clone(),None),
            member(2,"geometry",placement("map:b",json!([3,4])),interval.clone(),Some(1)),
            member(3,"placement",placement("map:a",json!([10,10])),valid_interval((20,1),(30,0)),None),
            member(4,"parentage",json!({"parent_id":null}),json!({"kind":"timeless"}),None)
        ]}),
        json!({"kind":"location","id":B,"spatial":placement("map:b",json!([40,40]))}),
        json!({"kind":"location","id":C,"spatial_history":[member(5,"placement",placement("map:b",json!([2,2])),interval.clone(),None)]}),
        json!({"kind":"location","id":"location:retired","status":"retired","spatial":placement("map:b",json!([3,4])),
            "spatial_history":[member(6,"geometry",placement("map:b",json!([3,4])),interval,None)]}),
        json!({"kind":"route","id":ROUTE,"from_location_id":A,"to_location_id":B,"direction":"one-way","modes":["foot"],
        "route_distance":{"value":12,"unit":"pace"},
        "spatial_history":[
            member(10,"route-metric",json!({"route_distance":{"value":25,"unit":"pace"}}),valid_interval((0,0),(10,0)),None),
            member(11,"route-metric",json!({"travel_cost":{"value":9,"unit":"coin"}}),valid_interval((0,0),(10,0)),None),
            member(12,"route-metric",json!({"travel_cost":{"value":7,"unit":"coin"}}),valid_interval((0,0),(10,0)),Some(11)),
            member(13,"route-availability",json!({"availability":"closed"}),valid_interval((11,0),(20,0)),None)
        ]}),
    ];
    source::world_from_records(
        PathBuf::new(),
        values.into_iter().map(record).collect(),
        Some("selected-spatial-history-revision".into()),
    )
    .unwrap()
}
fn edited(world: &World, id: &str, change: impl FnOnce(&mut Yaml)) -> World {
    let old = &world.records[id];
    let mut fields = old.frontmatter.clone();
    change(&mut fields);
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&fields).unwrap(),
        old.body
    );
    let replacement = source::parse_record(raw.as_bytes(), &old.source_path).unwrap();
    source::world_from_records(
        world.root.clone(),
        world
            .records
            .values()
            .map(|r| {
                if r.id == id {
                    replacement.clone()
                } else {
                    r.clone()
                }
            })
            .collect(),
        world.revision.clone(),
    )
    .unwrap()
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn valid(world: &World) {
    let report = validation::validate_world(world);
    assert!(report.valid, "{:?}", report.diagnostics);
}
fn ids(value: &Json) -> Vec<&str> {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["locationId"].as_str().unwrap())
        .collect()
}
fn bbox(
    world: &World,
    map: &str,
    min: &[f64],
    max: &[f64],
    time: &StoryTime,
    limit: usize,
) -> Json {
    geometry::bbox_at(world, map, min, max, "intersects", time, limit).unwrap()
}

#[test]
fn dated_geometry_discovers_moved_in_and_previously_unplaced_locations() {
    let world = world();
    valid(&world);
    assert!(
        ids(&bbox(
            &world,
            "map:b",
            &[3.0, 4.0],
            &[3.0, 4.0],
            &at(10, -1),
            1
        ))
        .is_empty()
    );
    for time in [at(10, 0), at(15, 0), at(20, 0)] {
        let value = bbox(&world, "map:b", &[1.0, 1.0], &[4.0, 5.0], &time, 2);
        assert_eq!(ids(&value), [A, C]);
        assert_eq!(value["scope"], "author-as-of-spatial-history");
        assert_eq!(value["basis"], "same-map-effective-geometry");
    }
    assert!(
        ids(&bbox(
            &world,
            "map:b",
            &[1.0, 1.0],
            &[4.0, 5.0],
            &at(20, 1),
            1
        ))
        .is_empty()
    );
    assert_eq!(
        ids(&bbox(
            &world,
            "map:a",
            &[10.0, 10.0],
            &[10.0, 10.0],
            &at(20, 1),
            1
        )),
        [A]
    );
    assert_eq!(
        ids(&bbox(
            &world,
            "map:a",
            &[-50.0, -50.0],
            &[-50.0, -50.0],
            &at(30, 1),
            1
        )),
        [A]
    );
    let other = StoryTime::new("other", 15, 0).unwrap();
    assert!(ids(&bbox(&world, "map:b", &[1.0, 1.0], &[4.0, 5.0], &other, 1)).is_empty());
}

#[test]
fn effective_shapes_reuse_three_dimensional_face_and_bounds_semantics() {
    let world = edited(&world(), C, |fields| {
        fields["spatial_history"]=serde_yaml::to_value(vec![member(20,"geometry",
        json!({"map_id":"map:levels","geometry":{"kind":"polygon","coordinates":[[0,0,5],[4,0,5],[4,4,5],[0,4,5],[0,0,5]]}}),
        valid_interval((0,0),(10,0)),None)]).unwrap()
    });
    valid(&world);
    let value =
        geometry::nearby_at(&world, "map:levels", &[2.0, 2.0, 7.0], 2.0, &at(5, 0), 1).unwrap();
    assert_eq!(ids(&value), [C]);
    assert_eq!(value["items"][0]["distance"], 2.0);
    assert!(
        ids(
            &geometry::nearby_at(&world, "map:levels", &[2.0, 2.0, 7.0], 1.9, &at(5, 0), 1)
                .unwrap()
        )
        .is_empty()
    );
    assert!(
        geometry::bbox_at(
            &world,
            "map:levels",
            &[1.0, 1.0, 5.0],
            &[3.0, 3.0, 5.0],
            "within",
            &at(5, 0),
            1
        )
        .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        ids(&geometry::bbox_at(
            &world,
            "map:levels",
            &[0.0, 0.0, 5.0],
            &[4.0, 4.0, 5.0],
            "within",
            &at(5, 0),
            1
        )
        .unwrap()),
        [C]
    );
    assert!(geometry::nearby_at(&world, "map:levels", &[2.0, 2.0], 2.0, &at(5, 0), 1).is_err());
}

#[test]
fn complete_limits_follow_effective_map_and_bounds_filtering() {
    let world = world();
    valid(&world);
    assert_eq!(
        ids(&bbox(
            &world,
            "map:b",
            &[3.0, 4.0],
            &[3.0, 4.0],
            &at(15, 0),
            1
        )),
        [A]
    );
    assert!(
        geometry::bbox_at(
            &world,
            "map:b",
            &[1.0, 1.0],
            &[4.0, 5.0],
            "intersects",
            &at(15, 0),
            1
        )
        .is_err()
    );
    assert_eq!(
        ids(&geometry::nearby_at(&world, "map:b", &[3.0, 4.0], 0.0, &at(15, 0), 1).unwrap()),
        [A]
    );
    assert!(geometry::nearby_at(&world, "map:b", &[3.0, 4.0], 3.0, &at(15, 0), 1).is_err());
    for limit in [0, 1001] {
        assert!(
            geometry::bbox_at(
                &world,
                "map:b",
                &[1.0, 1.0],
                &[4.0, 5.0],
                "intersects",
                &at(15, 0),
                limit
            )
            .is_err()
        );
    }
}

#[test]
fn correction_and_fallback_citations_name_the_actual_selected_source() {
    let world = world();
    valid(&world);
    let selected = bbox(&world, "map:b", &[3.0, 4.0], &[3.0, 4.0], &at(15, 0), 1);
    let row = &selected["items"][0];
    assert_eq!(row["citation"]["memberId"], history_id(2));
    let citations = row["citations"].as_array().unwrap();
    assert_eq!(
        citations
            .iter()
            .map(|c| c["memberId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [history_id(1), history_id(2)]
    );
    for citation in citations {
        assert_eq!(citation["sourcePath"], world.records[A].source_path);
        assert_eq!(citation["fingerprint"], world.fingerprint);
        assert_eq!(citation["revision"], world.revision.clone().unwrap());
    }
    let fallback = bbox(
        &world,
        "map:a",
        &[-50.0, -50.0],
        &[-50.0, -50.0],
        &at(31, 0),
        1,
    );
    assert_eq!(fallback["items"][0]["citation"]["section"], "spatial");
    assert_eq!(
        fallback["items"][0]["citations"].as_array().unwrap().len(),
        1
    );
    assert!(fallback["items"][0]["citation"].get("memberId").is_none());
}

#[test]
fn route_inspection_resolves_independent_fields_and_reverts_to_static_absence() {
    let world = world();
    valid(&world);
    let baseline = spatial::route_info(&world, ROUTE, None).unwrap();
    assert_eq!(baseline["availability"], "open");
    assert_eq!(
        baseline["routeDistance"],
        json!({"value":"12","unit":"pace"})
    );
    assert_eq!(baseline["travelCost"], Json::Null);
    assert_eq!(baseline["duration"], Json::Null);
    let active = spatial::route_info(&world, ROUTE, Some(&at(5, 0))).unwrap();
    assert_eq!(active["routeDistance"], json!({"value":"25","unit":"pace"}));
    assert_eq!(active["travelCost"], json!({"value":"7","unit":"coin"}));
    assert_eq!(
        active["fieldCitations"]["travelCost"][1]["memberId"],
        history_id(12)
    );
    assert_eq!(active["fieldCitations"]["availability"], json!([]));
    assert_eq!(active["fieldCitations"]["duration"], json!([]));
    let closed = spatial::route_info(&world, ROUTE, Some(&at(15, 0))).unwrap();
    assert_eq!(closed["availability"], "closed");
    assert_eq!(closed["routeDistance"], baseline["routeDistance"]);
    assert_eq!(closed["travelCost"], Json::Null);
    let fallback = spatial::route_info(&world, ROUTE, Some(&at(20, 1))).unwrap();
    for field in ["availability", "routeDistance", "travelCost", "duration"] {
        assert_eq!(fallback[field], baseline[field]);
    }
}

#[test]
fn exact_metric_values_and_extreme_horizons_remain_decimal_strings() {
    let tick = 9_007_199_254_740_993;
    let world = edited(&world(), ROUTE, |fields| {
        fields["spatial_history"] = serde_yaml::to_value(vec![member(
            30,
            "route-metric",
            json!({"duration":{"value":u64::MAX,"unit":"author beats"}}),
            valid_interval((tick, i32::MIN), (i64::MAX, i32::MAX)),
            None,
        )])
        .unwrap()
    });
    valid(&world);
    assert_eq!(
        spatial::route_info(&world, ROUTE, Some(&at(tick - 1, i32::MAX))).unwrap()["duration"],
        Json::Null
    );
    for time in [at(tick, i32::MIN), at(i64::MAX, i32::MAX)] {
        let value = spatial::route_info(&world, ROUTE, Some(&time)).unwrap();
        assert_eq!(value["at"]["tick"], time.tick.to_string());
        assert_eq!(value["at"]["order"], time.order.to_string());
        assert_eq!(value["duration"]["value"], u64::MAX.to_string());
        assert_eq!(value["duration"]["unit"], "author beats");
    }
}

#[test]
fn dated_queries_require_opt_in_valid_source_and_declared_timelines() {
    let world = world();
    valid(&world);
    let no_cap = edited(&world, WORLD, |fields| {
        fields["capabilities"]
            .as_sequence_mut()
            .unwrap()
            .retain(|v| v.as_str() != Some("spatial-history-v1"))
    });
    let invalid = edited(&world, A, |fields| {
        fields["spatial_history"][1]["value"]["map_id"] = Yaml::String("map:missing".into())
    });
    for source in [&no_cap, &invalid] {
        assert!(
            geometry::bbox_at(
                source,
                "map:b",
                &[3.0, 4.0],
                &[3.0, 4.0],
                "intersects",
                &at(15, 0),
                1
            )
            .is_err()
        );
        assert!(geometry::nearby_at(source, "map:b", &[3.0, 4.0], 0.0, &at(15, 0), 1).is_err());
        assert!(spatial::route_info(source, ROUTE, Some(&at(15, 0))).is_err());
    }
    let unknown = StoryTime::new("absent", 0, 0).unwrap();
    assert!(geometry::nearby_at(&world, "map:b", &[3.0, 4.0], 0.0, &unknown, 1).is_err());
    assert!(spatial::route_info(&world, ROUTE, Some(&unknown)).is_err());
    assert!(matches!(
        spatial::route_info(&world, A, None),
        Err(Error::NotFound(_))
    ));
    let retired = edited(&world, ROUTE, |fields| {
        fields["status"] = Yaml::String("retired".into())
    });
    valid(&retired);
    assert!(matches!(
        spatial::route_info(&retired, ROUTE, Some(&at(15, 0))),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn historical_reads_are_pure_and_never_change_static_paths_or_geometry() {
    let world = world();
    valid(&world);
    let sources: Vec<_> = world
        .records
        .values()
        .map(|r| (r.id.clone(), r.frontmatter.clone(), r.raw_bytes.clone()))
        .collect();
    let bbox_before =
        geometry::bbox(&world, "map:a", [-100.0, -100.0], [100.0, 100.0], 100).unwrap();
    let path_before = spatial::path(
        &world,
        A,
        B,
        Some("foot"),
        "route_distance",
        Some("pace"),
        100,
    )
    .unwrap();
    assert_eq!(path_before["total"]["value"], "12");
    assert_eq!(
        spatial::route_info(&world, ROUTE, Some(&at(15, 0))).unwrap()["availability"],
        "closed"
    );
    bbox(&world, "map:b", &[1.0, 1.0], &[4.0, 5.0], &at(15, 0), 2);
    assert_eq!(
        geometry::bbox(&world, "map:a", [-100.0, -100.0], [100.0, 100.0], 100).unwrap(),
        bbox_before
    );
    assert_eq!(
        spatial::path(
            &world,
            A,
            B,
            Some("foot"),
            "route_distance",
            Some("pace"),
            100
        )
        .unwrap(),
        path_before
    );
    assert!(bbox_before.get("at").is_none());
    assert!(bbox_before["items"][0].get("citations").is_none());
    assert_eq!(
        world
            .records
            .values()
            .map(|r| (r.id.clone(), r.frontmatter.clone(), r.raw_bytes.clone()))
            .collect::<Vec<_>>(),
        sources
    );
}

#[test]
fn future_history_and_canonical_corrections_do_not_grant_character_evidence() {
    let before = world();
    valid(&before);
    let after = edited(&before, A, |fields| {
        fields["spatial_history"][1]["value"]["geometry"]["coordinates"] =
            serde_yaml::to_value([30, 40]).unwrap();
        fields["title"] = Yaml::String("Unlearned relocation label".into());
    });
    valid(&after);
    assert_ne!(before.fingerprint, after.fingerprint);
    for time in [at(5, 0), at(15, 0), at(31, 0)] {
        assert_eq!(
            character::context(&before, HERO, &time, 100_000).unwrap(),
            character::context(&after, HERO, &time, 100_000).unwrap()
        );
        for query in ["lane", "relocation", "spatial", "moving"] {
            assert_eq!(
                character::search(&before, HERO, query, &time, 20).unwrap(),
                character::search(&after, HERO, query, &time, 20).unwrap()
            );
        }
        assert_eq!(
            character::context(&before, HERO, &time, 1)
                .unwrap_err()
                .json(),
            character::context(&after, HERO, &time, 1)
                .unwrap_err()
                .json()
        );
    }
}
