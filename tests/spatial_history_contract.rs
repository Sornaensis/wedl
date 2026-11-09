//! Exact author history, correction provenance and effective containment safety.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    character,
    model::{Record, StoryTime, World},
    source, spatial_history, validation,
};

const WORLD: &str = "world_00000000000000000000000000";
const HERO: &str = "char_00000000000000000000000001";
const KNOW: &str = "know_00000000000000000000000001";
const A: &str = "location:a";
const B: &str = "location:b";
const C: &str = "location:c";
const MAP: &str = "map:main";
const ROUTE: &str = "route:ab";
fn yaml(value: &str) -> Yaml {
    serde_yaml::from_str(value).unwrap()
}
fn record(kind: &str, id: &str, fields: &str) -> Record {
    let raw = format!(
        "---\nschema: wedl/v1\nkind: {kind}\nid: {id}\ntitle: {id}\n{fields}---\nPreserved author prose\n"
    );
    source::parse_record(
        raw.as_bytes(),
        &format!("story/{}.md", id.replace([':', '/'], "-")),
    )
    .unwrap()
}
fn base() -> World {
    source::world_from_records(PathBuf::new(),vec![
        record("world",WORLD,"capabilities: [spatial-core-v1, geometry-v1, route-v1, spatial-history-v1]\ndefault_timeline: main\ntimelines: [{id: main, label: Main}, {id: other, label: Other}]\nstate_keys: {}\n"),
        record("map",MAP,"crs: local-planar:town\naxis_order: [east, north]\nunit: m\nbounds: {min: [-10,-10], max: [10,10]}\n"),
        record("location",A,""),
        record("location",B,"parent_id: location:a\nspatial: {map_id: map:main, geometry: {kind: point, coordinates: [0,0]}}\n"),
        record("location",C,""),
        record("route",ROUTE,"from_location_id: location:a\nto_location_id: location:b\ndirection: two-way\nmodes: [foot]\navailability: open\nroute_distance: {value: 12, unit: m}\nduration: {value: 3, unit: seconds}\n"),
    ],Some("a".repeat(40))).unwrap()
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
    let mut changed = source::parse_record(raw.as_bytes(), &record.source_path).unwrap();
    changed.blob_oid = Some("b".repeat(40));
    *record = changed;
    source::world_from_records(
        world.root.clone(),
        records.into_values().collect(),
        world.revision.clone(),
    )
    .unwrap()
}
fn added(world: &World, record: Record) -> World {
    let mut records = world.records.clone();
    records.insert(record.id.clone(), record);
    source::world_from_records(
        world.root.clone(),
        records.into_values().collect(),
        world.revision.clone(),
    )
    .unwrap()
}
fn local(number: usize) -> String {
    format!("spatialhistory_{number:026}")
}
fn interval(timeline: &str, start: Option<(i64, i32)>, end: Option<(i64, i32)>) -> Yaml {
    let mut value = yaml(&format!("kind: interval\ntimeline: {timeline}\n"));
    for (key, coordinate) in [("start", start), ("end", end)] {
        if let Some((tick, order)) = coordinate {
            value[key] =
                serde_yaml::to_value(json!({"timeline":timeline,"tick":tick,"order":order}))
                    .unwrap();
        }
    }
    value
}
fn member(number: usize, aspect: &str, value: Yaml, valid: Yaml, replaces: Option<usize>) -> Yaml {
    let mut result = serde_yaml::to_value(json!({"id":local(number),"aspect":aspect})).unwrap();
    result["value"] = value;
    result["valid"] = valid;
    if let Some(number) = replaces {
        result["replaces"] = Yaml::String(local(number));
    }
    result
}
fn root(number: usize, valid: Yaml) -> Yaml {
    member(number, "parentage", yaml("{parent_id: null}"), valid, None)
}
fn shape(number: usize, aspect: &str, valid: Yaml) -> Yaml {
    member(
        number,
        aspect,
        yaml("{map_id: 'map:main', geometry: {kind: point, coordinates: [3,4]}}"),
        valid,
        None,
    )
}
fn set_history(world: &World, id: &str, values: Vec<Yaml>) -> World {
    edited(world, id, |data| {
        data["spatial_history"] = Yaml::Sequence(values)
    })
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn timeless() -> Yaml {
    yaml("{kind: timeless}")
}
fn valid(world: &World) {
    assert!(
        spatial_history::validate(world).is_empty(),
        "{:?}",
        spatial_history::validate(world)
    );
    let report = validation::validate_world(world);
    assert!(report.valid, "{:?}", report.diagnostics);
}
fn invalid(world: &World) {
    assert!(!spatial_history::validate(world).is_empty());
    assert!(!validation::validate_world(world).valid);
}
fn history(world: &World, id: &str, at: Option<&StoryTime>, limit: usize) -> Json {
    spatial_history::history(world, id, at, limit).unwrap()
}
fn ids(value: &Json) -> Vec<&str> {
    value["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value["memberId"].as_str().unwrap())
        .collect()
}

#[test]
fn corrected_members_retain_exact_source_and_ordered_citation_chains() {
    let valid_time = interval("main", Some((0, 0)), Some((10, 0)));
    let first = member(
        1,
        "route-metric",
        yaml("{travel_cost: {value: 9, unit: coin}}"),
        valid_time.clone(),
        None,
    );
    let second = member(
        2,
        "route-metric",
        yaml("{travel_cost: {value: 7, unit: coin}}"),
        valid_time.clone(),
        Some(1),
    );
    let third = member(
        3,
        "route-metric",
        yaml("{travel_cost: {value: 5, unit: coin}}"),
        valid_time,
        Some(2),
    );
    let world = set_history(&base(), ROUTE, vec![first, second, third]);
    valid(&world);
    let before = world.records[ROUTE].raw_bytes.clone();
    let all = history(&world, ROUTE, None, 3);
    assert_eq!(ids(&all), [local(1), local(2), local(3)]);
    assert_eq!(all["scope"], "author-all-time-spatial-history");
    assert_eq!(all["at"], Json::Null);
    let current = history(&world, ROUTE, Some(&at(5, 0)), 1);
    assert_eq!(ids(&current), [local(3)]);
    assert_eq!(current["items"][0]["value"]["travel_cost"]["value"], "5");
    let citations = current["items"][0]["citations"].as_array().unwrap();
    assert_eq!(
        citations
            .iter()
            .map(|c| c["memberId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [local(1), local(2), local(3)]
    );
    for citation in citations {
        assert_eq!(citation["entityId"], ROUTE);
        assert_eq!(citation["sourcePath"], world.records[ROUTE].source_path);
        assert_eq!(citation["fingerprint"], world.fingerprint);
        assert_eq!(citation["revision"], world.revision.as_deref().unwrap());
        assert_eq!(citation["blobOid"], "b".repeat(40));
    }
    assert_eq!(world.records[ROUTE].raw_bytes, before);
    assert_eq!(
        world.records[ROUTE].list("spatial_history").unwrap().len(),
        3
    );
    assert_eq!(history(&world, ROUTE, Some(&at(10, 1)), 1)["count"], 0);
}

#[test]
fn inclusive_order_boundaries_and_open_intervals_preserve_exact_time() {
    let world = set_history(
        &base(),
        B,
        vec![
            root(1, interval("main", None, Some((0, 0)))),
            root(
                2,
                interval(
                    "main",
                    Some((0, 1)),
                    Some((9_007_199_254_740_993, i32::MIN)),
                ),
            ),
            root(
                3,
                interval("main", Some((9_007_199_254_740_993, i32::MIN + 1)), None),
            ),
            root(4, interval("other", None, None)),
        ],
    );
    valid(&world);
    for (time, id) in [
        (at(i64::MIN, i32::MIN), 1),
        (at(0, 0), 1),
        (at(0, 1), 2),
        (at(9_007_199_254_740_993, i32::MIN), 2),
        (at(9_007_199_254_740_993, i32::MIN + 1), 3),
        (at(i64::MAX, i32::MAX), 3),
    ] {
        let value = history(&world, B, Some(&time), 1);
        assert_eq!(ids(&value), [local(id)]);
        assert_eq!(value["at"]["tick"], time.tick.to_string());
        assert_eq!(value["at"]["order"], time.order.to_string());
    }
    assert_eq!(
        ids(&history(
            &world,
            B,
            Some(&StoryTime::new("other", 0, 0).unwrap()),
            1
        )),
        [local(4)]
    );
    assert_eq!(
        history(&world, B, None, 4)["items"][1]["valid"]["end"]["tick"],
        "9007199254740993"
    );
    assert!(
        spatial_history::history(
            &world,
            B,
            Some(&StoryTime::new("undeclared", 0, 0).unwrap()),
            10
        )
        .is_err()
    );
}

#[test]
fn timeless_history_and_empty_aggregates_have_explicit_author_scope() {
    let world = set_history(&base(), B, vec![root(1, timeless())]);
    valid(&world);
    for timeline in ["main", "other"] {
        assert_eq!(
            ids(&history(
                &world,
                B,
                Some(&StoryTime::new(timeline, -100, 4).unwrap()),
                1
            )),
            [local(1)]
        );
    }
    assert_eq!(history(&world, C, None, 1)["count"], 0);
    let empty = set_history(&world, B, vec![]);
    valid(&empty);
    assert_eq!(history(&empty, B, Some(&at(0, 0)), 1)["items"], json!([]));
    let retired = edited(&world, C, |v| v["status"] = yaml("retired"));
    valid(&retired);
    assert!(spatial_history::history(&retired, C, None, 10).is_err());
    assert!(spatial_history::history(&world, MAP, None, 10).is_err());
}

#[test]
fn corrections_must_be_earlier_local_exact_and_unforked() {
    let applicability = interval("main", Some((0, 0)), Some((10, 0)));
    let first = root(1, applicability.clone());
    let good = member(
        2,
        "parentage",
        yaml("{parent_id: location:c}"),
        applicability.clone(),
        Some(1),
    );
    valid(&set_history(&base(), B, vec![first.clone(), good.clone()]));
    let mut bads = Vec::new();
    let mut missing = good.clone();
    missing["replaces"] = Yaml::String(local(99));
    bads.push(vec![first.clone(), missing]);
    let mut self_ref = good.clone();
    self_ref["replaces"] = Yaml::String(local(2));
    bads.push(vec![first.clone(), self_ref]);
    bads.push(vec![good.clone(), first.clone()]);
    let mut changed_valid = good.clone();
    changed_valid["valid"] = interval("main", Some((0, 0)), Some((10, 1)));
    bads.push(vec![first.clone(), changed_valid]);
    let fork = member(
        3,
        "parentage",
        yaml("{parent_id: location:a}"),
        applicability.clone(),
        Some(1),
    );
    bads.push(vec![first.clone(), good.clone(), fork]);
    let mut wrong_aspect = shape(2, "geometry", applicability.clone());
    wrong_aspect["replaces"] = Yaml::String(local(1));
    bads.push(vec![first, wrong_aspect]);
    for values in bads {
        assert!(spatial_history::validate_request(&Yaml::Sequence(values.clone())).is_ok());
        invalid(&set_history(&base(), B, values));
    }
    let cost = member(
        1,
        "route-metric",
        yaml("{travel_cost: {value: 9, unit: coin}}"),
        timeless(),
        None,
    );
    let distance = member(
        2,
        "route-metric",
        yaml("{route_distance: {value: 7, unit: m}}"),
        timeless(),
        Some(1),
    );
    invalid(&set_history(&base(), ROUTE, vec![cost, distance]));
}

#[test]
fn correction_validity_matches_literal_open_bounds_not_only_effective_extent() {
    let first = root(1, interval("main", None, None));
    let correction = member(
        2,
        "parentage",
        yaml("{parent_id: null}"),
        interval(
            "main",
            Some((i64::MIN, i32::MIN)),
            Some((i64::MAX, i32::MAX)),
        ),
        Some(1),
    );
    invalid(&set_history(&base(), B, vec![first, correction]));
    let first = root(1, timeless());
    let correction = member(
        2,
        "parentage",
        yaml("{parent_id: null}"),
        interval("main", None, None),
        Some(1),
    );
    invalid(&set_history(&base(), B, vec![first, correction]));
}

#[test]
fn placement_and_geometry_share_one_field_and_adjacency_is_order_exact() {
    let first = shape(1, "geometry", interval("main", Some((0, 0)), Some((10, 0))));
    let overlapping = shape(
        2,
        "placement",
        interval("main", Some((10, 0)), Some((20, 0))),
    );
    invalid(&set_history(&base(), B, vec![first.clone(), overlapping]));
    let adjacent = shape(
        2,
        "placement",
        interval("main", Some((10, 1)), Some((20, 0))),
    );
    let world = set_history(&base(), B, vec![first.clone(), adjacent]);
    valid(&world);
    assert_eq!(ids(&history(&world, B, Some(&at(10, 0)), 1)), [local(1)]);
    assert_eq!(ids(&history(&world, B, Some(&at(10, 1)), 1)), [local(2)]);
    let mut wrong = shape(
        2,
        "placement",
        interval("main", Some((0, 0)), Some((10, 0))),
    );
    wrong["replaces"] = Yaml::String(local(1));
    invalid(&set_history(&base(), B, vec![first, wrong]));
}

#[test]
fn closed_history_and_validity_reject_malformed_or_tagged_source() {
    let good = root(1, timeless());
    for invalid_array in [
        yaml("null"),
        yaml("{}"),
        yaml("[null]"),
        yaml("!history []"),
    ] {
        assert!(spatial_history::validate_request(&invalid_array).is_err());
        invalid(&edited(&base(), B, |data| {
            data["spatial_history"] = invalid_array
        }));
    }
    for (key, value) in [
        ("id", yaml("spatialhistory_IIIIIIIIIIIIIIIIIIIIIIIIII")),
        ("id", yaml("wrong")),
        ("aspect", yaml("[]")),
        ("aspect", yaml("move")),
        ("value", yaml("null")),
        ("value", yaml("{parent_id: null, extra: 1}")),
        ("audience", yaml("[author]")),
        ("replaces", yaml("null")),
        ("value", yaml("{parent_id: !literal location:a}")),
    ] {
        let mut value_member = good.clone();
        value_member[key] = value;
        assert!(
            spatial_history::validate_request(&Yaml::Sequence(vec![value_member.clone()])).is_err()
        );
        invalid(&set_history(&base(), B, vec![value_member]));
    }
    for value in [
        "{kind: timeless, timeline: main}",
        "{kind: other}",
        "{kind: interval}",
        "{kind: interval, timeline: main, start: null}",
        "{kind: interval, timeline: main, end: {tick: 1, order: 0}}",
        "{kind: interval, timeline: main, start: {timeline: main, tick: 1, order: 2147483648}}",
        "{kind: interval, timeline: main, start: {timeline: main, tick: 1.0, order: 0}}",
        "{kind: interval, timeline: main, start: {timeline: main, tick: '1', order: 0}}",
        "{kind: interval, timeline: main, start: {timeline: other, tick: 1, order: 0}}",
        "{kind: interval, timeline: main, start: {timeline: main, tick: 1, order: 1}, end: {timeline: main, tick: 1, order: 0}}",
    ] {
        let mut value_member = good.clone();
        value_member["valid"] = yaml(value);
        assert!(
            spatial_history::validate_request(&Yaml::Sequence(vec![value_member.clone()])).is_err(),
            "{value}"
        );
        invalid(&set_history(&base(), B, vec![value_member]));
    }
    invalid(&set_history(
        &base(),
        B,
        vec![root(1, interval("undeclared", None, None))],
    ));
    invalid(&set_history(&base(), B, vec![good.clone(), good]));
}

#[test]
fn geometry_history_reuses_map_dimensions_bounds_and_canonical_endpoints() {
    let good = shape(1, "placement", timeless());
    valid(&set_history(&base(), C, vec![good.clone()]));
    for value in [
        "{map_id: map:missing, geometry: {kind: point, coordinates: [0,0]}}",
        "{map_id: map:main, geometry: {kind: point, coordinates: [0,0,0]}}",
        "{map_id: map:main, geometry: {kind: point, coordinates: [11,0]}}",
        "{map_id: location:a, geometry: {kind: point, coordinates: [0,0]}}",
        "{map_id: map:main, geometry: {kind: line, coordinates: [[0,0]]}}",
        "{map_id: map:main, geometry: {kind: polygon, coordinates: [[0,0],[1,0],[1,1],[0,1]]}}",
    ] {
        let mut bad = good.clone();
        bad["value"] = yaml(value);
        invalid(&set_history(&base(), C, vec![bad]));
    }
    let unplaced = edited(&base(), B, |value| {
        value.as_mapping_mut().unwrap().remove("spatial");
    });
    let retired_map = edited(&unplaced, MAP, |value| value["status"] = yaml("retired"));
    invalid(&set_history(&retired_map, C, vec![good.clone()]));
    let retired_owner = edited(&retired_map, C, |value| value["status"] = yaml("retired"));
    valid(&set_history(&retired_owner, C, vec![good]));
    let parent = member(
        1,
        "parentage",
        yaml("{parent_id: location:missing}"),
        timeless(),
        None,
    );
    invalid(&set_history(&base(), B, vec![parent]));
    let self_parent = member(
        1,
        "parentage",
        yaml("{parent_id: location:b}"),
        timeless(),
        None,
    );
    invalid(&set_history(&base(), B, vec![self_parent]));
}

#[test]
fn inherited_geometry_keeps_its_vertex_budget_within_bounded_history() {
    let mut authored = shape(1, "geometry", timeless());
    authored["value"]["geometry"]["kind"] = yaml("line");
    authored["value"]["geometry"]["coordinates"] = Yaml::Sequence(vec![yaml("[1,1]"); 10_000]);
    valid(&set_history(&base(), B, vec![authored.clone()]));
    authored["value"]["geometry"]["coordinates"]
        .as_sequence_mut()
        .unwrap()
        .push(yaml("[1,1]"));
    invalid(&set_history(&base(), B, vec![authored]));
}

#[test]
fn route_metrics_are_independent_literal_fields_with_exact_integer_values() {
    let values = vec![
        member(
            1,
            "route-metric",
            yaml("{route_distance: {value: 18446744073709551615, unit: pace}}"),
            timeless(),
            None,
        ),
        member(
            2,
            "route-metric",
            yaml("{travel_cost: {value: 0.1, unit: coin}}"),
            timeless(),
            None,
        ),
        member(
            3,
            "route-metric",
            yaml("{duration: {value: 2, unit: beats}}"),
            timeless(),
            None,
        ),
        member(
            4,
            "route-availability",
            yaml("{availability: restricted}"),
            timeless(),
            None,
        ),
    ];
    let world = set_history(&base(), ROUTE, values);
    valid(&world);
    let value = history(&world, ROUTE, Some(&at(0, 0)), 4);
    assert_eq!(
        value["items"][0]["value"]["route_distance"]["value"],
        u64::MAX.to_string()
    );
    assert_eq!(value["items"][1]["value"]["travel_cost"]["value"], 0.1);
    assert_eq!(value["items"][3]["value"]["availability"], "restricted");
    for payload in [
        "{route_distance: {value: -1, unit: m}}",
        "{duration: {value: .inf, unit: s}}",
        "{travel_cost: {value: 1, unit: ''}}",
        "{duration: {value: 1, unit: s, extra: 2}}",
        "{route_distance: {value: 1, unit: m}, duration: {value: 1, unit: s}}",
        "{speed: {value: 1, unit: m}}",
    ] {
        invalid(&set_history(
            &base(),
            ROUTE,
            vec![member(1, "route-metric", yaml(payload), timeless(), None)],
        ));
    }
    invalid(&set_history(
        &base(),
        ROUTE,
        vec![member(
            1,
            "route-availability",
            yaml("{availability: maybe}"),
            timeless(),
            None,
        )],
    ));
}

#[test]
fn capability_owner_kind_and_local_identity_are_explicit() {
    let authored = root(1, timeless());
    let world = set_history(&base(), B, vec![authored.clone()]);
    let no_history = edited(&world, WORLD, |data| {
        data["capabilities"] = yaml("[spatial-core-v1, geometry-v1, route-v1]")
    });
    invalid(&no_history);
    assert!(spatial_history::history(&no_history, B, None, 10).is_err());
    let no_spatial = edited(&world, WORLD, |data| {
        data["capabilities"] = yaml("[geometry-v1, route-v1, spatial-history-v1]")
    });
    invalid(&no_spatial);
    let no_geometry = edited(
        &set_history(&base(), B, vec![shape(1, "geometry", timeless())]),
        WORLD,
        |data| data["capabilities"] = yaml("[spatial-core-v1, route-v1, spatial-history-v1]"),
    );
    invalid(&no_geometry);
    let no_route = edited(
        &set_history(
            &base(),
            ROUTE,
            vec![member(
                1,
                "route-availability",
                yaml("{availability: closed}"),
                timeless(),
                None,
            )],
        ),
        WORLD,
        |data| data["capabilities"] = yaml("[spatial-core-v1, geometry-v1, spatial-history-v1]"),
    );
    invalid(&no_route);
    invalid(&set_history(&base(), MAP, vec![]));
    invalid(&set_history(&base(), ROUTE, vec![authored.clone()]));
    invalid(&set_history(
        &base(),
        B,
        vec![member(
            1,
            "route-availability",
            yaml("{availability: closed}"),
            timeless(),
            None,
        )],
    ));
    let two_owners = set_history(&world, C, vec![authored.clone()]);
    valid(&two_owners);
    let mut cross_owner = authored;
    cross_owner["id"] = Yaml::String(local(2));
    cross_owner["replaces"] = Yaml::String(local(1));
    invalid(&set_history(&world, C, vec![cross_owner]));
}

#[test]
fn containment_cycles_are_checked_at_start_and_after_fallback_boundaries() {
    let cycle = member(
        1,
        "parentage",
        yaml("{parent_id: location:b}"),
        interval("main", Some((10, 1)), Some((20, 0))),
        None,
    );
    invalid(&set_history(&base(), A, vec![cycle]));
    // B temporarily becomes a root while A adopts B. At the root override's
    // inclusive end successor, B falls back to A and creates a cycle.
    for end in [(10, 0), (10, i32::MAX), (i64::MAX, i32::MAX - 1)] {
        let a_parent = member(
            1,
            "parentage",
            yaml("{parent_id: location:b}"),
            interval("main", Some((0, 0)), None),
            None,
        );
        let a = set_history(&base(), A, vec![a_parent]);
        let b = set_history(
            &a,
            B,
            vec![root(2, interval("main", Some((0, 0)), Some(end)))],
        );
        invalid(&b);
    }
    let a_parent = member(
        1,
        "parentage",
        yaml("{parent_id: location:b}"),
        interval("main", Some((0, 0)), Some((10, i32::MAX))),
        None,
    );
    let a = set_history(&base(), A, vec![a_parent]);
    let b = set_history(
        &a,
        B,
        vec![root(
            2,
            interval("main", Some((0, 0)), Some((10, i32::MAX))),
        )],
    );
    valid(&b);
}

#[test]
fn last_representable_boundary_never_overflows_and_timelines_stay_independent() {
    let a_parent = member(
        1,
        "parentage",
        yaml("{parent_id: location:b}"),
        interval("main", Some((0, 0)), None),
        None,
    );
    let a = set_history(&base(), A, vec![a_parent]);
    let b = set_history(
        &a,
        B,
        vec![root(
            2,
            interval("main", Some((0, 0)), Some((i64::MAX, i32::MAX))),
        )],
    );
    valid(&b);
    let main_root = root(2, interval("other", Some((0, 0)), None));
    invalid(&set_history(&a, B, vec![main_root]));
    // A timeless root override removes the static B→A edge on every timeline.
    valid(&set_history(&a, B, vec![root(2, timeless())]));
}

#[test]
fn source_and_complete_result_bounds_are_independent_and_never_truncate() {
    let values: Vec<_> = (0..1024)
        .map(|number| {
            root(
                number + 1,
                interval("main", Some((number as i64, 0)), Some((number as i64, 0))),
            )
        })
        .collect();
    let world = set_history(&base(), B, values.clone());
    valid(&world);
    assert!(spatial_history::history(&world, B, None, 1000).is_err());
    assert_eq!(history(&world, B, Some(&at(400, 0)), 1)["count"], 1);
    for limit in [0, 1001, usize::MAX] {
        assert!(spatial_history::history(&world, B, Some(&at(400, 0)), limit).is_err());
    }
    let mut oversized = values;
    oversized.push(root(
        1025,
        interval("main", Some((1024, 0)), Some((1024, 0))),
    ));
    invalid(&set_history(&base(), B, oversized));
}

#[test]
fn canonical_history_never_changes_complete_generic_character_evidence() {
    let mut before = base();
    before = added(&before, record("character", HERO, "initial_state: {}\n"));
    before = added(
        &before,
        record(
            "knowledge",
            KNOW,
            &format!(
                "knower: {HERO}\nclaim: {{key: lamp, statement: I saw a lamp}}\ntransitions: [{{id: learned, time: {{tick: 0}}, state: accepted, acquisition: observed, confidence: 1}}]\n"
            ),
        ),
    );
    valid(&before);
    let first = shape(
        1,
        "placement",
        interval("main", Some((0, 0)), Some((10, 0))),
    );
    let mut corrected = first.clone();
    corrected["id"] = Yaml::String(local(2));
    corrected["replaces"] = Yaml::String(local(1));
    corrected["value"]["geometry"]["coordinates"] = yaml("[8,9]");
    let changed = set_history(&before, B, vec![first, corrected]);
    let changed = edited(&changed, B, |data| {
        data["title"] = yaml("Hidden spatial label")
    });
    valid(&changed);
    for point in [at(0, 0), at(5, 0), at(10, 1)] {
        assert_eq!(
            character::context(&before, HERO, &point, 100_000).unwrap(),
            character::context(&changed, HERO, &point, 100_000).unwrap()
        );
        assert_eq!(
            character::context(&before, HERO, &point, 1)
                .unwrap_err()
                .json(),
            character::context(&changed, HERO, &point, 1)
                .unwrap_err()
                .json()
        );
        for query in ["lamp", "spatial", "hidden"] {
            assert_eq!(
                character::search(&before, HERO, query, &point, 20).unwrap(),
                character::search(&changed, HERO, query, &point, 20).unwrap()
            );
        }
    }
}
