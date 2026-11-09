use serde_json::Value as Json;
use std::{collections::BTreeMap, path::PathBuf};
use wedl::{
    Error,
    model::{Record, StoryTime, World},
    navigation::causal::{causal, continuity_advisories},
};

fn record(id: &str, kind: &str, fields: &str) -> Record {
    let yaml = format!("schema: wedl/v1\nid: {id}\nkind: {kind}\ntitle: {id}\n{fields}");
    Record {
        id: id.into(),
        kind: kind.into(),
        title: id.into(),
        source_path: format!("story/{id}.md"),
        frontmatter: serde_yaml::from_str(&yaml).unwrap(),
        body: "Body prose never establishes causality.".into(),
        raw_bytes: yaml.into_bytes(),
        blob_oid: None,
    }
}
fn world(records: Vec<Record>) -> World {
    let config = record(
        "world",
        "world",
        "default_timeline: main\ntimelines: [{id: main}, {id: other}]\n",
    );
    let mut all = BTreeMap::from([(config.id.clone(), config)]);
    all.extend(
        records
            .into_iter()
            .map(|record| (record.id.clone(), record)),
    );
    World {
        root: PathBuf::new(),
        records: all,
        fingerprint: "fixture".into(),
        revision: None,
    }
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime {
        timeline: "main".into(),
        tick,
        order,
    }
}
fn event(id: &str, tick: i64, order: i32, causes: &[&str]) -> Record {
    record(
        id,
        "event",
        &format!(
            "time: {{tick: {tick}, order: {order}}}\ncauses: [{}]\n",
            causes.join(", ")
        ),
    )
}
fn ids(result: &Json) -> Vec<&str> {
    result["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node["eventId"].as_str().unwrap())
        .collect()
}
fn replace(world: &mut World, id: &str, field: &str, value: &str) {
    world.records.get_mut(id).unwrap().frontmatter[field] = serde_yaml::from_str(value).unwrap();
}
fn graph() -> World {
    world(vec![
        event("origin", 1, 0, &[]),
        event("north", 2, 1, &["origin"]),
        event("south", 2, 1, &["origin"]),
        event("reunion", 3, 0, &["north", "south"]),
        event("southern-detour", 4, 0, &["south"]),
        event("unrelated", 3, 1, &[]),
    ])
}

#[test]
fn directions_follow_only_authored_edges_and_both_includes_shared_cause_siblings() {
    let world = graph();
    let horizon = at(10, 0);
    let upstream = causal(&world, "north", &horizon, "upstream", 1000).unwrap();
    assert_eq!(ids(&upstream), vec!["origin", "north"]);
    let downstream = causal(&world, "north", &horizon, "downstream", 1000).unwrap();
    assert_eq!(ids(&downstream), vec!["north", "reunion"]);
    let both = causal(&world, "north", &horizon, "both", 1000).unwrap();
    assert_eq!(
        ids(&both),
        vec!["origin", "north", "south", "reunion", "southern-detour"]
    );
    assert_eq!(both["edges"].as_array().unwrap().len(), 5);
    assert_eq!(both["direction"], "both");
    assert_eq!(both["targetEventId"], "north");
    assert_eq!(both["focusEvent"]["title"], "north");
    assert!(
        both["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|node| node["status"] == "canonical")
    );
    assert!(!both.to_string().contains("unrelated"));
    assert!(!both.to_string().contains("Body prose"));
    let links: Vec<_> = both["edges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|edge| {
            assert_eq!(edge["causeId"], edge["cause"]["id"]);
            assert_eq!(edge["eventId"], edge["effect"]["id"]);
            assert!(
                edge["citation"]["section"]
                    .as_str()
                    .unwrap()
                    .starts_with("causes[")
            );
            (
                edge["causeId"].as_str().unwrap(),
                edge["eventId"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        links,
        vec![
            ("north", "reunion"),
            ("origin", "north"),
            ("origin", "south"),
            ("south", "reunion"),
            ("south", "southern-detour")
        ]
    );
}

#[test]
fn same_tick_order_and_exact_extreme_coordinates_are_preserved() {
    let world = world(vec![
        event("minimum", i64::MIN, i32::MIN, &[]),
        event("exact", 9_007_199_254_740_993, 1, &["minimum"]),
        event("later", 9_007_199_254_740_993, 2, &["exact"]),
        event("maximum", i64::MAX, i32::MAX, &["later"]),
        record("other", "event", "time: {timeline: other, tick: 0}\n"),
    ]);
    let clipped = causal(
        &world,
        "exact",
        &at(9_007_199_254_740_993, 1),
        "downstream",
        1000,
    )
    .unwrap();
    assert_eq!(ids(&clipped), vec!["exact"]);
    assert_eq!(clipped["nodes"][0]["time"]["tick"], "9007199254740993");
    let full = causal(&world, "maximum", &at(i64::MAX, i32::MAX), "upstream", 1000).unwrap();
    assert_eq!(ids(&full), vec!["minimum", "exact", "later", "maximum"]);
    assert_eq!(full["nodes"][0]["time"]["tick"], i64::MIN.to_string());
    assert_eq!(full["nodes"][3]["time"]["order"], i32::MAX.to_string());
    assert!(!full.to_string().contains("\"other\""));
}

#[test]
fn future_and_withheld_edges_do_not_enter_nodes_advisories_or_limits() {
    let mut world = world(vec![
        record("north-place", "location", ""),
        record("south-place", "location", ""),
        event("root", 5, 1, &["hidden"]),
        event("hidden", 1, 0, &[]),
        event("future", 5, 2, &["root"]),
        event("hidden-later", 5, 1, &["hidden"]),
        record(
            "hidden-plot",
            "story-point",
            "activation_policy: manual\nlifecycle: {initial_state: dormant}\noutcome_events: [root]\naudience: []\n",
        ),
    ]);
    replace(&mut world, "root", "location", "north-place");
    replace(&mut world, "hidden", "location", "south-place");
    replace(&mut world, "hidden", "audience", "[]");
    replace(&mut world, "hidden-later", "perspectives", "[]");
    replace(&mut world, "future", "location", "south-place");
    let before = causal(&world, "root", &at(5, 1), "both", 1).unwrap();
    assert_eq!(ids(&before), vec!["root"]);
    assert!(before["edges"].as_array().unwrap().is_empty());
    assert!(
        before["continuityAdvisories"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    for id in ["hidden", "future", "hidden-later", "hidden-plot"] {
        world.records.get_mut(id).unwrap().title = format!("SECRET {id}");
    }
    replace(&mut world, "future", "causes", "[root, hidden]");
    replace(
        &mut world,
        "hidden-plot",
        "outcome_events",
        "[root, future, hidden]",
    );
    assert_eq!(
        before,
        causal(&world, "root", &at(5, 1), "both", 1).unwrap()
    );
    assert!(!before.to_string().contains("SECRET"));
}

#[test]
fn focus_name_resolution_ignores_withheld_future_and_non_event_alias_collisions() {
    let mut world = world(vec![
        event("visible-id", 1, 0, &[]),
        event("hidden-id", 1, 0, &[]),
        event("future-id", 2, 0, &[]),
        record("non-event-id", "character", "aliases: [focus]\n"),
    ]);
    replace(&mut world, "visible-id", "aliases", "[focus]");
    replace(&mut world, "hidden-id", "audience", "[]");
    let before = causal(&world, "focus", &at(1, 0), "both", 1000).unwrap();
    assert_eq!(before["targetEventId"], "visible-id");
    replace(&mut world, "hidden-id", "aliases", "[focus]");
    replace(&mut world, "future-id", "aliases", "[focus]");
    assert_eq!(
        before,
        causal(&world, "focus", &at(1, 0), "both", 1000).unwrap()
    );
    let error = causal(&world, "focus", &at(2, 0), "both", 1000).unwrap_err();
    assert!(matches!(error, Error::Ambiguous(_)));
    assert!(!error.to_string().contains("hidden-id"));
    assert!(matches!(
        causal(&world, "hidden-id", &at(2, 0), "both", 1000),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn continuity_advisories_are_exact_authored_gaps_and_not_implied_causal_links() {
    let mut world = world(vec![
        record("north-place", "location", ""),
        record("south-place", "location", ""),
        event("cause", 1, 0, &[]),
        event("effect", 2, 0, &["cause"]),
        event("unrelated-outcome", 3, 0, &[]),
        event("future", 4, 0, &[]),
        record(
            "plot",
            "story-point",
            "activation_policy: manual\nlifecycle: {initial_state: dormant}\noutcome_events: [effect, unrelated-outcome, future]\n",
        ),
    ]);
    replace(&mut world, "cause", "location", "north-place");
    replace(&mut world, "effect", "location", "south-place");
    replace(&mut world, "effect", "related_story_points", "[plot]");
    let result = causal(&world, "effect", &at(3, 0), "both", 1000).unwrap();
    assert_eq!(ids(&result), vec!["cause", "effect"]);
    let notices = result["continuityAdvisories"].as_array().unwrap();
    assert_eq!(notices.len(), 2);
    assert_eq!(notices[0]["kind"], "plot-link-asymmetry");
    assert_eq!(notices[0]["event"]["id"], "unrelated-outcome");
    assert_eq!(notices[0]["storyPoint"]["id"], "plot");
    assert_eq!(notices[0]["citation"]["section"], "outcome_events[1]");
    assert_eq!(notices[1]["kind"], "cross-location-causality");
    assert_eq!(notices[1]["cause"]["id"], "cause");
    assert_eq!(notices[1]["effect"]["id"], "effect");
    assert_eq!(notices[1]["causeLocation"]["id"], "north-place");
    assert_eq!(notices[1]["effectLocation"]["id"], "south-place");
    assert_eq!(
        notices,
        &continuity_advisories(&world, &at(3, 0), 1000).unwrap()
    );
    assert_eq!(
        continuity_advisories(&world, &at(1, 0), 1000).unwrap(),
        Vec::<Json>::new()
    );
    replace(&mut world, "south-place", "audience", "[]");
    let hidden_location = continuity_advisories(&world, &at(3, 0), 1000).unwrap();
    assert_eq!(hidden_location.len(), 1);
    assert!(
        !serde_json::to_string(&hidden_location)
            .unwrap()
            .contains("south-place")
    );
}

#[test]
fn noncanonical_withheld_wrong_kind_and_future_focus_are_refused() {
    let mut world = graph();
    replace(&mut world, "north", "status", "draft");
    assert!(
        causal(&world, "north", &at(10, 0), "both", 1000)
            .unwrap_err()
            .to_string()
            .contains("canonical")
    );
    replace(&mut world, "north", "audience", "[]");
    assert!(matches!(
        causal(&world, "north", &at(10, 0), "both", 1000),
        Err(Error::NotFound(_))
    ));
    assert!(matches!(
        causal(&world, "reunion", &at(2, 1), "both", 1000),
        Err(Error::NotFound(_))
    ));
    assert!(matches!(
        causal(&world, "world", &at(10, 0), "both", 1000),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        causal(&world, "missing", &at(10, 0), "both", 1000),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn graph_and_advisory_limits_fail_completely_and_directions_are_closed() {
    let world = graph();
    assert!(
        causal(&world, "north", &at(10, 0), "upstream", 1)
            .unwrap_err()
            .to_string()
            .contains("nodes")
    );
    assert!(causal(&world, "origin", &at(1, 0), "upstream", 1).is_ok());
    assert!(causal(&world, "north", &at(10, 0), "neighbors", 1000).is_err());
    assert!(causal(&world, "north", &at(10, 0), "both", 0).is_err());
    assert!(causal(&world, "north", &at(10, 0), "both", 1001).is_err());
    let dense = world_with_dense_edges();
    assert!(
        causal(&dense, "fourth", &at(4, 0), "upstream", 4)
            .unwrap_err()
            .to_string()
            .contains("edges")
    );
    let notices = world_with_multiple_advisories();
    assert!(
        continuity_advisories(&notices, &at(3, 0), 1)
            .unwrap_err()
            .to_string()
            .contains("advisories")
    );
    assert_eq!(
        continuity_advisories(&notices, &at(3, 0), 2).unwrap().len(),
        2
    );
}
fn world_with_dense_edges() -> World {
    world(vec![
        event("first", 1, 0, &[]),
        event("second", 2, 0, &["first"]),
        event("third", 3, 0, &["first", "second"]),
        event("fourth", 4, 0, &["first", "second", "third"]),
    ])
}
fn world_with_multiple_advisories() -> World {
    world(vec![
        event("first", 1, 0, &[]),
        event("second", 2, 0, &[]),
        record(
            "plot",
            "story-point",
            "activation_policy: manual\nlifecycle: {initial_state: dormant}\noutcome_events: [first, second]\n",
        ),
    ])
}

#[test]
fn malformed_visible_graph_cannot_turn_equal_or_reverse_time_into_causality() {
    let equal = world(vec![
        event("first", 1, 0, &[]),
        event("second", 1, 0, &["first"]),
    ]);
    assert!(causal(&equal, "second", &at(2, 0), "upstream", 1000).is_err());
    let reversed = world(vec![
        event("first", 2, 0, &[]),
        event("second", 1, 0, &["first"]),
    ]);
    assert!(causal(&reversed, "second", &at(2, 0), "upstream", 1000).is_err());
    let bounded = causal(&reversed, "second", &at(1, 0), "upstream", 1000).unwrap();
    assert_eq!(ids(&bounded), vec!["second"]);
    assert!(bounded["edges"].as_array().unwrap().is_empty());
    let duplicate = world(vec![
        event("first", 1, 0, &[]),
        event("second", 2, 0, &["first", "first"]),
    ]);
    assert!(causal(&duplicate, "second", &at(2, 0), "upstream", 1000).is_err());
}

#[test]
fn causal_reads_leave_every_authored_value_and_body_untouched() {
    let world = graph();
    let before: Vec<_> = world
        .records
        .values()
        .map(|record| {
            (
                record.id.clone(),
                record.raw_bytes.clone(),
                record.frontmatter.clone(),
                record.body.clone(),
            )
        })
        .collect();
    for direction in ["upstream", "downstream", "both"] {
        causal(&world, "north", &at(10, 0), direction, 1000).unwrap();
    }
    for (id, raw, fields, body) in before {
        assert_eq!(world.records[&id].raw_bytes, raw);
        assert_eq!(world.records[&id].frontmatter, fields);
        assert_eq!(world.records[&id].body, body);
    }
}

#[test]
fn graph_preflight_bounds_admitted_evidence_before_selected_traversal() {
    let mut events = vec![event("root", 0, 0, &[])];
    events.extend((0..16_384).map(|index| event(&format!("event-{index:05}"), 1, 0, &[])));
    let world = world(events);
    assert_eq!(
        ids(&causal(&world, "root", &at(0, 0), "upstream", 1).unwrap()),
        vec!["root"]
    );
    let error = causal(&world, "root", &at(1, 0), "upstream", 1).unwrap_err();
    assert!(error.to_string().contains("16384 admitted events"));
}

#[test]
fn graph_preflight_refuses_excessive_admitted_edge_work() {
    let names: Vec<_> = (0..363).map(|index| format!("event-{index:03}")).collect();
    let events = names
        .iter()
        .enumerate()
        .map(|(index, id)| {
            let causes: Vec<_> = names[..index].iter().map(String::as_str).collect();
            event(id, index as i64, 0, &causes)
        })
        .collect();
    let world = world(events);
    let error = causal(&world, &names[0], &at(363, 0), "upstream", 1).unwrap_err();
    assert!(error.to_string().contains("65536 admitted edges"));
}
