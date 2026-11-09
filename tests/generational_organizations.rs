//! Authored organization identity/containment and affiliation lifecycles.
use serde_json::{Value as Json, json};
use serde_yaml::Value as Yaml;
use std::path::PathBuf;
use wedl::{
    generational as genealogy,
    model::{Record, StoryTime, World},
    source,
};

fn id(kind: &str, number: u32) -> String {
    let prefix = match kind {
        "character" => "char",
        "location" => "loc",
        _ => kind,
    };
    format!("{prefix}_{number:026}")
}
fn at(tick: i64, order: i32) -> StoryTime {
    StoryTime::new("main", tick, order).unwrap()
}
fn tr(number: u32, kind: &str, tick: i64, order: i32, payload: Json) -> Json {
    json!({"transition_id":id("transition",number),"transition_kind":kind,
        "applicability":{"applicability_kind":"instant","point":{"timeline":"main","tick":tick,"order":order}},"payload":payload})
}
fn org(number: u32, parent: Option<u32>) -> Json {
    json!({"kind":"organization","id":id("organization",number),"title":format!("Header {number}"),
        "organization_kind":"institution","parent_id":parent.map(|value|id("organization",value)),"location_id":id("location",1),
        "initialization":tr(number*1000,"organization-initialize",0,0,json!({"title":format!("Original {number}"),"aliases":[]})),"transitions":[]})
}
fn aff(number: u32, person: u32, organization: u32, tick: i64, order: i32) -> Json {
    json!({"kind":"affiliation","id":id("affiliation",number),"title":format!("Membership {number}"),
        "character_id":id("character",person),"organization_id":id("organization",organization),
        "initialization":tr(number*1000+500,"affiliation-initialize",tick,order,json!({"role":null})),"transitions":[]})
}
fn record(mut value: Json) -> Record {
    value["schema"] = json!(source::SOURCE_SCHEMA);
    let path = format!("story/{}.md", value["id"].as_str().unwrap());
    let raw = format!(
        "---\n{}---\nPreserved source.\n",
        serde_yaml::to_string(&value).unwrap()
    );
    source::parse_record(raw.as_bytes(), &path).unwrap()
}
fn world(extra: Vec<Json>) -> World {
    let mut records = vec![
        record(
            json!({"kind":"world","id":id("world",1),"title":"Organizations",
        "capabilities":["generational-core-v1"],"default_timeline":"main","timelines":[{"id":"main","label":"Main"},{"id":"other","label":"Other"}],"state_keys":{}}),
        ),
        record(json!({"kind":"location","id":id("location",1),"title":"A place"})),
    ];
    records.extend((1..=4).map(|number|record(json!({"kind":"character","id":id("character",number),"title":format!("Person {number}")}))));
    records.extend(extra.into_iter().map(record));
    source::world_from_records(PathBuf::new(), records, Some("organization-fixture".into()))
        .unwrap()
}
fn valid(world: &World) {
    assert!(
        genealogy::validate(world).is_empty(),
        "{:?}",
        genealogy::validate(world)
    );
}
fn invalid(world: &World) {
    let diagnostics = genealogy::validate(world);
    assert!(!diagnostics.is_empty());
    assert!(
        diagnostics
            .iter()
            .all(|item| item.field.is_some() && item.path.is_some())
    );
    assert!(
        genealogy::organization(world, &id("organization", 1), &at(20, 0), 8, false, 100).is_err()
    );
}
fn query(world: &World, number: u32, tick: i64, order: i32) -> Json {
    genealogy::organization(
        world,
        &id("organization", number),
        &at(tick, order),
        8,
        false,
        100,
    )
    .unwrap()
}
fn hist(world: &World, kind: &str, number: u32, tick: i64, order: i32) -> Json {
    genealogy::history(world, &id(kind, number), &at(tick, order), 100).unwrap()
}

#[test]
fn organization_names_are_historical_and_dormancy_does_not_reactivate() {
    let mut organization = org(1, None);
    organization["transitions"] = json!([
        tr(
            1001,
            "organization-rename",
            5,
            2,
            json!({"title":"New name","aliases":["Alias A","Alias B"]})
        ),
        tr(1002, "organization-dormant", 6, 0, json!({})),
        tr(
            1003,
            "organization-rename",
            7,
            0,
            json!({"title":"Dormant name","aliases":[]})
        ),
        tr(1004, "organization-dissolve", 9, 0, json!({}))
    ]);
    let world = world(vec![organization]);
    valid(&world);
    assert_eq!(
        query(&world, 1, 5, 1)["organization"]["value"]["title"],
        "Original 1"
    );
    assert_eq!(
        query(&world, 1, 5, 2)["organization"]["value"]["title"],
        "New name"
    );
    let dormant = query(&world, 1, 7, 0);
    assert_eq!(dormant["organization"]["lifecycle"], "dormant");
    assert_eq!(dormant["organization"]["value"]["title"], "Dormant name");
    assert_eq!(dormant["organization"]["active"], true);
    let dissolved = query(&world, 1, 9, 0);
    assert_eq!(dissolved["available"], true);
    assert_eq!(dissolved["organization"]["lifecycle"], "dissolved");
    assert_eq!(dissolved["organization"]["active"], false);
    assert_eq!(
        dissolved["organization"]["value"]["locationId"],
        id("location", 1)
    );
    let mut record = org(1, None);
    record["transitions"] = json!([
        tr(1001, "organization-dormant", 1, 0, json!({})),
        tr(1002, "organization-dormant", 2, 0, json!({}))
    ]);
    invalid(&world_for(record));
}
fn world_for(record: Json) -> World {
    world(vec![record])
}

#[test]
fn visibility_respects_initialization_exact_order_and_timeline() {
    let start = 9_007_199_254_740_993_i64;
    let mut organization = org(1, None);
    organization["initialization"]["applicability"]["point"] =
        json!({"timeline":"main","tick":start,"order":-2});
    let world = world(vec![organization]);
    let before = query(&world, 1, start, -3);
    assert_eq!(before["available"], false);
    assert_eq!(before["reason"], "before_initialization");
    assert_eq!(before["organization"], Json::Null);
    let exact = query(&world, 1, start, -2);
    assert_eq!(exact["available"], true);
    assert_eq!(exact["effectiveTime"]["tick"], start.to_string());
    let other = genealogy::organization(
        &world,
        &id("organization", 1),
        &StoryTime::new("other", i64::MAX, i32::MAX).unwrap(),
        8,
        false,
        100,
    )
    .unwrap();
    assert_eq!(other["reason"], "different_timeline");
    assert_eq!(other["roles"], json!([]));
}

#[test]
fn containment_folds_reparent_and_detach_instead_of_combining_history() {
    let mut child = org(1, Some(2));
    child["transitions"] = json!([
        tr(
            1001,
            "organization-reparent",
            4,
            3,
            json!({"parent_id":id("organization",3)})
        ),
        tr(
            1002,
            "organization-reparent",
            6,
            0,
            json!({"parent_id":null})
        )
    ]);
    let world = world(vec![child, org(2, None), org(3, None)]);
    valid(&world);
    assert_eq!(
        query(&world, 1, 4, 2)["parentPath"][0]["targetId"],
        id("organization", 2)
    );
    let after = query(&world, 1, 4, 3);
    assert_eq!(after["parentPath"][0]["targetId"], id("organization", 3));
    assert_eq!(
        after["parentPath"][0]["edges"][0]["from"],
        id("organization", 1)
    );
    assert_eq!(
        after["parentPath"][0]["edges"][0]["recordId"],
        id("organization", 1)
    );
    assert_eq!(
        after["parentPath"][0]["edges"][0]["citations"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(query(&world, 1, 6, 0)["parentPath"], json!([]));
    assert_eq!(
        hist(&world, "organization", 1, 6, 0)["value"]["parentId"],
        Json::Null
    );
}

#[test]
fn containment_stops_before_future_parents_and_at_dissolved_parents() {
    let child = org(1, Some(2));
    let mut parent = org(2, None);
    parent["initialization"]["applicability"]["point"]["tick"] = json!(2);
    parent["transitions"] = json!([
        tr(2001, "organization-dormant", 3, 0, json!({})),
        tr(2002, "organization-dissolve", 4, 2, json!({}))
    ]);
    let world = world(vec![child, parent]);
    valid(&world);
    assert_eq!(query(&world, 1, 1, 0)["parentPath"], json!([]));
    assert_eq!(
        query(&world, 1, 3, 0)["parentPath"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(query(&world, 1, 4, 2)["parentPath"], json!([]));
}

#[test]
fn header_cycles_are_invalid_but_nonconcurrent_reparenting_is_valid() {
    invalid(&world(vec![org(1, Some(2)), org(2, Some(1))]));
    let mut first = org(1, Some(2));
    first["transitions"] = json!([tr(
        1001,
        "organization-reparent",
        3,
        0,
        json!({"parent_id":null})
    )]);
    let mut second = org(2, None);
    second["transitions"] = json!([tr(
        2001,
        "organization-reparent",
        4,
        0,
        json!({"parent_id":id("organization",1)})
    )]);
    let candidate = world(vec![first.clone(), second.clone()]);
    valid(&candidate);
    assert_eq!(
        query(&candidate, 2, 4, 0)["parentPath"][0]["targetId"],
        id("organization", 1)
    );
    second["transitions"][0]["applicability"]["point"]["tick"] = json!(2);
    let cyclic = world(vec![first, second]);
    valid(&cyclic); // Upstream validates static headers; query guards folded cycles.
    assert!(
        genealogy::organization(&cyclic, &id("organization", 1), &at(2, 0), 8, false, 100)
            .unwrap_err()
            .to_string()
            .contains("cycle")
    );
    assert_eq!(hist(&cyclic, "organization", 1, 2, 0)["available"], true);
}

#[test]
fn roles_end_at_the_exact_horizon_and_former_roles_remain_explicit() {
    let mut membership = aff(1, 1, 1, 0, 0);
    membership["transitions"] = json!([
        tr(1501, "affiliation-role", 3, 2, json!({"role":"Archivist"})),
        tr(1502, "affiliation-end", 5, 4, json!({}))
    ]);
    let world = world(vec![org(1, None), membership]);
    valid(&world);
    assert_eq!(
        query(&world, 1, 3, 1)["roles"][0]["value"]["role"],
        Json::Null
    );
    assert_eq!(
        query(&world, 1, 3, 2)["roles"][0]["value"]["role"],
        "Archivist"
    );
    assert_eq!(query(&world, 1, 5, 3)["roles"].as_array().unwrap().len(), 1);
    let ended = query(&world, 1, 5, 4);
    assert_eq!(ended["roles"], json!([]));
    assert!(ended.get("formerRoles").is_none());
    let former =
        genealogy::organization(&world, &id("organization", 1), &at(5, 4), 8, true, 100).unwrap();
    assert_eq!(former["formerRoles"][0]["lifecycle"], "ended");
    assert_eq!(former["formerRoles"][0]["value"]["role"], "Archivist");
    assert_eq!(
        former["formerRoles"][0]["value"]["characterId"],
        id("character", 1)
    );
    let history = hist(&world, "affiliation", 1, 5, 4);
    assert_eq!(history["count"], 3);
    assert_eq!(history["history"][1]["payload"]["role"], "Archivist");
    assert_eq!(history["active"], false);
}

#[test]
fn affiliation_overlaps_include_shared_endpoints_and_ignore_role_differences() {
    let mut first = aff(1, 1, 1, 0, 0);
    first["initialization"]["payload"]["role"] = json!("First role");
    first["transitions"] = json!([tr(1501, "affiliation-end", 5, 2, json!({}))]);
    let mut second = aff(2, 1, 1, 5, 2);
    second["initialization"]["payload"]["role"] = json!("Different role");
    invalid(&world(vec![org(1, None), first.clone(), second.clone()]));
    second["initialization"]["applicability"]["point"]["order"] = json!(3);
    valid(&world(vec![org(1, None), first.clone(), second.clone()]));
    second["initialization"]["applicability"]["point"] =
        json!({"timeline":"other","tick":0,"order":0});
    valid(&world(vec![org(1, None), first.clone(), second.clone()]));
    second["initialization"]["applicability"]["point"] =
        json!({"timeline":"main","tick":0,"order":0});
    second["status"] = json!("draft");
    valid(&world(vec![org(1, None), first, second]));
}

#[test]
fn replaced_affiliation_ends_do_not_hide_overlapping_histories() {
    let mut first = aff(1, 1, 1, 0, 0);
    let mut corrected = tr(
        1502,
        "affiliation-role",
        3,
        0,
        json!({"role":"Still a member"}),
    );
    corrected["replaces_transition_id"] = json!(id("transition", 1501));
    first["transitions"] = json!([tr(1501, "affiliation-end", 3, 0, json!({})), corrected]);
    let only = world(vec![org(1, None), first.clone()]);
    valid(&only);
    assert_eq!(query(&only, 1, 4, 0)["roles"][0]["lifecycle"], "active");
    assert_eq!(
        hist(&only, "affiliation", 1, 3, 0)["history"][1]["superseded"],
        true
    );
    invalid(&world(vec![org(1, None), first, aff(2, 1, 1, 4, 0)]));
}

#[test]
fn surviving_terminal_fork_is_not_reopened_by_a_role_payload() {
    let mut membership = aff(1, 1, 1, 0, 0);
    let mut terminal = tr(1502, "affiliation-end", 3, 0, json!({}));
    terminal["replaces_transition_id"] = json!(id("transition", 1501));
    let mut role = tr(
        1503,
        "affiliation-role",
        3,
        0,
        json!({"role":"Historical role"}),
    );
    role["replaces_transition_id"] = json!(id("transition", 1501));
    membership["transitions"] =
        json!([tr(1501, "affiliation-end", 3, 0, json!({})), terminal, role]);
    let candidate = world(vec![org(1, None), membership]);
    valid(&candidate);
    let history = hist(&candidate, "affiliation", 1, 3, 0);
    assert_eq!(history["lifecycle"], "ended");
    assert_eq!(history["active"], false);
    assert_eq!(history["value"]["role"], "Historical role");
    assert_eq!(
        history["appliedTransitionIds"],
        json!([
            id("transition", 1500),
            id("transition", 1502),
            id("transition", 1503)
        ])
    );
    assert_eq!(query(&candidate, 1, 3, 0)["roles"], json!([]));
    let organization =
        genealogy::organization(&candidate, &id("organization", 1), &at(3, 0), 8, true, 100)
            .unwrap();
    assert_eq!(
        organization["formerRoles"][0]["value"]["role"],
        "Historical role"
    );
    assert_eq!(organization["formerRoles"][0]["lifecycle"], "ended");
}

#[test]
fn parent_references_are_projected_at_the_horizon_in_values_and_history_payloads() {
    let mut child = org(1, Some(2));
    child["transitions"] = json!([
        tr(
            1001,
            "organization-reparent",
            1,
            0,
            json!({"parent_id":id("organization", 2)})
        ),
        tr(
            1002,
            "organization-reparent",
            12,
            0,
            json!({"parent_id":null})
        )
    ]);
    let mut parent = org(2, None);
    parent["initialization"]["applicability"]["point"] =
        json!({"timeline":"main","tick":10,"order":3});
    let candidate = world(vec![child.clone(), parent.clone()]);
    valid(&candidate);
    let before_bytes: Vec<_> = candidate
        .records
        .values()
        .map(|record| record.raw_bytes.clone())
        .collect();
    let early = hist(&candidate, "organization", 1, 10, 2);
    assert!(early["value"].get("parentId").is_none());
    assert!(early["history"][1]["payload"].get("parent_id").is_none());
    assert_eq!(early["value"]["locationId"], id("location", 1));
    assert_eq!(early["citations"].as_array().unwrap().len(), 2);
    let exact = hist(&candidate, "organization", 1, 10, 3);
    assert_eq!(exact["value"]["parentId"], id("organization", 2));
    assert_eq!(
        exact["history"][1]["payload"]["parent_id"],
        id("organization", 2)
    );
    assert_eq!(
        query(&candidate, 1, 10, 3)["parentPath"][0]["targetId"],
        id("organization", 2)
    );
    let detached = hist(&candidate, "organization", 1, 12, 0);
    assert_eq!(detached["value"].get("parentId"), Some(&Json::Null));
    assert_eq!(
        detached["history"][2]["payload"].get("parent_id"),
        Some(&Json::Null)
    );
    assert_eq!(
        before_bytes,
        candidate
            .records
            .values()
            .map(|record| record.raw_bytes.clone())
            .collect::<Vec<_>>()
    );
    parent["initialization"]["applicability"]["point"] =
        json!({"timeline":"other","tick":0,"order":0});
    let other = world(vec![child, parent]);
    valid(&other);
    let projected = hist(&other, "organization", 1, 11, 0);
    assert!(projected["value"].get("parentId").is_none());
    assert!(
        projected["history"][1]["payload"]
            .get("parent_id")
            .is_none()
    );
    assert_eq!(query(&other, 1, 11, 0)["parentPath"], json!([]));
}

#[test]
fn organization_corrections_restore_prestate_and_keep_literal_evidence() {
    let mut organization = org(1, None);
    let mut replacement = tr(
        1002,
        "organization-rename",
        3,
        1,
        json!({"title":"Restored","aliases":[]}),
    );
    replacement["replaces_transition_id"] = json!(id("transition", 1001));
    organization["transitions"] = json!([
        tr(1001, "organization-dissolve", 3, 1, json!({})),
        replacement,
        tr(1003, "organization-dormant", 4, 0, json!({}))
    ]);
    let world = world(vec![organization]);
    valid(&world);
    let before = hist(&world, "organization", 1, 3, 0);
    assert_eq!(before["count"], 1);
    assert!(!before.to_string().contains("Restored"));
    let after = hist(&world, "organization", 1, 3, 1);
    assert_eq!(after["lifecycle"], "active");
    assert_eq!(after["value"]["title"], "Restored");
    assert_eq!(
        after["appliedTransitionIds"],
        json!([id("transition", 1000), id("transition", 1002)])
    );
    assert_eq!(after["citations"].as_array().unwrap().len(), 3);
    assert_eq!(after["history"][1]["superseded"], true);
    assert_eq!(after["history"][2]["applicability"]["point"]["order"], "1");
    assert_eq!(
        hist(&world, "organization", 1, 4, 0)["lifecycle"],
        "dormant"
    );
}

#[test]
fn closed_payloads_references_and_lifecycles_reject_invalid_inputs() {
    for (pointer, value) in [
        ("/organization_kind", json!("company")),
        ("/parent_id", json!(id("character", 1))),
        ("/location_id", json!(id("organization", 1))),
        ("/initialization/payload/title", json!("  ")),
        ("/initialization/payload/aliases", json!(["B", "A"])),
        ("/initialization/payload/aliases", json!(["A", "A"])),
        (
            "/initialization/applicability",
            json!({"applicability_kind":"static"}),
        ),
    ] {
        let mut source = org(1, None);
        *source.pointer_mut(pointer).unwrap() = value;
        invalid(&world(vec![source]));
    }
    for transitions in [
        json!([
            tr(1001, "organization-dissolve", 1, 0, json!({})),
            tr(
                1002,
                "organization-rename",
                2,
                0,
                json!({"title":"Reopened","aliases":[]})
            )
        ]),
        json!([tr(
            1001,
            "organization-reparent",
            1,
            0,
            json!({"parent_id":null,"extra":true})
        )]),
        json!([tr(1001, "organization-activate", 1, 0, json!({}))]),
        json!([tr(
            1001,
            "organization-rename",
            0,
            0,
            json!({"title":"Same instant","aliases":[]})
        )]),
    ] {
        let mut source = org(1, None);
        source["transitions"] = transitions;
        invalid(&world(vec![source]));
    }
    for role in [json!(null), json!(""), json!("   "), json!(true)] {
        let mut membership = aff(1, 1, 1, 0, 0);
        membership["transitions"] =
            json!([tr(1501, "affiliation-role", 1, 0, json!({"role":role}))]);
        invalid(&world(vec![org(1, None), membership]));
    }
    let mut membership = aff(1, 1, 1, 0, 0);
    membership["initialization"]["payload"]["role"] = json!("");
    valid(&world(vec![org(1, None), membership.clone()])); // Literal initial role permits any string.
    membership["transitions"] = json!([
        tr(1501, "affiliation-end", 1, 0, json!({})),
        tr(1502, "affiliation-role", 2, 0, json!({"role":"Reopened"}))
    ]);
    invalid(&world(vec![org(1, None), membership]));
}

#[test]
fn tagged_null_status_and_nested_temporal_extensions_do_not_execute() {
    let base = world(vec![org(1, None)]);
    for field in ["status", "parent_id", "organization_kind", "initialization"] {
        let mut candidate = base.clone();
        let source = &mut candidate
            .records
            .get_mut(&id("organization", 1))
            .unwrap()
            .frontmatter;
        let value = if field == "status" {
            Yaml::String("canonical".into())
        } else {
            source[field].clone()
        };
        source[field] = Yaml::Tagged(Box::new(serde_yaml::value::TaggedValue {
            tag: serde_yaml::value::Tag::new("custom"),
            value,
        }));
        invalid(&candidate);
    }
    let mut candidate = base.clone();
    candidate
        .records
        .get_mut(&id("organization", 1))
        .unwrap()
        .frontmatter["status"] = Yaml::Null;
    invalid(&candidate);
    let mut candidate = base.clone();
    candidate
        .records
        .get_mut(&id("organization", 1))
        .unwrap()
        .frontmatter["initialization"]["applicability"]["point"]["chronology"] = Yaml::Null;
    invalid(&candidate);
}

#[test]
fn exact_cause_event_order_and_replacement_applicability_are_enforced() {
    let mut organization = org(1, None);
    let mut rename = tr(
        1001,
        "organization-rename",
        3,
        2,
        json!({"title":"Caused","aliases":[]}),
    );
    rename["cause_event_id"] = json!(id("event", 1));
    organization["transitions"] = json!([rename]);
    let mut event = json!({"kind":"event","id":id("event",1),"title":"Cause","time":{"timeline":"main","tick":3,"order":1}});
    let candidate = world(vec![organization.clone(), event.clone()]);
    valid(&candidate);
    assert_eq!(
        hist(&candidate, "organization", 1, 3, 2)["citations"][1]["causeCitation"]["entityId"],
        id("event", 1)
    );
    event["time"]["order"] = json!(2);
    invalid(&world(vec![organization, event]));
    let mut organization = org(1, None);
    let mut replacement = tr(
        1002,
        "organization-rename",
        2,
        0,
        json!({"title":"Invalid time","aliases":[]}),
    );
    replacement["replaces_transition_id"] = json!(id("transition", 1001));
    organization["transitions"] = json!([
        tr(1001, "organization-dissolve", 1, 0, json!({})),
        replacement
    ]);
    invalid(&world(vec![organization]));
}

#[test]
fn complete_budgets_apply_after_filtering_and_to_containment_and_evidence() {
    let mut ended1 = aff(1, 1, 1, 0, 0);
    ended1["transitions"] = json!([tr(1501, "affiliation-end", 1, 0, json!({}))]);
    let mut ended2 = aff(2, 2, 1, 0, 0);
    ended2["transitions"] = json!([tr(2501, "affiliation-end", 1, 0, json!({}))]);
    let candidate = world(vec![org(1, None), ended1, ended2, aff(3, 3, 1, 0, 0)]);
    let active =
        genealogy::organization(&candidate, &id("organization", 1), &at(2, 0), 0, false, 1)
            .unwrap();
    assert_eq!(active["roles"].as_array().unwrap().len(), 1);
    assert!(
        genealogy::organization(&candidate, &id("organization", 1), &at(2, 0), 0, true, 2).is_err()
    );
    let all =
        genealogy::organization(&candidate, &id("organization", 1), &at(2, 0), 0, true, 3).unwrap();
    assert_eq!(all["formerRoles"].as_array().unwrap().len(), 2);
    let chain = world(vec![org(1, Some(2)), org(2, Some(3)), org(3, None)]);
    for (depth, limit) in [(0, 100), (1, 100), (2, 1)] {
        assert!(
            genealogy::organization(
                &chain,
                &id("organization", 1),
                &at(0, 0),
                depth,
                false,
                limit
            )
            .is_err()
        );
    }
    assert_eq!(genealogy::organization(&chain,&id("organization",1),&at(0,0),2,false,2).unwrap()["parentPath"].as_array().unwrap().len(),2);
    let mut organization = org(1, None);
    organization["initialization"]["payload"]["aliases"] = json!(["A", "B"]);
    let candidate = world(vec![organization]);
    assert!(
        genealogy::organization(&candidate, &id("organization", 1), &at(0, 0), 0, false, 1)
            .is_err()
    );
    assert!(genealogy::history(&candidate, &id("organization", 1), &at(0, 0), 1).is_err());
}

#[test]
fn noncanonical_histories_still_validate_and_queries_require_capability() {
    let mut organization = org(1, None);
    organization["status"] = json!("retired");
    let candidate = world(vec![organization.clone()]);
    valid(&candidate);
    assert_eq!(query(&candidate, 1, 0, 0)["reason"], "noncanonical");
    assert_eq!(
        hist(&candidate, "organization", 1, 0, 0)["history"],
        json!([])
    );
    organization["initialization"]["payload"]["aliases"] = Json::Null;
    invalid(&world(vec![organization]));
    let mut candidate = world(vec![org(1, None)]);
    candidate
        .records
        .get_mut(&id("world", 1))
        .unwrap()
        .frontmatter["capabilities"] = Yaml::Sequence(vec![]);
    assert!(
        genealogy::organization(&candidate, &id("organization", 1), &at(0, 0), 0, false, 100)
            .is_err()
    );
    for (depth, limit) in [(33, 100), (0, 0), (0, 1001)] {
        assert!(
            genealogy::organization(
                &world(vec![org(1, None)]),
                &id("organization", 1),
                &at(0, 0),
                depth,
                false,
                limit
            )
            .is_err()
        );
    }
}

#[test]
fn packaged_organization_memberships_remain_literal_and_source_unchanged() {
    let world = source::load_world(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/tideglass"),
        None,
    )
    .unwrap();
    let before: Vec<_> = world
        .records
        .values()
        .map(|record| record.raw_bytes.clone())
        .collect();
    valid(&world);
    let beacon = "organization_4YNRVZSRYTGBB27YKD7NW1T7CZ";
    let early = genealogy::organization(&world, beacon, &at(-40, 0), 8, false, 100).unwrap();
    assert_eq!(
        early["organization"]["value"]["title"],
        "Beacon Cooperative"
    );
    assert_eq!(early["roles"][0]["value"]["role"], Json::Null);
    let current = genealogy::organization(&world, beacon, &at(0, 0), 8, false, 100).unwrap();
    assert_eq!(current["roles"][0]["value"]["role"], "repair coordinator");
    assert_eq!(current["parentPath"], json!([]));
    assert_eq!(
        before,
        world
            .records
            .values()
            .map(|record| record.raw_bytes.clone())
            .collect::<Vec<_>>()
    );
}
