//! Portal edges and overlay presentation scopes preserve explicit source meaning.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    Error, character,
    model::{Record, StoryTime, World},
    source, spatial, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const HERO: &str = "char_00000000000000000000000001";
const KNOW: &str = "know_00000000000000000000000001";

fn record(kind: &str, id: &str, fields: &str) -> Record {
    let raw = format!(
        "---\nschema: wedl/v1\nkind: {kind}\nid: {id}\ntitle: {id}\n{fields}---\nAuthor spatial prose\n"
    );
    source::parse_record(
        raw.as_bytes(),
        &format!("story/{}.md", id.replace([':', '/'], "-")),
    )
    .unwrap()
}
fn world(mut records: Vec<Record>) -> World {
    records.push(record("world",WORLD,"capabilities: [spatial-core-v1, geometry-v1, route-v1, overlay-v1]\ndefault_timeline: main\ntimelines: [{id: main, label: Main}, {id: other, label: Other}]\nstate_keys: {}\n"));
    source::world_from_records(PathBuf::new(), records, None).unwrap()
}
fn map() -> Record {
    record(
        "map",
        "map:main",
        "crs: local-planar:main\naxis_order: [east, north]\nunit: m\nbounds: {min: [-10,-10], max: [10,10]}\n",
    )
}
fn portal(id: &str, from: &str, to: &str, modes: &str) -> Record {
    record(
        "portal",
        id,
        &format!("from_location_id: {from}\nto: {to}\nmodes: {modes}\n"),
    )
}
fn route(id: &str, from: &str, to: &str, metric: &str) -> Record {
    record(
        "route",
        id,
        &format!(
            "from_location_id: {from}\nto_location_id: {to}\ndirection: one-way\nmodes: [foot]\n{metric}"
        ),
    )
}
fn overlay(
    id: &str,
    lifecycle: &str,
    members: &str,
    audience: &str,
    views: &str,
    valid: &str,
) -> Record {
    record(
        "overlay",
        id,
        &format!(
            "lifecycle: {lifecycle}\nmembership: {{location_ids: {members}}}\naudience: {audience}\nperspectives: {views}\n{valid}"
        ),
    )
}
fn base() -> World {
    world(vec![
        map(),
        record("location", "location:a", ""),
        record("location", "location:b", "parent_id: location:a\n"),
        record("location", "location:c", ""),
        record(
            "location",
            "location:island",
            "spatial: {map_id: map:main, geometry: {kind: point, coordinates: [1,2]}}\n",
        ),
        route(
            "route:ab",
            "location:a",
            "location:b",
            "route_distance: {value: 10, unit: m}\n",
        ),
        portal("portal:bc", "location:b", "location:c", "[foot]"),
        portal(
            "portal:position",
            "location:b",
            "{map_id: map:main, coordinates: [1,2]}",
            "[fly, foot]",
        ),
        overlay(
            "overlay:static",
            "static",
            "[location:b]",
            "[author]",
            "[author]",
            "",
        ),
        overlay(
            "overlay:timed",
            "time-bounded",
            "[location:b]",
            "[author, public]",
            "[author, ordinary]",
            "valid: {start: {timeline: main, tick: -1, order: -1}, end: {timeline: main, tick: 1, order: 1}}\n",
        ),
    ])
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn edited(world: &World, id: &str, edit: impl FnOnce(&mut Yaml)) -> World {
    let mut records = world.records.clone();
    let record = records.get_mut(id).unwrap();
    edit(&mut record.frontmatter);
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&record.frontmatter).unwrap(),
        record.body
    );
    *record = source::parse_record(raw.as_bytes(), &record.source_path).unwrap();
    source::world_from_records(
        world.root.clone(),
        records.into_values().collect(),
        world.revision.clone(),
    )
    .unwrap()
}
fn add(world: &World, record: Record) -> World {
    let mut records = world.records.clone();
    records.insert(record.id.clone(), record);
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
    assert!(report.valid, "{:?}", report.diagnostics);
    assert!(
        spatial::validate(world).is_empty(),
        "{:?}",
        spatial::validate(world)
    );
}
fn invalid(world: &World) {
    assert!(!spatial::validate(world).is_empty());
    assert!(!validation::validate_world(world).valid);
}
fn overlays(
    world: &World,
    at: &StoryTime,
    audience: &str,
    view: &str,
    selected: Option<&str>,
    limit: usize,
) -> Json {
    spatial::overlays(world, "location:b", at, audience, view, selected, limit).unwrap()
}

#[test]
fn explicit_portals_are_directed_mode_filtered_and_separate_from_routes() {
    let world = base();
    valid(&world);
    let location = spatial::location(&world, "location:b", Some("foot"), 100).unwrap();
    assert!(location["outgoingRoutes"].as_array().unwrap().is_empty());
    assert_eq!(location["outgoingPortals"].as_array().unwrap().len(), 2);
    assert_eq!(location["outgoingPortals"][0]["portalId"], "portal:bc");
    assert_eq!(location["outgoingPortals"][1]["targetKind"], "position");
    assert_eq!(
        location["outgoingPortals"][1]["position"]["coordinates"],
        json!(["1", "2"])
    );
    assert_eq!(
        location["outgoingPortals"][0]["citation"]["entityId"],
        "portal:bc"
    );
    let path = spatial::path(
        &world,
        "location:a",
        "location:c",
        Some("foot"),
        "hops",
        None,
        100,
    )
    .unwrap();
    assert_eq!(
        path["locationIds"],
        json!(["location:a", "location:b", "location:c"])
    );
    assert_eq!(path["routeIds"], json!(["route:ab"]));
    assert_eq!(path["portalIds"], json!(["portal:bc"]));
    assert_eq!(path["edgeIds"], json!(["route:ab", "portal:bc"]));
    assert_eq!(path["edges"][1]["toLocationId"], "location:c");
    assert!(path["edges"][1].get("routeDistance").is_none());
    assert_eq!(
        spatial::path(&world, "location:c", "location:b", None, "hops", None, 100).unwrap()["found"],
        false
    );
    assert_eq!(
        spatial::path(
            &world,
            "location:a",
            "location:c",
            Some("fly"),
            "hops",
            None,
            100
        )
        .unwrap()["found"],
        false
    );
}

#[test]
fn position_portals_never_snap_to_colocated_locations_or_supply_parent_travel() {
    let world = base();
    valid(&world);
    assert_eq!(
        spatial::path(
            &world,
            "location:b",
            "location:island",
            None,
            "hops",
            None,
            100
        )
        .unwrap()["found"],
        false
    );
    assert_eq!(
        spatial::path(&world, "location:b", "location:a", None, "hops", None, 100).unwrap()["found"],
        false
    );
    assert_eq!(
        spatial::reachable(&world, "location:b", Some("fly"), 100).unwrap()["locationIds"],
        json!(["location:b"])
    );
    let retired = edited(&world, "portal:bc", |v| v["status"] = yaml("retired"));
    valid(&retired);
    assert_eq!(
        spatial::path(
            &retired,
            "location:b",
            "location:c",
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
fn portals_supply_no_zero_cost_or_metric_conversion_in_weighted_paths() {
    let world = base();
    valid(&world);
    for metric in ["route_distance", "travel_cost", "duration"] {
        let path = spatial::path(
            &world,
            "location:a",
            "location:c",
            None,
            metric,
            Some("m"),
            100,
        )
        .unwrap();
        assert_eq!(path["found"], false);
        assert_eq!(path["total"], Json::Null);
    }
    let world = add(
        &world,
        route(
            "route:bc",
            "location:b",
            "location:c",
            "route_distance: {value: 20, unit: m}\n",
        ),
    );
    valid(&world);
    let path = spatial::path(
        &world,
        "location:a",
        "location:c",
        None,
        "route_distance",
        Some("m"),
        100,
    )
    .unwrap();
    assert_eq!(path["total"], json!({"value":"30","unit":"m"}));
    assert_eq!(path["routeIds"], json!(["route:ab", "route:bc"]));
    assert_eq!(path["portalIds"], json!([]));
    let hops = spatial::path(&world, "location:a", "location:c", None, "hops", None, 100).unwrap();
    assert_eq!(hops["portalIds"], json!(["portal:bc"]));
}

#[test]
fn reachability_is_complete_cycle_safe_and_bounded_after_mode_filtering() {
    let world = add(
        &base(),
        portal("portal:ca", "location:c", "location:a", "[foot]"),
    );
    valid(&world);
    assert_eq!(
        spatial::reachable(&world, "location:a", None, 3).unwrap()["locationIds"],
        json!(["location:a", "location:b", "location:c"])
    );
    assert!(spatial::reachable(&world, "location:a", None, 2).is_err());
    assert_eq!(
        spatial::reachable(&world, "location:a", Some("boat"), 1).unwrap()["count"],
        1
    );
    for limit in [0, 1001] {
        assert!(spatial::reachable(&world, "location:a", None, limit).is_err());
    }
    assert!(spatial::reachable(&world, "location:a", Some(" "), 100).is_err());
    assert!(spatial::path(&world, "location:a", "location:c", None, "hops", None, 2).is_err());
}

#[test]
fn portal_source_checks_exact_targets_modes_canonical_endpoints_and_capabilities() {
    let world = base();
    for target in [
        "location:missing",
        "location:b",
        "map:main",
        "true",
        "null",
        "!tag location:c",
        "{map_id: map:main, coordinates: [11,0]}",
        "{map_id: map:main, coordinates: [1,2,3]}",
        "{map_id: map:main, coordinates: [1,true]}",
        "{map_id: map:main, coordinates: [1,2], extra: no}",
    ] {
        invalid(&edited(&world, "portal:bc", |v| v["to"] = yaml(target)));
    }
    for modes in ["[]", "[foot,foot]", "[foot,fly]", "[true]", "[' ']"] {
        invalid(&edited(&world, "portal:bc", |v| v["modes"] = yaml(modes)));
    }
    for field in [
        "route_distance",
        "availability",
        "direction",
        "to_location_id",
    ] {
        invalid(&edited(&world, "portal:bc", |v| v[field] = yaml("ignored")));
    }
    invalid(&edited(&world, "location:c", |v| {
        v["status"] = yaml("retired")
    }));
    invalid(&edited(&world, "map:main", |v| {
        v["status"] = yaml("retired")
    }));
    invalid(&edited(&world, WORLD, |v| {
        v["capabilities"] = yaml("[spatial-core-v1,geometry-v1,overlay-v1]")
    }));
    let retired = edited(
        &edited(&world, "portal:bc", |v| v["status"] = yaml("retired")),
        "location:c",
        |v| v["status"] = yaml("retired"),
    );
    valid(&retired);
}

#[test]
fn static_and_bounded_overlays_use_exact_inclusive_timeline_points() {
    let world = base();
    valid(&world);
    for (point, count) in [
        (at(-1, -2), 1),
        (at(-1, -1), 2),
        (at(1, 1), 2),
        (at(1, 2), 1),
    ] {
        assert_eq!(
            overlays(&world, &point, "author", "author", None, 100)["count"],
            count
        );
    }
    assert_eq!(
        overlays(
            &world,
            &StoryTime::new("other", 0, 0).unwrap(),
            "author",
            "author",
            None,
            100
        )["count"],
        1
    );
    let result = overlays(
        &world,
        &at(0, 0),
        "author",
        "author",
        Some("overlay:timed"),
        100,
    );
    assert_eq!(result["items"][0]["valid"]["start"]["tick"], "-1");
    assert_eq!(result["items"][0]["valid"]["end"]["order"], "1");
    assert_eq!(result["perspective"], "author");
    assert_eq!(result["scope"], "author-as-of-overlay-membership");
    assert_eq!(result["effectiveTime"]["tick"], "0");
    assert_eq!(
        spatial::overlays(
            &world,
            "location:a",
            &at(0, 0),
            "author",
            "author",
            Some("overlay:timed"),
            100
        )
        .unwrap()["count"],
        0
    );
}

#[test]
fn overlay_scope_filtering_precedes_membership_horizon_and_bounds() {
    let world = base();
    assert_eq!(
        overlays(&world, &at(0, 0), "public", "ordinary", None, 1)["count"],
        1
    );
    assert_eq!(
        overlays(&world, &at(0, 0), "public", "unknown", None, 1)["count"],
        0
    );
    let hidden = spatial::overlays(
        &world,
        "location:b",
        &at(100, 0),
        "public",
        "ordinary",
        Some("overlay:static"),
        1,
    )
    .unwrap_err();
    let missing = spatial::overlays(
        &world,
        "location:b",
        &at(0, 0),
        "public",
        "ordinary",
        Some("overlay:missing"),
        1,
    )
    .unwrap_err();
    assert_eq!(hidden.json(), missing.json());
    assert!(matches!(hidden, Error::NotFound(_)));
    assert_eq!(
        overlays(
            &world,
            &at(100, 0),
            "public",
            "ordinary",
            Some("overlay:timed"),
            1
        )["count"],
        0
    );
    assert!(
        spatial::overlays(&world, "location:b", &at(0, 0), "author", "author", None, 1).is_err()
    );
    let retired = edited(&world, "overlay:static", |v| v["status"] = yaml("retired"));
    valid(&retired);
    assert_eq!(
        spatial::overlays(
            &retired,
            "location:b",
            &at(0, 0),
            "author",
            "author",
            Some("overlay:static"),
            1
        )
        .unwrap_err()
        .json(),
        missing.json()
    );
}

#[test]
fn hidden_or_inapplicable_overlays_cannot_consume_visible_result_limit() {
    let mut world = base();
    for i in 0..120 {
        world = add(
            &world,
            overlay(
                &format!("overlay:hidden-{i}"),
                "static",
                "[location:b]",
                "[secret]",
                "[secret]",
                "",
            ),
        );
        world = add(
            &world,
            overlay(
                &format!("overlay:elsewhere-{i}"),
                "static",
                "[location:a]",
                "[public]",
                "[ordinary]",
                "",
            ),
        );
    }
    valid(&world);
    let result = overlays(&world, &at(0, 0), "public", "ordinary", None, 1);
    assert_eq!(result["count"], 1);
    assert_eq!(result["items"][0]["overlayId"], "overlay:timed");
    assert!(!result.to_string().contains("overlay:hidden"));
}

#[test]
fn overlay_source_rejects_open_shapes_bad_bounds_legacy_fields_and_unsorted_scopes() {
    let world = base();
    for (key, value) in [
        ("lifecycle", "unknown"),
        ("membership", "{location_ids: [location:b], extra: []}"),
        ("membership", "{location_ids: [location:missing]}"),
        ("membership", "{location_ids: [location:b,location:b]}"),
        ("membership", "{location_ids: [location:c,location:b]}"),
        ("audience", "[public,author]"),
        ("audience", "[author,author]"),
        ("perspectives", "[true]"),
        ("valid", "null"),
        ("faction", "character:unknown"),
        ("institution", "x"),
        ("calendar", "x"),
    ] {
        invalid(&edited(&world, "overlay:static", |v| v[key] = yaml(value)));
    }
    for valid_value in [
        "null",
        "{start: {tick: 0}, end: {tick: 1}}",
        "{start: {timeline: main, tick: 0, order: 0}, end: {timeline: other, tick: 1, order: 0}}",
        "{start: {timeline: main, tick: 0, order: 1}, end: {timeline: main, tick: 0, order: 0}}",
        "{start: {timeline: main, tick: 0, order: 0}, end: {timeline: main, tick: 1, order: 0}, extra: []}",
    ] {
        invalid(&edited(&world, "overlay:timed", |v| {
            v["valid"] = yaml(valid_value)
        }));
    }
    invalid(&edited(&world, WORLD, |v| {
        v["capabilities"] = yaml("[spatial-core-v1,geometry-v1,route-v1]")
    }));
    let empty = edited(&world, "overlay:static", |v| {
        v["membership"] = yaml("{location_ids: []}");
        v["audience"] = yaml("[]");
        v["perspectives"] = yaml("[]");
    });
    valid(&empty);
    assert_eq!(
        overlays(&empty, &at(0, 0), "author", "author", None, 100)["count"],
        1
    );
}

#[test]
fn overlay_extreme_coordinates_remain_exact_strings_and_queries_are_closed() {
    let world = edited(&base(), "overlay:timed", |v| {
        v["valid"] = yaml(
            "{start: {timeline: main, tick: 9007199254740993, order: -2147483648}, end: {timeline: main, tick: 9223372036854775807, order: 2147483647}}",
        )
    });
    valid(&world);
    assert_eq!(
        overlays(
            &world,
            &at(9007199254740992, i32::MAX),
            "public",
            "ordinary",
            None,
            1
        )["count"],
        0
    );
    let value = overlays(
        &world,
        &at(9007199254740993, i32::MIN),
        "public",
        "ordinary",
        None,
        1,
    );
    assert_eq!(
        value["items"][0]["valid"]["end"]["tick"],
        i64::MAX.to_string()
    );
    assert_eq!(
        value["items"][0]["valid"]["start"]["order"],
        i32::MIN.to_string()
    );
    for (audience, view) in [(" ", "author"), ("author", "")] {
        assert!(
            spatial::overlays(&world, "location:b", &at(0, 0), audience, view, None, 100).is_err()
        );
    }
    for selector in ["overlay title", "route:ab"] {
        assert!(
            spatial::overlays(
                &world,
                "location:b",
                &at(0, 0),
                "author",
                "author",
                Some(selector),
                100
            )
            .is_err()
        );
    }
    assert!(
        spatial::overlays(
            &world,
            "location:b",
            &StoryTime::new("undeclared", 0, 0).unwrap(),
            "author",
            "author",
            None,
            100
        )
        .is_err()
    );
}

#[test]
fn portal_overlay_and_scope_metadata_never_grant_character_evidence() {
    let before = world(vec![
        record("character", HERO, "initial_state: {}\n"),
        record(
            "knowledge",
            KNOW,
            &format!(
                "knower: {HERO}\nclaim: {{key: lamp, statement: I saw a lamp}}\ntransitions: [{{id: learned, time: {{tick: 0}}, state: accepted, acquisition: observed, confidence: 1}}]\n"
            ),
        ),
    ]);
    let mut after = before.clone();
    for record in base()
        .records
        .into_values()
        .filter(|record| record.kind != "world")
    {
        after = add(&after, record);
    }
    valid(&before);
    valid(&after);
    assert_ne!(before.fingerprint, after.fingerprint);
    for point in [at(0, 0), at(2, 0)] {
        assert_eq!(
            character::context(&before, HERO, &point, 100_000).unwrap(),
            character::context(&after, HERO, &point, 100_000).unwrap()
        );
        for query in ["lamp", "portal", "overlay", "spatial"] {
            assert_eq!(
                character::search(&before, HERO, query, &point, 20).unwrap(),
                character::search(&after, HERO, query, &point, 20).unwrap()
            );
        }
        assert_eq!(
            character::context(&before, HERO, &point, 1)
                .unwrap_err()
                .json(),
            character::context(&after, HERO, &point, 1)
                .unwrap_err()
                .json()
        );
    }
}
