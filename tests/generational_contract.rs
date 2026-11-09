//! Literal source transitions, complete graph answers and exact-horizon evidence.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::{collections::BTreeMap, path::PathBuf};
use wedl::{
    generational::{self as genealogy, LineageDirection},
    model::{Record, StoryTime, World},
    source,
};

fn id(kind: &str, number: u32) -> String {
    let prefix = match kind {
        "character" => "char",
        "parentage" => "kinship",
        _ => kind,
    };
    format!("{prefix}_{number:026}")
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn transition(number: u32, kind: &str, tick: i64, order: i32, payload: Json) -> Json {
    json!({"transition_id":id("transition",number),"transition_kind":kind,
      "applicability":{"applicability_kind":"instant","point":{"timeline":"main","tick":tick,"order":order}},"payload":payload})
}
fn parentage(number: u32, child: u32, parent: u32) -> Json {
    json!({"kind":"parentage","id":id("parentage",number),"title":format!("Parentage {number}"),
      "child_id":id("character",child),"parent_id":id("character",parent),
      "initialization":transition(number*100,"parentage-initialize",0,0,json!({"basis":"biological"})),"transitions":[]})
}
fn union(number: u32, people: &[u32]) -> Json {
    let people: Vec<_> = people
        .iter()
        .map(|number| id("character", *number))
        .collect();
    json!({"kind":"union","id":id("union",number),"title":format!("Union {number}"),"participant_ids":people,
      "initialization":transition(number*100+50,"union-initialize",0,0,json!({"participant_ids":people})),"transitions":[]})
}
fn roster(people: &[u32]) -> Json {
    json!({"participant_ids":people.iter().map(|number|id("character",*number)).collect::<Vec<_>>()})
}
fn record(mut value: Json) -> Record {
    value["schema"] = json!(source::SOURCE_SCHEMA);
    let path = format!("story/{}.md", value["id"].as_str().unwrap());
    let raw = format!(
        "---\n{}---\nRetained source prose.\n",
        serde_yaml::to_string(&value).unwrap()
    );
    source::parse_record(raw.as_bytes(), &path).unwrap()
}
fn fixture_world(extra: Vec<Json>) -> World {
    let mut records = vec![record(
        json!({"kind":"world","id":id("world",1),"title":"Generational test",
      "capabilities":["generational-core-v1"],"default_timeline":"main",
      "timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"state_keys":{}}),
    )];
    records.extend((1..=8).map(|number| {
        record(json!({"kind":"character","id":id("character",number),
      "title":format!("Character {number}"),"aliases":[format!("Person {number}")]}))
    }));
    records.extend(extra.into_iter().map(record));
    source::world_from_records(PathBuf::new(), records, Some("fixture-revision".into())).unwrap()
}
fn edited(base: &World, target: &str, edit: impl FnOnce(&mut Yaml)) -> World {
    let original = &base.records[target];
    let mut value = original.frontmatter.clone();
    edit(&mut value);
    let raw = format!(
        "---\n{}---\n{}",
        serde_yaml::to_string(&value).unwrap(),
        original.body
    );
    let replacement = source::parse_record(raw.as_bytes(), &original.source_path).unwrap();
    let records = base
        .records
        .values()
        .map(|record| {
            if record.id == target {
                replacement.clone()
            } else {
                record.clone()
            }
        })
        .collect();
    source::world_from_records(base.root.clone(), records, base.revision.clone()).unwrap()
}
fn yaml(value: Json) -> Yaml {
    serde_yaml::to_value(value).unwrap()
}
fn tag(value: Yaml) -> Yaml {
    Yaml::Tagged(Box::new(serde_yaml::value::TaggedValue {
        tag: serde_yaml::value::Tag::new("custom"),
        value,
    }))
}
fn valid(world: &World) {
    let diagnostics = genealogy::validate(world);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}
fn invalid(world: &World) {
    let diagnostics = genealogy::validate(world);
    assert!(!diagnostics.is_empty());
    assert!(
        diagnostics
            .iter()
            .all(|item| item.severity == "error" && item.field.is_some() && item.path.is_some())
    );
    assert!(genealogy::family(world, &id("character", 1), &at(20, 0), 100).is_err());
}
fn family(world: &World, number: u32, tick: i64, order: i32) -> Json {
    genealogy::family(world, &id("character", number), &at(tick, order), 100).unwrap()
}

#[test]
fn parentage_is_inclusive_literal_and_stops_at_its_exact_terminal_instant() {
    let mut edge = parentage(1, 1, 2);
    edge["transitions"] = json!([
        transition(101, "parentage-confirm", 3, 2, json!({"basis":"adoptive"})),
        transition(102, "parentage-end", 10, 5, json!({}))
    ]);
    let world = fixture_world(vec![edge]);
    valid(&world);
    let report = wedl::validation::validate_world(&world);
    assert!(report.valid, "{:?}", report.diagnostics);
    let original: BTreeMap<_, _> = world
        .records
        .iter()
        .map(|(id, record)| (id.clone(), record.raw_bytes.clone()))
        .collect();
    for (tick, order, lifecycle, basis, count) in [
        (0, 0, "asserted", "biological", 1),
        (3, 1, "asserted", "biological", 1),
        (3, 2, "confirmed", "adoptive", 2),
        (10, 4, "confirmed", "adoptive", 2),
    ] {
        let result = family(&world, 1, tick, order);
        let parent = &result["parents"][0];
        assert_eq!(parent["targetId"], id("character", 2));
        assert_eq!(parent["lifecycle"], lifecycle);
        assert_eq!(parent["value"]["basis"], basis);
        assert_eq!(parent["citations"].as_array().unwrap().len(), count);
        assert_eq!(parent["citations"][0]["field"], "initialization");
        assert_eq!(parent["citations"][0]["revision"], "fixture-revision");
        assert_eq!(parent["citations"][0]["fingerprint"], world.fingerprint);
    }
    assert_eq!(family(&world, 1, -1, i32::MAX)["parents"], json!([]));
    assert_eq!(family(&world, 1, 10, 5)["parents"], json!([]));
    let history = genealogy::history(&world, &id("parentage", 1), &at(10, 5), 100).unwrap();
    assert_eq!(history["lifecycle"], "ended");
    assert_eq!(history["active"], false);
    assert_eq!(history["count"], 3);
    assert_eq!(
        family(&world, 2, 0, 0)["children"][0]["targetId"],
        id("character", 1)
    );
    assert_eq!(
        original,
        world
            .records
            .iter()
            .map(|(id, record)| (id.clone(), record.raw_bytes.clone()))
            .collect()
    );
}

#[test]
fn history_horizons_do_not_leak_future_values_or_cross_timeline_boundaries() {
    let start = 9_007_199_254_740_993_i64;
    let mut edge = parentage(1, 1, 2);
    edge["initialization"]["applicability"]["point"]["tick"] = json!(start);
    edge["initialization"]["applicability"]["point"]["order"] = json!(-1);
    edge["transitions"] = json!([transition(
        101,
        "parentage-confirm",
        start,
        1,
        json!({"basis":"adoptive"})
    )]);
    let world = fixture_world(vec![edge]);
    for (at, reason) in [
        (at(start, -2), "before_initialization"),
        (
            StoryTime::new("other", i64::MAX, i32::MAX).unwrap(),
            "different_timeline",
        ),
    ] {
        let history = genealogy::history(&world, &id("parentage", 1), &at, 100).unwrap();
        assert_eq!(history["available"], false);
        assert_eq!(history["reason"], reason);
        assert!(history.get("value").is_none());
        assert_eq!(history["history"], json!([]));
    }
    let before = genealogy::history(&world, &id("parentage", 1), &at(start, 0), 100).unwrap();
    assert_eq!(before["lifecycle"], "asserted");
    assert_eq!(before["effectiveTime"]["tick"], start.to_string());
    assert_eq!(before["history"][0]["time"]["order"], "-1");
    assert_eq!(before["value"]["basis"], "biological");
    assert!(!before.to_string().contains("adoptive"));
    assert_eq!(
        genealogy::history(&world, &id("parentage", 1), &at(start, 1), 100).unwrap()["value"]["basis"],
        "adoptive"
    );
}

#[test]
fn union_membership_uses_the_current_payload_roster_and_filters_before_budgets() {
    let mut source = union(1, &[1, 2]);
    source["initialization"]["payload"] = roster(&[2, 3]);
    source["transitions"] = json!([
        transition(151, "union-form", 3, 1, roster(&[3, 4])),
        transition(152, "union-reconcile", 6, 2, roster(&[4, 5])),
        transition(153, "union-end", 9, 3, json!({}))
    ]);
    let world = fixture_world(vec![source]);
    valid(&world);
    assert_eq!(family(&world, 1, 0, 0)["unions"], json!([]));
    assert_eq!(
        family(&world, 2, 0, 0)["unions"][0]["lifecycle"],
        "declared"
    );
    assert_eq!(
        family(&world, 2, 3, 0)["unions"].as_array().unwrap().len(),
        1
    );
    assert_eq!(family(&world, 2, 3, 1)["unions"], json!([]));
    assert_eq!(
        family(&world, 3, 6, 1)["unions"].as_array().unwrap().len(),
        1
    );
    assert_eq!(family(&world, 3, 6, 2)["unions"], json!([]));
    let current = family(&world, 5, 6, 2);
    assert_eq!(
        current["unions"][0]["value"]["participantIds"],
        json!([id("character", 4), id("character", 5)])
    );
    assert_eq!(current["unions"][0]["lifecycle"], "formed");
    // A removed member does not consume the current roster/history budget.
    assert_eq!(
        genealogy::family(&world, &id("character", 2), &at(6, 2), 1).unwrap()["count"],
        0
    );
    assert_eq!(family(&world, 5, 9, 3)["unions"], json!([]));
    assert_eq!(
        genealogy::history(&world, &id("union", 1), &at(9, 3), 100).unwrap()["active"],
        false
    );
    assert_eq!(
        genealogy::kinship_path(
            &world,
            &id("character", 4),
            &id("character", 5),
            &at(6, 2),
            0,
            1
        )
        .unwrap()["found"],
        false
    );
}

#[test]
fn correction_chains_restore_target_prestate_and_retain_all_source_evidence() {
    let mut edge = parentage(1, 1, 2);
    let original = transition(101, "parentage-end", 5, 3, json!({}));
    let mut correction = transition(102, "parentage-confirm", 5, 3, json!({"basis":"adoptive"}));
    correction["replaces_transition_id"] = json!(id("transition", 101));
    let mut correction2 = transition(
        103,
        "parentage-confirm",
        5,
        3,
        json!({"basis":"biological"}),
    );
    correction2["replaces_transition_id"] = json!(id("transition", 102));
    edge["transitions"] = json!([
        original,
        correction,
        correction2,
        transition(104, "parentage-end", 9, 0, json!({}))
    ]);
    let world = fixture_world(vec![edge]);
    valid(&world);
    let before = genealogy::history(&world, &id("parentage", 1), &at(5, 2), 100).unwrap();
    assert_eq!(before["count"], 1);
    assert_eq!(before["lifecycle"], "asserted");
    let after = genealogy::history(&world, &id("parentage", 1), &at(5, 3), 100).unwrap();
    assert_eq!(after["lifecycle"], "confirmed");
    assert_eq!(after["value"]["basis"], "biological");
    assert_eq!(
        after["appliedTransitionIds"],
        json!([id("transition", 100), id("transition", 103)])
    );
    assert_eq!(after["citations"].as_array().unwrap().len(), 4);
    assert_eq!(after["history"][1]["superseded"], true);
    assert_eq!(after["history"][2]["superseded"], true);
    assert_eq!(after["history"][3]["superseded"], false);
    assert_eq!(
        after["citations"][3]["replacesTransitionId"],
        id("transition", 102)
    );
    assert_eq!(
        after["citations"][2]["replacesTransitionId"],
        id("transition", 101)
    );
    assert_eq!(
        family(&world, 1, 5, 3)["parents"].as_array().unwrap().len(),
        1
    );
    assert_eq!(family(&world, 1, 9, 0)["parents"], json!([]));
}

#[test]
fn replacement_forks_validate_from_the_target_and_replay_surviving_leaves_in_order() {
    let mut edge = parentage(1, 1, 2);
    let mut terminal = transition(102, "parentage-end", 4, 0, json!({}));
    terminal["replaces_transition_id"] = json!(id("transition", 101));
    let mut confirmed = transition(103, "parentage-confirm", 4, 0, json!({"basis":"adoptive"}));
    confirmed["replaces_transition_id"] = json!(id("transition", 101));
    edge["transitions"] = json!([
        transition(
            101,
            "parentage-confirm",
            4,
            0,
            json!({"basis":"biological"})
        ),
        terminal,
        confirmed,
        transition(104, "parentage-end", 6, 0, json!({}))
    ]);
    let world = fixture_world(vec![edge]);
    valid(&world);
    let result = genealogy::history(&world, &id("parentage", 1), &at(4, 0), 100).unwrap();
    assert_eq!(result["lifecycle"], "confirmed");
    assert_eq!(result["value"]["basis"], "adoptive");
    assert_eq!(
        result["appliedTransitionIds"],
        json!([
            id("transition", 100),
            id("transition", 102),
            id("transition", 103)
        ])
    );
    assert_eq!(result["history"][1]["superseded"], true);
    assert_eq!(result["history"][2]["superseded"], false);
    assert_eq!(result["history"][3]["superseded"], false);
}

#[test]
fn replacements_require_an_earlier_same_record_exact_applicability_target() {
    for replacement in [None, Some(100), Some(101), Some(999)] {
        let mut edge = parentage(1, 1, 2);
        let mut update = transition(101, "parentage-confirm", 0, 0, json!({"basis":"adoptive"}));
        if let Some(target) = replacement {
            update["replaces_transition_id"] = json!(id("transition", target));
        }
        edge["transitions"] = json!([update]);
        invalid(&fixture_world(vec![edge]));
    }
    let mut edge = parentage(1, 1, 2);
    let mut wrong_time = transition(102, "parentage-end", 2, 0, json!({}));
    wrong_time["replaces_transition_id"] = json!(id("transition", 101));
    edge["transitions"] = json!([
        transition(101, "parentage-confirm", 1, 0, json!({"basis":"adoptive"})),
        wrong_time
    ]);
    invalid(&fixture_world(vec![edge]));
    let mut edge = parentage(1, 1, 2);
    let mut other_record = transition(101, "parentage-confirm", 0, 0, json!({"basis":"adoptive"}));
    other_record["replaces_transition_id"] = json!(id("transition", 200));
    edge["transitions"] = json!([other_record]);
    invalid(&fixture_world(vec![edge, parentage(2, 2, 3)]));
}

#[test]
fn parentage_has_closed_state_and_basis_rules_without_a_two_parent_cap() {
    for transitions in [
        vec![
            transition(101, "parentage-confirm", 1, 0, json!({"basis":"adoptive"})),
            transition(
                102,
                "parentage-confirm",
                2,
                0,
                json!({"basis":"biological"}),
            ),
        ],
        vec![
            transition(101, "parentage-end", 1, 0, json!({})),
            transition(
                102,
                "parentage-confirm",
                2,
                0,
                json!({"basis":"biological"}),
            ),
        ],
        vec![
            transition(101, "parentage-end", 1, 0, json!({})),
            transition(102, "parentage-end", 2, 0, json!({})),
        ],
        vec![transition(
            101,
            "parentage-confirm",
            1,
            0,
            json!({"basis":"step"}),
        )],
        vec![transition(
            101,
            "parentage-end",
            1,
            0,
            json!({"basis":"biological"}),
        )],
    ] {
        let mut edge = parentage(1, 1, 2);
        edge["transitions"] = json!(transitions);
        invalid(&fixture_world(vec![edge]));
    }
    let mut ended = parentage(1, 1, 2);
    ended["transitions"] = json!([transition(101, "parentage-end", 1, 0, json!({}))]);
    valid(&fixture_world(vec![ended]));
    let world = fixture_world(vec![
        parentage(1, 1, 2),
        parentage(2, 1, 3),
        parentage(3, 1, 4),
    ]);
    valid(&world);
    assert_eq!(
        family(&world, 1, 0, 0)["parents"].as_array().unwrap().len(),
        3
    );
}

#[test]
fn unions_allow_terminal_from_declared_or_formed_and_never_reopen() {
    for terminal in ["union-end", "union-annul"] {
        for formed in [false, true] {
            let mut source = union(1, &[1, 2]);
            let mut transitions = Vec::new();
            if formed {
                transitions.push(transition(151, "union-form", 1, 0, roster(&[1, 2])));
            }
            transitions.push(transition(152, terminal, 2, 0, json!({})));
            source["transitions"] = json!(transitions);
            let candidate = fixture_world(vec![source.clone()]);
            valid(&candidate);
            let history = genealogy::history(&candidate, &id("union", 1), &at(2, 0), 100).unwrap();
            assert_eq!(
                history["lifecycle"],
                if terminal == "union-end" {
                    "ended"
                } else {
                    "annulled"
                }
            );
            assert_eq!(family(&candidate, 1, 2, 0)["unions"], json!([]));
            transitions.push(transition(153, "union-reconcile", 3, 0, roster(&[2, 3])));
            source["transitions"] = json!(transitions);
            invalid(&fixture_world(vec![source]));
        }
    }
    let mut source = union(1, &[1, 2]);
    source["transitions"] = json!([transition(151, "union-reconcile", 1, 0, roster(&[2, 3]))]);
    invalid(&fixture_world(vec![source]));
    let mut source = union(1, &[1, 2]);
    source["transitions"] = json!([
        transition(151, "union-form", 1, 0, roster(&[1, 2])),
        transition(152, "union-form", 2, 0, roster(&[1, 2]))
    ]);
    invalid(&fixture_world(vec![source]));
}

#[test]
fn causes_require_canonical_events_at_strictly_earlier_exact_points() {
    let mut edge = parentage(1, 1, 2);
    let mut confirmed = transition(
        101,
        "parentage-confirm",
        5,
        4,
        json!({"basis":"biological"}),
    );
    confirmed["cause_event_id"] = json!(id("event", 1));
    edge["transitions"] = json!([confirmed]);
    let event = json!({"kind":"event","id":id("event",1),"title":"Cause","time":{"timeline":"main","tick":5,"order":3}});
    let base = fixture_world(vec![edge, event]);
    valid(&base);
    let history = genealogy::history(&base, &id("parentage", 1), &at(5, 4), 100).unwrap();
    assert_eq!(
        history["citations"][1]["causeCitation"]["entityId"],
        id("event", 1)
    );
    assert_eq!(history["citations"][1]["causeCitation"]["field"], "time");
    for time in [
        json!({"timeline":"main","tick":5,"order":4}),
        json!({"timeline":"main","tick":6,"order":0}),
        json!({"timeline":"other","tick":0,"order":0}),
        json!({"tick":0}),
        json!({"timeline":"main","tick":0,"order":0,"extra":true}),
    ] {
        invalid(&edited(&base, &id("event", 1), |event| {
            event["time"] = yaml(time)
        }));
    }
    invalid(&edited(&base, &id("event", 1), |event| {
        event["status"] = yaml(json!("draft"))
    }));
    invalid(&edited(&base, &id("parentage", 1), |record| {
        record["transitions"][0]["cause_event_id"] = Yaml::Null
    }));
    invalid(&edited(&base, &id("parentage", 1), |record| {
        record["transitions"][0]["cause_event_id"] = yaml(json!(id("character", 1)))
    }));
}

#[test]
fn source_shapes_reject_implicit_times_null_extensions_and_tagged_execution_values() {
    let base = fixture_world(vec![parentage(1, 1, 2)]);
    for (path, value) in [
        ("/initialization/transition_id", json!("transition_short")),
        ("/initialization/transition_kind", json!("union-initialize")),
        (
            "/initialization/payload",
            json!({"basis":"biological","extra":1}),
        ),
        (
            "/initialization/applicability",
            json!({"applicability_kind":"static"}),
        ),
        ("/initialization/applicability/point", json!({"tick":0})),
        ("/initialization/applicability/point/tick", json!(0.5)),
        (
            "/initialization/applicability/point/order",
            json!(2_147_483_648_i64),
        ),
        (
            "/initialization/applicability/point/timeline",
            json!("unknown"),
        ),
        ("/child_id", json!(id("character", 2))),
        ("/parent_id", json!("Character 2")),
        ("/parent_id", json!(id("character", 999))),
        ("/transitions", Json::Null),
    ] {
        let mut edge = parentage(1, 1, 2);
        *edge.pointer_mut(path).unwrap() = value;
        invalid(&fixture_world(vec![edge]));
    }
    invalid(&edited(&base, &id("parentage", 1), |record| {
        record["initialization"]["cause_event_id"] = yaml(json!(id("event", 1)))
    }));
    invalid(&edited(&base, &id("parentage", 1), |record| {
        record["initialization"]["unexpected"] = Yaml::Null
    }));
    invalid(&edited(&base, &id("parentage", 1), |record| {
        record["status"] = Yaml::Null
    }));
    for field in ["timeline", "tick", "order"] {
        invalid(&edited(&base, &id("parentage", 1), |record| {
            let point = &mut record["initialization"]["applicability"]["point"];
            point[field] = tag(point[field].clone());
        }));
    }
    for field in ["initialization", "transitions", "parent_id", "status"] {
        invalid(&edited(&base, &id("parentage", 1), |record| {
            let value = if field == "status" {
                yaml(json!("canonical"))
            } else {
                record[field].clone()
            };
            record[field] = tag(value);
        }));
    }
    invalid(&edited(&base, &id("parentage", 1), |record| {
        record["initialization"]["payload"]["basis"] = tag(yaml(json!("biological")))
    }));
    let mut edge = parentage(1, 1, 2);
    let mut transition = transition(
        101,
        "parentage-confirm",
        1,
        0,
        json!({"basis":"biological"}),
    );
    transition["replaces_transition_id"] = Json::Null;
    edge["transitions"] = json!([transition]);
    invalid(&fixture_world(vec![edge]));
}

#[test]
fn histories_must_be_monotone_on_one_declared_timeline() {
    for (tick, order, timeline) in [
        (-1, 0, "main"),
        (0, -1, "main"),
        (0, 0, "main"),
        (1, 0, "other"),
    ] {
        let mut edge = parentage(1, 1, 2);
        let mut update = transition(
            101,
            "parentage-confirm",
            tick,
            order,
            json!({"basis":"biological"}),
        );
        update["applicability"]["point"]["timeline"] = json!(timeline);
        edge["transitions"] = json!([update]);
        invalid(&fixture_world(vec![edge]));
    }
}

#[test]
fn union_members_are_sorted_unique_canonical_characters_in_header_and_every_payload() {
    for roster in [
        json!([]),
        json!([id("character", 1)]),
        json!([id("character", 2), id("character", 1)]),
        json!([id("character", 1), id("character", 1)]),
        json!([id("character", 1), id("world", 1)]),
        json!([id("character", 1), id("character", 999)]),
        Json::Null,
    ] {
        let mut header = union(1, &[1, 2]);
        header["participant_ids"] = roster.clone();
        invalid(&fixture_world(vec![header]));
        let mut payload = union(1, &[1, 2]);
        payload["initialization"]["payload"]["participant_ids"] = roster;
        invalid(&fixture_world(vec![payload]));
    }
    let base = fixture_world(vec![union(1, &[1, 2])]);
    invalid(&edited(&base, &id("character", 2), |record| {
        record["status"] = yaml(json!("retired"))
    }));
    invalid(&edited(&base, &id("union", 1), |record| {
        record["participant_ids"] = tag(record["participant_ids"].clone())
    }));
}

#[test]
fn source_status_does_not_bypass_validation_or_admit_noncanonical_edges() {
    let mut draft = parentage(1, 1, 2);
    draft["status"] = json!("draft");
    let mut retired = union(1, &[1, 2]);
    retired["status"] = json!("retired");
    let world = fixture_world(vec![draft, retired]);
    valid(&world);
    assert_eq!(family(&world, 1, 0, 0)["count"], 0);
    for target in [id("parentage", 1), id("union", 1)] {
        let history = genealogy::history(&world, &target, &at(10, 0), 100).unwrap();
        assert_eq!(history["available"], false);
        assert_eq!(history["reason"], "noncanonical");
        assert_eq!(history["history"], json!([]));
    }
    invalid(&edited(&world, &id("parentage", 1), |record| {
        record["initialization"]["payload"]["basis"] = yaml(json!("unknown"))
    }));
    invalid(&edited(&world, &id("character", 2), |record| {
        record["status"] = yaml(json!("retired"))
    }));
    let unsupported = edited(&world, &id("union", 1), |record| {
        record["status"] = yaml(json!("formed"))
    });
    invalid(&unsupported);
}

#[test]
fn transition_identifiers_are_global_across_records_statuses_and_sibling_kinds() {
    let first = parentage(1, 1, 2);
    let mut second = parentage(2, 2, 3);
    second["status"] = json!("draft");
    second["initialization"]["transition_id"] = json!(id("transition", 100));
    let candidate = fixture_world(vec![first.clone(), second]);
    assert!(
        genealogy::validate(&candidate)
            .iter()
            .any(|diagnostic| diagnostic.code == "GEN-TRANSITION-006")
    );
    invalid(&candidate);
    let organization = json!({"kind":"organization","id":id("organization",1),"title":"Sibling namespace","organization_kind":"institution",
      "initialization":transition(100,"organization-initialize",0,0,json!({"title":"Group","aliases":[]})),"transitions":[]});
    invalid(&fixture_world(vec![first, organization]));
    let mut duplicate = parentage(1, 1, 2);
    duplicate["transitions"] = json!([transition(
        100,
        "parentage-confirm",
        1,
        0,
        json!({"basis":"biological"})
    )]);
    invalid(&fixture_world(vec![duplicate]));
}

#[test]
fn cycle_validation_includes_ended_edges_other_timelines_and_all_bases() {
    let mut ended = parentage(1, 1, 2);
    ended["transitions"] = json!([transition(101, "parentage-end", 1, 0, json!({}))]);
    let mut reverse = parentage(2, 2, 1);
    reverse["initialization"]["payload"]["basis"] = json!("adoptive");
    reverse["initialization"]["applicability"]["point"] =
        json!({"timeline":"other","tick":100,"order":0});
    let candidate = fixture_world(vec![ended.clone(), reverse.clone()]);
    assert!(
        genealogy::validate(&candidate)
            .iter()
            .any(|diagnostic| diagnostic.code == "GEN-PARENTAGE-002")
    );
    invalid(&candidate);
    reverse["status"] = json!("draft");
    valid(&fixture_world(vec![ended, reverse]));
}

#[test]
fn duplicates_use_only_the_exact_initial_edge_tuple_and_keep_distinct_evidence() {
    let mut duplicate = parentage(2, 1, 2);
    let candidate = fixture_world(vec![parentage(1, 1, 2), duplicate.clone()]);
    assert!(
        genealogy::validate(&candidate)
            .iter()
            .any(|diagnostic| diagnostic.code == "GEN-PARENTAGE-003")
    );
    invalid(&candidate);
    duplicate["initialization"]["payload"]["basis"] = json!("adoptive");
    let mut different_time = parentage(3, 1, 2);
    different_time["initialization"]["applicability"]["point"]["order"] = json!(1);
    let candidate = fixture_world(vec![parentage(1, 1, 2), duplicate, different_time]);
    valid(&candidate);
    let parents = family(&candidate, 1, 0, 1)["parents"].clone();
    assert_eq!(parents.as_array().unwrap().len(), 3);
    assert_eq!(parents[0]["recordId"], id("parentage", 1));
    assert_eq!(parents[1]["recordId"], id("parentage", 2));
    let ancestors = genealogy::lineage(
        &candidate,
        &id("character", 1),
        &at(0, 1),
        LineageDirection::Ancestors,
        1,
        100,
    )
    .unwrap();
    assert_eq!(ancestors["count"], 1);
    assert_eq!(
        ancestors["items"][0]["edges"][0]["recordId"],
        id("parentage", 1)
    );
}

#[test]
fn graph_answers_use_stable_shortest_continuous_authored_paths_and_complete_bounds() {
    let world = fixture_world(vec![
        parentage(1, 1, 2),
        parentage(2, 1, 3),
        parentage(3, 2, 4),
        parentage(4, 3, 4),
        parentage(5, 5, 2),
        union(1, &[4, 6]),
    ]);
    valid(&world);
    let ancestors = genealogy::lineage(
        &world,
        "Person 1",
        &at(0, 0),
        LineageDirection::Ancestors,
        2,
        3,
    )
    .unwrap();
    assert_eq!(ancestors["count"], 3);
    assert_eq!(ancestors["items"][2]["targetId"], id("character", 4));
    assert_eq!(ancestors["items"][2]["generationDistance"], "2");
    assert_eq!(
        ancestors["items"][2]["edges"][0]["recordId"],
        id("parentage", 1)
    );
    assert_eq!(
        ancestors["items"][2]["edges"][1]["recordId"],
        id("parentage", 3)
    );
    let descendants = genealogy::lineage(
        &world,
        &id("character", 4),
        &at(0, 0),
        LineageDirection::Descendants,
        2,
        100,
    )
    .unwrap();
    assert_eq!(descendants["count"], 4);
    let path = genealogy::kinship_path(
        &world,
        &id("character", 1),
        &id("character", 5),
        &at(0, 0),
        2,
        100,
    )
    .unwrap();
    assert_eq!(path["distance"], "2");
    assert_eq!(path["edges"][0]["from"], id("character", 1));
    assert_eq!(path["edges"][0]["to"], path["edges"][1]["from"]);
    assert_eq!(path["edges"][1]["to"], id("character", 5));
    assert_eq!(path["edges"][1]["recordId"], id("parentage", 5));
    let isolated = genealogy::kinship_path(
        &world,
        &id("character", 1),
        &id("character", 6),
        &at(0, 0),
        8,
        100,
    )
    .unwrap();
    assert_eq!(isolated["found"], false);
    assert_eq!(isolated["reason"], "no_match");
    for (depth, limit) in [(0, 100), (1, 100), (2, 2)] {
        assert!(
            genealogy::lineage(
                &world,
                &id("character", 1),
                &at(0, 0),
                LineageDirection::Ancestors,
                depth,
                limit
            )
            .is_err()
        );
    }
    for (hops, limit) in [(0, 100), (1, 100), (8, 1)] {
        assert!(
            genealogy::kinship_path(
                &world,
                &id("character", 1),
                &id("character", 5),
                &at(0, 0),
                hops,
                limit
            )
            .is_err()
        );
    }
    let one = genealogy::kinship_path(
        &world,
        &id("character", 1),
        &id("character", 2),
        &at(0, 0),
        1,
        1,
    )
    .unwrap();
    assert_eq!(one["visitedCount"], 2); // The starting character is outside the discovery budget.
    let empty = genealogy::lineage(
        &world,
        &id("character", 6),
        &at(0, 0),
        LineageDirection::Ancestors,
        0,
        1,
    )
    .unwrap();
    assert_eq!(empty["items"], json!([]));
    let same = genealogy::kinship_path(
        &world,
        &id("character", 1),
        &id("character", 1),
        &at(0, 0),
        0,
        1,
    )
    .unwrap();
    assert_eq!(same["distance"], "0");
    assert_eq!(same["edges"], json!([]));
}

#[test]
fn query_limits_include_visible_evidence_rosters_and_whole_family_rows() {
    let mut edge = parentage(1, 1, 2);
    edge["transitions"] = json!([transition(
        101,
        "parentage-confirm",
        1,
        0,
        json!({"basis":"biological"})
    )]);
    let world = fixture_world(vec![edge, union(1, &[3, 4, 5])]);
    assert!(genealogy::family(&world, &id("character", 1), &at(1, 0), 1).is_err());
    assert!(genealogy::history(&world, &id("parentage", 1), &at(1, 0), 1).is_err());
    assert!(
        genealogy::kinship_path(
            &world,
            &id("character", 1),
            &id("character", 2),
            &at(1, 0),
            1,
            1
        )
        .is_err()
    );
    assert!(genealogy::family(&world, &id("character", 3), &at(0, 0), 2).is_err());
    assert!(genealogy::history(&world, &id("union", 1), &at(0, 0), 2).is_err());
    assert_eq!(
        genealogy::family(&world, &id("character", 3), &at(0, 0), 3).unwrap()["count"],
        1
    );
    let world = fixture_world(vec![
        parentage(1, 1, 2),
        parentage(2, 3, 1),
        union(1, &[1, 4]),
    ]);
    assert!(genealogy::family(&world, &id("character", 1), &at(0, 0), 2).is_err());
    assert_eq!(
        genealogy::family(&world, &id("character", 1), &at(0, 0), 3).unwrap()["count"],
        3
    );
}

#[test]
fn queries_validate_selectors_limits_and_capabilities_even_without_edges() {
    let base = fixture_world(vec![]);
    for selector in [id("world", 1), id("character", 999)] {
        assert!(genealogy::family(&base, &selector, &at(0, 0), 100).is_err());
        assert!(
            genealogy::kinship_path(&base, &id("character", 1), &selector, &at(0, 0), 8, 100)
                .is_err()
        );
    }
    assert!(genealogy::history(&base, &id("character", 1), &at(0, 0), 100).is_err());
    for limit in [0, 1001] {
        assert!(genealogy::family(&base, &id("character", 1), &at(0, 0), limit).is_err());
    }
    assert!(
        genealogy::lineage(
            &base,
            &id("character", 1),
            &at(0, 0),
            LineageDirection::Ancestors,
            33,
            100
        )
        .is_err()
    );
    assert!(
        genealogy::family(
            &base,
            &id("character", 1),
            &StoryTime::new("unknown", 0, 0).unwrap(),
            100
        )
        .is_err()
    );
    for status in [
        yaml(json!("retired")),
        Yaml::Null,
        tag(yaml(json!("canonical"))),
    ] {
        let candidate = edited(&base, &id("character", 1), |record| {
            record["status"] = status
        });
        assert!(genealogy::family(&candidate, &id("character", 1), &at(0, 0), 100).is_err());
    }
    let no_capability = edited(&base, &id("world", 1), |record| {
        record["capabilities"] = yaml(json!([]))
    });
    assert_eq!(family(&no_capability, 1, 0, 0)["count"], 0);
    let source = fixture_world(vec![parentage(1, 1, 2)]);
    let no_capability = edited(&source, &id("world", 1), |record| {
        record["capabilities"] = yaml(json!([]))
    });
    assert!(genealogy::family(&no_capability, &id("character", 1), &at(0, 0), 100).is_err());
}

#[test]
fn bounded_source_histories_and_member_arrays_reject_oversized_input() {
    let mut source = union(1, &[1, 2]);
    source["transitions"] = json!(
        (0..1000)
            .map(|index| transition(
                1000 + index,
                "union-reconcile",
                i64::from(index) + 1,
                0,
                roster(&[1, 2])
            ))
            .collect::<Vec<_>>()
    );
    let candidate = fixture_world(vec![source]);
    assert!(
        genealogy::validate(&candidate)
            .iter()
            .any(
                |diagnostic| diagnostic.field.as_deref() == Some("transitions")
                    && diagnostic.message.contains("1000")
            )
    );
    let mut source = union(1, &[1, 2]);
    source["participant_ids"] = json!(
        (1..=501)
            .map(|number| id("character", number))
            .collect::<Vec<_>>()
    );
    let candidate = fixture_world(vec![source]);
    assert!(
        genealogy::validate(&candidate)
            .iter()
            .any(
                |diagnostic| diagnostic.field.as_deref() == Some("participant_ids")
                    && diagnostic.message.contains("500")
            )
    );
}

#[test]
fn bundled_parentage_and_union_remain_compatible_without_source_changes() {
    let world = source::load_world(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/tideglass"),
        None,
    )
    .unwrap();
    valid(&world);
    let sela = "char_39E9EWWC017E40519033S4RW4X";
    let mina = "char_4JVNDN6QW0BAFFV379AJJSMEPZ";
    let orin = "char_62PCSFAX4YQE48JK476XKGG6B8";
    let parents = genealogy::family(&world, mina, &at(0, 0), 100).unwrap();
    assert_eq!(parents["parents"][0]["targetId"], sela);
    assert_eq!(parents["parents"][0]["lifecycle"], "confirmed");
    let family = genealogy::family(&world, sela, &at(-40, 0), 100).unwrap();
    assert_eq!(family["children"][0]["lifecycle"], "asserted");
    assert_eq!(family["unions"][0]["lifecycle"], "declared");
    assert_eq!(
        genealogy::kinship_path(&world, mina, orin, &at(0, 0), 8, 100).unwrap()["found"],
        false
    );
}
